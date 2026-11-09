//! Extraction de frames et image BGR partagées par les contrôles Stage 1.
//!
//! `ffmpeg` sort une frame brute `bgr24` : le format est compact et évite un
//! décodage PNG. Les dimensions viennent de `ffprobe` (Stage 0), car le flux
//! brut n'est pas auto-descriptif. Les contrôles ne modifient jamais la vidéo.

use anyhow::{ensure, Context, Result};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

/// Nombre d'échantillons partagés par le gate OCR A2 et le détecteur de
/// logos. Les deux consommateurs doivent lire exactement les mêmes instants.
pub(crate) const SAMPLE_COUNT: usize = 12;

/// Instant central de l'échantillon `index` sur `SAMPLE_COUNT`.
pub(crate) fn sample_time(duration_s: f64, index: usize) -> f64 {
    duration_s * (index as f64 + 0.5) / SAMPLE_COUNT as f64
}

/// Image BGR 8 bits, canaux entrelacés. L'ordre BGR est celui d'OpenCV, donc
/// celui des modèles YuNet et PP-OCR importés du PoC.
#[derive(Debug, Clone)]
pub(crate) struct BgrFrame {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) data: Vec<u8>,
}

impl BgrFrame {
    pub(crate) fn new(width: usize, height: usize, data: Vec<u8>) -> Result<Self> {
        ensure!(
            data.len() == width * height * 3,
            "frame {}x{}: {} octets au lieu de {}",
            width,
            height,
            data.len(),
            width * height * 3
        );
        Ok(Self {
            width,
            height,
            data,
        })
    }

    pub(crate) fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let offset = (y * self.width + x) * 3;
        [
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
        ]
    }

    /// Recadre une sous-image. Les bornes sont supposées valides : les
    /// appelants calculent la bande basse depuis les specs ffprobe.
    pub(crate) fn crop(&self, x0: usize, y0: usize, width: usize, height: usize) -> Self {
        let width = width.min(self.width.saturating_sub(x0));
        let height = height.min(self.height.saturating_sub(y0));
        let mut data = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            let start = ((y0 + y) * self.width + x0) * 3;
            data.extend_from_slice(&self.data[start..start + width * 3]);
        }
        Self {
            width,
            height,
            data,
        }
    }
}

/// Rééchantillonnage bilinéaire avec la convention demi-pixel d'OpenCV
/// (`INTER_LINEAR`), utilisé par YuNet et PP-OCR.
pub(crate) fn resize_bilinear(source: &BgrFrame, width: usize, height: usize) -> BgrFrame {
    if source.width == width && source.height == height {
        return source.clone();
    }
    let mut data = vec![0u8; width * height * 3];
    let scale_x = source.width as f32 / width as f32;
    let scale_y = source.height as f32 / height as f32;
    for y in 0..height {
        let source_y = (y as f32 + 0.5) * scale_y - 0.5;
        let y0 = source_y.floor().max(0.0) as usize;
        let y1 = (y0 + 1).min(source.height.saturating_sub(1));
        let weight_y = (source_y - y0 as f32).clamp(0.0, 1.0);
        for x in 0..width {
            let source_x = (x as f32 + 0.5) * scale_x - 0.5;
            let x0 = source_x.floor().max(0.0) as usize;
            let x1 = (x0 + 1).min(source.width.saturating_sub(1));
            let weight_x = (source_x - x0 as f32).clamp(0.0, 1.0);
            let top_left = source.pixel(x0, y0);
            let top_right = source.pixel(x1, y0);
            let bottom_left = source.pixel(x0, y1);
            let bottom_right = source.pixel(x1, y1);
            let offset = (y * width + x) * 3;
            for channel in 0..3 {
                let top = f32::from(top_left[channel]) * (1.0 - weight_x)
                    + f32::from(top_right[channel]) * weight_x;
                let bottom = f32::from(bottom_left[channel]) * (1.0 - weight_x)
                    + f32::from(bottom_right[channel]) * weight_x;
                data[offset + channel] = (top * (1.0 - weight_y) + bottom * weight_y).round() as u8;
            }
        }
    }
    BgrFrame {
        width,
        height,
        data,
    }
}

pub(crate) fn grab_frame(
    ffmpeg: &Path,
    video: &Path,
    time: f64,
    width: usize,
    height: usize,
) -> Result<Option<BgrFrame>> {
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-ss",
            &format!("{time:.3}"),
            "-i",
        ])
        .arg(video)
        .args([
            "-frames:v",
            "1",
            "-an",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "bgr24",
            "-",
        ])
        .output()
        .with_context(|| format!("ffmpeg frame à {time:.3}s pour {}", video.display()))?;
    if !output.status.success() || output.stdout.is_empty() {
        return Ok(None);
    }
    if output.stdout.len() != width * height * 3 {
        tracing::warn!(
            expected = width * height * 3,
            received = output.stdout.len(),
            time,
            "frame de dimensions inattendues"
        );
        return Ok(None);
    }
    Ok(Some(BgrFrame::new(width, height, output.stdout)?))
}

/// Cache mono-projet des 12 frames source. Il est partagé par le gate OCR A2
/// et le détecteur de logos lorsque `a2_logo_detector` est actif : l'OCR
/// remplit le cache, le détecteur le relit sans relancer ffmpeg ni recopier
/// 75 Mio. Une frame manquante est mémorisée pour ne pas réessayer le même
/// décodage en échec.
pub(crate) struct FrameCache {
    video: PathBuf,
    width: usize,
    height: usize,
    duration_s: f64,
    slots: RefCell<Vec<FrameSlot>>,
}

enum FrameSlot {
    Pending,
    Ready(Rc<BgrFrame>),
    Missing,
}

impl FrameCache {
    pub(crate) fn new(video: &Path, width: usize, height: usize, duration_s: f64) -> Self {
        Self {
            video: video.to_path_buf(),
            width,
            height,
            duration_s,
            slots: RefCell::new((0..SAMPLE_COUNT).map(|_| FrameSlot::Pending).collect()),
        }
    }

    /// Retourne la frame `index`, en la décodant au premier accès.
    pub(crate) fn frame(&self, ffmpeg: &Path, index: usize) -> Result<Option<Rc<BgrFrame>>> {
        ensure!(
            index < SAMPLE_COUNT,
            "échantillon {index} hors 0..{SAMPLE_COUNT}"
        );
        {
            let slots = self.slots.borrow();
            match &slots[index] {
                FrameSlot::Ready(frame) => return Ok(Some(Rc::clone(frame))),
                FrameSlot::Missing => return Ok(None),
                FrameSlot::Pending => {}
            }
        }
        let time = sample_time(self.duration_s, index);
        let grabbed = grab_frame(ffmpeg, &self.video, time, self.width, self.height);
        let mut slots = self.slots.borrow_mut();
        match grabbed {
            Ok(Some(frame)) => {
                let frame = Rc::new(frame);
                slots[index] = FrameSlot::Ready(Rc::clone(&frame));
                Ok(Some(frame))
            }
            Ok(None) => {
                slots[index] = FrameSlot::Missing;
                Ok(None)
            }
            Err(error) => {
                // Erreur transitoire (ffmpeg, disque) : on laisse la case
                // Pending pour que le second consommateur puisse réessayer.
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_extracts_the_requested_window() {
        let frame = BgrFrame::new(3, 2, (0..18).collect()).unwrap();
        let crop = frame.crop(1, 1, 2, 1);
        assert_eq!(crop.width, 2);
        assert_eq!(crop.height, 1);
        assert_eq!(crop.pixel(0, 0), [12, 13, 14]);
        assert_eq!(crop.pixel(1, 0), [15, 16, 17]);
    }

    #[test]
    fn new_rejects_inconsistent_buffers() {
        assert!(BgrFrame::new(2, 2, vec![0; 11]).is_err());
    }

    #[test]
    fn resize_preserves_a_uniform_frame() {
        let source = BgrFrame::new(4, 3, [1, 2, 3].repeat(4 * 3)).unwrap();
        let resized = resize_bilinear(&source, 8, 8);
        assert_eq!(resized.width, 8);
        assert_eq!(resized.height, 8);
        assert_eq!(resized.pixel(0, 0), [1, 2, 3]);
        assert_eq!(resized.pixel(7, 7), [1, 2, 3]);
    }

    #[test]
    fn sample_times_are_centered_on_twelve_buckets() {
        assert_eq!(SAMPLE_COUNT, 12);
        assert_eq!(sample_time(60.0, 0), 2.5);
        assert_eq!(sample_time(60.0, 11), 57.5);
    }

    #[test]
    fn frame_cache_rejects_an_out_of_range_sample() {
        let cache = FrameCache::new(Path::new("absent.mp4"), 10, 10, 60.0);
        assert!(cache.frame(Path::new("ffmpeg"), SAMPLE_COUNT).is_err());
    }
}
