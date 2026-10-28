//! Exécution de STEP5 sur une arborescence de projets.
//!
//! L'orchestrateur ne réimplémente pas la réduction : il appelle ce mode
//! batch, qui réutilise exactement les mêmes fonctions que la CLI unitaire.
//! Les lignes de progression reproduisent celles du script Python.

use super::audio::reduce_audio_file;
use super::video::{reduce_video_file, VideoOptions};
use crate::StepError;
use std::fs;
use std::path::{Path, PathBuf};

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm"];
const AUDIO_SUFFIX: &str = "_audio.json";
const TRACKING_SUFFIX: &str = "_tracking.json";

#[derive(Debug, Clone)]
pub struct Step5BatchOptions {
    pub work_dir: PathBuf,
    pub keyword: String,
    pub docs_subfolder: String,
    pub tracking_analytics: bool,
    pub expression_summary: bool,
    pub expression_keys: Vec<String>,
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
    pub ffprobe: PathBuf,
    /// `Some(prefix)` ignore les vidéos du template direct.
    pub template_prefix: Option<String>,
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Step5BatchReport {
    pub projects: usize,
    pub files: usize,
    pub skipped_template: usize,
    pub failures: Vec<String>,
}

impl Step5BatchReport {
    pub fn success(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Réduit tous les projets filtrés par mot-clé.
///
/// Une erreur sur un fichier est journalisée et n'empêche pas les autres
/// fichiers d'être traités ; le rapport final porte la liste des échecs.
pub fn run_step5_batch(options: &Step5BatchOptions) -> Result<Step5BatchReport, StepError> {
    let mut report = Step5BatchReport::default();
    let projects = matching_projects(&options.work_dir, &options.keyword);
    println!("TOTAL_JSON_TO_REDUCE: {}", projects.len());

    for (index, project) in projects.iter().enumerate() {
        println!(
            "REDUCING_JSON: {}/{}: {}",
            index + 1,
            projects.len(),
            project_name(project)
        );
        let docs = project.join(&options.docs_subfolder);
        if !docs.is_dir() {
            eprintln!(
                "Avertissement: le dossier '{}' est absent de {}",
                options.docs_subfolder,
                project.display()
            );
            continue;
        }

        let videos = videos_in(&docs, options);
        let (videos, skipped_template) = videos;
        report.skipped_template += skipped_template;
        for (video_index, video) in videos.iter().enumerate() {
            let Some(stem) = video.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let tracking = tracking_path(&docs, stem);
            let Some((tracking_in, tracking_out)) = tracking else {
                eprintln!("Tracking JSON introuvable pour '{}'.", video.display());
                continue;
            };

            let audio = docs.join(format!("{stem}{AUDIO_SUFFIX}"));
            println!(
                "INTERNAL_PROGRESS: {}/{} items ({}%) - {}",
                video_index + 1,
                videos.len(),
                ((video_index + 1) as f64 / videos.len() as f64 * 100.0).round() as u32,
                video.display()
            );

            let audio_meta = if audio.exists() {
                match reduce_audio_file(&audio, &audio) {
                    Ok(meta) => Some(meta),
                    Err(error) => {
                        let message = format!("audio {}: {error}", audio.display());
                        eprintln!("ERREUR: {message}");
                        report.failures.push(message);
                        None
                    }
                }
            } else {
                eprintln!(
                    "Avertissement: fichier audio '{}' manquant pour '{}'.",
                    audio.display(),
                    video.display()
                );
                None
            };

            let video_options = VideoOptions {
                tracking_analytics: options.tracking_analytics,
                expression_summary: options.expression_summary,
                expression_keys: options.expression_keys.clone(),
                audio: audio_meta,
            };
            match reduce_video_file(&tracking_in, &tracking_out, &video_options) {
                Ok(Some(_)) => report.files += 1,
                Ok(None) => {
                    let message = format!(
                        "tracking {}: aucun tableau de frames",
                        tracking_in.display()
                    );
                    eprintln!("AVERTISSEMENT: {message}");
                    report.failures.push(message);
                }
                Err(error) => {
                    let message = format!("vidéo {}: {error}", tracking_in.display());
                    eprintln!("ERREUR: {message}");
                    report.failures.push(message);
                }
            }
        }
        report.projects += 1;
        println!(
            "Succès: réduction JSON terminée pour {}",
            project_name(project)
        );
    }

    Ok(report)
}

fn matching_projects(work_dir: &Path, keyword: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(work_dir) else {
        return Vec::new();
    };
    let mut projects: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| keyword.is_empty() || path.to_string_lossy().contains(keyword))
        .collect();
    projects.sort();
    projects
}

fn videos_in(docs: &Path, options: &Step5BatchOptions) -> (Vec<PathBuf>, usize) {
    let Ok(entries) = fs::read_dir(docs) else {
        return (Vec::new(), 0);
    };
    let mut skipped_template = 0;
    let mut videos = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                VIDEO_EXTENSIONS
                    .iter()
                    .any(|candidate| extension.eq_ignore_ascii_case(candidate))
            })
        {
            continue;
        }
        if let Some(prefix) = options.template_prefix.as_deref() {
            if crate::template::matches_name(&path, prefix)
                && matches!(
                    crate::template::probe_direct(&options.ffprobe, &path, prefix),
                    Ok(Some(_))
                )
            {
                eprintln!("Template direct ignoré: {}", path.display());
                skipped_template += 1;
                continue;
            }
        }
        let preserved = crate::step2::is_preserved_media_with(
            &path,
            options.skip_preserved_mov,
            &options.preserved_mov_names,
            &options.ffprobe,
        );
        if preserved {
            eprintln!(
                "Média préservé ignoré (alpha/liste noire): {}",
                path.display()
            );
            continue;
        }
        videos.push(path);
    }
    videos.sort();
    (videos, skipped_template)
}

fn tracking_path(docs: &Path, stem: &str) -> Option<(PathBuf, PathBuf)> {
    let reduced = docs.join(format!("{stem}{TRACKING_SUFFIX}"));
    if reduced.exists() {
        return Some((reduced.clone(), reduced));
    }
    let raw = docs.join(format!("{stem}.json"));
    if raw.exists() {
        return Some((raw, docs.join(format!("{stem}{TRACKING_SUFFIX}"))));
    }
    None
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("?")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_json(path: &Path) {
        let mut file = fs::File::create(path).unwrap();
        writeln!(
            file,
            "{{\"fps\":25,\"total_frames\":2,\"frames\":[{{\"frame\":1,\"tracked_objects\":[]}}]}}"
        )
        .unwrap();
    }

    #[test]
    fn batch_skips_projects_without_tracking_without_failing() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("100 Camille").join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("clip.mp4"), b"video").unwrap();
        let options = Step5BatchOptions {
            work_dir: dir.path().to_path_buf(),
            keyword: "Camille".into(),
            docs_subfolder: "docs".into(),
            tracking_analytics: false,
            expression_summary: false,
            expression_keys: vec![],
            skip_preserved_mov: true,
            preserved_mov_names: vec![],
            ffprobe: PathBuf::from("ffprobe"),
            template_prefix: None,
        };
        let report = run_step5_batch(&options).unwrap();
        assert_eq!(report.projects, 1);
        assert_eq!(report.files, 0);
        assert!(report.success());
    }

    #[cfg(unix)]
    #[test]
    fn batch_skips_direct_templates_without_tracking() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("100 Camille").join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("M6 - SDM - clip.mp4"), b"video").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let options = Step5BatchOptions {
            work_dir: dir.path().to_path_buf(),
            keyword: "Camille".into(),
            docs_subfolder: "docs".into(),
            tracking_analytics: false,
            expression_summary: false,
            expression_keys: vec![],
            skip_preserved_mov: true,
            preserved_mov_names: vec![],
            ffprobe,
            template_prefix: Some("M6 - SDM - ".into()),
        };
        let report = run_step5_batch(&options).unwrap();
        assert_eq!(report.skipped_template, 1);
        assert_eq!(report.files, 0);
        assert!(report.success());
    }

    #[test]
    fn batch_skips_preserved_mov_even_with_tracking() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("100 Camille").join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("m6+.mov"), b"video").unwrap();
        write_json(&docs.join("m6+.json"));
        let options = Step5BatchOptions {
            work_dir: dir.path().to_path_buf(),
            keyword: "Camille".into(),
            docs_subfolder: "docs".into(),
            tracking_analytics: false,
            expression_summary: false,
            expression_keys: vec![],
            skip_preserved_mov: true,
            preserved_mov_names: vec!["m6+.mov".into()],
            ffprobe: PathBuf::from("/nonexistent/ffprobe"),
            template_prefix: None,
        };
        let report = run_step5_batch(&options).unwrap();
        assert_eq!(report.files, 0);
        assert!(!docs.join("m6+_tracking.json").exists());
    }

    #[test]
    fn batch_reduces_raw_tracking_to_tracking_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("100 Camille").join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("clip.mp4"), b"video").unwrap();
        write_json(&docs.join("clip.json"));
        let options = Step5BatchOptions {
            work_dir: dir.path().to_path_buf(),
            keyword: "Camille".into(),
            docs_subfolder: "docs".into(),
            tracking_analytics: false,
            expression_summary: false,
            expression_keys: vec![],
            skip_preserved_mov: true,
            preserved_mov_names: vec![],
            ffprobe: PathBuf::from("ffprobe"),
            template_prefix: None,
        };
        let report = run_step5_batch(&options).unwrap();
        assert_eq!(report.files, 1);
        assert!(docs.join("clip_tracking.json").exists());
    }
}
