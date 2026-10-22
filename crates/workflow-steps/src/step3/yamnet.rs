//! Inférence YAMNet Split sur l'Edge TPU (ou CPU pour les tests explicites).

use crate::step3::mel::{YAMNET_MELS, YAMNET_PATCH_FRAMES};
use crate::step3::Step3Device;
use anyhow::{bail, Context, Result};
use std::path::Path;
use workflow_tpu::ffi::TFLITE_INT8;
use workflow_tpu::{Session, TensorSpec};

pub struct YamnetEngine {
    session: Session,
    output_index: usize,
    input_scale: f64,
    input_zero_point: f64,
    output_scale: f32,
    output_zero_point: f32,
}

impl YamnetEngine {
    pub fn load(
        device: Step3Device,
        model: &Path,
        cpu_model: &Path,
        tflite: &Path,
        edgetpu: &Path,
        cpu_threads: i32,
    ) -> Result<Self> {
        let session = match device {
            Step3Device::Tpu => Session::open(model, tflite, edgetpu, cpu_threads)
                .with_context(|| format!("chargement YAMNet Edge TPU {}", model.display()))?,
            Step3Device::Cpu => Session::open_cpu(cpu_model, tflite, cpu_threads)
                .with_context(|| format!("chargement YAMNet CPU {}", cpu_model.display()))?,
        };
        let input = session
            .input_specs()
            .first()
            .context("YAMNet sans tenseur d'entrée")?;
        validate_input(input)?;
        let (output_index, output) = session
            .output_specs()
            .iter()
            .enumerate()
            .find(|(_, spec)| spec.dims.last().is_some_and(|dim| *dim == 521))
            .context("YAMNet: sortie 521 classes absente")?;
        if output.ty != TFLITE_INT8 {
            bail!("YAMNet: sortie de scores non int8 ({})", output.ty);
        }
        let input_scale = f64::from(input.scale);
        let input_zero_point = f64::from(input.zero_point);
        let output_scale = output.scale;
        let output_zero_point = output.zero_point as f32;

        Ok(Self {
            session,
            output_index,
            input_scale,
            input_zero_point,
            output_scale,
            output_zero_point,
        })
    }

    pub fn edgetpu_version(&self) -> Option<String> {
        self.session.edgetpu_version()
    }

    pub fn device_paths(&self) -> Vec<String> {
        self.session.device_paths()
    }

    /// Exécute un patch `(96, 64)` déjà calculé en float32 et retourne la
    /// probabilité de la classe 0 (parole).
    pub fn infer_patch(&mut self, patch: &[f32]) -> Result<f32> {
        let expected = YAMNET_PATCH_FRAMES * YAMNET_MELS;
        if patch.len() != expected {
            bail!(
                "patch YAMNet: {} valeurs au lieu de {expected}",
                patch.len()
            );
        }
        let mut input = Vec::with_capacity(expected);
        if self.input_scale <= 0.0 {
            for value in patch {
                input.push(*value as i8 as u8);
            }
        } else {
            for value in patch {
                let quantized = (f64::from(*value) / self.input_scale + self.input_zero_point)
                    .round_ties_even()
                    .clamp(-128.0, 127.0);
                input.push(quantized as i8 as u8);
            }
        }
        self.session
            .invoke(&[input.as_slice()])
            .context("invocation YAMNet")?;
        let segments = self.session.output_segments();
        let (_, bytes) = segments
            .get(self.output_index)
            .context("YAMNet: sortie absente après invocation")?;
        let quantized = *bytes.first().context("YAMNet: score vide")?;
        let raw = quantized as i8 as f32;
        Ok((raw - self.output_zero_point) * self.output_scale)
    }
}

fn validate_input(spec: &TensorSpec) -> Result<()> {
    let expected = [1, YAMNET_PATCH_FRAMES as i32, YAMNET_MELS as i32];
    if spec.dims != expected {
        bail!("YAMNet: entrée {:?} au lieu de {:?}", spec.dims, expected);
    }
    if spec.ty != TFLITE_INT8 {
        bail!("YAMNet: entrée non int8 ({})", spec.ty);
    }
    if spec.bytes != YAMNET_PATCH_FRAMES * YAMNET_MELS {
        bail!(
            "YAMNet: entrée de {} octets au lieu de {}",
            spec.bytes,
            YAMNET_PATCH_FRAMES * YAMNET_MELS
        );
    }
    Ok(())
}
