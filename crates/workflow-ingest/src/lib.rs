//! Ingestion des liens Dropbox / R2.
//!
//! Ce crate est l'unique implémentation du moniteur webhook, de l'historique
//! SQLite, du téléchargement reprenable et du suivi des téléchargements pour
//! l'interface. Le serveur ne fait que l'exposer.

pub mod download;
pub mod history;
pub mod monitor;
pub mod service;
pub mod tracker;
pub mod url;
pub mod webhook;

pub use download::{DownloadResult, Downloader, ErrorKind};
pub use history::{cooldown_remaining_s, FailureRecord, HistoryRepository, ImportReport};
pub use monitor::{plan_downloads, DownloadPlan, PlanContext};
pub use service::{validate_monitor_startup, IngestionService, MonitorHandle, MonitorStatus};
pub use tracker::{DownloadRecord, DownloadTracker};
pub use url::{classify_url_type, normalize_url};
pub use webhook::{parse_records, WebhookClient, WebhookRecord, WebhookStatus};
