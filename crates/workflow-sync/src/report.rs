//! Orchestration de la synchronisation et rapport JSON.

use crate::archives::{
    sync_downloads, sync_step7_archives, DownloadsMode, DownloadsSyncReport, Step7Mode,
    Step7SyncReport,
};
use crate::history::{merge_history, HistoryMergeOptions, HistoryMergeReport};
use crate::lock::{ensure_no_conflicting_processes, ensure_servers_stopped, ProcessInfo, SyncLock};
use crate::processed::{merge_processed_archives, ProcessedMergeOptions, ProcessedMergeReport};
use crate::{MergeMode, SyncDirection};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use workflow_core::{ResolvedLegacyPaths, ResolvedPaths};

#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub python: ResolvedLegacyPaths,
    pub rust: ResolvedPaths,
    pub direction: SyncDirection,
    pub history: MergeMode,
    pub processed: MergeMode,
    pub downloads: DownloadsMode,
    pub step7: Step7Mode,
    pub normalize_history: bool,
    pub apply: bool,
    pub backup_dir: PathBuf,
    pub lock_path: PathBuf,
    pub server_ports: Vec<u16>,
    pub allow_running: bool,
    pub now: DateTime<Local>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    pub applied: bool,
    pub started_at: String,
    pub finished_at: String,
    pub backup_dir: PathBuf,
    pub direction: String,
    pub history: Option<HistoryMergeReport>,
    pub processed: Option<ProcessedMergeReport>,
    pub downloads: Option<DownloadsSyncReport>,
    pub step7_archives: Option<Step7SyncReport>,
    pub open_ports: Vec<u16>,
    pub processes: Vec<ProcessInfo>,
    pub warnings: Vec<String>,
    pub applied_manifest: Option<PathBuf>,
}

pub fn run_sync(options: &SyncOptions) -> Result<SyncReport> {
    let _lock = SyncLock::acquire(&options.lock_path)?;
    let open_ports = ensure_servers_stopped(&options.server_ports, options.allow_running)?;
    let processes = ensure_no_conflicting_processes(options.allow_running)?;

    if same_path(&options.python.history_db, &options.rust.history_db) {
        bail!(
            "les bases Python et Rust sont identiques ({}); la synchronisation n'a pas de sens",
            options.python.history_db.display()
        );
    }
    if options.apply {
        fs::create_dir_all(&options.backup_dir).with_context(|| {
            format!(
                "création du dossier de sauvegarde {}",
                options.backup_dir.display()
            )
        })?;
    }

    let mut warnings = Vec::new();
    if !open_ports.is_empty() {
        warnings.push(format!(
            "serveur(s) encore actif(s) sur le(s) port(s) {} : synchronisation forcée",
            open_ports
                .iter()
                .map(u16::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !processes.is_empty() {
        warnings.push(format!(
            "{} processus actif(s) pendant la synchronisation forcée",
            processes.len()
        ));
    }

    let history = if options.history == MergeMode::Merge {
        Some(merge_history(&HistoryMergeOptions {
            python_db: options.python.history_db.clone(),
            rust_db: options.rust.history_db.clone(),
            direction: options.direction,
            normalize: options.normalize_history,
            apply: options.apply,
            backup_dir: options.backup_dir.clone(),
        })?)
    } else {
        None
    };

    let logs_shared = same_path(&options.python.logs, &options.rust.logs);
    if logs_shared {
        warnings.push(format!(
            "les journaux Python et Rust pointent vers {} : fusion STEP1 ignorée",
            options.python.logs.display()
        ));
    }
    let processed = if options.processed == MergeMode::Merge && !logs_shared {
        Some(merge_processed_archives(&ProcessedMergeOptions {
            python_logs: options.python.logs.join("step1"),
            rust_logs: options.rust.logs.join("step1"),
            python_downloads: options.python.downloads.clone(),
            rust_downloads: options.rust.downloads.clone(),
            direction: options.direction,
            apply: options.apply,
            backup_dir: options.backup_dir.clone(),
            now: options.now,
        })?)
    } else {
        None
    };

    let downloads_shared = same_path(&options.python.downloads, &options.rust.downloads);
    if downloads_shared {
        warnings.push(format!(
            "les dossiers de téléchargement sont identiques ({}) : copie ignorée",
            options.python.downloads.display()
        ));
    }
    let downloads = if options.downloads != DownloadsMode::Skip && !downloads_shared {
        Some(sync_downloads(&crate::archives::DownloadsSyncOptions {
            python_dir: options.python.downloads.clone(),
            rust_dir: options.rust.downloads.clone(),
            direction: options.direction,
            mode: options.downloads,
            keyword: options.python.keyword.clone(),
            apply: options.apply,
        })?)
    } else {
        None
    };

    let archives_shared = same_path(&options.python.archives, &options.rust.archives);
    if archives_shared {
        warnings.push(format!(
            "les archives STEP7 sont identiques ({}) : synchronisation ignorée",
            options.python.archives.display()
        ));
    }
    let step7_archives = if options.step7 != Step7Mode::Skip && !archives_shared {
        Some(sync_step7_archives(&crate::archives::Step7SyncOptions {
            python_dir: options.python.archives.clone(),
            rust_dir: options.rust.archives.clone(),
            direction: options.direction,
            mode: options.step7,
            apply: options.apply,
        })?)
    } else {
        None
    };

    if options.normalize_history {
        warnings.push(
            "normalisation Rust demandée (--normalize) : les côtés écrits utilisent les clés normalisées"
                .into(),
        );
    }
    warn_if_projects_non_empty("python", &options.python.projects, &mut warnings);
    warn_if_projects_non_empty("rust", &options.rust.projects, &mut warnings);

    let mut report = SyncReport {
        applied: options.apply,
        started_at: options.now.to_rfc3339(),
        finished_at: Local::now().to_rfc3339(),
        backup_dir: options.backup_dir.clone(),
        direction: options.direction.as_str().into(),
        history,
        processed,
        downloads,
        step7_archives,
        open_ports,
        processes,
        warnings,
        applied_manifest: None,
    };

    if options.apply {
        fs::create_dir_all(&options.backup_dir).with_context(|| {
            format!(
                "création du dossier de sauvegarde {}",
                options.backup_dir.display()
            )
        })?;
        let manifest = options.backup_dir.join("sync-report.json");
        report.applied_manifest = Some(manifest.clone());
        fs::write(&manifest, serde_json::to_vec_pretty(&report)?)
            .with_context(|| format!("écriture du rapport {}", manifest.display()))?;
    }

    Ok(report)
}

fn same_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

fn warn_if_projects_non_empty(label: &str, path: &Path, warnings: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("_temp_") {
            continue;
        }
        names.push(name);
    }
    if names.is_empty() {
        return;
    }
    names.sort();
    warnings.push(format!(
        "{label}: {} projet(s) en cours dans {} (non synchronisés) : {}",
        names.len(),
        path.display(),
        names.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rusqlite::Connection;

    fn create_db(path: &Path, urls: &[&str]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let connection = Connection::open(path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE download_history (url TEXT PRIMARY KEY, timestamp TEXT NOT NULL DEFAULT '');
                 CREATE TABLE download_failures (
                     url TEXT PRIMARY KEY,
                     kind TEXT NOT NULL DEFAULT '',
                     status_code INTEGER,
                     attempts INTEGER NOT NULL DEFAULT 0,
                     last_error TEXT NOT NULL DEFAULT '',
                     first_seen_at TEXT NOT NULL DEFAULT '',
                     last_attempt_at TEXT NOT NULL DEFAULT ''
                 );",
            )
            .unwrap();
        for url in urls {
            connection
                .execute(
                    "INSERT INTO download_history(url, timestamp) VALUES (?1, '2026-01-01 00:00:00')",
                    rusqlite::params![url],
                )
                .unwrap();
        }
    }

    fn options(root: &Path) -> SyncOptions {
        let python_root = root.join("python");
        let rust_base = root.join("rust");
        SyncOptions {
            python: ResolvedLegacyPaths {
                root: python_root.clone(),
                downloads: python_root.join("downloads"),
                history_db: python_root.join("download_history.sqlite3"),
                logs: python_root.join("logs"),
                archives: python_root.join("archives"),
                projects: python_root.join("projets_extraits"),
                keyword: "Camille".into(),
            },
            rust: ResolvedPaths {
                base: rust_base.clone(),
                models: rust_base.join("models"),
                projects: rust_base.join("projets_extraits"),
                archives: rust_base.join("archives"),
                logs: rust_base.join("logs"),
                downloads: rust_base.join("downloads"),
                cache_root: root.join("cache"),
                history_db: rust_base.join("download_history.sqlite3"),
                finalized: root.join("cache"),
                finalized_fallback: rust_base.join("_finalized"),
            },
            direction: SyncDirection::Both,
            history: MergeMode::Merge,
            processed: MergeMode::Merge,
            downloads: DownloadsMode::Skip,
            step7: Step7Mode::Skip,
            normalize_history: false,
            apply: true,
            backup_dir: root.join("backups"),
            lock_path: root.join("sync.lock"),
            server_ports: Vec::new(),
            allow_running: true,
            now: Local.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap(),
        }
    }

    #[test]
    fn runs_a_minimal_sync_and_writes_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        create_db(
            &dir.path().join("python/download_history.sqlite3"),
            &["https://a.com/a.zip"],
        );
        create_db(&dir.path().join("rust/download_history.sqlite3"), &[]);
        fs::create_dir_all(dir.path().join("python/logs/step1")).unwrap();
        fs::write(
            dir.path()
                .join("python/logs/step1/processed_archives.last_reset"),
            "2026-10",
        )
        .unwrap();
        fs::write(
            dir.path().join("python/logs/step1/processed_archives.txt"),
            "/python/downloads/101 Camille.zip\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("rust/logs/step1")).unwrap();
        fs::write(
            dir.path()
                .join("rust/logs/step1/processed_archives.last_reset"),
            "2026-10",
        )
        .unwrap();
        fs::write(
            dir.path().join("rust/logs/step1/processed_archives.txt"),
            "",
        )
        .unwrap();

        let report = run_sync(&options(dir.path())).unwrap();
        assert!(report.applied);
        assert!(report.history.is_some());
        assert!(report.processed.is_some());
        assert!(report.applied_manifest.as_ref().unwrap().exists());
        assert!(!report
            .warnings
            .iter()
            .any(|warning| warning.contains("projet")));
    }
}
