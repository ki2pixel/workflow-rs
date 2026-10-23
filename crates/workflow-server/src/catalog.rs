//! Catalogue unique des étapes exposées par l'orchestrateur.
//!
//! Les commandes ne contiennent aucune logique métier : elles délèguent à
//! `workflow-cli`, qui reste l'unique implémentation des étapes portées.

use serde::Serialize;
use serde_json::{json, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use workflow_core::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum StepKey {
    Step1,
    Step2,
    Step3,
    Step4,
    Step5,
    Step6,
    Step7,
    Step8,
}

impl StepKey {
    pub const ALL: [StepKey; 8] = [
        StepKey::Step1,
        StepKey::Step2,
        StepKey::Step3,
        StepKey::Step4,
        StepKey::Step5,
        StepKey::Step6,
        StepKey::Step7,
        StepKey::Step8,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_uppercase().as_str() {
            "STEP1" => Some(Self::Step1),
            "STEP2" => Some(Self::Step2),
            "STEP3" => Some(Self::Step3),
            "STEP4" => Some(Self::Step4),
            "STEP5" => Some(Self::Step5),
            "STEP6" => Some(Self::Step6),
            "STEP7" => Some(Self::Step7),
            "STEP8" => Some(Self::Step8),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Step1 => "STEP1",
            Self::Step2 => "STEP2",
            Self::Step3 => "STEP3",
            Self::Step4 => "STEP4",
            Self::Step5 => "STEP5",
            Self::Step6 => "STEP6",
            Self::Step7 => "STEP7",
            Self::Step8 => "STEP8",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Step1 => "1. Extraction et normalisation des archives",
            Self::Step2 => "2. Analyse des transitions",
            Self::Step3 => "3. Analyse audio",
            Self::Step4 => "4. Analyse du tracking",
            Self::Step5 => "5. Réduction JSON",
            Self::Step6 => "6. Pré-traitement AE",
            Self::Step7 => "7. Finalisation",
            Self::Step8 => "8. Media Solution (EffectCraft)",
        }
    }

    pub fn post_completion_message(self) -> Option<&'static str> {
        match self {
            Self::Step1 => Some("Extraction et normalisation terminées."),
            Self::Step2 => None,
            Self::Step3 => None,
            Self::Step4 => Some("Traitement du tracking terminé."),
            Self::Step5 => None,
            Self::Step6 => Some("Pré-traitement AE terminé."),
            Self::Step7 => Some("Finalisation des projets terminée."),
            Self::Step8 => Some("Projets EffectCraft générés."),
        }
    }

    /// Toutes les étapes du catalogue sont portées en Rust.
    pub fn is_available(self) -> bool {
        true
    }

    pub fn unavailable_reason(self) -> Option<&'static str> {
        None
    }

    pub fn timeout_s(self, config: &Config) -> u64 {
        match self {
            Self::Step1 => config.steps.step1.timeout_s,
            Self::Step8 => config.steps.step8.timeout_s,
            _ => config.steps.default_timeout_s,
        }
    }

    pub fn specific_logs(self, config: &Config) -> Vec<SpecificLog> {
        let logs = config.paths.resolve().logs;
        let mut entries = Vec::new();
        let (name, pattern, lines) = match self {
            Self::Step1 => ("Log Extraction", "*.log", 150),
            Self::Step2 => ("Log Analyse Transitions", "*.log", 150),
            Self::Step3 => ("Log Analyse Audio", "*.log", 150),
            Self::Step4 => ("Log Tracking TPU", "tpu_tracking*.log", 100),
            Self::Step5 => ("Log Réduction JSON", "*.log", 150),
            Self::Step6 => ("Log Pré-traitement AE", "*.log", 150),
            Self::Step7 => ("Log Finalisation", "*.log", 150),
            Self::Step8 => ("Log Media Solution", "*.log", 200),
        };
        entries.push(SpecificLog {
            name: name.into(),
            kind: LogKind::DirectoryLatest,
            path: logs.join(self.as_str().to_ascii_lowercase()),
            pattern: pattern.into(),
            lines,
        });
        if matches!(self, Self::Step1) {
            entries.push(SpecificLog {
                name: "Liste Archives Traitées".into(),
                kind: LogKind::File,
                path: logs.join("step1").join("processed_archives.txt"),
                pattern: String::new(),
                lines: 50,
            });
        }
        entries
    }

    /// Construit la commande `workflow-cli` complète (sans l'exécution).
    pub fn command(
        self,
        config: &Config,
        config_path: &Path,
        cli: &Path,
    ) -> Result<Vec<OsString>, String> {
        if let Some(reason) = self.unavailable_reason() {
            return Err(reason.to_string());
        }
        let resolved = config.paths.resolve();
        let mut command: Vec<OsString> = Vec::new();
        command.push(cli.as_os_str().to_os_string());
        command.push("--config".into());
        command.push(config_path.as_os_str().to_os_string());

        match self {
            Self::Step1 => {
                command.extend(["step1", "run", "--source-dir"].map(OsString::from));
                command.push(config.steps.step1.source_dir.clone().into_os_string());
            }
            Self::Step2 => {
                command.extend(["step2", "detect", "--input-dir"].map(OsString::from));
                command.push(resolved.projects.into_os_string());
            }
            Self::Step3 => {
                command.extend(["step3", "diarize", "--input-dir"].map(OsString::from));
                command.push(resolved.projects.into_os_string());
            }
            Self::Step4 => {
                command.extend(["step4", "track", "--input-dir"].map(OsString::from));
                command.push(resolved.projects.into_os_string());
            }
            Self::Step5 => {
                command.extend(["step5", "project", "--work-dir"].map(OsString::from));
                command.push(resolved.projects.into_os_string());
                command.push("--keyword".into());
                command.push(config.steps.filter.keyword.clone().into());
            }
            Self::Step6 => {
                command.extend(["step6", "project", "--work-dir"].map(OsString::from));
                command.push(resolved.projects.into_os_string());
                command.push("--keyword".into());
                command.push(config.steps.filter.keyword.clone().into());
            }
            Self::Step7 => {
                command.extend(["step7", "finalize", "--work-dir"].map(OsString::from));
                command.push(resolved.projects.into_os_string());
                command.push("--logs-dir".into());
                command.push(resolved.logs.into_os_string());
            }
            Self::Step8 => {
                command.extend(["step8", "run", "--logs-dir"].map(OsString::from));
                command.push(resolved.logs.into_os_string());
            }
        }
        Ok(command)
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LogKind {
    File,
    DirectoryLatest,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpecificLog {
    pub name: String,
    #[serde(skip)]
    pub kind: LogKind,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    pub pattern: String,
    pub lines: usize,
}

/// Configuration minimale consommée par le frontend existant.
pub fn frontend_config(config: &Config) -> Value {
    let mut steps = serde_json::Map::new();
    for step in StepKey::ALL {
        let mut value = json!({
            "display_name": step.display_name(),
            "specific_logs": step.specific_logs(config),
            "available": step.is_available(),
        });
        if let Some(message) = step.post_completion_message() {
            value["post_completion_message_ui"] = Value::String(message.into());
        }
        steps.insert(step.as_str().into(), value);
    }
    Value::Object(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_eight_explicit_steps() {
        assert_eq!(StepKey::ALL.len(), 8);
        assert_eq!(StepKey::parse("step3"), Some(StepKey::Step3));
        assert_eq!(StepKey::parse("STEP9"), None);
        assert!(StepKey::Step1.is_available());
        assert!(StepKey::Step2.is_available());
    }

    #[test]
    fn commands_are_explicit_and_use_the_shared_cli() {
        let config = Config::default();
        let config_path = Path::new("/tmp/workflow.toml");
        let cli = Path::new("/opt/workflow-cli");
        let command = StepKey::Step5
            .command(&config, config_path, cli)
            .expect("commande STEP5");
        assert_eq!(command[0], OsString::from("/opt/workflow-cli"));
        assert_eq!(command[1], OsString::from("--config"));
        assert_eq!(command[3], OsString::from("step5"));
        let step1 = StepKey::Step1
            .command(&config, config_path, cli)
            .expect("commande STEP1");
        assert_eq!(step1[3], OsString::from("step1"));
        assert_eq!(step1[4], OsString::from("run"));
    }

    #[test]
    fn frontend_config_exposes_only_the_polling_contract() {
        let config = Config::default();
        let value = frontend_config(&config);
        assert_eq!(value.as_object().unwrap().len(), 8);
        assert_eq!(value["STEP3"]["display_name"], "3. Analyse audio");
        assert!(value["STEP3"]["specific_logs"].is_array());
        assert!(!value.to_string().contains("progress_patterns"));
    }
}
