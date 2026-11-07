//! Moteurs PP-OCRv6 (détection DB + reconnaissance CTC) sur ONNX Runtime CPU.
//!
//! Le module est neutre : il ne connaît ni la bande basse du QA ni les
//! candidats A2. Les orchestrateurs fournissent une image BGR et interprètent
//! les boîtes retournées.

use crate::vision::frame::{resize_bilinear, BgrFrame};
use crate::vision::inference::cpu_session;
use anyhow::{ensure, Context, Result};
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

pub const OCR_DET_FILENAME: &str = "PP-OCRv6_det_small.onnx";
pub const OCR_REC_FILENAME: &str = "PP-OCRv6_rec_small.onnx";
pub const OCR_LICENSE: &str = "Apache-2.0";

/// Taille minimale de la détection OCR. Les bandes 1080×480 (sous-titres)
/// et 1080×614 (logos) restent à leur résolution native : le passage à 736
/// (valeur du PoC Python) fusionne les boîtes des logos et dégrade le rappel
/// (D-045).
const DET_LIMIT_SIDE: f64 = 480.0;
const DET_THRESH: f32 = 0.3;
const DET_BOX_THRESH: f32 = 0.5;
const DET_UNCLIP_RATIO: f64 = 1.6;
const DET_MIN_SIZE: f64 = 3.0;
const DET_MAX_CANDIDATES: usize = 1000;

const REC_HEIGHT: usize = 48;
const REC_BASE_WIDTH: usize = 320;
const REC_BATCH: usize = 6;

#[derive(Debug, Clone, Copy)]
pub(crate) struct DetBox {
    pub(crate) bbox: [i64; 4],
}

pub(crate) struct DetEngine {
    session: Session,
    output_name: String,
}

impl DetEngine {
    pub(crate) fn load(
        model: &Path,
        onnxruntime: Option<&Path>,
        intra_threads: Option<usize>,
    ) -> Result<Self> {
        let session = cpu_session(model, onnxruntime, intra_threads)
            .with_context(|| format!("ouverture du modèle OCR det {}", model.display()))?;
        ensure!(
            session.inputs.len() == 1,
            "PP-OCR det doit avoir une entrée, trouvé {}",
            session.inputs.len()
        );
        ensure!(
            session.outputs.len() == 1,
            "PP-OCR det doit avoir une sortie, trouvé {}",
            session.outputs.len()
        );
        let output_name = session.outputs[0].name.clone();
        Ok(Self {
            session,
            output_name,
        })
    }

    pub(crate) fn detect(&mut self, image: &BgrFrame) -> Result<Vec<DetBox>> {
        let (det_width, det_height) = det_input_size(image.width, image.height);
        let resized = resize_bilinear(image, det_width, det_height);
        let input = normalized_chw(&resized);
        let tensor = Tensor::from_array(([1usize, 3, det_height, det_width], input))
            .context("création du tenseur PP-OCR det")?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .context("inférence PP-OCR det")?;
        let (shape, data) = outputs
            .get(&self.output_name)
            .with_context(|| format!("sortie OCR det absente: {}", self.output_name))?
            .try_extract_tensor::<f32>()
            .context("lecture de la sortie PP-OCR det")?;
        ensure!(
            shape.len() == 4 && shape[1] == 1,
            "sortie OCR det inattendue: {:?}",
            shape
        );
        let height = shape[2] as usize;
        let width = shape[3] as usize;
        ensure!(
            data.len() >= height * width,
            "sortie OCR det: {} valeurs pour {}x{}",
            data.len(),
            width,
            height
        );
        let prob = &data[..height * width];
        let mask = prob
            .iter()
            .map(|value| *value > DET_THRESH)
            .collect::<Vec<_>>();
        let mask = dilate(&mask, width, height);
        let components = components(&mask, width, height);
        let scale_x = image.width as f64 / width as f64;
        let scale_y = image.height as f64 / height as f64;
        let mut boxes = Vec::new();
        for component in components.into_iter().take(DET_MAX_CANDIDATES) {
            let box_width = component.width as f64;
            let box_height = component.height as f64;
            if box_width.min(box_height) < DET_MIN_SIZE {
                continue;
            }
            let score = box_score(prob, width, &component);
            if score < DET_BOX_THRESH {
                continue;
            }
            let distance =
                box_width * box_height * DET_UNCLIP_RATIO / (2.0 * (box_width + box_height));
            let x1 = round_half_even((component.x0 as f64 - distance) * scale_x)
                .clamp(0.0, image.width as f64 - 1.0) as i64;
            let y1 = round_half_even((component.y0 as f64 - distance) * scale_y)
                .clamp(0.0, image.height as f64 - 1.0) as i64;
            let x2 = round_half_even((component.x0 as f64 + box_width + distance) * scale_x)
                .clamp(0.0, image.width as f64 - 1.0) as i64;
            let y2 = round_half_even((component.y0 as f64 + box_height + distance) * scale_y)
                .clamp(0.0, image.height as f64 - 1.0) as i64;
            // `filter_det_res` de RapidOCR écarte les boîtes scalées dont un
            // côté fait 3 px ou moins.
            if x2 - x1 <= 3 || y2 - y1 <= 3 {
                continue;
            }
            boxes.push(DetBox {
                bbox: [x1, y1, x2, y2],
            });
        }
        Ok(boxes)
    }
}

fn det_input_size(width: usize, height: usize) -> (usize, usize) {
    let min_side = width.min(height) as f64;
    let ratio = if min_side < DET_LIMIT_SIDE {
        DET_LIMIT_SIDE / min_side
    } else {
        1.0
    };
    let resized_height = (height as f64 * ratio) as usize;
    let resized_width = (width as f64 * ratio) as usize;
    let aligned_height = (round_half_even(resized_height as f64 / 32.0) as usize * 32).max(32);
    let aligned_width = (round_half_even(resized_width as f64 / 32.0) as usize * 32).max(32);
    (aligned_width, aligned_height)
}

fn round_half_even(value: f64) -> f64 {
    value.round_ties_even()
}

fn normalized_chw(image: &BgrFrame) -> Vec<f32> {
    let plane = image.width * image.height;
    let mut input = vec![0.0f32; 3 * plane];
    for y in 0..image.height {
        for x in 0..image.width {
            let pixel = image.pixel(x, y);
            let offset = y * image.width + x;
            for channel in 0..3 {
                // mean = std = 0.5, échelle 1/255 : (v/255 - 0.5) / 0.5.
                input[channel * plane + offset] = f32::from(pixel[channel]) / 127.5 - 1.0;
            }
        }
    }
    input
}

struct Component {
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
}

fn dilate(mask: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut dilated = vec![false; mask.len()];
    for y in 0..height {
        for x in 0..width {
            if !mask[y * width + x] {
                continue;
            }
            // cv2.dilate avec un noyau 2×2 et l'ancre par défaut (0,0) étend
            // le pixel vers la droite et le bas.
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let nx = x + dx;
                let ny = y + dy;
                if nx < width && ny < height {
                    dilated[ny * width + nx] = true;
                }
            }
        }
    }
    dilated
}

/// Composantes 8-connexes de la carte binaire, équivalent de `findContours`
/// pour des lignes de texte sans trou.
fn components(mask: &[bool], width: usize, height: usize) -> Vec<Component> {
    let mut visited = vec![false; mask.len()];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            if !mask[index] || visited[index] {
                continue;
            }
            visited[index] = true;
            stack.push((x, y));
            let (mut x0, mut y0, mut x1, mut y1) = (x, y, x, y);
            while let Some((current_x, current_y)) = stack.pop() {
                x0 = x0.min(current_x);
                y0 = y0.min(current_y);
                x1 = x1.max(current_x);
                y1 = y1.max(current_y);
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let nx = current_x as i64 + dx;
                        let ny = current_y as i64 + dy;
                        if nx < 0 || ny < 0 || nx >= width as i64 || ny >= height as i64 {
                            continue;
                        }
                        let neighbor = ny as usize * width + nx as usize;
                        if mask[neighbor] && !visited[neighbor] {
                            visited[neighbor] = true;
                            stack.push((nx as usize, ny as usize));
                        }
                    }
                }
            }
            components.push(Component {
                x0,
                y0,
                width: x1 - x0 + 1,
                height: y1 - y0 + 1,
            });
        }
    }
    components
}

/// Moyenne de la carte de probabilité sur la boîte, comme `box_score_fast`.
fn box_score(prob: &[f32], width: usize, component: &Component) -> f32 {
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for y in component.y0..component.y0 + component.height {
        for x in component.x0..component.x0 + component.width {
            sum += f64::from(prob[y * width + x]);
            count += 1;
        }
    }
    if count == 0 {
        0.0
    } else {
        (sum / count as f64) as f32
    }
}

pub(crate) struct RecEngine {
    session: Session,
    output_name: String,
    characters: Vec<String>,
}

impl RecEngine {
    pub(crate) fn load(
        model: &Path,
        onnxruntime: Option<&Path>,
        intra_threads: Option<usize>,
    ) -> Result<Self> {
        let session = cpu_session(model, onnxruntime, intra_threads)
            .with_context(|| format!("ouverture du modèle OCR rec {}", model.display()))?;
        ensure!(
            session.outputs.len() == 1,
            "PP-OCR rec doit avoir une sortie, trouvé {}",
            session.outputs.len()
        );
        let output_name = session.outputs[0].name.clone();
        let characters = {
            let metadata = session
                .metadata()
                .with_context(|| format!("métadonnées OCR rec {}", model.display()))?;
            metadata
                .custom("character")
                .with_context(|| format!("métadonnées OCR rec {}", model.display()))?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "le modèle OCR rec {} n'embarque pas la liste de caractères",
                        model.display()
                    )
                })?
        };
        let characters = characters.strip_suffix('\n').unwrap_or(characters.as_str());
        let mut characters = characters
            .split('\n')
            .map(str::to_string)
            .collect::<Vec<_>>();
        // RapidOCR ajoute l'espace puis le blank : l'index 0 est le blank et
        // le dernier l'espace.
        characters.push(" ".into());
        characters.insert(0, "blank".into());
        Ok(Self {
            session,
            output_name,
            characters,
        })
    }

    pub(crate) fn recognize(
        &mut self,
        image: &BgrFrame,
        boxes: &[DetBox],
    ) -> Result<Vec<(String, f64)>> {
        let mut results = Vec::with_capacity(boxes.len());
        for chunk in boxes.chunks(REC_BATCH) {
            let ratios = chunk
                .iter()
                .map(|det_box| {
                    let [x1, y1, x2, y2] = det_box.bbox;
                    (x2 - x1) as f64 / (y2 - y1).max(1) as f64
                })
                .collect::<Vec<_>>();
            // RapidOCR calcule la largeur cible sur le lot entier ; on la
            // calcule par paquet de `REC_BATCH` pour borner la mémoire. Les
            // boîtes plus étroites sont complétées par des zéros, comme un
            // padding après normalisation.
            let max_ratio = ratios
                .iter()
                .copied()
                .fold(REC_BASE_WIDTH as f64 / REC_HEIGHT as f64, f64::max);
            let target_width = (REC_HEIGHT as f64 * max_ratio) as usize;
            let mut batch = vec![0.0f32; chunk.len() * 3 * REC_HEIGHT * target_width];
            for (index, det_box) in chunk.iter().enumerate() {
                let resized_width =
                    ((REC_HEIGHT as f64 * ratios[index]).ceil() as usize).min(target_width);
                let crop = crop_box(image, det_box.bbox);
                let resized = resize_bilinear(&crop, resized_width.max(1), REC_HEIGHT);
                let plane = REC_HEIGHT * target_width;
                for y in 0..REC_HEIGHT {
                    for x in 0..resized.width {
                        let pixel = resized.pixel(x, y);
                        let offset = y * target_width + x;
                        for channel in 0..3 {
                            batch[index * 3 * plane + channel * plane + offset] =
                                f32::from(pixel[channel]) / 127.5 - 1.0;
                        }
                    }
                }
            }
            let tensor = Tensor::from_array(([chunk.len(), 3, REC_HEIGHT, target_width], batch))
                .context("création du tenseur PP-OCR rec")?;
            let outputs = self
                .session
                .run(ort::inputs![tensor])
                .context("inférence PP-OCR rec")?;
            let (shape, data) = outputs
                .get(&self.output_name)
                .with_context(|| format!("sortie OCR rec absente: {}", self.output_name))?
                .try_extract_tensor::<f32>()
                .context("lecture de la sortie PP-OCR rec")?;
            ensure!(
                shape.len() == 3 && shape[0] as usize == chunk.len(),
                "sortie OCR rec inattendue: {:?}",
                shape
            );
            let steps = shape[1] as usize;
            let classes = shape[2] as usize;
            ensure!(
                classes == self.characters.len(),
                "OCR rec: {classes} classes pour {} caractères",
                self.characters.len()
            );
            for index in 0..chunk.len() {
                results.push(decode_ctc(
                    &data[index * steps * classes..(index + 1) * steps * classes],
                    steps,
                    classes,
                    &self.characters,
                ));
            }
        }
        Ok(results)
    }
}

fn crop_box(image: &BgrFrame, bbox: [i64; 4]) -> BgrFrame {
    let [x1, y1, x2, y2] = bbox;
    let x1 = x1.clamp(0, image.width as i64 - 1) as usize;
    let y1 = y1.clamp(0, image.height as i64 - 1) as usize;
    let x2 = x2.clamp(x1 as i64 + 1, image.width as i64) as usize;
    let y2 = y2.clamp(y1 as i64 + 1, image.height as i64) as usize;
    image.crop(x1, y1, x2 - x1, y2 - y1)
}

fn decode_ctc(data: &[f32], steps: usize, classes: usize, characters: &[String]) -> (String, f64) {
    let mut text = String::new();
    let mut score_sum = 0.0f64;
    let mut score_count = 0usize;
    let mut previous = usize::MAX;
    for step in 0..steps {
        let row = &data[step * classes..(step + 1) * classes];
        let mut best_index = 0usize;
        let mut best_prob = row[0];
        for (index, prob) in row.iter().enumerate().skip(1) {
            if *prob > best_prob {
                best_prob = *prob;
                best_index = index;
            }
        }
        if best_index != 0 && best_index != previous {
            text.push_str(&characters[best_index]);
            score_sum += f64::from(best_prob);
            score_count += 1;
        }
        previous = best_index;
    }
    let score = if score_count == 0 {
        0.0
    } else {
        score_sum / score_count as f64
    };
    (text, score)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn det_size_aligns_to_32_and_preserves_aspect() {
        let (width, height) = det_input_size(1080, 480);
        assert_eq!(height, 480);
        assert_eq!(width, 1088);
        assert_eq!(width % 32, 0);
        assert!((width as f64 / height as f64 - 1080.0 / 480.0).abs() < 0.05);
    }

    #[test]
    fn det_size_leaves_the_logo_band_native() {
        // Bande haute 1080×614 : 1088×608, sans sur-échantillonnage.
        assert_eq!(det_input_size(1080, 614), (1088, 608));
    }

    #[test]
    fn det_size_leaves_a_full_frame_unscaled() {
        let (width, height) = det_input_size(1080, 1920);
        assert_eq!((width, height), (1088, 1920));
    }

    #[test]
    fn dilate_expands_right_and_bottom() {
        let mut mask = vec![false; 16];
        mask[5] = true;
        let dilated = dilate(&mask, 4, 4);
        assert!(dilated[5]);
        assert!(dilated[6]);
        assert!(dilated[9]);
        assert!(dilated[10]);
        assert!(!dilated[0]);
    }

    #[test]
    fn components_groups_adjacent_pixels() {
        let mask = vec![
            true, true, false, false, true, true, false, false, false, false, false, false, false,
            false, false, false,
        ];
        let components = components(&mask, 4, 4);
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].x0, 0);
        assert_eq!(components[0].width, 2);
        assert_eq!(components[0].height, 2);
    }

    #[test]
    fn ctc_decode_collapses_repeats_and_drops_blank() {
        let characters = vec!["blank".to_string(), "A".to_string(), "B".to_string()];
        // 4 pas, 3 classes : A A blank B.
        let data = vec![0.0, 0.9, 0.1, 0.0, 0.8, 0.2, 0.9, 0.0, 0.1, 0.0, 0.1, 0.9];
        let (text, score) = decode_ctc(&data, 4, 3, &characters);
        assert_eq!(text, "AB");
        assert!((score - 0.9).abs() < 1e-6);
    }

    #[test]
    fn ctc_decode_returns_an_empty_text_when_only_blanks() {
        let characters = vec!["blank".to_string(), "A".to_string()];
        let data = vec![0.9, 0.1, 0.9, 0.1];
        let (text, score) = decode_ctc(&data, 2, 2, &characters);
        assert!(text.is_empty());
        assert_eq!(score, 0.0);
    }

    #[test]
    fn components_separate_disconnected_blobs() {
        let mut mask = vec![false; 16];
        mask[0] = true;
        mask[10] = true;
        let components = components(&mask, 4, 4);
        assert_eq!(components.len(), 2);
    }
}
