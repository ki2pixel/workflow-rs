use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;
use workflow_core::Config;
use workflow_ingest::download::Downloader;
use workflow_ingest::history::HistoryRepository;
use workflow_ingest::monitor::{plan_downloads, PlanContext};
use workflow_ingest::url::normalize_url;
use workflow_ingest::webhook::WebhookClient;
use workflow_steps::retention::{apply_retention, plan_retention, RetentionOptions};
use workflow_steps::step1::{run_normalize_only, run_step1, Step1Options};
use workflow_steps::step2::{run_step2, Step2Options};
#[cfg(unix)]
use workflow_steps::step3::{run_step3, Step3Options};
#[cfg(unix)]
use workflow_steps::step4::{run_step4, Step4Options};
use workflow_steps::step5::audio::{read_audio_meta, reduce_audio_file};
use workflow_steps::step5::batch::{run_step5_batch, Step5BatchOptions};
use workflow_steps::step5::video::{reduce_video_file, VideoOptions};
use workflow_steps::step6::{
    build_ae_payload_file, run_step6_batch, AeOptions, SegmentConfig, Step6BatchOptions,
};
use workflow_steps::step7::{
    finalize_work_dir, find_projects_to_finalize, FinalizeMode, Step7Options,
};
use workflow_steps::step8::{run_step8, Step8Options};

#[derive(Debug, Parser)]
#[command(name = "workflow-cli", version, about = "CLI du pipeline Rust")]
struct Cli {
    /// Fichier TOML. Les secrets sont surchargés par l'environnement.
    #[arg(long, global = true, default_value = "config/config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Vérifie ou affiche la configuration.
    Config(ConfigArgs),
    /// Historique SQLite des téléchargements (import depuis Python, stats).
    History(HistoryArgs),
    /// Diagnostic du webhook, sans écriture d'historique.
    Webhook(WebhookArgs),
    /// Télécharge une URL avec le même moteur que le moniteur (sans historique).
    Download(DownloadArgs),
    /// Extraction des archives et normalisation vidéo STEP1.
    Step1(Step1Args),
    /// Détection de scènes STEP2 (TransNetV2 ONNX Runtime).
    Step2(Step2Args),
    /// Diarisation audio STEP3 (YAMNet Split Coral, ECAPA-TDNN, AHC).
    #[cfg(unix)]
    Step3(Step3Args),
    /// Tracking facial et objets STEP4 (BlazeFace, FaceMesh, Coral).
    #[cfg(unix)]
    Step4(Step4Args),
    /// Sous-commandes de STEP5 (réduction JSON).
    Step5(Step5Args),
    /// Sous-commandes de STEP6 (pré-traitement AE).
    Step6(Step6Args),
    /// Finalisation STEP7 et archivage des analyses.
    Step7(Step7Args),
    /// Génération des projets EffectCraft (STEP8, effectcraft-cli headless).
    Step8(Step8Args),
    /// Planifie ou applique la rétention des données de travail.
    Retention(RetentionArgs),
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(subcommand)]
    command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Charge et valide la configuration.
    Check,
    /// Affiche la configuration en JSON, secrets masqués.
    Show,
}

#[derive(Debug, Args)]
struct HistoryArgs {
    #[command(subcommand)]
    command: HistoryCommand,
}

#[derive(Debug, Subcommand)]
enum HistoryCommand {
    /// Importe la base Python en lecture seule vers la base Rust isolée.
    Import {
        /// Base Python (`download_history.sqlite3`).
        #[arg(long)]
        from: PathBuf,
        /// Remplace l'historique Rust au lieu de fusionner.
        #[arg(long)]
        replace: bool,
        /// Compte les lignes sans écrire.
        #[arg(long)]
        dry_run: bool,
    },
    /// Affiche le nombre d'URL et d'échecs dans la base Rust.
    Stats,
    /// Vérifie que les URL stockées sont déjà normalisées (migration).
    Audit {
        /// Nombre d'exemples affichés.
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
}

#[derive(Debug, Args)]
struct WebhookArgs {
    #[command(subcommand)]
    command: WebhookCommand,
}

#[derive(Debug, Subcommand)]
enum WebhookCommand {
    /// Interroge le webhook une fois et affiche le nombre d'enregistrements.
    Probe {
        /// Sortie JSON complète.
        #[arg(long)]
        json: bool,
        /// Compare le webhook à l'historique Rust et liste ce qui serait téléchargé.
        #[arg(long)]
        plan: bool,
    },
}

#[derive(Debug, Args)]
struct DownloadArgs {
    #[arg(long)]
    url: String,
    /// Timestamp utilisé pour le nom de repli et le journal.
    #[arg(long)]
    timestamp: Option<String>,
    /// Répertoire de destination, explicite pour ne pas polluer STEP1.
    #[arg(long)]
    output_dir: PathBuf,
    #[arg(long)]
    filename: Option<String>,
}

#[derive(Debug, Args)]
struct Step1Args {
    #[command(subcommand)]
    command: Step1Command,
}

#[derive(Debug, Subcommand)]
enum Step1Command {
    /// Extrait les nouvelles archives puis normalise les projets.
    Run {
        /// Remplace `steps.step1.source_dir`.
        #[arg(long)]
        source_dir: Option<PathBuf>,
        /// Désactive la suppression des archives après succès.
        #[arg(long)]
        keep_archives: bool,
    },
    /// Normalise les projets déjà extraits (rattrapage).
    NormalizeOnly {
        /// Ré-encode aussi les vidéos déjà conformes.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Args)]
struct Step2Args {
    #[command(subcommand)]
    command: Step2Command,
}

#[derive(Debug, Subcommand)]
enum Step2Command {
    /// Analyse récursivement un dossier et écrit un CSV à côté de chaque vidéo.
    Detect {
        /// Dossier contenant les vidéos (obligatoire, pas de cwd implicite).
        #[arg(long)]
        input_dir: PathBuf,
        /// Re-traite même si le CSV existe déjà.
        #[arg(long)]
        force: bool,
        /// Modèle ONNX (défaut : `steps.step2.model` sous `paths.models`).
        #[arg(long)]
        model: Option<PathBuf>,
        /// `cuda` (nominal) ou `cpu` (tests/dépannage).
        #[arg(long)]
        device: Option<String>,
        /// Bibliothèque ONNX Runtime à charger dynamiquement.
        #[arg(long)]
        onnxruntime: Option<PathBuf>,
        /// Répertoires CUDA/cuDNN supplémentaires (répétable).
        #[arg(long = "cuda-lib-dir")]
        cuda_lib_dirs: Vec<PathBuf>,
        /// Désactive la découverte automatique des `nvidia/*/lib` voisins.
        #[arg(long)]
        no_auto_cuda_libs: bool,
        #[arg(long)]
        batch_size: Option<usize>,
        #[arg(long)]
        window: Option<usize>,
        #[arg(long)]
        stride: Option<usize>,
        #[arg(long)]
        padding: Option<usize>,
        #[arg(long)]
        threshold: Option<f32>,
        #[arg(long)]
        ffmpeg: Option<PathBuf>,
        #[arg(long)]
        ffmpeg_threads: Option<u32>,
    },
}

#[cfg(unix)]
#[derive(Debug, Args)]
struct Step3Args {
    #[command(subcommand)]
    command: Step3Command,
}

#[cfg(unix)]
#[derive(Debug, Subcommand)]
enum Step3Command {
    /// Analyse récursivement un dossier et écrit `<stem>_audio.json`.
    Diarize {
        /// Dossier contenant les vidéos.
        #[arg(long)]
        input_dir: PathBuf,
        /// Re-traite même si le JSON audio existe déjà.
        #[arg(long)]
        force: bool,
        /// `tpu` (nominal) ou `cpu` (tests explicites).
        #[arg(long)]
        device: Option<String>,
        /// Surcharge le modèle YAMNet Split compilé.
        #[arg(long)]
        yamnet: Option<PathBuf>,
        /// Surcharge le modèle YAMNet quantifié de test CPU.
        #[arg(long)]
        yamnet_fallback: Option<PathBuf>,
        /// Surcharge le modèle ECAPA-TDNN.
        #[arg(long)]
        ecapa: Option<PathBuf>,
        /// Surcharge la matrice mel YAMNet.
        #[arg(long)]
        mel_matrix: Option<PathBuf>,
        /// Surcharge la bibliothèque TFLite C.
        #[arg(long)]
        tflite: Option<PathBuf>,
        /// Surcharge la bibliothèque Edge TPU.
        #[arg(long)]
        edgetpu: Option<PathBuf>,
        /// Surcharge ONNX Runtime.
        #[arg(long)]
        onnxruntime: Option<PathBuf>,
        /// Remplace le répertoire des journaux STEP3.
        #[arg(long)]
        logs_dir: Option<PathBuf>,
    },
}

#[cfg(unix)]
#[derive(Debug, Args)]
struct Step4Args {
    #[command(subcommand)]
    command: Step4Command,
}

#[cfg(unix)]
#[derive(Debug, Subcommand)]
enum Step4Command {
    /// Analyse les vidéos d'un dossier et écrit un JSON à côté de chacune.
    Track {
        /// Dossier contenant les vidéos.
        #[arg(long)]
        input_dir: PathBuf,
        /// `tpu` (nominal) ou `cpu` (tests/dépannage).
        #[arg(long)]
        device: Option<String>,
        /// Désactive EfficientDet et ne conserve que les visages.
        #[arg(long)]
        no_object_detection: bool,
        /// Nombre de frames sans visage avant d'activer les objets.
        #[arg(long)]
        object_min_missing: Option<u32>,
        /// Nombre de frames consécutives avec visage avant réinitialisation.
        #[arg(long)]
        face_reset_frames: Option<u32>,
        /// Force une largeur de décodage (0 = résolution source).
        #[arg(long)]
        decode_width: Option<u32>,
        /// Force une hauteur de décodage (0 = résolution source).
        #[arg(long)]
        decode_height: Option<u32>,
        /// Remplace le répertoire des journaux STEP4.
        #[arg(long)]
        logs_dir: Option<PathBuf>,
        /// Surcharge la bibliothèque TFLite C.
        #[arg(long)]
        tflite: Option<PathBuf>,
        /// Surcharge la bibliothèque Edge TPU.
        #[arg(long)]
        edgetpu: Option<PathBuf>,
        /// Autorise un JSON partiel si le CSV ou l'audio manquent (tests).
        #[arg(long)]
        allow_partial_speaking: bool,
    },
}

#[derive(Debug, Args)]
struct Step5Args {
    #[command(subcommand)]
    command: Step5Command,
}

#[derive(Debug, Subcommand)]
enum Step5Command {
    /// Réduit tous les projets d'un répertoire (mode orchestrateur).
    Project {
        /// Dossier contenant les projets (`projets_extraits`).
        #[arg(long)]
        work_dir: PathBuf,
        /// Filtre les dossiers projet par mot-clé (vide = tous).
        #[arg(long)]
        keyword: Option<String>,
    },
    /// Réduit un fichier `_audio.json` (ou un audio brut STEP3).
    Audio {
        #[arg(long)]
        input: PathBuf,
        /// Par défaut, réécrit le fichier d'entrée de façon atomique.
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Réduit un fichier de tracking STEP4 (ou un tracking déjà réduit).
    Video {
        #[arg(long)]
        input: PathBuf,
        /// Par défaut : `<stem>_tracking.json` (ou réécriture si le nom le contient déjà).
        #[arg(long)]
        output: Option<PathBuf>,
        /// Audio réduit ou brut, utilisé pour `temporal_alignment`.
        #[arg(long)]
        audio: Option<PathBuf>,
        #[arg(long)]
        no_tracking_analytics: bool,
        #[arg(long)]
        expression_summary: bool,
        /// Clés séparées par des virgules, par exemple `jawOpen,eyeBlinkLeft`.
        #[arg(long, value_delimiter = ',')]
        expression_keys: Option<Vec<String>>,
    },
}

#[derive(Debug, Args)]
struct Step6Args {
    #[command(subcommand)]
    command: Step6Command,
}

#[derive(Debug, Subcommand)]
enum Step6Command {
    /// Construit tous les `_ae.json` d'un répertoire (mode orchestrateur).
    Project {
        /// Dossier contenant les projets (`projets_extraits`).
        #[arg(long)]
        work_dir: PathBuf,
        /// Filtre les dossiers projet par mot-clé (vide = tous).
        #[arg(long)]
        keyword: Option<String>,
    },
    /// Construit `<stem>_ae.json` à partir du tracking, de l'audio et du CSV.
    Build {
        #[arg(long)]
        tracking: PathBuf,
        /// Par défaut : `<stem>_ae.json`.
        #[arg(long)]
        output: Option<PathBuf>,
        /// Par défaut : `<stem>_audio.json` à côté du tracking.
        #[arg(long)]
        audio: Option<PathBuf>,
        /// Par défaut : `<stem>.csv` à côté du tracking.
        #[arg(long)]
        csv: Option<PathBuf>,
    },
}

#[derive(Debug, Args)]
struct Step7Args {
    #[command(subcommand)]
    command: Step7Command,
}

#[derive(Debug, Subcommand)]
enum Step7Command {
    /// Archive, copie et publie les projets prêts vers la destination finale.
    Finalize {
        /// Par défaut : `paths.projects`.
        #[arg(long)]
        work_dir: Option<PathBuf>,
        /// Par défaut : `paths.finalized` (`/mnt/cache`).
        #[arg(long)]
        output_dir: Option<PathBuf>,
        /// Par défaut : `paths.archives`.
        #[arg(long)]
        archives_dir: Option<PathBuf>,
        /// Par défaut : `paths.logs`.
        #[arg(long)]
        logs_dir: Option<PathBuf>,
        /// `lenient` (défaut), `strict` ou `videos`.
        #[arg(long)]
        mode: Option<String>,
        /// Ne traiter qu'un seul projet.
        #[arg(long)]
        project: Option<String>,
        /// Conserve le dossier source après publication.
        #[arg(long)]
        no_delete: bool,
        /// Restaure les analyses archivées dans `docs/` après la copie.
        #[arg(long)]
        restore_archives: bool,
        /// Liste les projets sans archiver, copier ni supprimer.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Args)]
struct Step8Args {
    #[command(subcommand)]
    command: Step8Command,
}

#[derive(Debug, Args)]
struct RetentionArgs {
    #[command(subcommand)]
    command: RetentionCommand,
}

#[derive(Debug, Subcommand)]
enum RetentionCommand {
    /// Liste les candidats sans supprimer.
    Plan(RetentionOverrides),
    /// Supprime les candidats retenus (action explicite).
    Apply(RetentionOverrides),
}

#[derive(Debug, Args, Clone)]
struct RetentionOverrides {
    /// Par défaut : `paths.projects`.
    #[arg(long)]
    projects_dir: Option<PathBuf>,
    /// Par défaut : `paths.archives`.
    #[arg(long)]
    archives_dir: Option<PathBuf>,
    /// Par défaut : `paths.logs`.
    #[arg(long)]
    logs_dir: Option<PathBuf>,
    /// Par défaut : `paths.downloads`.
    #[arg(long)]
    downloads_dir: Option<PathBuf>,
    /// Seuil d'inactivité des projets sources, en heures (0 désactive).
    #[arg(long)]
    orphan_hours: Option<f64>,
    /// Rétention des archives d'analyses, en jours (0 désactive).
    #[arg(long)]
    archives_days: Option<u64>,
    /// Rétention des journaux, en jours (0 désactive).
    #[arg(long)]
    logs_days: Option<u64>,
    /// Rétention des téléchargements, en jours (0 désactive).
    #[arg(long)]
    downloads_days: Option<u64>,
    /// Rétention des rapports JSON, en jours (0 désactive).
    #[arg(long)]
    reports_days: Option<u64>,
}

#[derive(Debug, Subcommand)]
enum Step8Command {
    /// Exécute le panel MediaSolution via `effectcraft-cli` en headless.
    Run {
        /// Manifeste STEP7. Par défaut : le plus récent de `logs/step7`.
        #[arg(long)]
        manifest: Option<PathBuf>,
        /// Dossier projet à traiter (répétable ; court-circuite le manifeste).
        #[arg(long = "folder")]
        folders: Vec<PathBuf>,
        /// Exécutable effectcraft-cli (défaut : configuration STEP8).
        #[arg(long)]
        cli: Option<PathBuf>,
        /// Panel `MediaSolution.jsx` (défaut : configuration STEP8).
        #[arg(long)]
        panel_jsx: Option<PathBuf>,
        /// Fichier `mediasolution_ec.json` du panel.
        #[arg(long)]
        config_json: Option<PathBuf>,
        /// Par défaut : `paths.logs`.
        #[arg(long)]
        logs_dir: Option<PathBuf>,
        /// Timeout interne en secondes (0 désactive ; défaut : configuration STEP8).
        #[arg(long)]
        timeout_s: Option<u64>,
        /// Génère le driver et affiche les dossiers sans lancer EffectCraft.
        #[arg(long)]
        dry_run: bool,
    },
}

fn default_ae_output(tracking: &std::path::Path) -> PathBuf {
    let name = tracking
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let lower = name.to_ascii_lowercase();
    if lower.ends_with("_ae.json") {
        return tracking.to_path_buf();
    }
    let stem = if lower.ends_with("_tracking.json") {
        &name[..name.len() - "_tracking.json".len()]
    } else if lower.ends_with(".json") {
        &name[..name.len() - ".json".len()]
    } else {
        name
    };
    tracking.with_file_name(format!("{stem}_ae.json"))
}

fn default_video_output(input: &std::path::Path) -> PathBuf {
    let name = input
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if name.ends_with("_tracking.json") {
        return input.to_path_buf();
    }
    let stem = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("tracking");
    input.with_file_name(format!("{stem}_tracking.json"))
}

fn retention_options(
    config: &Config,
    overrides: RetentionOverrides,
    dry_run: bool,
) -> RetentionOptions {
    let resolved = config.paths.resolve();
    let downloads_dir = overrides.downloads_dir.unwrap_or(resolved.downloads);
    let downloads_managed =
        normalized_path(&downloads_dir).starts_with(normalized_path(&resolved.base));
    RetentionOptions {
        projects_dir: overrides.projects_dir.unwrap_or(resolved.projects),
        archives_dir: overrides.archives_dir.unwrap_or(resolved.archives),
        logs_dir: overrides.logs_dir.unwrap_or(resolved.logs),
        downloads_dir,
        downloads_managed,
        orphan_hours: overrides
            .orphan_hours
            .unwrap_or(config.steps.step7.cleanup_threshold_hours),
        archives_days: overrides
            .archives_days
            .unwrap_or(config.retention.archives_days),
        logs_days: overrides.logs_days.unwrap_or(config.retention.logs_days),
        downloads_days: overrides
            .downloads_days
            .unwrap_or(config.retention.downloads_days),
        reports_days: overrides
            .reports_days
            .unwrap_or(config.retention.reports_days),
        dry_run,
    }
}

fn normalized_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn validate_retention_paths(options: &RetentionOptions, config: &Config) -> Result<()> {
    let resolved = config.paths.resolve();
    let cwd = std::env::current_dir().context("répertoire courant introuvable")?;
    let mut protected = vec![
        normalized_path(&resolved.base),
        normalized_path(&resolved.finalized),
        normalized_path(&resolved.cache_root),
        normalized_path(&cwd),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        protected.push(normalized_path(Path::new(&home)));
    }
    let deliverables = [
        normalized_path(&resolved.finalized),
        normalized_path(&resolved.cache_root),
    ];
    let roots = [
        ("projets", &options.projects_dir),
        ("archives", &options.archives_dir),
        ("journaux", &options.logs_dir),
        ("téléchargements", &options.downloads_dir),
    ];
    for (label, root) in roots {
        let root = normalized_path(root);
        if root == Path::new("/")
            || protected.iter().any(|guard| guard.starts_with(&root))
            || deliverables.iter().any(|guard| root.starts_with(guard))
        {
            anyhow::bail!(
                "rétention refusée pour les {label}: {} est un répertoire racine, un parent du runtime ou un livrable",
                root.display()
            );
        }
    }
    if options.downloads_days > 0 && !options.downloads_managed {
        anyhow::bail!(
            "nettoyage des téléchargements refusé: {} est hors du runtime ; utilisez 0 ou un répertoire dédié sous paths.base",
            options.downloads_dir.display()
        );
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,ort=error")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config)
        .with_context(|| format!("chargement de {}", cli.config.display()))?;
    config.validate_common()?;

    match cli.command {
        Command::Config(args) => match args.command {
            ConfigCommand::Check => {
                println!("configuration valide: {}", cli.config.display());
            }
            ConfigCommand::Show => {
                let mut value = serde_json::to_value(&config)?;
                if let Some(server) = value
                    .get_mut("server")
                    .and_then(|value| value.as_object_mut())
                {
                    for key in ["secret_key", "worker_token"] {
                        let configured = server
                            .get(key)
                            .and_then(|value| value.as_str())
                            .is_some_and(|value| !value.is_empty());
                        if configured {
                            server.insert(key.to_string(), json!("***"));
                        }
                    }
                }
                println!("{}", serde_json::to_string_pretty(&value)?);
            }
        },
        Command::History(args) => {
            let resolved = config.paths.resolve();
            match args.command {
                HistoryCommand::Import {
                    from,
                    replace,
                    dry_run,
                } => {
                    if dry_run {
                        let source_count = HistoryRepository::preview_import(&from)?;
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&json!({
                                "dry_run": true,
                                "source": from,
                                "source_count": source_count,
                                "destination": resolved.history_db,
                            }))?
                        );
                    } else {
                        let repository = HistoryRepository::open(&resolved.history_db)?;
                        let report = repository.import_from(&from, replace)?;
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    }
                }
                HistoryCommand::Stats => {
                    let repository = HistoryRepository::open(&resolved.history_db)?;
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "path": repository.path(),
                            "history": repository.count()?,
                            "failures": repository.failure_count()?,
                        }))?
                    );
                }
                HistoryCommand::Audit { limit } => {
                    let repository = HistoryRepository::open(&resolved.history_db)?;
                    let entries = repository.get_ts_by_url()?;
                    let mut changes = Vec::new();
                    for url in entries.keys() {
                        let normalized = normalize_url(url);
                        if &normalized != url {
                            changes.push(json!({
                                "stored": url,
                                "normalized": normalized,
                            }));
                        }
                    }
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "path": repository.path(),
                            "total": entries.len(),
                            "changes": changes.len(),
                            "examples": changes.iter().take(limit).collect::<Vec<_>>(),
                        }))?
                    );
                    if !changes.is_empty() {
                        anyhow::bail!(
                            "{} URL(s) ne sont pas normalisées: le moniteur risque de les retélécharger",
                            changes.len()
                        );
                    }
                }
            }
        }
        Command::Webhook(args) => match args.command {
            WebhookCommand::Probe {
                json: as_json,
                plan,
            } => {
                let client = WebhookClient::new(&config.webhook)?;
                let runtime = tokio::runtime::Runtime::new()?;
                let records = runtime.block_on(client.fetch());
                let status = client.status();
                let Some(records) = records else {
                    anyhow::bail!(
                        "webhook indisponible: {}",
                        status.error.unwrap_or_else(|| "erreur inconnue".into())
                    );
                };
                if plan {
                    let resolved = config.paths.resolve();
                    let repository = HistoryRepository::open(&resolved.history_db)?;
                    let history = repository.get_urls()?;
                    let failures = repository.get_failure_map()?;
                    let tracked = std::collections::HashSet::new();
                    let plans = plan_downloads(
                        &records,
                        &PlanContext {
                            history: &history,
                            failures: &failures,
                            tracked: &tracked,
                            now: chrono::Local::now().naive_local(),
                            interval_s: config.webhook.interval_s,
                            cooldown_max_s: config.download.cooldown_max_s,
                        },
                    );
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "history": history.len(),
                            "failures": failures.len(),
                            "records": records.len(),
                            "planned": plans.len(),
                            "plans": plans,
                        }))?
                    );
                    return Ok(());
                }
                if as_json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "status": status,
                            "count": records.len(),
                            "records": records.iter().take(10).collect::<Vec<_>>(),
                        }))?
                    );
                } else {
                    println!(
                        "webhook: {} enregistrement(s), disponible={}, dernier échange={}",
                        records.len(),
                        status.available,
                        status.last_fetch_ts.unwrap_or_else(|| "n/a".into())
                    );
                }
            }
        },
        Command::Download(args) => {
            let output_dir = args.output_dir;
            std::fs::create_dir_all(&output_dir)?;
            let timestamp = args
                .timestamp
                .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
            let downloader = Downloader::new(&config.download)?;
            let runtime = tokio::runtime::Runtime::new()?;
            let result = runtime.block_on(downloader.download(
                &args.url,
                &timestamp,
                &output_dir,
                args.filename.as_deref(),
                None,
            ));
            println!("{}", serde_json::to_string_pretty(&result)?);
            if !result.success {
                std::process::exit(1);
            }
        }
        Command::Step1(args) => match args.command {
            Step1Command::Run {
                source_dir,
                keep_archives,
            } => {
                let mut options = Step1Options::from_config(&config, source_dir)?;
                if keep_archives {
                    options.delete_archive = false;
                }
                let report = run_step1(&options)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.success() {
                    anyhow::bail!(
                        "STEP1: {} archive(s) en échec, {} vidéo(s) non normalisée(s)",
                        report.failed,
                        report.normalization.failed
                    );
                }
            }
            Step1Command::NormalizeOnly { force } => {
                let mut options = Step1Options::from_config(&config, None)?;
                options.force_normalize = force;
                let summary = run_normalize_only(&options)?;
                println!("{}", serde_json::to_string_pretty(&summary)?);
                if !summary.is_success() {
                    anyhow::bail!("STEP1: {} vidéo(s) non normalisée(s)", summary.failed);
                }
            }
        },
        Command::Step2(args) => match args.command {
            Step2Command::Detect {
                input_dir,
                force,
                model,
                device,
                onnxruntime,
                cuda_lib_dirs,
                no_auto_cuda_libs,
                batch_size,
                window,
                stride,
                padding,
                threshold,
                ffmpeg,
                ffmpeg_threads,
            } => {
                let mut options = Step2Options::from_config(&config, input_dir)?;
                options.force = force;
                if let Some(model) = model {
                    options.model = model;
                }
                if let Some(device) = device {
                    options.device =
                        workflow_steps::step2::engine::ExecutionDevice::parse(&device)?;
                }
                if let Some(runtime) = onnxruntime {
                    options.onnxruntime = Some(runtime);
                }
                options.cuda_lib_dirs.extend(cuda_lib_dirs);
                if no_auto_cuda_libs {
                    options.auto_cuda_libs = false;
                }
                if let Some(value) = batch_size {
                    options.batch_size = value;
                }
                if let Some(value) = window {
                    options.window = value;
                }
                if let Some(value) = stride {
                    options.stride = value;
                }
                if let Some(value) = padding {
                    options.padding = value;
                }
                if let Some(value) = threshold {
                    options.threshold = value;
                }
                if let Some(value) = ffmpeg {
                    options.ffmpeg = value;
                }
                if let Some(value) = ffmpeg_threads {
                    options.ffmpeg_threads = value;
                }
                let report = run_step2(&options)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.success() {
                    anyhow::bail!("STEP2: {} vidéo(s) en échec", report.failures.len());
                }
            }
        },
        #[cfg(unix)]
        Command::Step3(args) => match args.command {
            Step3Command::Diarize {
                input_dir,
                force,
                device,
                yamnet,
                yamnet_fallback,
                ecapa,
                mel_matrix,
                tflite,
                edgetpu,
                onnxruntime,
                logs_dir,
            } => {
                let mut options = Step3Options::from_config(&config, input_dir)?;
                options.force = force;
                if let Some(value) = device {
                    options.device = workflow_steps::step3::Step3Device::parse(&value)?;
                }
                if let Some(path) = yamnet {
                    options.yamnet = path;
                }
                if let Some(path) = yamnet_fallback {
                    options.yamnet_fallback = path;
                }
                if let Some(path) = ecapa {
                    options.ecapa = path;
                }
                if let Some(path) = mel_matrix {
                    options.mel_matrix = path;
                }
                if let Some(path) = tflite {
                    options.tflite = path;
                }
                if let Some(path) = edgetpu {
                    options.edgetpu = path;
                }
                if let Some(path) = onnxruntime {
                    options.onnxruntime = Some(path);
                }
                if let Some(dir) = logs_dir {
                    options.logs_dir = Some(dir);
                }
                let report = run_step3(&options)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.success() {
                    anyhow::bail!("STEP3: {} vidéo(s) en échec", report.failures.len());
                }
            }
        },
        #[cfg(unix)]
        Command::Step4(args) => match args.command {
            Step4Command::Track {
                input_dir,
                device,
                no_object_detection,
                object_min_missing,
                face_reset_frames,
                decode_width,
                decode_height,
                logs_dir,
                tflite,
                edgetpu,
                allow_partial_speaking,
            } => {
                let mut options = Step4Options::from_config(&config, input_dir)?;
                options.allow_partial_speaking = allow_partial_speaking;
                if let Some(path) = tflite {
                    options.models.tflite = path;
                }
                if let Some(path) = edgetpu {
                    options.models.edgetpu = path;
                }
                if let Some(device) = device {
                    let device = workflow_steps::step4::engine::ExecutionDevice::parse(&device)?;
                    options.set_device(&config, device);
                }
                if no_object_detection {
                    options.object_detection = false;
                    options.models.object_detector = None;
                }
                if let Some(value) = object_min_missing {
                    options.object_min_missing = value;
                }
                if let Some(value) = face_reset_frames {
                    options.face_reset_frames = value;
                }
                if let Some(value) = decode_width {
                    options.decode_width = value;
                }
                if let Some(value) = decode_height {
                    options.decode_height = value;
                }
                if let Some(dir) = logs_dir {
                    options.logs_dir = Some(dir);
                }
                let report = run_step4(&options)?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.success() {
                    anyhow::bail!("STEP4: {} vidéo(s) en échec", report.failures.len());
                }
            }
        },
        Command::Step5(args) => match args.command {
            Step5Command::Project { work_dir, keyword } => {
                let report = run_step5_batch(&Step5BatchOptions {
                    work_dir,
                    keyword: keyword.unwrap_or_else(|| config.steps.filter.keyword.clone()),
                    docs_subfolder: config.steps.filter.docs_subfolder.clone(),
                    tracking_analytics: config.steps.step5.tracking_analytics,
                    expression_summary: config.steps.step5.expression_summary,
                    expression_keys: config.steps.step5.expression_keys.clone(),
                    skip_preserved_mov: config.steps.step1.skip_preserved_mov,
                    preserved_mov_names: if config.steps.step1.preserved_mov_names.is_empty() {
                        vec!["m6+.mov".into()]
                    } else {
                        config.steps.step1.preserved_mov_names.clone()
                    },
                    ffprobe: config.steps.step2.ffprobe.clone(),
                })?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.success() {
                    anyhow::bail!("STEP5: {} fichier(s) en échec", report.failures.len());
                }
            }
            Step5Command::Audio { input, output } => {
                let output = output.unwrap_or_else(|| input.clone());
                let meta = reduce_audio_file(&input, &output)
                    .with_context(|| format!("réduction audio de {}", input.display()))?;
                println!("{}", serde_json::to_string(&meta)?);
            }
            Step5Command::Video {
                input,
                output,
                audio,
                no_tracking_analytics,
                expression_summary,
                expression_keys,
            } => {
                let output = output.unwrap_or_else(|| default_video_output(&input));
                let audio = match audio {
                    Some(path) => Some(read_audio_meta(&path).with_context(|| {
                        format!("lecture des métadonnées audio {}", path.display())
                    })?),
                    None => None,
                };
                let options = VideoOptions {
                    tracking_analytics: config.steps.step5.tracking_analytics
                        && !no_tracking_analytics,
                    expression_summary: config.steps.step5.expression_summary || expression_summary,
                    expression_keys: expression_keys
                        .unwrap_or_else(|| config.steps.step5.expression_keys.clone()),
                    audio,
                };
                let meta = reduce_video_file(&input, &output, &options)
                    .with_context(|| format!("réduction vidéo de {}", input.display()))?;
                println!("{}", serde_json::to_string(&meta)?);
            }
        },
        Command::Step6(args) => match args.command {
            Step6Command::Project { work_dir, keyword } => {
                let report = run_step6_batch(&Step6BatchOptions {
                    work_dir,
                    keyword: keyword.unwrap_or_else(|| config.steps.filter.keyword.clone()),
                    docs_subfolder: config.steps.filter.docs_subfolder.clone(),
                    segment: SegmentConfig {
                        spread_threshold: f64::from(config.steps.step6.spread_threshold),
                        confidence_weight: config.steps.step6.confidence_weight,
                    },
                    skip_preserved_mov: config.steps.step1.skip_preserved_mov,
                    preserved_mov_names: if config.steps.step1.preserved_mov_names.is_empty() {
                        vec!["m6+.mov".into()]
                    } else {
                        config.steps.step1.preserved_mov_names.clone()
                    },
                    ffprobe: config.steps.step2.ffprobe.clone(),
                })?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.success() {
                    anyhow::bail!("STEP6: {} fichier(s) en échec", report.failures.len());
                }
            }
            Step6Command::Build {
                tracking,
                output,
                audio,
                csv,
            } => {
                let output = output.unwrap_or_else(|| default_ae_output(&tracking));
                let options = AeOptions {
                    audio,
                    csv,
                    config: SegmentConfig {
                        spread_threshold: f64::from(config.steps.step6.spread_threshold),
                        confidence_weight: config.steps.step6.confidence_weight,
                    },
                };
                let payload = build_ae_payload_file(&tracking, &output, &options)
                    .with_context(|| format!("pré-traitement AE de {}", tracking.display()))?;
                println!(
                    "{}",
                    serde_json::json!({
                        "output": output,
                        "written": payload.is_some()
                    })
                );
            }
        },
        Command::Step7(args) => match args.command {
            Step7Command::Finalize {
                work_dir,
                output_dir,
                archives_dir,
                logs_dir,
                mode,
                project,
                no_delete,
                restore_archives,
                dry_run,
            } => {
                let resolved = config.paths.resolve();
                let work_dir = work_dir.unwrap_or(resolved.projects);
                let output_dir = output_dir.unwrap_or(resolved.finalized);
                let archives_dir = archives_dir.unwrap_or(resolved.archives);
                let logs_dir = logs_dir.unwrap_or(resolved.logs);
                let mode_text = mode.unwrap_or_else(|| config.steps.step7.mode.clone());
                let mode = mode_text
                    .parse::<FinalizeMode>()
                    .map_err(|error| anyhow::anyhow!(error))?;

                if dry_run {
                    let projects = find_projects_to_finalize(&work_dir, mode, project.as_deref())
                        .with_context(|| format!("analyse de {}", work_dir.display()))?;
                    let names: Vec<String> = projects
                        .iter()
                        .filter_map(|path| path.file_name())
                        .map(|name| name.to_string_lossy().to_string())
                        .collect();
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "dry_run": true,
                            "work_dir": work_dir,
                            "output_dir": output_dir,
                            "archives_dir": archives_dir,
                            "mode": mode.as_str(),
                            "projects": names,
                        }))?
                    );
                    return Ok(());
                }

                let options = Step7Options {
                    work_dir,
                    output_dir,
                    archives_dir,
                    logs_dir,
                    fallback_output_dir: resolved.finalized_fallback,
                    mode,
                    restore_archives: restore_archives || config.steps.step7.restore_archives,
                    delete_source: !no_delete && config.steps.step7.delete_source,
                    project_filter: project,
                };
                let report = finalize_work_dir(&options)
                    .with_context(|| format!("finalisation de {}", options.work_dir.display()))?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.failures.is_empty() {
                    anyhow::bail!(
                        "{} projet(s) non finalisé(s): {}",
                        report.failures.len(),
                        report.failures.join(", ")
                    );
                }
            }
        },
        Command::Step8(args) => match args.command {
            Step8Command::Run {
                manifest,
                folders,
                cli,
                panel_jsx,
                config_json,
                logs_dir,
                timeout_s,
                dry_run,
            } => {
                let resolved = config.paths.resolve();
                let logs_dir = logs_dir.unwrap_or(resolved.logs);
                let cli = cli.unwrap_or_else(|| config.steps.step8.cli.clone());
                let panel_jsx = panel_jsx.unwrap_or_else(|| config.steps.step8.panel_jsx.clone());
                let configured_panel_config = &config.steps.step8.panel_config;
                let panel_config = config_json.or_else(|| {
                    (!configured_panel_config.as_os_str().is_empty())
                        .then(|| configured_panel_config.clone())
                });
                let cwd = std::env::current_dir().context("répertoire courant introuvable")?;
                let options = Step8Options {
                    manifest,
                    folders,
                    cli,
                    panel_jsx,
                    panel_config,
                    docs_subfolder: config.steps.step8.docs_subfolder.clone(),
                    logs_dir,
                    cwd,
                    dry_run,
                    max_drivers_kept: config.steps.step8.max_drivers_kept,
                    timeout_s: timeout_s.unwrap_or(config.steps.step8.timeout_s),
                };
                let outcome = run_step8(&options).context("exécution de STEP8")?;
                println!("{}", serde_json::to_string_pretty(&outcome)?);
                if !outcome.success {
                    std::process::exit(outcome.exit_code);
                }
            }
        },
        Command::Retention(args) => match args.command {
            RetentionCommand::Plan(overrides) => {
                let options = retention_options(&config, overrides, true);
                validate_retention_paths(&options, &config)?;
                let report = plan_retention(&options).context("calcul du plan de rétention")?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            RetentionCommand::Apply(overrides) => {
                let options = retention_options(&config, overrides, false);
                validate_retention_paths(&options, &config)?;
                let report = apply_retention(&options).context("application de la rétention")?;
                println!("{}", serde_json::to_string_pretty(&report)?);
                if !report.errors.is_empty() {
                    anyhow::bail!("{} suppression(s) en échec", report.errors.len());
                }
            }
        },
    }
    Ok(())
}
