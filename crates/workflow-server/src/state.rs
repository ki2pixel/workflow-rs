//! État thread-safe de l'orchestrateur.
//!
//! Le contrat JSON reprend celui de `WorkflowState` côté Python : statuts,
//! progression, journal borné et résultat de séquence.
//!
//! Chaque exécution porte une génération (`RunId`). Une finalisation tardive
//! ne peut donc plus écraser l'état d'une exécution plus récente, et le statut
//! `stopping` conserve le créneau jusqu'à la mort effective du sous-processus.

use crate::catalog::StepKey;
use crate::error::WorkflowServerError;
use chrono::{SecondsFormat, Utc};
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const MAX_STEP_LOG_LINES: usize = 300;
/// Une ligne de journal plus longue est tronquée : 300 lignes ne doivent pas
/// pouvoir immobiliser plusieurs dizaines de Mio.
pub const MAX_STEP_LOG_LINE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(u64);

impl RunId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SequenceId(u64);

impl SequenceId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPhase {
    Idle,
    Starting,
    Running,
    Stopping,
    Initiated,
    Completed,
    Failed,
    Stopped,
}

impl StepPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Stopping => "stopping",
            Self::Initiated => "initiated",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::Stopping | Self::Initiated
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Stopped)
    }
}

#[derive(Debug)]
pub struct RunControl {
    run_id: RunId,
    stop: AtomicBool,
    pid: AtomicI32,
}

impl RunControl {
    pub fn new(run_id: RunId) -> Self {
        Self {
            run_id,
            stop: AtomicBool::new(false),
            pid: AtomicI32::new(0),
        }
    }

    pub fn run_id(&self) -> RunId {
        self.run_id
    }

    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }

    pub fn is_stop_requested(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    pub fn set_pid(&self, pid: u32) {
        self.pid.store(pid as i32, Ordering::SeqCst);
    }

    pub fn pid(&self) -> Option<u32> {
        let pid = self.pid.load(Ordering::SeqCst);
        (pid > 0).then_some(pid as u32)
    }

    pub fn clear_pid(&self) {
        self.pid.store(0, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SequenceOutcome {
    pub status: String,
    #[serde(rename = "type")]
    pub sequence_type: Option<String>,
    pub message: Option<String>,
    pub timestamp: Option<String>,
}

impl Default for SequenceOutcome {
    fn default() -> Self {
        Self {
            status: "never_run".into(),
            sequence_type: None,
            message: None,
            timestamp: None,
        }
    }
}

#[derive(Debug)]
struct StepRecord {
    phase: StepPhase,
    run_id: Option<RunId>,
    return_code: Option<i32>,
    progress_current: u64,
    progress_total: u64,
    progress_current_fractional: Option<f64>,
    progress_text: String,
    duration_str: Option<String>,
    log: VecDeque<String>,
    control: Option<Arc<RunControl>>,
}

impl StepRecord {
    fn idle() -> Self {
        Self {
            phase: StepPhase::Idle,
            run_id: None,
            return_code: None,
            progress_current: 0,
            progress_total: 0,
            progress_current_fractional: None,
            progress_text: String::new(),
            duration_str: None,
            log: VecDeque::new(),
            control: None,
        }
    }

    fn starting(run_id: RunId, control: Arc<RunControl>) -> Self {
        Self {
            phase: StepPhase::Starting,
            run_id: Some(run_id),
            return_code: None,
            progress_current: 0,
            progress_total: 0,
            progress_current_fractional: None,
            progress_text: "En attente...".into(),
            duration_str: None,
            log: VecDeque::new(),
            control: Some(control),
        }
    }
}

#[derive(Debug)]
struct Inner {
    steps: HashMap<StepKey, StepRecord>,
    sequence_running: bool,
    sequence_stop: bool,
    sequence_id: Option<SequenceId>,
    sequence_steps: Vec<StepKey>,
    sequence_completed: Vec<StepKey>,
    sequence_outcome: SequenceOutcome,
    next_run_id: u64,
    next_sequence_id: u64,
}

pub struct WorkflowState {
    inner: RwLock<Inner>,
}

impl Default for WorkflowState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkflowState {
    pub fn new() -> Self {
        let steps = StepKey::ALL
            .into_iter()
            .map(|step| (step, StepRecord::idle()))
            .collect();
        Self {
            inner: RwLock::new(Inner {
                steps,
                sequence_running: false,
                sequence_stop: false,
                sequence_id: None,
                sequence_steps: Vec::new(),
                sequence_completed: Vec::new(),
                sequence_outcome: SequenceOutcome::default(),
                next_run_id: 0,
                next_sequence_id: 0,
            }),
        }
    }

    fn allocate_run_id(inner: &mut Inner) -> RunId {
        inner.next_run_id = inner.next_run_id.saturating_add(1);
        RunId(inner.next_run_id)
    }

    pub fn reserve_step(&self, step: StepKey) -> Result<Arc<RunControl>, WorkflowServerError> {
        let mut inner = self.inner.write();
        if inner.sequence_running {
            return Err(WorkflowServerError::SequenceRunning);
        }
        if inner.steps.values().any(|record| record.phase.is_active()) {
            return Err(WorkflowServerError::StepRunning);
        }
        let run_id = Self::allocate_run_id(&mut inner);
        let control = Arc::new(RunControl::new(run_id));
        let record = inner
            .steps
            .get_mut(&step)
            .ok_or(WorkflowServerError::UnknownStep)?;
        *record = StepRecord::starting(run_id, control.clone());
        Ok(control)
    }

    pub fn start_sequence(
        &self,
        steps: Vec<StepKey>,
        sequence_type: &str,
    ) -> Result<SequenceId, WorkflowServerError> {
        let mut inner = self.inner.write();
        if inner.sequence_running {
            return Err(WorkflowServerError::AnotherSequenceRunning);
        }
        if inner.steps.values().any(|record| record.phase.is_active()) {
            return Err(WorkflowServerError::StepRunningInSequence);
        }
        inner.next_sequence_id = inner.next_sequence_id.saturating_add(1);
        let sequence_id = SequenceId(inner.next_sequence_id);
        inner.sequence_running = true;
        inner.sequence_stop = false;
        inner.sequence_id = Some(sequence_id);
        inner.sequence_steps = steps;
        inner.sequence_completed.clear();
        inner.sequence_outcome = SequenceOutcome {
            status: format!("running_{}", sequence_type.to_ascii_lowercase()),
            sequence_type: Some(sequence_type.into()),
            message: None,
            timestamp: Some(now_iso()),
        };
        Ok(sequence_id)
    }

    pub fn begin_sequence_step(
        &self,
        step: StepKey,
    ) -> Result<Arc<RunControl>, WorkflowServerError> {
        let mut inner = self.inner.write();
        if !inner.sequence_running || inner.sequence_stop {
            return Err(WorkflowServerError::NoSequenceRunning);
        }
        if inner.steps.values().any(|record| record.phase.is_active()) {
            return Err(WorkflowServerError::StepRunningInSequence);
        }
        let run_id = Self::allocate_run_id(&mut inner);
        let control = Arc::new(RunControl::new(run_id));
        let record = inner
            .steps
            .get_mut(&step)
            .ok_or(WorkflowServerError::UnknownStep)?;
        *record = StepRecord::starting(run_id, control.clone());
        Ok(control)
    }

    pub fn mark_running(&self, step: StepKey, run_id: RunId) {
        let mut inner = self.inner.write();
        if let Some(record) = inner.steps.get_mut(&step) {
            if record.run_id == Some(run_id) && record.phase == StepPhase::Starting {
                record.phase = StepPhase::Running;
            }
        }
    }

    pub fn append_log(&self, step: StepKey, line: impl Into<String>) {
        let mut inner = self.inner.write();
        if let Some(record) = inner.steps.get_mut(&step) {
            let mut line = line.into();
            if line.len() > MAX_STEP_LOG_LINE_BYTES {
                let mut boundary = MAX_STEP_LOG_LINE_BYTES;
                while !line.is_char_boundary(boundary) {
                    boundary -= 1;
                }
                line.truncate(boundary);
                line.push_str(" …[tronqué]");
            }
            record.log.push_back(line);
            while record.log.len() > MAX_STEP_LOG_LINES {
                record.log.pop_front();
            }
        }
    }

    pub fn set_progress_total(&self, step: StepKey, total: u64) {
        let mut inner = self.inner.write();
        if let Some(record) = inner.steps.get_mut(&step) {
            record.progress_total = total;
        }
    }

    pub fn progress_total(&self, step: StepKey) -> u64 {
        self.inner
            .read()
            .steps
            .get(&step)
            .map(|record| record.progress_total)
            .unwrap_or(0)
    }

    pub fn set_progress(
        &self,
        step: StepKey,
        current: u64,
        total: Option<u64>,
        text: Option<String>,
        fractional: Option<Option<f64>>,
    ) {
        let mut inner = self.inner.write();
        if let Some(record) = inner.steps.get_mut(&step) {
            record.progress_current = current;
            if let Some(total) = total {
                record.progress_total = total;
            }
            if let Some(text) = text {
                record.progress_text = text;
            }
            if let Some(fractional) = fractional {
                record.progress_current_fractional = fractional;
            }
        }
    }

    pub fn set_progress_text(&self, step: StepKey, text: String) {
        let mut inner = self.inner.write();
        if let Some(record) = inner.steps.get_mut(&step) {
            record.progress_text = text;
        }
    }

    pub fn progress_current_fractional(&self, step: StepKey) -> Option<f64> {
        self.inner
            .read()
            .steps
            .get(&step)
            .and_then(|record| record.progress_current_fractional)
    }

    /// Finalise une exécution. Une génération périmée est ignorée sans muter
    /// l'état courant. Retourne `true` si la finalisation a été appliquée.
    pub fn finish_step(
        &self,
        step: StepKey,
        run_id: RunId,
        phase: StepPhase,
        return_code: Option<i32>,
        duration: String,
    ) -> bool {
        debug_assert!(phase.is_terminal(), "phase de fin non terminale");
        let mut inner = self.inner.write();
        let sequence_active = inner.sequence_running;
        let Some(record) = inner.steps.get_mut(&step) else {
            return false;
        };
        if record.run_id != Some(run_id) {
            tracing::warn!(
                step = step.as_str(),
                run_id = run_id.get(),
                "finalisation périmée ignorée"
            );
            return false;
        }
        record.phase = phase;
        record.return_code = return_code;
        record.duration_str = Some(duration);
        record.control = None;
        if phase == StepPhase::Completed {
            if record.progress_total > 0 && record.progress_current < record.progress_total {
                record.progress_current = record.progress_total;
            }
            if record.progress_text.is_empty() || record.progress_text == "En attente..." {
                record.progress_text = "Terminé".into();
            }
        }
        if sequence_active
            && inner.sequence_steps.contains(&step)
            && !inner.sequence_completed.contains(&step)
        {
            inner.sequence_completed.push(step);
        }
        true
    }

    pub fn set_duration(&self, step: StepKey, duration: String) {
        let mut inner = self.inner.write();
        if let Some(record) = inner.steps.get_mut(&step) {
            record.duration_str = Some(duration);
        }
    }

    pub fn stop_step(&self, step: StepKey) -> Result<(), WorkflowServerError> {
        let pid = {
            let mut inner = self.inner.write();
            let Some(record) = inner.steps.get_mut(&step) else {
                return Err(WorkflowServerError::UnknownStep);
            };
            if !record.phase.is_active() {
                return Err(WorkflowServerError::StepNotRunning {
                    step: step.as_str().into(),
                });
            }
            if record.phase == StepPhase::Stopping {
                return Ok(());
            }
            if let Some(control) = &record.control {
                control.request_stop();
            }
            let pid = record.control.as_ref().and_then(|control| control.pid());
            record.phase = StepPhase::Stopping;
            record.return_code = Some(-1);
            record
                .log
                .push_back("\n--- Arrêt demandé par l'utilisateur ---\n".into());
            pid
        };
        if let Some(pid) = pid {
            terminate_pid(pid);
        }
        Ok(())
    }

    pub fn stop_sequence(&self) -> Result<(), WorkflowServerError> {
        let running: Vec<(StepKey, Option<u32>)> = {
            let mut inner = self.inner.write();
            if !inner.sequence_running {
                return Err(WorkflowServerError::NoSequenceRunning);
            }
            inner.sequence_stop = true;
            inner.sequence_outcome = SequenceOutcome {
                status: "error".into(),
                sequence_type: inner.sequence_outcome.sequence_type.clone(),
                message: Some("Sequence manually stopped by user".into()),
                timestamp: Some(now_iso()),
            };
            inner
                .steps
                .iter_mut()
                .filter(|(_, record)| record.phase.is_active())
                .map(|(step, record)| {
                    if let Some(control) = &record.control {
                        control.request_stop();
                    }
                    record.phase = StepPhase::Stopping;
                    record.return_code = Some(-1);
                    record
                        .log
                        .push_back("\n--- Séquence arrêtée par l'utilisateur ---\n".into());
                    (
                        *step,
                        record.control.as_ref().and_then(|control| control.pid()),
                    )
                })
                .collect()
        };
        for (_, pid) in running {
            if let Some(pid) = pid {
                terminate_pid(pid);
            }
        }
        Ok(())
    }

    /// Arrêt demandé par le serveur : conserve les créneaux jusqu'à la fin
    /// réelle des sous-processus.
    pub fn request_stop_all(&self) {
        let running: Vec<Option<u32>> = {
            let mut inner = self.inner.write();
            if inner.sequence_running {
                inner.sequence_stop = true;
                inner.sequence_outcome = SequenceOutcome {
                    status: "error".into(),
                    sequence_type: inner.sequence_outcome.sequence_type.clone(),
                    message: Some("Server shutdown requested".into()),
                    timestamp: Some(now_iso()),
                };
            }
            inner
                .steps
                .iter_mut()
                .filter(|(_, record)| record.phase.is_active())
                .map(|(_, record)| {
                    if let Some(control) = &record.control {
                        control.request_stop();
                    }
                    let pid = record.control.as_ref().and_then(|control| control.pid());
                    if record.phase != StepPhase::Stopping {
                        record.phase = StepPhase::Stopping;
                        record.return_code = Some(-1);
                    }
                    pid
                })
                .collect()
        };
        for pid in running.into_iter().flatten() {
            terminate_pid(pid);
        }
    }

    pub fn force_kill_all(&self) {
        let pids: Vec<u32> = {
            let inner = self.inner.read();
            inner
                .steps
                .values()
                .filter_map(|record| record.control.as_ref().and_then(|control| control.pid()))
                .collect()
        };
        for pid in pids {
            terminate_pid_force(pid);
        }
    }

    pub async fn wait_until_idle(&self, timeout: Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while tokio::time::Instant::now() < deadline {
            if !self.is_any_step_running() {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        !self.is_any_step_running()
    }

    pub fn sequence_stop_requested(&self) -> bool {
        self.inner.read().sequence_stop
    }

    pub fn complete_sequence(
        &self,
        sequence_id: SequenceId,
        success: bool,
        message: String,
        sequence_type: &str,
    ) {
        let mut inner = self.inner.write();
        if inner.sequence_id != Some(sequence_id) {
            tracing::warn!(
                sequence_id = sequence_id.get(),
                "finalisation de séquence périmée ignorée"
            );
            return;
        }
        if !inner.sequence_running {
            return;
        }
        inner.sequence_running = false;
        // Un arrêt manuel ou serveur a déjà publié son résultat ; le worker ne
        // doit pas l'écraser.
        if inner.sequence_stop {
            return;
        }
        inner.sequence_outcome = SequenceOutcome {
            status: if success { "success" } else { "error" }.into(),
            sequence_type: Some(sequence_type.into()),
            message: Some(message),
            timestamp: Some(now_iso()),
        };
    }

    pub fn sequence_running(&self) -> bool {
        self.inner.read().sequence_running
    }

    pub fn sequence_outcome(&self) -> SequenceOutcome {
        self.inner.read().sequence_outcome.clone()
    }

    pub fn is_any_step_running(&self) -> bool {
        self.inner
            .read()
            .steps
            .values()
            .any(|record| record.phase.is_active())
    }

    pub fn step_status(&self, step: StepKey, include_logs: bool) -> Option<StepStatus> {
        let inner = self.inner.read();
        let record = inner.steps.get(&step)?;
        Some(StepStatus {
            step: step.as_str().into(),
            display_name: step.display_name().into(),
            status: record.phase.as_str().into(),
            is_stopping: record.phase == StepPhase::Stopping,
            return_code: record.return_code,
            progress_current: record.progress_current,
            progress_total: record.progress_total,
            progress_current_fractional: record.progress_current_fractional,
            progress_text: record.progress_text.clone(),
            duration_str: record.duration_str.clone(),
            is_any_sequence_running: inner.sequence_running,
            log: include_logs.then(|| record.log.iter().cloned().collect()),
        })
    }

    pub fn sequence_status(&self) -> SequenceStatus {
        let inner = self.inner.read();
        let current_step = inner
            .steps
            .iter()
            .find(|(_, record)| record.phase.is_active())
            .map(|(step, _)| step.as_str().to_string());
        let (completed, total) = if inner.sequence_steps.is_empty() {
            (
                inner
                    .steps
                    .values()
                    .filter(|record| record.phase == StepPhase::Completed)
                    .count(),
                StepKey::ALL.len(),
            )
        } else {
            (inner.sequence_completed.len(), inner.sequence_steps.len())
        };
        SequenceStatus {
            is_running: inner.sequence_running && !inner.sequence_stop,
            current_step,
            progress: SequenceProgress {
                current: completed,
                total,
            },
            last_outcome: inner.sequence_outcome.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StepStatus {
    pub step: String,
    pub display_name: String,
    pub status: String,
    /// Vrai pendant la fenêtre SIGTERM/SIGKILL qui suit un arrêt demandé.
    pub is_stopping: bool,
    pub return_code: Option<i32>,
    pub progress_current: u64,
    pub progress_total: u64,
    pub progress_current_fractional: Option<f64>,
    pub progress_text: String,
    pub duration_str: Option<String>,
    pub is_any_sequence_running: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SequenceStatus {
    pub is_running: bool,
    pub current_step: Option<String>,
    pub progress: SequenceProgress,
    pub last_outcome: SequenceOutcome,
}

#[derive(Debug, Clone, Serialize)]
pub struct SequenceProgress {
    pub current: usize,
    pub total: usize,
}

pub fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn now_epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

pub fn calculate_duration(start_time_epoch: Option<f64>) -> String {
    let Some(start) = start_time_epoch else {
        return "N/A".into();
    };
    let elapsed = (now_epoch() - start).max(0.0);
    if elapsed < 60.0 {
        format!("{}s", elapsed as u64)
    } else if elapsed < 3_600.0 {
        format!("{}m {}s", (elapsed / 60.0) as u64, (elapsed % 60.0) as u64)
    } else {
        format!(
            "{}h {}m",
            (elapsed / 3_600.0) as u64,
            ((elapsed % 3_600.0) / 60.0) as u64
        )
    }
}

pub fn terminate_pid(pid: u32) {
    signal_process(pid, libc::SIGTERM);
}

pub fn terminate_pid_force(pid: u32) {
    signal_process(pid, libc::SIGKILL);
}

#[cfg(unix)]
fn signal_process(pid: u32, signal: libc::c_int) {
    let pid = pid as libc::pid_t;
    // Les sous-processus sont lancés avec process_group(0) : le PID est aussi
    // le PGID. Le repli couvre les appels historiques sans groupe dédié.
    let grouped = unsafe { libc::kill(-pid, signal) };
    if grouped != 0 {
        unsafe {
            libc::kill(pid, signal);
        }
    }
}

#[cfg(not(unix))]
fn signal_process(_pid: u32, _signal: i32) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::WorkflowServerError;

    #[test]
    fn given_running_step_when_reserved_again_then_conflict() {
        let state = WorkflowState::new();
        let _control = state.reserve_step(StepKey::Step5).unwrap();
        assert!(matches!(
            state.reserve_step(StepKey::Step6),
            Err(WorkflowServerError::StepRunning)
        ));
    }

    #[test]
    fn given_stopped_step_when_finished_then_slot_is_released() {
        let state = WorkflowState::new();
        let control = state.reserve_step(StepKey::Step5).unwrap();
        state.stop_step(StepKey::Step5).unwrap();
        assert!(state.reserve_step(StepKey::Step6).is_err());
        state.finish_step(
            StepKey::Step5,
            control.run_id(),
            StepPhase::Stopped,
            Some(-1),
            "0s".into(),
        );
        assert!(state.reserve_step(StepKey::Step6).is_ok());
    }

    #[test]
    fn given_stale_finish_when_new_generation_then_state_unchanged() {
        let state = WorkflowState::new();
        let first = state.reserve_step(StepKey::Step5).unwrap();
        state.finish_step(
            StepKey::Step5,
            first.run_id(),
            StepPhase::Completed,
            Some(0),
            "1s".into(),
        );
        let second = state.reserve_step(StepKey::Step5).unwrap();
        let second_id = second.run_id();
        let applied = state.finish_step(
            StepKey::Step5,
            first.run_id(),
            StepPhase::Failed,
            Some(9),
            "9s".into(),
        );
        assert!(!applied);
        let status = state.step_status(StepKey::Step5, false).unwrap();
        assert_eq!(status.status, "starting");
        assert_eq!(second.run_id(), second_id);
        assert!(state.reserve_step(StepKey::Step6).is_err());
    }

    #[test]
    fn given_sequence_stopped_when_new_sequence_then_conflict() {
        let state = WorkflowState::new();
        state
            .start_sequence(vec![StepKey::Step5, StepKey::Step6], "Custom")
            .unwrap();
        state.stop_sequence().unwrap();
        assert!(!state.sequence_status().is_running);
        assert_eq!(
            state.start_sequence(vec![StepKey::Step7], "Custom"),
            Err(WorkflowServerError::AnotherSequenceRunning)
        );
        assert!(matches!(
            state.reserve_step(StepKey::Step7),
            Err(WorkflowServerError::SequenceRunning)
        ));
    }

    #[test]
    fn given_manual_sequence_stop_when_worker_completes_then_outcome_preserved() {
        let state = WorkflowState::new();
        let sequence_id = state
            .start_sequence(vec![StepKey::Step5], "Custom")
            .unwrap();
        state.stop_sequence().unwrap();
        state.complete_sequence(sequence_id, true, "ne doit pas écraser".into(), "Custom");
        assert!(!state.sequence_running());
        assert_eq!(state.sequence_outcome().status, "error");
        assert_eq!(
            state.sequence_outcome().message.as_deref(),
            Some("Sequence manually stopped by user")
        );
    }

    #[test]
    fn given_completed_step_when_finished_then_waiting_text_replaced() {
        let state = WorkflowState::new();
        let control = state.reserve_step(StepKey::Step5).unwrap();
        state.finish_step(
            StepKey::Step5,
            control.run_id(),
            StepPhase::Completed,
            Some(0),
            "1s".into(),
        );
        let status = state.step_status(StepKey::Step5, false).unwrap();
        assert_eq!(status.progress_text, "Terminé");
    }

    #[test]
    fn given_three_step_sequence_when_two_complete_then_progress_is_three() {
        let state = WorkflowState::new();
        let sequence_id = state
            .start_sequence(
                vec![StepKey::Step5, StepKey::Step6, StepKey::Step7],
                "Custom",
            )
            .unwrap();
        let step5 = state.begin_sequence_step(StepKey::Step5).unwrap();
        state.finish_step(
            StepKey::Step5,
            step5.run_id(),
            StepPhase::Completed,
            Some(0),
            "1s".into(),
        );
        let status = state.sequence_status();
        assert_eq!(status.progress.total, 3);
        assert_eq!(status.progress.current, 1);
        state.complete_sequence(sequence_id, true, "ok".into(), "Custom");
        assert!(!state.sequence_status().is_running);
    }

    #[cfg(unix)]
    #[test]
    fn given_process_group_ignoring_sigterm_when_forced_then_killed() {
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};
        use std::time::Instant;

        let mut command = Command::new("sh");
        command
            .args(["-c", "trap '' TERM; while true; do sleep 1; done"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        command.process_group(0);
        let mut child = command.spawn().unwrap();
        let pid = child.id();
        std::thread::sleep(Duration::from_millis(100));
        terminate_pid(pid);
        std::thread::sleep(Duration::from_millis(100));
        assert!(child.try_wait().unwrap().is_none());
        terminate_pid_force(pid);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        panic!("le groupe de processus a survécu à SIGKILL");
    }

    #[test]
    fn log_is_bounded() {
        let state = WorkflowState::new();
        for index in 0..(MAX_STEP_LOG_LINES + 20) {
            state.append_log(StepKey::Step2, format!("{index}\n"));
        }
        let status = state.step_status(StepKey::Step2, true).unwrap();
        assert_eq!(status.log.unwrap().len(), MAX_STEP_LOG_LINES);
    }
}
