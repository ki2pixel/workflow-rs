//! Base SQLite isolée du serveur Rust.
//!
//! L'historique et les échecs de téléchargement sont gérés par
//! `workflow-ingest`. Ce module ajoute seulement la table d'audit des
//! séquences, sans modifier le schéma partagé avec l'ingestion.

use anyhow::{Context, Result};
use rusqlite::params;
use std::path::Path;
use workflow_ingest::HistoryRepository;

pub struct HistoryDb {
    repository: HistoryRepository,
}

impl HistoryDb {
    pub fn open(path: &Path) -> Result<Self> {
        let repository = HistoryRepository::open(path)
            .with_context(|| format!("base isolée {}", path.display()))?;
        repository.with_connection(|connection| {
            connection.execute_batch(
                "CREATE TABLE IF NOT EXISTS orchestrator_runs (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     sequence_type TEXT NOT NULL,
                     started_at TEXT NOT NULL,
                     finished_at TEXT NOT NULL,
                     success INTEGER NOT NULL,
                     message TEXT NOT NULL
                 );",
            )?;
            Ok(())
        })?;
        Ok(Self { repository })
    }

    pub fn repository(&self) -> HistoryRepository {
        self.repository.clone()
    }

    pub fn record_sequence(
        &self,
        sequence_type: &str,
        started_at: &str,
        finished_at: &str,
        success: bool,
        message: &str,
    ) -> Result<()> {
        self.repository.with_connection(|connection| {
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
        })
    }

    pub fn sequence_run_count(&self) -> Result<u64> {
        self.repository.with_connection(|connection| {
            let count: i64 =
                connection.query_row("SELECT COUNT(*) FROM orchestrator_runs", [], |row| {
                    row.get(0)
                })?;
            Ok(count.max(0) as u64)
        })
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
        assert_eq!(db.repository().count().unwrap(), 0);
    }
}
