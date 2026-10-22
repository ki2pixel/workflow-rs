//! Rétention des données de travail.
//!
//! Le nettoyage ne touche jamais aux sorties finalisées (`paths.finalized`) :
//! ce sont des livrables. Il se limite aux projets sources, aux archives
//! d'analyses, aux journaux, aux rapports et aux téléchargements.
//!
//! Par défaut, le plan est calculé sans rien supprimer. L'application exige
//! `--apply` et revalide chaque candidat juste avant sa suppression.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

const PROTECTED_NAMES: &[&str] = &[
    "archives",
    "logs",
    "docs",
    "static",
    "templates",
    "_finalized_output",
    "venv",
    "env",
    "models",
    "downloads",
    "runtime",
    "projets_extraits",
];

#[derive(Debug, Clone)]
pub struct RetentionOptions {
    pub projects_dir: PathBuf,
    pub archives_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub downloads_dir: PathBuf,
    /// Seuil d'inactivité des projets sources. 0 désactive.
    pub orphan_hours: f64,
    /// 0 désactive le nettoyage des archives.
    pub archives_days: u64,
    /// 0 désactive le nettoyage des journaux.
    pub logs_days: u64,
    /// 0 désactive le nettoyage des téléchargements.
    pub downloads_days: u64,
    /// N'autorise le nettoyage des téléchargements que dans un répertoire
    /// dédié au pipeline (jamais le dossier Téléchargements de l'utilisateur).
    pub downloads_managed: bool,
    /// 0 désactive le nettoyage des rapports JSON.
    pub reports_days: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionThresholds {
    pub orphan_hours: f64,
    pub archives_days: u64,
    pub logs_days: u64,
    pub downloads_days: u64,
    pub reports_days: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionCandidate {
    pub path: PathBuf,
    pub kind: String,
    pub age_hours: f64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RetentionReport {
    pub dry_run: bool,
    pub projects_dir: PathBuf,
    pub archives_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub downloads_dir: PathBuf,
    pub thresholds: RetentionThresholds,
    pub candidates: Vec<RetentionCandidate>,
    pub deleted: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
    pub errors: Vec<String>,
    pub bytes_reclaimable: u64,
    pub bytes_reclaimed: u64,
}

pub fn plan_retention(options: &RetentionOptions) -> Result<RetentionReport> {
    for (label, root) in [
        ("projets", &options.projects_dir),
        ("archives", &options.archives_dir),
        ("journaux", &options.logs_dir),
        ("téléchargements", &options.downloads_dir),
    ] {
        if root.exists() && !is_safe_root(root) {
            bail!(
                "rétention refusée pour les {label}: {} est un répertoire trop large ou un lien symbolique",
                root.display()
            );
        }
    }
    if options.downloads_days > 0 && !options.downloads_managed {
        bail!(
            "nettoyage des téléchargements refusé: {} n'est pas un répertoire géré par le pipeline",
            options.downloads_dir.display()
        );
    }
    let mut report = base_report(options);
    report.candidates = collect_candidates(options);
    report.bytes_reclaimable = report.candidates.iter().map(|item| item.bytes).sum();
    Ok(report)
}

pub fn apply_retention(options: &RetentionOptions) -> Result<RetentionReport> {
    let mut report = plan_retention(options)?;
    if options.dry_run {
        return Ok(report);
    }

    for candidate in report.candidates.clone() {
        if !still_valid(options, &candidate.kind, &candidate.path) {
            report.skipped.push(candidate.path);
            continue;
        }
        match remove_candidate(&candidate) {
            Ok(()) => {
                report.deleted.push(candidate.path);
                report.bytes_reclaimed += candidate.bytes;
            }
            Err(error) => report.errors.push(format!(
                "suppression de {} impossible: {error}",
                candidate.path.display()
            )),
        }
    }
    Ok(report)
}

fn base_report(options: &RetentionOptions) -> RetentionReport {
    RetentionReport {
        dry_run: options.dry_run,
        projects_dir: options.projects_dir.clone(),
        archives_dir: options.archives_dir.clone(),
        logs_dir: options.logs_dir.clone(),
        downloads_dir: options.downloads_dir.clone(),
        thresholds: RetentionThresholds {
            orphan_hours: options.orphan_hours,
            archives_days: options.archives_days,
            logs_days: options.logs_days,
            downloads_days: options.downloads_days,
            reports_days: options.reports_days,
        },
        candidates: Vec::new(),
        deleted: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
        bytes_reclaimable: 0,
        bytes_reclaimed: 0,
    }
}

fn collect_candidates(options: &RetentionOptions) -> Vec<RetentionCandidate> {
    let now = SystemTime::now();
    let mut candidates = Vec::new();

    if options.orphan_hours > 0.0 {
        for path in direct_directories(&options.projects_dir) {
            if is_protected(&path) {
                continue;
            }
            let Some(age_hours) = age_hours(&path, now) else {
                continue;
            };
            if age_hours > options.orphan_hours {
                candidates.push(RetentionCandidate {
                    bytes: directory_size(&path),
                    path,
                    kind: "projet_orphelin".into(),
                    age_hours: round_hours(age_hours),
                });
            }
        }
    }

    if options.archives_days > 0 {
        let threshold = days_as_hours(options.archives_days);
        for path in direct_directories(&options.archives_dir) {
            if is_protected(&path) || !is_archive_directory(&path) {
                continue;
            }
            let Some(age_hours) = age_hours(&path, now) else {
                continue;
            };
            if age_hours > threshold {
                candidates.push(RetentionCandidate {
                    bytes: directory_size(&path),
                    path,
                    kind: "archive".into(),
                    age_hours: round_hours(age_hours),
                });
            }
        }
    }

    if options.logs_days > 0 {
        let threshold = days_as_hours(options.logs_days);
        collect_files(
            &options.logs_dir,
            now,
            threshold,
            "journal",
            &mut candidates,
            is_log_name,
        );
    }

    if options.reports_days > 0 {
        let threshold = days_as_hours(options.reports_days);
        collect_files(
            &options.logs_dir,
            now,
            threshold,
            "rapport",
            &mut candidates,
            is_report_name,
        );
    }

    if options.downloads_days > 0 && options.downloads_managed {
        let threshold = days_as_hours(options.downloads_days);
        collect_files(
            &options.downloads_dir,
            now,
            threshold,
            "telechargement",
            &mut candidates,
            |_| true,
        );
    }

    // STEP8 a besoin du manifeste STEP7 le plus récent, même ancien. On garde
    // aussi le dernier rapport STEP8 comme trace de la dernière exécution.
    let latest_step7 = latest_report(&options.logs_dir, "finalized_projects_");
    let latest_step8 = latest_report(&options.logs_dir, "step8_run_");
    candidates.retain(|candidate| {
        candidate.kind != "rapport"
            || (Some(&candidate.path) != latest_step7.as_ref()
                && Some(&candidate.path) != latest_step8.as_ref())
    });

    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    candidates
}

fn latest_report(root: &Path, prefix: &str) -> Option<PathBuf> {
    WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(prefix) && name.ends_with(".json"))
        })
        .max_by_key(|entry| {
            entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
        })
        .map(|entry| entry.into_path())
}

fn collect_files<F>(
    root: &Path,
    now: SystemTime,
    threshold_hours: f64,
    kind: &str,
    candidates: &mut Vec<RetentionCandidate>,
    accept: F,
) where
    F: Fn(&str) -> bool,
{
    for entry in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str() else {
            continue;
        };
        if !accept(name) {
            continue;
        }
        let Some(age_hours) = age_hours(entry.path(), now) else {
            continue;
        };
        if age_hours <= threshold_hours {
            continue;
        }
        candidates.push(RetentionCandidate {
            bytes: entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
            path: entry.into_path(),
            kind: kind.into(),
            age_hours: round_hours(age_hours),
        });
    }
}

fn still_valid(options: &RetentionOptions, kind: &str, path: &Path) -> bool {
    if is_protected(path) {
        return false;
    }
    let (root, threshold_hours) = match kind {
        "projet_orphelin" => (&options.projects_dir, options.orphan_hours),
        "archive" => (&options.archives_dir, days_as_hours(options.archives_days)),
        "journal" => (&options.logs_dir, days_as_hours(options.logs_days)),
        "rapport" => (&options.logs_dir, days_as_hours(options.reports_days)),
        "telechargement" => (
            &options.downloads_dir,
            if options.downloads_managed {
                days_as_hours(options.downloads_days)
            } else {
                0.0
            },
        ),
        _ => return false,
    };
    if threshold_hours <= 0.0 {
        return false;
    }

    let Some(age_hours) = age_hours(path, SystemTime::now()) else {
        return false;
    };
    if age_hours <= threshold_hours {
        return false;
    }

    match kind {
        "projet_orphelin" => {
            path.parent() == Some(root)
                && is_real_directory(path)
                && !path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(protected_name)
        }
        "archive" => {
            path.parent() == Some(root) && is_real_directory(path) && is_archive_directory(path)
        }
        "journal" => {
            path.starts_with(root)
                && is_real_file(path)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(is_log_name)
        }
        "rapport" => {
            path.starts_with(root)
                && is_real_file(path)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(is_report_name)
        }
        "telechargement" => path.starts_with(root) && is_real_file(path),
        _ => false,
    }
}

fn remove_candidate(candidate: &RetentionCandidate) -> Result<()> {
    if candidate.kind == "projet_orphelin" || candidate.kind == "archive" {
        if !is_real_directory(&candidate.path) {
            anyhow::bail!("le chemin n'est plus un répertoire réel");
        }
        fs::remove_dir_all(&candidate.path)
            .with_context(|| format!("suppression de {}", candidate.path.display()))
    } else {
        if !is_real_file(&candidate.path) {
            anyhow::bail!("le chemin n'est plus un fichier réel");
        }
        fs::remove_file(&candidate.path)
            .with_context(|| format!("suppression de {}", candidate.path.display()))
    }
}

fn direct_directories(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_real_directory(path))
        .collect()
}

fn is_safe_root(root: &Path) -> bool {
    let Ok(canonical) = fs::canonicalize(root) else {
        return false;
    };
    let symlink = fs::symlink_metadata(root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(true);
    if symlink || canonical == Path::new("/") {
        return false;
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        if canonical == home || home.starts_with(&canonical) {
            return false;
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if canonical == cwd || cwd.starts_with(&canonical) {
            return false;
        }
    }
    let Some(name) = canonical.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if canonical.parent() == Some(Path::new("/")) {
        return !matches!(
            name,
            "home"
                | "mnt"
                | "tmp"
                | "var"
                | "usr"
                | "opt"
                | "srv"
                | "run"
                | "etc"
                | "proc"
                | "sys"
                | "dev"
                | "boot"
                | "bin"
                | "sbin"
                | "lib"
        );
    }
    true
}

fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir())
        .unwrap_or(false)
}

fn is_real_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

fn is_protected(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(protected_name)
}

fn protected_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    PROTECTED_NAMES.contains(&lower.as_str())
        || lower.starts_with("_temp_")
        || lower.starts_with('.')
}

fn is_archive_directory(path: &Path) -> bool {
    WalkDir::new(path)
        .max_depth(4)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .any(|name| {
            name == "metadata.json"
                || name.ends_with("_audio.json")
                || name.ends_with("_tracking.json")
                || name.ends_with("_scenes.csv")
        })
}

fn is_log_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".log") {
        return true;
    }
    lower.split_once(".log.").is_some_and(|(_, suffix)| {
        !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
    })
}

fn is_report_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".json")
        && (lower.starts_with("step8_run_") || lower.starts_with("finalized_projects_"))
}

fn age_hours(path: &Path, now: SystemTime) -> Option<f64> {
    let newest = newest_mtime(path)?;
    Some(
        now.duration_since(newest)
            .unwrap_or(Duration::ZERO)
            .as_secs_f64()
            / 3600.0,
    )
}

fn newest_mtime(path: &Path) -> Option<SystemTime> {
    let mut newest = fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()?;
    for entry in WalkDir::new(path).follow_links(false).into_iter().flatten() {
        if entry.depth() == 0 || entry.file_type().is_symlink() {
            continue;
        }
        if let Ok(metadata) = entry.metadata() {
            if let Ok(modified) = metadata.modified() {
                newest = newest.max(modified);
            }
        }
    }
    Some(newest)
}

fn directory_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .flatten()
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum()
}

fn days_as_hours(days: u64) -> f64 {
    days as f64 * 24.0
}

fn round_hours(hours: f64) -> f64 {
    (hours * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::time::SystemTime;

    struct Fixture {
        _root: tempfile::TempDir,
        options: RetentionOptions,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let projects_dir = root.path().join("projects");
            let archives_dir = root.path().join("archives");
            let logs_dir = root.path().join("logs");
            let downloads_dir = root.path().join("downloads");
            for dir in [&projects_dir, &archives_dir, &logs_dir, &downloads_dir] {
                fs::create_dir_all(dir).unwrap();
            }
            Self {
                options: RetentionOptions {
                    projects_dir,
                    archives_dir,
                    logs_dir,
                    downloads_dir,
                    orphan_hours: 48.0,
                    archives_days: 90,
                    logs_days: 30,
                    downloads_days: 30,
                    downloads_managed: true,
                    reports_days: 90,
                    dry_run: true,
                },
                _root: root,
            }
        }

        fn dir(&self, path: impl AsRef<Path>) -> PathBuf {
            self.options.projects_dir.join(path)
        }
    }

    fn set_age(path: &Path, days: u64) {
        let file = File::options().read(true).open(path).unwrap();
        let modified = SystemTime::now() - Duration::from_secs(days * 86_400);
        file.set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }

    fn set_tree_age(path: &Path, days: u64) {
        for entry in WalkDir::new(path)
            .contents_first(true)
            .into_iter()
            .flatten()
        {
            if !entry.file_type().is_symlink() {
                set_age(entry.path(), days);
            }
        }
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    #[test]
    fn plan_skips_recent_and_protected_projects() {
        let fixture = Fixture::new();
        let old = fixture.dir("ancien");
        write(&old.join("clip.mp4"), "video");
        write(&old.join("docs").join("clip_audio.json"), "{}");
        set_tree_age(&old, 10);

        let recent = fixture.dir("recent");
        write(&recent.join("clip.mp4"), "video");
        let protected = fixture.dir("archives");
        write(&protected.join("clip.mp4"), "video");
        set_tree_age(&protected, 10);
        let temp = fixture.dir("_temp_extraction");
        write(&temp.join("clip.mp4"), "video");
        set_tree_age(&temp, 10);

        let report = plan_retention(&fixture.options).unwrap();
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].path, old);
        assert!(report.candidates[0].bytes > 0);
        assert!(report.deleted.is_empty());
    }

    #[test]
    fn apply_deletes_only_eligible_candidates() {
        let fixture = Fixture::new();
        let old_project = fixture.dir("ancien");
        write(&old_project.join("clip.mp4"), "video");
        set_tree_age(&old_project, 10);
        let recent_project = fixture.dir("recent");
        write(&recent_project.join("clip.mp4"), "video");

        let old_log = fixture.options.logs_dir.join("step_STEP4.log");
        write(&old_log, "old");
        set_age(&old_log, 40);
        let recent_log = fixture.options.logs_dir.join("step_STEP5.log");
        write(&recent_log, "recent");

        let old_report = fixture
            .options
            .logs_dir
            .join("step8")
            .join("step8_run_20260101_120000.json");
        write(&old_report, "{}");
        set_age(&old_report, 120);
        let recent_report = fixture
            .options
            .logs_dir
            .join("step7")
            .join("finalized_projects_20261022_120000.json");
        write(&recent_report, "{}");
        let recent_step8_report = fixture
            .options
            .logs_dir
            .join("step8")
            .join("step8_run_20261022_120000.json");
        write(&recent_step8_report, "{}");

        let old_archive = fixture
            .options
            .archives_dir
            .join("250 Camille 2026-01-01_12-00-00");
        write(&old_archive.join("hash").join("clip_audio.json"), "{}");
        write(&old_archive.join("hash").join("metadata.json"), "{}");
        set_tree_age(&old_archive, 120);
        let unmarked_archive = fixture.options.archives_dir.join("stray");
        write(&unmarked_archive.join("notes.txt"), "keep");
        set_tree_age(&unmarked_archive, 120);

        let old_download = fixture.options.downloads_dir.join("video.zip");
        write(&old_download, "zip");
        set_age(&old_download, 40);
        let recent_download = fixture.options.downloads_dir.join("recent.zip");
        write(&recent_download, "zip");

        let mut options = fixture.options.clone();
        options.dry_run = false;
        let report = apply_retention(&options).unwrap();

        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert!(report.deleted.contains(&old_project));
        assert!(report.deleted.contains(&old_log));
        assert!(report.deleted.contains(&old_report));
        assert!(report.deleted.contains(&old_archive));
        assert!(report.deleted.contains(&old_download));
        assert!(!old_project.exists());
        assert!(!old_log.exists());
        assert!(!old_report.exists());
        assert!(!old_archive.exists());
        assert!(!old_download.exists());
        assert!(recent_project.exists());
        assert!(recent_log.exists());
        assert!(recent_report.exists());
        assert!(unmarked_archive.exists());
        assert!(recent_download.exists());
        assert!(report.bytes_reclaimed > 0);
    }

    #[test]
    fn dry_run_never_deletes() {
        let fixture = Fixture::new();
        let old_project = fixture.dir("ancien");
        write(&old_project.join("clip.mp4"), "video");
        set_tree_age(&old_project, 10);

        let report = apply_retention(&fixture.options).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.deleted.len(), 0);
        assert!(old_project.exists());
        assert_eq!(report.candidates.len(), 1);
    }

    #[test]
    fn refuses_unsafe_roots() {
        let fixture = Fixture::new();
        let mut options = fixture.options.clone();
        options.projects_dir = PathBuf::from("/");
        assert!(plan_retention(&options).is_err());
        options.projects_dir = std::env::current_dir().unwrap();
        assert!(plan_retention(&options).is_err());
    }

    #[test]
    fn keeps_latest_step7_manifest_and_step8_report() {
        let fixture = Fixture::new();
        let older = fixture
            .options
            .logs_dir
            .join("step7")
            .join("finalized_projects_20260101_120000.json");
        write(&older, "{}");
        set_age(&older, 150);
        let latest = fixture
            .options
            .logs_dir
            .join("step7")
            .join("finalized_projects_20260102_120000.json");
        write(&latest, "{}");
        set_age(&latest, 120);
        let report = fixture
            .options
            .logs_dir
            .join("step8")
            .join("step8_run_20260102_130000.json");
        write(&report, "{}");
        set_age(&report, 120);

        let mut options = fixture.options.clone();
        options.orphan_hours = 0.0;
        options.archives_days = 0;
        options.logs_days = 0;
        options.downloads_days = 0;
        options.reports_days = 90;
        options.dry_run = false;
        let outcome = apply_retention(&options).unwrap();
        assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
        assert!(!older.exists());
        assert!(latest.exists());
        assert!(report.exists());
    }

    #[test]
    fn disabled_thresholds_do_not_scan() {
        let fixture = Fixture::new();
        let old_project = fixture.dir("ancien");
        write(&old_project.join("clip.mp4"), "video");
        set_tree_age(&old_project, 10);

        let mut options = fixture.options.clone();
        options.orphan_hours = 0.0;
        options.archives_days = 0;
        options.logs_days = 0;
        options.downloads_days = 0;
        options.reports_days = 0;
        let report = plan_retention(&options).unwrap();
        assert!(report.candidates.is_empty());
    }
}
