//! Initialisation ONNX Runtime partagée par YuNet et PP-OCR.
//!
//! ONNX Runtime ne s'initialise qu'une fois par processus : les contrôles
//! partagent donc le même `OnceLock`. Le runtime est explicite (`--onnxruntime`)
//! ou celui du système. Les sessions restent CPU, avec un nombre de threads
//! intra-op configurable pour le mode lot : le QA tourne à côté du pipeline,
//! sans verrou matériel ni GPU.

use anyhow::{bail, Context, Result};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use std::path::Path;
use std::sync::OnceLock;

pub(crate) fn init_ort(onnxruntime: Option<&Path>) -> Result<()> {
    static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();
    let initialized = ORT_INIT.get_or_init(|| {
        if let Some(runtime) = onnxruntime {
            ort::init_from(runtime.to_string_lossy().to_string())
                .commit()
                .map(|_| ())
                .map_err(|error| format!("chargement ONNX Runtime {}: {error}", runtime.display()))
        } else {
            ort::init()
                .commit()
                .map(|_| ())
                .map_err(|error| format!("initialisation ONNX Runtime système: {error}"))
        }
    });
    if let Err(message) = initialized {
        bail!("{message}");
    }
    Ok(())
}

pub(crate) fn cpu_session(
    model: &Path,
    onnxruntime: Option<&Path>,
    intra_threads: Option<usize>,
) -> Result<Session> {
    init_ort(onnxruntime)?;
    let mut builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_parallel_execution(false)?
        .with_memory_pattern(false)?
        .with_execution_providers([CPUExecutionProvider::default().build()])
        .context("enregistrement du provider CPU ONNX")?;
    if let Some(threads) = intra_threads {
        builder = builder
            .with_intra_threads(threads.max(1))
            .context("configuration des threads intra-op ONNX")?;
    }
    builder
        .commit_from_file(model)
        .with_context(|| format!("ouverture du modèle ONNX {}", model.display()))
}
