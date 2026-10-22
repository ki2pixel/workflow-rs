//! Socle partagé de l'application Rust : configuration, chemins et verrous.

pub mod config;
pub mod error;
pub mod lock;
pub mod paths;

pub use config::Config;
pub use error::ConfigError;
pub use lock::HardwareLock;
pub use paths::ResolvedPaths;
