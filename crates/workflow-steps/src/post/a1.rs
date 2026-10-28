//! A1 — letterbox noir et compensation de Position Y.
//!
//! A1 ne modifie jamais l'Ancre ni les keyframes. Le scale est calculé sur le
//! 10e percentile des hauteurs actives du segment ; la couverture de tous les
//! crops est vérifiée après scale et compensation avant toute application.

use crate::post::detect::{cropdetect, Crop};
use crate::post::ecproj::{CompMeta, Segment};
use crate::post::model::Decision;
use serde_json::{json, Map, Value};
use std::path::Path;
use workflow_schema::round::round_to;

const ACTIVE_NOOP: u32 = 1072;
const INTERMITTENT_DELTA: i64 = 24;
const OUTLIER_FRAC: f64 = 0.10;
const COVERAGE_TOL: f64 = 0.5;

pub fn decide_a1(
    segments: &[Segment],
    video: Option<&Path>,
    ffmpeg: &Path,
    meta: &CompMeta,
    source_width: u32,
    source_height: u32,
    margin: f64,
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
        let (decision, delta) =
            decide_segment(segment, &crops, meta, source_width, source_height, margin);
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
    margin: f64,
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

    let scale = round_to(meta.height as f64 / active as f64 * 100.0 * margin, 1);
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
        }
    }

    #[test]
    fn computes_scale_and_y_compensation_for_black_letterbox() {
        let crops = vec![
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
        ];
        let (decision, delta) = decide_segment(&segment(), &crops, &meta(), 1920, 1080, 1.04);
        assert_eq!(decision.action, crate::post::model::Action::Apply);
        assert_eq!(decision.details["scale_target"], 249.6);
        assert!(delta.unwrap().abs() > 100.0);
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
        let (decision, delta) = decide_segment(&segment, &crops, &meta(), 1920, 1080, 1.04);
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
        let (decision, delta) = decide_segment(&segment(), &crops, &meta(), 1280, 720, 1.04);
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
        let (decision, delta) = decide_segment(&segment(), &crops, &meta(), 1920, 1080, 1.04);
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert!(delta.is_none());
    }
}
