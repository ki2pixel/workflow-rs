use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use workflow_tpu::ffi::{TFLITE_FLOAT32, TFLITE_INT8, TFLITE_UINT8};
use workflow_tpu::{Session, TensorSpec};

use super::image::{normalize_float_input, resize_linear, resize_nearest, RgbImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionDevice {
    Tpu,
    Cpu,
}

impl ExecutionDevice {
    pub fn parse(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "tpu" | "coral" => Ok(Self::Tpu),
            "cpu" | "xnnpack" => Ok(Self::Cpu),
            other => bail!("device STEP4 inconnu: {other} (attendu tpu ou cpu)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tpu => "tpu",
            Self::Cpu => "cpu",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelPaths {
    pub tflite: PathBuf,
    pub edgetpu: PathBuf,
    pub blazeface: PathBuf,
    pub facemesh: PathBuf,
    pub blendshapes: PathBuf,
    pub object_detector: Option<PathBuf>,
}

pub struct FaceCascade {
    pub blazeface: Session,
    pub facemesh: Session,
    /// Le modèle blendshapes tourne toujours sur CPU dans le pipeline Python.
    pub blendshapes: Session,
    pub object_detector: Option<Session>,
    pub device: ExecutionDevice,
}

impl FaceCascade {
    pub fn open(
        device: ExecutionDevice,
        paths: &ModelPaths,
        cpu_threads: i32,
        object_detection: bool,
    ) -> Result<Self> {
        let open_tpu = |path: &Path| -> Result<Session> {
            Session::open(path, &paths.tflite, &paths.edgetpu, cpu_threads)
                .with_context(|| format!("chargement du modèle TPU {}", path.display()))
        };
        let open_cpu = |path: &Path| -> Result<Session> {
            Session::open_cpu(path, &paths.tflite, cpu_threads)
                .with_context(|| format!("chargement du modèle CPU {}", path.display()))
        };

        let (blazeface, facemesh, object_detector) = match device {
            ExecutionDevice::Tpu => {
                let blazeface = open_tpu(&paths.blazeface)?;
                let facemesh = open_tpu(&paths.facemesh)?;
                let object_detector = if object_detection {
                    match &paths.object_detector {
                        Some(path) => Some(open_tpu(path)?),
                        None => None,
                    }
                } else {
                    None
                };
                (blazeface, facemesh, object_detector)
            }
            ExecutionDevice::Cpu => {
                let blazeface = open_cpu(&paths.blazeface)?;
                let facemesh = open_cpu(&paths.facemesh)?;
                let object_detector = if object_detection {
                    match &paths.object_detector {
                        Some(path) => Some(open_cpu(path)?),
                        None => None,
                    }
                } else {
                    None
                };
                (blazeface, facemesh, object_detector)
            }
        };

        let blendshapes = open_cpu(&paths.blendshapes)?;

        Ok(Self {
            blazeface,
            facemesh,
            blendshapes,
            object_detector,
            device,
        })
    }
}

pub fn input_size(session: &Session) -> Result<(usize, usize)> {
    let spec = session
        .input_specs()
        .first()
        .context("modèle sans tenseur d'entrée")?;
    input_size_from_spec(spec)
}

fn input_size_from_spec(spec: &TensorSpec) -> Result<(usize, usize)> {
    if spec.dims.len() != 4 {
        bail!(
            "tenseur d'entrée inattendu pour {}: {:?}",
            spec.name,
            spec.dims
        );
    }
    let height = usize::try_from(spec.dims[1]).context("hauteur d'entrée négative")?;
    let width = usize::try_from(spec.dims[2]).context("largeur d'entrée négative")?;
    Ok((width, height))
}

pub fn prepare_input(session: &Session, image: &RgbImage) -> Result<Vec<u8>> {
    let spec = session
        .input_specs()
        .first()
        .context("modèle sans tenseur d'entrée")?;
    let (width, height) = input_size_from_spec(spec)?;
    let resized = resize_linear(image, width, height);
    encode_input(spec, &resized)
}

pub fn prepare_input_nearest(session: &Session, image: &RgbImage) -> Result<Vec<u8>> {
    let spec = session
        .input_specs()
        .first()
        .context("modèle sans tenseur d'entrée")?;
    let (width, height) = input_size_from_spec(spec)?;
    let resized = resize_nearest(image, width, height);
    encode_input(spec, &resized)
}

fn encode_input(spec: &TensorSpec, image: &RgbImage) -> Result<Vec<u8>> {
    let expected = image.width * image.height * 3;
    if expected != spec.bytes && spec.ty != TFLITE_FLOAT32 {
        bail!(
            "taille d'image {} != tenseur {} pour {}",
            expected,
            spec.bytes,
            spec.name
        );
    }
    let mut bytes = Vec::with_capacity(spec.bytes);
    match spec.ty {
        TFLITE_FLOAT32 => normalize_float_input(image, &mut bytes),
        TFLITE_UINT8 => bytes.extend_from_slice(&image.data),
        TFLITE_INT8 => {
            let scale = if spec.scale == 0.0 { 1.0 } else { spec.scale };
            for value in &image.data {
                let quantized = (*value as f32 / scale + spec.zero_point as f32)
                    .round()
                    .clamp(-128.0, 127.0);
                bytes.push(quantized as i8 as u8);
            }
        }
        other => bail!("type d'entrée TFLite non supporté: {other}"),
    }
    if bytes.len() != spec.bytes {
        bail!(
            "entrée {}: {} octets préparés pour {} attendus",
            spec.name,
            bytes.len(),
            spec.bytes
        );
    }
    Ok(bytes)
}

pub fn run_prepared(session: &mut Session, input: &[u8]) -> Result<()> {
    session.invoke(&[input]).context("invocation TFLite")
}

pub fn run_model_linear(session: &mut Session, image: &RgbImage) -> Result<()> {
    let input = prepare_input(session, image)?;
    run_prepared(session, &input)
}

pub fn run_model_nearest(session: &mut Session, image: &RgbImage) -> Result<()> {
    let input = prepare_input_nearest(session, image)?;
    run_prepared(session, &input)
}

pub fn run_float_input(session: &mut Session, values: &[f32]) -> Result<()> {
    let spec = session
        .input_specs()
        .first()
        .context("modèle sans tenseur d'entrée")?;
    if values.len() * 4 != spec.bytes {
        bail!(
            "entrée {}: {} valeurs f32, {} octets attendus",
            spec.name,
            values.len(),
            spec.bytes
        );
    }
    let mut bytes = Vec::with_capacity(spec.bytes);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    session
        .invoke(&[bytes.as_slice()])
        .context("invocation TFLite")
}

pub fn f32_outputs(session: &Session) -> Result<Vec<Vec<f32>>> {
    let mut outputs = Vec::new();
    for (spec, bytes) in session.output_segments() {
        if spec.ty != TFLITE_FLOAT32 {
            bail!("sortie {} non float32 (type {})", spec.name, spec.ty);
        }
        if bytes.len() % 4 != 0 {
            bail!("sortie {} de taille non multiple de 4", spec.name);
        }
        let mut values = Vec::with_capacity(bytes.len() / 4);
        for chunk in bytes.chunks_exact(4) {
            values.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        outputs.push(values);
    }
    Ok(outputs)
}

pub fn f32_output(session: &Session, index: usize) -> Result<Vec<f32>> {
    let mut outputs = f32_outputs(session)?;
    if index >= outputs.len() {
        bail!("sortie TFLite {index} absente");
    }
    Ok(outputs.swap_remove(index))
}
