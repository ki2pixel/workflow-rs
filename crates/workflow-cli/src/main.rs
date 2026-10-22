use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::path::{Path, PathBuf};
use tracing_subscriber::EnvFilter;
use workflow_core::Config;
use workflow_steps::retention::{apply_retention, plan_retention, RetentionOptions};
use workflow_steps::step5::audio::{read_audio_meta, reduce_audio_file};
use workflow_steps::step5::video::{reduce_video_file, VideoOptions};
use workflow_steps::step6::{build_ae_payload_file, AeOptions, SegmentConfig};
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
struct Step5Args {
    #[command(subcommand)]
    command: Step5Command,
}

#[derive(Debug, Subcommand)]
enum Step5Command {
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
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
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
        Command::Step5(args) => match args.command {
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
