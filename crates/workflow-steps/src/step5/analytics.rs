use serde_json::{Map, Value};
use std::collections::HashMap;
use workflow_schema::round::{round2, round4};

use super::util::{as_f64_loose, json_number};

const CONFIDENCE_BUCKETS: [(f64, f64); 5] = [
    (0.0, 0.25),
    (0.25, 0.5),
    (0.5, 0.75),
    (0.75, 0.9),
    (0.9, 1.000_000_1),
];

const CONFIDENCE_LABELS: [&str; 5] = [
    "0.00-0.25",
    "0.25-0.50",
    "0.50-0.75",
    "0.75-0.90",
    "0.90-1.00",
];

#[derive(Debug, Default)]
struct ObjectStats {
    source: Value,
    label: Value,
    presence_frames: u64,
    confidence_sum: f64,
    confidence_count: u64,
    bbox_surface_sum: f64,
    bbox_surface_count: u64,
    centroid_x_min: Option<f64>,
    centroid_x_max: Option<f64>,
}

impl ObjectStats {
    fn new(obj: &Map<String, Value>) -> Self {
        Self {
            source: obj.get("source").cloned().unwrap_or(Value::Null),
            label: obj.get("label").cloned().unwrap_or(Value::Null),
            ..Self::default()
        }
    }
}

/// Agrégateur des statistiques de suivi de STEP5.
///
/// Il est alimenté avec les frames **réduites**, comme le fait Python, et
/// conserve l'ordre de première apparition des objets.
#[derive(Debug, Default)]
pub struct TrackingAnalyticsBuilder {
    frames_with_objects: u64,
    objects_per_frame_sum: u64,
    confidence_histogram: [u64; 5],
    objects: Vec<(String, ObjectStats)>,
    index: HashMap<String, usize>,
}

impl TrackingAnalyticsBuilder {
    pub fn add_frame(&mut self, frame: &Value) {
        let Some(tracked) = frame.get("tracked_objects").and_then(Value::as_array) else {
            return;
        };
        if tracked.is_empty() {
            return;
        }

        self.frames_with_objects += 1;
        self.objects_per_frame_sum += tracked.len() as u64;

        for object in tracked {
            let Some(object) = object.as_object() else {
                continue;
            };
            let Some(object_id) = object.get("id").and_then(Value::as_str) else {
                continue;
            };
            if object_id.is_empty() {
                continue;
            }

            let index = if let Some(index) = self.index.get(object_id).copied() {
                index
            } else {
                let index = self.objects.len();
                self.objects
                    .push((object_id.to_string(), ObjectStats::new(object)));
                self.index.insert(object_id.to_string(), index);
                index
            };
            let stats = &mut self.objects[index].1;
            stats.presence_frames += 1;

            if let Some(confidence) = object.get("confidence").filter(|value| !value.is_null()) {
                if let Some(confidence) = as_f64_loose(confidence) {
                    let confidence = confidence.clamp(0.0, 1.0);
                    stats.confidence_sum += confidence;
                    stats.confidence_count += 1;
                    if let Some(bucket) = CONFIDENCE_BUCKETS
                        .iter()
                        .position(|(min, max)| *min <= confidence && confidence < *max)
                    {
                        self.confidence_histogram[bucket] += 1;
                    }
                }
            }

            if let (Some(width), Some(height)) = (
                object.get("bbox_width").filter(|value| !value.is_null()),
                object.get("bbox_height").filter(|value| !value.is_null()),
            ) {
                if let (Some(width), Some(height)) = (as_f64_loose(width), as_f64_loose(height)) {
                    stats.bbox_surface_sum += width.max(0.0) * height.max(0.0);
                    stats.bbox_surface_count += 1;
                }
            }

            if let Some(centroid_x) = object
                .get("centroid_x")
                .filter(|value| !value.is_null())
                .and_then(as_f64_loose)
            {
                stats.centroid_x_min = Some(
                    stats
                        .centroid_x_min
                        .map_or(centroid_x, |current| current.min(centroid_x)),
                );
                stats.centroid_x_max = Some(
                    stats
                        .centroid_x_max
                        .map_or(centroid_x, |current| current.max(centroid_x)),
                );
            }
        }
    }

    pub fn build(&self, total_frames: Option<i64>, frames_count: u64) -> Value {
        let frames_total = total_frames.unwrap_or(frames_count as i64);
        let mut objects_out = Map::new();

        for (object_id, stats) in &self.objects {
            let mut object = Map::new();
            object.insert("presence_frames".into(), Value::from(stats.presence_frames));
            object.insert("source".into(), stats.source.clone());
            object.insert("label".into(), stats.label.clone());

            if frames_total > 0 {
                object.insert(
                    "presence_ratio".into(),
                    json_number(round4(stats.presence_frames as f64 / frames_total as f64)),
                );
            }
            if stats.confidence_count > 0 {
                object.insert(
                    "avg_confidence".into(),
                    json_number(round4(stats.confidence_sum / stats.confidence_count as f64)),
                );
            }
            if stats.bbox_surface_count > 0 {
                object.insert(
                    "avg_bbox_surface".into(),
                    json_number(round2(
                        stats.bbox_surface_sum / stats.bbox_surface_count as f64,
                    )),
                );
            }
            if let (Some(min), Some(max)) = (stats.centroid_x_min, stats.centroid_x_max) {
                object.insert("centroid_x_spread".into(), json_number(round2(max - min)));
            }

            objects_out.insert(object_id.clone(), Value::Object(object));
        }

        let mut histogram = Map::new();
        for (label, count) in CONFIDENCE_LABELS.iter().zip(self.confidence_histogram) {
            histogram.insert((*label).to_string(), Value::from(count));
        }

        let mut global = Map::new();
        global.insert("frames_total".into(), Value::from(frames_total));
        global.insert(
            "frames_with_objects".into(),
            Value::from(self.frames_with_objects),
        );
        global.insert("objects_total".into(), Value::from(self.objects.len()));
        global.insert("confidence_histogram".into(), Value::Object(histogram));
        if self.frames_with_objects > 0 {
            global.insert(
                "avg_objects_per_frame".into(),
                json_number(round4(
                    self.objects_per_frame_sum as f64 / self.frames_with_objects as f64,
                )),
            );
        }

        let mut output = Map::new();
        output.insert("schema_version".into(), Value::from(1));
        output.insert("global".into(), Value::Object(global));
        output.insert("objects".into(), Value::Object(objects_out));
        Value::Object(output)
    }
}

#[derive(Debug, Default)]
struct ExpressionStats {
    count: u64,
    sum: Map<String, Value>,
    max: Map<String, Value>,
}

/// Collecte `expression_summary` quand l'option est active (désactivée par
/// défaut en production).
#[derive(Debug)]
pub struct ExpressionCollector {
    keys: Vec<String>,
    stats: Vec<(String, ExpressionStats)>,
    index: HashMap<String, usize>,
}

impl ExpressionCollector {
    pub fn new(keys: Vec<String>) -> Self {
        Self {
            keys,
            stats: Vec::new(),
            index: HashMap::new(),
        }
    }

    pub fn add_object(&mut self, object_id: &str, object: &Map<String, Value>) {
        if object_id.is_empty() {
            return;
        }
        let Some(blendshapes) = object.get("blendshapes").and_then(Value::as_object) else {
            return;
        };
        if blendshapes.is_empty() {
            return;
        }

        let index = if let Some(index) = self.index.get(object_id).copied() {
            index
        } else {
            let index = self.stats.len();
            self.stats
                .push((object_id.to_string(), ExpressionStats::default()));
            self.index.insert(object_id.to_string(), index);
            index
        };
        let stats = &mut self.stats[index].1;
        stats.count += 1;

        for key in &self.keys {
            let Some(value) = blendshapes.get(key).and_then(as_f64_loose) else {
                continue;
            };
            let new_sum = stats
                .sum
                .get(key)
                .and_then(as_f64_loose)
                .map_or(value, |current| current + value);
            stats.sum.insert(key.clone(), json_number(new_sum));

            let new_max = stats
                .max
                .get(key)
                .and_then(as_f64_loose)
                .map_or(value, |current| current.max(value));
            stats.max.insert(key.clone(), json_number(new_max));
        }
    }

    pub fn build(&self) -> Option<Value> {
        let mut objects = Map::new();
        for (object_id, stats) in &self.stats {
            if stats.count == 0 {
                continue;
            }
            let mut mean = Map::new();
            for (key, value) in &stats.sum {
                if let Some(value) = as_f64_loose(value) {
                    mean.insert(key.clone(), json_number(round4(value / stats.count as f64)));
                }
            }
            let mut max = Map::new();
            for (key, value) in &stats.max {
                if let Some(value) = as_f64_loose(value) {
                    max.insert(key.clone(), json_number(round4(value)));
                }
            }
            if mean.is_empty() && max.is_empty() {
                continue;
            }
            let mut entry = Map::new();
            entry.insert("count".into(), Value::from(stats.count));
            entry.insert("mean".into(), Value::Object(mean));
            entry.insert("max".into(), Value::Object(max));
            objects.insert(object_id.clone(), Value::Object(entry));
        }

        if objects.is_empty() {
            return None;
        }
        let mut output = Map::new();
        output.insert(
            "keys".into(),
            Value::Array(
                self.keys
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect::<Vec<_>>(),
            ),
        );
        output.insert("objects".into(), Value::Object(objects));
        Some(Value::Object(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn analytics_matches_python_shape() {
        let mut builder = TrackingAnalyticsBuilder::default();
        builder.add_frame(&json!({
            "frame": 1,
            "tracked_objects": [
                {"id": "obj_1", "centroid_x": 10.0, "source": "face",
                 "label": "face", "confidence": 0.95, "bbox_width": 10,
                 "bbox_height": 20},
                {"id": "obj_2", "centroid_x": 20.0, "source": "face",
                 "label": "face", "confidence": 0.4, "bbox_width": 4,
                 "bbox_height": 5}
            ]
        }));
        builder.add_frame(&json!({
            "frame": 2,
            "tracked_objects": [
                {"id": "obj_1", "centroid_x": 30.0, "source": "face",
                 "label": "face", "confidence": 1.0, "bbox_width": 10,
                 "bbox_height": 20}
            ]
        }));

        let analytics = builder.build(Some(2), 2);
        assert_eq!(analytics["global"]["frames_total"], json!(2));
        assert_eq!(analytics["global"]["frames_with_objects"], json!(2));
        assert_eq!(analytics["global"]["objects_total"], json!(2));
        assert_eq!(analytics["global"]["avg_objects_per_frame"], json!(1.5));
        assert_eq!(
            analytics["global"]["confidence_histogram"]["0.90-1.00"],
            json!(2)
        );
        assert_eq!(
            analytics["global"]["confidence_histogram"]["0.25-0.50"],
            json!(1)
        );
        assert_eq!(analytics["objects"]["obj_1"]["presence_frames"], json!(2));
        assert_eq!(analytics["objects"]["obj_1"]["presence_ratio"], json!(1.0));
        assert_eq!(
            analytics["objects"]["obj_1"]["avg_confidence"],
            json!(0.975)
        );
        assert_eq!(
            analytics["objects"]["obj_1"]["centroid_x_spread"],
            json!(20.0)
        );
    }

    #[test]
    fn expression_summary_uses_enabled_keys() {
        let mut collector = ExpressionCollector::new(vec!["jawOpen".into()]);
        collector.add_object(
            "obj_1",
            json!({"blendshapes": {"jawOpen": 0.2, "other": 1.0}})
                .as_object()
                .unwrap(),
        );
        collector.add_object(
            "obj_1",
            json!({"blendshapes": {"jawOpen": 0.4}})
                .as_object()
                .unwrap(),
        );
        let summary = collector.build().expect("résumé");
        assert_eq!(summary["keys"], json!(["jawOpen"]));
        assert_eq!(summary["objects"]["obj_1"]["count"], json!(2));
        assert_eq!(summary["objects"]["obj_1"]["mean"]["jawOpen"], json!(0.3));
        assert_eq!(summary["objects"]["obj_1"]["max"]["jawOpen"], json!(0.4));
    }
}
