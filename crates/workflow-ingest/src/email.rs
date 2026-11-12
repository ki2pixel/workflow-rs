//! Missions client : client HTTP `email_proxy.php`, cache SQLite et import
//! d'emails hors ligne.
//!
//! Le pipeline ne dépend jamais d'un accès réseau : `EmailProxyClient` expose
//! des erreurs, et l'appelant (CLI) continue sans mission lorsque l'endpoint
//! est indisponible. Le cache est une base dédiée, distincte de
//! `download_history.sqlite3`.

use crate::error::{IngestError, IngestResult};
use chrono::{DateTime, Duration, Local, Utc};
use reqwest::Client;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration as StdDuration, Instant};
use workflow_core::config::EmailProxyConfig;
use workflow_core::mission::{
    decode_encoded_words, extract_delivery_links, html_to_text, ClientMission, MissionIndexEntry,
    MissionLink, VideoMatch,
};

const MAX_INDEX_LIMIT: usize = 200;

#[derive(Debug, Clone, Default, Serialize)]
pub struct EmailSyncReport {
    pub index_entries: usize,
    pub fetched: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum MissionResolution<'a> {
    Found(&'a ClientMission),
    Ambiguous(usize),
    NotFound,
}

impl PartialEq for MissionResolution<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Found(left), Self::Found(right)) => left.id == right.id,
            (Self::Ambiguous(left), Self::Ambiguous(right)) => left == right,
            (Self::NotFound, Self::NotFound) => true,
            _ => false,
        }
    }
}

impl Eq for MissionResolution<'_> {}

/// Résolution possédant la mission, utilisée par la CLI.
#[derive(Debug, Clone)]
pub enum MissionLookup {
    Found(Box<ClientMission>),
    Ambiguous(usize),
    NotFound,
}

/// Cache SQLite des missions.
#[derive(Clone)]
pub struct EmailMissionStore {
    connection: Arc<Mutex<Connection>>,
    path: Arc<PathBuf>,
}

impl EmailMissionStore {
    pub fn open(path: &Path) -> IngestResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path).map_err(|error| {
            IngestError::Message(format!("ouverture {}: {error}", path.display()))
        })?;
        connection
            .busy_timeout(StdDuration::from_secs(30))
            .map_err(|error| IngestError::Message(format!("busy_timeout SQLite: {error}")))?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA synchronous=NORMAL;
                 CREATE TABLE IF NOT EXISTS email_missions (
                     id                 TEXT PRIMARY KEY,
                     lot                INTEGER NOT NULL DEFAULT 0,
                     subject            TEXT NOT NULL DEFAULT '',
                     received_at        TEXT NOT NULL DEFAULT '',
                     stored_at          TEXT NOT NULL DEFAULT '',
                     deadline           TEXT NOT NULL DEFAULT '',
                     urgent             INTEGER NOT NULL DEFAULT 0,
                     sender_masked      TEXT NOT NULL DEFAULT '',
                     snippet            TEXT NOT NULL DEFAULT '',
                     body_text          TEXT NOT NULL DEFAULT '',
                     body_omitted       INTEGER NOT NULL DEFAULT 0,
                     delivery_links_json TEXT NOT NULL DEFAULT '[]',
                     fetched_at         TEXT NOT NULL DEFAULT ''
                 );
                 CREATE INDEX IF NOT EXISTS email_missions_lot
                     ON email_missions(lot, received_at DESC);
                 CREATE INDEX IF NOT EXISTS email_missions_received
                     ON email_missions(received_at DESC);",
            )
            .map_err(|error| {
                IngestError::Message(format!("initialisation {}: {error}", path.display()))
            })?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path: Arc::new(path.to_path_buf()),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn with_connection<T>(
        &self,
        action: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> IngestResult<T> {
        let connection = self.connection.lock().expect("base email verrouillée");
        action(&connection).map_err(|error| IngestError::Message(format!("SQLite email: {error}")))
    }

    /// Insère ou met à jour une mission. Le corps brut est conservé : une
    /// amélioration du parseur est appliquée à la lecture suivante.
    pub fn upsert(&self, mission: &ClientMission, fetched_at: &str) -> IngestResult<bool> {
        let links = serde_json::to_string(&mission.delivery_links)?;
        let stored_at = if mission.stored_at.trim().is_empty() {
            fetched_at
        } else {
            mission.stored_at.as_str()
        };
        self.with_connection(|connection| {
            let existing: Option<String> = connection
                .query_row(
                    "SELECT stored_at FROM email_missions WHERE id = ?1",
                    params![mission.id],
                    |row| row.get(0),
                )
                .optional()?;
            connection.execute(
                "INSERT INTO email_missions(
                     id, lot, subject, received_at, stored_at, deadline, urgent,
                     sender_masked, snippet, body_text, body_omitted,
                     delivery_links_json, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(id) DO UPDATE SET
                     lot = excluded.lot,
                     subject = excluded.subject,
                     received_at = excluded.received_at,
                     stored_at = excluded.stored_at,
                     deadline = excluded.deadline,
                     urgent = excluded.urgent,
                     sender_masked = excluded.sender_masked,
                     snippet = excluded.snippet,
                     body_text = excluded.body_text,
                     body_omitted = excluded.body_omitted,
                     delivery_links_json = excluded.delivery_links_json,
                     fetched_at = excluded.fetched_at",
                params![
                    mission.id,
                    mission.lot,
                    mission.subject,
                    mission.received_at,
                    stored_at,
                    mission.deadline,
                    mission.urgent,
                    mission.sender_masked,
                    mission.snippet,
                    mission.body_text,
                    mission.body_omitted,
                    links,
                    fetched_at,
                ],
            )?;
            Ok(existing.as_deref() != Some(stored_at))
        })
    }

    /// Indique si le détail doit être retéléchargé pour cette entrée d'index.
    pub fn needs_detail(&self, entry: &MissionIndexEntry) -> IngestResult<bool> {
        if entry.id.trim().is_empty() {
            return Ok(false);
        }
        let stored = self.with_connection(|connection| {
            connection
                .query_row(
                    "SELECT stored_at FROM email_missions WHERE id = ?1",
                    params![entry.id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
        })?;
        Ok(stored.as_deref() != Some(entry.stored_at.as_str()))
    }

    pub fn get(&self, id: &str) -> IngestResult<Option<ClientMission>> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, lot, subject, received_at, stored_at, deadline, urgent,
                        sender_masked, snippet, body_text, body_omitted,
                        delivery_links_json
                 FROM email_missions WHERE id = ?1",
            )?;
            let mut rows = statement.query(params![id])?;
            rows.next()?.map(row_to_mission).transpose()
        })
    }

    pub fn list(&self, limit: usize) -> IngestResult<Vec<ClientMission>> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, lot, subject, received_at, stored_at, deadline, urgent,
                        sender_masked, snippet, body_text, body_omitted,
                        delivery_links_json
                 FROM email_missions
                 ORDER BY received_at DESC, stored_at DESC
                 LIMIT ?1",
            )?;
            let rows = statement.query_map(params![limit.max(1) as i64], row_to_mission)?;
            let mut missions = Vec::new();
            for row in rows {
                missions.push(row?);
            }
            Ok(missions)
        })
    }

    pub fn list_for_lot(&self, lot: u32) -> IngestResult<Vec<ClientMission>> {
        self.with_connection(|connection| {
            let mut statement = connection.prepare(
                "SELECT id, lot, subject, received_at, stored_at, deadline, urgent,
                        sender_masked, snippet, body_text, body_omitted,
                        delivery_links_json
                 FROM email_missions
                 WHERE lot = ?1
                 ORDER BY received_at DESC, stored_at DESC",
            )?;
            let rows = statement.query_map(params![lot], row_to_mission)?;
            let mut missions = Vec::new();
            for row in rows {
                missions.push(row?);
            }
            Ok(missions)
        })
    }

    pub fn count(&self) -> IngestResult<u64> {
        self.with_connection(|connection| {
            let count: i64 =
                connection
                    .query_row("SELECT COUNT(*) FROM email_missions", [], |row| row.get(0))?;
            Ok(count.max(0) as u64)
        })
    }

    pub fn delete(&self, id: &str) -> IngestResult<()> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM email_missions WHERE id = ?1", params![id])?;
            Ok(())
        })
    }

    pub fn delete_all(&self) -> IngestResult<()> {
        self.with_connection(|connection| {
            connection.execute("DELETE FROM email_missions", [])?;
            Ok(())
        })
    }

    /// Purge les missions reçues avant `retention_days`. 0 désactive.
    pub fn purge_retention(&self, retention_days: u64) -> IngestResult<usize> {
        if retention_days == 0 {
            return Ok(0);
        }
        let cutoff = Utc::now() - Duration::days(retention_days as i64);
        let missions = self.list(10_000)?;
        let mut removed = 0;
        for mission in missions {
            let Some(received) = parse_timestamp(&mission.received_at) else {
                continue;
            };
            if received < cutoff {
                self.delete(&mission.id)?;
                removed += 1;
            }
        }
        Ok(removed)
    }
}

fn row_to_mission(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClientMission> {
    let links_json: String = row.get(11)?;
    let links: Vec<MissionLink> = serde_json::from_str(&links_json).unwrap_or_default();
    let mut mission = ClientMission {
        schema_version: 1,
        id: row.get(0)?,
        lot: row.get::<_, i64>(1)?.max(0) as u32,
        subject: row.get(2)?,
        received_at: row.get(3)?,
        stored_at: row.get(4)?,
        deadline: row.get(5)?,
        urgent: row.get::<_, i64>(6)? != 0,
        sender_masked: row.get(7)?,
        snippet: row.get(8)?,
        body_text: row.get(9)?,
        body_omitted: row.get::<_, i64>(10)? != 0,
        delivery_links: links,
        ..ClientMission::default()
    };
    mission.parse_body();
    Ok(mission)
}

fn parse_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            DateTime::parse_from_rfc2822(value)
                .map(|value| value.with_timezone(&Utc))
                .ok()
        })
}

/// Client du proxy d'emails.
pub struct EmailProxyClient {
    url: String,
    client: Client,
    index_cache: Mutex<Option<(Instant, Vec<MissionIndexEntry>)>>,
    cache_ttl: StdDuration,
}

impl EmailProxyClient {
    pub fn new(config: &EmailProxyConfig) -> IngestResult<Self> {
        let client = Client::builder()
            .connect_timeout(StdDuration::from_secs(config.timeout_s.max(1)))
            .timeout(StdDuration::from_secs(config.timeout_s.max(1)))
            .build()
            .map_err(|error| IngestError::Message(format!("client email: {error}")))?;
        Ok(Self {
            url: config.url.trim().trim_end_matches('/').to_string(),
            client,
            index_cache: Mutex::new(None),
            cache_ttl: StdDuration::from_secs(config.cache_ttl_s),
        })
    }

    pub fn endpoint(&self) -> &str {
        &self.url
    }

    /// Index léger, avec cache TTL en mémoire.
    pub async fn index(&self, limit: usize) -> IngestResult<Vec<MissionIndexEntry>> {
        let limit = limit.clamp(1, MAX_INDEX_LIMIT);
        {
            let cache = self.index_cache.lock().expect("cache email verrouillé");
            if let Some((fetched_at, entries)) = cache.as_ref() {
                if fetched_at.elapsed() < self.cache_ttl {
                    return Ok(entries.iter().take(limit).cloned().collect());
                }
            }
        }
        let response = self
            .client
            .get(&self.url)
            .query(&[("limit", limit.to_string())])
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(IngestError::Message(format!(
                "email_proxy index HTTP {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        let entries = parse_index_text(&text)?;
        let mut cache = self.index_cache.lock().expect("cache email verrouillé");
        *cache = Some((Instant::now(), entries.clone()));
        Ok(entries)
    }

    /// Détail d'une mission. Un objet `{"error": ...}` est une erreur métier.
    pub async fn detail(&self, id: &str) -> IngestResult<ClientMission> {
        if id.trim().is_empty() {
            return Err(IngestError::Message("id de mission vide".into()));
        }
        let response = self
            .client
            .get(&self.url)
            .query(&[("id", id)])
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(IngestError::Message(format!(
                "email_proxy détail HTTP {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        let value: Value = serde_json::from_str(&text)?;
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            return Err(IngestError::Message(format!("mission {id}: {error}")));
        }
        ClientMission::from_detail(&value)
            .map_err(|error| IngestError::Message(format!("mission {id}: {error}")))
    }

    /// Synchronise l'index et les détails absents ou modifiés.
    pub async fn sync(
        &self,
        store: &EmailMissionStore,
        limit: usize,
        force: bool,
    ) -> IngestResult<EmailSyncReport> {
        let entries = self.index(limit).await?;
        let mut report = EmailSyncReport {
            index_entries: entries.len(),
            ..EmailSyncReport::default()
        };
        let now = Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
        for entry in &entries {
            if entry.id.trim().is_empty() {
                report.skipped += 1;
                continue;
            }
            match store.needs_detail(entry) {
                Ok(false) if !force => {
                    report.skipped += 1;
                    continue;
                }
                Ok(_) => {}
                Err(error) => {
                    report.errors.push(format!("{}: {error}", entry.id));
                    continue;
                }
            }
            match self.detail(&entry.id).await {
                Ok(mut mission) => {
                    if mission.stored_at.is_empty() {
                        mission.stored_at = entry.stored_at.clone();
                    }
                    if mission.received_at.is_empty() {
                        mission.received_at = entry.received_at.clone();
                    }
                    if mission.deadline.is_empty() {
                        mission.deadline = entry.deadline.clone();
                    }
                    if mission.lot == 0 {
                        mission.lot = entry.lot;
                    }
                    match store.upsert(&mission, &now) {
                        Ok(updated) => {
                            report.fetched += 1;
                            if updated {
                                report.updated += 1;
                            }
                        }
                        Err(error) => report.errors.push(format!("{}: {error}", entry.id)),
                    }
                }
                Err(error) => report.errors.push(format!("{}: {error}", entry.id)),
            }
        }
        Ok(report)
    }

    /// Résout une mission unique pour un lot. Plusieurs missions pour le
    /// même lot restent ambiguës : aucun choix « le plus récent ».
    pub fn resolve_lot(&self, store: &EmailMissionStore, lot: u32) -> IngestResult<MissionLookup> {
        let missions = store.list_for_lot(lot)?;
        Ok(match missions.len() {
            0 => MissionLookup::NotFound,
            1 => MissionLookup::Found(Box::new(missions.into_iter().next().expect("une mission"))),
            count => MissionLookup::Ambiguous(count),
        })
    }
}

pub fn parse_index_text(text: &str) -> IngestResult<Vec<MissionIndexEntry>> {
    let value: Value = serde_json::from_str(text)?;
    parse_index_value(&value)
}

pub fn parse_index_value(value: &Value) -> IngestResult<Vec<MissionIndexEntry>> {
    let Some(items) = value.as_array() else {
        return Err(IngestError::Message(
            "email_proxy: tableau d'index attendu".into(),
        ));
    };
    let mut entries = Vec::new();
    for item in items {
        let Some(object) = item.as_object() else {
            continue;
        };
        let text = |name: &str| {
            object
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        entries.push(MissionIndexEntry {
            id: text("id"),
            lot: object.get("lot").and_then(Value::as_u64).unwrap_or(0) as u32,
            subject: text("subject"),
            received_at: text("received_at"),
            stored_at: text("stored_at"),
            deadline: text("deadline"),
            urgent: object
                .get("urgent")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sender: text("sender"),
            snippet: text("snippet"),
            file: text("file"),
            links_count: object
                .get("links_count")
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32,
            size_bytes: object
                .get("size_bytes")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        });
    }
    entries.sort_by(|left, right| right.received_at.cmp(&left.received_at));
    Ok(entries)
}

/// Résultat d'import d'un fichier `.eml`.
#[derive(Debug, Clone, Serialize)]
pub struct EmlImportReport {
    pub source: PathBuf,
    pub mission_id: String,
    pub lot: u32,
    pub videos: usize,
    pub warnings: usize,
}

/// Importe un email au format RFC 822 (fichier `.eml`) dans le cache.
pub fn import_eml(store: &EmailMissionStore, path: &Path) -> IngestResult<EmlImportReport> {
    let bytes = std::fs::read(path)?;
    let message = parse_eml(&bytes)?;
    let lot = workflow_core::mission::extract_lot(&message.subject).unwrap_or(0);
    if lot == 0 {
        return Err(IngestError::Message(format!(
            "{}: numéro de lot introuvable dans le sujet",
            path.display()
        )));
    }
    let id = mission_id(lot, &message.date);
    let mut mission = ClientMission::from_body(id.clone(), lot, message.subject, message.body_text);
    mission.received_at = message.date;
    mission.sender_masked = mask_sender(&message.from);
    mission.delivery_links = extract_delivery_links(&mission.body_text);
    let now = Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    store.upsert(&mission, &now)?;
    Ok(EmlImportReport {
        source: path.to_path_buf(),
        mission_id: id,
        lot,
        videos: mission.videos.len(),
        warnings: mission.warnings.len(),
    })
}

fn mission_id(lot: u32, date: &str) -> String {
    let stamp = DateTime::parse_from_rfc3339(date)
        .map(|value| value.format("%Y%m%d").to_string())
        .unwrap_or_else(|_| Local::now().format("%Y%m%d").to_string());
    format!("recadrage_lot_{lot}_{stamp}")
}

fn mask_sender(from: &str) -> String {
    let address = from
        .split('<')
        .nth(1)
        .and_then(|value| value.split('>').next())
        .unwrap_or(from)
        .trim();
    match address.split_once('@') {
        Some((local, domain)) => {
            let first = local.chars().next().unwrap_or('*');
            format!("{first}***@{domain}")
        }
        None => String::new(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct EmlMessage {
    pub subject: String,
    pub from: String,
    pub date: String,
    pub body_text: String,
}

/// Parseur MIME minimal : multipart/alternative, quoted-printable et base64.
pub fn parse_eml(bytes: &[u8]) -> IngestResult<EmlMessage> {
    let text = String::from_utf8_lossy(bytes).to_string();
    let headers = parse_headers(&text);
    let body = body_after_headers(&text);
    let content_type = headers
        .get("content-type")
        .cloned()
        .unwrap_or_else(|| "text/plain".into());
    let transfer = headers
        .get("content-transfer-encoding")
        .cloned()
        .unwrap_or_default();
    let date = headers
        .get("date")
        .and_then(|value| DateTime::parse_from_rfc2822(value).ok())
        .map(|value| {
            value
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        })
        .unwrap_or_else(|| Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false));
    Ok(EmlMessage {
        subject: decode_encoded_words(&headers.get("subject").cloned().unwrap_or_default()),
        from: headers.get("from").cloned().unwrap_or_default(),
        date,
        body_text: extract_text(&content_type, &transfer, body),
    })
}

fn parse_headers(text: &str) -> std::collections::HashMap<String, String> {
    let mut headers = std::collections::HashMap::new();
    let mut current = String::new();
    let mut value = String::new();
    for line in text.lines() {
        if line.is_empty() {
            break;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            value.push(' ');
            value.push_str(line.trim());
            continue;
        }
        if !current.is_empty() {
            headers.insert(current.clone(), value.trim().to_string());
        }
        let Some((name, rest)) = line.split_once(':') else {
            current.clear();
            value.clear();
            continue;
        };
        current = name.trim().to_lowercase();
        value = rest.trim().to_string();
    }
    if !current.is_empty() {
        headers.insert(current, value.trim().to_string());
    }
    headers
}

fn body_after_headers(text: &str) -> &str {
    text.find("\r\n\r\n")
        .map(|index| &text[index + 4..])
        .or_else(|| text.find("\n\n").map(|index| &text[index + 2..]))
        .unwrap_or("")
}

fn extract_text(content_type: &str, transfer: &str, body: &str) -> String {
    let lower = content_type.to_lowercase();
    if lower.starts_with("multipart/") {
        let Some(boundary) = content_type
            .split(';')
            .find_map(|part| part.trim().strip_prefix("boundary="))
            .map(|value| value.trim_matches('"'))
        else {
            return String::new();
        };
        let marker = format!("--{boundary}");
        let mut plain = None;
        let mut html = None;
        for part in body.split(&marker) {
            let part = part.trim_start_matches(['\r', '\n']);
            if part.is_empty() || part.starts_with("--") {
                continue;
            }
            let headers = parse_headers(part);
            let content_type = headers
                .get("content-type")
                .cloned()
                .unwrap_or_else(|| "text/plain".into());
            let transfer = headers
                .get("content-transfer-encoding")
                .cloned()
                .unwrap_or_default();
            let text = extract_text(&content_type, &transfer, body_after_headers(part));
            if text.trim().is_empty() {
                continue;
            }
            if content_type.to_lowercase().starts_with("text/plain") {
                plain = Some(text);
                break;
            }
            if content_type.to_lowercase().starts_with("text/html") && html.is_none() {
                html = Some(text);
            }
        }
        return plain
            .or_else(|| html.map(|html| html_to_text(&html)))
            .unwrap_or_default();
    }
    let decoded = match transfer.trim().to_lowercase().as_str() {
        "quoted-printable" => decode_quoted_printable(body),
        "base64" => decode_base64(body).unwrap_or_else(|| body.to_string()),
        _ => body.to_string(),
    };
    if lower.starts_with("text/html") {
        html_to_text(&decoded)
    } else {
        decoded
    }
}

fn decode_quoted_printable(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            if index + 2 < bytes.len() && bytes[index + 1] == b'\r' && bytes[index + 2] == b'\n' {
                index += 3;
                continue;
            }
            if index + 1 < bytes.len() && bytes[index + 1] == b'\n' {
                index += 2;
                continue;
            }
            if index + 2 < bytes.len() {
                let hex = &input[index + 1..index + 3];
                if let Ok(value) = u8::from_str_radix(hex, 16) {
                    output.push(value);
                    index += 3;
                    continue;
                }
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).to_string()
}

fn decode_base64(input: &str) -> Option<String> {
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

/// Associe une mission unique à un dossier, sans jamais choisir par récence.
pub fn mission_for_video<'a>(
    missions: &'a [ClientMission],
    lot: u32,
    video_name: &str,
) -> MissionResolution<'a> {
    let candidates: Vec<&ClientMission> = missions
        .iter()
        .filter(|mission| mission.lot == lot)
        .collect();
    match candidates.len() {
        0 => MissionResolution::NotFound,
        1 => match candidates[0].match_video(video_name) {
            VideoMatch::Matched(_) => MissionResolution::Found(candidates[0]),
            VideoMatch::Ambiguous => MissionResolution::Ambiguous(1),
            VideoMatch::NotFound => MissionResolution::NotFound,
        },
        count => MissionResolution::Ambiguous(count),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY_136: &str = "Voici les missions pour le lot 136.\n\nRecadrage 9:16 standard à faire pour 13h45 (aujourd'hui 10/11) :\n- M6+ - THE 100 – Extrait 7 – Lexa Promesse.mp4 (Logos requis: m6+.png, the 100.png, m6+.mov)\n\nMerci !";

    fn store() -> (tempfile::TempDir, EmailMissionStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = EmailMissionStore::open(&dir.path().join("email.sqlite3")).unwrap();
        (dir, store)
    }

    fn mission() -> ClientMission {
        ClientMission::from_body("recadrage_lot_136_20261110", 136, "Lot 136", BODY_136)
    }

    #[test]
    fn parses_index_and_ignores_unknown_fields() {
        let value = serde_json::json!([
            {
                "id": "recadrage_lot_136_20261110",
                "lot": 136,
                "subject": "Lot 136",
                "received_at": "2026-11-10T12:56:17+01:00",
                "stored_at": "2026-11-11T20:30:14+01:00",
                "urgent": false,
                "links_count": 1,
                "size_bytes": 1920,
                "unknown": true
            }
        ]);
        let entries = parse_index_value(&value).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].lot, 136);
        assert_eq!(entries[0].links_count, 1);
    }

    #[test]
    fn rejects_a_non_array_index() {
        assert!(parse_index_value(&serde_json::json!({"error": "x"})).is_err());
    }

    #[test]
    fn store_round_trip_reparses_body() {
        let (_dir, store) = store();
        let mission = mission();
        assert!(store.upsert(&mission, "2026-11-11T21:00:00+01:00").unwrap());
        let loaded = store.get(&mission.id).unwrap().unwrap();
        assert_eq!(loaded.lot, 136);
        assert_eq!(loaded.videos.len(), 1);
        assert_eq!(loaded.videos[0].logos.len(), 3);
        assert_eq!(loaded.delivery_links.len(), 0);
    }

    #[test]
    fn needs_detail_compares_stored_at() {
        let (_dir, store) = store();
        let mut mission = mission();
        let entry = MissionIndexEntry {
            id: mission.id.clone(),
            lot: 136,
            stored_at: "2026-11-11T20:30:14+01:00".into(),
            ..MissionIndexEntry::default()
        };
        assert!(store.needs_detail(&entry).unwrap());
        mission.stored_at = entry.stored_at.clone();
        store.upsert(&mission, "2026-11-11T21:00:00+01:00").unwrap();
        assert!(!store.needs_detail(&entry).unwrap());
        let changed = MissionIndexEntry {
            stored_at: "2026-11-12T08:00:00+01:00".into(),
            ..entry
        };
        assert!(store.needs_detail(&changed).unwrap());
    }

    #[test]
    fn list_for_lot_and_ambiguity() {
        let (_dir, store) = store();
        store
            .upsert(&mission(), "2026-11-11T21:00:00+01:00")
            .unwrap();
        assert_eq!(store.list_for_lot(136).unwrap().len(), 1);
        assert_eq!(store.list_for_lot(80).unwrap().len(), 0);
        let mut second = mission();
        second.id = "recadrage_lot_136_20261111".into();
        second.received_at = "2026-11-11T09:00:00+01:00".into();
        store.upsert(&second, "2026-11-11T21:00:00+01:00").unwrap();
        assert_eq!(store.list_for_lot(136).unwrap().len(), 2);
    }

    #[test]
    fn parses_multipart_eml_with_quoted_printable() {
        let eml = "Subject: Média Solution - Missions Recadrage - Lot 136\r\n\
From: Technique <technique@media-solution.fr>\r\n\
Date: Tue, 10 Nov 2026 03:56:19 -0800\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/alternative; boundary=\"b1\"\r\n\
\r\n\
--b1\r\n\
Content-Type: text/plain; charset=UTF-8\r\n\
Content-Transfer-Encoding: quoted-printable\r\n\
\r\n\
Recadrage 9:16 standard =C3=A0 faire pour 13h45 :\r\n\
- A.mp4 (Logos requis: a.png)\r\n\
--b1--\r\n";
        let message = parse_eml(eml.as_bytes()).unwrap();
        assert!(message.subject.contains("Lot 136"));
        assert!(message.body_text.contains("Recadrage 9:16"));
        assert!(message.body_text.contains("- A.mp4 (Logos requis: a.png)"));
    }

    #[test]
    fn mission_for_video_never_falls_back_to_another_lot() {
        let mission = mission();
        let missions = vec![mission];
        assert!(matches!(
            mission_for_video(
                &missions,
                136,
                "M6+ - THE 100 - Extrait 7 - Lexa Promesse.mp4"
            ),
            MissionResolution::Found(_)
        ));
        assert_eq!(
            mission_for_video(
                &missions,
                80,
                "M6+ - THE 100 - Extrait 7 - Lexa Promesse.mp4"
            ),
            MissionResolution::NotFound
        );
    }
}
