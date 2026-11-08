//! Confirmation visuelle locale des logos source (D-052, phase C).
//!
//! Le registre spatial donne une position ; ce module vérifie que les pixels
//! de l'export ressemblent réellement au crop de référence. Il ne modifie ni
//! la vidéo ni le projet et ne produit aucune décision : il enrichit le
//! résumé du check `source_logo`.
//!
//! Deux chemins :
//! - émission connue : NCC couleur sur le masque alpha + NCC des contours,
//!   avec plusieurs variantes et plusieurs instants ;
//! - source inconnue : stabilité temporelle des zones génériques, sans
//!   affirmation d'identité.
//!
//! Les crops de référence restent externes. Le chemin est fourni par
//! `--source-logo-references` (ou `reference_root` du catalogue) et n'est
//! jamais copié dans le dépôt.

use crate::post::ecproj::{self, CompMeta, Segment};
use crate::qa::source_logo::source_size;
use crate::vision::frame::{grab_frame, resize_bilinear, BgrFrame};
use crate::vision::source_logo::{
    bbox_area, clip_bbox, resolve_reference_path, SourceLogoCatalog, SourceLogoPrior,
    SourceLogoVariantPrior,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Nombre maximal d'instants échantillonnés pour une émission connue.
const MAX_KNOWN_SAMPLES: usize = 6;
/// Nombre maximal d'instants par zone générique.
const MAX_GENERIC_SAMPLES: usize = 4;
/// Seuils de confirmation d'un logo connu.
const MIN_COLOR_NCC: f64 = 0.45;
const MIN_EDGE_NCC: f64 = 0.10;
const MIN_COMBINED_NCC: f64 = 0.50;
const MIN_STRONG_SAMPLES: usize = 2;
/// Seuil de gradient (luminance normalisée 0..1) pour le masque de contours.
const EDGE_THRESHOLD: f64 = 0.15;
const MIN_MASK_PIXELS: usize = 200;
/// Stabilité temporelle d'une zone générique. La zone est large et le logo
/// n'en occupe qu'une partie : on mesure donc la présence d'une poche
/// stable avec des contours persistants, pas la stabilité de toute la zone.
const GENERIC_STABLE_STD: f64 = 12.0 / 255.0;
const GENERIC_STABLE_RATIO: f64 = 0.05;
const GENERIC_EDGE_THRESHOLD: f64 = 0.12;
const GENERIC_EDGE_PERSISTENCE: f64 = 0.80;
const GENERIC_PERSISTENT_EDGE_RATIO: f64 = 0.008;
const GENERIC_EDGE_DENSITY: f64 = 0.04;
const GENERIC_MIN_SAMPLES: usize = 3;
const GENERIC_WIDTH: usize = 128;

pub struct VisualOptions<'a> {
    pub video: &'a Path,
    pub ffmpeg: &'a Path,
    pub frame_width: u32,
    pub frame_height: u32,
    pub references: Option<&'a Path>,
}

/// Résumé ajouté au check `source_logo`. `None` signifie que la géométrie ne
/// laisse aucune observation à confirmer (logo hors cadre, clean feed, etc.).
pub fn summarize(
    doc: &Value,
    catalog: &SourceLogoCatalog,
    options: &VisualOptions<'_>,
) -> Option<String> {
    let meta = ecproj::comp_meta(doc).ok()?;
    let segments = ecproj::segments(doc).ok()?;
    if segments.is_empty() {
        return None;
    }
    // Un rush 1920×1080 évalué avec une composition 1080×1920 ne permet pas
    // de comparer les pixels : la géométrie reste utile, la confirmation non.
    let frame_aspect = f64::from(options.frame_width) / f64::from(options.frame_height.max(1));
    let comp_aspect = f64::from(meta.width) / f64::from(meta.height.max(1));
    if (frame_aspect - comp_aspect).abs() > comp_aspect * 0.05 {
        return Some(
            "confirmation visuelle non disponible (format de l'export différent de la composition)"
                .into(),
        );
    }
    let source_name = segments
        .iter()
        .find_map(|segment| segment.source.as_deref())
        .map(str::to_string)?;
    let (source_width, source_height) = source_size(doc, &source_name)?;
    if let Some(prior) = catalog.prior_for_source(&source_name, source_width, source_height) {
        return confirm_known(&prior, &source_name, &segments, &meta, catalog, options);
    }
    if catalog.is_clean_feed(&source_name) {
        return None;
    }
    let zones = catalog.generic_zones(source_width, source_height);
    if zones.is_empty() {
        return None;
    }
    confirm_generic(&zones, &source_name, &segments, &meta, options)
}

fn confirm_known(
    prior: &SourceLogoPrior,
    source_name: &str,
    segments: &[Segment],
    meta: &CompMeta,
    catalog: &SourceLogoCatalog,
    options: &VisualOptions<'_>,
) -> Option<String> {
    let samples = visible_samples(segments, source_name, prior.bbox, meta, MAX_KNOWN_SAMPLES);
    if samples.is_empty() {
        return None;
    }
    let Some(root) = catalog.reference_root(options.references) else {
        return Some("confirmation visuelle non disponible (références absentes)".into());
    };
    if !root.is_dir() {
        return Some(format!(
            "confirmation visuelle non disponible (références introuvables: {})",
            root.display()
        ));
    }
    let templates = load_templates(&prior.variants, &root);
    if templates.is_empty() {
        return Some("confirmation visuelle impossible (crops de référence introuvables)".into());
    }
    let mut frames = 0usize;
    let mut errors = 0usize;
    let mut strong = 0usize;
    let mut best = 0.0f64;
    for sample in &samples {
        let Ok(Some(frame)) = grab_frame(
            options.ffmpeg,
            options.video,
            sample.time,
            options.frame_width as usize,
            options.frame_height as usize,
        ) else {
            errors += 1;
            continue;
        };
        frames += 1;
        let segment = &segments[sample.segment_index];
        let mut sample_best = 0.0f64;
        let mut sample_strong = false;
        for template in &templates {
            if let Some(score) = score_template(template, segment, sample.time, meta, &frame) {
                if score.combined > sample_best {
                    sample_best = score.combined;
                }
                if score.strong() {
                    sample_strong = true;
                }
            }
        }
        best = best.max(sample_best);
        if sample_strong {
            strong += 1;
        }
    }
    if frames == 0 {
        return Some(format!(
            "confirmation visuelle en échec ({errors} frame(s) non extractible(s))"
        ));
    }
    let errors_suffix = if errors == 0 {
        String::new()
    } else {
        format!(", {errors} frame(s) en échec")
    };
    if strong >= MIN_STRONG_SAMPLES {
        Some(format!(
            "logo confirmé visuellement (NCC max {best:.2}, {strong}/{frames} échantillon(s) au-dessus du seuil{errors_suffix})"
        ))
    } else if strong == 1 {
        Some(format!(
            "logo confirmé sur 1 échantillon (NCC max {best:.2}), confirmation partielle{errors_suffix}"
        ))
    } else {
        Some(format!(
            "logo non confirmé visuellement (NCC max {best:.2}, seuil {MIN_COMBINED_NCC:.2}{errors_suffix})"
        ))
    }
}

fn confirm_generic(
    zones: &[(&'static str, [f64; 4])],
    source_name: &str,
    segments: &[Segment],
    meta: &CompMeta,
    options: &VisualOptions<'_>,
) -> Option<String> {
    let mut best: Option<(f64, f64, &'static str, usize)> = None;
    for (position, bbox) in zones {
        let samples = visible_samples(segments, source_name, *bbox, meta, MAX_GENERIC_SAMPLES);
        if samples.len() < GENERIC_MIN_SAMPLES {
            continue;
        }
        let width = ((bf_box_width(*bbox) / bf_box_height(*bbox)) * GENERIC_WIDTH as f64)
            .round()
            .clamp(32.0, 256.0) as usize;
        let height = GENERIC_WIDTH;
        let mut gray_samples = Vec::with_capacity(samples.len());
        for sample in &samples {
            let Ok(Some(frame)) = grab_frame(
                options.ffmpeg,
                options.video,
                sample.time,
                options.frame_width as usize,
                options.frame_height as usize,
            ) else {
                continue;
            };
            let segment = &segments[sample.segment_index];
            let Some(crop) = project_crop(*bbox, segment, sample.time, meta, &frame) else {
                continue;
            };
            let region = frame.crop(crop.x0, crop.y0, crop.width, crop.height);
            let resized = resize_bilinear(&region, width, height);
            gray_samples.push(gray_bgr(&resized));
        }
        if gray_samples.len() < GENERIC_MIN_SAMPLES {
            continue;
        }
        let (stable, persistent_edges) = temporal_stability(&gray_samples, width, height);
        if stable < GENERIC_STABLE_RATIO || persistent_edges < GENERIC_PERSISTENT_EDGE_RATIO {
            continue;
        }
        if persistent_edges / stable.max(1e-6) < GENERIC_EDGE_DENSITY {
            continue;
        }
        let candidate = (stable, persistent_edges, *position, gray_samples.len());
        if best.is_none_or(|current| candidate.0 * candidate.1 > current.0 * current.1) {
            best = Some(candidate);
        }
    }
    best.map(|(stable, edges, position, count)| {
        format!(
            "zone générique {position} stable sur {count} échantillon(s) \
             (stabilité {:.0} %, contours persistants {:.1} %), logo possible non identifié",
            stable * 100.0,
            edges * 100.0
        )
    })
}

#[derive(Debug, Clone)]
struct SamplePoint {
    time: f64,
    segment_index: usize,
}

/// Instants où la bbox est réellement dans le cadre, répartis sur toute la
/// timeline. On garde le début et la fin pour ne pas rater un cartouche
/// présent uniquement à l'ouverture.
fn visible_samples(
    segments: &[Segment],
    source_name: &str,
    bbox: [f64; 4],
    meta: &CompMeta,
    max: usize,
) -> Vec<SamplePoint> {
    let mut out = Vec::new();
    for (segment_index, segment) in segments.iter().enumerate() {
        if segment.source.as_deref() != Some(source_name) || !segment.projectable() {
            continue;
        }
        let duration = segment.duration();
        if duration <= 0.0 {
            continue;
        }
        for fraction in [0.1, 0.5, 0.9] {
            let time = segment.start + duration * fraction;
            let Some(projected) = segment.project_bbox(bbox, time) else {
                continue;
            };
            let Some(clipped) = clip_bbox(projected, meta.width, meta.height) else {
                continue;
            };
            let ratio = bbox_area(clipped) / bbox_area(projected).max(1.0);
            if ratio < 0.15 || bbox_area(clipped) < 400.0 {
                continue;
            }
            out.push(SamplePoint {
                time,
                segment_index,
            });
        }
    }
    out.sort_by(|left, right| left.time.total_cmp(&right.time));
    out.dedup_by(|left, right| (left.time - right.time).abs() < 1e-6);
    select_evenly(out, max)
}

fn select_evenly(items: Vec<SamplePoint>, max: usize) -> Vec<SamplePoint> {
    if items.len() <= max || max == 0 {
        return items;
    }
    let mut indices = Vec::with_capacity(max);
    for index in 0..max {
        let source = (index * (items.len() - 1) + (max - 1) / 2) / (max - 1).max(1);
        if indices.last() != Some(&source) {
            indices.push(source);
        }
    }
    indices
        .into_iter()
        .map(|index| items[index].clone())
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct ProjectedCrop {
    x0: usize,
    y0: usize,
    width: usize,
    height: usize,
    /// Part de la bbox source effectivement visible, normalisée dans la bbox
    /// projetée.
    rel: [f64; 4],
}

fn project_crop(
    bbox: [f64; 4],
    segment: &Segment,
    time: f64,
    meta: &CompMeta,
    frame: &BgrFrame,
) -> Option<ProjectedCrop> {
    let projected = segment.project_bbox(bbox, time)?;
    let clipped = clip_bbox(projected, meta.width, meta.height)?;
    let scale_x = frame.width as f64 / f64::from(meta.width);
    let scale_y = frame.height as f64 / f64::from(meta.height);
    let x0 = (clipped[0] * scale_x).floor().max(0.0) as usize;
    let y0 = (clipped[1] * scale_y).floor().max(0.0) as usize;
    let x1 = (clipped[2] * scale_x)
        .ceil()
        .min(frame.width as f64)
        .max(0.0) as usize;
    let y1 = (clipped[3] * scale_y)
        .ceil()
        .min(frame.height as f64)
        .max(0.0) as usize;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let comp_x0 = x0 as f64 / scale_x;
    let comp_y0 = y0 as f64 / scale_y;
    let comp_x1 = x1 as f64 / scale_x;
    let comp_y1 = y1 as f64 / scale_y;
    let width = projected[2] - projected[0];
    let height = projected[3] - projected[1];
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let rel = [
        ((comp_x0.max(projected[0]) - projected[0]) / width).clamp(0.0, 1.0),
        ((comp_y0.max(projected[1]) - projected[1]) / height).clamp(0.0, 1.0),
        ((comp_x1.min(projected[2]) - projected[0]) / width).clamp(0.0, 1.0),
        ((comp_y1.min(projected[3]) - projected[1]) / height).clamp(0.0, 1.0),
    ];
    if rel[2] <= rel[0] || rel[3] <= rel[1] {
        return None;
    }
    Some(ProjectedCrop {
        x0,
        y0,
        width: x1 - x0,
        height: y1 - y0,
        rel,
    })
}

#[derive(Debug, Clone)]
struct TemplateImage {
    /// Bbox dans les coordonnées de la source réelle, pour la projection.
    bbox: [f64; 4],
    width: usize,
    height: usize,
    /// RGBA entrelacé, recadré sur `reference_bbox`.
    data: Vec<u8>,
}

#[derive(Debug, Clone)]
struct TemplateRegion {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
struct VariantScore {
    color: f64,
    edge: f64,
    combined: f64,
}

impl VariantScore {
    fn strong(self) -> bool {
        self.color >= MIN_COLOR_NCC
            && self.edge >= MIN_EDGE_NCC
            && self.combined >= MIN_COMBINED_NCC
    }
}

fn load_templates(variants: &[SourceLogoVariantPrior], root: &Path) -> Vec<TemplateImage> {
    variants
        .iter()
        .filter_map(|variant| {
            let path = resolve_reference_path(root, &variant.crop);
            match load_template(&path, variant.reference_bbox, variant.bbox) {
                Ok(template) => Some(template),
                Err(error) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %error,
                        "confirmation visuelle: crop de référence ignoré"
                    );
                    None
                }
            }
        })
        .collect()
}

fn load_template(path: &Path, bbox: [i64; 4], source_bbox: [f64; 4]) -> Result<TemplateImage> {
    let file = File::open(path)
        .with_context(|| format!("ouverture du crop de référence {}", path.display()))?;
    let mut decoder = png::Decoder::new(BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("lecture PNG {}", path.display()))?;
    let buffer_size = reader
        .output_buffer_size()
        .context("PNG sans taille de sortie")?;
    let mut buffer = vec![0u8; buffer_size];
    let info = reader
        .next_frame(&mut buffer)
        .with_context(|| format!("décodage PNG {}", path.display()))?;
    let data = &buffer[..info.buffer_size()];
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::Indexed => bail!("PNG indexé non développé"),
    };
    let width = info.width as usize;
    let height = info.height as usize;
    let x0 = bbox[0].max(0) as usize;
    let y0 = bbox[1].max(0) as usize;
    let x1 = (bbox[2].max(0) as usize).min(width);
    let y1 = (bbox[3].max(0) as usize).min(height);
    if x1 <= x0 || y1 <= y0 {
        bail!(
            "bbox de référence {:?} hors du crop {}x{}",
            bbox,
            width,
            height
        );
    }
    let mut cropped = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        for x in x0..x1 {
            let offset = (y * width + x) * channels;
            let pixel = match channels {
                4 => [
                    data[offset],
                    data[offset + 1],
                    data[offset + 2],
                    data[offset + 3],
                ],
                3 => [data[offset], data[offset + 1], data[offset + 2], 255],
                2 => [data[offset], data[offset], data[offset], data[offset + 1]],
                _ => [data[offset], data[offset], data[offset], 255],
            };
            cropped.extend_from_slice(&pixel);
        }
    }
    Ok(TemplateImage {
        bbox: source_bbox,
        width: x1 - x0,
        height: y1 - y0,
        data: cropped,
    })
}

fn score_template(
    template: &TemplateImage,
    segment: &Segment,
    time: f64,
    meta: &CompMeta,
    frame: &BgrFrame,
) -> Option<VariantScore> {
    let crop = project_crop(template.bbox, segment, time, meta, frame)?;
    let region = crop_template(template, crop.rel)?;
    let resized = resize_rgba(&region, crop.width, crop.height);
    let frame_crop = frame.crop(crop.x0, crop.y0, crop.width, crop.height);
    score_rgba_bgr(&resized, crop.width, crop.height, &frame_crop)
}

fn crop_template(template: &TemplateImage, rel: [f64; 4]) -> Option<TemplateRegion> {
    let x0 = (rel[0] * template.width as f64).floor() as usize;
    let y0 = (rel[1] * template.height as f64).floor() as usize;
    let x1 = (rel[2] * template.width as f64).ceil() as usize;
    let y1 = (rel[3] * template.height as f64).ceil() as usize;
    let x0 = x0.min(template.width.saturating_sub(1));
    let y0 = y0.min(template.height.saturating_sub(1));
    let x1 = x1.min(template.width).max(x0 + 1);
    let y1 = y1.min(template.height).max(y0 + 1);
    let width = x1 - x0;
    let height = y1 - y0;
    let mut data = Vec::with_capacity(width * height * 4);
    for y in y0..y1 {
        let start = (y * template.width + x0) * 4;
        data.extend_from_slice(&template.data[start..start + width * 4]);
    }
    Some(TemplateRegion {
        width,
        height,
        data,
    })
}

fn resize_rgba(source: &TemplateRegion, width: usize, height: usize) -> Vec<u8> {
    if width == 0 || height == 0 {
        return Vec::new();
    }
    let mut out = vec![0u8; width * height * 4];
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
            let top_left = premultiplied(source, x0, y0);
            let top_right = premultiplied(source, x1, y0);
            let bottom_left = premultiplied(source, x0, y1);
            let bottom_right = premultiplied(source, x1, y1);
            let offset = (y * width + x) * 4;
            let mut premult = [0f32; 4];
            for channel in 0..4 {
                let top = top_left[channel] * (1.0 - weight_x) + top_right[channel] * weight_x;
                let bottom =
                    bottom_left[channel] * (1.0 - weight_x) + bottom_right[channel] * weight_x;
                premult[channel] = top * (1.0 - weight_y) + bottom * weight_y;
            }
            let alpha = (premult[3] * 255.0).round().clamp(0.0, 255.0) as u8;
            if alpha == 0 {
                out[offset..offset + 4].copy_from_slice(&[0, 0, 0, 0]);
            } else {
                let denominator = premult[3].max(1e-6);
                out[offset] = (premult[0] / denominator * 255.0).round().clamp(0.0, 255.0) as u8;
                out[offset + 1] =
                    (premult[1] / denominator * 255.0).round().clamp(0.0, 255.0) as u8;
                out[offset + 2] =
                    (premult[2] / denominator * 255.0).round().clamp(0.0, 255.0) as u8;
                out[offset + 3] = alpha;
            }
        }
    }
    out
}

fn premultiplied(source: &TemplateRegion, x: usize, y: usize) -> [f32; 4] {
    let offset = (y * source.width + x) * 4;
    let alpha = source.data[offset + 3] as f32 / 255.0;
    [
        source.data[offset] as f32 / 255.0 * alpha,
        source.data[offset + 1] as f32 / 255.0 * alpha,
        source.data[offset + 2] as f32 / 255.0 * alpha,
        alpha,
    ]
}

fn score_rgba_bgr(
    template: &[u8],
    width: usize,
    height: usize,
    frame: &BgrFrame,
) -> Option<VariantScore> {
    if frame.width != width || frame.height != height {
        return None;
    }
    let total = width * height;
    let mut template_gray = vec![0f32; total];
    let mut mask = vec![false; total];
    for index in 0..total {
        let offset = index * 4;
        let alpha = template[offset + 3];
        mask[index] = alpha >= 128;
        template_gray[index] = (0.299 * f32::from(template[offset])
            + 0.587 * f32::from(template[offset + 1])
            + 0.114 * f32::from(template[offset + 2]))
            / 255.0;
    }
    if mask.iter().filter(|value| **value).count() < MIN_MASK_PIXELS {
        return None;
    }
    let frame_gray = gray_bgr(frame);
    let color = ncc(&template_gray, &frame_gray, &mask)?;
    let template_masked: Vec<f32> = template_gray
        .iter()
        .zip(mask.iter())
        .map(|(value, visible)| if *visible { *value } else { 0.0 })
        .collect();
    let template_edges = gradient(&template_masked, width, height);
    let frame_edges = gradient(&frame_gray, width, height);
    let edge_mask: Vec<bool> = template_edges
        .iter()
        .map(|value| *value > EDGE_THRESHOLD as f32)
        .collect();
    let edge = if edge_mask.iter().filter(|value| **value).count() >= 50 {
        ncc(&template_edges, &frame_edges, &edge_mask)
            .map(|value| value.max(0.0))
            .unwrap_or(0.0)
    } else {
        0.0
    };
    let color = color.max(0.0);
    Some(VariantScore {
        color,
        edge,
        combined: 0.65 * color + 0.35 * edge,
    })
}

fn ncc(left: &[f32], right: &[f32], mask: &[bool]) -> Option<f64> {
    if left.len() != right.len() || left.len() != mask.len() {
        return None;
    }
    let mut count = 0usize;
    let mut left_sum = 0f64;
    let mut right_sum = 0f64;
    for index in 0..left.len() {
        if !mask[index] {
            continue;
        }
        count += 1;
        left_sum += f64::from(left[index]);
        right_sum += f64::from(right[index]);
    }
    if count < 50 {
        return None;
    }
    let left_mean = left_sum / count as f64;
    let right_mean = right_sum / count as f64;
    let mut covariance = 0f64;
    let mut left_variance = 0f64;
    let mut right_variance = 0f64;
    for index in 0..left.len() {
        if !mask[index] {
            continue;
        }
        let left_delta = f64::from(left[index]) - left_mean;
        let right_delta = f64::from(right[index]) - right_mean;
        covariance += left_delta * right_delta;
        left_variance += left_delta * left_delta;
        right_variance += right_delta * right_delta;
    }
    let denominator = (left_variance * right_variance).sqrt();
    (denominator > 1e-9).then_some(covariance / denominator)
}

fn gray_bgr(frame: &BgrFrame) -> Vec<f32> {
    let mut gray = Vec::with_capacity(frame.width * frame.height);
    for pixel in frame.data.chunks_exact(3) {
        gray.push(
            (0.114 * f32::from(pixel[0])
                + 0.587 * f32::from(pixel[1])
                + 0.299 * f32::from(pixel[2]))
                / 255.0,
        );
    }
    gray
}

fn gradient(gray: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut edges = vec![0f32; width * height];
    if width < 3 || height < 3 {
        return edges;
    }
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let index = y * width + x;
            let gx = gray[index + 1] - gray[index - 1];
            let gy = gray[index + width] - gray[index - width];
            edges[index] = (gx * gx + gy * gy).sqrt();
        }
    }
    edges
}

fn temporal_stability(samples: &[Vec<f32>], width: usize, height: usize) -> (f64, f64) {
    let total = width * height;
    if samples.is_empty() || total == 0 {
        return (0.0, 0.0);
    }
    let count = samples.len() as f32;
    let mut mean = vec![0f32; total];
    for sample in samples {
        for (index, value) in sample.iter().enumerate() {
            mean[index] += *value;
        }
    }
    for value in &mut mean {
        *value /= count;
    }
    let mut variance = vec![0f32; total];
    for sample in samples {
        for (index, value) in sample.iter().enumerate() {
            let delta = *value - mean[index];
            variance[index] += delta * delta;
        }
    }
    let stable = variance
        .iter()
        .filter(|value| (*value / count).sqrt() <= GENERIC_STABLE_STD as f32)
        .count() as f64
        / total as f64;
    let mut edge_counts = vec![0u16; total];
    for sample in samples {
        let edges = gradient(sample, width, height);
        for (index, value) in edges.iter().enumerate() {
            if f64::from(*value) > GENERIC_EDGE_THRESHOLD {
                edge_counts[index] = edge_counts[index].saturating_add(1);
            }
        }
    }
    let persistent = edge_counts
        .iter()
        .filter(|value| f64::from(**value) / count as f64 >= GENERIC_EDGE_PERSISTENCE)
        .count() as f64
        / total as f64;
    (stable, persistent)
}

fn bf_box_width(bbox: [f64; 4]) -> f64 {
    (bbox[2] - bbox[0]).max(1.0)
}

fn bf_box_height(bbox: [f64; 4]) -> f64 {
    (bbox[3] - bbox[1]).max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ncc_is_one_for_the_same_signal() {
        let signal: Vec<f32> = (0..64).map(|index| (index as f32 * 0.13).sin()).collect();
        let mask = vec![true; signal.len()];
        let value = ncc(&signal, &signal, &mask).unwrap();
        assert!((value - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ncc_rejects_a_constant_signal() {
        let signal = vec![0.5; 64];
        let mask = vec![true; 64];
        assert!(ncc(&signal, &signal, &mask).is_none());
    }

    #[test]
    fn gradient_is_zero_on_a_flat_image() {
        let gray = vec![0.5; 16];
        assert!(gradient(&gray, 4, 4).iter().all(|value| *value == 0.0));
    }

    #[test]
    fn temporal_stability_separates_a_static_pattern_from_noise() {
        let width = 8;
        let height = 4;
        let static_sample: Vec<f32> = (0..width * height)
            .map(|index| if index % width < width / 2 { 0.2 } else { 0.8 })
            .collect();
        let stable = temporal_stability(
            &[
                static_sample.clone(),
                static_sample.clone(),
                static_sample.clone(),
            ],
            width,
            height,
        );
        assert!(stable.0 > 0.99);
        assert!(stable.1 > 0.0);

        let noisy = vec![
            vec![0.1; width * height],
            vec![0.9; width * height],
            vec![0.4; width * height],
        ];
        let unstable = temporal_stability(&noisy, width, height);
        assert!(unstable.0 < 0.1);
    }

    #[test]
    fn resize_rgba_keeps_transparent_pixels_transparent() {
        let source = TemplateRegion {
            width: 1,
            height: 1,
            data: vec![10, 20, 30, 0],
        };
        let resized = resize_rgba(&source, 4, 4);
        assert!(resized.chunks_exact(4).all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn resize_rgba_preserves_an_opaque_color() {
        let source = TemplateRegion {
            width: 1,
            height: 1,
            data: vec![10, 20, 30, 255],
        };
        let resized = resize_rgba(&source, 2, 2);
        for pixel in resized.chunks_exact(4) {
            assert_eq!(pixel, [10, 20, 30, 255]);
        }
    }

    #[test]
    fn identical_template_and_frame_score_one() {
        let width = 32;
        let height = 24;
        let mut rgba = Vec::with_capacity(width * height * 4);
        let mut bgr = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                let value = ((x * 7 + y * 11) % 200 + 20) as u8;
                rgba.extend_from_slice(&[value, value, value, 255]);
                bgr.extend_from_slice(&[value, value, value]);
            }
        }
        let frame = BgrFrame::new(width, height, bgr).unwrap();
        let score = score_rgba_bgr(&rgba, width, height, &frame).unwrap();
        assert!((score.color - 1.0).abs() < 1e-6);
        assert!((score.combined - 1.0).abs() < 1e-6);
    }
}
