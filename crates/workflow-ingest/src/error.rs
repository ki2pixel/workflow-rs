//! Erreur publique du crate d'ingestion.
//!
//! Les détails internes restent en `anyhow` ; la frontière expose un type
//! stable, exigé par le standard « thiserror en bibliothèque ». Les étapes
//! STEP2/3/4/7/8 conservent une dérogation documentée (D-026).

#[derive(Debug, thiserror::Error)]
pub enum IngestError {
    #[error("webhook: {0}")]
    Webhook(String),
    #[error("téléchargement: {0}")]
    Download(String),
    #[error("configuration: {0}")]
    Message(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type IngestResult<T> = std::result::Result<T, IngestError>;
