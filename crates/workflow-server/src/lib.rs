//! Orchestrateur HTTP du pipeline Rust.
//!
//! Le serveur ne contient aucune implémentation d'étape : il sérialise les
//! exécutions, expose le contrat de polling et délègue chaque étape à
//! `workflow-cli`.

pub mod catalog;
pub mod db;
pub mod error;
pub mod http;
pub mod progress;
pub mod runner;
pub mod sequence;
pub mod state;

pub use catalog::StepKey;
pub use http::{router, AppState};
pub use runner::Runner;
pub use state::WorkflowState;
