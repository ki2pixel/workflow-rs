//! Stage 1 visage — YuNet ONNX Runtime CPU.
//!
//! Le contrôle reprend le PoC validé : YuNet tourne sur la frame et sur une
//! version étendue de 15 % de chaque côté, les deux jeux de boîtes sont fusionnés
//! par IoU, puis seuls les visages dont un landmark (yeux, nez, bouche) approche
//! un bord gauche/droit sont retenus. Un gros plan de profil qui coupe la nuque
//! ou le menton sans couper les landmarks reste valide.
//!
//! Aucune écriture : le module retourne des checks et des findings, et laisse
//! `qa.json` à `report::write_report`.

use crate::post::detect;
use crate::qa::model::{
    DefectType, ModelInfo, QaCheck, QaFinding, QaSpecs, Severity, Stage1FaceEvidence,
};
use crate::qa::sample::{sample_times, SegmentSpan};
use crate::vision::frame::{grab_frame, resize_bilinear, BgrFrame};
use crate::vision::inference::cpu_session;
use anyhow::{bail, ensure, Context, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::Instant;
use workflow_schema::round::round_to;

pub const FACE_MODEL_FILENAME: &str = "face_detection_yunet_2023mar.onnx";
pub const FACE_MODEL_LICENSE: &str = "MIT";

const INPUT_SIZE: usize = 640;
const STRIDES: [usize; 3] = [8, 16, 32];
const MIN_FACE_W_RATIO: f32 = 0.08;
const MIN_FACE_H_RATIO: f32 = 0.08;
const EDGE_MARGIN_RATIO: f32 = 0.03;
const PAD_RATIO: f32 = 0.15;
const MIN_CONSECUTIVE: usize = 3;
const DEFAULT_SCORE_THRESHOLD: f32 = 0.6;
const DEFAULT_NMS_THRESHOLD: f32 = 0.3;

#[derive(Debug, Clone)]
pub struct FaceOptions {
    pub model: PathBuf,
    pub onnxruntime: Option<PathBuf>,
    /// Threads intra-op ONNX (None : défaut du runtime).
    pub intra_threads: Option<usize>,
    pub ffmpeg: PathBuf,
    /// 1 ou 3 échantillons par segment, comme le PoC.
    pub per_segment: usize,
}

impl Default for FaceOptions {
    fn default() -> Self {
        Self {
            model: PathBuf::new(),
            onnxruntime: None,
            intra_threads: None,
            ffmpeg: PathBuf::from("ffmpeg"),
            per_segment: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FaceResult {
    pub checks: Vec<QaCheck>,
    pub findings: Vec<QaFinding>,
    pub evidence: Stage1FaceEvidence,
    pub model: ModelInfo,
    pub timing_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceSource {
    Original,
    Padded,
}

impl FaceSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Padded => "padded",
        }
    }
}

#[derive(Debug, Clone)]
struct RawFace {
    bbox: [f32; 4],
    landmarks: [[f32; 2]; 5],
    score: f32,
    source: FaceSource,
}

#[derive(Debug, Clone)]
struct CutFace {
    bbox: [i64; 4],
    edge_landmarks: Vec<usize>,
    side: &'static str,
    score: f32,
    source: FaceSource,
}

#[derive(Debug, Clone)]
struct FaceSample {
    segment: String,
    time: f64,
    cut: Vec<CutFace>,
}

pub fn run_face(
    video: &Path,
    specs: &QaSpecs,
    segments: &[SegmentSpan],
    options: &FaceOptions,
) -> Result<FaceResult> {
    if !matches!(options.per_segment, 1 | 3) {
        bail!(
            "per_segment invalide: {} (attendu: 1 ou 3)",
            options.per_segment
        );
    }
    if !options.model.is_file() {
        bail!("modèle YuNet introuvable: {}", options.model.display());
    }
    if !video.is_file() {
        bail!("vidéo introuvable: {}", video.display());
    }

    let started = Instant::now();
    let model = ModelInfo {
        name: options
            .model
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(FACE_MODEL_FILENAME)
            .to_string(),
        sha256: detect::sha256_file(&options.model)?,
        license: FACE_MODEL_LICENSE.into(),
    };
    let mut engine = YunetEngine::load(
        &options.model,
        options.onnxruntime.as_deref(),
        options.intra_threads,
    )?;
    let samples_meta = sample_times(specs.duration_s, segments, options.per_segment);
    let mut samples = Vec::with_capacity(samples_meta.len());
    let mut frame_errors = 0usize;
    let mut samples_with_face = 0usize;
    let mut samples_with_cut = 0usize;

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
                    "extraction de frame YuNet en échec"
                );
                frame_errors += 1;
                continue;
            }
        };
        let faces = engine.detect(&frame)?;
        for face in &faces {
            tracing::debug!(
                time = item.time,
                bbox = ?face.bbox,
                landmarks = ?face.landmarks,
                score = face.score,
                source = face.source.as_str(),
                "visage YuNet"
            );
        }
        let cut = faces
            .iter()
            .filter_map(|face| classify_face(face, frame.width, frame.height))
            .collect::<Vec<_>>();
        if !faces.is_empty() {
            samples_with_face += 1;
        }
        if !cut.is_empty() {
            samples_with_cut += 1;
        }
        samples.push(FaceSample {
            segment: item.segment,
            time: item.time,
            cut,
        });
    }

    let findings = aggregate(&samples);
    let samples_total = samples.len();
    let evidence = Stage1FaceEvidence {
        model: model.name.clone(),
        samples_total,
        samples_with_face,
        samples_with_cut,
        frame_errors,
        per_segment: options.per_segment,
        score_threshold: round_to(f64::from(DEFAULT_SCORE_THRESHOLD), 3),
        nms_threshold: round_to(f64::from(DEFAULT_NMS_THRESHOLD), 3),
        pad_ratio: round_to(f64::from(PAD_RATIO), 3),
    };
    let check = face_check(&evidence, &findings);
    Ok(FaceResult {
        checks: vec![check],
        findings,
        evidence,
        model,
        timing_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

struct YunetEngine {
    session: Session,
}

impl YunetEngine {
    fn load(
        model: &Path,
        onnxruntime: Option<&Path>,
        intra_threads: Option<usize>,
    ) -> Result<Self> {
        let session = cpu_session(model, onnxruntime, intra_threads)
            .with_context(|| format!("ouverture du modèle YuNet {}", model.display()))?;
        for name in output_names() {
            ensure!(
                session.outputs.iter().any(|output| output.name == name),
                "sortie YuNet absente: {name}"
            );
        }
        if session.inputs.len() != 1 {
            bail!(
                "YuNet doit avoir une entrée, trouvé {}",
                session.inputs.len()
            );
        }
        Ok(Self { session })
    }

    fn detect(&mut self, frame: &BgrFrame) -> Result<Vec<RawFace>> {
        let mut faces = self.infer_frame(frame, FaceSource::Original)?;
        let px = (frame.width as f32 * PAD_RATIO).round() as usize;
        let py = (frame.height as f32 * PAD_RATIO).round() as usize;
        let padded = pad_replicate(frame, px, py);
        let mut padded_faces = self.infer_frame(&padded, FaceSource::Padded)?;
        for face in &mut padded_faces {
            face.bbox[0] -= px as f32;
            face.bbox[1] -= py as f32;
            for landmark in &mut face.landmarks {
                landmark[0] -= px as f32;
                landmark[1] -= py as f32;
            }
        }
        let margin_x = EDGE_MARGIN_RATIO * frame.width as f32;
        for face in padded_faces {
            let padded_cut = face
                .landmarks
                .iter()
                .any(|point| point[0] < margin_x || point[0] > frame.width as f32 - margin_x);
            // Une détection padded qui recoupe la détection originale est
            // ignorée, sauf si elle révèle un landmark hors cadre : c'est
            // précisément le cas que le padding doit rattraper.
            if padded_cut || faces.iter().all(|other| iou(&face.bbox, &other.bbox) < 0.3) {
                faces.push(face);
            }
        }
        Ok(faces)
    }

    fn infer_frame(&mut self, frame: &BgrFrame, source: FaceSource) -> Result<Vec<RawFace>> {
        let (letterboxed, scale, offset_x, offset_y) = letterbox(frame);
        let mut input = vec![0.0f32; 3 * INPUT_SIZE * INPUT_SIZE];
        for y in 0..INPUT_SIZE {
            for x in 0..INPUT_SIZE {
                let pixel = letterboxed.pixel(x, y);
                let offset = y * INPUT_SIZE + x;
                // YuNet attend les valeurs 0–255, comme `blobFromImage`
                // d'OpenCV avec son facteur d'échelle par défaut.
                input[offset] = f32::from(pixel[0]);
                input[INPUT_SIZE * INPUT_SIZE + offset] = f32::from(pixel[1]);
                input[2 * INPUT_SIZE * INPUT_SIZE + offset] = f32::from(pixel[2]);
            }
        }
        let tensor = Tensor::from_array(([1usize, 3, INPUT_SIZE, INPUT_SIZE], input))
            .context("création du tenseur YuNet")?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .context("inférence YuNet")?;
        // Les sorties YuNet sont dans le repère 640×640 du letterbox. On
        // retire la marge puis on revient aux pixels de la frame d'origine.
        let inverse_scale = 1.0 / scale;
        let offset_x = offset_x as f32;
        let offset_y = offset_y as f32;
        let mut faces = Vec::new();
        for stride in STRIDES {
            let cls_name = format!("cls_{stride}");
            let obj_name = format!("obj_{stride}");
            let bbox_name = format!("bbox_{stride}");
            let kps_name = format!("kps_{stride}");
            let cls = outputs
                .get(&cls_name)
                .with_context(|| format!("sortie YuNet absente: {cls_name}"))?
                .try_extract_tensor::<f32>()
                .with_context(|| format!("lecture de la sortie YuNet {cls_name}"))?
                .1;
            let obj = outputs
                .get(&obj_name)
                .with_context(|| format!("sortie YuNet absente: {obj_name}"))?
                .try_extract_tensor::<f32>()
                .with_context(|| format!("lecture de la sortie YuNet {obj_name}"))?
                .1;
            let bbox = outputs
                .get(&bbox_name)
                .with_context(|| format!("sortie YuNet absente: {bbox_name}"))?
                .try_extract_tensor::<f32>()
                .with_context(|| format!("lecture de la sortie YuNet {bbox_name}"))?
                .1;
            let kps = outputs
                .get(&kps_name)
                .with_context(|| format!("sortie YuNet absente: {kps_name}"))?
                .try_extract_tensor::<f32>()
                .with_context(|| format!("lecture de la sortie YuNet {kps_name}"))?
                .1;
            faces.extend(decode_level(
                cls,
                obj,
                bbox,
                kps,
                stride,
                inverse_scale,
                inverse_scale,
                offset_x,
                offset_y,
                source,
                DEFAULT_SCORE_THRESHOLD,
            )?);
        }
        Ok(nms(faces, DEFAULT_NMS_THRESHOLD))
    }
}

fn output_names() -> [&'static str; 12] {
    [
        "cls_8", "cls_16", "cls_32", "obj_8", "obj_16", "obj_32", "bbox_8", "bbox_16", "bbox_32",
        "kps_8", "kps_16", "kps_32",
    ]
}

#[allow(clippy::too_many_arguments)]
fn decode_level(
    cls: &[f32],
    obj: &[f32],
    bbox: &[f32],
    kps: &[f32],
    stride: usize,
    scale_x: f32,
    scale_y: f32,
    offset_x: f32,
    offset_y: f32,
    source: FaceSource,
    score_threshold: f32,
) -> Result<Vec<RawFace>> {
    decode_level_grid(
        cls,
        obj,
        bbox,
        kps,
        stride,
        INPUT_SIZE / stride,
        INPUT_SIZE / stride,
        scale_x,
        scale_y,
        offset_x,
        offset_y,
        source,
        score_threshold,
    )
}

#[allow(clippy::too_many_arguments)]
fn decode_level_grid(
    cls: &[f32],
    obj: &[f32],
    bbox: &[f32],
    kps: &[f32],
    stride: usize,
    cols: usize,
    rows: usize,
    scale_x: f32,
    scale_y: f32,
    offset_x: f32,
    offset_y: f32,
    source: FaceSource,
    score_threshold: f32,
) -> Result<Vec<RawFace>> {
    let cells = rows * cols;
    ensure!(
        cls.len() == cells && obj.len() == cells,
        "YuNet stride {stride}: cls/obj {} valeurs pour {cells} cellules",
        cls.len()
    );
    ensure!(
        bbox.len() == cells * 4,
        "YuNet stride {stride}: bbox {} valeurs pour {}",
        bbox.len(),
        cells * 4
    );
    ensure!(
        kps.len() == cells * 10,
        "YuNet stride {stride}: kps {} valeurs pour {}",
        kps.len(),
        cells * 10
    );

    let mut faces = Vec::new();
    for row in 0..rows {
        for col in 0..cols {
            let index = row * cols + col;
            let cls_score = cls[index].clamp(0.0, 1.0);
            let obj_score = obj[index].clamp(0.0, 1.0);
            let score = (cls_score * obj_score).sqrt();
            if score < score_threshold {
                continue;
            }
            let center_x = (col as f32 + bbox[index * 4]) * stride as f32;
            let center_y = (row as f32 + bbox[index * 4 + 1]) * stride as f32;
            let width = bbox[index * 4 + 2].exp() * stride as f32;
            let height = bbox[index * 4 + 3].exp() * stride as f32;
            if !center_x.is_finite()
                || !center_y.is_finite()
                || !width.is_finite()
                || !height.is_finite()
                || width <= 0.0
                || height <= 0.0
            {
                continue;
            }
            let mut landmarks = [[0.0f32; 2]; 5];
            for (landmark, point) in landmarks.iter_mut().enumerate() {
                point[0] = (kps[index * 10 + 2 * landmark] + col as f32) * stride as f32;
                point[1] = (kps[index * 10 + 2 * landmark + 1] + row as f32) * stride as f32;
            }
            faces.push(RawFace {
                bbox: [
                    (center_x - width / 2.0 - offset_x) * scale_x,
                    (center_y - height / 2.0 - offset_y) * scale_y,
                    width * scale_x,
                    height * scale_y,
                ],
                landmarks: landmarks.map(|point| {
                    [
                        (point[0] - offset_x) * scale_x,
                        (point[1] - offset_y) * scale_y,
                    ]
                }),
                score,
                source,
            });
        }
    }
    Ok(faces)
}

fn nms(mut faces: Vec<RawFace>, threshold: f32) -> Vec<RawFace> {
    faces.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    let mut kept: Vec<RawFace> = Vec::new();
    for face in faces {
        if kept
            .iter()
            .all(|other| iou(&face.bbox, &other.bbox) <= threshold)
        {
            kept.push(face);
        }
    }
    kept
}

fn iou(left: &[f32; 4], right: &[f32; 4]) -> f32 {
    let x1 = left[0].max(right[0]);
    let y1 = left[1].max(right[1]);
    let x2 = (left[0] + left[2]).min(right[0] + right[2]);
    let y2 = (left[1] + left[3]).min(right[1] + right[3]);
    let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let union = left[2] * left[3] + right[2] * right[3] - intersection;
    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

fn classify_face(face: &RawFace, width: usize, height: usize) -> Option<CutFace> {
    let bbox_width = face.bbox[2];
    let bbox_height = face.bbox[3];
    if bbox_width < MIN_FACE_W_RATIO * width as f32
        || bbox_height < MIN_FACE_H_RATIO * height as f32
    {
        return None;
    }
    let margin_x = EDGE_MARGIN_RATIO * width as f32;
    let edge_landmarks = face
        .landmarks
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let near_left = point[0] < margin_x;
            let near_right = point[0] > width as f32 - margin_x;
            (near_left || near_right).then_some(index)
        })
        .collect::<Vec<_>>();
    if edge_landmarks.is_empty() {
        return None;
    }
    let side = if face.landmarks.iter().any(|point| point[0] < margin_x) {
        "left"
    } else {
        "right"
    };
    Some(CutFace {
        bbox: [
            round_to(f64::from(face.bbox[0]), 0) as i64,
            round_to(f64::from(face.bbox[1]), 0) as i64,
            round_to(f64::from(face.bbox[2]), 0) as i64,
            round_to(f64::from(face.bbox[3]), 0) as i64,
        ],
        edge_landmarks,
        side,
        score: face.score,
        source: face.source,
    })
}

fn aggregate(samples: &[FaceSample]) -> Vec<QaFinding> {
    struct Episode {
        start: f64,
        end: f64,
        samples: usize,
        faces: Vec<CutFace>,
        segments: Vec<String>,
    }

    let mut episodes = Vec::new();
    let mut current: Option<Episode> = None;
    for sample in samples {
        if sample.cut.is_empty() {
            if let Some(episode) = current.take() {
                episodes.push(episode);
            }
            continue;
        }
        let episode = current.get_or_insert_with(|| Episode {
            start: sample.time,
            end: sample.time,
            samples: 0,
            faces: Vec::new(),
            segments: Vec::new(),
        });
        episode.end = sample.time;
        episode.samples += 1;
        episode.faces.extend(sample.cut.iter().cloned());
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
        let worst = episode
            .faces
            .iter()
            .max_by(|left, right| {
                left.score
                    .partial_cmp(&right.score)
                    .unwrap_or(Ordering::Equal)
            })
            .expect("un épisode non vide contient au moins un visage");
        let severity = if episode.samples >= 4 {
            Severity::High
        } else {
            Severity::Medium
        };
        let mut segments = episode.segments.iter().take(6).cloned().collect::<Vec<_>>();
        if episode.segments.len() > segments.len() {
            segments.push("…".into());
        }
        let notes = format!(
            "visage coupé à {} sur {} échantillon(s) [{}], source {}, landmarks aux bords {:?}, score {:.3}",
            worst.side,
            episode.samples,
            segments.join(", "),
            worst.source.as_str(),
            worst.edge_landmarks,
            worst.score
        );
        findings.push(
            QaFinding::new(
                DefectType::FaceCut,
                round_to(episode.start, 2),
                Some(round_to(episode.end, 2)),
                severity,
                round_to(f64::from(worst.score.clamp(0.0, 1.0)), 3),
                "stage1_yunet_face",
                notes,
            )
            .with_bbox([
                worst.bbox[0] as f64,
                worst.bbox[1] as f64,
                worst.bbox[2] as f64,
                worst.bbox[3] as f64,
            ]),
        );
    }
    findings
}

fn face_check(evidence: &Stage1FaceEvidence, findings: &[QaFinding]) -> QaCheck {
    if evidence.samples_total == 0 {
        return QaCheck::warn(
            "face_cut",
            format!(
                "aucune frame exploitable ({} erreur(s) d'extraction)",
                evidence.frame_errors
            ),
        );
    }
    let detail = format!(
        "{} échantillon(s), {} avec visage, {} coupé(s), {} erreur(s)",
        evidence.samples_total,
        evidence.samples_with_face,
        evidence.samples_with_cut,
        evidence.frame_errors
    );
    if findings.is_empty() {
        QaCheck::pass("face_cut", detail)
    } else {
        QaCheck::warn(
            "face_cut",
            format!("{} épisode(s) de visage coupé ; {detail}", findings.len()),
        )
    }
}

fn letterbox(source: &BgrFrame) -> (BgrFrame, f32, usize, usize) {
    let scale =
        (INPUT_SIZE as f32 / source.width as f32).min(INPUT_SIZE as f32 / source.height as f32);
    let width = ((source.width as f32 * scale).round() as usize).clamp(1, INPUT_SIZE);
    let height = ((source.height as f32 * scale).round() as usize).clamp(1, INPUT_SIZE);
    let offset_x = (INPUT_SIZE - width) / 2;
    let offset_y = (INPUT_SIZE - height) / 2;
    let resized = resize_bilinear(source, width, height);
    let mut data = vec![0u8; INPUT_SIZE * INPUT_SIZE * 3];
    for y in 0..height {
        let source_offset = y * width * 3;
        let target_offset = ((y + offset_y) * INPUT_SIZE + offset_x) * 3;
        data[target_offset..target_offset + width * 3]
            .copy_from_slice(&resized.data[source_offset..source_offset + width * 3]);
    }
    (
        BgrFrame {
            width: INPUT_SIZE,
            height: INPUT_SIZE,
            data,
        },
        scale,
        offset_x,
        offset_y,
    )
}

fn pad_replicate(source: &BgrFrame, pad_x: usize, pad_y: usize) -> BgrFrame {
    let width = source.width + 2 * pad_x;
    let height = source.height + 2 * pad_y;
    let mut data = vec![0u8; width * height * 3];
    for y in 0..height {
        let source_y = (y as isize - pad_y as isize).clamp(0, source.height as isize - 1) as usize;
        for x in 0..width {
            let source_x =
                (x as isize - pad_x as isize).clamp(0, source.width as isize - 1) as usize;
            let pixel = source.pixel(source_x, source_y);
            let offset = (y * width + x) * 3;
            data[offset..offset + 3].copy_from_slice(&pixel);
        }
    }
    BgrFrame {
        width,
        height,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(color: [u8; 3]) -> BgrFrame {
        BgrFrame::new(4, 3, color.repeat(4 * 3)).unwrap()
    }

    fn face(bbox: [f32; 4], landmarks: [[f32; 2]; 5], score: f32) -> RawFace {
        RawFace {
            bbox,
            landmarks,
            score,
            source: FaceSource::Original,
        }
    }

    #[test]
    fn decode_level_extracts_one_face_and_scales_to_frame() {
        let mut cls = vec![0.0f32; 4];
        let mut obj = vec![0.0f32; 4];
        let mut bbox = vec![0.0f32; 16];
        let mut kps = vec![0.0f32; 40];
        cls[0] = 0.9;
        obj[0] = 0.8;
        bbox[2] = 0.0;
        bbox[3] = 0.0;
        for index in 0..5 {
            kps[index * 2] = 0.5;
            kps[index * 2 + 1] = 0.5;
        }
        let faces = decode_level_grid(
            &cls,
            &obj,
            &bbox,
            &kps,
            8,
            2,
            2,
            2.0,
            3.0,
            0.0,
            0.0,
            FaceSource::Original,
            0.5,
        )
        .unwrap();
        assert_eq!(faces.len(), 1);
        let first = &faces[0];
        assert!((first.score - 0.8485).abs() < 1e-4);
        assert_eq!(first.bbox[2], 16.0);
        assert_eq!(first.bbox[3], 24.0);
    }

    #[test]
    fn nms_keeps_the_best_overlapping_face() {
        let first = face([0.0, 0.0, 100.0, 100.0], [[50.0; 2]; 5], 0.9);
        let second = face([5.0, 5.0, 100.0, 100.0], [[50.0; 2]; 5], 0.8);
        let kept = nms(vec![second, first], 0.3);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].score, 0.9);
    }

    #[test]
    fn classify_requires_landmarks_near_the_frame_edge() {
        let centered = face(
            [400.0, 800.0, 300.0, 300.0],
            [
                [540.0, 900.0],
                [700.0, 900.0],
                [620.0, 980.0],
                [560.0, 1050.0],
                [680.0, 1050.0],
            ],
            0.9,
        );
        assert!(classify_face(&centered, 1080, 1920).is_none());

        let mut cut = centered.clone();
        cut.landmarks[0] = [10.0, 900.0];
        let classified = classify_face(&cut, 1080, 1920).unwrap();
        assert_eq!(classified.side, "left");
        assert_eq!(classified.edge_landmarks, vec![0]);
    }

    #[test]
    fn classify_detects_a_landmark_near_the_right_edge() {
        let mut cut = face(
            [750.0, 800.0, 300.0, 300.0],
            [
                [800.0, 900.0],
                [900.0, 900.0],
                [850.0, 980.0],
                [810.0, 1050.0],
                [890.0, 1050.0],
            ],
            0.9,
        );
        cut.landmarks[1] = [1070.0, 900.0];
        let classified = classify_face(&cut, 1080, 1920).unwrap();
        assert_eq!(classified.side, "right");
        assert_eq!(classified.edge_landmarks, vec![1]);
    }

    #[test]
    fn letterbox_preserves_aspect_and_centers_the_image() {
        let source = BgrFrame::new(1080, 1920, vec![7; 1080 * 1920 * 3]).unwrap();
        let (letterboxed, scale, offset_x, offset_y) = letterbox(&source);
        assert_eq!(letterboxed.width, 640);
        assert_eq!(letterboxed.height, 640);
        assert!((scale - 1.0 / 3.0).abs() < 1e-6);
        assert_eq!(offset_x, 140);
        assert_eq!(offset_y, 0);
        assert_eq!(letterboxed.pixel(0, 0), [0, 0, 0]);
        assert_eq!(letterboxed.pixel(140, 0), [7, 7, 7]);
    }

    #[test]
    fn classify_ignores_small_faces() {
        let small = face([10.0, 10.0, 20.0, 20.0], [[10.0; 2]; 5], 0.9);
        assert!(classify_face(&small, 1080, 1920).is_none());
    }

    #[test]
    fn aggregate_requires_three_consecutive_samples() {
        let cut = CutFace {
            bbox: [10, 10, 100, 100],
            edge_landmarks: vec![0],
            side: "left",
            score: 0.9,
            source: FaceSource::Original,
        };
        let samples = vec![
            FaceSample {
                segment: "1".into(),
                time: 1.0,
                cut: vec![cut.clone()],
            },
            FaceSample {
                segment: "1".into(),
                time: 2.0,
                cut: vec![cut.clone()],
            },
            FaceSample {
                segment: "1".into(),
                time: 3.0,
                cut: vec![cut],
            },
        ];
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::FaceCut);
        assert_eq!(findings[0].timecode_start_sec, 1.0);
        assert_eq!(findings[0].timecode_end_sec, Some(3.0));
    }

    #[test]
    fn aggregate_skips_two_samples() {
        let cut = CutFace {
            bbox: [10, 10, 100, 100],
            edge_landmarks: vec![0],
            side: "left",
            score: 0.9,
            source: FaceSource::Original,
        };
        let samples = vec![
            FaceSample {
                segment: "1".into(),
                time: 1.0,
                cut: vec![cut.clone()],
            },
            FaceSample {
                segment: "1".into(),
                time: 2.0,
                cut: vec![cut],
            },
        ];
        assert!(aggregate(&samples).is_empty());
    }

    #[test]
    fn pad_replicates_edges() {
        let source = frame([10, 20, 30]);
        let padded = pad_replicate(&source, 1, 2);
        assert_eq!(padded.width, 6);
        assert_eq!(padded.height, 7);
        assert_eq!(padded.pixel(0, 0), [10, 20, 30]);
        assert_eq!(padded.pixel(5, 6), [10, 20, 30]);
    }

    #[test]
    fn resize_preserves_a_uniform_frame() {
        let source = frame([1, 2, 3]);
        let resized = resize_bilinear(&source, 8, 8);
        assert_eq!(resized.width, 8);
        assert_eq!(resized.height, 8);
        assert_eq!(resized.pixel(0, 0), [1, 2, 3]);
        assert_eq!(resized.pixel(7, 7), [1, 2, 3]);
    }

    #[test]
    fn face_check_requests_review_on_findings() {
        let evidence = Stage1FaceEvidence {
            model: FACE_MODEL_FILENAME.into(),
            samples_total: 10,
            samples_with_face: 8,
            samples_with_cut: 3,
            frame_errors: 0,
            per_segment: 1,
            score_threshold: 0.6,
            nms_threshold: 0.3,
            pad_ratio: 0.15,
        };
        let check = face_check(
            &evidence,
            &[QaFinding::new(
                DefectType::FaceCut,
                1.0,
                Some(3.0),
                Severity::Medium,
                0.9,
                "stage1_yunet_face",
                "test",
            )],
        );
        assert_eq!(check.status, crate::qa::model::QaStatus::Review);
    }
}
