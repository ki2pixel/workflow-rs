//! Lecture et écriture minimales des projets EffectCraft (`.ecproj`).
//!
//! Le post-traitement ne doit jamais réécrire le projet brut : il lit une
//! copie en mémoire, applique éventuellement une décision, puis publie un
//! second projet `_assist.ecproj`. Les fonctions de ce module conservent
//! l'ordre des clés de `serde_json` (`preserve_order`) et la timebase
//! After Effects.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use workflow_schema::round::round_to;

/// Timebase des projets After Effects / EffectCraft.
pub const TICKS_PER_SECOND: f64 = 254_016_000_000.0;

#[derive(Debug, Clone, Serialize)]
pub struct CompMeta {
    pub id: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub n_layers: usize,
    pub n_segments: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Keyframe {
    pub t: f64,
    pub v: [f64; 3],
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub name: String,
    pub layer_id: i64,
    pub parent: Option<i64>,
    pub start: f64,
    pub end: f64,
    pub source: Option<String>,
    pub anchor: [f64; 3],
    pub scale: f64,
    pub position: [f64; 3],
    pub position_keys: Vec<Keyframe>,
    pub scale_keys: Vec<Keyframe>,
    pub anchor_keys: Vec<Keyframe>,
    pub comp_x: f64,
    pub parent_dy: f64,
    pub null_parent: bool,
    pub parent_keyframed: bool,
}

impl Segment {
    pub fn duration(&self) -> f64 {
        self.end - self.start
    }

    pub fn keyframed(&self) -> bool {
        !self.position_keys.is_empty()
            || !self.scale_keys.is_empty()
            || !self.anchor_keys.is_empty()
    }
}

pub fn load(path: &Path) -> Result<Value> {
    let bytes =
        std::fs::read(path).with_context(|| format!("lecture du projet {}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("projet JSON invalide {}", path.display()))?;
    if !value.is_object() {
        bail!("projet invalide (objet JSON attendu): {}", path.display());
    }
    Ok(value)
}

pub fn comp_id(doc: &Value) -> Result<String> {
    let items = doc
        .get("items")
        .and_then(Value::as_object)
        .context("projet sans objet items")?;
    let mut best: Option<(usize, String)> = None;
    for (id, item) in items {
        let Some(kind) = item.get("kind") else {
            continue;
        };
        if kind.get("type").and_then(Value::as_str) != Some("Comp") {
            continue;
        }
        let count = kind
            .get("layers")
            .and_then(Value::as_array)
            .map(|layers| {
                layers
                    .iter()
                    .filter(|layer| {
                        layer_name(layer).is_some_and(|name| name.starts_with("Segment"))
                    })
                    .count()
            })
            .unwrap_or(0);
        if best.as_ref().is_none_or(|(current, _)| count > *current) {
            best = Some((count, id.clone()));
        }
    }
    best.map(|(_, id)| id)
        .context("aucune composition dans le projet")
}

pub fn comp(doc: &Value) -> Result<&Value> {
    let id = comp_id(doc)?;
    doc.get("items")
        .and_then(|items| items.get(&id))
        .and_then(|item| item.get("kind"))
        .context("composition introuvable")
}

pub fn comp_mut(doc: &mut Value) -> Result<&mut Value> {
    let id = comp_id(doc)?;
    doc.get_mut("items")
        .and_then(|items| items.get_mut(&id))
        .and_then(|item| item.get_mut("kind"))
        .context("composition introuvable")
}

pub fn comp_meta(doc: &Value) -> Result<CompMeta> {
    let id = comp_id(doc)?;
    let comp = comp(doc)?;
    let width = comp
        .get("width")
        .and_then(Value::as_u64)
        .context("composition sans largeur")? as u32;
    let height = comp
        .get("height")
        .and_then(Value::as_u64)
        .context("composition sans hauteur")? as u32;
    let fps = comp
        .get("frame_rate")
        .and_then(|rate| {
            let num = rate.get("num").and_then(Value::as_f64)?;
            let den = rate.get("den").and_then(Value::as_f64)?;
            (den != 0.0).then_some(num / den)
        })
        .unwrap_or(0.0);
    let duration = comp
        .get("duration")
        .and_then(Value::as_f64)
        .map(|ticks| ticks / TICKS_PER_SECOND)
        .unwrap_or(0.0);
    let layers = comp
        .get("layers")
        .and_then(Value::as_array)
        .context("composition sans calques")?;
    Ok(CompMeta {
        id,
        width,
        height,
        fps,
        duration,
        n_layers: layers.len(),
        n_segments: layers
            .iter()
            .filter(|layer| layer_name(layer).is_some_and(|name| name.starts_with("Segment")))
            .count(),
    })
}

pub fn segments(doc: &Value) -> Result<Vec<Segment>> {
    let comp = comp(doc)?;
    let items = doc
        .get("items")
        .and_then(Value::as_object)
        .context("projet sans objet items")?;
    let layers = comp
        .get("layers")
        .and_then(Value::as_array)
        .context("composition sans calques")?;
    let mut out = Vec::new();
    for layer in layers {
        let Some(name) = layer_name(layer) else {
            continue;
        };
        if !name.starts_with("Segment") {
            continue;
        }
        let Some(transform) = transform(layer) else {
            continue;
        };
        let anchor = transform_prop(layer, "Anchor Point")
            .and_then(vec3)
            .context("Segment sans Anchor Point")?;
        let position = transform_prop(layer, "Position")
            .and_then(vec3)
            .context("Segment sans Position")?;
        let scale = transform_prop(layer, "Scale")
            .and_then(vec3)
            .map(|value| value[0])
            .context("Segment sans Scale")?;
        let _ = transform;
        let start = layer
            .get("in_point")
            .and_then(Value::as_f64)
            .map(|ticks| ticks / TICKS_PER_SECOND)
            .unwrap_or(0.0);
        let end = layer
            .get("out_point")
            .and_then(Value::as_f64)
            .map(|ticks| ticks / TICKS_PER_SECOND)
            .unwrap_or(0.0);
        let source = layer
            .get("source")
            .and_then(|source| source.get("item"))
            .and_then(as_i64)
            .and_then(|item_id| items.get(&item_id.to_string()))
            .and_then(|item| item.get("name"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let parent = parent_info(comp, layer);
        let position_keys = transform_prop(layer, "Position")
            .map(keys)
            .unwrap_or_default();
        let scale_keys = transform_prop(layer, "Scale").map(keys).unwrap_or_default();
        let anchor_keys = transform_prop(layer, "Anchor Point")
            .map(keys)
            .unwrap_or_default();
        out.push(Segment {
            name: name.to_string(),
            layer_id: layer.get("id").and_then(as_i64).unwrap_or(0),
            parent: layer.get("parent").and_then(as_i64),
            start,
            end,
            source,
            anchor,
            scale,
            position,
            position_keys,
            scale_keys,
            anchor_keys,
            comp_x: position[0] + parent.dx,
            parent_dy: parent.dy,
            null_parent: parent.null_parent,
            parent_keyframed: parent.keyframed,
        });
    }
    out.sort_by(|a, b| {
        a.start
            .total_cmp(&b.start)
            .then(a.end.total_cmp(&b.end))
            .then(a.name.cmp(&b.name))
    });
    Ok(out)
}

struct ParentInfo {
    dx: f64,
    dy: f64,
    null_parent: bool,
    keyframed: bool,
}

impl Default for ParentInfo {
    fn default() -> Self {
        Self {
            dx: 0.0,
            dy: 0.0,
            null_parent: false,
            keyframed: false,
        }
    }
}

fn parent_info(comp: &Value, layer: &Value) -> ParentInfo {
    let Some(parent_id) = layer.get("parent").and_then(as_i64) else {
        return ParentInfo::default();
    };
    let Some(parent) = comp
        .get("layers")
        .and_then(Value::as_array)
        .and_then(|layers| {
            layers
                .iter()
                .find(|candidate| candidate.get("id").and_then(as_i64) == Some(parent_id))
        })
    else {
        return ParentInfo::default();
    };
    let null_parent = parent
        .get("source")
        .and_then(|source| source.get("type"))
        .and_then(Value::as_str)
        == Some("Null");
    let Some(_) = transform(parent) else {
        return ParentInfo {
            null_parent: true,
            ..ParentInfo::default()
        };
    };
    let position = transform_prop(parent, "Position")
        .and_then(vec3)
        .unwrap_or([0.0; 3]);
    let anchor = transform_prop(parent, "Anchor Point")
        .and_then(vec3)
        .unwrap_or([0.0; 3]);
    let scale = transform_prop(parent, "Scale")
        .and_then(vec3)
        .map(|value| value[0] / 100.0)
        .unwrap_or(1.0);
    let keyframed = transform_prop(parent, "Position")
        .map(|prop| !keys(prop).is_empty())
        .unwrap_or(false)
        || transform_prop(parent, "Scale")
            .map(|prop| !keys(prop).is_empty())
            .unwrap_or(false)
        || transform_prop(parent, "Anchor Point")
            .map(|prop| !keys(prop).is_empty())
            .unwrap_or(false);
    ParentInfo {
        dx: (position[0] - anchor[0]) * scale,
        dy: (position[1] - anchor[1]) * scale,
        null_parent,
        keyframed,
    }
}

pub fn project_stem(project: &Path, segments: &[Segment]) -> String {
    if let Some(source) = segments
        .iter()
        .find_map(|segment| segment.source.as_deref())
        .filter(|source| !source.is_empty())
    {
        if let Some(stem) = Path::new(source).file_stem().and_then(|stem| stem.to_str()) {
            return stem.to_string();
        }
    }
    let stem = project
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("projet");
    stem.strip_suffix("_9x16").unwrap_or(stem).to_string()
}

pub fn assist_path(project: &Path, suffix: &str) -> Result<PathBuf> {
    if suffix.is_empty() {
        bail!("suffixe de projet assisté vide");
    }
    let stem = project
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("nom de projet non UTF-8")?;
    if stem.ends_with(suffix) {
        bail!("le projet {} est déjà un projet assisté", project.display());
    }
    let extension = project
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("ecproj");
    Ok(project.with_file_name(format!("{stem}{suffix}.{extension}")))
}

pub fn save_bytes(doc: &Value) -> Result<Vec<u8>> {
    serde_json::to_vec(doc).context("sérialisation du projet")
}

pub fn transform(layer: &Value) -> Option<&Value> {
    layer
        .get("props")?
        .get("children")?
        .as_array()?
        .iter()
        .find(|child| child.get("name").and_then(Value::as_str) == Some("Transform"))
}

pub fn transform_mut(layer: &mut Value) -> Option<&mut Value> {
    layer
        .get_mut("props")?
        .get_mut("children")?
        .as_array_mut()?
        .iter_mut()
        .find(|child| child.get("name").and_then(Value::as_str) == Some("Transform"))
}

pub fn transform_prop<'a>(layer: &'a Value, name: &str) -> Option<&'a Value> {
    transform(layer)?
        .get("children")?
        .as_array()?
        .iter()
        .find(|child| child.get("name").and_then(Value::as_str) == Some(name))
}

pub fn transform_prop_mut<'a>(layer: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    transform_mut(layer)?
        .get_mut("children")?
        .as_array_mut()?
        .iter_mut()
        .find(|child| child.get("name").and_then(Value::as_str) == Some(name))
}

pub fn set_transform(
    layer: &mut Value,
    anchor: [f64; 3],
    position: [f64; 3],
    scale: f64,
) -> Result<()> {
    set_prop_vec(layer, "Anchor Point", anchor)?;
    set_prop_vec(layer, "Position", position)?;
    set_prop_vec(layer, "Scale", [scale, scale, 0.0])?;
    Ok(())
}

pub fn set_position_y(layer: &mut Value, y: f64) -> Result<()> {
    let prop = transform_prop_mut(layer, "Position").context("calque sans Position")?;
    let value = prop.get_mut("value").context("Position sans valeur")?;
    let vector = value
        .get_mut("v")
        .and_then(Value::as_array_mut)
        .context("Position sans vecteur")?;
    if vector.len() < 2 {
        bail!("Position invalide");
    }
    vector[1] = json!(y);
    Ok(())
}

pub fn set_scale(layer: &mut Value, scale: f64) -> Result<()> {
    set_prop_vec(layer, "Scale", [scale, scale, 0.0])
}

fn set_prop_vec(layer: &mut Value, name: &str, vector: [f64; 3]) -> Result<()> {
    let prop =
        transform_prop_mut(layer, name).with_context(|| format!("calque sans propriété {name}"))?;
    let value = prop
        .get_mut("value")
        .with_context(|| format!("{name} sans valeur"))?;
    value["v"] = json!([vector[0], vector[1], vector[2]]);
    Ok(())
}

pub fn keys(prop: &Value) -> Vec<Keyframe> {
    prop.get("keys")
        .and_then(Value::as_array)
        .map(|keys| {
            keys.iter()
                .filter_map(|key| {
                    let t = key.get("time").and_then(Value::as_f64)? / TICKS_PER_SECOND;
                    let vector = key.get("value")?.get("v")?.as_array()?;
                    if vector.len() < 3 {
                        return None;
                    }
                    Some(Keyframe {
                        t,
                        v: [
                            vector[0].as_f64()?,
                            vector[1].as_f64()?,
                            vector[2].as_f64()?,
                        ],
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn vec3(prop: &Value) -> Option<[f64; 3]> {
    let vector = prop.get("value")?.get("v")?.as_array()?;
    if vector.len() < 3 {
        return None;
    }
    Some([
        vector[0].as_f64()?,
        vector[1].as_f64()?,
        vector[2].as_f64()?,
    ])
}

pub fn layer_name(layer: &Value) -> Option<&str> {
    layer.get("name").and_then(Value::as_str)
}

pub fn layer_by_id(comp: &Value, id: i64) -> Option<&Value> {
    comp.get("layers")
        .and_then(Value::as_array)?
        .iter()
        .find(|layer| layer.get("id").and_then(as_i64) == Some(id))
}

pub fn max_layer_id(comp: &Value) -> i64 {
    comp.get("layers")
        .and_then(Value::as_array)
        .map(|layers| {
            layers
                .iter()
                .filter_map(|layer| layer.get("id").and_then(as_i64))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

pub fn max_uid(value: &Value) -> i64 {
    fn walk(value: &Value, max: &mut i64) {
        match value {
            Value::Object(map) => {
                if let Some(uid) = map.get("uid").and_then(as_i64) {
                    *max = (*max).max(uid);
                }
                for child in map.values() {
                    walk(child, max);
                }
            }
            Value::Array(array) => {
                for child in array {
                    walk(child, max);
                }
            }
            _ => {}
        }
    }
    let mut max = 0;
    walk(value, &mut max);
    max
}

pub fn offset_uids(value: &mut Value, offset: i64) {
    match value {
        Value::Object(map) => {
            if let Some(uid) = map.get("uid").and_then(as_i64) {
                map.insert("uid".into(), json!(uid + offset));
            }
            for child in map.values_mut() {
                offset_uids(child, offset);
            }
        }
        Value::Array(array) => {
            for child in array {
                offset_uids(child, offset);
            }
        }
        _ => {}
    }
}

pub fn find_item_by_name<'a>(
    doc: &'a Value,
    name: &str,
    kind_type: &str,
) -> Option<(i64, &'a Value)> {
    let items = doc.get("items")?.as_object()?;
    for (id, item) in items {
        if item.get("name").and_then(Value::as_str) != Some(name) {
            continue;
        }
        if item
            .get("kind")
            .and_then(|kind| kind.get("type"))
            .and_then(Value::as_str)
            != Some(kind_type)
        {
            continue;
        }
        if let Ok(id) = id.parse::<i64>() {
            return Some((id, item));
        }
    }
    None
}

pub fn duration_ticks(meta: &CompMeta) -> i64 {
    round_to(meta.duration * TICKS_PER_SECOND, 0) as i64
}

pub fn as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
}

pub fn map_get_f64(map: &Map<String, Value>, key: &str) -> Option<f64> {
    map.get(key).and_then(Value::as_f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Value {
        json!({
            "items": {
                "1": {"name": "clip.mp4", "kind": {"type": "Footage", "width": 1920, "height": 1080}},
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
                                "name": "Null Segments",
                                "source": {"type": "Null"},
                                "props": {"children": [{"name": "Transform", "children": [
                                    {"name": "Anchor Point", "value": {"v": [50.0, 50.0, 0.0]}},
                                    {"name": "Position", "value": {"v": [540.0, 0.0, 0.0]}},
                                    {"name": "Scale", "value": {"v": [100.0, 100.0, 100.0]}}
                                ]}]}
                            },
                            {
                                "id": 11,
                                "name": "Segment 1",
                                "parent": 10,
                                "in_point": 0,
                                "out_point": 2540160000000_i64,
                                "source": {"type": "Footage", "item": 1},
                                "props": {"children": [{"name": "Transform", "children": [
                                    {"name": "Anchor Point", "value": {"v": [960.0, 0.0, 0.0]}},
                                    {"name": "Position", "value": {"v": [540.0, 0.0, 0.0]}},
                                    {"name": "Scale", "value": {"v": [180.0, 180.0, 0.0]}}
                                ]}]}
                            }
                        ]
                    }
                }
            }
        })
    }

    #[test]
    fn parses_comp_segments_and_parent_offset() {
        let doc = project();
        let meta = comp_meta(&doc).unwrap();
        assert_eq!((meta.width, meta.height), (1080, 1920));
        assert_eq!(meta.n_segments, 1);
        let segments = segments(&doc).unwrap();
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].source.as_deref(), Some("clip.mp4"));
        assert!(segments[0].null_parent);
        assert_eq!(segments[0].comp_x, 1030.0);
        assert_eq!(segments[0].parent_dy, -50.0);
    }

    #[test]
    fn assist_name_appends_suffix_before_extension() {
        let path = Path::new("/tmp/Voisins_9x16.ecproj");
        let assist = assist_path(path, "_assist").unwrap();
        assert_eq!(assist, Path::new("/tmp/Voisins_9x16_assist.ecproj"));
        assert!(assist_path(Path::new("/tmp/Voisins_9x16_assist.ecproj"), "_assist").is_err());
    }

    #[test]
    fn project_stem_prefers_segment_source() {
        let doc = project();
        let segments = segments(&doc).unwrap();
        assert_eq!(
            project_stem(Path::new("/tmp/autre_9x16.ecproj"), &segments),
            "clip"
        );
        assert_eq!(
            project_stem(Path::new("/tmp/autre_9x16.ecproj"), &[]),
            "autre"
        );
    }

    #[test]
    fn set_transform_preserves_other_properties() {
        let mut doc = project();
        let comp = comp_mut(&mut doc).unwrap();
        let layer = comp["layers"].as_array_mut().unwrap().get_mut(1).unwrap();
        set_transform(layer, [960.0, 0.0, 0.0], [540.0, 171.0, 0.0], 84.0).unwrap();
        assert_eq!(
            transform_prop(layer, "Scale").and_then(vec3).unwrap()[0],
            84.0
        );
        assert_eq!(
            transform_prop(layer, "Position").and_then(vec3).unwrap()[1],
            171.0
        );
    }
}
