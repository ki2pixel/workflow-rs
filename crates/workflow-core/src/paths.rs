use crate::config::{LegacyConfig, PathsConfig};
use std::path::PathBuf;

/// Chemins absolus après application des valeurs par défaut relatives à `base`.
#[derive(Debug, Clone)]
pub struct ResolvedPaths {
    pub base: PathBuf,
    pub models: PathBuf,
    pub projects: PathBuf,
    pub archives: PathBuf,
    pub logs: PathBuf,
    pub downloads: PathBuf,
    pub cache_root: PathBuf,
    pub history_db: PathBuf,
    pub finalized: PathBuf,
    pub finalized_fallback: PathBuf,
}

/// Chemins Python legacy résolus, pour la synchronisation explicite.
#[derive(Debug, Clone)]
pub struct ResolvedLegacyPaths {
    pub root: PathBuf,
    pub downloads: PathBuf,
    pub history_db: PathBuf,
    pub logs: PathBuf,
    pub archives: PathBuf,
    pub projects: PathBuf,
    pub keyword: String,
}

impl LegacyConfig {
    pub fn resolve(&self, fallback_keyword: &str) -> ResolvedLegacyPaths {
        let root = self.root.clone();
        ResolvedLegacyPaths {
            downloads: self.downloads.clone(),
            history_db: self
                .history_db
                .clone()
                .unwrap_or_else(|| root.join("download_history.sqlite3")),
            logs: self.logs.clone().unwrap_or_else(|| root.join("logs")),
            archives: self
                .archives
                .clone()
                .unwrap_or_else(|| root.join("archives")),
            projects: self
                .projects
                .clone()
                .unwrap_or_else(|| root.join("projets_extraits")),
            keyword: if self.keyword.trim().is_empty() {
                fallback_keyword.to_string()
            } else {
                self.keyword.clone()
            },
            root,
        }
    }
}

impl PathsConfig {
    pub fn resolve(&self) -> ResolvedPaths {
        let base = self.base.clone();
        ResolvedPaths {
            models: self.models.clone(),
            projects: self
                .projects
                .clone()
                .unwrap_or_else(|| base.join("projets_extraits")),
            archives: self
                .archives
                .clone()
                .unwrap_or_else(|| base.join("archives")),
            logs: self.logs.clone().unwrap_or_else(|| base.join("logs")),
            downloads: self
                .downloads
                .clone()
                .unwrap_or_else(|| base.join("downloads")),
            cache_root: self.cache_root.clone(),
            history_db: self
                .history_db
                .clone()
                .unwrap_or_else(|| base.join("download_history.sqlite3")),
            finalized: self.finalized.clone(),
            finalized_fallback: self
                .finalized_fallback
                .clone()
                .unwrap_or_else(|| base.join("_finalized_output")),
            base,
        }
    }
}
