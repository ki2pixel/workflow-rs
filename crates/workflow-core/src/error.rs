use std::path::PathBuf;

/// Erreurs de chargement et de validation de la configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("lecture de {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("TOML invalide: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("valeur de configuration invalide: {0}")]
    Invalid(String),
}
