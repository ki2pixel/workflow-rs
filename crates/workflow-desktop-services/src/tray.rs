//! Politique du tray : état visuel, progression et menu.
//!
//! Ce module ne dépend d'aucun toolkit graphique. L'adaptateur Tauri
//! (`tauri::tray`) implémente [`TrayPresenter`] et reste dans l'hôte.

use crate::error::ServicesError;
use crate::models::StepStatus;
use crate::polling::{ConnectionState, StatusSnapshot, TerminalKind, TerminalOutcome};
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TrayState {
    Offline,
    Degraded,
    Idle,
    Running,
    Stopping,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrayProgress {
    pub current: u64,
    pub total: u64,
    pub percent: Option<u8>,
    pub text: String,
}

impl TrayProgress {
    pub fn from_step(step: &StepStatus) -> Self {
        let percent = step
            .progress_current_fractional
            .map(|value| (value * 100.0).round().clamp(0.0, 100.0) as u8)
            .or_else(|| {
                (step.progress_total > 0)
                    .then(|| ((step.progress_current * 100 / step.progress_total).min(100)) as u8)
            });
        Self {
            current: step.progress_current,
            total: step.progress_total,
            percent,
            text: if step.progress_text.is_empty() {
                percent
                    .map(|value| format!("{value} %"))
                    .unwrap_or_else(|| "En cours".to_string())
            } else {
                step.progress_text.clone()
            },
        }
    }

    pub fn from_counts(current: u64, total: u64) -> Self {
        let percent = (total > 0).then(|| ((current * 100 / total).min(100)) as u8);
        Self {
            current,
            total,
            percent,
            text: percent
                .map(|value| format!("{value} %"))
                .unwrap_or_else(|| "En cours".to_string()),
        }
    }

    pub fn label(&self) -> String {
        self.percent
            .map(|value| format!("{value} %"))
            .unwrap_or_else(|| self.text.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrayVisualKey {
    pub state: TrayState,
    pub headline: String,
    pub detail: String,
    pub percent: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrayStatus {
    pub state: TrayState,
    pub headline: String,
    pub detail: String,
    pub progress: Option<TrayProgress>,
    pub last_outcome: Option<String>,
    pub updated_at: DateTime<Utc>,
}

impl TrayStatus {
    pub fn from_snapshot(snapshot: &StatusSnapshot) -> Self {
        if snapshot.connection == ConnectionState::Unreachable {
            if snapshot.sequence.is_none() && snapshot.error.is_none() {
                return Self::degraded(snapshot, "Connexion…");
            }
            return Self::offline(snapshot);
        }

        if let Some(step) = &snapshot.active_step {
            let state = if step.is_stopping {
                TrayState::Stopping
            } else {
                TrayState::Running
            };
            let progress = TrayProgress::from_step(step);
            return Self {
                state,
                headline: format!("{} — {}", step.step, progress.label()),
                detail: step.display_name.clone(),
                progress: Some(progress),
                last_outcome: None,
                updated_at: snapshot.observed_at,
            };
        }

        if snapshot.connection == ConnectionState::Degraded {
            return Self::degraded(snapshot, "Statut d’étape indisponible");
        }

        // Une séquence active prime sur le dernier terminal : entre deux
        // étapes, `last_terminal` peut encore décrire la séquence précédente
        // et afficherait un faux « Succès » pendant que la nouvelle tourne.
        if let Some(sequence) = &snapshot.sequence {
            if sequence.is_running {
                let progress = TrayProgress::from_counts(
                    sequence.progress.current as u64,
                    sequence.progress.total as u64,
                );
                return Self {
                    state: TrayState::Running,
                    headline: format!("Séquence — {}", progress.label()),
                    detail: sequence
                        .current_step
                        .clone()
                        .unwrap_or_else(|| "Séquence en cours".to_string()),
                    progress: Some(progress),
                    last_outcome: None,
                    updated_at: snapshot.observed_at,
                };
            }
        }

        if let Some(terminal) = &snapshot.last_terminal {
            if let Some(status) = Self::from_terminal(terminal, snapshot.observed_at) {
                return status;
            }
        }

        Self::idle(snapshot.observed_at)
    }

    pub fn visual_key(&self) -> TrayVisualKey {
        TrayVisualKey {
            state: self.state,
            headline: self.headline.clone(),
            detail: self.detail.clone(),
            percent: self.progress.as_ref().and_then(|progress| progress.percent),
        }
    }

    pub fn icon_key(&self) -> TrayIconKey {
        match self.state {
            TrayState::Offline => TrayIconKey::Offline,
            TrayState::Degraded | TrayState::Stopping => TrayIconKey::Degraded,
            TrayState::Idle => TrayIconKey::Idle,
            TrayState::Running => TrayIconKey::Running,
            TrayState::Success => TrayIconKey::Success,
            TrayState::Error => TrayIconKey::Error,
        }
    }

    pub fn menu_lines(&self) -> (String, String, String) {
        (
            self.headline.clone(),
            self.detail.clone(),
            self.progress
                .as_ref()
                .map(TrayProgress::label)
                .unwrap_or_default(),
        )
    }

    fn offline(snapshot: &StatusSnapshot) -> Self {
        Self {
            state: TrayState::Offline,
            headline: "Serveur indisponible".to_string(),
            detail: snapshot
                .error
                .clone()
                .unwrap_or_else(|| "connexion perdue".to_string()),
            progress: None,
            last_outcome: None,
            updated_at: snapshot.observed_at,
        }
    }

    fn degraded(snapshot: &StatusSnapshot, fallback: &str) -> Self {
        Self {
            state: TrayState::Degraded,
            headline: "Serveur instable".to_string(),
            detail: snapshot
                .error
                .clone()
                .unwrap_or_else(|| fallback.to_string()),
            progress: None,
            last_outcome: None,
            updated_at: snapshot.observed_at,
        }
    }

    fn idle(updated_at: DateTime<Utc>) -> Self {
        Self {
            state: TrayState::Idle,
            headline: "Inactif".to_string(),
            detail: "Aucune étape en cours".to_string(),
            progress: None,
            last_outcome: None,
            updated_at,
        }
    }

    fn from_terminal(terminal: &TerminalOutcome, updated_at: DateTime<Utc>) -> Option<Self> {
        let (state, label) = match terminal.status.as_str() {
            "success" | "completed" => (TrayState::Success, "terminé"),
            "error" | "failed" => (TrayState::Error, "en échec"),
            "stopped" => (TrayState::Idle, "arrêté"),
            _ => return None,
        };
        let subject = match terminal.kind {
            TerminalKind::Step => terminal.step.clone().unwrap_or_else(|| "Étape".to_string()),
            TerminalKind::Sequence => "Séquence".to_string(),
        };
        Some(Self {
            state,
            headline: format!("{subject} {label}"),
            detail: terminal.message.clone().unwrap_or_else(|| subject.clone()),
            progress: None,
            last_outcome: Some(terminal.status.clone()),
            updated_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayIconKey {
    Offline,
    Degraded,
    Idle,
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ToggleWindow,
    OpenTelemetry,
    Quit,
}

pub trait TrayPresenter: Send + Sync {
    fn apply(&self, status: &TrayStatus) -> Result<(), ServicesError>;
}

#[derive(Debug, Default)]
pub struct NoopTray;

impl TrayPresenter for NoopTray {
    fn apply(&self, _status: &TrayStatus) -> Result<(), ServicesError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{SequenceOutcome, SequenceProgress, SequenceStatus};
    use crate::polling::StatusSnapshot;
    use chrono::Utc;

    fn step(status: &str, current: u64, total: u64, fractional: Option<f64>) -> StepStatus {
        StepStatus {
            step: "STEP2".to_string(),
            display_name: "2. Analyse des transitions".to_string(),
            status: status.to_string(),
            is_stopping: status == "stopping",
            return_code: None,
            progress_current: current,
            progress_total: total,
            progress_current_fractional: fractional,
            progress_text: format!("{current}/{total}"),
            duration_str: None,
            is_any_sequence_running: false,
            log: None,
        }
    }

    fn snapshot() -> StatusSnapshot {
        StatusSnapshot {
            connection: ConnectionState::Reachable,
            sequence: Some(SequenceStatus {
                is_running: false,
                current_step: None,
                progress: SequenceProgress {
                    current: 0,
                    total: 8,
                },
                last_outcome: SequenceOutcome {
                    status: "never_run".to_string(),
                    sequence_type: None,
                    message: None,
                    timestamp: None,
                },
            }),
            active_step: None,
            last_step: None,
            last_terminal: None,
            terminal_seq: 0,
            observed_at: Utc::now(),
            latency_ms: 1,
            consecutive_errors: 0,
            error: None,
        }
    }

    #[test]
    fn idle_snapshot_maps_to_idle() {
        let status = TrayStatus::from_snapshot(&snapshot());
        assert_eq!(status.state, TrayState::Idle);
        assert_eq!(status.headline, "Inactif");
    }

    #[test]
    fn running_step_maps_to_running_with_progress() {
        let mut snapshot = snapshot();
        snapshot.active_step = Some(step("running", 750, 1500, Some(0.5)));
        let status = TrayStatus::from_snapshot(&snapshot);
        assert_eq!(status.state, TrayState::Running);
        assert_eq!(status.progress.as_ref().unwrap().percent, Some(50));
        assert!(status.headline.contains("STEP2"));
    }

    #[test]
    fn stopping_step_maps_to_stopping() {
        let mut snapshot = snapshot();
        snapshot.active_step = Some(step("stopping", 10, 100, None));
        let status = TrayStatus::from_snapshot(&snapshot);
        assert_eq!(status.state, TrayState::Stopping);
        assert_eq!(status.icon_key(), TrayIconKey::Degraded);
    }

    #[test]
    fn unreachable_maps_to_offline() {
        let mut snapshot = snapshot();
        snapshot.connection = ConnectionState::Unreachable;
        snapshot.error = Some("connexion refusée".to_string());
        let status = TrayStatus::from_snapshot(&snapshot);
        assert_eq!(status.state, TrayState::Offline);
        assert_eq!(status.detail, "connexion refusée");
    }

    #[test]
    fn initial_snapshot_maps_to_degraded_connection() {
        let status = TrayStatus::from_snapshot(&StatusSnapshot::initial());
        assert_eq!(status.state, TrayState::Degraded);
        assert_eq!(status.headline, "Serveur instable");
    }

    #[test]
    fn sequence_success_maps_to_success() {
        let mut snapshot = snapshot();
        snapshot.last_terminal = Some(TerminalOutcome {
            kind: TerminalKind::Sequence,
            status: "success".to_string(),
            message: Some("Séquence terminée".to_string()),
            step: None,
        });
        let status = TrayStatus::from_snapshot(&snapshot);
        assert_eq!(status.state, TrayState::Success);
        assert!(status.headline.contains("Séquence"));
    }

    #[test]
    fn running_sequence_ignores_stale_terminal_between_steps() {
        let mut snapshot = snapshot();
        snapshot.sequence.as_mut().unwrap().is_running = true;
        snapshot.sequence.as_mut().unwrap().current_step = None;
        snapshot.last_terminal = Some(TerminalOutcome {
            kind: TerminalKind::Sequence,
            status: "success".to_string(),
            message: Some("Séquence précédente".to_string()),
            step: None,
        });
        let status = TrayStatus::from_snapshot(&snapshot);
        assert_eq!(status.state, TrayState::Running);
        assert!(status.headline.starts_with("Séquence"));
    }

    #[test]
    fn step_failure_maps_to_error() {
        let mut snapshot = snapshot();
        snapshot.last_terminal = Some(TerminalOutcome {
            kind: TerminalKind::Step,
            status: "failed".to_string(),
            message: Some("2. Analyse des transitions".to_string()),
            step: Some("STEP2".to_string()),
        });
        let status = TrayStatus::from_snapshot(&snapshot);
        assert_eq!(status.state, TrayState::Error);
        assert!(status.headline.contains("STEP2"));
    }

    #[test]
    fn visual_key_ignores_timestamp() {
        let mut first = snapshot();
        let mut second = snapshot();
        first.observed_at = Utc::now();
        second.observed_at = first.observed_at + chrono::Duration::seconds(10);
        let first = TrayStatus::from_snapshot(&first);
        let second = TrayStatus::from_snapshot(&second);
        assert_eq!(first.visual_key(), second.visual_key());
    }

    #[test]
    fn progress_falls_back_to_counts() {
        let progress = TrayProgress::from_counts(3, 8);
        assert_eq!(progress.percent, Some(37));
        assert_eq!(progress.label(), "37 %");
    }
}
