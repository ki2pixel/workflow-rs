//! Isolation des chemins Rust.
//!
//! Tous les répertoires de travail doivent rester sous `paths.base` et le
//! dépôt Python legacy ne doit jamais servir de racine de projets Rust. La
//! fonction est partagée par le serveur et la CLI afin que `config check` et
//! un lancement direct appliquent exactement les mêmes règles.

use crate::config::Config;
use crate::error::ConfigError;
use std::path::{Path, PathBuf};

pub fn validate_isolation(config: &Config) -> Result<(), ConfigError> {
    let resolved = config.paths.resolve();
    let base = canonical_directory(&resolved.base, "paths.base")?;

    let db_parent = resolved
        .history_db
        .parent()
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(db_parent).map_err(|source| ConfigError::Read {
        path: db_parent.to_path_buf(),
        source,
    })?;
    let db_parent = std::fs::canonicalize(db_parent).map_err(|source| ConfigError::Read {
        path: db_parent.to_path_buf(),
        source,
    })?;
    let db = std::fs::canonicalize(&resolved.history_db).unwrap_or_else(|_| {
        db_parent.join(
            resolved
                .history_db
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("history.sqlite3")),
        )
    });
    if !db.starts_with(&base) {
        return Err(ConfigError::Invalid(format!(
            "isolation refusée: paths.history_db ({}) doit être sous paths.base ({})",
            db.display(),
            base.display()
        )));
    }

    for (label, path) in [
        ("paths.projects", &resolved.projects),
        ("paths.archives", &resolved.archives),
        ("paths.logs", &resolved.logs),
        ("paths.downloads", &resolved.downloads),
    ] {
        let canonical = canonical_directory(path, label)?;
        if !canonical.starts_with(&base) {
            return Err(ConfigError::Invalid(format!(
                "isolation refusée: {label} ({}) doit être sous paths.base ({})",
                canonical.display(),
                base.display()
            )));
        }
    }

    let projects =
        std::fs::canonicalize(&resolved.projects).map_err(|source| ConfigError::Read {
            path: resolved.projects.clone(),
            source,
        })?;
    let legacy_root = config.legacy.resolve(&config.steps.filter.keyword).root;
    if !legacy_root.as_os_str().is_empty() {
        let legacy = std::fs::canonicalize(&legacy_root).unwrap_or(legacy_root);
        if projects.starts_with(&legacy) {
            return Err(ConfigError::Invalid(format!(
                "isolation refusée: paths.projects pointe vers le dépôt Python legacy: {}",
                projects.display()
            )));
        }
    }
    Ok(())
}

fn canonical_directory(path: &Path, _label: &str) -> Result<PathBuf, ConfigError> {
    std::fs::create_dir_all(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    std::fs::canonicalize(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_config(root: &Path) -> Config {
        let mut config = Config::default();
        config.paths.base = root.join("runtime");
        config.paths.history_db = Some(config.paths.base.join("history.sqlite3"));
        config.paths.projects = Some(config.paths.base.join("projets_extraits"));
        config.paths.logs = Some(config.paths.base.join("logs"));
        config.hardware.lock_path = root.join("hardware.lock");
        config.legacy.root = root.join("legacy");
        config
    }

    #[test]
    fn given_dedicated_runtime_when_validated_then_ok() {
        let dir = tempfile::tempdir().unwrap();
        validate_isolation(&isolated_config(dir.path())).unwrap();
    }

    #[test]
    fn given_database_outside_runtime_when_validated_then_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = isolated_config(dir.path());
        config.paths.history_db = Some(dir.path().join("elsewhere.sqlite3"));
        assert!(validate_isolation(&config).is_err());
    }

    #[test]
    fn given_project_root_outside_runtime_when_validated_then_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = isolated_config(dir.path());
        config.paths.projects = Some(dir.path().join("projects"));
        assert!(validate_isolation(&config).is_err());
    }

    #[test]
    fn given_projects_under_legacy_root_when_validated_then_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        let mut config = isolated_config(dir.path());
        config.paths.base = legacy.clone();
        config.paths.history_db = Some(legacy.join("history.sqlite3"));
        config.paths.projects = Some(legacy.join("projets_extraits"));
        config.paths.logs = Some(legacy.join("logs"));
        assert!(validate_isolation(&config).is_err());
    }

    #[test]
    fn given_history_file_metadata_when_canonicalized_then_under_base() {
        let dir = tempfile::tempdir().unwrap();
        let config = isolated_config(dir.path());
        validate_isolation(&config).unwrap();
        assert!(config
            .paths
            .history_db
            .as_ref()
            .unwrap()
            .starts_with(std::fs::canonicalize(dir.path().join("runtime")).unwrap()));
    }
}
