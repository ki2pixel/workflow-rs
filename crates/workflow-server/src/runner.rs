//! Exécution isolée d'une étape.
//!
//! Chaque étape est lancée comme un sous-processus `workflow-cli`. Le serveur
//! ne partage donc ni l'état ORT, ni les sessions Coral, ni les handles GPU
//! entre deux étapes. Le sous-processus acquiert lui-même le verrou matériel
//! lorsque l'étape en a besoin.

use crate::catalog::StepKey;
use crate::db::HistoryDb;
use crate::progress::ProgressParser;
use crate::state::{calculate_duration, now_epoch, terminate_pid, RunControl, WorkflowState};
use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use workflow_core::Config;

#[derive(Debug, Clone)]
pub struct StepOutcome {
    pub step: StepKey,
    pub status: String,
    pub return_code: Option<i32>,
    pub duration: String,
    pub success: bool,
}

pub struct Runner {
    config: Config,
    config_path: PathBuf,
    workdir: PathBuf,
    cli: PathBuf,
    state: Arc<WorkflowState>,
    db: Arc<HistoryDb>,
    max_log_bytes: u64,
}

impl Runner {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        state: Arc<WorkflowState>,
    ) -> Result<Arc<Self>> {
        let resolved = config.paths.resolve();
        for directory in [
            &resolved.base,
            &resolved.projects,
            &resolved.archives,
            &resolved.logs,
            &resolved.downloads,
        ] {
            fs::create_dir_all(directory)
                .with_context(|| format!("création du répertoire {}", directory.display()))?;
        }
        let workdir = absolute_from_cwd(&config.server.workdir)?;
        let cli = if config.server.workflow_cli.is_absolute() {
            config.server.workflow_cli.clone()
        } else {
            workdir.join(&config.server.workflow_cli)
        };
        let db = HistoryDb::open(&resolved.history_db)
            .with_context(|| format!("base isolée {}", resolved.history_db.display()))?;
        let config_path = if config_path.is_absolute() {
            config_path
        } else {
            std::env::current_dir()
                .context("répertoire courant introuvable")?
                .join(config_path)
        };
        let max_log_bytes = config.server.max_step_log_bytes;
        Ok(Arc::new(Self {
            config,
            config_path,
            workdir,
            cli,
            state,
            db: Arc::new(db),
            max_log_bytes,
        }))
    }

    pub fn state(&self) -> &Arc<WorkflowState> {
        &self.state
    }

    pub fn db(&self) -> &Arc<HistoryDb> {
        &self.db
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn cli_path(&self) -> &Path {
        &self.cli
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn check_ready(&self) -> Result<()> {
        if !self.cli.is_file() {
            anyhow::bail!(
                "workflow-cli introuvable: {} (compiler avec `cargo build --release -p workflow-cli`)",
                self.cli.display()
            );
        }
        if !self.workdir.is_dir() {
            anyhow::bail!("server.workdir introuvable: {}", self.workdir.display());
        }
        if !self.config_path.is_file() {
            anyhow::bail!("configuration introuvable: {}", self.config_path.display());
        }
        if !self.config.server.template.is_file() {
            anyhow::bail!(
                "gabarit introuvable: {}",
                self.config.server.template.display()
            );
        }
        if !self.config.server.frontend_dir.is_dir() {
            anyhow::bail!(
                "répertoire statique introuvable: {}",
                self.config.server.frontend_dir.display()
            );
        }
        if !self.config.server.sound_design_dir.is_dir() {
            anyhow::bail!(
                "répertoire sound-design introuvable: {}",
                self.config.server.sound_design_dir.display()
            );
        }
        Ok(())
    }

    /// Exécute une étape déjà réservée dans `WorkflowState`.
    pub async fn execute_reserved(
        self: &Arc<Self>,
        step: StepKey,
        control: Arc<RunControl>,
    ) -> StepOutcome {
        let started_epoch = now_epoch();
        self.state.append_log(
            step,
            format!("--- Lancement de: {} ---\n", step.display_name()),
        );

        if control.is_stop_requested() {
            return self.finish(step, "stopped", Some(-1), started_epoch, None);
        }
        if let Some(reason) = step.unavailable_reason() {
            self.state.append_log(step, format!("{reason}\n"));
            return self.finish(step, "failed", Some(-1), started_epoch, Some(reason.into()));
        }

        let command = match step.command(&self.config, &self.config_path, &self.cli) {
            Ok(command) => command,
            Err(error) => {
                self.state.append_log(step, format!("{error}\n"));
                return self.finish(step, "failed", Some(-1), started_epoch, Some(error));
            }
        };
        let command_line = command
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        self.state
            .append_log(step, format!("Commande: {}\n", command_line));
        self.state
            .append_log(step, format!("Dans: {}\n\n", self.workdir.display()));

        let mut log_file = open_log(
            &self.state,
            step,
            &self.config.paths.resolve().logs,
            self.max_log_bytes,
        );
        if let Some(file) = &mut log_file {
            let _ = writeln!(file, "--- Lancement de: {} ---", step.display_name());
            let _ = writeln!(file, "Commande: {command_line}");
            let _ = writeln!(file, "Dans: {}", self.workdir.display());
        }

        let mut process = Command::new(&command[0]);
        process
            .args(&command[1..])
            .current_dir(&self.workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => {
                let message = format!("Échec du lancement de {}: {error}", command_line);
                self.state.append_log(step, format!("{message}\n"));
                if let Some(file) = &mut log_file {
                    let _ = writeln!(file, "{message}");
                }
                return self.finish(step, "failed", Some(-1), started_epoch, Some(message));
            }
        };
        if let Some(pid) = child.id() {
            control.set_pid(pid);
        }
        self.state.mark_running(step);

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(pump(stdout, tx.clone()));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(pump(stderr, tx));
        }

        let mut parser = ProgressParser::new(step);
        let mut timed_out = false;
        let mut terminating_at: Option<Instant> = None;
        let timeout_at = Instant::now() + Duration::from_secs(step.timeout_s(&self.config));

        let exit_status = loop {
            while let Ok(line) = rx.try_recv() {
                write_line(&mut log_file, &line);
                parser.process(&line, &self.state);
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let message = format!("Erreur d'attente du sous-processus: {error}");
                    self.state.append_log(step, format!("{message}\n"));
                    let _ = child.start_kill();
                    return self.finish(step, "failed", Some(-1), started_epoch, Some(message));
                }
            }

            if control.is_stop_requested() && terminating_at.is_none() {
                terminating_at = Some(Instant::now() + Duration::from_secs(5));
                if let Some(pid) = control.pid() {
                    terminate_pid(pid);
                }
            } else if Instant::now() >= timeout_at && terminating_at.is_none() {
                timed_out = true;
                terminating_at = Some(Instant::now() + Duration::from_secs(5));
                self.state.append_log(
                    step,
                    format!(
                        "\n⚠️ ERREUR : Le script a dépassé le délai maximum de {} secondes.\nArrêt forcé en cours...\n",
                        step.timeout_s(&self.config)
                    ),
                );
                if let Some(pid) = control.pid() {
                    terminate_pid(pid);
                }
            }

            if terminating_at.is_some_and(|deadline| Instant::now() >= deadline) {
                let _ = child.start_kill();
                terminating_at = None;
            }

            tokio::select! {
                line = rx.recv() => {
                    match line {
                        Some(line) => {
                            write_line(&mut log_file, &line);
                            parser.process(&line, &self.state);
                        }
                        None => {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            }
        };

        // Lire les dernières lignes sans bloquer si un tube reste ouvert.
        let drain_deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < drain_deadline {
            match tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {
                Ok(Some(line)) => {
                    write_line(&mut log_file, &line);
                    parser.process(&line, &self.state);
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
        control.clear_pid();

        let stopped = control.is_stop_requested();
        let (status, code) = if stopped {
            ("stopped", Some(-1))
        } else if timed_out {
            ("failed", Some(-9))
        } else if exit_status.success() {
            ("completed", exit_status.code())
        } else {
            ("failed", exit_status.code())
        };
        let suffix = match status {
            "completed" => "terminé avec succès",
            "stopped" => "arrêté",
            _ => "a échoué",
        };
        self.state.append_log(
            step,
            format!("\n--- {} {} ---\n", step.display_name(), suffix),
        );
        self.finish(step, status, code, started_epoch, None)
    }

    fn finish(
        &self,
        step: StepKey,
        status: &str,
        return_code: Option<i32>,
        started_epoch: f64,
        _message: Option<String>,
    ) -> StepOutcome {
        let duration = calculate_duration(Some(started_epoch));
        self.state
            .finish_step(step, status, return_code, duration.clone());
        StepOutcome {
            step,
            status: status.into(),
            return_code,
            duration,
            success: status == "completed",
        }
    }
}

async fn pump<R>(reader: R, tx: mpsc::UnboundedSender<String>)
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buffer)
                    .trim_end_matches(['\n', '\r'])
                    .to_string();
                if tx.send(line).is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

fn write_line(file: &mut Option<BufWriter<File>>, line: &str) {
    if let Some(file) = file {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

fn open_log(
    state: &WorkflowState,
    step: StepKey,
    logs_dir: &Path,
    max_bytes: u64,
) -> Option<BufWriter<File>> {
    let path = logs_dir.join(format!("step_{}.log", step.as_str()));
    if let Err(error) = fs::create_dir_all(logs_dir) {
        state.append_log(
            step,
            format!("Avertissement: dossier de journaux indisponible: {error}\n"),
        );
        return None;
    }
    rotate_log(&path, max_bytes);
    match OpenOptions::new().create(true).append(true).open(&path) {
        Ok(file) => Some(BufWriter::new(file)),
        Err(error) => {
            state.append_log(
                step,
                format!(
                    "Avertissement: journal {} indisponible: {error}\n",
                    path.display()
                ),
            );
            None
        }
    }
}

fn rotate_log(path: &Path, max_bytes: u64) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() <= max_bytes {
        return;
    }
    for index in (1..=5).rev() {
        let source = path.with_extension(format!("log.{index}"));
        let target = path.with_extension(format!("log.{}", index + 1));
        if source.exists() {
            let _ = fs::rename(source, target);
        }
    }
    let _ = fs::rename(path, path.with_extension("log.1"));
}

fn absolute_from_cwd(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("répertoire courant introuvable")?
            .join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn fake_cli(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("fake-workflow-cli");
        fs::write(&path, body).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn test_runner(script: &str) -> (tempfile::TempDir, Arc<Runner>) {
        let dir = tempfile::tempdir().unwrap();
        let cli = fake_cli(dir.path(), script);
        let mut config = Config::default();
        config.paths.base = dir.path().join("runtime");
        config.paths.logs = Some(config.paths.base.join("logs"));
        config.paths.projects = Some(config.paths.base.join("projets_extraits"));
        config.paths.history_db = Some(config.paths.base.join("history.sqlite3"));
        config.server.workflow_cli = cli;
        config.server.workdir = dir.path().to_path_buf();
        config.hardware.lock_path = dir.path().join("hardware.lock");
        let state = Arc::new(WorkflowState::new());
        let runner = Runner::new(config, dir.path().join("config.toml"), state).unwrap();
        (dir, runner)
    }

    #[tokio::test]
    async fn successful_command_updates_state_and_log() {
        let (_dir, runner) = test_runner(
            "#!/bin/sh\necho 'TOTAL_JSON_TO_REDUCE: 1'\necho 'REDUCING_JSON: 1/1: demo'\necho 'Succès: réduction JSON terminée pour demo'\n",
        );
        let control = runner.state().reserve_step(StepKey::Step5).unwrap();
        let outcome = runner.execute_reserved(StepKey::Step5, control).await;
        assert_eq!(outcome.status, "completed");
        assert!(outcome.success);
        let status = runner.state().step_status(StepKey::Step5, true).unwrap();
        assert_eq!(status.progress_current, 1);
        assert_eq!(status.return_code, Some(0));
        assert!(status.log.unwrap().join("").contains("réduction JSON"));
    }

    #[tokio::test]
    async fn failed_command_is_reported() {
        let (_dir, runner) = test_runner("#!/bin/sh\necho boom >&2\nexit 7\n");
        let control = runner.state().reserve_step(StepKey::Step6).unwrap();
        let outcome = runner.execute_reserved(StepKey::Step6, control).await;
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.return_code, Some(7));
        assert_eq!(
            runner
                .state()
                .step_status(StepKey::Step6, false)
                .unwrap()
                .status,
            "failed"
        );
    }

    #[tokio::test]
    async fn timeout_is_reported_as_failure() {
        let (_dir, runner) = test_runner("#!/bin/sh\nexec sleep 30\n");
        // Le runner clone la configuration au démarrage : on reconstruit un
        // runner avec un timeout court pour ce cas.
        let mut config = runner.config().clone();
        config.steps.default_timeout_s = 1;
        let state = Arc::new(WorkflowState::new());
        let runner = Runner::new(config, runner.config_path().to_path_buf(), state).unwrap();
        let control = runner.state().reserve_step(StepKey::Step5).unwrap();
        let outcome = runner.execute_reserved(StepKey::Step5, control).await;
        assert_eq!(outcome.status, "failed");
        assert_eq!(outcome.return_code, Some(-9));
    }

    #[tokio::test]
    async fn stop_sends_sigterm_and_marks_stopped() {
        let (_dir, runner) = test_runner(
            "#!/bin/sh\necho 'TOTAL_JSON_TO_REDUCE: 1'\ntrap 'exit 0' TERM\nexec sleep 30\n",
        );
        let control = runner.state().reserve_step(StepKey::Step5).unwrap();
        let task = {
            let runner = runner.clone();
            tokio::spawn(async move { runner.execute_reserved(StepKey::Step5, control).await })
        };
        tokio::time::sleep(Duration::from_millis(250)).await;
        runner.state().stop_step(StepKey::Step5).unwrap();
        let outcome = task.await.unwrap();
        assert_eq!(outcome.status, "stopped");
        assert_eq!(outcome.return_code, Some(-1));
    }
}
