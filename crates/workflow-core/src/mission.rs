//! Missions client extraites des emails de recadrage.
//!
//! Le parseur est volontairement tolérant : il ne doit jamais faire échouer le
//! pipeline. Toute ambiguïté est consignée dans `warnings`, et les décisions
//! d'insertion (A7) restent à la charge du post-traitement.
//!
//! Les corps d'email sont normalisés avant analyse : espaces insécables,
//! apostrophes et tirets typographiques sont ramenés à leurs équivalents
//! ASCII. Les accents sont conservés pour l'affichage mais retirés des clés de
//! comparaison de fichiers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Extensions vidéo reconnues dans une ligne d'instruction.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "avi", "mkv", "webm", "mxf", "wmv"];

/// Nombre maximal de lignes physiques fusionnées en une instruction logique.
const MAX_LOGICAL_LINES: usize = 4;
/// Longueur maximale d'une instruction logique fusionnée.
const MAX_LOGICAL_CHARS: usize = 600;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionLink {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub raw_url: String,
    #[serde(default)]
    pub direct_url: Option<String>,
    #[serde(default)]
    pub r2_url: Option<String>,
    #[serde(default)]
    pub original_filename: Option<String>,
}

/// Entrée légère de l'index `email_proxy.php`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionIndexEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub lot: u32,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub received_at: String,
    #[serde(default)]
    pub stored_at: String,
    #[serde(default)]
    pub deadline: String,
    #[serde(default)]
    pub urgent: bool,
    #[serde(default)]
    pub sender: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub links_count: u32,
    #[serde(default)]
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MissionWarning {
    pub code: String,
    pub line: usize,
    pub detail: String,
}

impl MissionWarning {
    fn new(code: &str, line: usize, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            line,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoInstruction {
    /// Nom tel qu'écrit dans l'email.
    pub filename: String,
    /// Nom normalisé (espaces insécables et caractères typographiques résolus).
    pub filename_normalized: String,
    /// Clé de comparaison insensible à la casse et aux accents, extension incluse.
    pub key: String,
    /// Clé de comparaison sans extension.
    pub stem_key: String,
    /// Extension en minuscules, sans point.
    pub extension: String,
    /// Échéance de la section qui porte l'instruction.
    pub deadline: String,
    /// Section urgente (`URGENT`, `IMMÉDIATEMENT`).
    pub urgent: bool,
    /// Libellé de la section d'origine, tronqué.
    pub section: String,
    /// Assets demandés, dans l'ordre de l'email.
    pub logos: Vec<String>,
    /// `Pas de logo` : aucun logo à ajouter.
    pub no_logo: bool,
    /// Logo demandé mais non fourni (`Logo demandé mais non trouvé/spécifié`).
    pub logo_unresolved: bool,
    /// Notes de continuation rattachées à l'instruction.
    pub notes: Vec<String>,
    /// Numéro de ligne (1-based) dans `body_text`.
    pub line: usize,
}

impl VideoInstruction {
    pub fn has_logos(&self) -> bool {
        !self.logos.is_empty()
    }

    pub fn is_ambiguous(&self) -> bool {
        self.logo_unresolved
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientMission {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub lot: u32,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub received_at: String,
    #[serde(default)]
    pub stored_at: String,
    #[serde(default)]
    pub deadline: String,
    #[serde(default)]
    pub urgent: bool,
    #[serde(default)]
    pub sender_masked: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub body_text: String,
    #[serde(default)]
    pub body_omitted: bool,
    #[serde(default)]
    pub delivery_links: Vec<MissionLink>,
    #[serde(default)]
    pub videos: Vec<VideoInstruction>,
    #[serde(default)]
    pub warnings: Vec<MissionWarning>,
}

#[derive(Debug, thiserror::Error)]
pub enum MissionError {
    #[error("mission JSON invalide: {0}")]
    Json(#[from] serde_json::Error),
    #[error("mission invalide: {0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoMatch<'a> {
    Matched(&'a VideoInstruction),
    Ambiguous,
    NotFound,
}

impl ClientMission {
    /// Construit le pivot depuis l'objet détail `email_proxy.php?id=...`.
    pub fn from_detail(value: &Value) -> Result<Self, MissionError> {
        let object = value
            .as_object()
            .ok_or_else(|| MissionError::Invalid("objet JSON attendu".into()))?;
        let text = |name: &str| {
            object
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let id = text("id");
        let subject = text("subject");
        let mut lot = object.get("lot").and_then(Value::as_u64).unwrap_or(0) as u32;
        if lot == 0 {
            lot = extract_lot(&format!("{subject} {id}")).unwrap_or(0);
        }
        let mut mission = Self {
            schema_version: object
                .get("schema_version")
                .and_then(Value::as_u64)
                .unwrap_or(1) as u32,
            id,
            lot,
            subject,
            received_at: text("received_at"),
            stored_at: text("stored_at"),
            deadline: normalize_text(&text("deadline")),
            urgent: object
                .get("urgent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sender_masked: text("sender_masked"),
            snippet: text("snippet"),
            body_text: text("body_text"),
            body_omitted: object
                .get("body_omitted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            delivery_links: Vec::new(),
            videos: Vec::new(),
            warnings: Vec::new(),
        };
        if let Some(links) = object.get("delivery_links").and_then(Value::as_array) {
            for link in links {
                let Some(link) = link.as_object() else {
                    continue;
                };
                mission.delivery_links.push(MissionLink {
                    provider: link
                        .get("provider")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    raw_url: link
                        .get("raw_url")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    direct_url: link
                        .get("direct_url")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    r2_url: link
                        .get("r2_url")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    original_filename: link
                        .get("original_filename")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                });
            }
        }
        if mission.id.is_empty() {
            return Err(MissionError::Invalid("id absent".into()));
        }
        mission.parse_body();
        Ok(mission)
    }

    pub fn from_detail_json(text: &str) -> Result<Self, MissionError> {
        let value: Value = serde_json::from_str(text)?;
        Self::from_detail(&value)
    }

    /// Construit une mission hors ligne (import `.eml` ou fixture).
    pub fn from_body(
        id: impl Into<String>,
        lot: u32,
        subject: impl Into<String>,
        body_text: impl Into<String>,
    ) -> Self {
        let mut mission = Self {
            schema_version: 1,
            id: id.into(),
            lot,
            subject: subject.into(),
            body_text: body_text.into(),
            ..Self::default()
        };
        mission.parse_body();
        if mission.deadline.is_empty() {
            mission.deadline = mission
                .videos
                .first()
                .map(|video| video.deadline.clone())
                .unwrap_or_default();
        }
        mission.urgent |= mission.videos.iter().any(|video| video.urgent);
        mission
    }

    /// Analyse `body_text`. Idempotent : les champs calculés sont remplacés.
    pub fn parse_body(&mut self) {
        let body = if self.body_text.trim().is_empty() {
            String::new()
        } else {
            self.body_text.clone()
        };
        let (videos, warnings) = parse_body(&body);
        self.videos = videos;
        self.warnings = warnings;
        if self.body_text.trim().is_empty() {
            self.warnings
                .push(MissionWarning::new("body_missing", 0, "body_text vide"));
        }
        if self.videos.is_empty() {
            self.warnings.push(MissionWarning::new(
                "no_video_instruction",
                0,
                "aucune ligne vidéo reconnue",
            ));
        }
    }

    /// Associe un nom de fichier ou un stem de projet à une instruction.
    pub fn match_video(&self, name: &str) -> VideoMatch<'_> {
        let candidate = filename_key(name);
        if candidate.is_empty() {
            return VideoMatch::NotFound;
        }
        let candidate_stem = strip_video_extension(&candidate);
        let mut matches: Vec<&VideoInstruction> = self
            .videos
            .iter()
            .filter(|video| {
                video.key == candidate
                    || video.stem_key == candidate
                    || video.stem_key == candidate_stem
            })
            .collect();
        matches.dedup_by(|left, right| left.key == right.key);
        match matches.len() {
            0 => VideoMatch::NotFound,
            1 => VideoMatch::Matched(matches[0]),
            _ => VideoMatch::Ambiguous,
        }
    }

    /// Numéro de lot déduit du nom de dossier (`136 Cvmille`, `Lot 136`).
    pub fn lot_from_folder(folder: &Path) -> Option<u32> {
        folder
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(extract_lot)
    }
}

/// Parse un corps d'email en instructions vidéo.
pub fn parse_body(body: &str) -> (Vec<VideoInstruction>, Vec<MissionWarning>) {
    let body = unescape_mission_text(body);
    let mut videos: Vec<VideoInstruction> = Vec::new();
    let mut warnings = Vec::new();
    let mut section = Section::default();

    for (line_number, line) in logical_lines(&body) {
        if is_section_header(&line) {
            section = Section::from_header(&line);
            continue;
        }
        let Some(rest) = strip_bullet(&line) else {
            if let Some(previous) = videos.last_mut() {
                if is_logo_note(&line) {
                    previous.logo_unresolved = true;
                    previous.notes.push(line.clone());
                    warnings.push(MissionWarning::new(
                        "logo_unresolved",
                        line_number,
                        format!("{}: {}", previous.filename, line),
                    ));
                }
            }
            continue;
        };
        let Some(parsed) = parse_instruction(rest, line_number, &section, &mut warnings) else {
            continue;
        };
        if videos.iter().any(|video| video.key == parsed.key) {
            warnings.push(MissionWarning::new(
                "duplicate_instruction",
                line_number,
                parsed.filename.clone(),
            ));
            continue;
        }
        if parsed.logo_unresolved {
            warnings.push(MissionWarning::new(
                "logo_unresolved",
                line_number,
                format!("{}: logos non fournis", parsed.filename),
            ));
        }
        if !parsed.no_logo && !parsed.logo_unresolved && parsed.logos.is_empty() {
            warnings.push(MissionWarning::new(
                "logos_missing",
                line_number,
                parsed.filename.clone(),
            ));
        }
        videos.push(parsed);
    }

    (videos, warnings)
}

/// Neutralise les échappements introduits par les exports mbox (`l\'ile`).
///
/// Les clients mail actuels n'échappent pas les apostrophes ; ce nettoyage
/// n'agit que sur les corpus historiques et reste sans effet sur un nom
/// légitime, aucun asset de production ne contenant de barre oblique inverse.
fn unescape_mission_text(input: &str) -> String {
    input.replace("\\'", "'")
}

/// Regroupe les lignes physiques d'une instruction coupée par le client mail.
///
/// Un email peut couper le nom de la vidéo (`…première \nrencontre.mp4`) ou
/// la liste des logos (`(Logos \nrequis: …)`). On ne fusionne que tant que
/// l'instruction est incomplète : extension vidéo absente ou parenthèses non
/// équilibrées. Les notes qui suivent une instruction complète restent des
/// lignes séparées pour le traitement `is_logo_note`.
fn logical_lines(body: &str) -> Vec<(usize, String)> {
    let mut output = Vec::new();
    let mut current: Option<(usize, String)> = None;
    let mut current_lines = 0usize;
    let body = body.replace("\r\n", "\n").replace('\r', "\n");

    for (index, raw_line) in body.split('\n').enumerate() {
        let line_number = index + 1;
        let line = normalize_text(raw_line.trim().trim_start_matches('\u{feff}'));
        if line.is_empty() {
            if let Some(item) = current.take() {
                output.push(item);
            }
            current_lines = 0;
            continue;
        }
        if is_section_header(&line) {
            if let Some(item) = current.take() {
                output.push(item);
            }
            output.push((line_number, line));
            current_lines = 0;
            continue;
        }
        if strip_bullet(&line).is_some() {
            if let Some(item) = current.take() {
                output.push(item);
            }
            current = Some((line_number, line));
            current_lines = 1;
            continue;
        }
        if let Some((_, text)) = current.as_mut() {
            let logo_continuation = filename_key(&line).starts_with("logo") && !text.contains('(');
            let paren_continuation = line.starts_with('(') && !text.contains('(');
            if current_lines < MAX_LOGICAL_LINES
                && text.chars().count() < MAX_LOGICAL_CHARS
                && (instruction_incomplete(text) || logo_continuation || paren_continuation)
            {
                text.push(' ');
                text.push_str(&line);
                current_lines += 1;
                continue;
            }
            if let Some(item) = current.take() {
                output.push(item);
            }
            current_lines = 0;
        }
        output.push((line_number, line));
    }
    if let Some(item) = current.take() {
        output.push(item);
    }
    output
}

/// Vrai tant que l'instruction n'a pas son nom de vidéo et ses parenthèses.
fn instruction_incomplete(line: &str) -> bool {
    split_video_filename(line).is_none() || !parentheses_balanced(line)
}

fn parentheses_balanced(input: &str) -> bool {
    let mut depth = 0i32;
    for character in input.chars() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

#[derive(Debug, Clone, Default)]
struct Section {
    deadline: String,
    urgent: bool,
    label: String,
}

impl Section {
    fn from_header(line: &str) -> Self {
        let normalized = filename_key(line);
        Self {
            deadline: extract_deadline(line).unwrap_or_default(),
            urgent: normalized.contains("urgent") || normalized.contains("immediatement"),
            label: line.chars().take(120).collect(),
        }
    }
}

fn parse_instruction(
    line: &str,
    line_number: usize,
    section: &Section,
    warnings: &mut Vec<MissionWarning>,
) -> Option<VideoInstruction> {
    let (filename, rest) = split_video_filename(line)?;
    let normalized = normalize_text(&filename);
    let file_key = filename_key(&normalized);
    let extension = Path::new(&normalized)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let stem_key = strip_video_extension(&file_key).trim().to_string();
    let mut instruction = VideoInstruction {
        filename: normalized,
        filename_normalized: normalize_text(&filename),
        key: file_key,
        stem_key,
        extension,
        deadline: section.deadline.clone(),
        urgent: section.urgent,
        section: section.label.clone(),
        logos: Vec::new(),
        no_logo: false,
        logo_unresolved: false,
        notes: Vec::new(),
        line: line_number,
    };

    let rest = rest.trim();
    if rest.is_empty() {
        instruction.logo_unresolved = true;
        warnings.push(MissionWarning::new(
            "logos_missing",
            line_number,
            format!("{}: aucune instruction de logo", instruction.filename),
        ));
        return Some(instruction);
    }

    let (parenthetical, trailing) = split_parenthetical(rest);
    if let Some(trailing) = trailing {
        instruction.notes.push(trailing);
    }
    let Some(parenthetical) = parenthetical else {
        // Forme sans parenthèses : `Logo chaîne : gulli.png` sur une ligne
        // de continuation, ou `Logo les marseillais`.
        let bare_key = filename_key(rest);
        if bare_key.contains("logo") {
            let bare_logos = parse_logos(rest);
            if !bare_logos.is_empty() {
                instruction.logos = bare_logos;
                warnings.push(MissionWarning::new(
                    "logo_form_inferred",
                    line_number,
                    format!("{}: {}", instruction.filename, rest),
                ));
                return Some(instruction);
            }
        }
        if let Some(logos) = parse_lenient_logos(rest) {
            if !logos.is_empty() {
                instruction.logos = logos;
                warnings.push(MissionWarning::new(
                    "logo_form_inferred",
                    line_number,
                    format!("{}: {}", instruction.filename, rest),
                ));
                return Some(instruction);
            }
        }
        instruction.notes.push(rest.to_string());
        instruction.logo_unresolved = true;
        warnings.push(MissionWarning::new(
            "unknown_instruction",
            line_number,
            format!("{}: {}", instruction.filename, rest),
        ));
        return Some(instruction);
    };

    let instruction_key = filename_key(&parenthetical);
    if instruction_key.contains("pas de logo") {
        instruction.no_logo = true;
        if instruction_key.contains("logos requis") || instruction_key.contains("logo demande") {
            warnings.push(MissionWarning::new(
                "logo_conflict",
                line_number,
                format!("{}: Pas de logo et logos demandés", instruction.filename),
            ));
        }
    } else if instruction_key.contains("logos requis") || instruction_key.contains("logo requis") {
        instruction.logos = parse_logos(&parenthetical);
        if instruction.logos.is_empty() {
            instruction.logo_unresolved = true;
        }
    } else if instruction_key.contains("logo")
        && (instruction_key.contains("non trouve")
            || instruction_key.contains("non specifie")
            || instruction_key.contains("absent")
            || instruction_key.contains("improviser"))
    {
        instruction.logo_unresolved = true;
    } else if let Some(logos) = parse_bare_asset_list(&parenthetical) {
        instruction.logos = logos;
        warnings.push(MissionWarning::new(
            "logo_form_inferred",
            line_number,
            format!("{}: {}", instruction.filename, parenthetical),
        ));
    } else if let Some(logos) = parse_lenient_logos(&parenthetical) {
        if logos.is_empty() {
            instruction.logo_unresolved = true;
        } else {
            instruction.logos = logos;
            warnings.push(MissionWarning::new(
                "logo_form_inferred",
                line_number,
                format!("{}: {}", instruction.filename, parenthetical),
            ));
        }
    } else {
        instruction.logo_unresolved = true;
        instruction.notes.push(parenthetical.clone());
        warnings.push(MissionWarning::new(
            "unknown_instruction",
            line_number,
            format!("{}: {}", instruction.filename, parenthetical),
        ));
    }
    Some(instruction)
}

/// Forme nue `(yaq.png)` ou `(a.png, b.mov)`, sans libellé.
///
/// On n'accepte que des tokens en forme d'asset : un titre entre parenthèses
/// (`(2)`) ou une note libre reste une note, jamais un logo deviné.
fn parse_bare_asset_list(parenthetical: &str) -> Option<Vec<String>> {
    let tokens = split_logo_tokens(parenthetical);
    if tokens.is_empty() {
        return None;
    }
    let all_assets = tokens.iter().all(|token| {
        let key = filename_key(token);
        !key.contains("logo")
            && !token.contains([':', '='])
            && Path::new(token)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    matches!(
                        extension.to_lowercase().as_str(),
                        "png" | "mov" | "jpg" | "jpeg" | "webp"
                    )
                })
    });
    all_assets.then_some(tokens)
}

/// Forme libre `Logo <nom>`, `Logo chaîne : <nom>`, sans « requis ».
///
/// Le token est ensuite résolu comme les autres : s'il ne correspond à aucun
/// asset, A7 part en revue. On ne devine jamais un asset à partir du programme.
fn parse_lenient_logos(parenthetical: &str) -> Option<Vec<String>> {
    let normalized = normalize_text(parenthetical);
    let key = filename_key(&normalized);
    if !key.starts_with("logo") || key.contains("pas de logo") {
        return None;
    }
    let lower = normalized.to_lowercase();
    let prefix_len = if lower.starts_with("logos") {
        5
    } else if lower.starts_with("logo") {
        4
    } else {
        return None;
    };
    let mut rest = normalized.get(prefix_len..)?.trim();
    for label in [
        "chaîne",
        "chaine",
        "émission",
        "emission",
        "programme",
        "demandé",
        "demande",
        "animé",
        "anime",
    ] {
        let lower = rest.to_lowercase();
        if lower.starts_with(label) {
            rest = rest.get(label.len()..).unwrap_or_default().trim();
            break;
        }
    }
    rest = rest.trim_start_matches([':', '=']).trim();
    if rest.is_empty() {
        return None;
    }
    Some(split_logo_tokens(rest))
}

/// Extrait le nom de fichier vidéo et le reste de la ligne.
fn split_video_filename(line: &str) -> Option<(String, String)> {
    let lower = line.to_lowercase();
    let mut best: Option<(usize, usize)> = None;
    for extension in VIDEO_EXTENSIONS {
        let needle = format!(".{extension}");
        let mut offset = 0;
        while let Some(position) = lower[offset..].find(&needle) {
            let start = offset + position;
            let end = start + needle.len();
            let boundary_ok = lower[end..]
                .chars()
                .next()
                .is_none_or(|next| next.is_whitespace() || next == '(');
            if boundary_ok && best.is_none_or(|(best_start, _)| start < best_start) {
                best = Some((start, end));
            }
            offset = end;
        }
    }
    let (_, end) = best?;
    let filename = line[..end].trim();
    if filename.is_empty() {
        return None;
    }
    Some((filename.to_string(), line[end..].trim().to_string()))
}

/// Sépare `(...)` du texte qui le suit.
///
/// La fermeture est trouvée par équilibrage : un titre peut contenir des
/// parenthèses (`…l'avion (2).png`) et une note peut suivre la liste
/// (`(Logos requis: …) (inclure signature…)`).
fn split_parenthetical(rest: &str) -> (Option<String>, Option<String>) {
    let rest = rest.trim();
    let Some(open) = rest.find('(') else {
        return (None, None);
    };
    let mut depth = 0usize;
    let mut close = None;
    for (offset, character) in rest[open..].char_indices() {
        match character {
            '(' => depth += 1,
            ')' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                if depth == 0 {
                    close = Some(open + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(close) = close else {
        return (None, Some(rest.to_string()));
    };
    let parenthetical = rest[open + 1..close].trim().to_string();
    let trailing = rest[close + 1..].trim();
    (
        Some(parenthetical),
        (!trailing.is_empty()).then(|| trailing.to_string()),
    )
}

/// Découpe une liste d'assets (`a.png, b.mov`) en conservant l'ordre.
pub fn parse_logos(parenthetical: &str) -> Vec<String> {
    let original = normalize_text(parenthetical);
    let Some(separator) = original
        .char_indices()
        .find(|(_, character)| *character == ':' || *character == '=')
        .map(|(index, _)| index)
    else {
        return Vec::new();
    };
    split_logo_tokens(&original[separator + 1..])
}

fn split_logo_tokens(list: &str) -> Vec<String> {
    let mut logos = Vec::new();
    let mut seen = Vec::new();
    for token in list.split([',', ';']) {
        for part in token.split(" et ") {
            let cleaned = part
                .trim()
                .trim_matches(|character: char| {
                    character.is_whitespace()
                        || matches!(character, '"' | '\'' | '«' | '»' | '.' | '(' | ')')
                })
                .to_string();
            if cleaned.is_empty() {
                continue;
            }
            let token_key = filename_key(&cleaned);
            if token_key.is_empty() || seen.contains(&token_key) {
                continue;
            }
            seen.push(token_key);
            logos.push(cleaned);
        }
    }
    logos
}

/// Résout un logo de mission sur disque, alias et extension implicite compris.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogoResolution {
    Found(PathBuf),
    KnownAbsent(&'static str),
    Ambiguous(&'static str),
    Missing,
}

/// Résolution d'acronyme vers un nom de fichier canonique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasResolution {
    Alias(&'static str),
    KnownAbsent(&'static str),
    Ambiguous(&'static str),
}

struct AliasEntry {
    /// Clé normalisée, sans extension (ex. `lrdlr`, `m6+max anime 1ep`).
    key: &'static str,
    /// Cibles canoniques. Vide = asset volontairement absent.
    targets: &'static [&'static str],
    /// Motif lisible pour l'audit.
    note: &'static str,
}

/// Table fermée des acronymes officiels et des variantes historiques.
///
/// Les entrées exactes sont documentaires : la résolution locale les trouve
/// avant l'alias. Les entrées sans cible sont des absences volontaires, pas des
/// oublis client.
const ALIASES: &[AliasEntry] = &[
    AliasEntry {
        key: "pp",
        targets: &["pp.png"],
        note: "",
    },
    AliasEntry {
        key: "laa9",
        targets: &["laa9.png"],
        note: "",
    },
    AliasEntry {
        key: "lacas",
        targets: &["LACAS.png"],
        note: "",
    },
    AliasEntry {
        key: "lrdlf",
        targets: &["lrdlf.png"],
        note: "",
    },
    AliasEntry {
        key: "lmcr",
        targets: &["lmcr.png"],
        note: "",
    },
    AliasEntry {
        key: "lmbf",
        targets: &["lmbf.png"],
        note: "",
    },
    AliasEntry {
        key: "lmf",
        targets: &["lmf.png"],
        note: "",
    },
    AliasEntry {
        key: "lmp",
        targets: &["lmp.png"],
        note: "",
    },
    AliasEntry {
        key: "adp",
        targets: &["adp.png"],
        note: "",
    },
    AliasEntry {
        key: "clf",
        targets: &["c'est la famille.png"],
        note: "",
    },
    AliasEntry {
        key: "idlt",
        targets: &["IDLT.png"],
        note: "",
    },
    AliasEntry {
        key: "idlt bilan",
        targets: &["IDLT BILAN.png"],
        note: "",
    },
    AliasEntry {
        key: "mapr",
        targets: &["mapr.png"],
        note: "",
    },
    AliasEntry {
        key: "mapr nouveau rdv",
        targets: &["mapr nouveau rdv avec l'amour.png"],
        note: "",
    },
    AliasEntry {
        key: "msl1",
        targets: &["msl1.png", "MSL1.mov"],
        note: "PNG ou MOV selon l'extension demandée",
    },
    AliasEntry {
        key: "qvema",
        targets: &["qvema.png"],
        note: "",
    },
    AliasEntry {
        key: "qvemf",
        targets: &["QVEMF.png"],
        note: "",
    },
    AliasEntry {
        key: "lrdlr",
        targets: &["Les reines de la route.png"],
        note: "alias historique, LRDLR.png prioritaire s'il existe",
    },
    AliasEntry {
        key: "lfauit",
        targets: &["LFAUIT.png"],
        note: "",
    },
    AliasEntry {
        key: "sdm",
        targets: &["sdm.png"],
        note: "",
    },
    AliasEntry {
        key: "yaq",
        targets: &["yaq.png"],
        note: "",
    },
    AliasEntry {
        key: "oclc",
        targets: &["OCLC.png"],
        note: "",
    },
    AliasEntry {
        key: "cec",
        targets: &["cauchemar en cuisine.png"],
        note: "alias historique (CEC)",
    },
    AliasEntry {
        key: "em6",
        targets: &["e=m6.png"],
        note: "alias historique, em6.png prioritaire s'il existe",
    },
    AliasEntry {
        key: "enquete d'action",
        targets: &["eqa.png"],
        note: "alias historique (Enquête d'action)",
    },
    AliasEntry {
        key: "enquetes criminelles",
        targets: &["eqc.png"],
        note: "alias historique (Enquêtes criminelles)",
    },
    AliasEntry {
        key: "si on se rencontrait",
        targets: &["et si on se rencontrait.png"],
        note: "titre court de l'émission",
    },
    AliasEntry {
        key: "m6+max",
        targets: &["m6+max.png"],
        note: "offre M6+ MAX",
    },
    AliasEntry {
        key: "m6+max anime",
        targets: &["m6+max anime 1ep.mov"],
        note: "bumper M6+ MAX Anime",
    },
    AliasEntry {
        key: "m6+max anime 1ep",
        targets: &["m6+max anime 1ep.mov"],
        note: "bumper M6+ MAX Anime",
    },
    AliasEntry {
        key: "lphdf",
        targets: &[],
        note: "asset retiré volontairement de la banque (La Petite Histoire de France)",
    },
    AliasEntry {
        key: "edc",
        targets: &[],
        note: "asset non fourni par le client (État de choc)",
    },
];

fn alias_key(name: &str) -> String {
    let key = filename_key(name);
    for extension in ["png", "mov", "jpg", "jpeg", "webp"] {
        let suffix = format!(".{extension}");
        if key.ends_with(&suffix) {
            return key[..key.len() - suffix.len()].to_string();
        }
    }
    key
}

/// Résout un acronyme ou une variante historique.
pub fn resolve_alias(name: &str) -> Option<AliasResolution> {
    let key = alias_key(name);
    let entry = ALIASES.iter().find(|entry| entry.key == key)?;
    if entry.targets.is_empty() {
        return Some(AliasResolution::KnownAbsent(entry.note));
    }
    if entry.targets.len() == 1 {
        return Some(AliasResolution::Alias(entry.targets[0]));
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_lowercase);
    if let Some(extension) = extension {
        let suffix = format!(".{extension}");
        if let Some(target) = entry
            .targets
            .iter()
            .find(|target| target.to_lowercase().ends_with(&suffix))
        {
            return Some(AliasResolution::Alias(target));
        }
    }
    Some(AliasResolution::Ambiguous(entry.note))
}

/// Provenance d'un asset de mission résolu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoProvenance {
    /// Fichier présent dans le dossier local du projet (`docs/`).
    LocalDocs,
    /// Repli depuis la banque centrale `paths.mission_overlays`.
    CentralFallback,
}

impl LogoProvenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalDocs => "local_docs",
            Self::CentralFallback => "central_fallback",
        }
    }
}

/// Résolution d'un asset avec sa provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogoTarget {
    Found {
        path: PathBuf,
        provenance: LogoProvenance,
    },
    KnownAbsent(&'static str),
    Ambiguous(&'static str),
    Missing,
}

impl LogoTarget {
    pub fn into_resolution(self) -> LogoResolution {
        match self {
            Self::Found { path, .. } => LogoResolution::Found(path),
            Self::KnownAbsent(note) => LogoResolution::KnownAbsent(note),
            Self::Ambiguous(note) => LogoResolution::Ambiguous(note),
            Self::Missing => LogoResolution::Missing,
        }
    }
}

/// Résolveur d'assets avec cache du dossier pour les audits de corpus.
///
/// Le repli central est optionnel : il n'est consulté qu'après l'échec des
/// recherches locales. La provenance est portée par `LogoTarget`.
pub struct LogoResolver {
    docs: PathBuf,
    entries: Vec<(String, PathBuf)>,
    fallback: Option<Box<LogoResolver>>,
}

impl LogoResolver {
    pub fn new(docs: impl Into<PathBuf>) -> Self {
        let docs = docs.into();
        let entries = std::fs::read_dir(&docs)
            .map(|entries| {
                entries
                    .flatten()
                    .filter_map(|entry| {
                        let path = entry.path();
                        if !path.is_file() {
                            return None;
                        }
                        let name = path.file_name()?.to_str()?.to_string();
                        Some((filename_key(&name), path))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            docs,
            entries,
            fallback: None,
        }
    }

    /// Ajoute une banque de repli, consultée après le dossier local.
    pub fn with_fallback(mut self, fallback: LogoResolver) -> Self {
        self.fallback = Some(Box::new(fallback));
        self
    }

    fn find(&self, name: &str) -> Option<PathBuf> {
        let direct = self.docs.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        let key = filename_key(name);
        if key.is_empty() {
            return None;
        }
        let mut matches = self
            .entries
            .iter()
            .filter(|(candidate, _)| *candidate == key)
            .map(|(_, path)| path.clone());
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    }

    fn find_fallback(&self, name: &str) -> Option<PathBuf> {
        self.fallback.as_ref()?.find(name)
    }

    /// Résout un token exclusivement dans la banque de repli.
    ///
    /// A7 l'utilise quand un asset local a été trouvé mais que sa sonde échoue
    /// (fichier tronqué, corrompu ou profil invalide) : la banque est alors
    /// réinterrogée avec la même sémantique que `resolve_target`
    /// (normalisation, extension implicite, alias) sans jamais retomber sur
    /// `docs/`. La provenance renvoyée est toujours `CentralFallback`.
    pub fn resolve_fallback_target(&self, name: &str) -> LogoTarget {
        let Some(fallback) = &self.fallback else {
            return LogoTarget::Missing;
        };
        match fallback.resolve_target(name) {
            LogoTarget::Found { path, .. } => LogoTarget::Found {
                path,
                provenance: LogoProvenance::CentralFallback,
            },
            LogoTarget::KnownAbsent(note) => LogoTarget::KnownAbsent(note),
            LogoTarget::Ambiguous(note) => LogoTarget::Ambiguous(note),
            LogoTarget::Missing => LogoTarget::Missing,
        }
    }

    fn extension_candidates(&self, name: &str) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        for extension in ["png", "mov"] {
            if let Some(path) = self.find(&format!("{name}.{extension}")) {
                candidates.push(path);
            }
        }
        candidates
    }

    /// Résout un token, puis tente des recollages de 2 ou 3 tokens.
    ///
    /// Un titre client peut contenir une virgule (`lorie, derrière la positive
    /// attitude.png`) ou une conjonction : le découpage initial produit alors
    /// plusieurs tokens. On ne recolle que si le nom reconstitué existe
    /// vraiment. Retourne la résolution, le token demandé et le nombre de
    /// tokens consommés.
    pub fn resolve_sequence_target(
        &self,
        logos: &[String],
        index: usize,
    ) -> (LogoTarget, String, usize) {
        let first = &logos[index];
        let first_resolution = self.resolve_target(first);
        if !matches!(first_resolution, LogoTarget::Missing) {
            return (first_resolution, first.clone(), 1);
        }
        for span in 2..=3 {
            if index + span > logos.len() {
                break;
            }
            for separator in [", ", " et "] {
                let joined = logos[index..index + span].join(separator);
                let resolution = self.resolve_target(&joined);
                if !matches!(resolution, LogoTarget::Missing) {
                    return (resolution, joined, span);
                }
            }
        }
        (first_resolution, first.clone(), 1)
    }

    /// Variante sans provenance, conservée pour les audits locaux.
    pub fn resolve_sequence(
        &self,
        logos: &[String],
        index: usize,
    ) -> (LogoResolution, String, usize) {
        let (target, requested, consumed) = self.resolve_sequence_target(logos, index);
        (target.into_resolution(), requested, consumed)
    }

    /// Résout un token : local exact/normalisé, banque exacte/normalisée,
    /// extension implicite, puis alias.
    pub fn resolve_target(&self, name: &str) -> LogoTarget {
        if let Some(path) = self.find(name) {
            return LogoTarget::Found {
                path,
                provenance: LogoProvenance::LocalDocs,
            };
        }
        if let Some(path) = self.find_fallback(name) {
            return LogoTarget::Found {
                path,
                provenance: LogoProvenance::CentralFallback,
            };
        }
        if Path::new(name).extension().is_none() {
            let local = self.extension_candidates(name);
            match local.len() {
                1 => {
                    return LogoTarget::Found {
                        path: local.into_iter().next().unwrap_or_default(),
                        provenance: LogoProvenance::LocalDocs,
                    };
                }
                0 => {}
                _ => return LogoTarget::Ambiguous("plusieurs formats pour ce nom"),
            }
            if let Some(fallback) = &self.fallback {
                let central = fallback.extension_candidates(name);
                match central.len() {
                    1 => {
                        return LogoTarget::Found {
                            path: central.into_iter().next().unwrap_or_default(),
                            provenance: LogoProvenance::CentralFallback,
                        };
                    }
                    0 => {}
                    _ => return LogoTarget::Ambiguous("plusieurs formats pour ce nom"),
                }
            }
        }
        match resolve_alias(name) {
            Some(AliasResolution::Alias(target)) => {
                if let Some(path) = self.find(target) {
                    return LogoTarget::Found {
                        path,
                        provenance: LogoProvenance::LocalDocs,
                    };
                }
                if let Some(path) = self.find_fallback(target) {
                    return LogoTarget::Found {
                        path,
                        provenance: LogoProvenance::CentralFallback,
                    };
                }
                LogoTarget::Missing
            }
            Some(AliasResolution::KnownAbsent(note)) => LogoTarget::KnownAbsent(note),
            Some(AliasResolution::Ambiguous(note)) => LogoTarget::Ambiguous(note),
            None => LogoTarget::Missing,
        }
    }

    /// Résout un token sans provenance (audits locaux).
    pub fn resolve(&self, name: &str) -> LogoResolution {
        self.resolve_target(name).into_resolution()
    }
}

/// Résout un logo avec un dossier ponctuel (wrapper sans cache).
pub fn resolve_logo_file(docs: &Path, name: &str) -> LogoResolution {
    LogoResolver::new(docs).resolve(name)
}

fn is_section_header(line: &str) -> bool {
    if line.starts_with('-') || line.starts_with('•') || line.starts_with('*') {
        return false;
    }
    if !line.ends_with(':') {
        return false;
    }
    let normalized = filename_key(line);
    normalized.contains("a faire pour")
        || normalized.contains("a rendre")
        || normalized.contains("urgent")
        || normalized.contains("immediatement")
        || normalized.contains("recadrage")
        || extract_deadline(line).is_some()
}

fn is_logo_note(line: &str) -> bool {
    let normalized = filename_key(line);
    normalized.starts_with("logo")
        && (normalized.contains("absent")
            || normalized.contains("non trouve")
            || normalized.contains("non specifie")
            || normalized.contains("improviser"))
}

fn strip_bullet(line: &str) -> Option<&str> {
    let mut characters = line.chars();
    let first = characters.next()?;
    if !matches!(first, '-' | '–' | '—' | '•' | '*') {
        return None;
    }
    if !characters.next().is_some_and(char::is_whitespace) {
        return None;
    }
    Some(characters.as_str().trim())
}

/// Extrait `13h45`, `11h00`, `18:00` d'une ligne.
pub fn extract_deadline(line: &str) -> Option<String> {
    let characters: Vec<char> = line.chars().collect();
    // Les ratios de cadrage (`9:16`) ne sont pas des échéances : on cherche
    // d'abord les formes explicites `13h45`, puis les formes `18:00`.
    for separators in [&['h', 'H'][..], &[':'][..]] {
        for index in 0..characters.len() {
            if !characters[index].is_ascii_digit() {
                continue;
            }
            let mut hour_end = index;
            while hour_end < characters.len() && characters[hour_end].is_ascii_digit() {
                hour_end += 1;
            }
            if hour_end - index > 2 {
                continue;
            }
            let mut separator = hour_end;
            while separator < characters.len() && characters[separator] == ' ' {
                separator += 1;
            }
            if separator >= characters.len() || !separators.contains(&characters[separator]) {
                continue;
            }
            let minute_start = separator + 1;
            if minute_start + 2 > characters.len()
                || !characters[minute_start].is_ascii_digit()
                || !characters[minute_start + 1].is_ascii_digit()
            {
                continue;
            }
            let hour: String = characters[index..hour_end].iter().collect();
            let minute: String = characters[minute_start..minute_start + 2].iter().collect();
            if let (Ok(hour), Ok(minute)) = (hour.parse::<u32>(), minute.parse::<u32>()) {
                if hour <= 23 && minute <= 59 {
                    return Some(format!("{hour:02}h{minute:02}"));
                }
            }
        }
    }
    None
}

/// Extrait le numéro de lot d'un sujet, d'un identifiant ou d'un dossier.
pub fn extract_lot(text: &str) -> Option<u32> {
    let normalized = filename_key(text);
    let bytes = normalized.as_bytes();
    let mut index = 0;
    while index + 3 <= bytes.len() {
        if &bytes[index..index + 3] == b"lot" {
            let mut cursor = index + 3;
            while cursor < bytes.len() && !bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            let start = cursor;
            while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
                cursor += 1;
            }
            if cursor > start {
                if let Ok(value) = normalized[start..cursor].parse::<u32>() {
                    return Some(value);
                }
            }
        }
        index += 1;
    }
    // Dossier `136 Cvmille` : lot en tête de nom.
    let leading: String = normalized
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    if !leading.is_empty() {
        if let Ok(value) = leading.parse::<u32>() {
            return Some(value);
        }
    }
    None
}

/// Normalise l'affichage : caractères typographiques et espaces.
pub fn normalize_text(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for character in input.chars() {
        match character {
            '\u{00a0}' | '\u{202f}' | '\u{2007}' | '\u{2009}' | '\u{200b}' => output.push(' '),
            '\u{2018}' | '\u{2019}' | '\u{201b}' | '\u{2032}' => output.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{2033}' => output.push('"'),
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => {
                output.push('-')
            }
            '\u{2026}' => output.push_str("..."),
            _ => output.push(character),
        }
    }
    let normalized: String = output.nfkc().collect();
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Clé de comparaison : normalisée, sans accent, en minuscules.
pub fn filename_key(input: &str) -> String {
    normalize_text(input)
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .collect::<String>()
        .replace(['«', '»'], "\"")
        // Les guillemets français portent des espaces que les guillemets
        // anglais n'ont pas : on les neutralise pour la comparaison.
        .replace("\" ", "\"")
        .replace(" \"", "\"")
        .to_lowercase()
        .trim()
        .to_string()
}

pub fn strip_video_extension(key: &str) -> &str {
    let lower = key.to_lowercase();
    for extension in VIDEO_EXTENSIONS {
        let suffix = format!(".{extension}");
        if lower.ends_with(&suffix) {
            return &key[..key.len() - suffix.len()];
        }
    }
    key
}

/// Extrait les liens de livraison présents dans un corps texte.
///
/// Utilisé pour les imports `.eml` hors ligne ; l'API fournit déjà
/// `delivery_links`, qui reste prioritaire.
pub fn extract_delivery_links(body: &str) -> Vec<MissionLink> {
    let mut links = Vec::new();
    let mut seen = Vec::new();
    for token in body.split_whitespace() {
        let trimmed = token.trim_matches(|character: char| {
            character.is_whitespace()
                || matches!(
                    character,
                    '<' | '>' | '(' | ')' | '[' | ']' | ',' | ';' | '"' | '\'' | '.'
                )
        });
        if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
            continue;
        }
        if seen.contains(&trimmed.to_string()) {
            continue;
        }
        seen.push(trimmed.to_string());
        let provider = if trimmed.contains("dropbox.com") {
            "dropbox"
        } else if trimmed.contains("r2.dev") || trimmed.contains("workers.dev") {
            "r2"
        } else {
            "link"
        };
        links.push(MissionLink {
            provider: provider.into(),
            raw_url: trimmed.to_string(),
            direct_url: Some(trimmed.to_string()),
            r2_url: None,
            original_filename: None,
        });
    }
    links
}

/// Décode les mots encodés RFC 2047 (`=?utf-8?q?...?=`).
pub fn decode_encoded_words(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("=?") {
        output.push_str(&rest[..start]);
        let candidate = &rest[start + 2..];
        let Some((charset, after_charset)) = candidate.split_once('?') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let Some((encoding, after_encoding)) = after_charset.split_once('?') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let Some(end) = after_encoding.find("?=") else {
            output.push_str(&rest[start..]);
            return output;
        };
        let payload = &after_encoding[..end];
        let decoded = match encoding.to_ascii_lowercase().as_str() {
            "b" => decode_base64_text(payload),
            "q" => Some(
                decode_quoted_printable_text(&payload.replace('_', " "))
                    .unwrap_or_else(|| payload.to_string()),
            ),
            _ => None,
        };
        if let Some(decoded) = decoded {
            if charset.eq_ignore_ascii_case("iso-8859-1") || charset.eq_ignore_ascii_case("latin1")
            {
                output.extend(decoded.bytes().map(char::from));
            } else {
                output.push_str(&decoded);
            }
            rest = &after_encoding[end + 2..];
        } else {
            output.push_str(&rest[start..start + 2]);
            rest = &rest[start + 2..];
        }
    }
    output.push_str(rest);
    output
}

fn decode_quoted_printable_text(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' && index + 2 < bytes.len() {
            let hex = &input[index + 1..index + 3];
            if let Ok(value) = u8::from_str_radix(hex, 16) {
                output.push(value);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(output).ok()
}

fn decode_base64_text(input: &str) -> Option<String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let Some(value) = TABLE.iter().position(|candidate| *candidate == byte) else {
            continue;
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    String::from_utf8(output).ok()
}

/// Convertit un fragment HTML minimal en texte lisible.
///
/// Utilisé uniquement lorsque l'API ne fournit pas `body_text` (import `.eml`
/// ancien). Les entités numériques et les sauts de bloc sont couverts.
pub fn html_to_text(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut index = 0;
    let lower = html.to_lowercase();
    while index < html.len() {
        let rest = &lower[index..];
        if rest.starts_with("<br") || rest.starts_with("</div") || rest.starts_with("</p") {
            output.push('\n');
            if let Some(end) = rest.find('>') {
                index += end + 1;
                continue;
            }
        }
        if rest.starts_with("<") {
            if let Some(end) = rest.find('>') {
                index += end + 1;
                continue;
            }
        }
        if rest.starts_with('&') {
            if let Some(end) = rest.find(';') {
                let entity = &rest[..=end];
                if let Some(character) = decode_entity(entity) {
                    output.push(character);
                    index += end + 1;
                    continue;
                }
            }
        }
        let character = html[index..].chars().next().unwrap_or_default();
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn decode_entity(entity: &str) -> Option<char> {
    match entity {
        "&amp;" => Some('&'),
        "&lt;" => Some('<'),
        "&gt;" => Some('>'),
        "&quot;" => Some('"'),
        "&#x27;" | "&#39;" | "&apos;" => Some('\''),
        "&nbsp;" => Some(' '),
        _ => {
            let number = entity.strip_prefix("&#")?.strip_suffix(';')?;
            let code = if let Some(hex) = number
                .strip_prefix('x')
                .or_else(|| number.strip_prefix('X'))
            {
                u32::from_str_radix(hex, 16).ok()?
            } else {
                number.parse::<u32>().ok()?
            };
            char::from_u32(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOT_136: &str = "Voici les missions pour le lot 136.\n\n\nRecadrage 9:16 standard à faire pour 13h45 (aujourd'hui 10/11) :\n- M6+ - THE 100 – Extrait 7 – Lexa Promesse.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)\n- M6+ - THE 100 – Extrait 8 – Bellamy croit que Clarke est morte.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)\n\nLien : https://www.dropbox.com/scl/fo/example\n\nMerci !";
    const LOT_131: &str = "Voici les missions pour le lot 131.\n\n\nRecadrage 9:16 standard à faire pour 11h50 (aujourd'hui 10/11) :\n- SDM - Leslie et Léo.mp4 (Logos requis: sdm.png)\n- Raymond.mp4 (Logos requis: sdm.png)\n\nLien : https://www.dropbox.com/scl/fo/example\n\nMerci !";
    const LOT_158: &str = "Voici les missions pour le lot 158.\n\n\nRecadrage 9:16 standard à faire pour 17h20 (aujourd'hui 10/11) :\n- CABINE TELEPHONE_TAT_Téva.mp4 (Pas de logo)\n- CAMERAS_TAT_Téva.mp4 (Pas de logo)\n- JEREMSTAR_TAT_Téva.mp4 (Pas de logo)\n\nLien : https://www.dropbox.com/scl/fo/example\n\nMerci !";
    const LOT_80: &str = "Voici les missions pour le lot 80.\n\n\nURGENT \nTraiter IMMÉDIATEMENT, à rendre au plus tard 11h00 (aujourd'hui 06/11) :\n- Leur belle-mère leur pourrit la vie !.mp4 (Pas de logo)\n\n\n\nRecadrage 9:16 standard à faire pour 13h25 (aujourd'hui 06/11) :\n- Gulli - MSL1 - L’interview « Quand j’étais petit » d’Igor Paixão.mp4 (Logos requis: MSL1.mov)\n\nLien : https://www.dropbox.com/scl/fo/example\n\nMerci !";
    const LOT_130: &str = "Voici les missions pour le lot 130.\n\n\nRecadrage 9:16 standard à faire pour 14h45 (aujourd'hui 12/10) :\n- M6 - AAL - Il abandonne les clients en plein service pour acheter des croissants et du pain !.mp4 (Logo demandé mais non trouvé/spécifié)\nlogo « AAL » absent de notre base : il vous sera transmis (ne pas improviser)\n\nLien : https://www.dropbox.com/scl/fo/example\n\nMerci !";

    #[test]
    fn parses_lot_136_two_videos_and_three_logos() {
        let mission = ClientMission::from_body("recadrage_lot_136", 136, "Lot 136", LOT_136);
        assert_eq!(mission.videos.len(), 2);
        let video = &mission.videos[0];
        assert_eq!(
            video.filename,
            "M6+ - THE 100 - Extrait 7 - Lexa Promesse.mp4"
        );
        assert_eq!(video.logos, vec!["m6+.png", "the 100.png", "m6+.mov"]);
        assert_eq!(video.deadline, "13h45");
        assert!(!video.urgent);
        assert!(!video.no_logo);
        assert!(!video.logo_unresolved);
        assert_eq!(video.extension, "mp4");
    }

    #[test]
    fn parses_lot_131_shared_logo() {
        let mission = ClientMission::from_body("recadrage_lot_131", 131, "Lot 131", LOT_131);
        assert_eq!(mission.videos.len(), 2);
        assert!(mission
            .videos
            .iter()
            .all(|video| video.logos == vec!["sdm.png".to_string()]));
        assert!(mission.warnings.is_empty());
    }

    #[test]
    fn parses_lot_158_no_logo() {
        let mission = ClientMission::from_body("recadrage_lot_158", 158, "Lot 158", LOT_158);
        assert_eq!(mission.videos.len(), 3);
        assert!(mission.videos.iter().all(|video| video.no_logo));
        assert!(mission.videos.iter().all(|video| video.logos.is_empty()));
        assert!(mission.warnings.is_empty());
    }

    #[test]
    fn parses_lot_80_urgent_and_standard_sections() {
        let mission = ClientMission::from_body("recadrage_lot_80", 80, "Lot 80", LOT_80);
        assert_eq!(mission.videos.len(), 2);
        let urgent = &mission.videos[0];
        assert_eq!(urgent.filename, "Leur belle-mère leur pourrit la vie !.mp4");
        assert!(urgent.no_logo);
        assert!(urgent.urgent);
        assert_eq!(urgent.deadline, "11h00");
        let standard = &mission.videos[1];
        assert_eq!(standard.logos, vec!["MSL1.mov"]);
        assert!(!standard.urgent);
        assert_eq!(standard.deadline, "13h25");
    }

    #[test]
    fn parses_lot_130_unresolved_logo_note() {
        let mission = ClientMission::from_body("recadrage_lot_130", 130, "Lot 130", LOT_130);
        assert_eq!(mission.videos.len(), 1);
        let video = &mission.videos[0];
        assert!(video.logo_unresolved);
        assert!(video.logos.is_empty());
        assert!(video
            .notes
            .iter()
            .any(|note| note.contains("absent de notre base")));
        assert!(mission
            .warnings
            .iter()
            .any(|warning| warning.code == "logo_unresolved"));
    }

    #[test]
    fn normalizes_non_breaking_spaces_and_typographic_quotes() {
        let body = "Recadrage standard à faire pour 9h05 :\n- L\u{2019}interview\u{00a0}–\u{00a0}Test.mp4 (Logos requis :\u{00a0}a.png)";
        let mission = ClientMission::from_body("x", 1, "Lot 1", body);
        assert_eq!(mission.videos.len(), 1);
        assert_eq!(mission.videos[0].filename, "L'interview - Test.mp4");
        assert_eq!(mission.videos[0].deadline, "09h05");
        assert_eq!(mission.videos[0].logos, vec!["a.png"]);
    }

    #[test]
    fn matches_filenames_ignoring_accents_and_typography() {
        let mission = ClientMission::from_body("x", 80, "Lot 80", LOT_80);
        assert!(matches!(
            mission.match_video(
                "Gulli - MSL1 - L’interview « Quand j’étais petit » d’Igor Paixão.mp4"
            ),
            VideoMatch::Matched(_)
        ));
        let candidate = "gulli - msl1 - l'interview \"quand j'etais petit\" d'igor paixao";
        let debug = mission
            .videos
            .iter()
            .map(|video| (video.key.clone(), video.stem_key.clone()))
            .collect::<Vec<_>>();
        assert!(
            matches!(mission.match_video(candidate), VideoMatch::Matched(_)),
            "candidate={:?} keys={debug:?}",
            filename_key(candidate)
        );
        assert_eq!(
            mission.match_video("Un autre fichier.mp4"),
            VideoMatch::NotFound
        );
    }

    #[test]
    fn matches_filename_with_space_before_extension() {
        let body = "Recadrage 9:16 standard à faire pour 16h10 :\n\
                    - Saurez-vous trouver les 3 trompe-l'œil .mp4 (Logos requis: m6+.png)";
        let mission = ClientMission::from_body("x", 20, "Lot 20", body);
        assert_eq!(mission.videos.len(), 1);
        assert_eq!(
            mission.videos[0].stem_key,
            "saurez-vous trouver les 3 trompe-l'œil"
        );
        assert!(matches!(
            mission.match_video("Saurez-vous trouver les 3 trompe-l'œil"),
            VideoMatch::Matched(_)
        ));
        assert!(matches!(
            mission.match_video("Saurez-vous trouver les 3 trompe-l'œil .mp4"),
            VideoMatch::Matched(_)
        ));
    }

    #[test]
    fn does_not_pick_a_mission_by_recency() {
        let mission = ClientMission::from_body("x", 136, "Lot 136", LOT_136);
        assert_eq!(
            mission.match_video("M6+ - THE 100 - Extrait 9.mp4"),
            VideoMatch::NotFound
        );
    }

    #[test]
    fn extracts_lot_from_folder_and_subject() {
        assert_eq!(
            ClientMission::lot_from_folder(Path::new("/mnt/cache/136 Cvmille")),
            Some(136)
        );
        assert_eq!(
            extract_lot("Média Solution - Missions Recadrage - Lot 80 - URGENT"),
            Some(80)
        );
        assert_eq!(extract_lot("recadrage_lot_158_20261110"), Some(158));
        assert_eq!(extract_lot("dossier sans numéro"), None);
    }

    #[test]
    fn html_fallback_decodes_entities() {
        let text =
            html_to_text("<div>Voici&nbsp;le lot 136.<br>- A&nbsp;– B.mp4 (Pas de logo)</div>");
        assert!(text.contains("Voici le lot 136."));
        assert!(normalize_text(&text).contains("- A - B.mp4 (Pas de logo)"));
    }

    #[test]
    fn detail_json_parses_videos_and_links() {
        let value = serde_json::json!({
            "schema_version": 1,
            "id": "recadrage_lot_136_20261110",
            "lot": 136,
            "subject": "Média Solution - Missions Recadrage - Lot 136",
            "received_at": "2026-11-10T12:56:17+01:00",
            "stored_at": "2026-11-11T20:30:14+01:00",
            "deadline": "13h45",
            "urgent": false,
            "body_text": LOT_136,
            "delivery_links": [{
                "provider": "dropbox",
                "raw_url": "https://www.dropbox.com/scl/fo/example",
                "direct_url": "https://www.dropbox.com/scl/fo/example",
                "r2_url": null,
                "original_filename": null
            }]
        });
        let mission = ClientMission::from_detail(&value).unwrap();
        assert_eq!(mission.lot, 136);
        assert_eq!(mission.videos.len(), 2);
        assert_eq!(mission.delivery_links.len(), 1);
        assert_eq!(mission.delivery_links[0].provider, "dropbox");
    }

    #[test]
    fn missing_body_is_a_warning_not_an_error() {
        let mission = ClientMission::from_detail(&serde_json::json!({
            "id": "recadrage_lot_1_20260101",
            "lot": 1,
            "subject": "Lot 1"
        }))
        .unwrap();
        assert!(mission.videos.is_empty());
        assert!(mission
            .warnings
            .iter()
            .any(|warning| warning.code == "body_missing"));
    }

    #[test]
    fn decodes_rfc2047_subjects_and_extracts_links() {
        assert_eq!(
            decode_encoded_words("=?utf-8?q?M=C3=A9dia?= Solution - Lot 130"),
            "Média Solution - Lot 130"
        );
        let links = extract_delivery_links(
            "Lien : https://www.dropbox.com/scl/fo/abc?rlkey=x&dl=0.\nMerci",
        );
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].provider, "dropbox");
        assert!(links[0].raw_url.ends_with("?rlkey=x&dl=0"));
    }

    #[test]
    fn parses_logo_lists_with_various_separators() {
        assert_eq!(
            parse_logos("Logos requis : a.png; b.mov et c.png"),
            vec!["a.png", "b.mov", "c.png"]
        );
        assert!(parse_logos("Logos requis: ").is_empty());
    }

    #[test]
    fn parses_crlf_bodies_without_losing_lines() {
        let body = "Recadrage à faire pour 10h00 :\r\n\
                    - premier.mp4 (Logos requis: a.png)\r\n\
                    - second.mp4 (Logos requis: b.png)\r\n";
        let mission = ClientMission::from_body("x", 1, "Lot 1", body);
        assert_eq!(mission.videos.len(), 2);
        assert_eq!(mission.videos[0].logos, vec!["a.png"]);
        assert_eq!(mission.videos[1].logos, vec!["b.png"]);
    }

    #[test]
    fn joins_wrapped_video_filename_and_logo_list() {
        let body = "Recadrage 9:16 standard à faire pour 10h00 :\n\
                    - Ça clashe entre Hillary et Jérémie lors de leur première \n\
                    rencontre.mp4 (Logos \n\
                    requis: les ch'tis.png)";
        let mission = ClientMission::from_body("x", 1, "Lot 1", body);
        assert_eq!(mission.videos.len(), 1);
        assert!(mission.videos[0].filename.ends_with("rencontre.mp4"));
        assert_eq!(mission.videos[0].logos, vec!["les ch'tis.png"]);
        assert!(!mission.videos[0].logo_unresolved);
    }

    #[test]
    fn keeps_title_parentheses_and_trailing_note() {
        let body = "Recadrage à faire pour 10h00 :\n\
                    - Maman j'ai encore raté l'avion (2).mp4 (Logos requis: m6+.png) \
                    (inclure signature animée en .mov de la chaine à la fin)\n\
                    - autre.mp4 (Logos requis: sdm.png)\n\
                    Merci !";
        let mission = ClientMission::from_body("x", 1, "Lot 1", body);
        assert_eq!(mission.videos.len(), 2);
        assert_eq!(
            mission.videos[0].filename,
            "Maman j'ai encore raté l'avion (2).mp4"
        );
        assert_eq!(mission.videos[0].logos, vec!["m6+.png"]);
        assert!(mission.videos[0]
            .notes
            .iter()
            .any(|note| note.contains("signature animée")));
        assert_eq!(mission.videos[1].logos, vec!["sdm.png"]);
        assert!(mission
            .videos
            .iter()
            .all(|video| !video.notes.iter().any(|note| note == "Merci !")));
    }

    #[test]
    fn unescapes_mbox_apostrophes() {
        let body = "Recadrage à faire pour 10h00 :\n\
                    - l\\'ile de la tentation.mp4 (Logos requis: c\\'est la famille.png)";
        let mission = ClientMission::from_body("x", 1, "Lot 1", body);
        assert_eq!(mission.videos.len(), 1);
        assert_eq!(mission.videos[0].filename, "l'ile de la tentation.mp4");
        assert_eq!(mission.videos[0].logos, vec!["c'est la famille.png"]);
    }

    #[test]
    fn parses_lenient_logo_form() {
        let body = "Recadrage à faire pour 10h00 :\n\
                    - premier.mp4 (Logo les marseillais)\n\
                    - second.mp4 (Logo chaîne : gulli.png)";
        let mission = ClientMission::from_body("x", 1, "Lot 1", body);
        assert_eq!(mission.videos.len(), 2);
        assert_eq!(mission.videos[0].logos, vec!["les marseillais"]);
        assert_eq!(mission.videos[1].logos, vec!["gulli.png"]);
        assert!(mission
            .warnings
            .iter()
            .any(|warning| warning.code == "logo_form_inferred"));
    }

    #[test]
    fn parses_bare_asset_parenthetical() {
        let body = "Recadrage à faire pour 10h00 :\n\
                    - EXTRAIT PREDIFF 1 – Il est étonné de voir sa fille derrière le rideau .mp4 (yaq.png)\n\
                    - PP - Laisser aller - Scène culte.mp4 (Logos requis: pp.png)";
        let mission = ClientMission::from_body("x", 151, "Lot 151", body);
        assert_eq!(mission.videos.len(), 2);
        assert_eq!(mission.videos[0].logos, vec!["yaq.png"]);
        assert_eq!(mission.videos[1].logos, vec!["pp.png"]);
        assert!(mission
            .warnings
            .iter()
            .any(|warning| warning.code == "logo_form_inferred"));
    }

    #[test]
    fn joins_logo_instruction_on_continuation_line() {
        let body = "Recadrage à faire pour 17h49 :\n\
                    - Aladdin - Il lui sauve la vie.mp4\n\
                        Logo chaîne : gulli.png\n\
                    - SOs fantome - Il s'est fait tout engluer.mp4\n\
                        Logo chaîne : teva.png";
        let mission = ClientMission::from_body("x", 91, "Lot 91", body);
        assert_eq!(mission.videos.len(), 2);
        assert_eq!(mission.videos[0].logos, vec!["gulli.png"]);
        assert_eq!(mission.videos[1].logos, vec!["teva.png"]);
        assert!(mission.videos.iter().all(|video| !video.logo_unresolved));
    }

    #[test]
    fn joins_parenthetical_on_continuation_line() {
        let body = "Recadrage à faire pour 11h59 :\n\
                    - Le chef goûte une crêpe fois gras chocolat et n'aime pas du tout.mp4 \n\
                    (Logos requis: cauchemar en cuisine.png)";
        let mission = ClientMission::from_body("x", 82, "Lot 82", body);
        assert_eq!(mission.videos.len(), 1);
        assert_eq!(mission.videos[0].logos, vec!["cauchemar en cuisine.png"]);
        assert!(!mission.videos[0].logo_unresolved);
    }

    #[test]
    fn alias_table_is_unique_and_covers_acronyms() {
        let mut keys = Vec::new();
        for entry in ALIASES {
            assert!(
                !keys.contains(&entry.key),
                "clé d'alias dupliquée: {}",
                entry.key
            );
            keys.push(entry.key);
        }
        assert_eq!(
            resolve_alias("IDLT.png"),
            Some(AliasResolution::Alias("IDLT.png"))
        );
        assert_eq!(
            resolve_alias("LRDLR.png"),
            Some(AliasResolution::Alias("Les reines de la route.png"))
        );
        assert_eq!(
            resolve_alias("MSL1.mov"),
            Some(AliasResolution::Alias("MSL1.mov"))
        );
        assert_eq!(
            resolve_alias("MSL1.png"),
            Some(AliasResolution::Alias("msl1.png"))
        );
        assert!(matches!(
            resolve_alias("MSL1"),
            Some(AliasResolution::Ambiguous(_))
        ));
        assert!(matches!(
            resolve_alias("lphdf.png"),
            Some(AliasResolution::KnownAbsent(_))
        ));
    }

    #[test]
    fn resolver_rejoins_asset_names_split_on_commas() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("lorie, derrière la positive attitude.png"),
            b"x",
        )
        .unwrap();
        let resolver = LogoResolver::new(dir.path());
        let logos = vec![
            "lorie".to_string(),
            "derrière la positive attitude.png".to_string(),
        ];
        let (resolution, requested, consumed) = resolver.resolve_sequence(&logos, 0);
        assert_eq!(consumed, 2);
        assert_eq!(requested, "lorie, derrière la positive attitude.png");
        assert!(matches!(resolution, LogoResolution::Found(_)));
    }

    #[test]
    fn fallback_resolver_keeps_local_priority_and_reports_provenance() {
        let local = tempfile::tempdir().unwrap();
        let bank = tempfile::tempdir().unwrap();
        for name in ["local.png", "both.png"] {
            std::fs::write(local.path().join(name), b"x").unwrap();
        }
        for name in ["both.png", "central.png", "Les reines de la route.png"] {
            std::fs::write(bank.path().join(name), b"x").unwrap();
        }
        let resolver =
            LogoResolver::new(local.path()).with_fallback(LogoResolver::new(bank.path()));

        assert_eq!(
            resolver.resolve_target("local.png"),
            LogoTarget::Found {
                path: local.path().join("local.png"),
                provenance: LogoProvenance::LocalDocs,
            }
        );
        assert_eq!(
            resolver.resolve_target("central.png"),
            LogoTarget::Found {
                path: bank.path().join("central.png"),
                provenance: LogoProvenance::CentralFallback,
            }
        );
        assert_eq!(
            resolver.resolve_target("CENTRAL"),
            LogoTarget::Found {
                path: bank.path().join("central.png"),
                provenance: LogoProvenance::CentralFallback,
            }
        );
        assert_eq!(
            resolver.resolve_target("both.png"),
            LogoTarget::Found {
                path: local.path().join("both.png"),
                provenance: LogoProvenance::LocalDocs,
            }
        );
        assert_eq!(
            resolver.resolve_target("lrdlr"),
            LogoTarget::Found {
                path: bank.path().join("Les reines de la route.png"),
                provenance: LogoProvenance::CentralFallback,
            }
        );
        assert!(matches!(
            resolver.resolve_target("lphdf.png"),
            LogoTarget::KnownAbsent(_)
        ));
        assert_eq!(resolver.resolve_target("inconnu.png"), LogoTarget::Missing);
    }

    #[test]
    fn fallback_target_bypasses_a_corrupt_local_asset() {
        let local = tempfile::tempdir().unwrap();
        let bank = tempfile::tempdir().unwrap();
        std::fs::write(local.path().join("LFAUIT.png"), b"logo de la base").unwrap();
        std::fs::write(bank.path().join("LFAUIT.png"), b"png sain").unwrap();
        let resolver =
            LogoResolver::new(local.path()).with_fallback(LogoResolver::new(bank.path()));
        assert_eq!(
            resolver.resolve_target("LFAUIT.png"),
            LogoTarget::Found {
                path: local.path().join("LFAUIT.png"),
                provenance: LogoProvenance::LocalDocs,
            }
        );
        assert_eq!(
            resolver.resolve_fallback_target("LFAUIT.png"),
            LogoTarget::Found {
                path: bank.path().join("LFAUIT.png"),
                provenance: LogoProvenance::CentralFallback,
            }
        );
        // Le repli conserve la sémantique des alias et de l'extension
        // implicite : `lfauit` retrouve l'asset canonique de la banque.
        assert_eq!(
            resolver.resolve_fallback_target("lfauit"),
            LogoTarget::Found {
                path: bank.path().join("LFAUIT.png"),
                provenance: LogoProvenance::CentralFallback,
            }
        );
        let isolated = LogoResolver::new(local.path());
        assert_eq!(
            isolated.resolve_fallback_target("LFAUIT.png"),
            LogoTarget::Missing
        );
    }

    #[test]
    fn logo_resolver_prefers_exact_then_alias_and_infers_extension() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "LRDLR.png",
            "Les reines de la route.png",
            "gulli.png",
            "msl1.png",
            "MSL1.mov",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        let resolver = LogoResolver::new(dir.path());
        assert_eq!(
            resolver.resolve("LRDLR.png"),
            LogoResolution::Found(dir.path().join("LRDLR.png"))
        );
        assert_eq!(
            resolver.resolve("gulli"),
            LogoResolution::Found(dir.path().join("gulli.png"))
        );
        assert!(matches!(
            resolver.resolve("MSL1"),
            LogoResolution::Ambiguous(_)
        ));
        assert!(matches!(
            resolver.resolve("lphdf.png"),
            LogoResolution::KnownAbsent(_)
        ));
        assert_eq!(resolver.resolve("inconnu.png"), LogoResolution::Missing);

        // Alias de repli quand le fichier exact n'existe pas.
        let alias_dir = tempfile::tempdir().unwrap();
        std::fs::write(alias_dir.path().join("Les reines de la route.png"), b"x").unwrap();
        let alias_resolver = LogoResolver::new(alias_dir.path());
        assert_eq!(
            alias_resolver.resolve("lrdlr"),
            LogoResolution::Found(alias_dir.path().join("Les reines de la route.png"))
        );
    }
}
