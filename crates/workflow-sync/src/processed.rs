//! Fusion de `logs/step1/processed_archives.txt`.
//!
//! Le fichier contient des chemins absolus, propres à chaque racine. La
//! synchronisation identifie une archive par son nom de fichier et réécrit le
//! chemin avec la racine du backend cible. La réinitialisation mensuelle est
//! respectée : une liste dont le marqueur n'est pas le mois courant est
//! sauvegardée puis vidée, comme le fait STEP1, avant la fusion.

use crate::SyncDirection;
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const PROCESSED_FILE: &str = "processed_archives.txt";
const RESET_MARKER: &str = "processed_archives.last_reset";

#[derive(Debug, Clone)]
pub struct ProcessedMergeOptions {
    pub python_logs: PathBuf,
    pub rust_logs: PathBuf,
    pub python_downloads: PathBuf,
    pub rust_downloads: PathBuf,
    pub direction: SyncDirection,
    pub apply: bool,
    pub backup_dir: PathBuf,
    pub now: DateTime<Local>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessedMergeReport {
    pub python_logs: PathBuf,
    pub rust_logs: PathBuf,
    pub current_month: String,
    pub direction: String,
    pub python_before: usize,
    pub rust_before: usize,
    pub python_after: usize,
    pub rust_after: usize,
    pub python_added: usize,
    pub rust_added: usize,
    pub python_remapped: usize,
    pub rust_remapped: usize,
    pub python_reset: bool,
    pub rust_reset: bool,
    pub python_initialized: bool,
    pub rust_initialized: bool,
    pub python_ignored: usize,
    pub rust_ignored: usize,
    pub python_written: bool,
    pub rust_written: bool,
    pub conflicts: Vec<String>,
    pub ignored_examples: Vec<String>,
    pub backups: Vec<PathBuf>,
    pub applied: bool,
}

struct SideState {
    before: usize,
    entries: BTreeMap<String, PathBuf>,
    reset: bool,
    initialized: bool,
    conflicts: Vec<String>,
    ignored: Vec<String>,
    backups: Vec<PathBuf>,
}

pub fn merge_processed_archives(options: &ProcessedMergeOptions) -> Result<ProcessedMergeReport> {
    let current_month = options.now.format("%Y-%m").to_string();
    let write_python = options.direction.includes_python();
    let write_rust = options.direction.includes_rust();
    // Un sens unique ne doit jamais réinitialiser la liste de la source.
    let python = load_side(
        &options.python_logs,
        &options.python_downloads,
        options.now,
        options.apply && write_python,
        &options.backup_dir,
        "python",
    )?;
    let rust = load_side(
        &options.rust_logs,
        &options.rust_downloads,
        options.now,
        options.apply && write_rust,
        &options.backup_dir,
        "rust",
    )?;

    let union: BTreeSet<String> = python
        .entries
        .keys()
        .chain(rust.entries.keys())
        .cloned()
        .collect();
    let python_output = output_entries(&python.entries, &options.python_downloads, &union);
    let rust_output = output_entries(&rust.entries, &options.rust_downloads, &union);
    let python_added = if write_python {
        union.len().saturating_sub(python.entries.len())
    } else {
        0
    };
    let rust_added = if write_rust {
        union.len().saturating_sub(rust.entries.len())
    } else {
        0
    };
    let python_remapped = if write_python {
        count_remapped(&python.entries, &python_output)
    } else {
        0
    };
    let rust_remapped = if write_rust {
        count_remapped(&rust.entries, &rust_output)
    } else {
        0
    };

    let mut report = ProcessedMergeReport {
        python_logs: options.python_logs.clone(),
        rust_logs: options.rust_logs.clone(),
        current_month,
        direction: options.direction.as_str().into(),
        python_before: python.before,
        rust_before: rust.before,
        python_after: if write_python {
            python_output.len()
        } else {
            python.before
        },
        rust_after: if write_rust {
            rust_output.len()
        } else {
            rust.before
        },
        python_added,
        rust_added,
        python_remapped,
        rust_remapped,
        python_reset: python.reset,
        rust_reset: rust.reset,
        python_initialized: python.initialized,
        rust_initialized: rust.initialized,
        python_ignored: python.ignored.len(),
        rust_ignored: rust.ignored.len(),
        python_written: options.apply && write_python,
        rust_written: options.apply && write_rust,
        conflicts: python
            .conflicts
            .iter()
            .map(|conflict| format!("python: {conflict}"))
            .chain(
                rust.conflicts
                    .iter()
                    .map(|conflict| format!("rust: {conflict}")),
            )
            .collect(),
        ignored_examples: python
            .ignored
            .iter()
            .map(|entry| format!("python: {entry}"))
            .chain(rust.ignored.iter().map(|entry| format!("rust: {entry}")))
            .collect(),
        backups: python
            .backups
            .iter()
            .chain(rust.backups.iter())
            .cloned()
            .collect(),
        applied: options.apply,
    };

    if !options.apply {
        return Ok(report);
    }

    if write_python {
        if let Some(backup) = backup_if_needed(
            &options.python_logs,
            &options.backup_dir,
            "python",
            options.now,
            &python.backups,
        )? {
            report.backups.push(backup);
        }
        write_processed(&options.python_logs, &python_output, &report.current_month)?;
    }
    if write_rust {
        if let Some(backup) = backup_if_needed(
            &options.rust_logs,
            &options.backup_dir,
            "rust",
            options.now,
            &rust.backups,
        )? {
            report.backups.push(backup);
        }
        write_processed(&options.rust_logs, &rust_output, &report.current_month)?;
    }

    report.backups.sort();
    report.backups.dedup();
    Ok(report)
}

fn load_side(
    logs_dir: &Path,
    downloads_dir: &Path,
    now: DateTime<Local>,
    apply: bool,
    backup_dir: &Path,
    label: &str,
) -> Result<SideState> {
    let processed_file = logs_dir.join(PROCESSED_FILE);
    let marker_file = logs_dir.join(RESET_MARKER);
    let marker = fs::read_to_string(&marker_file)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let content = fs::read_to_string(&processed_file).unwrap_or_default();
    let (parsed, conflicts) = parse_entries(&content);
    let (parsed, ignored) = filter_entries(parsed, downloads_dir);
    // Un conflit sur une entrée hors du dossier de téléchargement n'a pas
    // d'effet sur STEP1 : ne pas le remonter comme actionnable.
    let conflicts = conflicts
        .into_iter()
        .filter(|conflict| {
            conflict
                .split(':')
                .next()
                .is_some_and(|name| parsed.contains_key(name))
        })
        .collect();
    let current_month = now.format("%Y-%m").to_string();
    let needs_reset = marker.as_deref() != Some(current_month.as_str());
    if !needs_reset {
        return Ok(SideState {
            before: parsed.len(),
            entries: parsed,
            reset: false,
            initialized: false,
            conflicts,
            ignored,
            backups: Vec::new(),
        });
    }

    let had_content = !content.trim().is_empty();
    let reset = marker.is_some() || had_content;
    let initialized = marker.is_none() && !had_content;
    let mut backups = Vec::new();
    if apply {
        fs::create_dir_all(logs_dir)
            .with_context(|| format!("création du dossier {}", logs_dir.display()))?;
        if had_content {
            fs::create_dir_all(backup_dir).with_context(|| {
                format!("création du dossier de sauvegarde {}", backup_dir.display())
            })?;
            let old_month = marker.as_deref().unwrap_or("previous");
            let backup = backup_dir.join(format!(
                "{label}-processed_archives_{old_month}_{}.txt",
                now.format("%Y%m%d_%H%M%S")
            ));
            fs::copy(&processed_file, &backup).with_context(|| {
                format!(
                    "sauvegarde de {} vers {}",
                    processed_file.display(),
                    backup.display()
                )
            })?;
            backups.push(backup);
        }
        if processed_file.exists() {
            fs::write(&processed_file, "")
                .with_context(|| format!("vidage de {}", processed_file.display()))?;
        }
        fs::write(&marker_file, &current_month)
            .with_context(|| format!("écriture du marqueur {}", marker_file.display()))?;
    }

    Ok(SideState {
        before: parsed.len(),
        entries: BTreeMap::new(),
        reset,
        initialized,
        conflicts,
        ignored,
        backups,
    })
}

fn parse_entries(content: &str) -> (BTreeMap<String, PathBuf>, Vec<String>) {
    let mut entries: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut conflicts = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let path = PathBuf::from(line);
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            conflicts.push(format!("entrée sans nom de fichier: {line}"));
            continue;
        };
        match entries.get(&name) {
            Some(existing) if existing != &path => {
                conflicts.push(format!(
                    "{name}: {} vs {}",
                    existing.display(),
                    path.display()
                ));
                let existing_exists = existing.exists();
                let path_exists = path.exists();
                let prefer_new = (path_exists && !existing_exists)
                    || (path_exists == existing_exists && path < *existing);
                if prefer_new {
                    entries.insert(name, path);
                }
            }
            _ => {
                entries.insert(name, path);
            }
        }
    }
    (entries, conflicts)
}

fn filter_entries(
    entries: BTreeMap<String, PathBuf>,
    downloads_dir: &Path,
) -> (BTreeMap<String, PathBuf>, Vec<String>) {
    let mut kept = BTreeMap::new();
    let mut ignored = Vec::new();
    for (name, path) in entries {
        let in_downloads =
            path_is_under(&path, downloads_dir) || downloads_dir.join(&name).exists();
        if in_downloads {
            kept.insert(name, path);
        } else {
            ignored.push(format!("{name}: {}", path.display()));
        }
    }
    (kept, ignored)
}

fn path_is_under(path: &Path, root: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(path), Ok(root)) => path.starts_with(root),
        _ => false,
    }
}

fn output_entries(
    entries: &BTreeMap<String, PathBuf>,
    downloads_dir: &Path,
    union: &BTreeSet<String>,
) -> BTreeMap<String, PathBuf> {
    let mut output = BTreeMap::new();
    for name in union {
        let path = entries
            .get(name)
            .filter(|path| path.exists())
            .cloned()
            .unwrap_or_else(|| map_to_downloads(downloads_dir, name));
        output.insert(name.clone(), path);
    }
    output
}

fn map_to_downloads(downloads_dir: &Path, name: &str) -> PathBuf {
    let candidate = downloads_dir.join(name);
    if let Ok(canonical) = candidate.canonicalize() {
        return canonical;
    }
    downloads_dir
        .canonicalize()
        .map(|root| root.join(name))
        .unwrap_or(candidate)
}

fn count_remapped(
    existing: &BTreeMap<String, PathBuf>,
    output: &BTreeMap<String, PathBuf>,
) -> usize {
    existing
        .iter()
        .filter(|(name, path)| output.get(*name) != Some(*path))
        .count()
}

fn backup_if_needed(
    logs_dir: &Path,
    backup_dir: &Path,
    label: &str,
    now: DateTime<Local>,
    existing_backups: &[PathBuf],
) -> Result<Option<PathBuf>> {
    if !existing_backups.is_empty() {
        return Ok(None);
    }
    let source = logs_dir.join(PROCESSED_FILE);
    if !source.is_file() {
        return Ok(None);
    }
    if source
        .metadata()
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        == 0
    {
        return Ok(None);
    }
    fs::create_dir_all(backup_dir)
        .with_context(|| format!("création du dossier de sauvegarde {}", backup_dir.display()))?;
    let destination = backup_dir.join(format!(
        "{label}-processed_archives_{}.txt",
        now.format("%Y%m%d_%H%M%S")
    ));
    fs::copy(&source, &destination).with_context(|| {
        format!(
            "sauvegarde de {} vers {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(Some(destination))
}

fn write_processed(
    logs_dir: &Path,
    entries: &BTreeMap<String, PathBuf>,
    current_month: &str,
) -> Result<()> {
    fs::create_dir_all(logs_dir)
        .with_context(|| format!("création du dossier {}", logs_dir.display()))?;
    let path = logs_dir.join(PROCESSED_FILE);
    let temp = logs_dir.join(format!(".{PROCESSED_FILE}.tmp-{}", std::process::id()));
    let mut content = String::new();
    for entry in entries.values() {
        content.push_str(&entry.to_string_lossy());
        content.push('\n');
    }
    fs::write(&temp, content)
        .with_context(|| format!("écriture temporaire de {}", temp.display()))?;
    if let Ok(metadata) = fs::metadata(&path) {
        let _ = fs::set_permissions(&temp, metadata.permissions());
    }
    if let Err(error) = fs::rename(&temp, &path) {
        let _ = fs::remove_file(&temp);
        return Err(error).with_context(|| format!("remplacement de {}", path.display()));
    }
    fs::write(logs_dir.join(RESET_MARKER), current_month).with_context(|| {
        format!(
            "écriture du marqueur {}",
            logs_dir.join(RESET_MARKER).display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn write_side(root: &Path, logs: &str, downloads: &str, month: &str, names: &[&str]) {
        let logs_dir = root.join(logs);
        let downloads_dir = root.join(downloads);
        fs::create_dir_all(&logs_dir).unwrap();
        fs::create_dir_all(&downloads_dir).unwrap();
        let content = if names.is_empty() {
            String::new()
        } else {
            names
                .iter()
                .map(|name| downloads_dir.join(name).display().to_string())
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        };
        fs::write(logs_dir.join(PROCESSED_FILE), content).unwrap();
        fs::write(logs_dir.join(RESET_MARKER), month).unwrap();
    }

    fn options(root: &Path, direction: SyncDirection) -> ProcessedMergeOptions {
        ProcessedMergeOptions {
            python_logs: root.join("python/logs/step1"),
            rust_logs: root.join("rust/logs/step1"),
            python_downloads: root.join("python/downloads"),
            rust_downloads: root.join("rust/downloads"),
            direction,
            apply: false,
            backup_dir: root.join("backups"),
            now: Local.with_ymd_and_hms(2026, 10, 23, 12, 0, 0).unwrap(),
        }
    }

    #[test]
    fn merges_by_basename_and_remaps_paths() {
        let dir = tempfile::tempdir().unwrap();
        write_side(
            dir.path(),
            "python/logs/step1",
            "python/downloads",
            "2026-10",
            &["101 Camille.zip"],
        );
        write_side(
            dir.path(),
            "rust/logs/step1",
            "rust/downloads",
            "2026-10",
            &[],
        );
        let mut options = options(dir.path(), SyncDirection::Both);
        options.apply = true;
        let report = merge_processed_archives(&options).unwrap();
        assert_eq!(report.python_after, 1);
        assert_eq!(report.rust_after, 1);
        assert_eq!(report.rust_added, 1);
        let rust = fs::read_to_string(options.rust_logs.join(PROCESSED_FILE)).unwrap();
        assert!(rust.contains("rust/downloads/101 Camille.zip"));
        assert!(!rust.contains("Téléchargements"));
    }

    #[test]
    fn one_way_does_not_reset_the_source_month() {
        let dir = tempfile::tempdir().unwrap();
        write_side(
            dir.path(),
            "python/logs/step1",
            "python/downloads",
            "2026-09",
            &["101 Camille.zip"],
        );
        write_side(
            dir.path(),
            "rust/logs/step1",
            "rust/downloads",
            "2026-10",
            &[],
        );
        let mut options = options(dir.path(), SyncDirection::PythonToRust);
        options.apply = true;
        let report = merge_processed_archives(&options).unwrap();
        assert!(report.python_reset);
        assert!(!report.python_written);
        assert_eq!(report.rust_after, 0);
        let python = fs::read_to_string(options.python_logs.join(PROCESSED_FILE)).unwrap();
        assert!(python.contains("101 Camille.zip"));
        let marker = fs::read_to_string(options.python_logs.join(RESET_MARKER)).unwrap();
        assert_eq!(marker.trim(), "2026-09");
    }

    #[test]
    fn ignores_entries_outside_the_downloads_root() {
        let dir = tempfile::tempdir().unwrap();
        let python_logs = dir.path().join("python/logs/step1");
        let python_downloads = dir.path().join("python/downloads");
        fs::create_dir_all(&python_logs).unwrap();
        fs::create_dir_all(&python_downloads).unwrap();
        fs::write(
            python_logs.join(PROCESSED_FILE),
            format!(
                "/tmp/Testalpha_Camille.zip\n{}\n",
                python_downloads.join("101 Camille.zip").display()
            ),
        )
        .unwrap();
        fs::write(python_logs.join(RESET_MARKER), "2026-10").unwrap();
        write_side(
            dir.path(),
            "rust/logs/step1",
            "rust/downloads",
            "2026-10",
            &[],
        );
        let mut options = options(dir.path(), SyncDirection::Both);
        options.apply = true;
        let report = merge_processed_archives(&options).unwrap();
        assert_eq!(report.python_ignored, 1);
        assert_eq!(report.python_after, 1);
        assert_eq!(report.rust_after, 1);
        let python = fs::read_to_string(options.python_logs.join(PROCESSED_FILE)).unwrap();
        assert!(!python.contains("Testalpha"));
        assert!(python.contains("101 Camille.zip"));
    }

    #[test]
    fn resets_a_stale_month_with_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        write_side(
            dir.path(),
            "python/logs/step1",
            "python/downloads",
            "2026-09",
            &["101 Camille.zip"],
        );
        write_side(
            dir.path(),
            "rust/logs/step1",
            "rust/downloads",
            "2026-10",
            &[],
        );
        let mut options = options(dir.path(), SyncDirection::Both);
        options.apply = true;
        let report = merge_processed_archives(&options).unwrap();
        assert!(report.python_reset);
        assert_eq!(report.python_before, 1);
        assert_eq!(report.python_after, 0);
        assert_eq!(report.rust_after, 0);
        assert_eq!(report.backups.len(), 1);
        assert!(report.backups[0].exists());
        let python = fs::read_to_string(options.python_logs.join(PROCESSED_FILE)).unwrap();
        assert!(python.is_empty());
    }

    #[test]
    fn dry_run_does_not_write_or_reset() {
        let dir = tempfile::tempdir().unwrap();
        write_side(
            dir.path(),
            "python/logs/step1",
            "python/downloads",
            "2026-10",
            &["101 Camille.zip"],
        );
        write_side(
            dir.path(),
            "rust/logs/step1",
            "rust/downloads",
            "2026-10",
            &[],
        );
        let report = merge_processed_archives(&options(dir.path(), SyncDirection::Both)).unwrap();
        assert!(!report.applied);
        assert_eq!(report.rust_after, 1);
        let rust_logs = options(dir.path(), SyncDirection::Both).rust_logs;
        let rust = fs::read_to_string(rust_logs.join(PROCESSED_FILE)).unwrap();
        assert!(rust.is_empty());
        assert!(!dir.path().join("backups").exists());
    }

    #[test]
    fn one_way_does_not_write_the_source() {
        let dir = tempfile::tempdir().unwrap();
        write_side(
            dir.path(),
            "python/logs/step1",
            "python/downloads",
            "2026-10",
            &["101 Camille.zip"],
        );
        write_side(
            dir.path(),
            "rust/logs/step1",
            "rust/downloads",
            "2026-10",
            &["102 Camille.zip"],
        );
        let mut options = options(dir.path(), SyncDirection::PythonToRust);
        options.apply = true;
        let report = merge_processed_archives(&options).unwrap();
        assert_eq!(report.python_added, 0);
        assert_eq!(report.rust_added, 1);
        assert!(!report.python_written);
        assert!(report.rust_written);
        let python = fs::read_to_string(options.python_logs.join(PROCESSED_FILE)).unwrap();
        assert!(python.contains("101 Camille.zip"));
        assert!(!python.contains("102"));
        let rust = fs::read_to_string(options.rust_logs.join(PROCESSED_FILE)).unwrap();
        assert!(rust.contains("101 Camille.zip"));
        assert!(rust.contains("102 Camille.zip"));
    }
}
