//! Surveillance périodique de l'état de séquence.

use crate::client::DesktopClient;
use crate::models::SequenceStatus;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy)]
pub struct PollingConfig {
    pub interval: Duration,
    pub max_errors: u32,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(1),
            max_errors: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Sequence(SequenceStatus),
    Error(String),
    Stopped,
}

pub struct StatusWatcher {
    client: DesktopClient,
    config: PollingConfig,
    cancel: CancellationToken,
    tx: broadcast::Sender<WatchEvent>,
}

impl StatusWatcher {
    pub fn new(
        client: DesktopClient,
        config: PollingConfig,
    ) -> (Self, broadcast::Receiver<WatchEvent>) {
        let (tx, rx) = broadcast::channel(64);
        (
            Self {
                client,
                config,
                cancel: CancellationToken::new(),
                tx,
            },
            rx,
        )
    }

    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    async fn run(self) {
        let mut consecutive_errors = 0_u32;
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    let _ = self.tx.send(WatchEvent::Stopped);
                    break;
                }
                _ = tokio::time::sleep(self.config.interval) => {
                    match self.client.sequence_status().await {
                        Ok(status) => {
                            consecutive_errors = 0;
                            let _ = self.tx.send(WatchEvent::Sequence(status));
                        }
                        Err(error) => {
                            consecutive_errors = consecutive_errors.saturating_add(1);
                            let _ = self.tx.send(WatchEvent::Error(error.to_string()));
                            if consecutive_errors >= self.config.max_errors {
                                let _ = self.tx.send(WatchEvent::Stopped);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_polling_config_is_one_second_and_three_errors() {
        let config = PollingConfig::default();
        assert_eq!(config.interval, Duration::from_secs(1));
        assert_eq!(config.max_errors, 3);
    }
}
