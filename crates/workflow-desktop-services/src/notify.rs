//! Notifications natives (D-Bus `org.freedesktop.Notifications`).

use crate::error::ServicesError;

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
}
