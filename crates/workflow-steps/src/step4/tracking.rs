use serde::Serialize;
use serde_json::Value;

use super::face::ARKIT_52_BLENDSHAPE_NAMES;
use super::speaking::{SpeakingDetector, SpeakingResult};

#[derive(Debug, Clone)]
pub struct Detection {
    pub centroid: (i64, i64),
    pub bbox: (i64, i64, i64, i64),
    pub blendshapes: Option<Vec<(String, f64)>>,
    pub source_detector: String,
    pub label: String,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
struct ActiveObject {
    id: String,
    centroid: (i64, i64),
    bbox: (i64, i64, i64, i64),
    blendshapes: Option<Vec<(String, f64)>>,
    source_detector: String,
    label: String,
    confidence: f64,
    frames_unseen: u32,
}

fn serialize_blendshapes<S>(
    value: &Option<Vec<(String, f64)>>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value {
        None => serializer.serialize_none(),
        Some(values) => {
            use serde::ser::SerializeMap;
            let mut map = serializer.serialize_map(Some(values.len()))?;
            for (name, value) in values {
                map.serialize_entry(name, value)?;
            }
            map.end()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputObject {
    pub id: String,
    pub centroid_x: i64,
    pub centroid_y: i64,
    pub bbox_xmin: i64,
    pub bbox_xmax: i64,
    pub source: String,
    pub label: String,
    pub confidence: f64,
    #[serde(serialize_with = "serialize_blendshapes")]
    pub blendshapes: Option<Vec<(String, f64)>>,
    pub bbox_ymin: i64,
    pub bbox_ymax: i64,
    pub bbox_width: i64,
    pub bbox_height: i64,
    pub is_speaking: bool,
    pub speaking_confidence: f64,
    pub speaking_method: String,
    pub speaking_sources: Value,
}

pub struct Tracker {
    active_objects: Vec<ActiveObject>,
    next_id: u64,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            active_objects: Vec::new(),
            next_id: 0,
        }
    }

    pub fn update(
        &mut self,
        detections: &[Detection],
        frame_num: u64,
        speaking_detector: &SpeakingDetector,
    ) -> Vec<OutputObject> {
        for object in &mut self.active_objects {
            object.frames_unseen += 1;
        }

        if !self.active_objects.is_empty() && !detections.is_empty() {
            let tracked_centroids: Vec<(f64, f64)> = self
                .active_objects
                .iter()
                .map(|object| (object.centroid.0 as f64, object.centroid.1 as f64))
                .collect();
            let detection_centroids: Vec<(f64, f64)> = detections
                .iter()
                .map(|detection| (detection.centroid.0 as f64, detection.centroid.1 as f64))
                .collect();

            let mut distances = vec![0.0f64; detections.len()];
            let mut nearest = vec![0usize; detections.len()];
            for (det_index, (dx, dy)) in detection_centroids.iter().enumerate() {
                let mut best_distance = f64::INFINITY;
                let mut best_index = 0usize;
                for (tracked_index, (tx, ty)) in tracked_centroids.iter().enumerate() {
                    let squared = (dx - tx) * (dx - tx) + (dy - ty) * (dy - ty);
                    if squared < best_distance {
                        best_distance = squared;
                        best_index = tracked_index;
                    }
                }
                distances[det_index] = best_distance.sqrt();
                nearest[det_index] = best_index;
            }

            let mut potential_matches: Vec<(f64, usize, usize)> = distances
                .iter()
                .enumerate()
                .map(|(det_index, distance)| (*distance, nearest[det_index], det_index))
                .collect();
            potential_matches.sort_by(|left, right| {
                left.0
                    .partial_cmp(&right.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(left.1.cmp(&right.1))
                    .then(left.2.cmp(&right.2))
            });

            let mut matched_tracked = vec![false; self.active_objects.len()];
            let mut matched_detections = vec![false; detections.len()];
            for (distance, tracked_index, detection_index) in potential_matches {
                if distance > 100.0
                    || matched_detections[detection_index]
                    || matched_tracked[tracked_index]
                {
                    continue;
                }
                let detection = &detections[detection_index];
                let object = &mut self.active_objects[tracked_index];
                object.centroid = detection.centroid;
                object.bbox = detection.bbox;
                object.blendshapes = detection.blendshapes.clone();
                object.source_detector = detection.source_detector.clone();
                object.label = detection.label.clone();
                object.confidence = detection.confidence;
                object.frames_unseen = 0;
                matched_detections[detection_index] = true;
                matched_tracked[tracked_index] = true;
            }

            for (index, detection) in detections.iter().enumerate() {
                if matched_detections[index] {
                    continue;
                }
                self.next_id += 1;
                let id = format!("obj_{}", self.next_id);
                self.active_objects.push(ActiveObject {
                    id,
                    centroid: detection.centroid,
                    bbox: detection.bbox,
                    blendshapes: detection.blendshapes.clone(),
                    source_detector: detection.source_detector.clone(),
                    label: detection.label.clone(),
                    confidence: detection.confidence,
                    frames_unseen: 0,
                });
            }
        } else if !detections.is_empty() {
            for detection in detections {
                self.next_id += 1;
                let id = format!("obj_{}", self.next_id);
                self.active_objects.push(ActiveObject {
                    id,
                    centroid: detection.centroid,
                    bbox: detection.bbox,
                    blendshapes: detection.blendshapes.clone(),
                    source_detector: detection.source_detector.clone(),
                    label: detection.label.clone(),
                    confidence: detection.confidence,
                    frames_unseen: 0,
                });
            }
        }

        let mut output = Vec::new();
        let mut kept = Vec::with_capacity(self.active_objects.len());
        for object in self.active_objects.drain(..) {
            if object.frames_unseen > 10 {
                continue;
            }
            if object.frames_unseen == 0 {
                let speaking: SpeakingResult = speaking_detector.detect(
                    frame_num,
                    object.blendshapes.as_deref(),
                    &object.source_detector,
                );
                let confidence = (object.confidence * 1000.0).round_ties_even() / 1000.0;
                output.push(OutputObject {
                    id: object.id.clone(),
                    centroid_x: object.centroid.0,
                    centroid_y: object.centroid.1,
                    bbox_xmin: object.bbox.0,
                    bbox_xmax: object.bbox.0 + object.bbox.2,
                    source: object.source_detector.clone(),
                    label: object.label.clone(),
                    confidence,
                    blendshapes: object.blendshapes.clone(),
                    bbox_ymin: object.bbox.1,
                    bbox_ymax: object.bbox.1 + object.bbox.3,
                    bbox_width: object.bbox.2,
                    bbox_height: object.bbox.3,
                    is_speaking: speaking.is_speaking,
                    speaking_confidence: speaking.confidence,
                    speaking_method: speaking.method,
                    speaking_sources: speaking.sources,
                });
            }
            kept.push(object);
        }
        self.active_objects = kept;
        output.sort_by_key(|object| object.centroid_x);
        output
    }
}

#[derive(Debug, Clone)]
pub struct BlendFilter {
    pub profile: String,
    pub export_keys: Vec<String>,
    pub include_tongue: bool,
}

impl BlendFilter {
    pub fn filter(&self, values: &[f64; 52]) -> Option<Vec<(String, f64)>> {
        let profile = self.profile.trim().to_ascii_lowercase();
        if matches!(profile.as_str(), "none" | "off" | "0" | "false" | "no") {
            return None;
        }
        if profile.is_empty() || matches!(profile.as_str(), "full" | "all") {
            return Some(Self::collect(|_| true, values));
        }
        if profile == "mouth" {
            let mut filtered = Self::collect(
                |name| name.starts_with("mouth") || name.starts_with("jaw"),
                values,
            );
            if self.include_tongue {
                if let Some(index) = blend_index("tongueOut") {
                    filtered.push(("tongueOut".to_string(), values[index]));
                }
            }
            return (!filtered.is_empty()).then_some(filtered);
        }
        if profile == "mediapipe" {
            let mut filtered = Self::collect(|name| name != "tongueOut", values);
            filtered.push(("_neutral".to_string(), 0.0));
            return Some(filtered);
        }
        if profile == "custom" {
            let mut filtered = Vec::with_capacity(self.export_keys.len());
            for key in &self.export_keys {
                if let Some(index) = blend_index(key) {
                    filtered.push((key.clone(), values[index]));
                }
            }
            return (!filtered.is_empty()).then_some(filtered);
        }
        Some(Self::collect(|_| true, values))
    }

    /// Ne matérialise que les entrées retenues : un profil `none` ou `custom`
    /// ne paie plus les 52 allocations de noms par frame.
    fn collect(predicate: impl Fn(&str) -> bool, values: &[f64; 52]) -> Vec<(String, f64)> {
        ARKIT_52_BLENDSHAPE_NAMES
            .iter()
            .enumerate()
            .filter(|(_, name)| predicate(name))
            .map(|(index, name)| ((*name).to_string(), values[index]))
            .collect()
    }
}

fn blend_index(name: &str) -> Option<usize> {
    ARKIT_52_BLENDSHAPE_NAMES
        .iter()
        .position(|candidate| *candidate == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_profile_keeps_arkit_order() {
        let filter = BlendFilter {
            profile: "full".into(),
            export_keys: Vec::new(),
            include_tongue: false,
        };
        let values = [0.5; 52];
        let output = filter.filter(&values).unwrap();
        assert_eq!(output.len(), 52);
        assert_eq!(output[0].0, "eyeBlinkLeft");
        assert_eq!(output[51].0, "tongueOut");
    }

    #[test]
    fn custom_profile_uses_requested_order() {
        let filter = BlendFilter {
            profile: "custom".into(),
            export_keys: vec!["jawOpen".into(), "eyeBlinkLeft".into()],
            include_tongue: false,
        };
        let values = [0.25; 52];
        let output = filter.filter(&values).unwrap();
        assert_eq!(output[0].0, "jawOpen");
        assert_eq!(output[1].0, "eyeBlinkLeft");
    }
}
