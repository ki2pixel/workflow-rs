//! A7 — logos de mission email dans le projet assisté.
//!
//! La passe est strictement additive : le projet brut n'est jamais modifié,
//! et sans mission résolue elle ne touche à rien. Les assets sont validés
//! (1080×1920, alpha) avant toute insertion ; un asset non conforme part en
//! revue et n'est jamais placé au hasard.
//!
//! Classification des `.mov` :
//! - durée >= fin du dernier Segment : overlay plein durée, tronqué à `main` ;
//! - durée <= seuil et au moins un PNG : end-card après `main`, comp prolongée ;
//! - sinon : revue explicite, aucun `.mov` inséré.

use crate::post::ecproj::{
    self, comp, comp_mut, find_item_by_name, max_layer_id, max_uid, offset_uids, CompMeta,
    TICKS_PER_SECOND,
};
use crate::post::model::Decision;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use workflow_core::mission::{filename_key, ClientMission, VideoInstruction, VideoMatch};
use workflow_schema::round::round_to;

const COMP_WIDTH: u32 = 1080;
const COMP_HEIGHT: u32 = 1920;

static LAYER_TEMPLATE: OnceLock<Value> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementMode {
    Static,
    EndCard,
    FullDuration,
}

impl PlacementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::EndCard => "end_card",
            Self::FullDuration => "full_duration",
        }
    }
}

#[derive(Debug, Clone)]
pub struct A7Options<'a> {
    pub project: &'a Path,
    pub docs: &'a Path,
    pub stem: &'a str,
    pub lot: Option<u32>,
    pub main_s: f64,
    pub meta: &'a CompMeta,
    pub missions: &'a [ClientMission],
    /// Insère réellement les calques. Faux : résolution et audit seulement.
    pub overlay: bool,
    pub end_card_max_s: f64,
    pub ffprobe: &'a Path,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct A7Added {
    pub name: String,
    pub mode: String,
    pub start_s: f64,
    pub end_s: f64,
}

#[derive(Debug, Clone, Default)]
pub struct A7Outcome {
    pub doc: Value,
    pub decisions: Vec<Decision>,
    pub mission: Option<Value>,
    pub sidecar: Option<Value>,
    pub tail_s: f64,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetKind {
    Png,
    Mov,
}

#[derive(Debug, Clone)]
struct Asset {
    name: String,
    path: PathBuf,
    kind: AssetKind,
    width: u32,
    height: u32,
    duration_s: Option<f64>,
    has_audio: bool,
    codec: String,
}

/// Point d'entrée. Toute erreur interne devient une revue, jamais un échec
/// du post-traitement.
pub fn apply(doc: &Value, options: &A7Options<'_>) -> A7Outcome {
    let mut outcome = A7Outcome {
        doc: doc.clone(),
        ..A7Outcome::default()
    };
    let Some(lot) = options.lot else {
        return outcome;
    };
    let missions: Vec<&ClientMission> = options
        .missions
        .iter()
        .filter(|mission| mission.lot == lot)
        .collect();
    match missions.len() {
        0 => return outcome,
        1 => {}
        count => {
            outcome.decisions.push(Decision::review(
                "A7",
                "comp",
                format!("{count} missions pour le lot {lot}, aucune sélection automatique"),
                0.2,
                json!({"severity": "high", "reason": "ambiguous_lot", "lot": lot, "missions": count}),
            ));
            outcome.mission = Some(json!({
                "lot": lot,
                "decision": "review",
                "reasons": ["ambiguous_lot"],
                "missions": count,
                "added": [],
                "missing": [],
                "tail_s": 0.0,
            }));
            return outcome;
        }
    }
    let mission = missions[0];
    let instruction = match mission.match_video(options.stem) {
        VideoMatch::Matched(instruction) => instruction.clone(),
        VideoMatch::Ambiguous => {
            outcome.decisions.push(Decision::review(
                "A7",
                "comp",
                format!("plusieurs instructions pour {}", options.stem),
                0.3,
                json!({"severity": "high", "reason": "ambiguous_video", "stem": options.stem}),
            ));
            outcome.mission = Some(mission_block(
                mission,
                None,
                "review",
                vec!["ambiguous_video"],
                0.0,
                &[],
                &[],
                &[],
            ));
            return outcome;
        }
        VideoMatch::NotFound => {
            outcome.decisions.push(Decision::review(
                "A7",
                "comp",
                format!("aucune instruction email pour {}", options.stem),
                0.2,
                json!({"severity": "medium", "reason": "video_not_in_mission", "stem": options.stem}),
            ));
            outcome.mission = Some(mission_block(
                mission,
                None,
                "review",
                vec!["video_not_in_mission"],
                0.0,
                &[],
                &[],
                &[],
            ));
            return outcome;
        }
    };

    if instruction.no_logo {
        let conflict = options.overlay && comp_has_named_layer(&outcome.doc, "sdm.png");
        outcome.mission = Some(mission_block(
            mission,
            Some(&instruction),
            if conflict { "review" } else { "noop" },
            if conflict {
                vec!["no_logo_conflict_a6"]
            } else {
                vec!["no_logo"]
            },
            0.0,
            &[],
            &[],
            &[],
        ));
        if conflict {
            outcome.decisions.push(Decision::review(
                "A7",
                "comp",
                "mission « Pas de logo » mais template A6 sdm.png présent (non supprimé)",
                0.5,
                json!({"severity": "medium", "reason": "no_logo_conflict_a6"}),
            ));
        } else {
            outcome.decisions.push(Decision::noop(
                "A7",
                "comp",
                "mission « Pas de logo » : aucun calque ajouté",
                1.0,
                json!({"reason": "no_logo"}),
            ));
        }
        return outcome;
    }

    if instruction.logo_unresolved {
        outcome.decisions.push(Decision::review(
            "A7",
            "comp",
            "logos demandés mais non fournis par le client",
            0.3,
            json!({
                "severity": "high",
                "reason": "logo_unresolved",
                "notes": instruction.notes,
            }),
        ));
        outcome.mission = Some(mission_block(
            mission,
            Some(&instruction),
            "review",
            vec!["logo_unresolved"],
            0.0,
            &[],
            &[],
            &[],
        ));
        return outcome;
    }

    if instruction.logos.is_empty() {
        outcome.decisions.push(Decision::review(
            "A7",
            "comp",
            "instruction sans logo exploitable",
            0.2,
            json!({"severity": "medium", "reason": "no_logo_instruction"}),
        ));
        outcome.mission = Some(mission_block(
            mission,
            Some(&instruction),
            "review",
            vec!["no_logo_instruction"],
            0.0,
            &[],
            &[],
            &[],
        ));
        return outcome;
    }

    let (assets, missing) = resolve_assets(options, &instruction);
    let mut reasons: Vec<String> = Vec::new();
    if !missing.is_empty() {
        reasons.push("missing_asset".into());
    }
    let mut decisions = Vec::new();
    for reason in &missing {
        decisions.push(Decision::review(
            "A7",
            reason.name.clone(),
            reason.detail.clone(),
            0.2,
            json!({
                "severity": "high",
                "reason": reason.code,
                "asset": reason.name,
            }),
        ));
    }

    if !options.overlay {
        outcome.decisions = decisions;
        outcome.decisions.push(Decision::noop(
            "A7",
            "comp",
            "mission résolue, insertion A7 désactivée",
            1.0,
            json!({"reason": "overlay_disabled", "requested": instruction.logos}),
        ));
        outcome.mission = Some(mission_block(
            mission,
            Some(&instruction),
            "review",
            vec!["overlay_disabled"],
            0.0,
            &[],
            &missing
                .iter()
                .map(|reason| reason.name.clone())
                .collect::<Vec<_>>(),
            &[],
        ));
        return outcome;
    }

    let (plan, mut mode_reviews) = plan_assets(&assets, options.main_s, options.end_card_max_s);
    decisions.append(&mut mode_reviews);
    reasons.extend(plan.reasons.iter().cloned());

    if plan.additions.is_empty() {
        outcome.decisions = decisions;
        outcome.mission = Some(mission_block(
            mission,
            Some(&instruction),
            if reasons.is_empty() { "noop" } else { "review" },
            reasons,
            0.0,
            &[],
            &missing
                .iter()
                .map(|reason| reason.name.clone())
                .collect::<Vec<_>>(),
            &[],
        ));
        return outcome;
    }

    match insert_assets(&outcome.doc, &plan.additions, options) {
        Ok((new_doc, added, skipped)) => {
            outcome.doc = new_doc;
            // Une queue n'existe que si l'end-card a réellement été inséré.
            outcome.tail_s = if added.iter().any(|addition| addition.mode == "end_card") {
                plan.tail_s
            } else {
                0.0
            };
            outcome.changed = !added.is_empty();
            // Un asset déjà présent (template A6) n'est jamais dupliqué : la
            // mission est satisfaite, mais A7 n'a rien ajouté.
            let mode = if reasons.is_empty() {
                if added.is_empty() {
                    "noop"
                } else {
                    "apply"
                }
            } else {
                "review"
            };
            outcome.mission = Some(mission_block(
                mission,
                Some(&instruction),
                mode,
                reasons,
                outcome.tail_s,
                &added,
                &missing
                    .iter()
                    .map(|reason| reason.name.clone())
                    .collect::<Vec<_>>(),
                &skipped,
            ));
            if !added.is_empty() {
                decisions.push(Decision::apply(
                    "A7",
                    "comp",
                    format!("{} logo(s) de mission ajouté(s)", added.len()),
                    0.95,
                    json!({
                        "mission": mission.id,
                        "added": added,
                        "tail_s": outcome.tail_s,
                    }),
                ));
            } else if !skipped.is_empty() {
                decisions.push(Decision::noop(
                    "A7",
                    "comp",
                    format!("{} logo(s) déjà présent(s), aucun doublon", skipped.len()),
                    1.0,
                    json!({"reason": "already_present", "skipped": skipped}),
                ));
            }
            outcome.decisions = decisions;
        }
        Err(error) => {
            decisions.push(Decision::review(
                "A7",
                "comp",
                format!("insertion A7 impossible: {error:#}"),
                0.1,
                json!({"severity": "high", "reason": "insert_failed"}),
            ));
            outcome.decisions = decisions;
            outcome.mission = Some(mission_block(
                mission,
                Some(&instruction),
                "review",
                vec!["insert_failed"],
                0.0,
                &[],
                &missing
                    .iter()
                    .map(|reason| reason.name.clone())
                    .collect::<Vec<_>>(),
                &[],
            ));
        }
    }
    outcome
}

#[derive(Debug, Clone)]
struct MissingAsset {
    name: String,
    code: &'static str,
    detail: String,
}

fn resolve_assets(
    options: &A7Options<'_>,
    instruction: &VideoInstruction,
) -> (Vec<Asset>, Vec<MissingAsset>) {
    let mut assets = Vec::new();
    let mut missing = Vec::new();
    let mut seen = Vec::new();
    for name in &instruction.logos {
        let key = filename_key(name);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let Some(path) = resolve_asset(options.docs, name) else {
            missing.push(MissingAsset {
                name: name.clone(),
                code: "missing_asset",
                detail: format!("asset absent de {}", options.docs.display()),
            });
            continue;
        };
        let kind = match path
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_lowercase)
            .as_deref()
        {
            Some("png") => AssetKind::Png,
            Some("mov") => AssetKind::Mov,
            Some(other) => {
                missing.push(MissingAsset {
                    name: name.clone(),
                    code: "unsupported_asset",
                    detail: format!("extension .{other} non supportée par A7"),
                });
                continue;
            }
            None => {
                missing.push(MissingAsset {
                    name: name.clone(),
                    code: "unsupported_asset",
                    detail: "asset sans extension".into(),
                });
                continue;
            }
        };
        match probe_asset(&path, kind, options.ffprobe) {
            Ok(asset) => {
                if asset.width != COMP_WIDTH || asset.height != COMP_HEIGHT {
                    missing.push(MissingAsset {
                        name: name.clone(),
                        code: "asset_dimensions",
                        detail: format!(
                            "{}×{} au lieu de {}×{}",
                            asset.width, asset.height, COMP_WIDTH, COMP_HEIGHT
                        ),
                    });
                    continue;
                }
                assets.push(asset);
            }
            Err(error) => missing.push(MissingAsset {
                name: name.clone(),
                code: "asset_unreadable",
                detail: format!("sonde impossible: {error:#}"),
            }),
        }
    }
    (assets, missing)
}

/// Résout un asset exact, puis insensible à la casse et aux accents si unique.
fn resolve_asset(docs: &Path, name: &str) -> Option<PathBuf> {
    let direct = docs.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    let key = filename_key(name);
    let mut matches = Vec::new();
    let entries = std::fs::read_dir(docs).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(filename) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if filename_key(filename) == key {
            matches.push(path);
        }
    }
    (matches.len() == 1).then(|| matches.remove(0))
}

fn probe_asset(path: &Path, kind: AssetKind, ffprobe: &Path) -> Result<Asset> {
    match kind {
        AssetKind::Png => probe_png(path),
        AssetKind::Mov => probe_mov(path, ffprobe),
    }
}

fn probe_png(path: &Path) -> Result<Asset> {
    let file =
        std::fs::File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let reader = decoder.read_info().context("lecture PNG")?;
    let info = reader.info();
    let has_alpha = matches!(
        info.color_type,
        png::ColorType::GrayscaleAlpha | png::ColorType::Rgba
    ) || (info.color_type == png::ColorType::Indexed && info.trns.is_some());
    if !has_alpha {
        anyhow::bail!("PNG sans canal alpha ({:?})", info.color_type);
    }
    Ok(Asset {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        path: path.to_path_buf(),
        kind: AssetKind::Png,
        width: info.width,
        height: info.height,
        duration_s: None,
        has_audio: false,
        codec: "PNG".into(),
    })
}

fn probe_mov(path: &Path, ffprobe: &Path) -> Result<Asset> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,pix_fmt,codec_name,codec_long_name,duration:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .with_context(|| format!("ffprobe {}", path.display()))?;
    if !output.status.success() {
        anyhow::bail!(
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
    let width = stream.get("width").and_then(Value::as_u64).unwrap_or(0) as u32;
    let height = stream.get("height").and_then(Value::as_u64).unwrap_or(0) as u32;
    let pix_fmt = stream
        .get("pix_fmt")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !pix_fmt_has_alpha(pix_fmt) {
        anyhow::bail!("MOV sans alpha (pix_fmt={pix_fmt})");
    }
    let duration = stream
        .get("duration")
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| {
            value
                .get("format")
                .and_then(|format| format.get("duration"))
                .and_then(Value::as_str)
                .and_then(|value| value.parse::<f64>().ok())
        })
        .context("durée MOV inconnue")?;
    if !duration.is_finite() || duration <= 0.0 {
        anyhow::bail!("durée MOV invalide: {duration}");
    }
    // `-select_streams v:0` masque l'audio : on refait une sonde légère.
    let has_audio = audio_stream_present(path, ffprobe);
    Ok(Asset {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        path: path.to_path_buf(),
        kind: AssetKind::Mov,
        width,
        height,
        duration_s: Some(duration),
        has_audio,
        codec: stream
            .get("codec_long_name")
            .and_then(Value::as_str)
            .or_else(|| stream.get("codec_name").and_then(Value::as_str))
            .unwrap_or("Video")
            .to_string(),
    })
}

fn audio_stream_present(path: &Path, ffprobe: &Path) -> bool {
    Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=index",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map(|output| output.status.success() && !output.stdout.is_empty())
        .unwrap_or(false)
}

fn pix_fmt_has_alpha(pix_fmt: &str) -> bool {
    pix_fmt.starts_with("yuva")
        || pix_fmt.starts_with("rgba")
        || pix_fmt.starts_with("bgra")
        || pix_fmt.starts_with("argb")
        || pix_fmt.starts_with("abgr")
        || pix_fmt.starts_with("ya")
}

#[derive(Debug, Clone)]
struct Addition {
    asset: Asset,
    mode: PlacementMode,
    start_s: f64,
    end_s: f64,
}

#[derive(Debug, Default)]
struct Plan {
    additions: Vec<Addition>,
    tail_s: f64,
    reasons: Vec<String>,
}

fn plan_assets(assets: &[Asset], main_s: f64, end_card_max_s: f64) -> (Plan, Vec<Decision>) {
    let mut plan = Plan::default();
    let mut decisions = Vec::new();
    let pngs: Vec<&Asset> = assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Png)
        .collect();
    let movs: Vec<&Asset> = assets
        .iter()
        .filter(|asset| asset.kind == AssetKind::Mov)
        .collect();

    let mut png_additions = Vec::new();
    for asset in &pngs {
        if main_s <= 0.0 {
            plan.reasons.push("main_duration_missing".into());
            continue;
        }
        png_additions.push(Addition {
            asset: (*asset).clone(),
            mode: PlacementMode::Static,
            start_s: 0.0,
            end_s: main_s,
        });
    }

    if movs.len() > 1 {
        plan.reasons.push("multiple_mov".into());
        decisions.push(Decision::review(
            "A7",
            "comp",
            format!("{} .mov dans la mission, aucun inséré", movs.len()),
            0.3,
            json!({
                "severity": "high",
                "reason": "multiple_mov",
                "movs": movs.iter().map(|asset| asset.name.clone()).collect::<Vec<_>>(),
            }),
        ));
        plan.additions = png_additions;
        return (plan, decisions);
    }

    if let Some(mov) = movs.first() {
        let duration = mov.duration_s.unwrap_or(0.0);
        if duration >= main_s - 0.001 {
            plan.additions.push(Addition {
                asset: (*mov).clone(),
                mode: PlacementMode::FullDuration,
                start_s: 0.0,
                end_s: main_s,
            });
            plan.additions.extend(png_additions);
        } else if duration <= end_card_max_s + 0.001 && !pngs.is_empty() {
            // End-card au-dessus des PNG, comme la vérité terrain du lot 136.
            let mut additions = vec![Addition {
                asset: (*mov).clone(),
                mode: PlacementMode::EndCard,
                start_s: main_s,
                end_s: main_s + duration,
            }];
            additions.extend(png_additions);
            plan.additions = additions;
            plan.tail_s = duration;
        } else {
            let reason = if duration <= end_card_max_s + 0.001 {
                "short_mov_without_png"
            } else {
                "intermediate_mov"
            };
            plan.reasons.push(reason.into());
            decisions.push(Decision::review(
                "A7",
                mov.name.clone(),
                format!(
                    ".mov de {duration:.3}s avec main={main_s:.3}s et {} PNG : insertion refusée",
                    pngs.len()
                ),
                0.3,
                json!({
                    "severity": if reason == "intermediate_mov" { "high" } else { "medium" },
                    "reason": reason,
                    "mov_duration_s": round_to(duration, 3),
                    "main_s": round_to(main_s, 3),
                    "has_png": !pngs.is_empty(),
                }),
            ));
            plan.additions = png_additions;
        }
    } else {
        plan.additions = png_additions;
    }
    (plan, decisions)
}

fn insert_assets(
    doc: &Value,
    additions: &[Addition],
    options: &A7Options<'_>,
) -> Result<(Value, Vec<A7Added>, Vec<String>)> {
    if additions.is_empty() {
        return Ok((doc.clone(), Vec::new(), Vec::new()));
    }
    let mut output = doc.clone();
    let existing_layers = comp(doc)?
        .get("layers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut layer_names: Vec<String> = existing_layers
        .iter()
        .filter_map(|layer| layer.get("name").and_then(Value::as_str))
        .map(filename_key)
        .collect();
    let mut added = Vec::new();
    let mut skipped = Vec::new();
    let mut layers = Vec::new();
    let mut next_item = max_item_id(doc) + 1;
    let max_layer = max_layer_id(comp(doc)?);
    let uid_base = max_uid(doc) + 1000;
    let mut index = 0usize;

    for addition in additions {
        let asset_key = filename_key(&addition.asset.name);
        if layer_names.contains(&asset_key) {
            skipped.push(addition.asset.name.clone());
            continue;
        }
        let ticks_start = seconds_to_ticks(addition.start_s);
        let ticks_end = seconds_to_ticks(addition.end_s);
        let item_id = existing_item_for(doc, &addition.asset).unwrap_or(next_item);
        if item_id == next_item {
            next_item += 1;
            let item = build_item(&addition.asset, options, item_id);
            output
                .get_mut("items")
                .and_then(Value::as_object_mut)
                .context("projet sans items")?
                .insert(item_id.to_string(), item);
        }
        let mut layer = LAYER_TEMPLATE
            .get_or_init(|| {
                serde_json::from_str(include_str!(
                    "../../../../assets/effectcraft/sdm_logo_layer.json"
                ))
                .expect("asset sdm_logo_layer.json invalide")
            })
            .clone();
        let offset = 1000 * (index as i64 + 1);
        offset_uids(&mut layer, uid_base + offset);
        layer["id"] = json!(max_layer + 1 + index as i64);
        layer["name"] = json!(addition.asset.name);
        layer["source"]["item"] = json!(item_id);
        layer["label"] = json!(if addition.asset.kind == AssetKind::Mov {
            "Aqua"
        } else {
            "Lavender"
        });
        layer["start_time"] = json!(ticks_start);
        layer["in_point"] = json!(ticks_start);
        layer["out_point"] = json!(ticks_end);
        layer["switches"]["audio"] =
            json!(addition.asset.kind == AssetKind::Mov && addition.asset.has_audio);
        layer["props"]["name"] = json!(addition.asset.name);
        layer_names.push(asset_key);
        added.push(A7Added {
            name: addition.asset.name.clone(),
            mode: addition.mode.as_str().into(),
            start_s: round_to(addition.start_s, 3),
            end_s: round_to(addition.end_s, 3),
        });
        layers.push(layer);
        index += 1;
    }

    if layers.is_empty() {
        return Ok((doc.clone(), added, skipped));
    }

    let comp = comp_mut(&mut output)?;
    let duration = (options.main_s + plan_tail(additions)).max(0.0);
    comp["duration"] = json!(seconds_to_ticks(duration));
    let target = comp
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .context("composition sans calques")?;
    let mut all = layers;
    all.append(target);
    *target = all;
    let max_id = ecproj::max_layer_id(comp);
    let max_uid_value = max_uid(&output);
    output["next_id"] = json!(max_id.max(max_uid_value) + 1);
    Ok((output, added, skipped))
}

fn plan_tail(additions: &[Addition]) -> f64 {
    additions
        .iter()
        .filter(|addition| addition.mode == PlacementMode::EndCard)
        .map(|addition| addition.end_s - addition.start_s)
        .fold(0.0, f64::max)
}

fn seconds_to_ticks(seconds: f64) -> i64 {
    round_to(seconds.max(0.0) * TICKS_PER_SECOND, 0) as i64
}

fn max_item_id(doc: &Value) -> i64 {
    doc.get("items")
        .and_then(Value::as_object)
        .map(|items| {
            items
                .keys()
                .filter_map(|key| key.parse::<i64>().ok())
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn existing_item_for(doc: &Value, asset: &Asset) -> Option<i64> {
    let (id, item) = find_item_by_name(doc, &asset.name, "Footage")?;
    let path = item
        .get("kind")
        .and_then(|kind| kind.get("path"))
        .and_then(Value::as_str)?;
    let existing_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    (filename_key(existing_name) == filename_key(&asset.name)).then_some(id)
}

fn build_item(asset: &Asset, options: &A7Options<'_>, id: i64) -> Value {
    let fps_num = if options.meta.fps > 0.0 {
        options.meta.fps.round() as u64
    } else {
        25
    };
    let path = relative_footage_path(options.project, &asset.path);
    let duration = asset.duration_s.map(seconds_to_ticks).unwrap_or(0);
    json!({
        "id": id,
        "name": asset.name,
        "label": if asset.kind == AssetKind::Mov { "Aqua" } else { "Lavender" },
        "comment": "",
        "parent": null,
        "kind": {
            "type": "Footage",
            "path": path,
            "kind": if asset.kind == AssetKind::Mov { "Video" } else { "Still" },
            "width": asset.width,
            "height": asset.height,
            "pixel_aspect": 1.0,
            "frame_rate": {"num": fps_num, "den": 1},
            "native_rate": if asset.kind == AssetKind::Mov {
                json!({"num": fps_num, "den": 1})
            } else {
                Value::Null
            },
            "duration": duration,
            "has_video": true,
            "has_audio": asset.has_audio,
            "alpha": "Straight",
            "premul_color": [0.0, 0.0, 0.0],
            "loop_count": 1,
            "codec": asset.codec,
            "missing": false,
        }
    })
}

/// Chemin relatif au projet (`../docs/<nom>`), ou absolu hors structure.
fn relative_footage_path(project: &Path, asset: &Path) -> String {
    let project_dir = project.parent().unwrap_or_else(|| Path::new("."));
    relative_path(project_dir, asset)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| asset.to_string_lossy().to_string())
}

fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components: Vec<Component<'_>> = from.components().collect();
    let to_components: Vec<Component<'_>> = to.components().collect();
    let common = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return None;
    }
    let mut output = PathBuf::new();
    for _ in common..from_components.len() {
        output.push("..");
    }
    for component in &to_components[common..] {
        output.push(component.as_os_str());
    }
    Some(output)
}

fn comp_has_named_layer(doc: &Value, name: &str) -> bool {
    let key = filename_key(name);
    comp(doc)
        .ok()
        .and_then(|comp| comp.get("layers"))
        .and_then(Value::as_array)
        .is_some_and(|layers| {
            layers.iter().any(|layer| {
                layer
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| filename_key(candidate) == key)
            })
        })
}

#[allow(clippy::too_many_arguments)]
fn mission_block<R>(
    mission: &ClientMission,
    instruction: Option<&VideoInstruction>,
    decision: &str,
    reasons: R,
    tail_s: f64,
    added: &[A7Added],
    missing: &[String],
    skipped: &[String],
) -> Value
where
    R: IntoIterator,
    R::Item: Into<String>,
{
    let mut object = Map::new();
    object.insert("id".into(), json!(mission.id));
    object.insert("lot".into(), json!(mission.lot));
    object.insert("deadline".into(), json!(mission.deadline));
    object.insert("urgent".into(), json!(mission.urgent));
    object.insert(
        "video".into(),
        json!(instruction.map(|instruction| instruction.filename.clone())),
    );
    object.insert(
        "no_logo".into(),
        json!(instruction.is_some_and(|instruction| instruction.no_logo)),
    );
    object.insert(
        "requested".into(),
        json!(instruction
            .map(|instruction| instruction.logos.clone())
            .unwrap_or_default()),
    );
    object.insert("added".into(), json!(added));
    object.insert("missing".into(), json!(missing));
    object.insert("skipped".into(), json!(skipped));
    object.insert("tail_s".into(), json!(round_to(tail_s, 3)));
    object.insert("decision".into(), json!(decision));
    object.insert(
        "reasons".into(),
        json!(reasons.into_iter().map(Into::into).collect::<Vec<String>>()),
    );
    object.insert("warnings".into(), json!(mission.warnings));
    Value::Object(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn minimal_doc() -> Value {
        json!({
            "items": {
                "1": {"id": 1, "name": "clip.mp4", "kind": {"type": "Footage", "width": 1920, "height": 1080}},
                "2": {
                    "id": 2,
                    "name": "comp",
                    "kind": {
                        "type": "Comp",
                        "width": 1080,
                        "height": 1920,
                        "duration": seconds_to_ticks(10.0),
                        "frame_rate": {"num": 25, "den": 1},
                        "layers": [
                            {"id": 10, "name": "Segment 1", "source": {"type": "Footage", "item": 1}, "start_time": 0, "in_point": 0, "out_point": seconds_to_ticks(10.0), "props": {"children": []}}
                        ]
                    }
                }
            },
            "next_id": 11
        })
    }

    fn mission_with(logos: &[&str]) -> ClientMission {
        let body = format!(
            "Recadrage 9:16 standard à faire pour 10h00 :\n- clip.mp4 (Logos requis: {})",
            logos.join(", ")
        );
        ClientMission::from_body("recadrage_lot_1_20260101", 1, "Lot 1", body)
    }

    fn options<'a>(
        project: &'a Path,
        docs: &'a Path,
        meta: &'a CompMeta,
        missions: &'a [ClientMission],
    ) -> A7Options<'a> {
        A7Options {
            project,
            docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta,
            missions,
            overlay: true,
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
        }
    }

    #[test]
    fn no_mission_is_a_strict_noop() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let options = options(&project, &docs, &meta, &[]);
        let outcome = apply(&doc, &options);
        assert_eq!(outcome.doc, doc);
        assert!(outcome.decisions.is_empty());
        assert!(outcome.mission.is_none());
        assert!(!outcome.changed);
    }

    #[test]
    fn no_logo_adds_nothing() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mission = ClientMission::from_body(
            "recadrage_lot_1_20260101",
            1,
            "Lot 1",
            "Recadrage à faire pour 10h00 :\n- clip.mp4 (Pas de logo)",
        );
        let missions = vec![mission];
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let options = options(&project, &docs, &meta, &missions);
        let outcome = apply(&doc, &options);
        assert_eq!(outcome.doc, doc);
        assert_eq!(outcome.mission.as_ref().unwrap()["no_logo"], true);
        assert_eq!(outcome.mission.as_ref().unwrap()["added"], json!([]));
    }

    #[test]
    fn existing_layer_is_skipped_without_duplicate() {
        let mut doc = minimal_doc();
        let layers = doc["items"]["2"]["kind"]["layers"].as_array_mut().unwrap();
        layers.insert(
            0,
            json!({
                "id": 20,
                "name": "sdm.png",
                "source": {"type": "Footage", "item": 1},
                "start_time": 0,
                "in_point": 0,
                "out_point": seconds_to_ticks(10.0),
                "props": {"children": []}
            }),
        );
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let options = options(&project, &docs, &meta, &[]);
        let additions = vec![Addition {
            asset: Asset {
                name: "sdm.png".into(),
                path: docs.join("sdm.png"),
                kind: AssetKind::Png,
                width: 1080,
                height: 1920,
                duration_s: None,
                has_audio: false,
                codec: "PNG".into(),
            },
            mode: PlacementMode::Static,
            start_s: 0.0,
            end_s: 10.0,
        }];
        let (output, added, skipped) = insert_assets(&doc, &additions, &options).unwrap();
        assert_eq!(output, doc);
        assert!(added.is_empty());
        assert_eq!(skipped, vec!["sdm.png".to_string()]);
        assert_eq!(
            output["items"]["2"]["kind"]["layers"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn ambiguous_lot_never_selects_the_most_recent() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut first = mission_with(&["a.png"]);
        first.received_at = "2026-01-01T00:00:00+01:00".into();
        let mut second = mission_with(&["b.png"]);
        second.id = "recadrage_lot_1_20260102".into();
        second.received_at = "2026-01-02T00:00:00+01:00".into();
        let missions = vec![first, second];
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let options = options(&project, &docs, &meta, &missions);
        let outcome = apply(&doc, &options);
        assert_eq!(outcome.doc, doc);
        assert_eq!(outcome.decisions[0].details["reason"], "ambiguous_lot");
        assert_eq!(
            outcome.mission.as_ref().unwrap()["decision"],
            json!("review")
        );
    }

    #[test]
    fn plan_end_card_and_full_duration() {
        let dir = tempfile::tempdir().unwrap();
        let png = Asset {
            name: "a.png".into(),
            path: dir.path().join("a.png"),
            kind: AssetKind::Png,
            width: 1080,
            height: 1920,
            duration_s: None,
            has_audio: false,
            codec: "PNG".into(),
        };
        let mov = Asset {
            name: "a.mov".into(),
            path: dir.path().join("a.mov"),
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(3.0),
            has_audio: true,
            codec: "ProRes".into(),
        };
        let (plan, decisions) = plan_assets(&[png.clone(), mov.clone()], 10.0, 5.0);
        assert!(decisions.is_empty());
        assert_eq!(plan.tail_s, 3.0);
        assert_eq!(plan.additions[0].mode, PlacementMode::EndCard);
        assert_eq!(plan.additions[1].mode, PlacementMode::Static);

        let long = Asset {
            duration_s: Some(150.0),
            ..mov.clone()
        };
        let (plan, decisions) = plan_assets(&[long], 79.2, 5.0);
        assert!(decisions.is_empty());
        assert_eq!(plan.additions[0].mode, PlacementMode::FullDuration);
        assert_eq!(plan.additions[0].end_s, 79.2);
        assert_eq!(plan.tail_s, 0.0);
    }

    #[test]
    fn short_mov_alone_and_intermediate_mov_are_reviewed() {
        let dir = tempfile::tempdir().unwrap();
        let mov = Asset {
            name: "a.mov".into(),
            path: dir.path().join("a.mov"),
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(3.0),
            has_audio: true,
            codec: "ProRes".into(),
        };
        let (plan, decisions) = plan_assets(std::slice::from_ref(&mov), 44.4, 5.0);
        assert!(plan.additions.is_empty());
        assert_eq!(decisions[0].details["reason"], "short_mov_without_png");

        let intermediate = Asset {
            duration_s: Some(10.0),
            ..mov
        };
        let (plan, decisions) = plan_assets(&[intermediate], 44.4, 5.0);
        assert!(plan.additions.is_empty());
        assert_eq!(decisions[0].details["reason"], "intermediate_mov");
    }

    #[test]
    fn multiple_mov_keeps_png_and_reviews_mov() {
        let dir = tempfile::tempdir().unwrap();
        let png = Asset {
            name: "a.png".into(),
            path: dir.path().join("a.png"),
            kind: AssetKind::Png,
            width: 1080,
            height: 1920,
            duration_s: None,
            has_audio: false,
            codec: "PNG".into(),
        };
        let first = Asset {
            name: "a.mov".into(),
            path: dir.path().join("a.mov"),
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(3.0),
            has_audio: true,
            codec: "ProRes".into(),
        };
        let second = Asset {
            name: "b.mov".into(),
            path: dir.path().join("b.mov"),
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(2.0),
            has_audio: true,
            codec: "ProRes".into(),
        };
        let (plan, decisions) = plan_assets(&[png, first, second], 44.4, 5.0);
        assert_eq!(plan.additions.len(), 1);
        assert_eq!(plan.additions[0].mode, PlacementMode::Static);
        assert_eq!(decisions[0].details["reason"], "multiple_mov");
    }

    #[test]
    fn relative_paths_stay_portable() {
        let path = relative_path(
            Path::new("/mnt/cache/136 Cvmille/projets"),
            Path::new("/mnt/cache/136 Cvmille/docs/m6+.png"),
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("../docs/m6+.png"));
    }

    fn comp_meta(doc: &Value) -> Result<CompMeta> {
        ecproj::comp_meta(doc)
    }
}
