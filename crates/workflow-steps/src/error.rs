use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error("lecture de {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("écriture de {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON invalide: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("verrou matériel: {0}")]
    Lock(String),
    #[error("schéma inattendu: {0}")]
    Schema(String),
}
