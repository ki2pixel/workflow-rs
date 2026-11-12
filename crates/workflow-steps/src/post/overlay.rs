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

use crate::post::ecproj;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use workflow_schema::round::round_to;

/// Seuil alpha qui définit la boîte visible (comme le prototype PoC).
const ALPHA_BBOX_THRESHOLD: u8 = 8;
/// Seuil alpha qui compte un pixel comme réellement opaque.
const ALPHA_OPAQUE_THRESHOLD: u8 = 200;

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
    /// presque entièrement transparent.
    pub opaque_pixels: usize,
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

/// Lit les overlays image d'un projet. Une erreur de projet est remontée ;
/// un overlay individuel illisible est seulement ignoré et listé.
pub fn read_mask(project: &Path) -> Result<OverlayMask> {
    // Un projet peut être un lien symbolique (corpus de validation, staging) :
    // les assets sont résolus depuis le dossier réel, pas depuis le lien.
    let resolved = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let doc = ecproj::load(&resolved)?;
    mask_from_doc(&doc, &resolved)
}

/// Variante pour les appelants qui ont déjà chargé le projet (A2).
pub fn mask_from_doc(doc: &Value, project: &Path) -> Result<OverlayMask> {
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
        if !is_image_asset(name) {
            continue;
        }
        let layer_id = layer.get("id").and_then(ecproj::as_i64).unwrap_or(0);
        match overlay_region(project, comp, layer, item, name, layer_id, &meta) {
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

fn overlay_region(
    project: &Path,
    comp: &Value,
    layer: &Value,
    item: &Value,
    name: &str,
    layer_id: i64,
    meta: &ecproj::CompMeta,
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

    let Some(asset) = resolve_asset(project, item) else {
        bail!("PNG introuvable");
    };
    let Some(bounds) = read_alpha_bounds(&asset)? else {
        bail!("PNG sans pixel visible");
    };

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
    if end <= start {
        bail!("intervalle vide");
    }

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
        end_s: round_to(end.min(meta.duration), 3),
        bbox_xywh: [
            round_to(x0, 2),
            round_to(y0, 2),
            round_to(x1 - x0, 2),
            round_to(y1 - y0, 2),
        ],
        opaque_pixels: bounds.opaque,
    }))
}

fn is_image_asset(name: &str) -> bool {
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "webp"
    )
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
}
