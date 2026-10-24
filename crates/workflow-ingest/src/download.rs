//! Téléchargement reprenable des archives Dropbox / R2.
//!
//! Reproduit `services/download_service.py` : sonde HEAD, fichier `.part`,
//! reprise HTTP `Range`, validation ZIP et erreurs typées.

use crate::error::IngestResult as Result;
use crate::url::{normalize_url, rewrite_dropbox_to_dl_host};
use futures_util::StreamExt;
use regex::Regex;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::header::{
    ACCEPT_ENCODING, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, RANGE,
};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::fs::{self, File, OpenOptions};
use tokio::io::AsyncWriteExt;
use workflow_core::DownloadConfig;

pub const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    NotFound,
    Auth,
    HttpError,
    Network,
    Stalled,
    Incomplete,
    InvalidPayload,
    LocalIo,
    Internal,
}

impl ErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::Auth => "auth",
            Self::HttpError => "http_error",
            Self::Network => "network",
            Self::Stalled => "stalled",
            Self::Incomplete => "incomplete",
            Self::InvalidPayload => "invalid_payload",
            Self::LocalIo => "local_io",
            Self::Internal => "internal",
        }
    }

    fn retryable(self) -> bool {
        !matches!(
            self,
            Self::NotFound | Self::Auth | Self::InvalidPayload | Self::LocalIo | Self::Internal
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    pub success: bool,
    pub download_id: String,
    pub filename: String,
    pub filepath: Option<PathBuf>,
    pub size_bytes: u64,
    pub message: String,
    pub status: String,
    pub status_code: Option<u16>,
    pub error_kind: Option<ErrorKind>,
    pub bytes_downloaded: u64,
    pub attempts: u32,
    pub resumed_from: u64,
    pub url: Option<String>,
}

pub type ProgressCallback = Arc<dyn Fn(&str, u32, &str) + Send + Sync>;

#[derive(Debug, Clone, Default)]
struct ProbeResult {
    status_code: Option<u16>,
    content_type: String,
    content_disposition: String,
    content_length: Option<u64>,
    accept_ranges: bool,
}

struct StreamOutcome {
    ok: bool,
    status_code: Option<u16>,
    error_kind: Option<ErrorKind>,
    error_message: String,
    total_on_disk: u64,
    expected_total: Option<u64>,
    content_type: String,
    restart_required: bool,
}

pub struct Downloader {
    client: Client,
    config: DownloadConfig,
}

/// Résolveur qui refuse les cibles loopback, privées, link-local et
/// multicast. Il protège le téléchargeur contre une URL webhook compromise
/// (SSRF), y compris après redirection.
#[derive(Debug)]
struct PublicOnlyResolver;

impl Resolve for PublicOnlyResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_string();
            let resolved = tokio::net::lookup_host((host.as_str(), 0)).await?;
            let addresses: Vec<SocketAddr> = resolved
                .filter(|address| is_public_ip(address.ip()))
                .collect();
            if addresses.is_empty() {
                let error: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("adresse non publique refusée pour {host}"),
                ));
                return Err(error);
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_public_ipv4(address),
        IpAddr::V6(address) => {
            if let Some(mapped) = address.to_ipv4_mapped() {
                return is_public_ipv4(mapped);
            }
            !(address.is_loopback()
                || address.is_unspecified()
                || address.is_multicast()
                || address.is_unique_local()
                || address.is_unicast_link_local())
        }
    }
}

fn is_public_ipv4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    let cgnat = octets[0] == 100 && (octets[1] & 0xc0) == 64;
    !(address.is_private()
        || address.is_loopback()
        || address.is_link_local()
        || address.is_broadcast()
        || address.is_documentation()
        || address.is_unspecified()
        || address.is_multicast()
        || octets[0] == 0
        || cgnat)
}

impl Downloader {
    pub fn new(config: &DownloadConfig) -> Result<Self> {
        let mut builder = Client::builder()
            .connect_timeout(Duration::from_secs(config.connect_timeout_s.max(1)))
            .read_timeout(Duration::from_secs(config.chunk_timeout_s.max(1)))
            .redirect(reqwest::redirect::Policy::limited(3))
            .user_agent(USER_AGENT);
        if !config.allow_private_hosts {
            builder = builder.dns_resolver(Arc::new(PublicOnlyResolver));
        }
        let client = builder.build()?;
        Ok(Self {
            client,
            config: config.clone(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn download(
        &self,
        url: &str,
        timestamp: &str,
        output_dir: &Path,
        forced_filename: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> DownloadResult {
        let download_id = new_download_id();
        let job_label = format!("CSV-DL-{}", clean_timestamp(timestamp));
        let mut attempts = 0;
        let mut resumed_from = 0;
        let mut last_status_code = None;
        let mut last_error_kind = None;
        let mut last_message = "Download failed".to_string();

        let normalized = normalize_url(url);
        let mut modified_url = normalized.replace("dl=0", "dl=1");
        tracing::info!("DOWNLOAD [{job_label} ID: {download_id}]: départ depuis {modified_url}");

        let probe = self.probe(&modified_url).await;
        tracing::debug!(
            "DOWNLOAD [{job_label}]: HEAD status={:?} type='{}' length={:?} range={}",
            probe.status_code,
            if probe.content_type.is_empty() {
                "n/a"
            } else {
                &probe.content_type
            },
            probe.content_length,
            if probe.accept_ranges { "yes" } else { "no" }
        );
        if (!probe.content_type.is_empty() || !probe.content_disposition.is_empty())
            && !looks_like_zip(&probe.content_type, &probe.content_disposition)
        {
            let alternative = rewrite_dropbox_to_dl_host(&modified_url);
            if alternative != modified_url {
                tracing::info!("DOWNLOAD [{job_label}]: essai dl.dropboxusercontent.com");
                modified_url = alternative;
            }
        }

        let filename = resolve_target_filename(
            forced_filename,
            &probe.content_disposition,
            timestamp,
            &modified_url,
        );
        let target_path = resolve_filepath_conflicts(output_dir, &filename);
        let part_path = part_path_for(&target_path);
        let mut restart_required = false;

        for attempt in 1..=self.config.max_attempts.max(1) {
            attempts = attempt;
            if restart_required {
                discard_partial(&part_path, &job_label).await;
            }
            let offset = partial_size(&part_path).await;
            if offset > 0 {
                resumed_from = resumed_from.max(offset);
                tracing::info!(
                    "DOWNLOAD [{job_label}]: tentative {attempt}/{} - reprise à {}",
                    self.config.max_attempts,
                    format_bytes_human(offset)
                );
            }

            let outcome = self
                .stream_to_part(
                    &modified_url,
                    &part_path,
                    &target_path,
                    offset,
                    &job_label,
                    progress.clone(),
                )
                .await;
            last_status_code = outcome.status_code;

            if outcome.ok {
                let (finalized, error_kind, message) = self
                    .finalize_download(
                        &part_path,
                        &target_path,
                        &outcome.content_type,
                        &modified_url,
                        outcome.expected_total,
                    )
                    .await;
                if finalized {
                    let final_bytes = fs::metadata(&target_path)
                        .await
                        .map(|metadata| metadata.len())
                        .unwrap_or(0);
                    let success_message = format!(
                        "File {} ({}) downloaded",
                        target_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("archive"),
                        format_bytes_human(final_bytes)
                    );
                    tracing::info!("DOWNLOAD [{job_label}]: {success_message}");
                    if let Some(callback) = &progress {
                        callback("completed", 100, &success_message);
                    }
                    return DownloadResult {
                        success: true,
                        download_id,
                        filename: target_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or_default()
                            .to_string(),
                        filepath: Some(target_path),
                        size_bytes: final_bytes,
                        message: success_message,
                        status: "completed".into(),
                        status_code: outcome.status_code,
                        error_kind: None,
                        bytes_downloaded: outcome.total_on_disk,
                        attempts: attempt,
                        resumed_from,
                        url: Some(modified_url),
                    };
                }
                last_error_kind = Some(error_kind);
                last_message = message.clone();
                tracing::error!("DOWNLOAD [{job_label}]: {message}");
                if error_kind == ErrorKind::InvalidPayload {
                    discard_partial(&part_path, &job_label).await;
                    break;
                }
                restart_required = true;
            } else {
                last_error_kind = outcome.error_kind;
                last_message = outcome.error_message;
                restart_required = outcome.restart_required;
                tracing::warn!(
                    "DOWNLOAD [{job_label}]: tentative {attempt}/{} échouée ({:?}, HTTP {:?}): {}",
                    self.config.max_attempts,
                    outcome.error_kind,
                    outcome.status_code,
                    last_message
                );
                if let Some(kind) = outcome.error_kind {
                    if !kind.retryable() {
                        break;
                    }
                }
                if restart_required {
                    discard_partial(&part_path, &job_label).await;
                    restart_required = false;
                }
            }

            if attempt < self.config.max_attempts {
                let delay = self.config.backoff_s * 2_f64.powi(attempt as i32 - 1);
                if delay > 0.0 {
                    tokio::time::sleep(Duration::from_secs_f64(delay)).await;
                }
            }
        }

        tracing::error!(
            "DOWNLOAD [{job_label}]: échec après {attempts} tentative(s) - {last_message} (kind={:?}, status={last_status_code:?})",
            last_error_kind
        );
        if let Some(callback) = &progress {
            callback("failed", 0, &last_message);
        }
        DownloadResult {
            success: false,
            download_id,
            filename: target_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
            filepath: None,
            size_bytes: 0,
            message: last_message,
            status: "failed".into(),
            status_code: last_status_code,
            error_kind: last_error_kind,
            bytes_downloaded: 0,
            attempts,
            resumed_from,
            url: Some(modified_url),
        }
    }

    async fn probe(&self, url: &str) -> ProbeResult {
        let request = self
            .client
            .head(url)
            .header(ACCEPT_ENCODING, "identity")
            .timeout(Duration::from_secs(self.config.probe_timeout_s.max(1)));
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!("DOWNLOAD: HEAD impossible ({error}), poursuite avec GET");
                return ProbeResult::default();
            }
        };
        ProbeResult {
            status_code: Some(response.status().as_u16()),
            content_type: header_string(&response, CONTENT_TYPE),
            content_disposition: header_string(&response, CONTENT_DISPOSITION),
            content_length: header_u64(&response, CONTENT_LENGTH),
            accept_ranges: response
                .headers()
                .get("accept-ranges")
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.to_ascii_lowercase().contains("bytes")),
        }
    }

    async fn stream_to_part(
        &self,
        url: &str,
        part_path: &Path,
        target_path: &Path,
        offset: u64,
        job_label: &str,
        progress: Option<ProgressCallback>,
    ) -> StreamOutcome {
        let mut request = self.client.get(url).header(ACCEPT_ENCODING, "identity");
        if offset > 0 {
            request = request.header(RANGE, format!("bytes={offset}-"));
        }
        let response = match request.send().await {
            Ok(response) => response,
            Err(error) => return transport_failure(error, offset, self.config.chunk_timeout_s),
        };

        let status = response.status();
        let status_code = Some(status.as_u16());
        let content_type = header_string(&response, CONTENT_TYPE);
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            return StreamOutcome {
                ok: false,
                status_code,
                error_kind: Some(ErrorKind::Incomplete),
                error_message: format!("Range {offset} rejected (HTTP 416)"),
                total_on_disk: offset,
                expected_total: None,
                content_type,
                restart_required: true,
            };
        }
        if status.is_client_error() || status.is_server_error() {
            let kind = classify_status_code(status);
            let reason = status.canonical_reason().unwrap_or_default();
            return StreamOutcome {
                ok: false,
                status_code,
                error_kind: Some(kind),
                error_message: format!(
                    "HTTP {} ({}) {} for url: {url}",
                    status.as_u16(),
                    kind.as_str(),
                    reason
                )
                .trim()
                .to_string(),
                total_on_disk: offset,
                expected_total: None,
                content_type,
                restart_required: false,
            };
        }

        let content_range = header_string(&response, CONTENT_RANGE);
        let effective_offset =
            if offset > 0 && status != StatusCode::PARTIAL_CONTENT && content_range.is_empty() {
                tracing::info!("DOWNLOAD [{job_label}]: le serveur ignore Range - reprise à zéro");
                0
            } else {
                offset
            };
        let expected_total = expected_total_size(status, &response, effective_offset);

        let mut file = if effective_offset > 0 {
            match OpenOptions::new().append(true).open(part_path).await {
                Ok(file) => file,
                Err(error) => return local_io_failure(error, effective_offset),
            }
        } else {
            match File::create(part_path).await {
                Ok(file) => file,
                Err(error) => return local_io_failure(error, effective_offset),
            }
        };

        let display_name = target_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("archive");
        let mut last_emit = Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap_or_else(Instant::now);
        let mut written = 0_u64;
        if let Some(callback) = &progress {
            let start = expected_total
                .map(|total| ((effective_offset as f64 / total as f64) * 100.0) as u32)
                .unwrap_or(0);
            callback(
                "downloading",
                start.min(100),
                &format!("Starting download of {display_name}"),
            );
        }

        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = match chunk {
                Ok(bytes) => bytes,
                Err(error) => {
                    return transport_failure(
                        error,
                        effective_offset + written,
                        self.config.chunk_timeout_s,
                    )
                }
            };
            if bytes.is_empty() {
                continue;
            }
            if let Err(error) = file.write_all(&bytes).await {
                return local_io_failure(error, effective_offset + written);
            }
            written += bytes.len() as u64;
            if let Some(callback) = &progress {
                if let Some(total) = expected_total {
                    if last_emit.elapsed()
                        >= Duration::from_secs_f64(self.config.progress_min_interval_s)
                    {
                        last_emit = Instant::now();
                        let done = effective_offset + written;
                        let percentage =
                            ((done as f64 / total.max(1) as f64) * 100.0).min(100.0) as u32;
                        callback(
                            "downloading",
                            percentage,
                            &format!(
                                "{} / {}",
                                format_bytes_human(done),
                                format_bytes_human(total)
                            ),
                        );
                    }
                }
            }
        }
        if let Err(error) = file.flush().await {
            return local_io_failure(error, effective_offset + written);
        }

        let total_on_disk = effective_offset + written;
        if let Some(expected) = expected_total {
            if total_on_disk != expected {
                return StreamOutcome {
                    ok: false,
                    status_code,
                    error_kind: Some(ErrorKind::Incomplete),
                    error_message: format!(
                        "Incomplete download: {total_on_disk} bytes received, {expected} expected"
                    ),
                    total_on_disk,
                    expected_total,
                    content_type,
                    restart_required: total_on_disk > expected,
                };
            }
        }

        StreamOutcome {
            ok: true,
            status_code,
            error_kind: None,
            error_message: String::new(),
            total_on_disk,
            expected_total,
            content_type,
            restart_required: false,
        }
    }

    async fn finalize_download(
        &self,
        part_path: &Path,
        target_path: &Path,
        content_type: &str,
        url: &str,
        expected_size: Option<u64>,
    ) -> (bool, ErrorKind, String) {
        let final_bytes = match fs::metadata(part_path).await {
            Ok(metadata) => metadata.len(),
            Err(_) => {
                return (
                    false,
                    ErrorKind::Incomplete,
                    format!(
                        "Downloaded data missing for {}",
                        target_path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("archive")
                    ),
                )
            }
        };
        if let Some(expected) = expected_size {
            if final_bytes != expected {
                return (
                    false,
                    ErrorKind::Incomplete,
                    format!(
                        "Incomplete download: {final_bytes} bytes received, {expected} expected"
                    ),
                );
            }
        }
        let expected_zip = target_path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
            || content_type.to_ascii_lowercase().contains("zip")
            || url.to_ascii_lowercase().contains(".zip");
        if !self
            .validate_download(part_path, content_type, url, final_bytes, expected_zip)
            .await
        {
            return (
                false,
                ErrorKind::InvalidPayload,
                format!(
                    "Invalid ZIP response (Content-Type='{content_type}', Size={final_bytes} bytes)"
                ),
            );
        }
        if let Err(error) = fs::rename(part_path, target_path).await {
            return (
                false,
                ErrorKind::LocalIo,
                format!(
                    "Unable to finalize {}: {error}",
                    target_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("archive")
                ),
            );
        }
        (true, ErrorKind::Internal, String::new())
    }

    async fn validate_download(
        &self,
        path: &Path,
        content_type: &str,
        url: &str,
        size_bytes: u64,
        expected_zip: bool,
    ) -> bool {
        if size_bytes == 0 {
            return false;
        }
        let content_type = content_type.to_ascii_lowercase();
        if content_type.contains("text/html")
            || content_type.contains("application/json")
            || content_type.contains("text/xml")
            || content_type.contains("application/xml")
        {
            return false;
        }
        let is_folder_link = url.to_ascii_lowercase().contains("/scl/fo/");
        let looks_like_zip = content_type.contains("zip") || content_type.contains("octet-stream");
        if is_folder_link && (size_bytes < self.config.min_zip_bytes || !looks_like_zip) {
            return false;
        }
        if expected_zip && self.config.validate_zip {
            return is_valid_zip_payload(path).await;
        }
        true
    }
}

fn transport_failure(
    error: reqwest::Error,
    total_on_disk: u64,
    chunk_timeout_s: u64,
) -> StreamOutcome {
    let kind = if error.is_timeout() {
        ErrorKind::Stalled
    } else if error.is_body() {
        ErrorKind::Incomplete
    } else {
        ErrorKind::Network
    };
    let message = match kind {
        ErrorKind::Stalled => format!(
            "Download stalled (timeout after {chunk_timeout_s}s with no data received): {error}"
        ),
        ErrorKind::Incomplete => format!("Incomplete download: {error}"),
        _ => format!("Network error: {error}"),
    };
    StreamOutcome {
        ok: false,
        status_code: None,
        error_kind: Some(kind),
        error_message: message,
        total_on_disk,
        expected_total: None,
        content_type: String::new(),
        restart_required: false,
    }
}

fn local_io_failure(error: std::io::Error, total_on_disk: u64) -> StreamOutcome {
    StreamOutcome {
        ok: false,
        status_code: None,
        error_kind: Some(ErrorKind::LocalIo),
        error_message: format!("Local write error: {error}"),
        total_on_disk,
        expected_total: None,
        content_type: String::new(),
        restart_required: false,
    }
}

fn classify_status_code(status: StatusCode) -> ErrorKind {
    match status.as_u16() {
        404 | 410 => ErrorKind::NotFound,
        401 | 403 => ErrorKind::Auth,
        _ => ErrorKind::HttpError,
    }
}

fn header_string(response: &reqwest::Response, name: reqwest::header::HeaderName) -> String {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

fn header_u64(response: &reqwest::Response, name: reqwest::header::HeaderName) -> Option<u64> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
}

fn expected_total_size(
    status: StatusCode,
    response: &reqwest::Response,
    effective_offset: u64,
) -> Option<u64> {
    let content_range = header_string(response, CONTENT_RANGE);
    if let Some(captured) = content_range_total_regex()
        .captures(&content_range)
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<u64>().ok())
    {
        return Some(captured);
    }
    let content_length = header_u64(response, CONTENT_LENGTH)?;
    if status == StatusCode::PARTIAL_CONTENT {
        Some(effective_offset + content_length)
    } else {
        Some(content_length)
    }
}

pub fn looks_like_zip(content_type: &str, content_disposition: &str) -> bool {
    if content_type.is_empty() && content_disposition.is_empty() {
        return false;
    }
    let content_type = content_type.to_ascii_lowercase();
    let content_disposition = content_disposition.to_ascii_lowercase();
    content_type.contains("zip")
        || content_type.contains("octet-stream")
        || content_disposition.contains("filename")
}

pub fn sanitize_filename(value: &str, max_length: usize) -> String {
    let mut sanitized: String = value
        .trim()
        .replace(' ', "_")
        .chars()
        .filter(|character| character.is_alphanumeric() || matches!(character, '_' | '-' | '.'))
        .collect();
    if sanitized.is_empty() {
        sanitized = "fichier_sans_nom_valide".into();
    }
    if sanitized.chars().count() > max_length {
        let path = Path::new(&sanitized);
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| format!(".{extension}"))
            .unwrap_or_default();
        let stem_length = max_length.saturating_sub(extension.chars().count());
        let stem: String = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("fichier")
            .chars()
            .take(stem_length)
            .collect();
        sanitized = format!("{stem}{extension}");
    }
    sanitized
}

pub fn resolve_target_filename(
    forced_filename: Option<&str>,
    content_disposition: &str,
    timestamp: &str,
    url: &str,
) -> String {
    let filename = if let Some(forced) = forced_filename.filter(|value| !value.trim().is_empty()) {
        let name = Path::new(forced.trim())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(forced.trim());
        let mut safe = sanitize_filename(name, 230);
        if is_dropbox_folder(url) && !has_archive_extension(&safe) {
            safe = replace_extension_with_zip(&safe);
        }
        safe
    } else {
        extract_filename(content_disposition, timestamp, url)
    };
    if is_dropbox_folder(url) && !has_archive_extension(&filename) {
        replace_extension_with_zip(&filename)
    } else {
        filename
    }
}

fn extract_filename(content_disposition: &str, timestamp: &str, url: &str) -> String {
    let default = format!("download_{}", clean_timestamp(timestamp));
    let utf8 = utf8_filename_regex();
    let simple = quoted_filename_regex();
    let mut filename = if content_disposition.is_empty() {
        default
    } else if let Some(captures) = utf8.captures(content_disposition) {
        let decoded = percent_encoding::percent_decode_str(&captures[1])
            .decode_utf8_lossy()
            .to_string();
        sanitize_filename(&decoded, 230)
    } else if let Some(captures) = simple.captures(content_disposition) {
        let extracted = &captures[1];
        let decoded = if extracted.contains('%') {
            percent_encoding::percent_decode_str(extracted)
                .decode_utf8_lossy()
                .to_string()
        } else {
            extracted.to_string()
        };
        sanitize_filename(&decoded, 230)
    } else {
        default
    };
    if is_dropbox_folder(url) && !has_archive_extension(&filename) {
        filename = replace_extension_with_zip(&filename);
    }
    filename
}

fn is_dropbox_folder(url: &str) -> bool {
    url.to_ascii_lowercase().contains("dropbox.com/scl/fo/")
}

fn has_archive_extension(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    [".zip", ".rar", ".7z", ".tar", ".gz", ".bz2"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

fn replace_extension_with_zip(filename: &str) -> String {
    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(filename);
    format!("{stem}.zip")
}

pub fn resolve_filepath_conflicts(output_dir: &Path, filename: &str) -> PathBuf {
    let mut path = output_dir.join(filename);
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("archive")
        .to_string();
    let suffix = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    for counter in 1..=1000 {
        path = output_dir.join(format!("{stem}_{counter}{suffix}"));
        if !path.exists() {
            return path;
        }
    }
    path
}

pub fn format_bytes_human(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes}B");
    }
    let kilobytes = bytes as f64 / 1024.0;
    if kilobytes < 1024.0 {
        return format!("{kilobytes:.1}KB");
    }
    format!("{:.1}MB", kilobytes / 1024.0)
}

async fn is_valid_zip_payload(path: &Path) -> bool {
    let metadata = match fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    if metadata.len() < 22 {
        return false;
    }
    let mut file = match File::open(path).await {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut header = [0_u8; 4];
    use tokio::io::AsyncReadExt;
    if file.read_exact(&mut header).await.is_err() || &header[..2] != b"PK" {
        return false;
    }
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::File::open(path)
            .ok()
            .and_then(|file| zip::ZipArchive::new(file).ok())
            .is_some()
    })
    .await
    .unwrap_or(false)
}

fn part_path_for(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive");
    target.with_file_name(format!("{name}.part"))
}

async fn partial_size(part: &Path) -> u64 {
    fs::metadata(part)
        .await
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

async fn discard_partial(part: &Path, job_label: &str) {
    if fs::metadata(part).await.is_ok() {
        if let Err(error) = fs::remove_file(part).await {
            tracing::warn!("DOWNLOAD [{job_label}]: suppression de .part impossible: {error}");
        } else {
            tracing::info!(
                "DOWNLOAD [{job_label}]: fichier partiel supprimé ({})",
                part.display()
            );
        }
    }
}

fn clean_timestamp(timestamp: &str) -> String {
    timestamp
        .replace('/', "")
        .replace(' ', "_")
        .replace(':', "")
}

fn new_download_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let mixed = nanos ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("csv_{:08x}", (mixed & 0xffff_ffff) as u32)
}

fn content_range_total_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"/(\d+)\s*$").expect("motif Content-Range"))
}

fn utf8_filename_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"(?i)filename\*=UTF-8''([^;\n\r]+)"#).expect("motif filename UTF-8")
    })
}

fn quoted_filename_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r#"(?i)filename="([^"]+)""#).expect("motif filename"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderMap, HeaderValue, StatusCode as AxumStatus};
    use axum::response::Response;
    use axum::routing::get;
    use axum::Router;
    use std::io::{Cursor, Write};
    use std::sync::atomic::AtomicUsize;

    fn zip_bytes(size: usize) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            writer
                .start_file(
                    "payload.bin",
                    zip::write::SimpleFileOptions::default()
                        .compression_method(zip::CompressionMethod::Stored),
                )
                .unwrap();
            writer.write_all(&vec![0_u8; size]).unwrap();
            writer.finish().unwrap();
        }
        buffer.into_inner()
    }

    #[derive(Clone)]
    struct MockState {
        payload: Arc<Vec<u8>>,
        ignore_range: bool,
        status: u16,
        content_type: &'static str,
        force_length: Option<u64>,
        reject_first_get: bool,
        requests: Arc<AtomicUsize>,
    }

    async fn mock_handler(
        method: axum::http::Method,
        State(state): State<MockState>,
        headers: HeaderMap,
    ) -> Response {
        let request_index = if method == axum::http::Method::GET {
            state.requests.fetch_add(1, Ordering::SeqCst)
        } else {
            0
        };
        if state.reject_first_get && request_index == 0 {
            return Response::builder()
                .status(AxumStatus::RANGE_NOT_SATISFIABLE)
                .body(Body::empty())
                .unwrap();
        }
        if state.status >= 400 {
            return Response::builder()
                .status(state.status)
                .body(Body::empty())
                .unwrap();
        }
        let total = state.payload.len();
        let range = headers
            .get("range")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("bytes="))
            .and_then(|value| value.split('-').next())
            .and_then(|value| value.parse::<usize>().ok());
        let (status, body, content_range, length) = match range {
            Some(offset) if !state.ignore_range && offset < total => (
                AxumStatus::PARTIAL_CONTENT,
                state.payload[offset..].to_vec(),
                Some(format!("bytes {offset}-{}/{total}", total - 1)),
                total - offset,
            ),
            _ => (AxumStatus::OK, (*state.payload).clone(), None, total),
        };
        let mut builder = Response::builder()
            .status(status)
            .header("content-type", state.content_type)
            .header(
                "content-length",
                state.force_length.unwrap_or(length as u64).to_string(),
            )
            .header("accept-ranges", "bytes")
            .header(
                "content-disposition",
                "attachment; filename=\"153 Camille.zip\"",
            );
        if let Some(content_range) = content_range {
            builder = builder.header("content-range", content_range);
        }
        builder.body(Body::from(body)).unwrap()
    }

    async fn spawn_mock(state: MockState) -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route("/file", get(mock_handler).head(mock_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}/file"), handle)
    }

    fn downloader() -> Downloader {
        let config = DownloadConfig {
            allow_private_hosts: true,
            backoff_s: 0.0,
            ..DownloadConfig::default()
        };
        Downloader::new(&config).unwrap()
    }

    #[test]
    fn given_private_addresses_when_classified_then_rejected() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fe80::1",
            "fc00::1",
        ] {
            assert!(!is_public_ip(address.parse().unwrap()), "{address}");
        }
        assert!(is_public_ip("93.184.216.34".parse().unwrap()));
        assert!(is_public_ip(
            "2606:2800:220:1:248:1893:25c8:1946".parse().unwrap()
        ));
    }

    #[tokio::test]
    async fn given_localhost_when_public_resolver_then_rejected() {
        use reqwest::dns::Resolve;
        let name = "localhost".parse().unwrap();
        assert!(PublicOnlyResolver.resolve(name).await.is_err());
    }

    #[test]
    fn sanitizes_and_extracts_filenames() {
        assert_eq!(sanitize_filename("61 Camille.zip", 230), "61_Camille.zip");
        assert_eq!(
            extract_filename("attachment; filename=\"test_file.zip\"", "2024-01-01", ""),
            "test_file.zip"
        );
        assert_eq!(
            extract_filename(
                "attachment; filename*=UTF-8''test%20file.zip",
                "2024-01-01",
                ""
            ),
            "test_file.zip"
        );
        assert_eq!(
            extract_filename(
                "attachment; filename=\"folder\"",
                "2024-01-01",
                "https://www.dropbox.com/scl/fo/abc"
            ),
            "folder.zip"
        );
        assert!(extract_filename("", "2024/01/01 12:00:00", "").starts_with("download_20240101"));
    }

    #[test]
    fn resolves_conflicts_with_a_counter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("test.zip"), b"x").unwrap();
        let path = resolve_filepath_conflicts(dir.path(), "test.zip");
        assert_eq!(path.file_name().unwrap(), "test_1.zip");
    }

    #[tokio::test]
    async fn downloads_and_validates_a_zip() {
        let payload = zip_bytes(2048);
        let state = MockState {
            payload: Arc::new(payload.clone()),
            ignore_range: false,
            status: 200,
            content_type: "application/zip",
            force_length: None,
            reject_first_get: false,
            requests: Arc::new(AtomicUsize::new(0)),
        };
        let (url, handle) = spawn_mock(state).await;
        let dir = tempfile::tempdir().unwrap();
        let result = downloader()
            .download(
                &url,
                "2026-09-21 16:47:14",
                dir.path(),
                Some("153 Camille.zip"),
                None,
            )
            .await;
        handle.abort();
        assert!(result.success, "{}", result.message);
        assert_eq!(result.filename, "153_Camille.zip");
        assert_eq!(
            std::fs::read(dir.path().join("153_Camille.zip")).unwrap(),
            payload
        );
        assert!(!dir.path().join("153_Camille.zip.part").exists());
    }

    #[tokio::test]
    async fn resumes_an_existing_part_with_range() {
        let payload = zip_bytes(4096);
        let state = MockState {
            payload: Arc::new(payload.clone()),
            ignore_range: false,
            status: 200,
            content_type: "application/zip",
            force_length: None,
            reject_first_get: false,
            requests: Arc::new(AtomicUsize::new(0)),
        };
        let (url, handle) = spawn_mock(state).await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("197_Camille.zip.part"), &payload[..500]).unwrap();
        let result = downloader()
            .download(
                &url,
                "2026-09-21 16:47:14",
                dir.path(),
                Some("197 Camille.zip"),
                None,
            )
            .await;
        handle.abort();
        assert!(result.success, "{}", result.message);
        assert_eq!(result.resumed_from, 500);
        assert_eq!(
            std::fs::read(dir.path().join("197_Camille.zip")).unwrap(),
            payload
        );
    }

    #[tokio::test]
    async fn restarts_when_range_is_rejected() {
        let payload = zip_bytes(2048);
        let state = MockState {
            payload: Arc::new(payload.clone()),
            ignore_range: false,
            status: 200,
            content_type: "application/zip",
            force_length: None,
            reject_first_get: true,
            requests: Arc::new(AtomicUsize::new(0)),
        };
        let (url, handle) = spawn_mock(state).await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("108_Camille.zip.part"), b"garbage").unwrap();
        let result = downloader()
            .download(
                &url,
                "2026-09-21 11:15:38",
                dir.path(),
                Some("108 Camille.zip"),
                None,
            )
            .await;
        handle.abort();
        assert!(result.success, "{}", result.message);
        assert_eq!(
            std::fs::read(dir.path().join("108_Camille.zip")).unwrap(),
            payload
        );
    }

    #[tokio::test]
    async fn ignores_range_when_the_server_sends_the_full_body() {
        let payload = zip_bytes(2048);
        let state = MockState {
            payload: Arc::new(payload.clone()),
            ignore_range: true,
            status: 200,
            content_type: "application/zip",
            force_length: None,
            reject_first_get: false,
            requests: Arc::new(AtomicUsize::new(0)),
        };
        let (url, handle) = spawn_mock(state).await;
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("194_Camille.zip.part"), b"stale").unwrap();
        let result = downloader()
            .download(
                &url,
                "2026-09-21 09:57:14",
                dir.path(),
                Some("194 Camille.zip"),
                None,
            )
            .await;
        handle.abort();
        assert!(result.success, "{}", result.message);
        assert_eq!(
            std::fs::read(dir.path().join("194_Camille.zip")).unwrap(),
            payload
        );
    }

    #[tokio::test]
    async fn rejects_html_payloads() {
        let state = MockState {
            payload: Arc::new(b"<html>error</html>".to_vec()),
            ignore_range: false,
            status: 200,
            content_type: "text/html",
            force_length: None,
            reject_first_get: false,
            requests: Arc::new(AtomicUsize::new(0)),
        };
        let (url, handle) = spawn_mock(state).await;
        let dir = tempfile::tempdir().unwrap();
        let result = downloader()
            .download(
                &url,
                "2026-09-21 10:00:00",
                dir.path(),
                Some("broken.zip"),
                None,
            )
            .await;
        handle.abort();
        assert!(!result.success);
        assert_eq!(result.error_kind, Some(ErrorKind::InvalidPayload));
        assert!(!dir.path().join("broken.zip").exists());
    }

    #[tokio::test]
    async fn reports_missing_objects_as_not_found() {
        let state = MockState {
            payload: Arc::new(Vec::new()),
            ignore_range: false,
            status: 404,
            content_type: "text/plain",
            force_length: None,
            reject_first_get: false,
            requests: Arc::new(AtomicUsize::new(0)),
        };
        let (url, handle) = spawn_mock(state).await;
        let dir = tempfile::tempdir().unwrap();
        let result = downloader()
            .download(
                &url,
                "2026-09-21 10:00:00",
                dir.path(),
                Some("missing.zip"),
                None,
            )
            .await;
        handle.abort();
        assert!(!result.success);
        assert_eq!(result.error_kind, Some(ErrorKind::NotFound));
        assert_eq!(result.status_code, Some(404));
        assert_eq!(result.attempts, 1);
    }

    #[tokio::test]
    async fn keeps_a_part_file_when_the_size_is_incomplete() {
        let payload = zip_bytes(4096);
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("197_Camille.zip.part");
        let target = dir.path().join("197_Camille.zip");
        std::fs::write(&part, &payload).unwrap();
        let (finalized, kind, message) = downloader()
            .finalize_download(
                &part,
                &target,
                "application/zip",
                "https://server.kidpixel.workers.dev/dropbox/a/b/file",
                Some(payload.len() as u64 + 100),
            )
            .await;
        assert!(!finalized);
        assert_eq!(kind, ErrorKind::Incomplete);
        assert!(message.contains("Incomplete download"));
        assert!(part.exists());
        assert!(!target.exists());
    }

    #[test]
    fn classifies_http_status_codes() {
        assert_eq!(
            classify_status_code(StatusCode::NOT_FOUND),
            ErrorKind::NotFound
        );
        assert_eq!(classify_status_code(StatusCode::GONE), ErrorKind::NotFound);
        assert_eq!(classify_status_code(StatusCode::FORBIDDEN), ErrorKind::Auth);
        assert_eq!(
            classify_status_code(StatusCode::SERVICE_UNAVAILABLE),
            ErrorKind::HttpError
        );
        assert!(looks_like_zip("application/zip", ""));
        assert!(looks_like_zip("", "filename=test.zip"));
        assert!(!looks_like_zip("", ""));
        let _ = HeaderValue::from_static("bytes");
    }
}
