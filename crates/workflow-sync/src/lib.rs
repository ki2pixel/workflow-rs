//! Synchronisation explicite entre l'état Python legacy et l'état Rust.
//!
//! Ce crate ne tourne jamais dans le serveur : il est appelé par
//! `workflow-cli sync`, après arrêt des deux backends. Il fusionne les états
//! qui ne peuvent pas être partagés sans changer les racines Python :
//!
//! - `download_history` (union bidirectionnelle, timestamps minimaux) ;
//! - `processed_archives.txt` (identité par nom de fichier, chemins remappés) ;
//! - archives non traitées des dossiers de téléchargement ;
//! - archives d'analyses STEP7 (identité projet + hash vidéo).
//!
//! Les échecs de téléchargement ne sont pas fusionnés : ce sont des cooldowns
//! transitoires, et une URL en échec d'un côté peut réussir de l'autre.

use serde::Serialize;

pub mod archives;
pub mod error;
pub mod history;
pub mod lock;
pub mod processed;
pub mod report;

pub use archives::{
    sync_downloads, sync_step7_archives, DownloadsMode, DownloadsSyncReport, Step7Mode,
    Step7SyncReport,
};
pub use error::{SyncError, SyncResult};
pub use history::{merge_history, HistoryMergeOptions, HistoryMergeReport};
pub use lock::{ensure_no_conflicting_processes, ensure_servers_stopped, SyncLock};
pub use processed::{merge_processed_archives, ProcessedMergeOptions, ProcessedMergeReport};
pub use report::{SyncOptions, SyncReport};

/// Sens de la synchronisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncDirection {
    Both,
    PythonToRust,
    RustToPython,
}

impl SyncDirection {
    pub fn includes_python(self) -> bool {
        matches!(self, Self::Both | Self::RustToPython)
    }

    pub fn includes_rust(self) -> bool {
        matches!(self, Self::Both | Self::PythonToRust)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::PythonToRust => "python-to-rust",
            Self::RustToPython => "rust-to-python",
        }
    }
}

/// Fusion ou abandon d'un état texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MergeMode {
    Merge,
    Skip,
}

impl MergeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Skip => "skip",
        }
    }
}

/// Point d'entrée public : convertit l'erreur interne `anyhow` en type stable.
pub fn run_sync(options: &SyncOptions) -> SyncResult<SyncReport> {
    report::run_sync(options).map_err(SyncError::from)
}
