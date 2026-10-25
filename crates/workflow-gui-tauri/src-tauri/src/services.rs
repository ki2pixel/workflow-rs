use crate::poc_driver;
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{WebviewWindow, Wry};
use tokio::runtime::Runtime;
use url::Url;
use workflow_desktop_services::polling::{PollingConfig, StatusWatcher, WatchEvent};
use workflow_desktop_services::{DesktopClient, Session};

pub struct AppServices {
    pub client: DesktopClient,
    pub session: Session,
    pub runtime: Arc<Runtime>,
    pub out_dir: PathBuf,
    pub ready_file: Option<PathBuf>,
    pub poc_driver: Option<PathBuf>,
    audit_started: AtomicBool,
}

impl AppServices {
    pub fn bootstrap(
        runtime: Arc<Runtime>,
        server_url: Url,
        out_dir: PathBuf,
        ready_file: Option<PathBuf>,
        poc_driver: Option<PathBuf>,
    ) -> Result<Self> {
        std::fs::create_dir_all(&out_dir)
            .with_context(|| format!("création du dossier {}", out_dir.display()))?;
        let client = DesktopClient::with_default_http(server_url)?;
        let session = runtime.block_on(client.bootstrap_session())?;
        let ping = runtime.block_on(client.ping())?;
        if ping.runtime != "rust" {
            anyhow::bail!("serveur non Rust: runtime={}", ping.runtime);
        }
        tracing::info!(server = %client.base_url(), "session initialisée");
        Ok(Self {
            client,
            session,
            runtime,
            out_dir,
            ready_file,
            poc_driver,
            audit_started: AtomicBool::new(false),
        })
    }

    pub fn start_background(&self) {
        let (watcher, mut receiver) =
            StatusWatcher::new(self.client.clone(), PollingConfig::default());
        self.runtime.spawn(async move {
            let handle = watcher.spawn();
            while let Ok(event) = receiver.recv().await {
                match event {
                    WatchEvent::Sequence(status) => tracing::debug!(
                        running = status.is_running,
                        step = ?status.current_step,
                        "état de séquence"
                    ),
                    WatchEvent::Error(error) => {
                        tracing::warn!(%error, "polling de séquence en erreur")
                    }
                    WatchEvent::Stopped => break,
                }
            }
            handle.abort();
        });
    }

    pub fn start_audit(&self, window: WebviewWindow<Wry>) {
        if self.audit_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let Some(path) = &self.poc_driver else {
            return;
        };
        let js = match std::fs::read_to_string(path) {
            Ok(js) => js,
            Err(error) => {
                tracing::error!(path = %path.display(), %error, "lecture du script d'audit impossible");
                return;
            }
        };
        let expected_sha = self.session.worker_token.sha256_hex();
        poc_driver::spawn(
            window,
            js,
            self.out_dir.clone(),
            self.ready_file.clone(),
            expected_sha,
        );
    }
}
