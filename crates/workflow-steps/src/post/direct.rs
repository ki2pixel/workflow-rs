//! Génération directe des projets bruts du template SDM.
//!
//! Aucun modèle n'est chargé : la détection se limite au préfixe, au format
//! 1920x1080 25 fps et à ffprobe. Le projet brut existant n'est jamais écrasé.

use crate::post::ecproj::save_bytes;
use crate::post::run::PostOptions;
use crate::template::{self, TemplateProbe};
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use workflow_core::fsutil::write_atomic;

#[derive(Debug, Clone, Serialize)]
pub struct TemplateVideo {
    pub video: PathBuf,
    pub stem: String,
    pub probe: TemplateProbe,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct DirectSummary {
    pub folders: usize,
    pub templates: usize,
    pub created: usize,
    pub existing: usize,
    pub failed: usize,
    pub names: Vec<String>,
    /// Dossiers dont toutes les vidéos sont des templates directs valides.
    pub pure_folders: Vec<String>,
}

impl DirectSummary {
    pub fn merge(&mut self, other: DirectSummary) {
        self.folders += other.folders;
        self.templates += other.templates;
        self.created += other.created;
        self.existing += other.existing;
        self.failed += other.failed;
        self.names.extend(other.names);
        self.pure_folders.extend(other.pure_folders);
    }
}

/// Vidéos du template direct dans `<folder>/<docs_subfolder>`.
pub fn templates_in_folder(
    folder: &Path,
    docs_subfolder: &str,
    ffprobe: &Path,
    prefix: &str,
) -> Vec<TemplateVideo> {
    let docs = folder.join(docs_subfolder);
    let mut templates = Vec::new();
    for video in template::docs_videos(&docs) {
        if !template::matches_name(&video, prefix) {
            continue;
        }
        match template::probe_direct(ffprobe, &video, prefix) {
            Ok(Some(probe)) => {
                let Some(stem) = video
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_string)
                else {
                    continue;
                };
                templates.push(TemplateVideo { video, stem, probe });
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    "Template direct: sonde ignorée pour {} ({error:#})",
                    video.display()
                );
            }
        }
    }
    templates
}

pub fn ensure_for_folders(
    folders: &[String],
    options: &PostOptions,
    dry_run: bool,
) -> DirectSummary {
    let mut summary = DirectSummary::default();
    if !options.direct {
        return summary;
    }
    for folder in folders {
        let folder = Path::new(folder);
        let folder_summary = ensure_folder(
            folder,
            &options.docs_subfolder,
            &options.projects_subfolder,
            &options.ffprobe,
            &options.template_prefix,
            dry_run,
        );
        summary.merge(folder_summary);
    }
    summary
}

pub fn ensure_folder(
    folder: &Path,
    docs_subfolder: &str,
    projects_subfolder: &str,
    ffprobe: &Path,
    prefix: &str,
    dry_run: bool,
) -> DirectSummary {
    let docs = folder.join(docs_subfolder);
    let templates = templates_in_folder(folder, docs_subfolder, ffprobe, prefix);
    let template_names: std::collections::HashSet<&str> = templates
        .iter()
        .filter_map(|template| template.video.file_name().and_then(|name| name.to_str()))
        .collect();
    let pure = !templates.is_empty()
        && template::docs_videos(&docs).iter().all(|video| {
            video
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| template_names.contains(name))
        });
    let projects = folder.join(projects_subfolder);
    let mut summary = DirectSummary {
        folders: usize::from(!templates.is_empty()),
        templates: templates.len(),
        names: templates
            .iter()
            .filter_map(|template| {
                template
                    .video
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .collect(),
        pure_folders: if pure {
            vec![folder.to_string_lossy().to_string()]
        } else {
            Vec::new()
        },
        ..DirectSummary::default()
    };
    for template in templates {
        let raw = projects.join(format!("{}_9x16.ecproj", template.stem));
        if raw.is_file() {
            summary.existing += 1;
            continue;
        }
        if dry_run {
            summary.created += 1;
            continue;
        }
        match write_raw_project(&template, &docs, &projects, ffprobe) {
            Ok(()) => {
                summary.created += 1;
                tracing::info!("Template direct: brut créé {}", raw.display());
            }
            Err(error) => {
                summary.failed += 1;
                tracing::warn!(
                    "Template direct: création impossible pour {} ({error:#})",
                    raw.display()
                );
            }
        }
    }
    summary
}

fn write_raw_project(
    template: &TemplateVideo,
    docs: &Path,
    projects: &Path,
    ffprobe: &Path,
) -> Result<()> {
    fs::create_dir_all(projects).with_context(|| format!("création de {}", projects.display()))?;
    let logo_path = docs.join("sdm.png");
    let logo = logo_path.is_file().then(|| {
        let size = template::probe_image_size(ffprobe, &logo_path)
            .unwrap_or((crate::template::COMP_WIDTH, crate::template::COMP_HEIGHT));
        (logo_path.as_path(), size)
    });
    let doc = template::raw_project(&template.probe, &template.video, &template.stem, logo)?;
    let raw = projects.join(format!("{}_9x16.ecproj", template.stem));
    let bytes = save_bytes(&doc).context("sérialisation du brut direct")?;
    write_atomic(&raw, &bytes).with_context(|| format!("publication de {}", raw.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateProbe;
    use serde_json::Value;

    fn probe() -> TemplateProbe {
        TemplateProbe {
            width: 1920,
            height: 1080,
            pixel_aspect: 1.0,
            fps: 25.0,
            duration_s: 21.24,
            frames: 531,
            has_audio: true,
            codec: "h264".into(),
        }
    }

    #[test]
    fn raw_project_is_written_once() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().join("docs");
        let projects = dir.path().join("projets");
        fs::create_dir_all(&docs).unwrap();
        let video = docs.join("M6 - SDM - Leslie et Léo.mp4");
        fs::write(&video, b"video").unwrap();
        let template = TemplateVideo {
            video: video.clone(),
            stem: "M6 - SDM - Leslie et Léo".into(),
            probe: probe(),
        };
        write_raw_project(&template, &docs, &projects, Path::new("ffprobe")).unwrap();
        let raw = projects.join("M6 - SDM - Leslie et Léo_9x16.ecproj");
        assert!(raw.is_file());
        let first = fs::read(&raw).unwrap();
        let doc: Value = serde_json::from_slice(&first).unwrap();
        assert_eq!(
            doc["items"]["2"]["kind"]["layers"][0]["name"],
            "Segment 1 (00:00:00,000 - 00:00:21,240)"
        );
    }

    #[test]
    fn single_scene_csv_matches_step2_shape() {
        let csv = String::from_utf8(template::single_scene_csv_bytes(531).unwrap()).unwrap();
        assert_eq!(
            csv,
            "No,Timecode In,Timecode Out,Frame In,Frame Out\r\n\
             1,00:00:00.000,00:00:21.200,1,531\r\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn write_failure_is_counted() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("340 Camille");
        let docs = folder.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("M6 - SDM - Leslie et Léo.mp4"), b"video").unwrap();
        // Un fichier à la place du dossier `projets` force l'échec.
        fs::write(folder.join("projets"), b"obstacle").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let summary = ensure_folder(&folder, "docs", "projets", &ffprobe, "M6 - SDM - ", false);
        assert_eq!(summary.templates, 1);
        assert_eq!(summary.created, 0);
        assert_eq!(summary.failed, 1);
    }

    #[cfg(unix)]
    #[test]
    fn ensure_folder_creates_raw_once() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("340 Camille");
        let docs = folder.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("M6 - SDM - Leslie et Léo.mp4"), b"video").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let first = ensure_folder(&folder, "docs", "projets", &ffprobe, "M6 - SDM - ", false);
        assert_eq!(first.templates, 1);
        assert_eq!(first.created, 1);
        assert_eq!(first.existing, 0);
        assert!(folder
            .join("projets")
            .join("M6 - SDM - Leslie et Léo_9x16.ecproj")
            .is_file());
        let second = ensure_folder(&folder, "docs", "projets", &ffprobe, "M6 - SDM - ", false);
        assert_eq!(second.created, 0);
        assert_eq!(second.existing, 1);
        assert!(!docs.join("M6 - SDM - Leslie et Léo.csv").exists());
    }
}
