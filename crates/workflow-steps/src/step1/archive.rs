//! Extraction sécurisée des archives et dérivation des noms de projets STEP1.
//!
//! Reproduit `workflow_scripts/step1/extract_archives.py` : liste des archives
//! traitées, réinitialisation mensuelle, extraction ZIP / RAR / TAR avec
//! assainissement des chemins, puis déplacement du contenu dans `docs/`.

use super::sanitize::{validate_extraction_path, FilenameSanitizer, SanitizationStats};
use super::RunLogger;
use chrono::{DateTime, Local};
use regex::Regex;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;

const PROCESSED_ARCHIVES_FILE: &str = "processed_archives.txt";
const PROCESSED_ARCHIVES_RESET_MARKER: &str = "processed_archives.last_reset";
const ARCHIVE_EXTENSIONS: [&str; 6] = [".zip", ".zipx", ".rar", ".tgz", ".7z", ".tar"];

/// Budgets appliqués à chaque archive. Ils bornent les bombes de
/// décompression sans pénaliser les vidéos (ratio faible, taille réelle).
#[derive(Debug, Clone, Copy)]
pub struct ExtractionLimits {
    pub max_bytes: u64,
    pub max_compression_ratio: u64,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_bytes: 100 * 1024 * 1024 * 1024,
            max_compression_ratio: 200,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberWrite {
    Written,
    Skipped,
    QuotaExceeded,
}

struct ExtractionBudget {
    remaining: u64,
    max_ratio: u64,
}

impl ExtractionBudget {
    fn new(limits: ExtractionLimits) -> Self {
        Self {
            remaining: limits.max_bytes,
            max_ratio: limits.max_compression_ratio,
        }
    }

    fn check_declared(&self, declared: u64, compressed: u64) -> bool {
        if declared > self.remaining {
            return false;
        }
        if compressed > 0 && declared / compressed.max(1) > self.max_ratio {
            return false;
        }
        true
    }

    fn consume(&mut self, bytes: u64) -> bool {
        if bytes > self.remaining {
            return false;
        }
        self.remaining -= bytes;
        true
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive invalide: {0}")]
    Invalid(String),
    #[error("format non supporté: {0}")]
    Unsupported(String),
    #[error("outil externe indisponible: {0}")]
    Tool(String),
    #[error("I/O: {0}")]
    Io(#[from] io::Error),
}

#[derive(Debug, Clone)]
pub struct ExtractionOutcome {
    pub project_dir: PathBuf,
    pub docs_dir: PathBuf,
    pub security_issues: usize,
    pub stats: SanitizationStats,
}

pub fn format_timestamp(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d_%H-%M-%S").to_string()
}

/// Dérive le nom de dossier projet à partir du nom d'archive.
pub fn derive_project_folder_name(archive_filename: &str) -> String {
    let stem = Path::new(archive_filename)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(archive_filename);
    let parts: Vec<&str> = stem.split('_').collect();
    let first_is_digit = parts
        .first()
        .is_some_and(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    let second_is_alpha = parts.get(1).is_some_and(|part| {
        !part.is_empty() && part.chars().all(|character| character.is_alphabetic())
    });

    let candidate = if first_is_digit
        && parts
            .get(1)
            .is_some_and(|part| part.to_lowercase().contains("camille"))
    {
        format!("{} {}", parts[0], parts[1])
    } else if parts[0].to_lowercase().contains("camille") {
        if second_is_alpha && parts[1].to_lowercase() != "camille" {
            format!("{} {}", parts[0], parts[1])
        } else {
            parts[0].to_string()
        }
    } else if first_is_digit && second_is_alpha {
        format!("{} {}", parts[0], parts[1])
    } else {
        stem.replace('_', " ")
    };

    let clean_name = clean_folder_name(&candidate);
    if !clean_name.is_empty() {
        return clean_name;
    }

    let fallback = clean_fallback(stem);
    if fallback.is_empty() {
        "projet_sans_nom".into()
    } else {
        format!("projet_{fallback}")
    }
}

fn clean_folder_name(value: &str) -> String {
    static FORBIDDEN: OnceLock<Regex> = OnceLock::new();
    static SPACES: OnceLock<Regex> = OnceLock::new();
    let forbidden = FORBIDDEN.get_or_init(|| Regex::new(r#"[<>:"/\\|?*&]"#).expect("motif nom"));
    let spaces = SPACES.get_or_init(|| Regex::new(r"\s\s+").expect("motif espaces"));
    let replaced = forbidden.replace_all(value, "_");
    let collapsed = spaces.replace_all(&replaced, " ");
    collapsed
        .trim_matches(|character| character == '_' || character == '.' || character == ' ')
        .to_string()
}

fn clean_fallback(value: &str) -> String {
    static FORBIDDEN: OnceLock<Regex> = OnceLock::new();
    let forbidden =
        FORBIDDEN.get_or_init(|| Regex::new(r#"[<>:"/\\|?*&\s]"#).expect("motif fallback"));
    forbidden
        .replace_all(value, "_")
        .trim_matches('_')
        .to_string()
}

/// Calcule un dossier projet unique, avec suffixe horodaté en cas de collision.
pub fn compute_unique_project_dir(
    base_name: &str,
    destination_base_dir: &Path,
    now: DateTime<Local>,
) -> PathBuf {
    let candidate = destination_base_dir.join(base_name);
    if !candidate.exists() {
        return candidate;
    }
    let timestamp = format_timestamp(now);
    let candidate_ts = destination_base_dir.join(format!("{base_name} {timestamp}"));
    if !candidate_ts.exists() {
        return candidate_ts;
    }
    let mut counter = 2;
    loop {
        let with_counter = destination_base_dir.join(format!("{base_name} {timestamp}-{counter}"));
        if !with_counter.exists() {
            return with_counter;
        }
        counter += 1;
    }
}

pub fn get_processed_archives(logs_dir: &Path) -> HashSet<String> {
    let path = logs_dir.join(PROCESSED_ARCHIVES_FILE);
    let Ok(content) = fs::read_to_string(path) else {
        return HashSet::new();
    };
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn mark_archive_as_processed(logs_dir: &Path, archive: &Path) -> Result<(), ArchiveError> {
    fs::create_dir_all(logs_dir)?;
    let path = logs_dir.join(PROCESSED_ARCHIVES_FILE);
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", archive.display())?;
    Ok(())
}

/// Réinitialise mensuellement la liste des archives traitées.
pub fn reset_processed_archives_if_needed(
    logs_dir: &Path,
    now: DateTime<Local>,
    logger: &RunLogger,
) -> bool {
    if let Err(error) = fs::create_dir_all(logs_dir) {
        logger.line(format!(
            "Impossible de créer le dossier des journaux STEP1 {}: {error}",
            logs_dir.display()
        ));
        return false;
    }
    let processed_file = logs_dir.join(PROCESSED_ARCHIVES_FILE);
    let marker_file = logs_dir.join(PROCESSED_ARCHIVES_RESET_MARKER);
    let current_month = now.format("%Y-%m").to_string();

    let last_month = fs::read_to_string(&marker_file)
        .ok()
        .map(|value| value.trim().to_string());
    if last_month.as_deref() == Some(current_month.as_str()) {
        return false;
    }

    let has_content = fs::metadata(&processed_file)
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false);
    if has_content {
        let backup_name = format!(
            "processed_archives_{}_backup_{}.txt",
            last_month.as_deref().unwrap_or("previous"),
            now.format("%Y%m%d_%H%M%S")
        );
        let backup = logs_dir.join(backup_name);
        if let Err(error) = fs::copy(&processed_file, &backup) {
            logger.line(format!(
                "Échec de sauvegarde avant réinitialisation: {error}"
            ));
        } else {
            logger.line(format!(
                "Réinitialisation mensuelle: sauvegarde créée '{}'.",
                backup
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default()
            ));
        }
        match File::create(&processed_file) {
            Ok(_) => {
                logger.line("Réinitialisation mensuelle: fichier 'processed_archives.txt' vidé.")
            }
            Err(error) => logger.line(format!(
                "Impossible de vider le fichier processed_archives.txt: {error}"
            )),
        }
    } else {
        logger.line("Réinitialisation mensuelle: aucun contenu existant à sauvegarder.");
    }

    match fs::write(&marker_file, &current_month) {
        Ok(()) => logger.line(format!(
            "Marqueur de réinitialisation mis à jour pour le mois: {current_month}"
        )),
        Err(error) => logger.line(format!(
            "Impossible d'écrire le marqueur de réinitialisation: {error}"
        )),
    }
    true
}

/// Liste les archives non traitées contenant le mot-clé, sans récursion.
pub fn find_archives_to_process(source_dir: &Path, keyword: &str, logs_dir: &Path) -> Vec<PathBuf> {
    let processed = get_processed_archives(logs_dir);
    let Ok(entries) = fs::read_dir(source_dir) else {
        return Vec::new();
    };
    let keyword = keyword.to_lowercase();
    let mut archives: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .metadata()
                .map(|metadata| metadata.is_file())
                .unwrap_or(false)
        })
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let lower = name.to_lowercase();
            ARCHIVE_EXTENSIONS
                .iter()
                .any(|extension| lower.ends_with(extension))
        })
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&keyword)
        })
        .map(|entry| entry.path())
        .filter(|path| {
            let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            !processed.contains(&absolute.display().to_string())
        })
        .collect();
    archives.sort();
    archives
}

/// Extrait une archive et déplace son contenu dans `<projet>/docs`.
pub fn extract_archive(
    archive: &Path,
    destination_base_dir: &Path,
    docs_subfolder: &str,
    unrar: &Path,
    logger: &RunLogger,
    limits: ExtractionLimits,
) -> Result<ExtractionOutcome, ArchiveError> {
    let project_folder_name = derive_project_folder_name(
        archive
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
    );
    let unique_project_dir =
        compute_unique_project_dir(&project_folder_name, destination_base_dir, Local::now());
    let final_destination = unique_project_dir.join(docs_subfolder);
    let temp_extract_dir = unique_temp_dir(&project_folder_name, destination_base_dir);
    let _guard = TempDirGuard::new(temp_extract_dir.clone());
    fs::create_dir_all(&temp_extract_dir)?;

    let archive_name = archive
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| archive.display().to_string());
    logger.line(format!(
        "Extraction sécurisée de {archive_name} vers {}",
        final_destination.display()
    ));

    let mut sanitizer = FilenameSanitizer::new();
    let mut budget = ExtractionBudget::new(limits);
    let (security_issues, extraction_ok) = match archive_extension(archive).as_str() {
        ".zip" | ".zipx" => {
            logger.line(format!("Extraction ZIP sécurisée de {archive_name}"));
            extract_zip(
                archive,
                &temp_extract_dir,
                &mut sanitizer,
                logger,
                &mut budget,
            )?
        }
        ".rar" => {
            logger.line(format!("Extraction RAR sécurisée de {archive_name}"));
            extract_rar(
                archive,
                &temp_extract_dir,
                unrar,
                &mut sanitizer,
                logger,
                &mut budget,
            )?
        }
        ".tar" | ".gz" | ".bz2" | ".xz" | ".tgz" => {
            logger.line(format!("Extraction TAR sécurisée de {archive_name}"));
            extract_tar(
                archive,
                &temp_extract_dir,
                &mut sanitizer,
                logger,
                &mut budget,
            )?
        }
        other => {
            logger.line(format!("Format non supporté: {}", archive.display()));
            return Err(ArchiveError::Unsupported(other.to_string()));
        }
    };

    if !extraction_ok {
        return Err(ArchiveError::Invalid(format!(
            "échec de l'extraction sécurisée de {}",
            archive.display()
        )));
    }

    if !contains_files(&temp_extract_dir)? {
        return Err(ArchiveError::Invalid(format!(
            "aucun fichier extrait de {}",
            archive.display()
        )));
    }

    let macosx = temp_extract_dir.join("__MACOSX");
    if macosx.is_dir() {
        logger.line("Suppression du dossier __MACOSX.");
        fs::remove_dir_all(&macosx)?;
    }

    let mut items: Vec<PathBuf> = fs::read_dir(&temp_extract_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    items.sort();
    let source_root = if items.len() == 1 && items[0].is_dir() {
        logger.line(format!(
            "L'archive contient un seul dossier racine '{}'. Utilisation comme source.",
            items[0]
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default()
        ));
        items.remove(0)
    } else {
        temp_extract_dir.clone()
    };

    fs::create_dir_all(&final_destination)?;
    let mut children: Vec<PathBuf> = fs::read_dir(&source_root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    children.sort();
    for item in children {
        let target = final_destination.join(
            item.file_name()
                .ok_or_else(|| ArchiveError::Invalid("nom de fichier absent".into()))?,
        );
        if target.exists() {
            logger.line(format!(
                "'{}' existe déjà dans la destination. Il sera écrasé.",
                target.display()
            ));
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        move_path(&item, &target)?;
    }

    let stats = sanitizer.stats();
    logger.line(format!(
        "Statistiques de sécurité pour {archive_name}: {} fichiers traités, {} modifiés, {} problèmes de sécurité détectés",
        stats.total_processed,
        stats.modified_count,
        security_issues
    ));
    if security_issues > 0 {
        logger.line(format!(
            "ATTENTION: {security_issues} problèmes de sécurité détectés dans {archive_name}"
        ));
    }
    logger.line(format!("Extraction terminée pour {archive_name}"));

    Ok(ExtractionOutcome {
        project_dir: unique_project_dir,
        docs_dir: final_destination,
        security_issues,
        stats,
    })
}

fn archive_extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_default()
}

fn unique_temp_dir(project_name: &str, base: &Path) -> PathBuf {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let mut candidate = base.join(format!("_temp_{project_name}_{epoch}"));
    let mut counter = 2;
    while candidate.exists() {
        candidate = base.join(format!("_temp_{project_name}_{epoch}-{counter}"));
        counter += 1;
    }
    candidate
}

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn contains_files(root: &Path) -> Result<bool, ArchiveError> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| ArchiveError::Io(io::Error::other(error)))?;
        if entry.file_type().is_file() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn extract_zip(
    archive: &Path,
    destination: &Path,
    sanitizer: &mut FilenameSanitizer,
    logger: &RunLogger,
    budget: &mut ExtractionBudget,
) -> Result<(usize, bool), ArchiveError> {
    let file = File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|error| ArchiveError::Invalid(format!("{}: {error}", archive.display())))?;
    let mut security_issues = 0;
    for index in 0..zip.len() {
        let mut entry = match zip.by_index(index) {
            Ok(entry) => entry,
            Err(error) => {
                logger.line(format!(
                    "Échec de lecture d'un membre ZIP de {}: {error}",
                    archive.display()
                ));
                continue;
            }
        };
        let name = entry.name().to_string();
        if name.ends_with('/') || entry.is_dir() {
            continue;
        }
        if !budget.check_declared(entry.size(), entry.compressed_size()) {
            return Err(ArchiveError::Invalid(format!(
                "budget de décompression dépassé pour '{}' dans {}",
                name,
                archive.display()
            )));
        }
        let result = sanitizer.sanitize_archive_member_path(&name);
        security_issues += result.security_issues.len();
        if result.was_modified {
            logger.line(format!(
                "Sanitized ZIP member: '{name}' -> '{}'",
                result.sanitized_name
            ));
        }
        match write_member(
            destination,
            &result.sanitized_name,
            &mut entry,
            logger,
            &name,
            budget,
        ) {
            MemberWrite::Written => {}
            MemberWrite::Skipped => security_issues += 1,
            MemberWrite::QuotaExceeded => {
                return Err(ArchiveError::Invalid(format!(
                    "budget de décompression dépassé dans {}",
                    archive.display()
                )))
            }
        }
    }
    Ok((security_issues, true))
}

fn extract_tar(
    archive: &Path,
    destination: &Path,
    sanitizer: &mut FilenameSanitizer,
    logger: &RunLogger,
    budget: &mut ExtractionBudget,
) -> Result<(usize, bool), ArchiveError> {
    let reader = open_tar_reader(archive)?;
    let mut tar = tar::Archive::new(reader);
    let mut security_issues = 0;
    let entries = tar
        .entries()
        .map_err(|error| ArchiveError::Invalid(format!("{}: {error}", archive.display())))?;
    for entry in entries {
        let mut entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                logger.line(format!(
                    "Échec de lecture d'un membre TAR de {}: {error}",
                    archive.display()
                ));
                continue;
            }
        };
        if !entry.header().entry_type().is_file() {
            let name = entry
                .path()
                .map(|path| path.display().to_string())
                .unwrap_or_default();
            logger.line(format!("Skipping non-regular file in TAR: {name}"));
            security_issues += 1;
            continue;
        }
        let name = entry
            .path()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let declared = entry.header().size().unwrap_or(0);
        if !budget.check_declared(declared, 0) {
            return Err(ArchiveError::Invalid(format!(
                "budget de décompression dépassé pour '{}' dans {}",
                name,
                archive.display()
            )));
        }
        let result = sanitizer.sanitize_archive_member_path(&name);
        security_issues += result.security_issues.len();
        if result.was_modified {
            logger.line(format!(
                "Sanitized TAR member: '{name}' -> '{}'",
                result.sanitized_name
            ));
        }
        match write_member(
            destination,
            &result.sanitized_name,
            &mut entry,
            logger,
            &name,
            budget,
        ) {
            MemberWrite::Written => {}
            MemberWrite::Skipped => security_issues += 1,
            MemberWrite::QuotaExceeded => {
                return Err(ArchiveError::Invalid(format!(
                    "budget de décompression dépassé dans {}",
                    archive.display()
                )))
            }
        }
    }
    Ok((security_issues, true))
}

fn extract_rar(
    archive: &Path,
    destination: &Path,
    unrar: &Path,
    sanitizer: &mut FilenameSanitizer,
    logger: &RunLogger,
    budget: &mut ExtractionBudget,
) -> Result<(usize, bool), ArchiveError> {
    let listing = Command::new(unrar)
        .args(["lb", "-p-", "--"])
        .arg(archive)
        .output()
        .map_err(|error| ArchiveError::Tool(format!("unrar: {error}")))?;
    if !listing.status.success() {
        return Err(ArchiveError::Invalid(format!(
            "{}: {}",
            archive.display(),
            String::from_utf8_lossy(&listing.stderr).trim()
        )));
    }

    let mut security_issues = 0;
    for member in String::from_utf8_lossy(&listing.stdout).lines() {
        let member = member.trim_end_matches('\r');
        if member.is_empty() || member.ends_with('/') || member.ends_with('\\') {
            continue;
        }
        let result = sanitizer.sanitize_archive_member_path(member);
        security_issues += result.security_issues.len();
        if result.was_modified {
            logger.line(format!(
                "Sanitized RAR member: '{member}' -> '{}'",
                result.sanitized_name
            ));
        }
        let mut child = Command::new(unrar)
            .args(["p", "-inul", "-p-", "--"])
            .arg(archive)
            .arg(member)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| ArchiveError::Tool(format!("unrar: {error}")))?;
        let _guard = crate::process::ChildGuard::register(child.id());
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| ArchiveError::Tool("unrar: stdout indisponible".into()))?;
        let target = destination.join(&result.sanitized_name);
        let member_write = write_member(
            destination,
            &result.sanitized_name,
            &mut stdout,
            logger,
            member,
            budget,
        );
        // Fermer le tube permet à unrar de terminer même si l'écriture a été
        // interrompue (chemin non sûr, disque plein, quota).
        drop(stdout);
        let output = child
            .wait_with_output()
            .map_err(|error| ArchiveError::Tool(format!("unrar: {error}")))?;
        if member_write == MemberWrite::QuotaExceeded {
            return Err(ArchiveError::Invalid(format!(
                "budget de décompression dépassé dans {}",
                archive.display()
            )));
        }
        if !output.status.success() {
            if member_write == MemberWrite::Written {
                let _ = fs::remove_file(&target);
            }
            logger.line(format!(
                "Échec de lecture du membre RAR '{member}' de {}: {}",
                archive.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
            continue;
        }
        if member_write == MemberWrite::Skipped {
            security_issues += 1;
        }
    }
    Ok((security_issues, true))
}

fn write_member(
    destination: &Path,
    sanitized_name: &str,
    reader: &mut impl Read,
    logger: &RunLogger,
    original_name: &str,
    budget: &mut ExtractionBudget,
) -> MemberWrite {
    if !validate_extraction_path(sanitized_name, destination) {
        logger.line(format!(
            "Unsafe extraction path detected, skipping: {original_name}"
        ));
        return MemberWrite::Skipped;
    }
    let target = destination.join(sanitized_name);
    if let Some(parent) = target.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            logger.line(format!("Impossible de créer {}: {error}", parent.display()));
            return MemberWrite::Skipped;
        }
    }
    let mut file = match File::create(&target) {
        Ok(file) => file,
        Err(error) => {
            logger.line(format!(
                "Échec d'extraction du membre '{original_name}': {error}"
            ));
            return MemberWrite::Skipped;
        }
    };
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) => {
                logger.line(format!(
                    "Échec d'extraction du membre '{original_name}': {error}"
                ));
                let _ = fs::remove_file(&target);
                return MemberWrite::Skipped;
            }
        };
        if !budget.consume(read as u64) {
            logger.line(format!(
                "Budget de décompression dépassé pour le membre '{original_name}'"
            ));
            drop(file);
            let _ = fs::remove_file(&target);
            return MemberWrite::QuotaExceeded;
        }
        if let Err(error) = file.write_all(&buffer[..read]) {
            logger.line(format!(
                "Échec d'extraction du membre '{original_name}': {error}"
            ));
            drop(file);
            let _ = fs::remove_file(&target);
            return MemberWrite::Skipped;
        }
    }
    MemberWrite::Written
}

fn open_tar_reader(archive: &Path) -> Result<Box<dyn Read>, ArchiveError> {
    let mut magic = [0_u8; 6];
    let mut probe = File::open(archive)?;
    let read = probe.read(&mut magic)?;
    drop(probe);
    let file = File::open(archive)?;
    let reader: Box<dyn Read> = if read >= 2 && magic[0] == 0x1f && magic[1] == 0x8b {
        Box::new(flate2::read::MultiGzDecoder::new(BufReader::new(file)))
    } else if read >= 3 && &magic[..3] == b"BZh" {
        Box::new(bzip2::read::MultiBzDecoder::new(BufReader::new(file)))
    } else if read >= 6 && magic == [0xfd, 0x37, 0x7a, 0x58, 0x5a, 0x00] {
        Box::new(xz2::read::XzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    Ok(reader)
}

fn move_path(source: &Path, destination: &Path) -> Result<(), ArchiveError> {
    if fs::rename(source, destination).is_ok() {
        return Ok(());
    }
    if source.is_dir() {
        copy_dir_recursive(source, destination)?;
        fs::remove_dir_all(source)?;
    } else {
        fs::copy(source, destination)?;
        fs::remove_file(source)?;
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixed_now() -> DateTime<Local> {
        Local
            .with_ymd_and_hms(2025, 10, 6, 7, 51, 55)
            .single()
            .unwrap()
    }

    #[test]
    fn project_names_follow_the_archive_conventions() {
        assert_eq!(
            derive_project_folder_name("13_Camille_dataset.zip"),
            "13 Camille"
        );
        assert_eq!(
            derive_project_folder_name("Camille_Moine.zip"),
            "Camille Moine"
        );
        assert_eq!(derive_project_folder_name("12_Alpha.zip"), "12 Alpha");
        assert_eq!(
            derive_project_folder_name("random_archive.zip"),
            "random archive"
        );
    }

    #[test]
    fn forbidden_characters_are_replaced_in_project_names() {
        assert_eq!(
            derive_project_folder_name("13_Camille&Co.zip"),
            "13 Camille_Co"
        );
    }

    #[test]
    fn unique_dirs_add_a_timestamp_then_a_counter() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path();
        assert_eq!(
            compute_unique_project_dir("13 Camille", base, fixed_now()),
            base.join("13 Camille")
        );
        fs::create_dir_all(base.join("13 Camille")).unwrap();
        assert_eq!(
            compute_unique_project_dir("13 Camille", base, fixed_now()),
            base.join("13 Camille 2025-10-06_07-51-55")
        );
        fs::create_dir_all(base.join("13 Camille 2025-10-06_07-51-55")).unwrap();
        assert_eq!(
            compute_unique_project_dir("13 Camille", base, fixed_now()),
            base.join("13 Camille 2025-10-06_07-51-55-2")
        );
    }

    #[test]
    fn monthly_reset_backs_up_and_truncates() {
        let temp = tempfile::tempdir().unwrap();
        let logs = temp.path().join("logs").join("step1");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join(PROCESSED_ARCHIVES_FILE), "/a.zip\n/b.zip\n").unwrap();
        fs::write(logs.join(PROCESSED_ARCHIVES_RESET_MARKER), "2025-09").unwrap();
        let logger = RunLogger::new(None);
        assert!(reset_processed_archives_if_needed(
            &logs,
            fixed_now(),
            &logger
        ));
        assert_eq!(
            fs::metadata(logs.join(PROCESSED_ARCHIVES_FILE))
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            fs::read_to_string(logs.join(PROCESSED_ARCHIVES_RESET_MARKER)).unwrap(),
            "2025-10"
        );
        let backups: Vec<_> = fs::read_dir(&logs)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("_backup_"))
            .collect();
        assert_eq!(backups.len(), 1);
    }

    #[test]
    fn monthly_reset_is_a_noop_within_the_same_month() {
        let temp = tempfile::tempdir().unwrap();
        let logs = temp.path().join("logs").join("step1");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join(PROCESSED_ARCHIVES_FILE), "/a.zip\n").unwrap();
        fs::write(logs.join(PROCESSED_ARCHIVES_RESET_MARKER), "2025-10").unwrap();
        let logger = RunLogger::new(None);
        assert!(!reset_processed_archives_if_needed(
            &logs,
            fixed_now(),
            &logger
        ));
        assert_eq!(
            fs::read_to_string(logs.join(PROCESSED_ARCHIVES_FILE)).unwrap(),
            "/a.zip\n"
        );
    }

    #[test]
    fn zip_extraction_sanitizes_traversal_and_flattens_single_root() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("13_Camille.zip");
        {
            let file = File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("root/ok.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"ok").unwrap();
            writer
                .start_file(
                    "root/../escape.txt",
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(b"escape").unwrap();
            writer.finish().unwrap();
        }
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        let logger = RunLogger::new(None);
        let outcome = extract_archive(
            &archive,
            &projects,
            "docs",
            Path::new("unrar"),
            &logger,
            ExtractionLimits::default(),
        )
        .unwrap();
        let docs = outcome.docs_dir;
        assert!(docs.join("ok.txt").exists());
        assert!(docs.join("escape.txt").exists());
        assert!(!temp.path().join("escape.txt").exists());
        assert!(!projects.join("_temp_13 Camille").exists());
    }

    #[cfg(unix)]
    #[test]
    fn rar_member_is_streamed_and_partial_file_removed_on_failure() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("17_Camille.rar");
        fs::write(&archive, b"fake rar").unwrap();
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        let unrar = temp.path().join("unrar");
        fs::write(
            &unrar,
            "#!/bin/sh\nif [ \"$1\" = \"lb\" ]; then echo 'root/clip.bin'; exit 0; fi\nif [ \"$1\" = \"p\" ]; then head -c 8388608 /dev/zero; exit 0; fi\nexit 2\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&unrar).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&unrar, permissions).unwrap();

        let logger = RunLogger::new(None);
        let outcome = extract_archive(
            &archive,
            &projects,
            "docs",
            &unrar,
            &logger,
            ExtractionLimits::default(),
        )
        .unwrap();
        let extracted = outcome.docs_dir.join("clip.bin");
        assert_eq!(fs::metadata(&extracted).unwrap().len(), 8_388_608);

        let second = temp.path().join("18_Camille.rar");
        fs::write(&second, b"fake rar").unwrap();
        let failing = temp.path().join("unrar-fail");
        fs::write(
            &failing,
            "#!/bin/sh\nif [ \"$1\" = \"lb\" ]; then echo 'root/broken.bin'; exit 0; fi\nif [ \"$1\" = \"p\" ]; then head -c 4096 /dev/zero; echo boom >&2; exit 1; fi\nexit 2\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&failing).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&failing, permissions).unwrap();
        assert!(extract_archive(
            &second,
            &projects,
            "docs",
            &failing,
            &logger,
            ExtractionLimits::default()
        )
        .is_err());
        assert!(!projects
            .join("18 Camille")
            .join("docs")
            .join("broken.bin")
            .exists());
    }

    #[test]
    fn given_zip_bomb_when_budget_exceeded_then_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("19_Camille.zip");
        {
            let file = File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("root/bomb.bin", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(&vec![0_u8; 2 * 1024 * 1024]).unwrap();
            writer.finish().unwrap();
        }
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        let logger = RunLogger::new(None);
        let error = extract_archive(
            &archive,
            &projects,
            "docs",
            Path::new("unrar"),
            &logger,
            ExtractionLimits {
                max_bytes: 1024 * 1024,
                max_compression_ratio: 200,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("budget"));
        assert!(!projects
            .join("19 Camille")
            .join("docs")
            .join("bomb.bin")
            .exists());
    }

    #[test]
    fn compressed_tar_extraction_is_supported() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("16_Camille.tar.gz");
        {
            let file = File::create(&archive).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);
            let data = b"ok";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "root/clip.txt", &data[..])
                .unwrap();
            builder.into_inner().unwrap().finish().unwrap();
        }
        let projects = temp.path().join("projects");
        fs::create_dir_all(&projects).unwrap();
        let logger = RunLogger::new(None);
        let outcome = extract_archive(
            &archive,
            &projects,
            "docs",
            Path::new("unrar"),
            &logger,
            ExtractionLimits::default(),
        )
        .unwrap();
        assert!(outcome.docs_dir.join("clip.txt").exists());
    }

    #[test]
    fn bzip2_and_xz_tar_extraction_are_supported() {
        let temp = tempfile::tempdir().unwrap();
        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let data = b"ok";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "clip.txt", &data[..])
                .unwrap();
            builder.finish().unwrap();
        }

        let mut bz2 = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
        bz2.write_all(&tar_bytes).unwrap();
        let bz2 = bz2.finish().unwrap();

        let mut xz = xz2::write::XzEncoder::new(Vec::new(), 6);
        xz.write_all(&tar_bytes).unwrap();
        let xz = xz.finish().unwrap();

        let logger = RunLogger::new(None);
        for (name, bytes) in [("21_Camille.tar.bz2", bz2), ("22_Camille.tar.xz", xz)] {
            let archive = temp.path().join(name);
            fs::write(&archive, bytes).unwrap();
            let projects = temp.path().join(format!("projects-{name}"));
            fs::create_dir_all(&projects).unwrap();
            let outcome = extract_archive(
                &archive,
                &projects,
                "docs",
                Path::new("unrar"),
                &logger,
                ExtractionLimits::default(),
            )
            .unwrap();
            assert!(outcome.docs_dir.join("clip.txt").exists());
        }
    }

    #[test]
    fn scan_matches_only_single_suffix_archives() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("downloads");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("13_Camille.zip"), b"zip").unwrap();
        fs::write(source.join("14_Camille.tar.gz"), b"tar").unwrap();
        fs::write(source.join("notes.txt"), b"text").unwrap();
        let found = find_archives_to_process(&source, "camille", &temp.path().join("logs"));
        assert_eq!(found, vec![source.join("13_Camille.zip")]);
    }
}
