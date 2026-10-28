//! STEP7 : finalisation, archivage et publication vers `/mnt/cache`.
//!
//! Le comportement suit `finalize_and_copy.py` : archivage des analyses,
//! copie du projet, normalisation des fichiers dans `docs/`, manifeste lu par
//! STEP8, puis suppression optionnelle de la source.

use crate::template;
use anyhow::{bail, Context, Result};
use chrono::{Local, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

const SCENES_SUFFIX: &str = "_scenes.csv";
const AUDIO_SUFFIX: &str = "_audio.json";
const TRACKING_SUFFIX: &str = "_tracking.json";

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm"];
const MEDIA_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm", "png", "jpg", "jpeg"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FinalizeMode {
    Lenient,
    Strict,
    Videos,
}

impl FinalizeMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lenient => "lenient",
            Self::Strict => "strict",
            Self::Videos => "videos",
        }
    }
}

impl FromStr for FinalizeMode {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "lenient" => Ok(Self::Lenient),
            "strict" => Ok(Self::Strict),
            "videos" => Ok(Self::Videos),
            other => Err(format!("mode de finalisation inconnu: {other}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Step7Options {
    pub work_dir: PathBuf,
    pub output_dir: PathBuf,
    pub archives_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub fallback_output_dir: PathBuf,
    pub mode: FinalizeMode,
    pub restore_archives: bool,
    pub delete_source: bool,
    pub project_filter: Option<String>,
    /// `Some(prefix)` active la lecture directe des vidéos du template.
    pub template_prefix: Option<String>,
    pub ffprobe: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinalizedProject {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step7Report {
    pub output_dir: String,
    pub mode: String,
    pub projects: Vec<FinalizedProject>,
    pub failures: Vec<String>,
    pub manifest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArchiveSummary {
    pub project_name: String,
    pub processed: usize,
    pub copied: usize,
    pub details: Vec<Value>,
}

#[derive(Debug, Clone)]
struct SyntheticScenes {
    frames: u64,
    bytes: Vec<u8>,
}

type SyntheticMap = HashMap<PathBuf, SyntheticScenes>;

/// Finalise tous les projets prêts du répertoire de travail.
pub fn finalize_work_dir(options: &Step7Options) -> Result<Step7Report> {
    let output_dir = select_output_dir(
        &options.output_dir,
        &options.fallback_output_dir,
        &options.work_dir,
    )?;
    let projects = find_projects_to_finalize(
        &options.work_dir,
        options.mode,
        options.project_filter.as_deref(),
        options.template_prefix.as_deref(),
        &options.ffprobe,
    )?;
    let mut archive_cache = HashMap::new();
    let mut finalized = Vec::new();
    let mut failures = Vec::new();

    for project_dir in projects {
        let name = project_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("projet")
            .to_string();
        match finalize_project(
            &project_dir,
            &name,
            &output_dir,
            options,
            &mut archive_cache,
        ) {
            Ok(Some(destination)) => finalized.push(FinalizedProject {
                name,
                path: destination.to_string_lossy().to_string(),
            }),
            Ok(None) => failures.push(name),
            Err(error) => {
                tracing::error!("Finalisation échouée pour {name}: {error:#}");
                failures.push(name);
            }
        }
    }

    let manifest =
        write_finalized_manifest(&options.logs_dir, &output_dir, options.mode, &finalized)
            .map(|path| path.to_string_lossy().to_string())
            .map_err(|error| tracing::warn!("Manifeste STEP7 non écrit: {error}"))
            .ok();

    Ok(Step7Report {
        output_dir: output_dir.to_string_lossy().to_string(),
        mode: options.mode.as_str().to_string(),
        projects: finalized,
        failures,
        manifest,
    })
}

pub fn find_projects_to_finalize(
    work_dir: &Path,
    mode: FinalizeMode,
    project_filter: Option<&str>,
    template_prefix: Option<&str>,
    ffprobe: &Path,
) -> Result<Vec<PathBuf>> {
    let mut projects = Vec::new();
    for entry in
        fs::read_dir(work_dir).with_context(|| format!("lecture de {}", work_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with("_temp_") {
            continue;
        }
        if let Some(filter) = project_filter {
            if name != filter {
                continue;
            }
        }
        if project_is_ready(&path, mode, template_prefix, ffprobe)? {
            projects.push(path);
        }
    }
    Ok(projects)
}

fn project_is_ready(
    project_dir: &Path,
    mode: FinalizeMode,
    template_prefix: Option<&str>,
    ffprobe: &Path,
) -> Result<bool> {
    let videos = find_mp4_videos(project_dir);
    if videos.is_empty() {
        return Ok(false);
    }
    if mode == FinalizeMode::Videos {
        return Ok(true);
    }

    for video in videos {
        // Le mode `strict` conserve sa sémantique : scènes + tracking réels.
        // Seul `lenient` accepte une vidéo SDM valide comme projet prêt.
        if mode == FinalizeMode::Lenient {
            if let Some(prefix) = template_prefix {
                if template::matches_name(&video, prefix) {
                    match template::probe_direct(ffprobe, &video, prefix) {
                        Ok(Some(_)) => return Ok(true),
                        Ok(None) => {}
                        Err(error) => {
                            tracing::warn!(
                                "STEP7: sonde template impossible pour {}: {error:#}",
                                video.display()
                            );
                        }
                    }
                }
            }
        }
        let stem = video
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let scenes = find_named(project_dir, &format!("{stem}{SCENES_SUFFIX}"));
        let tracking = find_named(project_dir, &format!("{stem}{TRACKING_SUFFIX}"));
        let audio = find_named(project_dir, &format!("{stem}{AUDIO_SUFFIX}"));
        let ready = match mode {
            FinalizeMode::Strict => scenes.is_some() && tracking.is_some(),
            FinalizeMode::Lenient => scenes.is_some() || tracking.is_some() || audio.is_some(),
            FinalizeMode::Videos => true,
        };
        if ready {
            return Ok(true);
        }
    }
    Ok(false)
}

fn finalize_project(
    project_dir: &Path,
    project_name: &str,
    output_root: &Path,
    options: &Step7Options,
    archive_cache: &mut HashMap<String, PathBuf>,
) -> Result<Option<PathBuf>> {
    let synthetic = synthetic_scenes(project_dir, options);
    if !synthetic.is_empty() {
        tracing::info!(
            "STEP7: {} CSV mono-segment template à archiver pour {}",
            synthetic.len(),
            project_name
        );
    }
    if let Err(error) = archive_project_analysis(
        project_dir,
        project_name,
        &options.archives_dir,
        archive_cache,
        &synthetic,
    ) {
        tracing::warn!("Archivage des analyses ignoré pour {project_name}: {error:#}");
    }

    let mut output_project_dir = output_root.join(project_name);
    if output_project_dir.exists() {
        let _ = fs::remove_dir_all(&output_project_dir);
        if output_project_dir.exists() {
            output_project_dir = alternative_output_dir(&output_project_dir);
        }
    }

    copy_tree(project_dir, &output_project_dir)
        .with_context(|| format!("copie de {project_name}"))?;
    normalize_project_docs_structure(&output_project_dir)?;

    if options.restore_archives {
        if let Err(error) =
            restore_archived_analysis(project_name, &output_project_dir, &options.archives_dir)
        {
            tracing::warn!("Restauration des archives ignorée pour {project_name}: {error:#}");
        }
    }
    write_output_synthetic_scenes(&output_project_dir, &synthetic);

    if project_dir.starts_with(&options.archives_dir) {
        tracing::error!(
            "Refus de supprimer {}: sous ARCHIVES_DIR",
            project_dir.display()
        );
        return Ok(None);
    }
    let project_abs = project_dir
        .canonicalize()
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let archives_abs = options
        .archives_dir
        .canonicalize()
        .unwrap_or_else(|_| options.archives_dir.clone());
    if project_abs.starts_with(&archives_abs) {
        tracing::error!(
            "Refus de supprimer {}: sous ARCHIVES_DIR",
            project_dir.display()
        );
        return Ok(None);
    }
    if options.delete_source {
        if let Err(error) = fs::remove_dir_all(project_dir) {
            tracing::warn!(
                "Suppression de la source {} impossible: {error}",
                project_dir.display()
            );
        }
    }

    Ok(Some(output_project_dir))
}

fn select_output_dir(preferred: &Path, fallback: &Path, work_dir: &Path) -> Result<PathBuf> {
    if is_dir_writable(preferred) {
        return Ok(preferred.to_path_buf());
    }
    if is_dir_writable(fallback) {
        tracing::warn!(
            "Destination préférée non inscriptible, repli vers {}",
            fallback.display()
        );
        return Ok(fallback.to_path_buf());
    }
    let local = work_dir.join("_finalized_output");
    fs::create_dir_all(&local)
        .with_context(|| format!("création du repli local {}", local.display()))?;
    tracing::warn!(
        "Destinations préférées non inscriptibles, repli local vers {}",
        local.display()
    );
    Ok(local)
}

fn is_dir_writable(path: &Path) -> bool {
    if fs::create_dir_all(path).is_err() {
        return false;
    }
    let Ok(mut temp) = NamedTempFile::new_in(path) else {
        return false;
    };
    temp.write_all(b"writable-check").is_ok() && temp.flush().is_ok()
}

fn alternative_output_dir(existing: &Path) -> PathBuf {
    let parent = existing.parent().unwrap_or_else(|| Path::new("."));
    let name = existing
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("projet");
    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let mut candidate = parent.join(format!("{name}__finalized_{stamp}"));
    let mut index = 1;
    while candidate.exists() {
        candidate = parent.join(format!("{name}__finalized_{stamp}_{index}"));
        index += 1;
    }
    candidate
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in WalkDir::new(source).min_depth(1) {
        let entry = entry?;
        let relative = match entry.path().strip_prefix(source) {
            Ok(relative) => relative,
            Err(_) => continue,
        };
        let target = destination.join(relative);
        if entry.file_type().is_symlink() {
            anyhow::bail!(
                "symlink interdit pendant la finalisation STEP7: {}",
                entry.path().display()
            );
        } else if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)
                .with_context(|| format!("copie de {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn normalize_project_docs_structure(project_dir: &Path) -> Result<()> {
    let docs_dir = project_dir.join("docs");
    fs::create_dir_all(&docs_dir)?;

    let mut video_stems = HashSet::new();
    if let Ok(entries) = fs::read_dir(project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_video(&path) {
                if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                    video_stems.insert(stem.to_string());
                }
            }
        }
    }
    for video in find_videos(&docs_dir) {
        if let Some(stem) = video.file_stem().and_then(|stem| stem.to_str()) {
            video_stems.insert(stem.to_string());
        }
    }

    let entries: Vec<PathBuf> = fs::read_dir(project_dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.file_name().and_then(|name| name.to_str()) != Some("docs"))
        .collect();

    for path in entries {
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let should_move = MEDIA_EXTENSIONS.contains(&extension.as_str())
            || extension == "csv"
            || (extension == "json"
                && (name.ends_with(AUDIO_SUFFIX)
                    || name.ends_with(TRACKING_SUFFIX)
                    || video_stems.contains(stem)));
        if !should_move {
            continue;
        }
        let target = docs_dir.join(name);
        if target.exists() {
            if target.is_dir() {
                fs::remove_dir_all(&target)?;
            } else {
                fs::remove_file(&target)?;
            }
        }
        move_file(&path, &target)?;
    }
    Ok(())
}

fn move_file(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(source, target) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, target)?;
            fs::remove_file(source)?;
            Ok(())
        }
    }
}

fn synthetic_scenes(project_dir: &Path, options: &Step7Options) -> SyntheticMap {
    let Some(prefix) = options.template_prefix.as_deref() else {
        return HashMap::new();
    };
    let mut synthetic = HashMap::new();
    for video in find_videos(project_dir) {
        if !template::matches_name(&video, prefix) {
            continue;
        }
        let Some(stem) = video.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if find_named(project_dir, &format!("{stem}{SCENES_SUFFIX}")).is_some()
            || find_named(project_dir, &format!("{stem}.csv")).is_some()
        {
            continue;
        }
        match template::probe_direct(&options.ffprobe, &video, prefix) {
            Ok(Some(probe)) => match template::single_scene_csv_bytes(probe.frames) {
                Ok(bytes) => {
                    synthetic.insert(
                        video,
                        SyntheticScenes {
                            frames: probe.frames,
                            bytes,
                        },
                    );
                }
                Err(error) => {
                    tracing::warn!(
                        "STEP7: CSV mono-segment impossible pour {}: {error:#}",
                        video.display()
                    );
                }
            },
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    "STEP7: sonde template impossible pour {}: {error:#}",
                    video.display()
                );
            }
        }
    }
    synthetic
}

fn write_output_synthetic_scenes(output_project_dir: &Path, synthetic: &SyntheticMap) {
    if synthetic.is_empty() {
        return;
    }
    let docs = output_project_dir.join("docs");
    for (video, scenes) in synthetic {
        let Some(stem) = video.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if docs.join(format!("{stem}{SCENES_SUFFIX}")).is_file()
            || docs.join(format!("{stem}.csv")).is_file()
        {
            continue;
        }
        let target = docs.join(format!("{stem}.csv"));
        if let Err(error) = template::write_single_scene_csv(&target, scenes.frames) {
            tracing::warn!(
                "STEP7: écriture du CSV template impossible dans {}: {error:#}",
                target.display()
            );
        }
    }
}

fn archive_project_analysis(
    project_dir: &Path,
    project_name: &str,
    archives_dir: &Path,
    cache: &mut HashMap<String, PathBuf>,
    synthetic: &SyntheticMap,
) -> Result<ArchiveSummary> {
    if !project_dir.is_dir() {
        bail!("dossier projet introuvable: {}", project_dir.display());
    }
    let project_archive = if let Some(path) = cache.get(project_name) {
        path.clone()
    } else {
        let path = archives_dir.join(format!(
            "{project_name} {}",
            Utc::now().format("%Y-%m-%d_%H-%M-%S")
        ));
        fs::create_dir_all(&path)?;
        cache.insert(project_name.to_string(), path.clone());
        path
    };

    let videos = find_videos(project_dir);
    let mut details = Vec::new();
    let mut copied_count = 0;
    for video in &videos {
        let stem = video
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        let scenes = find_named(project_dir, &format!("{stem}{SCENES_SUFFIX}"))
            .or_else(|| find_named(project_dir, &format!("{stem}.csv")));
        let audio = find_named(project_dir, &format!("{stem}{AUDIO_SUFFIX}"));
        let tracking = find_named(project_dir, &format!("{stem}{TRACKING_SUFFIX}"))
            .or_else(|| find_named(project_dir, &format!("{stem}.json")));
        let synthetic_scenes = synthetic.get(video);
        let mut scenes_origin = if scenes.is_some() {
            json!("step2")
        } else {
            Value::Null
        };

        let Some(video_hash) = compute_video_hash(video) else {
            details.push(json!({
                "video": video.file_name().and_then(|name| name.to_str()).unwrap_or(""),
                "copied": {"scenes": false, "audio": false, "tracking": false, "archived": false},
                "archive_dir": Value::Null
            }));
            continue;
        };
        let video_archive = project_archive.join(&video_hash);
        fs::create_dir_all(&video_archive)?;

        let scenes_copied = if let Some(source) = scenes.as_deref() {
            copy_artifact(Some(source), &video_archive, stem, SCENES_SUFFIX)
        } else if let Some(synthetic) = synthetic_scenes {
            let target = video_archive.join(format!("{stem}{SCENES_SUFFIX}"));
            match fs::write(&target, &synthetic.bytes) {
                Ok(()) => {
                    scenes_origin = json!("template");
                    Some(target.to_string_lossy().to_string())
                }
                Err(error) => {
                    tracing::warn!(
                        "STEP7: écriture du CSV template impossible dans {}: {error}",
                        target.display()
                    );
                    None
                }
            }
        } else {
            None
        };
        let audio_copied = copy_artifact(audio.as_deref(), &video_archive, stem, AUDIO_SUFFIX);
        let tracking_copied =
            copy_artifact(tracking.as_deref(), &video_archive, stem, TRACKING_SUFFIX);
        let copied = scenes_copied.is_some() || audio_copied.is_some() || tracking_copied.is_some();
        if copied {
            copied_count += 1;
        }

        let metadata = json!({
            "video_path": video.to_string_lossy(),
            "video_stem": stem,
            "video_hash": video_hash,
            "created_at": Utc::now().to_rfc3339_opts(SecondsFormat::Micros, false),
            "scenes_origin": scenes_origin.clone(),
            "copied": {
                "scenes": scenes_copied,
                "audio": audio_copied,
                "tracking": tracking_copied,
            }
        });
        fs::write(
            video_archive.join("metadata.json"),
            serde_json::to_vec_pretty(&metadata)?,
        )?;

        details.push(json!({
            "video": video.strip_prefix(project_dir).unwrap_or(video).to_string_lossy(),
            "scenes_origin": scenes_origin,
            "copied": {
                "scenes": scenes_copied.is_some(),
                "audio": audio.is_some(),
                "tracking": tracking.is_some(),
                "archived": true,
            },
            "archive_dir": video_archive.to_string_lossy(),
        }));
    }

    Ok(ArchiveSummary {
        project_name: project_name.to_string(),
        processed: videos.len(),
        copied: copied_count,
        details,
    })
}

fn copy_artifact(
    source: Option<&Path>,
    archive: &Path,
    stem: &str,
    suffix: &str,
) -> Option<String> {
    let source = source?;
    if !source.exists() {
        return None;
    }
    let target = archive.join(format!("{stem}{suffix}"));
    fs::copy(source, &target).ok()?;
    Some(target.to_string_lossy().to_string())
}

fn restore_archived_analysis(
    project_name: &str,
    output_project: &Path,
    archives_dir: &Path,
) -> Result<()> {
    let docs_dir = output_project.join("docs");
    fs::create_dir_all(&docs_dir)?;
    let mut videos = find_videos(&docs_dir);
    videos.extend(find_videos(output_project));
    videos.sort();
    videos.dedup();

    for video in videos {
        let target_dir = docs_dir.clone();
        let stem = video
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");
        for suffix in [SCENES_SUFFIX, AUDIO_SUFFIX, TRACKING_SUFFIX] {
            if let Some(archived) = find_analysis_file(project_name, &video, suffix, archives_dir) {
                let target = target_dir.join(format!("{stem}{suffix}"));
                if let Err(error) = fs::copy(&archived, &target) {
                    tracing::warn!(
                        "Restauration de {} vers {} impossible: {error}",
                        archived.display(),
                        target.display()
                    );
                }
            }
        }
    }
    Ok(())
}

fn find_analysis_file(
    project_name: &str,
    video: &Path,
    suffix: &str,
    archives_dir: &Path,
) -> Option<PathBuf> {
    let stem = video.file_stem().and_then(|stem| stem.to_str())?;
    if let Some(hash) = compute_video_hash(video) {
        for project_archive in matching_archive_dirs(archives_dir, project_name) {
            let candidate = project_archive.join(&hash).join(format!("{stem}{suffix}"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    for project_archive in matching_archive_dirs(archives_dir, project_name) {
        for entry in WalkDir::new(project_archive)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if entry.file_type().is_file()
                && entry.file_name().to_string_lossy() == format!("{stem}{suffix}")
            {
                return Some(entry.into_path());
            }
        }
    }
    None
}

fn matching_archive_dirs(archives_dir: &Path, project_name: &str) -> Vec<PathBuf> {
    let prefix = format!("{project_name} ");
    let mut matches: Vec<PathBuf> = fs::read_dir(archives_dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| {
                    path.is_dir()
                        && path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name == project_name || name.starts_with(&prefix))
                })
                .collect()
        })
        .unwrap_or_default();
    matches.sort();
    matches.reverse();
    matches
}

fn write_finalized_manifest(
    logs_dir: &Path,
    output_dir: &Path,
    mode: FinalizeMode,
    projects: &[FinalizedProject],
) -> Result<PathBuf> {
    let step_dir = logs_dir.join("step7");
    fs::create_dir_all(&step_dir)?;
    let path = step_dir.join(format!(
        "finalized_projects_{}.json",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    let payload = json!({
        "schema": 1,
        "timestamp": Local::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
        "output_dir": output_dir.to_string_lossy(),
        "mode": mode.as_str(),
        "projects": projects,
    });
    workflow_core::fsutil::write_atomic(&path, &serde_json::to_vec_pretty(&payload)?)?;
    Ok(path)
}

fn find_videos(root: &Path) -> Vec<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file() && is_video(entry.path()))
        .map(|entry| entry.into_path())
        .collect()
}

fn find_mp4_videos(root: &Path) -> Vec<PathBuf> {
    find_videos(root)
        .into_iter()
        .filter(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
        })
        .collect()
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            VIDEO_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

fn find_named(root: &Path, name: &str) -> Option<PathBuf> {
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .find(|entry| entry.file_type().is_file() && entry.file_name() == OsStr::new(name))
        .map(|entry| entry.into_path())
}

fn compute_video_hash(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
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

    fn project(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        write(&path.join("clip.mp4"), b"video");
        write(&path.join("clip.csv"), b"scenes");
        write(&path.join("clip_scenes.csv"), b"scenes");
        write(&path.join("clip_audio.json"), b"{}");
        write(&path.join("clip_tracking.json"), b"{}");
        path
    }

    #[test]
    fn finds_projects_by_mode() {
        let dir = tempfile::tempdir().unwrap();
        project(dir.path(), "ready");
        let audio_only = dir.path().join("audio-only");
        write(&audio_only.join("clip.mp4"), b"video");
        write(&audio_only.join("clip_audio.json"), b"{}");
        let mov_only = dir.path().join("mov-only");
        write(&mov_only.join("clip.mov"), b"video");
        write(&mov_only.join("clip_tracking.json"), b"{}");
        fs::create_dir_all(dir.path().join("_temp_skip")).unwrap();

        let lenient = find_projects_to_finalize(
            dir.path(),
            FinalizeMode::Lenient,
            None,
            None,
            Path::new("ffprobe"),
        )
        .unwrap();
        assert_eq!(lenient.len(), 2);
        let strict = find_projects_to_finalize(
            dir.path(),
            FinalizeMode::Strict,
            None,
            None,
            Path::new("ffprobe"),
        )
        .unwrap();
        assert_eq!(strict.len(), 1);
        assert_eq!(strict[0].file_name().unwrap(), "ready");
        let videos = find_projects_to_finalize(
            dir.path(),
            FinalizeMode::Videos,
            None,
            None,
            Path::new("ffprobe"),
        )
        .unwrap();
        assert_eq!(videos.len(), 2);
    }

    #[test]
    fn normalizes_docs_and_keeps_other_json() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("projet");
        write(&project.join("clip.mp4"), b"video");
        write(&project.join("clip.csv"), b"csv");
        write(&project.join("clip_audio.json"), b"audio");
        write(&project.join("clip.json"), b"tracking");
        write(&project.join("notes.json"), b"notes");

        normalize_project_docs_structure(&project).unwrap();
        assert!(project.join("docs/clip.mp4").exists());
        assert!(project.join("docs/clip.csv").exists());
        assert!(project.join("docs/clip_audio.json").exists());
        assert!(project.join("docs/clip.json").exists());
        assert!(project.join("notes.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn given_symlink_when_copying_tree_then_rejected() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        let destination = dir.path().join("destination");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("clip.mp4"), b"ok").unwrap();
        symlink(source.join("clip.mp4"), source.join("lien.mp4")).unwrap();
        assert!(copy_tree(&source, &destination).is_err());
    }

    #[test]
    fn archives_hash_and_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), "projet");
        let archives = dir.path().join("archives");
        let mut cache = HashMap::new();
        let summary =
            archive_project_analysis(&project, "projet", &archives, &mut cache, &HashMap::new())
                .unwrap();
        assert_eq!(summary.processed, 1);
        assert_eq!(summary.copied, 1);
        let archive_project = matching_archive_dirs(&archives, "projet").remove(0);
        let hash = compute_video_hash(&project.join("clip.mp4")).unwrap();
        assert!(archive_project.join(&hash).join("clip_scenes.csv").exists());
        assert!(archive_project.join(&hash).join("clip_audio.json").exists());
        assert!(archive_project
            .join(&hash)
            .join("clip_tracking.json")
            .exists());
        let metadata: Value = serde_json::from_slice(
            &fs::read(archive_project.join(&hash).join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(metadata["video_hash"], hash);
    }

    #[test]
    fn falls_back_when_preferred_destination_is_not_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let preferred = dir.path().join("preferred");
        write(&preferred, b"not a directory");
        let fallback = dir.path().join("fallback");
        let selected = select_output_dir(&preferred, &fallback, dir.path()).unwrap();
        assert_eq!(selected, fallback);
        assert!(fallback.is_dir());
    }

    #[test]
    fn restores_archived_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let project = project(dir.path(), "projet");
        let archives = dir.path().join("archives");
        let mut cache = HashMap::new();
        archive_project_analysis(&project, "projet", &archives, &mut cache, &HashMap::new())
            .unwrap();

        let output = dir.path().join("out");
        fs::create_dir_all(output.join("docs")).unwrap();
        fs::copy(
            project.join("clip.mp4"),
            output.join("docs").join("clip.mp4"),
        )
        .unwrap();
        restore_archived_analysis("projet", &output, &archives).unwrap();
        assert!(output.join("docs/clip_scenes.csv").exists());
        assert!(output.join("docs/clip_audio.json").exists());
        assert!(output.join("docs/clip_tracking.json").exists());
    }

    #[test]
    fn writes_empty_manifest_when_nothing_is_ready() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        fs::create_dir_all(&work).unwrap();
        let options = Step7Options {
            work_dir: work,
            output_dir: dir.path().join("out"),
            archives_dir: dir.path().join("archives"),
            logs_dir: dir.path().join("logs"),
            fallback_output_dir: dir.path().join("fallback"),
            mode: FinalizeMode::Lenient,
            restore_archives: false,
            delete_source: false,
            project_filter: None,
            template_prefix: None,
            ffprobe: PathBuf::from("ffprobe"),
        };
        let report = finalize_work_dir(&options).unwrap();
        assert!(report.projects.is_empty());
        assert!(report.failures.is_empty());
        let manifest = PathBuf::from(report.manifest.unwrap());
        let payload: Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
        assert_eq!(payload["projects"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn finalizes_without_deleting_source() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        let project = project(&work, "projet");
        let output = dir.path().join("out");
        let options = Step7Options {
            work_dir: work,
            output_dir: output.clone(),
            archives_dir: dir.path().join("archives"),
            logs_dir: dir.path().join("logs"),
            fallback_output_dir: dir.path().join("fallback"),
            mode: FinalizeMode::Lenient,
            restore_archives: false,
            delete_source: false,
            project_filter: None,
            template_prefix: None,
            ffprobe: PathBuf::from("ffprobe"),
        };
        let report = finalize_work_dir(&options).unwrap();
        assert_eq!(report.projects.len(), 1);
        assert!(project.exists());
        assert!(output.join("projet/docs/clip.mp4").exists());
        assert!(output.join("projet/docs/clip.csv").exists());
        let manifest = PathBuf::from(report.manifest.unwrap());
        let payload: Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
        assert_eq!(payload["projects"][0]["name"], "projet");
    }

    #[test]
    fn deletes_source_when_requested() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        let project = project(&work, "projet");
        let options = Step7Options {
            work_dir: work,
            output_dir: dir.path().join("out"),
            archives_dir: dir.path().join("archives"),
            logs_dir: dir.path().join("logs"),
            fallback_output_dir: dir.path().join("fallback"),
            mode: FinalizeMode::Lenient,
            restore_archives: false,
            delete_source: true,
            project_filter: None,
            template_prefix: None,
            ffprobe: PathBuf::from("ffprobe"),
        };
        let report = finalize_work_dir(&options).unwrap();
        assert_eq!(report.projects.len(), 1);
        assert!(!project.exists());
    }

    #[cfg(unix)]
    #[test]
    fn archives_synthetic_scene_csv_for_direct_template() {
        let dir = tempfile::tempdir().unwrap();
        let work = dir.path().join("work");
        let project = work.join("340 Camille");
        let docs = project.join("docs");
        fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("M6 - SDM - Leslie et Léo.mp4"), b"video").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let strict = find_projects_to_finalize(
            &work,
            FinalizeMode::Strict,
            None,
            Some("M6 - SDM - "),
            &ffprobe,
        )
        .unwrap();
        assert!(strict.is_empty(), "strict garde scènes + tracking réels");
        let lenient = find_projects_to_finalize(
            &work,
            FinalizeMode::Lenient,
            None,
            Some("M6 - SDM - "),
            &ffprobe,
        )
        .unwrap();
        assert_eq!(lenient.len(), 1);
        let archives = dir.path().join("archives");
        let output = dir.path().join("out");
        let options = Step7Options {
            work_dir: work,
            output_dir: output.clone(),
            archives_dir: archives.clone(),
            logs_dir: dir.path().join("logs"),
            fallback_output_dir: dir.path().join("fallback"),
            mode: FinalizeMode::Lenient,
            restore_archives: false,
            delete_source: false,
            project_filter: None,
            template_prefix: Some("M6 - SDM - ".into()),
            ffprobe,
        };
        let report = finalize_work_dir(&options).unwrap();
        assert_eq!(report.projects.len(), 1);
        let archive_project = matching_archive_dirs(&archives, "340 Camille").remove(0);
        let video = project.join("docs/M6 - SDM - Leslie et Léo.mp4");
        let hash = compute_video_hash(&video).unwrap();
        let csv = archive_project
            .join(&hash)
            .join("M6 - SDM - Leslie et Léo_scenes.csv");
        assert_eq!(
            fs::read_to_string(&csv).unwrap(),
            "No,Timecode In,Timecode Out,Frame In,Frame Out\r\n\
             1,00:00:00.000,00:00:21.200,1,531\r\n"
        );
        let metadata: Value = serde_json::from_slice(
            &fs::read(archive_project.join(&hash).join("metadata.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(metadata["scenes_origin"], "template");
        assert!(metadata["copied"]["scenes"].is_string());
        assert!(metadata["copied"]["audio"].is_null());
        assert!(output
            .join("340 Camille/docs/M6 - SDM - Leslie et Léo.csv")
            .is_file());
        assert!(!docs.join("M6 - SDM - Leslie et Léo.csv").exists());
    }
}
