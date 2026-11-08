//! Contrôle géométrique des logos source dans l'export final (D-052).
//!
//! Phase A : le registre spatial ne fait qu'annoter la visibilité du logo de
//! programme à travers les transformations du `.ecproj`. Aucun pixel n'est
//! analysé et aucune décision n'est prise : le check est `pass` ou `info`.
//! La confirmation visuelle et le passage en `review` viendront dans un lot
//! ultérieur, après comparaison avec le corpus GOOD.

use crate::post::ecproj;
use crate::qa::model::QaCheck;
use crate::qa::overlay::{self, OverlayMask};
use crate::vision::source_logo::{bbox_area, clip_bbox, SourceLogoCatalog};
use anyhow::Result;
use serde_json::Value;
use std::path::Path;

/// Part minimale de la bbox logo visible pour retenir une observation.
const MIN_VISIBILITY: f64 = 0.15;
/// Surface minimale visible dans la composition (pixels).
const MIN_VISIBLE_AREA: f64 = 400.0;

pub fn run_geometry(project: &Path, catalog_path: &Path) -> Result<QaCheck> {
    let catalog = SourceLogoCatalog::load(catalog_path)?;
    let doc = ecproj::load(project)?;
    // Le masque d'overlay déclaré prime sur le prior : un logo de programme
    // remplacé par un habillage intentionnel ne doit pas être signalé.
    let mask = overlay::read_mask(project).ok();
    Ok(check_doc(&doc, &catalog, mask.as_ref()))
}

fn check_doc(doc: &Value, catalog: &SourceLogoCatalog, mask: Option<&OverlayMask>) -> QaCheck {
    let Ok(meta) = ecproj::comp_meta(doc) else {
        return QaCheck::not_applicable("source_logo", "composition illisible");
    };
    let Ok(segments) = ecproj::segments(doc) else {
        return QaCheck::not_applicable("source_logo", "segments illisibles");
    };
    if segments.is_empty() {
        return QaCheck::not_applicable("source_logo", "aucun segment dans le projet");
    }
    let Some(source_name) = segments
        .iter()
        .find_map(|segment| segment.source.as_deref())
        .map(str::to_string)
    else {
        return QaCheck::not_applicable("source_logo", "source du projet introuvable");
    };
    let Some((source_width, source_height)) = source_size(doc, &source_name) else {
        return QaCheck::not_applicable(
            "source_logo",
            format!("dimensions de la source {source_name} introuvables"),
        );
    };
    let Some(prior) = catalog.prior_for_source(&source_name, source_width, source_height) else {
        let detail = if catalog.is_clean_feed(&source_name) {
            format!("clean feed sans logo: {source_name}")
        } else {
            let zones = catalog
                .generic_zones(source_width, source_height)
                .into_iter()
                .map(|(position, bbox)| {
                    format!(
                        "{position} [{:.0}, {:.0}, {:.0}, {:.0}]",
                        bbox[0], bbox[1], bbox[2], bbox[3]
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            if zones.is_empty() {
                format!("aucun logo source connu pour {source_name}")
            } else {
                format!(
                    "aucun logo source connu pour {source_name}; zone(s) générique(s) {zones} non confirmée(s)"
                )
            }
        };
        return QaCheck::pass("source_logo", detail);
    };
    let mut visible_segments = 0usize;
    let mut skipped_segments = 0usize;
    let mut unsupported_segments = 0usize;
    let mut matching_segments = 0usize;
    let mut masked_observations = 0usize;
    let mut best_ratio = 0.0f64;
    let mut best_bbox = None;
    let mut first_start = f64::INFINITY;
    let mut last_end = f64::NEG_INFINITY;
    for segment in &segments {
        if segment.source.as_deref() != Some(source_name.as_str()) {
            skipped_segments += 1;
            continue;
        }
        matching_segments += 1;
        if !segment.projectable() {
            unsupported_segments += 1;
            continue;
        }
        let middle = segment.start + segment.duration() / 2.0;
        let mut segment_ratio = 0.0f64;
        let mut segment_bbox = None;
        let mut projected_any = false;
        for time in [
            segment.start,
            segment.start + segment.duration() * 0.25,
            middle,
            segment.start + segment.duration() * 0.75,
            segment.end,
        ] {
            let Some(projected) = segment.project_bbox(prior.bbox, time) else {
                continue;
            };
            projected_any = true;
            let Some(clipped) = clip_bbox(projected, meta.width, meta.height) else {
                continue;
            };
            if let Some(mask) = mask {
                let bbox_xywh = [
                    clipped[0],
                    clipped[1],
                    clipped[2] - clipped[0],
                    clipped[3] - clipped[1],
                ];
                if mask.overlap_ratio(bbox_xywh, time, meta.width, meta.height) >= 0.5 {
                    masked_observations += 1;
                    continue;
                }
            }
            let projected_area = bbox_area(projected).max(1.0);
            let ratio = bbox_area(clipped) / projected_area;
            if ratio > segment_ratio {
                segment_ratio = ratio;
                segment_bbox = Some(clipped);
            }
        }
        if !projected_any {
            unsupported_segments += 1;
            continue;
        }
        if segment_ratio >= MIN_VISIBILITY
            && segment_bbox.is_some_and(|bbox| bbox_area(bbox) >= MIN_VISIBLE_AREA)
        {
            visible_segments += 1;
            first_start = first_start.min(segment.start);
            last_end = last_end.max(segment.end);
            if segment_ratio > best_ratio {
                best_ratio = segment_ratio;
                best_bbox = segment_bbox;
            }
        }
    }
    if visible_segments == 0 {
        if masked_observations > 0 {
            return QaCheck::pass(
                "source_logo",
                format!(
                    "logo {} couvert par un overlay déclaré ({masked_observations} observation(s))",
                    prior.name
                ),
            );
        }
        if matching_segments > 0 && unsupported_segments == matching_segments {
            return QaCheck::not_applicable(
                "source_logo",
                format!(
                    "transformations non prises en charge pour {} segment(s) (rotation ou parent animé)",
                    unsupported_segments
                ),
            );
        }
        let skipped = if skipped_segments == 0 {
            String::new()
        } else {
            format!(", {skipped_segments} segment(s) d'une autre source ignoré(s)")
        };
        let unsupported = if unsupported_segments == 0 {
            String::new()
        } else {
            format!(", {unsupported_segments} segment(s) non projetable(s)")
        };
        return QaCheck::pass(
            "source_logo",
            format!(
                "logo {} hors cadre du recadrage ({} segment(s) examinés{skipped}{unsupported})",
                prior.name,
                matching_segments - unsupported_segments
            ),
        );
    }
    let bbox = best_bbox
        .map(|bbox| {
            format!(
                "[{:.0}, {:.0}, {:.0}, {:.0}]",
                bbox[0], bbox[1], bbox[2], bbox[3]
            )
        })
        .unwrap_or_else(|| "?".into());
    let masked = if masked_observations == 0 {
        String::new()
    } else {
        format!(", {masked_observations} observation(s) couverte(s) par un overlay déclaré")
    };
    let unsupported = if unsupported_segments == 0 {
        String::new()
    } else {
        format!(", {unsupported_segments} segment(s) non projetable(s)")
    };
    QaCheck::info(
        "source_logo",
        format!(
            "logo {} ({}) projeté dans {} segment(s) du cadre, de {:.1} à {:.1} s, \
             visibilité max {:.0} %, bbox {}{masked}{unsupported} ; confirmation visuelle non activée",
            prior.name,
            prior.channel,
            visible_segments,
            first_start,
            last_end,
            best_ratio * 100.0,
            bbox
        ),
    )
}

fn source_size(doc: &Value, name: &str) -> Option<(u32, u32)> {
    let items = doc.get("items")?.as_object()?;
    items.values().find_map(|item| {
        if item.get("name").and_then(Value::as_str) != Some(name) {
            return None;
        }
        let kind = item.get("kind")?;
        if kind.get("type").and_then(Value::as_str) != Some("Footage") {
            return None;
        }
        Some((
            kind.get("width").and_then(Value::as_u64)? as u32,
            kind.get("height").and_then(Value::as_u64)? as u32,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CATALOG: &str = r#"
schema = 1
source_resolution = [1920, 1080]

[[emissions]]
name = "66 Minutes"
channel = "M6"
position = "bottom_left"
bbox = [91, 846, 298, 1026]
aliases = ["66M -"]
"#;

    fn catalog() -> SourceLogoCatalog {
        toml::from_str(CATALOG).unwrap()
    }

    fn doc(anchor_x: f64, position_x: f64, parent: bool) -> Value {
        let mut layers = Vec::new();
        if parent {
            layers.push(json!({
                "id": 10,
                "name": "Null Segments",
                "source": {"type": "Null"},
                "props": {"children": [{"name": "Transform", "children": [
                    {"name": "Anchor Point", "value": {"v": [50.0, 50.0, 0.0]}},
                    {"name": "Position", "value": {"v": [540.0, 0.0, 0.0]}},
                    {"name": "Scale", "value": {"v": [100.0, 100.0, 100.0]}}
                ]}]}
            }));
        }
        layers.push(json!({
            "id": 11,
            "name": "Segment 1",
            "parent": if parent { json!(10) } else { json!(null) },
            "in_point": 0,
            "out_point": 2540160000000_i64,
            "source": {"type": "Footage", "item": 1},
            "props": {"children": [{"name": "Transform", "children": [
                {"name": "Anchor Point", "value": {"v": [anchor_x, 0.0, 0.0]}},
                {"name": "Position", "value": {"v": [position_x, 0.0, 0.0]}},
                {"name": "Scale", "value": {"v": [180.0, 180.0, 0.0]}}
            ]}]}
        }));
        json!({
            "items": {
                "1": {"name": "66M - VENTES.mp4", "kind": {"type": "Footage", "width": 1920, "height": 1080}},
                "2": {"name": "Comp", "kind": {
                    "type": "Comp", "width": 1080, "height": 1920,
                    "frame_rate": {"num": 25, "den": 1},
                    "duration": 2540160000000_i64,
                    "layers": layers
                }}
            }
        })
    }

    #[test]
    fn logo_outside_the_crop_is_a_pass() {
        let check = check_doc(&doc(960.0, 540.0, true), &catalog(), None);
        assert_eq!(check.status, crate::qa::model::QaStatus::Pass);
        assert!(check.detail.contains("hors cadre"));
    }

    #[test]
    fn logo_inside_the_crop_is_an_info() {
        let check = check_doc(&doc(0.0, 0.0, false), &catalog(), None);
        assert_eq!(check.status, crate::qa::model::QaStatus::Info);
        assert!(check.detail.contains("66 Minutes"));
        assert!(check.detail.contains("confirmation visuelle non activée"));
    }

    #[test]
    fn declared_overlay_primes_over_the_source_prior() {
        use crate::qa::model::OverlayRegion;
        let mask = OverlayMask {
            comp_width: 1080,
            comp_height: 1920,
            regions: vec![OverlayRegion {
                name: "logo.png".into(),
                layer_id: 1,
                start_s: 0.0,
                end_s: 10.0,
                bbox_xywh: [0.0, 1500.0, 1080.0, 400.0],
                opaque_pixels: 1000,
            }],
            skipped: Vec::new(),
        };
        let check = check_doc(&doc(0.0, 0.0, false), &catalog(), Some(&mask));
        assert_eq!(check.status, crate::qa::model::QaStatus::Pass);
        assert!(check.detail.contains("couvert par un overlay déclaré"));
    }

    #[test]
    fn rotated_projection_is_not_reported_as_out_of_frame() {
        let mut project = doc(0.0, 0.0, false);
        let transform =
            &mut project["items"]["2"]["kind"]["layers"][0]["props"]["children"][0]["children"];
        transform.as_array_mut().unwrap().push(json!({
            "name": "Rotation",
            "value": {"v": [45.0, 0.0, 0.0]}
        }));
        let check = check_doc(&project, &catalog(), None);
        assert_eq!(check.status, crate::qa::model::QaStatus::NotApplicable);
        assert!(check
            .detail
            .contains("transformations non prises en charge"));
    }

    #[test]
    fn unknown_source_has_no_prior() {
        let mut project = doc(0.0, 0.0, false);
        project["items"]["1"]["name"] = json!("Son travail empiète sur sa vie de famille.mp4");
        let check = check_doc(&project, &catalog(), None);
        assert_eq!(check.status, crate::qa::model::QaStatus::Pass);
        assert!(check.detail.contains("aucun logo source connu"));
    }
}
