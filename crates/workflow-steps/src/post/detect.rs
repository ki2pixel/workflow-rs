//! Détecteurs média du post-traitement (ffprobe, cropdetect, bandes A2).
//!
//! Les algorithmes reprennent le prototype validé A5 v2. Le module ne fait
//! aucune écriture : il retourne des mesures et laisse les décisions à
//! `a1`/`a2`.

use crate::post::model::VideoHash;
use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use workflow_schema::round::round_to;

pub const A2_TOP_MAX_CENTER: f64 = 0.45;

#[derive(Debug, Clone, Serialize)]
pub struct VideoSpecs {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub duration_s: f64,
    pub bit_rate: u64,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub w: u32,
    pub h: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone)]
pub struct BandCandidate {
    pub y0_src: i64,
    pub y1_src: i64,
    pub height_src: i64,
    pub x0_src: i64,
    pub x1_src: i64,
    pub width_frac: f64,
    pub center_x_frac: f64,
    pub center_y_frac: f64,
    pub aspect: f64,
    pub persistence: f64,
    pub median_brightness: f64,
    pub mean_brightness: f64,
    pub temporal_std: f64,
    pub center_y_std: f64,
    pub height_std: f64,
    pub n_frames: usize,
    pub zone: &'static str,
    pub source: &'static str,
}

impl BandCandidate {
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "y0_src": self.y0_src,
            "y1_src": self.y1_src,
            "height_src": self.height_src,
            "x0_src": self.x0_src,
            "x1_src": self.x1_src,
            "width_frac": self.width_frac,
            "center_x_frac": self.center_x_frac,
            "center_y_frac": self.center_y_frac,
            "aspect": self.aspect,
            "persistence": self.persistence,
            "median_brightness": self.median_brightness,
            "mean_brightness": self.mean_brightness,
            "temporal_std": self.temporal_std,
            "center_y_std": self.center_y_std,
            "height_std": self.height_std,
            "n_frames": self.n_frames,
            "zone": self.zone,
            "source": self.source,
        })
    }
}

pub fn probe_video(ffprobe: &Path, path: &Path) -> Result<VideoSpecs> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,width,height,r_frame_rate,bit_rate",
            "-show_entries",
            "format=duration,bit_rate,size",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .with_context(|| format!("ffprobe introuvable ou en échec pour {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "ffprobe: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let payload: Value = serde_json::from_slice(&output.stdout).context("ffprobe JSON invalide")?;
    let streams = payload
        .get("streams")
        .and_then(Value::as_array)
        .context("ffprobe sans flux")?;
    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))
        .context("pas de flux vidéo")?;
    let width = video
        .get("width")
        .and_then(Value::as_u64)
        .context("ffprobe sans largeur")? as u32;
    let height = video
        .get("height")
        .and_then(Value::as_u64)
        .context("ffprobe sans hauteur")? as u32;
    let rate = video
        .get("r_frame_rate")
        .and_then(Value::as_str)
        .unwrap_or("0/1");
    let fps = rate
        .split_once('/')
        .and_then(|(num, den)| {
            let num = num.parse::<f64>().ok()?;
            let den = den.parse::<f64>().ok()?;
            (den != 0.0).then_some(num / den)
        })
        .unwrap_or(0.0);
    let format = payload.get("format").cloned().unwrap_or(Value::Null);
    let duration_s = format
        .get("duration")
        .and_then(|value| {
            value
                .as_str()
                .and_then(|text| text.parse::<f64>().ok())
                .or_else(|| value.as_f64())
        })
        .unwrap_or(0.0);
    let bit_rate = format
        .get("bit_rate")
        .and_then(|value| {
            value
                .as_str()
                .and_then(|text| text.parse::<u64>().ok())
                .or_else(|| value.as_u64())
        })
        .unwrap_or(0);
    let size_bytes = format
        .get("size")
        .and_then(|value| {
            value
                .as_str()
                .and_then(|text| text.parse::<u64>().ok())
                .or_else(|| value.as_u64())
        })
        .unwrap_or(0);
    Ok(VideoSpecs {
        width,
        height,
        fps: round_to(fps, 3),
        codec: video
            .get("codec_name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        duration_s: round_to(duration_s, 3),
        bit_rate,
        size_bytes,
    })
}

pub fn cropdetect(ffmpeg: &Path, video: &Path, start: f64, duration: f64) -> Result<Vec<Crop>> {
    let start = start.max(0.0);
    let duration = duration.max(0.2);
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-ss", &format!("{start:.3}"), "-i"])
        .arg(video)
        .args([
            "-t",
            &format!("{duration:.3}"),
            "-vf",
            "cropdetect=24:16:0",
            "-f",
            "null",
            "-",
        ])
        .output()
        .with_context(|| format!("ffmpeg cropdetect introuvable pour {}", video.display()))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let regex = crop_regex();
    Ok(stderr
        .lines()
        .filter_map(|line| {
            let captures = regex.captures(line)?;
            Some(Crop {
                w: captures.get(1)?.as_str().parse().ok()?,
                h: captures.get(2)?.as_str().parse().ok()?,
                x: captures.get(3)?.as_str().parse().ok()?,
                y: captures.get(4)?.as_str().parse().ok()?,
            })
        })
        .collect())
}

fn crop_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"crop=(\d+):(\d+):(\d+):(\d+)").expect("regex cropdetect"))
}

/// Bandes horizontales candidates sur la médiane temporelle.
pub fn band_candidates(ffmpeg: &Path, ffprobe: &Path, video: &Path) -> Result<Vec<BandCandidate>> {
    let specs = probe_video(ffprobe, video)?;
    let width = 480usize;
    let height = ((round_to(
        width as f64 * specs.height as f64 / specs.width as f64 / 2.0,
        0,
    ) as usize)
        * 2)
    .max(2);
    let frame_size = width * height;

    let mut child = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(video)
        .args([
            "-vf",
            &format!("fps=1,scale={width}:{height}"),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("ffmpeg bandes introuvable pour {}", video.display()))?;
    let stderr = child.stderr.take().context("stderr ffmpeg absent")?;
    let stderr_handle = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut reader = stderr;
        let _ = reader.read_to_end(&mut buffer);
        buffer
    });
    let stdout = child.stdout.take().context("stdout ffmpeg absent")?;
    let mut reader = BufReader::new(stdout);

    let pixels = width * height;
    let mut histogram = vec![0u32; pixels * 256];
    let mut row_sums: Vec<u32> = Vec::new();
    let mut row_brights: Vec<u32> = Vec::new();
    let mut row_grads: Vec<f32> = Vec::new();
    let mut frame = vec![0u8; frame_size];
    let mut n_frames = 0usize;
    loop {
        match reader.read_exact(&mut frame) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error).context("lecture des frames ffmpeg"),
        }
        let mut sums = vec![0u32; height];
        let mut brights = vec![0u32; height];
        let mut grads = vec![0f32; height];
        for y in 0..height {
            let row = &frame[y * width..(y + 1) * width];
            let mut sum = 0u32;
            let mut bright = 0u32;
            for (x, pixel) in row.iter().enumerate() {
                sum += *pixel as u32;
                if *pixel > 170 {
                    bright += 1;
                }
                histogram[(y * width + x) * 256 + *pixel as usize] += 1;
            }
            sums[y] = sum;
            brights[y] = bright;
            grads[y] = sobel_row_mean(&frame, width, height, y);
        }
        row_sums.extend_from_slice(&sums);
        row_brights.extend_from_slice(&brights);
        row_grads.extend_from_slice(&grads);
        n_frames += 1;
    }
    drop(reader);
    let status = child.wait().context("attente ffmpeg bandes")?;
    let stderr = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        bail!("ffmpeg bandes: {}", String::from_utf8_lossy(&stderr).trim());
    }
    if n_frames < 3 {
        return Ok(Vec::new());
    }

    let mut median = vec![0u8; pixels];
    for (index, pixel) in median.iter_mut().enumerate() {
        *pixel = median_from_hist(&histogram[index * 256..(index + 1) * 256], n_frames);
    }
    drop(histogram);
    let (median_grad, median_grad_rows) = sobel_y(&median, width, height);
    let bright_frac: Vec<f64> = (0..height)
        .map(|y| {
            median[y * width..(y + 1) * width]
                .iter()
                .filter(|pixel| **pixel > 190)
                .count() as f64
                / width as f64
        })
        .collect();

    let hot: Vec<usize> = (0..height)
        .filter(|&y| median_grad_rows[y] > 18.0 || bright_frac[y] > 0.04)
        .collect();
    let groups = group_rows(&hot, 4);
    let mut candidates: Vec<BandCandidate> = Vec::new();
    for group in &groups {
        let a = group[0];
        let b = *group.last().unwrap_or(&a);
        let rows = b - a + 1;
        let mut columns = vec![0u32; width];
        for y in a..=b {
            for (x, pixel) in median[y * width..(y + 1) * width].iter().enumerate() {
                if *pixel > 170 {
                    columns[x] += 1;
                }
            }
        }
        let x_hot: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, count)| **count as f64 / rows as f64 > 0.05)
            .map(|(x, _)| x)
            .collect();
        let (Some(x0), Some(x1)) = (x_hot.first().copied(), x_hot.last().copied()) else {
            continue;
        };
        let stats = band_stats(
            &row_sums,
            &row_brights,
            &row_grads,
            n_frames,
            height,
            width,
            a,
            b,
            specs.height as f64,
        );
        let median_brightness = median_u8(
            &median[a * width..=(b * width + width - 1).min(pixels - 1)],
            rows * width,
        );
        candidates.push(BandCandidate {
            y0_src: round_to(a as f64 * specs.height as f64 / height as f64, 0) as i64,
            y1_src: round_to(b as f64 * specs.height as f64 / height as f64, 0) as i64,
            height_src: round_to(rows as f64 * specs.height as f64 / height as f64, 0) as i64,
            x0_src: round_to(x0 as f64 * specs.width as f64 / width as f64, 0) as i64,
            x1_src: round_to(x1 as f64 * specs.width as f64 / width as f64, 0) as i64,
            width_frac: round_to((x1 - x0 + 1) as f64 / width as f64, 3),
            center_x_frac: round_to(((x0 + x1) as f64 / 2.0) / width as f64, 3),
            center_y_frac: round_to(((a + b) as f64 / 2.0) / height as f64, 3),
            aspect: round_to(
                round_to((x1 - x0 + 1) as f64 * specs.width as f64 / width as f64, 0)
                    / (round_to(rows as f64 * specs.height as f64 / height as f64, 0)).max(1.0),
                2,
            ),
            persistence: round_to(stats.persistence, 3),
            median_brightness: round_to(median_brightness, 1),
            mean_brightness: round_to(stats.mean, 1),
            temporal_std: round_to(stats.std, 1),
            center_y_std: round_to(stats.center_std, 1),
            height_std: round_to(stats.height_std, 1),
            n_frames,
            zone: if (a + b) as f64 / 2.0 < height as f64 * A2_TOP_MAX_CENTER {
                "top"
            } else {
                "bottom"
            },
            source: "bright",
        });
    }

    // Bandes sombres : paires de bords horizontaux dans la médiane.
    let peaks: Vec<usize> = (1..height.saturating_sub(1))
        .filter(|&y| {
            median_grad_rows[y] > 18.0
                && median_grad_rows[y] >= median_grad_rows[y - 1]
                && median_grad_rows[y] >= median_grad_rows[y + 1]
        })
        .collect();
    let edge_groups = group_rows(&peaks, 4);
    for pair in edge_groups.windows(2) {
        let y0 = *pair[0].last().unwrap_or(&0);
        let y1 = pair[1][0];
        let gap = y1.saturating_sub(y0);
        if !(24..=200).contains(&gap) {
            continue;
        }
        let line = &median_grad[y0 * width..(y0 + 1) * width];
        let xs: Vec<usize> = line
            .iter()
            .enumerate()
            .filter(|(_, value)| **value > 25.0)
            .map(|(x, _)| x)
            .collect();
        if xs.len() < (width as f64 * 0.2) as usize {
            continue;
        }
        let x0 = *xs.first().unwrap_or(&0);
        let x1 = *xs.last().unwrap_or(&0);
        let width_frac = (x1 - x0 + 1) as f64 / width as f64;
        if width_frac < 0.35 {
            continue;
        }
        let y0_src = round_to(y0 as f64 * specs.height as f64 / height as f64, 0) as i64;
        let y1_src = round_to(y1 as f64 * specs.height as f64 / height as f64, 0) as i64;
        if candidates.iter().any(|candidate| {
            (candidate.y0_src - y0_src).abs() < 12 && (candidate.y1_src - y1_src).abs() < 12
        }) {
            continue;
        }
        let median_brightness = median_u8(
            &median[y0 * width..=(y1 * width + width - 1).min(pixels - 1)],
            (gap + 1) * width,
        );
        if median_brightness >= 60.0 {
            continue;
        }
        let stats = band_stats(
            &row_sums,
            &row_brights,
            &row_grads,
            n_frames,
            height,
            width,
            y0,
            y1,
            specs.height as f64,
        );
        candidates.push(BandCandidate {
            y0_src,
            y1_src,
            height_src: round_to(gap as f64 * specs.height as f64 / height as f64, 0) as i64,
            x0_src: round_to(x0 as f64 * specs.width as f64 / width as f64, 0) as i64,
            x1_src: round_to(x1 as f64 * specs.width as f64 / width as f64, 0) as i64,
            width_frac: round_to(width_frac, 3),
            center_x_frac: round_to(((x0 + x1) as f64 / 2.0) / width as f64, 3),
            center_y_frac: round_to(((y0 + y1) as f64 / 2.0) / height as f64, 3),
            aspect: round_to(
                round_to((x1 - x0 + 1) as f64 * specs.width as f64 / width as f64, 0)
                    / (round_to(gap as f64 * specs.height as f64 / height as f64, 0)).max(1.0),
                2,
            ),
            persistence: 1.0,
            median_brightness: round_to(median_brightness, 1),
            mean_brightness: round_to(stats.mean, 1),
            temporal_std: round_to(stats.std, 1),
            center_y_std: round_to(stats.center_std, 1),
            height_std: round_to(stats.height_std, 1),
            n_frames,
            zone: if (y0 + y1) as f64 / 2.0 < height as f64 * A2_TOP_MAX_CENTER {
                "top"
            } else {
                "bottom"
            },
            source: "edge_pair_dark",
        });
    }

    candidates.sort_by_key(|candidate| candidate.y1_src);
    Ok(candidates)
}

struct BandStats {
    mean: f64,
    std: f64,
    persistence: f64,
    center_std: f64,
    height_std: f64,
}

#[allow(clippy::too_many_arguments)]
fn band_stats(
    row_sums: &[u32],
    row_brights: &[u32],
    row_grads: &[f32],
    n_frames: usize,
    height: usize,
    width: usize,
    a: usize,
    b: usize,
    source_height: f64,
) -> BandStats {
    let rows = b - a + 1;
    let total = (rows * width) as f64;
    let mut means = Vec::with_capacity(n_frames);
    let mut persistent = 0usize;
    for frame in 0..n_frames {
        let base = frame * height;
        let sum: u64 = row_sums[base + a..=base + b]
            .iter()
            .map(|value| *value as u64)
            .sum();
        let bright: u64 = row_brights[base + a..=base + b]
            .iter()
            .map(|value| *value as u64)
            .sum();
        means.push(sum as f64 / total);
        if bright as f64 / total > 0.05 {
            persistent += 1;
        }
    }
    let mean = means.iter().sum::<f64>() / n_frames as f64;
    let variance = means
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / n_frames as f64;
    let (center_std, height_std) = edge_geometry(row_grads, n_frames, height, a, b, source_height);
    BandStats {
        mean,
        std: variance.sqrt(),
        persistence: persistent as f64 / n_frames as f64,
        center_std,
        height_std,
    }
}

fn edge_geometry(
    row_grads: &[f32],
    n_frames: usize,
    height: usize,
    a: usize,
    b: usize,
    source_height: f64,
) -> (f64, f64) {
    let mut centers = Vec::new();
    let mut heights = Vec::new();
    for frame in 0..n_frames {
        let rows = &row_grads[frame * height..(frame + 1) * height];
        let (Some(top), Some(bottom)) = (find_edge(rows, a), find_edge(rows, b)) else {
            continue;
        };
        centers.push((top + bottom) as f64 / 2.0);
        heights.push(bottom as f64 - top as f64);
    }
    if centers.len() < 2 {
        return (0.0, 0.0);
    }
    let scale = source_height / height as f64;
    (std(&centers) * scale, std(&heights) * scale)
}

fn find_edge(rows: &[f32], center: usize) -> Option<usize> {
    let start = center.saturating_sub(12);
    let end = (center + 12).min(rows.len());
    if end <= start {
        return None;
    }
    let mut best = start;
    for index in start + 1..end {
        if rows[index] > rows[best] {
            best = index;
        }
    }
    (rows[best] >= 5.0).then_some(best)
}

fn std(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64)
        .sqrt()
}

fn group_rows(rows: &[usize], gap: usize) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for &row in rows {
        if let Some(group) = groups.last_mut() {
            if row - *group.last().unwrap_or(&row) <= gap {
                group.push(row);
                continue;
            }
        }
        groups.push(vec![row]);
    }
    groups
}

fn median_from_hist(histogram: &[u32], count: usize) -> u8 {
    if count == 0 {
        return 0;
    }
    if count % 2 == 1 {
        let target = count / 2;
        let mut seen = 0usize;
        for (value, hits) in histogram.iter().enumerate() {
            seen += *hits as usize;
            if seen > target {
                return value as u8;
            }
        }
        return 0;
    }
    let first = count / 2 - 1;
    let second = count / 2;
    let mut seen = 0usize;
    let mut low = 0usize;
    let mut low_set = false;
    for (value, hits) in histogram.iter().enumerate() {
        seen += *hits as usize;
        if !low_set && seen > first {
            low = value;
            low_set = true;
        }
        if seen > second {
            return ((low + value) / 2) as u8;
        }
    }
    0
}

fn median_u8(values: &[u8], count: usize) -> f64 {
    if values.is_empty() || count == 0 {
        return 0.0;
    }
    let mut histogram = [0u32; 256];
    for value in values.iter().take(count) {
        histogram[*value as usize] += 1;
    }
    if count % 2 == 1 {
        let target = count / 2;
        let mut seen = 0usize;
        for (value, hits) in histogram.iter().enumerate() {
            seen += *hits as usize;
            if seen > target {
                return value as f64;
            }
        }
        return 0.0;
    }
    let first = count / 2 - 1;
    let second = count / 2;
    let mut seen = 0usize;
    let mut low = 0usize;
    let mut low_set = false;
    for (value, hits) in histogram.iter().enumerate() {
        seen += *hits as usize;
        if !low_set && seen > first {
            low = value;
            low_set = true;
        }
        if seen > second {
            return (low + value) as f64 / 2.0;
        }
    }
    0.0
}

fn sobel_row_mean(image: &[u8], width: usize, height: usize, y: usize) -> f32 {
    let ym1 = if y == 0 { 1 } else { y - 1 };
    let yp1 = if y + 1 >= height { height - 2 } else { y + 1 };
    let mut sum = 0f32;
    for x in 0..width {
        let xm1 = if x == 0 { 1 } else { x - 1 };
        let xp1 = if x + 1 >= width { width - 2 } else { x + 1 };
        let gradient = -(image[ym1 * width + xm1] as f32)
            - 2.0 * image[ym1 * width + x] as f32
            - image[ym1 * width + xp1] as f32
            + image[yp1 * width + xm1] as f32
            + 2.0 * image[yp1 * width + x] as f32
            + image[yp1 * width + xp1] as f32;
        sum += gradient.abs();
    }
    sum / width as f32
}

fn sobel_y(image: &[u8], width: usize, height: usize) -> (Vec<f32>, Vec<f32>) {
    let mut pixels = vec![0f32; width * height];
    let mut rows = vec![0f32; height];
    for y in 0..height {
        let ym1 = if y == 0 { 1 } else { y - 1 };
        let yp1 = if y + 1 >= height { height - 2 } else { y + 1 };
        let mut sum = 0f32;
        for x in 0..width {
            let xm1 = if x == 0 { 1 } else { x - 1 };
            let xp1 = if x + 1 >= width { width - 2 } else { x + 1 };
            let gradient = -(image[ym1 * width + xm1] as f32)
                - 2.0 * image[ym1 * width + x] as f32
                - image[ym1 * width + xp1] as f32
                + image[yp1 * width + xm1] as f32
                + 2.0 * image[yp1 * width + x] as f32
                + image[yp1 * width + xp1] as f32;
            let gradient = gradient.abs();
            pixels[y * width + x] = gradient;
            sum += gradient;
        }
        rows[y] = sum / width as f32;
    }
    (pixels, rows)
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("lecture de {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn sha256_sample(path: &Path, chunk: u64) -> Result<VideoHash> {
    let size = std::fs::metadata(path)
        .with_context(|| format!("métadonnées de {}", path.display()))?
        .len();
    if size <= chunk.saturating_mul(3) {
        return Ok(VideoHash {
            mode: "full",
            size,
            chunk,
            offsets: vec![0],
            sha256: sha256_file(path)?,
        });
    }
    let offsets = vec![0, (size - chunk) / 2, size - chunk];
    let mut file = File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(format!("size={size};chunk={chunk};").as_bytes());
    let mut buffer = vec![0u8; chunk as usize];
    for offset in &offsets {
        file.seek(SeekFrom::Start(*offset))
            .with_context(|| format!("seek dans {}", path.display()))?;
        file.read_exact(&mut buffer)
            .with_context(|| format!("lecture de {}", path.display()))?;
        hasher.update(&buffer);
    }
    Ok(VideoHash {
        mode: "sample",
        size,
        chunk,
        offsets,
        sha256: format!("{:x}", hasher.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_histogram_matches_odd_and_even_counts() {
        let mut histogram = [0u32; 256];
        for value in [10u8, 20, 30] {
            histogram[value as usize] += 1;
        }
        assert_eq!(median_from_hist(&histogram, 3), 20);
        histogram[40] += 1;
        assert_eq!(median_from_hist(&histogram, 4), 25);
    }

    #[test]
    fn groups_rows_with_small_gaps() {
        let groups = group_rows(&[1, 2, 7, 8, 20], 4);
        assert_eq!(groups, vec![vec![1, 2], vec![7, 8], vec![20]]);
    }

    #[test]
    fn sobel_edge_is_detected() {
        let width = 8;
        let height = 8;
        let mut image = vec![0u8; width * height];
        for y in 4..height {
            for x in 0..width {
                image[y * width + x] = 255;
            }
        }
        let (_, rows) = sobel_y(&image, width, height);
        assert!(rows[3] > 18.0);
        assert!(rows[4] > 18.0);
        assert!(rows[0] < 1.0);
    }

    #[test]
    fn sample_hash_includes_size_and_offsets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("video.bin");
        std::fs::write(&path, vec![7u8; 100]).unwrap();
        let hash = sha256_sample(&path, 10).unwrap();
        assert_eq!(hash.mode, "sample");
        assert_eq!(hash.offsets, vec![0, 45, 90]);
        assert_eq!(hash.size, 100);
    }
}
