//! A3 — candidats de keyframes, jamais appliqués (D-030/D-056).
//!
//! Les revues sont triées par amplitude et nombre de clés : sous 250 px avec
//! au plus deux clés, la décision devient `noop` (R5). La confiance est
//! signalée dans `high_confidence` (R5). Le trou de suivi peut être assoupli
//! en revue seulement via `a3_relaxed_gap` (R6). Aucun chemin ne produit
//! `Action::Apply` (R7).

use crate::post::ecproj::Segment;
use crate::post::model::Decision;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use workflow_schema::round::round_to;

const A3_MIN_AMPLITUDE: f64 = 100.0;
const A3_NOOP_AMPLITUDE: f64 = 250.0;
const A3_NOOP_MAX_KEYS: i64 = 2;
const A3_RELAXED_GAP_AMPLITUDE: f64 = 200.0;
const A3_STRICT_GAP: f64 = 0.30;
const A3_RELAXED_GAP: f64 = 0.70;
const A3_HIGH_AMPLITUDE: f64 = 700.0;
const A3_HIGH_KEYS: i64 = 6;
const A3_STRONG_AMPLITUDE: f64 = 300.0;
const A3_STRONG_KEYS: i64 = 4;

#[derive(Debug, Clone)]
struct Recommendation {
    start_frame: i64,
    end_frame: i64,
    object_id: Option<String>,
}

#[derive(Debug, Clone)]
struct TrackedObject {
    id: Option<String>,
    centroid_x: f64,
}

pub fn decide_a3(
    segments: &[Segment],
    ae_path: Option<&Path>,
    fps: f64,
    relaxed_gap: bool,
) -> Vec<Decision> {
    let Some(ae_path) = ae_path else {
        return vec![Decision::review(
            "A3",
            "comp",
            "pas de _ae.json, A3 non calculé",
            0.3,
            serde_json::json!({"reason": "ae_missing"}),
        )];
    };
    if !ae_path.is_file() {
        return vec![Decision::review(
            "A3",
            "comp",
            format!("{} introuvable, A3 non calculé", ae_path.display()),
            0.3,
            serde_json::json!({"reason": "ae_missing"}),
        )];
    }
    let bytes = match std::fs::read(ae_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return vec![Decision::review(
                "A3",
                "comp",
                format!("lecture _ae.json impossible: {error}"),
                0.3,
                serde_json::json!({"reason": "ae_unreadable"}),
            )];
        }
    };
    let payload: Value = match serde_json::from_slice(&bytes) {
        Ok(payload) => payload,
        Err(error) => {
            return vec![Decision::review(
                "A3",
                "comp",
                format!("_ae.json invalide: {error}"),
                0.3,
                serde_json::json!({"reason": "ae_invalid"}),
            )];
        }
    };
    let frames = parse_frames(payload.get("dataByFrame"));
    let recommendations = parse_recommendations(payload.get("segment_recommendations"));
    if recommendations.is_empty() || frames.is_empty() {
        return vec![Decision::review(
            "A3",
            "comp",
            "pas de segment_recommendations/dataByFrame exploitables",
            0.3,
            serde_json::json!({"reason": "ae_empty"}),
        )];
    }

    let mut decisions = Vec::new();
    for segment in segments {
        let f0 = round_to(segment.start * fps, 0) as i64 + 1;
        let f1 = round_to(segment.end * fps, 0) as i64;
        let Some(recommendation) = pick_recommendation(&recommendations, f0, f1) else {
            continue;
        };
        let object_id = recommendation.object_id.clone();
        let mut xs = Vec::new();
        if f1 >= f0 {
            for frame in f0..=f1 {
                if let Some(objects) = frames.get(&frame) {
                    if let Some(object) = objects.iter().find(|object| {
                        object.id.is_some() && object.id.as_deref() == object_id.as_deref()
                    }) {
                        xs.push(object.centroid_x);
                    }
                }
            }
        }
        let total = (f1 - f0 + 1).max(1);
        let gap = 1.0 - xs.len() as f64 / total as f64;
        let amplitude = if xs.len() >= 2 {
            xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - xs.iter().copied().fold(f64::INFINITY, f64::min)
        } else {
            0.0
        };
        let details = serde_json::json!({
            "start": round_to(segment.start, 3),
            "end": round_to(segment.end, 3),
            "object": object_id,
            "amplitude_px": round_to(amplitude, 1),
            "gap": round_to(gap, 3),
            "frames": xs.len(),
            "total": total
        });
        decisions.push(classify_tracking(
            segment,
            amplitude,
            gap,
            details,
            relaxed_gap,
        ));
    }
    decisions
}

fn candidate_keys(amplitude: f64) -> i64 {
    (round_to(amplitude / 120.0, 0) as i64 + 1).clamp(2, 6)
}

fn classify_tracking(
    segment: &Segment,
    amplitude: f64,
    gap: f64,
    details: Value,
    relaxed_gap: bool,
) -> Decision {
    if segment.keyframed() {
        return Decision::noop(
            "A3",
            segment.name.clone(),
            "segment déjà keyframé dans le brut",
            0.8,
            details,
        );
    }
    if segment.duration() < 1.0 {
        return Decision::noop("A3", segment.name.clone(), "segment < 1 s", 0.7, details);
    }

    let keys = candidate_keys(amplitude);
    let strict_gap = gap < A3_STRICT_GAP;
    let relaxed = relaxed_gap && amplitude >= A3_RELAXED_GAP_AMPLITUDE && gap < A3_RELAXED_GAP;
    let gap_ok = strict_gap || relaxed;
    if amplitude <= A3_MIN_AMPLITUDE || !gap_ok {
        return Decision::noop(
            "A3",
            segment.name.clone(),
            format!(
                "mouvement faible/instable (amp {amplitude:.0} px, trou {:.0}%)",
                gap * 100.0
            ),
            0.6,
            details,
        );
    }

    // R5 : sous 250 px avec au plus deux clés candidates, la revue est du
    // bruit pour les keyframes (4,1 % de vrais positifs dans le corpus GOOD).
    let mut details = details;
    details["candidate_keys"] = json!(keys);
    if amplitude < A3_NOOP_AMPLITUDE && keys <= A3_NOOP_MAX_KEYS {
        return Decision::noop(
            "A3",
            segment.name.clone(),
            "mouvement faible ou nombre de clés insuffisant (< 250 px, <= 2 clés)",
            0.6,
            details,
        );
    }

    if relaxed && !strict_gap {
        details["gap_relaxed"] = json!(true);
    }
    let high_confidence = amplitude >= A3_HIGH_AMPLITUDE || keys == A3_HIGH_KEYS;
    if high_confidence {
        details["high_confidence"] = json!(true);
    }
    // Signal réutilisé par R2 dans run.rs : clés nombreuses ou mouvement
    // ample et continu, sans seuil supplémentaire côté A1.
    if keys >= A3_STRONG_KEYS || (amplitude >= A3_STRONG_AMPLITUDE && gap < A3_STRICT_GAP) {
        details["strong_candidate"] = json!(true);
    }
    let reason = if high_confidence {
        format!("candidat {keys} clés haute confiance (amplitude {amplitude:.0} px), A3 désactivé")
    } else {
        format!("candidat {keys} clés (amplitude {amplitude:.0} px), A3 désactivé")
    };
    let confidence = if high_confidence { 0.55 } else { 0.35 };
    Decision::review("A3", segment.name.clone(), reason, confidence, details)
}

fn parse_frames(value: Option<&Value>) -> BTreeMap<i64, Vec<TrackedObject>> {
    let mut frames = BTreeMap::new();
    let Some(object) = value.and_then(Value::as_object) else {
        return frames;
    };
    for (key, items) in object {
        let Ok(frame) = key.parse::<i64>() else {
            continue;
        };
        let Some(items) = items.as_array() else {
            continue;
        };
        let objects = items
            .iter()
            .filter_map(|item| {
                Some(TrackedObject {
                    id: item.get("id").and_then(value_as_id),
                    centroid_x: item.get("centroid_x").and_then(Value::as_f64)?,
                })
            })
            .collect();
        frames.insert(frame, objects);
    }
    frames
}

fn value_as_id(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| crate::post::ecproj::as_i64(value).map(|id| id.to_string()))
}

fn parse_recommendations(value: Option<&Value>) -> Vec<Recommendation> {
    let Some(object) = value.and_then(Value::as_object) else {
        return Vec::new();
    };
    object
        .values()
        .filter_map(|item| {
            Some(Recommendation {
                start_frame: item
                    .get("start_frame")
                    .and_then(crate::post::ecproj::as_i64)?,
                end_frame: item
                    .get("end_frame")
                    .and_then(crate::post::ecproj::as_i64)?,
                object_id: item.get("recommended_object_id").and_then(value_as_id),
            })
        })
        .collect()
}

fn pick_recommendation(
    recommendations: &[Recommendation],
    f0: i64,
    f1: i64,
) -> Option<&Recommendation> {
    let mut best: Option<(i64, &Recommendation)> = None;
    for recommendation in recommendations {
        if recommendation.start_frame > f1 || recommendation.end_frame < f0 {
            continue;
        }
        let overlap = f1.min(recommendation.end_frame) - f0.max(recommendation.start_frame);
        if best.is_none_or(|(current, _)| overlap > current) {
            best = Some((overlap, recommendation));
        }
    }
    best.map(|(_, recommendation)| recommendation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_largest_overlap() {
        let recommendations = vec![
            Recommendation {
                start_frame: 0,
                end_frame: 10,
                object_id: Some("obj_1".into()),
            },
            Recommendation {
                start_frame: 5,
                end_frame: 20,
                object_id: Some("obj_2".into()),
            },
        ];
        assert_eq!(
            pick_recommendation(&recommendations, 6, 15)
                .and_then(|recommendation| recommendation.object_id.as_deref()),
            Some("obj_2")
        );
    }

    fn tracking_segment(duration: f64) -> Segment {
        Segment {
            name: "Segment 1".into(),
            layer_id: 11,
            parent: None,
            start: 0.0,
            end: duration,
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

    fn tracking_decision(amplitude: f64, gap: f64, relaxed_gap: bool) -> Decision {
        classify_tracking(
            &tracking_segment(4.0),
            amplitude,
            gap,
            json!({"start": 0.0, "end": 4.0}),
            relaxed_gap,
        )
    }

    #[test]
    fn low_signal_below_250px_and_two_keys_is_noop() {
        // Given: un mouvement de 120 px avec deux clés candidates.
        // When:  A3 classe le segment.
        // Then:  la revue est filtrée en noop explicite.
        let decision = tracking_decision(120.0, 0.1, false);
        assert_eq!(decision.action, crate::post::model::Action::Noop);
        assert!(decision.reason.contains("< 250 px"));
        assert_eq!(decision.details["candidate_keys"], 2);
    }

    #[test]
    fn three_keys_below_250px_stay_reviewed() {
        let decision = tracking_decision(200.0, 0.1, false);
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert_eq!(decision.details["candidate_keys"], 3);
    }

    #[test]
    fn amplitude_700px_is_high_confidence() {
        let decision = tracking_decision(700.0, 0.1, false);
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert_eq!(decision.details["high_confidence"], true);
        assert!(decision.reason.contains("haute confiance"));
    }

    #[test]
    fn six_candidate_keys_are_high_confidence() {
        let decision = tracking_decision(620.0, 0.1, false);
        assert_eq!(decision.action, crate::post::model::Action::Review);
        assert_eq!(decision.details["candidate_keys"], 6);
        assert_eq!(decision.details["high_confidence"], true);
    }

    #[test]
    fn strong_candidate_is_exposed_for_r2() {
        let decision = tracking_decision(320.0, 0.1, false);
        assert_eq!(decision.details["strong_candidate"], true);
        let weak = tracking_decision(160.0, 0.1, false);
        assert_eq!(weak.details["strong_candidate"], Value::Null);
    }

    #[test]
    fn relaxed_gap_is_opt_in() {
        let strict = tracking_decision(250.0, 0.5, false);
        assert_eq!(strict.action, crate::post::model::Action::Noop);
        let relaxed = tracking_decision(250.0, 0.5, true);
        assert_eq!(relaxed.action, crate::post::model::Action::Review);
        assert_eq!(relaxed.details["gap_relaxed"], true);
    }

    #[test]
    fn relaxed_gap_requires_200px() {
        let decision = tracking_decision(150.0, 0.5, true);
        assert_eq!(decision.action, crate::post::model::Action::Noop);
        assert_eq!(decision.details["gap_relaxed"], Value::Null);
    }

    #[test]
    fn relaxed_gap_still_requires_070() {
        let decision = tracking_decision(300.0, 0.75, true);
        assert_eq!(decision.action, crate::post::model::Action::Noop);
    }

    #[test]
    fn keyframed_and_short_segments_are_noop() {
        let mut keyframed = tracking_segment(4.0);
        keyframed.position_keys.push(crate::post::ecproj::Keyframe {
            t: 1.0,
            v: [0.0, 0.0, 0.0],
        });
        let decision = classify_tracking(&keyframed, 800.0, 0.0, json!({}), true);
        assert_eq!(decision.action, crate::post::model::Action::Noop);
        assert!(decision.reason.contains("keyframé"));

        let decision = classify_tracking(&tracking_segment(0.5), 800.0, 0.0, json!({}), true);
        assert_eq!(decision.action, crate::post::model::Action::Noop);
        assert!(decision.reason.contains("< 1 s"));
    }

    #[test]
    fn a3_never_returns_apply() {
        for amplitude in [0.0, 120.0, 250.0, 320.0, 620.0, 800.0] {
            for gap in [0.0, 0.1, 0.5, 0.75] {
                for relaxed in [false, true] {
                    let decision = tracking_decision(amplitude, gap, relaxed);
                    assert_ne!(decision.action, crate::post::model::Action::Apply);
                }
            }
        }
    }
}
