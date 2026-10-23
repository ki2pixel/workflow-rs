//! STEP1 : extraction des archives et normalisation vidéo.
//!
//! Point d'entrée unique, appelé par `workflow-cli`. Le serveur Rust lance un
//! sous-processus dédié ; aucune implémentation Python n'est utilisée.

pub mod archive;
pub mod normalize;
pub mod sanitize;

use crate::error::StepError;
use archive::{extract_archive, find_archives_to_process, mark_archive_as_processed};
use chrono::Local;
use normalize::{normalize_videos_in, NormalizationSummary};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use workflow_core::Config;

/// Journal STEP1 : sortie standard pour le serveur et copie horodatée dans
/// `logs/step1/extract_archives_*.log` comme l'implémentation Python.
pub struct RunLogger {
    file: Mutex<Option<File>>,
}

impl RunLogger {
    pub fn new(path: Option<PathBuf>) -> Self {
        let file = path.and_then(|path| {
            if let Some(parent) = path.parent() {
                if let Err(error) = fs::create_dir_all(parent) {
                    eprintln!(
                        "Impossible de créer le dossier de journal {}: {error}",
                        parent.display()
                    );
                    return None;
                }
            }
            match OpenOptions::new().create(true).append(true).open(&path) {
                Ok(file) => Some(file),
                Err(error) => {
                    eprintln!("Impossible d'ouvrir le journal {}: {error}", path.display());
                    None
                }
            }
        });
        Self {
            file: Mutex::new(file),
        }
    }

    pub fn line(&self, message: impl AsRef<str>) {
        let message = message.as_ref();
        println!("{message}");
        if let Ok(mut file) = self.file.lock() {
            if let Some(file) = file.as_mut() {
                let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S,%3f");
                let _ = writeln!(file, "{timestamp} - INFO - {message}");
                let _ = file.flush();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Step1Options {
    pub source_dir: PathBuf,
    pub projects_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub keyword: String,
    pub docs_subfolder: String,
    pub delete_archive: bool,
    pub normalize: bool,
    pub force_normalize: bool,
    pub fps: f64,
    pub use_gpu: bool,
    pub crf: u32,
    pub gpu_workers: usize,
    pub probe_workers: usize,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub unrar: PathBuf,
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
    /// `None` désactive le fichier de journal (tests).
    pub log_file: Option<PathBuf>,
}

impl Step1Options {
    pub fn from_config(
        config: &Config,
        source_override: Option<PathBuf>,
    ) -> Result<Self, StepError> {
        let resolved = config.paths.resolve();
        let source_dir = source_override.unwrap_or_else(|| config.steps.step1.source_dir.clone());
        if source_dir.as_os_str().is_empty() {
            return Err(StepError::Schema(
                "steps.step1.source_dir est vide et aucune source n'a été fournie".into(),
            ));
        }
        let preserved_mov_names = if config.steps.step1.preserved_mov_names.is_empty() {
            vec!["m6+.mov".into()]
        } else {
            config.steps.step1.preserved_mov_names.clone()
        };
        let log_file = Some(resolved.logs.join("step1").join(format!(
            "extract_archives_{}.log",
            Local::now().format("%Y%m%d_%H%M%S")
        )));
        Ok(Self {
            source_dir,
            projects_dir: resolved.projects,
            logs_dir: resolved.logs.join("step1"),
            keyword: config.steps.filter.keyword.clone(),
            docs_subfolder: config.steps.filter.docs_subfolder.clone(),
            delete_archive: config.steps.step1.delete_archive,
            normalize: config.steps.step1.normalize,
            force_normalize: false,
            fps: f64::from(config.steps.step1.fps),
            use_gpu: config.steps.step1.use_gpu,
            crf: config.steps.step1.crf,
            gpu_workers: config.steps.step1.gpu_workers as usize,
            probe_workers: 10,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            unrar: PathBuf::from("unrar"),
            skip_preserved_mov: config.steps.step1.skip_preserved_mov,
            preserved_mov_names,
            log_file,
        })
    }

    pub fn logger(&self) -> RunLogger {
        RunLogger::new(self.log_file.clone())
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Step1Report {
    pub archives_found: usize,
    pub extracted: usize,
    pub failed: usize,
    pub deleted_archives: usize,
    pub security_issues: usize,
    pub normalization: NormalizationSummary,
}

impl Step1Report {
    pub fn success(&self) -> bool {
        self.failed == 0 && self.normalization.is_success()
    }
}

/// Extrait les nouvelles archives puis normalise chaque projet.
pub fn run_step1(options: &Step1Options) -> Result<Step1Report, StepError> {
    let logger = options.logger();
    fs::create_dir_all(&options.projects_dir)?;

    logger.line("--- Démarrage du script d'extraction d'archives ---");
    logger.line(format!("Dossier source: {}", options.source_dir.display()));
    logger.line(format!(
        "Dossier de destination des projets: {}",
        options.projects_dir.display()
    ));

    let did_reset =
        archive::reset_processed_archives_if_needed(&options.logs_dir, Local::now(), &logger);
    if did_reset {
        logger.line("Réinitialisation mensuelle exécutée (ou marqueur mis à jour).");
    } else {
        logger.line("Aucune réinitialisation mensuelle nécessaire.");
    }

    if !options.source_dir.exists() {
        logger.line(format!(
            "Le dossier d'archives '{}' n'existe pas.",
            options.source_dir.display()
        ));
    }
    logger.line(format!(
        "Recherche des archives dans '{}' avec le mot-clé '{}'...",
        options.source_dir.display(),
        options.keyword
    ));

    let archives =
        find_archives_to_process(&options.source_dir, &options.keyword, &options.logs_dir);
    for archive in &archives {
        logger.line(format!(
            "Archive correspondante trouvée: {}",
            display_name(archive)
        ));
    }
    let total = archives.len();
    logger.line(format!("Trouvé {total} nouvelle(s) archive(s) à traiter."));
    println!("Trouvé {total} archive(s) à traiter");

    let mut report = Step1Report {
        archives_found: total,
        ..Step1Report::default()
    };
    if total == 0 {
        logger.line("Aucune nouvelle archive à traiter. Fin du script.");
        return Ok(report);
    }

    for (index, archive) in archives.iter().enumerate() {
        logger.line(format!(
            "--- Traitement {}/{}: {} ---",
            index + 1,
            total,
            display_name(archive)
        ));
        match extract_archive(
            archive,
            &options.projects_dir,
            &options.docs_subfolder,
            &options.unrar,
            &logger,
        ) {
            Ok(outcome) => {
                report.extracted += 1;
                report.security_issues += outcome.security_issues;

                let resolved = fs::canonicalize(archive).unwrap_or_else(|_| archive.clone());
                if let Err(error) = mark_archive_as_processed(&options.logs_dir, &resolved) {
                    logger.line(format!(
                        "Erreur d'écriture dans le fichier des archives traitées: {error}"
                    ));
                }

                if options.normalize && outcome.docs_dir.exists() {
                    let summary = normalize_videos_in(&outcome.docs_dir, options, &logger);
                    report.normalization.total += summary.total;
                    report.normalization.success += summary.success;
                    report.normalization.failed += summary.failed;
                    report.normalization.skipped += summary.skipped;
                }

                if options.delete_archive {
                    match fs::remove_file(archive) {
                        Ok(()) => {
                            report.deleted_archives += 1;
                            logger.line(format!(
                                "Archive source '{}' supprimée avec succès.",
                                display_name(archive)
                            ));
                        }
                        Err(error) => logger.line(format!(
                            "Impossible de supprimer l'archive source '{}': {error}",
                            display_name(archive)
                        )),
                    }
                }
            }
            Err(error) => {
                report.failed += 1;
                logger.line(format!(
                    "L'extraction de '{}' a échoué: {error}",
                    display_name(archive)
                ));
            }
        }
    }

    logger.line("--- Traitement terminé ---");
    logger.line(format!(
        "Résumé: {}/{} archive(s) extraite(s) avec succès.",
        report.extracted, report.archives_found
    ));
    if report.normalization.failed > 0 {
        logger.line(format!(
            "Résumé: {} vidéo(s) non normalisée(s) — relancer avec --normalize-only.",
            report.normalization.failed
        ));
    }
    Ok(report)
}

/// Normalise les projets déjà extraits, sans toucher aux archives.
pub fn run_normalize_only(options: &Step1Options) -> Result<NormalizationSummary, StepError> {
    let logger = options.logger();
    logger.line("--- Démarrage de la normalisation seule (rattrapage) ---");
    logger.line(format!(
        "Dossier de destination des projets: {}",
        options.projects_dir.display()
    ));
    let summary = normalize_videos_in(&options.projects_dir, options, &logger);
    logger.line(format!("Résumé normalisation: {summary:?}"));
    Ok(summary)
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn base_options(temp: &tempfile::TempDir) -> Step1Options {
        Step1Options {
            source_dir: temp.path().join("downloads"),
            projects_dir: temp.path().join("projects"),
            logs_dir: temp.path().join("logs").join("step1"),
            keyword: "Camille".into(),
            docs_subfolder: "docs".into(),
            delete_archive: true,
            normalize: true,
            force_normalize: false,
            fps: 25.0,
            use_gpu: false,
            crf: 28,
            gpu_workers: 1,
            probe_workers: 1,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            unrar: PathBuf::from("unrar"),
            skip_preserved_mov: true,
            preserved_mov_names: vec!["m6+.mov".into()],
            log_file: None,
        }
    }

    #[test]
    fn extraction_marks_archive_and_deletes_it() {
        let temp = tempfile::tempdir().unwrap();
        let mut options = base_options(&temp);
        options.normalize = false;
        fs::create_dir_all(&options.projects_dir).unwrap();
        fs::create_dir_all(&options.source_dir).unwrap();
        let archive = options.source_dir.join("13_Camille.zip");
        {
            let file = File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("docs/clip.txt", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"ok").unwrap();
            writer.finish().unwrap();
        }

        let report = run_step1(&options).unwrap();
        assert_eq!(report.archives_found, 1);
        assert_eq!(report.extracted, 1);
        assert!(report.success());
        assert!(!archive.exists());
        assert!(options
            .projects_dir
            .join("13 Camille")
            .join("docs")
            .join("clip.txt")
            .exists());
        let processed =
            fs::read_to_string(options.logs_dir.join("processed_archives.txt")).unwrap();
        assert_eq!(processed.trim(), archive.display().to_string());
    }

    #[test]
    fn second_run_skips_processed_archives() {
        let temp = tempfile::tempdir().unwrap();
        let mut options = base_options(&temp);
        options.normalize = false;
        options.delete_archive = false;
        fs::create_dir_all(&options.projects_dir).unwrap();
        fs::create_dir_all(&options.source_dir).unwrap();
        let archive = options.source_dir.join("14_Camille.zip");
        {
            let file = File::create(&archive).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            writer
                .start_file("clip.txt", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"ok").unwrap();
            writer.finish().unwrap();
        }

        let first = run_step1(&options).unwrap();
        assert_eq!(first.extracted, 1);
        let second = run_step1(&options).unwrap();
        assert_eq!(second.archives_found, 0);
        assert!(second.success());
    }
}
