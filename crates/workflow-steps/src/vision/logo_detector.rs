//! Détecteur de logos A2 : YOLOX-Nano ONNX Runtime, sans sidecar Python.
//!
//! Le lot B1 a validé un modèle 1 classe (`logo`) à 416 px sur le GPU :
//! rappel D-060 95,5 %, présence lower-third 96,7 %, open-set 90,3 % et
//! 8,01 ms p95. Ce module expose l'inférence brute ; le post-traitement
//! décide de l'annotation et ne modifie jamais une action A2.
//!
//! Le prétraitement reproduit l'évaluation Python : BGR 0–255, letterbox
//! 416×416 avec remplissage 114 (interpolation bilinéaire demi-pixel), puis
//! NCHW. La sortie YOLOX `[1, N, 5 + C]` est décodée, filtrée par score,
//! NMS classe-aware, puis remappée en pixels source. Le mode ROI permet de
//! recadrer la bande basse ou le coin bas-gauche avant letterbox ; le pilote
//! B2 utilise `full`, seul point de fonctionnement validé sur les deux jeux.

use crate::vision::frame::BgrFrame;
use crate::vision::hash::sha256_file;
use crate::vision::inference::init_ort;
use anyhow::{bail, ensure, Context, Result};
use ort::execution_providers::cuda::{CUDAExecutionProvider, CuDNNConvAlgorithmSearch};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use std::cmp::Ordering;
use std::path::PathBuf;

/// Taille d'entrée validée par le lot B1.
pub const DEFAULT_INPUT_SIZE: u32 = 416;
/// Seuil de confiance du point de fonctionnement B1/B2.
pub const DEFAULT_CONF_THRESHOLD: f32 = 0.30;
/// Seuil NMS du point de fonctionnement B1/B2.
pub const DEFAULT_IOU_THRESHOLD: f32 = 0.65;

/// Périphérique d'inférence. `Cuda` échoue explicitement si le provider
/// CUDA n'est pas disponible : pas de repli CPU silencieux.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExecutionDevice {
    #[default]
    Cuda,
    Cpu,
}

impl ExecutionDevice {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cuda" | "gpu" => Ok(Self::Cuda),
            "cpu" => Ok(Self::Cpu),
            other => bail!("périphérique de détection inconnu: {other} (attendu: cuda ou cpu)"),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Cpu => "cpu",
        }
    }
}

/// Recadrage appliqué avant le letterbox. `Full` est le point validé B1 ;
/// `Bottom` et `BottomLeft` sont disponibles pour les essais de résolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Roi {
    #[default]
    Full,
    Bottom,
    BottomLeft,
}

impl Roi {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "full" | "frame" => Ok(Self::Full),
            "bottom" | "lower" => Ok(Self::Bottom),
            "bottom_left" | "bottom-left" | "corner" => Ok(Self::BottomLeft),
            other => {
                bail!("ROI de détection inconnue: {other} (attendu: full, bottom ou bottom_left)")
            }
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Bottom => "bottom",
            Self::BottomLeft => "bottom_left",
        }
    }

    /// Fenêtre source du recadrage. La bande basse commence à 55 % de la
    /// hauteur ; le coin bas-gauche garde 50 % de la largeur.
    pub(crate) fn window(self, width: usize, height: usize) -> RoiWindow {
        let width = width.max(1);
        let height = height.max(1);
        match self {
            Self::Full => RoiWindow {
                x: 0,
                y: 0,
                width,
                height,
            },
            Self::Bottom => {
                let y = ((height as f64) * 0.55).round() as usize;
                let y = y.min(height - 1);
                RoiWindow {
                    x: 0,
                    y,
                    width,
                    height: height - y,
                }
            }
            Self::BottomLeft => {
                let y = ((height as f64) * 0.55).round() as usize;
                let y = y.min(height - 1);
                let w = ((width as f64) * 0.50).round() as usize;
                let w = w.clamp(1, width);
                RoiWindow {
                    x: 0,
                    y,
                    width: w,
                    height: height - y,
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RoiWindow {
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

/// Configuration du détecteur, indépendante de `post` et de la config TOML.
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    pub model: PathBuf,
    pub device: ExecutionDevice,
    pub onnxruntime: Option<PathBuf>,
    pub input_size: u32,
    pub conf_threshold: f32,
    pub iou_threshold: f32,
    pub class_names: Vec<String>,
    pub roi: Roi,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            model: PathBuf::new(),
            device: ExecutionDevice::Cuda,
            onnxruntime: None,
            input_size: DEFAULT_INPUT_SIZE,
            conf_threshold: DEFAULT_CONF_THRESHOLD,
            iou_threshold: DEFAULT_IOU_THRESHOLD,
            class_names: vec!["logo".into()],
            roi: Roi::Full,
        }
    }
}

impl DetectorConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.model.as_os_str().is_empty(),
            "modèle de détection vide"
        );
        ensure!(
            matches!(self.input_size, 416 | 640),
            "taille d'entrée {} invalide (attendu: 416 ou 640)",
            self.input_size
        );
        ensure!(
            (0.0..=1.0).contains(&self.conf_threshold),
            "seuil de confiance {} hors [0, 1]",
            self.conf_threshold
        );
        ensure!(
            (0.0..=1.0).contains(&self.iou_threshold),
            "seuil NMS {} hors [0, 1]",
            self.iou_threshold
        );
        ensure!(
            !self.class_names.is_empty(),
            "aucune classe de détection déclarée"
        );
        Ok(())
    }
}

/// Détection en pixels de la frame source, bbox `[x1, y1, x2, y2]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub class_id: usize,
    pub label: String,
    pub score: f32,
    pub bbox: [f32; 4],
}

/// Session ONNX Runtime et métadonnées du modèle.
pub struct LogoDetector {
    session: Session,
    output_name: String,
    config: DetectorConfig,
    model_sha256: String,
    input: Vec<f32>,
}

impl LogoDetector {
    pub fn load(config: DetectorConfig) -> Result<Self> {
        config.validate()?;
        ensure!(
            config.model.is_file(),
            "modèle de détection introuvable: {}",
            config.model.display()
        );
        let model_sha256 = sha256_file(&config.model)?;
        init_ort(config.onnxruntime.as_deref())?;

        let size = config.input_size as usize;
        let mut builder = Session::builder()?
            .with_optimization_level(GraphOptimizationLevel::Level3)?
            .with_parallel_execution(false)?
            .with_memory_pattern(true)?;
        let providers = match config.device {
            ExecutionDevice::Cuda => vec![
                CUDAExecutionProvider::default()
                    .with_device_id(0)
                    .with_conv_algorithm_search(CuDNNConvAlgorithmSearch::Heuristic)
                    .with_tf32(false)
                    .build()
                    .error_on_failure(),
                CPUExecutionProvider::default().build(),
            ],
            ExecutionDevice::Cpu => vec![CPUExecutionProvider::default().build()],
        };
        builder = builder
            .with_execution_providers(providers)
            .context("enregistrement des execution providers ONNX Runtime")?;
        let session = builder
            .commit_from_file(&config.model)
            .with_context(|| format!("ouverture du modèle {}", config.model.display()))?;

        if let Some(shape) = session
            .inputs
            .first()
            .and_then(|input| input.input_type.tensor_shape())
        {
            let shape: Vec<i64> = shape.iter().copied().collect();
            if shape.len() == 4 && shape.iter().all(|dim| *dim > 0) {
                let expected = [1i64, 3, size as i64, size as i64];
                ensure!(
                    shape == expected,
                    "forme d'entrée {shape:?} incompatible avec --size {size} (attendu {expected:?})"
                );
            }
        }
        if let Some(shape) = session
            .outputs
            .first()
            .and_then(|output| output.output_type.tensor_shape())
        {
            let shape: Vec<i64> = shape.iter().copied().collect();
            if shape.len() == 3 && shape[2] > 0 {
                let expected = 5 + config.class_names.len() as i64;
                ensure!(
                    shape[2] == expected,
                    "sortie de largeur {} au lieu de {} ({} classes)",
                    shape[2],
                    expected,
                    config.class_names.len()
                );
            }
        }
        let output_name = session
            .outputs
            .first()
            .map(|output| output.name.clone())
            .context("le modèle n'a aucune sortie")?;

        let mut detector = Self {
            session,
            output_name,
            config,
            model_sha256,
            input: vec![0.0; 3 * size * size],
        };
        // Chauffe le contexte CUDA et valide la forme de sortie avant la
        // première frame de production.
        let dummy = BgrFrame::new(size, size, vec![114u8; size * size * 3])?;
        let _ = detector.detect(&dummy)?;
        Ok(detector)
    }

    pub fn model_sha256(&self) -> &str {
        &self.model_sha256
    }

    pub fn config(&self) -> &DetectorConfig {
        &self.config
    }

    pub(crate) fn detect(&mut self, frame: &BgrFrame) -> Result<Vec<Detection>> {
        ensure!(
            frame.width > 0 && frame.height > 0,
            "frame de dimensions nulles"
        );
        let size = self.config.input_size as usize;
        let window = self.config.roi.window(frame.width, frame.height);
        let mut canvas = vec![114u8; size * size * 3];
        let (scale, new_w, new_h) = letterbox_roi(frame, window, size, &mut canvas)?;
        fill_chw(&canvas, size, &mut self.input);

        let tensor = Tensor::from_array(([1usize, 3, size, size], self.input.clone()))
            .context("création du tenseur d'entrée")?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .context("inférence du détecteur de logos")?;
        let value = outputs
            .get(&self.output_name)
            .with_context(|| format!("sortie {} absente", self.output_name))?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .context("lecture de la sortie du détecteur")?;
        decode_yolox(
            data,
            shape,
            &self.config,
            scale,
            window,
            frame.width,
            frame.height,
            new_w,
            new_h,
        )
    }
}

/// Letterbox bilinéaire d'une fenêtre source dans un canvas carré rempli de
/// 114. Retourne l'échelle globale et les dimensions utiles (sans padding).
pub(crate) fn letterbox_roi(
    frame: &BgrFrame,
    window: RoiWindow,
    size: usize,
    canvas: &mut [u8],
) -> Result<(f32, usize, usize)> {
    ensure!(
        canvas.len() == size * size * 3,
        "canvas de letterbox de taille invalide"
    );
    ensure!(
        window.width > 0
            && window.height > 0
            && window.x + window.width <= frame.width
            && window.y + window.height <= frame.height,
        "fenêtre ROI hors frame"
    );
    canvas.fill(114);
    let scale = (size as f32 / window.width as f32).min(size as f32 / window.height as f32);
    let new_w = ((window.width as f32) * scale).round().max(1.0) as usize;
    let new_h = ((window.height as f32) * scale).round().max(1.0) as usize;
    let new_w = new_w.min(size);
    let new_h = new_h.min(size);
    let scale_x = window.width as f32 / new_w as f32;
    let scale_y = window.height as f32 / new_h as f32;
    for y in 0..new_h {
        let sy = (y as f32 + 0.5) * scale_y - 0.5;
        let y0 = sy.floor().max(0.0) as usize;
        let y1 = (y0 + 1).min(window.height - 1);
        let wy = (sy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..new_w {
            let sx = (x as f32 + 0.5) * scale_x - 0.5;
            let x0 = sx.floor().max(0.0) as usize;
            let x1 = (x0 + 1).min(window.width - 1);
            let wx = (sx - x0 as f32).clamp(0.0, 1.0);
            let p00 = ((window.y + y0) * frame.width + window.x + x0) * 3;
            let p10 = ((window.y + y0) * frame.width + window.x + x1) * 3;
            let p01 = ((window.y + y1) * frame.width + window.x + x0) * 3;
            let p11 = ((window.y + y1) * frame.width + window.x + x1) * 3;
            let dst = (y * size + x) * 3;
            for channel in 0..3 {
                let top = frame.data[p00 + channel] as f32 * (1.0 - wx)
                    + frame.data[p10 + channel] as f32 * wx;
                let bottom = frame.data[p01 + channel] as f32 * (1.0 - wx)
                    + frame.data[p11 + channel] as f32 * wx;
                canvas[dst + channel] =
                    (top * (1.0 - wy) + bottom * wy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok((scale, new_w, new_h))
}

fn fill_chw(canvas: &[u8], size: usize, input: &mut [f32]) {
    let plane = size * size;
    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) * 3;
            for channel in 0..3 {
                input[channel * plane + y * size + x] = canvas[index + channel] as f32;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_yolox(
    data: &[f32],
    shape: &[i64],
    config: &DetectorConfig,
    scale: f32,
    window: RoiWindow,
    frame_width: usize,
    frame_height: usize,
    new_w: usize,
    new_h: usize,
) -> Result<Vec<Detection>> {
    if shape.len() != 3 {
        bail!("sortie de détecteur de rang {} au lieu de 3", shape.len());
    }
    let anchors = shape[1] as usize;
    let stride = shape[2] as usize;
    let classes = config.class_names.len();
    if stride != 5 + classes {
        bail!(
            "sortie YOLOX de largeur {stride} au lieu de {}",
            5 + classes
        );
    }
    if data.len() < anchors * stride {
        bail!("sortie de détecteur tronquée");
    }
    if !(scale.is_finite() && scale > 0.0) {
        bail!("échelle de letterbox invalide: {scale}");
    }
    let frame_area = (frame_width * frame_height) as f32;
    let mut detections = Vec::new();
    for anchor in 0..anchors {
        let base = anchor * stride;
        let objectness = data[base + 4];
        if !objectness.is_finite() || objectness < config.conf_threshold {
            continue;
        }
        let mut class_id = 0usize;
        let mut class_score = 0.0f32;
        for class in 0..classes {
            let score = data[base + 5 + class];
            if score.is_finite() && score > class_score {
                class_score = score;
                class_id = class;
            }
        }
        let score = objectness * class_score;
        if !score.is_finite() || score < config.conf_threshold {
            continue;
        }
        let center_x = data[base];
        let center_y = data[base + 1];
        let width = data[base + 2];
        let height = data[base + 3];
        if ![center_x, center_y, width, height]
            .iter()
            .all(|value| value.is_finite())
        {
            continue;
        }
        // Une détection dont le centre tombe dans le padding n'est pas
        // exploitable : elle ne correspond à aucun pixel source.
        let center_in = center_x >= 0.0
            && center_y >= 0.0
            && center_x <= new_w as f32
            && center_y <= new_h as f32;
        if !center_in {
            continue;
        }
        let mut x1 = (center_x - width / 2.0) / scale + window.x as f32;
        let mut y1 = (center_y - height / 2.0) / scale + window.y as f32;
        let mut x2 = (center_x + width / 2.0) / scale + window.x as f32;
        let mut y2 = (center_y + height / 2.0) / scale + window.y as f32;
        x1 = x1.clamp(window.x as f32, (window.x + window.width) as f32);
        x2 = x2.clamp(window.x as f32, (window.x + window.width) as f32);
        y1 = y1.clamp(window.y as f32, (window.y + window.height) as f32);
        y2 = y2.clamp(window.y as f32, (window.y + window.height) as f32);
        if x2 <= x1 || y2 <= y1 {
            continue;
        }
        let bbox = [x1, y1, x2, y2];
        let box_width = x2 - x1;
        let box_height = y2 - y1;
        if box_height < 2.0
            || box_width > 0.80 * frame_width as f32
            || box_width * box_height > 0.60 * frame_area
        {
            continue;
        }
        detections.push(Detection {
            class_id,
            label: config
                .class_names
                .get(class_id)
                .cloned()
                .unwrap_or_else(|| format!("class_{class_id}")),
            score,
            bbox,
        });
    }
    Ok(nms(detections, config.iou_threshold))
}

fn nms(mut detections: Vec<Detection>, threshold: f32) -> Vec<Detection> {
    detections.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    let mut kept: Vec<Detection> = Vec::new();
    for detection in detections {
        let overlaps = kept.iter().any(|other| {
            other.class_id == detection.class_id && iou(&other.bbox, &detection.bbox) > threshold
        });
        if !overlaps {
            kept.push(detection);
        }
    }
    kept
}

pub(crate) fn iou(left: &[f32; 4], right: &[f32; 4]) -> f32 {
    let x1 = left[0].max(right[0]);
    let y1 = left[1].max(right[1]);
    let x2 = left[2].min(right[2]);
    let y2 = left[3].min(right[3]);
    let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let left_area = (left[2] - left[0]).max(0.0) * (left[3] - left[1]).max(0.0);
    let right_area = (right[2] - right[0]).max(0.0) * (right[3] - right[1]).max(0.0);
    let union = left_area + right_area - intersection;
    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vision::frame::FrameCache;
    use std::path::Path;
    use std::time::Instant;

    fn frame(width: usize, height: usize) -> BgrFrame {
        BgrFrame::new(width, height, vec![0u8; width * height * 3]).unwrap()
    }

    #[test]
    fn roi_windows_cover_the_expected_zones() {
        // Given: une frame 1920x1080.
        // When:  les trois ROI sont résolues.
        // Then:  la bande basse commence à 594 px et le coin garde 960 px.
        assert_eq!(
            Roi::Full.window(1920, 1080),
            RoiWindow {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080
            }
        );
        assert_eq!(
            Roi::Bottom.window(1920, 1080),
            RoiWindow {
                x: 0,
                y: 594,
                width: 1920,
                height: 486
            }
        );
        assert_eq!(
            Roi::BottomLeft.window(1920, 1080),
            RoiWindow {
                x: 0,
                y: 594,
                width: 960,
                height: 486
            }
        );
    }

    #[test]
    fn roi_window_never_returns_an_empty_frame() {
        let window = Roi::Bottom.window(1, 1);
        assert_eq!(window.height, 1);
        let window = Roi::BottomLeft.window(1, 1);
        assert_eq!(window.width, 1);
        assert_eq!(window.height, 1);
    }

    #[test]
    fn letterbox_preserves_aspect_and_padding() {
        // Given: une frame 1920x1080 et un canvas 416.
        // When:  la ROI pleine frame est letterboxée.
        // Then:  l'échelle vaut 416/1920 et la hauteur utile 234 px.
        let source = frame(1920, 1080);
        let mut canvas = vec![0u8; 416 * 416 * 3];
        let (scale, new_w, new_h) =
            letterbox_roi(&source, Roi::Full.window(1920, 1080), 416, &mut canvas).unwrap();
        assert!((scale - 416.0 / 1920.0).abs() < 1e-6);
        assert_eq!((new_w, new_h), (416, 234));
        // Le padding reste à 114.
        assert_eq!(
            &canvas[(234 * 416) * 3..(234 * 416) * 3 + 3],
            &[114, 114, 114]
        );
        assert_eq!(&canvas[0..3], &[0, 0, 0]);
    }

    #[test]
    fn letterbox_rejects_an_out_of_frame_roi() {
        let source = frame(4, 4);
        let mut canvas = vec![0u8; 416 * 416 * 3];
        let result = letterbox_roi(
            &source,
            RoiWindow {
                x: 3,
                y: 0,
                width: 2,
                height: 1,
            },
            416,
            &mut canvas,
        );
        assert!(result.is_err());
    }

    fn config() -> DetectorConfig {
        DetectorConfig {
            model: PathBuf::from("modele.onnx"),
            device: ExecutionDevice::Cpu,
            onnxruntime: None,
            input_size: 416,
            conf_threshold: 0.30,
            iou_threshold: 0.65,
            class_names: vec!["logo".into()],
            roi: Roi::Full,
        }
    }

    #[test]
    fn decode_maps_boxes_back_to_source_pixels() {
        // Given: une sortie synthetique 1 ancrage au centre du letterbox.
        // When:  elle est décodée en pleine frame.
        // Then:  la boîte est remappée par l'échelle 0,5 et l'offset ROI.
        let mut conf = config();
        conf.conf_threshold = 0.25;
        let data = vec![208.0, 117.0, 104.0, 52.0, 0.90, 0.80];
        let detections = decode_yolox(
            &data,
            &[1, 1, 6],
            &conf,
            0.5,
            RoiWindow {
                x: 100,
                y: 50,
                width: 800,
                height: 600,
            },
            1920,
            1080,
            416,
            416,
        )
        .unwrap();
        assert_eq!(detections.len(), 1);
        let bbox = detections[0].bbox;
        assert!((bbox[0] - 412.0).abs() < 1e-4);
        assert!((bbox[1] - 232.0).abs() < 1e-4);
        assert!((bbox[2] - 620.0).abs() < 1e-4);
        assert!((bbox[3] - 336.0).abs() < 1e-4);
        assert_eq!(detections[0].label, "logo");
    }

    #[test]
    fn decode_filters_low_scores_and_padding() {
        let mut conf = config();
        conf.conf_threshold = 0.30;
        // Ligne 1 : score 0,45 -> gardée ; ligne 2 : score 0,20 -> rejetée ;
        // ligne 3 : centre dans le padding -> rejetée.
        let data = vec![
            100.0, 100.0, 20.0, 20.0, 0.90, 0.50, 100.0, 100.0, 20.0, 20.0, 0.40, 0.50, 410.0,
            410.0, 20.0, 20.0, 0.90, 0.90,
        ];
        let detections = decode_yolox(
            &data,
            &[1, 3, 6],
            &conf,
            0.5,
            Roi::Full.window(800, 600),
            800,
            600,
            400,
            300,
        )
        .unwrap();
        assert_eq!(detections.len(), 1);
        assert!((detections[0].score - 0.45).abs() < 1e-6);
    }

    #[test]
    fn nms_keeps_the_best_overlapping_box() {
        let first = Detection {
            class_id: 0,
            label: "logo".into(),
            score: 0.9,
            bbox: [0.0, 0.0, 10.0, 10.0],
        };
        let second = Detection {
            class_id: 0,
            label: "logo".into(),
            score: 0.8,
            bbox: [1.0, 1.0, 11.0, 11.0],
        };
        let kept = nms(vec![first.clone(), second], 0.5);
        assert_eq!(kept, vec![first]);
    }

    #[test]
    fn decode_rejects_a_truncated_output() {
        let result = decode_yolox(
            &[0.0; 5],
            &[1, 2, 6],
            &config(),
            0.5,
            Roi::Full.window(10, 10),
            10,
            10,
            10,
            10,
        );
        assert!(result.is_err());
    }

    #[test]
    fn config_validation_rejects_invalid_values() {
        let mut conf = config();
        conf.input_size = 300;
        assert!(conf.validate().is_err());
        let mut conf = config();
        conf.conf_threshold = 1.5;
        assert!(conf.validate().is_err());
        let mut conf = config();
        conf.class_names.clear();
        assert!(conf.validate().is_err());
    }

    #[test]
    fn device_and_roi_parsing_are_strict() {
        assert_eq!(
            ExecutionDevice::parse("CUDA").unwrap(),
            ExecutionDevice::Cuda
        );
        assert_eq!(ExecutionDevice::parse("cpu").unwrap(), ExecutionDevice::Cpu);
        assert!(ExecutionDevice::parse("metal").is_err());
        assert_eq!(Roi::parse("bottom-left").unwrap(), Roi::BottomLeft);
        assert!(Roi::parse("top_right").is_err());
    }

    #[test]
    #[ignore = "labo B2: LOGO_DETECTOR_ONNX et LOGO_DETECTOR_BGR requis"]
    fn lab_detects_an_a2_frame_with_the_exported_model() {
        let Ok(model) = std::env::var("LOGO_DETECTOR_ONNX") else {
            return;
        };
        let Ok(bgr_path) = std::env::var("LOGO_DETECTOR_BGR") else {
            return;
        };
        let onnxruntime = std::env::var("LOGO_DETECTOR_ORT").ok().map(PathBuf::from);
        let bytes = std::fs::read(&bgr_path).unwrap();
        let frame = BgrFrame::new(1920, 1080, bytes).unwrap();
        let config = DetectorConfig {
            model: PathBuf::from(model),
            device: ExecutionDevice::Cpu,
            onnxruntime,
            input_size: 416,
            conf_threshold: 0.30,
            iou_threshold: 0.65,
            class_names: vec!["logo".into()],
            roi: Roi::Full,
        };
        let mut detector = LogoDetector::load(config).unwrap();
        let detections = detector.detect(&frame).unwrap();
        for detection in &detections {
            println!("{detection:?}");
        }
        assert!(!detections.is_empty());
    }

    #[test]
    #[ignore = "labo B2: LOGO_DETECTOR_ONNX, LOGO_DETECTOR_VIDEO et LOGO_DETECTOR_ORT requis"]
    fn lab_profiles_load_grab_and_inference() {
        let Ok(model) = std::env::var("LOGO_DETECTOR_ONNX") else {
            return;
        };
        let Ok(video) = std::env::var("LOGO_DETECTOR_VIDEO") else {
            return;
        };
        let ffmpeg = std::env::var("LOGO_DETECTOR_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
        let ffprobe = std::env::var("LOGO_DETECTOR_FFPROBE").unwrap_or_else(|_| "ffprobe".into());
        let device = std::env::var("LOGO_DETECTOR_DEVICE")
            .ok()
            .and_then(|value| ExecutionDevice::parse(&value).ok())
            .unwrap_or(ExecutionDevice::Cuda);
        let duration: f64 = String::from_utf8(
            std::process::Command::new(ffprobe)
                .args([
                    "-v",
                    "error",
                    "-show_entries",
                    "format=duration",
                    "-of",
                    "csv=p=0",
                    &video,
                ])
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap()
        .trim()
        .parse()
        .unwrap();
        let config = DetectorConfig {
            model: PathBuf::from(model),
            device,
            onnxruntime: std::env::var("LOGO_DETECTOR_ORT").ok().map(PathBuf::from),
            input_size: 416,
            conf_threshold: 0.30,
            iou_threshold: 0.65,
            class_names: vec!["logo".into()],
            roi: Roi::Full,
        };
        let load = Instant::now();
        let mut detector = LogoDetector::load(config).unwrap();
        println!(
            "chargement: {:.1} ms",
            load.elapsed().as_secs_f64() * 1000.0
        );

        let cache = FrameCache::new(Path::new(&video), 1920, 1080, duration);
        let grab = Instant::now();
        let mut frames = Vec::new();
        for index in 0..crate::vision::frame::SAMPLE_COUNT {
            if let Some(frame) = cache.frame(Path::new(&ffmpeg), index).unwrap() {
                frames.push(frame);
            }
        }
        println!(
            "12 frames (1er passage): {:.1} ms ({} frames)",
            grab.elapsed().as_secs_f64() * 1000.0,
            frames.len()
        );

        let cached = Instant::now();
        for index in 0..crate::vision::frame::SAMPLE_COUNT {
            assert!(cache.frame(Path::new(&ffmpeg), index).unwrap().is_some());
        }
        println!(
            "12 frames (2e passage): {:.1} ms",
            cached.elapsed().as_secs_f64() * 1000.0
        );

        let infer = Instant::now();
        for frame in &frames {
            detector.detect(frame).unwrap();
        }
        println!(
            "12 inférences: {:.1} ms",
            infer.elapsed().as_secs_f64() * 1000.0
        );
    }
}
