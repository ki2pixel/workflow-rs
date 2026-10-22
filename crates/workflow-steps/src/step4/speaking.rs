use std::collections::HashMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};

#[derive(Debug, Clone)]
pub struct Segment {
    pub segment_id: i64,
    pub frame_start: i64,
    pub frame_end: i64,
}

#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub is_speech_present: bool,
    pub num_speakers: i64,
    pub active_speakers: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SpeakingResult {
    pub is_speaking: bool,
    pub confidence: f64,
    pub method: String,
    pub sources: Value,
}

#[derive(Debug, Clone)]
pub struct SpeakingDetector {
    pub segments: Vec<Segment>,
    pub audio: Option<HashMap<u64, AudioInfo>>,
    pub jaw_threshold: f64,
}

impl SpeakingDetector {
    pub fn load(video_path: &Path, jaw_threshold: f64, allow_partial: bool) -> Result<Self> {
        let segments = match load_segments(video_path) {
            Ok(segments) => segments,
            Err(_) if allow_partial => Vec::new(),
            Err(error) => return Err(error),
        };
        let audio = match load_audio(video_path) {
            Ok(audio) => audio,
            Err(_) if allow_partial => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            segments,
            audio,
            jaw_threshold,
        })
    }

    pub fn detect(
        &self,
        frame_num: u64,
        blendshapes: Option<&[(String, f64)]>,
        source_detector: &str,
    ) -> SpeakingResult {
        let mut sources = Map::new();
        let mut confidence_scores = Vec::new();

        let mut audio_confidence = 0.0;
        if let Some(audio) = &self.audio {
            if let Some(info) = audio.get(&frame_num) {
                if info.is_speech_present {
                    audio_confidence = (0.8 + info.num_speakers as f64 * 0.1).min(1.0);
                }
                let mut audio_source = Map::new();
                audio_source.insert(
                    "is_speech_present".into(),
                    Value::Bool(info.is_speech_present),
                );
                audio_source.insert("num_speakers".into(), Value::from(info.num_speakers));
                audio_source.insert(
                    "active_speakers".into(),
                    Value::Array(
                        info.active_speakers
                            .iter()
                            .map(|speaker| Value::String(speaker.clone()))
                            .collect(),
                    ),
                );
                audio_source.insert("confidence".into(), Value::from(audio_confidence));
                sources.insert("audio".into(), Value::Object(audio_source));
                confidence_scores.push(audio_confidence * 0.6);
            }
        }

        let mut visual_confidence = 0.0;
        if source_detector == "face_landmarker" {
            if let Some(values) = blendshapes.filter(|values| !values.is_empty()) {
                if let Some(jaw_open) = lookup(values, "jawOpen") {
                    let mut mouth_activity = 0.0;
                    if jaw_open > self.jaw_threshold {
                        visual_confidence = (jaw_open / self.jaw_threshold * 0.7).min(1.0);
                        let mouth_open = lookup(values, "mouthOpen").unwrap_or(0.0);
                        let mouth_shrug_upper = lookup(values, "mouthShrugUpper").unwrap_or(0.0);
                        let mouth_shrug_lower = lookup(values, "mouthShrugLower").unwrap_or(0.0);
                        mouth_activity = (mouth_open + mouth_shrug_upper + mouth_shrug_lower) / 3.0;
                        visual_confidence = (visual_confidence + mouth_activity * 0.2).min(1.0);
                    }
                    let mut visual = Map::new();
                    visual.insert("jaw_open".into(), Value::from(jaw_open));
                    visual.insert("jaw_threshold".into(), Value::from(self.jaw_threshold));
                    visual.insert("mouth_activity".into(), Value::from(mouth_activity));
                    visual.insert("confidence".into(), Value::from(visual_confidence));
                    sources.insert("visual".into(), Value::Object(visual));
                    confidence_scores.push(visual_confidence * 0.4);
                }
            }
        }

        if let Some(segment) = self.segment_for_frame(frame_num as i64) {
            let segment_duration = segment.frame_end - segment.frame_start;
            let frame_position = if segment_duration > 0 {
                (frame_num as f64 - segment.frame_start as f64) / segment_duration as f64
            } else {
                0.0
            };
            let mut segment_source = Map::new();
            segment_source.insert("segment_id".into(), Value::from(segment.segment_id));
            segment_source.insert("frame_position".into(), Value::from(frame_position));
            segment_source.insert("segment_duration".into(), Value::from(segment_duration));
            segment_source.insert(
                "is_zero_duration".into(),
                Value::Bool(segment_duration == 0),
            );
            sources.insert("segment".into(), Value::Object(segment_source));
        }

        let final_confidence: f64 = confidence_scores.iter().sum();
        let is_speaking = final_confidence > 0.3;
        let method = if audio_confidence > visual_confidence {
            "audio_primary"
        } else if visual_confidence > 0.0 {
            "visual_primary"
        } else if source_detector == "object_detector" {
            "object_detection_fallback"
        } else {
            "no_detection"
        };

        SpeakingResult {
            is_speaking,
            confidence: final_confidence,
            method: method.to_string(),
            sources: Value::Object(sources),
        }
    }

    fn segment_for_frame(&self, frame_num: i64) -> Option<&Segment> {
        self.segments
            .iter()
            .find(|segment| segment.frame_start <= frame_num && frame_num <= segment.frame_end)
    }
}

fn lookup(values: &[(String, f64)], key: &str) -> Option<f64> {
    values
        .iter()
        .find_map(|(name, value)| (name == key).then_some(*value))
}

fn load_segments(video_path: &Path) -> Result<Vec<Segment>> {
    let csv_path = video_path.with_extension("csv");
    let content = std::fs::read_to_string(&csv_path)
        .with_context(|| format!("lecture CSV {}", csv_path.display()))?;
    let mut lines = content.lines();
    let header = match lines.next() {
        Some(header) => split_csv_line(header),
        None => bail!("CSV de scènes vide: {}", csv_path.display()),
    };
    let column = |name: &str| header.iter().position(|value| value == name);
    let (Some(no), Some(frame_in), Some(frame_out)) =
        (column("No"), column("Frame In"), column("Frame Out"))
    else {
        bail!(
            "colonnes No/Frame In/Frame Out absentes du CSV {}",
            csv_path.display()
        );
    };

    let mut segments = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let values = split_csv_line(line);
        let parse = |index: usize| -> Option<i64> {
            values
                .get(index)
                .and_then(|value| value.trim().parse::<i64>().ok())
        };
        if let (Some(segment_id), Some(frame_start), Some(frame_end)) =
            (parse(no), parse(frame_in), parse(frame_out))
        {
            segments.push(Segment {
                segment_id,
                frame_start,
                frame_end,
            });
        }
    }
    Ok(segments)
}

fn split_csv_line(line: &str) -> Vec<String> {
    // Les CSV de production n'utilisent pas de virgules dans les champs
    // nécessaires ; on conserve néanmoins les guillemets simples.
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

fn load_audio(video_path: &Path) -> Result<Option<HashMap<u64, AudioInfo>>> {
    let stem = video_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let audio_path = video_path.with_file_name(format!("{stem}_audio.json"));
    if !audio_path.exists() {
        bail!("JSON audio introuvable: {}", audio_path.display());
    }
    let data = std::fs::read(&audio_path)
        .with_context(|| format!("lecture audio {}", audio_path.display()))?;
    let payload: Value = serde_json::from_slice(&data)
        .with_context(|| format!("JSON audio invalide {}", audio_path.display()))?;
    let frames = payload
        .get("frames_analysis")
        .and_then(Value::as_array)
        .cloned()
        .with_context(|| {
            format!(
                "tableau frames_analysis absent du JSON audio {}",
                audio_path.display()
            )
        })?;

    let mut audio = HashMap::new();
    for frame in frames {
        let Some(frame_num) = frame.get("frame").and_then(Value::as_u64) else {
            continue;
        };
        let Some(info) = frame.get("audio_info").and_then(Value::as_object) else {
            continue;
        };
        let is_speech_present = info
            .get("is_speech_present")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let active_speakers = info
            .get("active_speaker_labels")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let num_speakers = info
            .get("num_distinct_speakers_audio")
            .or_else(|| info.get("num_speakers"))
            .and_then(Value::as_i64)
            .unwrap_or(active_speakers.len() as i64);
        audio.insert(
            frame_num,
            AudioInfo {
                is_speech_present,
                num_speakers,
                active_speakers,
            },
        );
    }
    Ok(Some(audio))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector(
        segments: Vec<Segment>,
        audio: Option<HashMap<u64, AudioInfo>>,
    ) -> SpeakingDetector {
        SpeakingDetector {
            segments,
            audio,
            jaw_threshold: 0.08,
        }
    }

    #[test]
    fn visual_confidence_is_weighted() {
        let detector = detector(Vec::new(), None);
        let values = vec![
            ("jawOpen".to_string(), 0.1),
            ("mouthOpen".to_string(), 0.0),
            ("mouthShrugUpper".to_string(), 0.0),
            ("mouthShrugLower".to_string(), 0.0),
        ];
        let result = detector.detect(1, Some(&values), "face_landmarker");
        assert!((result.confidence - 0.35).abs() < 1e-12);
        assert!(result.is_speaking);
        assert_eq!(result.method, "visual_primary");
    }

    #[test]
    fn missing_speaking_inputs_are_explicit() {
        let directory = tempfile::tempdir().unwrap();
        let video = directory.path().join("video.mp4");
        assert!(SpeakingDetector::load(&video, 0.08, false).is_err());
        assert!(SpeakingDetector::load(&video, 0.08, true).is_ok());

        let csv = directory.path().join("video.csv");
        std::fs::write(&csv, "No,Frame In,Frame Out\n1,0,10\n").unwrap();
        assert!(SpeakingDetector::load(&video, 0.08, false).is_err());
    }

    #[test]
    fn audio_and_segment_are_both_reported() {
        let mut audio = HashMap::new();
        audio.insert(
            1,
            AudioInfo {
                is_speech_present: false,
                num_speakers: 0,
                active_speakers: Vec::new(),
            },
        );
        let detector = detector(
            vec![Segment {
                segment_id: 1,
                frame_start: 1,
                frame_end: 54,
            }],
            Some(audio),
        );
        let result = detector.detect(1, None, "object_detector");
        let sources = result.sources.as_object().unwrap();
        assert!(sources.contains_key("audio"));
        assert!(sources.contains_key("segment"));
        assert_eq!(sources["segment"]["frame_position"], 0.0);
    }
}
