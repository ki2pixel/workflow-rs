//! Stage 1 OCR — sous-titres incrustés, PP-OCRv6 sur ONNX Runtime CPU.
//!
//! Le contrôle reprend les seuils validés dans le PoC : seule la bande basse
//! (25 % inférieurs) est lue, la détection DB renvoie des lignes de texte, la
//! reconnaissance CTC donne texte et score, puis des filtres géométriques
//! écartent logos, cartons et textes de scène. Un texte n'est retenu qu'après
//! trois échantillons d'un même cluster stable ; la dominance du texte
//! distingue un bandeau fixe d'un sous-titre incrusté.
//!
//! Différence assumée avec le PoC : les boîtes sont axis-aligned. Les
//! sous-titres broadcast le sont aussi ; les quadrilatères tournés ne sont pas
//! nécessaires ici et évitent d'embarquer un offsetter de polygones.
//!
//! Aucune écriture : le module retourne des checks et des findings, et laisse
//! `qa.json` à `report::write_report`.

use crate::post::detect;
use crate::qa::logo;
use crate::qa::model::{
    DefectType, ModelInfo, QaCheck, QaFinding, QaSpecs, Severity, Stage1LogoEvidence,
    Stage1LogoVisualEvidence, Stage1OcrEvidence,
};
use crate::qa::overlay::OverlayMask;
use crate::qa::sample::{sample_times, SegmentSpan};
use crate::vision::dino::DinoOptions;
use crate::vision::frame::{grab_frame, BgrFrame};
use crate::vision::ocr::{DetBox, DetEngine, RecEngine};
pub use crate::vision::ocr::{OCR_DET_FILENAME, OCR_LICENSE, OCR_REC_FILENAME};
use crate::vision::text::{bbox_contains, normalize_text};
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;
use workflow_schema::round::round_to;

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
const MIN_CLUSTER_COUNT: usize = 3;
const MAX_Y_DRIFT: f64 = 0.04;
/// Un bandeau fixe doit être présent sur au moins 60 % des échantillons et
/// son texte dominant doit couvrir 80 % du cluster. En dessous, le texte est
/// considéré comme changeant (sous-titres probables).
const FIXED_PERSISTENCE: f64 = 0.60;
const FIXED_DOMINANT_RATIO: f64 = 0.80;

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
    /// Second étage visuel DINOv2, optionnel.
    pub logo_visual: Option<DinoOptions>,
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
            logo_visual: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub checks: Vec<QaCheck>,
    pub findings: Vec<QaFinding>,
    pub evidence: Stage1OcrEvidence,
    pub logo_evidence: Option<Stage1LogoEvidence>,
    pub logo_visual_evidence: Option<Stage1LogoVisualEvidence>,
    pub det_model: ModelInfo,
    pub rec_model: ModelInfo,
    pub logo_visual_model: Option<ModelInfo>,
    /// Temps du contrôle sous-titres, hors passe logos.
    pub timing_ms: u64,
    /// Temps de la passe logos, quand elle est active.
    pub logo_timing_ms: u64,
    /// Part du temps logos consacrée à DINOv2.
    pub logo_visual_timing_ms: u64,
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
    detections: Vec<Detection>,
}

pub fn run_ocr(
    video: &Path,
    specs: &QaSpecs,
    segments: &[SegmentSpan],
    options: &OcrOptions,
    logo_mask: Option<&OverlayMask>,
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
        samples.push(OcrSample {
            segment: item.segment,
            time: item.time,
            detections,
        });
    }

    let subtitle_timing_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let ocr_findings = aggregate(&samples, specs.height as f64);
    let subtitle_total = ocr_findings
        .iter()
        .filter(|finding| finding.defect_type == DefectType::SubtitleBurnedIn)
        .count();
    let fixed_total = ocr_findings
        .iter()
        .filter(|finding| finding.defect_type == DefectType::FixedOverlay)
        .count();
    let evidence = Stage1OcrEvidence {
        det_model: det_model.name.clone(),
        rec_model: rec_model.name.clone(),
        samples_total: samples.len(),
        samples_with_text,
        detections_total,
        findings_total: subtitle_total,
        frame_errors,
        per_segment: options.per_segment,
        min_score: MIN_SCORE,
        bottom_start: BOTTOM_START,
        fixed_overlays_total: fixed_total,
    };
    let mut checks = vec![ocr_check(&evidence)];
    if fixed_total > 0 {
        checks.push(QaCheck::warn(
            "fixed_overlay",
            format!(
                "{fixed_total} bandeau(x) fixe(s) détecté(s) ; vérifier l'intentionnalité \
                 (habillage de chaîne ou défaut)"
            ),
        ));
    }
    let mut findings = ocr_findings;
    let mut logo_evidence = None;
    let mut logo_visual_evidence = None;
    let mut logo_visual_model = None;
    let mut logo_timing_ms = 0;
    let mut logo_visual_timing_ms = 0;
    if let Some(mask) = logo_mask {
        let result = logo::run_logo(
            video,
            specs,
            mask,
            options,
            &det_model.name,
            &rec_model.name,
            &mut det,
            &mut rec,
            options.logo_visual.as_ref(),
        )?;
        checks.push(result.check);
        findings.extend(result.findings);
        logo_evidence = Some(result.evidence);
        logo_visual_evidence = result.visual_evidence;
        logo_visual_model = result.visual_model;
        logo_timing_ms = result.timing_ms;
        logo_visual_timing_ms = result.visual_timing_ms;
    }
    Ok(OcrResult {
        checks,
        findings,
        evidence,
        logo_evidence,
        logo_visual_evidence,
        det_model,
        rec_model,
        logo_visual_model,
        timing_ms: subtitle_timing_ms,
        logo_timing_ms,
        logo_visual_timing_ms,
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

/// Regroupe les détections par boîte stable, puis décide s'il s'agit d'un
/// bandeau fixe (texte persistant et dominant) ou d'un sous-titre incrusté
/// (texte qui change dans la même zone). Les clusters de moins de trois
/// échantillons restent du bruit de décor.
///
/// Le texte n'entre pas dans la clé de regroupement : deux sous-titres
/// successifs partagent la même boîte mais pas le même texte, et doivent
/// rester un seul cluster pour que leur dominance basse les classe
/// `subtitle_burned_in`. Le texte normalisé sert uniquement à mesurer cette
/// dominance.
fn aggregate(samples: &[OcrSample], frame_height: f64) -> Vec<QaFinding> {
    struct Cluster {
        variants: Vec<(String, String, usize)>,
        first: f64,
        last: f64,
        count: usize,
        scores: Vec<f64>,
        ys: Vec<f64>,
        last_bbox: [i64; 4],
        union_bbox: [i64; 4],
        segments: Vec<String>,
        last_sample: usize,
    }

    if samples.is_empty() || frame_height <= 0.0 {
        return Vec::new();
    }
    let total = samples.len();
    let mut clusters: Vec<Cluster> = Vec::new();
    for (sample_index, sample) in samples.iter().enumerate() {
        for detection in &sample.detections {
            let normalized = normalize_text(&detection.text);
            if normalized.is_empty() {
                continue;
            }
            let index = clusters
                .iter()
                .position(|cluster| boxes_similar(cluster.last_bbox, detection.bbox));
            let index = match index {
                Some(index) => index,
                None => {
                    clusters.push(Cluster {
                        variants: vec![(normalized.clone(), detection.text.clone(), 0)],
                        first: sample.time,
                        last: sample.time,
                        count: 0,
                        scores: Vec::new(),
                        ys: Vec::new(),
                        last_bbox: detection.bbox,
                        union_bbox: detection.bbox,
                        segments: Vec::new(),
                        last_sample: usize::MAX,
                    });
                    clusters.len() - 1
                }
            };
            let cluster = &mut clusters[index];
            // Un même emplacement ne compte qu'une fois par échantillon, même
            // si l'OCR renvoie deux variantes de la même ligne.
            if cluster.last_sample == sample_index {
                continue;
            }
            cluster.last_sample = sample_index;
            cluster.count += 1;
            cluster.last = sample.time;
            cluster.scores.push(detection.score);
            cluster
                .ys
                .push((detection.bbox[1] as f64 + detection.bbox[3] as f64 / 2.0) / frame_height);
            if let Some((_, _, count)) = cluster
                .variants
                .iter_mut()
                .find(|(variant, _, _)| *variant == normalized)
            {
                *count += 1;
            } else {
                cluster
                    .variants
                    .push((normalized, detection.text.clone(), 1));
            }
            cluster.last_bbox = detection.bbox;
            let right = (cluster.union_bbox[0] + cluster.union_bbox[2])
                .max(detection.bbox[0] + detection.bbox[2]);
            let bottom = (cluster.union_bbox[1] + cluster.union_bbox[3])
                .max(detection.bbox[1] + detection.bbox[3]);
            cluster.union_bbox[0] = cluster.union_bbox[0].min(detection.bbox[0]);
            cluster.union_bbox[1] = cluster.union_bbox[1].min(detection.bbox[1]);
            cluster.union_bbox[2] = right - cluster.union_bbox[0];
            cluster.union_bbox[3] = bottom - cluster.union_bbox[1];
            cluster.segments.push(sample.segment.clone());
        }
    }

    let mut findings = Vec::new();
    for cluster in clusters {
        if cluster.count < MIN_CLUSTER_COUNT {
            continue;
        }
        let (Some(y_min), Some(y_max)) = (
            cluster.ys.iter().copied().reduce(f64::min),
            cluster.ys.iter().copied().reduce(f64::max),
        ) else {
            continue;
        };
        if y_max - y_min > MAX_Y_DRIFT {
            continue;
        }
        let persistence = cluster.count as f64 / total as f64;
        let dominant = cluster
            .variants
            .iter()
            .map(|(_, _, count)| *count)
            .max()
            .unwrap_or(0);
        let dominant_ratio = dominant as f64 / cluster.count as f64;
        let fixed = persistence >= FIXED_PERSISTENCE && dominant_ratio >= FIXED_DOMINANT_RATIO;
        let mean_score = cluster.scores.iter().sum::<f64>() / cluster.scores.len() as f64;
        let center_y = cluster.ys.iter().sum::<f64>() / cluster.ys.len() as f64;
        let mut texts = cluster
            .variants
            .iter()
            .take(4)
            .map(|(_, text, _)| text.clone())
            .collect::<Vec<_>>();
        if cluster.variants.len() > texts.len() {
            texts.push("…".into());
        }
        let mut segments = cluster.segments.iter().take(6).cloned().collect::<Vec<_>>();
        if cluster.segments.len() > segments.len() {
            segments.push("…".into());
        }
        let (defect_type, severity, notes) = if fixed {
            (
                DefectType::FixedOverlay,
                if persistence >= 0.8 {
                    Severity::Medium
                } else {
                    Severity::Low
                },
                format!(
                    "bandeau fixe sur {} échantillon(s) [{}], texte: {}, persistance {:.0}%, \
                     dominance {:.0}%, score moyen {:.3}, centre y {:.4}",
                    cluster.count,
                    segments.join(", "),
                    texts.join(" / "),
                    persistence * 100.0,
                    dominant_ratio * 100.0,
                    mean_score,
                    center_y
                ),
            )
        } else {
            (
                DefectType::SubtitleBurnedIn,
                if cluster.count >= 4 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                format!(
                    "sous-titre incrusté sur {} échantillon(s) [{}], textes: {}, score moyen {:.3}, \
                     dominance {:.0}%, centre y {:.4}",
                    cluster.count,
                    segments.join(", "),
                    texts.join(" / "),
                    mean_score,
                    dominant_ratio * 100.0,
                    center_y
                ),
            )
        };
        findings.push(
            QaFinding::new(
                defect_type,
                round_to(cluster.first, 2),
                Some(round_to(cluster.last, 2)),
                severity,
                round_to(mean_score.clamp(0.0, 1.0), 3),
                "stage1_ppocr_bottom",
                notes,
            )
            .with_bbox([
                cluster.union_bbox[0] as f64,
                cluster.union_bbox[1] as f64,
                cluster.union_bbox[2] as f64,
                cluster.union_bbox[3] as f64,
            ]),
        );
    }
    findings
}

/// Deux lignes OCR partagent le même emplacement si l'une contient l'autre,
/// si leur IoU atteint 0,3, ou si leurs centres sont proches sur la même
/// ligne avec des tailles comparables. Le seuil vertical est volontairement
/// strict (demi-hauteur) : un sous-titre au-dessus d'un bandeau ne doit pas
/// être fusionné avec lui.
fn boxes_similar(left: [i64; 4], right: [i64; 4]) -> bool {
    if bbox_contains(left, right) || bbox_contains(right, left) {
        return true;
    }
    let area_left = (left[2].max(1) * left[3].max(1)) as f64;
    let area_right = (right[2].max(1) * right[3].max(1)) as f64;
    let intersection_width = (left[0] + left[2]).min(right[0] + right[2]) - left[0].max(right[0]);
    let intersection_height = (left[1] + left[3]).min(right[1] + right[3]) - left[1].max(right[1]);
    let intersection = (intersection_width.max(0) * intersection_height.max(0)) as f64;
    let union = area_left + area_right - intersection;
    if union > 0.0 && intersection / union >= 0.3 {
        return true;
    }
    let center_left = (
        left[0] as f64 + left[2] as f64 / 2.0,
        left[1] as f64 + left[3] as f64 / 2.0,
    );
    let center_right = (
        right[0] as f64 + right[2] as f64 / 2.0,
        right[1] as f64 + right[3] as f64 / 2.0,
    );
    let dx = (center_left.0 - center_right.0).abs();
    let dy = (center_left.1 - center_right.1).abs();
    let max_width = left[2].max(right[2]) as f64;
    let max_height = left[3].max(right[3]) as f64;
    let area_ratio = area_left / area_right;
    dx <= 0.25 * max_width && dy <= 0.5 * max_height && (0.5..=2.0).contains(&area_ratio)
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
        "{} échantillon(s), {} avec texte, {} détection(s), {} bandeau(x) fixe(s), {} erreur(s)",
        evidence.samples_total,
        evidence.samples_with_text,
        evidence.detections_total,
        evidence.fixed_overlays_total,
        evidence.frame_errors
    );
    if evidence.findings_total == 0 {
        QaCheck::pass("subtitle_ocr", detail)
    } else {
        QaCheck::warn(
            "subtitle_ocr",
            format!(
                "{} cluster(s) de sous-titre incrusté ; {detail}",
                evidence.findings_total
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: usize, height: usize, color: [u8; 3]) -> BgrFrame {
        BgrFrame::new(width, height, color.repeat(width * height)).unwrap()
    }

    const FRAME_HEIGHT: f64 = 1920.0;

    fn detection(text: &str, y: i64) -> Detection {
        Detection {
            text: text.into(),
            score: 0.9,
            bbox: [100, y, 800, 70],
        }
    }

    fn sample(index: usize, detections: Vec<Detection>) -> OcrSample {
        OcrSample {
            segment: "1".into(),
            time: index as f64,
            detections,
        }
    }

    #[test]
    fn bottom_band_keeps_the_last_quarter() {
        let image = frame(4, 8, [1, 2, 3]);
        let band = bottom_band(&image);
        assert_eq!(band.height, 2);
        assert_eq!(band.pixel(0, 0), [1, 2, 3]);
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
    fn aggregate_promotes_a_persistent_text_to_fixed_overlay() {
        let samples = (0..3)
            .map(|index| sample(index, vec![detection("BANDEAU FIXE", 1500)]))
            .collect::<Vec<_>>();
        let findings = aggregate(&samples, FRAME_HEIGHT);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::FixedOverlay);
        assert_eq!(findings[0].timecode_start_sec, 0.0);
        assert_eq!(findings[0].timecode_end_sec, Some(2.0));
        assert_eq!(findings[0].bbox_xywh, Some([100.0, 1500.0, 800.0, 70.0]));
    }

    #[test]
    fn aggregate_separates_fixed_banner_from_transient_subtitles() {
        let samples = (0..10)
            .map(|index| {
                let mut detections = vec![detection("BANDEAU FIXE", 1500)];
                if index < 2 {
                    detections.push(detection("SOUS TITRE", 1400));
                }
                sample(index, detections)
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples, FRAME_HEIGHT);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::FixedOverlay);
        // Le sous-titre au-dessus ne doit pas élargir la boîte du bandeau.
        assert_eq!(findings[0].bbox_xywh, Some([100.0, 1500.0, 800.0, 70.0]));
    }

    #[test]
    fn aggregate_reports_changing_banner_text_as_subtitle() {
        let texts = ["ALPHA", "BRAVO", "CHARLIE"];
        let samples = (0..9)
            .map(|index| sample(index, vec![detection(texts[index % texts.len()], 1500)]))
            .collect::<Vec<_>>();
        let findings = aggregate(&samples, FRAME_HEIGHT);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::SubtitleBurnedIn);
    }

    #[test]
    fn aggregate_marks_mixed_variants_as_subtitle() {
        let samples = (0..6)
            .map(|index| {
                let text = if index % 2 == 0 {
                    "CLIENT HOTEL"
                } else {
                    "CLIENT HOTES"
                };
                sample(index, vec![detection(text, 1500)])
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples, FRAME_HEIGHT);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::SubtitleBurnedIn);
    }

    #[test]
    fn aggregate_ignores_one_or_two_occurrences() {
        let samples = (0..3)
            .map(|index| {
                let detections = if index < 2 {
                    vec![detection("TEXTE ISOLE", 1500)]
                } else {
                    Vec::new()
                };
                sample(index, detections)
            })
            .collect::<Vec<_>>();
        assert!(aggregate(&samples, FRAME_HEIGHT).is_empty());
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
        let samples = (0..8)
            .map(|index| {
                let detections = if index < 4 {
                    vec![detection("SOUS-TITRE", 1500)]
                } else {
                    Vec::new()
                };
                sample(index, detections)
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples, FRAME_HEIGHT);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::SubtitleBurnedIn);
        assert_eq!(findings[0].severity, Severity::High);
    }

    #[test]
    fn aggregate_skips_unstable_vertical_position() {
        let mut samples = (0..3)
            .map(|index| sample(index, vec![detection("BANDEAU FIXE", 1500)]))
            .collect::<Vec<_>>();
        samples[2].detections[0].bbox[1] = 1620;
        assert!(aggregate(&samples, FRAME_HEIGHT).is_empty());
    }
}
