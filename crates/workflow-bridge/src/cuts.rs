//! Pont de découpe CSV (`media_solution_bridge.py`).
//!
//! Ce mode est appelé par le panneau After Effects avec un manifeste
//! `{"mode": "cuts", "csv_path", "frame_rate", "comp_duration", "config"}`.

use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
struct Segment {
    num: String,
    start_time: f64,
    end_time: f64,
}

pub fn analyze(manifest: &Value) -> Result<Value, String> {
    let object = manifest
        .as_object()
        .ok_or_else(|| "Manifest invalide (doit être un objet JSON)".to_string())?;
    if object.get("mode").and_then(Value::as_str) != Some("cuts") {
        return Err("manifest.mode doit être 'cuts'".into());
    }
    let csv_path = object
        .get("csv_path")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "manifest.csv_path requis".to_string())?;
    let frame_rate = parse_float(object.get("frame_rate"))
        .filter(|value| *value > 0.0)
        .ok_or_else(|| "manifest.frame_rate invalide".to_string())?;
    let comp_duration = parse_float(object.get("comp_duration"));
    let snap_factor = object
        .get("config")
        .and_then(Value::as_object)
        .and_then(|config| parse_float(config.get("SNAP_FACTOR")))
        .filter(|value| *value > 0.0)
        .unwrap_or(1.1);

    let csv_path = Path::new(csv_path);
    if !csv_path.exists() {
        return Err(format!("CSV introuvable: {}", csv_path.display()));
    }
    let content = fs::read_to_string(csv_path)
        .map_err(|error| format!("CSV introuvable: {} ({error})", csv_path.display()))?;
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let raw_segments = parse_segments(content, frame_rate);
    let (segments, warnings) =
        adjust_segments(&raw_segments, frame_rate, comp_duration, snap_factor);

    Ok(json!({
        "segments": segments,
        "warnings": warnings,
        "stats": {
            "raw_segments": raw_segments.len(),
            "kept_segments": segments.len(),
        }
    }))
}

fn parse_segments(content: &str, frame_rate: f64) -> Vec<Segment> {
    let lines: Vec<&str> = content.lines().collect();
    let start_line = detect_start_line(&lines);
    let mut segments = Vec::new();

    for line in lines.iter().skip(start_line) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let columns: Vec<&str> = trimmed.split(',').map(str::trim).collect();
        if columns.len() < 3 {
            continue;
        }

        let mut start_time = -1.0;
        let mut end_time = -1.0;
        if columns.len() >= 5 {
            let frame_in = parse_int_str(columns[3]);
            let frame_out = parse_int_str(columns[4]);
            if let (Some(frame_in), Some(frame_out)) = (frame_in, frame_out) {
                if frame_in >= 1 && frame_out >= frame_in {
                    start_time = (frame_in - 1) as f64 / frame_rate;
                    end_time = frame_out as f64 / frame_rate;
                }
            }
        }
        if start_time < 0.0 || end_time < 0.0 {
            start_time = timecode_to_seconds(columns[1]);
            end_time = timecode_to_seconds(columns[2]);
        }
        if start_time >= 0.0 && end_time > start_time {
            segments.push(Segment {
                num: columns[0].to_string(),
                start_time,
                end_time,
            });
        }
    }

    segments.sort_by(|left, right| {
        left.start_time
            .partial_cmp(&right.start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    segments
}

fn detect_start_line(lines: &[&str]) -> usize {
    let Some(first) = lines.first() else {
        return 1;
    };
    let first = first.trim();
    if first.is_empty() {
        return 1;
    }
    let columns: Vec<&str> = first.split(',').map(str::trim).collect();
    if columns.len() >= 3 && is_timecode_like(columns[1]) && is_timecode_like(columns[2]) {
        0
    } else {
        1
    }
}

fn is_timecode_like(value: &str) -> bool {
    let value = value.trim();
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() != 3 {
        return false;
    }
    if parts[0].len() != 2 || parts[1].len() != 2 {
        return false;
    }
    if !parts[0].bytes().all(|byte| byte.is_ascii_digit())
        || !parts[1].bytes().all(|byte| byte.is_ascii_digit())
    {
        return false;
    }
    let seconds = parts[2];
    if seconds.len() < 2 || !seconds.as_bytes()[..2].iter().all(u8::is_ascii_digit) {
        return false;
    }
    if seconds.len() == 2 {
        return true;
    }
    let separator = seconds.as_bytes()[2];
    (separator == b'.' || separator == b',')
        && seconds.len() > 3
        && seconds[3..].bytes().all(|byte| byte.is_ascii_digit())
}

fn timecode_to_seconds(timecode: &str) -> f64 {
    let raw = timecode.trim().replace(',', ".");
    let parts: Vec<&str> = raw.split(':').collect();
    if parts.len() != 3 {
        return -1.0;
    }
    let (Ok(hours), Ok(minutes), Ok(seconds)) = (
        parts[0].parse::<f64>(),
        parts[1].parse::<f64>(),
        parts[2].parse::<f64>(),
    ) else {
        return -1.0;
    };
    if hours < 0.0 || minutes < 0.0 || seconds < 0.0 {
        return -1.0;
    }
    hours * 3600.0 + minutes * 60.0 + seconds
}

fn adjust_segments(
    segments: &[Segment],
    frame_rate: f64,
    comp_duration: Option<f64>,
    snap_factor: f64,
) -> (Vec<Value>, Vec<String>) {
    let frame_duration = 1.0 / frame_rate;
    let mut warnings = Vec::new();
    let mut output = Vec::new();

    for (index, segment) in segments.iter().enumerate() {
        let next = segments.get(index + 1);
        let layer_in = segment.start_time;
        let mut layer_out = segment.end_time;

        if let Some(next) = next {
            if next.start_time > 0.0 {
                let gap = next.start_time - layer_out;
                if gap > 0.0 && gap <= frame_duration * snap_factor {
                    layer_out = next.start_time;
                } else {
                    layer_out = layer_out.min(next.start_time);
                }
            }
        }
        if let Some(comp_duration) = comp_duration {
            layer_out = layer_out.min(comp_duration);
        }
        if (layer_out - layer_in) < frame_duration {
            warnings.push(format!("segment_too_short:{}", segment.num));
            continue;
        }

        let mut item = Map::new();
        item.insert("num".into(), Value::String(segment.num.clone()));
        item.insert("startTime".into(), json!(layer_in));
        item.insert("endTime".into(), json!(layer_out));
        item.insert(
            "startTc".into(),
            Value::String(seconds_to_precise_timecode(layer_in)),
        );
        item.insert(
            "endTc".into(),
            Value::String(seconds_to_precise_timecode(layer_out)),
        );
        output.push(Value::Object(item));
    }

    (output, warnings)
}

fn seconds_to_precise_timecode(seconds_value: f64) -> String {
    let sign = if seconds_value < 0.0 { "-" } else { "" };
    let absolute = seconds_value.abs();
    let hours = (absolute / 3600.0).floor() as i64;
    let minutes = ((absolute % 3600.0) / 60.0).floor() as i64;
    let seconds = absolute % 60.0;
    format!("{sign}{hours:02}:{minutes:02}:{seconds:06.3}").replace('.', ",")
}

fn parse_int_str(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok()
}

fn parse_float(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timecode_format_matches_python() {
        assert_eq!(seconds_to_precise_timecode(0.0), "00:00:00,000");
        assert_eq!(seconds_to_precise_timecode(2.12), "00:00:02,120");
        assert_eq!(seconds_to_precise_timecode(3661.5), "01:01:01,500");
        assert_eq!(seconds_to_precise_timecode(-1.25), "-00:00:01,250");
    }

    #[test]
    fn parses_frames_and_adjusts_gap() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("cuts.csv");
        std::fs::write(
            &csv,
            "No,Timecode In,Timecode Out,Frame In,Frame Out\n1,00:00:00.000,00:00:02.120,1,53\n2,00:00:02.160,00:00:04.960,55,124\n",
        )
        .unwrap();
        let manifest = json!({
            "mode": "cuts",
            "csv_path": csv,
            "frame_rate": 25.0,
            "comp_duration": 10.0,
            "config": {"SNAP_FACTOR": 1.1}
        });
        let result = analyze(&manifest).unwrap();
        assert_eq!(result["stats"]["raw_segments"], json!(2));
        assert_eq!(result["stats"]["kept_segments"], json!(2));
        assert_eq!(result["segments"][0]["startTc"], json!("00:00:00,000"));
        assert_eq!(result["segments"][0]["endTc"], json!("00:00:02,160"));
        assert_eq!(result["segments"][1]["startTc"], json!("00:00:02,160"));
        assert_eq!(result["segments"][1]["endTc"], json!("00:00:04,960"));
    }

    #[test]
    fn snaps_tiny_gap_to_next_segment() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("cuts.csv");
        std::fs::write(
            &csv,
            "No,Timecode In,Timecode Out,Frame In,Frame Out\n1,00:00:00.000,00:00:02.000,1,50\n2,00:00:02.040,00:00:04.000,52,100\n",
        )
        .unwrap();
        let manifest = json!({
            "mode": "cuts",
            "csv_path": csv,
            "frame_rate": 25.0,
            "comp_duration": 10.0
        });
        let result = analyze(&manifest).unwrap();
        assert_eq!(result["segments"][0]["endTime"], json!(2.04));
        assert_eq!(result["segments"][0]["endTc"], json!("00:00:02,040"));
    }

    #[test]
    fn drops_too_short_segment() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("cuts.csv");
        std::fs::write(&csv, "1,00:00:00.000,00:00:00.020\n").unwrap();
        let manifest = json!({
            "mode": "cuts",
            "csv_path": csv,
            "frame_rate": 25.0
        });
        let result = analyze(&manifest).unwrap();
        assert_eq!(result["stats"]["raw_segments"], json!(1));
        assert_eq!(result["stats"]["kept_segments"], json!(0));
        assert_eq!(result["warnings"], json!(["segment_too_short:1"]));
    }

    #[test]
    fn headerless_csv_starts_at_first_line() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("cuts.csv");
        std::fs::write(&csv, "1,00:00:00.000,00:00:02.000\n").unwrap();
        let manifest = json!({
            "mode": "cuts",
            "csv_path": csv,
            "frame_rate": 25.0
        });
        let result = analyze(&manifest).unwrap();
        assert_eq!(result["stats"]["raw_segments"], json!(1));
        assert_eq!(result["stats"]["kept_segments"], json!(1));
    }
}
