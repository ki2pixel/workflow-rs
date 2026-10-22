//! Réduction de `<stem>_audio.json` (moitié audio de STEP5).
//!
//! Le format de sortie est celui consommé par STEP6 :
//!
//! ```json
//! {
//!   "fps": 25.0,
//!   "total_frames": 4920,
//!   "frames_analysis": [
//!     {"frame": 1, "audio_info": {"is_speech_present": true,
//!      "active_speaker_labels": ["SPEAKER_00"], "timecode_sec": 0.04}}
//!   ],
//!   "speaker_stats": {"unique_speakers": ["SPEAKER_00"],
//!                     "speaker_frame_counts": {"SPEAKER_00": 123}}
//! }
//! ```
//!
//! Contrairement à Python, l'en-tête (`fps`, `total_frames`) est écrit avant le
//! tableau. Cela rend une seconde réduction idempotente et permet aux parseurs
//! streaming de STEP5/STEP6 de s'arrêter au tableau sans perdre les métadonnées.

use crate::error::StepError;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use workflow_schema::audio::{AudioMeta, SpeakerStats};
use workflow_schema::AUDIO_SUFFIX;

/// Réduit un fichier audio JSON et l'écrit de façon atomique.
pub fn reduce_audio_file(input: &Path, output: &Path) -> Result<AudioMeta, StepError> {
    let bytes = fs::read(input).map_err(|source| StepError::Read {
        path: input.to_path_buf(),
        source,
    })?;
    let root: Value = serde_json::from_slice(&bytes)?;
    let (reduced, meta) = reduce_audio_value(&root)?;
    write_json_atomically(output, &reduced)?;
    Ok(meta)
}

/// Variante sans I/O, utilisée par les tests et le mode shadow.
pub fn reduce_audio_value(root: &Value) -> Result<(Value, AudioMeta), StepError> {
    let object = root
        .as_object()
        .ok_or_else(|| StepError::Schema("la racine audio n'est pas un objet".into()))?;

    let frames = object
        .get("frames_analysis")
        .or_else(|| object.get("frames"))
        .and_then(Value::as_array)
        .ok_or_else(|| StepError::Schema("aucun tableau frames_analysis dans l'audio".into()))?;

    let fps = read_fps(object);
    let mut total_frames = read_total_frames(object);

    let mut reduced_frames = Vec::with_capacity(frames.len());
    let mut speaker_counts: BTreeMap<String, u64> = BTreeMap::new();
    let mut max_frame_seen: i64 = 0;

    for frame in frames {
        let Some(frame_object) = frame.as_object() else {
            continue;
        };
        let Some(audio_info) = frame_object.get("audio_info").and_then(Value::as_object) else {
            continue;
        };

        let is_speech_present = audio_info
            .get("is_speech_present")
            .cloned()
            .unwrap_or(Value::Bool(false));
        let active_speakers = audio_info
            .get("active_speaker_labels")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));

        let mut new_info = Map::new();
        new_info.insert("is_speech_present".into(), is_speech_present);
        new_info.insert("active_speaker_labels".into(), active_speakers.clone());
        if let Some(timecode) = audio_info.get("timecode_sec") {
            if !timecode.is_null() {
                new_info.insert("timecode_sec".into(), timecode.clone());
            }
        }

        let frame_value = frame_object.get("frame").cloned().unwrap_or(Value::Null);
        if let Some(frame_number) = frame_value.as_i64() {
            max_frame_seen = max_frame_seen.max(frame_number);
        } else if let Some(frame_number) = frame_value.as_f64() {
            max_frame_seen = max_frame_seen.max(frame_number as i64);
        }

        if let Some(labels) = active_speakers.as_array() {
            for label in labels {
                if let Some(label) = label.as_str() {
                    if !label.is_empty() {
                        *speaker_counts.entry(label.to_string()).or_insert(0) += 1;
                    }
                }
            }
        }

        let mut reduced_frame = Map::new();
        reduced_frame.insert("frame".into(), frame_value);
        reduced_frame.insert("audio_info".into(), Value::Object(new_info));
        reduced_frames.push(Value::Object(reduced_frame));
    }

    if total_frames.is_none() && max_frame_seen > 0 {
        total_frames = Some(max_frame_seen);
    }

    let mut output = Map::new();
    if let Some(fps) = fps {
        output.insert("fps".into(), number_value(fps));
    }
    if let Some(total_frames) = total_frames {
        output.insert("total_frames".into(), Value::from(total_frames));
    }
    output.insert("frames_analysis".into(), Value::Array(reduced_frames));

    if !speaker_counts.is_empty() {
        let stats = SpeakerStats {
            unique_speakers: speaker_counts.keys().cloned().collect(),
            speaker_frame_counts: speaker_counts,
        };
        output.insert("speaker_stats".into(), serde_json::to_value(stats)?);
    }

    if let Some(embeddings) = object.get("speaker_embeddings") {
        if embeddings
            .as_object()
            .is_some_and(|value| !value.is_empty())
        {
            output.insert("speaker_embeddings".into(), embeddings.clone());
        }
    }

    Ok((Value::Object(output), AudioMeta { fps, total_frames }))
}

fn read_fps(object: &Map<String, Value>) -> Option<f64> {
    object.get("fps").and_then(Value::as_f64).or_else(|| {
        object
            .get("metadata")
            .and_then(Value::as_object)
            .and_then(|metadata| metadata.get("fps"))
            .and_then(Value::as_f64)
    })
}

fn read_total_frames(object: &Map<String, Value>) -> Option<i64> {
    object
        .get("total_frames")
        .and_then(Value::as_i64)
        .or_else(|| {
            object
                .get("metadata")
                .and_then(Value::as_object)
                .and_then(|metadata| metadata.get("total_frames"))
                .and_then(Value::as_i64)
        })
}

fn number_value(value: f64) -> Value {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn write_json_atomically(output: &Path, payload: &Value) -> Result<(), StepError> {
    let temp_path = output.with_extension("tmp");
    let write_result = (|| -> Result<(), StepError> {
        let file = File::create(&temp_path).map_err(|source| StepError::Write {
            path: temp_path.clone(),
            source,
        })?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, payload)?;
        writer.write_all(b"\n").map_err(|source| StepError::Write {
            path: temp_path.clone(),
            source,
        })?;
        writer.flush().map_err(|source| StepError::Write {
            path: temp_path.clone(),
            source,
        })?;
        fs::rename(&temp_path, output).map_err(|source| StepError::Write {
            path: output.to_path_buf(),
            source,
        })?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    write_result
}

/// Construit le nom `<stem>_audio.json` à partir d'un stem.
pub fn audio_path_for_stem(stem: &str) -> PathBuf {
    PathBuf::from(format!("{stem}{AUDIO_SUFFIX}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reduces_frames_and_counts_speakers() {
        let root = json!({
            "fps": 25.0,
            "total_frames": 3,
            "frames_analysis": [
                {"frame": 1, "audio_info": {"is_speech_present": true,
                  "active_speaker_labels": ["A"], "timecode_sec": 0.04}},
                {"frame": 2, "audio_info": null},
                {"frame": 3, "audio_info": {"active_speaker_labels": ["A", "B"]}}
            ]
        });

        let (reduced, meta) = reduce_audio_value(&root).expect("réduction");
        assert_eq!(meta.fps, Some(25.0));
        assert_eq!(meta.total_frames, Some(3));
        let frames = reduced["frames_analysis"].as_array().unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(
            frames[0]["audio_info"]["active_speaker_labels"],
            json!(["A"])
        );
        assert_eq!(frames[1]["frame"], json!(3));
        assert_eq!(
            reduced["speaker_stats"]["unique_speakers"],
            json!(["A", "B"])
        );
        assert_eq!(
            reduced["speaker_stats"]["speaker_frame_counts"]["A"],
            json!(2)
        );
        assert_eq!(
            reduced["speaker_stats"]["speaker_frame_counts"]["B"],
            json!(1)
        );
    }

    #[test]
    fn preserves_fps_when_header_is_after_frames() {
        // C'est le schéma produit par STEP5 : `fps` après `frames_analysis`.
        let root = json!({
            "frames_analysis": [
                {"frame": 1, "audio_info": {"is_speech_present": false,
                  "active_speaker_labels": []}}
            ],
            "fps": 25.0,
            "total_frames": 1
        });

        let (_, meta) = reduce_audio_value(&root).expect("réduction");
        assert_eq!(meta.fps, Some(25.0));
        assert_eq!(meta.total_frames, Some(1));
    }

    #[test]
    fn derives_total_frames_from_max_frame() {
        let root = json!({
            "frames_analysis": [
                {"frame": 10, "audio_info": {"active_speaker_labels": []}}
            ]
        });
        let (reduced, meta) = reduce_audio_value(&root).expect("réduction");
        assert_eq!(meta.total_frames, Some(10));
        assert_eq!(reduced["total_frames"], json!(10));
    }

    #[test]
    fn file_round_trip_is_atomic() {
        let dir = tempfile::tempdir().expect("tempdir");
        let input = dir.path().join("audio.json");
        let output = dir.path().join("audio_reduced.json");
        let payload = json!({
            "fps": 25.0,
            "total_frames": 1,
            "frames_analysis": [
                {"frame": 1, "audio_info": {"active_speaker_labels": ["A"]}}
            ]
        });
        std::fs::write(&input, serde_json::to_vec(&payload).unwrap()).unwrap();
        let meta = reduce_audio_file(&input, &output).expect("réduction fichier");
        assert_eq!(meta.total_frames, Some(1));
        let text = std::fs::read_to_string(&output).unwrap();
        assert!(text.contains("\"frames_analysis\""));
        assert!(!output.with_extension("tmp").exists());
    }
}
