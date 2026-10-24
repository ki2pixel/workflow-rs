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
    pub retention: RetentionConfig,
    pub hardware: HardwareConfig,
    /// Racines Python legacy, utilisées uniquement par `workflow-cli sync`.
    /// Le serveur Rust ne les lit jamais.
    pub legacy: LegacyConfig,
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
        config.apply_env_overrides()?;
        Ok(config)
    }

    /// Surcharges d'environnement. Les secrets utilisent `WORKFLOW_*` en
    /// priorité, puis les anciens noms pour la période de transition.
    ///
    /// Une valeur non parsable est une erreur : une faute de frappe dans
    /// `launcher.env` ne doit pas être silencieusement ignorée.
    pub fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        self.apply_env_lookup(|name| env::var(name).ok())
    }

    pub(crate) fn apply_env_lookup(
        &mut self,
        lookup: impl Fn(&str) -> Option<String>,
    ) -> Result<(), ConfigError> {
        if let Some(value) = env_first(&lookup, &["WORKFLOW_SERVER_SECRET_KEY", "FLASK_SECRET_KEY"])
        {
            self.server.secret_key = value;
        }
        if let Some(value) = env_first(
            &lookup,
            &[
                "WORKFLOW_SERVER_WORKER_TOKEN",
                "INTERNAL_WORKER_COMMS_TOKEN",
            ],
        ) {
            self.server.worker_token = value;
        }
        if let Some(value) = env_first(&lookup, &["WORKFLOW_SERVER_PORT"]) {
            self.server.port = parse_env(&value, "WORKFLOW_SERVER_PORT")?;
        }
        if let Some(value) = env_first(&lookup, &["WORKFLOW_SERVER_ALLOW_REMOTE"]) {
            self.server.allow_remote = parse_bool_env(&value, "WORKFLOW_SERVER_ALLOW_REMOTE")?;
        }
        if let Some(value) = env_first(&lookup, &["WORKFLOW_PATHS_BASE"]) {
            self.paths.base = PathBuf::from(value);
        }
        if let Some(value) = env_first(&lookup, &["WORKFLOW_WEBHOOK_URL", "WEBHOOK_JSON_URL"]) {
            self.webhook.url = value;
        }
        if let Some(value) = env_first(&lookup, &["WORKFLOW_WEBHOOK_ENABLED", "WEBHOOK_ENABLED"]) {
            self.webhook.enabled = parse_bool_env(&value, "WORKFLOW_WEBHOOK_ENABLED")?;
        }
        if let Some(value) = env_first(&lookup, &["WEBHOOK_TIMEOUT"]) {
            self.webhook.timeout_s = parse_env(&value, "WEBHOOK_TIMEOUT")?;
        }
        if let Some(value) = env_first(&lookup, &["WEBHOOK_CACHE_TTL"]) {
            self.webhook.cache_ttl_s = parse_env(&value, "WEBHOOK_CACHE_TTL")?;
        }
        if let Some(value) = env_first(&lookup, &["WEBHOOK_MONITOR_INTERVAL"]) {
            self.webhook.interval_s = parse_env(&value, "WEBHOOK_MONITOR_INTERVAL")?;
        }
        if let Some(value) = env_first(&lookup, &["WEBHOOK_REQUIRE_HISTORY"]) {
            self.webhook.require_history = parse_bool_env(&value, "WEBHOOK_REQUIRE_HISTORY")?;
        }
        if let Some(value) = env_first(&lookup, &["DRY_RUN_DOWNLOADS"]) {
            self.webhook.dry_run = parse_bool_env(&value, "DRY_RUN_DOWNLOADS")?;
        }
        if let Some(value) = env_first(&lookup, &["WORKFLOW_PATHS_DOWNLOADS"]) {
            self.paths.downloads = Some(PathBuf::from(value));
        }
        if let Some(value) = env_first(&lookup, &["WORKFLOW_STEP1_SOURCE_DIR"]) {
            self.steps.step1.source_dir = PathBuf::from(value);
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_CONNECT_TIMEOUT_S"]) {
            self.download.connect_timeout_s = parse_env(&value, "DOWNLOAD_CONNECT_TIMEOUT_S")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_CHUNK_TIMEOUT_S"]) {
            self.download.chunk_timeout_s = parse_env(&value, "DOWNLOAD_CHUNK_TIMEOUT_S")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_PROBE_TIMEOUT_S"]) {
            self.download.probe_timeout_s = parse_env(&value, "DOWNLOAD_PROBE_TIMEOUT_S")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_MAX_ATTEMPTS"]) {
            self.download.max_attempts = parse_env(&value, "DOWNLOAD_MAX_ATTEMPTS")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_RETRY_BACKOFF_S"]) {
            self.download.backoff_s = parse_env(&value, "DOWNLOAD_RETRY_BACKOFF_S")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_PROGRESS_MIN_INTERVAL_S"]) {
            self.download.progress_min_interval_s =
                parse_env(&value, "DOWNLOAD_PROGRESS_MIN_INTERVAL_S")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_MIN_ZIP_SIZE_BYTES"]) {
            self.download.min_zip_bytes = parse_env(&value, "DOWNLOAD_MIN_ZIP_SIZE_BYTES")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_VALIDATE_ZIP"]) {
            self.download.validate_zip = parse_bool_env(&value, "DOWNLOAD_VALIDATE_ZIP")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_COOLDOWN_MAX_S"]) {
            self.download.cooldown_max_s = parse_env(&value, "DOWNLOAD_COOLDOWN_MAX_S")?;
        }
        if let Some(value) = env_first(&lookup, &["DOWNLOAD_MAX_CONCURRENT"]) {
            self.download.max_concurrent = parse_env(&value, "DOWNLOAD_MAX_CONCURRENT")?;
        }
        Ok(())
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
        if self.steps.step1.gpu_workers == 0 || self.steps.step1.timeout_s == 0 {
            return Err(ConfigError::Invalid(
                "steps.step1.gpu_workers et steps.step1.timeout_s doivent être > 0".into(),
            ));
        }
        if self.steps.step1.crf > 51 {
            return Err(ConfigError::Invalid(
                "steps.step1.crf doit être dans [0, 51]".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.steps.step2.threshold) {
            return Err(ConfigError::Invalid(
                "steps.step2.threshold doit être dans [0, 1]".into(),
            ));
        }
        if self.steps.step2.batch_size == 0
            || self.steps.step2.window == 0
            || self.steps.step2.stride == 0
            || self.steps.step2.padding * 2 >= self.steps.step2.window
        {
            return Err(ConfigError::Invalid(
                "steps.step2: batch_size, window et stride doivent être > 0 et padding*2 < window"
                    .into(),
            ));
        }
        if !matches!(self.steps.step2.device.as_str(), "cuda" | "cpu") {
            return Err(ConfigError::Invalid(
                "steps.step2.device doit être cuda ou cpu".into(),
            ));
        }
        if !matches!(self.steps.step3.device.as_str(), "tpu" | "coral" | "cpu") {
            return Err(ConfigError::Invalid(
                "steps.step3.device doit être tpu ou cpu".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.steps.step3.vad_threshold) {
            return Err(ConfigError::Invalid(
                "steps.step3.vad_threshold doit être dans [0, 1]".into(),
            ));
        }
        if !(0.0..1.0).contains(&self.steps.step3.hop_overlap) {
            return Err(ConfigError::Invalid(
                "steps.step3.hop_overlap doit être dans [0, 1[".into(),
            ));
        }
        if self.steps.step3.median_filter_size == 0
            || self.steps.step3.embedding_stride == 0
            || self.steps.step3.adaptive_threshold == 0
            || self.steps.step3.max_speakers == 0
            || self.steps.step3.min_cluster_embeddings == 0
            || self.steps.step3.ecapa_batch_size == 0
        {
            return Err(ConfigError::Invalid(
                "steps.step3: les tailles et seuils doivent être > 0".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.steps.step3.clustering_threshold) {
            return Err(ConfigError::Invalid(
                "steps.step3.clustering_threshold doit être dans [0, 1]".into(),
            ));
        }
        if self.steps.step3.hangover_sec < 0.0 || self.steps.step3.min_speaker_speech_duration < 0.0
        {
            return Err(ConfigError::Invalid(
                "steps.step3.hangover_sec et min_speaker_speech_duration doivent être positifs"
                    .into(),
            ));
        }
        if self.steps.step1.max_extracted_bytes == 0 || self.steps.step1.max_compression_ratio == 0
        {
            return Err(ConfigError::Invalid(
                "steps.step1: max_extracted_bytes et max_compression_ratio doivent être > 0".into(),
            ));
        }
        if self.steps.step7.timeout_s == 0 {
            return Err(ConfigError::Invalid(
                "steps.step7.timeout_s doit être strictement positif".into(),
            ));
        }
        if !matches!(
            self.steps.step4.device.to_ascii_lowercase().as_str(),
            "tpu" | "coral" | "cpu" | "xnnpack"
        ) {
            return Err(ConfigError::Invalid(
                "steps.step4.device doit être tpu ou cpu".into(),
            ));
        }
        if self.steps.step4.object_min_missing == 0 || self.steps.step4.face_reset_frames == 0 {
            return Err(ConfigError::Invalid(
                "steps.step4.object_min_missing et face_reset_frames doivent être > 0".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.steps.step4.object_score_threshold) {
            return Err(ConfigError::Invalid(
                "steps.step4.object_score_threshold doit être dans [0, 1]".into(),
            ));
        }
        if self.steps.step4.object_max == 0 {
            return Err(ConfigError::Invalid(
                "steps.step4.object_max doit être strictement positif".into(),
            ));
        }
        let profile = self.steps.step4.blendshapes_profile.to_ascii_lowercase();
        if !matches!(
            profile.as_str(),
            "full"
                | "all"
                | ""
                | "none"
                | "off"
                | "0"
                | "false"
                | "no"
                | "mouth"
                | "mediapipe"
                | "custom"
        ) {
            return Err(ConfigError::Invalid(
                "steps.step4.blendshapes_profile invalide".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.steps.step4.jaw_open_threshold) {
            return Err(ConfigError::Invalid(
                "steps.step4.jaw_open_threshold doit être dans [0, 1]".into(),
            ));
        }
        if (self.steps.step4.decode_width == 0) != (self.steps.step4.decode_height == 0) {
            return Err(ConfigError::Invalid(
                "steps.step4.decode_width et decode_height doivent être tous deux 0 ou > 0".into(),
            ));
        }
        if self.webhook.enabled && self.webhook.url.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "webhook.url est vide alors que webhook.enabled = true".into(),
            ));
        }
        if self.webhook.timeout_s == 0 || self.webhook.interval_s == 0 {
            return Err(ConfigError::Invalid(
                "webhook.timeout_s et webhook.interval_s doivent être > 0".into(),
            ));
        }
        if self.download.connect_timeout_s == 0
            || self.download.chunk_timeout_s == 0
            || self.download.probe_timeout_s == 0
            || self.download.max_attempts == 0
            || self.download.max_concurrent == 0
            || self.download.min_zip_bytes == 0
        {
            return Err(ConfigError::Invalid(
                "download: timeouts, tentatives, concurrence et taille ZIP doivent être > 0".into(),
            ));
        }
        if self.download.backoff_s < 0.0 || self.download.progress_min_interval_s < 0.0 {
            return Err(ConfigError::Invalid(
                "download.backoff_s et download.progress_min_interval_s doivent être positifs"
                    .into(),
            ));
        }
        if self
            .hardware
            .lock_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "workflow-hw.lock")
        {
            return Err(ConfigError::Invalid(
                "hardware.lock_path ne peut pas utiliser workflow-hw.lock (verrou Python legacy)"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Validation complète du serveur, secrets compris.
    pub fn validate_server(&self) -> Result<(), ConfigError> {
        self.validate_common()?;
        if self.server.port == 5003 {
            return Err(ConfigError::Invalid(
                "server.port 5003 est réservé au serveur Python legacy".into(),
            ));
        }
        if self.server.workflow_cli.as_os_str().is_empty() {
            return Err(ConfigError::Invalid(
                "server.workflow_cli ne peut pas être vide".into(),
            ));
        }
        if self.server.workdir.as_os_str().is_empty() {
            return Err(ConfigError::Invalid(
                "server.workdir ne peut pas être vide".into(),
            ));
        }
        if self.steps.step1.source_dir.as_os_str().is_empty() {
            return Err(ConfigError::Invalid(
                "steps.step1.source_dir ne peut pas être vide".into(),
            ));
        }
        if self.server.max_step_log_bytes < 1024 {
            return Err(ConfigError::Invalid(
                "server.max_step_log_bytes doit être supérieur ou égal à 1024".into(),
            ));
        }
        if !is_loopback_host(&self.server.host) && !self.server.allow_remote {
            return Err(ConfigError::Invalid(format!(
                "server.host ({}) n'est pas une adresse loopback; définir server.allow_remote = true pour exposer le serveur",
                self.server.host
            )));
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

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<std::net::IpAddr>()
        .map(|address| address.is_loopback())
        .unwrap_or(false)
}

fn env_first(lookup: &impl Fn(&str) -> Option<String>, names: &[&str]) -> Option<String> {
    for name in names {
        if let Some(value) = lookup(name) {
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn parse_env<T>(value: &str, name: &str) -> Result<T, ConfigError>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| ConfigError::Invalid(format!("{name}={value:?} n'est pas une valeur valide")))
}

fn parse_bool_env(value: &str, name: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(ConfigError::Invalid(format!(
            "{name}={value:?} n'est pas un booléen valide"
        ))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Autorise explicitement une écoute non-loopback. Sans ce flag, le
    /// serveur refuse de démarrer hors loopback.
    pub allow_remote: bool,
    /// Conservé pour compatibilité avec `launcher.env` ; aucune session
    /// signée ne l'utilise encore côté Rust.
    pub secret_key: String,
    pub worker_token: String,
    /// Âge minimal des fichiers orphelins purgés au démarrage. 0 désactive.
    pub orphan_max_age_hours: u64,
    /// Binaire `workflow-cli` utilisé pour exécuter les étapes dans un
    /// processus isolé. Un chemin relatif est résolu depuis `workdir`.
    pub workflow_cli: PathBuf,
    /// Répertoire de travail du serveur et des sous-processus.
    pub workdir: PathBuf,
    /// Racine des fichiers statiques servis sous `/static`.
    pub frontend_dir: PathBuf,
    /// Gabarit HTML de la page principale.
    pub template: PathBuf,
    /// Répertoire des sons du panneau.
    pub sound_design_dir: PathBuf,
    /// Taille maximale d'un journal d'étape avant rotation.
    pub max_step_log_bytes: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 5004,
            allow_remote: false,
            secret_key: String::new(),
            worker_token: String::new(),
            orphan_max_age_hours: 24,
            workflow_cli: PathBuf::from("target/release/workflow-cli"),
            workdir: PathBuf::from("."),
            frontend_dir: PathBuf::from("assets/web/static"),
            template: PathBuf::from("assets/web/templates/index_new.html"),
            sound_design_dir: PathBuf::from("assets/web/sound-design"),
            max_step_log_bytes: 5 * 1024 * 1024,
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

/// Chemins de l'installation Python legacy.
///
/// Ces valeurs ne sont utilisées que par la commande de synchronisation
/// explicite `workflow-cli sync`, qui refuse de tourner si un serveur écoute.
/// Le serveur Rust et ses étapes ne les lisent pas.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LegacyConfig {
    /// Racine du dépôt Python (`workflow_mediapipe`).
    pub root: PathBuf,
    /// Dossier de téléchargement Python (`~/Téléchargements`).
    pub downloads: PathBuf,
    /// Base d'historique Python. Défaut : `<root>/download_history.sqlite3`.
    pub history_db: Option<PathBuf>,
    /// Journaux Python. Défaut : `<root>/logs`.
    pub logs: Option<PathBuf>,
    /// Archives d'analyses STEP7 Python. Défaut : `<root>/archives`.
    pub archives: Option<PathBuf>,
    /// Projets extraits Python. Défaut : `<root>/projets_extraits`.
    pub projects: Option<PathBuf>,
    /// Mot-clé STEP1. Vide : reprend `steps.filter.keyword`.
    pub keyword: String,
}

impl Default for LegacyConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::new(),
            downloads: PathBuf::new(),
            history_db: None,
            logs: None,
            archives: None,
            projects: None,
            keyword: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WebhookConfig {
    /// Démarre le moniteur d'ingestion. Désactivé par défaut : une base vide
    /// ferait télécharger tout l'historique du webhook.
    pub enabled: bool,
    /// Refuse de démarrer le moniteur si `download_history` est vide.
    pub require_history: bool,
    pub url: String,
    pub timeout_s: u64,
    pub cache_ttl_s: u64,
    pub interval_s: u64,
    /// Simule les téléchargements et n'écrit que l'historique (tests/CI).
    pub dry_run: bool,
}

impl Default for WebhookConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            require_history: true,
            url: String::new(),
            timeout_s: 10,
            cache_ttl_s: 60,
            interval_s: 15,
            dry_run: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DownloadConfig {
    /// Autorise les adresses privées/loopback. Réservé aux tests locaux et
    /// aux déploiements internes explicites ; false en production.
    pub allow_private_hosts: bool,
    pub connect_timeout_s: u64,
    pub chunk_timeout_s: u64,
    pub probe_timeout_s: u64,
    pub max_attempts: u32,
    pub backoff_s: f64,
    pub progress_min_interval_s: f64,
    pub min_zip_bytes: u64,
    pub validate_zip: bool,
    pub cooldown_max_s: u64,
    /// Téléchargements simultanés. Python lançait un thread par lien sans
    /// limite ; le port borne la pression disque/réseau (D-020).
    pub max_concurrent: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            allow_private_hosts: false,
            connect_timeout_s: 15,
            chunk_timeout_s: 60,
            probe_timeout_s: 10,
            max_attempts: 3,
            backoff_s: 1.0,
            progress_min_interval_s: 1.0,
            min_zip_bytes: 1_000_000,
            validate_zip: true,
            cooldown_max_s: 3_600,
            max_concurrent: 4,
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
    /// Répertoire des archives à extraire (aucun parcours récursif).
    pub source_dir: PathBuf,
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
    pub normalize: bool,
    pub fps: u32,
    pub use_gpu: bool,
    pub crf: u32,
    pub gpu_workers: u32,
    /// Supprime l'archive source après extraction réussie.
    pub delete_archive: bool,
    /// Délai maximal du sous-processus STEP1 (extraction + normalisation).
    pub timeout_s: u64,
    /// Budget cumulé de décompression par archive (anti zip bomb).
    pub max_extracted_bytes: u64,
    /// Ratio maximal taille décompressée / taille compressée par membre.
    pub max_compression_ratio: u64,
}

impl Default for Step1Config {
    fn default() -> Self {
        Self {
            source_dir: PathBuf::new(),
            skip_preserved_mov: true,
            preserved_mov_names: Vec::new(),
            normalize: true,
            fps: 25,
            use_gpu: true,
            crf: 28,
            gpu_workers: 3,
            delete_archive: true,
            timeout_s: 14_400,
            max_extracted_bytes: 100 * 1024 * 1024 * 1024,
            max_compression_ratio: 200,
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
    pub padding: u32,
    pub threshold: f64,
    /// `cuda` (nominal) ou `cpu` (tests et dépannage).
    pub device: String,
    /// Chemin relatif à `paths.models`.
    pub model: PathBuf,
    /// Bibliothèque ONNX Runtime à charger dynamiquement. Vide : nom système.
    pub onnxruntime: Option<PathBuf>,
    /// Répertoires CUDA/cuDNN à ajouter à `LD_LIBRARY_PATH`.
    pub cuda_lib_dirs: Vec<PathBuf>,
    /// Ajoute automatiquement les répertoires `nvidia/*/lib` voisins de
    /// `onnxruntime` dans un site-packages Python.
    pub auto_cuda_libs: bool,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg_threads: u32,
    pub use_tf32: bool,
}

impl Default for Step2Config {
    fn default() -> Self {
        Self {
            cudnn_algo_search: true,
            batch_size: 8,
            window: 100,
            stride: 50,
            padding: 25,
            threshold: 0.5,
            device: "cuda".into(),
            model: PathBuf::from("step2/transnetv2.onnx"),
            onnxruntime: None,
            cuda_lib_dirs: Vec::new(),
            auto_cuda_libs: true,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            ffmpeg_threads: 0,
            use_tf32: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step3Config {
    pub enabled: bool,
    pub yamnet_split: bool,
    pub require_edgetpu: bool,
    /// `tpu` (nominal) ou `cpu` (tests explicites uniquement).
    pub device: String,
    pub embedding_stride: u32,
    pub adaptive_stride: bool,
    /// Nombre de fenêtres de parole à partir duquel le stride adaptatif vaut 2.
    pub adaptive_threshold: u32,
    /// Modèle YAMNet Split compilé pour l'Edge TPU.
    pub yamnet: PathBuf,
    /// Modèle YAMNet Split quantifié, utilisé uniquement par `device = "cpu"`.
    pub yamnet_fallback: PathBuf,
    /// Matrice mel YAMNet (257, 64) au format NPY.
    pub mel_matrix: PathBuf,
    /// ECAPA-TDNN float32 exécuté sur CPU via ONNX Runtime.
    pub ecapa: PathBuf,
    pub tflite: PathBuf,
    pub edgetpu: PathBuf,
    /// Bibliothèque ONNX Runtime à charger dynamiquement. Vide : nom système.
    pub onnxruntime: Option<PathBuf>,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg_threads: u32,
    pub cpu_threads: u32,
    pub vad_threshold: f64,
    pub hop_overlap: f64,
    pub median_filter_size: u32,
    pub hangover_sec: f64,
    pub max_speakers: u32,
    pub clustering_threshold: f64,
    pub min_speaker_speech_duration: f64,
    pub min_cluster_embeddings: u32,
    pub ecapa_batch_size: u32,
}

impl Default for Step3Config {
    fn default() -> Self {
        Self {
            enabled: true,
            yamnet_split: true,
            require_edgetpu: true,
            device: "tpu".into(),
            embedding_stride: 1,
            adaptive_stride: false,
            adaptive_threshold: 500,
            yamnet: PathBuf::from("step3/yamnet_split_edgetpu_compiled.tflite"),
            yamnet_fallback: PathBuf::from("step3/yamnet_split_quantized.tflite"),
            mel_matrix: PathBuf::from("step3/yamnet_mel_matrix.npy"),
            ecapa: PathBuf::from("step3/ecapa_tdnn_speaker_float.onnx"),
            tflite: PathBuf::from("vendor/libtensorflowlite_c.so"),
            edgetpu: PathBuf::from("/usr/lib/x86_64-linux-gnu/libedgetpu.so.1"),
            onnxruntime: None,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            ffmpeg_threads: 0,
            cpu_threads: 1,
            vad_threshold: 0.2,
            hop_overlap: 0.5,
            median_filter_size: 5,
            hangover_sec: 1.0,
            max_speakers: 5,
            clustering_threshold: 0.32,
            min_speaker_speech_duration: 7.0,
            min_cluster_embeddings: 4,
            ecapa_batch_size: 32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Step4Config {
    pub enabled: bool,
    /// `tpu` (nominal) ou `cpu` (tests/dépannage, même chemin de code).
    pub device: String,
    /// Utilise la paire BlazeFace/FaceMesh co-compilée pour partager la SRAM.
    pub cocompiled: bool,
    pub tflite: PathBuf,
    pub edgetpu: PathBuf,
    pub blazeface: PathBuf,
    pub facemesh: PathBuf,
    pub blendshapes: PathBuf,
    pub object_detector: String,
    pub object_detector_path: PathBuf,
    pub blazeface_cpu: PathBuf,
    pub facemesh_cpu: PathBuf,
    pub object_detector_cpu: PathBuf,
    pub object_detection: bool,
    pub object_min_missing: u32,
    pub face_reset_frames: u32,
    pub object_score_threshold: f64,
    pub object_max: u32,
    pub fast: bool,
    pub cpu_threads: u32,
    pub blendshapes_profile: String,
    pub blendshapes_export: Vec<String>,
    pub blendshapes_include_tongue: bool,
    /// Conservé pour compatibilité avec les anciens TOML ; le chemin TPU
    /// calcule les blendshapes à chaque frame, comme le script Python.
    pub blendshapes_throttle: u32,
    pub voluminous_exports: bool,
    pub test_cpu_workers: u32,
    pub jaw_open_threshold: f64,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    /// Largeur de décodage ; 0 conserve la résolution source.
    pub decode_width: u32,
    /// Hauteur de décodage ; 0 conserve la résolution source.
    pub decode_height: u32,
}

impl Default for Step4Config {
    fn default() -> Self {
        Self {
            enabled: true,
            device: "tpu".into(),
            cocompiled: true,
            tflite: PathBuf::from("vendor/libtensorflowlite_c.so"),
            edgetpu: PathBuf::from("/usr/lib/x86_64-linux-gnu/libedgetpu.so.1"),
            blazeface: PathBuf::from("step4/blazeface_front_quantized_edgetpu.tflite"),
            facemesh: PathBuf::from("step4/facemesh_quantized_edgetpu.tflite"),
            blendshapes: PathBuf::from("step4/face_blendshapes.tflite"),
            object_detector: "detect_edgetpu".into(),
            object_detector_path: PathBuf::from("step4/detect_edgetpu.tflite"),
            blazeface_cpu: PathBuf::from("step4/blazeface_front_quantized.tflite"),
            facemesh_cpu: PathBuf::from("step4/facemesh_quantized.tflite"),
            object_detector_cpu: PathBuf::from("step4/detect.tflite"),
            object_detection: true,
            object_min_missing: 25,
            face_reset_frames: 1,
            object_score_threshold: 0.3,
            object_max: 5,
            fast: true,
            cpu_threads: 1,
            blendshapes_profile: "full".into(),
            blendshapes_export: vec!["jawOpen".into()],
            blendshapes_include_tongue: false,
            blendshapes_throttle: 2,
            voluminous_exports: false,
            test_cpu_workers: 1,
            jaw_open_threshold: 0.08,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            decode_width: 0,
            decode_height: 0,
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
    pub mode: String,
    pub restore_archives: bool,
    pub delete_source: bool,
    pub timeout_s: u64,
}

impl Default for Step7Config {
    fn default() -> Self {
        Self {
            cleanup_threshold_hours: 48.0,
            mode: "lenient".into(),
            restore_archives: false,
            delete_source: true,
            timeout_s: 14_400,
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
    pub docs_subfolder: String,
    pub max_drivers_kept: usize,
}

impl Default for Step8Config {
    fn default() -> Self {
        Self {
            timeout_s: 14_400,
            cli: PathBuf::from("effectcraft-cli"),
            panel_jsx: PathBuf::from("assets/effectcraft/MediaSolution.jsx"),
            panel_config: PathBuf::new(),
            docs_subfolder: "docs".into(),
            max_drivers_kept: 10,
        }
    }
}

/// Politique de rétention des données de travail. Les sorties finalisées
/// (`paths.finalized`) en sont volontairement exclues : elles ne sont jamais
/// supprimées automatiquement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RetentionConfig {
    /// Archives d'analyses conservées, en jours. 0 désactive le nettoyage.
    pub archives_days: u64,
    /// Journaux `.log` conservés, en jours. 0 désactive le nettoyage.
    pub logs_days: u64,
    /// Téléchargements conservés, en jours. 0 désactive le nettoyage.
    /// Le nettoyage n'est autorisé que sous `paths.base`.
    pub downloads_days: u64,
    /// Manifestes et rapports JSON conservés, en jours. 0 désactive.
    pub reports_days: u64,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            archives_days: 90,
            logs_days: 30,
            downloads_days: 0,
            reports_days: 90,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HardwareConfig {
    pub lock_path: PathBuf,
}

impl Default for HardwareConfig {
    fn default() -> Self {
        Self {
            // Toujours distinct du verrou Python legacy `/tmp/workflow-hw.lock` (D-012).
            lock_path: PathBuf::from("/tmp/workflow-rs-hw.lock"),
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
    fn retention_section_is_typed() {
        let config = Config::from_toml("[retention]\narchives_days = 12\nlogs_days = 7\n").unwrap();
        assert_eq!(config.retention.archives_days, 12);
        assert_eq!(config.retention.logs_days, 7);
        assert_eq!(config.retention.downloads_days, 0);
        assert_eq!(config.retention.reports_days, 90);
    }

    #[test]
    fn worker_token_default_is_rejected_for_server() {
        let config = Config::default();
        assert!(config.validate_server().is_err());
    }

    #[test]
    fn legacy_port_is_rejected_for_the_rust_server() {
        let mut config = Config::default();
        config.server.secret_key = "secret".into();
        config.server.worker_token = "token".into();
        config.steps.step1.source_dir = PathBuf::from("/tmp/downloads");
        config.server.port = 5003;
        assert!(config.validate_server().is_err());
        config.server.port = 5004;
        assert!(config.validate_server().is_ok());
    }

    #[test]
    fn step3_thresholds_are_validated() {
        let mut config = Config::default();
        config.steps.step3.vad_threshold = 1.5;
        assert!(config.validate_common().is_err());
        let mut config = Config::default();
        config.steps.step3.adaptive_threshold = 0;
        assert!(config.validate_common().is_err());
    }

    #[test]
    fn env_override_applies_secret() {
        let mut config = Config::default();
        config
            .apply_env_lookup(|name| match name {
                "WORKFLOW_SERVER_SECRET_KEY" => Some("secret".into()),
                "WORKFLOW_SERVER_WORKER_TOKEN" => Some("token".into()),
                _ => None,
            })
            .unwrap();
        assert_eq!(config.server.secret_key, "secret");
        assert_eq!(config.server.worker_token, "token");
    }

    #[test]
    fn invalid_env_override_is_rejected() {
        let mut config = Config::default();
        let error = config
            .apply_env_lookup(|name| (name == "WORKFLOW_SERVER_PORT").then(|| "abc".into()))
            .unwrap_err();
        assert!(error.to_string().contains("WORKFLOW_SERVER_PORT"));
    }

    #[test]
    fn invalid_bool_env_override_is_rejected() {
        let mut config = Config::default();
        let error = config
            .apply_env_lookup(|name| (name == "WORKFLOW_WEBHOOK_ENABLED").then(|| "oui".into()))
            .unwrap_err();
        assert!(error.to_string().contains("WORKFLOW_WEBHOOK_ENABLED"));
    }

    #[test]
    fn default_hardware_lock_is_the_rust_one() {
        let config = Config::from_toml("").unwrap();
        assert_eq!(
            config.hardware.lock_path,
            PathBuf::from("/tmp/workflow-rs-hw.lock")
        );
    }

    #[test]
    fn python_hardware_lock_is_rejected_everywhere() {
        let mut config = Config::default();
        config.hardware.lock_path = PathBuf::from("/run/user/1000/workflow-hw.lock");
        assert!(config.validate_common().is_err());
    }

    #[test]
    fn non_loopback_host_requires_allow_remote() {
        let mut config = Config::default();
        config.server.worker_token = "token".into();
        config.steps.step1.source_dir = PathBuf::from("/tmp/downloads");
        config.server.host = "0.0.0.0".into();
        assert!(config.validate_server().is_err());
        config.server.allow_remote = true;
        assert!(config.validate_server().is_ok());
    }

    #[test]
    fn empty_secret_key_is_accepted() {
        let mut config = Config::default();
        config.server.worker_token = "token".into();
        config.steps.step1.source_dir = PathBuf::from("/tmp/downloads");
        assert!(config.validate_server().is_ok());
    }
}
