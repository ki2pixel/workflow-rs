//! A4 — fusions/splits mécaniques, en `review` uniquement.
//!
//! La détection expérimentale des cuts durs intra-segment reste hors
//! production (trop de faux positifs sur les mouvements rapides). Les segments
//! de moins de 0,3 s sans continuité de source des deux côtés sont tagués
//! `flash_probable` (D-056), sans changer l'action.

use crate::post::ecproj::Segment;
use crate::post::model::Decision;
use std::path::Path;
use workflow_schema::round::round_to;

const SHORT_SEGMENT: f64 = 0.5;
const FLASH_MAX_DURATION: f64 = 0.3;
const SOURCE_CONTINUITY_TOL_S: f64 = 0.002;

#[derive(Debug, Clone)]
pub struct CsvRow {
    pub frame_in: f64,
    pub frame_out: f64,
}

pub fn parse_csv(path: &Path) -> Vec<CsvRow> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let Some(header) = lines.first() else {
        return Vec::new();
    };
    let header = split_csv_line(header);
    let frame_in = header.iter().position(|value| value == "Frame In");
    let frame_out = header.iter().position(|value| value == "Frame Out");
    let (Some(frame_in), Some(frame_out)) = (frame_in, frame_out) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for line in lines.iter().skip(1) {
        let fields = split_csv_line(line);
        let (Some(input), Some(output)) = (fields.get(frame_in), fields.get(frame_out)) else {
            continue;
        };
        let (Ok(input), Ok(output)) = (input.trim().parse::<f64>(), output.trim().parse::<f64>())
        else {
            continue;
        };
        rows.push(CsvRow {
            frame_in: input,
            frame_out: output,
        });
    }
    rows
}

pub fn decide_a4(segments: &[Segment], rows: &[CsvRow], fps: f64) -> Vec<Decision> {
    if rows.is_empty() {
        return vec![Decision::review(
            "A4",
            "comp",
            "CSV absent ou vide, A4 non calculé",
            0.3,
            serde_json::json!({"reason": "csv_missing"}),
        )];
    }
    let mut decisions = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        let f0 = round_to(segment.start * fps, 0) as i64 + 1;
        let f1 = round_to(segment.end * fps, 0) as i64;
        let (previous_continuity, next_continuity) = source_continuity(segments, index);
        let mut details = serde_json::json!({
            "start": round_to(segment.start, 3),
            "end": round_to(segment.end, 3)
        });
        if segment.duration() < FLASH_MAX_DURATION {
            details["source_continuity"] = serde_json::json!({
                "previous": previous_continuity,
                "next": next_continuity
            });
            if !(previous_continuity && next_continuity) {
                details["flash_probable"] = serde_json::json!(true);
            }
        }
        if segment.duration() < SHORT_SEGMENT {
            decisions.push(Decision::review(
                "A4",
                segment.name.clone(),
                format!(
                    "segment court ({:.2} s), fusion candidate",
                    segment.duration()
                ),
                0.5,
                details,
            ));
            continue;
        }
        let covering: Vec<&CsvRow> = rows
            .iter()
            .filter(|row| row.frame_in <= f0 as f64 && row.frame_out >= f1 as f64)
            .collect();
        if covering.len() > 1 {
            decisions.push(Decision::review(
                "A4",
                segment.name.clone(),
                format!("couvre {} entrées CSV, fusion candidate", covering.len()),
                0.5,
                details,
            ));
        } else if let Some(row) = covering.first() {
            if (f0 as f64 - row.frame_in).abs() > 1.0 || (f1 as f64 - row.frame_out).abs() > 1.0 {
                decisions.push(Decision::review(
                    "A4",
                    segment.name.clone(),
                    "bornes non alignées sur le CSV, split candidate",
                    0.5,
                    details,
                ));
            }
        } else {
            decisions.push(Decision::review(
                "A4",
                segment.name.clone(),
                "segment hors CSV",
                0.4,
                details,
            ));
        }
    }
    decisions
}

/// Voisinage temporel de source, indépendant de l'ordre des calques.
///
/// Un flash est un segment court qui n'est pas encadré par deux segments de
/// même source : dans le corpus GOOD, les 7 `none` < 0,3 s sont des segments de
/// bord, alors que les 4 `extend` sont intérieurs et continus des deux côtés.
/// Les voisins sont cherchés par bornes temporelles, pas par index, car
/// `ecproj::segments` suit l'ordre des calques de la composition.
fn source_continuity(segments: &[Segment], index: usize) -> (bool, bool) {
    let segment = &segments[index];
    let previous = segments
        .iter()
        .enumerate()
        .any(|(candidate_index, candidate)| {
            candidate_index != index
                && same_source(candidate, segment)
                && candidate.start < segment.start
                && (candidate.end - segment.start).abs() <= SOURCE_CONTINUITY_TOL_S
        });
    let next = segments
        .iter()
        .enumerate()
        .any(|(candidate_index, candidate)| {
            candidate_index != index
                && same_source(segment, candidate)
                && candidate.start > segment.start
                && (segment.end - candidate.start).abs() <= SOURCE_CONTINUITY_TOL_S
        });
    (previous, next)
}

fn same_source(left: &Segment, right: &Segment) -> bool {
    matches!(
        (left.source.as_deref(), right.source.as_deref()),
        (Some(left), Some(right)) if left == right
    )
}

fn split_csv_line(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                values.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(character);
    }
    values.push(current);
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_frame_columns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.csv");
        std::fs::write(
            &path,
            "No,Timecode In,Timecode Out,Frame In,Frame Out\n1,00:00:00.000,00:00:00.200,1,6\n",
        )
        .unwrap();
        let rows = parse_csv(&path);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].frame_in, 1.0);
        assert_eq!(rows[0].frame_out, 6.0);
    }

    #[test]
    fn missing_csv_is_a_single_review() {
        let decisions = decide_a4(&[], &[], 25.0);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
    }

    fn segment(name: &str, start: f64, end: f64, source: Option<&str>) -> Segment {
        Segment {
            name: name.into(),
            layer_id: 11,
            parent: None,
            start,
            end,
            source: source.map(str::to_string),
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

    fn covering_rows() -> Vec<CsvRow> {
        vec![CsvRow {
            frame_in: 1.0,
            frame_out: 10_000.0,
        }]
    }

    #[test]
    fn first_short_segment_is_a_flash_probable() {
        let segments = vec![
            segment("Segment 1", 0.0, 0.12, Some("clip.mp4")),
            segment("Segment 2", 0.12, 1.0, Some("clip.mp4")),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
        assert_eq!(decisions[0].details["flash_probable"], true);
        assert_eq!(
            decisions[0].details["source_continuity"],
            serde_json::json!({"previous": false, "next": true})
        );
    }

    #[test]
    fn interior_short_segment_with_continuity_is_not_a_flash() {
        let segments = vec![
            segment("Segment 1", 0.0, 1.0, Some("clip.mp4")),
            segment("Segment 2", 1.0, 1.16, Some("clip.mp4")),
            segment("Segment 3", 1.16, 2.0, Some("clip.mp4")),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(
            decisions[1].details["flash_probable"],
            serde_json::Value::Null
        );
        assert_eq!(
            decisions[1].details["source_continuity"],
            serde_json::json!({"previous": true, "next": true})
        );
    }

    #[test]
    fn continuity_does_not_depend_on_layer_order() {
        // Given: les mêmes segments que le cas intérieur, calques désordonnés.
        // When:  A4 calcule la continuité du segment court.
        // Then:  les voisins sont retrouvés par leurs bornes temporelles.
        let segments = vec![
            segment("Segment 3", 1.16, 2.0, Some("clip.mp4")),
            segment("Segment 2", 1.0, 1.16, Some("clip.mp4")),
            segment("Segment 1", 0.0, 1.0, Some("clip.mp4")),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(
            decisions[1].details["flash_probable"],
            serde_json::Value::Null
        );
        assert_eq!(
            decisions[1].details["source_continuity"],
            serde_json::json!({"previous": true, "next": true})
        );
    }

    #[test]
    fn different_source_on_one_side_is_a_flash() {
        let segments = vec![
            segment("Segment 1", 0.0, 1.0, Some("clip.mp4")),
            segment("Segment 2", 1.0, 1.16, Some("other.mp4")),
            segment("Segment 3", 1.16, 2.0, Some("clip.mp4")),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(decisions[1].details["flash_probable"], true);
    }

    #[test]
    fn flash_threshold_is_strictly_below_030() {
        let segments = vec![
            segment("Segment 1", 0.0, 0.3, Some("clip.mp4")),
            segment("Segment 2", 0.3, 1.0, Some("clip.mp4")),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(
            decisions[0].details["flash_probable"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn missing_source_is_a_flash() {
        let segments = vec![
            segment("Segment 1", 0.0, 0.2, None),
            segment("Segment 2", 0.2, 1.0, None),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(decisions[0].details["flash_probable"], true);
    }

    #[test]
    fn source_gap_is_a_flash() {
        let segments = vec![
            segment("Segment 1", 0.0, 1.0, Some("clip.mp4")),
            segment("Segment 2", 1.04, 1.2, Some("clip.mp4")),
            segment("Segment 3", 1.2, 2.0, Some("clip.mp4")),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(decisions[1].details["flash_probable"], true);
    }

    #[test]
    fn long_segment_has_no_flash_metadata() {
        let segments = vec![
            segment("Segment 1", 0.0, 0.5, Some("clip.mp4")),
            segment("Segment 2", 0.5, 1.0, Some("clip.mp4")),
        ];
        let decisions = decide_a4(&segments, &covering_rows(), 25.0);
        assert_eq!(
            decisions[0].details["source_continuity"],
            serde_json::Value::Null
        );
        assert_eq!(
            decisions[0].details["flash_probable"],
            serde_json::Value::Null
        );
    }
}
