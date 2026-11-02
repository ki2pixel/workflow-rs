//! Parsing des lignes de progression et journal HTML.
//!
//! Les motifs couvrent les sorties Rust de `workflow-cli` et les variantes
//! historiques du Python afin que le contrat de polling reste identique.

use crate::catalog::StepKey;
use crate::state::WorkflowState;
use regex::Regex;
use std::collections::HashSet;

pub struct ProgressParser {
    step: StepKey,
    total: Regex,
    numbered: Regex,
    processing: Regex,
    internal: Regex,
    internal_batches: Regex,
    post_total: Regex,
    post_current: Regex,
    post_progress: Regex,
    post_done: Regex,
    post_base: Option<u64>,
    success: Regex,
    success_batch: Regex,
    success_finalize: Regex,
    success_extract: Regex,
    failure: Regex,
    failure_batch: Regex,
    completed_items: HashSet<String>,
    files_completed: u64,
}

impl ProgressParser {
    pub fn new(step: StepKey) -> Self {
        Self {
            step,
            total: Regex::new(
                r"(?:TOTAL[_ ](?:VIDEOS[_ ]TO[_ ]PROCESS|JSON_TO_REDUCE|AE_PREPROCESS)|MS_BATCH_TOTAL)\s*:\s*(\d+)",
            )
            .expect("motif total"),
            numbered: Regex::new(
                r"(?:REDUCING_JSON|PREPROCESS_AE|MS_BATCH_PROGRESS)\s*:\s*(\d+)/(\d+)\s*(?::-|:)?\s*(.*)",
            )
            .expect("motif numéroté"),
            processing: Regex::new(r"PROCESSING[_ ]VIDEO\s*:\s*(?:(\d+)/(\d+)\s*:\s*)?(.*)")
                .expect("motif processing"),
            internal: Regex::new(
                r"INTERNAL[_ ]PROGRESS\s*:\s*(\d+)/(\d+)\s*(?:items|frames|vidéos)?\s*\((\d+)%\)\s*-\s*(.*)",
            )
            .expect("motif interne"),
            internal_batches: Regex::new(r"INTERNAL[_ ]PROGRESS\s*:\s*(\d+)\s*batches\s*-\s*(.*)")
                .expect("motif batches"),
            post_total: Regex::new(r"^POST_TOTAL\s*:\s*(\d+)\s*$").expect("motif total post"),
            post_current: Regex::new(
                r"^POST_CURRENT\s*:\s*(\d+)/(\d+)\s*(?:-|:)?\s*(.*)$",
            )
            .expect("motif projet courant post"),
            post_progress: Regex::new(
                r"^POST_PROGRESS\s*:\s*(\d+)/(\d+)\s*(?:-|:)?\s*(.*)$",
            )
            .expect("motif progression post"),
            post_done: Regex::new(r"^POST_DONE\s*:\s*(\d+)(?:\s*-\s*(\d+))?\s*$")
                .expect("motif fin post"),
            post_base: None,
            success: Regex::new(r"^\s*Succès\s*:\s*(.*)$").expect("motif succès"),
            success_batch: Regex::new(r"MS_BATCH_OK\s*:\s*(.*)$").expect("motif succès batch"),
            success_finalize: Regex::new(r"Finalisation terminée pour '(.*?)'")
                .expect("motif finalisation"),
            success_extract: Regex::new(r"Extraction terminée pour (.*)$")
                .expect("motif extraction"),
            failure: Regex::new(r"^\s*Échec\s*:\s*(.*)$").expect("motif échec"),
            failure_batch: Regex::new(r"MS_BATCH_FAIL\s*:\s*(.*)$").expect("motif échec batch"),
            completed_items: HashSet::new(),
            files_completed: 0,
        }
    }

    /// Traite une ligne brute (sans `\n`). Retourne `true` si la ligne a été
    /// ajoutée au journal.
    pub fn process(&mut self, line: &str, state: &WorkflowState) -> bool {
        if line.contains('\r') || line.contains("\u{1b}[") {
            return false;
        }
        let trimmed = line.trim();
        if trimmed.starts_with("[Progression-MultiLine]") {
            let text = trimmed
                .trim_start_matches("[Progression-MultiLine]")
                .replace(" || ", "\n");
            state.set_progress_text(self.step, html_escape(&text));
            return false;
        }

        state.append_log(self.step, format!("{}\n", html_escape(line)));

        if let Some(captures) = self.total.captures(trimmed) {
            if let Some(total) = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok())
            {
                state.set_progress_total(self.step, total);
                if self.files_completed == 0 {
                    state.set_progress(self.step, 0, Some(total), None, None);
                }
            }
        }
        if let Some(captures) = self.numbered.captures(trimmed) {
            let current = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let total = captures
                .get(2)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let text = captures.get(3).map(|value| value.as_str().trim());
            if let Some(current) = current {
                self.files_completed = current.saturating_sub(1);
                state.set_progress(
                    self.step,
                    current,
                    total,
                    text.filter(|value| !value.is_empty()).map(html_escape),
                    Some(None),
                );
            }
        }
        if let Some(captures) = self.processing.captures(trimmed) {
            let current = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let total = captures
                .get(2)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let text = captures.get(3).map(|value| value.as_str().trim());
            if let Some(text) = text.filter(|value| !value.is_empty()) {
                state.set_progress_text(self.step, html_escape(text));
            }
            if let Some(current) = current {
                self.files_completed = current.saturating_sub(1);
                state.set_progress(self.step, current, total, None, Some(None));
            }
        }
        if let Some(captures) = self.internal.captures(trimmed) {
            let percent = captures
                .get(3)
                .and_then(|value| value.as_str().parse::<f64>().ok());
            let text = captures.get(4).map(|value| value.as_str().trim());
            let total = state.progress_total(self.step);
            if total > 0 {
                let fractional = (self.files_completed as f64 + percent.unwrap_or(0.0) / 100.0)
                    .min(total as f64);
                state.set_progress(
                    self.step,
                    self.files_completed.min(total),
                    Some(total),
                    text.filter(|value| !value.is_empty()).map(html_escape),
                    Some(Some(fractional)),
                );
            }
        }
        if let Some(captures) = self.internal_batches.captures(trimmed) {
            let text = captures.get(2).map(|value| value.as_str().trim());
            if let Some(text) = text.filter(|value| !value.is_empty()) {
                state.set_progress_text(self.step, html_escape(text));
            }
        }
        if let Some(captures) = self.post_total.captures(trimmed) {
            if let Some(total) = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok())
            {
                let base = self.post_base(state);
                state.set_progress_phase(self.step, Some("post".into()));
                state.set_progress(
                    self.step,
                    base,
                    Some(base + total),
                    Some(format!("Post-traitement A5/A6 : 0/{total}")),
                    Some(None),
                );
            }
        }
        if let Some(captures) = self.post_current.captures(trimmed) {
            let current = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let total = captures
                .get(2)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let label = captures
                .get(3)
                .map(|value| value.as_str().trim())
                .unwrap_or_default();
            if let (Some(current), Some(total)) = (current, total) {
                self.set_post_cursor(state, current, total, label, false);
            }
        }
        if let Some(captures) = self.post_progress.captures(trimmed) {
            let current = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let total = captures
                .get(2)
                .and_then(|value| value.as_str().parse::<u64>().ok());
            let label = captures
                .get(3)
                .map(|value| value.as_str().trim())
                .unwrap_or_default();
            if let (Some(current), Some(total)) = (current, total) {
                self.set_post_cursor(state, current, total, label, true);
            }
        }
        if let Some(captures) = self.post_done.captures(trimmed) {
            let total = captures
                .get(1)
                .and_then(|value| value.as_str().parse::<u64>().ok())
                .unwrap_or(0);
            let failures = captures
                .get(2)
                .and_then(|value| value.as_str().parse::<u64>().ok())
                .unwrap_or(0);
            let base = self.post_base(state);
            let text = if failures > 0 {
                format!("Post-traitement A5/A6 : {total} projet(s), {failures} échec(s)")
            } else {
                format!("Post-traitement A5/A6 : {total} projet(s)")
            };
            state.set_progress_phase(self.step, Some("post".into()));
            state.set_progress(
                self.step,
                base + total,
                Some(base + total),
                Some(html_escape(&text)),
                Some(None),
            );
        }

        let success = self
            .success
            .captures(trimmed)
            .or_else(|| self.success_batch.captures(trimmed))
            .or_else(|| self.success_finalize.captures(trimmed))
            .or_else(|| self.success_extract.captures(trimmed));
        if let Some(captures) = success {
            let name = captures
                .get(1)
                .map(|value| value.as_str().trim().to_string())
                .unwrap_or_default();
            self.record_success(name, state);
        }
        if let Some(captures) = self
            .failure
            .captures(trimmed)
            .or_else(|| self.failure_batch.captures(trimmed))
        {
            let name = captures
                .get(1)
                .map(|value| value.as_str().trim().to_string())
                .unwrap_or_default();
            if !name.is_empty() {
                self.record_success(name, state);
            }
        }
        true
    }

    fn post_base(&mut self, state: &WorkflowState) -> u64 {
        *self
            .post_base
            .get_or_insert_with(|| state.progress_total(self.step))
    }

    fn set_post_cursor(
        &mut self,
        state: &WorkflowState,
        current: u64,
        total: u64,
        label: &str,
        completed: bool,
    ) {
        let base = self.post_base(state);
        let done = if completed {
            current.min(total)
        } else {
            current.saturating_sub(1).min(total)
        };
        let text = if label.is_empty() {
            format!("Post-traitement A5/A6 : {current}/{total}")
        } else {
            format!("Post-traitement A5/A6 : {current}/{total} — {label}")
        };
        state.set_progress_phase(self.step, Some("post".into()));
        state.set_progress(
            self.step,
            base + done,
            Some(base + total),
            Some(html_escape(&text)),
            Some(None),
        );
    }

    fn record_success(&mut self, name: String, state: &WorkflowState) {
        if !name.is_empty() && !self.completed_items.insert(name.clone()) {
            return;
        }
        self.files_completed += 1;
        let total = state.progress_total(self.step);
        let current = if total > 0 {
            self.files_completed.min(total)
        } else {
            self.files_completed
        };
        let text = (!name.is_empty()).then(|| html_escape(&name));
        state.set_progress(self.step, current, None, text, Some(None));
    }
}

pub fn html_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_and_numbered_lines_update_progress() {
        let state = WorkflowState::new();
        let mut parser = ProgressParser::new(StepKey::Step5);
        parser.process("TOTAL_JSON_TO_REDUCE: 3", &state);
        parser.process("REDUCING_JSON: 2/3: 200 Camille", &state);
        let status = state.step_status(StepKey::Step5, false).unwrap();
        assert_eq!(status.progress_total, 3);
        assert_eq!(status.progress_current, 2);
        assert_eq!(status.progress_text, "200 Camille");
    }

    #[test]
    fn success_deduplicates_and_clamps_to_total() {
        let state = WorkflowState::new();
        let mut parser = ProgressParser::new(StepKey::Step2);
        parser.process("TOTAL_VIDEOS_TO_PROCESS: 1", &state);
        parser.process("Succès: clip.csv créé avec 2 scènes", &state);
        parser.process("Succès: clip.csv créé avec 2 scènes", &state);
        let status = state.step_status(StepKey::Step2, false).unwrap();
        assert_eq!(status.progress_current, 1);
        assert_eq!(status.progress_total, 1);
    }

    #[test]
    fn internal_progress_is_fractional() {
        let state = WorkflowState::new();
        let mut parser = ProgressParser::new(StepKey::Step2);
        parser.process("TOTAL_VIDEOS_TO_PROCESS: 2", &state);
        parser.process("INTERNAL_PROGRESS: 25/100 frames (50%) - clip.mp4", &state);
        let status = state.step_status(StepKey::Step2, false).unwrap();
        assert_eq!(status.progress_current_fractional, Some(0.5));
        assert_eq!(status.progress_text, "clip.mp4");
    }

    #[test]
    fn failure_lines_count_as_finished_items() {
        let state = WorkflowState::new();
        let mut parser = ProgressParser::new(StepKey::Step8);
        parser.process("MS_BATCH_TOTAL: 2", &state);
        parser.process("MS_BATCH_FAIL: clip.mp4", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 1);
    }

    #[test]
    fn post_phase_extends_step8_progress() {
        let state = WorkflowState::new();
        let mut parser = ProgressParser::new(StepKey::Step8);
        parser.process("MS_BATCH_TOTAL: 1", &state);
        parser.process("MS_BATCH_PROGRESS: 1/1 - 300 Camille", &state);
        parser.process("POST_TOTAL: 3", &state);

        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_total, 4);
        assert_eq!(status.progress_current, 1);
        assert_eq!(status.progress_text, "Post-traitement A5/A6 : 0/3");
        assert_eq!(status.progress_phase.as_deref(), Some("post"));

        parser.process("POST_CURRENT: 1/3 - clip-a", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 1);
        assert_eq!(status.progress_total, 4);
        assert_eq!(status.progress_text, "Post-traitement A5/A6 : 1/3 — clip-a");

        parser.process("POST_PROGRESS: 1/3 - clip-a", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 2);
        assert_eq!(status.progress_total, 4);

        parser.process("POST_CURRENT: 3/3 - clip-c", &state);
        parser.process("POST_PROGRESS: 3/3 - clip-c", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 4);
        assert_eq!(status.progress_total, 4);

        parser.process("POST_DONE: 3 - 0", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 4);
        assert_eq!(status.progress_total, 4);
        assert_eq!(status.progress_text, "Post-traitement A5/A6 : 3 projet(s)");
    }

    #[test]
    fn post_phase_without_effectcraft_base_starts_at_zero() {
        let state = WorkflowState::new();
        let mut parser = ProgressParser::new(StepKey::Step8);
        parser.process("POST_TOTAL: 2", &state);
        parser.process("POST_CURRENT: 1/2 - clip-a", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 0);
        assert_eq!(status.progress_total, 2);

        parser.process("POST_PROGRESS: 1/2 - clip-a", &state);
        parser.process("POST_CURRENT: 2/2 - clip-b", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 1);

        parser.process("POST_PROGRESS: 2/2 - clip-b", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 2);

        parser.process("POST_DONE: 2 - 1", &state);
        let status = state.step_status(StepKey::Step8, false).unwrap();
        assert_eq!(status.progress_current, 2);
        assert_eq!(
            status.progress_text,
            "Post-traitement A5/A6 : 2 projet(s), 1 échec(s)"
        );
    }

    #[test]
    fn ansi_lines_are_ignored() {
        let state = WorkflowState::new();
        let mut parser = ProgressParser::new(StepKey::Step4);
        assert!(!parser.process("\u{1b}[31mrouge", &state));
        assert!(state
            .step_status(StepKey::Step4, true)
            .unwrap()
            .log
            .unwrap()
            .is_empty());
    }
}
