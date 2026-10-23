//! Fusion bidirectionnelle de `download_history`.
//!
//! Les deux implémentations utilisent la même règle : conserver le plus petit
//! timestamp vu pour une URL. Une fusion est donc idempotente. Par défaut, les
//! URL sont fusionnées telles quelles ; `normalize = true` réécrit les clés
//! avec la normalisation Rust (migration explicite). Les échecs ne sont pas
//! fusionnés : ce sont des cooldowns transitoires.

use crate::SyncDirection;
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use workflow_ingest::url::normalize_url;

#[derive(Debug, Clone)]
pub struct HistoryMergeOptions {
    pub python_db: PathBuf,
    pub rust_db: PathBuf,
    pub direction: SyncDirection,
    pub normalize: bool,
    pub apply: bool,
    pub backup_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryMergeReport {
    pub python_db: PathBuf,
    pub rust_db: PathBuf,
    pub direction: String,
    pub normalize: bool,
    pub python_before: u64,
    pub rust_before: u64,
    pub python_after: u64,
    pub rust_after: u64,
    pub target: u64,
    pub python_added: u64,
    pub python_updated: u64,
    pub python_removed: u64,
    pub rust_added: u64,
    pub rust_updated: u64,
    pub rust_removed: u64,
    pub python_written: bool,
    pub rust_written: bool,
    pub non_normalized_python: u64,
    pub non_normalized_rust: u64,
    pub non_normalized_examples: Vec<String>,
    pub failures_python: u64,
    pub failures_rust: u64,
    pub backups: Vec<PathBuf>,
    pub applied: bool,
}

pub fn merge_history(options: &HistoryMergeOptions) -> Result<HistoryMergeReport> {
    if !options.python_db.is_file() {
        bail!(
            "base Python introuvable: {} (le sync ne crée jamais une base vide)",
            options.python_db.display()
        );
    }
    if !options.rust_db.is_file() {
        bail!("base Rust introuvable: {}", options.rust_db.display());
    }
    let python_raw = read_raw(&options.python_db)?;
    let rust_raw = read_raw(&options.rust_db)?;
    let python_before = python_raw.len() as u64;
    let rust_before = rust_raw.len() as u64;

    let mut non_normalized_examples = Vec::new();
    let python_non_normalized =
        count_non_normalized(&python_raw, "python", &mut non_normalized_examples);
    let rust_non_normalized = count_non_normalized(&rust_raw, "rust", &mut non_normalized_examples);
    if !options.normalize && (python_non_normalized > 0 || rust_non_normalized > 0) {
        bail!(
            "{} URL non normalisée(s) détectée(s) (python: {}, rust: {}). \
             Relancer avec --normalize pour réécrire les clés, ou corriger les bases.",
            python_non_normalized + rust_non_normalized,
            python_non_normalized,
            rust_non_normalized
        );
    }

    let python_map = build_map(&python_raw, options.normalize);
    let rust_map = build_map(&rust_raw, options.normalize);
    let target = match options.direction {
        SyncDirection::Both => union(&python_map, &rust_map),
        SyncDirection::PythonToRust => union(&rust_map, &python_map),
        SyncDirection::RustToPython => union(&python_map, &rust_map),
    };

    let write_python = options.direction.includes_python();
    let write_rust = options.direction.includes_rust();
    let python_added = if write_python {
        count_missing(&target, &python_map)
    } else {
        0
    };
    let rust_added = if write_rust {
        count_missing(&target, &rust_map)
    } else {
        0
    };
    let python_updated = if write_python {
        count_updated(&target, &python_map)
    } else {
        0
    };
    let rust_updated = if write_rust {
        count_updated(&target, &rust_map)
    } else {
        0
    };
    let python_removed = if write_python {
        count_removed(&python_raw, &target, options.normalize)
    } else {
        0
    };
    let rust_removed = if write_rust {
        count_removed(&rust_raw, &target, options.normalize)
    } else {
        0
    };

    let mut report = HistoryMergeReport {
        python_db: options.python_db.clone(),
        rust_db: options.rust_db.clone(),
        direction: options.direction.as_str().into(),
        normalize: options.normalize,
        python_before,
        rust_before,
        python_after: if write_python {
            target.len() as u64
        } else {
            python_before
        },
        rust_after: if write_rust {
            target.len() as u64
        } else {
            rust_before
        },
        target: target.len() as u64,
        python_added,
        python_updated,
        python_removed,
        rust_added,
        rust_updated,
        rust_removed,
        python_written: options.apply && write_python,
        rust_written: options.apply && write_rust,
        non_normalized_python: python_non_normalized,
        non_normalized_rust: rust_non_normalized,
        non_normalized_examples,
        failures_python: count_failures(&options.python_db)?,
        failures_rust: count_failures(&options.rust_db)?,
        backups: Vec::new(),
        applied: options.apply,
    };

    if options.apply && (write_python || write_rust) {
        std::fs::create_dir_all(&options.backup_dir).with_context(|| {
            format!(
                "création du dossier de sauvegarde {}",
                options.backup_dir.display()
            )
        })?;
        if write_python {
            let backup = options.backup_dir.join("python-download_history.sqlite3");
            backup_sqlite(&options.python_db, &backup)?;
            report.backups.push(backup);
            let count = replace_history(&options.python_db, &target)?;
            if count != report.target {
                bail!(
                    "après écriture, la base Python contient {} URL au lieu de {}",
                    count,
                    report.target
                );
            }
        }
        if write_rust {
            let backup = options.backup_dir.join("rust-download_history.sqlite3");
            backup_sqlite(&options.rust_db, &backup)?;
            report.backups.push(backup);
            let count = replace_history(&options.rust_db, &target)?;
            if count != report.target {
                bail!(
                    "après écriture, la base Rust contient {} URL au lieu de {}",
                    count,
                    report.target
                );
            }
        }
    }

    Ok(report)
}

fn read_raw(path: &Path) -> Result<Vec<(String, String)>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("ouverture en lecture seule de {}", path.display()))?;
    connection
        .busy_timeout(Duration::from_secs(30))
        .context("busy_timeout SQLite")?;
    let table_exists: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'download_history'",
        [],
        |row| row.get(0),
    )?;
    if table_exists == 0 {
        bail!(
            "la base {} ne contient pas la table download_history",
            path.display()
        );
    }
    let mut statement = connection.prepare("SELECT url, timestamp FROM download_history")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

fn count_failures(path: &Path) -> Result<u64> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("ouverture en lecture seule de {}", path.display()))?;
    let table_exists: i64 = connection.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'download_failures'",
        [],
        |row| row.get(0),
    )?;
    if table_exists == 0 {
        return Ok(0);
    }
    let count: i64 = connection.query_row("SELECT COUNT(*) FROM download_failures", [], |row| {
        row.get(0)
    })?;
    Ok(count.max(0) as u64)
}

fn count_non_normalized(
    entries: &[(String, String)],
    side: &str,
    examples: &mut Vec<String>,
) -> u64 {
    let mut count = 0;
    for (url, _) in entries {
        let normalized = normalize_url(url);
        if normalized != *url {
            count += 1;
            if examples.len() < 10 {
                examples.push(format!("{side}: {url} -> {normalized}"));
            }
        }
    }
    count
}

fn build_map(entries: &[(String, String)], normalize: bool) -> BTreeMap<String, String> {
    let mut map: BTreeMap<String, String> = BTreeMap::new();
    for (url, timestamp) in entries {
        let key: String = if normalize {
            normalize_url(url)
        } else {
            url.trim().to_string()
        };
        if key.is_empty() {
            continue;
        }
        match map.get(&key) {
            Some(existing) => {
                let merged = min_timestamp(existing, timestamp);
                map.insert(key, merged);
            }
            None => {
                map.insert(key, timestamp.clone());
            }
        }
    }
    map
}

fn union(
    base: &BTreeMap<String, String>,
    extra: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = base.clone();
    for (url, timestamp) in extra {
        match merged.get(url) {
            Some(existing) => {
                let value = min_timestamp(existing, timestamp);
                merged.insert(url.clone(), value);
            }
            None => {
                merged.insert(url.clone(), timestamp.clone());
            }
        }
    }
    merged
}

fn count_missing(target: &BTreeMap<String, String>, existing: &BTreeMap<String, String>) -> u64 {
    target
        .keys()
        .filter(|url| !existing.contains_key(*url))
        .count() as u64
}

fn count_updated(target: &BTreeMap<String, String>, existing: &BTreeMap<String, String>) -> u64 {
    target
        .iter()
        .filter(|(url, timestamp)| existing.get(*url).is_some_and(|old| old != *timestamp))
        .count() as u64
}

fn count_removed(
    entries: &[(String, String)],
    target: &BTreeMap<String, String>,
    normalize: bool,
) -> u64 {
    entries
        .iter()
        .filter(|(url, _)| {
            let key = if normalize {
                normalize_url(url)
            } else {
                url.trim().to_string()
            };
            key.is_empty() || !target.contains_key(&key)
        })
        .count() as u64
}

fn min_timestamp(left: &str, right: &str) -> String {
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() {
        return right.to_string();
    }
    if right.is_empty() {
        return left.to_string();
    }
    if left <= right {
        left.to_string()
    } else {
        right.to_string()
    }
}

fn backup_sqlite(source: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        std::fs::remove_file(destination).with_context(|| {
            format!(
                "suppression de l'ancienne sauvegarde {}",
                destination.display()
            )
        })?;
    }
    let connection = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("ouverture en lecture seule de {}", source.display()))?;
    let destination_text = destination.to_string_lossy().to_string();
    connection
        .execute("VACUUM INTO ?1", params![destination_text])
        .with_context(|| {
            format!(
                "sauvegarde SQLite de {} vers {}",
                source.display(),
                destination.display()
            )
        })?;
    Ok(())
}

fn replace_history(path: &Path, entries: &BTreeMap<String, String>) -> Result<u64> {
    let connection = Connection::open(path)
        .with_context(|| format!("ouverture en écriture de {}", path.display()))?;
    connection
        .busy_timeout(Duration::from_secs(30))
        .context("busy_timeout SQLite")?;
    let transaction = connection.unchecked_transaction()?;
    transaction.execute("DELETE FROM download_history", [])?;
    {
        let mut statement =
            transaction.prepare("INSERT INTO download_history(url, timestamp) VALUES (?1, ?2)")?;
        for (url, timestamp) in entries {
            statement.execute(params![url, timestamp])?;
        }
    }
    let count: i64 = transaction.query_row("SELECT COUNT(*) FROM download_history", [], |row| {
        row.get(0)
    })?;
    transaction.commit()?;
    Ok(count.max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn create_db(path: &Path, entries: &[(&str, &str)]) {
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
        for (url, timestamp) in entries {
            connection
                .execute(
                    "INSERT INTO download_history(url, timestamp) VALUES (?1, ?2)",
                    params![url, timestamp],
                )
                .unwrap();
        }
    }

    fn options(dir: &Path, direction: SyncDirection) -> HistoryMergeOptions {
        HistoryMergeOptions {
            python_db: dir.join("python.sqlite3"),
            rust_db: dir.join("rust.sqlite3"),
            direction,
            normalize: false,
            apply: false,
            backup_dir: dir.join("backups"),
        }
    }

    #[test]
    fn merges_both_ways_and_keeps_the_earliest_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        create_db(
            &dir.path().join("python.sqlite3"),
            &[
                ("https://a.com/file.zip", "2025-01-02 00:00:00"),
                ("https://b.com/file.zip", "2025-01-03 00:00:00"),
            ],
        );
        create_db(
            &dir.path().join("rust.sqlite3"),
            &[
                ("https://a.com/file.zip", "2025-01-01 00:00:00"),
                ("https://c.com/file.zip", "2025-01-04 00:00:00"),
            ],
        );
        let mut options = options(dir.path(), SyncDirection::Both);
        options.apply = true;
        let report = merge_history(&options).unwrap();
        assert_eq!(report.target, 3);
        assert_eq!(report.python_added, 1);
        assert_eq!(report.rust_added, 1);
        assert_eq!(report.python_updated, 1);
        assert_eq!(report.rust_updated, 0);
        assert_eq!(report.python_removed, 0);
        assert_eq!(report.rust_removed, 0);
        assert_eq!(report.backups.len(), 2);

        let python = read_raw(&options.python_db).unwrap();
        let rust = read_raw(&options.rust_db).unwrap();
        let python_map = build_map(&python, false);
        let rust_map = build_map(&rust, false);
        assert_eq!(python_map, rust_map);
        assert_eq!(python_map["https://a.com/file.zip"], "2025-01-01 00:00:00");
    }

    #[test]
    fn dry_run_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        create_db(
            &dir.path().join("python.sqlite3"),
            &[("https://a.com/file.zip", "2025-01-02 00:00:00")],
        );
        create_db(
            &dir.path().join("rust.sqlite3"),
            &[("https://b.com/file.zip", "2025-01-03 00:00:00")],
        );
        let report = merge_history(&options(dir.path(), SyncDirection::Both)).unwrap();
        assert!(!report.applied);
        assert_eq!(report.target, 2);
        assert_eq!(
            read_raw(&dir.path().join("python.sqlite3")).unwrap().len(),
            1
        );
        assert_eq!(read_raw(&dir.path().join("rust.sqlite3")).unwrap().len(), 1);
        assert!(!dir.path().join("backups").exists());
    }

    #[test]
    fn one_way_only_writes_the_target() {
        let dir = tempfile::tempdir().unwrap();
        create_db(
            &dir.path().join("python.sqlite3"),
            &[("https://a.com/file.zip", "2025-01-02 00:00:00")],
        );
        create_db(
            &dir.path().join("rust.sqlite3"),
            &[("https://b.com/file.zip", "2025-01-03 00:00:00")],
        );
        let mut options = options(dir.path(), SyncDirection::PythonToRust);
        options.apply = true;
        let report = merge_history(&options).unwrap();
        assert_eq!(report.python_added, 0);
        assert_eq!(report.rust_added, 1);
        assert!(!report.python_written);
        assert!(report.rust_written);
        assert_eq!(read_raw(&options.python_db).unwrap().len(), 1);
        assert_eq!(read_raw(&options.rust_db).unwrap().len(), 2);
    }

    #[test]
    fn empty_urls_are_dropped_and_counted_as_removals() {
        let dir = tempfile::tempdir().unwrap();
        create_db(
            &dir.path().join("python.sqlite3"),
            &[("", "2025-01-02 00:00:00")],
        );
        create_db(&dir.path().join("rust.sqlite3"), &[]);
        let mut options = options(dir.path(), SyncDirection::Both);
        options.apply = true;
        let report = merge_history(&options).unwrap();
        assert_eq!(report.target, 0);
        assert_eq!(report.python_removed, 1);
        assert_eq!(report.rust_removed, 0);
        assert_eq!(read_raw(&options.python_db).unwrap().len(), 0);
    }

    #[test]
    fn normalizes_urls_only_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        create_db(
            &dir.path().join("python.sqlite3"),
            &[(
                "https://www.dropbox.com/scl/fo/a?dl=0",
                "2025-01-02 00:00:00",
            )],
        );
        create_db(&dir.path().join("rust.sqlite3"), &[]);
        let mut options = options(dir.path(), SyncDirection::PythonToRust);
        let error = merge_history(&options).unwrap_err().to_string();
        assert!(error.contains("non normalisée"));
        options.normalize = true;
        options.apply = true;
        let report = merge_history(&options).unwrap();
        assert_eq!(report.target, 1);
        let rust = build_map(&read_raw(&options.rust_db).unwrap(), false);
        assert!(rust.keys().all(|url| url == &normalize_url(url)));
    }

    #[test]
    fn failures_are_counted_but_not_merged() {
        let dir = tempfile::tempdir().unwrap();
        create_db(
            &dir.path().join("python.sqlite3"),
            &[("https://a.com/file.zip", "2025-01-02 00:00:00")],
        );
        create_db(&dir.path().join("rust.sqlite3"), &[]);
        {
            let connection = Connection::open(dir.path().join("python.sqlite3")).unwrap();
            connection
                .execute(
                    "INSERT INTO download_failures(url, kind, attempts) VALUES ('https://x.com/a.zip', 'timeout', 2)",
                    [],
                )
                .unwrap();
        }
        let mut options = options(dir.path(), SyncDirection::Both);
        options.apply = true;
        let report = merge_history(&options).unwrap();
        assert_eq!(report.failures_python, 1);
        assert_eq!(report.failures_rust, 0);
        let rust = Connection::open(&options.rust_db).unwrap();
        let count: i64 = rust
            .query_row("SELECT COUNT(*) FROM download_failures", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}
