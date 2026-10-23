//! Synchronisation des fichiers lourds : archives téléchargées et analyses
//! STEP7.
//!
//! Les téléchargements sont copiés ou déplacés entre les deux dossiers, sans
//! jamais écraser un fichier existant de contenu différent. Les analyses STEP7
//! sont identifiées par `(projet, hash vidéo)` : un hash déjà présent côté
//! cible n'est pas recopié, seuls les fichiers manquants d'une archive
//! partielle sont ajoutés.

use crate::SyncDirection;
use anyhow::{bail, Context, Result};
use chrono::NaiveDateTime;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const ARCHIVE_EXTENSIONS: [&str; 6] = [".zip", ".zipx", ".rar", ".tgz", ".7z", ".tar"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownloadsMode {
    Copy,
    Move,
    Skip,
}

impl DownloadsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Move => "move",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadsSyncOptions {
    pub python_dir: PathBuf,
    pub rust_dir: PathBuf,
    pub direction: SyncDirection,
    pub mode: DownloadsMode,
    pub keyword: String,
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileTransfer {
    pub filename: String,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub size_bytes: u64,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileConflict {
    pub filename: String,
    pub python_path: PathBuf,
    pub rust_path: PathBuf,
    pub python_size: u64,
    pub rust_size: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadsSyncReport {
    pub python_dir: PathBuf,
    pub rust_dir: PathBuf,
    pub mode: String,
    pub direction: String,
    pub python_files: usize,
    pub rust_files: usize,
    pub python_to_rust: Vec<FileTransfer>,
    pub rust_to_python: Vec<FileTransfer>,
    pub skipped_identical: usize,
    pub skipped_other: usize,
    pub conflicts: Vec<FileConflict>,
    pub bytes_copied: u64,
    pub warnings: Vec<String>,
    pub applied: bool,
}

#[derive(Debug, Clone, Copy)]
enum TransferDirection {
    PythonToRust,
    RustToPython,
}

pub fn sync_downloads(options: &DownloadsSyncOptions) -> Result<DownloadsSyncReport> {
    let mut report = DownloadsSyncReport {
        python_dir: options.python_dir.clone(),
        rust_dir: options.rust_dir.clone(),
        mode: options.mode.as_str().into(),
        direction: options.direction.as_str().into(),
        python_files: 0,
        rust_files: 0,
        python_to_rust: Vec::new(),
        rust_to_python: Vec::new(),
        skipped_identical: 0,
        skipped_other: 0,
        conflicts: Vec::new(),
        bytes_copied: 0,
        warnings: Vec::new(),
        applied: options.apply,
    };
    if options.mode == DownloadsMode::Skip {
        return Ok(report);
    }
    let (python_files, python_skipped) =
        list_download_archives(&options.python_dir, &options.keyword)?;
    let (rust_files, rust_skipped) = list_download_archives(&options.rust_dir, &options.keyword)?;
    report.python_files = python_files.len();
    report.rust_files = rust_files.len();
    report.skipped_other = python_skipped + rust_skipped;

    if options.direction.includes_rust() {
        for source in &python_files {
            transfer_download(
                source,
                &options.rust_dir,
                options.mode,
                options.apply,
                &mut report,
                TransferDirection::PythonToRust,
            )?;
        }
    }
    if options.direction.includes_python() {
        for source in &rust_files {
            transfer_download(
                source,
                &options.python_dir,
                options.mode,
                options.apply,
                &mut report,
                TransferDirection::RustToPython,
            )?;
        }
    }
    // Les deux sens peuvent signaler le même conflit avec les chemins
    // inversés : ne garder qu'une entrée par paire de fichiers.
    report.conflicts.sort_by(|left, right| {
        left.filename
            .cmp(&right.filename)
            .then_with(|| left.python_path.cmp(&right.python_path))
            .then_with(|| left.rust_path.cmp(&right.rust_path))
    });
    report.conflicts.dedup_by(|left, right| {
        left.filename == right.filename
            && left.python_path == right.python_path
            && left.rust_path == right.rust_path
    });
    Ok(report)
}

fn list_download_archives(dir: &Path, keyword: &str) -> Result<(Vec<PathBuf>, usize)> {
    if !dir.is_dir() {
        return Ok((Vec::new(), 0));
    }
    let keyword = keyword.trim().to_lowercase();
    let mut selected = Vec::new();
    let mut skipped = 0;
    for entry in fs::read_dir(dir).with_context(|| format!("lecture de {}", dir.display()))? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let lower = name.to_lowercase();
        let is_archive = ARCHIVE_EXTENSIONS
            .iter()
            .any(|extension| lower.ends_with(extension));
        let is_partial =
            name.starts_with('.') || lower.ends_with(".part") || lower.ends_with(".crdownload");
        let matches_keyword = keyword.is_empty() || lower.contains(&keyword);
        if is_archive && !is_partial && matches_keyword {
            selected.push(entry.path());
        } else {
            skipped += 1;
        }
    }
    selected.sort();
    Ok((selected, skipped))
}

fn transfer_download(
    source: &Path,
    destination_dir: &Path,
    mode: DownloadsMode,
    apply: bool,
    report: &mut DownloadsSyncReport,
    direction: TransferDirection,
) -> Result<()> {
    let filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("archive")
        .to_string();
    let destination = destination_dir.join(&filename);
    let source_size = source
        .metadata()
        .with_context(|| format!("métadonnées de {}", source.display()))?
        .len();
    if destination.exists() {
        let destination_size = destination
            .metadata()
            .with_context(|| format!("métadonnées de {}", destination.display()))?
            .len();
        if source_size == destination_size && files_same_content(source, &destination)? {
            report.skipped_identical += 1;
            return Ok(());
        }
        report.conflicts.push(FileConflict {
            filename,
            python_path: if matches!(direction, TransferDirection::PythonToRust) {
                source.to_path_buf()
            } else {
                destination.clone()
            },
            rust_path: if matches!(direction, TransferDirection::PythonToRust) {
                destination.clone()
            } else {
                source.to_path_buf()
            },
            python_size: if matches!(direction, TransferDirection::PythonToRust) {
                source_size
            } else {
                destination_size
            },
            rust_size: if matches!(direction, TransferDirection::PythonToRust) {
                destination_size
            } else {
                source_size
            },
            reason: "fichier de même nom mais de contenu différent".into(),
        });
        return Ok(());
    }

    if apply {
        fs::create_dir_all(destination_dir)
            .with_context(|| format!("création de {}", destination_dir.display()))?;
        match mode {
            DownloadsMode::Copy => copy_atomic(source, &destination)?,
            DownloadsMode::Move => move_file(source, &destination)?,
            DownloadsMode::Skip => unreachable!(),
        }
    }

    let transfer = FileTransfer {
        filename,
        source: source.to_path_buf(),
        destination,
        size_bytes: source_size,
        action: mode.as_str().into(),
    };
    match direction {
        TransferDirection::PythonToRust => report.python_to_rust.push(transfer),
        TransferDirection::RustToPython => report.rust_to_python.push(transfer),
    }
    report.bytes_copied += source_size;
    Ok(())
}

fn move_file(source: &Path, destination: &Path) -> Result<()> {
    let source_size = source
        .metadata()
        .with_context(|| format!("métadonnées de {}", source.display()))?
        .len();
    if fs::rename(source, destination).is_ok() {
        return Ok(());
    }
    copy_atomic(source, destination)?;
    let copied_size = destination
        .metadata()
        .with_context(|| format!("métadonnées de {}", destination.display()))?
        .len();
    if copied_size != source_size {
        bail!(
            "copie incomplète avant déplacement: {} octets sur {}",
            copied_size,
            source_size
        );
    }
    fs::remove_file(source).with_context(|| format!("suppression de {}", source.display()))?;
    Ok(())
}

fn copy_atomic(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .with_context(|| format!("destination sans parent: {}", destination.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("création de {}", parent.display()))?;
    let filename = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("fichier");
    let temp = parent.join(format!(".{filename}.part-{}", std::process::id()));
    if temp.exists() {
        fs::remove_file(&temp)
            .with_context(|| format!("suppression de l'ancien temporaire {}", temp.display()))?;
    }
    let result = (|| -> Result<()> {
        let copied = fs::copy(source, &temp)
            .with_context(|| format!("copie de {} vers {}", source.display(), temp.display()))?;
        let source_size = source
            .metadata()
            .with_context(|| format!("métadonnées de {}", source.display()))?
            .len();
        if copied != source_size {
            bail!(
                "copie incomplète de {}: {} octets sur {}",
                source.display(),
                copied,
                source_size
            );
        }
        if destination.exists() {
            bail!(
                "destination apparue pendant la copie: {}",
                destination.display()
            );
        }
        fs::rename(&temp, destination).with_context(|| {
            format!(
                "renommage de {} vers {}",
                temp.display(),
                destination.display()
            )
        })?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn files_same_content(left: &Path, right: &Path) -> Result<bool> {
    let left_size = left.metadata()?.len();
    let right_size = right.metadata()?.len();
    if left_size != right_size {
        return Ok(false);
    }
    Ok(hash_file(left)? == hash_file(right)?)
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("ouverture de {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("lecture de {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Step7Mode {
    Copy,
    Link,
    Skip,
}

impl Step7Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Link => "link",
            Self::Skip => "skip",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Step7SyncOptions {
    pub python_dir: PathBuf,
    pub rust_dir: PathBuf,
    pub direction: SyncDirection,
    pub mode: Step7Mode,
    pub apply: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HashTransfer {
    pub project: String,
    pub hash: String,
    pub source_project_dir: PathBuf,
    pub destination_project_dir: PathBuf,
    pub files: usize,
    pub bytes: u64,
    pub partial: bool,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveFileConflict {
    pub project: String,
    pub hash: String,
    pub relative_path: PathBuf,
    pub source: PathBuf,
    pub destination: PathBuf,
    pub source_size: u64,
    pub destination_size: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step7SyncReport {
    pub python_dir: PathBuf,
    pub rust_dir: PathBuf,
    pub mode: String,
    pub direction: String,
    pub python_hashes: usize,
    pub rust_hashes: usize,
    pub python_to_rust: Vec<HashTransfer>,
    pub rust_to_python: Vec<HashTransfer>,
    pub copied_hashes: usize,
    pub copied_files: usize,
    pub bytes_copied: u64,
    pub duplicates_skipped: usize,
    pub partial_merges: usize,
    pub conflicts: Vec<ArchiveFileConflict>,
    pub warnings: Vec<String>,
    pub applied: bool,
}

struct ArchiveHashDir {
    project: String,
    hash: String,
    project_dir: PathBuf,
    hash_dir: PathBuf,
    files: Vec<PathBuf>,
}

pub fn sync_step7_archives(options: &Step7SyncOptions) -> Result<Step7SyncReport> {
    let python_index = scan_step7_archives(&options.python_dir)?;
    let rust_index = scan_step7_archives(&options.rust_dir)?;
    let mut report = Step7SyncReport {
        python_dir: options.python_dir.clone(),
        rust_dir: options.rust_dir.clone(),
        mode: options.mode.as_str().into(),
        direction: options.direction.as_str().into(),
        python_hashes: python_index.len(),
        rust_hashes: rust_index.len(),
        python_to_rust: Vec::new(),
        rust_to_python: Vec::new(),
        copied_hashes: 0,
        copied_files: 0,
        bytes_copied: 0,
        duplicates_skipped: 0,
        partial_merges: 0,
        conflicts: Vec::new(),
        warnings: Vec::new(),
        applied: options.apply,
    };
    if options.mode == Step7Mode::Skip {
        return Ok(report);
    }
    if options.direction.includes_rust() {
        sync_step7_direction(
            &python_index,
            &options.rust_dir,
            &rust_index,
            options,
            &mut report,
            TransferDirection::PythonToRust,
        )?;
    }
    if options.direction.includes_python() {
        sync_step7_direction(
            &rust_index,
            &options.python_dir,
            &python_index,
            options,
            &mut report,
            TransferDirection::RustToPython,
        )?;
    }
    // En mode `both`, le même conflit est vu deux fois avec les rôles
    // inversés. Ne garder qu'une entrée par fichier.
    report.conflicts.sort_by(|left, right| {
        left.project
            .cmp(&right.project)
            .then_with(|| left.hash.cmp(&right.hash))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    report.conflicts.dedup_by(|left, right| {
        left.project == right.project
            && left.hash == right.hash
            && left.relative_path == right.relative_path
    });
    Ok(report)
}

fn scan_step7_archives(root: &Path) -> Result<BTreeMap<(String, String), Vec<ArchiveHashDir>>> {
    let mut index: BTreeMap<(String, String), Vec<ArchiveHashDir>> = BTreeMap::new();
    if !root.is_dir() {
        return Ok(index);
    }
    for project_entry in
        fs::read_dir(root).with_context(|| format!("lecture de {}", root.display()))?
    {
        let project_entry = project_entry?;
        if !project_entry.file_type()?.is_dir() {
            continue;
        }
        let project_name = project_entry.file_name().to_string_lossy().to_string();
        let (project, _timestamp) = split_project_name(&project_name);
        for hash_entry in fs::read_dir(project_entry.path())
            .with_context(|| format!("lecture de {}", project_entry.path().display()))?
        {
            let hash_entry = hash_entry?;
            if !hash_entry.file_type()?.is_dir() {
                continue;
            }
            let hash = hash_entry.file_name().to_string_lossy().to_string();
            if !is_sha256_hex(&hash) {
                continue;
            }
            let hash_dir = hash_entry.path();
            let files = collect_files(&hash_dir)?;
            if files.is_empty() {
                continue;
            }
            index
                .entry((project.clone(), hash.clone()))
                .or_default()
                .push(ArchiveHashDir {
                    project: project.clone(),
                    hash,
                    project_dir: project_entry.path(),
                    hash_dir,
                    files,
                });
        }
    }
    for entries in index.values_mut() {
        // Comme Python (`_list_matching_project_dirs`), préférer l'archive la
        // plus récente quand un même hash apparaît dans plusieurs dossiers.
        entries.sort_by(|left, right| right.project_dir.cmp(&left.project_dir));
    }
    Ok(index)
}

fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

fn sync_step7_direction(
    source_index: &BTreeMap<(String, String), Vec<ArchiveHashDir>>,
    destination_root: &Path,
    destination_index: &BTreeMap<(String, String), Vec<ArchiveHashDir>>,
    options: &Step7SyncOptions,
    report: &mut Step7SyncReport,
    direction: TransferDirection,
) -> Result<()> {
    for (key, source_candidates) in source_index {
        let source = &source_candidates[0];
        // Un même hash peut apparaître dans plusieurs archives horodatées.
        // Copier l'union des fichiers, en préférant l'archive la plus récente.
        let mut source_files: BTreeMap<PathBuf, PathBuf> = BTreeMap::new();
        for candidate in source_candidates {
            for file in &candidate.files {
                let relative = file.strip_prefix(&candidate.hash_dir)?.to_path_buf();
                source_files.entry(relative).or_insert_with(|| file.clone());
            }
        }
        if let Some(destination_candidates) = destination_index.get(key) {
            report.duplicates_skipped += 1;
            let destination = &destination_candidates[0];
            let mut transfer = HashTransfer {
                project: source.project.clone(),
                hash: source.hash.clone(),
                source_project_dir: source.project_dir.clone(),
                destination_project_dir: destination.project_dir.clone(),
                files: 0,
                bytes: 0,
                partial: true,
                action: options.mode.as_str().into(),
            };
            for (relative, source_file) in &source_files {
                let destination_file = destination.hash_dir.join(relative);
                match file_state(source_file, &destination_file)? {
                    FileState::Missing => {
                        if options.apply {
                            if let Some(parent) = destination_file.parent() {
                                fs::create_dir_all(parent)
                                    .with_context(|| format!("création de {}", parent.display()))?;
                            }
                            copy_or_link(source_file, &destination_file, options.mode)?;
                        }
                        transfer.files += 1;
                        transfer.bytes += source_file.metadata()?.len();
                        report.copied_files += 1;
                        report.bytes_copied += source_file.metadata()?.len();
                    }
                    FileState::Same => {}
                    FileState::Different {
                        source_size,
                        destination_size,
                    } => report.conflicts.push(ArchiveFileConflict {
                        project: source.project.clone(),
                        hash: source.hash.clone(),
                        relative_path: relative.clone(),
                        source: source_file.clone(),
                        destination: destination_file,
                        source_size,
                        destination_size,
                        reason: "fichier présent des deux côtés avec un contenu différent".into(),
                    }),
                }
            }
            if transfer.files > 0 {
                report.partial_merges += 1;
                push_hash_transfer(report, direction, transfer);
            }
            continue;
        }

        let destination_project_dir = destination_root.join(
            source
                .project_dir
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("archive")),
        );
        let destination_hash_dir = destination_project_dir.join(&source.hash);
        let mut transfer = HashTransfer {
            project: source.project.clone(),
            hash: source.hash.clone(),
            source_project_dir: source.project_dir.clone(),
            destination_project_dir: destination_project_dir.clone(),
            files: 0,
            bytes: 0,
            partial: false,
            action: options.mode.as_str().into(),
        };
        for (relative, source_file) in &source_files {
            let destination_file = destination_hash_dir.join(relative);
            match file_state(source_file, &destination_file)? {
                FileState::Missing => {
                    if options.apply {
                        if let Some(parent) = destination_file.parent() {
                            fs::create_dir_all(parent)
                                .with_context(|| format!("création de {}", parent.display()))?;
                        }
                        copy_or_link(source_file, &destination_file, options.mode)?;
                    }
                    transfer.files += 1;
                    transfer.bytes += source_file.metadata()?.len();
                    report.copied_files += 1;
                    report.bytes_copied += source_file.metadata()?.len();
                }
                FileState::Same => {}
                FileState::Different {
                    source_size,
                    destination_size,
                } => report.conflicts.push(ArchiveFileConflict {
                    project: source.project.clone(),
                    hash: source.hash.clone(),
                    relative_path: relative.clone(),
                    source: source_file.clone(),
                    destination: destination_file,
                    source_size,
                    destination_size,
                    reason: "fichier présent des deux côtés avec un contenu différent".into(),
                }),
            }
        }
        if transfer.files > 0 {
            report.copied_hashes += 1;
            push_hash_transfer(report, direction, transfer);
        }
    }
    Ok(())
}

enum FileState {
    Missing,
    Same,
    Different {
        source_size: u64,
        destination_size: u64,
    },
}

fn file_state(source: &Path, destination: &Path) -> Result<FileState> {
    if !destination.exists() {
        return Ok(FileState::Missing);
    }
    let source_size = source
        .metadata()
        .with_context(|| format!("métadonnées de {}", source.display()))?
        .len();
    let destination_size = destination
        .metadata()
        .with_context(|| format!("métadonnées de {}", destination.display()))?
        .len();
    if source_size != destination_size {
        return Ok(FileState::Different {
            source_size,
            destination_size,
        });
    }
    if !files_same_content(source, destination)? {
        return Ok(FileState::Different {
            source_size,
            destination_size,
        });
    }
    Ok(FileState::Same)
}

fn copy_or_link(source: &Path, destination: &Path, mode: Step7Mode) -> Result<()> {
    match mode {
        Step7Mode::Link => {
            if fs::hard_link(source, destination).is_ok() {
                Ok(())
            } else {
                copy_atomic(source, destination)
            }
        }
        Step7Mode::Copy => copy_atomic(source, destination),
        Step7Mode::Skip => Ok(()),
    }
}

fn push_hash_transfer(
    report: &mut Step7SyncReport,
    direction: TransferDirection,
    transfer: HashTransfer,
) {
    match direction {
        TransferDirection::PythonToRust => report.python_to_rust.push(transfer),
        TransferDirection::RustToPython => report.rust_to_python.push(transfer),
    }
}

fn split_project_name(name: &str) -> (String, Option<String>) {
    if let Some((base, suffix)) = name.rsplit_once(' ') {
        if is_timestamp(suffix) {
            return (base.to_string(), Some(suffix.to_string()));
        }
    }
    (name.to_string(), None)
}

fn is_timestamp(value: &str) -> bool {
    if NaiveDateTime::parse_from_str(value, "%Y-%m-%d_%H-%M-%S").is_ok() {
        return true;
    }
    if let Some((base, counter)) = value.rsplit_once('-') {
        if !counter.is_empty() && counter.chars().all(|character| character.is_ascii_digit()) {
            return NaiveDateTime::parse_from_str(base, "%Y-%m-%d_%H-%M-%S").is_ok();
        }
    }
    false
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, content: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn download_options(root: &Path, mode: DownloadsMode) -> DownloadsSyncOptions {
        DownloadsSyncOptions {
            python_dir: root.join("python/downloads"),
            rust_dir: root.join("rust/downloads"),
            direction: SyncDirection::Both,
            mode,
            keyword: "Camille".into(),
            apply: false,
        }
    }

    #[test]
    fn copies_missing_downloads_and_skips_identical_ones() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("python/downloads/101 Camille.zip"),
            b"archive-101",
        );
        write(
            &dir.path().join("python/downloads/102 Camille.zip"),
            b"archive-102",
        );
        write(
            &dir.path().join("rust/downloads/102 Camille.zip"),
            b"archive-102",
        );
        let mut options = download_options(dir.path(), DownloadsMode::Copy);
        options.apply = true;
        let report = sync_downloads(&options).unwrap();
        assert_eq!(report.python_to_rust.len(), 1);
        assert_eq!(report.skipped_identical, 2);
        assert!(dir.path().join("rust/downloads/101 Camille.zip").exists());
        assert!(dir.path().join("python/downloads/101 Camille.zip").exists());
    }

    #[test]
    fn moves_downloads_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("python/downloads/101 Camille.zip"),
            b"archive-101",
        );
        let mut options = download_options(dir.path(), DownloadsMode::Move);
        options.apply = true;
        let report = sync_downloads(&options).unwrap();
        assert_eq!(report.python_to_rust.len(), 1);
        assert!(!dir.path().join("python/downloads/101 Camille.zip").exists());
        assert!(dir.path().join("rust/downloads/101 Camille.zip").exists());
    }

    #[test]
    fn refuses_to_overwrite_a_different_download() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("python/downloads/101 Camille.zip"),
            b"python-version",
        );
        write(
            &dir.path().join("rust/downloads/101 Camille.zip"),
            b"rust-version",
        );
        let mut options = download_options(dir.path(), DownloadsMode::Copy);
        options.apply = true;
        let report = sync_downloads(&options).unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(
            fs::read(dir.path().join("rust/downloads/101 Camille.zip")).unwrap(),
            b"rust-version"
        );
    }

    #[test]
    fn ignores_other_files_and_partial_downloads() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("python/downloads/notes.pdf"), b"pdf");
        write(
            &dir.path().join("python/downloads/101 Camille.zip.part"),
            b"partial",
        );
        write(&dir.path().join("python/downloads/autre.zip"), b"other");
        let mut options = download_options(dir.path(), DownloadsMode::Copy);
        options.apply = true;
        let report = sync_downloads(&options).unwrap();
        assert!(report.python_to_rust.is_empty());
        assert_eq!(report.skipped_other, 3);
    }

    fn step7_options(root: &Path, mode: Step7Mode) -> Step7SyncOptions {
        Step7SyncOptions {
            python_dir: root.join("python/archives"),
            rust_dir: root.join("rust/archives"),
            direction: SyncDirection::Both,
            mode,
            apply: false,
        }
    }

    fn hash(character: char) -> String {
        std::iter::repeat_n(character, 64).collect()
    }

    #[test]
    fn copies_step7_hashes_and_skips_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let hash_a = hash('a');
        let hash_b = hash('b');
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-10-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"scenes-a",
        );
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-10-01_10-00-00")
                .join(&hash_b)
                .join("video_audio.json"),
            b"audio-b",
        );
        write(
            &dir.path()
                .join("rust/archives/101 Camille 2026-09-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"scenes-a",
        );
        let mut options = step7_options(dir.path(), Step7Mode::Copy);
        options.apply = true;
        let report = sync_step7_archives(&options).unwrap();
        assert_eq!(report.copied_hashes, 1);
        assert_eq!(report.duplicates_skipped, 2);
        assert!(dir
            .path()
            .join("rust/archives/101 Camille 2026-10-01_10-00-00")
            .join(&hash_b)
            .join("video_audio.json")
            .exists());
    }

    #[test]
    fn merges_files_from_multiple_source_archives_for_the_same_hash() {
        let dir = tempfile::tempdir().unwrap();
        let hash_a = hash('a');
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-09-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"scenes",
        );
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-10-01_10-00-00")
                .join(&hash_a)
                .join("video_audio.json"),
            b"audio",
        );
        let mut options = step7_options(dir.path(), Step7Mode::Copy);
        options.apply = true;
        let report = sync_step7_archives(&options).unwrap();
        assert_eq!(report.copied_hashes, 1);
        assert_eq!(report.copied_files, 2);
        let destination = dir
            .path()
            .join("rust/archives/101 Camille 2026-10-01_10-00-00")
            .join(&hash_a);
        assert!(destination.join("video_scenes.csv").exists());
        assert!(destination.join("video_audio.json").exists());
    }

    #[test]
    fn completes_a_partial_step7_hash() {
        let dir = tempfile::tempdir().unwrap();
        let hash_a = hash('a');
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-10-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"scenes",
        );
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-10-01_10-00-00")
                .join(&hash_a)
                .join("video_audio.json"),
            b"audio",
        );
        write(
            &dir.path()
                .join("rust/archives/101 Camille 2026-09-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"scenes",
        );
        let mut options = step7_options(dir.path(), Step7Mode::Copy);
        options.apply = true;
        let report = sync_step7_archives(&options).unwrap();
        assert_eq!(report.partial_merges, 1);
        assert!(dir
            .path()
            .join("rust/archives/101 Camille 2026-09-01_10-00-00")
            .join(&hash_a)
            .join("video_audio.json")
            .exists());
    }

    #[test]
    fn reports_same_size_step7_conflicts_without_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        let hash_a = hash('a');
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-10-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"source",
        );
        write(
            &dir.path()
                .join("rust/archives/101 Camille 2026-09-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"target",
        );
        let mut options = step7_options(dir.path(), Step7Mode::Copy);
        options.apply = true;
        let report = sync_step7_archives(&options).unwrap();
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(
            fs::read(
                dir.path()
                    .join("rust/archives/101 Camille 2026-09-01_10-00-00")
                    .join(&hash_a)
                    .join("video_scenes.csv")
            )
            .unwrap(),
            b"target"
        );
    }

    #[test]
    fn dry_run_reports_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let hash_a = hash('a');
        write(
            &dir.path()
                .join("python/archives/101 Camille 2026-10-01_10-00-00")
                .join(&hash_a)
                .join("video_scenes.csv"),
            b"scenes",
        );
        let report = sync_step7_archives(&step7_options(dir.path(), Step7Mode::Copy)).unwrap();
        assert!(!report.applied);
        assert_eq!(report.copied_hashes, 1);
        assert!(!dir.path().join("rust/archives").exists());
    }

    #[test]
    fn link_mode_creates_hardlinks() {
        let dir = tempfile::tempdir().unwrap();
        let hash_a = hash('a');
        let source = dir
            .path()
            .join("python/archives/101 Camille 2026-10-01_10-00-00")
            .join(&hash_a)
            .join("video_scenes.csv");
        write(&source, b"scenes");
        let mut options = step7_options(dir.path(), Step7Mode::Link);
        options.apply = true;
        sync_step7_archives(&options).unwrap();
        let destination = dir
            .path()
            .join("rust/archives/101 Camille 2026-10-01_10-00-00")
            .join(&hash_a)
            .join("video_scenes.csv");
        assert!(destination.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(
                fs::metadata(&source).unwrap().ino(),
                fs::metadata(&destination).unwrap().ino()
            );
        }
    }

    #[test]
    fn splits_project_names_and_timestamps() {
        assert_eq!(
            split_project_name("101 Camille 2026-10-01_10-00-00"),
            ("101 Camille".into(), Some("2026-10-01_10-00-00".into()))
        );
        assert_eq!(
            split_project_name("101 Camille"),
            ("101 Camille".into(), None)
        );
        assert!(is_sha256_hex(&hash('a')));
        assert!(!is_sha256_hex("pas-un-hash"));
    }
}
