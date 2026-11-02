//! Stage 1 logos — texte persistant de la bande haute.
//!
//! Le contrôle reprend le baseline texte du PoC : douze frames réparties sur
//! l'export, OCR de la bande haute (32 % supérieurs), puis conservation des
//! textes présents sur au moins la moitié des échantillons. Un texte stable
//! dans cette bande est un logo de chaîne ou un rating candidat.
//!
//! Le masque des overlays déclarés dans le `.ecproj` est appliqué avant
//! l'agrégation : les habillages intentionnels (sticker SDM, logo M6+, titre
//! d'émission) ne doivent pas devenir des findings. Sans projet, le contrôle
//! est marqué `not_applicable` par l'orchestrateur : un OCR de bande haute
//! seul confond logo et habillage éditorial.
//!
//! L'agrégation regroupe d'abord le texte exact, puis fusionne les variantes
//! OCR proches dans la même zone (M6+ lu « 161+ », « 11+ », « 1+ »), y compris
//! les boîtes imbriquées (« e=m6 » / « =m6 »), sans fusionner deux logos
//! éloignés.

use crate::qa::frame::{grab_frame, BgrFrame};
use crate::qa::model::{DefectType, QaCheck, QaFinding, QaSpecs, Severity, Stage1LogoEvidence};
use crate::qa::ocr::{DetBox, DetEngine, OcrOptions, RecEngine};
use crate::qa::overlay::OverlayMask;
use anyhow::Result;
use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;
use workflow_schema::round::round_to;

pub const TOP_END: f64 = 0.32;
pub const SAMPLES_PER_VIDEO: usize = 12;
pub const MIN_SCORE: f64 = 0.50;
pub const MIN_TEXT_LEN: usize = 2;
pub const MIN_BOX_HEIGHT: i64 = 18;
pub const MAX_BOX_HEIGHT: i64 = 180;
pub const MIN_BOX_WIDTH: i64 = 30;
pub const MIN_ASPECT: f64 = 1.2;
pub const PERSISTENCE: f64 = 0.5;
/// Part du plus petit rectangle (détection ou overlay) couverte par
/// l'intersection ; une boîte OCR qui contient l'overlay est donc écartée.
pub const MASK_OVERLAP: f64 = 0.5;

#[derive(Debug, Clone)]
pub(crate) struct LogoResult {
    pub(crate) check: QaCheck,
    pub(crate) findings: Vec<QaFinding>,
    pub(crate) evidence: Stage1LogoEvidence,
    pub(crate) timing_ms: u64,
}

#[derive(Debug, Clone)]
struct LogoDetection {
    text: String,
    normalized: String,
    score: f64,
    bbox: [i64; 4],
}

#[derive(Debug, Clone)]
struct LogoSample {
    time: f64,
    detections: Vec<LogoDetection>,
}

#[derive(Debug, Clone)]
struct Group {
    key: String,
    text: String,
    variants: Vec<String>,
    first: f64,
    last: f64,
    count: usize,
    scores: Vec<f64>,
    bbox: [i64; 4],
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_logo(
    video: &Path,
    specs: &QaSpecs,
    mask: &OverlayMask,
    options: &OcrOptions,
    det_model: &str,
    rec_model: &str,
    det: &mut DetEngine,
    rec: &mut RecEngine,
) -> Result<LogoResult> {
    let started = Instant::now();
    let mut samples = Vec::with_capacity(SAMPLES_PER_VIDEO);
    let mut frame_errors = 0usize;
    let mut detections_total = 0usize;
    let mut masked_total = 0usize;
    let mut samples_with_text = 0usize;

    for index in 0..SAMPLES_PER_VIDEO {
        let time = specs.duration_s * (index as f64 + 0.5) / SAMPLES_PER_VIDEO as f64;
        let frame = match grab_frame(
            &options.ffmpeg,
            video,
            time,
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
                    time,
                    error = %error,
                    "extraction de frame logo en échec"
                );
                frame_errors += 1;
                continue;
            }
        };
        let band = top_band(&frame);
        let boxes = det.detect(&band)?;
        let texts = rec.recognize(&band, &boxes)?;
        let mut detections = Vec::new();
        for ((text, score), det_box) in texts.into_iter().zip(boxes.iter()) {
            tracing::trace!(
                time,
                text = %text,
                score,
                bbox = ?det_box.bbox,
                "OCR brut bande haute"
            );
            let Some(detection) = filter_detection(&text, score, det_box) else {
                continue;
            };
            detections_total += 1;
            tracing::debug!(
                time,
                text = %detection.text,
                score = detection.score,
                bbox = ?detection.bbox,
                "détection logo bande haute"
            );
            let bbox = [
                detection.bbox[0] as f64,
                detection.bbox[1] as f64,
                detection.bbox[2] as f64,
                detection.bbox[3] as f64,
            ];
            let overlap = mask.overlap_ratio(bbox, time, frame.width as u32, frame.height as u32);
            if overlap >= MASK_OVERLAP {
                masked_total += 1;
                tracing::debug!(
                    time,
                    text = %detection.text,
                    overlap = round_to(overlap, 3),
                    "texte de bande haute masqué par un overlay déclaré"
                );
                continue;
            }
            detections.push(detection);
        }
        if !detections.is_empty() {
            samples_with_text += 1;
        }
        samples.push(LogoSample { time, detections });
    }

    let findings = aggregate(&samples);
    let evidence = Stage1LogoEvidence {
        det_model: det_model.to_string(),
        rec_model: rec_model.to_string(),
        samples_total: samples.len(),
        samples_with_text,
        detections_total,
        masked_total,
        findings_total: findings.len(),
        frame_errors,
        persistence_min: PERSISTENCE,
        top_end: TOP_END,
        overlays_total: mask.regions.len(),
        overlays: mask.regions.clone(),
        overlays_skipped: mask.skipped.clone(),
    };
    let check = logo_check(&evidence);
    Ok(LogoResult {
        check,
        findings,
        evidence,
        timing_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn top_band(frame: &BgrFrame) -> BgrFrame {
    let height = ((frame.height as f64 * TOP_END) as usize).max(1);
    frame.crop(0, 0, frame.width, height)
}

/// Filtres du baseline PoC. La bande commence à y=0 : les coordonnées de la
/// boîte sont déjà celles de la frame complète.
fn filter_detection(text: &str, score: f64, det_box: &DetBox) -> Option<LogoDetection> {
    let clean = text.trim();
    if score < MIN_SCORE || clean.chars().count() < MIN_TEXT_LEN {
        return None;
    }
    let [x1, y1, x2, y2] = det_box.bbox;
    let width = x2 - x1;
    let height = y2 - y1;
    if !(MIN_BOX_HEIGHT..=MAX_BOX_HEIGHT).contains(&height) || width < MIN_BOX_WIDTH {
        return None;
    }
    let aspect = width as f64 / height.max(1) as f64;
    if aspect < MIN_ASPECT {
        return None;
    }
    let normalized = normalize_text(clean);
    if normalized.is_empty() {
        return None;
    }
    Some(LogoDetection {
        text: clean.to_string(),
        normalized,
        score: round_to(score, 3),
        bbox: [x1, y1, width, height],
    })
}

/// Clé de persistance : minuscules, caractères alphanumériques seulement.
fn normalize_text(text: &str) -> String {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Fusion prudente des variantes OCR d'un même logo : texte proche et boîte
/// au même endroit. Deux logos distincts de même texte restent fusionnés par
/// la clé exacte, comme dans le baseline.
fn can_merge(group: &Group, detection: &LogoDetection) -> bool {
    if group.bbox[2] <= 0 || group.bbox[3] <= 0 {
        return false;
    }
    text_distance(&group.key, &detection.normalized) <= max_edits(&group.key, &detection.normalized)
        && text_similarity(&group.key, &detection.normalized) >= 0.6
        && bbox_similarity(group.bbox, detection.bbox)
}

fn max_edits(left: &str, right: &str) -> usize {
    let shortest = left.chars().count().min(right.chars().count());
    if shortest <= 4 {
        1
    } else {
        2
    }
}

fn text_similarity(left: &str, right: &str) -> f64 {
    let longest = left.chars().count().max(right.chars().count());
    if longest == 0 {
        return 1.0;
    }
    1.0 - text_distance(left, right) as f64 / longest as f64
}

fn text_distance(left: &str, right: &str) -> usize {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0usize; right.len() + 1];
    for (i, left_char) in left.iter().enumerate() {
        current[0] = i + 1;
        for (j, right_char) in right.iter().enumerate() {
            let substitution = previous[j] + usize::from(left_char != right_char);
            current[j + 1] = (current[j] + 1).min(previous[j + 1] + 1).min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

/// Même zone : une boîte contenue dans l'autre, un IoU suffisant, ou des
/// centres proches avec une taille comparable. La containment couvre les
/// recadrages OCR d'un même logo (« e=m6 », « =m6 », « m6 »).
fn bbox_similarity(left: [i64; 4], right: [i64; 4]) -> bool {
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
    let distance = ((center_left.0 - center_right.0).powi(2)
        + (center_left.1 - center_right.1).powi(2))
    .sqrt();
    let scale = left[2].max(left[3]).max(right[2]).max(right[3]) as f64;
    let ratio = area_left / area_right;
    distance <= 0.25 * scale && (0.5..=2.0).contains(&ratio)
}

fn bbox_contains(outer: [i64; 4], inner: [i64; 4]) -> bool {
    const MARGIN: i64 = 2;
    inner[0] >= outer[0] - MARGIN
        && inner[1] >= outer[1] - MARGIN
        && inner[0] + inner[2] <= outer[0] + outer[2] + MARGIN
        && inner[1] + inner[3] <= outer[1] + outer[3] + MARGIN
}

fn aggregate(samples: &[LogoSample]) -> Vec<QaFinding> {
    let total = samples.len();
    if total == 0 {
        return Vec::new();
    }
    let mut groups: Vec<Group> = Vec::new();
    for sample in samples {
        let mut seen = HashSet::new();
        let mut used_groups = HashSet::new();
        for detection in &sample.detections {
            if !seen.insert(detection.normalized.clone()) {
                continue;
            }
            let index = groups
                .iter()
                .position(|group| group.key == detection.normalized)
                .or_else(|| groups.iter().position(|group| can_merge(group, detection)));
            if let Some(index) = index {
                // Un même logo ne compte qu'une fois par échantillon, même si
                // l'OCR en produit deux variantes.
                if !used_groups.insert(index) {
                    continue;
                }
                let group = &mut groups[index];
                group.count += 1;
                group.last = sample.time;
                group.scores.push(detection.score);
                if !group.variants.iter().any(|text| text == &detection.text) {
                    group.variants.push(detection.text.clone());
                }
            } else {
                groups.push(Group {
                    key: detection.normalized.clone(),
                    text: detection.text.clone(),
                    variants: vec![detection.text.clone()],
                    first: sample.time,
                    last: sample.time,
                    count: 1,
                    scores: vec![detection.score],
                    bbox: detection.bbox,
                });
                used_groups.insert(groups.len() - 1);
            }
        }
    }

    let mut findings = Vec::new();
    for group in groups {
        let ratio = group.count as f64 / total as f64;
        if ratio < PERSISTENCE {
            continue;
        }
        let mean_score = group.scores.iter().sum::<f64>() / group.scores.len() as f64;
        let severity = if ratio >= 0.8 {
            Severity::Medium
        } else {
            Severity::Low
        };
        let variants = if group.variants.len() > 1 {
            let listed = group
                .variants
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            format!(" (variantes OCR : {listed})")
        } else {
            String::new()
        };
        let notes = format!(
            "texte « {} » persistant sur {}/{} échantillon(s) de la bande haute (0–{:.0} %), score moyen {:.3}{variants}",
            group.text,
            group.count,
            total,
            TOP_END * 100.0,
            mean_score
        );
        findings.push(
            QaFinding::new(
                DefectType::LogoResidual,
                round_to(group.first, 2),
                Some(round_to(group.last, 2)),
                severity,
                round_to(ratio.min(1.0), 3),
                "stage1_ppocr_top",
                notes,
            )
            .with_bbox([
                group.bbox[0] as f64,
                group.bbox[1] as f64,
                group.bbox[2] as f64,
                group.bbox[3] as f64,
            ]),
        );
    }
    findings
}

fn logo_check(evidence: &Stage1LogoEvidence) -> QaCheck {
    if evidence.samples_total == 0 {
        return QaCheck::warn(
            "logo_residual",
            format!(
                "aucune frame exploitable ({} erreur(s) d'extraction)",
                evidence.frame_errors
            ),
        );
    }
    let skipped = if evidence.overlays_skipped.is_empty() {
        String::new()
    } else {
        format!(
            ", {} overlay(s) déclaré(s) ignoré(s)",
            evidence.overlays_skipped.len()
        )
    };
    let detail = format!(
        "{} échantillon(s), {} avec texte hors overlay, {} détection(s), {} masquée(s) par overlay, {} erreur(s){skipped}",
        evidence.samples_total,
        evidence.samples_with_text,
        evidence.detections_total,
        evidence.masked_total,
        evidence.frame_errors
    );
    if evidence.findings_total == 0 {
        QaCheck::pass("logo_residual", detail)
    } else {
        QaCheck::warn(
            "logo_residual",
            format!(
                "{} texte(s) persistant(s) hors overlays déclarés ; {detail}",
                evidence.findings_total
            ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detection(text: &str, bbox: [i64; 4]) -> LogoDetection {
        LogoDetection {
            text: text.into(),
            normalized: normalize_text(text),
            score: 0.9,
            bbox,
        }
    }

    #[test]
    fn normalization_keeps_alphanumerics_and_lowercases() {
        assert_eq!(normalize_text("LES 100"), "les100");
        assert_eq!(normalize_text("É=M6"), "ém6");
    }

    #[test]
    fn filter_rejects_small_or_narrow_boxes() {
        let small = DetBox {
            bbox: [10, 10, 30, 20],
        };
        assert!(filter_detection("100", 0.9, &small).is_none());
        let narrow = DetBox {
            bbox: [10, 10, 40, 60],
        };
        assert!(filter_detection("100", 0.9, &narrow).is_none());
        let valid = DetBox {
            bbox: [100, 200, 260, 250],
        };
        assert!(filter_detection("100", 0.9, &valid).is_some());
    }

    #[test]
    fn aggregate_requires_half_the_samples() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: if index < 5 {
                    vec![detection("100", [70, 280, 240, 140])]
                } else {
                    Vec::new()
                },
            })
            .collect::<Vec<_>>();
        assert!(aggregate(&samples).is_empty());

        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: if index < 6 {
                    vec![detection("100", [70, 280, 240, 140])]
                } else {
                    Vec::new()
                },
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::LogoResidual);
        assert!((findings[0].confidence - 0.5).abs() < 1e-9);
        assert_eq!(findings[0].bbox_xywh, Some([70.0, 280.0, 240.0, 140.0]));
    }

    #[test]
    fn aggregate_merges_ocr_variants_at_the_same_position() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: vec![detection(
                    match index {
                        0..=4 => "161+",
                        5..=6 => "11+",
                        _ => "1+",
                    },
                    [930, 316, 150, 114],
                )],
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert!((findings[0].confidence - 0.583).abs() < 1e-9);
        assert!(findings[0].notes.contains("variantes OCR"));
        assert!(findings[0].notes.contains("11+"));
    }

    #[test]
    fn aggregate_merges_a_tight_box_inside_a_wider_box() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: if index >= 10 {
                    Vec::new()
                } else {
                    vec![match index {
                        0 | 2 | 4 | 6 | 8 => detection("e=m6", [125, 357, 93, 40]),
                        1 => detection("m6", [176, 367, 32, 21]),
                        _ => detection("=m6", [158, 363, 53, 29]),
                    }]
                },
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].notes.contains("10/12"));
    }

    #[test]
    fn aggregate_does_not_merge_distinct_texts_in_nested_boxes() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: vec![detection(
                    if index < 6 { "PARIS" } else { "LONDRES" },
                    if index < 6 {
                        [100, 300, 200, 80]
                    } else {
                        [120, 305, 100, 40]
                    },
                )],
            })
            .collect::<Vec<_>>();
        assert_eq!(aggregate(&samples).len(), 2);
    }

    #[test]
    fn aggregate_does_not_merge_variants_at_distinct_positions() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: vec![detection(
                    if index < 6 { "161+" } else { "11+" },
                    if index < 6 {
                        [100, 300, 150, 100]
                    } else {
                        [700, 300, 150, 100]
                    },
                )],
            })
            .collect::<Vec<_>>();
        assert_eq!(aggregate(&samples).len(), 2);
    }

    #[test]
    fn aggregate_does_not_merge_distinct_texts_at_the_same_position() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: vec![detection(
                    if index < 6 { "PARIS" } else { "LONDRES" },
                    [100, 300, 150, 100],
                )],
            })
            .collect::<Vec<_>>();
        assert_eq!(aggregate(&samples).len(), 2);
    }

    #[test]
    fn aggregate_counts_a_group_once_per_sample() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: if index == 11 {
                    vec![
                        detection("161+", [930, 316, 150, 114]),
                        detection("11+", [930, 316, 150, 114]),
                    ]
                } else {
                    vec![detection("161+", [930, 316, 150, 114])]
                },
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].notes.contains("12/12"));
    }

    #[test]
    fn aggregate_groups_case_and_punctuation_variants() {
        let samples = (0..12)
            .map(|index| LogoSample {
                time: index as f64,
                detections: vec![detection(
                    if index % 2 == 0 { "LES 100" } else { "les-100" },
                    [70, 280, 240, 140],
                )],
            })
            .collect::<Vec<_>>();
        let findings = aggregate(&samples);
        assert_eq!(findings.len(), 1);
        assert!((findings[0].confidence - 1.0).abs() < 1e-9);
    }
}
