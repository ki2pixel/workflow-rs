use crate::poc_driver;
use anyhow::{Context, Result};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{WebviewWindow, Wry};
use tokio::runtime::Runtime;
use url::Url;
use workflow_desktop_services::polling::{
    ConnectionState, PollingConfig, StatusSnapshot, StatusWatcher, TerminalKind, TerminalOutcome,
    WatcherStats, WatcherStatsSnapshot,
};
use workflow_desktop_services::tray::{TrayPresenter, TrayStatus, TrayVisualKey};
use workflow_desktop_services::{
    AudioPlayer, DesktopClient, NoopAudio, NoopNotifier, Notifier, NotifyRustNotifier, RodioAudio,
    Session, SoundEvent, TransitionGate,
};

pub struct BootstrapOptions {
    pub runtime: Arc<Runtime>,
    pub server_url: Url,
    pub out_dir: PathBuf,
    pub ready_file: Option<PathBuf>,
    pub poc_driver: Option<PathBuf>,
    pub notifications: bool,
    pub audio: bool,
    pub sound_dir: PathBuf,
}

pub struct AppServices {
    pub client: DesktopClient,
    pub session: Session,
    pub runtime: Arc<Runtime>,
    pub out_dir: PathBuf,
    pub ready_file: Option<PathBuf>,
    pub poc_driver: Option<PathBuf>,
    notifier: Arc<dyn Notifier>,
    player: Arc<dyn AudioPlayer>,
    audit_started: AtomicBool,
}

#[derive(Serialize)]
struct TrayStateFile<'a> {
    timestamp: String,
    status: &'a TrayStatus,
    connection: ConnectionState,
    current_step: Option<String>,
    consecutive_errors: u32,
    latency_ms: u64,
    watcher: WatcherStatsSnapshot,
}

#[derive(Serialize)]
struct WatcherStatsFile {
    timestamp: String,
    watcher: WatcherStatsSnapshot,
}

impl AppServices {
    pub fn bootstrap(options: BootstrapOptions) -> Result<Self> {
        let BootstrapOptions {
            runtime,
            server_url,
            out_dir,
            ready_file,
            poc_driver,
            notifications,
            audio,
            sound_dir,
        } = options;
        std::fs::create_dir_all(&out_dir)
            .with_context(|| format!("création du dossier {}", out_dir.display()))?;
        let client = DesktopClient::with_default_http(server_url)?;
        let session = runtime.block_on(client.bootstrap_session())?;
        let ping = runtime.block_on(client.ping())?;
        if ping.runtime != "rust" {
            anyhow::bail!("serveur non Rust: runtime={}", ping.runtime);
        }
        let notifier: Arc<dyn Notifier> = if notifications {
            Arc::new(NotifyRustNotifier)
        } else {
            Arc::new(NoopNotifier)
        };
        let player: Arc<dyn AudioPlayer> = if audio {
            Arc::new(RodioAudio::new(sound_dir))
        } else {
            Arc::new(NoopAudio)
        };
        tracing::info!(
            server = %client.base_url(),
            notifications,
            audio,
            "session initialisée"
        );
        Ok(Self {
            client,
            session,
            runtime,
            out_dir,
            ready_file,
            poc_driver,
            notifier,
            player,
            audit_started: AtomicBool::new(false),
        })
    }

    /// Démarre le watcher et applique ses snapshots au tray.
    ///
    /// Aucun appel à la WebView n'est effectué : le watcher parle uniquement
    /// HTTP au serveur, puis le présentateur met à jour l'icône native.
    pub fn start_background(&self, tray: Arc<dyn TrayPresenter>) {
        let (watcher, mut receiver) =
            StatusWatcher::new(self.client.clone(), PollingConfig::tray());
        let stats = watcher.stats();
        let out_dir = self.out_dir.clone();
        let notifier = self.notifier.clone();
        let player = self.player.clone();
        self.runtime.spawn(async move {
            let handle = watcher.spawn();
            let mut last_visual: Option<TrayVisualKey> = None;
            let mut last_stats = Instant::now();
            let mut last_state = Instant::now();
            let mut gate = TransitionGate::default();
            write_watcher_stats(&out_dir, &stats);
            while receiver.changed().await.is_ok() {
                let snapshot = receiver.borrow_and_update().clone();
                let status = TrayStatus::from_snapshot(&snapshot);
                let visual = status.visual_key();

                if let Some(notification) =
                    gate.observe(snapshot.last_terminal.as_ref(), snapshot.terminal_seq)
                {
                    let notifier = notifier.clone();
                    let player = player.clone();
                    let event = audio_event(snapshot.last_terminal.as_ref());
                    tokio::task::spawn_blocking(move || {
                        if let Err(error) = notifier.notify(&notification) {
                            tracing::warn!(%error, "notification impossible");
                        }
                        if let Some(event) = event {
                            if let Err(error) = player.play(event) {
                                tracing::warn!(%error, "lecture audio impossible");
                            }
                        }
                    });
                }

                if last_visual.as_ref() != Some(&visual) {
                    if let Err(error) = tray.apply(&status) {
                        tracing::warn!(%error, "mise à jour du tray impossible");
                    }
                    write_tray_state(&out_dir, &status, &snapshot, &stats);
                    tracing::info!(
                        state = ?status.state,
                        headline = %status.headline,
                        "état du tray"
                    );
                    last_visual = Some(visual);
                    last_state = Instant::now();
                }
                // Les compteurs sont rafraîchis toutes les 5 s pour que le
                // moniteur d'endurance lise un total proche de la fin.
                if last_stats.elapsed() >= Duration::from_secs(5) {
                    write_watcher_stats(&out_dir, &stats);
                    last_stats = Instant::now();
                }
                if last_state.elapsed() >= Duration::from_secs(60) {
                    write_tray_state(&out_dir, &status, &snapshot, &stats);
                    last_state = Instant::now();
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

fn write_tray_state(
    out_dir: &Path,
    status: &TrayStatus,
    snapshot: &StatusSnapshot,
    stats: &Arc<WatcherStats>,
) {
    let payload = TrayStateFile {
        timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        status,
        connection: snapshot.connection,
        current_step: snapshot
            .sequence
            .as_ref()
            .and_then(|sequence| sequence.current_step.clone()),
        consecutive_errors: snapshot.consecutive_errors,
        latency_ms: snapshot.latency_ms,
        watcher: stats.snapshot(),
    };
    write_json_atomic(&out_dir.join("tray-state.json"), &payload);
}

fn write_watcher_stats(out_dir: &Path, stats: &Arc<WatcherStats>) {
    let payload = WatcherStatsFile {
        timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        watcher: stats.snapshot(),
    };
    write_json_atomic(&out_dir.join("watcher-stats.json"), &payload);
}

fn audio_event(terminal: Option<&TerminalOutcome>) -> Option<SoundEvent> {
    let terminal = terminal?;
    match terminal.status.as_str() {
        "success" | "completed" => Some(match terminal.kind {
            TerminalKind::Step => SoundEvent::StepSuccess,
            TerminalKind::Sequence => SoundEvent::WorkflowCompletion,
        }),
        "error" | "failed" => Some(SoundEvent::ErrorEvent),
        _ => None,
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let tmp = parent.join(format!(".{name}.tmp"));
    let Ok(bytes) = serde_json::to_vec_pretty(value) else {
        return;
    };
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}
