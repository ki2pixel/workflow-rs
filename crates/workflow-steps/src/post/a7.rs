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
use workflow_core::mission::{
    filename_key, strip_video_extension, ClientMission, LogoProvenance, LogoResolver, LogoTarget,
    MissionWarning, VideoInstruction, VideoMatch,
};
use workflow_schema::round::round_to;

const COMP_WIDTH: u32 = 1080;
const COMP_HEIGHT: u32 = 1920;

static LAYER_TEMPLATE: OnceLock<Value> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlacementMode {
    Static,
    EndCard,
    TailOverlay,
    FullDuration,
}

impl PlacementMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::EndCard => "end_card",
            Self::TailOverlay => "tail_overlay",
            Self::FullDuration => "full_duration",
        }
    }
}

/// Politique explicite d'un `.mov` de la banque de mission.
///
/// La table est fermée en 12.1 : elle fige les huit MOV vérifiés du corpus.
/// Les bumpers de 19,72 s restent en revue tant qu'ils n'ont pas été arbitrés.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovPolicy {
    EndCard,
    TailOverlay,
    FullDuration,
    Review(&'static str),
}

impl MovPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EndCard => "end_card",
            Self::TailOverlay => "tail_overlay",
            Self::FullDuration => "full_duration",
            Self::Review(_) => "review",
        }
    }

    pub fn reason(self) -> Option<&'static str> {
        match self {
            Self::Review(reason) => Some(reason),
            _ => None,
        }
    }
}

const MOV_POLICIES: &[(&str, MovPolicy)] = &[
    ("m6+.mov", MovPolicy::EndCard),
    ("MOTION_FIN_EM6_V03.mov", MovPolicy::TailOverlay),
    ("MSL1.mov", MovPolicy::FullDuration),
    (
        "9_16_RDV_PROGRAMME_EM6_V02_5MIN.mov",
        MovPolicy::FullDuration,
    ),
    ("the power.mov", MovPolicy::FullDuration),
    ("hayu x below deck.mov", MovPolicy::FullDuration),
    (
        "m6+max anime 1ep.mov",
        MovPolicy::Review("bumper de 19,72 s non arbitré (end-card ou tail overlay)"),
    ),
    (
        "W9_M6+MAX.mov",
        MovPolicy::Review("bumper de 19,72 s non arbitré (end-card ou tail overlay)"),
    ),
];

/// Politique déclarée pour un `.mov`, ou `None` si l'asset est inconnu.
pub fn mov_policy(name: &str) -> Option<MovPolicy> {
    let key = filename_key(name);
    MOV_POLICIES
        .iter()
        .find(|(candidate, _)| filename_key(candidate) == key)
        .map(|(_, policy)| *policy)
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
    /// Banque centrale de repli, optionnelle.
    pub mission_overlays: Option<&'a Path>,
    pub end_card_max_s: f64,
    pub ffprobe: &'a Path,
    /// Recherche une mission globale par titre quand le lot ne couvre pas le
    /// projet. Désactivé par défaut.
    pub global_fallback: bool,
    /// Borne haute (Unix) des missions acceptées par le fallback :
    /// génération du projet + 1 jour. `None` : pas de borne haute.
    pub cutoff_ts: Option<i64>,
    /// Borne basse (Unix) : génération du projet − ancienneté maximale.
    /// Une mission plus ancienne est ignorée, car les titres et les lots sont
    /// réutilisés au fil de l'année. `None` : pas de borne basse.
    pub window_start_ts: Option<i64>,
}

/// Ancienneté maximale d'une mission acceptée par le fallback global.
///
/// Validation Corpus 4 : 299 des 313 « nouveaux candidats » du shadow
/// correspondaient à des missions de plus de 90 jours, donc à des titres
/// réutilisés. 30 jours conservent les seules récupérations plausibles.
pub const GLOBAL_FALLBACK_MAX_AGE_S: i64 = 30 * 86_400;

#[derive(Debug, Clone, Default, Serialize)]
pub struct A7Added {
    pub name: String,
    /// Token tel qu'écrit dans l'email (ou nom recollé après découpage).
    pub requested: String,
    pub mode: String,
    pub start_s: f64,
    pub end_s: f64,
    /// `local_docs` ou `central_fallback`.
    pub source: String,
}

#[derive(Debug, Clone, Default)]
pub struct A7Outcome {
    pub doc: Value,
    pub decisions: Vec<Decision>,
    pub mission: Option<Value>,
    pub sidecar: Option<Value>,
    pub tail_s: f64,
    pub changed: bool,
    /// `lot` ou `global_fallback`, pour l'audit.
    pub match_source: Option<&'static str>,
}

/// Résultat de la sélection mission/instruction.
enum Selection<'a> {
    Mission(&'a ClientMission, &'a VideoInstruction, &'static str),
    AmbiguousLot { count: usize },
    AmbiguousVideo(&'a ClientMission),
    AmbiguousGlobal { missions: Vec<String> },
    NotFound,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssetKind {
    Png,
    Mov,
}

#[derive(Debug, Clone)]
struct Asset {
    name: String,
    /// Token d'origine dans l'email, conservé pour la traçabilité.
    requested: String,
    path: PathBuf,
    /// `local_docs` ou `central_fallback`.
    provenance: LogoProvenance,
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
    let (mission, instruction, match_source) = match select_mission(options) {
        Selection::Mission(mission, instruction, source) => (mission, instruction, source),
        Selection::AmbiguousLot { count } => {
            let lot = options.lot.unwrap_or(0);
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
        Selection::AmbiguousVideo(mission) => {
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
        Selection::AmbiguousGlobal { missions } => {
            outcome.decisions.push(Decision::review(
                "A7",
                "comp",
                format!(
                    "plusieurs missions globales pour {} : sélection refusée",
                    options.stem
                ),
                0.3,
                json!({
                    "severity": "high",
                    "reason": "ambiguous_global_video",
                    "stem": options.stem,
                    "missions": missions,
                }),
            ));
            return outcome;
        }
        Selection::NotFound => {
            let mission = options
                .lot
                .and_then(|lot| options.missions.iter().find(|mission| mission.lot == lot));
            outcome.decisions.push(Decision::review(
                "A7",
                "comp",
                format!("aucune instruction email pour {}", options.stem),
                0.2,
                json!({"severity": "medium", "reason": "video_not_in_mission", "stem": options.stem}),
            ));
            outcome.mission = mission.map(|mission| {
                mission_block(
                    mission,
                    None,
                    "review",
                    vec!["video_not_in_mission"],
                    0.0,
                    &[],
                    &[],
                    &[],
                )
            });
            return outcome;
        }
        Selection::None => return outcome,
    };
    outcome.match_source = Some(match_source);
    let instruction = instruction.clone();

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

    let (assets, missing, asset_warnings) = resolve_assets(options, &instruction);
    // Un repli central sur asset local corrompu est un incident non bloquant :
    // il est consigné dans `warnings` sans modifier `reasons` ni la décision.
    let mut mission_with_asset_warnings = None;
    if !asset_warnings.is_empty() {
        let mut copy = mission.clone();
        copy.warnings.extend(asset_warnings);
        mission_with_asset_warnings = Some(copy);
    }
    let mission = mission_with_asset_warnings.as_ref().unwrap_or(mission);
    let mut reasons: Vec<String> = Vec::new();
    if missing.iter().any(|reason| reason.code != "known_absent") {
        reasons.push("missing_asset".into());
    }
    if missing.iter().any(|reason| reason.code == "known_absent") {
        reasons.push("known_absent".into());
    }
    let mut decisions = Vec::new();
    for reason in &missing {
        let severity = if reason.code == "known_absent" {
            "medium"
        } else {
            "high"
        };
        decisions.push(Decision::review(
            "A7",
            reason.name.clone(),
            reason.detail.clone(),
            0.2,
            json!({
                "severity": severity,
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
            &missing,
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
            &missing,
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
                &missing,
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
                &missing,
                &[],
            ));
        }
    }
    outcome
}

/// Sélectionne la mission et l'instruction du projet.
///
/// Ordre : mission du lot (comportement historique), puis, si le titre n'y
/// figure pas et que le fallback est actif, mission globale par titre exact ou
/// par tokens identiques. Un lot porteur de plusieurs missions reste ambigu.
fn select_mission<'a>(options: &A7Options<'a>) -> Selection<'a> {
    let Some(lot) = options.lot else {
        return if options.global_fallback {
            global_match(options).unwrap_or(Selection::None)
        } else {
            Selection::None
        };
    };
    let lot_missions: Vec<&ClientMission> = options
        .missions
        .iter()
        .filter(|mission| mission.lot == lot)
        .collect();
    match lot_missions.len() {
        0 => {
            if options.global_fallback {
                global_match(options).unwrap_or(Selection::None)
            } else {
                Selection::None
            }
        }
        1 => {
            let mission = lot_missions[0];
            match mission.match_video(options.stem) {
                VideoMatch::Matched(instruction) => Selection::Mission(mission, instruction, "lot"),
                VideoMatch::Ambiguous => Selection::AmbiguousVideo(mission),
                VideoMatch::NotFound if options.global_fallback => {
                    global_match(options).unwrap_or(Selection::NotFound)
                }
                VideoMatch::NotFound => Selection::NotFound,
            }
        }
        count => Selection::AmbiguousLot { count },
    }
}

/// Cherche une mission globale par titre, sous cutoff temporel.
///
/// Le titre exact (`match_video`) prime. À défaut, une correspondance par
/// tokens identiques (Jaccard ≥ 0,85, au moins 5 tokens) couvre les variantes
/// d'ordre des mots. Plusieurs missions ne sont acceptées que si leurs
/// instructions sont équivalentes ; sinon la sélection est refusée.
fn global_match<'a>(options: &A7Options<'a>) -> Option<Selection<'a>> {
    let mut exact: Vec<(&ClientMission, &VideoInstruction)> = Vec::new();
    let mut fuzzy: Vec<(&ClientMission, &VideoInstruction)> = Vec::new();
    let mut ambiguous: Option<&ClientMission> = None;
    for mission in options.missions {
        if !within_window(mission, options.window_start_ts, options.cutoff_ts) {
            continue;
        }
        let exact_instructions = exact_instructions(mission, options.stem);
        match exact_instructions.len() {
            0 => {
                if let Some(instruction) = fuzzy_instruction(mission, options.stem) {
                    fuzzy.push((mission, instruction));
                }
            }
            1 => exact.push((mission, exact_instructions[0])),
            _ => {
                ambiguous.get_or_insert(mission);
            }
        }
    }
    if let Some(selection) = choose_match(exact) {
        return Some(selection);
    }
    if let Some(selection) = choose_match(fuzzy) {
        return Some(selection);
    }
    ambiguous.map(Selection::AmbiguousVideo)
}

fn choose_match<'a>(
    mut candidates: Vec<(&'a ClientMission, &'a VideoInstruction)>,
) -> Option<Selection<'a>> {
    candidates.sort_by(|left, right| left.0.id.cmp(&right.0.id));
    candidates.dedup_by(|left, right| left.0.id == right.0.id);
    match candidates.len() {
        0 => None,
        1 => Some(Selection::Mission(
            candidates[0].0,
            candidates[0].1,
            "global_fallback",
        )),
        _ => {
            let first = instruction_signature(candidates[0].1);
            if candidates
                .iter()
                .all(|(_, instruction)| instruction_signature(instruction) == first)
            {
                let best = candidates
                    .iter()
                    .max_by_key(|(mission, _)| mission_timestamp(mission).unwrap_or(0))
                    .copied()
                    .unwrap_or(candidates[0]);
                Some(Selection::Mission(best.0, best.1, "global_fallback"))
            } else {
                Some(Selection::AmbiguousGlobal {
                    missions: candidates
                        .iter()
                        .map(|(mission, _)| mission.id.clone())
                        .collect(),
                })
            }
        }
    }
}

fn instruction_signature(instruction: &VideoInstruction) -> (bool, bool, Vec<String>) {
    let mut logos: Vec<String> = instruction
        .logos
        .iter()
        .map(|logo| filename_key(logo))
        .collect();
    logos.sort();
    logos.dedup();
    (instruction.no_logo, instruction.logo_unresolved, logos)
}

/// Instructions dont le titre correspond exactement au projet, avec la
/// normalisation du fallback (suffixe `_TAT_<chaîne>` retiré).
fn exact_instructions<'a>(mission: &'a ClientMission, stem: &str) -> Vec<&'a VideoInstruction> {
    let candidate = filename_key(stem);
    let candidate_stem = strip_video_extension(&candidate);
    let candidate_fallback = fallback_key(stem);
    let mut found: Vec<&VideoInstruction> = Vec::new();
    for instruction in &mission.videos {
        let direct = instruction.key == candidate
            || instruction.stem_key == candidate
            || instruction.stem_key == candidate_stem;
        let fallback = !candidate_fallback.is_empty()
            && fallback_key(&instruction.filename) == candidate_fallback;
        if (direct || fallback)
            && !found
                .iter()
                .any(|existing| std::ptr::eq(*existing, instruction))
        {
            found.push(instruction);
        }
    }
    found
}

/// Clé de comparaison du fallback : `filename_key` sans extension et sans
/// suffixe `_TAT_<chaîne>` (nommage interne des médias de mission).
fn fallback_key(value: &str) -> String {
    let key = filename_key(value);
    let key = strip_video_extension(&key);
    let Some(index) = key.rfind("_tat_") else {
        return key.trim().to_string();
    };
    let suffix = &key[index + 5..];
    if !suffix.is_empty() && suffix.chars().all(|character| character.is_alphanumeric()) {
        key[..index].trim().to_string()
    } else {
        key.trim().to_string()
    }
}

/// Variante d'ordre des mots : mêmes tokens (Jaccard ≥ 0,85).
fn fuzzy_instruction<'a>(mission: &'a ClientMission, stem: &str) -> Option<&'a VideoInstruction> {
    let stem_tokens = token_set(&fallback_key(stem));
    if stem_tokens.len() < 5 {
        return None;
    }
    let mut found: Option<&VideoInstruction> = None;
    for instruction in &mission.videos {
        let tokens = token_set(&fallback_key(&instruction.filename));
        if tokens.len() < 5 {
            continue;
        }
        let intersection = tokens.intersection(&stem_tokens).count();
        let union = tokens.union(&stem_tokens).count();
        if union == 0 || intersection as f64 / (union as f64) < 0.85 {
            continue;
        }
        if found.is_some() {
            return None;
        }
        found = Some(instruction);
    }
    found
}

fn token_set(value: &str) -> std::collections::HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

fn within_window(mission: &ClientMission, start: Option<i64>, end: Option<i64>) -> bool {
    if start.is_none() && end.is_none() {
        return true;
    }
    let Some(timestamp) = mission_timestamp(mission) else {
        return true;
    };
    start.is_none_or(|start| timestamp >= start) && end.is_none_or(|end| timestamp <= end)
}

fn mission_timestamp(mission: &ClientMission) -> Option<i64> {
    [mission.received_at.as_str(), mission.stored_at.as_str()]
        .into_iter()
        .filter_map(parse_timestamp)
        .max()
}

fn parse_timestamp(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.timestamp())
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc2822(value)
                .map(|value| value.timestamp())
                .ok()
        })
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
) -> (Vec<Asset>, Vec<MissingAsset>, Vec<MissionWarning>) {
    let mut assets = Vec::new();
    let mut missing = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = Vec::new();
    let mut seen_paths: Vec<PathBuf> = Vec::new();
    let mut retained_paths: Vec<PathBuf> = Vec::new();
    let mut resolver = LogoResolver::new(options.docs);
    if let Some(bank) = options
        .mission_overlays
        .and_then(|path| path.canonicalize().ok())
    {
        if bank.is_dir() {
            resolver = resolver.with_fallback(LogoResolver::new(bank));
        }
    }
    let logos = &instruction.logos;
    let mut index = 0usize;
    while index < logos.len() {
        let (target, requested, consumed) = resolver.resolve_sequence_target(logos, index);
        index += consumed;
        let key = filename_key(&requested);
        if key.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        let (path, provenance) = match target {
            LogoTarget::Found { path, provenance } => (path, provenance),
            LogoTarget::KnownAbsent(note) => {
                missing.push(MissingAsset {
                    name: requested,
                    code: "known_absent",
                    detail: note.to_string(),
                });
                continue;
            }
            LogoTarget::Ambiguous(note) => {
                missing.push(MissingAsset {
                    name: requested,
                    code: "ambiguous_alias",
                    detail: note.to_string(),
                });
                continue;
            }
            LogoTarget::Missing => {
                let mut detail = format!("asset absent de {}", options.docs.display());
                if let Some(bank) = options.mission_overlays {
                    detail.push_str(&format!(" et de {}", bank.display()));
                }
                missing.push(MissingAsset {
                    name: requested,
                    code: "missing_asset",
                    detail,
                });
                continue;
            }
        };
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if seen_paths.contains(&canonical) {
            continue;
        }
        seen_paths.push(canonical.clone());
        let kind = match asset_kind(&requested, &path) {
            Ok(kind) => kind,
            Err(reason) => {
                missing.push(reason);
                continue;
            }
        };
        match probe_checked(&requested, &path, kind, options.ffprobe) {
            Ok(mut asset) => {
                retained_paths.push(canonical);
                asset.requested = requested;
                asset.provenance = provenance;
                assets.push(asset);
            }
            Err(local_error) => {
                // Un asset local présent mais illisible (tronqué, corrompu)
                // ou hors profil ne doit jamais masquer son homologue sain de
                // la banque centrale (D-069).
                let mut failure = local_error;
                let mut recovered = false;
                if provenance == LogoProvenance::LocalDocs {
                    if let LogoTarget::Found {
                        path: fallback_path,
                        ..
                    } = resolver.resolve_fallback_target(&requested)
                    {
                        let fallback_canonical = fallback_path
                            .canonicalize()
                            .unwrap_or_else(|_| fallback_path.clone());
                        if fallback_canonical == canonical {
                            // `mission_overlays` confondu avec `docs/` : la
                            // banque renvoie le même fichier invalide.
                        } else if retained_paths.contains(&fallback_canonical) {
                            // Asset déjà retenu pour un autre token : la
                            // mission reste satisfaite sans doublon.
                            recovered = true;
                        } else if let Ok(fallback_kind) = asset_kind(&requested, &fallback_path) {
                            match probe_checked(
                                &requested,
                                &fallback_path,
                                fallback_kind,
                                options.ffprobe,
                            ) {
                                Ok(mut asset) => {
                                    seen_paths.push(fallback_canonical.clone());
                                    retained_paths.push(fallback_canonical);
                                    asset.requested = requested.clone();
                                    asset.provenance = LogoProvenance::CentralFallback;
                                    warnings.push(MissionWarning {
                                        code: "local_asset_corrupted_fallback_used".into(),
                                        line: instruction.line,
                                        detail: format!(
                                            "{}: {} ; repli sur la banque centrale ({})",
                                            requested, failure.detail, asset.name
                                        ),
                                    });
                                    assets.push(asset);
                                    recovered = true;
                                }
                                Err(fallback_error) => {
                                    let detail = format!(
                                        "{} ; repli central {} non exploitable: {}",
                                        failure.detail,
                                        fallback_path.display(),
                                        fallback_error.detail
                                    );
                                    failure = MissingAsset {
                                        name: requested.clone(),
                                        code: failure.code,
                                        detail,
                                    };
                                }
                            }
                        }
                    }
                }
                if !recovered {
                    missing.push(failure);
                }
            }
        }
    }
    (assets, missing, warnings)
}

/// Détermine le type A7 à partir de l'extension, sans ouvrir le fichier.
fn asset_kind(name: &str, path: &Path) -> Result<AssetKind, MissingAsset> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => Ok(AssetKind::Png),
        Some("mov") => Ok(AssetKind::Mov),
        Some(other) => Err(MissingAsset {
            name: name.to_string(),
            code: "unsupported_asset",
            detail: format!("extension .{other} non supportée par A7"),
        }),
        None => Err(MissingAsset {
            name: name.to_string(),
            code: "unsupported_asset",
            detail: "asset sans extension".into(),
        }),
    }
}

/// Sonde un asset et vérifie le profil 1080×1920 imposé par A7.
fn probe_checked(
    name: &str,
    path: &Path,
    kind: AssetKind,
    ffprobe: &Path,
) -> Result<Asset, MissingAsset> {
    let asset = probe_asset(path, kind, ffprobe).map_err(|error| MissingAsset {
        name: name.to_string(),
        code: "asset_unreadable",
        detail: format!("sonde impossible: {error:#}"),
    })?;
    if asset.width != COMP_WIDTH || asset.height != COMP_HEIGHT {
        return Err(MissingAsset {
            name: name.to_string(),
            code: "asset_dimensions",
            detail: format!(
                "{}×{} au lieu de {}×{}",
                asset.width, asset.height, COMP_WIDTH, COMP_HEIGHT
            ),
        });
    }
    Ok(asset)
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
        requested: String::new(),
        path: path.to_path_buf(),
        provenance: LogoProvenance::LocalDocs,
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
        requested: String::new(),
        path: path.to_path_buf(),
        provenance: LogoProvenance::LocalDocs,
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
    requested: String,
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
            requested: asset.requested.clone(),
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
        let policy = mov_policy(&mov.name);
        match policy {
            Some(MovPolicy::TailOverlay) => {
                if main_s <= 0.0 {
                    plan.reasons.push("main_duration_missing".into());
                    plan.additions = png_additions;
                } else if duration <= 0.0 {
                    plan.reasons.push("invalid_mov_duration".into());
                    decisions.push(Decision::review(
                        "A7",
                        mov.name.clone(),
                        "tail overlay avec durée nulle".to_string(),
                        0.3,
                        json!({
                            "severity": "high",
                            "reason": "invalid_mov_duration",
                        }),
                    ));
                    plan.additions = png_additions;
                } else if duration > main_s + 0.001 {
                    plan.reasons.push("short_video_for_tail_overlay".into());
                    decisions.push(Decision::review(
                        "A7",
                        mov.name.clone(),
                        format!(
                            ".mov de {duration:.3}s ne tient pas dans les {main_s:.3}s de la vidéo"
                        ),
                        0.3,
                        json!({
                            "severity": "high",
                            "reason": "short_video_for_tail_overlay",
                            "mov_duration_s": round_to(duration, 3),
                            "main_s": round_to(main_s, 3),
                        }),
                    ));
                    plan.additions = png_additions;
                } else {
                    plan.additions.push(Addition {
                        asset: (*mov).clone(),
                        mode: PlacementMode::TailOverlay,
                        start_s: (main_s - duration).max(0.0),
                        end_s: main_s,
                        requested: mov.requested.clone(),
                    });
                    plan.additions.extend(png_additions);
                }
            }
            Some(MovPolicy::EndCard) => {
                if pngs.is_empty() {
                    plan.reasons.push("short_mov_without_png".into());
                    decisions.push(Decision::review(
                        "A7",
                        mov.name.clone(),
                        "end-card déclarée sans PNG valide dans la mission".to_string(),
                        0.3,
                        json!({
                            "severity": "medium",
                            "reason": "short_mov_without_png",
                            "mov_duration_s": round_to(duration, 3),
                        }),
                    ));
                    plan.additions = png_additions;
                } else if duration <= 0.0 {
                    plan.reasons.push("invalid_mov_duration".into());
                    decisions.push(Decision::review(
                        "A7",
                        mov.name.clone(),
                        "end-card avec durée nulle".to_string(),
                        0.3,
                        json!({
                            "severity": "high",
                            "reason": "invalid_mov_duration",
                        }),
                    ));
                    plan.additions = png_additions;
                } else {
                    let mut additions = vec![Addition {
                        asset: (*mov).clone(),
                        mode: PlacementMode::EndCard,
                        start_s: main_s,
                        end_s: main_s + duration,
                        requested: mov.requested.clone(),
                    }];
                    additions.extend(png_additions);
                    plan.additions = additions;
                    plan.tail_s = duration;
                }
            }
            Some(MovPolicy::FullDuration) => {
                if main_s <= 0.0 {
                    plan.reasons.push("main_duration_missing".into());
                    plan.additions = png_additions;
                } else if duration + 0.001 < main_s {
                    plan.reasons.push("full_duration_too_short".into());
                    decisions.push(Decision::review(
                        "A7",
                        mov.name.clone(),
                        format!(
                            "habillage de {duration:.3}s plus court que la vidéo ({main_s:.3}s)"
                        ),
                        0.3,
                        json!({
                            "severity": "high",
                            "reason": "full_duration_too_short",
                            "mov_duration_s": round_to(duration, 3),
                            "main_s": round_to(main_s, 3),
                        }),
                    ));
                    plan.additions = png_additions;
                } else {
                    plan.additions.push(Addition {
                        asset: (*mov).clone(),
                        mode: PlacementMode::FullDuration,
                        start_s: 0.0,
                        end_s: main_s,
                        requested: mov.requested.clone(),
                    });
                    plan.additions.extend(png_additions);
                }
            }
            Some(MovPolicy::Review(reason)) => {
                plan.reasons.push("policy_review".into());
                decisions.push(Decision::review(
                    "A7",
                    mov.name.clone(),
                    reason.to_string(),
                    0.3,
                    json!({
                        "severity": "medium",
                        "reason": "policy_review",
                        "detail": reason,
                        "mov_duration_s": round_to(duration, 3),
                    }),
                ));
                plan.additions = png_additions;
            }
            None => {
                if duration >= main_s - 0.001 {
                    plan.additions.push(Addition {
                        asset: (*mov).clone(),
                        mode: PlacementMode::FullDuration,
                        start_s: 0.0,
                        end_s: main_s,
                        requested: mov.requested.clone(),
                    });
                    plan.additions.extend(png_additions);
                } else if duration <= end_card_max_s + 0.001 && !pngs.is_empty() {
                    // End-card au-dessus des PNG, comme la vérité terrain du lot 136.
                    let mut additions = vec![Addition {
                        asset: (*mov).clone(),
                        mode: PlacementMode::EndCard,
                        start_s: main_s,
                        end_s: main_s + duration,
                        requested: mov.requested.clone(),
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
            }
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
        layer["switches"]["audio"] = json!(
            addition.asset.kind == AssetKind::Mov
                && addition.asset.has_audio
                && addition.mode != PlacementMode::TailOverlay
        );
        layer["props"]["name"] = json!(addition.asset.name);
        layer_names.push(asset_key);
        added.push(A7Added {
            name: addition.asset.name.clone(),
            requested: addition.requested.clone(),
            mode: addition.mode.as_str().into(),
            start_s: round_to(addition.start_s, 3),
            end_s: round_to(addition.end_s, 3),
            source: addition.asset.provenance.as_str().into(),
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
    missing: &[MissingAsset],
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
    object.insert(
        "missing".into(),
        json!(missing
            .iter()
            .map(|reason| reason.name.clone())
            .collect::<Vec<_>>()),
    );
    object.insert(
        "known_absent".into(),
        json!(missing
            .iter()
            .filter(|reason| reason.code == "known_absent")
            .map(|reason| json!({"name": reason.name, "note": reason.detail}))
            .collect::<Vec<_>>()),
    );
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

    fn write_test_png(path: &Path) {
        write_test_png_at(path, COMP_WIDTH, COMP_HEIGHT);
    }

    fn write_test_png_at(path: &Path, width: u32, height: u32) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![0u8; (width * height * 4) as usize])
            .unwrap();
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
            mission_overlays: None,
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
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
    fn global_fallback_resolves_no_logo_across_lots() {
        // Given: le lot 1 ne porte aucune mission, le lot 2 porte `clip.mp4`.
        // When:  le fallback global est actif.
        // Then:  la mission est retrouvée et produit un noop `no_logo`.
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mission = ClientMission::from_body(
            "recadrage_lot_2_20260101",
            2,
            "Lot 2",
            "Recadrage à faire pour 10h00 :\n- clip_TAT_M6.mp4 (Pas de logo)",
        );
        let missions = vec![mission];
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let mut options = options(&project, &docs, &meta, &missions);
        options.global_fallback = true;
        let outcome = apply(&doc, &options);
        assert_eq!(outcome.match_source, Some("global_fallback"));
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "noop");
        assert_eq!(block["no_logo"], true);
        assert!(outcome
            .decisions
            .iter()
            .any(|decision| decision.details["reason"] == "no_logo"));
    }

    #[test]
    fn global_fallback_respects_cutoff() {
        // Given: une mission datée après la génération du projet.
        // When:  le cutoff est antérieur.
        // Then:  aucune mission n'est retenue.
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut mission = ClientMission::from_body(
            "recadrage_lot_2_20260101",
            2,
            "Lot 2",
            "Recadrage à faire pour 10h00 :\n- clip.mp4 (Pas de logo)",
        );
        mission.received_at = "2026-11-20T12:00:00+00:00".into();
        let missions = vec![mission];
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let mut options = options(&project, &docs, &meta, &missions);
        options.global_fallback = true;
        options.cutoff_ts = Some(1_700_000_000);
        let outcome = apply(&doc, &options);
        assert!(outcome.mission.is_none());
        assert!(outcome.decisions.is_empty());
    }

    #[test]
    fn global_fallback_rejects_stale_mission() {
        // Given: une mission de janvier et un projet de novembre.
        // When:  la fenêtre basse est fixée à 30 jours avant le projet.
        // Then:  la mission est ignorée, même si le titre correspond.
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut mission = ClientMission::from_body(
            "recadrage_lot_2_20260123",
            2,
            "Lot 2",
            "Recadrage à faire pour 10h00 :\n- clip.mp4 (Pas de logo)",
        );
        mission.received_at = "2026-01-23T10:00:00+01:00".into();
        let missions = vec![mission];
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let mut options = options(&project, &docs, &meta, &missions);
        options.global_fallback = true;
        options.window_start_ts = Some(
            chrono::DateTime::parse_from_rfc3339("2026-10-20T00:00:00+00:00")
                .unwrap()
                .timestamp(),
        );
        options.cutoff_ts = Some(
            chrono::DateTime::parse_from_rfc3339("2026-11-21T00:00:00+00:00")
                .unwrap()
                .timestamp(),
        );
        let outcome = apply(&doc, &options);
        assert!(outcome.mission.is_none());
        assert!(outcome.decisions.is_empty());
    }

    #[test]
    fn global_fallback_refuses_divergent_missions() {
        // Given: deux missions du même titre, l'une no_logo, l'autre avec logo.
        // When:  le fallback global les trouve toutes les deux.
        // Then:  la sélection est refusée en revue.
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let no_logo = ClientMission::from_body(
            "recadrage_lot_2_20260101",
            2,
            "Lot 2",
            "Recadrage à faire pour 10h00 :\n- clip.mp4 (Pas de logo)",
        );
        let with_logo = ClientMission::from_body(
            "recadrage_lot_3_20260101",
            3,
            "Lot 3",
            "Recadrage à faire pour 10h00 :\n- clip.mp4 (Logos requis: central.png)",
        );
        let missions = vec![no_logo, with_logo];
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        let mut options = options(&project, &docs, &meta, &missions);
        options.global_fallback = true;
        let outcome = apply(&doc, &options);
        assert!(outcome.mission.is_none());
        assert!(outcome
            .decisions
            .iter()
            .any(|decision| decision.details["reason"] == "ambiguous_global_video"));
    }

    #[test]
    fn fallback_key_strips_tat_suffix() {
        assert_eq!(fallback_key("COUPLE LIBRE_TAT_Téva.mp4"), "couple libre");
        assert_eq!(fallback_key("clip.mp4"), "clip");
        assert_eq!(fallback_key("Maeva_TAT_M6_v2.mp4"), "maeva_tat_m6_v2");
    }

    #[test]
    fn fuzzy_instruction_matches_reordered_title() {
        // Given: un titre réordonné qui partage 8 tokens sur 9.
        // When:  la variante floue est évaluée.
        // Then:  elle est reconnue.
        let mission = ClientMission::from_body(
            "recadrage_lot_277_20261120",
            277,
            "Lot 277",
            "Recadrage à faire pour 10h00 :\n\
             - Greg fait une déclaration d'amour à Maeva.mp4 (Pas de logo)",
        );
        let found = fuzzy_instruction(
            &mission,
            "Maeva fait une grosse déclaration d’amour à Greg Greg fait une déclaration d’amour à Maeva",
        );
        assert!(found.is_some());
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
                requested: "sdm.png".into(),
                path: docs.join("sdm.png"),
                provenance: LogoProvenance::LocalDocs,
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
            requested: "sdm.png".into(),
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
            requested: "a.png".into(),
            path: dir.path().join("a.png"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Png,
            width: 1080,
            height: 1920,
            duration_s: None,
            has_audio: false,
            codec: "PNG".into(),
        };
        let mov = Asset {
            name: "a.mov".into(),
            requested: "a.mov".into(),
            path: dir.path().join("a.mov"),
            provenance: LogoProvenance::LocalDocs,
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
            requested: "a.mov".into(),
            path: dir.path().join("a.mov"),
            provenance: LogoProvenance::LocalDocs,
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
            requested: "a.png".into(),
            path: dir.path().join("a.png"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Png,
            width: 1080,
            height: 1920,
            duration_s: None,
            has_audio: false,
            codec: "PNG".into(),
        };
        let first = Asset {
            name: "a.mov".into(),
            requested: "a.mov".into(),
            path: dir.path().join("a.mov"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(3.0),
            has_audio: true,
            codec: "ProRes".into(),
        };
        let second = Asset {
            name: "b.mov".into(),
            requested: "b.mov".into(),
            path: dir.path().join("b.mov"),
            provenance: LogoProvenance::LocalDocs,
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

    #[test]
    fn central_fallback_adds_layer_with_portable_path() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        let bank = dir.path().join("assets/mission_overlays");
        std::fs::create_dir_all(&bank).unwrap();
        write_test_png(&bank.join("central.png"));
        let missions = vec![mission_with(&["central.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "apply");
        assert_eq!(block["added"][0]["source"], "central_fallback");
        assert_eq!(block["added"][0]["requested"], "central.png");
        let layer = &outcome.doc["items"]["2"]["kind"]["layers"][0];
        assert_eq!(layer["name"], "central.png");
        let item_id = layer["source"]["item"].as_i64().unwrap().to_string();
        let item = outcome.doc["items"].get(&item_id).unwrap();
        assert_eq!(
            item["kind"]["path"],
            "../../assets/mission_overlays/central.png"
        );
    }

    #[test]
    fn local_docs_wins_over_central_fallback() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        let bank = dir.path().join("assets/mission_overlays");
        std::fs::create_dir_all(&bank).unwrap();
        write_test_png(&docs.join("both.png"));
        write_test_png(&bank.join("both.png"));
        let missions = vec![mission_with(&["both.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["added"][0]["source"], "local_docs");
        let layer = &outcome.doc["items"]["2"]["kind"]["layers"][0];
        let item_id = layer["source"]["item"].as_i64().unwrap().to_string();
        let item = outcome.doc["items"].get(&item_id).unwrap();
        assert_eq!(item["kind"]["path"], "../docs/both.png");
    }

    #[test]
    fn central_fallback_rejects_invalid_profile() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        let bank = dir.path().join("assets/mission_overlays");
        std::fs::create_dir_all(&bank).unwrap();
        write_test_png_at(&bank.join("petit.png"), 8, 8);
        let missions = vec![mission_with(&["petit.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "review");
        assert_eq!(block["added"], json!([]));
        assert_eq!(block["reasons"], json!(["missing_asset"]));
        assert!(outcome
            .decisions
            .iter()
            .any(|decision| decision.details["reason"] == "asset_dimensions"));
    }

    #[test]
    fn corrupt_local_asset_falls_back_to_central_bank() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        // Cas réel du lot 183 : 15 octets ASCII sans signature PNG.
        std::fs::write(docs.join("LFAUIT.png"), b"logo de la base").unwrap();
        let bank = dir.path().join("assets/mission_overlays");
        std::fs::create_dir_all(&bank).unwrap();
        write_test_png(&bank.join("LFAUIT.png"));
        let missions = vec![mission_with(&["LFAUIT.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "apply");
        assert_eq!(block["reasons"], json!([]));
        assert_eq!(block["missing"], json!([]));
        assert_eq!(block["added"][0]["name"], "LFAUIT.png");
        assert_eq!(block["added"][0]["requested"], "LFAUIT.png");
        assert_eq!(block["added"][0]["source"], "central_fallback");
        assert!(block["warnings"].as_array().unwrap().iter().any(|warning| {
            warning["code"] == "local_asset_corrupted_fallback_used"
                && warning["detail"]
                    .as_str()
                    .is_some_and(|detail| detail.contains("LFAUIT.png"))
        }));
        let layer = &outcome.doc["items"]["2"]["kind"]["layers"][0];
        assert_eq!(layer["name"], "LFAUIT.png");
        let item_id = layer["source"]["item"].as_i64().unwrap().to_string();
        let item = outcome.doc["items"].get(&item_id).unwrap();
        assert_eq!(
            item["kind"]["path"],
            "../../assets/mission_overlays/LFAUIT.png"
        );
        assert!(outcome.changed);
    }

    #[test]
    fn invalid_local_profile_falls_back_to_central_bank() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        write_test_png_at(&docs.join("petit.png"), 8, 8);
        let bank = dir.path().join("assets/mission_overlays");
        std::fs::create_dir_all(&bank).unwrap();
        write_test_png(&bank.join("petit.png"));
        let missions = vec![mission_with(&["petit.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "apply");
        assert_eq!(block["added"][0]["source"], "central_fallback");
        assert!(block["warnings"].as_array().unwrap().iter().any(|warning| {
            warning["code"] == "local_asset_corrupted_fallback_used"
                && warning["detail"]
                    .as_str()
                    .is_some_and(|detail| detail.contains("8×8"))
        }));
    }

    #[test]
    fn corrupt_local_asset_without_fallback_degrades_to_review() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("LFAUIT.png"), b"logo de la base").unwrap();
        let bank = dir.path().join("assets/mission_overlays");
        std::fs::create_dir_all(&bank).unwrap();
        let missions = vec![mission_with(&["LFAUIT.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "review");
        assert_eq!(block["reasons"], json!(["missing_asset"]));
        assert_eq!(block["added"], json!([]));
        assert_eq!(block["missing"], json!(["LFAUIT.png"]));
        assert!(block["warnings"].as_array().unwrap().is_empty());
        assert!(outcome
            .decisions
            .iter()
            .any(|decision| decision.details["reason"] == "asset_unreadable"));
        assert!(!outcome.changed);

        // Banque désactivée : même dégradation gracieuse, sans warning.
        let disabled = A7Options {
            mission_overlays: None,
            ..options.clone()
        };
        let disabled_outcome = apply(&doc, &disabled);
        let disabled_block = disabled_outcome.mission.as_ref().unwrap();
        assert_eq!(disabled_block["decision"], "review");
        assert_eq!(disabled_block["reasons"], json!(["missing_asset"]));
        assert_eq!(disabled_block["added"], json!([]));
        assert_eq!(disabled_block["missing"], json!(["LFAUIT.png"]));
        assert!(disabled_block["warnings"].as_array().unwrap().is_empty());
        assert!(!disabled_outcome.changed);
    }

    #[test]
    fn corrupt_fallback_keeps_local_error_and_reviews() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("LFAUIT.png"), b"logo de la base").unwrap();
        let bank = dir.path().join("assets/mission_overlays");
        std::fs::create_dir_all(&bank).unwrap();
        std::fs::write(bank.join("LFAUIT.png"), b"logo central corrompu").unwrap();
        let missions = vec![mission_with(&["LFAUIT.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "review");
        assert_eq!(block["reasons"], json!(["missing_asset"]));
        assert_eq!(block["added"], json!([]));
        assert_eq!(block["missing"], json!(["LFAUIT.png"]));
        assert!(block["warnings"].as_array().unwrap().is_empty());
        assert!(outcome.decisions.iter().any(|decision| {
            decision.details["reason"] == "asset_unreadable"
                && decision.reason.contains("repli central")
        }));
        assert!(!outcome.changed);
    }

    #[test]
    fn missing_central_bank_degrades_to_review() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("lot/projets/clip_assist.ecproj");
        let docs = dir.path().join("lot/docs");
        std::fs::create_dir_all(&docs).unwrap();
        let bank = dir.path().join("banque-absente");
        let missions = vec![mission_with(&["absent.png"])];
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 10.0,
            meta: &meta,
            missions: &missions,
            overlay: true,
            mission_overlays: Some(&bank),
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let outcome = apply(&doc, &options);
        let block = outcome.mission.as_ref().unwrap();
        assert_eq!(block["decision"], "review");
        assert_eq!(block["reasons"], json!(["missing_asset"]));
        assert_eq!(block["added"], json!([]));
        assert_eq!(block["missing"], json!(["absent.png"]));
    }

    #[test]
    fn mov_policy_is_explicit_for_known_assets() {
        assert_eq!(mov_policy("m6+.mov"), Some(MovPolicy::EndCard));
        assert_eq!(
            mov_policy("MOTION_FIN_EM6_V03.mov"),
            Some(MovPolicy::TailOverlay)
        );
        assert_eq!(mov_policy("MSL1.mov"), Some(MovPolicy::FullDuration));
        assert!(matches!(
            mov_policy("m6+max anime 1ep.mov"),
            Some(MovPolicy::Review(_))
        ));
        assert_eq!(mov_policy("inconnu.mov"), None);
    }

    #[test]
    fn tail_overlay_keeps_composition_duration() {
        let dir = tempfile::tempdir().unwrap();
        let png = Asset {
            name: "a.png".into(),
            requested: "a.png".into(),
            path: dir.path().join("a.png"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Png,
            width: 1080,
            height: 1920,
            duration_s: None,
            has_audio: false,
            codec: "PNG".into(),
        };
        let mov = Asset {
            name: "MOTION_FIN_EM6_V03.mov".into(),
            requested: "MOTION_FIN_EM6_V03.mov".into(),
            path: dir.path().join("MOTION_FIN_EM6_V03.mov"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(12.0),
            has_audio: false,
            codec: "ProRes".into(),
        };
        let (plan, decisions) = plan_assets(&[png, mov], 60.0, 5.0);
        assert!(decisions.is_empty());
        assert_eq!(plan.tail_s, 0.0);
        assert_eq!(plan.additions[0].mode, PlacementMode::TailOverlay);
        assert_eq!(plan.additions[0].start_s, 48.0);
        assert_eq!(plan.additions[0].end_s, 60.0);
        assert_eq!(plan.additions[1].mode, PlacementMode::Static);
    }

    #[test]
    fn tail_overlay_rejects_short_video() {
        let dir = tempfile::tempdir().unwrap();
        let mov = Asset {
            name: "MOTION_FIN_EM6_V03.mov".into(),
            requested: "MOTION_FIN_EM6_V03.mov".into(),
            path: dir.path().join("MOTION_FIN_EM6_V03.mov"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(12.0),
            has_audio: false,
            codec: "ProRes".into(),
        };
        let (plan, decisions) = plan_assets(&[mov], 8.0, 5.0);
        assert!(plan.additions.is_empty());
        assert_eq!(plan.tail_s, 0.0);
        assert_eq!(
            decisions[0].details["reason"],
            "short_video_for_tail_overlay"
        );
    }

    #[test]
    fn tail_overlay_rejects_zero_duration() {
        let dir = tempfile::tempdir().unwrap();
        let mov = Asset {
            name: "MOTION_FIN_EM6_V03.mov".into(),
            requested: "MOTION_FIN_EM6_V03.mov".into(),
            path: dir.path().join("MOTION_FIN_EM6_V03.mov"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(0.0),
            has_audio: false,
            codec: "ProRes".into(),
        };
        let (plan, decisions) = plan_assets(&[mov], 60.0, 5.0);
        assert!(plan.additions.is_empty());
        assert_eq!(decisions[0].details["reason"], "invalid_mov_duration");
    }

    #[test]
    fn tail_overlay_insertion_does_not_extend_the_comp_and_mutes_audio() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        let project = dir.path().join("projets/clip_assist.ecproj");
        let mov = Asset {
            name: "MOTION_FIN_EM6_V03.mov".into(),
            requested: "MOTION_FIN_EM6_V03.mov".into(),
            path: docs.join("MOTION_FIN_EM6_V03.mov"),
            provenance: LogoProvenance::LocalDocs,
            kind: AssetKind::Mov,
            width: 1080,
            height: 1920,
            duration_s: Some(12.0),
            has_audio: true,
            codec: "ProRes".into(),
        };
        let (plan, decisions) = plan_assets(&[mov], 60.0, 5.0);
        assert!(decisions.is_empty());
        let options = A7Options {
            project: &project,
            docs: &docs,
            stem: "clip",
            lot: Some(1),
            main_s: 60.0,
            meta: &meta,
            missions: &[],
            overlay: true,
            mission_overlays: None,
            end_card_max_s: 5.0,
            ffprobe: Path::new("ffprobe"),
            global_fallback: false,
            cutoff_ts: None,
            window_start_ts: None,
        };
        let (output, added, _) = insert_assets(&doc, &plan.additions, &options).unwrap();
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].mode, "tail_overlay");
        assert_eq!(added[0].source, "local_docs");
        assert_eq!(added[0].requested, "MOTION_FIN_EM6_V03.mov");
        assert_eq!(added[0].start_s, 48.0);
        assert_eq!(added[0].end_s, 60.0);
        let comp = comp(&output).unwrap();
        assert_eq!(
            comp.get("duration").and_then(Value::as_i64),
            Some(seconds_to_ticks(60.0))
        );
        let layer = comp
            .get("layers")
            .and_then(Value::as_array)
            .and_then(|layers| layers.first())
            .unwrap();
        assert_eq!(layer["switches"]["audio"], json!(false));
        assert_eq!(layer["out_point"], json!(seconds_to_ticks(60.0)));
    }

    #[test]
    fn known_absent_is_reported_without_missing_asset_noise() {
        let doc = minimal_doc();
        let meta = comp_meta(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("projets/clip_assist.ecproj");
        let docs = dir.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        let mission = mission_with(&["lphdf.png"]);
        let missions = vec![mission];
        let options = options(&project, &docs, &meta, &missions);
        let outcome = apply(&doc, &options);
        let block = outcome.mission.unwrap();
        assert_eq!(block["decision"], json!("review"));
        assert_eq!(block["reasons"], json!(["known_absent"]));
        assert_eq!(block["known_absent"][0]["name"], json!("lphdf.png"));
        assert!(block["missing"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("lphdf.png")));
    }

    fn comp_meta(doc: &Value) -> Result<CompMeta> {
        ecproj::comp_meta(doc)
    }
}
