//! Masque des overlays intentionnels lu dans le `.ecproj`.
//!
//! Un export 9:16 contient des habillages ajoutés par le monteur (logo de
//! chaîne, sticker de template, titre d'émission). Ces PNG sont déclarés dans
//! la composition : leur zone alpha visible donne une région à ignorer lors de
//! la recherche de logos résiduels. Le masque ne modifie pas le projet ; il
//! sert au contrôle QA et à A2, qui peut ainsi savoir si un logo source est
//! caché par un habillage déclaré.
//!
//! Le cas courant est un PNG plein cadre presque transparent : seule la boîte
//! des pixels opaques compte, sinon le masque couvrirait toute l'image. Les
//! transformations sont prises en charge quand elles sont statiques ; un
//! calque animé ou pivoté est ignoré et signalé dans `skipped`.
//!
//! Depuis le Lot 12.3, les calques vidéo `.mov` porteurs d'alpha sont aussi
//! masqués. Leurs frames sont échantillonnées entre `in_point` et `out_point`
//! (12 à 24 frames, `ffmpeg` rawvideo RGBA), puis seules les boîtes stables
//! autour de la médiane sont fusionnées en une région `kind: "mov"`. Un MOV
//! animé ou multi-positions est ignoré : mieux vaut un masque incomplet,
//! signalé et non bloquant, qu'un masque qui couvrirait toute la frame.

use crate::post::ecproj;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use workflow_schema::round::round_to;

/// Seuil alpha qui définit la boîte visible (comme le prototype PoC).
const ALPHA_BBOX_THRESHOLD: u8 = 8;
/// Seuil alpha qui compte un pixel comme réellement opaque.
const ALPHA_OPAQUE_THRESHOLD: u8 = 200;
/// Nombre minimal de frames échantillonnées dans un MOV.
const MOV_SAMPLES_MIN: usize = 12;
/// Nombre maximal de frames échantillonnées dans un MOV.
const MOV_SAMPLES_MAX: usize = 24;
/// IoU minimale avec la boîte médiane pour retenir un échantillon stable.
const MOV_STABLE_IOU: f64 = 0.70;
/// Part minimale d'échantillons stables pour accepter la région MOV.
const MOV_STABLE_MIN_RATIO: f64 = 0.75;

fn default_overlay_kind() -> String {
    "image".into()
}

/// Overlay déclaré dans le `.ecproj`, réduit à sa zone alpha visible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayRegion {
    pub name: String,
    pub layer_id: i64,
    pub start_s: f64,
    pub end_s: f64,
    /// Boîte dans les coordonnées de la composition.
    pub bbox_xywh: [f64; 4],
    /// Pixels dont l'alpha est ≥ 200, pour distinguer un vrai logo d'un PNG
    /// presque entièrement transparent. Pour un MOV, maximum observé sur les
    /// échantillons stables.
    pub opaque_pixels: usize,
    /// `image` pour un PNG/JPG/WebP, `mov` pour un habillage vidéo.
    #[serde(default = "default_overlay_kind")]
    pub kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct OverlayMask {
    pub comp_width: u32,
    pub comp_height: u32,
    pub regions: Vec<OverlayRegion>,
    /// Overlays déclarés mais non exploitables (calque animé, PNG absent...).
    pub skipped: Vec<String>,
}

impl OverlayMask {
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Part du plus petit des deux rectangles (détection ou overlay) couverte
    /// par l'intersection. Une boîte OCR plus grande que l'overlay mais qui le
    /// contient est donc expliquée : la détection inclut du padding. Une valeur
    /// ≥ 0,5 signifie que la détection est expliquée par un habillage déclaré.
    pub fn overlap_ratio(
        &self,
        bbox: [f64; 4],
        time_s: f64,
        frame_width: u32,
        frame_height: u32,
    ) -> f64 {
        if self.comp_width == 0 || self.comp_height == 0 {
            return 0.0;
        }
        let scale_x = f64::from(frame_width) / f64::from(self.comp_width);
        let scale_y = f64::from(frame_height) / f64::from(self.comp_height);
        let detection_area = (bbox[2] * bbox[3]).max(1.0);
        self.regions
            .iter()
            .filter(|region| time_s >= region.start_s && time_s <= region.end_s)
            .map(|region| {
                let region_bbox = [
                    region.bbox_xywh[0] * scale_x,
                    region.bbox_xywh[1] * scale_y,
                    region.bbox_xywh[2] * scale_x,
                    region.bbox_xywh[3] * scale_y,
                ];
                let region_area = (region_bbox[2] * region_bbox[3]).max(1.0);
                intersection(bbox, region_bbox) / detection_area.min(region_area)
            })
            .fold(0.0, f64::max)
    }

    /// Part de `bbox` (détection) couverte par un overlay actif. Contrairement
    /// à `overlap_ratio`, le dénominateur est toujours la surface de la
    /// détection : un petit habillage ne peut pas « couvrir » un grand logo.
    /// Les régions sont évaluées séparément ; leur recouvrement mutuel n'est
    /// pas fusionné.
    pub fn coverage_ratio(
        &self,
        bbox: [f64; 4],
        time_s: f64,
        frame_width: u32,
        frame_height: u32,
    ) -> f64 {
        if self.comp_width == 0 || self.comp_height == 0 {
            return 0.0;
        }
        let scale_x = f64::from(frame_width) / f64::from(self.comp_width);
        let scale_y = f64::from(frame_height) / f64::from(self.comp_height);
        let detection_area = (bbox[2] * bbox[3]).max(1.0);
        self.regions
            .iter()
            .filter(|region| time_s >= region.start_s && time_s <= region.end_s)
            .map(|region| {
                let region_bbox = [
                    region.bbox_xywh[0] * scale_x,
                    region.bbox_xywh[1] * scale_y,
                    region.bbox_xywh[2] * scale_x,
                    region.bbox_xywh[3] * scale_y,
                ];
                intersection(bbox, region_bbox) / detection_area
            })
            .fold(0.0, f64::max)
            .min(1.0)
    }
}

/// Lit les overlays d'un projet avec les outils par défaut.
pub fn read_mask(project: &Path) -> Result<OverlayMask> {
    read_mask_with(project, Path::new("ffmpeg"), Path::new("ffprobe"))
}

/// Lit les overlays d'un projet avec des chemins d'outils explicites.
pub fn read_mask_with(project: &Path, ffmpeg: &Path, ffprobe: &Path) -> Result<OverlayMask> {
    // Un projet peut être un lien symbolique (corpus de validation, staging) :
    // les assets sont résolus depuis le dossier réel, pas depuis le lien.
    let resolved = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let doc = ecproj::load(&resolved)?;
    mask_from_doc_with(&doc, &resolved, ffmpeg, ffprobe)
}

/// Variante pour les appelants qui ont déjà chargé le projet (A2).
pub fn mask_from_doc(doc: &Value, project: &Path) -> Result<OverlayMask> {
    mask_from_doc_with(doc, project, Path::new("ffmpeg"), Path::new("ffprobe"))
}

/// Variante avec outils explicites, utilisée par A2 et le QA.
pub fn mask_from_doc_with(
    doc: &Value,
    project: &Path,
    ffmpeg: &Path,
    ffprobe: &Path,
) -> Result<OverlayMask> {
    let meta = ecproj::comp_meta(doc)?;
    let comp = ecproj::comp(doc)?;
    let items = doc
        .get("items")
        .and_then(Value::as_object)
        .context("projet sans objet items")?;
    let layers = comp
        .get("layers")
        .and_then(Value::as_array)
        .context("composition sans calques")?;

    let mut mask = OverlayMask {
        comp_width: meta.width,
        comp_height: meta.height,
        ..OverlayMask::default()
    };
    let mut mov_probes: HashMap<PathBuf, Option<MovInfo>> = HashMap::new();
    for layer in layers {
        let Some(item_id) = layer
            .get("source")
            .and_then(|source| source.get("item"))
            .and_then(ecproj::as_i64)
        else {
            continue;
        };
        let Some(item) = items.get(&item_id.to_string()) else {
            continue;
        };
        let Some(name) = item.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(kind) = overlay_asset_kind(name) else {
            continue;
        };
        let layer_id = layer.get("id").and_then(ecproj::as_i64).unwrap_or(0);
        match overlay_region(
            project,
            comp,
            layer,
            item,
            name,
            layer_id,
            &meta,
            kind,
            ffmpeg,
            ffprobe,
            &mut mov_probes,
        ) {
            Ok(Some(region)) => mask.regions.push(region),
            Ok(None) => {}
            Err(error) => {
                let reason = format!("{name}: {error:#}");
                tracing::warn!(overlay = name, error = %error, "overlay ignoré");
                mask.skipped.push(reason);
            }
        }
    }
    Ok(mask)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayAssetKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Copy)]
struct MovInfo {
    width: u32,
    height: u32,
    /// Durée du média, utilisée pour ramener une fenêtre de composition au
    /// temps source du MOV.
    duration: f64,
}

#[allow(clippy::too_many_arguments)]
fn overlay_region(
    project: &Path,
    comp: &Value,
    layer: &Value,
    item: &Value,
    name: &str,
    layer_id: i64,
    meta: &ecproj::CompMeta,
    kind: OverlayAssetKind,
    ffmpeg: &Path,
    ffprobe: &Path,
    mov_probes: &mut HashMap<PathBuf, Option<MovInfo>>,
) -> Result<Option<OverlayRegion>> {
    if layer.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Ok(None);
    }
    let opacity = transform_scalar(layer, "Opacity").unwrap_or(100.0);
    if opacity <= 0.0 {
        return Ok(None);
    }
    let rotation = transform_scalar(layer, "Rotation").unwrap_or(0.0);
    if rotation.abs() > 0.01 {
        bail!("calque pivoté ({rotation:.1}°) non pris en charge");
    }
    if transform_is_animated(layer) {
        bail!("transform animée non prise en charge");
    }
    let scale = ecproj::transform_prop(layer, "Scale")
        .and_then(ecproj::vec3)
        .map(|value| value[0] / 100.0)
        .context("calque sans Scale")?;
    if !scale.is_finite() || scale <= 0.0 {
        bail!("échelle invalide");
    }
    let anchor = ecproj::transform_prop(layer, "Anchor Point")
        .and_then(ecproj::vec3)
        .context("calque sans Anchor Point")?;
    let position = ecproj::transform_prop(layer, "Position")
        .and_then(ecproj::vec3)
        .context("calque sans Position")?;
    let (parent_dx, parent_dy) = parent_offset(comp, layer)?;

    let start = layer
        .get("in_point")
        .and_then(Value::as_f64)
        .map(|ticks| ticks / ecproj::TICKS_PER_SECOND)
        .unwrap_or(0.0);
    let end = layer
        .get("out_point")
        .and_then(Value::as_f64)
        .map(|ticks| ticks / ecproj::TICKS_PER_SECOND)
        .unwrap_or(meta.duration);
    let end = end.min(meta.duration);
    if end <= start {
        bail!("intervalle vide");
    }

    let Some(asset) = resolve_asset(project, item) else {
        if kind == OverlayAssetKind::Image {
            bail!("asset introuvable");
        }
        // Une vidéo source absente (segment, rush) n'est pas un habillage :
        // elle est ignorée sans polluer `skipped`.
        return Ok(None);
    };
    let bounds = match kind {
        OverlayAssetKind::Image => read_alpha_bounds(&asset)?,
        OverlayAssetKind::Video => {
            let info = match mov_probes.get(&asset) {
                Some(info) => *info,
                None => {
                    let info = probe_mov(&asset, ffprobe)?;
                    mov_probes.insert(asset.clone(), info);
                    info
                }
            };
            // Une vidéo sans alpha est du contenu, pas un habillage.
            let Some(info) = info else {
                return Ok(None);
            };
            let (sample_start, sample_end) = mov_source_window(info, start, end);
            sample_mov_bounds(ffmpeg, &asset, info, sample_start, sample_end)?
        }
    };
    let Some(bounds) = bounds else {
        match kind {
            OverlayAssetKind::Image => bail!("PNG sans pixel visible"),
            OverlayAssetKind::Video => bail!("MOV sans pixel visible"),
        }
    };

    let x = position[0] + parent_dx - anchor[0] * scale + bounds.x as f64 * scale;
    let y = position[1] + parent_dy - anchor[1] * scale + bounds.y as f64 * scale;
    let x0 = x.max(0.0);
    let y0 = y.max(0.0);
    let x1 = (x + bounds.width as f64 * scale).min(f64::from(meta.width));
    let y1 = (y + bounds.height as f64 * scale).min(f64::from(meta.height));
    if x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }

    Ok(Some(OverlayRegion {
        name: name.to_string(),
        layer_id,
        start_s: round_to(start, 3),
        end_s: round_to(end, 3),
        bbox_xywh: [
            round_to(x0, 2),
            round_to(y0, 2),
            round_to(x1 - x0, 2),
            round_to(y1 - y0, 2),
        ],
        opaque_pixels: bounds.opaque,
        kind: match kind {
            OverlayAssetKind::Image => "image".into(),
            OverlayAssetKind::Video => "mov".into(),
        },
    }))
}

fn overlay_asset_kind(name: &str) -> Option<OverlayAssetKind> {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    match extension.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "webp" => Some(OverlayAssetKind::Image),
        // Seuls les MOV sont des habillages A7 ; un MP4 reste du contenu.
        "mov" => Some(OverlayAssetKind::Video),
        _ => None,
    }
}

fn pix_fmt_has_alpha(pix_fmt: &str) -> bool {
    pix_fmt.starts_with("yuva")
        || pix_fmt.starts_with("rgba")
        || pix_fmt.starts_with("bgra")
        || pix_fmt.starts_with("argb")
        || pix_fmt.starts_with("abgr")
        || pix_fmt.starts_with("ya")
}

/// Vérifie qu'un MOV porte un canal alpha ; `None` pour une vidéo opaque.
fn probe_mov(path: &Path, ffprobe: &Path) -> Result<Option<MovInfo>> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,pix_fmt,duration:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .with_context(|| format!("ffprobe {}", path.display()))?;
    if !output.status.success() {
        bail!(
            "ffprobe a échoué: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let value: Value = serde_json::from_slice(&output.stdout).context("JSON ffprobe")?;
    let stream = value
        .get("streams")
        .and_then(Value::as_array)
        .and_then(|streams| streams.first())
        .context("aucun flux vidéo")?;
    let pix_fmt = stream
        .get("pix_fmt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !pix_fmt_has_alpha(pix_fmt) {
        return Ok(None);
    }
    let width = stream.get("width").and_then(Value::as_u64).unwrap_or(0) as u32;
    let height = stream.get("height").and_then(Value::as_u64).unwrap_or(0) as u32;
    if width == 0 || height == 0 {
        bail!("dimensions MOV invalides");
    }
    let duration = stream
        .get("duration")
        .and_then(json_f64)
        .or_else(|| {
            value
                .get("format")
                .and_then(|format| format.get("duration"))
                .and_then(json_f64)
        })
        .unwrap_or(0.0);
    Ok(Some(MovInfo {
        width,
        height,
        duration,
    }))
}

fn json_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.parse::<f64>().ok()))
}

/// Ramène la fenêtre de composition au temps source du MOV. Un end-card de
/// 3 s placé à 44 s dans la comp doit être échantillonné sur 0–3 s, pas sur
/// 44–47 s (sinon ffmpeg ne renvoie aucune frame).
fn mov_source_window(info: MovInfo, start: f64, end: f64) -> (f64, f64) {
    let layer_duration = (end - start).max(0.0);
    if info.duration <= 0.0 {
        return (start, end);
    }
    let source_start = if start >= info.duration { 0.0 } else { start };
    let source_end = (source_start + layer_duration).min(info.duration);
    (source_start, source_end)
}

/// Échantillonne `start..end` en une passe ffmpeg et fusionne les boîtes
/// stables. Le nombre de frames est borné à 12–24, soit environ 1 fps pour un
/// MOV de 12–24 s, avec un plancher pour les clips courts.
fn sample_mov_bounds(
    ffmpeg: &Path,
    path: &Path,
    info: MovInfo,
    start: f64,
    end: f64,
) -> Result<Option<AlphaBounds>> {
    let duration = end - start;
    if !duration.is_finite() || duration <= 0.0 {
        bail!("intervalle invalide");
    }
    let count = (duration.ceil() as usize).clamp(MOV_SAMPLES_MIN, MOV_SAMPLES_MAX);
    let fps = count as f64 / duration;
    let filter = format!("fps={fps:.6},format=rgba");
    let mut child = Command::new(ffmpeg)
        .args(["-v", "error", "-ss", &format!("{start:.3}"), "-i"])
        .arg(path)
        .args([
            "-t",
            &format!("{duration:.3}"),
            "-an",
            "-vf",
            &filter,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("ffmpeg MOV {}", path.display()))?;
    let stdout = child.stdout.take().context("stdout ffmpeg indisponible")?;
    let mut reader = BufReader::new(stdout);
    let width = info.width as usize;
    let height = info.height as usize;
    let mut frame = vec![0u8; width * height * 4];
    let mut samples = Vec::new();
    loop {
        match reader.read_exact(&mut frame) {
            Ok(()) => {
                if let Some(bounds) = alpha_bounds_from_rgba(&frame, width, height) {
                    samples.push(bounds);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => bail!("lecture frame MOV: {error}"),
        }
    }
    drop(reader);
    let status = child
        .wait()
        .with_context(|| format!("attente ffmpeg {}", path.display()))?;
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    if !status.success() {
        bail!("ffmpeg a échoué ({status}): {}", stderr.trim());
    }
    if samples.is_empty() {
        return Ok(None);
    }
    Ok(Some(merge_stable_bounds(&samples)?))
}

/// Boîte des pixels visibles d'une frame RGBA.
fn alpha_bounds_from_rgba(data: &[u8], width: usize, height: usize) -> Option<AlphaBounds> {
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut opaque = 0usize;
    let mut visible = false;
    for y in 0..height {
        for x in 0..width {
            let alpha = data[(y * width + x) * 4 + 3];
            if alpha > ALPHA_BBOX_THRESHOLD {
                visible = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            if alpha >= ALPHA_OPAQUE_THRESHOLD {
                opaque += 1;
            }
        }
    }
    if !visible {
        return None;
    }
    Some(AlphaBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
        opaque,
    })
}

/// Fusionne les échantillons proches de la boîte médiane. Les frames
/// aberrantes (flash plein cadre, élément qui traverse l'écran) sont écartées ;
/// si trop peu de frames sont stables, le MOV est refusé.
fn merge_stable_bounds(samples: &[AlphaBounds]) -> Result<AlphaBounds> {
    if samples.is_empty() {
        bail!("aucun échantillon visible");
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by_key(|bounds| bounds.width * bounds.height);
    let median = sorted[sorted.len() / 2];
    let stable: Vec<&AlphaBounds> = samples
        .iter()
        .filter(|bounds| bounds_iou(bounds, &median) >= MOV_STABLE_IOU)
        .collect();
    let required = ((samples.len() as f64 * MOV_STABLE_MIN_RATIO).ceil() as usize)
        .max(1)
        .min(samples.len());
    if stable.len() < required {
        bail!(
            "MOV instable: {}/{} échantillons stables",
            stable.len(),
            samples.len()
        );
    }
    let x0 = stable.iter().map(|bounds| bounds.x).min().unwrap_or(0);
    let y0 = stable.iter().map(|bounds| bounds.y).min().unwrap_or(0);
    let x1 = stable
        .iter()
        .map(|bounds| bounds.x + bounds.width)
        .max()
        .unwrap_or(0);
    let y1 = stable
        .iter()
        .map(|bounds| bounds.y + bounds.height)
        .max()
        .unwrap_or(0);
    Ok(AlphaBounds {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
        opaque: stable.iter().map(|bounds| bounds.opaque).max().unwrap_or(0),
    })
}

fn bounds_iou(a: &AlphaBounds, b: &AlphaBounds) -> f64 {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.width).min(b.x + b.width);
    let bottom = (a.y + a.height).min(b.y + b.height);
    let intersection = (right.saturating_sub(left) * bottom.saturating_sub(top)) as f64;
    let union = (a.width * a.height + b.width * b.height) as f64 - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn transform_scalar(layer: &Value, name: &str) -> Option<f64> {
    let value = ecproj::transform_prop(layer, name)?
        .get("value")?
        .get("v")?;
    value
        .as_f64()
        .or_else(|| value.as_array()?.first()?.as_f64())
}

fn transform_is_animated(layer: &Value) -> bool {
    ["Anchor Point", "Position", "Scale"]
        .iter()
        .filter_map(|name| ecproj::transform_prop(layer, name))
        .any(|prop| !ecproj::keys(prop).is_empty())
}

/// Décalage statique d'un calque parent (typiquement un Null). Un parent
/// animé est refusé : le masque ne pourrait pas suivre.
fn parent_offset(comp: &Value, layer: &Value) -> Result<(f64, f64)> {
    let Some(parent_id) = layer.get("parent").and_then(ecproj::as_i64) else {
        return Ok((0.0, 0.0));
    };
    let parent = ecproj::layer_by_id(comp, parent_id).context("calque parent introuvable")?;
    if transform_is_animated(parent) {
        bail!("calque parent animé non pris en charge");
    }
    let position = ecproj::transform_prop(parent, "Position")
        .and_then(ecproj::vec3)
        .unwrap_or([0.0; 3]);
    let anchor = ecproj::transform_prop(parent, "Anchor Point")
        .and_then(ecproj::vec3)
        .unwrap_or([0.0; 3]);
    let scale = ecproj::transform_prop(parent, "Scale")
        .and_then(ecproj::vec3)
        .map(|value| value[0] / 100.0)
        .unwrap_or(1.0);
    Ok((
        (position[0] - anchor[0]) * scale,
        (position[1] - anchor[1]) * scale,
    ))
}

/// Résout le PNG : chemin déclaré, puis `docs/` à côté du dossier `projets/`,
/// puis le dossier du projet. Le corpus de validation stocke les habillages
/// dans `docs/`, alors que le projet publié pointe vers `/mnt/cache`.
fn resolve_asset(project: &Path, item: &Value) -> Option<PathBuf> {
    let name = item.get("name").and_then(Value::as_str)?;
    let file_name = Path::new(name).file_name()?;
    let mut candidates = Vec::new();
    if let Some(declared) = item
        .get("kind")
        .and_then(|kind| kind.get("path"))
        .and_then(Value::as_str)
    {
        let declared = PathBuf::from(declared);
        if declared.is_absolute() {
            candidates.push(declared);
        } else if let Some(parent) = project.parent() {
            // Un chemin relatif déclaré (`../docs/x.png`) est relatif au
            // projet, pas au répertoire courant du processus.
            candidates.push(parent.join(declared));
        }
    }
    if let Some(parent) = project.parent() {
        candidates.push(parent.join("docs").join(file_name));
        candidates.push(parent.join("..").join("docs").join(file_name));
        candidates.push(parent.join(file_name));
        candidates.push(parent.join("..").join(file_name));
    }
    candidates.into_iter().find(|path| path.is_file())
}

#[derive(Debug, Clone, Copy)]
struct AlphaBounds {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    opaque: usize,
}

fn read_alpha_bounds(path: &Path) -> Result<Option<AlphaBounds>> {
    let file =
        std::fs::File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
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
        // Un asset opaque n'a pas de zone visible : le masquer couvrirait
        // toute la frame et supprimerait le contrôle logos.
        png::ColorType::Rgb | png::ColorType::Grayscale => bail!("asset sans canal alpha"),
        png::ColorType::Indexed => bail!("PNG indexé non développé"),
    };
    let width = info.width as usize;
    let height = info.height as usize;
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut opaque = 0usize;
    let mut visible = false;
    for y in 0..height {
        for x in 0..width {
            let offset = (y * width + x) * channels;
            let alpha = if channels == 4 {
                data[offset + 3]
            } else {
                data[offset + 1]
            };
            if alpha > ALPHA_BBOX_THRESHOLD {
                visible = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
            if alpha >= ALPHA_OPAQUE_THRESHOLD {
                opaque += 1;
            }
        }
    }
    if !visible {
        return Ok(None);
    }
    Ok(Some(AlphaBounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
        opaque,
    }))
}

fn intersection(a: [f64; 4], b: [f64; 4]) -> f64 {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    (right - left).max(0.0) * (bottom - top).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use std::io::BufWriter;
    use tempfile::tempdir;

    const TICKS: f64 = ecproj::TICKS_PER_SECOND;

    fn write_png(path: &Path, width: u32, height: u32, opaque: &[(u32, u32)]) {
        let file = fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut data = vec![0u8; width as usize * height as usize * 4];
        for (x, y) in opaque {
            let offset = (*y as usize * width as usize + *x as usize) * 4;
            data[offset] = 255;
            data[offset + 1] = 255;
            data[offset + 2] = 255;
            data[offset + 3] = 255;
        }
        writer.write_image_data(&data).unwrap();
    }

    fn write_png_rgb(path: &Path, width: u32, height: u32) {
        let file = fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![255u8; width as usize * height as usize * 3])
            .unwrap();
    }

    fn tools_available() -> bool {
        ["ffmpeg", "ffprobe"].iter().all(|tool| {
            Command::new(tool)
                .arg("-version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false)
        })
    }

    /// Génère un MOV alpha 1080×1920 avec une boîte opaque 200×100 en
    /// (100, 300). Retourne `false` si ffmpeg est absent.
    fn write_alpha_mov(path: &Path) -> bool {
        if !tools_available() {
            return false;
        }
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=1080x1920:d=2:r=25",
                "-vf",
                "format=rgba,geq=r='255':g='255':b='255':a='if(between(X,100,299)*between(Y,300,399),255,0)'",
                "-c:v",
                "qtrle",
                "-pix_fmt",
                "argb",
            ])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
        true
    }

    fn write_opaque_mov(path: &Path) -> bool {
        if !tools_available() {
            return false;
        }
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=64x64:d=1:r=10",
                "-c:v",
                "qtrle",
                "-pix_fmt",
                "rgb24",
            ])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
        true
    }

    fn bounds(x: usize, y: usize, width: usize, height: usize) -> AlphaBounds {
        AlphaBounds {
            x,
            y,
            width,
            height,
            opaque: width * height,
        }
    }

    fn project(name: &str, asset: &str, scale: f64) -> Value {
        json!({
            "items": {
                "1": {"name": "clip.mp4", "kind": {"type": "Footage"}},
                "2": {
                    "name": "Comp",
                    "kind": {
                        "type": "Comp",
                        "width": 1080,
                        "height": 1920,
                        "frame_rate": {"num": 25, "den": 1},
                        "duration": 2540160000000_i64,
                        "layers": [
                            {
                                "id": 10,
                                "name": name,
                                "in_point": 0,
                                "out_point": 2540160000000_i64,
                                "source": {"type": "Footage", "item": 3},
                                "props": {"children": [{"name": "Transform", "children": [
                                    {"name": "Anchor Point", "value": {"v": [540.0, 960.0, 0.0]}},
                                    {"name": "Position", "value": {"v": [540.0, 960.0, 0.0]}},
                                    {"name": "Scale", "value": {"v": [scale, scale, 0.0]}}
                                ]}]}
                            }
                        ]
                    }
                },
                "3": {
                    "name": asset,
                    "kind": {
                        "type": "Footage",
                        "path": format!("/mnt/cache/docs/{asset}"),
                        "width": 1080,
                        "height": 1920
                    }
                }
            }
        })
    }

    #[test]
    fn mask_uses_only_the_alpha_bbox() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        write_png(&docs.join("logo.png"), 8, 8, &[(2, 3), (5, 6)]);
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(
            &project_path,
            serde_json::to_vec(&project("logo.png", "logo.png", 100.0)).unwrap(),
        )
        .unwrap();

        let mask = read_mask(&project_path).unwrap();
        assert_eq!(mask.regions.len(), 1);
        let region = &mask.regions[0];
        assert_eq!(region.bbox_xywh, [2.0, 3.0, 4.0, 4.0]);
        assert_eq!(region.opaque_pixels, 2);
        assert!((region.start_s - 0.0).abs() < 1e-9);
        assert!((region.end_s - 10.0).abs() < 1e-9);
        assert!(mask.skipped.is_empty());
    }

    #[test]
    fn mask_applies_scale_and_position() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        write_png(&docs.join("logo.png"), 8, 8, &[(2, 3)]);
        let mut doc = project("logo.png", "logo.png", 200.0);
        let layer = &mut doc["items"]["2"]["kind"]["layers"][0];
        layer["props"]["children"][0]["children"][0]["value"]["v"] = json!([4.0, 4.0, 0.0]);
        layer["props"]["children"][0]["children"][1]["value"]["v"] = json!([104.0, 204.0, 0.0]);
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(&project_path, serde_json::to_vec(&doc).unwrap()).unwrap();

        let mask = read_mask(&project_path).unwrap();
        assert_eq!(mask.regions[0].bbox_xywh, [100.0, 202.0, 2.0, 2.0]);
    }

    #[test]
    fn overlap_ignores_inactive_overlays() {
        let mask = OverlayMask {
            comp_width: 1080,
            comp_height: 1920,
            regions: vec![OverlayRegion {
                name: "logo.png".into(),
                layer_id: 10,
                start_s: 0.0,
                end_s: 5.0,
                bbox_xywh: [80.0, 280.0, 200.0, 100.0],
                opaque_pixels: 10,
                kind: "image".into(),
            }],
            skipped: Vec::new(),
        };
        assert!(
            (mask.overlap_ratio([100.0, 300.0, 100.0, 50.0], 2.0, 1080, 1920) - 1.0).abs() < 1e-9
        );
        assert_eq!(
            mask.overlap_ratio([100.0, 300.0, 100.0, 50.0], 6.0, 1080, 1920),
            0.0
        );
        assert_eq!(
            mask.overlap_ratio([100.0, 300.0, 100.0, 50.0], 2.0, 2160, 3840),
            0.0
        );
        assert!(
            (mask.overlap_ratio([60.0, 530.0, 200.0, 100.0], 2.0, 2160, 3840) - 0.35).abs() < 1e-9
        );
    }

    #[test]
    fn overlap_accepts_a_detection_that_contains_the_overlay() {
        let mask = OverlayMask {
            comp_width: 1080,
            comp_height: 1920,
            regions: vec![OverlayRegion {
                name: "m6+.png".into(),
                layer_id: 11,
                start_s: 0.0,
                end_s: 6.0,
                bbox_xywh: [869.0, 341.0, 168.0, 87.0],
                opaque_pixels: 100,
                kind: "image".into(),
            }],
            skipped: Vec::new(),
        };
        // Boîte OCR avec padding : elle contient entièrement le logo.
        let ratio = mask.overlap_ratio([844.0, 307.0, 218.0, 150.0], 2.0, 1080, 1920);
        assert!((ratio - 1.0).abs() < 1e-9);
        // Détection à moitié dans l'overlay : ratio 0,5.
        let partial = mask.overlap_ratio([785.0, 341.0, 168.0, 87.0], 2.0, 1080, 1920);
        assert!((partial - 0.5).abs() < 1e-9);
    }

    #[test]
    fn coverage_ratio_uses_the_detection_area() {
        let mask = OverlayMask {
            comp_width: 1080,
            comp_height: 1920,
            regions: vec![OverlayRegion {
                name: "habillage.png".into(),
                layer_id: 42,
                start_s: 1.0,
                end_s: 3.0,
                bbox_xywh: [100.0, 300.0, 50.0, 100.0],
                opaque_pixels: 5000,
                kind: "image".into(),
            }],
            skipped: Vec::new(),
        };
        // La détection fait 200x100 ; l'overlay n'en couvre que 50x100.
        let coverage = mask.coverage_ratio([100.0, 300.0, 200.0, 100.0], 2.0, 1080, 1920);
        assert!((coverage - 0.25).abs() < 1e-9);
        // `overlap_ratio` répond 1,0 car l'overlay est plus petit que la
        // détection : les deux mesures ne sont pas interchangeables.
        let overlap = mask.overlap_ratio([100.0, 300.0, 200.0, 100.0], 2.0, 1080, 1920);
        assert!((overlap - 1.0).abs() < 1e-9);
        // Hors de la fenêtre temporelle de l'overlay.
        assert_eq!(
            mask.coverage_ratio([100.0, 300.0, 200.0, 100.0], 6.0, 1080, 1920),
            0.0
        );
    }

    #[test]
    fn relative_declared_path_is_resolved_from_the_project_folder() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        write_png(&docs.join("logo.png"), 8, 8, &[(2, 3), (5, 6)]);
        let projets = dir.path().join("projets");
        fs::create_dir_all(&projets).unwrap();
        let project_path = projets.join("clip_9x16.ecproj");
        fs::write(
            &project_path,
            serde_json::to_vec(&project("logo.png", "../docs/logo.png", 100.0)).unwrap(),
        )
        .unwrap();
        let mask = read_mask(&project_path).unwrap();
        assert_eq!(mask.regions.len(), 1);
        assert_eq!(mask.regions[0].bbox_xywh, [2.0, 3.0, 4.0, 4.0]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_project_resolves_assets_from_the_real_folder() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real");
        let docs = real.join("docs");
        fs::create_dir_all(&docs).unwrap();
        write_png(&docs.join("logo.png"), 8, 8, &[(2, 3), (5, 6)]);
        let real_projets = real.join("projets");
        fs::create_dir_all(&real_projets).unwrap();
        let real_project = real_projets.join("clip_9x16.ecproj");
        fs::write(
            &real_project,
            serde_json::to_vec(&project("logo.png", "logo.png", 100.0)).unwrap(),
        )
        .unwrap();

        let staging = dir.path().join("staging");
        let staging_projets = staging.join("projets");
        fs::create_dir_all(&staging_projets).unwrap();
        let linked = staging_projets.join("clip_9x16.ecproj");
        std::os::unix::fs::symlink(&real_project, &linked).unwrap();

        let mask = read_mask(&linked).unwrap();
        assert_eq!(mask.regions.len(), 1);
        assert_eq!(mask.regions[0].bbox_xywh, [2.0, 3.0, 4.0, 4.0]);
    }

    #[test]
    fn opaque_asset_without_alpha_is_refused() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        write_png_rgb(&docs.join("opaque.png"), 8, 8);
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(
            &project_path,
            serde_json::to_vec(&project("opaque.png", "opaque.png", 100.0)).unwrap(),
        )
        .unwrap();

        let mask = read_mask(&project_path).unwrap();
        assert!(mask.regions.is_empty());
        assert_eq!(mask.skipped.len(), 1);
        assert!(mask.skipped[0].contains("canal alpha"));
    }

    #[test]
    fn missing_asset_is_reported_as_skipped() {
        let dir = tempdir().unwrap();
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(
            &project_path,
            serde_json::to_vec(&project("absent.png", "absent.png", 100.0)).unwrap(),
        )
        .unwrap();
        let mask = read_mask(&project_path).unwrap();
        assert!(mask.regions.is_empty());
        assert_eq!(mask.skipped.len(), 1);
        assert!(mask.skipped[0].contains("absent.png"));
    }

    #[test]
    fn ticks_match_the_after_effects_timebase() {
        assert!((TICKS - 254_016_000_000.0).abs() < 1e-9);
    }

    #[test]
    fn legacy_region_without_kind_deserializes_as_image() {
        let region: OverlayRegion = serde_json::from_value(json!({
            "name": "logo.png",
            "layer_id": 10,
            "start_s": 0.0,
            "end_s": 5.0,
            "bbox_xywh": [80.0, 280.0, 200.0, 100.0],
            "opaque_pixels": 100
        }))
        .unwrap();
        assert_eq!(region.kind, "image");
    }

    #[test]
    fn stable_samples_ignore_an_outlier_frame() {
        let mut samples = vec![bounds(100, 300, 200, 100); 11];
        samples.push(bounds(0, 0, 1080, 1920));
        let merged = merge_stable_bounds(&samples).unwrap();
        assert_eq!(
            (merged.x, merged.y, merged.width, merged.height),
            (100, 300, 200, 100)
        );
        assert_eq!(merged.opaque, 20_000);
    }

    #[test]
    fn mov_window_maps_timeline_to_source_time() {
        let info = MovInfo {
            width: 10,
            height: 10,
            duration: 3.0,
        };
        assert_eq!(mov_source_window(info, 44.4, 47.4), (0.0, 3.0));
        assert_eq!(mov_source_window(info, 1.0, 5.0), (1.0, 3.0));
        assert_eq!(mov_source_window(info, 0.0, 2.0), (0.0, 2.0));
    }

    #[test]
    fn unstable_samples_are_refused() {
        let mut samples = Vec::new();
        for index in 0..12 {
            samples.push(if index % 2 == 0 {
                bounds(0, 300, 200, 100)
            } else {
                bounds(800, 300, 200, 100)
            });
        }
        let error = merge_stable_bounds(&samples).unwrap_err();
        assert!(error.to_string().contains("MOV instable"), "{error:#}");
    }

    #[test]
    fn mov_layer_is_sampled_and_classified() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        if !write_alpha_mov(&docs.join("overlay.mov")) {
            eprintln!("ffmpeg absent, test MOV ignoré");
            return;
        }
        let mut doc = project("overlay.mov", "overlay.mov", 100.0);
        doc["items"]["2"]["kind"]["layers"][0]["out_point"] = json!((TICKS * 2.0) as i64);
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(&project_path, serde_json::to_vec(&doc).unwrap()).unwrap();

        let mask = read_mask(&project_path).unwrap();
        assert_eq!(mask.regions.len(), 1, "{:?}", mask.skipped);
        let region = &mask.regions[0];
        assert_eq!(region.kind, "mov");
        assert_eq!(region.bbox_xywh, [100.0, 300.0, 200.0, 100.0]);
        assert_eq!(region.opaque_pixels, 20_000);
        assert!((region.start_s - 0.0).abs() < 1e-9);
        assert!((region.end_s - 2.0).abs() < 1e-9);
        assert!(mask.skipped.is_empty());
    }

    #[test]
    fn mov_layer_placed_late_uses_source_time() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        if !write_alpha_mov(&docs.join("overlay.mov")) {
            eprintln!("ffmpeg absent, test MOV ignoré");
            return;
        }
        let mut doc = project("overlay.mov", "overlay.mov", 100.0);
        doc["items"]["2"]["kind"]["duration"] = json!((TICKS * 47.0) as i64);
        let layer = &mut doc["items"]["2"]["kind"]["layers"][0];
        layer["in_point"] = json!((TICKS * 44.0) as i64);
        layer["out_point"] = json!((TICKS * 46.0) as i64);
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(&project_path, serde_json::to_vec(&doc).unwrap()).unwrap();

        let mask = read_mask(&project_path).unwrap();
        assert_eq!(mask.regions.len(), 1, "{:?}", mask.skipped);
        let region = &mask.regions[0];
        assert_eq!(region.kind, "mov");
        assert_eq!(region.bbox_xywh, [100.0, 300.0, 200.0, 100.0]);
        assert!((region.start_s - 44.0).abs() < 1e-9);
        assert!((region.end_s - 46.0).abs() < 1e-9);
    }

    #[test]
    fn opaque_mov_is_content_not_an_overlay() {
        let dir = tempdir().unwrap();
        let docs = dir.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        if !write_opaque_mov(&docs.join("source.mov")) {
            eprintln!("ffmpeg absent, test MOV ignoré");
            return;
        }
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(
            &project_path,
            serde_json::to_vec(&project("source.mov", "source.mov", 100.0)).unwrap(),
        )
        .unwrap();
        let mask = read_mask(&project_path).unwrap();
        assert!(mask.regions.is_empty());
        assert!(mask.skipped.is_empty());
    }

    #[test]
    fn missing_mov_is_ignored_silently() {
        let dir = tempdir().unwrap();
        let project_path = dir.path().join("clip_9x16.ecproj");
        fs::write(
            &project_path,
            serde_json::to_vec(&project("absent.mov", "absent.mov", 100.0)).unwrap(),
        )
        .unwrap();
        let mask = read_mask(&project_path).unwrap();
        assert!(mask.regions.is_empty());
        assert!(mask.skipped.is_empty());
    }
}
