//! Single-instance D-Bus du client desktop.
//!
//! Le premier processus possède le nom `org.workflow.desktop` et sert
//! `org.freedesktop.Application`. Un second lancement détecte le nom déjà
//! pris, transmet ses arguments à l'instance active, puis se termine. Le
//! client ne dépend pas du plugin single-instance de Tauri : le contrat reste
//! explicite et testable avec `gdbus`.

use crate::Args;
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use url::Url;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::zvariant::{OwnedValue, Value};

pub const BUS_NAME: &str = "org.workflow.desktop";
pub const OBJECT_PATH: &str = "/org/workflow/desktop";

const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_ARGS: usize = 64;
const MAX_ARG_BYTES: usize = 4096;
const MAX_TOTAL_BYTES: usize = 32 * 1024;

/// Arguments transmis par un second lancement à l'instance active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activation {
    pub argv: Vec<String>,
    pub cwd: String,
    pub server_url: String,
    pub explicit_server_url: bool,
}

impl Activation {
    fn from_args(args: &Args) -> Result<Self> {
        let argv: Vec<String> = std::env::args_os()
            .map(|value| value.to_string_lossy().into_owned())
            .collect();
        let cwd = std::env::current_dir()
            .context("répertoire courant")?
            .to_string_lossy()
            .into_owned();
        let explicit_server_url = has_explicit_server_url(&argv);
        let activation = Self {
            argv,
            cwd,
            server_url: args.server_url.to_string(),
            explicit_server_url,
        };
        activation
            .validate()
            .map_err(|error| anyhow!("arguments d'activation invalides: {error}"))?;
        Ok(activation)
    }

    fn from_platform_data(data: HashMap<String, OwnedValue>) -> zbus::fdo::Result<Self> {
        let argv = data
            .get("argv")
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("argv absent".into()))?;
        let argv =
            Vec::<String>::try_from(argv.try_clone().map_err(|error| {
                zbus::fdo::Error::InvalidArgs(format!("argv illisible: {error}"))
            })?)
            .map_err(|error| zbus::fdo::Error::InvalidArgs(format!("argv invalide: {error}")))?;
        let cwd = data
            .get("cwd")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| String::try_from(value).ok())
            .unwrap_or_default();
        let server_url = data
            .get("server-url")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| String::try_from(value).ok())
            .unwrap_or_default();
        let explicit_server_url = data
            .get("explicit-server-url")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false);
        let activation = Self {
            argv,
            cwd,
            server_url,
            explicit_server_url,
        };
        activation
            .validate()
            .map_err(zbus::fdo::Error::InvalidArgs)?;
        Ok(activation)
    }

    fn validate(&self) -> Result<(), String> {
        if self.argv.is_empty() {
            return Err("argv vide".into());
        }
        if self.argv.len() > MAX_ARGS {
            return Err(format!("trop d'arguments (max {MAX_ARGS})"));
        }
        let mut total = self.cwd.len() + self.server_url.len();
        for argument in &self.argv {
            if argument.len() > MAX_ARG_BYTES {
                return Err(format!("argument trop long (max {MAX_ARG_BYTES} octets)"));
            }
            total += argument.len();
        }
        if total > MAX_TOTAL_BYTES {
            return Err(format!(
                "payload trop volumineux (max {MAX_TOTAL_BYTES} octets)"
            ));
        }
        Ok(())
    }

    fn to_platform_data(&self) -> zbus::Result<HashMap<String, OwnedValue>> {
        let mut data = HashMap::new();
        data.insert(
            "argv".to_string(),
            OwnedValue::try_from(Value::from(self.argv.clone()))?,
        );
        data.insert(
            "cwd".to_string(),
            OwnedValue::try_from(Value::from(self.cwd.clone()))?,
        );
        data.insert(
            "server-url".to_string(),
            OwnedValue::try_from(Value::from(self.server_url.clone()))?,
        );
        data.insert(
            "explicit-server-url".to_string(),
            OwnedValue::try_from(Value::from(self.explicit_server_url))?,
        );
        Ok(data)
    }

    /// Une URL n'est comparée que si le second lancement l'a explicitement
    /// fournie. Sans cela, un lancement depuis le menu (URL par défaut) serait
    /// rejeté alors que l'instance active utilise un port de test.
    pub fn conflicts_with(&self, active: &Url) -> bool {
        if !self.explicit_server_url {
            return false;
        }
        normalized_url(&self.server_url) != normalized_url(active.as_str())
    }
}

fn normalized_url(value: &str) -> &str {
    value.trim_end_matches('/')
}

pub fn has_explicit_server_url(argv: &[String]) -> bool {
    argv.iter()
        .any(|argument| argument == "--server-url" || argument.starts_with("--server-url="))
}

/// Guard qui maintient la connexion D-Bus et donc le nom bien connu.
pub struct Guard {
    _connection: zbus::Connection,
}

/// État de l'instance primaire : guard à conserver et flux d'activations.
pub struct Primary {
    pub guard: Guard,
    pub receiver: mpsc::Receiver<Activation>,
}

pub enum Acquire {
    Primary(Primary),
    Secondary,
    Unavailable(String),
}

/// Tente de devenir l'instance primaire ou active l'instance existante.
pub async fn acquire(args: &Args) -> Result<Acquire> {
    let activation = Activation::from_args(args)?;
    let connection = match zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(error) => {
            return Ok(Acquire::Unavailable(format!(
                "bus de session indisponible: {error}"
            )))
        }
    };

    let (sender, receiver) = mpsc::channel(8);
    if let Err(error) = connection
        .object_server()
        .at(
            OBJECT_PATH,
            ApplicationIface {
                sender,
                active_server_url: args.server_url.clone(),
            },
        )
        .await
    {
        return Ok(Acquire::Unavailable(format!(
            "enregistrement de l'interface D-Bus impossible: {error}"
        )));
    }

    let reply = connection
        .request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())
        .await;

    match reply {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
            Ok(Acquire::Primary(Primary {
                guard: Guard {
                    _connection: connection,
                },
                receiver,
            }))
        }
        Ok(RequestNameReply::Exists | RequestNameReply::InQueue) | Err(zbus::Error::NameTaken) => {
            activate_existing(&connection, &activation).await?;
            Ok(Acquire::Secondary)
        }
        Err(error) => Ok(Acquire::Unavailable(format!(
            "revendication du nom D-Bus impossible: {error}"
        ))),
    }
}

async fn activate_existing(connection: &zbus::Connection, activation: &Activation) -> Result<()> {
    let proxy = ApplicationProxy::new(connection)
        .await
        .context("création du proxy org.freedesktop.Application")?;
    let data = activation
        .to_platform_data()
        .context("encodage des arguments d'activation")?;
    match tokio::time::timeout(ACTIVATION_TIMEOUT, proxy.activate(data)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(anyhow!(
            "activation refusée par l'instance existante: {error}"
        )),
        Err(_) => Err(anyhow!(
            "délai d'activation dépassé ({} ms)",
            ACTIVATION_TIMEOUT.as_millis()
        )),
    }
}

struct ApplicationIface {
    sender: mpsc::Sender<Activation>,
    active_server_url: Url,
}

#[zbus::interface(name = "org.freedesktop.Application")]
impl ApplicationIface {
    async fn activate(&self, platform_data: HashMap<String, OwnedValue>) -> zbus::fdo::Result<()> {
        self.enqueue(platform_data).await
    }

    async fn open(
        &self,
        _uris: Vec<String>,
        platform_data: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        self.enqueue(platform_data).await
    }

    async fn activate_action(
        &self,
        _action_name: String,
        _parameter: Vec<OwnedValue>,
        platform_data: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<()> {
        self.enqueue(platform_data).await
    }
}

impl ApplicationIface {
    async fn enqueue(&self, platform_data: HashMap<String, OwnedValue>) -> zbus::fdo::Result<()> {
        let activation = Activation::from_platform_data(platform_data)?;
        if activation.conflicts_with(&self.active_server_url) {
            return Err(zbus::fdo::Error::InvalidArgs(format!(
                "URL divergente: instance active sur {}, second lancement sur {}",
                self.active_server_url, activation.server_url
            )));
        }
        self.sender
            .send(activation)
            .await
            .map_err(|_| zbus::fdo::Error::Failed("instance desktop arrêtée".into()))
    }
}

#[zbus::proxy(
    interface = "org.freedesktop.Application",
    default_service = "org.workflow.desktop",
    default_path = "/org/workflow/desktop"
)]
trait Application {
    fn activate(&self, platform_data: HashMap<String, OwnedValue>) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activation() -> Activation {
        Activation {
            argv: vec![
                "workflow-gui-tauri".to_string(),
                "--server-url".to_string(),
                "http://127.0.0.1:5005".to_string(),
            ],
            cwd: "/home/kidpixel".to_string(),
            server_url: "http://127.0.0.1:5005/".to_string(),
            explicit_server_url: true,
        }
    }

    #[test]
    fn platform_data_roundtrip_preserves_activation() {
        let original = activation();
        let data = original.to_platform_data().unwrap();
        let decoded = Activation::from_platform_data(data).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn missing_argv_is_rejected() {
        let mut data = activation().to_platform_data().unwrap();
        data.remove("argv");
        assert!(Activation::from_platform_data(data).is_err());
    }

    #[test]
    fn oversized_argv_is_rejected() {
        let mut activation = activation();
        activation.argv = vec!["x".repeat(MAX_ARG_BYTES + 1)];
        assert!(activation.validate().is_err());
    }

    #[test]
    fn too_many_arguments_are_rejected() {
        let mut activation = activation();
        activation.argv = vec!["x".to_string(); MAX_ARGS + 1];
        assert!(activation.validate().is_err());
    }

    #[test]
    fn explicit_server_url_is_detected() {
        assert!(has_explicit_server_url(&[
            "workflow-gui-tauri".to_string(),
            "--server-url".to_string(),
            "http://127.0.0.1:5005".to_string(),
        ]));
        assert!(has_explicit_server_url(&[
            "workflow-gui-tauri".to_string(),
            "--server-url=http://127.0.0.1:5005".to_string(),
        ]));
        assert!(!has_explicit_server_url(&[
            "workflow-gui-tauri".to_string(),
            "--no-tray".to_string(),
        ]));
    }

    #[test]
    fn only_explicit_divergent_url_conflicts() {
        let active = Url::parse("http://127.0.0.1:5005/").unwrap();
        let mut candidate = activation();
        candidate.server_url = "http://127.0.0.1:5005".to_string();
        assert!(!candidate.conflicts_with(&active));

        candidate.server_url = "http://127.0.0.1:5004/".to_string();
        assert!(candidate.conflicts_with(&active));

        candidate.explicit_server_url = false;
        assert!(!candidate.conflicts_with(&active));
    }
}
