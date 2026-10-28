//! Post-traitement déterministe après STEP8 (A5 + A6).
//!
//! Le module lit le `.ecproj` brut, écrit un second projet `_assist.ecproj`
//! (copie du brut si aucune décision n'est applicable) et publie trois
//! artefacts additifs : `post.json`, `review.json`, `audit.json`.

pub mod a1;
pub mod a2;
pub mod a3;
pub mod a4;
pub mod a6;
pub mod detect;
pub mod ecproj;
pub mod model;
pub mod run;

pub use model::{PostProjectOutcome, PostRunSummary, PostSummary};
pub use run::{run_post_dir, run_post_for_folders, run_post_project, PostOptions};
