//! Erreurs typées du client desktop.

use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum ServicesError {
    #[error("URL invalide: {0}")]
    Url(#[from] url::ParseError),

    #[error("erreur HTTP: {0}")]
    Http(#[from] reqwest::Error),

    #[error("statut HTTP {status}: {body}")]
    HttpStatus { status: StatusCode, body: String },

    #[error("authentification expirée")]
    AuthExpired,

    #[error("worker-token absent ou invalide")]
    Token,

    #[error("serveur non Rust: {0}")]
    NotRustServer(String),

    #[error("réponse JSON invalide: {0}")]
    Json(#[from] serde_json::Error),

    #[error("configuration invalide: {0}")]
    Config(String),

    #[error("notification indisponible: {0}")]
    Notify(String),

    #[error("lecture/écriture: {0}")]
    Io(#[from] std::io::Error),
}
