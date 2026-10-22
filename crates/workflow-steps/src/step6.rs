//! STEP6 : pré-traitement AE.
//!
//! Le pipeline produit `<stem>_ae.json` à partir de :
//! - `<stem>_tracking.json` (sortie STEP5) ;
//! - `<stem>_audio.json` (sortie STEP3/STEP5) ;
//! - `<stem>.csv` (scènes STEP2).
//!
//! Le mode `analyze_manifest` du panneau AE sera porté avec le pont Windows.

use crate::value::{
    as_f64_loose, as_i64_loose, json_number, normalize_str_list, object_speakers, stringify,
    value_truthy,
};
use crate::StepError;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use workflow_schema::round::round_to;

#[derive(Debug, Clone)]
pub struct SegmentConfig {
    pub spread_threshold: f64,
    pub confidence_weight: f64,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            spread_threshold: 200.0,
            confidence_weight: 0.35,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AeOptions {
    pub audio: Option<PathBuf>,
    pub csv: Option<PathBuf>,
    pub config: SegmentConfig,
}

#[derive(Debug, Clone)]
struct TrackingIndex {
    data_by_frame: BTreeMap<i64, Vec<Value>>,
    max_frame: i64,
    fps: Option<f64>,
    meta: Map<String, Value>,
}

#[derive(Debug, Clone)]
struct AudioIndex {
    by_frame: BTreeMap<i64, Value>,
    max_frame: Option<i64>,
}

#[derive(Debug, Clone)]
struct Segment {
    num: String,
    start_frame: i64,
    end_frame: i64,
}

#[derive(Debug, Clone)]
struct SegmentStats {
    id: String,
    is_face: bool,
    presence: u64,
    speech_frames: u64,
    conf_sum: f64,
    area_sum: f64,
    x_values: Vec<f64>,
    tier: u8,
    score: f64,
    avg_area: f64,
}

/// Construit et écrit le payload AE. Retourne `None` si aucun tableau de
/// frames exploitable n'est trouvé.
pub fn build_ae_payload_file(
    tracking_path: &Path,
    output: &Path,
    options: &AeOptions,
) -> Result<Option<Value>, StepError> {
    let tracking = read_json(tracking_path)?;
    let audio_path = options
        .audio
        .clone()
        .or_else(|| default_audio_path(tracking_path));
    let audio = match audio_path {
        Some(path) if path.exists() => Some(read_json(&path)?),
        _ => None,
    };
    let csv_path = options
        .csv
        .clone()
        .or_else(|| default_csv_path(tracking_path));
    let csv = match csv_path {
        Some(path) if path.exists() => Some(path),
        _ => None,
    };

    let payload = build_ae_payload(&tracking, audio.as_ref(), csv.as_deref(), &options.config)?;
    if let Some(payload) = &payload {
        write_json_atomically(output, payload)?;
    }
    Ok(payload)
}

pub fn build_ae_payload(
    tracking: &Value,
    audio: Option<&Value>,
    csv_path: Option<&Path>,
    config: &SegmentConfig,
) -> Result<Option<Value>, StepError> {
    let Some(index) = parse_tracking(tracking)? else {
        return Ok(None);
    };
    let audio_index = match audio {
        Some(audio) => parse_audio(audio)?,
        None => AudioIndex {
            by_frame: BTreeMap::new(),
            max_frame: None,
        },
    };

    let mut data_by_frame = index.data_by_frame;
    enrich_frames(&mut data_by_frame, &audio_index.by_frame);

    let segments = csv_path
        .map(|path| parse_csv_segments(path, index.fps.filter(|fps| *fps != 0.0).unwrap_or(25.0)))
        .unwrap_or_default();
    let recommendations = if segments.is_empty() {
        None
    } else {
        Some(compute_segment_recommendations(
            &data_by_frame,
            &audio_index.by_frame,
            &segments,
            config,
        ))
    };

    let audio_max_frame = audio_index.max_frame;
    let mut output = Map::new();
    output.insert("schema".into(), Value::String("ae_preprocessed".into()));
    output.insert("schema_version".into(), Value::from(1));
    output.insert(
        "dataByFrame".into(),
        Value::Object(data_by_frame_to_json(data_by_frame)),
    );
    output.insert("maxFrame".into(), Value::from(index.max_frame));
    output.insert(
        "audioByFrame".into(),
        Value::Object(audio_by_frame_to_json(audio_index.by_frame)),
    );
    output.insert(
        "audioMaxFrame".into(),
        audio_max_frame.map_or(Value::Null, Value::from),
    );
    if let Some(recommendations) = recommendations {
        output.insert(
            "segment_recommendations".into(),
            Value::Object(recommendations),
        );
    }
    for key in [
        "fps",
        "total_frames",
        "tracking_analytics",
        "expression_summary",
        "temporal_alignment",
        "speaker_embeddings",
    ] {
        if let Some(value) = index.meta.get(key) {
            output.insert(key.into(), value.clone());
        }
    }

    Ok(Some(Value::Object(output)))
}

fn parse_tracking(root: &Value) -> Result<Option<TrackingIndex>, StepError> {
    let Some(object) = root.as_object() else {
        return Ok(None);
    };

    if object.get("schema").and_then(Value::as_str) == Some("ae_preprocessed") {
        return parse_preprocessed_tracking(object);
    }

    let frames = object
        .get("frames_analysis")
        .or_else(|| object.get("frames"))
        .and_then(Value::as_array);
    let Some(frames) = frames else {
        return Ok(None);
    };

    let mut data_by_frame = BTreeMap::new();
    let mut max_frame = 0_i64;
    for frame in frames {
        let Some(frame) = frame.as_object() else {
            continue;
        };
        let Some(frame_number) = frame.get("frame").and_then(as_i64_loose) else {
            continue;
        };
        max_frame = max_frame.max(frame_number);

        let Some(tracked) = frame.get("tracked_objects").and_then(Value::as_array) else {
            continue;
        };
        let mut output_objects = Vec::new();
        for object in tracked {
            if let Some(object) = process_tracking_object(object) {
                output_objects.push(object);
            }
        }
        if !output_objects.is_empty() {
            data_by_frame.insert(frame_number, output_objects);
        }
    }

    let fps = object.get("fps").and_then(as_f64_loose);
    let mut meta = Map::new();
    if let Some(fps) = fps {
        meta.insert("fps".into(), json_number(fps));
    }
    if let Some(total) = object.get("total_frames").and_then(Value::as_i64) {
        meta.insert("total_frames".into(), Value::from(total));
    }
    for key in [
        "tracking_analytics",
        "expression_summary",
        "temporal_alignment",
        "speaker_embeddings",
    ] {
        if let Some(value) = object.get(key).filter(|value| !value.is_null()) {
            meta.insert(key.into(), value.clone());
        }
    }

    Ok(Some(TrackingIndex {
        data_by_frame,
        max_frame,
        fps,
        meta,
    }))
}

fn parse_preprocessed_tracking(
    object: &Map<String, Value>,
) -> Result<Option<TrackingIndex>, StepError> {
    let Some(data) = object.get("dataByFrame").and_then(Value::as_object) else {
        return Ok(None);
    };
    let mut data_by_frame = BTreeMap::new();
    for (key, value) in data {
        if let (Some(frame), Some(objects)) = (key.parse::<i64>().ok(), value.as_array()) {
            if !objects.is_empty() {
                data_by_frame.insert(frame, objects.clone());
            }
        }
    }
    let mut meta = Map::new();
    for key in [
        "fps",
        "total_frames",
        "tracking_analytics",
        "expression_summary",
        "temporal_alignment",
        "speaker_embeddings",
    ] {
        if let Some(value) = object.get(key).filter(|value| !value.is_null()) {
            meta.insert(key.into(), value.clone());
        }
    }
    Ok(Some(TrackingIndex {
        data_by_frame,
        max_frame: object.get("maxFrame").and_then(as_i64_loose).unwrap_or(0),
        fps: object.get("fps").and_then(as_f64_loose),
        meta,
    }))
}

fn process_tracking_object(object: &Value) -> Option<Value> {
    let object = object.as_object()?;
    let source = object.get("source")?;
    let label = object.get("label");
    let source_str = source.as_str();
    let label_str = label.and_then(Value::as_str);
    let eligible = source_str == Some("face_landmarker")
        || (source_str == Some("object_detector") && label_str == Some("person"));
    if !eligible {
        return None;
    }

    let id = object.get("id").filter(|value| value_truthy(value))?;
    let centroid_x = match object.get("centroid_x") {
        None => 0.0,
        Some(value) if value.is_null() => return None,
        Some(value) => as_f64_loose(value)?,
    };
    let bbox_width = numeric_or_zero(object.get("bbox_width"));
    let bbox_height = numeric_or_zero(object.get("bbox_height"));
    let bbox_surface = bbox_width.max(0.0) * bbox_height.max(0.0);
    let confidence = numeric_or_zero(object.get("confidence")).clamp(0.0, 1.0);
    let speakers = object_speakers(object);
    let jaw_value = if object.contains_key("jawOpen") {
        object.get("jawOpen")
    } else {
        object.get("jaw_open")
    };
    let jaw_open = jaw_value.and_then(|value| {
        if value.is_null() {
            None
        } else {
            as_f64_loose(value)
        }
    });
    let speaker_label = object
        .get("speaker_label")
        .filter(|value| value_truthy(value))
        .map(stringify);

    let mut tracked = Map::new();
    tracked.insert("id".into(), id.clone());
    tracked.insert("centroid_x".into(), json_number(centroid_x));
    tracked.insert("source".into(), source.clone());
    tracked.insert("label".into(), label.cloned().unwrap_or(Value::Null));
    tracked.insert(
        "video_speakers".into(),
        Value::Array(speakers.into_iter().map(Value::String).collect()),
    );
    tracked.insert("bbox_width".into(), json_number(bbox_width));
    tracked.insert("bbox_height".into(), json_number(bbox_height));
    tracked.insert("bbox_surface".into(), json_number(bbox_surface));
    tracked.insert("confidence".into(), json_number(confidence));
    if let Some(speaker_label) = speaker_label {
        tracked.insert("speaker_label".into(), Value::String(speaker_label));
    }
    if let Some(jaw_open) = jaw_open {
        tracked.insert("jawOpen".into(), json_number(jaw_open));
    }
    if let Some(value) = object.get("is_speaking") {
        tracked.insert("is_speaking".into(), Value::Bool(value_truthy(value)));
    }
    if let Some(value) = object.get("tier") {
        if let Some(tier) = as_i64_loose(value) {
            tracked.insert("tier".into(), Value::from(tier));
        }
    }

    Some(Value::Object(tracked))
}

fn numeric_or_zero(value: Option<&Value>) -> f64 {
    value
        .filter(|value| value_truthy(value))
        .and_then(as_f64_loose)
        .unwrap_or(0.0)
}

fn parse_audio(root: &Value) -> Result<AudioIndex, StepError> {
    let Some(object) = root.as_object() else {
        return Ok(AudioIndex {
            by_frame: BTreeMap::new(),
            max_frame: None,
        });
    };
    let frames = object
        .get("frames_analysis")
        .or_else(|| object.get("frames"))
        .and_then(Value::as_array);
    let Some(frames) = frames else {
        return Ok(AudioIndex {
            by_frame: BTreeMap::new(),
            max_frame: None,
        });
    };

    let mut by_frame = BTreeMap::new();
    let mut max_frame = None;
    for frame in frames {
        let Some(frame) = frame.as_object() else {
            continue;
        };
        let Some(frame_number) = frame.get("frame").and_then(as_i64_loose) else {
            continue;
        };
        let Some(audio_info) = frame.get("audio_info").and_then(Value::as_object) else {
            continue;
        };
        by_frame.insert(frame_number, Value::Object(audio_info.clone()));
        max_frame = Some(max_frame.map_or(frame_number, |current: i64| current.max(frame_number)));
    }
    Ok(AudioIndex {
        by_frame,
        max_frame,
    })
}

fn enrich_frames(
    data_by_frame: &mut BTreeMap<i64, Vec<Value>>,
    audio_by_frame: &BTreeMap<i64, Value>,
) {
    for (frame_number, objects) in data_by_frame.iter_mut() {
        let audio_info = audio_by_frame.get(frame_number).and_then(Value::as_object);
        let is_speech = audio_info
            .and_then(|audio| audio.get("is_speech_present"))
            .is_some_and(value_truthy);
        let active_speakers = audio_info.map_or_else(Vec::new, |audio| {
            let selected = audio
                .get("active_speaker_labels")
                .filter(|value| value_truthy(value))
                .or_else(|| {
                    audio
                        .get("active_speakers")
                        .filter(|value| value_truthy(value))
                })
                .or_else(|| {
                    audio
                        .get("video_speakers")
                        .filter(|value| value_truthy(value))
                });
            normalize_str_list(selected)
        });
        let object_count = objects.len();

        for object in objects.iter_mut() {
            let Some(object) = object.as_object_mut() else {
                continue;
            };
            let is_face = object.get("source").and_then(Value::as_str) == Some("face_landmarker")
                || object.get("label").and_then(Value::as_str) == Some("face");

            let mut is_speaking = false;
            if is_speech {
                if object.get("is_speaking") == Some(&Value::Bool(true)) {
                    is_speaking = true;
                } else {
                    let mut object_speakers = object_speakers(object);
                    if let Some(label) = object
                        .get("speaker_label")
                        .filter(|value| value_truthy(value))
                    {
                        object_speakers.push(stringify(label));
                    }
                    let speakers_match = !active_speakers.is_empty()
                        && !object_speakers.is_empty()
                        && active_speakers
                            .iter()
                            .any(|speaker| object_speakers.contains(speaker));
                    let jaw_speaks = is_face
                        && object
                            .get("jawOpen")
                            .and_then(as_f64_loose)
                            .is_some_and(|jaw| jaw > 0.12);
                    if speakers_match || object_count == 1 || jaw_speaks {
                        is_speaking = true;
                    }
                }
            }

            let tier = if is_speaking && is_face {
                4
            } else if is_speaking {
                3
            } else if is_face {
                2
            } else {
                1
            };
            object.insert("is_speaking".into(), Value::Bool(is_speaking));
            object.insert("tier".into(), Value::from(tier));
        }
    }
}

fn data_by_frame_to_json(data_by_frame: BTreeMap<i64, Vec<Value>>) -> Map<String, Value> {
    let mut map = Map::new();
    for (frame, objects) in data_by_frame {
        map.insert(frame.to_string(), Value::Array(objects));
    }
    map
}

fn audio_by_frame_to_json(audio_by_frame: BTreeMap<i64, Value>) -> Map<String, Value> {
    let mut map = Map::new();
    for (frame, audio) in audio_by_frame {
        map.insert(frame.to_string(), audio);
    }
    map
}

fn parse_csv_segments(csv_path: &Path, fps: f64) -> Vec<Segment> {
    let Ok(text) = fs::read_to_string(csv_path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let header: Vec<&str> = lines[0].split(',').map(str::trim).collect();
    let has_frame_columns = header.contains(&"Frame In") && header.contains(&"Frame Out");
    let frame_in_index = header.iter().position(|value| *value == "Frame In");
    let frame_out_index = header.iter().position(|value| *value == "Frame Out");
    let number_index = header.iter().position(|value| *value == "No").unwrap_or(0);
    let tc_in_index = header
        .iter()
        .position(|value| *value == "Timecode In")
        .unwrap_or(1);
    let tc_out_index = header
        .iter()
        .position(|value| *value == "Timecode Out")
        .unwrap_or(2);

    let mut segments = Vec::new();
    for line in lines.iter().skip(1) {
        let parts: Vec<&str> = line.split(',').map(str::trim).collect();
        if parts.len() < 3 {
            continue;
        }
        let number = parts.get(number_index).copied().unwrap_or("").to_string();

        let mut frame_in = None;
        let mut frame_out = None;
        if has_frame_columns {
            if let (Some(in_index), Some(out_index)) = (frame_in_index, frame_out_index) {
                if let (Some(in_value), Some(out_value)) =
                    (parts.get(in_index), parts.get(out_index))
                {
                    if let (Ok(in_frame), Ok(out_frame)) =
                        (in_value.parse::<i64>(), out_value.parse::<i64>())
                    {
                        frame_in = Some(in_frame);
                        frame_out = Some(out_frame);
                    }
                }
            }
        }

        if frame_in.is_none() || frame_out.is_none() {
            let tc_in = parts.get(tc_in_index).copied().unwrap_or("00:00:00.000");
            let tc_out = parts.get(tc_out_index).copied().unwrap_or("00:00:00.000");
            frame_in = Some(round_to(tc_to_seconds(tc_in) * fps, 0) as i64);
            frame_out = Some(round_to(tc_to_seconds(tc_out) * fps, 0) as i64);
        }

        if let (Some(start_frame), Some(end_frame)) = (frame_in, frame_out) {
            if end_frame >= start_frame {
                segments.push(Segment {
                    num: number,
                    start_frame,
                    end_frame,
                });
            }
        }
    }
    segments
}

fn tc_to_seconds(value: &str) -> f64 {
    let value = value.replace(',', ".");
    let parts: Vec<&str> = value.split(':').collect();
    match parts.len() {
        3 => match (
            parts[0].parse::<f64>(),
            parts[1].parse::<f64>(),
            parts[2].parse::<f64>(),
        ) {
            (Ok(hours), Ok(minutes), Ok(seconds)) => hours * 3600.0 + minutes * 60.0 + seconds,
            _ => 0.0,
        },
        2 => match (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
            (Ok(minutes), Ok(seconds)) => minutes * 60.0 + seconds,
            _ => 0.0,
        },
        _ => value.parse::<f64>().unwrap_or(0.0),
    }
}

fn compute_segment_recommendations(
    data_by_frame: &BTreeMap<i64, Vec<Value>>,
    audio_by_frame: &BTreeMap<i64, Value>,
    segments: &[Segment],
    config: &SegmentConfig,
) -> Map<String, Value> {
    let mut recommendations = Map::new();

    for segment in segments {
        let total_frames = (segment.end_frame - segment.start_frame + 1).max(1) as f64;
        let mut stats: Vec<SegmentStats> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();

        for frame in segment.start_frame..=segment.end_frame {
            let frame_objects = data_by_frame.get(&frame).map(Vec::as_slice).unwrap_or(&[]);
            let speech_active = audio_by_frame
                .get(&frame)
                .and_then(Value::as_object)
                .and_then(|audio| audio.get("is_speech_present"))
                .is_some_and(value_truthy);
            let object_count = frame_objects.len();

            for object in frame_objects {
                let Some(object) = object.as_object() else {
                    continue;
                };
                let id = object.get("id").map(stringify).unwrap_or_default();
                if id.is_empty() {
                    continue;
                }
                let is_face = object.get("source").and_then(Value::as_str)
                    == Some("face_landmarker")
                    || object.get("label").and_then(Value::as_str) == Some("face");

                let stats_index = if let Some(index) = index.get(&id).copied() {
                    index
                } else {
                    let stats_index = stats.len();
                    stats.push(SegmentStats {
                        id: id.clone(),
                        is_face,
                        presence: 0,
                        speech_frames: 0,
                        conf_sum: 0.0,
                        area_sum: 0.0,
                        x_values: Vec::new(),
                        tier: 0,
                        score: 0.0,
                        avg_area: 0.0,
                    });
                    index.insert(id.clone(), stats_index);
                    stats_index
                };
                let candidate = &mut stats[stats_index];
                if is_face {
                    candidate.is_face = true;
                }
                candidate.presence += 1;

                let confidence = object
                    .get("confidence")
                    .filter(|value| value_truthy(value))
                    .and_then(as_f64_loose)
                    .unwrap_or(0.5);
                candidate.conf_sum += confidence;

                let mut surface = object
                    .get("bbox_surface")
                    .filter(|value| value_truthy(value))
                    .and_then(as_f64_loose)
                    .unwrap_or(0.0);
                if surface <= 0.0 {
                    let width = object
                        .get("bbox_width")
                        .filter(|value| value_truthy(value))
                        .or_else(|| object.get("width").filter(|value| value_truthy(value)))
                        .and_then(as_f64_loose)
                        .unwrap_or(0.0);
                    let height = object
                        .get("bbox_height")
                        .filter(|value| value_truthy(value))
                        .or_else(|| object.get("height").filter(|value| value_truthy(value)))
                        .and_then(as_f64_loose)
                        .unwrap_or(0.0);
                    surface = width.max(0.0) * height.max(0.0);
                }
                candidate.area_sum += surface;

                let centroid = object.get("centroid_x").or_else(|| object.get("center_x"));
                if let Some(centroid) = centroid {
                    if let Some(centroid) = as_f64_loose(centroid) {
                        candidate.x_values.push(centroid);
                    }
                }

                if object.get("is_speaking").is_some_and(value_truthy)
                    || (speech_active && object_count == 1)
                {
                    candidate.speech_frames += 1;
                }
            }
        }

        let mut best: Option<usize> = None;
        for index in 0..stats.len() {
            let presence = stats[index].presence;
            if presence == 0 || stats[index].x_values.is_empty() {
                continue;
            }
            if presence < 3 && (presence as f64 / total_frames) < 0.10 {
                continue;
            }

            let (tier, score, avg_area) = {
                let candidate = &stats[index];
                let presence_f = candidate.presence as f64;
                let avg_confidence = candidate.conf_sum / presence_f;
                let speech_ratio = candidate.speech_frames as f64 / presence_f;
                let has_audio = candidate.speech_frames >= 3 || speech_ratio >= 0.20;
                let tier = if has_audio && candidate.is_face {
                    4
                } else if has_audio {
                    3
                } else if candidate.is_face {
                    2
                } else {
                    1
                };
                let score = presence_f
                    * (1.0 + avg_confidence * config.confidence_weight + speech_ratio * 0.50);
                let avg_area = candidate.area_sum / presence_f;
                (tier, score, avg_area)
            };
            {
                let candidate = &mut stats[index];
                candidate.tier = tier;
                candidate.score = score;
                candidate.avg_area = avg_area;
            }

            match best {
                None => best = Some(index),
                Some(best_index) => {
                    let current = &stats[best_index];
                    let better = tier > current.tier
                        || (tier == current.tier
                            && (score > current.score
                                || (is_close(score, current.score, 1e-4)
                                    && avg_area > current.avg_area)));
                    if better {
                        best = Some(index);
                    }
                }
            }
        }

        let recommendation = if let Some(best_index) = best {
            let best = &stats[best_index];
            let (_, _, spread, average) = trimmed_stats(&best.x_values);
            let spread_x = round_to(spread, 2);
            let needs_review = spread_x > config.spread_threshold;
            let mut output = Map::new();
            output.insert(
                "recommended_object_id".into(),
                Value::String(best.id.clone()),
            );
            output.insert("tier".into(), Value::from(u64::from(best.tier)));
            output.insert("target_anchor_x".into(), json_number(round_to(average, 2)));
            output.insert("spread_x".into(), json_number(spread_x));
            output.insert("needs_review".into(), Value::Bool(needs_review));
            output.insert("start_frame".into(), Value::from(segment.start_frame));
            output.insert("end_frame".into(), Value::from(segment.end_frame));
            Value::Object(output)
        } else {
            let mut output = Map::new();
            output.insert("recommended_object_id".into(), Value::Null);
            output.insert("tier".into(), Value::from(0));
            output.insert("target_anchor_x".into(), Value::Null);
            output.insert("spread_x".into(), json_number(0.0));
            output.insert("needs_review".into(), Value::Bool(true));
            output.insert("start_frame".into(), Value::from(segment.start_frame));
            output.insert("end_frame".into(), Value::from(segment.end_frame));
            Value::Object(output)
        };
        recommendations.insert(segment.num.clone(), recommendation);
    }

    recommendations
}

fn is_close(left: f64, right: f64, abs_tolerance: f64) -> bool {
    (left - right).abs() <= (1e-9 * left.abs().max(right.abs())).max(abs_tolerance)
}

fn trimmed_stats(values: &[f64]) -> (f64, f64, f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let mut minimum = values[0];
    let mut maximum = values[0];
    for value in values {
        minimum = minimum.min(*value);
        maximum = maximum.max(*value);
    }
    let spread = maximum - minimum;
    let count = values.len();
    let average = if count <= 10 {
        values.iter().sum::<f64>() / count as f64
    } else {
        let mut sorted = values.to_vec();
        sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
        let k = ((count as f64 * 0.10).floor() as usize).max(1);
        let trimmed = &sorted[k..count - k];
        if trimmed.is_empty() {
            values.iter().sum::<f64>() / count as f64
        } else {
            trimmed.iter().sum::<f64>() / trimmed.len() as f64
        }
    };
    (minimum, maximum, spread, average)
}

fn read_json(path: &Path) -> Result<Value, StepError> {
    let bytes = fs::read(path).map_err(|source| StepError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_json_atomically(path: &Path, payload: &Value) -> Result<(), StepError> {
    let temp_path = path.with_extension("tmp");
    let result = (|| -> Result<(), StepError> {
        let file = File::create(&temp_path).map_err(|source| StepError::Write {
            path: temp_path.clone(),
            source,
        })?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, payload)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        fs::rename(&temp_path, path).map_err(|source| StepError::Write {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn default_audio_path(tracking: &Path) -> Option<PathBuf> {
    let name = tracking.file_name()?.to_str()?;
    let lower = name.to_ascii_lowercase();
    let stem = if lower.ends_with("_ae.json") {
        &name[..name.len() - "_ae.json".len()]
    } else if lower.ends_with("_tracking.json") {
        &name[..name.len() - "_tracking.json".len()]
    } else if lower.ends_with(".json") {
        &name[..name.len() - ".json".len()]
    } else {
        name
    };
    Some(tracking.with_file_name(format!("{stem}_audio.json")))
}

fn default_csv_path(tracking: &Path) -> Option<PathBuf> {
    let stem = tracking.file_stem()?.to_str()?;
    let lower = stem.to_ascii_lowercase();
    let stem = if lower.ends_with("_tracking") {
        &stem[..stem.len() - "_tracking".len()]
    } else if lower.ends_with("_ae") {
        &stem[..stem.len() - "_ae".len()]
    } else {
        stem
    };
    Some(tracking.with_file_name(format!("{stem}.csv")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_csv_with_frame_columns() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("scenes.csv");
        std::fs::write(
            &csv,
            "No,Timecode In,Timecode Out,Frame In,Frame Out\n1,00:00:00.000,00:00:02.120,1,54\n2,00:00:02.160,00:00:04.960,55,125\n",
        )
        .unwrap();
        let segments = parse_csv_segments(&csv, 25.0);
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].num, "1");
        assert_eq!(segments[0].start_frame, 1);
        assert_eq!(segments[0].end_frame, 54);
        assert_eq!(segments[1].start_frame, 55);
        assert_eq!(segments[1].end_frame, 125);
    }

    #[test]
    fn parses_csv_with_timecodes_only() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("scenes.csv");
        std::fs::write(
            &csv,
            "No,Timecode In,Timecode Out\n1,00:00:01.000,00:00:02.000\n",
        )
        .unwrap();
        let segments = parse_csv_segments(&csv, 25.0);
        assert_eq!(segments[0].start_frame, 25);
        assert_eq!(segments[0].end_frame, 50);
    }

    #[test]
    fn trimmed_stats_matches_python_shape() {
        let values: Vec<f64> = (0..20).map(|value| value as f64).collect();
        let (minimum, maximum, spread, average) = trimmed_stats(&values);
        assert_eq!(minimum, 0.0);
        assert_eq!(maximum, 19.0);
        assert_eq!(spread, 19.0);
        assert!((average - 9.5).abs() < 1e-12);
    }

    #[test]
    fn trimmed_stats_keeps_small_series_average() {
        let (minimum, maximum, spread, average) = trimmed_stats(&[1.0, 2.0, 3.0]);
        assert_eq!(minimum, 1.0);
        assert_eq!(maximum, 3.0);
        assert_eq!(spread, 2.0);
        assert_eq!(average, 2.0);
    }

    #[test]
    fn builds_payload_and_enriches_speaking() {
        let tracking = json!({
            "frames_analysis": [
                {"frame": 1, "tracked_objects": [
                    {"id": "obj_1", "centroid_x": 10, "source": "face_landmarker",
                     "label": "face", "confidence": 0.9, "bbox_width": 2,
                     "bbox_height": 3, "active_speakers": ["A"]}
                ]}
            ],
            "total_frames": 1,
            "tracking_analytics": {"schema_version": 1}
        });
        let audio = json!({
            "frames_analysis": [
                {"frame": 1, "audio_info": {"is_speech_present": true,
                  "active_speaker_labels": ["A"], "timecode_sec": 0.0}}
            ]
        });
        let payload = build_ae_payload(&tracking, Some(&audio), None, &SegmentConfig::default())
            .unwrap()
            .unwrap();
        assert_eq!(payload["schema"], json!("ae_preprocessed"));
        assert_eq!(payload["maxFrame"], json!(1));
        assert_eq!(payload["audioMaxFrame"], json!(1));
        assert_eq!(
            payload["dataByFrame"]["1"][0]["video_speakers"],
            json!(["A"])
        );
        assert_eq!(payload["dataByFrame"]["1"][0]["is_speaking"], json!(true));
        assert_eq!(payload["dataByFrame"]["1"][0]["tier"], json!(4));
        assert_eq!(payload["tracking_analytics"]["schema_version"], json!(1));
    }

    #[test]
    fn segment_recommendations_emit_empty_when_no_candidate() {
        let data = BTreeMap::new();
        let audio = BTreeMap::new();
        let segments = vec![Segment {
            num: "1".into(),
            start_frame: 1,
            end_frame: 10,
        }];
        let recommendations =
            compute_segment_recommendations(&data, &audio, &segments, &SegmentConfig::default());
        assert_eq!(recommendations["1"]["recommended_object_id"], Value::Null);
        assert_eq!(recommendations["1"]["tier"], json!(0));
        assert_eq!(recommendations["1"]["target_anchor_x"], Value::Null);
        assert_eq!(recommendations["1"]["needs_review"], json!(true));
    }

    #[test]
    fn segment_recommendations_select_highest_tier() {
        let data: BTreeMap<i64, Vec<Value>> = BTreeMap::from([
            (
                1,
                vec![json!({
                    "id": "face", "source": "face_landmarker", "label": "face",
                    "centroid_x": 100.0, "bbox_surface": 10.0,
                    "confidence": 0.9, "is_speaking": true
                })],
            ),
            (
                2,
                vec![json!({
                    "id": "person", "source": "object_detector", "label": "person",
                    "centroid_x": 200.0, "bbox_surface": 20.0,
                    "confidence": 0.8, "is_speaking": false
                })],
            ),
        ]);
        let audio = BTreeMap::from([(
            1,
            json!({"is_speech_present": true, "active_speaker_labels": ["A"]}),
        )]);
        let segments = vec![Segment {
            num: "1".into(),
            start_frame: 1,
            end_frame: 2,
        }];
        let recommendations =
            compute_segment_recommendations(&data, &audio, &segments, &SegmentConfig::default());
        assert_eq!(recommendations["1"]["recommended_object_id"], json!("face"));
        assert_eq!(recommendations["1"]["tier"], json!(4));
    }
}
