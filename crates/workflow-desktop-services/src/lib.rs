//! Services non graphiques du client desktop Linux de `workflow-server`.
//!
//! Ce crate est partagé entre le client Tauri et le futur repli GTK4/Relm4.
//! Il ne dépend d'aucun toolkit graphique.

pub mod audio;
pub mod auth;
pub mod client;
pub mod error;
pub mod logs;
pub mod models;
pub mod notify;
pub mod polling;
pub mod telemetry;
pub mod tray;

pub use audio::{AudioPlayer, NoopAudio, RodioAudio, SoundEvent};
pub use auth::{parse_session_html, Session, WorkerToken};
pub use client::{validate_base_url, DesktopClient};
pub use error::ServicesError;
pub use models::{
    ActionResponse, DiskMetrics, GpuMemory, GpuMetrics, MemoryMetrics, PingResponse,
    SequenceOutcome, SequenceProgress, SequenceStatus, SpecificLog, StepKey, StepStatus,
    SystemMonitor,
};
pub use notify::{
    NoopNotifier, Notification, Notifier, NotifyRustNotifier, TransitionGate, Urgency,
};
pub use polling::{
    ConnectionState, PollingConfig, StatusSnapshot, StatusWatcher, TerminalKind, TerminalOutcome,
    WatcherStats, WatcherStatsSnapshot,
};
pub use tray::{
    NoopTray, TrayAction, TrayIconKey, TrayPresenter, TrayProgress, TrayState, TrayStatus,
    TrayVisualKey,
};
