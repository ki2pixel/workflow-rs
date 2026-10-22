//! Frontends log-mel de STEP3.
//!
//! Les deux modèles utilisent des grilles différentes, fixées par la
//! production Python :
//!
//! - YAMNet Split : 96 trames de 64 mel, STFT 512 sur une fenêtre de 400
//!   échantillons, hop 160 ;
//! - ECAPA-TDNN : 80 mel, STFT 400, fenêtre Hann périodique de 400,
//!   hop 160, 300 trames après normalisation moyenne.
//!
//! Les calculs reprennent l'ordre des opérations de `numpy.fft` (double) pour
//! YAMNet et de `scipy.fft` (float32) pour ECAPA. La matrice mel YAMNet est
//! chargée depuis le NPY versionné ; la matrice ECAPA est calculée avec la
//! formule HTK du script Python.

use anyhow::{bail, Context, Result};
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::path::Path;
use std::sync::Arc;

pub const SAMPLE_RATE: usize = 16_000;
pub const YAMNET_SEGMENT_SAMPLES: usize = 15_360;
pub const YAMNET_PATCH_FRAMES: usize = 96;
pub const YAMNET_MELS: usize = 64;
pub const YAMNET_FFT: usize = 512;
pub const YAMNET_FRAME_LENGTH: usize = 400;
pub const YAMNET_HOP: usize = 160;

pub const ECAPA_MELS: usize = 80;
pub const ECAPA_FFT: usize = 400;
pub const ECAPA_HOP: usize = 160;
pub const ECAPA_SEGMENT_SAMPLES: usize = 48_000;
pub const ECAPA_TARGET_FRAMES: usize = 300;

/// Matrice mel chargée depuis un fichier NPY float32 C-order.
#[derive(Debug, Clone)]
pub struct MelMatrix {
    rows: usize,
    cols: usize,
    data: Vec<f32>,
}

impl MelMatrix {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("lecture de la matrice mel {}", path.display()))?;
        parse_npy_f32(&bytes).with_context(|| format!("matrice mel invalide: {}", path.display()))
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    #[cfg(test)]
    fn row(&self, row: usize) -> &[f32] {
        let start = row * self.cols;
        &self.data[start..start + self.cols]
    }
}

fn parse_npy_f32(bytes: &[u8]) -> Result<MelMatrix> {
    const MAGIC: &[u8; 6] = b"\x93NUMPY";
    if bytes.len() < 10 || &bytes[..6] != MAGIC {
        bail!("signature NPY absente");
    }
    let major = bytes[6];
    let (header_len, header_start): (usize, usize) = match major {
        1 => {
            let len = u16::from_le_bytes([bytes[8], bytes[9]]) as usize;
            (len, 10)
        }
        2 | 3 => {
            if bytes.len() < 12 {
                bail!("en-tête NPY tronqué");
            }
            let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
            (len, 12)
        }
        other => bail!("version NPY non supportée: {other}"),
    };
    let header_end = header_start
        .checked_add(header_len)
        .context("longueur d'en-tête NPY débordante")?;
    let header = bytes
        .get(header_start..header_end)
        .context("en-tête NPY tronqué")?;
    let header = std::str::from_utf8(header).context("en-tête NPY non UTF-8")?;

    if !header.contains("'<f4'") && !header.contains("\"<f4\"") {
        bail!("NPY: type attendu <f4, en-tête={header}");
    }
    if !header.contains("'fortran_order': False") {
        bail!("NPY: ordre Fortran non supporté");
    }
    let shape_start = header.find("'shape':").context("NPY: clé shape absente")?;
    let parenthese = header[shape_start..]
        .find('(')
        .map(|offset| shape_start + offset)
        .context("NPY: shape invalide")?;
    let shape_end = header[parenthese..]
        .find(')')
        .map(|offset| parenthese + offset)
        .context("NPY: shape non fermée")?;
    let shape_text = &header[parenthese + 1..shape_end];
    let mut dimensions = Vec::new();
    for part in shape_text.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        dimensions.push(part.parse::<usize>().context("NPY: dimension invalide")?);
    }
    if dimensions.len() != 2 {
        bail!("NPY: matrice 2D attendue, shape={dimensions:?}");
    }
    let rows = dimensions[0];
    let cols = dimensions[1];
    let expected = rows
        .checked_mul(cols)
        .and_then(|value| value.checked_mul(4))
        .context("NPY: taille de matrice débordante")?;
    let payload = bytes.get(header_end..).context("NPY: données absentes")?;
    if payload.len() != expected {
        bail!(
            "NPY: {} octets de données pour {expected} attendus",
            payload.len()
        );
    }
    let data = payload
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    Ok(MelMatrix { rows, cols, data })
}

/// Frontend YAMNet Split.
pub struct YamnetFrontend {
    matrix: MelMatrix,
    window: Vec<f32>,
    fft: Arc<dyn Fft<f64>>,
}

impl YamnetFrontend {
    pub fn new(matrix: MelMatrix) -> Result<Self> {
        if matrix.rows() != YAMNET_FFT / 2 + 1 || matrix.cols() != YAMNET_MELS {
            bail!(
                "matrice mel YAMNet: shape ({}, {}) au lieu de ({}, {})",
                matrix.rows(),
                matrix.cols(),
                YAMNET_FFT / 2 + 1,
                YAMNET_MELS
            );
        }
        let window = (0..YAMNET_FRAME_LENGTH)
            .map(|index| {
                let angle = 2.0 * std::f64::consts::PI * index as f64 / YAMNET_FRAME_LENGTH as f64;
                (0.5 - 0.5 * angle.cos()) as f32
            })
            .collect();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(YAMNET_FFT);
        Ok(Self {
            matrix,
            window,
            fft,
        })
    }

    /// Calcule le log-mel continu d'un signal PCM16 déjà décodé à 16 kHz.
    pub fn log_mel(&self, samples: &[i16]) -> Vec<f32> {
        if samples.len() < YAMNET_FRAME_LENGTH {
            return Vec::new();
        }
        let frames = 1 + (samples.len() - YAMNET_FRAME_LENGTH) / YAMNET_HOP;
        let mut output = Vec::with_capacity(frames * YAMNET_MELS);
        let mut buffer = vec![Complex::<f64>::new(0.0, 0.0); YAMNET_FFT];
        let mut magnitudes = vec![0.0_f32; YAMNET_FFT / 2 + 1];
        for frame in 0..frames {
            let start = frame * YAMNET_HOP;
            buffer.fill(Complex::new(0.0, 0.0));
            for (index, value) in samples[start..start + YAMNET_FRAME_LENGTH]
                .iter()
                .enumerate()
            {
                let sample = f32::from(*value) / 32768.0;
                buffer[index].re = f64::from(sample * self.window[index]);
            }
            self.fft.process(&mut buffer);
            for (index, value) in buffer.iter().take(YAMNET_FFT / 2 + 1).enumerate() {
                magnitudes[index] = (value.re * value.re + value.im * value.im).sqrt() as f32;
            }
            for mel in 0..YAMNET_MELS {
                // Le NPY est (fréquence, mel) : chaque colonne est un filtre.
                let mut sum = 0.0_f32;
                for (bin, magnitude) in magnitudes.iter().enumerate() {
                    sum += self.matrix.data[bin * YAMNET_MELS + mel] * *magnitude;
                }
                output.push((sum + 0.001).ln());
            }
        }
        output
    }

    /// Découpe le log-mel continu en patchs (96, 64), avec répétition de la
    /// dernière trame disponible et zéros si la fenêtre est hors signal.
    pub fn patches(&self, log_mel: &[f32], window_starts: &[usize]) -> Vec<f32> {
        let total_frames = log_mel.len() / YAMNET_MELS;
        let mut patches = vec![0.0_f32; window_starts.len() * YAMNET_PATCH_FRAMES * YAMNET_MELS];
        for (patch_index, start_sample) in window_starts.iter().enumerate() {
            let start_frame = *start_sample / YAMNET_HOP;
            let destination = patch_index * YAMNET_PATCH_FRAMES * YAMNET_MELS;
            if start_frame >= total_frames {
                continue;
            }
            let available = (total_frames - start_frame).min(YAMNET_PATCH_FRAMES);
            let source =
                &log_mel[start_frame * YAMNET_MELS..(start_frame + available) * YAMNET_MELS];
            patches[destination..destination + source.len()].copy_from_slice(source);
            if available < YAMNET_PATCH_FRAMES {
                let last = &log_mel[(total_frames - 1) * YAMNET_MELS..total_frames * YAMNET_MELS];
                for frame in available..YAMNET_PATCH_FRAMES {
                    let start = destination + frame * YAMNET_MELS;
                    patches[start..start + YAMNET_MELS].copy_from_slice(last);
                }
            }
        }
        patches
    }
}

/// Frontend ECAPA-TDNN (log-mel 80 × 300, CMN).
pub struct EcapaFrontend {
    matrix: Vec<f32>,
    window: Vec<f32>,
    window_sum: f64,
    fft: Arc<dyn Fft<f32>>,
}

impl Default for EcapaFrontend {
    fn default() -> Self {
        Self::new()
    }
}

impl EcapaFrontend {
    pub fn new() -> Self {
        let n_freqs = ECAPA_FFT / 2 + 1;
        let matrix = create_ecapa_filterbank();
        assert_eq!(matrix.len(), ECAPA_MELS * n_freqs);
        let raw_window: Vec<f64> = (0..ECAPA_FFT)
            .map(|index| {
                if index == 0 {
                    0.0
                } else {
                    0.5 - 0.5 * (2.0 * std::f64::consts::PI * index as f64 / ECAPA_FFT as f64).cos()
                }
            })
            .collect();
        let window_sum: f64 = raw_window.iter().sum();
        let window = raw_window.iter().map(|value| *value as f32).collect();
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(ECAPA_FFT);
        Self {
            matrix,
            window,
            window_sum,
            fft,
        }
    }

    /// Calcule un tenseur `(80, 300)` pour un segment de 3 s.
    pub fn log_mel(&self, samples: &[i16]) -> Result<Vec<f32>> {
        if samples.len() < ECAPA_FFT {
            bail!(
                "segment trop court pour ECAPA: {} échantillons",
                samples.len()
            );
        }
        let raw_frames = 1 + (samples.len() - ECAPA_FFT) / ECAPA_HOP;
        let n_frames = raw_frames.min(ECAPA_TARGET_FRAMES);
        let scale = (1.0 / self.window_sum) as f32;
        let mut output = vec![(1e-10_f32).ln(); ECAPA_MELS * ECAPA_TARGET_FRAMES];
        let mut buffer = vec![Complex::<f32>::new(0.0, 0.0); ECAPA_FFT];
        let mut power = vec![0.0_f32; ECAPA_FFT / 2 + 1];
        for frame in 0..n_frames {
            let start = frame * ECAPA_HOP;
            buffer.fill(Complex::new(0.0, 0.0));
            for (index, value) in samples[start..start + ECAPA_FFT].iter().enumerate() {
                buffer[index].re = (f32::from(*value) / 32768.0) * self.window[index];
            }
            self.fft.process(&mut buffer);
            for (index, value) in buffer.iter().take(ECAPA_FFT / 2 + 1).enumerate() {
                let real = value.re * scale;
                let imag = value.im * scale;
                let magnitude = (real * real + imag * imag).sqrt();
                power[index] = magnitude * magnitude;
            }
            for mel in 0..ECAPA_MELS {
                let weights =
                    &self.matrix[mel * (ECAPA_FFT / 2 + 1)..(mel + 1) * (ECAPA_FFT / 2 + 1)];
                let mut sum = 0.0_f32;
                for (weight, value) in weights.iter().zip(power.iter()) {
                    sum += *weight * *value;
                }
                output[mel * ECAPA_TARGET_FRAMES + frame] = sum.max(1e-10).ln();
            }
        }
        // Les trames de padding restent à log(1e-10), comme `_pad_or_trim_mel`.
        for mel in 0..ECAPA_MELS {
            let slice = &mut output[mel * ECAPA_TARGET_FRAMES..(mel + 1) * ECAPA_TARGET_FRAMES];
            let mean = slice.iter().copied().sum::<f32>() / ECAPA_TARGET_FRAMES as f32;
            for value in slice {
                *value -= mean;
            }
        }
        Ok(output)
    }
}

fn create_ecapa_filterbank() -> Vec<f32> {
    let n_freqs = ECAPA_FFT / 2 + 1;
    let hz_to_mel = |hz: f64| 2595.0 * (1.0 + hz / 700.0).log10();
    let mel_to_hz = |mel: f64| 700.0 * (10.0_f64.powf(mel / 2595.0) - 1.0);
    let low_mel = hz_to_mel(0.0);
    let high_mel = hz_to_mel(SAMPLE_RATE as f64 / 2.0);
    let mut mel_points = Vec::with_capacity(ECAPA_MELS + 2);
    for index in 0..=ECAPA_MELS + 1 {
        mel_points.push(low_mel + (high_mel - low_mel) * index as f64 / (ECAPA_MELS + 1) as f64);
    }
    let hz_points: Vec<f64> = mel_points.into_iter().map(mel_to_hz).collect();
    let bin_points: Vec<usize> = hz_points
        .into_iter()
        .map(|hz| ((ECAPA_FFT + 1) as f64 * hz / SAMPLE_RATE as f64).floor() as usize)
        .collect();
    let mut matrix = vec![0.0_f32; ECAPA_MELS * n_freqs];
    for mel in 0..ECAPA_MELS {
        let left = bin_points[mel];
        let center = bin_points[mel + 1];
        let right = bin_points[mel + 2];
        for bin in left..center {
            if center > left {
                matrix[mel * n_freqs + bin] = ((bin - left) as f64 / (center - left) as f64) as f32;
            }
        }
        for bin in center..right {
            if right > center {
                matrix[mel * n_freqs + bin] =
                    ((right - bin) as f64 / (right - center) as f64) as f32;
            }
        }
    }
    matrix
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn npy_parser_reads_little_endian_matrix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matrix.npy");
        let header = b"{'descr': '<f4', 'fortran_order': False, 'shape': (2, 3), }";
        let mut payload = Vec::new();
        payload.extend_from_slice(b"\x93NUMPY\x01\x00");
        payload.extend_from_slice(&(header.len() as u16).to_le_bytes());
        payload.extend_from_slice(header);
        for value in [1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        std::fs::write(&path, payload).unwrap();
        let matrix = MelMatrix::load(&path).unwrap();
        assert_eq!(matrix.rows(), 2);
        assert_eq!(matrix.cols(), 3);
        assert_eq!(matrix.row(1), &[4.0, 5.0, 6.0]);
    }

    #[test]
    fn npy_parser_rejects_fortran_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matrix.npy");
        let header = b"{'descr': '<f4', 'fortran_order': True, 'shape': (1, 1), }";
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"\x93NUMPY\x01\x00").unwrap();
        file.write_all(&(header.len() as u16).to_le_bytes())
            .unwrap();
        file.write_all(header).unwrap();
        file.write_all(&1.0_f32.to_le_bytes()).unwrap();
        assert!(MelMatrix::load(&path).is_err());
    }

    #[test]
    fn ecapa_filterbank_has_expected_shape_and_edges() {
        let matrix = create_ecapa_filterbank();
        assert_eq!(matrix.len(), ECAPA_MELS * (ECAPA_FFT / 2 + 1));
        assert!(matrix.iter().all(|value| *value >= 0.0));
        assert!(matrix.iter().any(|value| *value > 0.0));
    }

    #[test]
    fn yamnet_patches_repeat_last_frame() {
        let matrix = MelMatrix {
            rows: YAMNET_FFT / 2 + 1,
            cols: YAMNET_MELS,
            data: vec![0.0; (YAMNET_FFT / 2 + 1) * YAMNET_MELS],
        };
        let frontend = YamnetFrontend::new(matrix).unwrap();
        let log_mel = vec![1.0_f32; 2 * YAMNET_MELS];
        let patches = frontend.patches(&log_mel, &[0]);
        assert_eq!(patches.len(), YAMNET_PATCH_FRAMES * YAMNET_MELS);
        assert_eq!(&patches[..YAMNET_MELS], &[1.0; YAMNET_MELS]);
        assert_eq!(
            &patches[(YAMNET_PATCH_FRAMES - 1) * YAMNET_MELS..],
            &[1.0; YAMNET_MELS]
        );
    }
}
