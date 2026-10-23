//! Historique SQLite des téléchargements et des échecs.
//!
//! Reprend le schéma et les règles de `DownloadHistoryRepository` :
//! `download_history(url, timestamp)` conserve le plus petit timestamp vu,
//! `download_failures` mémorise les tentatives pour appliquer un cooldown
//! exponentiel.

use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime};
use rusqlite::{params, Connection, OpenFlags};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FailureRecord {
    pub url: String,
    pub kind: String,
    pub status_code: Option<i64>,
    pub attempts: u32,
    pub last_error: String,
    pub first_seen_at: String,
    pub last_attempt_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_count: u64,
    pub imported: u64,
    pub destination_count: u64,
    pub replaced: bool,
}

#[derive(Clone)]
pub struct HistoryRepository {
    connection: Arc<Mutex<Connection>>,
    path: Arc<PathBuf>,
}

impl HistoryRepository {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("création du dossier de base {}", parent.display()))?;
        }
        let connection = Connection::open(path)
            .with_context(|| format!("ouverture de la base {}", path.display()))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(30))
            .context("busy_timeout SQLite")?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 PRAGMA foreign_keys=ON;
                 CREATE TABLE IF NOT EXISTS download_history (
                     url TEXT PRIMARY KEY,
                     timestamp TEXT NOT NULL DEFAULT ''
                 );
                 CREATE TABLE IF NOT EXISTS download_failures (
                     url             TEXT PRIMARY KEY,
                     kind            TEXT NOT NULL DEFAULT '',
                     status_code     INTEGER,
                     attempts        INTEGER NOT NULL DEFAULT 0,
                     last_error      TEXT NOT NULL DEFAULT '',
                     first_seen_at   TEXT NOT NULL DEFAULT '',
                     last_attempt_at TEXT NOT NULL DEFAULT ''
                 );",
            )
            .with_context(|| format!("initialisation de la base {}", path.display()))?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path: Arc::new(path.to_path_buf()),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn with_connection<T>(&self, action: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let connection = self.connection.lock().expect("base verrouillée");
        action(&connection)
    }

    pub fn count(&self) -> Result<u64> {
        self.with_connection(|connection| {
            let count: i64 =
                connection.query_row("SELECT COUNT(*) FROM download_history", [], |row| {
                    row.get(0)
                })?;
            Ok(count.max(0) as u64)
        })
    }

    pub fn get_urls(&self) -> Result<HashSet<String>> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare("SELECT url FROM download_history")?;
            let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
            let mut urls = HashSet::new();
            for row in rows {
                urls.insert(row?);
            }
            Ok(urls)
        })
    }

    pub fn get_ts_by_url(&self) -> Result<HashMap<String, String>> {
        self.with_connection(|connection| {
            let mut statement =
                connection.prepare("SELECT url, timestamp FROM download_history")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            let mut timestamps = HashMap::new();
            for row in rows {
                let (url, timestamp) = row?;
                timestamps.insert(url, timestamp);
            }
            Ok(timestamps)
        })
    }

    pub fn upsert(&self, url: &str, timestamp: &str) -> Result<()> {
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO download_history(url, timestamp)
                 VALUES (?1, ?2)
                 ON CONFLICT(url) DO UPDATE SET
                   timestamp =
                     CASE
                       WHEN download_history.timestamp IS NULL OR download_history.timestamp = '' THEN excluded.timestamp
                       WHEN excluded.timestamp IS NULL OR excluded.timestamp = '' THEN download_history.timestamp
                       ELSE MIN(download_history.timestamp, excluded.timestamp)
                     END",
                params![url, timestamp],
            )?;
            Ok(())
        })
    }

    pub fn upsert_many(&self, entries: &[(String, String)]) -> Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            {
                let mut statement = transaction.prepare(
                    "INSERT INTO download_history(url, timestamp)
                     VALUES (?1, ?2)
                     ON CONFLICT(url) DO UPDATE SET
                       timestamp =
                         CASE
                           WHEN download_history.timestamp IS NULL OR download_history.timestamp = '' THEN excluded.timestamp
                           WHEN excluded.timestamp IS NULL OR excluded.timestamp = '' THEN download_history.timestamp
                           ELSE MIN(download_history.timestamp, excluded.timestamp)
                         END",
                )?;
                for (url, timestamp) in entries {
                    if url.is_empty() {
                        continue;
                    }
                    statement.execute(params![url, timestamp])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn replace_all(&self, entries: &[(String, String)]) -> Result<()> {
        self.with_connection(|connection| {
            let transaction = connection.unchecked_transaction()?;
            transaction.execute("DELETE FROM download_history", [])?;
            {
                let mut statement = transaction
                    .prepare("INSERT INTO download_history(url, timestamp) VALUES (?1, ?2)")?;
                for (url, timestamp) in entries {
                    if url.is_empty() {
                        continue;
                    }
                    statement.execute(params![url, timestamp])?;
                }
            }
            transaction.commit()?;
            Ok(())
        })
    }

    pub fn delete_all(&self) -> Result<()> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM download_history", [])?;
            Ok(())
        })
    }

    pub fn record_failure(
        &self,
        url: &str,
        kind: &str,
        status_code: Option<i64>,
        error: &str,
    ) -> Result<()> {
        if url.is_empty() {
            return Ok(());
        }
        let now = Local::now()
            .naive_local()
            .format("%Y-%m-%dT%H:%M:%S")
            .to_string();
        let truncated: String = error.chars().take(500).collect();
        self.with_connection(|connection| {
            connection.execute(
                "INSERT INTO download_failures(url, kind, status_code, attempts, last_error, first_seen_at, last_attempt_at)
                 VALUES (?1, ?2, ?3, 1, ?4, ?5, ?5)
                 ON CONFLICT(url) DO UPDATE SET
                   kind = excluded.kind,
                   status_code = excluded.status_code,
                   attempts = download_failures.attempts + 1,
                   last_error = excluded.last_error,
                   last_attempt_at = excluded.last_attempt_at",
                params![url, kind, status_code, truncated, now],
            )?;
            Ok(())
        })
    }

    pub fn clear_failure(&self, url: &str) -> Result<()> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM download_failures WHERE url = ?1", params![url])?;
            Ok(())
        })
    }

    pub fn get_failure(&self, url: &str) -> Result<Option<FailureRecord>> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT url, kind, status_code, attempts, last_error, first_seen_at, last_attempt_at
                 FROM download_failures WHERE url = ?1",
            )?;
            let mut rows = statement.query(params![url])?;
            Ok(rows.next()?.map(row_to_failure).transpose()?)
        })
    }

    pub fn list_failures(&self, limit: usize) -> Result<Vec<FailureRecord>> {
        let limit = limit.max(1) as i64;
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT url, kind, status_code, attempts, last_error, first_seen_at, last_attempt_at
                 FROM download_failures ORDER BY last_attempt_at DESC LIMIT ?1",
            )?;
            let rows = statement.query_map(params![limit], row_to_failure)?;
            let mut failures = Vec::new();
            for row in rows {
                failures.push(row?);
            }
            Ok(failures)
        })
    }

    pub fn get_failure_map(&self) -> Result<HashMap<String, FailureRecord>> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT url, kind, status_code, attempts, last_error, first_seen_at, last_attempt_at
                 FROM download_failures",
            )?;
            let rows = statement.query_map([], row_to_failure)?;
            let mut failures = HashMap::new();
            for row in rows {
                let failure = row?;
                failures.insert(failure.url.clone(), failure);
            }
            Ok(failures)
        })
    }

    pub fn failure_count(&self) -> Result<u64> {
        self.with_connection(|connection| {
            let count: i64 =
                connection.query_row("SELECT COUNT(*) FROM download_failures", [], |row| {
                    row.get(0)
                })?;
            Ok(count.max(0) as u64)
        })
    }

    /// Compte les lignes d'une base Python sans rien écrire.
    pub fn preview_import(source: &Path) -> Result<u64> {
        let source = std::fs::canonicalize(source)
            .with_context(|| format!("base source introuvable: {}", source.display()))?;
        let connection = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("ouverture en lecture seule de {}", source.display()))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(30))
            .context("busy_timeout de la base source")?;
        let table_exists: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'download_history'",
            [],
            |row| row.get(0),
        )?;
        if table_exists == 0 {
            anyhow::bail!(
                "la base source ne contient pas la table download_history: {}",
                source.display()
            );
        }
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM download_history", [], |row| {
                row.get(0)
            })?;
        Ok(count.max(0) as u64)
    }

    /// Importe une base Python en lecture seule. Idempotent : `upsert_many`
    /// conserve le plus petit timestamp ; `replace` remplace tout l'historique.
    pub fn import_from(&self, source: &Path, replace: bool) -> Result<ImportReport> {
        let source_canonical = std::fs::canonicalize(source)
            .with_context(|| format!("base source introuvable: {}", source.display()))?;
        let destination_canonical =
            std::fs::canonicalize(self.path()).unwrap_or_else(|_| self.path().to_path_buf());
        if source_canonical == destination_canonical {
            anyhow::bail!("la base source et la base Rust sont identiques");
        }
        let connection =
            Connection::open_with_flags(&source_canonical, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .with_context(|| {
                    format!(
                        "ouverture en lecture seule de {}",
                        source_canonical.display()
                    )
                })?;
        connection
            .busy_timeout(std::time::Duration::from_secs(30))
            .context("busy_timeout de la base source")?;
        let table_exists: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'download_history'",
            [],
            |row| row.get(0),
        )?;
        if table_exists == 0 {
            anyhow::bail!(
                "la base source ne contient pas la table download_history: {}",
                source_canonical.display()
            );
        }
        let mut statement = connection
            .prepare("SELECT url, timestamp FROM download_history ORDER BY timestamp, url")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        let source_count = entries.len() as u64;
        if replace {
            self.replace_all(&entries)?;
        } else {
            self.upsert_many(&entries)?;
        }
        Ok(ImportReport {
            source: source_canonical,
            destination: self.path().to_path_buf(),
            source_count,
            imported: source_count,
            destination_count: self.count()?,
            replaced: replace,
        })
    }
}

fn row_to_failure(row: &rusqlite::Row<'_>) -> rusqlite::Result<FailureRecord> {
    Ok(FailureRecord {
        url: row.get(0)?,
        kind: row.get(1)?,
        status_code: row.get(2)?,
        attempts: row.get::<_, i64>(3)?.max(0) as u32,
        last_error: row.get(4)?,
        first_seen_at: row.get(5)?,
        last_attempt_at: row.get(6)?,
    })
}

/// Secondes restantes avant de réessayer une URL en échec.
pub fn cooldown_remaining_s(
    failure: &FailureRecord,
    now: NaiveDateTime,
    base_interval_s: u64,
    max_cooldown_s: u64,
) -> f64 {
    let attempts = failure.attempts.max(1);
    let base = base_interval_s.max(1) as f64;
    let max_cooldown = max_cooldown_s.max(base_interval_s.max(1)) as f64;
    let exponent = (attempts - 1).min(30) as i32;
    let delay = (base * 2_f64.powi(exponent)).min(max_cooldown);
    let Some(last_attempt) = parse_local_datetime(&failure.last_attempt_at) else {
        return 0.0;
    };
    let elapsed = (now - last_attempt).num_milliseconds() as f64 / 1000.0;
    (delay - elapsed).max(0.0)
}

fn parse_local_datetime(value: &str) -> Option<NaiveDateTime> {
    if let Ok(parsed) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        return Some(parsed);
    }
    if let Ok(parsed) = NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(parsed);
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Local).naive_local());
    }
    if let Ok(parsed) = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return Some(parsed);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository() -> (tempfile::TempDir, HistoryRepository) {
        let dir = tempfile::tempdir().unwrap();
        let repo = HistoryRepository::open(&dir.path().join("history.sqlite3")).unwrap();
        (dir, repo)
    }

    #[test]
    fn keeps_the_earliest_timestamp_on_upsert() {
        let (_dir, repo) = repository();
        repo.upsert("https://a.com/file.zip", "2025-01-02 00:00:00")
            .unwrap();
        repo.upsert("https://a.com/file.zip", "2025-01-01 00:00:00")
            .unwrap();
        assert_eq!(repo.count().unwrap(), 1);
        assert_eq!(
            repo.get_ts_by_url().unwrap()["https://a.com/file.zip"],
            "2025-01-01 00:00:00"
        );
    }

    #[test]
    fn replaces_and_deletes_history() {
        let (_dir, repo) = repository();
        repo.upsert("https://old.com/file.zip", "2024-01-01")
            .unwrap();
        repo.replace_all(&[("https://new.com/file.zip".into(), "2025-01-01".into())])
            .unwrap();
        assert_eq!(repo.count().unwrap(), 1);
        assert!(repo
            .get_urls()
            .unwrap()
            .contains("https://new.com/file.zip"));
        repo.delete_all().unwrap();
        assert_eq!(repo.count().unwrap(), 0);
    }

    #[test]
    fn records_and_increments_failures() {
        let (_dir, repo) = repository();
        let url = "https://server.kidpixel.workers.dev/dropbox/a/b/file";
        repo.record_failure(url, "not_found", Some(404), "HTTP 404")
            .unwrap();
        repo.record_failure(url, "not_found", Some(404), "HTTP 404")
            .unwrap();
        let failure = repo.get_failure(url).unwrap().unwrap();
        assert_eq!(failure.attempts, 2);
        assert_eq!(failure.status_code, Some(404));
        assert_eq!(repo.failure_count().unwrap(), 1);
        repo.clear_failure(url).unwrap();
        assert!(repo.get_failure(url).unwrap().is_none());
    }

    #[test]
    fn cooldown_grows_and_expires() {
        let failure = FailureRecord {
            url: "https://a.com/file.zip".into(),
            kind: "not_found".into(),
            status_code: Some(404),
            attempts: 3,
            last_error: String::new(),
            first_seen_at: "2026-01-01T00:00:00".into(),
            last_attempt_at: "2026-01-01T00:00:00".into(),
        };
        let now =
            NaiveDateTime::parse_from_str("2026-01-01T00:00:20", "%Y-%m-%dT%H:%M:%S").unwrap();
        let remaining = cooldown_remaining_s(&failure, now, 15, 3600);
        assert!((remaining - 40.0).abs() < 0.001);
        let later =
            NaiveDateTime::parse_from_str("2026-01-01T01:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();
        assert_eq!(cooldown_remaining_s(&failure, later, 15, 3600), 0.0);
    }

    #[test]
    fn imports_a_python_history_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("python.sqlite3");
        {
            let connection = Connection::open(&source).unwrap();
            connection
                .execute_batch(
                    "CREATE TABLE download_history (url TEXT PRIMARY KEY, timestamp TEXT NOT NULL DEFAULT '');
                     INSERT INTO download_history(url, timestamp) VALUES
                       ('https://a.com/file.zip', '2025-01-01 00:00:00'),
                       ('https://b.com/file.zip', '2025-01-02 00:00:00');",
                )
                .unwrap();
        }
        let destination = HistoryRepository::open(&dir.path().join("rust.sqlite3")).unwrap();
        let report = destination.import_from(&source, false).unwrap();
        assert_eq!(report.source_count, 2);
        assert_eq!(destination.count().unwrap(), 2);
        assert!(destination
            .get_urls()
            .unwrap()
            .contains("https://b.com/file.zip"));
    }
}
