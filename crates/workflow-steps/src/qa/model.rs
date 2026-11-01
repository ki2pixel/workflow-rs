//! Modèles sérialisables du contrôle qualité scénario B.
//!
//! Le contrat `qa.json` est volontairement séparé de l'`audit.json` de A5/A6 :
//! il décrit un contrôle en lecture seule de l'export final, pas une décision
//! de post-traitement. Les champs sont ordonnés comme dans le fichier publié.

use serde::{Deserialize, Serialize};

pub const QA_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QaStatus {
    /// Contrôle conforme.
    Pass,
    /// Observation conservée pour la revue, sans dégradation du statut global.
    Info,
    /// Défaut candidat : revue humaine requise.
    Review,
    /// Non-conformité objective : le statut global est `fail`.
    Fail,
    /// Contrôle non exécuté (modèle absent, projet absent, hors périmètre).
    NotApplicable,
}

impl QaStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Info => "info",
            Self::Review => "review",
            Self::Fail => "fail",
            Self::NotApplicable => "not_applicable",
        }
    }

    pub fn degrades_overall(self) -> bool {
        matches!(self, Self::Review | Self::Fail)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectType {
    SpecMismatch,
    BlackFrame,
    Freeze,
    TemplateLogoMissing,
    CutContinuity,
    SubtitleBurnedIn,
    LogoResidual,
    FaceCut,
    FramingReview,
    ReactionShotReview,
}

impl DefectType {
    /// Défauts objectifs qui font passer le rapport en `fail`.
    pub fn is_blocking(self) -> bool {
        matches!(self, Self::SpecMismatch | Self::TemplateLogoMissing)
    }

    /// Nom stable utilisé dans les chemins de preuve et les résumés.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SpecMismatch => "spec_mismatch",
            Self::BlackFrame => "black_frame",
            Self::Freeze => "freeze",
            Self::TemplateLogoMissing => "template_logo_missing",
            Self::CutContinuity => "cut_continuity",
            Self::SubtitleBurnedIn => "subtitle_burned_in",
            Self::LogoResidual => "logo_residual",
            Self::FaceCut => "face_cut",
            Self::FramingReview => "framing_review",
            Self::ReactionShotReview => "reaction_shot_review",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaCheck {
    pub id: String,
    pub status: QaStatus,
    pub detail: String,
}

impl QaCheck {
    pub fn new(id: impl Into<String>, status: QaStatus, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            status,
            detail: detail.into(),
        }
    }

    pub fn pass(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(id, QaStatus::Pass, detail)
    }

    pub fn info(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(id, QaStatus::Info, detail)
    }

    pub fn warn(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(id, QaStatus::Review, detail)
    }

    pub fn fail(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(id, QaStatus::Fail, detail)
    }

    pub fn not_applicable(id: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(id, QaStatus::NotApplicable, detail)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaFinding {
    pub defect_type: DefectType,
    pub timecode_start_sec: f64,
    pub timecode_end_sec: Option<f64>,
    pub severity: Severity,
    pub confidence: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bbox_xywh: Option<[f64; 4]>,
    pub detected_by_stage: String,
    pub notes: String,
    /// Frame de preuve relative au répertoire `--out`, si elle a été écrite.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_frame: Option<String>,
}

impl QaFinding {
    pub fn new(
        defect_type: DefectType,
        timecode_start_sec: f64,
        timecode_end_sec: Option<f64>,
        severity: Severity,
        confidence: f64,
        detected_by_stage: impl Into<String>,
        notes: impl Into<String>,
    ) -> Self {
        Self {
            defect_type,
            timecode_start_sec,
            timecode_end_sec,
            severity,
            confidence,
            bbox_xywh: None,
            detected_by_stage: detected_by_stage.into(),
            notes: notes.into(),
            evidence_frame: None,
        }
    }

    pub fn with_bbox(mut self, bbox_xywh: [f64; 4]) -> Self {
        self.bbox_xywh = Some(bbox_xywh);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaSpecs {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub video_codec: String,
    pub audio_codec: String,
    pub duration_s: f64,
    pub bit_rate: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaProjectMeta {
    pub comp_id: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_s: f64,
    pub n_layers: usize,
    pub n_segments: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStructure {
    pub first_segment_start: Option<f64>,
    pub last_segment_end: Option<f64>,
    pub shortest_segment_s: Option<f64>,
    pub has_sdm_logo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Interval {
    pub start: f64,
    pub end: Option<f64>,
    pub duration: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage0Thresholds {
    pub black_finding_s: f64,
    pub freeze_finding_s: f64,
    pub coverage_warn_s: f64,
    pub short_segment_info_s: f64,
    pub short_segment_finding_s: f64,
}

impl Default for Stage0Thresholds {
    fn default() -> Self {
        Self {
            black_finding_s: 3.0,
            freeze_finding_s: 5.0,
            coverage_warn_s: 5.0,
            short_segment_info_s: 0.5,
            short_segment_finding_s: 0.08,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage0Evidence {
    pub black_intervals: Vec<Interval>,
    pub freeze_intervals: Vec<Interval>,
    pub thresholds: Stage0Thresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage1FaceEvidence {
    pub model: String,
    pub samples_total: usize,
    pub samples_with_face: usize,
    pub samples_with_cut: usize,
    pub frame_errors: usize,
    pub per_segment: usize,
    pub score_threshold: f64,
    pub nms_threshold: f64,
    pub pad_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage1OcrEvidence {
    pub det_model: String,
    pub rec_model: String,
    pub samples_total: usize,
    pub samples_with_text: usize,
    pub detections_total: usize,
    pub findings_total: usize,
    pub frame_errors: usize,
    pub per_segment: usize,
    pub min_score: f64,
    pub bottom_start: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub sha256: String,
    pub license: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelsProvenance {
    pub stage0: String,
    pub ocr_det: Option<ModelInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ocr_rec: Option<ModelInfo>,
    pub face: Option<ModelInfo>,
    pub embedding: Option<ModelInfo>,
}

impl ModelsProvenance {
    pub fn stage0_only() -> Self {
        Self {
            stage0: "deterministic".into(),
            ocr_det: None,
            ocr_rec: None,
            face: None,
            embedding: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timings {
    pub ffprobe_ms: u64,
    pub black_freeze_ms: u64,
    pub project_ms: u64,
    #[serde(default)]
    pub stage1_ocr_ms: u64,
    #[serde(default)]
    pub stage1_face_ms: u64,
    #[serde(default)]
    pub evidence_ms: u64,
    pub total_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub timings_ms: Timings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaReport {
    pub schema_version: String,
    pub generated_at: String,
    pub video_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    pub video: String,
    /// SHA-256 de l'export contrôlé, pour le cache et la traçabilité.
    #[serde(default)]
    pub export_sha256: String,
    pub project: Option<String>,
    pub specs: QaSpecs,
    pub project_meta: Option<QaProjectMeta>,
    pub project_structure: Option<ProjectStructure>,
    pub checks: Vec<QaCheck>,
    pub findings: Vec<QaFinding>,
    pub stage0_evidence: Stage0Evidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage1_ocr_evidence: Option<Stage1OcrEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage1_face_evidence: Option<Stage1FaceEvidence>,
    pub overall_status: QaStatus,
    pub auto_approve_eligible: bool,
    pub auto_approve_reason: String,
    pub models_provenance: ModelsProvenance,
    pub execution_metrics: ExecutionMetrics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_serialize_in_snake_case() {
        assert_eq!(
            serde_json::to_string(&QaStatus::NotApplicable).unwrap(),
            "\"not_applicable\""
        );
        assert_eq!(
            serde_json::to_string(&DefectType::SubtitleBurnedIn).unwrap(),
            "\"subtitle_burned_in\""
        );
        assert_eq!(serde_json::to_string(&Severity::High).unwrap(), "\"high\"");
    }

    #[test]
    fn only_review_and_fail_degrade_overall() {
        assert!(!QaStatus::Pass.degrades_overall());
        assert!(!QaStatus::Info.degrades_overall());
        assert!(!QaStatus::NotApplicable.degrades_overall());
        assert!(QaStatus::Review.degrades_overall());
        assert!(QaStatus::Fail.degrades_overall());
    }

    #[test]
    fn finding_omits_absent_bbox() {
        let finding = QaFinding::new(
            DefectType::BlackFrame,
            4.28,
            Some(5.8),
            Severity::Medium,
            1.0,
            "stage0",
            "noir de 1,52 s",
        );
        let value = serde_json::to_value(&finding).unwrap();
        assert!(value.get("bbox_xywh").is_none());
        assert_eq!(value["defect_type"], "black_frame");
        assert_eq!(value["timecode_start_sec"], 4.28);
    }

    #[test]
    fn report_keys_keep_declared_order() {
        let report = QaReport {
            schema_version: QA_SCHEMA_VERSION.into(),
            generated_at: "2026-10-30T00:00:00Z".into(),
            video_id: "300_TEST".into(),
            folder_id: None,
            split: None,
            video: "/tmp/test.mp4".into(),
            export_sha256: "0".repeat(64),
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
            project_meta: None,
            project_structure: None,
            checks: vec![QaCheck::pass("resolution", "1080x1920")],
            findings: Vec::new(),
            stage0_evidence: Stage0Evidence {
                black_intervals: Vec::new(),
                freeze_intervals: Vec::new(),
                thresholds: Stage0Thresholds::default(),
            },
            stage1_ocr_evidence: None,
            stage1_face_evidence: None,
            overall_status: QaStatus::Pass,
            auto_approve_eligible: false,
            auto_approve_reason: "v1".into(),
            models_provenance: ModelsProvenance::stage0_only(),
            execution_metrics: ExecutionMetrics::default(),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.starts_with("{\"schema_version\":"));
        assert!(json.contains("\"overall_status\":\"pass\""));
    }
}
