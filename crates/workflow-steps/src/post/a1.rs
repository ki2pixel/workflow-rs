//! A1 — letterbox noir et compensation de Position Y.
//!
//! A1 ne modifie jamais l'Ancre ni les keyframes. Le scale est calculé sur le
//! 10e percentile des hauteurs actives du segment avec la marge 1,015 de D-056 ;
//! la couverture de tous les crops est vérifiée après scale et compensation
//! avant toute application. Un candidat A3 fort peut différer le `apply` en
//! revue (R2, `a1_defer_on_a3`).

use crate::post::detect::{cropdetect, native_letterbox, Crop, NativeLetterbox, NativeParams};
use crate::post::ecproj::{CompMeta, Segment};
use crate::post::model::{Action, Decision};
use anyhow::Result;
use serde_json::{json, Map, Value};
use std::collections::HashSet;
use std::path::Path;
use workflow_schema::round::round_to;

const ACTIVE_NOOP: u32 = 1072;
const INTERMITTENT_DELTA: i64 = 24;
const OUTLIER_FRAC: f64 = 0.10;
const COVERAGE_TOL: f64 = 0.5;
/// Marge A1 validée par D-056 (médiane humaine 1,0125).
const DEFAULT_A1_MARGIN: f64 = 1.015;

/// Politique A1 : marge de scale et cibles différées sur candidat A3 (R2).
#[derive(Debug, Clone)]
pub struct A1Options {
    pub margin: f64,
    /// Segments dont le `apply` A1 doit être transformé en revue : A3 y a
    /// détecté un candidat fort, donc un keyframing humain probable.
    pub defer_targets: HashSet<String>,
    /// D-058 option A : raffine un `apply` `cropdetect` avec le détecteur
    /// natif. Désactivé par défaut ; le natif ne peut jamais créer un `apply`
    /// ni changer un `review`/`noop`.
    pub refine_native: bool,
}

impl Default for A1Options {
    fn default() -> Self {
        Self {
            margin: DEFAULT_A1_MARGIN,
            defer_targets: HashSet::new(),
            refine_native: false,
        }
    }
}

pub fn decide_a1(
    segments: &[Segment],
    video: Option<&Path>,
    ffmpeg: &Path,
    meta: &CompMeta,
    source_width: u32,
    source_height: u32,
    options: &A1Options,
) -> Vec<Decision> {
    let Some(video) = video else {
        return vec![Decision::review(
            "A1",
            "comp",
            "vidéo source introuvable, A1 non calculé",
            0.2,
            json!({"reason": "video_missing"}),
        )];
    };
    if meta.width != 1080 || meta.height != 1920 {
        return vec![Decision::review(
            "A1",
            "comp",
            format!(
                "composition {}x{}, A1 exige 1080x1920",
                meta.width, meta.height
            ),
            0.2,
            json!({"width": meta.width, "height": meta.height}),
        )];
    }
    let mut decisions = Vec::new();
    let mut position_deltas = Vec::new();
    for segment in segments {
        let duration = (segment.duration() - 0.1).clamp(0.3, 4.0);
        let crops = match cropdetect(ffmpeg, video, segment.start + 0.05, duration) {
            Ok(crops) => crops,
            Err(error) => {
                decisions.push(Decision::review(
                    "A1",
                    segment.name.clone(),
                    format!("cropdetect en échec: {error}"),
                    0.2,
                    json!({"start": round_to(segment.start, 3), "end": round_to(segment.end, 3)}),
                ));
                continue;
            }
        };
        let (mut decision, mut delta) =
            decide_segment(segment, &crops, meta, source_width, source_height, options);
        if options.refine_native && decision.action == Action::Apply {
            match refine_native(
                segment,
                &crops,
                &decision,
                video,
                ffmpeg,
                meta,
                source_width,
                source_height,
                options,
            ) {
                Ok(Some((refined, refined_delta))) => {
                    decision = refined;
                    delta = refined_delta;
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(
                    segment = %segment.name,
                    error = %error,
                    "A1: raffinement natif ignoré, décision cropdetect conservée"
                ),
            }
        }
        if let Some(delta) = delta {
            position_deltas.push(delta);
        }
        decisions.push(decision);
    }
    if !position_deltas.is_empty() {
        position_deltas.sort_by(f64::total_cmp);
        let median = position_deltas[position_deltas.len() / 2];
        decisions.push(Decision::review(
            "A1",
            "comp",
            format!(
                "compensation Y médiane {median:+.1} px sur {} segments, cadrage vertical à valider",
                position_deltas.len()
            ),
            0.5,
            json!({
                "position_y_deltas": position_deltas
                    .iter()
                    .map(|value| round_to(*value, 1))
                    .collect::<Vec<_>>()
            }),
        ));
    }
    decisions
}

fn decide_segment(
    segment: &Segment,
    crops: &[Crop],
    meta: &CompMeta,
    source_width: u32,
    source_height: u32,
    options: &A1Options,
) -> (Decision, Option<f64>) {
    if crops.is_empty() {
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                "cropdetect sans résultat",
                0.2,
                json!({"start": round_to(segment.start, 3), "end": round_to(segment.end, 3)}),
            ),
            None,
        );
    }
    let heights: Vec<u32> = crops.iter().map(|crop| crop.h).collect();
    let active = round_to(percentile(&heights, 0.10), 0) as u32;
    let min_height = *heights.iter().min().unwrap_or(&0);
    let max_height = *heights.iter().max().unwrap_or(&0);

    let mut counts: Vec<(Crop, usize)> = Vec::new();
    for crop in crops {
        if let Some(entry) = counts.iter_mut().find(|(candidate, _)| candidate == crop) {
            entry.1 += 1;
        } else {
            counts.push((*crop, 1));
        }
    }
    let mut dominant = counts[0];
    let mut dominant_key = (
        (dominant.0.h as i64 - active as i64).abs(),
        -(dominant.1 as i64),
    );
    for entry in counts.iter().skip(1) {
        let key = ((entry.0.h as i64 - active as i64).abs(), -(entry.1 as i64));
        if key < dominant_key {
            dominant = *entry;
            dominant_key = key;
        }
    }
    let (crop, crop_count) = dominant;
    let ratio = round_to(crop_count as f64 / crops.len() as f64, 3);
    let top = crop.y as i64;
    let bottom = source_height as i64 - (crop.y + crop.h) as i64;
    let left = crop.x as i64;
    let right = source_width as i64 - (crop.x + crop.w) as i64;
    let scale_ratio = source_height as f64 / 1080.0;
    let bar_min = round_to(8.0 * scale_ratio, 0).max(1.0) as i64;
    let intermittent_delta = round_to(INTERMITTENT_DELTA as f64 * scale_ratio, 0) as i64;
    let vertical = top >= bar_min
        && bottom >= bar_min
        && (top - bottom).abs() as f64 <= 0.5 * top.max(bottom) as f64;
    let horizontal = left >= bar_min
        && right >= bar_min
        && (left - right).abs() as f64 <= 0.5 * left.max(right) as f64;
    let barred = crops
        .iter()
        .filter(|crop| (crop.h as i64) < active as i64 - intermittent_delta)
        .count();
    let barred_frac = round_to(barred as f64 / crops.len() as f64, 3);

    let mut details = Map::new();
    details.insert("start".into(), json!(round_to(segment.start, 3)));
    details.insert("end".into(), json!(round_to(segment.end, 3)));
    details.insert("active_height".into(), json!(crop.h));
    details.insert("active_p10".into(), json!(active));
    details.insert("active_min".into(), json!(min_height));
    details.insert("active_max".into(), json!(max_height));
    details.insert(
        "crop".into(),
        json!({"w": crop.w, "h": crop.h, "x": crop.x, "y": crop.y, "ratio": ratio}),
    );
    details.insert("n_crops".into(), json!(crops.len()));
    details.insert("scale_current".into(), json!(round_to(segment.scale, 1)));
    details.insert(
        "bars".into(),
        json!({"top": top, "bottom": bottom, "left": left, "right": right}),
    );
    details.insert("symmetric".into(), json!(vertical || horizontal));
    details.insert("barred_frac".into(), json!(barred_frac));
    details.insert("barred_crops".into(), json!(barred));
    let details = Value::Object(details);

    if segment.keyframed() {
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                format!(
                    "segment keyframé ({} clés), A1 ne modifie pas les clés",
                    segment.position_keys.len()
                        + segment.scale_keys.len()
                        + segment.anchor_keys.len()
                ),
                0.5,
                details,
            ),
            None,
        );
    }
    let active_noop = round_to(ACTIVE_NOOP as f64 * scale_ratio, 0) as u32;
    if active >= active_noop {
        if barred_frac >= OUTLIER_FRAC {
            return (
                Decision::review(
                    "A1",
                    segment.name.clone(),
                    format!(
                        "letterbox intermittent: {barred} crops sur {} ont des barres",
                        crops.len()
                    ),
                    0.4,
                    details,
                ),
                None,
            );
        }
        let mut flat = details.as_object().cloned().unwrap_or_default();
        flat.insert("scale".into(), json!(segment.scale));
        return (
            Decision::noop(
                "A1",
                segment.name.clone(),
                "pas de barres noires",
                0.9,
                Value::Object(flat),
            ),
            None,
        );
    }
    if barred_frac >= OUTLIER_FRAC {
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                format!(
                    "letterbox variable: {barred} crops sur {} ont plus de barres que le p10",
                    crops.len()
                ),
                0.4,
                details,
            ),
            None,
        );
    }
    if !vertical {
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                format!("barres asymétriques (haut {top}, bas {bottom}), crop de contenu probable"),
                0.4,
                details,
            ),
            None,
        );
    }

    decide_letterbox(
        segment,
        crops,
        crop,
        active,
        details,
        meta,
        source_height,
        options,
    )
}

#[allow(clippy::too_many_arguments)]
fn decide_letterbox(
    segment: &Segment,
    crops: &[Crop],
    crop: Crop,
    active: u32,
    details: Value,
    meta: &CompMeta,
    source_height: u32,
    options: &A1Options,
) -> (Decision, Option<f64>) {
    let scale = round_to(
        meta.height as f64 / active as f64 * 100.0 * options.margin,
        1,
    );
    let active_center = crop.y as f64 + crop.h as f64 / 2.0;
    let scale_factor = scale / 100.0;
    let comp_y_target =
        meta.height as f64 / 2.0 - (active_center - segment.anchor[1]) * scale_factor;
    let position_y_target = comp_y_target - segment.parent_dy;
    let position_y_delta = position_y_target - segment.position[1];
    let coverage = coverage(crops, segment, scale, position_y_target, meta);

    let mut details = details.as_object().cloned().unwrap_or_default();
    details.insert("scale_target".into(), json!(scale));
    details.insert("active_center_y".into(), json!(round_to(active_center, 1)));
    details.insert(
        "position_y_target".into(),
        json!(round_to(position_y_target, 1)),
    );
    details.insert(
        "position_y_delta".into(),
        json!(round_to(position_y_delta, 1)),
    );
    details.insert("coverage".into(), coverage);
    let details = Value::Object(details);

    if scale > 300.0 {
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                format!("scale {scale}% > 300%, zoom excessif"),
                0.4,
                details,
            ),
            None,
        );
    }
    if position_y_delta.abs() > 600.0 {
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                format!(
                    "compensation Y {position_y_delta:.0} px > 600, cadrage vertical à valider"
                ),
                0.4,
                details,
            ),
            None,
        );
    }
    let covered = details
        .get("coverage")
        .and_then(|coverage| coverage.get("covered"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let fail_frac = details
        .get("coverage")
        .and_then(|coverage| coverage.get("fail_frac"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    if !covered && fail_frac > OUTLIER_FRAC {
        let fail_count = details
            .get("coverage")
            .and_then(|coverage| coverage.get("fail_count"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let top_min = details
            .get("coverage")
            .and_then(|coverage| coverage.get("top_min"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let bottom_max = details
            .get("coverage")
            .and_then(|coverage| coverage.get("bottom_max"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                format!(
                    "la zone active ne couvre pas le comp après scale {scale}% ({fail_count} crops, haut {top_min}, bas {bottom_max})"
                ),
                0.35,
                details,
            ),
            None,
        );
    }
    if (scale - segment.scale).abs() <= 1.0 && position_y_delta.abs() <= 1.0 {
        return (
            Decision::noop(
                "A1",
                segment.name.clone(),
                format!("scale déjà à {:.1}% et position Y stable", segment.scale),
                0.8,
                details,
            ),
            None,
        );
    }
    if options.defer_targets.contains(&segment.name) {
        let mut details = details;
        details["a1_deferred_on_a3"] = json!(true);
        details["defer_reason"] = json!("keyframes probables détectées par A3");
        return (
            Decision::review(
                "A1",
                segment.name.clone(),
                "compensation Y différée : keyframes probables détectées par A3",
                0.45,
                details,
            ),
            None,
        );
    }
    let mut reason = format!(
        "letterbox {} px -> {scale}%",
        source_height as i64 - active as i64
    );
    if position_y_delta.abs() > 1.0 {
        reason.push_str(&format!(", position Y {position_y_delta:+.1} px"));
    }
    let delta = (position_y_delta.abs() > 100.0).then_some(position_y_delta);
    (
        Decision::apply("A1", segment.name.clone(), reason, 0.9, details),
        delta,
    )
}

#[allow(clippy::too_many_arguments)]
fn refine_native(
    segment: &Segment,
    crops: &[Crop],
    original: &Decision,
    video: &Path,
    ffmpeg: &Path,
    meta: &CompMeta,
    source_width: u32,
    source_height: u32,
    options: &A1Options,
) -> Result<Option<(Decision, Option<f64>)>> {
    let duration = (segment.duration() - 0.1).clamp(0.3, 4.0);
    let fps = round_to((2.0 / duration).max(2.0), 3);
    let params = NativeParams::default();
    let Some(native) = native_letterbox(
        ffmpeg,
        video,
        segment.start + 0.05,
        duration,
        fps,
        source_width as usize,
        source_height as usize,
        &params,
    )?
    else {
        return Ok(None);
    };
    Ok(refine_with_native(
        segment,
        crops,
        original,
        &native,
        meta,
        source_width,
        source_height,
        options,
        &params,
    ))
}

/// Applique le natif uniquement comme raffinement d'un `apply` `cropdetect`.
///
/// Le natif doit être stable (≥ 2 frames, cluster dominant ≥ 60 %), voir un
/// letterbox vertical symétrique et produire un `apply` qui repasse tous les
/// vetos A1 (scale, Position Y, couverture). Sinon `None` : la décision
/// `cropdetect` d'origine est conservée telle quelle.
#[allow(clippy::too_many_arguments)]
fn refine_with_native(
    segment: &Segment,
    crops: &[Crop],
    original: &Decision,
    native: &NativeLetterbox,
    meta: &CompMeta,
    source_width: u32,
    source_height: u32,
    options: &A1Options,
    params: &NativeParams,
) -> Option<(Decision, Option<f64>)> {
    if native.n_frames < 2 || native.fraction() < params.cluster_min_frac {
        return None;
    }
    let active = native.active(source_height);
    if active < 1 {
        return None;
    }
    let scale_ratio = source_height as f64 / 1080.0;
    let active_noop = round_to(ACTIVE_NOOP as f64 * scale_ratio, 0) as u32;
    if active >= active_noop {
        return None;
    }
    let bar_min = round_to(8.0 * scale_ratio, 0).max(1.0) as i64;
    let top = native.top as i64;
    let bottom = native.bottom as i64;
    if top < bar_min || bottom < bar_min {
        return None;
    }
    let vertical = (top - bottom).abs() as f64 <= 0.5 * top.max(bottom) as f64;
    if !vertical {
        return None;
    }
    let crop = Crop {
        w: source_width,
        h: active,
        x: 0,
        y: native.top,
    };
    let details = native_details(
        original.details.clone(),
        native,
        active,
        source_width,
        params,
    );
    let (decision, delta) = decide_letterbox(
        segment,
        crops,
        crop,
        active,
        details,
        meta,
        source_height,
        options,
    );
    (decision.action == Action::Apply).then_some((decision, delta))
}

fn native_details(
    details: Value,
    native: &NativeLetterbox,
    active: u32,
    source_width: u32,
    params: &NativeParams,
) -> Value {
    let mut map = details.as_object().cloned().unwrap_or_default();
    let cropdetect = json!({
        "active_height": map.get("active_height").cloned().unwrap_or(Value::Null),
        "active_p10": map.get("active_p10").cloned().unwrap_or(Value::Null),
        "crop": map.get("crop").cloned().unwrap_or(Value::Null),
        "bars": map.get("bars").cloned().unwrap_or(Value::Null),
        "n_crops": map.get("n_crops").cloned().unwrap_or(Value::Null),
        "barred_frac": map.get("barred_frac").cloned().unwrap_or(Value::Null),
        "scale_target": map.get("scale_target").cloned().unwrap_or(Value::Null),
        "position_y_target": map.get("position_y_target").cloned().unwrap_or(Value::Null),
        "coverage": map.get("coverage").cloned().unwrap_or(Value::Null),
    });
    map.insert("active_height".into(), json!(active));
    map.insert("active_p10".into(), json!(active));
    map.insert(
        "crop".into(),
        json!({
            "w": source_width,
            "h": active,
            "x": 0,
            "y": native.top,
            "ratio": round_to(native.fraction(), 3)
        }),
    );
    map.insert(
        "bars".into(),
        json!({"top": native.top, "bottom": native.bottom, "left": 0, "right": 0}),
    );
    map.insert("symmetric".into(), json!(true));
    map.insert("cropdetect".into(), cropdetect);
    map.insert(
        "native".into(),
        json!({
            "top": native.top,
            "bottom": native.bottom,
            "active": active,
            "n_frames": native.n_frames,
            "cluster_frames": native.cluster_frames,
            "fraction": round_to(native.fraction(), 3),
            "edge_threshold": params.edge_threshold,
            "dark_coverage": params.dark_coverage,
            "dark_luma": params.dark_luma,
            "cluster_min_frac": params.cluster_min_frac
        }),
    );
    map.insert("native_refined".into(), json!(true));
    Value::Object(map)
}

fn coverage(
    crops: &[Crop],
    segment: &Segment,
    scale: f64,
    position_y: f64,
    meta: &CompMeta,
) -> Value {
    let factor = scale / 100.0;
    let comp_y = position_y + segment.parent_dy;
    let mut tops = Vec::with_capacity(crops.len());
    let mut bottoms = Vec::with_capacity(crops.len());
    let mut lefts = Vec::with_capacity(crops.len());
    let mut rights = Vec::with_capacity(crops.len());
    let mut fails = Vec::new();
    for crop in crops {
        let top = comp_y + (crop.y as f64 - segment.anchor[1]) * factor;
        let bottom = top + crop.h as f64 * factor;
        let left = segment.comp_x + (crop.x as f64 - segment.anchor[0]) * factor;
        let right = left + crop.w as f64 * factor;
        tops.push(top);
        bottoms.push(bottom);
        lefts.push(left);
        rights.push(right);
        if !(top <= COVERAGE_TOL
            && bottom >= meta.height as f64 - COVERAGE_TOL
            && left <= COVERAGE_TOL
            && right >= meta.width as f64 - COVERAGE_TOL)
        {
            fails.push(json!({
                "crop": [crop.w, crop.h, crop.x, crop.y],
                "top": round_to(top, 1),
                "bottom": round_to(bottom, 1)
            }));
        }
    }
    json!({
        "covered": fails.is_empty(),
        "fail_count": fails.len(),
        "fail_frac": round_to(fails.len() as f64 / crops.len().max(1) as f64, 3),
        "worst_fail": fails.first().cloned().unwrap_or(Value::Null),
        "top_min": round_to(tops.iter().copied().fold(f64::INFINITY, f64::min), 1),
        "bottom_max": round_to(bottoms.iter().copied().fold(f64::NEG_INFINITY, f64::max), 1),
        "left_min": round_to(lefts.iter().copied().fold(f64::INFINITY, f64::min), 1),
        "right_max": round_to(rights.iter().copied().fold(f64::NEG_INFINITY, f64::max), 1),
        "tolerance_px": COVERAGE_TOL
    })
}

fn percentile(values: &[u32], quantile: f64) -> f64 {
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let index = round_to((ordered.len().saturating_sub(1)) as f64 * quantile, 0) as usize;
    ordered[index.min(ordered.len() - 1)] as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> CompMeta {
        CompMeta {
            id: "2".into(),
            width: 1080,
            height: 1920,
            fps: 25.0,
            duration: 10.0,
            n_layers: 2,
            n_segments: 1,
        }
    }

    fn segment() -> Segment {
        Segment {
            name: "Segment 1".into(),
            layer_id: 11,
            parent: None,
            start: 0.0,
            end: 10.0,
            source: Some("clip.mp4".into()),
            anchor: [960.0, 0.0, 0.0],
            scale: 180.0,
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

    fn letterbox_crops() -> Vec<Crop> {
        vec![
            Crop {
                w: 1920,
                h: 800,
                x: 0,
                y: 140,
            },
            Crop {
                w: 1920,
                h: 800,
                x: 0,
                y: 140,
            },
            Crop {
                w: 1920,
                h: 800,
                x: 0,
                y: 140,
            },
        ]
    }

    #[test]
    fn default_margin_matches_d056() {
        let (decision, delta) = decide_segment(
            &segment(),
            &letterbox_crops(),
            &meta(),
            1920,
            1080,
            &A1Options::default(),
        );
        assert_eq!(decision.action, crate::post::model::Action::Apply);
        assert_eq!(decision.details["scale_target"], 243.6);
        assert!(delta.unwrap().abs() > 100.0);
    }

    #[test]
    fn explicit_margin_is_honoured() {
        let options = A1Options {
            margin: 1.04,
            ..A1Options::default()
        };
        let (decision, _) = decide_segment(
            &segment(),
            &letterbox_crops(),
            &meta(),
            1920,
            1080,
            &options,
        );
        assert_eq!(decision.action, crate::post::model::Action::Apply);
        assert_eq!(decision.details["scale_target"], 249.6);
    }

    #[test]
    fn a3_candidate_defers_a1_apply_to_review() {
        let mut options = A1Options::default();
        options.defer_targets.insert("Segment 1".into());
        let (decision, delta) = decide_segment(
            &segment(),
            &letterbox_crops(),
            &meta(),
            1920,
            1080,
            &options,
        );
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert_eq!(decision.details["a1_deferred_on_a3"], true);
        assert!(decision.reason.contains("keyframes probables"));
        assert!(delta.is_none());
    }

    #[test]
    fn no_a3_candidate_keeps_a1_apply() {
        let mut options = A1Options::default();
        options.defer_targets.insert("Segment 2".into());
        let (decision, _) = decide_segment(
            &segment(),
            &letterbox_crops(),
            &meta(),
            1920,
            1080,
            &options,
        );
        assert_eq!(decision.action, crate::post::model::Action::Apply);
    }

    #[test]
    fn keyframed_segment_is_never_applied() {
        let mut segment = segment();
        segment.position_keys.push(crate::post::ecproj::Keyframe {
            t: 1.0,
            v: [0.0, 0.0, 0.0],
        });
        let crops = vec![Crop {
            w: 1920,
            h: 800,
            x: 0,
            y: 140,
        }];
        let (decision, delta) =
            decide_segment(&segment, &crops, &meta(), 1920, 1080, &A1Options::default());
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(delta.is_none());
    }

    #[test]
    fn full_frame_720p_is_noop() {
        let crops = vec![Crop {
            w: 1280,
            h: 720,
            x: 0,
            y: 0,
        }];
        let (decision, delta) = decide_segment(
            &segment(),
            &crops,
            &meta(),
            1280,
            720,
            &A1Options::default(),
        );
        assert_eq!(decision.action, crate::post::model::Action::Noop);
        assert!(delta.is_none());
    }

    #[test]
    fn asymmetric_bars_are_reviewed() {
        let crops = vec![Crop {
            w: 1920,
            h: 900,
            x: 0,
            y: 0,
        }];
        let (decision, delta) = decide_segment(
            &segment(),
            &crops,
            &meta(),
            1920,
            1080,
            &A1Options::default(),
        );
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(delta.is_none());
    }

    fn cropdetect_880() -> Vec<Crop> {
        vec![
            Crop {
                w: 1920,
                h: 880,
                x: 0,
                y: 100,
            },
            Crop {
                w: 1920,
                h: 880,
                x: 0,
                y: 100,
            },
            Crop {
                w: 1920,
                h: 880,
                x: 0,
                y: 100,
            },
        ]
    }

    fn native(top: u32, bottom: u32, n_frames: usize, cluster_frames: usize) -> NativeLetterbox {
        NativeLetterbox {
            top,
            bottom,
            n_frames,
            cluster_frames,
        }
    }

    fn refine(native: &NativeLetterbox, crops: &[Crop]) -> Option<(Decision, Option<f64>)> {
        let (original, _) = decide_segment(
            &segment(),
            crops,
            &meta(),
            1920,
            1080,
            &A1Options::default(),
        );
        assert_eq!(original.action, Action::Apply);
        refine_with_native(
            &segment(),
            crops,
            &original,
            native,
            &meta(),
            1920,
            1080,
            &A1Options::default(),
            &NativeParams::default(),
        )
    }

    #[test]
    fn native_refinement_uses_native_geometry() {
        // Given: cropdetect 100/100 (880 px actifs), natif 140/140 (800 px).
        // When:  le natif raffine un apply déjà décidé.
        // Then:  le scale suit le natif et les deux mesures restent tracées.
        let crops = cropdetect_880();
        let (refined, delta) = refine(&native(140, 140, 4, 4), &crops).expect("raffinement");
        assert_eq!(refined.action, Action::Apply);
        assert_eq!(refined.details["scale_target"], 243.6);
        assert_eq!(refined.details["native_refined"], true);
        assert_eq!(refined.details["native"]["top"], 140);
        assert_eq!(refined.details["native"]["bottom"], 140);
        assert_eq!(refined.details["native"]["fraction"], 1.0);
        assert_eq!(refined.details["cropdetect"]["active_height"], 880);
        assert_eq!(refined.details["cropdetect"]["scale_target"], 221.5);
        assert!(delta.unwrap().abs() > 100.0);
    }

    #[test]
    fn native_noop_keeps_cropdetect_apply() {
        // Given: le natif ne voit aucune barre (0/0).
        // When:  il est proposé comme raffinement.
        // Then:  il est ignoré et cropdetect reste la décision.
        assert!(refine(&native(0, 0, 4, 4), &cropdetect_880()).is_none());
    }

    #[test]
    fn native_asymmetric_review_keeps_cropdetect_apply() {
        // Given: une seule barre native (0/140) ou des barres trop inégales.
        // When:  le natif est proposé comme raffinement.
        // Then:  le veto asymétrie A1 s'applique et cropdetect reste seul.
        assert!(refine(&native(0, 140, 4, 4), &cropdetect_880()).is_none());
        assert!(refine(&native(140, 40, 4, 4), &cropdetect_880()).is_none());
        assert!(refine(&native(600, 600, 4, 4), &cropdetect_880()).is_none());
    }

    #[test]
    fn unstable_native_cluster_keeps_cropdetect_apply() {
        // Given: un cluster dominant à 50 %, sous le seuil de 60 %.
        // When:  le natif est proposé comme raffinement.
        // Then:  la stabilité temporelle le rejette.
        assert!(refine(&native(140, 140, 4, 2), &cropdetect_880()).is_none());
        assert!(refine(&native(140, 140, 1, 1), &cropdetect_880()).is_none());
    }

    #[test]
    fn native_coverage_veto_keeps_cropdetect_apply() {
        // Given: un natif 20/20 qui remonterait la zone active au-dessus du
        //        comp pour les crops cropdetect à y=100.
        // When:  le natif est proposé comme raffinement.
        // Then:  le veto couverture A1 le rejette et cropdetect reste seul.
        assert!(refine(&native(20, 20, 4, 4), &cropdetect_880()).is_none());
    }

    #[test]
    fn native_refinement_is_opt_in() {
        assert!(!A1Options::default().refine_native);
    }
}
