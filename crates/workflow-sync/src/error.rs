//! Erreur publique de la synchronisation.
//!
//! Les modules internes conservent `anyhow` (contexte de fichiers et de
//! SQLite) ; `run_sync` expose un type stable à la CLI.

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type SyncResult<T> = std::result::Result<T, SyncError>;
