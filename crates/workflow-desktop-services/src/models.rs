//! Modèles JSON du serveur `workflow-server` (contrat HTTP).
//!
//! `StepKey` est volontairement dupliqué ici pour que ce crate ne dépende pas
//! de `workflow-server` (axum, état, runner). Il doit rester aligné sur
//! `crates/workflow-server/src/catalog.rs`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StepKey {
    Step1,
    Step2,
    Step3,
    Step4,
    Step5,
    Step6,
    Step7,
    Step8,
}

impl StepKey {
    pub const ALL: [StepKey; 8] = [
        StepKey::Step1,
        StepKey::Step2,
        StepKey::Step3,
        StepKey::Step4,
        StepKey::Step5,
        StepKey::Step6,
        StepKey::Step7,
        StepKey::Step8,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_uppercase().as_str() {
            "STEP1" => Some(Self::Step1),
            "STEP2" => Some(Self::Step2),
            "STEP3" => Some(Self::Step3),
            "STEP4" => Some(Self::Step4),
            "STEP5" => Some(Self::Step5),
            "STEP6" => Some(Self::Step6),
            "STEP7" => Some(Self::Step7),
            "STEP8" => Some(Self::Step8),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Step1 => "STEP1",
            Self::Step2 => "STEP2",
            Self::Step3 => "STEP3",
            Self::Step4 => "STEP4",
            Self::Step5 => "STEP5",
            Self::Step6 => "STEP6",
            Self::Step7 => "STEP7",
            Self::Step8 => "STEP8",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Step1 => "1. Extraction et normalisation des archives",
            Self::Step2 => "2. Analyse des transitions",
            Self::Step3 => "3. Analyse audio",
            Self::Step4 => "4. Analyse du tracking",
            Self::Step5 => "5. Réduction JSON",
            Self::Step6 => "6. Pré-traitement AE",
            Self::Step7 => "7. Finalisation",
            Self::Step8 => "8. Media Solution (EffectCraft)",
        }
    }
}

impl std::fmt::Display for StepKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StepStatus {
    pub step: String,
    pub display_name: String,
    pub status: String,
    pub is_stopping: bool,
    pub return_code: Option<i32>,
    pub progress_current: u64,
    pub progress_total: u64,
    pub progress_current_fractional: Option<f64>,
    pub progress_text: String,
    pub duration_str: Option<String>,
    pub is_any_sequence_running: bool,
    #[serde(default)]
    pub log: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SequenceStatus {
    pub is_running: bool,
    pub current_step: Option<String>,
    pub progress: SequenceProgress,
    pub last_outcome: SequenceOutcome,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SequenceProgress {
    pub current: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SequenceOutcome {
    pub status: String,
    #[serde(rename = "type")]
    pub sequence_type: Option<String>,
    pub message: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PingResponse {
    pub status: String,
    pub worker_type: String,
    pub runtime: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActionResponse {
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SpecificLog {
    pub content: String,
    pub error: Option<String>,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub lines_requested: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SystemMonitor {
    pub cpu_percent: f64,
    pub memory: MemoryMetrics,
    pub gpu: Option<GpuMetrics>,
    pub disk: DiskMetrics,
    pub timestamp: String,
    #[serde(default)]
    pub snapshot_age_s: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryMetrics {
    pub percent: f64,
    pub used_gb: f64,
    pub total_gb: f64,
    pub available_gb: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DiskMetrics {
    pub percent: f64,
    pub used_gb: f64,
    pub total_gb: f64,
    pub free_gb: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum GpuMetrics {
    Available {
        name: String,
        utilization_percent: u64,
        memory: GpuMemory,
        temperature_c: i64,
    },
    Unavailable {
        error: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GpuMemory {
    pub percent: f64,
    pub used_gb: f64,
    pub total_gb: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_keys_round_trip() {
        for step in StepKey::ALL {
            assert_eq!(StepKey::parse(step.as_str()), Some(step));
            assert!(!step.display_name().is_empty());
        }
        assert_eq!(StepKey::parse("step3"), Some(StepKey::Step3));
        assert_eq!(StepKey::parse("STEP9"), None);
    }

    #[test]
    fn deserialize_step_status_with_and_without_logs() {
        let with_logs = r#"{
            "step":"STEP2",
            "display_name":"2. Analyse des transitions",
            "status":"running",
            "is_stopping":false,
            "return_code":null,
            "progress_current":10,
            "progress_total":100,
            "progress_current_fractional":0.1,
            "progress_text":"10/100",
            "duration_str":"2s",
            "is_any_sequence_running":true,
            "log":["ligne 1","ligne 2"]
        }"#;
        let status: StepStatus = serde_json::from_str(with_logs).unwrap();
        assert_eq!(status.step, "STEP2");
        assert_eq!(status.log.as_ref().map(Vec::len), Some(2));

        let without_logs = r#"{
            "step":"STEP2",
            "display_name":"2. Analyse des transitions",
            "status":"idle",
            "is_stopping":false,
            "return_code":null,
            "progress_current":0,
            "progress_total":0,
            "progress_current_fractional":null,
            "progress_text":"",
            "duration_str":null,
            "is_any_sequence_running":false
        }"#;
        let status: StepStatus = serde_json::from_str(without_logs).unwrap();
        assert!(status.log.is_none());
    }

    #[test]
    fn deserialize_system_monitor_gpu_variants() {
        let available = r#"{
            "cpu_percent": 12.5,
            "memory": {"percent": 40.0, "used_gb": 24.0, "total_gb": 62.0, "available_gb": 38.0},
            "gpu": {"name":"GTX 1650","utilization_percent":3,"memory":{"percent":20.0,"used_gb":0.8,"total_gb":4.0},"temperature_c":45},
            "disk": {"percent": 50.0, "used_gb": 100.0, "total_gb": 200.0, "free_gb": 100.0},
            "timestamp": "2026-10-25T00:00:00.000Z",
            "snapshot_age_s": 0.0
        }"#;
        let monitor: SystemMonitor = serde_json::from_str(available).unwrap();
        assert!(matches!(monitor.gpu, Some(GpuMetrics::Available { .. })));

        let unavailable = available.replace(
            r#"{"name":"GTX 1650","utilization_percent":3,"memory":{"percent":20.0,"used_gb":0.8,"total_gb":4.0},"temperature_c":45}"#,
            r#"{"error":"GPU data not available"}"#,
        );
        let monitor: SystemMonitor = serde_json::from_str(&unavailable).unwrap();
        assert!(matches!(monitor.gpu, Some(GpuMetrics::Unavailable { .. })));

        let null_gpu = available.replace(
            r#"{"name":"GTX 1650","utilization_percent":3,"memory":{"percent":20.0,"used_gb":0.8,"total_gb":4.0},"temperature_c":45}"#,
            "null",
        );
        let monitor: SystemMonitor = serde_json::from_str(&null_gpu).unwrap();
        assert!(monitor.gpu.is_none());
    }
}
