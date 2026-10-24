//! Erreurs typées de l'orchestrateur.
//!
//! Les handlers HTTP ne doivent jamais déduire un code de statut d'un message
//! libre : les variantes ci-dessous portent la sémantique et le message
//! contractuel consommé par le frontend.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkflowServerError {
    #[error("Une séquence de workflow est en cours. Veuillez attendre.")]
    SequenceRunning,
    #[error("Une autre séquence de workflow est déjà en cours.")]
    AnotherSequenceRunning,
    #[error("Une autre étape est déjà en cours.")]
    StepRunning,
    #[error("Une étape est déjà en cours.")]
    StepRunningInSequence,
    #[error("Step '{step}' is not running")]
    StepNotRunning { step: String },
    #[error("Aucune séquence en cours d'exécution")]
    NoSequenceRunning,
    #[error("Step introuvable")]
    UnknownStep,
}

impl WorkflowServerError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::SequenceRunning
            | Self::AnotherSequenceRunning
            | Self::StepRunning
            | Self::StepRunningInSequence
            | Self::NoSequenceRunning => StatusCode::CONFLICT,
            Self::StepNotRunning { .. } => StatusCode::BAD_REQUEST,
            Self::UnknownStep => StatusCode::NOT_FOUND,
        }
    }

    pub fn is_conflict(&self) -> bool {
        self.status_code() == StatusCode::CONFLICT
    }
}

impl IntoResponse for WorkflowServerError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        (
            status,
            Json(json!({"status": "error", "message": self.to_string()})),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_conflict_when_mapped_then_409() {
        assert_eq!(
            WorkflowServerError::StepRunning.status_code(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            WorkflowServerError::SequenceRunning.status_code(),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn given_step_not_running_when_mapped_then_contract_message() {
        let error = WorkflowServerError::StepNotRunning {
            step: "STEP3".into(),
        };
        assert_eq!(error.to_string(), "Step 'STEP3' is not running");
        assert_eq!(error.status_code(), StatusCode::BAD_REQUEST);
    }
}
