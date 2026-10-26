mod host;
mod poc_driver;
mod services;
mod single_instance;
mod tray;
mod x11_activation;

use anyhow::{Context, Result};
use clap::Parser;
use std::fs::{File, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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

    /// Dossier des artefacts de télémétrie et d'audit.
    #[arg(long, default_value_os_t = default_telemetry_dir())]
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

    /// Fichier de log hôte, avec rotation 8 Mio + 1 archive.
    #[arg(long)]
    pub log_file: Option<PathBuf>,

    /// Désactive la fermeture vers le tray (utile aux scripts J3).
    #[arg(long)]
    pub no_close_to_tray: bool,

    /// Démarre sans tray (contrôle ou environnement sans AppIndicator).
    #[arg(long)]
    pub no_tray: bool,

    /// Désactive le single-instance D-Bus (diagnostic).
    #[arg(long)]
    pub no_single_instance: bool,

    /// Active les notifications natives sur les terminaux (opt-in).
    #[arg(long)]
    pub notifications: bool,

    /// Active les sons locaux sur les terminaux (opt-in).
    #[arg(long)]
    pub audio: bool,

    /// Dossier des sons MP3 (utilisé seulement avec `--audio`).
    #[arg(long, default_value = "assets/web/sound-design")]
    pub sound_dir: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

/// Répertoire d'état conforme XDG, utilisable hors du dépôt et hors `/tmp`.
fn default_telemetry_dir() -> PathBuf {
    if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        PathBuf::from(state).join("workflow-manager")
    } else if let Some(home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        PathBuf::from(home).join(".local/state/workflow-manager")
    } else {
        PathBuf::from("/tmp/workflow-manager")
    }
}

fn run() -> Result<()> {
    let mut args = Args::parse();
    args.server_url.set_path("/");
    args.server_url.set_query(None);
    args.server_url.set_fragment(None);
    validate_base_url(&args.server_url).context("validation de l'URL serveur")?;

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .context("création du runtime Tokio")?,
    );

    // L'acquisition D-Bus doit précéder le bootstrap HTTP : un second
    // lancement doit pouvoir focaliser l'instance active même si le serveur
    // est momentanément indisponible.
    let (primary, single_instance_warning) = if args.no_single_instance {
        (None, None)
    } else {
        match runtime.block_on(single_instance::acquire(&args))? {
            single_instance::Acquire::Primary(primary) => (Some(primary), None),
            single_instance::Acquire::Secondary => return Ok(()),
            single_instance::Acquire::Unavailable(reason) => (None, Some(reason)),
        }
    };

    std::fs::create_dir_all(&args.telemetry_dir)
        .with_context(|| format!("création du dossier {}", args.telemetry_dir.display()))?;
    if let Some(ready) = &args.ready_file {
        if let Some(parent) = ready.parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    init_tracing(&args.log_level, args.log_file.as_deref());
    if let Some(reason) = single_instance_warning {
        tracing::warn!(%reason, "single-instance désactivé");
    }

    let services = services::AppServices::bootstrap(services::BootstrapOptions {
        runtime,
        server_url: args.server_url.clone(),
        out_dir: args.telemetry_dir.clone(),
        ready_file: args.ready_file.clone(),
        poc_driver: args.poc_driver.clone(),
        notifications: args.notifications,
        audio: args.audio,
        sound_dir: args.sound_dir.clone(),
    })?;

    host::run(args, services, primary)
}

fn init_tracing(level: &str, log_file: Option<&Path>) {
    let filter = tracing_subscriber::EnvFilter::try_new(level)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    if let Some(path) = log_file {
        let writer = RotatingWriter::new(path.to_path_buf(), 8 * 1024 * 1024, 1)
            .expect("initialisation du log hôte");
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .with_writer(writer)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_ansi(std::io::stderr().is_terminal())
            .init();
    }
}

#[derive(Clone)]
struct RotatingWriter {
    inner: Arc<Mutex<RotatingInner>>,
}

struct RotatingInner {
    path: PathBuf,
    file: File,
    size: u64,
    max_bytes: u64,
    backups: usize,
}

impl RotatingWriter {
    fn new(path: PathBuf, max_bytes: u64, backups: usize) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            inner: Arc::new(Mutex::new(RotatingInner {
                path,
                file,
                size,
                max_bytes: max_bytes.max(1),
                backups: backups.max(1),
            })),
        })
    }

    fn rotate(inner: &mut RotatingInner) -> std::io::Result<()> {
        inner.file.flush()?;
        let name = inner
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "nom de fichier de log invalide",
                )
            })?
            .to_string();
        for index in (1..inner.backups).rev() {
            let from = inner.path.with_file_name(format!("{name}.{index}"));
            let to = inner.path.with_file_name(format!("{name}.{}", index + 1));
            if from.exists() {
                std::fs::rename(from, to)?;
            }
        }
        let first = inner.path.with_file_name(format!("{name}.1"));
        std::fs::rename(&inner.path, &first)?;
        inner.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&inner.path)?;
        inner.size = 0;
        Ok(())
    }
}

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| std::io::Error::other("verrou de log"))?;
        if inner.size + buf.len() as u64 > inner.max_bytes {
            Self::rotate(&mut inner)?;
        }
        let written = inner.file.write(buf)?;
        inner.size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner
            .lock()
            .map_err(|_| std::io::Error::other("verrou de log"))?
            .file
            .flush()
    }
}

impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for RotatingWriter {
    type Writer = RotatingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
