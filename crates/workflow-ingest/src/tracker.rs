//! Suivi mémoire des téléchargements pour `/api/csv_downloads_status`.
//!
//! Reprend `WorkflowState` : téléchargements actifs, historique borné à 20
//! entrées, et mêmes champs JSON que l'application Python.

use crate::url::normalize_url;
use chrono::{DateTime, Local, SecondsFormat, Utc};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const KEPT_LIMIT: usize = 20;

pub fn new_download_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let mixed = nanos ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("csv_{:08x}", (mixed & 0xffff_ffff) as u32)
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadRecord {
    pub id: String,
    pub filename: String,
    pub original_url: String,
    pub url: String,
    pub url_type: String,
    pub status: String,
    pub progress: u32,
    pub message: String,
    pub display_timestamp: String,
    pub csv_timestamp: Option<String>,
    pub start_time: String,
    #[serde(skip)]
    created_at: DateTime<Local>,
}

impl DownloadRecord {
    pub fn new(
        id: String,
        original_url: String,
        url_type: String,
        csv_timestamp: Option<String>,
    ) -> Self {
        let now = Local::now();
        Self {
            id,
            filename: "Détermination en cours...".into(),
            original_url: original_url.clone(),
            url: original_url,
            url_type,
            status: "pending".into(),
            progress: 0,
            message: "En attente de démarrage...".into(),
            display_timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            csv_timestamp,
            start_time: Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false),
            created_at: now,
        }
    }
}

struct Inner {
    active: HashMap<String, DownloadRecord>,
    kept: VecDeque<DownloadRecord>,
}

pub struct DownloadTracker {
    inner: Mutex<Inner>,
}

impl Default for DownloadTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl DownloadTracker {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                active: HashMap::new(),
                kept: VecDeque::new(),
            }),
        }
    }

    pub fn add(&self, record: DownloadRecord) {
        self.inner
            .lock()
            .expect("tracker verrouillé")
            .active
            .insert(record.id.clone(), record);
    }

    pub fn update(
        &self,
        id: &str,
        status: &str,
        progress: Option<u32>,
        message: Option<&str>,
        filename: Option<&str>,
    ) {
        let mut inner = self.inner.lock().expect("tracker verrouillé");
        let Some(record) = inner.active.get_mut(id) else {
            return;
        };
        record.status = status.to_string();
        if let Some(progress) = progress {
            record.progress = progress;
        }
        if let Some(message) = message {
            record.message = message.to_string();
        }
        if let Some(filename) = filename {
            record.filename = filename.to_string();
        }
        if matches!(
            status,
            "completed" | "failed" | "cancelled" | "unknown_error"
        ) {
            if let Some(record) = inner.active.remove(id) {
                inner.kept.push_back(record);
                while inner.kept.len() > KEPT_LIMIT {
                    inner.kept.pop_front();
                }
            }
        }
    }

    /// Liste triée comme la route Python : plus récent d'abord.
    pub fn status_list(&self) -> Vec<DownloadRecord> {
        let inner = self.inner.lock().expect("tracker verrouillé");
        let mut records: Vec<DownloadRecord> = inner
            .active
            .values()
            .chain(inner.kept.iter())
            .cloned()
            .collect();
        records.sort_by_key(|record| std::cmp::Reverse(record.created_at));
        records
    }

    /// URL normalisées déjà actives ou récemment terminées avec succès.
    pub fn tracked_urls(&self) -> HashSet<String> {
        let inner = self.inner.lock().expect("tracker verrouillé");
        let mut urls = HashSet::new();
        for record in inner.active.values().chain(inner.kept.iter()) {
            let status = record.status.to_ascii_lowercase();
            if matches!(status.as_str(), "failed" | "cancelled" | "unknown_error") {
                continue;
            }
            let raw = if !record.original_url.is_empty() {
                &record.original_url
            } else {
                &record.url
            };
            let normalized = normalize_url(raw);
            if !normalized.is_empty() {
                urls.insert(normalized);
            }
        }
        urls
    }

    pub fn active_count(&self) -> usize {
        self.inner.lock().expect("tracker verrouillé").active.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_completed_downloads_to_a_bounded_history() {
        let tracker = DownloadTracker::new();
        let record = DownloadRecord::new(
            "csv_00000001".into(),
            "https://www.dropbox.com/scl/fo/abc?dl=1".into(),
            "dropbox".into(),
            Some("2026-09-21 16:47:14".into()),
        );
        tracker.add(record);
        assert_eq!(tracker.active_count(), 1);
        tracker.update(
            "csv_00000001",
            "completed",
            Some(100),
            Some("ok"),
            Some("153_Camille.zip"),
        );
        assert_eq!(tracker.active_count(), 0);
        let statuses = tracker.status_list();
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].status, "completed");
        assert_eq!(statuses[0].filename, "153_Camille.zip");
        assert!(tracker
            .tracked_urls()
            .contains("https://www.dropbox.com/scl/fo/abc?dl=1"));
    }

    #[test]
    fn failed_downloads_do_not_block_a_retry() {
        let tracker = DownloadTracker::new();
        tracker.add(DownloadRecord::new(
            "csv_00000002".into(),
            "https://www.dropbox.com/scl/fo/abc?dl=1".into(),
            "dropbox".into(),
            None,
        ));
        tracker.update("csv_00000002", "failed", None, Some("boom"), None);
        assert!(tracker.tracked_urls().is_empty());
    }
}
