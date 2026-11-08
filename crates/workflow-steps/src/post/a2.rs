//! A2 — bannière basse pleine largeur, avec abstentions conservatrices.
//!
//! Une bande haute, un logo, une bande partielle, un décor ou une bande dont
//! la géométrie bouge partent en `review`. Seule une bannière basse large,
//! persistante, claire et géométriquement stable est appliquée. Le gate OCR
//! optionnel (`a2_ocr`) peut confirmer un texte persistant et suppléer une
//! géométrie modérément instable, dans une amplitude bornée ; il ne
//! contourne jamais les garde-fous durs et oppose un veto aux textes
//! changeants.

use crate::post::detect::{band_candidates, probe_video, BandCandidate, VideoSpecs};
use crate::post::ecproj::{CompMeta, Segment};
use crate::post::model::Decision;
use crate::vision::frame::grab_frame;
use crate::vision::ocr::{DetEngine, RecEngine};
use crate::vision::source_logo::{bbox_area, bbox_intersection_area, clip_bbox, SourceLogoPrior};
use crate::vision::text::normalize_text;
use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use workflow_schema::round::round_to;

const BANNER_MIN_WIDTH: f64 = 0.50;
const LOGO_MAX_WIDTH: f64 = 0.35;
const BANNER_MAX_HEIGHT: i64 = 200;
const BANNER_MIN_HEIGHT: i64 = 24;
const DARK_MEDIAN: f64 = 60.0;
const ANIM_STD: f64 = 35.0;
const GEOM_STD: f64 = 12.0;

/// Échantillonnage du gate OCR A2 : même cadence que le contrôle logos QA.
const A2_OCR_SAMPLES: usize = 12;
const A2_OCR_MARGIN_PX: i64 = 16;
const A2_OCR_MIN_CROP_HEIGHT: i64 = 30;
const A2_OCR_MIN_FRAMES: usize = 6;
const A2_OCR_MIN_SCORE: f64 = 0.55;
const A2_OCR_MIN_TEXT_LEN: usize = 2;
const A2_OCR_TEXT_PERSISTENCE: f64 = 0.60;
const A2_OCR_DOMINANT_RATIO: f64 = 0.80;
const A2_OCR_CHANGING_RATIO: f64 = 0.60;
const A2_OCR_Y_DRIFT_1080: f64 = 12.0;
const A2_OCR_SCORE: f64 = 0.80;
const A2_OCR_OVERLAP: f64 = 0.50;
const A2_OCR_COVERAGE_X: f64 = 0.25;
const A2_OCR_INTRA_THREADS: usize = 2;
/// L'override OCR de la géométrie reste borné : au-delà de deux fois
/// `GEOM_STD`, la hauteur du candidat n'est plus fiable pour la formule de
/// scale, même si le texte est stable.
const OCR_GEOM_OVERRIDE_FACTOR: f64 = 2.0;

/// Modèles et runtime du gate OCR A2. Les chemins sont résolus par
/// `PostOptions` depuis `paths.models` quand ils sont relatifs.
#[derive(Debug, Clone)]
pub struct A2OcrOptions {
    pub det_model: PathBuf,
    pub rec_model: PathBuf,
    pub onnxruntime: Option<PathBuf>,
}

/// Politique A2 : marge de scale et gate OCR optionnel.
#[derive(Debug, Clone)]
pub struct A2Options {
    pub margin: f64,
    pub ocr: Option<A2OcrOptions>,
    /// Prior spatial du logo de programme, déjà remis à l'échelle source.
    /// Il ne change pas la décision : il documente le candidat et permet de
    /// savoir si le logo sera visible dans la composition.
    pub logo_prior: Option<SourceLogoPrior>,
}

#[derive(Debug, Clone)]
struct A2OcrEvidence {
    samples: usize,
    samples_with_text: usize,
    detections_total: usize,
    frame_errors: usize,
    dominant_text: String,
    dominant_bbox: [i64; 4],
    texts: Vec<A2OcrText>,
    text_persistence: f64,
    dominant_ratio: f64,
    y_drift_px: f64,
    score: f64,
    overlap: f64,
    coverage_x: f64,
    timing_ms: u64,
}

#[derive(Debug, Clone)]
struct A2OcrText {
    text: String,
    samples: usize,
    coverage_x: f64,
}

impl A2OcrEvidence {
    fn to_json(&self) -> Value {
        let mut object = Map::new();
        object.insert("samples".into(), json!(self.samples));
        object.insert("samples_with_text".into(), json!(self.samples_with_text));
        object.insert("detections_total".into(), json!(self.detections_total));
        object.insert("frame_errors".into(), json!(self.frame_errors));
        object.insert("text".into(), json!(self.dominant_text));
        object.insert("bbox".into(), json!(self.dominant_bbox));
        object.insert("text_persistence".into(), json!(self.text_persistence));
        object.insert("dominant_ratio".into(), json!(self.dominant_ratio));
        object.insert("y_drift_px".into(), json!(self.y_drift_px));
        object.insert("score".into(), json!(self.score));
        object.insert("overlap".into(), json!(self.overlap));
        object.insert("coverage_x".into(), json!(self.coverage_x));
        object.insert(
            "texts".into(),
            Value::Array(
                self.texts
                    .iter()
                    .map(|text| {
                        json!({
                            "text": text.text,
                            "samples": text.samples,
                            "coverage_x": text.coverage_x,
                        })
                    })
                    .collect(),
            ),
        );
        object.insert("timing_ms".into(), json!(self.timing_ms));
        Value::Object(object)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OcrVerdict {
    /// Texte persistant, dominant et stable : bandeau fixe confirmé.
    Confirmed,
    /// Texte présent mais dominant trop faible : sous-titres probables.
    Changing,
    /// Preuve insuffisante : la décision A2 actuelle reste, avec revue si
    /// elle aurait appliqué.
    Ambiguous,
    /// Texte absent ou trop rare : décision A2 actuelle, sans dégradation.
    Sparse,
}

struct OcrGate<'a> {
    video: &'a Path,
    ffmpeg: &'a Path,
    specs: &'a VideoSpecs,
    options: &'a A2OcrOptions,
}

trait OcrEvidenceSource {
    fn evidence(&self, candidate: &BandCandidate) -> Option<A2OcrEvidence>;
}

impl OcrEvidenceSource for OcrGate<'_> {
    fn evidence(&self, candidate: &BandCandidate) -> Option<A2OcrEvidence> {
        match collect_ocr_evidence(self.video, self.ffmpeg, self.specs, candidate, self.options) {
            Ok(evidence) => Some(evidence),
            Err(error) => {
                tracing::warn!("A2 OCR ignoré pour {}: {error:#}", self.video.display());
                None
            }
        }
    }
}

pub fn decide_a2(
    segments: &[Segment],
    video: Option<&Path>,
    ffmpeg: &Path,
    ffprobe: &Path,
    meta: &CompMeta,
    source_height: u32,
    options: &A2Options,
) -> Vec<Decision> {
    let margin = options.margin;
    let ocr = options.ocr.as_ref();
    let Some(video) = video else {
        return vec![Decision::review(
            "A2",
            "comp",
            "vidéo source introuvable, A2 non calculé",
            0.2,
            json!({"reason": "video_missing"}),
        )];
    };
    if meta.width != 1080 || meta.height != 1920 {
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "composition {}x{}, A2 exige 1080x1920",
                meta.width, meta.height
            ),
            0.2,
            json!({"width": meta.width, "height": meta.height}),
        )];
    }
    let candidates = match band_candidates(ffmpeg, ffprobe, video) {
        Ok(candidates) => candidates,
        Err(error) => {
            return vec![Decision::review(
                "A2",
                "comp",
                format!("détection des bandes en échec: {error}"),
                0.2,
                json!({"reason": "band_detection_failed"}),
            )];
        }
    };
    let ocr_specs = match ocr {
        Some(options) if !options.det_model.is_file() || !options.rec_model.is_file() => {
            tracing::warn!(
                "A2 OCR activé mais modèles absents ({} / {}), décision A2 actuelle",
                options.det_model.display(),
                options.rec_model.display()
            );
            None
        }
        Some(_) => match probe_video(ffprobe, video) {
            Ok(specs) => Some(specs),
            Err(error) => {
                tracing::warn!("A2 OCR: ffprobe en échec pour {}: {error}", video.display());
                None
            }
        },
        None => None,
    };
    let gate = match (ocr, ocr_specs.as_ref()) {
        (Some(options), Some(specs)) => Some(OcrGate {
            video,
            ffmpeg,
            specs,
            options,
        }),
        _ => None,
    };
    let mut decisions = decide_impl(
        segments,
        &candidates,
        source_height,
        meta.height as f64,
        margin,
        gate.as_ref().map(|gate| gate as &dyn OcrEvidenceSource),
    );
    if let Some(prior) = options.logo_prior.as_ref() {
        annotate_source_logo(&mut decisions, prior, segments, meta);
    }
    decisions
}

pub fn decide_from_candidates(
    segments: &[Segment],
    candidates: &[BandCandidate],
    source_height: u32,
    comp_height: f64,
    margin: f64,
) -> Vec<Decision> {
    decide_impl(
        segments,
        candidates,
        source_height,
        comp_height,
        margin,
        None,
    )
}

/// Annote les décisions A2 avec le prior spatial du logo, sans changer leur
/// action. La visibilité dans la composition est calculée avec les
/// transformations du projet (keyframes de Position comprises).
fn annotate_source_logo(
    decisions: &mut [Decision],
    prior: &SourceLogoPrior,
    segments: &[Segment],
    meta: &CompMeta,
) {
    for decision in decisions {
        if decision.code != "A2" {
            continue;
        }
        let Some(candidate) = bbox_from_details(&decision.details) else {
            continue;
        };
        if let Some(details) = source_logo_details(prior, candidate, segments, meta) {
            if let Value::Object(object) = &mut decision.details {
                object.insert("source_logo".into(), details);
            }
        }
    }
}

fn bbox_from_details(details: &Value) -> Option<[f64; 4]> {
    let values = details.get("bbox")?.as_array()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].as_f64()?,
        values[1].as_f64()?,
        values[2].as_f64()?,
        values[3].as_f64()?,
    ])
}

fn source_logo_details(
    prior: &SourceLogoPrior,
    candidate: [f64; 4],
    segments: &[Segment],
    meta: &CompMeta,
) -> Option<Value> {
    let overlap = bbox_intersection_area(prior.bbox, candidate);
    if overlap <= 0.0 {
        return None;
    }
    let logo_area = bbox_area(prior.bbox).max(1.0);
    let candidate_area = bbox_area(candidate).max(1.0);
    let mut best_ratio = 0.0f64;
    let mut best_bbox = None;
    let mut projected_any = false;
    for segment in segments {
        let middle = segment.start + segment.duration() / 2.0;
        for time in [segment.start, middle, segment.end] {
            let Some(projected) = segment.project_bbox(prior.bbox, time) else {
                continue;
            };
            projected_any = true;
            let Some(clipped) = clip_bbox(projected, meta.width, meta.height) else {
                continue;
            };
            let projected_area = bbox_area(projected).max(1.0);
            let ratio = bbox_area(clipped) / projected_area;
            if ratio > best_ratio {
                best_ratio = ratio;
                best_bbox = Some(clipped);
            }
        }
    }
    let mut object = Map::new();
    object.insert("name".into(), json!(prior.name));
    object.insert("channel".into(), json!(prior.channel));
    object.insert("position".into(), json!(prior.position));
    object.insert(
        "bbox".into(),
        json!([
            round_to(prior.bbox[0], 1),
            round_to(prior.bbox[1], 1),
            round_to(prior.bbox[2], 1),
            round_to(prior.bbox[3], 1)
        ]),
    );
    object.insert(
        "logo_in_candidate".into(),
        json!(round_to((overlap / logo_area).min(1.0), 3)),
    );
    object.insert(
        "candidate_covered".into(),
        json!(round_to((overlap / candidate_area).min(1.0), 3)),
    );
    if projected_any {
        object.insert("visible_in_comp".into(), json!(best_ratio >= 0.10));
        object.insert(
            "visible_ratio".into(),
            json!(round_to(best_ratio.min(1.0), 3)),
        );
    } else {
        object.insert("projection".into(), json!("unsupported"));
    }
    if let Some(bbox) = best_bbox {
        object.insert(
            "projected_bbox".into(),
            json!([
                round_to(bbox[0], 1),
                round_to(bbox[1], 1),
                round_to(bbox[2], 1),
                round_to(bbox[3], 1)
            ]),
        );
    }
    Some(Value::Object(object))
}

fn decide_impl(
    segments: &[Segment],
    candidates: &[BandCandidate],
    source_height: u32,
    comp_height: f64,
    margin: f64,
    ocr: Option<&dyn OcrEvidenceSource>,
) -> Vec<Decision> {
    if candidates.is_empty() {
        return vec![Decision::noop(
            "A2",
            "comp",
            "aucune bande persistante",
            0.7,
            json!({}),
        )];
    }
    let scale_ratio = source_height as f64 / 1080.0;
    let min_height = round_to(BANNER_MIN_HEIGHT as f64 * scale_ratio, 0) as i64;
    let max_height = round_to(BANNER_MAX_HEIGHT as f64 * scale_ratio, 0) as i64;
    let geom_std = GEOM_STD * scale_ratio;
    let keyframed: Vec<&Segment> = segments
        .iter()
        .filter(|segment| segment.keyframed())
        .collect();
    if !keyframed.is_empty() {
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "{} segment(s) keyframé(s), A2 ne modifie pas les clés de Position",
                keyframed.len()
            ),
            0.4,
            json!({
                "keyframed_segments": keyframed
                    .iter()
                    .map(|segment| segment.name.clone())
                    .collect::<Vec<_>>()
            }),
        )];
    }

    let mut scales: Vec<f64> = segments.iter().map(|segment| segment.scale).collect();
    scales.sort_by(f64::total_cmp);
    scales.dedup_by(|left, right| left.total_cmp(right) == std::cmp::Ordering::Equal);
    if scales.len() > 1 {
        return vec![Decision::review(
            "A2",
            "comp",
            format!("échelles hétérogènes ({scales:?}), A2 n'applique pas un scale global"),
            0.4,
            json!({"scales": scales}),
        )];
    }

    let top: Vec<&BandCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.zone == "top" && plausible(candidate, min_height, max_height))
        .collect();
    if !top.is_empty() {
        let all: Vec<Value> = top.iter().map(|candidate| base(candidate)).collect();
        return top
            .iter()
            .map(|candidate| {
                let mut details = base(candidate);
                details["candidates"] = Value::Array(all.clone());
                Decision::review(
                    "A2",
                    "comp",
                    format!(
                        "bande haute {} px, largeur {:.0}%, position à valider",
                        candidate.height_src,
                        candidate.width_frac * 100.0
                    ),
                    0.45,
                    details,
                )
            })
            .collect();
    }

    let bottom: Vec<&BandCandidate> = candidates
        .iter()
        .filter(|candidate| candidate.zone == "bottom")
        .collect();
    if bottom.is_empty() {
        return vec![Decision::noop(
            "A2",
            "comp",
            "aucune bande basse persistante",
            0.7,
            json!({}),
        )];
    }
    let mut selected = bottom[0];
    let mut selected_key = rank(selected, max_height);
    for candidate in bottom.iter().skip(1) {
        let key = rank(candidate, max_height);
        if key > selected_key {
            selected = candidate;
            selected_key = key;
        }
    }
    let mut details = base(selected);
    details["candidates"] = Value::Array(
        bottom
            .iter()
            .take(5)
            .map(|candidate| base(candidate))
            .collect(),
    );

    // Candidat étroit : l'OCR ne peut jamais autoriser un recadrage, il
    // qualifie seulement la revue (logo fixe, sous-titre, nature inconnue).
    if selected.width_frac < LOGO_MAX_WIDTH {
        let (evidence, verdict) = ocr_verdict(ocr, selected, source_height);
        if let Some(evidence) = &evidence {
            details["ocr"] = evidence.to_json();
        }
        if matches!(verdict, Some(OcrVerdict::Changing)) {
            return vec![Decision::noop(
                "A2",
                "comp",
                "zone basse étroite, texte changeant : sous-titre probable, pas une bannière",
                0.55,
                details,
            )];
        }
        if matches!(verdict, Some(OcrVerdict::Confirmed)) {
            return vec![Decision::review(
                "A2",
                "comp",
                "zone basse étroite, texte persistant : logo ou watermark fixe, jamais recadré",
                0.55,
                details,
            )];
        }
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "zone basse étroite ({:.0}% de largeur), logo probable",
                selected.width_frac * 100.0
            ),
            0.5,
            details,
        )];
    }
    // Garde-fous durs : l'OCR ne peut pas les contourner, donc on l'évite
    // quand la décision est déjà prise.
    if selected.height_src > max_height {
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "zone basse haute ({} px), décor statique probable",
                selected.height_src
            ),
            0.4,
            details,
        )];
    }
    if selected.height_src < min_height {
        return vec![Decision::noop(
            "A2",
            "comp",
            format!(
                "zone basse trop fine ({} px), pas une bannière",
                selected.height_src
            ),
            0.6,
            details,
        )];
    }
    if selected.persistence < 0.6 {
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "bande basse non persistante ({:.0}%)",
                selected.persistence * 100.0
            ),
            0.4,
            details,
        )];
    }
    if selected.width_frac < BANNER_MIN_WIDTH {
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "largeur {:.0}% < {:.0}%, bannière partielle",
                selected.width_frac * 100.0,
                BANNER_MIN_WIDTH * 100.0
            ),
            0.5,
            details,
        )];
    }

    let (evidence, verdict) = ocr_verdict(ocr, selected, source_height);
    if let Some(evidence) = &evidence {
        details["ocr"] = evidence.to_json();
    }
    let ocr_confirmed = matches!(verdict, Some(OcrVerdict::Confirmed));
    if matches!(verdict, Some(OcrVerdict::Changing)) {
        let dominance = evidence
            .as_ref()
            .map(|evidence| evidence.dominant_ratio * 100.0)
            .unwrap_or(0.0);
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "texte changeant (dominance {dominance:.0}%), sous-titres probables ; \
                 pas de recadrage destructif"
            ),
            0.5,
            details,
        )];
    }
    let geometry_unstable = selected.center_y_std > geom_std || selected.height_std > geom_std;
    let geometry_cap = geom_std * OCR_GEOM_OVERRIDE_FACTOR;
    let geometry_capped =
        selected.center_y_std > geometry_cap || selected.height_std > geometry_cap;
    if geometry_unstable && !(ocr_confirmed && !geometry_capped) {
        let reason = if ocr_confirmed {
            format!(
                "bande basse très instable (centre ±{:.0} px, hauteur ±{:.0} px), \
                 scale non fiable malgré le texte OCR",
                selected.center_y_std, selected.height_std
            )
        } else {
            format!(
                "bande basse animée (centre ±{:.0} px, hauteur ±{:.0} px), géométrie non fiable",
                selected.center_y_std, selected.height_std
            )
        };
        return vec![Decision::review("A2", "comp", reason, 0.4, details)];
    }
    if selected.median_brightness < DARK_MEDIAN {
        if ocr_confirmed {
            if let Some(scale) = scale_target(selected, source_height, comp_height, margin) {
                details["scale_target"] = json!(scale);
                if scale > 300.0 {
                    return vec![Decision::review(
                        "A2",
                        "comp",
                        format!("scale {scale}% > 300%, zoom excessif"),
                        0.4,
                        details,
                    )];
                }
            }
            return vec![Decision::review(
                "A2",
                "comp",
                format!(
                    "bande basse sombre (médiane {:.0}), texte persistant confirmé par OCR ; \
                     revue avec scale proposé",
                    selected.median_brightness
                ),
                0.55,
                details,
            )];
        }
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "bande basse sombre (médiane {:.0}), contraste insuffisant",
                selected.median_brightness
            ),
            0.4,
            details,
        )];
    }
    if selected.temporal_std > ANIM_STD {
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "bande basse animée (écart temporel {:.0}), géométrie non fiable",
                selected.temporal_std
            ),
            0.4,
            details,
        )];
    }
    let active_src = source_height as f64 - selected.height_src as f64;
    if active_src <= 0.0 {
        return vec![Decision::review(
            "A2",
            "comp",
            "bande basse plus haute que la source",
            0.4,
            details,
        )];
    }
    let scale = round_to(comp_height / active_src * 100.0 * margin, 1);
    if scale > 300.0 {
        details["scale_target"] = json!(scale);
        return vec![Decision::review(
            "A2",
            "comp",
            format!("scale {scale}% > 300%, zoom excessif"),
            0.4,
            details,
        )];
    }
    details["scale_target"] = json!(scale);
    if matches!(verdict, Some(OcrVerdict::Ambiguous)) {
        return vec![Decision::review(
            "A2",
            "comp",
            "preuve OCR ambiguë sur bande claire, recadrage refusé",
            0.5,
            details,
        )];
    }
    let (reason, confidence) = if ocr_confirmed {
        (
            format!(
                "bannière {} px, largeur {:.0}% -> {scale}%, texte persistant confirmé par OCR",
                selected.height_src,
                selected.width_frac * 100.0
            ),
            0.95,
        )
    } else {
        (
            format!(
                "bannière {} px, largeur {:.0}% -> {scale}%",
                selected.height_src,
                selected.width_frac * 100.0
            ),
            0.9,
        )
    };
    vec![Decision::apply("A2", "comp", reason, confidence, details)]
}

fn scale_target(
    candidate: &BandCandidate,
    source_height: u32,
    comp_height: f64,
    margin: f64,
) -> Option<f64> {
    let active_src = source_height as f64 - candidate.height_src as f64;
    (active_src > 0.0).then(|| round_to(comp_height / active_src * 100.0 * margin, 1))
}

fn ocr_verdict(
    ocr: Option<&dyn OcrEvidenceSource>,
    candidate: &BandCandidate,
    source_height: u32,
) -> (Option<A2OcrEvidence>, Option<OcrVerdict>) {
    let evidence = ocr.and_then(|gate| gate.evidence(candidate));
    let verdict = evidence
        .as_ref()
        .map(|evidence| classify_ocr(evidence, source_height));
    (evidence, verdict)
}

fn classify_ocr(evidence: &A2OcrEvidence, source_height: u32) -> OcrVerdict {
    if evidence.text_persistence < A2_OCR_TEXT_PERSISTENCE {
        return OcrVerdict::Sparse;
    }
    let y_limit = A2_OCR_Y_DRIFT_1080 * source_height as f64 / 1080.0;
    let confirmed = evidence.dominant_ratio >= A2_OCR_DOMINANT_RATIO
        && evidence.y_drift_px <= y_limit
        && evidence.score >= A2_OCR_SCORE
        && evidence.overlap >= A2_OCR_OVERLAP
        && evidence.coverage_x >= A2_OCR_COVERAGE_X;
    if confirmed {
        return OcrVerdict::Confirmed;
    }
    if evidence.dominant_ratio < A2_OCR_CHANGING_RATIO {
        return OcrVerdict::Changing;
    }
    OcrVerdict::Ambiguous
}

#[derive(Debug, Clone)]
struct SampleDetection {
    normalized: String,
    text: String,
    score: f64,
    bbox: [i64; 4],
}

/// PP-OCR découpe parfois une ligne en mots ou groupes de mots. Sans fusion,
/// un mot constant (« PHRASE ») dominerait à tort des sous-titres changeants.
/// On regroupe donc les boîtes d'une même ligne, dans l'ordre de lecture.
fn merge_lines(detections: Vec<SampleDetection>) -> Vec<SampleDetection> {
    let mut lines: Vec<Vec<SampleDetection>> = Vec::new();
    for detection in detections {
        match lines
            .iter_mut()
            .find(|line| same_line(line[0].bbox, detection.bbox))
        {
            Some(line) => line.push(detection),
            None => lines.push(vec![detection]),
        }
    }
    let mut merged = Vec::new();
    for mut line in lines {
        line.sort_by_key(|detection| detection.bbox[0]);
        let mut current = line.remove(0);
        for detection in line {
            let gap = detection.bbox[0] - (current.bbox[0] + current.bbox[2]);
            let max_height = current.bbox[3].max(detection.bbox[3]) as f64;
            if gap as f64 <= 0.8 * max_height {
                current.text = format!("{} {}", current.text, detection.text);
                current.normalized = normalize_text(&current.text);
                current.score = current.score.min(detection.score);
                current.bbox = union_bbox(current.bbox, detection.bbox);
            } else {
                merged.push(current);
                current = detection;
            }
        }
        merged.push(current);
    }
    merged.sort_by_key(|detection| detection.bbox[1]);
    merged
}

fn same_line(left: [i64; 4], right: [i64; 4]) -> bool {
    let left_center = left[1] as f64 + left[3] as f64 / 2.0;
    let right_center = right[1] as f64 + right[3] as f64 / 2.0;
    let min_height = left[3].min(right[3]).max(1) as f64;
    let max_height = left[3].max(right[3]).max(1) as f64;
    // Un logo de hauteur très différente peut partager l'axe vertical du
    // bandeau sans appartenir à la même ligne de texte (UNDOC / bannière).
    if max_height / min_height > 1.35 {
        return false;
    }
    let intersection = (left[1] + left[3]).min(right[1] + right[3]) - left[1].max(right[1]);
    let overlap = intersection.max(0) as f64 / min_height;
    (left_center - right_center).abs() <= 0.5 * min_height && overlap >= 0.5
}

fn union_bbox(left: [i64; 4], right: [i64; 4]) -> [i64; 4] {
    let x0 = left[0].min(right[0]);
    let y0 = left[1].min(right[1]);
    let x1 = (left[0] + left[2]).max(right[0] + right[2]);
    let y1 = (left[1] + left[3]).max(right[1] + right[3]);
    [x0, y0, x1 - x0, y1 - y0]
}

struct TextGroup {
    text: String,
    samples: Vec<usize>,
    count: usize,
    scores: Vec<f64>,
    boxes: Vec<[i64; 4]>,
}

fn group_bbox(group: &TextGroup) -> [i64; 4] {
    group
        .boxes
        .iter()
        .copied()
        .reduce(union_bbox)
        .unwrap_or([0, 0, 0, 0])
}

fn group_coverage(group: &TextGroup, candidate_width: f64) -> f64 {
    let bbox = group_bbox(group);
    (bbox[2] as f64 / candidate_width.max(1.0)).min(1.0)
}

fn collect_ocr_evidence(
    video: &Path,
    ffmpeg: &Path,
    specs: &VideoSpecs,
    candidate: &BandCandidate,
    options: &A2OcrOptions,
) -> Result<A2OcrEvidence> {
    let started = Instant::now();
    let x0 = (candidate.x0_src - A2_OCR_MARGIN_PX).max(0);
    let x1 = (candidate.x1_src + A2_OCR_MARGIN_PX).min(specs.width as i64);
    let y0 = (candidate.y0_src - A2_OCR_MARGIN_PX).max(0);
    let y1 = (candidate.y1_src + A2_OCR_MARGIN_PX).min(specs.height as i64);
    if x1 <= x0 || y1 - y0 < A2_OCR_MIN_CROP_HEIGHT {
        anyhow::bail!(
            "crop A2 trop petit: {}x{}",
            (x1 - x0).max(0),
            (y1 - y0).max(0)
        );
    }
    let mut det = DetEngine::load(
        &options.det_model,
        options.onnxruntime.as_deref(),
        Some(A2_OCR_INTRA_THREADS),
    )
    .with_context(|| {
        format!(
            "ouverture du modèle A2 OCR det {}",
            options.det_model.display()
        )
    })?;
    let mut rec = RecEngine::load(
        &options.rec_model,
        options.onnxruntime.as_deref(),
        Some(A2_OCR_INTRA_THREADS),
    )
    .with_context(|| {
        format!(
            "ouverture du modèle A2 OCR rec {}",
            options.rec_model.display()
        )
    })?;

    let mut samples: Vec<Vec<SampleDetection>> = Vec::new();
    let mut frame_errors = 0usize;
    let mut detections_total = 0usize;
    for index in 0..A2_OCR_SAMPLES {
        let time = specs.duration_s * (index as f64 + 0.5) / A2_OCR_SAMPLES as f64;
        let frame = match grab_frame(
            ffmpeg,
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
                tracing::warn!(time, error = %error, "extraction de frame A2 OCR en échec");
                frame_errors += 1;
                continue;
            }
        };
        let crop = frame.crop(
            x0 as usize,
            y0 as usize,
            (x1 - x0) as usize,
            (y1 - y0) as usize,
        );
        let boxes = det.detect(&crop)?;
        let texts = rec.recognize(&crop, &boxes)?;
        let mut detections = Vec::new();
        for ((text, score), det_box) in texts.into_iter().zip(boxes.iter()) {
            let clean = text.trim();
            let normalized = normalize_text(clean);
            if score < A2_OCR_MIN_SCORE || normalized.chars().count() < A2_OCR_MIN_TEXT_LEN {
                continue;
            }
            let [bx1, by1, bx2, by2] = det_box.bbox;
            let bbox = [bx1 + x0, by1 + y0, bx2 - bx1, by2 - by1];
            if box_overlap(bbox, candidate) <= 0.0 {
                continue;
            }
            detections.push(SampleDetection {
                normalized,
                text: clean.to_string(),
                score,
                bbox,
            });
            tracing::debug!(
                time,
                text = %clean,
                score,
                bbox = ?bbox,
                "A2 OCR détection"
            );
        }
        detections_total += detections.len();
        samples.push(merge_lines(detections));
    }
    if samples.len() < A2_OCR_MIN_FRAMES {
        anyhow::bail!(
            "frames A2 OCR insuffisantes: {}/{} ({} erreur(s))",
            samples.len(),
            A2_OCR_SAMPLES,
            frame_errors
        );
    }

    let mut groups: HashMap<String, TextGroup> = HashMap::new();
    let mut samples_with_text = 0usize;
    for (index, detections) in samples.iter().enumerate() {
        if !detections.is_empty() {
            samples_with_text += 1;
        }
        let mut seen: Vec<&str> = Vec::new();
        for detection in detections {
            if seen.contains(&detection.normalized.as_str()) {
                continue;
            }
            seen.push(&detection.normalized);
            let group = groups
                .entry(detection.normalized.clone())
                .or_insert_with(|| TextGroup {
                    text: detection.text.clone(),
                    samples: Vec::new(),
                    count: 0,
                    scores: Vec::new(),
                    boxes: Vec::new(),
                });
            group.samples.push(index);
            group.count += 1;
            group.scores.push(detection.score);
            group.boxes.push(detection.bbox);
        }
    }
    let total = samples.len();
    let text_persistence = samples_with_text as f64 / total as f64;
    for group in groups.values() {
        tracing::debug!(
            text = %group.text,
            samples = group.samples.len(),
            count = group.count,
            "A2 OCR groupe"
        );
    }
    let candidate_width = (candidate.x1_src - candidate.x0_src).max(1) as f64;
    // Tri déterministe : nombre d'échantillons, puis largeur couverte (un
    // fragment fréquent ne doit pas masquer la ligne complète), puis texte.
    let mut ranked: Vec<&TextGroup> = groups.values().collect();
    ranked.sort_by(|left, right| {
        right
            .samples
            .len()
            .cmp(&left.samples.len())
            .then_with(|| {
                group_coverage(right, candidate_width)
                    .total_cmp(&group_coverage(left, candidate_width))
            })
            .then_with(|| left.text.cmp(&right.text))
    });
    let texts: Vec<A2OcrText> = ranked
        .iter()
        .take(3)
        .map(|group| A2OcrText {
            text: group.text.clone(),
            samples: group.samples.len(),
            coverage_x: round_to(group_coverage(group, candidate_width), 4),
        })
        .collect();
    let dominant = ranked.first().copied();
    let (dominant_text, dominant_bbox, dominant_ratio, y_drift_px, score, overlap, coverage_x) =
        match dominant {
            Some(group) => {
                let ratio = group.samples.len() as f64 / total as f64;
                let centers = group
                    .boxes
                    .iter()
                    .map(|bbox| bbox[1] as f64 + bbox[3] as f64 / 2.0)
                    .collect::<Vec<_>>();
                let y_min = centers.iter().copied().reduce(f64::min).unwrap_or(0.0);
                let y_max = centers.iter().copied().reduce(f64::max).unwrap_or(0.0);
                let score = median(group.scores.clone());
                let overlaps = group
                    .boxes
                    .iter()
                    .map(|bbox| box_overlap(*bbox, candidate))
                    .collect::<Vec<_>>();
                let overlap = median(overlaps);
                (
                    group.text.clone(),
                    group_bbox(group),
                    ratio,
                    y_max - y_min,
                    score,
                    overlap,
                    group_coverage(group, candidate_width),
                )
            }
            None => (String::new(), [0, 0, 0, 0], 0.0, 0.0, 0.0, 0.0, 0.0),
        };
    Ok(A2OcrEvidence {
        samples: total,
        samples_with_text,
        detections_total,
        frame_errors,
        dominant_text,
        dominant_bbox,
        texts,
        text_persistence: round_to(text_persistence, 4),
        dominant_ratio: round_to(dominant_ratio, 4),
        y_drift_px: round_to(y_drift_px, 1),
        score: round_to(score, 3),
        overlap: round_to(overlap, 4),
        coverage_x: round_to(coverage_x, 4),
        timing_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

/// Intersection sur la plus petite boîte : 1,0 quand la détection est
/// entièrement dans le candidat, 0,0 quand elle est entièrement dehors.
fn box_overlap(detection: [i64; 4], candidate: &BandCandidate) -> f64 {
    let [x, y, width, height] = detection;
    let intersection_width = (x + width).min(candidate.x1_src) - x.max(candidate.x0_src);
    let intersection_height = (y + height).min(candidate.y1_src) - y.max(candidate.y0_src);
    let intersection = (intersection_width.max(0) * intersection_height.max(0)) as f64;
    let detection_area = (width.max(1) * height.max(1)) as f64;
    let candidate_area = ((candidate.x1_src - candidate.x0_src).max(1)
        * (candidate.y1_src - candidate.y0_src).max(1)) as f64;
    if detection_area <= 0.0 || candidate_area <= 0.0 {
        0.0
    } else {
        intersection / detection_area.min(candidate_area)
    }
}

fn plausible(candidate: &BandCandidate, min_height: i64, max_height: i64) -> bool {
    (min_height..=max_height).contains(&candidate.height_src)
        && candidate.width_frac >= LOGO_MAX_WIDTH
        && candidate.persistence >= 0.6
        && candidate.aspect >= 3.0
}

fn rank(candidate: &BandCandidate, max_height: i64) -> (bool, i64, f64, f64) {
    (
        candidate.width_frac >= BANNER_MIN_WIDTH,
        candidate.height_src.min(max_height),
        candidate.persistence,
        candidate.width_frac,
    )
}

fn base(candidate: &BandCandidate) -> Value {
    let mut object = Map::new();
    object.insert(
        "bbox".into(),
        json!([
            candidate.x0_src,
            candidate.y0_src,
            candidate.x1_src,
            candidate.y1_src
        ]),
    );
    object.insert("height".into(), json!(candidate.height_src));
    object.insert("width_frac".into(), json!(candidate.width_frac));
    object.insert("persistence".into(), json!(candidate.persistence));
    object.insert("zone".into(), json!(candidate.zone));
    object.insert("center_x_frac".into(), json!(candidate.center_x_frac));
    object.insert("aspect".into(), json!(candidate.aspect));
    object.insert(
        "median_brightness".into(),
        json!(candidate.median_brightness),
    );
    object.insert("temporal_std".into(), json!(candidate.temporal_std));
    object.insert("center_y_std".into(), json!(candidate.center_y_std));
    object.insert("height_std".into(), json!(candidate.height_std));
    object.insert("source".into(), json!(candidate.source));
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> BandCandidate {
        BandCandidate {
            y0_src: 928,
            y1_src: 1032,
            height_src: 104,
            x0_src: 422,
            x1_src: 1498,
            width_frac: 0.56,
            center_x_frac: 0.5,
            center_y_frac: 0.91,
            aspect: 10.0,
            persistence: 1.0,
            median_brightness: 255.0,
            mean_brightness: 255.0,
            temporal_std: 0.0,
            center_y_std: 0.0,
            height_std: 0.0,
            n_frames: 5,
            zone: "bottom",
            source: "bright",
        }
    }

    fn segment(scale: f64) -> Segment {
        Segment {
            name: "Segment 1".into(),
            layer_id: 10,
            parent: None,
            start: 0.0,
            end: 10.0,
            source: Some("clip.mp4".into()),
            anchor: [960.0, 0.0, 0.0],
            scale,
            position: [540.0, 0.0, 0.0],
            position_keys: Vec::new(),
            scale_keys: Vec::new(),
            anchor_keys: Vec::new(),
            comp_x: 540.0,
            parent_dy: 0.0,
            null_parent: false,
            parent_keyframed: false,
            rotated: false,
        }
    }

    struct FakeGate(Option<A2OcrEvidence>);

    impl OcrEvidenceSource for FakeGate {
        fn evidence(&self, _candidate: &BandCandidate) -> Option<A2OcrEvidence> {
            self.0.clone()
        }
    }

    fn confirmed_evidence() -> A2OcrEvidence {
        A2OcrEvidence {
            samples: 12,
            samples_with_text: 12,
            detections_total: 12,
            frame_errors: 0,
            dominant_text: "BANDEAU FIXE".into(),
            dominant_bbox: [0, 0, 100, 40],
            texts: vec![A2OcrText {
                text: "BANDEAU FIXE".into(),
                samples: 12,
                coverage_x: 0.8,
            }],
            text_persistence: 1.0,
            dominant_ratio: 1.0,
            y_drift_px: 2.0,
            score: 0.98,
            overlap: 1.0,
            coverage_x: 0.8,
            timing_ms: 10,
        }
    }

    fn sample_detection(text: &str, bbox: [i64; 4]) -> SampleDetection {
        SampleDetection {
            normalized: normalize_text(text),
            text: text.into(),
            score: 0.9,
            bbox,
        }
    }

    #[test]
    fn line_fragments_are_merged_in_reading_order() {
        let merged = merge_lines(vec![
            sample_detection("PHRASE", [568, 1595, 300, 77]),
            sample_detection("PREMIERE", [210, 1592, 372, 80]),
        ]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].text, "PREMIERE PHRASE");
        assert_eq!(merged[0].normalized, "premierephrase");
        assert_eq!(merged[0].bbox, [210, 1592, 658, 80]);
    }

    #[test]
    fn taller_logo_is_not_merged_with_banner_text() {
        let merged = merge_lines(vec![
            sample_detection("UNDOC", [98, 967, 196, 60]),
            sample_detection("FAUX CLIENTS", [316, 978, 907, 38]),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn stacked_lines_are_not_merged() {
        let merged = merge_lines(vec![
            sample_detection("LIGNE UN", [100, 1500, 400, 60]),
            sample_detection("LIGNE DEUX", [100, 1600, 400, 60]),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn distant_boxes_on_the_same_line_are_not_merged() {
        let merged = merge_lines(vec![
            sample_detection("GAUCHE", [0, 100, 100, 50]),
            sample_detection("DROITE", [300, 100, 100, 50]),
        ]);
        assert_eq!(merged.len(), 2);
    }

    fn decide_with_evidence(
        candidate: BandCandidate,
        source_height: u32,
        evidence: Option<A2OcrEvidence>,
    ) -> Decision {
        let gate = FakeGate(evidence);
        decide_impl(&[], &[candidate], source_height, 1920.0, 1.05, Some(&gate))
            .into_iter()
            .next()
            .unwrap()
    }

    fn meta() -> CompMeta {
        CompMeta {
            id: "2".into(),
            width: 1080,
            height: 1920,
            fps: 25.0,
            duration: 10.0,
            n_layers: 1,
            n_segments: 1,
        }
    }

    fn source_logo_prior() -> SourceLogoPrior {
        SourceLogoPrior {
            name: "66 Minutes".into(),
            channel: "M6".into(),
            position: "bottom_left".into(),
            bbox: [91.0, 846.0, 298.0, 1026.0],
            variants: Vec::new(),
        }
    }

    #[test]
    fn source_logo_overlap_is_documented_without_changing_action() {
        let prior = source_logo_prior();
        let details = source_logo_details(
            &prior,
            [120.0, 856.0, 1204.0, 1008.0],
            &[segment(180.0)],
            &meta(),
        )
        .unwrap();
        assert_eq!(details["name"], "66 Minutes");
        assert_eq!(details["logo_in_candidate"], 0.726);
        assert_eq!(details["visible_in_comp"], false);

        let mut decisions = vec![Decision::apply(
            "A2",
            "comp",
            "bannière test",
            0.9,
            json!({"bbox": [120, 856, 1204, 1008]}),
        )];
        annotate_source_logo(&mut decisions, &prior, &[segment(180.0)], &meta());
        assert_eq!(decisions[0].action, crate::post::model::Action::Apply);
        assert_eq!(decisions[0].details["source_logo"]["name"], "66 Minutes");
    }

    #[test]
    fn source_logo_visibility_uses_the_project_transform() {
        let mut segment = segment(180.0);
        segment.anchor = [0.0, 0.0, 0.0];
        segment.position = [0.0, 0.0, 0.0];
        segment.comp_x = 0.0;
        let details = source_logo_details(
            &source_logo_prior(),
            [120.0, 856.0, 1204.0, 1008.0],
            &[segment],
            &meta(),
        )
        .unwrap();
        assert_eq!(details["visible_in_comp"], true);
        assert_eq!(details["visible_ratio"], 1.0);
        assert!(details["projected_bbox"].is_array());
    }

    #[test]
    fn source_logo_rotation_skips_visibility() {
        let mut segment = segment(180.0);
        segment.rotated = true;
        let details = source_logo_details(
            &source_logo_prior(),
            [120.0, 856.0, 1204.0, 1008.0],
            &[segment],
            &meta(),
        )
        .unwrap();
        assert_eq!(details["projection"], "unsupported");
        assert!(details.get("visible_in_comp").is_none());
    }

    #[test]
    fn source_logo_outside_candidate_is_not_added() {
        let prior = source_logo_prior();
        let mut decisions = vec![Decision::apply(
            "A2",
            "comp",
            "bannière test",
            0.9,
            json!({"bbox": [600, 900, 1200, 1000]}),
        )];
        annotate_source_logo(&mut decisions, &prior, &[segment(180.0)], &meta());
        assert!(decisions[0].details.get("source_logo").is_none());
    }

    #[test]
    fn confirmed_text_overrides_moderate_geometry() {
        let mut candidate = candidate();
        candidate.center_y_std = 15.0;
        candidate.height_std = 20.0;
        let decision = decide_with_evidence(candidate, 1080, Some(confirmed_evidence()));
        assert_eq!(decision.action, crate::post::model::Action::Apply);
        assert!(decision.details["ocr"].is_object());
        assert_eq!(decision.details["scale_target"], 206.6);
    }

    #[test]
    fn confirmed_text_does_not_override_extreme_geometry() {
        let mut candidate = candidate();
        candidate.center_y_std = 40.0;
        candidate.height_std = 40.0;
        let decision = decide_with_evidence(candidate, 1080, Some(confirmed_evidence()));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("très instable"));
    }

    #[test]
    fn confirmed_text_does_not_override_temporal_animation() {
        let mut candidate = candidate();
        candidate.temporal_std = 60.0;
        let decision = decide_with_evidence(candidate, 1080, Some(confirmed_evidence()));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("écart temporel"));
    }

    #[test]
    fn ocr_thresholds_reject_each_weak_signal() {
        type Mutation = fn(&mut A2OcrEvidence);
        let cases: [(Mutation, OcrVerdict); 5] = [
            (|e| e.coverage_x = 0.1, OcrVerdict::Ambiguous),
            (|e| e.y_drift_px = 20.0, OcrVerdict::Ambiguous),
            (|e| e.score = 0.7, OcrVerdict::Ambiguous),
            (|e| e.overlap = 0.4, OcrVerdict::Ambiguous),
            (|e| e.text_persistence = 0.5, OcrVerdict::Sparse),
        ];
        for (mutate, expected) in cases {
            let mut evidence = confirmed_evidence();
            mutate(&mut evidence);
            assert_eq!(classify_ocr(&evidence, 1080), expected);
        }
    }

    #[test]
    fn no_text_never_vetoes() {
        let mut evidence = confirmed_evidence();
        evidence.samples_with_text = 0;
        evidence.text_persistence = 0.0;
        evidence.dominant_ratio = 0.0;
        assert_eq!(classify_ocr(&evidence, 1080), OcrVerdict::Sparse);
    }

    #[test]
    fn sparse_text_does_not_become_changing() {
        let mut evidence = confirmed_evidence();
        evidence.text_persistence = 0.5;
        evidence.dominant_ratio = 0.2;
        assert_eq!(classify_ocr(&evidence, 1080), OcrVerdict::Sparse);
    }

    #[test]
    fn sparse_text_keeps_the_current_apply() {
        let mut evidence = confirmed_evidence();
        evidence.text_persistence = 0.4;
        evidence.dominant_ratio = 0.2;
        let decision = decide_with_evidence(candidate(), 1080, Some(evidence));
        assert_eq!(decision.action, crate::post::model::Action::Apply);
        assert!(decision.details["ocr"].is_object());
    }

    #[test]
    fn y_drift_limit_scales_with_source_height() {
        let mut evidence = confirmed_evidence();
        evidence.y_drift_px = 8.0;
        assert_eq!(classify_ocr(&evidence, 720), OcrVerdict::Confirmed);
        evidence.y_drift_px = 9.0;
        assert_eq!(classify_ocr(&evidence, 720), OcrVerdict::Ambiguous);
    }

    #[test]
    fn confirmed_dark_banner_is_reviewed_with_scale() {
        let mut candidate = candidate();
        candidate.median_brightness = 37.0;
        let decision = decide_with_evidence(candidate, 1080, Some(confirmed_evidence()));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("sombre"));
        assert_eq!(decision.details["scale_target"], 206.6);
        assert!(decision.details["ocr"].is_object());
    }

    #[test]
    fn changing_text_vetoes_an_otherwise_valid_banner() {
        let mut evidence = confirmed_evidence();
        evidence.text_persistence = 0.9;
        evidence.dominant_ratio = 0.3;
        let decision = decide_with_evidence(candidate(), 1080, Some(evidence));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("texte changeant"));
    }

    #[test]
    fn ambiguous_text_downgrades_apply_to_review() {
        let mut evidence = confirmed_evidence();
        evidence.dominant_ratio = 0.7;
        let decision = decide_with_evidence(candidate(), 1080, Some(evidence));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("ambiguë"));
    }

    #[test]
    fn narrow_confirmed_text_is_a_logo_review() {
        let mut candidate = candidate();
        candidate.width_frac = 0.2;
        let decision = decide_with_evidence(candidate, 1080, Some(confirmed_evidence()));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("logo"));
    }

    #[test]
    fn narrow_changing_text_is_a_noop() {
        let mut candidate = candidate();
        candidate.width_frac = 0.2;
        let mut evidence = confirmed_evidence();
        evidence.dominant_ratio = 0.3;
        let decision = decide_with_evidence(candidate, 1080, Some(evidence));
        assert_eq!(decision.action, crate::post::model::Action::Noop);
        assert!(decision.reason.contains("sous-titre"));
    }

    #[test]
    fn confirmed_text_with_excessive_scale_is_reviewed() {
        let mut candidate = candidate();
        candidate.height_src = 100;
        let decision = decide_with_evidence(candidate, 720, Some(confirmed_evidence()));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("zoom excessif"));
    }

    #[test]
    fn confirmed_text_does_not_override_partial_width() {
        let mut candidate = candidate();
        candidate.width_frac = 0.4;
        let decision = decide_with_evidence(candidate, 1080, Some(confirmed_evidence()));
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(decision.reason.contains("partielle"));
    }

    #[test]
    fn absent_ocr_keeps_the_current_apply() {
        let decision = decide_with_evidence(candidate(), 1080, None);
        assert_eq!(decision.action, crate::post::model::Action::Apply);
        assert!(decision.details.get("ocr").is_none());
    }

    #[test]
    fn heterogeneous_scales_are_reviewed() {
        let decisions = decide_from_candidates(
            &[segment(180.0), segment(200.0)],
            &[candidate()],
            1080,
            1920.0,
            1.05,
        );
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
        assert!(decisions[0].reason.contains("hétérogènes"));
    }

    #[test]
    fn clear_full_width_bottom_banner_is_applied() {
        let decisions = decide_from_candidates(&[], &[candidate()], 1080, 1920.0, 1.05);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].action, crate::post::model::Action::Apply);
        assert_eq!(decisions[0].details["scale_target"], 206.6);
    }

    #[test]
    fn banner_scale_uses_source_height() {
        let mut candidate = candidate();
        candidate.height_src = 40;
        let decisions = decide_from_candidates(&[], &[candidate], 720, 1920.0, 1.05);
        assert_eq!(decisions[0].action, crate::post::model::Action::Apply);
        assert_eq!(decisions[0].details["scale_target"], 296.5);
    }

    #[test]
    fn top_band_is_reviewed() {
        let mut candidate = candidate();
        candidate.zone = "top";
        candidate.y0_src = 40;
        candidate.y1_src = 144;
        let decisions = decide_from_candidates(&[], &[candidate], 1080, 1920.0, 1.05);
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
        assert!(decisions[0].reason.contains("bande haute"));
    }

    #[test]
    fn dark_banner_is_reviewed() {
        let mut candidate = candidate();
        candidate.median_brightness = 30.0;
        let decisions = decide_from_candidates(&[], &[candidate], 1080, 1920.0, 1.05);
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
        assert!(decisions[0].reason.contains("sombre"));
    }

    #[test]
    fn moving_banner_is_reviewed() {
        let mut candidate = candidate();
        candidate.center_y_std = 40.0;
        let decisions = decide_from_candidates(&[], &[candidate], 1080, 1920.0, 1.05);
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
        assert!(decisions[0].reason.contains("animée"));
    }
}
