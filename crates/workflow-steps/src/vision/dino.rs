//! Vérificateur visuel DINOv2 ViT-S/14 des candidats OCR de la bande haute.
//!
//! Le contrôle logos texte ne voit que les candidats dont l'OCR est lisible.
//! Ce second étage reprend les boîtes qui passent la géométrie du pipeline
//! (hauteur 18–220 px, largeur ≥ 30 px, aspect ≥ 1,2), les recadre dans la
//! bande haute, puis les note avec un probe linéaire entraîné sur les assets
//! dev de la galerie synthétique. Le modèle DINOv2 reste hors du dépôt ; le
//! probe (384 poids + biais + seuil) est versionné.
//!
//! Le préprocessing reproduit le PoC Python : pad carré avec la moyenne des
//! bords, resize bicubique 256×256, crop central 224×224, normalisation
//! ImageNet. Le score est la sigmoïde de `w·CLS + b`, le CLS étant normalisé
//! L2. Aucune écriture : le module retourne des scores.

use crate::vision::frame::BgrFrame;
use crate::vision::hash::sha256_file;
use crate::vision::inference::cpu_session;
use crate::vision::ModelInfo;
use anyhow::{bail, ensure, Context, Result};
use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;
use std::path::PathBuf;

pub const DINO_MODEL_FILENAME: &str = "dinov2_vits14_quantized.onnx";
pub const DINO_PROBE_FILENAME: &str = "dinov2_logo_probe.json";
pub const DINO_LICENSE: &str = "Apache-2.0";
pub const DINO_THRESHOLD: f32 = 0.50;
pub const DINO_MIN_TIMES: usize = 3;
/// Le contrôle texte garde 18–180 px ; le vérificateur visuel accepte jusqu'à
/// 220 px pour les logos plus hauts (Les 100 à l'échelle 1,0), sans laisser
/// passer les boîtes pleine frame.
pub const DINO_MAX_BOX_HEIGHT: i64 = 220;
pub const DINO_MIN_BOX_HEIGHT: i64 = 18;
pub const DINO_MIN_BOX_WIDTH: i64 = 30;
pub const DINO_MIN_ASPECT: f64 = 1.2;

const INPUT_SIZE: usize = 224;
const RESIZE_SIZE: usize = 256;
const BATCH_SIZE: usize = 12;
const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

#[derive(Debug, Clone)]
pub struct DinoOptions {
    pub model: PathBuf,
    pub probe: PathBuf,
    pub onnxruntime: Option<PathBuf>,
    pub intra_threads: Option<usize>,
}

impl Default for DinoOptions {
    fn default() -> Self {
        Self {
            model: PathBuf::new(),
            probe: PathBuf::new(),
            onnxruntime: None,
            intra_threads: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ProbeFile {
    version: u32,
    input_size: usize,
    threshold: f32,
    bias: f32,
    weights: Vec<f32>,
}

pub(crate) struct DinoEngine {
    session: Session,
    weights: Vec<f32>,
    bias: f32,
    threshold: f32,
    model: ModelInfo,
}

impl DinoEngine {
    pub(crate) fn load(options: &DinoOptions) -> Result<Self> {
        if !options.model.is_file() {
            bail!("modèle DINOv2 introuvable: {}", options.model.display());
        }
        if !options.probe.is_file() {
            bail!("probe DINOv2 introuvable: {}", options.probe.display());
        }
        let raw = std::fs::read_to_string(&options.probe)
            .with_context(|| format!("lecture du probe DINOv2 {}", options.probe.display()))?;
        let probe: ProbeFile = serde_json::from_str(&raw)
            .with_context(|| format!("probe DINOv2 invalide: {}", options.probe.display()))?;
        ensure!(probe.version == 1, "version de probe DINOv2 inattendue");
        ensure!(
            probe.input_size == INPUT_SIZE,
            "taille d'entrée DINOv2 inattendue: {}",
            probe.input_size
        );
        ensure!(
            probe.weights.len() == 384,
            "probe DINOv2: {} poids au lieu de 384",
            probe.weights.len()
        );
        ensure!(
            probe.threshold.is_finite() && (0.0..=1.0).contains(&probe.threshold),
            "seuil DINOv2 invalide: {}",
            probe.threshold
        );
        ensure!(
            (probe.threshold - DINO_THRESHOLD).abs() < 1e-6,
            "seuil DINOv2 {} différent du seuil attendu {}",
            probe.threshold,
            DINO_THRESHOLD
        );
        ensure!(probe.bias.is_finite(), "biais DINOv2 non fini");
        ensure!(
            probe.weights.iter().all(|value| value.is_finite()),
            "poids DINOv2 non finis"
        );

        let session = cpu_session(
            &options.model,
            options.onnxruntime.as_deref(),
            options.intra_threads,
        )
        .with_context(|| format!("ouverture du modèle DINOv2 {}", options.model.display()))?;
        ensure!(
            session
                .inputs
                .iter()
                .any(|input| input.name == "pixel_values"),
            "entrée pixel_values absente du modèle DINOv2"
        );
        ensure!(
            session
                .outputs
                .iter()
                .any(|output| output.name == "last_hidden_state"),
            "sortie last_hidden_state absente du modèle DINOv2"
        );

        let model = ModelInfo {
            name: options
                .model
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(DINO_MODEL_FILENAME)
                .to_string(),
            sha256: sha256_file(&options.model)?,
            license: DINO_LICENSE.into(),
        };
        Ok(Self {
            session,
            weights: probe.weights,
            bias: probe.bias,
            threshold: probe.threshold,
            model,
        })
    }

    pub(crate) fn model_info(&self) -> &ModelInfo {
        &self.model
    }

    pub(crate) fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Note chaque boîte de `boxes` (coordonnées de la frame) dans `frame`.
    pub(crate) fn score_boxes(&mut self, frame: &BgrFrame, boxes: &[[i64; 4]]) -> Result<Vec<f32>> {
        if boxes.is_empty() {
            return Ok(Vec::new());
        }
        let mut scores = Vec::with_capacity(boxes.len());
        for chunk in boxes.chunks(BATCH_SIZE) {
            let mut input = Vec::with_capacity(chunk.len() * 3 * INPUT_SIZE * INPUT_SIZE);
            for bbox in chunk {
                let crop = crop_box(frame, *bbox)?;
                input.extend(preprocess(&crop));
            }
            let tensor = Tensor::from_array(([chunk.len(), 3, INPUT_SIZE, INPUT_SIZE], input))
                .context("création du tenseur DINOv2")?;
            let outputs = self
                .session
                .run(ort::inputs![tensor])
                .context("inférence DINOv2")?;
            let (shape, data) = outputs
                .get("last_hidden_state")
                .context("sortie DINOv2 last_hidden_state absente")?
                .try_extract_tensor::<f32>()
                .context("lecture de la sortie DINOv2")?;
            ensure!(shape.len() == 3, "sortie DINOv2 inattendue: {:?}", shape);
            let sequence = shape[1] as usize;
            let hidden = shape[2] as usize;
            ensure!(
                hidden == self.weights.len(),
                "DINOv2: dimension cachée {} au lieu de {}",
                hidden,
                self.weights.len()
            );
            ensure!(
                data.len() >= chunk.len() * sequence * hidden,
                "DINOv2: {} valeurs pour {}×{}×{}",
                data.len(),
                chunk.len(),
                sequence,
                hidden
            );
            for batch in 0..chunk.len() {
                let start = batch * sequence * hidden;
                let cls = &data[start..start + hidden];
                let norm = cls.iter().map(|value| value * value).sum::<f32>().sqrt();
                let norm = norm.max(1e-9);
                let mut logit = self.bias;
                for (value, weight) in cls.iter().zip(self.weights.iter()) {
                    logit += (value / norm) * weight;
                }
                scores.push(sigmoid(logit));
            }
        }
        Ok(scores)
    }
}

fn crop_box(frame: &BgrFrame, bbox: [i64; 4]) -> Result<BgrFrame> {
    ensure!(frame.width > 0 && frame.height > 0, "frame DINOv2 vide");
    let x = bbox[0].clamp(0, frame.width as i64 - 1) as usize;
    let y = bbox[1].clamp(0, frame.height as i64 - 1) as usize;
    let width = bbox[2].clamp(1, frame.width as i64 - x as i64).max(1) as usize;
    let height = bbox[3].clamp(1, frame.height as i64 - y as i64).max(1) as usize;
    Ok(frame.crop(x, y, width, height))
}

/// Pad carré avec la moyenne des pixels de bord, comme le PoC Python.
fn square_pad(frame: &BgrFrame) -> BgrFrame {
    let side = frame.width.max(frame.height);
    if frame.width == side && frame.height == side {
        return frame.clone();
    }
    let mut sums = [0u64; 3];
    let mut count = 0u64;
    for x in 0..frame.width {
        for y in [0, frame.height - 1] {
            let pixel = frame.pixel(x, y);
            for channel in 0..3 {
                sums[channel] += u64::from(pixel[channel]);
            }
            count += 1;
        }
    }
    for y in 0..frame.height {
        for x in [0, frame.width - 1] {
            let pixel = frame.pixel(x, y);
            for channel in 0..3 {
                sums[channel] += u64::from(pixel[channel]);
            }
            count += 1;
        }
    }
    let mean = [
        (sums[0] / count) as u8,
        (sums[1] / count) as u8,
        (sums[2] / count) as u8,
    ];
    let mut data = Vec::with_capacity(side * side * 3);
    for _ in 0..side * side {
        data.extend_from_slice(&mean);
    }
    let top = (side - frame.height) / 2;
    let left = (side - frame.width) / 2;
    for y in 0..frame.height {
        let target = ((top + y) * side + left) * 3;
        let source = y * frame.width * 3;
        data[target..target + frame.width * 3]
            .copy_from_slice(&frame.data[source..source + frame.width * 3]);
    }
    BgrFrame {
        width: side,
        height: side,
        data,
    }
}

/// Bicubique Catmull-Rom/OpenCV (A = -0,75), coordonnées demi-pixel.
fn resize_bicubic(source: &BgrFrame, width: usize, height: usize) -> BgrFrame {
    if source.width == width && source.height == height {
        return source.clone();
    }
    let mut data = vec![0u8; width * height * 3];
    let scale_x = source.width as f32 / width as f32;
    let scale_y = source.height as f32 / height as f32;
    for y in 0..height {
        let source_y = (y as f32 + 0.5) * scale_y - 0.5;
        let base_y = source_y.floor() as i64;
        for x in 0..width {
            let source_x = (x as f32 + 0.5) * scale_x - 0.5;
            let base_x = source_x.floor() as i64;
            let mut sum = [0f32; 3];
            let mut weight_sum = 0f32;
            for offset_y in -1i64..=2 {
                let weight_y = cubic(source_y - (base_y + offset_y) as f32);
                if weight_y == 0.0 {
                    continue;
                }
                let sample_y = (base_y + offset_y).clamp(0, source.height as i64 - 1) as usize;
                for offset_x in -1i64..=2 {
                    let weight_x = cubic(source_x - (base_x + offset_x) as f32);
                    if weight_x == 0.0 {
                        continue;
                    }
                    let sample_x = (base_x + offset_x).clamp(0, source.width as i64 - 1) as usize;
                    let weight = weight_x * weight_y;
                    let pixel = source.pixel(sample_x, sample_y);
                    for channel in 0..3 {
                        sum[channel] += f32::from(pixel[channel]) * weight;
                    }
                    weight_sum += weight;
                }
            }
            let offset = (y * width + x) * 3;
            for channel in 0..3 {
                let value = if weight_sum.abs() > 1e-9 {
                    sum[channel] / weight_sum
                } else {
                    0.0
                };
                data[offset + channel] = value.clamp(0.0, 255.0).round() as u8;
            }
        }
    }
    BgrFrame {
        width,
        height,
        data,
    }
}

fn cubic(value: f32) -> f32 {
    const A: f32 = -0.75;
    let x = value.abs();
    if x <= 1.0 {
        ((A + 2.0) * x - (A + 3.0)) * x * x + 1.0
    } else if x < 2.0 {
        (((x - 5.0) * x + 8.0) * x - 4.0) * A
    } else {
        0.0
    }
}

fn preprocess(crop: &BgrFrame) -> Vec<f32> {
    let square = square_pad(crop);
    let resized = resize_bicubic(&square, RESIZE_SIZE, RESIZE_SIZE);
    let margin = (RESIZE_SIZE - INPUT_SIZE) / 2;
    let plane = INPUT_SIZE * INPUT_SIZE;
    let mut input = vec![0f32; 3 * plane];
    for y in 0..INPUT_SIZE {
        for x in 0..INPUT_SIZE {
            let pixel = resized.pixel(x + margin, y + margin);
            let rgb = [pixel[2], pixel[1], pixel[0]];
            let offset = y * INPUT_SIZE + x;
            for channel in 0..3 {
                input[channel * plane + offset] = (f32::from(rgb[channel]) / 255.0
                    - IMAGENET_MEAN[channel])
                    / IMAGENET_STD[channel];
            }
        }
    }
    input
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

/// Géométrie du vérificateur visuel, distincte des filtres texte.
pub fn visual_geometry_ok(width: i64, height: i64, aspect: f64) -> bool {
    (DINO_MIN_BOX_HEIGHT..=DINO_MAX_BOX_HEIGHT).contains(&height)
        && width >= DINO_MIN_BOX_WIDTH
        && aspect >= DINO_MIN_ASPECT
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: usize, height: usize, pixel: [u8; 3]) -> BgrFrame {
        BgrFrame::new(width, height, pixel.repeat(width * height)).unwrap()
    }

    #[test]
    fn geometry_accepts_large_logo_but_rejects_full_frame() {
        assert!(visual_geometry_ok(267, 204, 267.0 / 204.0));
        assert!(visual_geometry_ok(180, 120, 1.5));
        assert!(!visual_geometry_ok(1080, 613, 1080.0 / 613.0));
        assert!(!visual_geometry_ok(20, 50, 0.4));
        assert!(!visual_geometry_ok(100, 200, 0.5));
    }

    #[test]
    fn square_pad_keeps_pixels_centered() {
        let source = frame(4, 2, [10, 20, 30]);
        let padded = square_pad(&source);
        assert_eq!(padded.width, 4);
        assert_eq!(padded.height, 4);
        assert_eq!(padded.pixel(0, 1), [10, 20, 30]);
        assert_eq!(padded.pixel(3, 2), [10, 20, 30]);
    }

    #[test]
    fn bicubic_preserves_a_uniform_image() {
        let source = frame(4, 3, [1, 2, 3]);
        let resized = resize_bicubic(&source, 8, 8);
        assert_eq!(resized.width, 8);
        assert_eq!(resized.height, 8);
        assert_eq!(resized.pixel(0, 0), [1, 2, 3]);
        assert_eq!(resized.pixel(7, 7), [1, 2, 3]);
    }

    #[test]
    fn crop_clamps_to_frame() {
        let source = frame(4, 4, [7, 8, 9]);
        let crop = crop_box(&source, [3, 3, 100, 100]).unwrap();
        assert_eq!(crop.width, 1);
        assert_eq!(crop.height, 1);
        assert_eq!(crop.pixel(0, 0), [7, 8, 9]);
    }
}
