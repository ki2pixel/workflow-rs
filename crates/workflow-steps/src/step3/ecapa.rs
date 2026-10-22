//! Extraction ECAPA-TDNN via ONNX Runtime CPU.

use anyhow::{bail, Context, Result};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::OnceLock;

use crate::step3::mel::{ECAPA_MELS, ECAPA_TARGET_FRAMES};

static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub struct EcapaEngine {
    session: Session,
    input_name: String,
    output_name: String,
}

impl EcapaEngine {
    pub fn load(model: &Path, onnxruntime: Option<&Path>) -> Result<Self> {
        if !model.is_file() {
            bail!("modèle ECAPA introuvable: {}", model.display());
        }
        let initialized = ORT_INIT.get_or_init(|| {
            if let Some(runtime) = onnxruntime {
                ort::init_from(runtime.to_string_lossy().to_string())
                    .commit()
                    .map(|_| ())
                    .map_err(|error| {
                        format!("chargement ONNX Runtime {}: {error}", runtime.display())
                    })
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

        let builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_parallel_execution(false)?
            .with_memory_pattern(false)?
            .with_execution_providers([CPUExecutionProvider::default().build()])
            .context("enregistrement du provider CPU ECAPA")?;
        let session = builder
            .commit_from_file(model)
            .with_context(|| format!("ouverture du modèle ECAPA {}", model.display()))?;
        let input_name = session
            .inputs
            .first()
            .context("ECAPA sans entrée ONNX")?
            .name
            .clone();
        let output_name = session
            .outputs
            .first()
            .context("ECAPA sans sortie ONNX")?
            .name
            .clone();
        Ok(Self {
            session,
            input_name,
            output_name,
        })
    }

    /// Infère un lot de spectrogrammes `(batch, 80, 300)`.
    pub fn infer(&mut self, log_mels: &[f32], batch_size: usize) -> Result<Vec<f32>> {
        if batch_size == 0 {
            return Ok(Vec::new());
        }
        let expected = batch_size * ECAPA_MELS * ECAPA_TARGET_FRAMES;
        if log_mels.len() != expected {
            bail!(
                "ECAPA: {} valeurs pour {expected} attendues",
                log_mels.len()
            );
        }
        let tensor = Tensor::from_array((
            [batch_size, ECAPA_MELS, ECAPA_TARGET_FRAMES],
            log_mels.to_vec(),
        ))
        .context("création du tenseur ECAPA")?;
        let outputs = self
            .session
            .run(ort::inputs![self.input_name.as_str() => tensor])
            .context("inférence ECAPA")?;
        let value = outputs
            .get(&self.output_name)
            .with_context(|| format!("sortie ECAPA {} absente", self.output_name))?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .context("lecture des embeddings ECAPA")?;
        let expected_values = batch_size * 192;
        if data.len() != expected_values {
            bail!(
                "ECAPA: sortie de {} valeurs au lieu de {expected_values} (shape {:?})",
                data.len(),
                shape
            );
        }
        Ok(data.to_vec())
    }
}
