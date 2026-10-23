//! Base SQLite isolée du serveur Rust.
//!
//! Le schéma de téléchargement est créé pour que le serveur n'ait jamais à
//! toucher `download_history.sqlite3` du Python. Une table d'audit des
//! séquences est ajoutée sans modifier le contrat du frontend.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

pub struct HistoryDb {
    connection: Mutex<Connection>,
}

impl HistoryDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("création du dossier de base {}", parent.display()))?;
        }
        let connection = Connection::open(path)
            .with_context(|| format!("ouverture de la base {}", path.display()))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
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
             );
             CREATE TABLE IF NOT EXISTS orchestrator_runs (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 sequence_type TEXT NOT NULL,
                 started_at TEXT NOT NULL,
                 finished_at TEXT NOT NULL,
                 success INTEGER NOT NULL,
                 message TEXT NOT NULL
             );",
            )
            .with_context(|| format!("initialisation de la base {}", path.display()))?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn record_sequence(
        &self,
        sequence_type: &str,
        started_at: &str,
        finished_at: &str,
        success: bool,
        message: &str,
    ) -> Result<()> {
        let connection = self.connection.lock().expect("base verrouillée");
        connection.execute(
            "INSERT INTO orchestrator_runs
                (sequence_type, started_at, finished_at, success, message)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                sequence_type,
                started_at,
                finished_at,
                i64::from(success),
                message
            ],
        )?;
        Ok(())
    }

    pub fn sequence_run_count(&self) -> Result<u64> {
        let connection = self.connection.lock().expect("base verrouillée");
        let count: i64 =
            connection.query_row("SELECT COUNT(*) FROM orchestrator_runs", [], |row| {
                row.get(0)
            })?;
        Ok(count.max(0) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_isolated_schema_and_records_runs() {
        let dir = tempfile::tempdir().unwrap();
        let db = HistoryDb::open(&dir.path().join("history.sqlite3")).unwrap();
        assert_eq!(db.sequence_run_count().unwrap(), 0);
        db.record_sequence("Custom", "start", "end", true, "ok")
            .unwrap();
        assert_eq!(db.sequence_run_count().unwrap(), 1);
    }
}
