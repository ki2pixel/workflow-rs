mod host;
mod poc_driver;
mod services;

use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use url::Url;
use workflow_desktop_services::validate_base_url;

#[derive(Debug, Parser)]
#[command(
    name = "workflow-gui-tauri",
    version,
    about = "Client desktop Linux de workflow-server (A-Strict)"
)]
pub(crate) struct Args {
    /// URL loopback du serveur Rust.
    #[arg(long, default_value = "http://127.0.0.1:5004")]
    pub server_url: Url,

    /// Dossier des artefacts de télémétrie et d'audit du PoC.
    #[arg(long, default_value = "/tmp/opencode/poc-gui")]
    pub telemetry_dir: PathBuf,

    /// Écrit l'URL d'audit dès que le hash `#poc=` est capturé.
    #[arg(long)]
    pub ready_file: Option<PathBuf>,

    /// Script JS d'audit A-Strict (PoC uniquement).
    #[arg(long)]
    pub poc_driver: Option<PathBuf>,

    /// Filtre de logs `tracing` (ex: info, debug).
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = Args::parse();
    args.server_url.set_path("/");
    args.server_url.set_query(None);
    args.server_url.set_fragment(None);
    validate_base_url(&args.server_url).context("validation de l'URL serveur")?;

    std::fs::create_dir_all(&args.telemetry_dir)
        .with_context(|| format!("création du dossier {}", args.telemetry_dir.display()))?;
    if let Some(ready) = &args.ready_file {
        if let Some(parent) = ready.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    init_tracing(&args.log_level);

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("création du runtime Tokio")?,
    );

    let services = services::AppServices::bootstrap(
        runtime,
        args.server_url.clone(),
        args.telemetry_dir.clone(),
        args.ready_file.clone(),
        args.poc_driver.clone(),
    )?;
    services.start_background();

    host::run(args, services)
}

fn init_tracing(level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_new(level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
}
