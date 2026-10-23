//! Exécution séquentielle et résultat global.

use crate::catalog::StepKey;
use crate::runner::Runner;
use crate::state::now_iso;
use std::sync::Arc;

pub async fn run_sequence(runner: Arc<Runner>, steps: Vec<StepKey>, sequence_type: String) {
    let started_at = now_iso();
    let mut summary: Vec<(String, String, String)> = Vec::new();
    let mut success = true;

    if steps.is_empty() {
        runner.state().complete_sequence(
            true,
            format!("Séquence {sequence_type} Terminée avec succès. Détails: "),
            &sequence_type,
        );
    } else {
        for step in &steps {
            if runner.state().sequence_stop_requested() {
                success = false;
                break;
            }
            let control = match runner.state().begin_sequence_step(*step) {
                Ok(control) => control,
                Err(error) => {
                    summary.push((
                        step.display_name().into(),
                        format!("Erreur ({error})"),
                        "0s".into(),
                    ));
                    success = false;
                    break;
                }
            };
            let outcome = runner.execute_reserved(*step, control).await;
            summary.push((
                step.display_name().into(),
                if outcome.success {
                    "Réussie".into()
                } else {
                    format!("Échouée ({})", outcome.status)
                },
                outcome.duration.clone(),
            ));
            if !outcome.success {
                success = false;
                break;
            }
        }

        if runner.state().sequence_stop_requested() {
            success = false;
        }
        let details = summary
            .iter()
            .map(|(name, status, duration)| format!("{name}: {status} ({duration})"))
            .collect::<Vec<_>>()
            .join(" | ");
        let overall = if success {
            "Terminée avec succès"
        } else {
            "Terminée avec erreurs"
        };
        let message = format!("Séquence {sequence_type} {overall}. Détails: {details}");
        runner
            .state()
            .complete_sequence(success, message, &sequence_type);
    }

    let manual_stop = runner.state().sequence_stop_requested();
    let final_success = success && !manual_stop;
    let message = runner
        .state()
        .sequence_outcome()
        .message
        .unwrap_or_else(|| format!("Séquence {sequence_type} terminée"));
    if let Err(error) = runner.db().record_sequence(
        &sequence_type,
        &started_at,
        &now_iso(),
        final_success,
        &message,
    ) {
        tracing::warn!("enregistrement de la séquence impossible: {error:#}");
    }
}

/// Utilisé par les tests et le mode CLI pour un enchaînement déjà validé.
pub async fn execute_steps(
    runner: Arc<Runner>,
    steps: Vec<StepKey>,
    sequence_type: &str,
) -> Result<(), String> {
    runner.state().start_sequence(sequence_type)?;
    run_sequence(runner, steps, sequence_type.into()).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WorkflowState;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use workflow_core::Config;

    fn runner_with_script(dir: &Path, script: &str) -> Arc<Runner> {
        let cli = dir.join("fake-workflow-cli");
        fs::write(&cli, script).unwrap();
        let mut permissions = fs::metadata(&cli).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&cli, permissions).unwrap();
        let mut config = Config::default();
        config.paths.base = dir.join("runtime");
        config.paths.projects = Some(config.paths.base.join("projets_extraits"));
        config.paths.logs = Some(config.paths.base.join("logs"));
        config.paths.history_db = Some(config.paths.base.join("history.sqlite3"));
        config.server.workflow_cli = cli;
        config.server.workdir = dir.to_path_buf();
        config.hardware.lock_path = dir.join("hardware.lock");
        Runner::new(
            config,
            dir.join("config.toml"),
            Arc::new(WorkflowState::new()),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn sequence_stops_on_first_failure() {
        let dir = tempfile::tempdir().unwrap();
        let runner = runner_with_script(
            dir.path(),
            "#!/bin/sh\necho 'TOTAL_JSON_TO_REDUCE: 1'\nexit 3\n",
        );
        execute_steps(
            runner.clone(),
            vec![StepKey::Step5, StepKey::Step6],
            "Custom",
        )
        .await
        .unwrap();
        let outcome = runner.state().sequence_outcome();
        assert_eq!(outcome.status, "error");
        assert_eq!(
            runner
                .state()
                .step_status(StepKey::Step5, false)
                .unwrap()
                .status,
            "failed"
        );
        assert_eq!(
            runner
                .state()
                .step_status(StepKey::Step6, false)
                .unwrap()
                .status,
            "idle"
        );
        assert_eq!(runner.db().sequence_run_count().unwrap(), 1);
    }
}
