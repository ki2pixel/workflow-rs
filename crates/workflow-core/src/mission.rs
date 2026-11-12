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
use std::path::Path;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

/// Extensions vidéo reconnues dans une ligne d'instruction.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "avi", "mkv", "webm"];

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
    let mut videos: Vec<VideoInstruction> = Vec::new();
    let mut warnings = Vec::new();
    let mut section = Section::default();

    for (index, raw_line) in body
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .enumerate()
    {
        let line_number = index + 1;
        let line = normalize_text(raw_line.trim().trim_start_matches('\u{feff}'));
        if line.is_empty() {
            continue;
        }
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
    let stem_key = strip_video_extension(&file_key).to_string();
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
fn split_parenthetical(rest: &str) -> (Option<String>, Option<String>) {
    let rest = rest.trim();
    let Some(open) = rest.find('(') else {
        return (None, None);
    };
    let Some(close) = rest.rfind(')') else {
        return (None, Some(rest.to_string()));
    };
    if close < open {
        return (None, Some(rest.to_string()));
    }
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
    let list = &original[separator + 1..];
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
}
