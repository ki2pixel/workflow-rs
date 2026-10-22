//! Réduction de `<stem>.json` / `<stem>_tracking.json` (moitié vidéo de STEP5).
//!
//! Le parseur lit le document en streaming : une frame est désérialisée en
//! `Value`, réduite, puis écrite dans un fichier temporaire. La mémoire reste
//! donc bornée par la frame courante, pas par le document complet.

use super::analytics::{ExpressionCollector, TrackingAnalyticsBuilder};
use super::util::{as_i64_loose, json_number};
use crate::error::StepError;
use serde::de::{DeserializeSeed, Error as DeError, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Number, Value};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;
use tempfile::tempfile_in;
use workflow_schema::audio::AudioMeta;

#[derive(Debug, Clone)]
pub struct VideoOptions {
    pub tracking_analytics: bool,
    pub expression_summary: bool,
    pub expression_keys: Vec<String>,
    pub audio: Option<AudioMeta>,
}

impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            tracking_analytics: true,
            expression_summary: false,
            expression_keys: vec!["jawOpen".into()],
            audio: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoMeta {
    pub fps: Option<f64>,
    pub total_frames: Option<i64>,
}

/// Vue typée d'une frame STEP4 : seuls les champs consommés par STEP5 sont
/// désérialisés. Les `blendshapes` et les autres champs volumineux sont ignorés
/// par serde sans allocation.
#[derive(Debug, Default, Deserialize)]
struct FastFrame {
    #[serde(default)]
    frame: Value,
    #[serde(default)]
    tracked_objects: Option<Vec<FastObject>>,
}

#[derive(Debug, Default, Deserialize)]
struct FastObject {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    centroid_x: Option<Number>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    confidence: Option<Number>,
    #[serde(default)]
    bbox_width: Option<Number>,
    #[serde(default)]
    bbox_height: Option<Number>,
    #[serde(default)]
    active_speakers: Option<Value>,
    #[serde(default)]
    speaking_sources: Option<Value>,
}

fn process_fast_object(object: FastObject) -> Value {
    let FastObject {
        id,
        centroid_x,
        source,
        label,
        confidence,
        bbox_width,
        bbox_height,
        active_speakers,
        speaking_sources,
    } = object;

    let mut new_object = Map::new();
    new_object.insert("id".into(), id.map_or(Value::Null, Value::String));
    new_object.insert(
        "centroid_x".into(),
        centroid_x.map_or(Value::Null, Value::Number),
    );
    new_object.insert("source".into(), source.map_or(Value::Null, Value::String));
    new_object.insert("label".into(), label.map_or(Value::Null, Value::String));
    new_object.insert(
        "confidence".into(),
        confidence.map_or(Value::Null, Value::Number),
    );

    let mut active = Value::Array(Vec::new());
    if let Some(value) = active_speakers {
        if value.is_array() {
            active = value;
        }
    }
    if let Some(speaking_sources) = speaking_sources.as_ref().and_then(Value::as_object) {
        if !speaking_sources.is_empty() {
            if let Some(audio) = speaking_sources.get("audio").and_then(Value::as_object) {
                if !audio.is_empty() {
                    active = audio
                        .get("active_speakers")
                        .cloned()
                        .unwrap_or_else(|| Value::Array(Vec::new()));
                }
            }
        }
    }
    new_object.insert("active_speakers".into(), active);

    if let (Some(width), Some(height)) = (bbox_width, bbox_height) {
        new_object.insert("bbox_width".into(), Value::Number(width));
        new_object.insert("bbox_height".into(), Value::Number(height));
    }

    Value::Object(new_object)
}

/// Réduit un fichier vidéo STEP4. Retourne `None` si aucun tableau de frames
/// n'est trouvé, comme le réducteur Python.
pub fn reduce_video_file(
    input: &Path,
    output: &Path,
    options: &VideoOptions,
) -> Result<Option<VideoMeta>, StepError> {
    let file = File::open(input).map_err(|source| StepError::Read {
        path: input.to_path_buf(),
        source,
    })?;

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let frames_file = tempfile_in(parent)?;
    let mut sink = FrameSink::new(frames_file, options)?;

    let top = {
        let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(file));
        let top = deserializer.deserialize_map(TopVisitor { sink: &mut sink })?;
        deserializer.end()?;
        top
    };

    if !top.seen_frames {
        return Ok(None);
    }

    sink.finish_array()?;

    let mut total_frames = top.total_frames;
    if total_frames.is_none() && sink.max_frame_seen > 0 {
        total_frames = Some(sink.max_frame_seen);
    }
    let meta = VideoMeta {
        fps: top.fps,
        total_frames,
    };

    let analytics = if options.tracking_analytics {
        Some(sink.analytics.build(meta.total_frames, sink.frames_count))
    } else {
        None
    };
    let expression = if options.expression_summary {
        sink.expression.build()
    } else {
        None
    };
    let temporal = options
        .audio
        .as_ref()
        .and_then(|audio| compute_temporal_alignment(meta.total_frames, meta.fps, audio));

    write_output(
        output,
        sink.frames.get_mut(),
        meta.fps,
        meta.total_frames,
        analytics.as_ref(),
        expression.as_ref(),
        temporal.as_ref(),
    )?;

    Ok(Some(meta))
}

fn process_tracked_object(object: &Map<String, Value>) -> Value {
    let mut new_object = Map::new();
    new_object.insert(
        "id".into(),
        object.get("id").cloned().unwrap_or(Value::Null),
    );
    new_object.insert(
        "centroid_x".into(),
        object.get("centroid_x").cloned().unwrap_or(Value::Null),
    );
    new_object.insert(
        "source".into(),
        object.get("source").cloned().unwrap_or(Value::Null),
    );
    new_object.insert(
        "label".into(),
        object.get("label").cloned().unwrap_or(Value::Null),
    );
    new_object.insert(
        "confidence".into(),
        object.get("confidence").cloned().unwrap_or(Value::Null),
    );

    let mut active_speakers = Value::Array(Vec::new());
    if let Some(value) = object.get("active_speakers") {
        if value.is_array() {
            active_speakers = value.clone();
        }
    }
    if let Some(audio) = object
        .get("speaking_sources")
        .and_then(Value::as_object)
        .and_then(|sources| sources.get("audio"))
        .and_then(Value::as_object)
    {
        active_speakers = audio
            .get("active_speakers")
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()));
    }
    new_object.insert("active_speakers".into(), active_speakers);

    if let (Some(width), Some(height)) = (object.get("bbox_width"), object.get("bbox_height")) {
        if !width.is_null() && !height.is_null() {
            new_object.insert("bbox_width".into(), width.clone());
            new_object.insert("bbox_height".into(), height.clone());
        }
    }

    Value::Object(new_object)
}

fn compute_temporal_alignment(
    video_total_frames: Option<i64>,
    video_fps: Option<f64>,
    audio: &AudioMeta,
) -> Option<Value> {
    if video_total_frames.is_none()
        && audio.total_frames.is_none()
        && video_fps.is_none()
        && audio.fps.is_none()
    {
        return None;
    }

    let mut warnings = Vec::new();
    if let (Some(video), Some(audio_total)) = (video_total_frames, audio.total_frames) {
        let ratio = (video - audio_total).abs() as f64 / audio_total.max(1) as f64;
        if ratio > 0.05 {
            warnings.push(format!(
                "frame_count_mismatch video={video} audio={audio_total}"
            ));
        }
    }
    if let (Some(video), Some(audio_fps)) = (video_fps, audio.fps) {
        if (video - audio_fps).abs() > 0.5 {
            warnings.push(format!(
                "fps_mismatch video={} audio={}",
                python_float(video),
                python_float(audio_fps)
            ));
        }
    }

    let mut output = Map::new();
    output.insert(
        "video_total_frames".into(),
        video_total_frames.map_or(Value::Null, Value::from),
    );
    output.insert(
        "audio_total_frames".into(),
        audio.total_frames.map_or(Value::Null, Value::from),
    );
    output.insert(
        "video_fps".into(),
        video_fps.map_or(Value::Null, json_number),
    );
    output.insert(
        "audio_fps".into(),
        audio.fps.map_or(Value::Null, json_number),
    );
    output.insert(
        "warnings".into(),
        Value::Array(warnings.into_iter().map(Value::String).collect()),
    );
    Some(Value::Object(output))
}

/// Formate un float comme `str(float)` de Python pour les avertissements.
fn python_float(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

struct TopResult {
    fps: Option<f64>,
    total_frames: Option<i64>,
    seen_frames: bool,
}

struct TopVisitor<'a> {
    sink: &'a mut FrameSink,
}

impl<'de, 'a> Visitor<'de> for TopVisitor<'a> {
    type Value = TopResult;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("un document JSON de tracking")
    }

    fn visit_map<A>(self, mut map: A) -> Result<TopResult, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut fps = None;
        let mut total_frames = None;
        let mut seen_frames = false;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "fps" => {
                    let value: Value = map.next_value()?;
                    fps = value.as_f64();
                }
                "total_frames" => {
                    let value: Value = map.next_value()?;
                    total_frames = value.as_i64();
                }
                "metadata" => {
                    let metadata: Value = map.next_value()?;
                    if let Some(metadata) = metadata.as_object() {
                        if fps.is_none() {
                            fps = metadata.get("fps").and_then(Value::as_f64);
                        }
                        if total_frames.is_none() {
                            total_frames = metadata.get("total_frames").and_then(Value::as_i64);
                        }
                    }
                }
                "frames" | "frames_analysis" if !seen_frames => {
                    seen_frames = true;
                    map.next_value_seed(FramesSeed {
                        sink: &mut *self.sink,
                    })?;
                }
                _ => {
                    let _: IgnoredAny = map.next_value()?;
                }
            }
        }

        Ok(TopResult {
            fps,
            total_frames,
            seen_frames,
        })
    }
}

struct FramesSeed<'a> {
    sink: &'a mut FrameSink,
}

impl<'de, 'a> DeserializeSeed<'de> for FramesSeed<'a> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(FramesVisitor { sink: self.sink })
    }
}

struct FramesVisitor<'a> {
    sink: &'a mut FrameSink,
}

impl<'de, 'a> Visitor<'de> for FramesVisitor<'a> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("un tableau de frames")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<(), A::Error>
    where
        A: SeqAccess<'de>,
    {
        if self.sink.fast {
            while let Some(frame) = sequence.next_element::<FastFrame>()? {
                self.sink
                    .push_fast(frame)
                    .map_err(|error| A::Error::custom(error.to_string()))?;
            }
        } else {
            while let Some(frame) = sequence.next_element::<Value>()? {
                self.sink
                    .push_value(&frame)
                    .map_err(|error| A::Error::custom(error.to_string()))?;
            }
        }
        Ok(())
    }
}

struct FrameSink {
    frames: BufWriter<File>,
    first: bool,
    frames_count: u64,
    max_frame_seen: i64,
    analytics: TrackingAnalyticsBuilder,
    expression: ExpressionCollector,
    expression_enabled: bool,
    fast: bool,
}

impl FrameSink {
    fn new(frames: File, options: &VideoOptions) -> Result<Self, StepError> {
        let mut frames = BufWriter::new(frames);
        frames.write_all(b"[")?;
        Ok(Self {
            frames,
            first: true,
            frames_count: 0,
            max_frame_seen: 0,
            analytics: TrackingAnalyticsBuilder::default(),
            expression: ExpressionCollector::new(options.expression_keys.clone()),
            expression_enabled: options.expression_summary,
            fast: !options.expression_summary,
        })
    }

    fn push_fast(&mut self, frame: FastFrame) -> Result<(), StepError> {
        let tracked_objects = frame
            .tracked_objects
            .unwrap_or_default()
            .into_iter()
            .map(process_fast_object)
            .collect();
        self.push_reduced(frame.frame, tracked_objects)
    }

    fn push_value(&mut self, frame: &Value) -> Result<(), StepError> {
        let frame = frame
            .as_object()
            .ok_or_else(|| StepError::Schema("une frame n'est pas un objet".into()))?;
        let frame_value = frame.get("frame").cloned().unwrap_or(Value::Null);

        let mut tracked_objects = Vec::new();
        if let Some(objects) = frame.get("tracked_objects") {
            if !objects.is_null() {
                if let Some(objects) = objects.as_array() {
                    for object in objects {
                        let Some(object) = object.as_object() else {
                            continue;
                        };
                        if self.expression_enabled {
                            if let Some(object_id) = object.get("id").and_then(Value::as_str) {
                                self.expression.add_object(object_id, object);
                            }
                        }
                        tracked_objects.push(process_tracked_object(object));
                    }
                }
            }
        }

        self.push_reduced(frame_value, tracked_objects)
    }

    fn push_reduced(
        &mut self,
        frame_value: Value,
        tracked_objects: Vec<Value>,
    ) -> Result<(), StepError> {
        if let Some(frame_number) = as_i64_loose(&frame_value) {
            self.max_frame_seen = self.max_frame_seen.max(frame_number);
        }

        let mut reduced_frame = Map::new();
        reduced_frame.insert("frame".into(), frame_value);
        reduced_frame.insert("tracked_objects".into(), Value::Array(tracked_objects));
        let reduced_frame = Value::Object(reduced_frame);

        self.analytics.add_frame(&reduced_frame);

        if !self.first {
            self.frames.write_all(b",")?;
        }
        self.first = false;
        serde_json::to_writer(&mut self.frames, &reduced_frame)?;
        self.frames_count += 1;
        Ok(())
    }

    fn finish_array(&mut self) -> Result<(), StepError> {
        self.frames.write_all(b"]")?;
        self.frames.flush()?;
        self.frames.get_mut().seek(SeekFrom::Start(0))?;
        Ok(())
    }
}

fn write_output(
    output: &Path,
    frames: &mut File,
    fps: Option<f64>,
    total_frames: Option<i64>,
    analytics: Option<&Value>,
    expression: Option<&Value>,
    temporal: Option<&Value>,
) -> Result<(), StepError> {
    let temp_path = output.with_extension("tmp");
    let result = (|| -> Result<(), StepError> {
        let file = File::create(&temp_path).map_err(|source| StepError::Write {
            path: temp_path.clone(),
            source,
        })?;
        let mut writer = BufWriter::new(file);
        writer.write_all(b"{")?;

        let mut first = true;
        if let Some(fps) = fps {
            if !first {
                writer.write_all(b",")?;
            }
            first = false;
            writer.write_all(b"\"fps\":")?;
            writer.write_all(serde_json::to_string(&fps)?.as_bytes())?;
        }
        if let Some(total_frames) = total_frames {
            if !first {
                writer.write_all(b",")?;
            }
            first = false;
            writer.write_all(b"\"total_frames\":")?;
            writer.write_all(serde_json::to_string(&total_frames)?.as_bytes())?;
        }
        if !first {
            writer.write_all(b",")?;
        }
        writer.write_all(b"\"frames_analysis\":")?;
        io::copy(frames, &mut writer).map_err(|source| StepError::Write {
            path: temp_path.clone(),
            source,
        })?;
        if let Some(expression) = expression {
            writer.write_all(b",\"expression_summary\":")?;
            serde_json::to_writer(&mut writer, expression)?;
        }
        if let Some(analytics) = analytics {
            writer.write_all(b",\"tracking_analytics\":")?;
            serde_json::to_writer(&mut writer, analytics)?;
        }
        if let Some(temporal) = temporal {
            writer.write_all(b",\"temporal_alignment\":")?;
            serde_json::to_writer(&mut writer, temporal)?;
        }
        writer.write_all(b"}\n")?;
        writer.flush()?;
        fs::rename(&temp_path, output).map_err(|source| StepError::Write {
            path: output.to_path_buf(),
            source,
        })?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn speaking_sources_override_active_speakers() {
        let object = json!({
            "id": "obj_1",
            "active_speakers": ["A"],
            "speaking_sources": {"audio": {"active_speakers": ["B"]}},
            "bbox_width": 10,
            "bbox_height": 20
        });
        let reduced = process_tracked_object(object.as_object().unwrap());
        assert_eq!(reduced["active_speakers"], json!(["B"]));
        assert_eq!(reduced["bbox_width"], json!(10));
        assert_eq!(reduced["bbox_height"], json!(20));
    }

    #[test]
    fn temporal_alignment_matches_python_fields() {
        let audio = AudioMeta {
            fps: Some(25.0),
            total_frames: Some(4921),
        };
        let alignment = compute_temporal_alignment(Some(4920), None, &audio).unwrap();
        assert_eq!(alignment["video_total_frames"], json!(4920));
        assert_eq!(alignment["audio_total_frames"], json!(4921));
        assert_eq!(alignment["video_fps"], Value::Null);
        assert_eq!(alignment["audio_fps"], json!(25.0));
        assert_eq!(alignment["warnings"], json!([]));
    }

    #[test]
    fn temporal_alignment_reports_fps_mismatch() {
        let audio = AudioMeta {
            fps: Some(24.0),
            total_frames: None,
        };
        let alignment = compute_temporal_alignment(None, Some(30.0), &audio).unwrap();
        assert_eq!(
            alignment["warnings"],
            json!(["fps_mismatch video=30.0 audio=24.0"])
        );
    }

    #[test]
    fn streams_frames_and_writes_header_first() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.json");
        let output = dir.path().join("output.json");
        let payload = json!({
            "metadata": {"video_path": "/tmp/x.mp4"},
            "frames": [
                {"frame": 1, "tracked_objects": [
                    {"id": "obj_1", "centroid_x": 10, "source": "face",
                     "label": "face", "confidence": 0.9, "bbox_width": 2,
                     "bbox_height": 3}
                ]},
                {"frame": 2, "tracked_objects": []}
            ]
        });
        let mut file = File::create(&input).unwrap();
        file.write_all(serde_json::to_string(&payload).unwrap().as_bytes())
            .unwrap();

        let meta = reduce_video_file(&input, &output, &VideoOptions::default())
            .unwrap()
            .expect("sortie");
        assert_eq!(meta.total_frames, Some(2));
        assert_eq!(meta.fps, None);

        let value: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(value["total_frames"], json!(2));
        assert!(value.get("fps").is_none());
        assert_eq!(value["frames_analysis"].as_array().unwrap().len(), 2);
        assert_eq!(
            value["frames_analysis"][0]["tracked_objects"][0]["id"],
            json!("obj_1")
        );
        assert_eq!(
            value["tracking_analytics"]["global"]["objects_total"],
            json!(1)
        );
        assert_eq!(
            value["tracking_analytics"]["global"]["frames_total"],
            json!(2)
        );
    }
}
