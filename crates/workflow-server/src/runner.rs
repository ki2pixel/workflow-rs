//! Exécution isolée d'une étape.
//!
//! Chaque étape est lancée comme un sous-processus `workflow-cli`. Le serveur
//! ne partage donc ni l'état ORT, ni les sessions Coral, ni les handles GPU
//! entre deux étapes. Le sous-processus acquiert lui-même le verrou matériel
//! lorsque l'étape en a besoin.

use crate::catalog::StepKey;
use crate::db::HistoryDb;
use crate::progress::ProgressParser;
use crate::state::{
    calculate_duration, now_epoch, terminate_pid, RunControl, RunId, StepPhase, WorkflowState,
};
use anyhow::{Context, Result};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use workflow_core::Config;

/// Capacité du canal de logs : un producteur bavard ne peut pas faire croître
/// la mémoire sans limite.
const LOG_CHANNEL_CAPACITY: usize = 256;
/// Taille maximale d'une ligne conservée (au-delà, elle est tronquée).
const MAX_LINE_BYTES: usize = 64 * 1024;
/// Secrets du serveur qui ne doivent jamais atteindre les étapes.
const CHILD_SECRET_ENV: [&str; 4] = [
    "WORKFLOW_SERVER_WORKER_TOKEN",
    "WORKFLOW_SERVER_SECRET_KEY",
    "FLASK_SECRET_KEY",
    "INTERNAL_WORKER_COMMS_TOKEN",
];

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
        let purged = workflow_core::fsutil::purge_orphans(
            &resolved.base,
            config.server.orphan_max_age_hours,
        );
        if purged > 0 {
            tracing::info!(purged, "résidus d'exécutions interrompues purgés");
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
        let run_id = control.run_id();
        let mut guard = FinishGuard::new(&self.state, step, run_id, started_epoch);
        self.state.append_log(
            step,
            format!("--- Lancement de: {} ---\n", step.display_name()),
        );

        if control.is_stop_requested() {
            return guard.finish("stopped", Some(-1));
        }
        if let Some(reason) = step.unavailable_reason() {
            self.state.append_log(step, format!("{reason}\n"));
            return guard.finish("failed", Some(-1));
        }

        let command = match step.command(&self.config, &self.config_path, &self.cli) {
            Ok(command) => command,
            Err(error) => {
                self.state.append_log(step, format!("{error}\n"));
                return guard.finish("failed", Some(-1));
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

        let log_sink = open_log(
            &self.state,
            step,
            &self.config.paths.resolve().logs,
            self.max_log_bytes,
        );
        let log_path = log_sink.as_ref().map(|sink| sink.path().to_path_buf());
        let log_writer = log_sink.map(LogWriter::spawn);
        write_log(
            &log_writer,
            format!("--- Lancement de: {} ---", step.display_name()),
        )
        .await;
        write_log(&log_writer, format!("Commande: {command_line}")).await;
        write_log(&log_writer, format!("Dans: {}", self.workdir.display())).await;

        let mut process = Command::new(&command[0]);
        process
            .args(&command[1..])
            .current_dir(&self.workdir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Le CLI et ses enfants (ffmpeg, EffectCraft, Python) n'ont aucun
        // besoin des secrets HTTP du serveur. Les retirer limite l'exposition
        // aux outils tiers tout en conservant l'environnement CUDA/cache.
        sanitize_child_environment(&mut process);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            process.as_std_mut().process_group(0);
        }
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(error) => {
                let message = format!("Échec du lancement de {}: {error}", command_line);
                self.state.append_log(step, format!("{message}\n"));
                write_log(&log_writer, message.clone()).await;
                tracing::error!(
                    step = step.as_str(),
                    run_id = run_id.get(),
                    error = %error,
                    "échec du lancement de l'étape"
                );
                if let Some(writer) = log_writer {
                    writer.finish().await;
                }
                return guard.finish("failed", Some(-1));
            }
        };
        if let Some(pid) = child.id() {
            control.set_pid(pid);
            tracing::info!(
                step = step.as_str(),
                run_id = run_id.get(),
                pid,
                command = %command_line,
                "étape lancée"
            );
        } else {
            tracing::info!(
                step = step.as_str(),
                run_id = run_id.get(),
                command = %command_line,
                "étape lancée"
            );
        }
        self.state.mark_running(step, run_id);

        let (tx, mut rx) = mpsc::channel::<String>(LOG_CHANNEL_CAPACITY);
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
                parser.process(&line, &self.state);
                write_log(&log_writer, line).await;
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let message = format!("Erreur d'attente du sous-processus: {error}");
                    self.state.append_log(step, format!("{message}\n"));
                    let _ = child.start_kill();
                    return guard.finish("failed", Some(-1));
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
                            parser.process(&line, &self.state);
                            write_log(&log_writer, line).await;
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
                    parser.process(&line, &self.state);
                    write_log(&log_writer, line).await;
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
        write_log(
            &log_writer,
            format!("\n--- {} {} ---\n", step.display_name(), suffix),
        )
        .await;
        let outcome = guard.finish(status, code);
        if let Some(writer) = log_writer {
            writer.finish().await;
        }
        tracing::info!(
            step = step.as_str(),
            run_id = run_id.get(),
            status = outcome.status.as_str(),
            return_code = ?outcome.return_code,
            duration = %outcome.duration,
            log = ?log_path,
            "étape terminée"
        );
        outcome
    }
}

/// Garde RAII : si la tâche disparaît sans finaliser (panique, annulation),
/// l'état publie un échec au lieu de rester bloqué en `running`.
struct FinishGuard<'a> {
    state: &'a WorkflowState,
    step: StepKey,
    run_id: RunId,
    started_epoch: f64,
    finished: bool,
}

impl<'a> FinishGuard<'a> {
    fn new(state: &'a WorkflowState, step: StepKey, run_id: RunId, started_epoch: f64) -> Self {
        Self {
            state,
            step,
            run_id,
            started_epoch,
            finished: false,
        }
    }

    fn finish(&mut self, status: &str, return_code: Option<i32>) -> StepOutcome {
        let duration = calculate_duration(Some(self.started_epoch));
        let phase = match status {
            "completed" => StepPhase::Completed,
            "stopped" => StepPhase::Stopped,
            _ => StepPhase::Failed,
        };
        self.finished = true;
        self.state
            .finish_step(self.step, self.run_id, phase, return_code, duration.clone());
        StepOutcome {
            step: self.step,
            status: status.into(),
            return_code,
            duration,
            success: status == "completed",
        }
    }
}

impl Drop for FinishGuard<'_> {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let duration = calculate_duration(Some(self.started_epoch));
        self.state.finish_step(
            self.step,
            self.run_id,
            StepPhase::Failed,
            Some(-1),
            duration,
        );
    }
}

async fn pump<R>(reader: R, tx: mpsc::Sender<String>)
where
    R: AsyncRead + Unpin,
{
    let mut reader = BufReader::new(reader);
    let mut buffer = Vec::new();
    loop {
        match read_capped_line(&mut reader, &mut buffer).await {
            Ok(Some(line)) => {
                if tx.send(line).await.is_err() {
                    break;
                }
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
}

/// Lit une ligne en plafonnant sa taille : le surplus est consommé puis jeté
/// au lieu d'être accumulé en mémoire.
async fn read_capped_line<R>(
    reader: &mut R,
    buffer: &mut Vec<u8>,
) -> std::io::Result<Option<String>>
where
    R: AsyncBufRead + Unpin,
{
    buffer.clear();
    let mut truncated = false;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            if buffer.is_empty() && !truncated {
                return Ok(None);
            }
            break;
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        let take = newline.map(|index| index + 1).unwrap_or(chunk.len());
        let room = MAX_LINE_BYTES.saturating_sub(buffer.len());
        let copied = take.min(room);
        buffer.extend_from_slice(&chunk[..copied]);
        if copied < take {
            truncated = true;
        }
        reader.consume(take);
        if newline.is_some() {
            break;
        }
    }
    let mut line = String::from_utf8_lossy(buffer)
        .trim_end_matches(['\n', '\r'])
        .to_string();
    if truncated {
        line.push_str(" …[ligne tronquée]");
    }
    Ok(Some(line))
}

struct LogSink {
    file: BufWriter<File>,
    path: PathBuf,
    max_bytes: u64,
    written: u64,
}

impl LogSink {
    fn new(path: PathBuf, max_bytes: u64, file: File) -> Self {
        let written = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
        Self {
            file: BufWriter::new(file),
            path,
            max_bytes,
            written,
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn flush(&mut self) {
        let _ = self.file.flush();
    }

    fn write_line(&mut self, line: &str) {
        let _ = writeln!(self.file, "{line}");
        self.written = self.written.saturating_add(line.len() as u64 + 1);
        if self.max_bytes > 0 && self.written >= self.max_bytes {
            self.rotate();
        }
    }

    fn rotate(&mut self) {
        let _ = self.file.flush();
        rotate_log(&self.path, 0);
        if let Ok(file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            self.file = BufWriter::new(file);
            self.written = 0;
        }
    }
}

enum LogCommand {
    Line(String),
}

/// Écrit les journaux sur un thread bloquant dédié : un disque lent ne bloque
/// plus un worker du runtime Tokio.
struct LogWriter {
    tx: mpsc::Sender<LogCommand>,
    handle: tokio::task::JoinHandle<()>,
}

impl LogWriter {
    fn spawn(mut sink: LogSink) -> Self {
        let (tx, mut rx) = mpsc::channel(128);
        let handle = tokio::task::spawn_blocking(move || {
            while let Some(LogCommand::Line(line)) = rx.blocking_recv() {
                sink.write_line(&line);
            }
            sink.flush();
        });
        Self { tx, handle }
    }

    async fn line(&self, line: String) {
        if self.tx.send(LogCommand::Line(line)).await.is_err() {
            tracing::warn!("écriture du journal interrompue");
        }
    }

    async fn finish(self) {
        drop(self.tx);
        if let Err(error) = self.handle.await {
            tracing::warn!(%error, "thread d'écriture du journal en échec");
        }
    }
}

async fn write_log(writer: &Option<LogWriter>, line: impl Into<String>) {
    if let Some(writer) = writer {
        writer.line(line.into()).await;
    }
}

fn open_log(
    state: &WorkflowState,
    step: StepKey,
    logs_dir: &Path,
    max_bytes: u64,
) -> Option<LogSink> {
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
        Ok(file) => Some(LogSink::new(path, max_bytes, file)),
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

fn sanitize_child_environment(command: &mut Command) {
    for key in CHILD_SECRET_ENV {
        command.env_remove(key);
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

    #[test]
    fn child_environment_excludes_server_secrets_but_keeps_cache_vars() {
        let mut command = Command::new("/bin/true");
        command.env("CUDA_CACHE_PATH", "/tmp/cuda-cache");
        for key in CHILD_SECRET_ENV {
            command.env(key, "secret-de-test");
        }
        sanitize_child_environment(&mut command);

        let envs: Vec<_> = command
            .as_std()
            .get_envs()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().to_string(),
                    value.map(|value| value.to_string_lossy().to_string()),
                )
            })
            .collect();
        for key in CHILD_SECRET_ENV {
            assert!(
                envs.iter()
                    .any(|(name, value)| name == key && value.is_none()),
                "secret encore présent: {key}"
            );
        }
        assert!(envs.iter().any(|(name, value)| {
            name == "CUDA_CACHE_PATH" && value.as_deref() == Some("/tmp/cuda-cache")
        }));
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
    async fn given_giant_line_when_captured_then_truncated() {
        let (_dir, runner) =
            test_runner("#!/bin/sh\nhead -c 200000 /dev/zero | tr '\\0' 'a'\necho\n");
        let control = runner.state().reserve_step(StepKey::Step5).unwrap();
        let outcome = runner.execute_reserved(StepKey::Step5, control).await;
        assert_eq!(outcome.status, "completed");
        let status = runner.state().step_status(StepKey::Step5, true).unwrap();
        let log = status.log.unwrap();
        assert!(log.iter().any(|line| line.contains("tronqué")));
        assert!(log.iter().all(|line| line.len() < 70 * 1024));
    }

    #[tokio::test]
    async fn given_log_exceeding_limit_when_running_then_rotated() {
        let dir = tempfile::tempdir().unwrap();
        let cli = fake_cli(
            dir.path(),
            "#!/bin/sh\ni=0\nwhile [ $i -lt 200 ]; do echo '0123456789012345678901234567890123456789'; i=$((i+1)); done\n",
        );
        let mut config = Config::default();
        config.paths.base = dir.path().join("runtime");
        config.paths.logs = Some(config.paths.base.join("logs"));
        config.paths.projects = Some(config.paths.base.join("projets_extraits"));
        config.paths.history_db = Some(config.paths.base.join("history.sqlite3"));
        config.server.workflow_cli = cli;
        config.server.workdir = dir.path().to_path_buf();
        config.server.max_step_log_bytes = 1024;
        config.hardware.lock_path = dir.path().join("hardware.lock");
        let state = Arc::new(WorkflowState::new());
        let runner = Runner::new(config, dir.path().join("config.toml"), state).unwrap();
        let control = runner.state().reserve_step(StepKey::Step5).unwrap();
        runner.execute_reserved(StepKey::Step5, control).await;
        let logs = dir.path().join("runtime").join("logs");
        assert!(logs.join("step_STEP5.log.1").exists());
    }

    #[tokio::test]
    async fn successful_command_updates_state_and_log() {
        let (dir, runner) = test_runner(
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
        let log = fs::read_to_string(
            dir.path()
                .join("runtime")
                .join("logs")
                .join("step_STEP5.log"),
        )
        .unwrap();
        assert!(log.contains("Succès: réduction JSON terminée"));
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
