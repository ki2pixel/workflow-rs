//! A4 — fusions/splits mécaniques, en `review` uniquement.
//!
//! La détection expérimentale des cuts durs intra-segment reste hors
//! production (trop de faux positifs sur les mouvements rapides).

use crate::post::ecproj::Segment;
use crate::post::model::Decision;
use std::path::Path;
use workflow_schema::round::round_to;

const SHORT_SEGMENT: f64 = 0.5;

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
    for segment in segments {
        let f0 = round_to(segment.start * fps, 0) as i64 + 1;
        let f1 = round_to(segment.end * fps, 0) as i64;
        let details = serde_json::json!({
            "start": round_to(segment.start, 3),
            "end": round_to(segment.end, 3)
        });
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
}
