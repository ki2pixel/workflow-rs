use anyhow::{Context, Result};
use clap::Parser;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::EnvFilter;
use workflow_core::{validate_isolation, Config};
use workflow_server::{AppState, Runner, WorkflowState};

#[derive(Debug, Parser)]
#[command(
    name = "workflow-server",
    version,
    about = "Serveur d'orchestration Rust"
)]
struct Cli {
    /// Fichier TOML. Les secrets sont surchargés par l'environnement.
    #[arg(long, global = true, default_value = "config/config.toml")]
    config: PathBuf,

    /// Valide la configuration, les chemins et la base sans ouvrir le port.
    #[arg(long)]
    check: bool,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    // Le lanceur redirige stderr vers un fichier : désactiver les séquences
    // ANSI dans ce cas, tout en gardant la couleur en exécution manuelle.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config)
        .with_context(|| format!("chargement de {}", cli.config.display()))?;
    config.validate_server()?;
    validate_isolation(&config)?;
    let resolved = config.paths.resolve();
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        config = %cli.config.display(),
        host = %config.server.host,
        port = config.server.port,
        base = %resolved.base.display(),
        projets = %resolved.projects.display(),
        logs = %resolved.logs.display(),
        telechargements = %resolved.downloads.display(),
        history_db = %resolved.history_db.display(),
        webhook_enabled = config.webhook.enabled,
        webhook_interval_s = config.webhook.interval_s,
        webhook_dry_run = config.webhook.dry_run,
        hardware_lock = %config.hardware.lock_path.display(),
        "configuration serveur chargée"
    );
    if config.server.allow_remote {
        tracing::warn!(
            host = config.server.host.as_str(),
            "server.allow_remote = true : le jeton worker et les routes de statut sont accessibles hors loopback"
        );
    }

    let state = Arc::new(WorkflowState::new());
    let runner = Runner::new(config.clone(), cli.config.clone(), state)
        .with_context(|| "initialisation de l'orchestrateur")?;
    runner.check_ready()?;
    let workflow_state = runner.state().clone();

    let history_count = runner.db().repository().count()?;
    workflow_ingest::validate_monitor_startup(&config, history_count)?;

    if cli.check {
        let resolved = config.paths.resolve();
        println!(
            "configuration server valide\n  port: {}\n  base: {}\n  db: {}\n  projets: {}\n  logs: {}\n  téléchargements: {}\n  historique: {} URL(s)\n  webhook: {}\n  dry-run: {}\n  verrou: {}\n  cli: {}",
            config.server.port,
            resolved.base.display(),
            resolved.history_db.display(),
            resolved.projects.display(),
            resolved.logs.display(),
            resolved.downloads.display(),
            history_count,
            if config.webhook.enabled {
                "activé"
            } else {
                "désactivé"
            },
            if config.webhook.dry_run { "oui" } else { "non" },
            config.hardware.lock_path.display(),
            runner.cli_path().display()
        );
        return Ok(());
    }

    tracing::info!(
        history_count,
        workflow_cli = %runner.cli_path().display(),
        "orchestrateur prêt"
    );
    let state = AppState::new(runner)?;
    let monitor = state.ingestion.spawn_monitor();
    let app = workflow_server::router(state);
    let address = listen_address(&config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind((config.server.host.as_str(), config.server.port))
        .await
        .with_context(|| format!("écoute impossible sur {address}"))?;
    tracing::info!("workflow-server à l'écoute sur http://{address}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(monitor, workflow_state))
        .await
        .context("serveur HTTP arrêté en erreur")
}

fn listen_address(host: &str, port: u16) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

#[cfg(unix)]
async fn shutdown_signal(monitor: workflow_ingest::MonitorHandle, state: Arc<WorkflowState>) {
    use tokio::signal::unix::{signal, SignalKind};
    let mut terminate = match signal(SignalKind::terminate()) {
        Ok(signal) => signal,
        Err(error) => {
            tracing::warn!("SIGTERM indisponible ({error}), attente de Ctrl+C");
            let _ = tokio::signal::ctrl_c().await;
            stop_runs_and_monitor(state, monitor).await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
    stop_runs_and_monitor(state, monitor).await;
}

#[cfg(not(unix))]
async fn shutdown_signal(monitor: workflow_ingest::MonitorHandle, state: Arc<WorkflowState>) {
    let _ = tokio::signal::ctrl_c().await;
    stop_runs_and_monitor(state, monitor).await;
}

async fn stop_runs_and_monitor(state: Arc<WorkflowState>, monitor: workflow_ingest::MonitorHandle) {
    tracing::info!("arrêt demandé");
    state.request_stop_all();
    if !state.wait_until_idle(Duration::from_secs(5)).await {
        tracing::warn!("arrêt gracieux dépassé : SIGKILL des groupes de processus");
        state.force_kill_all();
        if !state.wait_until_idle(Duration::from_secs(2)).await {
            tracing::error!("des sous-processus sont encore actifs après SIGKILL");
        }
    }
    monitor.stop().await;
}
