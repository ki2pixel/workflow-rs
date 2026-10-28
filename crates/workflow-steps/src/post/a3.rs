//! A3 — candidats de keyframes, jamais appliqués (D-030).

use crate::post::ecproj::Segment;
use crate::post::model::Decision;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use workflow_schema::round::round_to;

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

pub fn decide_a3(segments: &[Segment], ae_path: Option<&Path>, fps: f64) -> Vec<Decision> {
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
        if segment.keyframed() {
            decisions.push(Decision::noop(
                "A3",
                segment.name.clone(),
                "segment déjà keyframé dans le brut",
                0.8,
                details,
            ));
        } else if segment.duration() < 1.0 {
            decisions.push(Decision::noop(
                "A3",
                segment.name.clone(),
                "segment < 1 s",
                0.7,
                details,
            ));
        } else if amplitude > 100.0 && gap < 0.3 {
            let count = (round_to(amplitude / 120.0, 0) as i64 + 1).clamp(2, 6);
            let mut details = details;
            details["candidate_keys"] = serde_json::json!(count);
            decisions.push(Decision::review(
                "A3",
                segment.name.clone(),
                format!("candidat {count} clés (amplitude {amplitude:.0} px), A3 désactivé"),
                0.35,
                details,
            ));
        } else {
            decisions.push(Decision::noop(
                "A3",
                segment.name.clone(),
                format!(
                    "mouvement faible/instable (amp {amplitude:.0} px, trou {:.0}%)",
                    gap * 100.0
                ),
                0.6,
                details,
            ));
        }
    }
    decisions
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
}
