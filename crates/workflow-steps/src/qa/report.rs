//! Publication de `qa.json`.
//!
//! Le rapport est écrit dans un répertoire `--out` explicite, jamais à côté du
//! MP4 ni dans le corpus. L'écriture est atomique : fichier temporaire voisin
//! puis `rename`.

use crate::qa::model::QaReport;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use workflow_core::fsutil::write_atomic;

pub const QA_FILENAME: &str = "qa.json";

pub fn report_path(out_dir: &Path) -> PathBuf {
    out_dir.join(QA_FILENAME)
}

pub fn write_report(out_dir: &Path, report: &QaReport) -> Result<PathBuf> {
    fs::create_dir_all(out_dir)
        .with_context(|| format!("création du répertoire QA {}", out_dir.display()))?;
    let mut bytes = serde_json::to_vec_pretty(report).context("sérialisation de qa.json")?;
    bytes.push(b'\n');
    let path = report_path(out_dir);
    write_atomic(&path, &bytes).with_context(|| format!("publication de {}", path.display()))?;
    Ok(path)
}

pub fn read_report(path: &Path) -> Result<QaReport> {
    let bytes = fs::read(path).with_context(|| format!("lecture de {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing de {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::model::{
        ExecutionMetrics, ModelsProvenance, QaCheck, QaProjectMeta, QaSpecs, QaStatus,
        Stage0Evidence, Stage0Thresholds,
    };
    use tempfile::tempdir;

    fn report() -> QaReport {
        QaReport {
            schema_version: "1.0.0".into(),
            generated_at: "2026-10-30T00:00:00Z".into(),
            video_id: "300_TEST".into(),
            folder_id: Some("300".into()),
            split: Some("dev".into()),
            video: "/tmp/test_9x16.mp4".into(),
            project: None,
            specs: QaSpecs {
                width: 1080,
                height: 1920,
                fps: 25.0,
                video_codec: "h264".into(),
                audio_codec: "aac".into(),
                duration_s: 60.0,
                bit_rate: 10_000_000,
                size_bytes: 75_000_000,
            },
            project_meta: Some(QaProjectMeta {
                comp_id: "2".into(),
                width: 1080,
                height: 1920,
                fps: 25.0,
                duration_s: 60.0,
                n_layers: 2,
                n_segments: 1,
            }),
            project_structure: None,
            checks: vec![QaCheck::pass("resolution", "1080x1920")],
            findings: Vec::new(),
            stage0_evidence: Stage0Evidence {
                black_intervals: Vec::new(),
                freeze_intervals: Vec::new(),
                thresholds: Stage0Thresholds::default(),
            },
            stage1_face_evidence: None,
            overall_status: QaStatus::Pass,
            auto_approve_eligible: false,
            auto_approve_reason: "v1".into(),
            models_provenance: ModelsProvenance::stage0_only(),
            execution_metrics: ExecutionMetrics::default(),
        }
    }

    #[test]
    fn writes_readable_report_atomically() {
        let dir = tempdir().unwrap();
        let path = write_report(dir.path(), &report()).unwrap();
        assert_eq!(path, dir.path().join("qa.json"));
        let parsed = read_report(&path).unwrap();
        assert_eq!(parsed.video_id, "300_TEST");
        assert_eq!(parsed.overall_status, QaStatus::Pass);
        let leftovers = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(".qa.json."))
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn creates_nested_output_directory() {
        // Le répertoire est créé récursivement, donc un parent absent est valide.
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        let path = write_report(&nested, &report()).unwrap();
        assert!(path.is_file());
    }
}
