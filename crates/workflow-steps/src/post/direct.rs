//! Génération directe des projets bruts du template SDM.
//!
//! Aucun modèle n'est chargé : la détection se limite au préfixe, au format
//! 1920x1080 25 fps et à ffprobe. Le projet brut existant n'est jamais écrasé.

use crate::post::ecproj::save_bytes;
use crate::post::run::PostOptions;
use crate::template::{self, Bypass, BypassMode, TemplateProbe};
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
    pub archive_only: usize,
    pub archive_only_folders: usize,
    pub created: usize,
    pub existing: usize,
    pub failed: usize,
    /// Tous les noms bypassés, injectés dans `videoSkipNames`.
    pub names: Vec<String>,
    /// Noms des vidéos archive-only, pour filtrer le post-traitement.
    pub archive_only_names: Vec<String>,
    /// Dossiers dont toutes les vidéos sont bypassées (SDM ou archive-only).
    pub pure_folders: Vec<String>,
}

impl DirectSummary {
    pub fn merge(&mut self, other: DirectSummary) {
        self.folders += other.folders;
        self.templates += other.templates;
        self.archive_only += other.archive_only;
        self.archive_only_folders += other.archive_only_folders;
        self.created += other.created;
        self.existing += other.existing;
        self.failed += other.failed;
        self.names.extend(other.names);
        self.archive_only_names.extend(other.archive_only_names);
        self.pure_folders.extend(other.pure_folders);
    }
}

/// Résultat du scan d'un dossier `docs`.
#[derive(Debug, Default)]
pub struct FolderScan {
    pub templates: Vec<TemplateVideo>,
    pub archive_only: Vec<PathBuf>,
}

/// Sépare les vidéos SDM valides des vidéos archive-only.
pub fn scan_folder(
    folder: &Path,
    docs_subfolder: &str,
    ffprobe: &Path,
    bypass: &Bypass,
) -> FolderScan {
    let docs = folder.join(docs_subfolder);
    let mut scan = FolderScan::default();
    for video in template::docs_videos(&docs) {
        match bypass.mode_for_name(&video) {
            Some(BypassMode::ArchiveOnly) => scan.archive_only.push(video),
            Some(BypassMode::Template) => match bypass.probe(ffprobe, &video) {
                Ok(Some((_, probe))) => {
                    let Some(stem) = video
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(str::to_string)
                    else {
                        continue;
                    };
                    scan.templates.push(TemplateVideo { video, stem, probe });
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        "Template direct: sonde ignorée pour {} ({error:#})",
                        video.display()
                    );
                }
            },
            None => {}
        }
    }
    scan
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
    let bypass = Bypass::new(&options.template_prefix, &options.archive_only_prefixes);
    if bypass.is_empty() {
        return summary;
    }
    for folder in folders {
        let folder = Path::new(folder);
        let folder_summary = ensure_folder(
            folder,
            &options.docs_subfolder,
            &options.projects_subfolder,
            &options.ffprobe,
            &bypass,
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
    bypass: &Bypass,
    dry_run: bool,
) -> DirectSummary {
    let docs = folder.join(docs_subfolder);
    let scan = scan_folder(folder, docs_subfolder, ffprobe, bypass);
    let template_names: std::collections::HashSet<&str> = scan
        .templates
        .iter()
        .filter_map(|template| template.video.file_name().and_then(|name| name.to_str()))
        .collect();
    let archive_names = scan
        .archive_only
        .iter()
        .filter_map(|video| video.file_name().and_then(|name| name.to_str()))
        .collect::<std::collections::HashSet<&str>>();
    let has_bypass = !scan.templates.is_empty() || !scan.archive_only.is_empty();
    let pure = has_bypass
        && template::docs_videos(&docs).iter().all(|video| {
            video
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| template_names.contains(name) || archive_names.contains(name))
        });
    let archive_only_names: Vec<String> = scan
        .archive_only
        .iter()
        .filter_map(|video| {
            video
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .collect();
    let mut names: Vec<String> = scan
        .templates
        .iter()
        .filter_map(|template| {
            template
                .video
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .collect();
    names.extend(archive_only_names.iter().cloned());
    let projects = folder.join(projects_subfolder);
    let mut summary = DirectSummary {
        folders: usize::from(has_bypass),
        templates: scan.templates.len(),
        archive_only: scan.archive_only.len(),
        archive_only_folders: usize::from(!scan.archive_only.is_empty()),
        names,
        archive_only_names,
        pure_folders: if pure {
            vec![folder.to_string_lossy().to_string()]
        } else {
            Vec::new()
        },
        ..DirectSummary::default()
    };
    for template in scan.templates {
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
        let bypass = Bypass::new("M6 - SDM - ", &[]);
        let summary = ensure_folder(&folder, "docs", "projets", &ffprobe, &bypass, false);
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
        let bypass = Bypass::new("M6 - SDM - ", &[]);
        let first = ensure_folder(&folder, "docs", "projets", &ffprobe, &bypass, false);
        assert_eq!(first.templates, 1);
        assert_eq!(first.created, 1);
        assert_eq!(first.existing, 0);
        assert!(folder
            .join("projets")
            .join("M6 - SDM - Leslie et Léo_9x16.ecproj")
            .is_file());
        let second = ensure_folder(&folder, "docs", "projets", &ffprobe, &bypass, false);
        assert_eq!(second.created, 0);
        assert_eq!(second.existing, 1);
        assert!(!docs.join("M6 - SDM - Leslie et Léo.csv").exists());
    }

    #[test]
    fn archive_only_videos_are_scanned_without_raw() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("341 Camille");
        let docs = folder.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("W9 - LPHDF - La découverte.mp4"), b"video").unwrap();
        let bypass = Bypass::new("M6 - SDM - ", &["W9 - LPHDF".to_string()]);
        let summary = ensure_folder(
            &folder,
            "docs",
            "projets",
            Path::new("ffprobe"),
            &bypass,
            false,
        );
        assert_eq!(summary.templates, 0);
        assert_eq!(summary.archive_only, 1);
        assert_eq!(summary.archive_only_folders, 1);
        assert_eq!(summary.pure_folders.len(), 1);
        assert_eq!(summary.names, vec!["W9 - LPHDF - La découverte.mp4"]);
        assert!(summary
            .archive_only_names
            .contains(&"W9 - LPHDF - La découverte.mp4".to_string()));
        assert!(!folder.join("projets").exists());
    }
}
