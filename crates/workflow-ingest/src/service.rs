//! Orchestration de l'ingestion : webhook → décision → téléchargement →
//! historique → statut UI.

use crate::download::{DownloadResult, Downloader, ErrorKind, ProgressCallback};
use crate::history::HistoryRepository;
use crate::monitor::{plan_downloads, DownloadPlan, PlanContext};
use crate::tracker::{new_download_id, DownloadRecord, DownloadTracker};
use crate::url::normalize_url;
use crate::webhook::WebhookClient;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{watch, Semaphore};
use tokio::task::JoinHandle;
use workflow_core::{Config, DownloadConfig, WebhookConfig};

#[derive(Debug, Clone, Serialize)]
pub struct MonitorStatus {
    pub status: String,
    pub last_check: Option<String>,
    pub error: Option<String>,
}

pub struct MonitorHandle {
    shutdown: watch::Sender<bool>,
    join: JoinHandle<()>,
}

impl MonitorHandle {
    pub async fn stop(self) {
        let _ = self.shutdown.send(true);
        let _ = self.join.await;
    }
}

/// Vérifie les conditions d'activation du moniteur avant de démarrer le
/// serveur. Une base vide ferait télécharger tout le webhook ; un dossier
/// STEP1 différent ferait traiter les archives par personne.
pub fn validate_monitor_startup(config: &Config, history_count: u64) -> Result<()> {
    if !config.webhook.enabled {
        return Ok(());
    }
    if config.webhook.require_history && history_count == 0 {
        anyhow::bail!(
            "webhook activé mais download_history est vide: importez d'abord la base Python avec `workflow-cli history import --from <base>` (ou désactivez webhook.require_history)"
        );
    }
    let resolved = config.paths.resolve();
    if config.steps.step1.source_dir != resolved.downloads {
        anyhow::bail!(
            "webhook activé mais steps.step1.source_dir ({}) diffère de paths.downloads ({}): STEP1 ne verrait pas les archives téléchargées",
            config.steps.step1.source_dir.display(),
            resolved.downloads.display()
        );
    }
    Ok(())
}

pub struct IngestionService {
    webhook: WebhookClient,
    history: HistoryRepository,
    downloader: Arc<Downloader>,
    tracker: Arc<DownloadTracker>,
    monitor: Mutex<MonitorStatus>,
    webhook_config: WebhookConfig,
    download_config: DownloadConfig,
    download_dir: PathBuf,
    semaphore: Arc<Semaphore>,
}

impl IngestionService {
    pub fn new(config: &Config, history: HistoryRepository) -> Result<Self> {
        let resolved = config.paths.resolve();
        std::fs::create_dir_all(&resolved.downloads)
            .with_context(|| format!("création de {}", resolved.downloads.display()))?;
        let webhook = WebhookClient::new(&config.webhook)?;
        let downloader = Downloader::new(&config.download)?;
        Ok(Self {
            webhook,
            history,
            downloader: Arc::new(downloader),
            tracker: Arc::new(DownloadTracker::new()),
            monitor: Mutex::new(MonitorStatus {
                status: "stopped".into(),
                last_check: None,
                error: None,
            }),
            webhook_config: config.webhook.clone(),
            download_config: config.download.clone(),
            download_dir: resolved.downloads,
            semaphore: Arc::new(Semaphore::new(config.download.max_concurrent.max(1))),
        })
    }

    pub fn history(&self) -> &HistoryRepository {
        &self.history
    }

    pub fn tracker(&self) -> &Arc<DownloadTracker> {
        &self.tracker
    }

    pub fn monitor_status(&self) -> MonitorStatus {
        self.monitor
            .lock()
            .expect("statut moniteur verrouillé")
            .clone()
    }

    pub fn downloads_status(&self) -> Vec<DownloadRecord> {
        self.tracker.status_list()
    }

    pub fn monitor_status_json(&self) -> Value {
        let failures = self.history.list_failures(20).unwrap_or_default();
        let failed_count = self.history.failure_count().unwrap_or(0);
        json!({
            "csv_monitor": self.monitor_status(),
            "data_source": "webhook",
            "monitor_interval": self.webhook_config.interval_s,
            "failed_urls": failures,
            "failed_urls_count": failed_count,
            "webhook": self.webhook.status(),
        })
    }

    pub fn spawn_monitor(self: &Arc<Self>) -> MonitorHandle {
        let (shutdown, receiver) = watch::channel(false);
        let service = self.clone();
        let join = tokio::spawn(async move {
            service.run_monitor(receiver).await;
        });
        MonitorHandle { shutdown, join }
    }

    async fn run_monitor(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        if !self.webhook_config.enabled {
            self.set_monitor("stopped", None);
            let _ = shutdown.changed().await;
            return;
        }
        tracing::info!(
            "moniteur webhook démarré (intervalle {}s, cache {}s)",
            self.webhook_config.interval_s,
            self.webhook_config.cache_ttl_s
        );
        loop {
            if *shutdown.borrow() {
                break;
            }
            self.set_monitor("checking", None);
            match self.check_once().await {
                Ok(count) => {
                    self.set_monitor("active", None);
                    tracing::debug!("moniteur webhook: {count} téléchargement(s) planifié(s)");
                }
                Err(error) => {
                    self.set_monitor("error", Some(error.to_string()));
                    tracing::error!("moniteur webhook: {error:#}");
                }
            }
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(self.webhook_config.interval_s.max(1))) => {}
            }
        }
        self.set_monitor("stopped", None);
        tracing::info!("moniteur webhook arrêté");
    }

    fn set_monitor(&self, status: &str, error: Option<String>) {
        let mut monitor = self.monitor.lock().expect("statut moniteur verrouillé");
        monitor.status = status.to_string();
        monitor.last_check = Some(now_local_iso());
        monitor.error = error;
    }

    /// Un passage du moniteur. Public pour les tests et un futur `run-once`.
    pub async fn check_once(self: &Arc<Self>) -> Result<usize> {
        let records = self
            .webhook
            .fetch()
            .await
            .ok_or_else(|| anyhow::anyhow!("webhook indisponible"))?;
        let history = self.history.get_urls()?;
        let failures = self.history.get_failure_map()?;
        let tracked = self.tracker.tracked_urls();
        let plans = plan_downloads(
            &records,
            &PlanContext {
                history: &history,
                failures: &failures,
                tracked: &tracked,
                now: Local::now().naive_local(),
                interval_s: self.webhook_config.interval_s,
                cooldown_max_s: self.download_config.cooldown_max_s,
            },
        );
        let count = plans.len();
        for plan in plans {
            if self.webhook_config.dry_run {
                let timestamp = effective_timestamp(&plan.timestamp);
                self.history.upsert(&normalize_url(&plan.url), &timestamp)?;
                if !plan.fallback_url.is_empty() {
                    self.history
                        .upsert(&normalize_url(&plan.fallback_url), &timestamp)?;
                }
                tracing::info!(
                    "[DRY RUN] téléchargement évité pour {} (timestamp: {})",
                    plan.url,
                    plan.timestamp
                );
            } else {
                // Enregistrer avant de rendre la main : un lien en attente de
                // permis doit déjà être « suivi », sinon le passage suivant du
                // moniteur le replanifierait et lancerait un doublon.
                let id = new_download_id();
                self.tracker.add(DownloadRecord::new(
                    id.clone(),
                    plan.url.clone(),
                    "dropbox".into(),
                    Some(plan.timestamp.clone()),
                ));
                let service = self.clone();
                tokio::spawn(async move {
                    service.execute(plan, id).await;
                });
            }
        }
        Ok(count)
    }

    async fn execute(self: Arc<Self>, plan: DownloadPlan, id: String) {
        let Ok(_permit) = self.semaphore.acquire().await else {
            return;
        };

        let mut urls = vec![plan.url.clone()];
        if !plan.fallback_url.is_empty() && plan.fallback_url != plan.url {
            urls.push(plan.fallback_url.clone());
        }
        let mut fallback_first = false;
        if urls.len() == 2 {
            if let Ok(Some(failure)) = self.history.get_failure(&normalize_url(&plan.url)) {
                if failure.kind == "not_found" {
                    urls.reverse();
                    fallback_first = true;
                    tracing::info!(
                        "CSV DOWNLOAD: R2 déjà absent pour {} - repli Dropbox en premier",
                        plan.url
                    );
                }
            }
        }

        let forced = if plan.original_filename.trim().is_empty() {
            None
        } else {
            Some(plan.original_filename.as_str())
        };
        let mut used_fallback = fallback_first;
        let mut failed: Vec<DownloadResult> = Vec::new();
        let mut last_result: Option<DownloadResult> = None;

        for attempt_url in &urls {
            let tracker = self.tracker.clone();
            let tracker_id = id.clone();
            // Le downloader signale aussi `completed`/`failed` pour chaque
            // tentative. Le worker ne doit basculer le statut final qu'après
            // le repli éventuel, sinon un échec R2 masque un succès Dropbox.
            let callback: ProgressCallback = Arc::new(move |status, progress, message| {
                if status == "downloading" {
                    tracker.update(&tracker_id, status, Some(progress), Some(message), None);
                }
            });
            let result = self
                .downloader
                .download(
                    attempt_url,
                    &plan.timestamp,
                    &self.download_dir,
                    forced,
                    Some(callback),
                )
                .await;

            if result.success {
                if attempt_url != &plan.url {
                    used_fallback = true;
                }
                let message = if used_fallback {
                    format!("{} (R2 absent - repli Dropbox utilisé)", result.message)
                } else {
                    result.message.clone()
                };
                self.tracker.update(
                    &id,
                    "completed",
                    Some(100),
                    Some(&message),
                    Some(&result.filename),
                );
                let timestamp = effective_timestamp(&plan.timestamp);
                if let Err(error) = self.history.upsert(&normalize_url(&plan.url), &timestamp) {
                    tracing::warn!("CSV DOWNLOAD: historique non écrit: {error:#}");
                }
                if !plan.fallback_url.is_empty() {
                    if let Err(error) = self
                        .history
                        .upsert(&normalize_url(&plan.fallback_url), &timestamp)
                    {
                        tracing::warn!("CSV DOWNLOAD: historique non écrit: {error:#}");
                    }
                }
                for url in &urls {
                    if let Err(error) = self.history.clear_failure(&normalize_url(url)) {
                        tracing::warn!("CSV DOWNLOAD: échec non effacé: {error:#}");
                    }
                }
                tracing::info!(
                    "CSV DOWNLOAD: '{}' téléchargé ({} octets)",
                    result.filename,
                    result.size_bytes
                );
                return;
            }

            if result.error_kind == Some(ErrorKind::NotFound)
                && attempt_url == &plan.url
                && urls.len() == 2
            {
                tracing::warn!(
                    "CSV DOWNLOAD: R2_MISSING url={attempt_url} filename={} - repli Dropbox",
                    plan.original_filename
                );
            }
            last_result = Some(result.clone());
            failed.push(result);
        }

        for result in &failed {
            let url = result.url.as_deref().unwrap_or(&plan.url);
            let kind = result.error_kind.unwrap_or(ErrorKind::Internal);
            if let Err(error) = self.history.record_failure(
                &normalize_url(url),
                kind.as_str(),
                result.status_code.map(i64::from),
                &result.message,
            ) {
                tracing::warn!("CSV DOWNLOAD: échec d'enregistrement de l'erreur: {error:#}");
            }
        }
        let message = last_result
            .as_ref()
            .map(|result| result.message.clone())
            .unwrap_or_else(|| "Download failed".into());
        let filename = last_result
            .as_ref()
            .map(|result| result.filename.clone())
            .filter(|filename| !filename.is_empty())
            .unwrap_or_else(|| "N/A".into());
        self.tracker
            .update(&id, "failed", None, Some(&message), Some(&filename));
        tracing::error!("CSV DOWNLOAD: échec - {message}");
    }
}

fn now_local_iso() -> String {
    Local::now()
        .naive_local()
        .format("%Y-%m-%dT%H:%M:%S%.6f")
        .to_string()
}

/// Comme `CSVService.add_to_download_history_with_timestamp` : un timestamp
/// absent est remplacé par l'heure locale, un timestamp aware est converti en
/// heure locale, un timestamp naïf ou invalide est conservé tel quel.
fn effective_timestamp(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    }
    let parsed = DateTime::parse_from_rfc3339(raw).ok().or_else(|| {
        ["%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%dT%H:%M:%S%.f%z"]
            .iter()
            .find_map(|format| DateTime::parse_from_str(raw, format).ok())
    });
    if let Some(parsed) = parsed {
        return parsed
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
    }
    if NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").is_ok() {
        return raw.to_string();
    }
    raw.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::Response as HttpResponse;
    use axum::routing::get;
    use axum::{Json, Router};
    use serde_json::json;
    use std::io::{Cursor, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    async fn spawn_webhook(records: Value) -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route(
                "/webhook",
                get(|State(records): State<Arc<Value>>| async move { Json((*records).clone()) }),
            )
            .with_state(Arc::new(records));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}/webhook"), handle)
    }

    async fn spawn_downloads(
        payload: Vec<u8>,
        r2_status: u16,
        source_status: u16,
    ) -> (String, tokio::task::JoinHandle<()>) {
        #[derive(Clone)]
        struct MockState {
            payload: Arc<Vec<u8>>,
            r2_status: u16,
            source_status: u16,
            requests: Arc<AtomicUsize>,
        }
        async fn head(State(state): State<MockState>) -> HttpResponse<Body> {
            HttpResponse::builder()
                .status(200)
                .header("content-type", "application/zip")
                .header("content-length", state.payload.len().to_string())
                .header("accept-ranges", "bytes")
                .header(
                    "content-disposition",
                    "attachment; filename=\"153 Camille.zip\"",
                )
                .body(Body::empty())
                .unwrap()
        }
        async fn get_source(State(state): State<MockState>) -> HttpResponse<Body> {
            state.requests.fetch_add(1, Ordering::SeqCst);
            if state.source_status >= 400 {
                return HttpResponse::builder()
                    .status(state.source_status)
                    .body(Body::empty())
                    .unwrap();
            }
            HttpResponse::builder()
                .status(200)
                .header("content-type", "application/zip")
                .header("content-length", state.payload.len().to_string())
                .header("accept-ranges", "bytes")
                .body(Body::from((*state.payload).clone()))
                .unwrap()
        }
        async fn get_r2(State(state): State<MockState>) -> HttpResponse<Body> {
            HttpResponse::builder()
                .status(state.r2_status)
                .body(Body::empty())
                .unwrap()
        }
        let state = MockState {
            payload: Arc::new(payload),
            r2_status,
            source_status,
            requests: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route("/source", get(get_source).head(head))
            .route("/r2", get(get_r2).head(head))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{address}"), handle)
    }

    fn test_config(root: &std::path::Path, webhook_url: String) -> Config {
        let mut config = Config::default();
        config.paths.base = root.join("runtime");
        config.paths.downloads = Some(config.paths.base.join("downloads"));
        config.paths.projects = Some(config.paths.base.join("projets_extraits"));
        config.paths.logs = Some(config.paths.base.join("logs"));
        config.paths.history_db = Some(config.paths.base.join("history.sqlite3"));
        config.steps.step1.source_dir = config.paths.downloads.clone().unwrap();
        config.webhook.enabled = true;
        config.webhook.require_history = false;
        config.webhook.url = webhook_url;
        config.webhook.interval_s = 15;
        config.webhook.cache_ttl_s = 0;
        config.download.backoff_s = 0.0;
        config
    }

    async fn wait_for_status(service: &Arc<IngestionService>, status: &str) {
        for _ in 0..200 {
            if service
                .downloads_status()
                .iter()
                .any(|record| record.status == status)
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "statut {status} non atteint: {:?}",
            service.downloads_status()
        );
    }

    #[tokio::test]
    async fn dry_run_marks_history_without_downloading() {
        let dir = tempfile::tempdir().unwrap();
        let (webhook_url, webhook_handle) = spawn_webhook(json!([{
            "source_url": "https://www.dropbox.com/scl/fo/abc?dl=1",
            "original_filename": "153 Camille.zip",
            "created_at": "2026-09-21T16:47:14+0200",
            "provider": "dropbox"
        }]))
        .await;
        let mut config = test_config(dir.path(), webhook_url);
        config.webhook.dry_run = true;
        let history = HistoryRepository::open(&config.paths.resolve().history_db).unwrap();
        let service = Arc::new(IngestionService::new(&config, history.clone()).unwrap());
        assert_eq!(service.check_once().await.unwrap(), 1);
        webhook_handle.abort();
        assert_eq!(history.count().unwrap(), 1);
        assert!(service.downloads_status().is_empty());
        assert_eq!(
            std::fs::read_dir(config.paths.resolve().downloads)
                .unwrap()
                .count(),
            0
        );
    }

    fn monitor_config(root: &std::path::Path) -> Config {
        let mut config = Config::default();
        config.paths.base = root.join("runtime");
        config.paths.downloads = Some(config.paths.base.join("downloads"));
        config.steps.step1.source_dir = config.paths.downloads.clone().unwrap();
        config.webhook.enabled = true;
        config
    }

    #[test]
    fn startup_guard_requires_history_and_a_matching_step1_source() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = monitor_config(dir.path());
        assert!(validate_monitor_startup(&config, 0).is_err());
        assert!(validate_monitor_startup(&config, 911).is_ok());
        config.steps.step1.source_dir = dir.path().join("ailleurs");
        assert!(validate_monitor_startup(&config, 911).is_err());
        config.webhook.enabled = false;
        assert!(validate_monitor_startup(&config, 0).is_ok());
        config.webhook.enabled = true;
        config.webhook.require_history = false;
        config.steps.step1.source_dir = config.paths.downloads.clone().unwrap();
        assert!(validate_monitor_startup(&config, 0).is_ok());
    }

    #[test]
    fn normalizes_history_timestamps_like_python() {
        assert!(
            NaiveDateTime::parse_from_str(&effective_timestamp(""), "%Y-%m-%d %H:%M:%S").is_ok(),
            "un timestamp vide doit devenir l'heure locale"
        );
        assert_eq!(
            effective_timestamp("2026-09-21 16:47:14"),
            "2026-09-21 16:47:14"
        );
        assert_eq!(
            effective_timestamp("2026-09-21T16:47:14+0200"),
            DateTime::parse_from_str("2026-09-21T16:47:14+0200", "%Y-%m-%dT%H:%M:%S%z")
                .unwrap()
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        );
        assert_eq!(effective_timestamp("pas une date"), "pas une date");
    }

    #[tokio::test]
    async fn registers_queued_downloads_before_waiting_for_a_permit() {
        let dir = tempfile::tempdir().unwrap();
        let (webhook_url, webhook_handle) = spawn_webhook(json!([
            {
                "source_url": "https://www.dropbox.com/scl/fo/aaa?dl=1",
                "original_filename": "A.zip",
                "provider": "dropbox"
            },
            {
                "source_url": "https://www.dropbox.com/scl/fo/bbb?dl=1",
                "original_filename": "B.zip",
                "provider": "dropbox"
            }
        ]))
        .await;
        let mut config = test_config(dir.path(), webhook_url);
        config.download.max_concurrent = 1;
        let history = HistoryRepository::open(&config.paths.resolve().history_db).unwrap();
        let service = Arc::new(IngestionService::new(&config, history).unwrap());
        // Bloque le seul permis : les deux téléchargements restent « pending ».
        let permit = service.semaphore.clone().acquire_owned().await.unwrap();
        assert_eq!(service.check_once().await.unwrap(), 2);
        assert_eq!(service.tracker.active_count(), 2);
        assert_eq!(service.tracker.tracked_urls().len(), 2);
        drop(permit);
        webhook_handle.abort();
    }

    #[tokio::test]
    async fn dry_run_uses_the_current_time_when_the_webhook_has_no_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let (webhook_url, webhook_handle) = spawn_webhook(json!([{
            "source_url": "https://www.dropbox.com/scl/fo/abc?dl=1",
            "original_filename": "153 Camille.zip",
            "provider": "dropbox"
        }]))
        .await;
        let mut config = test_config(dir.path(), webhook_url);
        config.webhook.dry_run = true;
        let history = HistoryRepository::open(&config.paths.resolve().history_db).unwrap();
        let service = Arc::new(IngestionService::new(&config, history.clone()).unwrap());
        assert_eq!(service.check_once().await.unwrap(), 1);
        webhook_handle.abort();
        let timestamps = history.get_ts_by_url().unwrap();
        let timestamp = timestamps.values().next().unwrap();
        assert!(!timestamp.trim().is_empty());
    }

    #[tokio::test]
    async fn falls_back_to_dropbox_when_r2_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let payload = zip_bytes(2048);
        let (base, downloads_handle) = spawn_downloads(payload.clone(), 404, 200).await;
        let (webhook_url, webhook_handle) = spawn_webhook(json!([{
            "r2_url": format!("{base}/r2"),
            "source_url": format!("{base}/source"),
            "original_filename": "153 Camille.zip",
            "provider": "dropbox",
            "created_at": "2026-09-21T16:47:14+0200"
        }]))
        .await;
        let config = test_config(dir.path(), webhook_url);
        let history = HistoryRepository::open(&config.paths.resolve().history_db).unwrap();
        let service = Arc::new(IngestionService::new(&config, history.clone()).unwrap());
        assert_eq!(service.check_once().await.unwrap(), 1);
        wait_for_status(&service, "completed").await;
        webhook_handle.abort();
        downloads_handle.abort();

        let statuses = service.downloads_status();
        assert_eq!(statuses[0].filename, "153_Camille.zip");
        assert!(statuses[0].message.contains("repli Dropbox"));
        let downloaded = config.paths.resolve().downloads.join("153_Camille.zip");
        assert_eq!(std::fs::read(&downloaded).unwrap(), payload);
        assert_eq!(history.count().unwrap(), 2);
        assert_eq!(history.failure_count().unwrap(), 0);
    }

    #[tokio::test]
    async fn records_typed_failures_when_both_urls_fail() {
        let dir = tempfile::tempdir().unwrap();
        let (base, downloads_handle) = spawn_downloads(Vec::new(), 404, 404).await;
        let (webhook_url, webhook_handle) = spawn_webhook(json!([{
            "r2_url": format!("{base}/r2"),
            "source_url": format!("{base}/source"),
            "original_filename": "153 Camille.zip",
            "provider": "dropbox",
            "created_at": "2026-09-21T16:47:14+0200"
        }]))
        .await;
        let config = test_config(dir.path(), webhook_url);
        let history = HistoryRepository::open(&config.paths.resolve().history_db).unwrap();
        let service = Arc::new(IngestionService::new(&config, history.clone()).unwrap());
        assert_eq!(service.check_once().await.unwrap(), 1);
        wait_for_status(&service, "failed").await;
        webhook_handle.abort();
        downloads_handle.abort();
        assert_eq!(history.count().unwrap(), 0);
        assert_eq!(history.failure_count().unwrap(), 2);
        let failures = history.list_failures(10).unwrap();
        assert!(failures.iter().all(|failure| failure.kind == "not_found"));
    }
}
