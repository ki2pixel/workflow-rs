//! A2 — bannière basse pleine largeur, avec abstentions conservatrices.
//!
//! Une bande haute, un logo, une bande partielle, un décor ou une bande dont
//! la géométrie bouge partent en `review`. Seule une bannière basse large,
//! persistante, claire et géométriquement stable est appliquée.

use crate::post::detect::{band_candidates, BandCandidate};
use crate::post::ecproj::{CompMeta, Segment};
use crate::post::model::Decision;
use serde_json::{json, Map, Value};
use std::path::Path;
use workflow_schema::round::round_to;

const BANNER_MIN_WIDTH: f64 = 0.50;
const LOGO_MAX_WIDTH: f64 = 0.35;
const BANNER_MAX_HEIGHT: i64 = 200;
const BANNER_MIN_HEIGHT: i64 = 24;
const DARK_MEDIAN: f64 = 60.0;
const ANIM_STD: f64 = 35.0;
const GEOM_STD: f64 = 12.0;

pub fn decide_a2(
    segments: &[Segment],
    video: Option<&Path>,
    ffmpeg: &Path,
    ffprobe: &Path,
    meta: &CompMeta,
    source_height: u32,
    margin: f64,
) -> Vec<Decision> {
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
    decide_from_candidates(
        segments,
        &candidates,
        source_height,
        meta.height as f64,
        margin,
    )
}

pub fn decide_from_candidates(
    segments: &[Segment],
    candidates: &[BandCandidate],
    source_height: u32,
    comp_height: f64,
    margin: f64,
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

    if selected.width_frac < LOGO_MAX_WIDTH {
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
    if selected.center_y_std > geom_std || selected.height_std > geom_std {
        return vec![Decision::review(
            "A2",
            "comp",
            format!(
                "bande basse animée (centre ±{:.0} px, hauteur ±{:.0} px), géométrie non fiable",
                selected.center_y_std, selected.height_std
            ),
            0.4,
            details,
        )];
    }
    if selected.median_brightness < DARK_MEDIAN {
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
    vec![Decision::apply(
        "A2",
        "comp",
        format!(
            "bannière {} px, largeur {:.0}% -> {scale}%",
            selected.height_src,
            selected.width_frac * 100.0
        ),
        0.9,
        details,
    )]
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
        }
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
