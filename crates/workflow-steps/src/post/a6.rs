//! A6 — template SDM trois panneaux, déclenché par une règle de nommage.
//!
//! Le module ne devine pas le template à partir du contenu. Le préfixe est
//! vérifié, puis les conditions structurelles (mono-segment, sans keyframe,
//! comp 1080x1920, source 1920x1080, parent Null statique). Toute condition
//! non remplie part en `review` et le projet assisté reste la copie du brut.

use crate::post::ecproj::{
    self, duration_ticks, find_item_by_name, offset_uids, set_transform, CompMeta, Segment,
};
use crate::post::model::{AuditCheck, Decision};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const DEFAULT_PREFIX: &str = "M6 - SDM - ";

static LOGO_LAYER: OnceLock<Value> = OnceLock::new();

#[derive(Debug, Clone, Serialize)]
pub struct TemplateInfo {
    pub name: &'static str,
    pub prefix: String,
    pub stem: String,
    pub matched: bool,
    pub eligible: Option<bool>,
    pub executor: &'static str,
    pub profile: Value,
    pub details: Value,
}

pub fn profile() -> Value {
    json!({
        "name": "sdm_v1",
        "comp_width": 1080.0,
        "comp_height": 1920.0,
        "source_width": 1920.0,
        "source_height": 1080.0,
        "background": {
            "scale": 180.0,
            "anchor": [960.0, 0.0],
            "position": [540.0950433039733, 0.0],
            "blur": 50.0
        },
        "panels": [
            {
                "role": "main",
                "scale": 84.3497774862575,
                "anchor": [960.0, 0.0],
                "position": [540.0950433039733, 171.03210271024682]
            },
            {
                "role": "small",
                "scale": 56.294573830734535,
                "anchor": [960.0, 0.0],
                "position": [540.0950433039733, 1082.0260859403052]
            }
        ],
        "logo": {
            "name": "sdm.png",
            "anchor": [540.0, 960.0],
            "position": [540.0, 960.0],
            "scale": 100.0
        }
    })
}

pub fn validate(
    doc: &Value,
    segments: &[Segment],
    meta: &CompMeta,
    stem: &str,
    prefix: &str,
) -> (Vec<Decision>, TemplateInfo) {
    let matched = !prefix.is_empty() && stem.starts_with(prefix);
    if !matched {
        let details = json!({
            "checks": [{"id": "prefix_match", "status": "pass", "detail": "projet hors template SDM"}],
            "valid": true,
            "prefix": prefix,
            "stem": stem
        });
        return (
            Vec::new(),
            TemplateInfo {
                name: "sdm_v1",
                prefix: prefix.to_string(),
                stem: stem.to_string(),
                matched: false,
                eligible: None,
                executor: "post",
                profile: profile(),
                details,
            },
        );
    }

    let mut checks = Vec::new();
    let mut valid = true;
    let mut add = |id: &str, status: &'static str, detail: String| {
        if status == "fail" {
            valid = false;
        }
        checks.push(AuditCheck {
            id: id.into(),
            status,
            detail,
        });
    };
    add(
        "project_readable",
        "pass",
        format!(
            "{} segment(s), comp {}x{}",
            segments.len(),
            meta.width,
            meta.height
        ),
    );
    if segments.len() != 1 {
        add(
            "single_segment",
            "fail",
            format!("{} segments, A6 exige 1 segment", segments.len()),
        );
    } else {
        add("single_segment", "pass", "1 segment".into());
    }
    let key_count: usize = segments
        .iter()
        .map(|segment| {
            segment.position_keys.len() + segment.scale_keys.len() + segment.anchor_keys.len()
        })
        .sum();
    if key_count > 0 {
        add(
            "no_keyframes",
            "fail",
            format!("{key_count} clé(s) de transformation, A6 ne duplique pas les keyframes"),
        );
    } else {
        add("no_keyframes", "pass", "aucune keyframe".into());
    }
    if (meta.width, meta.height) != (1080, 1920) {
        add(
            "comp_1080x1920",
            "fail",
            format!("comp {}x{}", meta.width, meta.height),
        );
    } else {
        add("comp_1080x1920", "pass", "1080x1920".into());
    }

    let unexpected_layers = ecproj::comp(doc)
        .ok()
        .and_then(|comp| comp.get("layers"))
        .and_then(Value::as_array)
        .map(|layers| {
            layers
                .iter()
                .filter(|layer| {
                    let name = layer
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let null = layer
                        .get("source")
                        .and_then(|source| source.get("type"))
                        .and_then(Value::as_str)
                        == Some("Null");
                    !name.starts_with("Segment") && !null
                })
                .count()
        })
        .unwrap_or(0);
    if unexpected_layers > 0 {
        add(
            "layers_supported",
            "fail",
            format!("{unexpected_layers} calque(s) hors Segment/Null, A6 ne les duplique pas"),
        );
    } else {
        add(
            "layers_supported",
            "pass",
            "Segment + Null uniquement".into(),
        );
    }
    if let Some(segment) = segments.first() {
        if segment.start > 0.001 || segment.end < meta.duration - 0.001 {
            add(
                "single_segment_full_comp",
                "fail",
                format!(
                    "segment {:.3}-{:.3}s, comp {:.3}s",
                    segment.start, segment.end, meta.duration
                ),
            );
        } else {
            add(
                "single_segment_full_comp",
                "pass",
                "le segment couvre la composition".into(),
            );
        }
        let parent_ok = segment.parent.is_none() || segment.null_parent;
        if !parent_ok {
            add(
                "parent_static",
                "fail",
                format!("{}: parent non Null", segment.name),
            );
        } else if segment.parent_keyframed {
            add(
                "parent_static",
                "fail",
                format!("{}: parent Null animé", segment.name),
            );
        } else {
            add("parent_static", "pass", "parent Null ou absent".into());
        }
        let source_ok = segment
            .source
            .as_deref()
            .and_then(|source| find_item_by_name(doc, source, "Footage"))
            .is_some_and(|(_, item)| {
                let kind = item.get("kind").unwrap_or(&Value::Null);
                kind.get("width").and_then(Value::as_u64) == Some(1920)
                    && kind.get("height").and_then(Value::as_u64) == Some(1080)
            });
        if source_ok {
            add("source_1920x1080", "pass", "source 1920x1080".into());
        } else {
            add(
                "source_1920x1080",
                "fail",
                "source absente ou différente de 1920x1080".into(),
            );
        }
    } else {
        add("parent_static", "fail", "aucun segment".into());
        add("source_1920x1080", "fail", "aucun segment".into());
    }
    let logo_available = find_item_by_name(doc, "sdm.png", "Footage").is_some();
    add(
        "logo_available",
        "pass",
        if logo_available {
            "sdm.png présent".into()
        } else {
            "sdm.png absent, template à trois panneaux".into()
        },
    );
    let details = json!({
        "checks": checks,
        "valid": valid,
        "prefix": prefix,
        "stem": stem,
        "logo": if logo_available { "available" } else { "missing" }
    });
    let action = if valid {
        Decision::apply(
            "A6",
            "comp",
            "template SDM éligible (mono-segment, sans keyframe)",
            0.95,
            json!({
                "prefix": prefix,
                "stem": stem,
                "matched": true,
                "valid": valid,
                "executor": "post",
                "logo": details["logo"].clone(),
                "checks": details["checks"].clone()
            }),
        )
    } else {
        Decision::review(
            "A6",
            "comp",
            "préfixe SDM mais structure incompatible (voir audit)",
            0.4,
            json!({
                "prefix": prefix,
                "stem": stem,
                "matched": true,
                "valid": valid,
                "executor": "post",
                "logo": details["logo"].clone(),
                "checks": details["checks"].clone()
            }),
        )
    };
    (
        vec![action],
        TemplateInfo {
            name: "sdm_v1",
            prefix: prefix.to_string(),
            stem: stem.to_string(),
            matched: true,
            eligible: Some(valid),
            executor: "post",
            profile: profile(),
            details,
        },
    )
}

pub struct ApplyInfo {
    pub logo: bool,
    pub layers: usize,
}

pub fn apply_template(doc: &Value, meta: &CompMeta) -> Result<(Value, ApplyInfo)> {
    let mut out = doc.clone();
    let comp_id = ecproj::comp_id(&out)?;
    let comp_ref = out
        .get("items")
        .and_then(|items| items.get(&comp_id))
        .and_then(|item| item.get("kind"))
        .context("composition introuvable")?;
    let layers_snapshot = comp_ref
        .get("layers")
        .and_then(Value::as_array)
        .context("composition sans calques")?
        .clone();
    let segment_indices: Vec<usize> = layers_snapshot
        .iter()
        .enumerate()
        .filter(|(_, layer)| {
            layer
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.starts_with("Segment"))
        })
        .map(|(index, _)| index)
        .collect();
    if segment_indices.len() != 1 {
        bail!(
            "A6 attend un seul segment, trouvé {}",
            segment_indices.len()
        );
    }
    let base_index = segment_indices[0];
    let mut base = layers_snapshot[base_index].clone();
    let base_name = base
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Segment")
        .to_string();
    let base_id = base.get("id").and_then(ecproj::as_i64).unwrap_or(0);
    base["parent"] = Value::Null;
    set_transform(
        &mut base,
        [960.0, 0.0, 0.0],
        [540.0950433039733, 0.0, 0.0],
        180.0,
    )?;
    set_switch(&mut base, "audio", true)?;

    let existing_uids = collect_uids(&out);
    let mut existing_ids: HashSet<i64> = layers_snapshot
        .iter()
        .filter_map(|layer| layer.get("id").and_then(ecproj::as_i64))
        .collect();
    let blur_base = if existing_uids.iter().any(|uid| *uid >= 9000) {
        existing_uids.iter().copied().max().unwrap_or(0) + 1
    } else {
        9000
    };
    set_effects(&mut base, json!([blur_effect(blur_base)]))?;

    let mut existing_uids = collect_uids(&out);
    existing_uids.extend([blur_base, blur_base + 1, blur_base + 2, blur_base + 3]);
    let mut panel_layers = Vec::new();
    let panels = [
        ("main", 84.3497774862575, 171.03210271024682),
        ("small", 56.294573830734535, 1082.0260859403052),
    ];
    for (index, (role, scale, y)) in panels.iter().enumerate() {
        let desired = ((index + 1) as i64) * 1000;
        let offset = choose_offset(&base, &existing_uids, &existing_ids, base_id, desired);
        let mut clone = base.clone();
        offset_uids(&mut clone, offset);
        clone["id"] = json!(base_id + offset);
        clone["name"] = json!(format!("{base_name} [{role}]"));
        set_switch(&mut clone, "audio", false)?;
        set_effects(&mut clone, json!([]))?;
        set_transform(
            &mut clone,
            [960.0, 0.0, 0.0],
            [540.0950433039733, *y, 0.0],
            *scale,
        )?;
        existing_ids.insert(base_id + offset);
        existing_uids.extend(collect_uids(&clone));
        panel_layers.push(clone);
    }

    let mut output_layers = Vec::new();
    let logo_item = find_item_by_name(&out, "sdm.png", "Footage").map(|(id, _)| id);
    if let Some(item_id) = logo_item {
        let mut logo = LOGO_LAYER
            .get_or_init(|| {
                serde_json::from_str(include_str!(
                    "../../../../assets/effectcraft/sdm_logo_layer.json"
                ))
                .expect("asset sdm_logo_layer.json invalide")
            })
            .clone();
        logo["source"]["item"] = json!(item_id);
        logo["in_point"] = json!(0);
        logo["out_point"] = json!(duration_ticks(meta));
        logo["start_time"] = json!(0);
        logo["parent"] = Value::Null;
        let logo_id = logo.get("id").and_then(ecproj::as_i64).unwrap_or(0);
        let offset = choose_offset(&logo, &existing_uids, &existing_ids, logo_id, 3000);
        offset_uids(&mut logo, offset);
        logo["id"] = json!(logo_id + offset);
        set_transform(&mut logo, [540.0, 960.0, 0.0], [540.0, 960.0, 0.0], 100.0)?;
        output_layers.push(logo);
    }
    output_layers.extend(panel_layers.into_iter().rev());
    output_layers.push(base);

    let comp = out
        .get_mut("items")
        .and_then(|items| items.get_mut(&comp_id))
        .and_then(|item| item.get_mut("kind"))
        .context("composition introuvable")?;
    comp["layers"] = Value::Array(output_layers);
    let max_id = ecproj::max_layer_id(comp);
    let layers_count = comp
        .get("layers")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let max_uid = ecproj::max_uid(&out);
    out["next_id"] = json!(max_id.max(max_uid) + 1);
    let logo = logo_item.is_some();
    Ok((
        out,
        ApplyInfo {
            logo,
            layers: layers_count,
        },
    ))
}

fn choose_offset(
    value: &Value,
    existing_uids: &HashSet<i64>,
    existing_ids: &HashSet<i64>,
    base_id: i64,
    desired: i64,
) -> i64 {
    let mut offset = desired.max(1000);
    loop {
        let mut clone = value.clone();
        offset_uids(&mut clone, offset);
        let uids = collect_uids(&clone);
        if !existing_ids.contains(&(base_id + offset))
            && !uids.iter().any(|uid| existing_uids.contains(uid))
        {
            return offset;
        }
        offset += 1000;
    }
}

fn collect_uids(value: &Value) -> HashSet<i64> {
    fn walk(value: &Value, uids: &mut HashSet<i64>) {
        match value {
            Value::Object(map) => {
                if let Some(uid) = map.get("uid").and_then(ecproj::as_i64) {
                    uids.insert(uid);
                }
                for child in map.values() {
                    walk(child, uids);
                }
            }
            Value::Array(array) => {
                for child in array {
                    walk(child, uids);
                }
            }
            _ => {}
        }
    }
    let mut uids = HashSet::new();
    walk(value, &mut uids);
    uids
}

fn set_switch(layer: &mut Value, name: &str, value: bool) -> Result<()> {
    let switches = layer
        .get_mut("switches")
        .and_then(Value::as_object_mut)
        .context("calque sans switches")?;
    switches.insert(name.into(), json!(value));
    Ok(())
}

fn set_effects(layer: &mut Value, children: Value) -> Result<()> {
    let groups = layer
        .get_mut("props")
        .and_then(|props| props.get_mut("children"))
        .and_then(Value::as_array_mut)
        .context("calque sans props.children")?;
    let effects = groups
        .iter_mut()
        .find(|child| child.get("name").and_then(Value::as_str) == Some("Effects"))
        .context("calque sans groupe Effects")?;
    effects["children"] = children;
    Ok(())
}

fn blur_effect(base_uid: i64) -> Value {
    json!({
        "node": "Group",
        "uid": base_uid,
        "match": "ec.blur.gaussian",
        "name": "Gaussian Blur",
        "kind": {"kind": "Effect", "effect": "ec.blur.gaussian"},
        "enabled": true,
        "children": [
            {
                "node": "Prop",
                "uid": base_uid + 1,
                "match": "blurriness",
                "name": "Blurriness",
                "value": {"t": "Scalar", "v": 50.0},
                "ui": {
                    "kind": "Slider",
                    "min": 0.0,
                    "max": 3000.0,
                    "slider_min": 0.0,
                    "slider_max": 50.0,
                    "decimals": 1
                },
                "spatial": false,
                "hold_only": false,
                "three_d_only": false,
                "two_d_only": false,
                "shown_dims": 0,
                "static_only": false
            },
            {
                "node": "Prop",
                "uid": base_uid + 2,
                "match": "dimensions",
                "name": "Blur Dimensions",
                "value": {"t": "Enum", "v": 0},
                "ui": {
                    "kind": "Popup",
                    "options": ["Horizontal and Vertical", "Horizontal", "Vertical"]
                },
                "spatial": false,
                "hold_only": true,
                "three_d_only": false,
                "two_d_only": false,
                "shown_dims": 0,
                "static_only": false
            },
            {
                "node": "Prop",
                "uid": base_uid + 3,
                "match": "repeatEdge",
                "name": "Repeat Edge Pixels",
                "value": {"t": "Bool", "v": true},
                "ui": {"kind": "Checkbox"},
                "spatial": false,
                "hold_only": true,
                "three_d_only": false,
                "two_d_only": false,
                "shown_dims": 0,
                "static_only": false
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::post::ecproj::comp_mut;

    fn doc() -> Value {
        json!({
            "next_id": 67,
            "items": {
                "1": {"name": "clip.mp4", "kind": {"type": "Footage", "width": 1920, "height": 1080}},
                "28": {"name": "sdm.png", "kind": {"type": "Footage", "width": 1080, "height": 1920}},
                "2": {
                    "name": "Comp",
                    "kind": {
                        "type": "Comp",
                        "width": 1080,
                        "height": 1920,
                        "frame_rate": {"num": 25, "den": 1},
                        "duration": 13381562880000_i64,
                        "layers": [
                            {
                                "id": 55,
                                "name": "Null Segments",
                                "source": {"type": "Null"},
                                "props": {"children": [{"name": "Transform", "children": [
                                    {"name": "Anchor Point", "value": {"v": [50.0, 50.0, 0.0]}},
                                    {"name": "Position", "value": {"v": [540.0, 0.0, 0.0]}},
                                    {"name": "Scale", "value": {"v": [100.0, 100.0, 100.0]}}
                                ]}]}
                            },
                            {
                                "id": 29,
                                "name": "Segment 1",
                                "parent": 55,
                                "in_point": 0,
                                "out_point": 13381562880000_i64,
                                "source": {"type": "Footage", "item": 1},
                                "switches": {"audio": true},
                                "props": {"children": [
                                    {"name": "Effects", "children": []},
                                    {"name": "Transform", "children": [
                                        {"name": "Anchor Point", "value": {"v": [1390.39, 0.0, 0.0]}},
                                        {"name": "Position", "value": {"v": [50.0, 50.0, 0.0]}},
                                        {"name": "Scale", "value": {"v": [180.0, 180.0, 0.0]}}
                                    ]}
                                ]}
                            }
                        ]
                    }
                }
            }
        })
    }

    fn meta() -> CompMeta {
        CompMeta {
            id: "2".into(),
            width: 1080,
            height: 1920,
            fps: 25.0,
            duration: 52.68,
            n_layers: 2,
            n_segments: 1,
        }
    }

    #[test]
    fn transforms_single_segment_into_four_layers() {
        let (out, info) = apply_template(&doc(), &meta()).unwrap();
        assert!(info.logo);
        assert_eq!(info.layers, 4);
        let comp = ecproj::comp(&out).unwrap();
        let layers = comp["layers"].as_array().unwrap();
        assert_eq!(layers.len(), 4);
        assert_eq!(layers[0]["name"], "sdm.png");
        assert!(layers[1]["name"].as_str().unwrap().ends_with("[small]"));
        assert!(layers[2]["name"].as_str().unwrap().ends_with("[main]"));
        assert_eq!(layers[3]["name"], "Segment 1");
        assert_eq!(
            ecproj::transform_prop(&layers[2], "Scale")
                .and_then(ecproj::vec3)
                .unwrap()[0],
            84.3497774862575
        );
    }

    #[test]
    fn validates_prefix_and_structure() {
        let doc = doc();
        let segments = ecproj::segments(&doc).unwrap();
        let (decisions, info) =
            validate(&doc, &segments, &meta(), "M6 - SDM - test", DEFAULT_PREFIX);
        assert!(info.matched);
        assert_eq!(info.eligible, Some(true));
        assert_eq!(decisions[0].action, crate::post::model::Action::Apply);

        let (decisions, info) = validate(&doc, &segments, &meta(), "autre", DEFAULT_PREFIX);
        assert!(!info.matched);
        assert!(decisions.is_empty());
    }

    #[test]
    fn empty_prefix_is_not_matched() {
        let doc = doc();
        let segments = ecproj::segments(&doc).unwrap();
        let (decisions, info) = validate(&doc, &segments, &meta(), "M6 - SDM - test", "");
        assert!(!info.matched);
        assert!(decisions.is_empty());
    }

    #[test]
    fn unexpected_layer_is_reviewed() {
        let mut doc = doc();
        let comp = comp_mut(&mut doc).unwrap();
        comp["layers"].as_array_mut().unwrap().push(json!({
            "id": 77,
            "name": "Logo",
            "source": {"type": "Footage", "item": 28},
            "props": {"children": []}
        }));
        let segments = ecproj::segments(&doc).unwrap();
        let (decisions, info) =
            validate(&doc, &segments, &meta(), "M6 - SDM - test", DEFAULT_PREFIX);
        assert_eq!(info.eligible, Some(false));
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
        assert!(decisions[0].details["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| check["id"] == "layers_supported" && check["status"] == "fail"));
    }

    #[test]
    fn anchor_keyframe_is_reviewed() {
        let mut doc = doc();
        let comp = comp_mut(&mut doc).unwrap();
        let layer = &mut comp["layers"].as_array_mut().unwrap()[1];
        ecproj::transform_prop_mut(layer, "Anchor Point").unwrap()["keys"] = json!([
            {"time": 0, "value": {"v": [1390.39, 0.0, 0.0]}}
        ]);
        let segments = ecproj::segments(&doc).unwrap();
        let (decisions, info) =
            validate(&doc, &segments, &meta(), "M6 - SDM - test", DEFAULT_PREFIX);
        assert_eq!(info.eligible, Some(false));
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
    }

    #[test]
    fn keyframed_project_is_reviewed() {
        let mut doc = doc();
        let comp = comp_mut(&mut doc).unwrap();
        let layer = &mut comp["layers"].as_array_mut().unwrap()[1];
        ecproj::transform_prop_mut(layer, "Position").unwrap()["keys"] = json!([
            {"time": 0, "value": {"v": [50.0, 50.0, 0.0]}}
        ]);
        let segments = ecproj::segments(&doc).unwrap();
        let (decisions, info) =
            validate(&doc, &segments, &meta(), "M6 - SDM - test", DEFAULT_PREFIX);
        assert_eq!(info.eligible, Some(false));
        assert_eq!(decisions[0].action, crate::post::model::Action::Review);
    }
}
