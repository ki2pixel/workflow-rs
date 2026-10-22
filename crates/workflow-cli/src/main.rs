use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use serde_json::json;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;
use workflow_core::Config;
use workflow_steps::step5::audio::{read_audio_meta, reduce_audio_file};
use workflow_steps::step5::video::{reduce_video_file, VideoOptions};
use workflow_steps::step6::{build_ae_payload_file, AeOptions, SegmentConfig};

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
    }
    Ok(())
}
