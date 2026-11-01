//! Stage 1 OCR — sous-titres incrustés, PP-OCRv6 sur ONNX Runtime CPU.
//!
//! Le contrôle reprend les seuils validés dans le PoC : seule la bande basse
//! (25 % inférieurs) est lue, la détection DB renvoie des lignes de texte, la
//! reconnaissance CTC donne texte et score, puis des filtres géométriques
//! écartent logos, cartons et textes de scène. Un sous-titre n'est retenu
//! qu'après trois échantillons consécutifs à hauteur stable.
//!
//! Différence assumée avec le PoC : les boîtes sont axis-aligned. Les
//! sous-titres broadcast le sont aussi ; les quadrilatères tournés ne sont pas
//! nécessaires ici et évitent d'embarquer un offsetter de polygones.
//!
//! Aucune écriture : le module retourne des checks et des findings, et laisse
//! `qa.json` à `report::write_report`.

use crate::post::detect;
use crate::qa::frame::{grab_frame, resize_bilinear, BgrFrame};
use crate::qa::inference::cpu_session;
use crate::qa::model::{
    DefectType, ModelInfo, QaCheck, QaFinding, QaSpecs, Severity, Stage1OcrEvidence,
};
use crate::qa::sample::{sample_times, SegmentSpan};
use anyhow::{bail, ensure, Context, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use std::time::Instant;
use workflow_schema::round::round_to;

pub const OCR_DET_FILENAME: &str = "PP-OCRv6_det_small.onnx";
pub const OCR_REC_FILENAME: &str = "PP-OCRv6_rec_small.onnx";
pub const OCR_LICENSE: &str = "Apache-2.0";

const BOTTOM_START: f64 = 0.75;
const SUBTITLE_TOP: f64 = 0.76;
const SUBTITLE_BOTTOM: f64 = 0.985;
const MIN_TEXT_LEN: usize = 2;
const MIN_SCORE: f64 = 0.55;
const MIN_BOX_HEIGHT: i64 = 24;
const MAX_BOX_HEIGHT: i64 = 140;
const MIN_BOX_WIDTH: i64 = 40;
const MAX_BOX_WIDTH_RATIO: f64 = 0.95;
const MIN_ASPECT: f64 = 2.0;
const MAX_AREA_RATIO: f64 = 0.15;
const CENTER_X_MIN: f64 = 0.12;
const CENTER_X_MAX: f64 = 0.88;
const MIN_CONSECUTIVE: usize = 3;
const MAX_Y_DRIFT: f64 = 0.04;

const DET_LIMIT_SIDE: f64 = 480.0;
const DET_THRESH: f32 = 0.3;
const DET_BOX_THRESH: f32 = 0.5;
const DET_UNCLIP_RATIO: f64 = 1.6;
const DET_MIN_SIZE: f64 = 3.0;
const DET_MAX_CANDIDATES: usize = 1000;

const REC_HEIGHT: usize = 48;
const REC_BASE_WIDTH: usize = 320;
const REC_BATCH: usize = 6;

#[derive(Debug, Clone)]
pub struct OcrOptions {
    pub det_model: PathBuf,
    pub rec_model: PathBuf,
    pub onnxruntime: Option<PathBuf>,
    /// Threads intra-op ONNX (None : défaut du runtime).
    pub intra_threads: Option<usize>,
    pub ffmpeg: PathBuf,
    /// 1 ou 3 échantillons par segment, comme le PoC.
    pub per_segment: usize,
}

impl Default for OcrOptions {
    fn default() -> Self {
        Self {
            det_model: PathBuf::new(),
            rec_model: PathBuf::new(),
            onnxruntime: None,
            intra_threads: None,
            ffmpeg: PathBuf::from("ffmpeg"),
            per_segment: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub checks: Vec<QaCheck>,
    pub findings: Vec<QaFinding>,
    pub evidence: Stage1OcrEvidence,
    pub det_model: ModelInfo,
    pub rec_model: ModelInfo,
    pub timing_ms: u64,
}

#[derive(Debug, Clone)]
struct Detection {
    text: String,
    score: f64,
    bbox: [i64; 4],
}

#[derive(Debug, Clone)]
struct OcrSample {
    segment: String,
    time: f64,
    block_y_ratio: Option<f64>,
    detections: Vec<Detection>,
}

#[derive(Debug, Clone, Copy)]
struct DetBox {
    bbox: [i64; 4],
}

pub fn run_ocr(
    video: &Path,
    specs: &QaSpecs,
    segments: &[SegmentSpan],
    options: &OcrOptions,
) -> Result<OcrResult> {
    if !matches!(options.per_segment, 1 | 3) {
        bail!(
            "per_segment invalide: {} (attendu: 1 ou 3)",
            options.per_segment
        );
    }
    for model in [&options.det_model, &options.rec_model] {
        if !model.is_file() {
            bail!("modèle OCR introuvable: {}", model.display());
        }
    }
    if !video.is_file() {
        bail!("vidéo introuvable: {}", video.display());
    }

    let started = Instant::now();
    let det_model = ModelInfo {
        name: options
            .det_model
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(OCR_DET_FILENAME)
            .to_string(),
        sha256: detect::sha256_file(&options.det_model)?,
        license: OCR_LICENSE.into(),
    };
    let rec_model = ModelInfo {
        name: options
            .rec_model
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(OCR_REC_FILENAME)
            .to_string(),
        sha256: detect::sha256_file(&options.rec_model)?,
        license: OCR_LICENSE.into(),
    };
    let mut det = DetEngine::load(
        &options.det_model,
        options.onnxruntime.as_deref(),
        options.intra_threads,
    )?;
    let mut rec = RecEngine::load(
        &options.rec_model,
        options.onnxruntime.as_deref(),
        options.intra_threads,
    )?;

    let samples_meta = sample_times(specs.duration_s, segments, options.per_segment);
    let mut samples = Vec::with_capacity(samples_meta.len());
    let mut frame_errors = 0usize;
    let mut samples_with_text = 0usize;
    let mut detections_total = 0usize;

    for item in samples_meta {
        let frame = match grab_frame(
            &options.ffmpeg,
            video,
            item.time,
            specs.width as usize,
            specs.height as usize,
        ) {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                frame_errors += 1;
                continue;
            }
            Err(error) => {
                tracing::warn!(
                    time = item.time,
                    error = %error,
                    "extraction de frame OCR en échec"
                );
                frame_errors += 1;
                continue;
            }
        };
        let band = bottom_band(&frame);
        let band_y0 = frame.height - band.height;
        let det_started = Instant::now();
        let boxes = det.detect(&band)?;
        let det_ms = det_started.elapsed().as_millis();
        let rec_started = Instant::now();
        let texts = rec.recognize(&band, &boxes)?;
        let rec_ms = rec_started.elapsed().as_millis();
        tracing::debug!(
            time = item.time,
            det_ms,
            rec_ms,
            boxes = boxes.len(),
            "OCR frame"
        );
        let detections = texts
            .into_iter()
            .zip(boxes.iter())
            .filter_map(|((text, score), det_box)| {
                filter_detection(&text, score, det_box, band_y0, &frame)
            })
            .collect::<Vec<_>>();
        for detection in &detections {
            tracing::debug!(
                time = item.time,
                text = %detection.text,
                score = detection.score,
                bbox = ?detection.bbox,
                "sous-titre candidat"
            );
        }
        if !detections.is_empty() {
            samples_with_text += 1;
        }
        detections_total += detections.len();
        let block_y_ratio = if detections.is_empty() {
            None
        } else {
            let mean = detections
                .iter()
                .map(|detection| detection.bbox[1] as f64 + detection.bbox[3] as f64 / 2.0)
                .sum::<f64>()
                / detections.len() as f64;
            Some(round_to(mean / frame.height as f64, 4))
        };
        samples.push(OcrSample {
            segment: item.segment,
            time: item.time,
            block_y_ratio,
            detections,
        });
    }

    let findings = aggregate(&samples);
    let evidence = Stage1OcrEvidence {
        det_model: det_model.name.clone(),
        rec_model: rec_model.name.clone(),
        samples_total: samples.len(),
        samples_with_text,
        detections_total,
        findings_total: findings.len(),
        frame_errors,
        per_segment: options.per_segment,
        min_score: MIN_SCORE,
        bottom_start: BOTTOM_START,
    };
    let check = ocr_check(&evidence);
    Ok(OcrResult {
        checks: vec![check],
        findings,
        evidence,
        det_model,
        rec_model,
        timing_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn bottom_band(frame: &BgrFrame) -> BgrFrame {
    let y0 = (frame.height as f64 * BOTTOM_START) as usize;
    frame.crop(0, y0, frame.width, frame.height - y0)
}

/// Filtres du PoC appliqués après reconnaissance. Les coordonnées de la boîte
/// sont dans la bande basse ; `band_y0` les ramène dans la frame complète.
fn filter_detection(
    text: &str,
    score: f64,
    det_box: &DetBox,
    band_y0: usize,
    frame: &BgrFrame,
) -> Option<Detection> {
    let clean = text.trim();
    if score < MIN_SCORE || clean.chars().count() < MIN_TEXT_LEN {
        return None;
    }
    let [x1, y1, x2, y2] = det_box.bbox;
    let width = x2 - x1;
    let height = y2 - y1;
    let y1 = y1 + band_y0 as i64;
    let y2 = y2 + band_y0 as i64;
    let frame_width = frame.width as f64;
    let frame_height = frame.height as f64;
    if !(MIN_BOX_HEIGHT..=MAX_BOX_HEIGHT).contains(&height)
        || width < MIN_BOX_WIDTH
        || (width as f64) > MAX_BOX_WIDTH_RATIO * frame_width
    {
        return None;
    }
    let aspect = width as f64 / height.max(1) as f64;
    let area_ratio = (width * height) as f64 / (frame_width * frame_height);
    if aspect < MIN_ASPECT || area_ratio > MAX_AREA_RATIO {
        return None;
    }
    if (y1 as f64) < SUBTITLE_TOP * frame_height || (y2 as f64) > SUBTITLE_BOTTOM * frame_height {
        return None;
    }
    let center_x = (x1 as f64 + x2 as f64) / 2.0 / frame_width;
    if !(CENTER_X_MIN..=CENTER_X_MAX).contains(&center_x) {
        return None;
    }
    Some(Detection {
        text: clean.to_string(),
        score: round_to(score, 3),
        bbox: [x1, y1, width, height],
    })
}

fn aggregate(samples: &[OcrSample]) -> Vec<QaFinding> {
    struct Episode {
        start: f64,
        end: f64,
        samples: usize,
        texts: Vec<String>,
        scores: Vec<f64>,
        segments: Vec<String>,
        ys: Vec<f64>,
    }

    let mut episodes = Vec::new();
    let mut current: Option<Episode> = None;
    for sample in samples {
        if sample.detections.is_empty() {
            if let Some(episode) = current.take() {
                episodes.push(episode);
            }
            continue;
        }
        let episode = current.get_or_insert_with(|| Episode {
            start: sample.time,
            end: sample.time,
            samples: 0,
            texts: Vec::new(),
            scores: Vec::new(),
            segments: Vec::new(),
            ys: Vec::new(),
        });
        episode.end = sample.time;
        episode.samples += 1;
        episode
            .texts
            .extend(sample.detections.iter().map(|det| det.text.clone()));
        episode
            .scores
            .extend(sample.detections.iter().map(|det| det.score));
        if let Some(ratio) = sample.block_y_ratio {
            episode.ys.push(ratio);
        }
        episode.segments.push(sample.segment.clone());
    }
    if let Some(episode) = current {
        episodes.push(episode);
    }

    let mut findings = Vec::new();
    for episode in episodes {
        if episode.samples < MIN_CONSECUTIVE {
            continue;
        }
        if let (Some(min), Some(max)) = (
            episode.ys.iter().copied().reduce(f64::min),
            episode.ys.iter().copied().reduce(f64::max),
        ) {
            if max - min > MAX_Y_DRIFT {
                continue;
            }
        }
        let mean_score = episode.scores.iter().sum::<f64>() / episode.scores.len() as f64;
        let max_score = episode
            .scores
            .iter()
            .copied()
            .reduce(f64::max)
            .unwrap_or_default();
        let center_y = if episode.ys.is_empty() {
            None
        } else {
            Some(episode.ys.iter().sum::<f64>() / episode.ys.len() as f64)
        };
        let mut texts = episode.texts.iter().take(20).cloned().collect::<Vec<_>>();
        if episode.texts.len() > texts.len() {
            texts.push("…".into());
        }
        let mut segments = episode.segments.iter().take(6).cloned().collect::<Vec<_>>();
        if episode.segments.len() > segments.len() {
            segments.push("…".into());
        }
        let notes = format!(
            "sous-titre incrusté sur {} échantillon(s) [{}], textes: {}, score moyen {:.3}, score max {:.3}, centre y {}",
            episode.samples,
            segments.join(", "),
            texts.join(" / "),
            mean_score,
            max_score,
            center_y.map_or_else(|| "n/a".into(), |value| format!("{value:.4}"))
        );
        let severity = if episode.samples >= 4 {
            Severity::High
        } else {
            Severity::Medium
        };
        findings.push(QaFinding::new(
            DefectType::SubtitleBurnedIn,
            round_to(episode.start, 2),
            Some(round_to(episode.end, 2)),
            severity,
            round_to(mean_score.clamp(0.0, 1.0), 3),
            "stage1_ppocr_bottom",
            notes,
        ));
    }
    findings
}

fn ocr_check(evidence: &Stage1OcrEvidence) -> QaCheck {
    if evidence.samples_total == 0 {
        return QaCheck::warn(
            "subtitle_ocr",
            format!(
                "aucune frame exploitable ({} erreur(s) d'extraction)",
                evidence.frame_errors
            ),
        );
    }
    let detail = format!(
        "{} échantillon(s), {} avec texte, {} détection(s), {} erreur(s)",
        evidence.samples_total,
        evidence.samples_with_text,
        evidence.detections_total,
        evidence.frame_errors
    );
    if evidence.findings_total == 0 {
        QaCheck::pass("subtitle_ocr", detail)
    } else {
        QaCheck::warn(
            "subtitle_ocr",
            format!(
                "{} épisode(s) de sous-titre incrusté ; {detail}",
                evidence.findings_total
            ),
        )
    }
}

struct DetEngine {
    session: Session,
    output_name: String,
}

impl DetEngine {
    fn load(
        model: &Path,
        onnxruntime: Option<&Path>,
        intra_threads: Option<usize>,
    ) -> Result<Self> {
        let session = cpu_session(model, onnxruntime, intra_threads)
            .with_context(|| format!("ouverture du modèle OCR det {}", model.display()))?;
        ensure!(
            session.inputs.len() == 1,
            "PP-OCR det doit avoir une entrée, trouvé {}",
            session.inputs.len()
        );
        ensure!(
            session.outputs.len() == 1,
            "PP-OCR det doit avoir une sortie, trouvé {}",
            session.outputs.len()
        );
        let output_name = session.outputs[0].name.clone();
        Ok(Self {
            session,
            output_name,
        })
    }

    fn detect(&mut self, image: &BgrFrame) -> Result<Vec<DetBox>> {
        let (det_width, det_height) = det_input_size(image.width, image.height);
        let resized = resize_bilinear(image, det_width, det_height);
        let input = normalized_chw(&resized);
        let tensor = Tensor::from_array(([1usize, 3, det_height, det_width], input))
            .context("création du tenseur PP-OCR det")?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .context("inférence PP-OCR det")?;
        let (shape, data) = outputs
            .get(&self.output_name)
            .with_context(|| format!("sortie OCR det absente: {}", self.output_name))?
            .try_extract_tensor::<f32>()
            .context("lecture de la sortie PP-OCR det")?;
        ensure!(
            shape.len() == 4 && shape[1] == 1,
            "sortie OCR det inattendue: {:?}",
            shape
        );
        let height = shape[2] as usize;
        let width = shape[3] as usize;
        ensure!(
            data.len() >= height * width,
            "sortie OCR det: {} valeurs pour {}x{}",
            data.len(),
            width,
            height
        );
        let prob = &data[..height * width];
        let mask = prob
            .iter()
            .map(|value| *value > DET_THRESH)
            .collect::<Vec<_>>();
        let mask = dilate(&mask, width, height);
        let components = components(&mask, width, height);
        let scale_x = image.width as f64 / width as f64;
        let scale_y = image.height as f64 / height as f64;
        let mut boxes = Vec::new();
        for component in components.into_iter().take(DET_MAX_CANDIDATES) {
            let box_width = component.width as f64;
            let box_height = component.height as f64;
            if box_width.min(box_height) < DET_MIN_SIZE {
                continue;
            }
            let score = box_score(prob, width, &component);
            if score < DET_BOX_THRESH {
                continue;
            }
            let distance =
                box_width * box_height * DET_UNCLIP_RATIO / (2.0 * (box_width + box_height));
            let x1 = round_half_even((component.x0 as f64 - distance) * scale_x)
                .clamp(0.0, image.width as f64 - 1.0) as i64;
            let y1 = round_half_even((component.y0 as f64 - distance) * scale_y)
                .clamp(0.0, image.height as f64 - 1.0) as i64;
            let x2 = round_half_even((component.x0 as f64 + box_width + distance) * scale_x)
                .clamp(0.0, image.width as f64 - 1.0) as i64;
            let y2 = round_half_even((component.y0 as f64 + box_height + distance) * scale_y)
                .clamp(0.0, image.height as f64 - 1.0) as i64;
            // `filter_det_res` de RapidOCR écarte les boîtes scalées dont un
            // côté fait 3 px ou moins.
            if x2 - x1 <= 3 || y2 - y1 <= 3 {
                continue;
            }
            boxes.push(DetBox {
                bbox: [x1, y1, x2, y2],
            });
        }
        Ok(boxes)
    }
}

fn det_input_size(width: usize, height: usize) -> (usize, usize) {
    let min_side = width.min(height) as f64;
    let ratio = if min_side < DET_LIMIT_SIDE {
        DET_LIMIT_SIDE / min_side
    } else {
        1.0
    };
    let resized_height = (height as f64 * ratio) as usize;
    let resized_width = (width as f64 * ratio) as usize;
    let aligned_height = (round_half_even(resized_height as f64 / 32.0) as usize * 32).max(32);
    let aligned_width = (round_half_even(resized_width as f64 / 32.0) as usize * 32).max(32);
    (aligned_width, aligned_height)
}

fn round_half_even(value: f64) -> f64 {
    value.round_ties_even()
}

fn normalized_chw(image: &BgrFrame) -> Vec<f32> {
    let plane = image.width * image.height;
    let mut input = vec![0.0f32; 3 * plane];
    for y in 0..image.height {
        for x in 0..image.width {
            let pixel = image.pixel(x, y);
            let offset = y * image.width + x;
            for channel in 0..3 {
                // mean = std = 0.5, échelle 1/255 : (v/255 - 0.5) / 0.5.
                input[channel * plane + offset] = f32::from(pixel[channel]) / 127.5 - 1.0;
            }
        }
    }
    input
}

struct Component {
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
}

fn dilate(mask: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut dilated = vec![false; mask.len()];
    for y in 0..height {
        for x in 0..width {
            if !mask[y * width + x] {
                continue;
            }
            // cv2.dilate avec un noyau 2×2 et l'ancre par défaut (0,0) étend
            // le pixel vers la droite et le bas.
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx < width && ny < height {
                    dilated[ny * width + nx] = true;
                }
            }
        }
    }
    dilated
}

/// Composantes 8-connexes de la carte binaire, équivalent de `findContours`
/// pour des lignes de texte sans trou.
fn components(mask: &[bool], width: usize, height: usize) -> Vec<Component> {
    let mut visited = vec![false; mask.len()];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if !mask[index] || visited[index] {
                continue;
            }
            visited[index] = true;
            stack.push((x, y));
            let (mut x0, mut y0, mut x1, mut y1) = (x, y, x, y);
            while let Some((current_x, current_y)) = stack.pop() {
                x0 = x0.min(current_x);
                y0 = y0.min(current_y);
                x1 = x1.max(current_x);
                y1 = y1.max(current_y);
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let nx = current_x as i64 + dx;
                        let ny = current_y as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                            continue;
                        }
                        let neighbor = ny as usize * width + nx as usize;
                        if mask[neighbor] && !visited[neighbor] {
                            visited[neighbor] = true;
                            stack.push((nx as usize, ny as usize));
                        }
                    }
                }
            }
            components.push(Component {
                x0,
                y0,
                width: x1 - x0 + 1,
                height: y1 - y0 + 1,
            });
        }
    }
    components
}

/// Moyenne de la carte de probabilité sur la boîte, comme `box_score_fast`.
fn box_score(prob: &[f32], width: usize, component: &Component) -> f32 {
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for y in component.y0..component.y0 + component.height {
        for x in component.x0..component.x0 + component.width {
            sum += f64::from(prob[y * width + x]);
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        (sum / count as f64) as f32
    }
}

struct RecEngine {
    session: Session,
    output_name: String,
    characters: Vec<String>,
}

impl RecEngine {
    fn load(
        model: &Path,
        onnxruntime: Option<&Path>,
        intra_threads: Option<usize>,
    ) -> Result<Self> {
        let session = cpu_session(model, onnxruntime, intra_threads)
            .with_context(|| format!("ouverture du modèle OCR rec {}", model.display()))?;
        ensure!(
            session.outputs.len() == 1,
            "PP-OCR rec doit avoir une sortie, trouvé {}",
            session.outputs.len()
        );
        let output_name = session.outputs[0].name.clone();
        let characters = {
            let metadata = session
                .metadata()
                .with_context(|| format!("métadonnées OCR rec {}", model.display()))?;
            metadata
                .custom("character")
                .with_context(|| format!("métadonnées OCR rec {}", model.display()))?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "le modèle OCR rec {} n'embarque pas la liste de caractères",
                        model.display()
                    )
                })?
        };
        let characters = characters.strip_suffix('\n').unwrap_or(characters.as_str());
        let mut characters = characters
            .split('\n')
            .map(str::to_string)
            .collect::<Vec<_>>();
        // RapidOCR ajoute l'espace puis le blank : l'index 0 est le blank et
        // le dernier l'espace.
        characters.push(" ".into());
        characters.insert(0, "blank".into());
        Ok(Self {
            session,
            output_name,
            characters,
        })
    }

    fn recognize(&mut self, image: &BgrFrame, boxes: &[DetBox]) -> Result<Vec<(String, f64)>> {
        let mut results = Vec::with_capacity(boxes.len());
        for chunk in boxes.chunks(REC_BATCH) {
            let ratios = chunk
                .iter()
                .map(|det_box| {
                    let [x1, y1, x2, y2] = det_box.bbox;
                    (x2 - x1) as f64 / (y2 - y1).max(1) as f64
                })
                .collect::<Vec<_>>();
            // RapidOCR calcule la largeur cible sur le lot entier ; on la
            // calcule par paquet de `REC_BATCH` pour borner la mémoire. Les
            // boîtes plus étroites sont complétées par des zéros, comme un
            // padding après normalisation.
            let max_ratio = ratios
                .iter()
                .copied()
                .fold(REC_BASE_WIDTH as f64 / REC_HEIGHT as f64, f64::max);
            let target_width = (REC_HEIGHT as f64 * max_ratio) as usize;
            let mut batch = vec![0.0f32; chunk.len() * 3 * REC_HEIGHT * target_width];
            for (index, det_box) in chunk.iter().enumerate() {
                let resized_width =
                    ((REC_HEIGHT as f64 * ratios[index]).ceil() as usize).min(target_width);
                let crop = crop_box(image, det_box.bbox);
                let resized = resize_bilinear(&crop, resized_width.max(1), REC_HEIGHT);
                let plane = REC_HEIGHT * target_width;
                for y in 0..REC_HEIGHT {
                    for x in 0..resized.width {
                        let pixel = resized.pixel(x, y);
                        let offset = y * target_width + x;
                        for channel in 0..3 {
                            batch[index * 3 * plane + channel * plane + offset] =
                                f32::from(pixel[channel]) / 127.5 - 1.0;
                        }
                    }
                }
            }
            let tensor = Tensor::from_array(([chunk.len(), 3, REC_HEIGHT, target_width], batch))
                .context("création du tenseur PP-OCR rec")?;
            let outputs = self
                .session
                .run(ort::inputs![tensor])
                .context("inférence PP-OCR rec")?;
            let (shape, data) = outputs
                .get(&self.output_name)
                .with_context(|| format!("sortie OCR rec absente: {}", self.output_name))?
                .try_extract_tensor::<f32>()
                .context("lecture de la sortie PP-OCR rec")?;
            ensure!(
                shape.len() == 3 && shape[0] as usize == chunk.len(),
                "sortie OCR rec inattendue: {:?}",
                shape
            );
            let steps = shape[1] as usize;
            let classes = shape[2] as usize;
            ensure!(
                classes == self.characters.len(),
                "OCR rec: {classes} classes pour {} caractères",
                self.characters.len()
            );
            for index in 0..chunk.len() {
                results.push(decode_ctc(
                    &data[index * steps * classes..(index + 1) * steps * classes],
                    steps,
                    classes,
                    &self.characters,
                ));
            }
        }
        Ok(results)
    }
}

fn crop_box(image: &BgrFrame, bbox: [i64; 4]) -> BgrFrame {
    let [x1, y1, x2, y2] = bbox;
    let x1 = x1.clamp(0, image.width as i64 - 1) as usize;
    let y1 = y1.clamp(0, image.height as i64 - 1) as usize;
    let x2 = x2.clamp(x1 as i64 + 1, image.width as i64) as usize;
    let y2 = y2.clamp(y1 as i64 + 1, image.height as i64) as usize;
    image.crop(x1, y1, x2 - x1, y2 - y1)
}

fn decode_ctc(data: &[f32], steps: usize, classes: usize, characters: &[String]) -> (String, f64) {
    let mut text = String::new();
    let mut score_sum = 0.0f64;
    let mut score_count = 0usize;
    let mut previous = usize::MAX;
    for step in 0..steps {
        let row = &data[step * classes..(step + 1) * classes];
        let mut best_index = 0usize;
        let mut best_prob = row[0];
        for (index, prob) in row.iter().enumerate().skip(1) {
            if *prob > best_prob {
                best_prob = *prob;
                best_index = index;
            }
        }
        if best_index != 0 && best_index != previous {
            text.push_str(&characters[best_index]);
            score_sum += f64::from(best_prob);
            score_count += 1;
        }
        previous = best_index;
    }
    let score = if score_count == 0 {
        0.0
    } else {
        score_sum / score_count as f64
    };
    (text, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: usize, height: usize, color: [u8; 3]) -> BgrFrame {
        BgrFrame::new(width, height, color.repeat(width * height)).unwrap()
    }

    #[test]
    fn bottom_band_keeps_the_last_quarter() {
        let image = frame(4, 8, [1, 2, 3]);
        let band = bottom_band(&image);
        assert_eq!(band.height, 2);
        assert_eq!(band.pixel(0, 0), [1, 2, 3]);
    }

    #[test]
    fn det_size_aligns_to_32_and_preserves_aspect() {
        let (width, height) = det_input_size(1080, 480);
        assert_eq!(height, 480);
        assert_eq!(width, 1088);
        assert_eq!(width % 32, 0);
        assert!((width as f64 / height as f64 - 1080.0 / 480.0).abs() < 0.05);
    }

    #[test]
    fn dilate_expands_right_and_bottom() {
        let mut mask = vec![false; 16];
        mask[5] = true;
        let dilated = dilate(&mask, 4, 4);
        assert!(dilated[5]);
        assert!(dilated[6]);
        assert!(dilated[9]);
        assert!(dilated[10]);
        assert!(!dilated[0]);
    }

    #[test]
    fn components_groups_adjacent_pixels() {
        let mask = vec![
            true, true, false, false, true, true, false, false, false, false, false, false, false,
            false, false, false,
        ];
        let components = components(&mask, 4, 4);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].x0, 0);
        assert_eq!(components[0].width, 2);
        assert_eq!(components[0].height, 2);
    }

    #[test]
    fn ctc_decode_collapses_repeats_and_drops_blank() {
        let characters = vec!["blank".to_string(), "A".to_string(), "B".to_string()];
        // 4 pas, 3 classes : A A blank B.
        let data = vec![0.0, 0.9, 0.1, 0.0, 0.8, 0.2, 0.9, 0.0, 0.1, 0.0, 0.1, 0.9];
        let (text, score) = decode_ctc(&data, 4, 3, &characters);
        assert_eq!(text, "AB");
        assert!((score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn filter_rejects_text_outside_the_subtitle_band() {
        let image = frame(1080, 1920, [0, 0, 0]);
        let det_box = DetBox {
            bbox: [100, 100, 900, 180],
        };
        assert!(filter_detection("TEXTE", 0.9, &det_box, 0, &image).is_none());
    }

    #[test]
    fn filter_accepts_a_centered_subtitle() {
        let image = frame(1080, 1920, [0, 0, 0]);
        // Bande basse démarrant à 1440 : boîte à 1560 dans la frame.
        let det_box = DetBox {
            bbox: [120, 120, 940, 190],
        };
        let detection = filter_detection("SOUS-TITRE", 0.9, &det_box, 1440, &image).unwrap();
        assert_eq!(detection.bbox, [120, 1560, 820, 70]);
    }

    #[test]
    fn aggregate_requires_three_consecutive_samples() {
        let detection = Detection {
            text: "SOUS-TITRE".into(),
            score: 0.9,
            bbox: [100, 1500, 800, 70],
        };
        let samples = (0..3)
            .map(|index| OcrSample {
                segment: "1".into(),
                time: f64::from(index),
                block_y_ratio: Some(0.82),
                detections: vec![detection.clone()],
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::SubtitleBurnedIn);
        assert_eq!(findings[0].timecode_start_sec, 0.0);
        assert_eq!(findings[0].timecode_end_sec, Some(2.0));
    }

    #[test]
    fn filter_rejects_a_box_outside_the_horizontal_center() {
        let image = frame(1080, 1920, [0, 0, 0]);
        let det_box = DetBox {
            bbox: [0, 120, 200, 190],
        };
        assert!(filter_detection("TEXTE", 0.9, &det_box, 1440, &image).is_none());
    }

    #[test]
    fn filter_rejects_a_box_too_tall_for_a_subtitle() {
        let image = frame(1080, 1920, [0, 0, 0]);
        let det_box = DetBox {
            bbox: [200, 100, 800, 260],
        };
        assert!(filter_detection("TEXTE", 0.9, &det_box, 1440, &image).is_none());
    }

    #[test]
    fn aggregate_marks_four_samples_as_high_severity() {
        let detection = Detection {
            text: "SOUS-TITRE".into(),
            score: 0.9,
            bbox: [100, 1500, 800, 70],
        };
        let samples = (0..4)
            .map(|index| OcrSample {
                segment: "1".into(),
                time: f64::from(index),
                block_y_ratio: Some(0.82),
                detections: vec![detection.clone()],
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn ctc_decode_returns_an_empty_text_when_only_blanks() {
        let characters = vec!["blank".to_string(), "A".to_string()];
        let data = vec![0.9, 0.1, 0.9, 0.1];
        let (text, score) = decode_ctc(&data, 2, 2, &characters);
        assert!(text.is_empty());
        assert_eq!(score, 0.0);
    }

    #[test]
    fn components_separate_disconnected_blobs() {
        let mut mask = vec![false; 16];
        mask[0] = true;
        mask[10] = true;
        let components = components(&mask, 4, 4);
        assert_eq!(components.len(), 2);
    }

    #[test]
    fn aggregate_skips_unstable_vertical_position() {
        let detection = Detection {
            text: "SOUS-TITRE".into(),
            score: 0.9,
            bbox: [100, 1500, 800, 70],
        };
        let mut samples = (0..3)
            .map(|index| OcrSample {
                segment: "1".into(),
                time: f64::from(index),
                block_y_ratio: Some(0.82),
                detections: vec![detection.clone()],
            })
            .collect::<Vec<_>>();
        samples[2].block_y_ratio = Some(0.92);
        assert!(aggregate(&samples).is_empty());
    }
}
