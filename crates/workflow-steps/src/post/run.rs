//! Orchestration du post-traitement : A5 (rapport) + A6 (template SDM).
//!
//! Invariant central : le `.ecproj` brut n'est jamais modifié. Un second
//! projet `_assist.ecproj` est toujours publié (copie du brut si aucune
//! décision n'est applicable). Les artefacts `post.json`, `review.json` et
//! `audit.json` sont additifs, sous `docs/post/<projet_assisté>/`.

use crate::post::ecproj::{
    self, assist_path, comp_meta, comp_mut, find_item_by_name, project_stem, save_bytes, segments,
    set_position_y, set_scale, CompMeta, Segment,
};
use crate::post::logo::LogoAnnotation;
use crate::post::model::{
    now_iso, verdict, Action, AuditCheck, Decision, PostProjectOutcome, PostRunSummary, PostSummary,
};
use crate::post::{a1, a2, a3, a4, a6, a7, detect, direct};
use crate::template::{Bypass, BypassMode};
use crate::vision::frame::FrameCache;
use crate::vision::logo_detector::{ExecutionDevice, Roi};
use crate::vision::source_logo::SourceLogoCatalog;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::{json, Map, Serializer, Value};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;
use workflow_core::fsutil::write_atomic;
use workflow_core::mission::ClientMission;
use workflow_schema::round::round_to;

const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm"];

#[derive(Debug, Clone)]
pub struct PostOptions {
    pub enabled: bool,
    /// Active le bypass direct des vidéos du template.
    pub direct: bool,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub suffix: String,
    pub projects_subfolder: String,
    pub docs_subfolder: String,
    pub artifacts_subfolder: String,
    pub template_prefixes: Vec<String>,
    pub archive_only_prefixes: Vec<String>,
    pub a1_margin: f64,
    /// R2 : diffère A1 en revue sur candidat A3 fort. Désactivé par défaut.
    pub a1_defer_on_a3: bool,
    /// D-058 option A : raffine un `apply` A1 avec le détecteur natif.
    pub a1_refine_native: bool,
    pub a2_margin: f64,
    /// R6 : autorise `gap < 0,7` en revue A3. Désactivé par défaut.
    pub a3_relaxed_gap: bool,
    /// Gate OCR A2 : désactivé, aucune session ONNX n'est ouverte.
    pub a2_ocr: bool,
    pub a2_ocr_det: PathBuf,
    pub a2_ocr_rec: PathBuf,
    pub a2_ocr_onnxruntime: Option<PathBuf>,
    /// Threads intra-op ONNX par session OCR A2 (det et rec séparément).
    pub a2_ocr_threads: usize,
    /// Registre spatial des logos source (D-052), annotation et exclusion OCR.
    pub a2_logo_prior: bool,
    pub a2_logo_catalog: PathBuf,
    /// Détecteur de logos B1/B2, annotation seule. Désactivé par défaut.
    pub a2_logo_detector: bool,
    pub a2_logo_detector_model: PathBuf,
    pub a2_logo_detector_onnxruntime: Option<PathBuf>,
    pub a2_logo_detector_device: ExecutionDevice,
    pub a2_logo_detector_size: u32,
    pub a2_logo_detector_conf: f32,
    pub a2_logo_detector_iou: f32,
    pub a2_logo_detector_min_persistence: f64,
    pub a2_logo_detector_roi: Roi,
    /// Verrou matériel partagé avec STEP2/3/4 (D-012).
    pub hardware_lock: PathBuf,
    /// Laboratoire uniquement : calcule les rapports sans publier `_assist`.
    pub report_only: bool,
    /// Émet les marqueurs `POST_*` sur `stdout` (hook STEP8) au lieu de
    /// `stderr` (commande `post run` seule).
    pub emit_progress: bool,
    /// A7 : insère les logos de mission dans le projet assisté.
    pub mission_overlay: bool,
    /// A7 : banque centrale de repli pour les assets de mission.
    pub mission_overlays: Option<PathBuf>,
    /// A7 : durée maximale d'un `.mov` interprété comme end-card.
    pub mission_end_card_max_s: f64,
    /// A2 : utilise les calques de mission comme masque.
    pub a2_mission_mask: bool,
    /// Missions résolues par la CLI pour les lots du run.
    pub missions: Vec<ClientMission>,
}

impl Default for PostOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            direct: false,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            suffix: "_assist".into(),
            projects_subfolder: "projets".into(),
            docs_subfolder: "docs".into(),
            artifacts_subfolder: "post".into(),
            template_prefixes: a6::default_prefixes(),
            archive_only_prefixes: Vec::new(),
            a1_margin: 1.015,
            a1_defer_on_a3: false,
            a1_refine_native: false,
            a2_margin: 1.05,
            a3_relaxed_gap: false,
            a2_ocr: false,
            a2_ocr_det: PathBuf::from("qa/PP-OCRv6_det_small.onnx"),
            a2_ocr_rec: PathBuf::from("qa/PP-OCRv6_rec_small.onnx"),
            a2_ocr_onnxruntime: None,
            a2_ocr_threads: a2::A2_OCR_DEFAULT_THREADS,
            a2_logo_prior: false,
            a2_logo_catalog: PathBuf::from("qa/source_logos.toml"),
            a2_logo_detector: false,
            a2_logo_detector_model: PathBuf::from("qa/logo_yolox_nano_a2.onnx"),
            a2_logo_detector_onnxruntime: None,
            a2_logo_detector_device: ExecutionDevice::Cuda,
            a2_logo_detector_size: 416,
            a2_logo_detector_conf: 0.30,
            a2_logo_detector_iou: 0.65,
            a2_logo_detector_min_persistence: 0.70,
            a2_logo_detector_roi: Roi::Full,
            hardware_lock: PathBuf::from("/tmp/workflow-rs-hw.lock"),
            report_only: false,
            emit_progress: false,
            mission_overlay: false,
            mission_overlays: None,
            mission_end_card_max_s: 5.0,
            a2_mission_mask: false,
            missions: Vec::new(),
        }
    }
}

impl PostOptions {
    pub fn from_config(config: &workflow_core::config::Config, docs_subfolder: &str) -> Self {
        let post = &config.steps.step8.post;
        Self {
            enabled: post.enabled,
            direct: post.direct,
            ffmpeg: post.ffmpeg.clone(),
            ffprobe: post.ffprobe.clone(),
            suffix: post.suffix.clone(),
            projects_subfolder: post.projects_subfolder.clone(),
            docs_subfolder: docs_subfolder.to_string(),
            artifacts_subfolder: post.artifacts_subfolder.clone(),
            template_prefixes: post.template_prefixes.clone(),
            archive_only_prefixes: post.archive_only_prefixes.clone(),
            a1_margin: post.a1_margin,
            a1_defer_on_a3: post.a1_defer_on_a3,
            a1_refine_native: post.a1_refine_native,
            a2_margin: post.a2_margin,
            a3_relaxed_gap: post.a3_relaxed_gap,
            a2_ocr: post.a2_ocr,
            a2_ocr_det: resolve_models_path(&config.paths.models, &post.a2_ocr_det),
            a2_ocr_rec: resolve_models_path(&config.paths.models, &post.a2_ocr_rec),
            a2_ocr_onnxruntime: post.a2_ocr_onnxruntime.clone(),
            a2_ocr_threads: post.a2_ocr_threads.max(1),
            a2_logo_prior: post.a2_logo_prior,
            a2_logo_catalog: resolve_models_path(&config.paths.models, &post.a2_logo_catalog),
            a2_logo_detector: post.a2_logo_detector,
            a2_logo_detector_model: resolve_models_path(
                &config.paths.models,
                &post.a2_logo_detector_model,
            ),
            a2_logo_detector_onnxruntime: post
                .a2_logo_detector_onnxruntime
                .clone()
                .filter(|path| !path.as_os_str().is_empty()),
            a2_logo_detector_device: ExecutionDevice::parse(&post.a2_logo_detector_device)
                .unwrap_or_default(),
            a2_logo_detector_size: post.a2_logo_detector_size,
            a2_logo_detector_conf: post.a2_logo_detector_conf as f32,
            a2_logo_detector_iou: post.a2_logo_detector_iou as f32,
            a2_logo_detector_min_persistence: post.a2_logo_detector_min_persistence,
            a2_logo_detector_roi: Roi::parse(&post.a2_logo_detector_roi).unwrap_or_default(),
            hardware_lock: config.hardware.lock_path.clone(),
            report_only: false,
            emit_progress: false,
            mission_overlay: post.mission_overlay,
            mission_overlays: config.paths.resolve().mission_overlays,
            mission_end_card_max_s: post.mission_end_card_max_s,
            a2_mission_mask: post.a2_mission_mask,
            missions: Vec::new(),
        }
    }
}

fn resolve_models_path(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

pub fn run_post_for_folders(folders: &[String], options: &PostOptions) -> PostRunSummary {
    let mut summary = PostRunSummary::default();
    if !options.enabled {
        return summary;
    }
    let bypass = Bypass::new(&options.template_prefixes, &options.archive_only_prefixes);
    // Pré-scan de tous les dossiers : le total A5/A6/A7 est publié avant le
    // premier projet pour que la barre STEP8 couvre aussi cette phase.
    let mut projects = Vec::new();
    for folder in folders {
        let folder = Path::new(folder);
        prepare_direct(options, folder, &bypass, &mut summary);
        let discovered = discover_projects(options, folder, &bypass);
        if discovered.is_empty() {
            tracing::debug!(
                "Post-traitement: aucun .ecproj brut dans {}",
                folder.join(&options.projects_subfolder).display()
            );
        } else {
            tracing::info!(
                "Post-traitement: {} projet(s) dans {}",
                discovered.len(),
                folder.display()
            );
            projects.extend(discovered);
        }
    }
    run_post_jobs(options, &projects, &mut summary);
    summary
}

pub fn run_post_dir(options: &PostOptions, folder: &Path) -> PostRunSummary {
    let mut summary = PostRunSummary::default();
    if !options.enabled {
        return summary;
    }
    let bypass = Bypass::new(&options.template_prefixes, &options.archive_only_prefixes);
    prepare_direct(options, folder, &bypass, &mut summary);
    let projects = discover_projects(options, folder, &bypass);
    if projects.is_empty() {
        tracing::debug!(
            "Post-traitement: aucun .ecproj brut dans {}",
            folder.join(&options.projects_subfolder).display()
        );
        return summary;
    }
    tracing::info!(
        "Post-traitement: {} projet(s) dans {}",
        projects.len(),
        folder.display()
    );
    run_post_jobs(options, &projects, &mut summary);
    summary
}

fn prepare_direct(
    options: &PostOptions,
    folder: &Path,
    bypass: &Bypass,
    summary: &mut PostRunSummary,
) {
    if !(options.direct && !options.report_only && !bypass.is_empty()) {
        return;
    }
    let direct = direct::ensure_folder(
        folder,
        &options.docs_subfolder,
        &options.projects_subfolder,
        &options.ffprobe,
        bypass,
        false,
    );
    if direct.templates > 0 {
        tracing::info!(
            "Template direct {}: {} vidéo(s), {} brut(s) créé(s), {} déjà présent(s)",
            folder.display(),
            direct.templates,
            direct.created,
            direct.existing
        );
    }
    if direct.archive_only > 0 {
        tracing::info!(
            "Archive-only {}: {} vidéo(s), aucun brut",
            folder.display(),
            direct.archive_only
        );
    }
    if direct.failed > 0 {
        summary
            .failures
            .push(format!("template direct: {}", folder.display()));
    }
}

fn discover_projects(options: &PostOptions, folder: &Path, bypass: &Bypass) -> Vec<PathBuf> {
    let projects_dir = folder.join(&options.projects_subfolder);
    if !projects_dir.is_dir() {
        tracing::debug!(
            "Post-traitement: pas de dossier {} pour {}",
            projects_dir.display(),
            folder.display()
        );
        return Vec::new();
    }
    let mut projects = Vec::new();
    let Ok(entries) = fs::read_dir(&projects_dir) else {
        tracing::warn!(
            "Post-traitement: lecture de {} impossible",
            projects_dir.display()
        );
        return Vec::new();
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ecproj"))
        {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') || name.contains(".tmp.") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if stem.ends_with(&options.suffix) {
            continue;
        }
        projects.push(path);
    }
    projects.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    if options.direct && !bypass.is_empty() {
        let archive_stems: std::collections::HashSet<String> =
            crate::template::docs_videos(&folder.join(&options.docs_subfolder))
                .into_iter()
                .filter(|video| bypass.mode_for_name(video) == Some(BypassMode::ArchiveOnly))
                .filter_map(|video| {
                    video
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .map(str::to_string)
                })
                .collect();
        if !archive_stems.is_empty() {
            let before = projects.len();
            projects.retain(|project| {
                let Some(stem) = project.file_stem().and_then(|stem| stem.to_str()) else {
                    return true;
                };
                let stem = stem.strip_suffix("_9x16").unwrap_or(stem);
                !archive_stems.contains(stem)
            });
            let skipped = before - projects.len();
            if skipped > 0 {
                tracing::info!(
                    "Post-traitement: {skipped} projet(s) archive-only ignoré(s) dans {}",
                    folder.display()
                );
            }
        }
    }
    projects
}

fn run_post_jobs(options: &PostOptions, projects: &[PathBuf], summary: &mut PostRunSummary) {
    if projects.is_empty() {
        return;
    }
    // Marqueurs lus par le serveur pour prolonger la barre STEP8 après
    // EffectCraft. Dans le hook STEP8 ils partagent `stdout` avec
    // `MS_BATCH_*` pour conserver l'ordre ; `post run` les garde sur `stderr`
    // afin que son JSON `stdout` reste parsable.
    emit_post_marker(options, format!("POST_TOTAL: {}", projects.len()));
    let mut logo = LogoAnnotation::new(options);
    for (index, project) in projects.iter().enumerate() {
        let name = project
            .file_stem()
            .and_then(|name| name.to_str())
            .or_else(|| project.file_name().and_then(|name| name.to_str()))
            .unwrap_or("?");
        emit_post_marker(
            options,
            format!("POST_CURRENT: {}/{} - {}", index + 1, projects.len(), name),
        );
        let outcome = run_post_project_inner(options, project, None, &mut logo);
        if let Some(error) = outcome.error.as_deref() {
            tracing::warn!("Post-traitement échoué pour {}: {error}", project.display());
        } else {
            tracing::info!(
                "Post-traitement {}: {} apply, {} review, {} noop, audit {}",
                project
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("?"),
                outcome.summary.apply,
                outcome.summary.review,
                outcome.summary.noop,
                outcome.audit_verdict
            );
        }
        emit_post_marker(
            options,
            format!("POST_PROGRESS: {}/{} - {}", index + 1, projects.len(), name),
        );
        summary.push(outcome);
    }
    emit_post_marker(
        options,
        format!("POST_DONE: {} - {}", projects.len(), summary.failures.len()),
    );
}

fn emit_post_marker(options: &PostOptions, line: String) {
    if options.emit_progress {
        println!("{line}");
        let _ = std::io::stdout().flush();
    } else {
        eprintln!("{line}");
    }
}

pub fn run_post_project(
    options: &PostOptions,
    project: &Path,
    docs_override: Option<&Path>,
) -> PostProjectOutcome {
    let mut logo = LogoAnnotation::new(options);
    run_post_project_inner(options, project, docs_override, &mut logo)
}

fn run_post_project_inner(
    options: &PostOptions,
    project: &Path,
    docs_override: Option<&Path>,
    logo: &mut LogoAnnotation,
) -> PostProjectOutcome {
    let started = Instant::now();
    let docs = docs_override.map(Path::to_path_buf).unwrap_or_else(|| {
        docs_dir_for(
            project,
            &options.projects_subfolder,
            &options.docs_subfolder,
        )
    });
    let assist = match assist_path(project, &options.suffix) {
        Ok(assist) => assist,
        Err(error) => {
            return failure_outcome(project, None, &docs, options, format!("{error:#}"), started);
        }
    };
    let raw_bytes = match fs::read(project) {
        Ok(bytes) => bytes,
        Err(error) => {
            return failure_outcome(
                project,
                Some(&assist),
                &docs,
                options,
                format!("lecture du projet impossible: {error}"),
                started,
            );
        }
    };
    let raw_sha = detect::sha256_file(project).unwrap_or_default();
    if !options.report_only {
        if let Err(error) = write_atomic(&assist, &raw_bytes) {
            return failure_outcome(
                project,
                None,
                &docs,
                options,
                format!("publication de {} impossible: {error}", assist.display()),
                started,
            );
        }
    }

    let doc = match ecproj::load(project) {
        Ok(doc) => doc,
        Err(error) => {
            return failure_outcome(
                project,
                Some(&assist),
                &docs,
                options,
                format!("{error:#}"),
                started,
            );
        }
    };
    let meta = match comp_meta(&doc) {
        Ok(meta) => meta,
        Err(error) => {
            return failure_outcome(
                project,
                Some(&assist),
                &docs,
                options,
                format!("{error:#}"),
                started,
            );
        }
    };
    let segments = match segments(&doc) {
        Ok(segments) => segments,
        Err(error) => {
            return failure_outcome(
                project,
                Some(&assist),
                &docs,
                options,
                format!("{error:#}"),
                started,
            );
        }
    };
    let stem = project_stem(project, &segments);
    let main_s = segments
        .iter()
        .map(|segment| segment.end)
        .fold(0.0_f64, f64::max);
    let video = resolve_video(&docs, &segments, &stem);
    let specs =
        video
            .as_deref()
            .and_then(|video| match detect::probe_video(&options.ffprobe, video) {
                Ok(specs) => Some(specs),
                Err(error) => {
                    tracing::warn!(
                        "Post-traitement: ffprobe en échec pour {}: {error}",
                        video.display()
                    );
                    None
                }
            });
    let read_specs_ms = started.elapsed().as_secs_f64() * 1000.0;
    let ae = find_docs_file(&docs, &stem, "_ae.json");
    let csv = find_docs_file(&docs, &stem, ".csv");
    let fps = if meta.fps > 0.0 {
        meta.fps
    } else {
        specs.as_ref().map(|specs| specs.fps).unwrap_or(25.0)
    };

    let mut timings = Map::new();
    timings.insert("read_specs_ms".into(), json!(round_to(read_specs_ms, 1)));

    let a6_start = Instant::now();
    let (a6_decisions, template) =
        a6::validate(&doc, &segments, &meta, &stem, &options.template_prefixes);
    let mut decisions = Vec::new();
    let mut a6_ms = a6_start.elapsed().as_secs_f64() * 1000.0;
    let mut a1_ms = 0.0;
    let mut a2_ms = 0.0;
    let mut a2_logo_detector_ms = 0.0;
    // Le cache n'existe que si le détecteur est actif : sans lui, A2 OCR
    // garde son extraction directe et la production reste inchangée. Quand il
    // est actif, le gate OCR remplit les 12 frames et le détecteur les relit.
    let frame_cache = if options.a2_logo_detector {
        match (video.as_deref(), specs.as_ref()) {
            (Some(video), Some(specs)) => Some(FrameCache::new(
                video,
                specs.width as usize,
                specs.height as usize,
                specs.duration_s,
            )),
            _ => None,
        }
    } else {
        None
    };
    let mut a3_ms = 0.0;
    let mut a4_ms = 0.0;
    let mut output_doc = doc.clone();
    let mut mission_block: Option<Value> = None;
    let mut tail_s = 0.0_f64;
    let mut a7_ms = 0.0_f64;
    let mut a7_pre_decisions = Vec::new();
    // Hors A6, A7 passe avant A1/A2 : A2 voit ainsi le masque réel des
    // logos mission. Le template A6 reste jugé sur le projet brut.
    if template.eligible != Some(true) && !options.missions.is_empty() {
        let started = Instant::now();
        let outcome = run_a7(&output_doc, project, &docs, &stem, main_s, &meta, options);
        a7_ms += started.elapsed().as_secs_f64() * 1000.0;
        a7_pre_decisions = outcome.decisions;
        if outcome.changed {
            output_doc = outcome.doc;
        }
        mission_block = outcome.mission;
        tail_s = outcome.tail_s;
    }
    if template.eligible != Some(true) {
        let (source_width, source_height) = specs
            .as_ref()
            .map(|specs| (specs.width, specs.height))
            .unwrap_or((1920, 1080));
        let source_logo_prior = if options.a2_logo_prior {
            let source_name = video
                .as_deref()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str());
            match (
                source_name,
                SourceLogoCatalog::load(&options.a2_logo_catalog),
            ) {
                (Some(source_name), Ok(catalog)) => {
                    catalog.prior_for_source(source_name, source_width, source_height)
                }
                (Some(_), Err(error)) => {
                    tracing::warn!(
                        catalog = %options.a2_logo_catalog.display(),
                        error = %error,
                        "A2: registre de logos source ignoré"
                    );
                    None
                }
                (None, _) => None,
            }
        } else {
            None
        };
        // Un logo caché par un habillage déclaré ne doit pas déclencher le
        // veto « logo seul » d'A2. Le masque n'est lu que si le prior est
        // actif : sans flag, aucun travail supplémentaire.
        let overlay_mask = if options.a2_logo_prior || options.a2_mission_mask {
            if options.a2_mission_mask && mission_block.is_some() {
                // A7 a déjà inséré les calques mission : A2 lit le document
                // en mémoire pour couvrir aussi ces logos.
                crate::post::overlay::mask_from_doc_with(
                    &output_doc,
                    project,
                    &options.ffmpeg,
                    &options.ffprobe,
                )
                .ok()
            } else {
                crate::post::overlay::read_mask_with(project, &options.ffmpeg, &options.ffprobe)
                    .ok()
            }
        } else {
            None
        };
        // R2 : A3 doit être connu avant A1 pour différer un apply statique sur
        // un segment où un keyframing humain est probable. L'ordre des
        // décisions publiées reste A5, A1, A2, A3, A4, A6.
        let mut a3_decisions = Vec::new();
        if options.a1_defer_on_a3 {
            let start = Instant::now();
            a3_decisions = a3::decide_a3(&segments, ae.as_deref(), fps, options.a3_relaxed_gap);
            a3_ms = start.elapsed().as_secs_f64() * 1000.0;
        }
        decisions.extend(coverage_reviews(
            &segments,
            &meta,
            source_width,
            source_height,
        ));
        let start = Instant::now();
        let a1_options = a1::A1Options {
            margin: options.a1_margin,
            defer_targets: if options.a1_defer_on_a3 {
                strong_a3_targets(&a3_decisions)
            } else {
                HashSet::new()
            },
            refine_native: options.a1_refine_native,
        };
        decisions.extend(a1::decide_a1(
            &segments,
            video.as_deref(),
            &options.ffmpeg,
            &meta,
            source_width,
            source_height,
            &a1_options,
        ));
        a1_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let a2_options = a2::A2Options {
            margin: options.a2_margin,
            ocr: options.a2_ocr.then(|| a2::A2OcrOptions {
                det_model: options.a2_ocr_det.clone(),
                rec_model: options.a2_ocr_rec.clone(),
                onnxruntime: options.a2_ocr_onnxruntime.clone(),
                intra_threads: options.a2_ocr_threads.max(1),
            }),
            logo_prior: source_logo_prior,
            overlay: overlay_mask,
        };
        decisions.extend(a2::decide_a2(
            &segments,
            video.as_deref(),
            &options.ffmpeg,
            &options.ffprobe,
            &meta,
            source_height,
            &a2_options,
            frame_cache.as_ref(),
        ));
        a2_ms = start.elapsed().as_secs_f64() * 1000.0;
        a2_logo_detector_ms = logo
            .annotate(
                &mut decisions,
                video.as_deref(),
                specs.as_ref(),
                &options.ffmpeg,
                a2_options.logo_prior.as_ref(),
                frame_cache.as_ref(),
            )
            .unwrap_or(0.0);
        if !options.a1_defer_on_a3 {
            let start = Instant::now();
            a3_decisions = a3::decide_a3(&segments, ae.as_deref(), fps, options.a3_relaxed_gap);
            a3_ms = start.elapsed().as_secs_f64() * 1000.0;
        }
        decisions.extend(a3_decisions);
        let start = Instant::now();
        let rows = csv.as_deref().map(a4::parse_csv).unwrap_or_default();
        decisions.extend(a4::decide_a4(&segments, &rows, fps));
        a4_ms = start.elapsed().as_secs_f64() * 1000.0;
    }
    decisions.extend(a6_decisions);
    decisions.extend(a7_pre_decisions);
    timings.insert("a1_ms".into(), json!(round_to(a1_ms, 1)));
    timings.insert("a2_ms".into(), json!(round_to(a2_ms, 1)));
    timings.insert(
        "a2_logo_detector_ms".into(),
        json!(round_to(a2_logo_detector_ms, 1)),
    );
    timings.insert("a3_ms".into(), json!(round_to(a3_ms, 1)));
    timings.insert("a4_ms".into(), json!(round_to(a4_ms, 1)));

    let mut conflict = false;
    let a1_apply = decisions
        .iter()
        .filter(|decision| decision.code == "A1" && decision.action == Action::Apply)
        .count();
    let a2_apply = decisions
        .iter()
        .filter(|decision| decision.code == "A2" && decision.action == Action::Apply)
        .count();
    if a1_apply > 0 && a2_apply > 0 {
        conflict = true;
        for decision in decisions.iter_mut() {
            if matches!(decision.code, "A1" | "A2") && decision.action == Action::Apply {
                decision.action = Action::Review;
                decision.reason =
                    "A1 et A2 simultanément applicables, combinaison non gérée".into();
                decision.confidence = 0.3;
            }
        }
        decisions.push(Decision::review(
            "A1",
            "comp",
            "A1 et A2 simultanément applicables, aucune action appliquée",
            0.3,
            json!({"conflict": ["A1", "A2"]}),
        ));
    }

    let a6_apply = decisions
        .iter()
        .any(|decision| decision.code == "A6" && decision.action == Action::Apply);
    let a1_applies: Vec<Decision> = decisions
        .iter()
        .filter(|decision| decision.code == "A1" && decision.action == Action::Apply)
        .cloned()
        .collect();
    let a2_applies: Vec<Decision> = decisions
        .iter()
        .filter(|decision| decision.code == "A2" && decision.action == Action::Apply)
        .cloned()
        .collect();

    let mut transform_error = None;
    let mut transformed = output_doc != doc;
    let mut template_applied = false;
    let mut output_layers = None;
    if a6_apply {
        let apply_start = Instant::now();
        match a6::apply_template(&doc, &meta) {
            Ok((transformed_doc, info)) => {
                output_doc = transformed_doc;
                output_layers = Some(info.layers);
                template_applied = true;
                transformed = true;
            }
            Err(error) => {
                transform_error = Some(format!("{error:#}"));
                decisions.push(Decision::review(
                    "A6",
                    "comp",
                    format!("application du template impossible: {error:#}"),
                    0.2,
                    json!({"reason": "template_apply_failed"}),
                ));
            }
        }
        a6_ms += apply_start.elapsed().as_secs_f64() * 1000.0;
    } else if !a1_applies.is_empty() {
        if let Err(error) = apply_a1(&mut output_doc, &a1_applies) {
            transform_error = Some(format!("{error:#}"));
        } else {
            transformed = true;
        }
    } else if !a2_applies.is_empty() {
        if let Err(error) = apply_a2(&mut output_doc, &a2_applies) {
            transform_error = Some(format!("{error:#}"));
        } else {
            transformed = true;
        }
    }
    if template_applied && !options.missions.is_empty() {
        let a7_start = Instant::now();
        let outcome = run_a7(&output_doc, project, &docs, &stem, main_s, &meta, options);
        a7_ms += a7_start.elapsed().as_secs_f64() * 1000.0;
        decisions.extend(outcome.decisions);
        if outcome.changed {
            output_doc = outcome.doc;
            transformed = true;
        }
        mission_block = outcome.mission;
        tail_s = outcome.tail_s;
    }
    timings.insert("a6_ms".into(), json!(round_to(a6_ms, 1)));
    if !options.missions.is_empty() {
        timings.insert("a7_ms".into(), json!(round_to(a7_ms, 1)));
    }

    let final_meta = comp_meta(&output_doc).unwrap_or_else(|_| meta.clone());
    let mission_added = mission_block
        .as_ref()
        .and_then(|block| block.get("added"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);

    let mut assist_written = options.report_only || (!options.report_only && assist.is_file());
    if !options.report_only && transformed && transform_error.is_none() {
        match save_bytes(&output_doc).and_then(|bytes| {
            write_atomic(&assist, &bytes)
                .with_context(|| format!("publication de {}", assist.display()))
        }) {
            Ok(()) => assist_written = true,
            Err(error) => transform_error = Some(format!("{error:#}")),
        }
    }
    let mut sidecar_written = false;
    if !options.report_only && transform_error.is_none() {
        if let Some(block) = mission_block.as_ref() {
            let sidecar = json!({
                "schema": 2,
                "generated_at": now_iso(),
                "project": project.to_string_lossy(),
                "project_stem": stem,
                "lot": ClientMission::lot_from_folder(&project_folder(project)),
                "mission": block,
            });
            let path = docs.join(format!("{stem}_mission.json"));
            if let Err(error) = serde_json::to_vec_pretty(&sidecar)
                .map_err(anyhow::Error::from)
                .and_then(|bytes| write_atomic(&path, &bytes).map_err(anyhow::Error::from))
            {
                tracing::warn!("Post-traitement: sidecar mission non écrit: {error:#}");
            } else {
                sidecar_written = true;
            }
        }
    }
    let assist_sha = if !options.report_only && assist.is_file() {
        detect::sha256_file(&assist).ok()
    } else {
        None
    };
    let raw_sha_after = detect::sha256_file(project).ok();
    let raw_preserved = raw_sha_after.as_deref() == Some(raw_sha.as_str());

    let mut summary = PostSummary::from_decisions(&decisions);
    summary.template_applied = template_applied;
    let mut checks = audit_checks(
        &segments,
        &final_meta,
        tail_s,
        video.as_deref(),
        &decisions,
        &template,
        assist_written,
        raw_preserved,
        conflict,
        transform_error.as_deref(),
        options.report_only,
    );
    if template_applied {
        if let Some(layers) = output_layers {
            let expected = if find_item_by_name(&doc, "sdm.png", "Footage").is_some() {
                4
            } else {
                3
            } + mission_added;
            if options.report_only {
                checks.push(AuditCheck::pass(
                    "output_layers",
                    format!("{layers} calques calculés (report-only)"),
                ));
            } else if transform_error.is_none() && assist.is_file() {
                let total_layers = layers + mission_added;
                checks.push(AuditCheck::pass(
                    "output_written",
                    format!("{}, {total_layers} calques", assist.display()),
                ));
                if total_layers == expected {
                    checks.push(AuditCheck::pass(
                        "output_layers",
                        format!("{expected} calques"),
                    ));
                } else {
                    checks.push(AuditCheck::warn(
                        "output_layers",
                        format!("{total_layers} calques au lieu de {expected}"),
                    ));
                }
                if let Ok(output) = ecproj::load(&assist) {
                    if let Ok(output_segments) = ecproj::segments(&output) {
                        let scales: Vec<f64> = output_segments
                            .iter()
                            .map(|segment| round_to(segment.scale, 3))
                            .collect();
                        checks.push(AuditCheck::pass(
                            "output_scales",
                            format!("échelles {scales:?}"),
                        ));
                    }
                }
            }
        }
    }
    if let Some(block) = mission_block.as_ref() {
        append_mission_checks(
            &mut checks,
            block,
            &segments,
            &final_meta,
            tail_s,
            options.mission_overlay,
            sidecar_written,
        );
    }
    let audit_verdict = verdict(&checks).to_string();
    let video_hash = video
        .as_deref()
        .and_then(|video| detect::sha256_sample(video, 4 << 20).ok());
    timings.insert(
        "total_ms".into(),
        json!(round_to(started.elapsed().as_secs_f64() * 1000.0, 1)),
    );

    let generated_at = now_iso();
    let artifacts = artifacts_dir(&docs, options, &assist);
    let post = post_json(
        project,
        &assist,
        video.as_deref(),
        ae.as_deref(),
        csv.as_deref(),
        specs.as_ref(),
        &final_meta,
        &template,
        &timings,
        &decisions,
        &summary,
        mission_block.as_ref(),
        options.report_only,
    );
    let review = review_json(project, video.as_deref(), &decisions, &generated_at);
    let audit = audit_json(
        project,
        &assist,
        &raw_sha,
        assist_sha.as_deref(),
        video.as_deref(),
        video_hash.as_ref(),
        specs.as_ref(),
        &final_meta,
        &checks,
        &audit_verdict,
        &summary,
        &timings,
        transform_error.as_deref(),
        options.report_only,
    );
    let artifacts_written = write_artifacts(&artifacts, &post, &review, &audit).is_ok();
    if !artifacts_written {
        tracing::warn!(
            "Post-traitement: écriture des artefacts impossible dans {}",
            artifacts.display()
        );
    }
    let outcome_error = transform_error.clone().or_else(|| {
        (audit_verdict == "fail").then(|| "audit du post-traitement en échec".to_string())
    });
    PostProjectOutcome {
        project: project.to_string_lossy().to_string(),
        assist_project: (!options.report_only).then(|| assist.to_string_lossy().to_string()),
        artifacts_dir: artifacts_written.then(|| artifacts.to_string_lossy().to_string()),
        summary,
        audit_verdict,
        error: outcome_error,
    }
}

fn run_a7(
    doc: &Value,
    project: &Path,
    docs: &Path,
    stem: &str,
    main_s: f64,
    meta: &CompMeta,
    options: &PostOptions,
) -> a7::A7Outcome {
    let folder = project_folder(project);
    let lot = ClientMission::lot_from_folder(&folder);
    a7::apply(
        doc,
        &a7::A7Options {
            project,
            docs,
            stem,
            lot,
            main_s,
            meta,
            missions: &options.missions,
            overlay: options.mission_overlay,
            mission_overlays: options.mission_overlays.as_deref(),
            end_card_max_s: options.mission_end_card_max_s,
            ffprobe: &options.ffprobe,
        },
    )
}

fn project_folder(project: &Path) -> PathBuf {
    let parent = project.parent().unwrap_or_else(|| Path::new("."));
    if parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "projets" || name == "projets_extraits")
    {
        parent
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| parent.to_path_buf())
    } else {
        parent.to_path_buf()
    }
}

#[allow(clippy::too_many_arguments)]
fn append_mission_checks(
    checks: &mut Vec<AuditCheck>,
    block: &Value,
    segments: &[Segment],
    meta: &CompMeta,
    tail_s: f64,
    overlay: bool,
    sidecar_written: bool,
) {
    let id = block.get("id").and_then(Value::as_str).unwrap_or("mission");
    checks.push(AuditCheck::pass(
        "mission_available",
        format!("{id} résolue"),
    ));
    let missing = block
        .get("missing")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let known_absent: Vec<&str> = block
        .get("known_absent")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name").and_then(Value::as_str))
                .collect()
        })
        .unwrap_or_default();
    if missing.is_empty() {
        checks.push(AuditCheck::pass(
            "mission_assets",
            "tous les assets sont valides",
        ));
    } else {
        let names: Vec<&str> = missing
            .iter()
            .filter_map(Value::as_str)
            .filter(|name| !known_absent.contains(name))
            .collect();
        if names.is_empty() {
            checks.push(AuditCheck::pass(
                "mission_assets",
                "aucun asset manquant hors absences volontaires",
            ));
        } else {
            checks.push(AuditCheck::warn(
                "mission_assets",
                format!("assets manquants ou non conformes: {names:?}"),
            ));
        }
    }
    if known_absent.is_empty() {
        checks.push(AuditCheck::pass(
            "mission_known_absent",
            "aucune absence volontaire demandée",
        ));
    } else {
        checks.push(AuditCheck::pass(
            "mission_known_absent",
            format!("assets volontairement absents: {known_absent:?}"),
        ));
    }
    let added = block
        .get("added")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let skipped = block
        .get("skipped")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    checks.push(AuditCheck::pass(
        "mission_layers",
        if skipped.is_empty() {
            format!("{} calque(s) de mission", added.len())
        } else {
            format!(
                "{} calque(s) ajouté(s), {} déjà présent(s)",
                added.len(),
                skipped.len()
            )
        },
    ));
    let no_logo = block
        .get("no_logo")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if no_logo {
        if block
            .get("reasons")
            .and_then(Value::as_array)
            .is_some_and(|reasons| {
                reasons
                    .iter()
                    .any(|reason| reason.as_str() == Some("no_logo_conflict_a6"))
            })
        {
            checks.push(AuditCheck::warn(
                "mission_no_logo",
                "Pas de logo respecté, mais A6 sdm.png reste présent (pas de suppression)",
            ));
        } else {
            checks.push(AuditCheck::pass(
                "mission_no_logo",
                "Pas de logo, aucun calque mission",
            ));
        }
    }
    let reasons: Vec<&str> = block
        .get("reasons")
        .and_then(Value::as_array)
        .map(|reasons| reasons.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let review_reasons: Vec<&str> = reasons
        .iter()
        .copied()
        .filter(|reason| !matches!(*reason, "no_logo" | "overlay_disabled" | "known_absent"))
        .collect();
    if !review_reasons.is_empty() {
        checks.push(AuditCheck::warn(
            "mission_modes",
            format!("décisions A7 en revue: {review_reasons:?}"),
        ));
    } else if reasons.contains(&"overlay_disabled") {
        checks.push(AuditCheck::warn(
            "mission_modes",
            "insertion A7 désactivée (diagnostic)",
        ));
    } else if reasons.contains(&"known_absent") {
        checks.push(AuditCheck::pass(
            "mission_modes",
            "absences volontaires uniquement",
        ));
    } else {
        checks.push(AuditCheck::pass(
            "mission_modes",
            "classification sans ambiguïté",
        ));
    }
    if !overlay && !added.is_empty() {
        checks.push(AuditCheck::warn(
            "mission_overlay",
            "insertion A7 désactivée mais calques annoncés",
        ));
    } else if !overlay {
        checks.push(AuditCheck::warn(
            "mission_overlay",
            "mission résolue, insertion A7 désactivée",
        ));
    }
    let sum: f64 = segments.iter().map(Segment::duration).sum();
    let coherent = meta.duration <= 0.0
        || (sum + tail_s - meta.duration).abs() <= 1.0_f64.max(0.01 * meta.duration);
    checks.push(if coherent {
        AuditCheck::pass(
            "mission_tail",
            format!(
                "segments {sum:.2}s + queue {tail_s:.2}s = comp {:.2}s",
                meta.duration
            ),
        )
    } else {
        AuditCheck::warn(
            "mission_tail",
            format!(
                "segments {sum:.2}s + queue {tail_s:.2}s != comp {:.2}s",
                meta.duration
            ),
        )
    });
    if sidecar_written {
        checks.push(AuditCheck::pass("mission_sidecar", "sidecar QA écrit"));
    } else {
        checks.push(AuditCheck::warn(
            "mission_sidecar",
            "sidecar QA absent (rapport seul ou écriture impossible)",
        ));
    }
}

fn docs_dir_for(project: &Path, projects_subfolder: &str, docs_subfolder: &str) -> PathBuf {
    let parent = project.parent().unwrap_or_else(|| Path::new("."));
    if parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == projects_subfolder)
    {
        parent
            .parent()
            .map(|root| root.join(docs_subfolder))
            .unwrap_or_else(|| parent.join(docs_subfolder))
    } else {
        let docs = parent.join(docs_subfolder);
        if docs.is_dir() {
            docs
        } else {
            parent.to_path_buf()
        }
    }
}

fn resolve_video(docs: &Path, segments: &[Segment], stem: &str) -> Option<PathBuf> {
    let mut sources: Vec<String> = segments
        .iter()
        .filter_map(|segment| segment.source.clone())
        .collect();
    sources.sort();
    sources.dedup();
    if sources.len() != 1 {
        return None;
    }
    let source = &sources[0];
    let direct = docs.join(source);
    if direct.is_file() {
        return Some(direct);
    }
    let source_stem = Path::new(source)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(stem);
    find_video(docs, source_stem).or_else(|| find_video(docs, stem))
}

fn find_video(docs: &Path, stem: &str) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(docs)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    entries.sort();
    entries.into_iter().find(|path| {
        path.file_stem().and_then(|value| value.to_str()) == Some(stem)
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    VIDEO_EXTENSIONS
                        .iter()
                        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                })
    })
}

fn find_docs_file(docs: &Path, stem: &str, suffix: &str) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(docs)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(suffix))
        })
        .collect();
    entries.sort();
    for path in &entries {
        let Some(file_stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        let normalized = file_stem.strip_suffix("_ae").unwrap_or(file_stem);
        if normalized == stem {
            return Some(path.clone());
        }
    }
    entries.into_iter().find(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(stem))
    })
}

fn apply_a1(doc: &mut Value, decisions: &[Decision]) -> Result<()> {
    let comp = comp_mut(doc)?;
    let layers = comp
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .context("composition sans calques")?;
    for decision in decisions {
        let layer = layers
            .iter_mut()
            .find(|layer| layer.get("name").and_then(Value::as_str) == Some(&decision.target))
            .with_context(|| format!("segment A1 introuvable: {}", decision.target))?;
        let scale = decision
            .details
            .get("scale_target")
            .and_then(Value::as_f64)
            .context("A1 sans scale_target")?;
        let position_y = decision
            .details
            .get("position_y_target")
            .and_then(Value::as_f64)
            .context("A1 sans position_y_target")?;
        set_scale(layer, scale)?;
        set_position_y(layer, position_y)?;
    }
    Ok(())
}

fn apply_a2(doc: &mut Value, decisions: &[Decision]) -> Result<()> {
    let comp = comp_mut(doc)?;
    let layers = comp
        .get_mut("layers")
        .and_then(Value::as_array_mut)
        .context("composition sans calques")?;
    for decision in decisions {
        let scale = decision
            .details
            .get("scale_target")
            .and_then(Value::as_f64)
            .context("A2 sans scale_target")?;
        for layer in layers.iter_mut() {
            if layer
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.starts_with("Segment"))
            {
                set_scale(layer, scale)?;
            }
        }
    }
    Ok(())
}

/// Cibles A1 à différer : A3 y a marqué un candidat fort (`strong_candidate`).
fn strong_a3_targets(decisions: &[Decision]) -> HashSet<String> {
    decisions
        .iter()
        .filter(|decision| decision.code == "A3" && decision.action == Action::Review)
        .filter(|decision| {
            decision
                .details
                .get("strong_candidate")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .map(|decision| decision.target.clone())
        .collect()
}

/// Contrôle structurel de la couverture source.
///
/// Le panel borne l'ancre pour que la source couvre la composition. Un projet
/// édité à la main peut sortir de cette plage : A5 émet alors une revue au
/// lieu de laisser un vide dans le rendu. Ces revues sont report-only et
/// résolues par le monteur (D-056) ; elles ne déclenchent aucun recadrage.
fn coverage_reviews(
    segments: &[Segment],
    meta: &CompMeta,
    source_width: u32,
    source_height: u32,
) -> Vec<Decision> {
    let mut decisions = Vec::new();
    for segment in segments {
        let factor = segment.scale / 100.0;
        let comp_y = segment.position[1] + segment.parent_dy;
        let left = segment.comp_x + (0.0 - segment.anchor[0]) * factor;
        let right = segment.comp_x + (source_width as f64 - segment.anchor[0]) * factor;
        let top = comp_y + (0.0 - segment.anchor[1]) * factor;
        let bottom = top + source_height as f64 * factor;
        let gaps = [
            left,
            meta.width as f64 - right,
            top,
            meta.height as f64 - bottom,
        ];
        let worst = gaps.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if worst > 0.5 {
            decisions.push(Decision::review(
                "A5",
                segment.name.clone(),
                format!(
                    "la source ne couvre pas la composition (vide max {worst:.1} px), clamp panel hors plage"
                ),
                0.4,
                json!({
                    "source": [source_width, source_height],
                    "comp": [meta.width, meta.height],
                    "scale": round_to(segment.scale, 3),
                    "anchor": segment.anchor,
                    "position": segment.position,
                    "gaps": {
                        "left": round_to(left, 1),
                        "right": round_to(meta.width as f64 - right, 1),
                        "top": round_to(top, 1),
                        "bottom": round_to(meta.height as f64 - bottom, 1)
                    },
                    "resolution": "resolved",
                    "report_only": true
                }),
            ));
        }
    }
    decisions
}

#[allow(clippy::too_many_arguments)]
fn audit_checks(
    segments: &[Segment],
    meta: &CompMeta,
    tail_s: f64,
    video: Option<&Path>,
    decisions: &[Decision],
    template: &a6::TemplateInfo,
    assist_written: bool,
    raw_preserved: bool,
    conflict: bool,
    transform_error: Option<&str>,
    report_only: bool,
) -> Vec<AuditCheck> {
    let mut checks = Vec::new();
    checks.push(AuditCheck::pass(
        "project_readable",
        format!(
            "{} segments, comp {}x{}",
            segments.len(),
            meta.width,
            meta.height
        ),
    ));
    if segments.is_empty() {
        checks.push(AuditCheck::fail("segments_present", "aucun calque Segment"));
    } else {
        checks.push(AuditCheck::pass(
            "segments_present",
            format!("{} segments", segments.len()),
        ));
    }
    let duration: f64 = segments.iter().map(Segment::duration).sum();
    let total_duration = duration + tail_s;
    if meta.duration > 0.0
        && (total_duration - meta.duration).abs() > 1.0_f64.max(0.01 * meta.duration)
    {
        checks.push(AuditCheck::warn(
            "duration_coherent",
            format!(
                "somme segments {duration:.2}s + queue {tail_s:.2}s vs comp {:.2}s",
                meta.duration
            ),
        ));
    } else {
        checks.push(AuditCheck::pass(
            "duration_coherent",
            if tail_s > 0.001 {
                format!("{duration:.2}s + {tail_s:.2}s")
            } else {
                format!("{duration:.2}s")
            },
        ));
    }
    let mut anchors: Vec<i64> = segments
        .iter()
        .map(|segment| (segment.anchor[0] * 1000.0).round() as i64)
        .collect();
    anchors.sort_unstable();
    anchors.dedup();
    checks.push(if segments.is_empty() {
        AuditCheck::fail("anchor_present", "aucune ancre")
    } else {
        AuditCheck::pass(
            "anchor_present",
            format!(
                "{} ancres distinctes, Anchor Point non déplacé par A1/A2",
                anchors.len()
            ),
        )
    });
    let keys: usize = segments
        .iter()
        .map(|segment| {
            segment.position_keys.len() + segment.scale_keys.len() + segment.anchor_keys.len()
        })
        .sum();
    let keyframed = segments
        .iter()
        .filter(|segment| segment.keyframed())
        .count();
    checks.push(AuditCheck::pass(
        "keyframes",
        format!("{keyframed} segments keyframés, {keys} clés"),
    ));
    let mut scales: Vec<f64> = segments
        .iter()
        .map(|segment| round_to(segment.scale, 3))
        .collect();
    scales.sort_by(f64::total_cmp);
    scales.dedup_by(|left, right| left.total_cmp(right) == std::cmp::Ordering::Equal);
    let scale_text = &scales[..scales.len().min(6)];
    checks.push(AuditCheck::pass(
        "scales",
        format!(
            "{} échelles distinctes: {:?}{}",
            scales.len(),
            scale_text,
            if scales.len() > 6 { "..." } else { "" }
        ),
    ));
    let source_stems: Vec<String> = segments
        .iter()
        .filter_map(|segment| segment.source.as_deref())
        .filter_map(|source| Path::new(source).file_stem().and_then(|stem| stem.to_str()))
        .map(str::to_string)
        .collect();
    if source_stems.is_empty() {
        checks.push(AuditCheck::warn(
            "media_paths",
            "aucune source dans les segments",
        ));
    } else if video.is_some_and(|video| {
        source_stems
            .iter()
            .all(|stem| Some(stem.as_str()) == video.file_stem().and_then(|value| value.to_str()))
    }) {
        checks.push(AuditCheck::pass(
            "media_paths",
            format!(
                "source unique: {}",
                video
                    .and_then(|video| video.file_stem())
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("?")
            ),
        ));
    } else {
        checks.push(AuditCheck::warn(
            "media_paths",
            format!(
                "sources non résolues: {:?}",
                &source_stems[..source_stems.len().min(3)]
            ),
        ));
    }
    checks.push(AuditCheck::pass(
        "actions_summary",
        format!(
            "{} actions auto, {} revues, {} noop",
            decisions
                .iter()
                .filter(|decision| decision.action == Action::Apply)
                .count(),
            decisions
                .iter()
                .filter(|decision| decision.action == Action::Review)
                .count(),
            decisions
                .iter()
                .filter(|decision| decision.action == Action::Noop)
                .count()
        ),
    ));
    let a3_high = decisions
        .iter()
        .filter(|decision| {
            decision.code == "A3"
                && decision.action == Action::Review
                && decision
                    .details
                    .get("high_confidence")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .count();
    checks.push(AuditCheck::pass(
        "a3_disabled",
        if a3_high == 0 {
            "A3 en review uniquement (D-030)".to_string()
        } else {
            format!("A3 en review uniquement (D-030/D-056), {a3_high} haute(s) confiance")
        },
    ));
    let coverage_reviews = decisions
        .iter()
        .filter(|decision| decision.code == "A5" && decision.action == Action::Review)
        .count();
    if coverage_reviews == 0 {
        checks.push(AuditCheck::pass(
            "source_coverage",
            "la source couvre la composition sur tous les segments",
        ));
    } else {
        checks.push(AuditCheck::warn(
            "source_coverage",
            format!(
                "{coverage_reviews} segment(s) hors couverture, résolus par le monteur (report-only)"
            ),
        ));
    }
    let a1_deferred = decisions
        .iter()
        .filter(|decision| {
            decision.code == "A1"
                && decision
                    .details
                    .get("a1_deferred_on_a3")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
        })
        .count();
    if a1_deferred > 0 {
        checks.push(AuditCheck::pass(
            "a1_deferred_on_a3",
            format!("{a1_deferred} segment(s) A1 différés sur candidat A3"),
        ));
    }
    if !template.matched {
        checks.push(AuditCheck::pass("a6_template", "aucun template demandé"));
    } else if template.eligible == Some(true) {
        checks.push(AuditCheck::pass(
            "a6_template",
            format!("template {} éligible", template.name),
        ));
    } else {
        checks.push(AuditCheck::warn(
            "a6_template",
            format!(
                "template {} détecté mais structure incompatible",
                template.name
            ),
        ));
    }
    if report_only {
        checks.push(AuditCheck::pass(
            "assist_written",
            "report-only, aucun projet publié",
        ));
    } else if assist_written {
        checks.push(AuditCheck::pass("assist_written", "projet assisté publié"));
    } else {
        checks.push(AuditCheck::fail("assist_written", "projet assisté absent"));
    }
    checks.push(if raw_preserved {
        AuditCheck::pass("raw_preserved", "le projet brut est inchangé")
    } else {
        AuditCheck::fail("raw_preserved", "le projet brut a changé")
    });
    if conflict {
        checks.push(AuditCheck::warn(
            "a1_a2_conflict",
            "A1 et A2 simultanément applicables, aucune action appliquée",
        ));
    }
    if let Some(error) = transform_error {
        checks.push(AuditCheck::fail("transform", error));
    }
    checks
}

#[allow(clippy::too_many_arguments)]
fn post_json(
    project: &Path,
    assist: &Path,
    video: Option<&Path>,
    ae: Option<&Path>,
    csv: Option<&Path>,
    specs: Option<&detect::VideoSpecs>,
    meta: &CompMeta,
    template: &a6::TemplateInfo,
    timings: &Map<String, Value>,
    decisions: &[Decision],
    summary: &PostSummary,
    mission: Option<&Value>,
    report_only: bool,
) -> Value {
    let mut object = Map::new();
    object.insert("schema".into(), json!(1));
    object.insert("generated_at".into(), json!(now_iso()));
    object.insert("project".into(), json!(project.to_string_lossy()));
    object.insert(
        "assist_project".into(),
        if report_only {
            Value::Null
        } else {
            json!(assist.to_string_lossy())
        },
    );
    object.insert(
        "video".into(),
        json!(video.map(|path| path.to_string_lossy())),
    );
    object.insert("ae".into(), json!(ae.map(|path| path.to_string_lossy())));
    object.insert("csv".into(), json!(csv.map(|path| path.to_string_lossy())));
    object.insert("specs".into(), json!(specs));
    object.insert("comp".into(), json!(meta));
    object.insert(
        "template".into(),
        if template.matched {
            json!(template)
        } else {
            Value::Null
        },
    );
    object.insert("timings_ms".into(), Value::Object(timings.clone()));
    object.insert(
        "actions".into(),
        Value::Array(decisions.iter().map(Decision::to_json).collect()),
    );
    object.insert("summary".into(), json!(summary));
    if let Some(mission) = mission {
        object.insert("mission".into(), mission.clone());
    }
    Value::Object(object)
}

fn review_json(
    project: &Path,
    video: Option<&Path>,
    decisions: &[Decision],
    generated_at: &str,
) -> Value {
    let mut object = Map::new();
    object.insert("schema".into(), json!(1));
    object.insert("generated_at".into(), json!(generated_at));
    object.insert("project".into(), json!(project.to_string_lossy()));
    object.insert(
        "video".into(),
        json!(video.map(|path| path.to_string_lossy())),
    );
    object.insert(
        "items".into(),
        Value::Array(
            decisions
                .iter()
                .filter(|decision| decision.action == Action::Review)
                .map(Decision::to_review_json)
                .collect(),
        ),
    );
    Value::Object(object)
}

#[allow(clippy::too_many_arguments)]
fn audit_json(
    project: &Path,
    assist: &Path,
    raw_sha: &str,
    assist_sha: Option<&str>,
    video: Option<&Path>,
    video_hash: Option<&crate::post::model::VideoHash>,
    specs: Option<&detect::VideoSpecs>,
    meta: &CompMeta,
    checks: &[AuditCheck],
    audit_verdict: &str,
    summary: &PostSummary,
    timings: &Map<String, Value>,
    error: Option<&str>,
    report_only: bool,
) -> Value {
    let mut object = Map::new();
    object.insert("schema".into(), json!(1));
    object.insert("generated_at".into(), json!(now_iso()));
    object.insert("project".into(), json!(project.to_string_lossy()));
    object.insert("project_sha256".into(), json!(raw_sha));
    object.insert(
        "assist_project".into(),
        if report_only {
            Value::Null
        } else {
            json!(assist.to_string_lossy())
        },
    );
    object.insert("assist_sha256".into(), json!(assist_sha));
    object.insert(
        "video".into(),
        json!(video.map(|path| path.to_string_lossy())),
    );
    object.insert(
        "video_sha256".into(),
        json!(video_hash.map(|hash| hash.sha256.clone())),
    );
    object.insert("video_hash".into(), json!(video_hash));
    object.insert("specs".into(), json!(specs));
    object.insert("comp".into(), json!(meta));
    object.insert("checks".into(), json!(checks));
    object.insert("verdict".into(), json!(audit_verdict));
    object.insert(
        "counts".into(),
        json!({"apply": summary.apply, "review": summary.review, "noop": summary.noop}),
    );
    object.insert("timings_ms".into(), Value::Object(timings.clone()));
    object.insert("error".into(), json!(error));
    Value::Object(object)
}

fn write_artifacts(dir: &Path, post: &Value, review: &Value, audit: &Value) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("création de {}", dir.display()))?;
    write_json_1(&dir.join("post.json"), post)?;
    write_json_1(&dir.join("review.json"), review)?;
    write_json_1(&dir.join("audit.json"), audit)?;
    Ok(())
}

fn write_json_1(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = Vec::new();
    let formatter = PrettyFormatter::with_indent(b"  ");
    let mut serializer = Serializer::with_formatter(&mut bytes, formatter);
    value.serialize(&mut serializer)?;
    write_atomic(path, &bytes).with_context(|| format!("publication de {}", path.display()))
}

fn artifacts_dir(docs: &Path, options: &PostOptions, assist: &Path) -> PathBuf {
    let stem = assist
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("projet");
    docs.join(&options.artifacts_subfolder).join(stem)
}

fn failure_outcome(
    project: &Path,
    assist: Option<&Path>,
    docs: &Path,
    options: &PostOptions,
    error: String,
    started: Instant,
) -> PostProjectOutcome {
    let generated_at = now_iso();
    let summary = PostSummary::default();
    let checks = vec![
        AuditCheck::fail("project_readable", error.clone()),
        if options.report_only {
            AuditCheck::pass("assist_written", "report-only, aucun projet publié")
        } else if assist.is_some_and(Path::is_file) {
            AuditCheck::pass("assist_written", "copie du brut publiée")
        } else {
            AuditCheck::fail("assist_written", "projet assisté absent")
        },
        AuditCheck::pass("raw_preserved", "le projet brut n'a pas été modifié"),
    ];
    let audit_verdict = verdict(&checks).to_string();
    let mut timings = Map::new();
    timings.insert(
        "total_ms".into(),
        json!(round_to(started.elapsed().as_secs_f64() * 1000.0, 1)),
    );
    let post = json!({
        "schema": 1,
        "generated_at": generated_at,
        "project": project.to_string_lossy(),
        "assist_project": assist.map(|path| path.to_string_lossy()),
        "error": error,
        "actions": [],
        "summary": summary,
        "template": Value::Null,
        "timings_ms": timings
    });
    let review = json!({
        "schema": 1,
        "generated_at": generated_at,
        "project": project.to_string_lossy(),
        "items": [{
            "code": "A0",
            "severity": "high",
            "target": project.file_name().and_then(|name| name.to_str()).unwrap_or("projet"),
            "reason": error,
            "confidence": 0.0,
            "details": {}
        }]
    });
    let audit = json!({
        "schema": 1,
        "generated_at": generated_at,
        "project": project.to_string_lossy(),
        "checks": checks,
        "verdict": audit_verdict,
        "counts": {"apply": 0, "review": 0, "noop": 0},
        "timings_ms": timings,
        "error": error
    });
    let artifacts = assist.map(|assist| artifacts_dir(docs, options, assist));
    let artifacts_written = artifacts
        .as_deref()
        .map(|dir| write_artifacts(dir, &post, &review, &audit).is_ok())
        .unwrap_or(false);
    PostProjectOutcome {
        project: project.to_string_lossy().to_string(),
        assist_project: assist
            .filter(|_| !options.report_only)
            .map(|path| path.to_string_lossy().to_string()),
        artifacts_dir: artifacts_written
            .then(|| artifacts.map(|path| path.to_string_lossy().to_string()))
            .flatten(),
        summary,
        audit_verdict,
        error: Some(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_options_carry_mission_overlays_from_config() {
        let mut config = workflow_core::Config::default();
        config.paths.mission_overlays = Some(PathBuf::from("assets/mission_overlays"));
        let options = PostOptions::from_config(&config, "docs");
        assert_eq!(
            options.mission_overlays,
            Some(PathBuf::from("assets/mission_overlays"))
        );
        config.paths.mission_overlays = Some(PathBuf::new());
        assert_eq!(
            PostOptions::from_config(&config, "docs").mission_overlays,
            None
        );
    }

    fn project_doc() -> Value {
        json!({
            "next_id": 20,
            "items": {
                "1": {"name": "clip.mp4", "kind": {"type": "Footage", "width": 1920, "height": 1080}},
                "2": {
                    "name": "Comp",
                    "kind": {
                        "type": "Comp",
                        "width": 1080,
                        "height": 1920,
                        "frame_rate": {"num": 25, "den": 1},
                        "duration": 2540160000000_i64,
                        "layers": [
                            {
                                "id": 10,
                                "name": "Segment 1",
                                "in_point": 0,
                                "out_point": 2540160000000_i64,
                                "source": {"type": "Footage", "item": 1},
                                "props": {"children": [
                                    {"name": "Effects", "children": []},
                                    {"name": "Transform", "children": [
                                        {"name": "Anchor Point", "value": {"v": [960.0, 0.0, 0.0]}},
                                        {"name": "Position", "value": {"v": [540.0, 0.0, 0.0]}},
                                        {"name": "Scale", "value": {"v": [180.0, 180.0, 0.0]}}
                                    ]}
                                ]}
                            }
                        ]
                    }
                }
            }
        })
    }

    fn segment_for_coverage(scale: f64, anchor_x: f64, position_x: f64) -> Segment {
        Segment {
            name: "Segment 1".into(),
            layer_id: 10,
            parent: None,
            start: 0.0,
            end: 10.0,
            source: Some("clip.mp4".into()),
            anchor: [anchor_x, 0.0, 0.0],
            scale,
            position: [position_x, 0.0, 0.0],
            position_keys: Vec::new(),
            scale_keys: Vec::new(),
            anchor_keys: Vec::new(),
            comp_x: position_x,
            parent_dy: 0.0,
            null_parent: false,
            parent_keyframed: false,
            rotated: false,
        }
    }

    #[test]
    fn source_coverage_gap_is_reviewed() {
        let meta = CompMeta {
            id: "2".into(),
            width: 1080,
            height: 1920,
            fps: 25.0,
            duration: 10.0,
            n_layers: 1,
            n_segments: 1,
        };
        let covered = coverage_reviews(
            &[segment_for_coverage(180.0, 960.0, 540.0)],
            &meta,
            1920,
            1080,
        );
        assert!(covered.is_empty());
        let gap = coverage_reviews(
            &[segment_for_coverage(50.0, 960.0, 540.0)],
            &meta,
            1920,
            1080,
        );
        assert_eq!(gap.len(), 1);
        assert_eq!(gap[0].code, "A5");
        assert_eq!(gap[0].action, Action::Review);
        assert!(gap[0].reason.contains("ne couvre pas"));
        assert_eq!(gap[0].details["resolution"], "resolved");
        assert_eq!(gap[0].details["report_only"], true);
    }

    #[test]
    fn strong_a3_targets_only_keep_reviewed_strong_candidates() {
        let decisions = vec![
            Decision::review(
                "A3",
                "Segment 1",
                "fort",
                0.35,
                json!({"strong_candidate": true}),
            ),
            Decision::review("A3", "Segment 2", "faible", 0.35, json!({})),
            Decision::noop(
                "A3",
                "Segment 3",
                "noop",
                0.6,
                json!({"strong_candidate": true}),
            ),
            Decision::review(
                "A4",
                "Segment 4",
                "autre étage",
                0.5,
                json!({"strong_candidate": true}),
            ),
        ];
        let targets = strong_a3_targets(&decisions);
        assert_eq!(targets.len(), 1);
        assert!(targets.contains("Segment 1"));
    }

    #[test]
    fn post_defaults_follow_d056() {
        let options = PostOptions::default();
        assert_eq!(options.a1_margin, 1.015);
        assert_eq!(options.a1_margin, a1::A1Options::default().margin);
        assert_eq!(
            options.a1_margin,
            workflow_core::config::PostConfig::default().a1_margin
        );
        assert!(!options.a1_defer_on_a3);
        assert!(!options.a1_refine_native);
        assert!(!options.a3_relaxed_gap);
        assert_eq!(
            options.a1_refine_native,
            workflow_core::config::PostConfig::default().a1_refine_native
        );
    }

    #[test]
    fn native_refine_flag_is_propagated_from_config() {
        let config = workflow_core::config::Config::from_toml(
            "[steps.step8.post]\na1_refine_native = true\n",
        )
        .unwrap();
        let options = PostOptions::from_config(&config, "docs");
        assert!(options.a1_refine_native);
        assert!(!PostOptions::default().a1_refine_native);
    }

    #[test]
    fn always_publishes_an_assist_copy_even_without_media() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projet");
        fs::create_dir_all(root.join("projets")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        let project = root.join("projets").join("clip_9x16.ecproj");
        fs::write(&project, serde_json::to_vec(&project_doc()).unwrap()).unwrap();
        let options = PostOptions::default();
        let outcome = run_post_project(&options, &project, None);
        assert!(outcome.assist_project.is_some());
        assert!(root
            .join("projets")
            .join("clip_9x16_assist.ecproj")
            .is_file());
        assert_eq!(outcome.audit_verdict, "warn");
        assert!(outcome.summary.review >= 1);
    }

    #[test]
    fn assist_is_byte_identical_when_no_decision_applies() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projet");
        fs::create_dir_all(root.join("projets")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        let project = root.join("projets").join("clip_9x16.ecproj");
        let bytes = serde_json::to_vec(&project_doc()).unwrap();
        fs::write(&project, &bytes).unwrap();
        let options = PostOptions::default();
        let outcome = run_post_project(&options, &project, None);
        assert!(outcome.error.is_none());
        let assist = root.join("projets").join("clip_9x16_assist.ecproj");
        assert_eq!(fs::read(assist).unwrap(), bytes);
    }

    #[test]
    fn audit_failure_is_reported_as_post_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projet");
        fs::create_dir_all(root.join("projets")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        let mut doc = project_doc();
        doc["items"]["2"]["kind"]["layers"] = json!([]);
        let project = root.join("projets").join("clip_9x16.ecproj");
        fs::write(&project, serde_json::to_vec(&doc).unwrap()).unwrap();
        let outcome = run_post_project(&PostOptions::default(), &project, None);
        assert_eq!(outcome.audit_verdict, "fail");
        assert!(outcome.error.is_some());
        assert!(root
            .join("projets")
            .join("clip_9x16_assist.ecproj")
            .is_file());
    }

    #[test]
    fn report_only_does_not_publish_assist() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("projet");
        fs::create_dir_all(root.join("projets")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        let project = root.join("projets").join("clip_9x16.ecproj");
        fs::write(&project, serde_json::to_vec(&project_doc()).unwrap()).unwrap();
        let options = PostOptions {
            report_only: true,
            ..PostOptions::default()
        };
        let outcome = run_post_project(&options, &project, None);
        assert!(outcome.assist_project.is_none());
        assert!(!root
            .join("projets")
            .join("clip_9x16_assist.ecproj")
            .exists());
    }

    #[test]
    fn run_post_for_folders_processes_every_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut folders = Vec::new();
        for name in ["a", "b"] {
            let root = dir.path().join(name);
            fs::create_dir_all(root.join("projets")).unwrap();
            fs::create_dir_all(root.join("docs")).unwrap();
            fs::write(
                root.join("projets").join("clip_9x16.ecproj"),
                serde_json::to_vec(&project_doc()).unwrap(),
            )
            .unwrap();
            folders.push(root.to_string_lossy().to_string());
        }

        let summary = run_post_for_folders(&folders, &PostOptions::default());

        assert_eq!(summary.projects, 2);
        assert_eq!(summary.assists, 2);
        assert!(summary.failures.is_empty());
        for name in ["a", "b"] {
            assert!(dir
                .path()
                .join(name)
                .join("projets")
                .join("clip_9x16_assist.ecproj")
                .is_file());
        }
    }

    #[test]
    fn archive_only_project_is_not_post_processed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("341 Camille");
        fs::create_dir_all(root.join("projets")).unwrap();
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(root.join("docs/W9 - LPHDF - La découverte.mp4"), b"video").unwrap();
        let project = root
            .join("projets")
            .join("W9 - LPHDF - La découverte_9x16.ecproj");
        fs::write(&project, serde_json::to_vec(&project_doc()).unwrap()).unwrap();
        let options = PostOptions {
            direct: true,
            archive_only_prefixes: vec!["W9 - LPHDF".into()],
            ..PostOptions::default()
        };
        let summary = run_post_dir(&options, &root);
        assert_eq!(summary.projects, 0);
        assert!(!root
            .join("projets")
            .join("W9 - LPHDF - La découverte_9x16_assist.ecproj")
            .exists());
    }
}
