//! Notifications natives (D-Bus `org.freedesktop.Notifications`).

use crate::error::ServicesError;
use crate::polling::{TerminalKind, TerminalOutcome};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
}

impl Notification {
    pub fn new(summary: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            body: body.into(),
            urgency: Urgency::Normal,
        }
    }

    pub fn with_urgency(mut self, urgency: Urgency) -> Self {
        self.urgency = urgency;
        self
    }
}

pub trait Notifier: Send + Sync {
    fn notify(&self, notification: &Notification) -> Result<(), ServicesError>;
}

#[derive(Debug, Default)]
pub struct NoopNotifier;

impl Notifier for NoopNotifier {
    fn notify(&self, _notification: &Notification) -> Result<(), ServicesError> {
        Ok(())
    }
}

/// Filtre les terminaux pour qu'une notification ne parte qu'une fois par
/// exécution, même si `last_terminal` reste présent dans les snapshots.
#[derive(Debug, Default)]
pub struct TransitionGate {
    last_seq: u64,
}

impl TransitionGate {
    pub fn observe(
        &mut self,
        terminal: Option<&TerminalOutcome>,
        terminal_seq: u64,
    ) -> Option<Notification> {
        let terminal = terminal?;
        if terminal_seq == 0 || terminal_seq == self.last_seq {
            return None;
        }
        self.last_seq = terminal_seq;
        let subject = match terminal.kind {
            TerminalKind::Step => terminal.step.clone().unwrap_or_else(|| "Étape".to_string()),
            TerminalKind::Sequence => "Séquence".to_string(),
        };
        let (summary, label, urgency) = match terminal.status.as_str() {
            "success" | "completed" => ("Workflow terminé", "terminé", Urgency::Normal),
            "error" | "failed" => ("Workflow en échec", "en échec", Urgency::Critical),
            "stopped" => ("Workflow arrêté", "arrêté", Urgency::Low),
            _ => return None,
        };
        let body = terminal
            .message
            .clone()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| format!("{subject} {label}"));
        Some(Notification::new(summary, body).with_urgency(urgency))
    }
}

#[derive(Debug, Default)]
pub struct NotifyRustNotifier;

impl Notifier for NotifyRustNotifier {
    fn notify(&self, notification: &Notification) -> Result<(), ServicesError> {
        let urgency = match notification.urgency {
            Urgency::Low => notify_rust::Urgency::Low,
            Urgency::Normal => notify_rust::Urgency::Normal,
            Urgency::Critical => notify_rust::Urgency::Critical,
        };
        notify_rust::Notification::new()
            .summary(&notification.summary)
            .body(&notification.body)
            .urgency(urgency)
            .show()
            .map(|_| ())
            .map_err(|error| ServicesError::Notify(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingNotifier(std::sync::Mutex<Vec<Notification>>);

    impl Notifier for RecordingNotifier {
        fn notify(&self, notification: &Notification) -> Result<(), ServicesError> {
            self.0.lock().unwrap().push(notification.clone());
            Ok(())
        }
    }

    #[test]
    fn noop_accepts_notifications() {
        NoopNotifier
            .notify(&Notification::new("titre", "corps"))
            .unwrap();
    }

    #[test]
    fn recording_notifier_captures_payload() {
        let notifier = RecordingNotifier(std::sync::Mutex::new(Vec::new()));
        notifier
            .notify(&Notification::new("titre", "corps").with_urgency(Urgency::Critical))
            .unwrap();
        let captured = notifier.0.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].summary, "titre");
        assert_eq!(captured[0].urgency, Urgency::Critical);
    }

    fn terminal(status: &str) -> TerminalOutcome {
        TerminalOutcome {
            kind: TerminalKind::Step,
            status: status.to_string(),
            message: Some("2. Analyse des transitions".to_string()),
            step: Some("STEP2".to_string()),
        }
    }

    #[test]
    fn transition_gate_notifies_once_per_terminal() {
        let mut gate = TransitionGate::default();
        assert!(gate.observe(None, 0).is_none());
        let notification = gate.observe(Some(&terminal("success")), 1).unwrap();
        assert_eq!(notification.summary, "Workflow terminé");
        assert_eq!(notification.urgency, Urgency::Normal);
        assert!(gate.observe(Some(&terminal("success")), 1).is_none());
    }

    #[test]
    fn transition_gate_notifies_a_new_terminal_even_if_identical() {
        let mut gate = TransitionGate::default();
        assert!(gate.observe(Some(&terminal("failed")), 1).is_some());
        let second = gate.observe(Some(&terminal("failed")), 2).unwrap();
        assert_eq!(second.summary, "Workflow en échec");
        assert_eq!(second.urgency, Urgency::Critical);
    }

    #[test]
    fn transition_gate_ignores_non_terminal_statuses() {
        let mut gate = TransitionGate::default();
        assert!(gate.observe(Some(&terminal("running")), 1).is_none());
    }
}
