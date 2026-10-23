//! État thread-safe de l'orchestrateur.
//!
//! Le contrat JSON reprend celui de `WorkflowState` côté Python : statuts,
//! progression, journal borné et résultat de séquence.

use crate::catalog::StepKey;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_STEP_LOG_LINES: usize = 300;

#[derive(Debug, Default)]
pub struct RunControl {
    stop: AtomicBool,
    pid: AtomicI32,
}

impl RunControl {
    pub fn new() -> Self {
        Self::default()
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
    status: String,
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
            status: "idle".into(),
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
}

#[derive(Debug)]
struct Inner {
    steps: HashMap<StepKey, StepRecord>,
    sequence_running: bool,
    sequence_stop: bool,
    sequence_outcome: SequenceOutcome,
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
                sequence_outcome: SequenceOutcome::default(),
            }),
        }
    }

    pub fn reserve_step(&self, step: StepKey) -> Result<Arc<RunControl>, String> {
        let mut inner = self.inner.write().expect("état verrouillé");
        if inner.sequence_running {
            return Err("Une séquence de workflow est en cours. Veuillez attendre.".into());
        }
        if inner.steps.values().any(|record| is_active(&record.status)) {
            return Err("Une autre étape est déjà en cours.".into());
        }
        let control = Arc::new(RunControl::new());
        let record = inner.steps.get_mut(&step).expect("étape initialisée");
        *record = StepRecord {
            status: "starting".into(),
            return_code: None,
            progress_current: 0,
            progress_total: 0,
            progress_current_fractional: None,
            progress_text: "En attente...".into(),
            duration_str: None,
            log: VecDeque::new(),
            control: Some(control.clone()),
        };
        Ok(control)
    }

    pub fn start_sequence(&self, sequence_type: &str) -> Result<(), String> {
        let mut inner = self.inner.write().expect("état verrouillé");
        if inner.sequence_running {
            return Err("Une autre séquence de workflow est déjà en cours.".into());
        }
        if inner.steps.values().any(|record| is_active(&record.status)) {
            return Err("Une étape est déjà en cours.".into());
        }
        inner.sequence_running = true;
        inner.sequence_stop = false;
        inner.sequence_outcome = SequenceOutcome {
            status: format!("running_{}", sequence_type.to_ascii_lowercase()),
            sequence_type: Some(sequence_type.into()),
            message: None,
            timestamp: Some(now_iso()),
        };
        Ok(())
    }

    pub fn begin_sequence_step(&self, step: StepKey) -> Result<Arc<RunControl>, String> {
        let mut inner = self.inner.write().expect("état verrouillé");
        if !inner.sequence_running {
            return Err("Aucune séquence en cours.".into());
        }
        if inner.steps.values().any(|record| is_active(&record.status)) {
            return Err("Une étape est déjà en cours.".into());
        }
        let control = Arc::new(RunControl::new());
        let record = inner.steps.get_mut(&step).expect("étape initialisée");
        *record = StepRecord {
            status: "starting".into(),
            return_code: None,
            progress_current: 0,
            progress_total: 0,
            progress_current_fractional: None,
            progress_text: "En attente...".into(),
            duration_str: None,
            log: VecDeque::new(),
            control: Some(control.clone()),
        };
        Ok(control)
    }

    pub fn mark_running(&self, step: StepKey) {
        let mut inner = self.inner.write().expect("état verrouillé");
        if let Some(record) = inner.steps.get_mut(&step) {
            record.status = "running".into();
        }
    }

    pub fn append_log(&self, step: StepKey, line: impl Into<String>) {
        let mut inner = self.inner.write().expect("état verrouillé");
        if let Some(record) = inner.steps.get_mut(&step) {
            record.log.push_back(line.into());
            while record.log.len() > MAX_STEP_LOG_LINES {
                record.log.pop_front();
            }
        }
    }

    pub fn set_progress_total(&self, step: StepKey, total: u64) {
        let mut inner = self.inner.write().expect("état verrouillé");
        if let Some(record) = inner.steps.get_mut(&step) {
            record.progress_total = total;
        }
    }

    pub fn progress_total(&self, step: StepKey) -> u64 {
        self.inner
            .read()
            .expect("état verrouillé")
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
        let mut inner = self.inner.write().expect("état verrouillé");
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
        let mut inner = self.inner.write().expect("état verrouillé");
        if let Some(record) = inner.steps.get_mut(&step) {
            record.progress_text = text;
        }
    }

    pub fn progress_current_fractional(&self, step: StepKey) -> Option<f64> {
        self.inner
            .read()
            .expect("état verrouillé")
            .steps
            .get(&step)
            .and_then(|record| record.progress_current_fractional)
    }

    pub fn finish_step(
        &self,
        step: StepKey,
        status: &str,
        return_code: Option<i32>,
        duration: String,
    ) {
        let mut inner = self.inner.write().expect("état verrouillé");
        if let Some(record) = inner.steps.get_mut(&step) {
            record.status = status.into();
            record.return_code = return_code;
            record.duration_str = Some(duration);
            record.control = None;
            if status == "completed" {
                if record.progress_total > 0 && record.progress_current < record.progress_total {
                    record.progress_current = record.progress_total;
                }
                if record.progress_text.is_empty() || record.progress_text == "En attente..." {
                    record.progress_text = "Terminé".into();
                }
            }
        }
    }

    pub fn set_duration(&self, step: StepKey, duration: String) {
        let mut inner = self.inner.write().expect("état verrouillé");
        if let Some(record) = inner.steps.get_mut(&step) {
            record.duration_str = Some(duration);
        }
    }

    pub fn stop_step(&self, step: StepKey) -> Result<(), String> {
        let pid = {
            let mut inner = self.inner.write().expect("état verrouillé");
            let Some(record) = inner.steps.get_mut(&step) else {
                return Err("Step introuvable".into());
            };
            if !is_active(&record.status) {
                return Err("L'étape n'est pas en cours.".into());
            }
            let pid = if let Some(control) = &record.control {
                control.request_stop();
                control.pid()
            } else {
                None
            };
            record.status = "stopped".into();
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

    pub fn stop_sequence(&self) -> Result<(), String> {
        let mut inner = self.inner.write().expect("état verrouillé");
        if !inner.sequence_running {
            return Err("Aucune séquence en cours d'exécution".into());
        }
        inner.sequence_stop = true;
        inner.sequence_running = false;
        inner.sequence_outcome = SequenceOutcome {
            status: "error".into(),
            sequence_type: inner.sequence_outcome.sequence_type.clone(),
            message: Some("Sequence manually stopped by user".into()),
            timestamp: Some(now_iso()),
        };
        let running: Vec<(StepKey, Option<u32>)> = inner
            .steps
            .iter_mut()
            .filter(|(_, record)| is_active(&record.status))
            .map(|(step, record)| {
                if let Some(control) = &record.control {
                    control.request_stop();
                }
                record.status = "stopped".into();
                record.return_code = Some(-1);
                record
                    .log
                    .push_back("\n--- Séquence arrêtée par l'utilisateur ---\n".into());
                (
                    *step,
                    record.control.as_ref().and_then(|control| control.pid()),
                )
            })
            .collect();
        drop(inner);
        for (_, pid) in running {
            if let Some(pid) = pid {
                terminate_pid(pid);
            }
        }
        Ok(())
    }

    pub fn sequence_stop_requested(&self) -> bool {
        self.inner.read().expect("état verrouillé").sequence_stop
    }

    pub fn complete_sequence(&self, success: bool, message: String, sequence_type: &str) {
        let mut inner = self.inner.write().expect("état verrouillé");
        // Un arrêt manuel a déjà publié son résultat ; le worker ne doit pas
        // l'écraser avec un statut générique.
        if inner.sequence_stop || !inner.sequence_running {
            return;
        }
        inner.sequence_running = false;
        inner.sequence_outcome = SequenceOutcome {
            status: if success { "success" } else { "error" }.into(),
            sequence_type: Some(sequence_type.into()),
            message: Some(message),
            timestamp: Some(now_iso()),
        };
    }

    pub fn sequence_running(&self) -> bool {
        self.inner.read().expect("état verrouillé").sequence_running
    }

    pub fn sequence_outcome(&self) -> SequenceOutcome {
        self.inner
            .read()
            .expect("état verrouillé")
            .sequence_outcome
            .clone()
    }

    pub fn is_any_step_running(&self) -> bool {
        self.inner
            .read()
            .expect("état verrouillé")
            .steps
            .values()
            .any(|record| is_active(&record.status))
    }

    pub fn step_status(&self, step: StepKey, include_logs: bool) -> Option<StepStatus> {
        let inner = self.inner.read().expect("état verrouillé");
        let record = inner.steps.get(&step)?;
        Some(StepStatus {
            step: step.as_str().into(),
            display_name: step.display_name().into(),
            status: record.status.clone(),
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
        let inner = self.inner.read().expect("état verrouillé");
        let current_step = inner
            .steps
            .iter()
            .find(|(_, record)| is_active(&record.status))
            .map(|(step, _)| step.as_str().to_string());
        let completed = inner
            .steps
            .values()
            .filter(|record| record.status == "completed")
            .count();
        SequenceStatus {
            is_running: inner.sequence_running,
            current_step,
            progress: SequenceProgress {
                current: completed,
                total: StepKey::ALL.len(),
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

fn is_active(status: &str) -> bool {
    matches!(status, "starting" | "running" | "initiated")
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
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_step_can_be_reserved() {
        let state = WorkflowState::new();
        let control = state.reserve_step(StepKey::Step5).unwrap();
        assert!(state.reserve_step(StepKey::Step6).is_err());
        control.request_stop();
        state.finish_step(StepKey::Step5, "stopped", Some(-1), "0s".into());
        assert!(state.reserve_step(StepKey::Step6).is_ok());
    }

    #[test]
    fn sequence_blocks_single_steps() {
        let state = WorkflowState::new();
        state.start_sequence("Custom").unwrap();
        assert!(state.reserve_step(StepKey::Step5).is_err());
        assert!(state.sequence_status().is_running);
        state.complete_sequence(true, "Séquence Custom terminée".into(), "Custom");
        assert!(!state.sequence_status().is_running);
        assert_eq!(state.sequence_outcome().status, "success");
    }

    #[test]
    fn manual_sequence_stop_is_not_overwritten() {
        let state = WorkflowState::new();
        state.start_sequence("Custom").unwrap();
        state.stop_sequence().unwrap();
        state.complete_sequence(true, "ne doit pas écraser".into(), "Custom");
        assert_eq!(state.sequence_outcome().status, "error");
        assert_eq!(
            state.sequence_outcome().message.as_deref(),
            Some("Sequence manually stopped by user")
        );
    }

    #[test]
    fn completed_step_replaces_the_waiting_text() {
        let state = WorkflowState::new();
        let control = state.reserve_step(StepKey::Step5).unwrap();
        assert!(!control.is_stop_requested());
        state.finish_step(StepKey::Step5, "completed", Some(0), "1s".into());
        let status = state.step_status(StepKey::Step5, false).unwrap();
        assert_eq!(status.progress_text, "Terminé");
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
