//! Verrou de synchronisation et contrôles de non-activité.
//!
//! La synchronisation écrit dans les bases et les listes des deux backends.
//! Elle refuse donc de démarrer si un serveur écoute encore, ou si un processus
//! d'étape tourne. Le verrou empêche deux synchronisations simultanées.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::time::Duration;
use workflow_core::HardwareLock;

/// Verrou exclusif tenu pendant toute la synchronisation.
pub struct SyncLock {
    _inner: HardwareLock,
}

impl SyncLock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let inner = HardwareLock::acquire(path, false).with_context(|| {
            format!(
                "une autre synchronisation tient déjà le verrou {}",
                path.display()
            )
        })?;
        Ok(Self { _inner: inner })
    }
}

/// Vérifie qu'aucun serveur n'écoute sur les ports donnés.
///
/// `allow_running` est réservé aux diagnostics : une synchronisation réelle
/// pendant qu'un moniteur tourne peut manquer les téléchargements en cours.
pub fn ensure_servers_stopped(ports: &[u16], allow_running: bool) -> Result<Vec<u16>> {
    let mut open = Vec::new();
    for port in ports.iter().copied().filter(|port| *port > 0) {
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        if TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok() {
            open.push(port);
        }
    }
    open.sort_unstable();
    open.dedup();
    if !open.is_empty() && !allow_running {
        let ports = open
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "serveur actif sur le port {ports} ; arrêter les backends avant la synchronisation \
             (ou utiliser --allow-running pour un diagnostic)"
        );
    }
    Ok(open)
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub command: String,
}

/// Refuse la synchronisation si un serveur ou une étape tourne encore.
///
/// Le contrôle lit `/proc` sur Linux. Sur les autres plateformes, il ne fait
/// rien : le projet cible Linux en production.
pub fn ensure_no_conflicting_processes(allow_running: bool) -> Result<Vec<ProcessInfo>> {
    let processes = find_conflicting_processes();
    if !processes.is_empty() && !allow_running {
        let examples = processes
            .iter()
            .take(5)
            .map(|process| format!("PID {}: {}", process.pid, process.command))
            .collect::<Vec<_>>()
            .join("; ");
        bail!(
            "processus actifs détectés ({examples}). Arrêter les backends et les étapes avant \
             la synchronisation, ou utiliser --allow-running pour un diagnostic"
        );
    }
    Ok(processes)
}

#[cfg(target_os = "linux")]
fn find_conflicting_processes() -> Vec<ProcessInfo> {
    let current = std::process::id();
    let mut result = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return result;
    };
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        if pid == current {
            continue;
        }
        let Ok(bytes) = fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        if bytes.is_empty() {
            continue;
        }
        let args: Vec<String> = bytes
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8_lossy(arg).into_owned())
            .collect();
        if args.is_empty() || !is_conflicting_command(&args) {
            continue;
        }
        result.push(ProcessInfo {
            pid,
            command: args.join(" "),
        });
    }
    result
}

#[cfg(not(target_os = "linux"))]
fn find_conflicting_processes() -> Vec<ProcessInfo> {
    Vec::new()
}

fn is_conflicting_command(args: &[String]) -> bool {
    let executable = Path::new(&args[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if executable == "workflow-server" {
        return true;
    }
    if executable == "workflow-cli" {
        // Une autre synchronisation est déjà protégée par le verrou. Les
        // étapes et téléchargements manuels, eux, doivent être arrêtés.
        return !args.iter().any(|arg| arg == "sync");
    }
    if executable.starts_with("python") {
        return args.iter().any(|arg| {
            let name = Path::new(arg)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            name == "app_new.py"
                || name == "extract_archives.py"
                || arg.contains("workflow_scripts/step")
        });
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_server_and_step_commands() {
        assert!(is_conflicting_command(&[
            "/home/kidpixel/workflow-rs/target/release/workflow-server".into(),
            "--config".into(),
            "config/config.toml".into(),
        ]));
        assert!(is_conflicting_command(&[
            "/home/kidpixel/workflow-rs/target/release/workflow-cli".into(),
            "step1".into(),
            "run".into(),
        ]));
        assert!(is_conflicting_command(&[
            "/home/kidpixel/workflow_mediapipe/env/bin/python".into(),
            "app_new.py".into(),
        ]));
        assert!(is_conflicting_command(&[
            "python3".into(),
            "/home/kidpixel/workflow_mediapipe/workflow_scripts/step1/extract_archives.py".into(),
        ]));
    }

    #[test]
    fn ignores_the_sync_command_and_unrelated_processes() {
        assert!(!is_conflicting_command(&[
            "workflow-cli".into(),
            "sync".into(),
            "--apply".into(),
        ]));
        assert!(!is_conflicting_command(&[
            "/usr/bin/firefox".into(),
            "http://127.0.0.1:5004/".into(),
        ]));
    }
}
