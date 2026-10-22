//! Fenêtrage TransNetV2 et conversion des probabilités en scènes.
//!
//! Le découpage reproduit exactement le padding global de la production :
//! 25 copies de la première trame, 25 copies de la dernière, puis des fenêtres
//! de 100 trames tous les 50 pas. Les prédictions utiles sont la tranche
//! centrale `[padding, window - padding)`.

use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::fs;
use std::path::Path;

pub const FRAME_HEIGHT: usize = 27;
pub const FRAME_WIDTH: usize = 48;
pub const FRAME_CHANNELS: usize = 3;
pub const FRAME_BYTES: usize = FRAME_HEIGHT * FRAME_WIDTH * FRAME_CHANNELS;
pub const FPS_TARGET: f64 = 25.0;
pub const DECISION_THRESHOLD: f32 = 0.5;

pub type Scene = (usize, usize);

/// Générateur incrémental de fenêtres, avec un tampon de 100 à 150 trames.
pub struct WindowStream {
    window: usize,
    stride: usize,
    padding: usize,
    buffer: VecDeque<Vec<u8>>,
    first_frame: Option<Vec<u8>>,
    last_frame: Option<Vec<u8>>,
    raw_frames: u64,
    emitted: usize,
}

impl WindowStream {
    pub fn new(window: usize, stride: usize, padding: usize) -> Self {
        Self {
            window,
            stride,
            padding,
            buffer: VecDeque::with_capacity(window + stride),
            first_frame: None,
            last_frame: None,
            raw_frames: 0,
            emitted: 0,
        }
    }

    pub fn push_frame(&mut self, frame: Vec<u8>) {
        debug_assert_eq!(frame.len(), FRAME_BYTES);
        if self.first_frame.is_none() {
            self.first_frame = Some(frame.clone());
            for _ in 0..self.padding {
                self.buffer.push_back(frame.clone());
            }
        }
        self.last_frame = Some(frame.clone());
        self.buffer.push_back(frame);
        self.raw_frames += 1;
    }

    /// Émet les fenêtres complètes. `eof` ajoute le padding droit et produit
    /// les dernières fenêtres.
    pub fn drain(&mut self, eof: bool) -> Vec<Vec<u8>> {
        let mut windows = Vec::new();
        if self.raw_frames == 0 {
            return windows;
        }
        let total_windows = if eof {
            self.raw_frames.div_ceil(self.stride as u64) as usize
        } else {
            usize::MAX
        };

        while self.buffer.len() >= self.window {
            if eof && self.emitted >= total_windows {
                break;
            }
            windows.push(flatten_window(&self.buffer, self.window));
            for _ in 0..self.stride {
                self.buffer.pop_front();
            }
            self.emitted += 1;
        }

        if eof {
            while self.emitted < total_windows {
                let Some(last) = self.last_frame.clone() else {
                    break;
                };
                while self.buffer.len() < self.window {
                    self.buffer.push_back(last.clone());
                }
                windows.push(flatten_window(&self.buffer, self.window));
                for _ in 0..self.stride {
                    self.buffer.pop_front();
                }
                self.emitted += 1;
            }
        }
        windows
    }

    pub fn raw_frames(&self) -> u64 {
        self.raw_frames
    }

    pub fn emitted_windows(&self) -> usize {
        self.emitted
    }
}

fn flatten_window(buffer: &VecDeque<Vec<u8>>, window: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(window * FRAME_BYTES);
    for frame in buffer.iter().take(window) {
        debug_assert_eq!(frame.len(), FRAME_BYTES);
        data.extend_from_slice(frame);
    }
    data
}

/// Référence non incrémentale, utilisée par les tests de parité du fenêtrage.
pub fn reference_windows(
    frames: &[Vec<u8>],
    window: usize,
    stride: usize,
    padding: usize,
) -> Vec<Vec<u8>> {
    if frames.is_empty() {
        return Vec::new();
    }
    let remainder = frames.len() % stride;
    let end_padding = padding + stride - if remainder == 0 { stride } else { remainder };
    let mut padded = Vec::with_capacity(padding + frames.len() + end_padding);
    for _ in 0..padding {
        padded.push(frames[0].clone());
    }
    padded.extend(frames.iter().cloned());
    for _ in 0..end_padding {
        padded.push(frames.last().unwrap().clone());
    }

    let mut windows = Vec::new();
    let mut start = 0;
    while start + window <= padded.len() {
        let mut data = Vec::with_capacity(window * FRAME_BYTES);
        for frame in &padded[start..start + window] {
            data.extend_from_slice(frame);
        }
        windows.push(data);
        start += stride;
    }
    windows
}

/// Conversion des probabilités par trame en segments inclusifs.
pub fn predictions_to_scenes(predictions: &[f32], threshold: f32) -> Vec<Scene> {
    if predictions.is_empty() {
        return Vec::new();
    }
    let boundaries: Vec<usize> = predictions
        .iter()
        .enumerate()
        .filter_map(|(index, probability)| (*probability > threshold).then_some(index))
        .collect();
    if boundaries.is_empty() {
        return vec![(0, predictions.len() - 1)];
    }

    let mut scenes = Vec::new();
    let mut last_cut: isize = -1;
    for cut in boundaries {
        if cut as isize > last_cut {
            scenes.push(((last_cut + 1) as usize, cut));
        }
        last_cut = cut as isize;
    }
    if (last_cut as usize) < predictions.len() - 1 {
        scenes.push(((last_cut + 1) as usize, predictions.len() - 1));
    }
    scenes
}

/// Timecode `HH:MM:SS.mmm`, arrondi au millième comme `FrameTimecode`.
pub fn frame_timecode(frame: usize, fps: f64) -> String {
    let total_millis = (frame as f64 * 1000.0 / fps).round() as u64;
    let hours = total_millis / 3_600_000;
    let minutes = (total_millis % 3_600_000) / 60_000;
    let seconds = (total_millis % 60_000) / 1000;
    let millis = total_millis % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

pub fn write_scenes_csv(path: &Path, scenes: &[Scene], fps: f64) -> Result<()> {
    let mut text = String::from("No,Timecode In,Timecode Out,Frame In,Frame Out\r\n");
    for (index, (start, end)) in scenes.iter().enumerate() {
        text.push_str(&format!(
            "{},{},{},{},{}\r\n",
            index + 1,
            frame_timecode(*start, fps),
            frame_timecode(*end, fps),
            start + 1,
            end + 1
        ));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("création de {}", parent.display()))?;
    }
    let temp = path.with_extension("csv.tmp");
    fs::write(&temp, text).with_context(|| format!("écriture de {}", temp.display()))?;
    // `rename` remplace la cible sous Unix, mais pas sous Windows.
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("remplacement de {}", path.display()))?;
    }
    fs::rename(&temp, path).with_context(|| format!("publication de {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(count: usize) -> Vec<Vec<u8>> {
        (0..count)
            .map(|index| vec![index as u8; FRAME_BYTES])
            .collect()
    }

    fn streamed(frames: &[Vec<u8>], window: usize, stride: usize, padding: usize) -> Vec<Vec<u8>> {
        let mut stream = WindowStream::new(window, stride, padding);
        let mut windows = Vec::new();
        for frame in frames {
            stream.push_frame(frame.clone());
            windows.extend(stream.drain(false));
        }
        windows.extend(stream.drain(true));
        windows
    }

    #[test]
    fn window_stream_matches_global_reference_for_short_videos() {
        for count in 0..=300 {
            let frames = frames(count);
            let expected = reference_windows(&frames, 100, 50, 25);
            let actual = streamed(&frames, 100, 50, 25);
            assert_eq!(actual, expected, "N={count}");
        }
    }

    #[test]
    fn window_stream_matches_reference_across_pipeline_boundaries() {
        for count in [2_499, 2_500, 2_501, 2_524, 2_525, 2_600, 4_920] {
            let frames = frames(count);
            let expected = reference_windows(&frames, 100, 50, 25);
            let actual = streamed(&frames, 100, 50, 25);
            assert_eq!(actual, expected, "N={count}");
        }
    }

    #[test]
    fn scene_conversion_handles_no_cut_first_cut_and_last_frame() {
        assert!(predictions_to_scenes(&[], 0.5).is_empty());
        assert_eq!(predictions_to_scenes(&[0.1, 0.2, 0.3], 0.5), vec![(0, 2)]);
        assert_eq!(
            predictions_to_scenes(&[0.9, 0.1, 0.8], 0.5),
            vec![(0, 0), (1, 2)]
        );
        assert_eq!(
            predictions_to_scenes(&[0.1, 0.9, 0.1], 0.5),
            vec![(0, 1), (2, 2)]
        );
        assert_eq!(
            predictions_to_scenes(&[0.5, 0.1], 0.5),
            vec![(0, 1)],
            "la comparaison Python est strictement supérieure"
        );
    }

    #[test]
    fn timecodes_match_production_format() {
        assert_eq!(frame_timecode(0, FPS_TARGET), "00:00:00.000");
        assert_eq!(frame_timecode(54, FPS_TARGET), "00:00:02.160");
        assert_eq!(frame_timecode(4920, FPS_TARGET), "00:03:16.800");
    }

    #[test]
    fn csv_uses_crlf_and_frame_offsets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.csv");
        write_scenes_csv(&path, &[(0, 53), (54, 55)], FPS_TARGET).unwrap();
        let text = fs::read_to_string(path).unwrap();
        assert!(text.starts_with("No,Timecode In,Timecode Out,Frame In,Frame Out\r\n"));
        assert!(text.contains("1,00:00:00.000,00:00:02.120,1,54\r\n"));
        assert!(text.contains("2,00:00:02.160,00:00:02.200,55,56\r\n"));
    }
}
