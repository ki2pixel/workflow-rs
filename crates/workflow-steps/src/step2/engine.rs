//! Moteur TransNetV2 ONNX Runtime.
//!
//! Une seule méthode d'inférence est exposée : le modèle ONNX Runtime. Le
//! backend CPU n'est pas un fallback implicite ; il sert aux tests et doit être
//! demandé explicitement (`device = "cpu"`).

use crate::step2::windows::{FRAME_BYTES, FRAME_CHANNELS, FRAME_HEIGHT, FRAME_WIDTH};
use anyhow::{bail, Context, Result};
use ort::execution_providers::cuda::{CUDAExecutionProvider, CuDNNConvAlgorithmSearch};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::env;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDevice {
    Cuda,
    Cpu,
}

impl ExecutionDevice {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cuda" => Ok(Self::Cuda),
            "cpu" => Ok(Self::Cpu),
            other => bail!("device inconnu: {other} (attendu: cuda ou cpu)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub model: PathBuf,
    pub device: ExecutionDevice,
    pub onnxruntime: Option<PathBuf>,
    pub cuda_lib_dirs: Vec<PathBuf>,
    pub auto_cuda_libs: bool,
    pub batch_size: usize,
    pub window: usize,
    pub padding: usize,
    pub cudnn_algo_search: bool,
    pub use_tf32: bool,
}

pub struct TransNetEngine {
    session: Session,
    output_name: String,
    batch_size: usize,
    window: usize,
    padding: usize,
}

impl TransNetEngine {
    pub fn load(config: &EngineConfig) -> Result<Self> {
        if config.window == 0 || config.padding * 2 >= config.window {
            bail!(
                "fenêtre TransNetV2 invalide: window={} padding={}",
                config.window,
                config.padding
            );
        }
        if config.batch_size == 0 {
            bail!("batch_size TransNetV2 doit être > 0");
        }
        if !config.model.is_file() {
            bail!("modèle TransNetV2 introuvable: {}", config.model.display());
        }

        configure_library_paths(config)?;
        if let Some(runtime) = &config.onnxruntime {
            ort::init_from(runtime.to_string_lossy())
                .commit()
                .with_context(|| format!("chargement ONNX Runtime: {}", runtime.display()))?;
        } else {
            ort::init()
                .commit()
                .context("initialisation ONNX Runtime système")?;
        }

        let mut builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_parallel_execution(false)?
            .with_memory_pattern(true)?;

        let providers = match config.device {
            ExecutionDevice::Cuda => {
                let cuda = CUDAExecutionProvider::default()
                    .with_device_id(0)
                    .with_conv_algorithm_search(if config.cudnn_algo_search {
                        CuDNNConvAlgorithmSearch::Exhaustive
                    } else {
                        CuDNNConvAlgorithmSearch::Heuristic
                    })
                    .with_tf32(config.use_tf32)
                    .build()
                    .error_on_failure();
                vec![cuda, CPUExecutionProvider::default().build()]
            }
            ExecutionDevice::Cpu => vec![CPUExecutionProvider::default().build()],
        };
        builder = builder
            .with_execution_providers(providers)
            .context("enregistrement des execution providers ONNX Runtime")?;

        let session = builder
            .commit_from_file(&config.model)
            .with_context(|| format!("ouverture du modèle {}", config.model.display()))?;
        let output_name = session
            .outputs
            .first()
            .map(|output| output.name.clone())
            .context("le modèle TransNetV2 n'a aucune sortie")?;
        if session.inputs.len() != 1 {
            bail!(
                "le modèle TransNetV2 doit avoir une entrée, trouvé {}",
                session.inputs.len()
            );
        }

        let mut engine = Self {
            session,
            output_name,
            batch_size: config.batch_size,
            window: config.window,
            padding: config.padding,
        };
        // Chauffe les noyaux CUDA/cuDNN avant le premier vrai lot.
        let warmup = vec![vec![0_u8; config.window * FRAME_BYTES]; config.batch_size];
        let _ = engine.infer(&warmup).context("warmup TransNetV2")?;
        Ok(engine)
    }

    pub fn infer(&mut self, windows: &[Vec<u8>]) -> Result<Vec<f32>> {
        if windows.is_empty() {
            return Ok(Vec::new());
        }
        if windows.len() > self.batch_size {
            bail!(
                "lot de {} fenêtres supérieur à batch_size={}",
                windows.len(),
                self.batch_size
            );
        }
        for (index, window) in windows.iter().enumerate() {
            if window.len() != self.window * FRAME_BYTES {
                bail!(
                    "fenêtre {index} invalide: {} octets au lieu de {}",
                    window.len(),
                    self.window * FRAME_BYTES
                );
            }
        }

        let mut input = vec![0_u8; self.batch_size * self.window * FRAME_BYTES];
        for (index, window) in windows.iter().enumerate() {
            let offset = index * self.window * FRAME_BYTES;
            input[offset..offset + window.len()].copy_from_slice(window);
        }
        let tensor = Tensor::from_array((
            [
                self.batch_size,
                self.window,
                FRAME_HEIGHT,
                FRAME_WIDTH,
                FRAME_CHANNELS,
            ],
            input,
        ))
        .context("création du tenseur d'entrée TransNetV2")?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .context("inférence TransNetV2")?;
        let value = outputs
            .get(&self.output_name)
            .with_context(|| format!("sortie {} absente", self.output_name))?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .context("lecture des probabilités TransNetV2")?;
        let last_dim = if shape.len() >= 3 {
            shape[2] as usize
        } else {
            1
        };
        if last_dim == 0 {
            bail!("sortie TransNetV2 de dernière dimension nulle");
        }

        let central = self.window - 2 * self.padding;
        let mut predictions = Vec::with_capacity(windows.len() * central);
        for batch in 0..windows.len() {
            for frame in self.padding..self.window - self.padding {
                let index = (batch * self.window + frame) * last_dim;
                let logit = *data
                    .get(index)
                    .with_context(|| format!("index de sortie TransNetV2 hors limites: {index}"))?;
                predictions.push(sigmoid(logit));
            }
        }
        Ok(predictions)
    }
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

fn configure_library_paths(config: &EngineConfig) -> Result<()> {
    let mut dirs = Vec::new();
    if let Some(runtime) = &config.onnxruntime {
        if let Some(parent) = runtime.parent() {
            dirs.push(parent.to_path_buf());
        }
        if config.auto_cuda_libs {
            dirs.extend(discover_nvidia_libs(runtime));
        }
    }
    dirs.extend(config.cuda_lib_dirs.iter().cloned());

    if config.onnxruntime.is_none() {
        return Ok(());
    }
    if dirs.is_empty() {
        return Ok(());
    }
    let variable = if cfg!(windows) {
        "PATH"
    } else {
        "LD_LIBRARY_PATH"
    };
    let mut paths = dirs.clone();
    if let Some(existing) = env::var_os(variable) {
        paths.extend(env::split_paths(&existing));
    }
    let joined = env::join_paths(paths)
        .with_context(|| format!("construction de {variable} pour ONNX Runtime"))?;
    env::set_var(variable, joined);
    if config.device == ExecutionDevice::Cuda {
        // Le loader glibc fige `LD_LIBRARY_PATH` au démarrage. Les bibliothèques
        // CUDA/cuDNN sont donc préchargées en RTLD_GLOBAL avant ONNX Runtime.
        preload_cuda_libraries(&dirs);
    }
    Ok(())
}

#[cfg(unix)]
fn preload_cuda_libraries(dirs: &[PathBuf]) {
    use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};
    use std::collections::HashSet;

    let mut candidates = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        let is_capi = dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == "capi");
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if is_capi && !name.starts_with("libonnxruntime_providers_shared") {
                continue;
            }
            if name.contains(".so") {
                candidates.push(path);
            }
        }
    }
    candidates.sort();

    let mut loaded = HashSet::new();
    let mut remaining = candidates;
    loop {
        let mut next = Vec::new();
        let mut progress = false;
        for path in remaining {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            if loaded.contains(&name) {
                continue;
            }
            let result = unsafe { Library::open(Some(&path), RTLD_NOW | RTLD_GLOBAL) };
            match result {
                Ok(library) => {
                    std::mem::forget(library);
                    loaded.insert(name);
                    progress = true;
                }
                Err(error) => {
                    next.push(path);
                    tracing::debug!(
                        library = %name,
                        error = %error,
                        "préchargement CUDA différé"
                    );
                }
            }
        }
        if !progress || next.is_empty() {
            break;
        }
        remaining = next;
    }
    if loaded.is_empty() {
        tracing::warn!("aucune bibliothèque CUDA préchargée pour ONNX Runtime");
    }
}

#[cfg(not(unix))]
fn preload_cuda_libraries(_dirs: &[PathBuf]) {}

fn discover_nvidia_libs(runtime: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for ancestor in runtime.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str()) != Some("site-packages") {
            continue;
        }
        let nvidia = ancestor.join("nvidia");
        let Ok(entries) = std::fs::read_dir(&nvidia) else {
            continue;
        };
        for entry in entries.flatten() {
            let lib = entry.path().join("lib");
            if lib.is_dir() {
                dirs.push(lib);
            }
        }
        break;
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_parser_is_strict() {
        assert_eq!(
            ExecutionDevice::parse("cuda").unwrap(),
            ExecutionDevice::Cuda
        );
        assert_eq!(ExecutionDevice::parse("CPU").unwrap(), ExecutionDevice::Cpu);
        assert!(ExecutionDevice::parse("auto").is_err());
    }

    #[test]
    fn sigmoid_is_stable_for_extreme_logits() {
        assert!((sigmoid(1000.0) - 1.0).abs() < 1e-6);
        assert!(sigmoid(-1000.0) < 1e-6);
    }
}
