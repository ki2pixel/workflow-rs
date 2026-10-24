//! Socle partagé de l'application Rust : configuration, chemins et verrous.

pub mod config;
pub mod error;
pub mod fsutil;
pub mod isolation;
pub mod lock;
pub mod paths;

pub use config::{Config, DownloadConfig, LegacyConfig, WebhookConfig};
pub use error::ConfigError;
pub use isolation::validate_isolation;
pub use lock::HardwareLock;
pub use paths::{ResolvedLegacyPaths, ResolvedPaths};
