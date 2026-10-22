use anyhow::{bail, Context, Result};
use std::sync::OnceLock;
use workflow_tpu::Session;

use super::engine::{f32_output, f32_outputs, run_float_input, run_model_linear};
use super::image::RgbImage;

pub const ARKIT_52_BLENDSHAPE_NAMES: [&str; 52] = [
    "eyeBlinkLeft",
    "eyeLookDownLeft",
    "eyeLookInLeft",
    "eyeLookOutLeft",
    "eyeLookUpLeft",
    "eyeSquintLeft",
    "eyeWideLeft",
    "eyeBlinkRight",
    "eyeLookDownRight",
    "eyeLookInRight",
    "eyeLookOutRight",
    "eyeLookUpRight",
    "eyeSquintRight",
    "eyeWideRight",
    "jawForward",
    "jawLeft",
    "jawRight",
    "jawOpen",
    "mouthClose",
    "mouthFunnel",
    "mouthPucker",
    "mouthLeft",
    "mouthRight",
    "mouthSmileLeft",
    "mouthSmileRight",
    "mouthFrownLeft",
    "mouthFrownRight",
    "mouthDimpleLeft",
    "mouthDimpleRight",
    "mouthStretchLeft",
    "mouthStretchRight",
    "mouthRollLower",
    "mouthRollUpper",
    "mouthShrugLower",
    "mouthShrugUpper",
    "mouthPressLeft",
    "mouthPressRight",
    "mouthLowerDownLeft",
    "mouthLowerDownRight",
    "mouthUpperUpLeft",
    "mouthUpperUpRight",
    "browDownLeft",
    "browDownRight",
    "browInnerUp",
    "browOuterUpLeft",
    "browOuterUpRight",
    "cheekPuff",
    "cheekSquintLeft",
    "cheekSquintRight",
    "noseSneerLeft",
    "noseSneerRight",
    "tongueOut",
];

const BLENDSHAPE_136_INDICES: [usize; 136] = [
    0, 1, 4, 5, 6, 7, 8, 10, 13, 14, 17, 21, 33, 37, 39, 40, 46, 52, 53, 54, 55, 58, 61, 63, 65,
    66, 67, 70, 78, 80, 81, 82, 84, 87, 88, 91, 93, 95, 103, 105, 107, 109, 127, 132, 133, 136,
    144, 145, 146, 148, 149, 150, 152, 153, 154, 155, 157, 158, 159, 160, 161, 162, 163, 168, 172,
    173, 176, 178, 181, 185, 191, 195, 197, 234, 246, 249, 251, 263, 267, 269, 270, 276, 282, 283,
    284, 285, 288, 291, 293, 295, 296, 297, 300, 308, 310, 311, 312, 314, 317, 318, 321, 323, 324,
    332, 334, 336, 338, 356, 361, 362, 365, 373, 374, 375, 377, 378, 379, 380, 381, 382, 384, 385,
    386, 387, 388, 389, 390, 397, 398, 400, 402, 405, 409, 415, 454, 466,
];

#[derive(Debug, Clone)]
pub struct FaceDetection {
    pub bbox: [f32; 4],
    pub right_eye: (f32, f32),
    pub left_eye: (f32, f32),
}

fn anchors() -> &'static Vec<[f32; 4]> {
    static ANCHORS: OnceLock<Vec<[f32; 4]>> = OnceLock::new();
    ANCHORS.get_or_init(|| {
        let mut anchors = Vec::with_capacity(896);
        for (stride, per_cell) in [(8usize, 2usize), (16, 6)] {
            let grid = 128 / stride;
            for y in 0..grid {
                let cy = (y as f64 + 0.5) / grid as f64;
                for x in 0..grid {
                    let cx = (x as f64 + 0.5) / grid as f64;
                    for _ in 0..per_cell {
                        anchors.push([cx as f32, cy as f32, 1.0, 1.0]);
                    }
                }
            }
        }
        anchors
    })
}

fn sigmoid_f32(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

fn decode_boxes(regressors: &[f32], anchors: &[[f32; 4]]) -> Vec<[f32; 8]> {
    let mut boxes = Vec::with_capacity(anchors.len());
    for (index, anchor) in anchors.iter().enumerate() {
        let offset = index * 16;
        let reg = &regressors[offset..offset + 16];
        let x_center = reg[0] / 128.0 + anchor[0];
        let y_center = reg[1] / 128.0 + anchor[1];
        let width = reg[2] / 128.0;
        let height = reg[3] / 128.0;
        let mut decoded = [0.0f32; 8];
        decoded[0] = x_center - width / 2.0;
        decoded[1] = y_center - height / 2.0;
        decoded[2] = x_center + width / 2.0;
        decoded[3] = y_center + height / 2.0;
        decoded[4] = reg[4] / 128.0 + anchor[0];
        decoded[5] = reg[5] / 128.0 + anchor[1];
        decoded[6] = reg[6] / 128.0 + anchor[0];
        decoded[7] = reg[7] / 128.0 + anchor[1];
        boxes.push(decoded);
    }
    boxes
}

fn nms(boxes: &[[f32; 8]], scores: &[f32], score_threshold: f32, iou_threshold: f32) -> Vec<usize> {
    let mut order: Vec<usize> = scores
        .iter()
        .enumerate()
        .filter_map(|(index, score)| (*score > score_threshold).then_some(index))
        .collect();
    order.sort_by(|left, right| {
        scores[*right]
            .partial_cmp(&scores[*left])
            .unwrap_or(std::cmp::Ordering::Equal)
            // NumPy `argsort()[::-1]` conserve l'ordre croissant des indices
            // pour les égalités, donc l'indice le plus grand passe en premier.
            .then_with(|| right.cmp(left))
    });

    let mut keep = Vec::new();
    let mut remaining = order;
    while let Some(first) = remaining.first().copied() {
        keep.push(first);
        let base = boxes[first];
        let base_area = (base[2] - base[0]).max(0.0) * (base[3] - base[1]).max(0.0);
        let mut next = Vec::with_capacity(remaining.len().saturating_sub(1));
        for index in remaining.iter().copied().skip(1) {
            let other = boxes[index];
            let xx1 = base[0].max(other[0]);
            let yy1 = base[1].max(other[1]);
            let xx2 = base[2].min(other[2]);
            let yy2 = base[3].min(other[3]);
            let width = (xx2 - xx1).max(0.0);
            let height = (yy2 - yy1).max(0.0);
            let intersection = width * height;
            let other_area = (other[2] - other[0]).max(0.0) * (other[3] - other[1]).max(0.0);
            let union = base_area + other_area - intersection;
            let iou = if union > 0.0 {
                intersection / union
            } else {
                0.0
            };
            if iou <= iou_threshold {
                next.push(index);
            }
        }
        remaining = next;
    }
    keep
}

/// BlazeFace 128x128 : une seule meilleure détection, comme le pipeline Python.
pub fn detect_face(session: &mut Session, image: &RgbImage) -> Result<Option<FaceDetection>> {
    run_model_linear(session, image)?;
    let outputs = f32_outputs(session)?;
    if outputs.len() < 2 {
        bail!("BlazeFace: moins de deux sorties");
    }

    let mut classifications: Option<&[f32]> = None;
    let mut regressions: Option<&[f32]> = None;
    for output in &outputs {
        if output.len() == 896 {
            classifications = Some(output);
        } else if output.len() == 896 * 16 {
            regressions = Some(output);
        }
    }
    // Les modèles Edge TPU peuvent inverser l'ordre : on retombe sur les tailles.
    let classifications = classifications.context("BlazeFace: sortie classification absente")?;
    let regressions = regressions.context("BlazeFace: sortie régression absente")?;

    let anchors = anchors();
    let scores: Vec<f32> = classifications
        .iter()
        .map(|value| sigmoid_f32(*value))
        .collect();
    let boxes = decode_boxes(regressions, anchors);
    let keep = nms(&boxes, &scores, 0.7, 0.3);
    let Some(best) = keep.first().copied() else {
        return Ok(None);
    };

    let box_data = boxes[best];
    Ok(Some(FaceDetection {
        bbox: [box_data[1], box_data[0], box_data[3], box_data[2]],
        right_eye: (box_data[4], box_data[5]),
        left_eye: (box_data[6], box_data[7]),
    }))
}

pub fn extract_landmarks(session: &mut Session, crop: &RgbImage) -> Result<Vec<[f32; 3]>> {
    run_model_linear(session, crop)?;
    let values = f32_output(session, 0)?;
    if values.len() < 468 * 3 {
        bail!(
            "FaceMesh: sortie de {} valeurs, 1404 attendues",
            values.len()
        );
    }
    let mut landmarks = Vec::with_capacity(468);
    for index in 0..468 {
        landmarks.push([
            values[index * 3],
            values[index * 3 + 1],
            values[index * 3 + 2],
        ]);
    }
    Ok(landmarks)
}

/// Applique le sous-ensemble 136 points + padding iris, puis sigmoïde f64.
pub fn extract_blendshapes(session: &mut Session, landmarks: &[[f32; 3]]) -> Result<[f64; 52]> {
    if landmarks.len() < 468 {
        bail!("Blendshapes: 468 landmarks attendus");
    }
    let mut values = Vec::with_capacity(146 * 2);
    for index in BLENDSHAPE_136_INDICES {
        values.push(landmarks[index][0]);
        values.push(landmarks[index][1]);
    }
    let left_iris = (
        (landmarks[386][0] + landmarks[374][0]) / 2.0,
        (landmarks[386][1] + landmarks[374][1]) / 2.0,
    );
    let right_iris = (
        (landmarks[159][0] + landmarks[145][0]) / 2.0,
        (landmarks[159][1] + landmarks[145][1]) / 2.0,
    );
    for _ in 0..5 {
        values.push(left_iris.0);
        values.push(left_iris.1);
    }
    for _ in 0..5 {
        values.push(right_iris.0);
        values.push(right_iris.1);
    }

    run_float_input(session, &values)?;
    let logits = f32_output(session, 0)?;
    if logits.len() < 52 {
        bail!("Blendshapes: sortie de {} valeurs", logits.len());
    }
    let mut result = [0.0f64; 52];
    for (index, logit) in logits.iter().take(52).enumerate() {
        // NumPy reste en float32 pour la sigmoïde ; la conversion en float64
        // n'intervient qu'à l'entrée du filtre One-Euro.
        let exp = (-*logit).exp();
        let score = 1.0f32 / (1.0f32 + exp);
        result[index] = f64::from(score);
    }
    Ok(result)
}

/// Les 136 indices doivent rester alignés avec le modèle MediaPipe.
pub fn blendshape_index_count() -> usize {
    BLENDSHAPE_136_INDICES.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_have_expected_layout() {
        let anchors = anchors();
        assert_eq!(anchors.len(), 896);
        assert!((anchors[0][0] - 0.03125).abs() < 1e-6);
        assert!((anchors[0][1] - 0.03125).abs() < 1e-6);
        assert!((anchors[512][0] - 0.0625).abs() < 1e-6);
        assert!((anchors[512][1] - 0.0625).abs() < 1e-6);
    }

    #[test]
    fn blendshape_indices_are_136() {
        assert_eq!(blendshape_index_count(), 136);
    }

    #[test]
    fn nms_keeps_best_box() {
        let boxes = vec![
            [0.0, 0.0, 10.0, 10.0, 0.0, 0.0, 0.0, 0.0],
            [1.0, 1.0, 9.0, 9.0, 0.0, 0.0, 0.0, 0.0],
            [20.0, 20.0, 30.0, 30.0, 0.0, 0.0, 0.0, 0.0],
        ];
        let scores = [0.9, 0.8, 0.7];
        let keep = nms(&boxes, &scores, 0.65, 0.3);
        assert_eq!(keep, vec![0, 2]);
    }
}
