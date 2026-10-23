//! Lecture et cache du webhook de liens.
//!
//! Reproduit `services/webhook_service.py` : deux schémas JSON acceptés,
//! normalisation des timestamps, cache TTL et trois tentatives.

use crate::url::classify_url_type;
use chrono::{DateTime, Local, NaiveDateTime};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use workflow_core::WebhookConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WebhookRecord {
    pub url: String,
    #[serde(default)]
    pub fallback_url: String,
    #[serde(default)]
    pub original_filename: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub url_type: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WebhookStatus {
    pub available: bool,
    pub last_fetch_ts: Option<String>,
    pub error: Option<String>,
    pub records: usize,
}

struct CacheState {
    data: Option<Vec<WebhookRecord>>,
    fetched_at: Option<Instant>,
    status: WebhookStatus,
}

pub struct WebhookClient {
    url: String,
    cache_ttl: Duration,
    timeout: Duration,
    client: Client,
    state: Mutex<CacheState>,
}

impl WebhookClient {
    pub fn new(config: &WebhookConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(config.timeout_s.max(1)))
            .timeout(Duration::from_secs(config.timeout_s.max(1)))
            .build()?;
        Ok(Self {
            url: config.url.clone(),
            cache_ttl: Duration::from_secs(config.cache_ttl_s),
            timeout: Duration::from_secs(config.timeout_s.max(1)),
            client,
            state: Mutex::new(CacheState {
                data: None,
                fetched_at: None,
                status: WebhookStatus::default(),
            }),
        })
    }

    pub fn status(&self) -> WebhookStatus {
        self.state
            .lock()
            .expect("cache webhook verrouillé")
            .status
            .clone()
    }

    /// Récupère les enregistrements, avec cache. `None` en cas d'échec dur.
    pub async fn fetch(&self) -> Option<Vec<WebhookRecord>> {
        {
            let state = self.state.lock().expect("cache webhook verrouillé");
            if let (Some(data), Some(fetched_at)) = (&state.data, state.fetched_at) {
                if fetched_at.elapsed() < self.cache_ttl {
                    return Some(data.clone());
                }
            }
        }

        let mut last_error = None;
        for attempt in 1..=3 {
            match self.fetch_once().await {
                Ok(records) => {
                    self.record_success(&records);
                    return Some(records);
                }
                Err(error) => {
                    last_error = Some(error.to_string());
                    self.record_failure(last_error.clone());
                    if attempt < 3 {
                        tokio::time::sleep(Duration::from_secs(2_u64.pow(attempt - 1))).await;
                    }
                }
            }
        }
        tracing::error!(
            "webhook: échec après 3 tentatives ({}): {}",
            self.url,
            last_error.unwrap_or_default()
        );
        None
    }

    fn record_success(&self, records: &[WebhookRecord]) {
        let mut state = self.state.lock().expect("cache webhook verrouillé");
        state.data = Some(records.to_vec());
        state.fetched_at = Some(Instant::now());
        state.status = WebhookStatus {
            available: true,
            last_fetch_ts: Some(now_local_string()),
            error: None,
            records: records.len(),
        };
    }

    fn record_failure(&self, error: Option<String>) {
        let mut state = self.state.lock().expect("cache webhook verrouillé");
        state.status.available = false;
        state.status.last_fetch_ts = Some(now_local_string());
        state.status.error = error;
    }

    async fn fetch_once(&self) -> anyhow::Result<Vec<WebhookRecord>> {
        if self.url.trim().is_empty() {
            anyhow::bail!("webhook.url est vide");
        }
        let response = self
            .client
            .get(&self.url)
            .timeout(self.timeout)
            .send()
            .await?;
        let response = response.error_for_status()?;
        let value: Value = response.json().await?;
        Ok(parse_records(&value))
    }
}

/// Convertit le JSON brut du webhook en lignes normalisées.
pub fn parse_records(value: &Value) -> Vec<WebhookRecord> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let mut records = Vec::new();
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let source_url = string_field(object.get("source_url"));
        let r2_url = string_field(object.get("r2_url"));
        let original_filename = string_field(object.get("original_filename"));
        let provider = string_field(object.get("provider"));
        let created_at = normalize_timestamp(object.get("created_at").and_then(Value::as_str));

        if !r2_url.is_empty() || !source_url.is_empty() {
            let has_r2 = !r2_url.is_empty();
            let preferred = if has_r2 {
                r2_url.clone()
            } else {
                source_url.clone()
            };
            let fallback = if has_r2 && !source_url.is_empty() && source_url != preferred {
                source_url.clone()
            } else {
                String::new()
            };
            let url_type = if provider.is_empty() {
                classify_url_type(&preferred).to_string()
            } else {
                provider.to_ascii_lowercase()
            };
            records.push(WebhookRecord {
                url: preferred,
                fallback_url: fallback,
                original_filename,
                provider,
                timestamp: created_at.unwrap_or_default(),
                source: "webhook".into(),
                url_type,
            });
            continue;
        }

        let url = string_field(object.get("url"));
        if url.is_empty() {
            continue;
        }
        let timestamp = normalize_timestamp(object.get("timestamp").and_then(Value::as_str))
            .unwrap_or_default();
        let source = match string_field(object.get("source")) {
            value if value.is_empty() => "webhook".to_string(),
            value => value,
        };
        let url_type = match string_field(object.get("url_type")) {
            value if value.is_empty() => classify_url_type(&url).to_string(),
            value => value,
        };
        records.push(WebhookRecord {
            url,
            fallback_url: String::new(),
            original_filename: String::new(),
            provider: String::new(),
            timestamp,
            source,
            url_type,
        });
    }
    records
}

fn string_field(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// Normalise un timestamp ISO. Les timestamps naïfs sont laissés tels quels,
/// comme `datetime.fromisoformat` dans le service Python.
pub fn normalize_timestamp(value: Option<&str>) -> Option<String> {
    let value = value?;
    if value.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(
            parsed
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string(),
        );
    }
    for format in ["%Y-%m-%dT%H:%M:%S%z", "%Y-%m-%dT%H:%M:%S%.f%z"] {
        if let Ok(parsed) = DateTime::parse_from_str(value, format) {
            return Some(
                parsed
                    .with_timezone(&Local)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string(),
            );
        }
    }
    if NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok() {
        return Some(value.to_string());
    }
    Some(value.to_string())
}

fn now_local_string() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_the_new_schema_and_prefers_r2() {
        let records = parse_records(&json!([{
            "source_url": "https://www.dropbox.com/scl/fo/abc?dl=0",
            "r2_url": "https://server.kidpixel.workers.dev/dropbox/a/b/file",
            "original_filename": "153 Camille.zip",
            "provider": "dropbox",
            "created_at": "2025-10-17T12:34:13+0200"
        }]));
        assert_eq!(records.len(), 1);
        assert_eq!(
            records[0].url,
            "https://server.kidpixel.workers.dev/dropbox/a/b/file"
        );
        assert_eq!(
            records[0].fallback_url,
            "https://www.dropbox.com/scl/fo/abc?dl=0"
        );
        assert_eq!(records[0].url_type, "dropbox");
        assert_eq!(records[0].original_filename, "153 Camille.zip");
        assert!(!records[0].timestamp.is_empty());
    }

    #[test]
    fn parses_the_legacy_schema_and_skips_empty_urls() {
        let records = parse_records(&json!([
            {"url": "https://fromsmash.com/file", "timestamp": "2025-10-17 12:34:13"},
            {"url": ""},
            "not an object",
            null
        ]));
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].url_type, "fromsmash");
        assert_eq!(records[0].source, "webhook");
    }

    #[test]
    fn normalizes_aware_timestamps_but_keeps_naive_ones() {
        assert_eq!(
            normalize_timestamp(Some("2025-10-17 12:34:13")).as_deref(),
            Some("2025-10-17 12:34:13")
        );
        assert_eq!(normalize_timestamp(None), None);
        assert_eq!(
            normalize_timestamp(Some("not-a-date")).as_deref(),
            Some("not-a-date")
        );
        assert!(normalize_timestamp(Some("2025-10-17T12:34:13Z")).is_some());
    }

    #[test]
    fn rejects_non_array_payloads() {
        assert!(parse_records(&json!({})).is_empty());
        assert!(parse_records(&json!("text")).is_empty());
    }
}
