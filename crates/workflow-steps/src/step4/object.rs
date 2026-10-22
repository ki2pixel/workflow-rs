use anyhow::{bail, Context, Result};
use workflow_tpu::Session;

use super::engine::{f32_outputs, run_model_nearest};
use super::image::RgbImage;

pub const EFFICIENTDET_LABELS: [&str; 91] = [
    "???",
    "person",
    "bicycle",
    "car",
    "motorcycle",
    "airplane",
    "bus",
    "train",
    "truck",
    "boat",
    "traffic light",
    "fire hydrant",
    "???",
    "stop sign",
    "parking meter",
    "bench",
    "bird",
    "cat",
    "dog",
    "horse",
    "sheep",
    "cow",
    "elephant",
    "bear",
    "zebra",
    "giraffe",
    "???",
    "backpack",
    "umbrella",
    "???",
    "???",
    "handbag",
    "tie",
    "suitcase",
    "frisbee",
    "skis",
    "snowboard",
    "sports ball",
    "kite",
    "baseball bat",
    "baseball glove",
    "skateboard",
    "surfboard",
    "tennis racket",
    "bottle",
    "???",
    "wine glass",
    "cup",
    "fork",
    "knife",
    "spoon",
    "bowl",
    "banana",
    "apple",
    "sandwich",
    "orange",
    "broccoli",
    "carrot",
    "hot dog",
    "pizza",
    "donut",
    "cake",
    "chair",
    "couch",
    "potted plant",
    "bed",
    "???",
    "dining table",
    "???",
    "???",
    "toilet",
    "???",
    "tv",
    "laptop",
    "mouse",
    "remote",
    "keyboard",
    "cell phone",
    "microwave",
    "oven",
    "toaster",
    "sink",
    "refrigerator",
    "???",
    "book",
    "clock",
    "vase",
    "scissors",
    "teddy bear",
    "hair drier",
    "toothbrush",
];

#[derive(Debug, Clone)]
pub struct ObjectDetection {
    pub centroid: (i64, i64),
    pub bbox: (i64, i64, i64, i64),
    pub label: String,
    pub confidence: f64,
}

pub fn label_for_class_index(class_index: i32) -> Option<&'static str> {
    let position = class_index + 1;
    if position < 0 || position as usize >= EFFICIENTDET_LABELS.len() {
        return None;
    }
    let label = EFFICIENTDET_LABELS[position as usize];
    (label != "???").then_some(label)
}

pub fn resolve_class_label(class_index: f32) -> String {
    let index = class_index.round() as i32;
    label_for_class_index(index)
        .map(str::to_string)
        .unwrap_or_else(|| format!("class_{index}"))
}

pub fn detect_objects(
    session: &mut Session,
    image: &RgbImage,
    width: usize,
    height: usize,
    score_threshold: f64,
    max_objects: usize,
) -> Result<Vec<ObjectDetection>> {
    run_model_nearest(session, image)?;
    let outputs = f32_outputs(session)?;
    if outputs.len() < 4 {
        bail!("détecteur d'objets: {} sorties au lieu de 4", outputs.len());
    }
    let boxes = &outputs[0];
    let classes = &outputs[1];
    let scores = &outputs[2];
    let count = outputs[3]
        .first()
        .copied()
        .context("détecteur d'objets: compteur absent")?;
    let count = count.max(0.0) as usize;
    let count = count.min(boxes.len() / 4).min(10);
    if scores.len() < count || classes.len() < count {
        bail!(
            "détecteur d'objets: {count} détections annoncées, {} scores et {} classes",
            scores.len(),
            classes.len()
        );
    }

    let mut detections = Vec::new();
    for index in 0..count.min(max_objects) {
        let score = scores[index] as f64;
        if score < score_threshold {
            continue;
        }
        // TFLite Detection PostProcess : [ymin, xmin, ymax, xmax].
        let ymin = boxes[index * 4] as f64;
        let xmin = boxes[index * 4 + 1] as f64;
        let ymax = boxes[index * 4 + 2] as f64;
        let xmax = boxes[index * 4 + 3] as f64;

        let o_xmin = (xmin * width as f64) as i64;
        let o_ymin = (ymin * height as f64) as i64;
        let o_xmax = (xmax * width as f64) as i64;
        let o_ymax = (ymax * height as f64) as i64;
        let o_xmin = o_xmin.max(0);
        let o_ymin = o_ymin.max(0);
        let o_xmax = o_xmax.min(width as i64);
        let o_ymax = o_ymax.min(height as i64);
        let o_width = o_xmax - o_xmin;
        let o_height = o_ymax - o_ymin;
        let o_cx = o_xmin + o_width / 2;
        let o_cy = o_ymin + o_height / 2;

        detections.push(ObjectDetection {
            centroid: (o_cx, o_cy),
            bbox: (o_xmin, o_ymin, o_width, o_height),
            label: resolve_class_label(classes[index]),
            confidence: score,
        });
    }
    Ok(detections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_coco_model_indices() {
        assert_eq!(label_for_class_index(0), Some("person"));
        assert_eq!(label_for_class_index(61), Some("chair"));
        assert_eq!(label_for_class_index(66), Some("dining table"));
        assert_eq!(label_for_class_index(11), None);
        assert_eq!(resolve_class_label(11.0), "class_11");
    }
}
