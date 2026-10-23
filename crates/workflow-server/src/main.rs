use anyhow::{Context, Result};
use clap::Parser;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing_subscriber::EnvFilter;
use workflow_core::Config;
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
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let config = Config::load(&cli.config)
        .with_context(|| format!("chargement de {}", cli.config.display()))?;
    config.validate_server()?;
    validate_isolation(&config)?;

    let state = Arc::new(WorkflowState::new());
    let runner = Runner::new(config.clone(), cli.config.clone(), state)
        .with_context(|| "initialisation de l'orchestrateur")?;
    runner.check_ready()?;

    if cli.check {
        let resolved = config.paths.resolve();
        println!(
            "configuration server valide\n  port: {}\n  base: {}\n  db: {}\n  projets: {}\n  logs: {}\n  verrou: {}\n  cli: {}",
            config.server.port,
            resolved.base.display(),
            resolved.history_db.display(),
            resolved.projects.display(),
            resolved.logs.display(),
            config.hardware.lock_path.display(),
            runner.cli_path().display()
        );
        return Ok(());
    }

    let state = AppState::new(runner);
    let app = workflow_server::router(state);
    let address = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&address)
        .await
        .with_context(|| format!("écoute impossible sur {address}"))?;
    tracing::info!("workflow-server à l'écoute sur http://{address}");
    println!("workflow-server à l'écoute sur http://{address}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("serveur HTTP arrêté en erreur")
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("arrêt demandé");
}

fn validate_isolation(config: &Config) -> Result<()> {
    let resolved = config.paths.resolve();
    let base = std::fs::canonicalize(&resolved.base).or_else(|_| {
        std::fs::create_dir_all(&resolved.base)
            .map_err(anyhow::Error::from)
            .and_then(|_| std::fs::canonicalize(&resolved.base).map_err(anyhow::Error::from))
    })?;
    let db_parent = resolved
        .history_db
        .parent()
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(db_parent)?;
    let db = std::fs::canonicalize(&resolved.history_db).unwrap_or_else(|_| {
        std::fs::canonicalize(db_parent)
            .unwrap_or_else(|_| db_parent.to_path_buf())
            .join(
                resolved
                    .history_db
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("history.sqlite3")),
            )
    });
    if !db.starts_with(&base) {
        anyhow::bail!(
            "isolation refusée: paths.history_db ({}) doit être sous paths.base ({})",
            db.display(),
            base.display()
        );
    }
    for (label, path) in [
        ("paths.projects", &resolved.projects),
        ("paths.archives", &resolved.archives),
        ("paths.logs", &resolved.logs),
        ("paths.downloads", &resolved.downloads),
    ] {
        std::fs::create_dir_all(path)?;
        let canonical = std::fs::canonicalize(path)?;
        if !canonical.starts_with(&base) {
            anyhow::bail!(
                "isolation refusée: {label} ({}) doit être sous paths.base ({})",
                canonical.display(),
                base.display()
            );
        }
    }
    if resolved
        .projects
        .starts_with(Path::new("/home/kidpixel/workflow_mediapipe"))
    {
        anyhow::bail!(
            "isolation refusée: paths.projects pointe vers le dépôt Python legacy: {}",
            resolved.projects.display()
        );
    }
    if config.hardware.lock_path == Path::new("/tmp/workflow-hw.lock") {
        anyhow::bail!("isolation refusée: hardware.lock_path doit être distinct du verrou Python");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_config(root: &Path) -> Config {
        let mut config = Config::default();
        config.paths.base = root.join("runtime");
        config.paths.history_db = Some(config.paths.base.join("history.sqlite3"));
        config.paths.projects = Some(config.paths.base.join("projets_extraits"));
        config.hardware.lock_path = root.join("hardware.lock");
        config
    }

    #[test]
    fn isolation_accepts_a_dedicated_runtime() {
        let dir = tempfile::tempdir().unwrap();
        validate_isolation(&isolated_config(dir.path())).unwrap();
    }

    #[test]
    fn isolation_rejects_database_outside_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = isolated_config(dir.path());
        config.paths.history_db = Some(dir.path().join("elsewhere.sqlite3"));
        assert!(validate_isolation(&config).is_err());
    }

    #[test]
    fn isolation_rejects_a_project_root_outside_runtime() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = isolated_config(dir.path());
        config.paths.projects = Some(dir.path().join("projects"));
        assert!(validate_isolation(&config).is_err());
    }

    #[test]
    fn isolation_rejects_the_python_hardware_lock() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = isolated_config(dir.path());
        config.hardware.lock_path = PathBuf::from("/tmp/workflow-hw.lock");
        assert!(validate_isolation(&config).is_err());
    }
}
