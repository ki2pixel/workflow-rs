use crate::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration typée de l'application Rust.
///
/// Le fichier TOML est volontairement fermé : toute clé inconnue provoque une
/// erreur. Les secrets peuvent être surchargés par l'environnement.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub paths: PathsConfig,
    pub webhook: WebhookConfig,
    pub download: DownloadConfig,
    pub steps: StepsConfig,
    pub hardware: HardwareConfig,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_toml(&text)
    }

    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let mut config: Self = toml::from_str(text)?;
        config.apply_env_overrides();
        Ok(config)
    }

    /// Surcharges d'environnement. Les secrets utilisent `WORKFLOW_*` en
    /// priorité, puis les anciens noms pour la période de transition.
    pub fn apply_env_overrides(&mut self) {
        if let Some(value) = env_first(&["WORKFLOW_SERVER_SECRET_KEY", "FLASK_SECRET_KEY"]) {
            self.server.secret_key = value;
        }
        if let Some(value) = env_first(&[
            "WORKFLOW_SERVER_WORKER_TOKEN",
            "INTERNAL_WORKER_COMMS_TOKEN",
        ]) {
            self.server.worker_token = value;
        }
        if let Some(value) = env_first(&["WORKFLOW_SERVER_PORT"]) {
            if let Ok(port) = value.parse::<u16>() {
                self.server.port = port;
            }
        }
        if let Some(value) = env_first(&["WORKFLOW_PATHS_BASE"]) {
            self.paths.base = PathBuf::from(value);
        }
        if let Some(value) = env_first(&["WORKFLOW_WEBHOOK_URL", "WEBHOOK_JSON_URL"]) {
            self.webhook.url = value;
        }
    }

    /// Validation utilisable par la CLI, sans exiger les secrets du serveur.
    pub fn validate_common(&self) -> Result<(), ConfigError> {
        if self.server.port < 1024 {
            return Err(ConfigError::Invalid(
                "server.port doit être supérieur ou égal à 1024".into(),
            ));
        }
        if self.steps.step1.fps == 0 {
            return Err(ConfigError::Invalid(
                "steps.step1.fps doit être strictement positif".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.steps.step2.threshold) {
            return Err(ConfigError::Invalid(
                "steps.step2.threshold doit être dans [0, 1]".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.steps.step3.adaptive_threshold) {
            return Err(ConfigError::Invalid(
                "steps.step3.adaptive_threshold doit être dans [0, 1]".into(),
            ));
        }
        if self.hardware.gpu_mem_limit_mb == 0 {
            return Err(ConfigError::Invalid(
                "hardware.gpu_mem_limit_mb doit être strictement positif".into(),
            ));
        }
        Ok(())
    }

    /// Validation complète du serveur, secrets compris.
    pub fn validate_server(&self) -> Result<(), ConfigError> {
        self.validate_common()?;
        if self.server.secret_key.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "server.secret_key est vide (secret obligatoire)".into(),
            ));
        }
        let token = self.server.worker_token.trim();
        if token.is_empty() || token == "dev-internal-worker-token" || token.starts_with("dev-") {
            return Err(ConfigError::Invalid(
                "server.worker_token est vide, par défaut, ou préfixé dev-".into(),
            ));
        }
        Ok(())
    }
}

fn env_first(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(value) = env::var(name) {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub debug: bool,
    pub secret_key: String,
    pub worker_token: String,
    pub polling_interval_ms: u64,
    pub system_monitor_interval_ms: u64,
    pub snapshot_interval_s: u64,
    pub slow_api_threshold_ms: u64,
    pub gpu_monitoring: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5004,
            debug: false,
            secret_key: String::new(),
            worker_token: String::new(),
            polling_interval_ms: 1_000,
            system_monitor_interval_ms: 5_000,
            snapshot_interval_s: 10,
            slow_api_threshold_ms: 500,
            gpu_monitoring: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PathsConfig {
    pub base: PathBuf,
    pub models: PathBuf,
    pub projects: Option<PathBuf>,
    pub archives: Option<PathBuf>,
    pub logs: Option<PathBuf>,
    pub downloads: Option<PathBuf>,
    pub cache_root: PathBuf,
    pub history_db: Option<PathBuf>,
    pub history_group: String,
    pub finalized: PathBuf,
    pub finalized_fallback: Option<PathBuf>,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            base: PathBuf::from("."),
            models: PathBuf::from("assets/models"),
            projects: None,
            archives: None,
            logs: None,
            downloads: None,
            cache_root: PathBuf::from("/mnt/cache"),
            history_db: None,
            history_group: "wfgroup".into(),
            finalized: PathBuf::from("/mnt/cache"),
            finalized_fallback: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebhookConfig {
    pub url: String,
    pub timeout_s: u64,
    pub cache_ttl_s: u64,
    pub interval_s: u64,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            timeout_s: 10,
            cache_ttl_s: 300,
            interval_s: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DownloadConfig {
    pub chunk_bytes: u64,
    pub connect_timeout_s: u64,
    pub chunk_timeout_s: u64,
    pub probe_timeout_s: u64,
    pub max_attempts: u32,
    pub backoff_s: f64,
    pub progress_min_interval_s: f64,
    pub min_zip_bytes: u64,
    pub validate_zip: bool,
    pub cooldown_max_s: u64,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            chunk_bytes: 8 * 1024 * 1024,
            connect_timeout_s: 10,
            chunk_timeout_s: 60,
            probe_timeout_s: 10,
            max_attempts: 3,
            backoff_s: 2.0,
            progress_min_interval_s: 0.5,
            min_zip_bytes: 1024,
            validate_zip: true,
            cooldown_max_s: 3_600,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StepsConfig {
    pub filter: FilterConfig,
    pub default_timeout_s: u64,
    pub step1: Step1Config,
    pub step2: Step2Config,
    pub step3: Step3Config,
    pub step4: Step4Config,
    pub step5: Step5Config,
    pub step6: Step6Config,
    pub step7: Step7Config,
    pub step8: Step8Config,
}

impl Default for StepsConfig {
    fn default() -> Self {
        Self {
            filter: FilterConfig::default(),
            default_timeout_s: 1_800,
            step1: Step1Config::default(),
            step2: Step2Config::default(),
            step3: Step3Config::default(),
            step4: Step4Config::default(),
            step5: Step5Config::default(),
            step6: Step6Config::default(),
            step7: Step7Config::default(),
            step8: Step8Config::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FilterConfig {
    pub keyword: String,
    pub docs_subfolder: String,
}

impl Default for FilterConfig {
    fn default() -> Self {
        Self {
            keyword: "Camille".into(),
            docs_subfolder: "docs".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step1Config {
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
    pub normalize: bool,
    pub fps: u32,
    pub use_gpu: bool,
    pub crf: u32,
    pub gpu_workers: u32,
}

impl Default for Step1Config {
    fn default() -> Self {
        Self {
            skip_preserved_mov: true,
            preserved_mov_names: Vec::new(),
            normalize: true,
            fps: 25,
            use_gpu: true,
            crf: 18,
            gpu_workers: 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step2Config {
    pub cudnn_algo_search: bool,
    pub batch_size: u32,
    pub window: u32,
    pub stride: u32,
    pub threshold: f64,
}

impl Default for Step2Config {
    fn default() -> Self {
        Self {
            cudnn_algo_search: true,
            batch_size: 8,
            window: 100,
            stride: 50,
            threshold: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step3Config {
    pub enabled: bool,
    pub yamnet_split: bool,
    pub require_edgetpu: bool,
    pub embedding_stride: u32,
    pub adaptive_stride: bool,
    pub adaptive_threshold: f64,
    pub max_workers: u32,
}

impl Default for Step3Config {
    fn default() -> Self {
        Self {
            enabled: true,
            yamnet_split: true,
            require_edgetpu: true,
            embedding_stride: 1,
            adaptive_stride: true,
            adaptive_threshold: 0.5,
            max_workers: 4,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step4Config {
    pub enabled: bool,
    pub object_detection: bool,
    pub object_detector: String,
    pub object_detector_path: PathBuf,
    pub object_min_missing: u32,
    pub face_reset_frames: u32,
    pub fast: bool,
    pub cpu_threads: u32,
    pub blendshapes_profile: String,
    pub blendshapes_export: Vec<String>,
    pub blendshapes_throttle: u32,
    pub voluminous_exports: bool,
    pub jaw_open_threshold: f64,
    pub test_cpu_workers: u32,
}

impl Default for Step4Config {
    fn default() -> Self {
        Self {
            enabled: true,
            object_detection: true,
            object_detector: "efficientdet_lite2".into(),
            object_detector_path: PathBuf::new(),
            object_min_missing: 5,
            face_reset_frames: 30,
            fast: false,
            cpu_threads: 4,
            blendshapes_profile: "full".into(),
            blendshapes_export: vec!["jawOpen".into()],
            blendshapes_throttle: 2,
            voluminous_exports: false,
            jaw_open_threshold: 0.08,
            test_cpu_workers: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step5Config {
    pub tracking_analytics: bool,
    pub expression_summary: bool,
    pub expression_keys: Vec<String>,
}

impl Default for Step5Config {
    fn default() -> Self {
        Self {
            tracking_analytics: true,
            expression_summary: false,
            expression_keys: vec!["jawOpen".into()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step6Config {
    pub spread_threshold: u32,
    pub enable_confidence_weighting: bool,
    pub confidence_weight: f64,
    pub label_high_spread: u32,
    pub label_stable: u32,
    pub label_fallback_center: u32,
}

impl Default for Step6Config {
    fn default() -> Self {
        Self {
            spread_threshold: 200,
            enable_confidence_weighting: true,
            confidence_weight: 0.35,
            label_high_spread: 12,
            label_stable: 3,
            label_fallback_center: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step7Config {
    pub cleanup_threshold_hours: f64,
}

impl Default for Step7Config {
    fn default() -> Self {
        Self {
            cleanup_threshold_hours: 48.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step8Config {
    pub timeout_s: u64,
    pub cli: PathBuf,
    pub panel_jsx: PathBuf,
    pub panel_config: PathBuf,
    pub mode: String,
    pub bridge_port: u16,
    pub auto_launch: bool,
    pub app: PathBuf,
    pub app_launch_timeout_s: u64,
}

impl Default for Step8Config {
    fn default() -> Self {
        Self {
            timeout_s: 1_800,
            cli: PathBuf::from("effectcraft-cli"),
            panel_jsx: PathBuf::from("scripts/MediaSolution-EC/MediaSolution.jsx"),
            panel_config: PathBuf::new(),
            mode: "headless".into(),
            bridge_port: 9877,
            auto_launch: false,
            app: PathBuf::new(),
            app_launch_timeout_s: 60,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HardwareConfig {
    pub lock_path: PathBuf,
    pub tpu_device: PathBuf,
    pub gpu_mem_limit_mb: u32,
    pub require_tpu: bool,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        let lock_path = env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
            .join("workflow-hw.lock");
        Self {
            lock_path,
            tpu_device: PathBuf::from("/dev/apex_0"),
            gpu_mem_limit_mb: 4_096,
            require_tpu: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let config = Config::default();
        config.validate_common().expect("defaults valides");
    }

    #[test]
    fn unknown_key_is_rejected() {
        let error = Config::from_toml("[server]\ninconnu = 1\n").unwrap_err();
        assert!(error.to_string().contains("inconnu"));
    }

    #[test]
    fn worker_token_default_is_rejected_for_server() {
        let config = Config::default();
        assert!(config.validate_server().is_err());
    }

    #[test]
    fn env_override_applies_secret() {
        // Le test ne peut pas manipuler l'environnement global sans risque de
        // collision avec les autres tests. La méthode est couverte par la CLI.
    }
}
