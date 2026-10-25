//! Surveillance périodique de l'état de séquence.
//!
//! Le watcher ne connaît ni le tray ni la WebView : il produit des
//! `StatusSnapshot` et les publie dans un canal `watch` à un seul slot. Le
//! réducteur du tray et l'adaptateur visuel vivent au-dessus.

use crate::client::DesktopClient;
use crate::models::{SequenceStatus, StepKey, StepStatus};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ConnectionState {
    Reachable,
    Degraded,
    Unreachable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TerminalKind {
    Step,
    Sequence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TerminalOutcome {
    pub kind: TerminalKind,
    pub status: String,
    pub message: Option<String>,
    pub step: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    pub connection: ConnectionState,
    pub sequence: Option<SequenceStatus>,
    pub active_step: Option<StepStatus>,
    pub last_step: Option<StepStatus>,
    pub last_terminal: Option<TerminalOutcome>,
    /// Numéro monotone du dernier terminal observé (notification unique).
    pub terminal_seq: u64,
    pub observed_at: DateTime<Utc>,
    pub latency_ms: u64,
    pub consecutive_errors: u32,
    pub error: Option<String>,
}

impl StatusSnapshot {
    pub fn initial() -> Self {
        Self {
            connection: ConnectionState::Degraded,
            sequence: None,
            active_step: None,
            last_step: None,
            last_terminal: None,
            terminal_seq: 0,
            observed_at: Utc::now(),
            latency_ms: 0,
            consecutive_errors: 0,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PollingConfig {
    pub interval: Duration,
    pub max_errors: Option<u32>,
    pub request_timeout: Duration,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            max_errors: Some(3),
            request_timeout: Duration::from_millis(800),
        }
    }
}

impl PollingConfig {
    /// Le tray ne s'arrête jamais sur erreur : il affiche Offline et continue.
    pub fn tray() -> Self {
        Self {
            max_errors: None,
            ..Self::default()
        }
    }
}

#[derive(Debug, Default)]
pub struct WatcherStats {
    pub ticks: AtomicU64,
    pub successes: AtomicU64,
    pub errors: AtomicU64,
    pub skipped: AtomicU64,
    pub last_latency_ms: AtomicU64,
    pub last_success_unix_ms: AtomicU64,
    pub consecutive_errors: AtomicU32,
}

#[derive(Debug, Clone, Serialize)]
pub struct WatcherStatsSnapshot {
    pub ticks: u64,
    pub successes: u64,
    pub errors: u64,
    pub skipped: u64,
    pub last_latency_ms: u64,
    pub last_success_unix_ms: u64,
    pub consecutive_errors: u32,
}

impl WatcherStats {
    pub fn snapshot(&self) -> WatcherStatsSnapshot {
        WatcherStatsSnapshot {
            ticks: self.ticks.load(Ordering::Relaxed),
            successes: self.successes.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            last_latency_ms: self.last_latency_ms.load(Ordering::Relaxed),
            last_success_unix_ms: self.last_success_unix_ms.load(Ordering::Relaxed),
            consecutive_errors: self.consecutive_errors.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Default)]
struct WatcherState {
    consecutive_errors: u32,
    last_step_key: Option<StepKey>,
    last_step: Option<StepStatus>,
    last_terminal: Option<TerminalOutcome>,
    terminal_seq: u64,
    previous_outcome: Option<(String, Option<String>)>,
    first_poll: bool,
    was_unreachable: bool,
}

pub struct StatusWatcher {
    client: DesktopClient,
    config: PollingConfig,
    cancel: CancellationToken,
    tx: watch::Sender<StatusSnapshot>,
    stats: Arc<WatcherStats>,
}

impl StatusWatcher {
    pub fn new(
        client: DesktopClient,
        config: PollingConfig,
    ) -> (Self, watch::Receiver<StatusSnapshot>) {
        let (tx, rx) = watch::channel(StatusSnapshot::initial());
        (
            Self {
                client,
                config,
                cancel: CancellationToken::new(),
                tx,
                stats: Arc::new(WatcherStats::default()),
            },
            rx,
        )
    }

    pub fn stats(&self) -> Arc<WatcherStats> {
        self.stats.clone()
    }

    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    async fn run(self) {
        let mut state = WatcherState {
            first_poll: true,
            ..WatcherState::default()
        };
        let mut next = tokio::time::Instant::now();
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => break,
                _ = tokio::time::sleep_until(next) => {
                    let snapshot = self.poll_once(&mut state).await;
                    let stop = self
                        .config
                        .max_errors
                        .map(|max| snapshot.consecutive_errors >= max)
                        .unwrap_or(false);
                    self.stats.ticks.fetch_add(1, Ordering::Relaxed);
                    // Sans récepteur, le watcher n'a plus de raison de
                    // interroger le serveur : on s'arrête au lieu de tourner
                    // indéfiniment (le tray a disparu ou a paniqué).
                    if self.tx.send(snapshot).is_err() {
                        break;
                    }
                    if stop {
                        break;
                    }
                    next += self.config.interval;
                    let now = tokio::time::Instant::now();
                    if next < now {
                        self.stats.skipped.fetch_add(1, Ordering::Relaxed);
                        next = now + self.config.interval;
                    }
                }
            }
        }
    }

    async fn poll_once(&self, state: &mut WatcherState) -> StatusSnapshot {
        let started = Instant::now();
        let sequence =
            match tokio::time::timeout(self.config.request_timeout, self.client.sequence_status())
                .await
            {
                Ok(Ok(sequence)) => sequence,
                Ok(Err(error)) => {
                    return self.error_snapshot(state, error.to_string(), started);
                }
                Err(_) => {
                    return self.error_snapshot(
                        state,
                        "timeout /sequence/status".to_string(),
                        started,
                    );
                }
            };

        if state.was_unreachable {
            state.last_step = None;
            state.last_step_key = None;
            state.last_terminal = None;
            state.previous_outcome = None;
            state.first_poll = true;
            state.was_unreachable = false;
        }
        state.consecutive_errors = 0;
        self.stats.successes.fetch_add(1, Ordering::Relaxed);
        self.stats.consecutive_errors.store(0, Ordering::Relaxed);
        self.stats.last_success_unix_ms.store(
            Utc::now().timestamp_millis().max(0) as u64,
            Ordering::Relaxed,
        );

        let active_key = sequence.current_step.as_deref().and_then(StepKey::parse);
        let mut active_step: Option<StepStatus> = None;
        let mut step_terminal: Option<TerminalOutcome> = None;
        let mut connection = ConnectionState::Reachable;

        if let Some(key) = active_key {
            state.last_step_key = Some(key);
            match tokio::time::timeout(
                self.config.request_timeout,
                self.client.step_status(key, false),
            )
            .await
            {
                Ok(Ok(step)) => {
                    if is_terminal(&step.status) {
                        step_terminal = Some(terminal_from_step(&step));
                        state.last_step_key = None;
                    } else {
                        active_step = Some(step.clone());
                    }
                    state.last_step = Some(step);
                }
                Ok(Err(_)) | Err(_) => {
                    connection = ConnectionState::Degraded;
                }
            }
        } else if let Some(key) = state.last_step_key.take() {
            // Dernier poll après la disparition de l'étape active : on lit son
            // état terminal une seule fois.
            match tokio::time::timeout(
                self.config.request_timeout,
                self.client.step_status(key, false),
            )
            .await
            {
                Ok(Ok(step)) => {
                    if is_terminal(&step.status) {
                        step_terminal = Some(terminal_from_step(&step));
                    }
                    state.last_step = Some(step);
                }
                Ok(Err(_)) | Err(_) => {
                    connection = ConnectionState::Degraded;
                }
            }
        }

        let outcome_key = (
            sequence.last_outcome.status.clone(),
            sequence.last_outcome.timestamp.clone(),
        );
        let outcome_changed = if state.first_poll {
            // Le premier poll mémorise le dernier résultat sans le traiter
            // comme un nouvel événement : un GUI fraîchement lancé ne doit
            // pas notifier une séquence terminée avant son démarrage.
            state.first_poll = false;
            state.previous_outcome = Some(outcome_key);
            false
        } else if state.previous_outcome.as_ref() != Some(&outcome_key) {
            state.previous_outcome = Some(outcome_key);
            true
        } else {
            false
        };
        let sequence_terminal = if outcome_changed {
            matches!(sequence.last_outcome.status.as_str(), "success" | "error").then(|| {
                TerminalOutcome {
                    kind: TerminalKind::Sequence,
                    status: sequence.last_outcome.status.clone(),
                    message: sequence.last_outcome.message.clone(),
                    step: None,
                }
            })
        } else {
            None
        };

        if let Some(terminal) = sequence_terminal {
            state.last_terminal = Some(terminal);
            state.terminal_seq = state.terminal_seq.saturating_add(1);
        } else if let Some(terminal) = step_terminal {
            state.last_terminal = Some(terminal);
            state.terminal_seq = state.terminal_seq.saturating_add(1);
        }

        let latency_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.stats
            .last_latency_ms
            .store(latency_ms, Ordering::Relaxed);

        StatusSnapshot {
            connection,
            sequence: Some(sequence),
            active_step,
            last_step: state.last_step.clone(),
            last_terminal: state.last_terminal.clone(),
            terminal_seq: state.terminal_seq,
            observed_at: Utc::now(),
            latency_ms,
            consecutive_errors: 0,
            error: None,
        }
    }

    fn error_snapshot(
        &self,
        state: &mut WatcherState,
        error: String,
        started: Instant,
    ) -> StatusSnapshot {
        state.consecutive_errors = state.consecutive_errors.saturating_add(1);
        self.stats.errors.fetch_add(1, Ordering::Relaxed);
        self.stats
            .consecutive_errors
            .store(state.consecutive_errors, Ordering::Relaxed);
        let connection = if state.consecutive_errors >= 3 {
            state.was_unreachable = true;
            ConnectionState::Unreachable
        } else {
            ConnectionState::Degraded
        };
        let latency_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        self.stats
            .last_latency_ms
            .store(latency_ms, Ordering::Relaxed);
        StatusSnapshot {
            connection,
            sequence: None,
            active_step: None,
            last_step: state.last_step.clone(),
            last_terminal: state.last_terminal.clone(),
            terminal_seq: state.terminal_seq,
            observed_at: Utc::now(),
            latency_ms,
            consecutive_errors: state.consecutive_errors,
            error: Some(error),
        }
    }
}

fn is_terminal(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "stopped")
}

fn terminal_from_step(step: &StepStatus) -> TerminalOutcome {
    TerminalOutcome {
        kind: TerminalKind::Step,
        status: step.status.clone(),
        message: Some(step.display_name.clone()),
        step: Some(step.step.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_polling_config_is_one_second_and_three_errors() {
        let config = PollingConfig::default();
        assert_eq!(config.interval, Duration::from_secs(1));
        assert_eq!(config.max_errors, Some(3));
        assert_eq!(config.request_timeout, Duration::from_millis(800));
    }

    #[test]
    fn tray_config_never_stops_on_error() {
        let config = PollingConfig::tray();
        assert_eq!(config.max_errors, None);
        assert_eq!(config.interval, Duration::from_secs(1));
    }

    #[test]
    fn initial_snapshot_is_degraded_without_error() {
        let snapshot = StatusSnapshot::initial();
        assert_eq!(snapshot.connection, ConnectionState::Degraded);
        assert!(snapshot.error.is_none());
        assert!(snapshot.sequence.is_none());
        assert_eq!(snapshot.terminal_seq, 0);
    }

    #[test]
    fn terminal_statuses_are_exhaustive() {
        assert!(is_terminal("completed"));
        assert!(is_terminal("failed"));
        assert!(is_terminal("stopped"));
        assert!(!is_terminal("running"));
        assert!(!is_terminal("idle"));
    }

    #[test]
    fn stats_snapshot_reflects_counters() {
        let stats = WatcherStats::default();
        stats.ticks.store(42, Ordering::Relaxed);
        stats.errors.store(2, Ordering::Relaxed);
        stats.last_latency_ms.store(7, Ordering::Relaxed);
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.ticks, 42);
        assert_eq!(snapshot.errors, 2);
        assert_eq!(snapshot.last_latency_ms, 7);
    }

    #[tokio::test]
    async fn watcher_stops_when_receiver_is_dropped() {
        let client = DesktopClient::with_default_http(
            url::Url::parse("http://127.0.0.1:1/").expect("URL loopback"),
        )
        .expect("client");
        let config = PollingConfig {
            interval: Duration::from_millis(20),
            max_errors: None,
            request_timeout: Duration::from_millis(20),
        };
        let (watcher, receiver) = StatusWatcher::new(client, config);
        let stats = watcher.stats();
        let handle = watcher.spawn();
        drop(receiver);

        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("le watcher doit s'arrêter sans récepteur")
            .expect("tâche watcher");
        assert!(stats.ticks.load(Ordering::Relaxed) >= 1);
    }
}
