//! Fenêtrage, filtre médian et machine à états du VAD STEP3.

use crate::step3::mel::{SAMPLE_RATE, YAMNET_SEGMENT_SAMPLES};
use anyhow::{bail, Result};

pub const DEFAULT_WINDOW_OVERLAP: f64 = 0.5;
pub const DEFAULT_VAD_THRESHOLD: f32 = 0.20;
pub const DEFAULT_MEDIAN_SIZE: usize = 5;
pub const DEFAULT_HANGOVER_SEC: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VadState {
    Silence,
    SpeechDetected,
    Hangover,
}

/// Calcule les débuts de fenêtres YAMNet, avec le dernier ancrage sur la fin
/// du signal lorsque le reste dépasse un quart de fenêtre.
pub fn window_starts(total_samples: usize, segment_size: usize, hop_size: usize) -> Vec<usize> {
    if segment_size == 0 || hop_size == 0 || total_samples < segment_size {
        return Vec::new();
    }
    let mut starts: Vec<usize> = (0..=total_samples - segment_size)
        .step_by(hop_size)
        .collect();
    if let Some(last) = starts.last().copied() {
        let remaining = total_samples - (last + segment_size);
        if remaining > segment_size / 4 {
            starts.push(total_samples - segment_size);
        }
    }
    starts
}

pub fn hop_for_overlap(segment_size: usize, overlap: f64) -> Result<usize> {
    if !(0.0..1.0).contains(&overlap) {
        bail!("hop_overlap doit être dans [0, 1[");
    }
    let hop = (segment_size as f64 * (1.0 - overlap)) as usize;
    if hop == 0 {
        bail!("hop_overlap produit un pas nul");
    }
    Ok(hop)
}

/// Reproduit le chemin générique de `apply_vad_postfilter` : médiane avec
/// remplissage nul, seuil strict, puis FSM hangover.
pub fn postfilter(
    probabilities: &[f32],
    threshold: f32,
    hop_sec: f64,
    hangover_sec: f64,
    median_size: usize,
) -> Vec<bool> {
    if probabilities.is_empty() {
        return Vec::new();
    }
    if probabilities.len() >= median_size && median_size == DEFAULT_MEDIAN_SIZE {
        return fused_medfilt5(
            probabilities,
            threshold,
            hangover_frames(hop_sec, hangover_sec),
        );
    }
    let filtered = median_filter(probabilities, median_size);
    let flags: Vec<bool> = filtered.iter().map(|value| *value > threshold).collect();
    apply_fsm(&flags, hop_sec, hangover_sec)
}

fn hangover_frames(hop_sec: f64, hangover_sec: f64) -> usize {
    if hop_sec <= 0.0 {
        return 1;
    }
    ((hangover_sec / hop_sec).ceil() as usize).max(1)
}

fn median_filter(values: &[f32], kernel_size: usize) -> Vec<f32> {
    if values.len() < kernel_size {
        return values.to_vec();
    }
    let kernel = if kernel_size % 2 == 0 {
        kernel_size + 1
    } else {
        kernel_size
    };
    let half = kernel / 2;
    let mut output = vec![0.0_f32; values.len()];
    let mut window = vec![0.0_f32; kernel];
    for (index, output_slot) in output.iter_mut().enumerate() {
        for (offset, slot) in window.iter_mut().enumerate() {
            let source = index as isize + offset as isize - half as isize;
            *slot = if source >= 0 && (source as usize) < values.len() {
                values[source as usize]
            } else {
                0.0
            };
        }
        window.sort_by(|a, b| a.total_cmp(b));
        *output_slot = window[half];
    }
    output
}

fn fused_medfilt5(raw_scores: &[f32], threshold: f32, hangover_frames: usize) -> Vec<bool> {
    let mut vad_mask = vec![false; raw_scores.len()];
    let mut hangover_counter = 0usize;
    let mut window = [0.0_f32; 5];
    for (index, mask_slot) in vad_mask.iter_mut().enumerate() {
        for offset in -2_i32..=2 {
            let source = index as i32 + offset;
            window[(offset + 2) as usize] = if source >= 0 && (source as usize) < raw_scores.len() {
                raw_scores[source as usize]
            } else {
                0.0
            };
        }
        window.sort_by(|a, b| a.total_cmp(b));
        let median = window[2];
        if median > threshold {
            *mask_slot = true;
            hangover_counter = hangover_frames;
        } else if hangover_counter > 0 {
            *mask_slot = true;
            hangover_counter -= 1;
        }
    }
    vad_mask
}

fn apply_fsm(flags: &[bool], hop_sec: f64, hangover_sec: f64) -> Vec<bool> {
    if flags.is_empty() {
        return Vec::new();
    }
    let limit = hangover_frames(hop_sec, hangover_sec);
    let mut state = VadState::Silence;
    let mut counter = 0usize;
    let mut output = Vec::with_capacity(flags.len());
    for flag in flags {
        match state {
            VadState::Silence => {
                if *flag {
                    state = VadState::SpeechDetected;
                    output.push(true);
                } else {
                    output.push(false);
                }
            }
            VadState::SpeechDetected => {
                if *flag {
                    output.push(true);
                } else {
                    state = VadState::Hangover;
                    counter = 0;
                    output.push(true);
                }
            }
            VadState::Hangover => {
                if *flag {
                    state = VadState::SpeechDetected;
                    counter = 0;
                    output.push(true);
                } else {
                    counter += 1;
                    if counter >= limit {
                        state = VadState::Silence;
                        output.push(false);
                    } else {
                        output.push(true);
                    }
                }
            }
        }
    }
    output
}

/// Construit les segments de parole à partir des fenêtres retenues.
pub fn speech_segments(starts: &[usize], mask: &[bool]) -> Vec<[f64; 2]> {
    let mut segments = Vec::new();
    for (index, selected) in mask.iter().enumerate() {
        if !selected || index >= starts.len() {
            continue;
        }
        let start = starts[index] as f64 / SAMPLE_RATE as f64;
        let end = (starts[index] + YAMNET_SEGMENT_SAMPLES) as f64 / SAMPLE_RATE as f64;
        segments.push([start, end]);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_starts_include_last_anchor() {
        assert_eq!(window_starts(15360, 15360, 7680), vec![0]);
        assert_eq!(window_starts(23040, 15360, 7680), vec![0, 7680]);
        assert!(window_starts(15359, 15360, 7680).is_empty());
    }

    #[test]
    fn median_uses_zero_padding_at_edges() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let filtered = median_filter(&values, 5);
        assert_eq!(filtered[0], 1.0);
        assert_eq!(filtered[1], 2.0);
        assert_eq!(filtered[5], 4.0);
    }

    #[test]
    fn fused_path_keeps_short_hangover() {
        let probs = [0.9_f32; 5];
        let mask = postfilter(&probs, 0.2, 0.48, 1.0, 5);
        assert_eq!(mask, vec![true, true, true, true, true]);
    }

    #[test]
    fn empty_signal_is_silent() {
        assert!(postfilter(&[], 0.2, 0.48, 1.0, 5).is_empty());
        assert!(speech_segments(&[], &[]).is_empty());
    }

    #[test]
    fn overlap_rejects_one() {
        assert!(hop_for_overlap(15360, 1.0).is_err());
        assert_eq!(hop_for_overlap(15360, 0.5).unwrap(), 7680);
    }
}
