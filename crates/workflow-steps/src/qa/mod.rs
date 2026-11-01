//! Contrôle qualité en lecture seule de l'export final (scénario B).
//!
//! `qa run` intervient **après** l'export humain, pas dans STEP8. Le module ne
//! modifie ni le `.ecproj`, ni le MP4, ni `docs/` ; il écrit uniquement
//! `qa.json` sous `--out`.
//!
//! État v1 : Stage 0 déterministe (specs, durée, timeline, template A6,
//! noir/gel), Stage 1 visage (YuNet ONNX CPU) et agrégation. L'OCR sous-titres
//! et les logos restent `not_applicable` ; `auto_approve_eligible` reste faux.

pub mod aggregate;
pub mod face;
pub mod model;
pub mod report;
pub mod stage0;

pub use face::{FaceOptions, SegmentSpan, FACE_MODEL_FILENAME};
pub use model::{
    DefectType, QaCheck, QaFinding, QaReport, QaStatus, Severity, Stage0Thresholds,
    Stage1FaceEvidence,
};
pub use report::{read_report, report_path, write_report};
pub use stage0::{run_stage0, Stage0Options, Stage0Result};

use crate::post::model::now_iso;
use crate::qa::model::{ExecutionMetrics, ModelsProvenance, QA_SCHEMA_VERSION};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct QaOptions {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub template_prefix: String,
    /// Répertoire de sortie explicite : jamais le dossier du MP4 par défaut.
    pub out_dir: PathBuf,
    /// Répertoire des modèles QA (YuNet, OCR).
    pub models_dir: Option<PathBuf>,
    /// Bibliothèque ONNX Runtime explicite.
    pub onnxruntime: Option<PathBuf>,
    /// Active le contrôle visage YuNet.
    pub face_enabled: bool,
    /// 1 ou 3 échantillons par segment.
    pub per_segment: usize,
    pub video_id: Option<String>,
    pub folder_id: Option<String>,
    pub split: Option<String>,
}

impl Default for QaOptions {
    fn default() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            template_prefix: crate::post::a6::DEFAULT_PREFIX.into(),
            out_dir: PathBuf::new(),
            models_dir: None,
            onnxruntime: None,
            face_enabled: true,
            per_segment: 1,
            video_id: None,
            folder_id: None,
            split: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QaOutcome {
    pub report: QaReport,
    pub qa_path: PathBuf,
}

/// Vérifications de Stage 1 non encore portées en Rust.
pub fn stage1_pending_checks() -> Vec<QaCheck> {
    vec![
        QaCheck::not_applicable(
            "subtitle_ocr",
            "OCR non porté en Rust ; PoC Python validé (0 faux positif sur 35 GOOD)",
        ),
        QaCheck::not_applicable(
            "logo_residual",
            "Détection de logos différée après construction du masque projet",
        ),
    ]
}

pub fn run_qa(video: &Path, project: Option<&Path>, options: &QaOptions) -> Result<QaOutcome> {
    if !video.is_file() {
        bail!("vidéo introuvable: {}", video.display());
    }
    if let Some(project) = project {
        if !project.is_file() {
            bail!("projet introuvable: {}", project.display());
        }
    }
    if options.out_dir.as_os_str().is_empty() {
        bail!("passer un répertoire --out explicite");
    }

    let started = Instant::now();
    let stage0 = run_stage0(
        video,
        project,
        &Stage0Options {
            ffmpeg: options.ffmpeg.clone(),
            ffprobe: options.ffprobe.clone(),
            template_prefix: options.template_prefix.clone(),
            thresholds: Stage0Thresholds::default(),
        },
    )
    .with_context(|| format!("Stage 0 sur {}", video.display()))?;

    let mut checks = stage0.checks;
    let mut findings = stage0.findings;
    let mut face_evidence = None;
    let mut face_model = None;
    let mut face_ms = 0;

    if !options.face_enabled {
        checks.push(QaCheck::not_applicable(
            "face_cut",
            "contrôle visage désactivé (--no-face)",
        ));
    } else if let Some(models_dir) = &options.models_dir {
        let model = models_dir.join(FACE_MODEL_FILENAME);
        if !model.is_file() {
            checks.push(QaCheck::not_applicable(
                "face_cut",
                format!("modèle YuNet introuvable: {}", model.display()),
            ));
        } else {
            let spans = stage0
                .segments
                .iter()
                .map(|segment| SegmentSpan {
                    name: segment.name.clone(),
                    start: segment.start,
                    end: segment.end,
                })
                .collect::<Vec<_>>();
            let result = face::run_face(
                video,
                &stage0.specs,
                &spans,
                &FaceOptions {
                    model,
                    onnxruntime: options.onnxruntime.clone(),
                    ffmpeg: options.ffmpeg.clone(),
                    per_segment: options.per_segment,
                },
            )
            .with_context(|| format!("Stage 1 visage sur {}", video.display()))?;
            checks.extend(result.checks);
            findings.extend(result.findings);
            face_evidence = Some(result.evidence);
            face_model = Some(result.model);
            face_ms = result.timing_ms;
        }
    } else {
        checks.push(QaCheck::not_applicable(
            "face_cut",
            "modèle YuNet non fourni (--models)",
        ));
    }
    checks.extend(stage1_pending_checks());

    let overall_status = aggregate::overall_status(&checks, &findings);
    let mut timings = stage0.timings;
    timings.stage1_face_ms = face_ms;
    timings.total_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let mut provenance = ModelsProvenance::stage0_only();
    provenance.face = face_model;

    let video_id = options
        .video_id
        .clone()
        .unwrap_or_else(|| video_id_from_path(video));
    let report = QaReport {
        schema_version: QA_SCHEMA_VERSION.into(),
        generated_at: now_iso(),
        video_id,
        folder_id: options.folder_id.clone(),
        split: options.split.clone(),
        video: video.to_string_lossy().into_owned(),
        project: project.map(|path| path.to_string_lossy().into_owned()),
        specs: stage0.specs,
        project_meta: stage0.project_meta,
        project_structure: stage0.project_structure,
        checks,
        findings,
        stage0_evidence: stage0.evidence,
        stage1_face_evidence: face_evidence,
        overall_status,
        auto_approve_eligible: false,
        auto_approve_reason: aggregate::auto_approve_reason().into(),
        models_provenance: provenance,
        execution_metrics: ExecutionMetrics {
            timings_ms: timings,
        },
    };
    let qa_path = report::write_report(&options.out_dir, &report)?;
    Ok(QaOutcome { report, qa_path })
}

fn video_id_from_path(video: &Path) -> String {
    video
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("export")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::model::QaStatus;

    #[test]
    fn pending_stage1_checks_are_not_applicable() {
        let checks = stage1_pending_checks();
        assert_eq!(checks.len(), 2);
        assert!(checks
            .iter()
            .all(|check| check.status == QaStatus::NotApplicable));
    }

    #[test]
    fn video_id_falls_back_to_stem() {
        assert_eq!(
            video_id_from_path(Path::new("/tmp/UJUD - JOSEPH_9x16.mp4")),
            "UJUD - JOSEPH_9x16"
        );
    }
}
