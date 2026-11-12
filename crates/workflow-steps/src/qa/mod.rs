//! Contrôle qualité en lecture seule de l'export final (scénario B).
//!
//! `qa run` intervient **après** l'export humain, pas dans STEP8. Le module ne
//! modifie ni le `.ecproj`, ni le MP4, ni `docs/` ; il écrit uniquement
//! `qa.json` sous `--out`.
//!
//! État v1 : Stage 0 déterministe (specs, durée, timeline, template A6,
//! noir/gel), Stage 1 OCR sous-titres et logos (PP-OCRv6) et visage (YuNet),
//! agrégation. Le contrôle logos exige un `.ecproj` pour masquer les overlays
//! déclarés ; sans projet il reste `not_applicable`. DINOv2 est actif par
//! défaut (`--no-logo-visual` pour l'opt-out). `--source-logo` ajoute, en
//! opt-in, l'annotation du registre spatial des logos source (D-052) : le
//! check `source_logo` est `pass` ou `info`, jamais bloquant.
//! `--source-logo-visual` (ou `--source-logo-references`) ajoute la
//! confirmation locale NCC/contours et la stabilité temporelle des zones
//! génériques, toujours sans décision. `auto_approve_eligible` n'est vrai que
//! sur un `pass` complet, avec les contrôles requis exécutés.

pub mod aggregate;
pub mod batch;
pub mod cache;
pub mod evidence;
pub mod face;
pub mod logo;
pub mod model;
pub mod ocr;
pub mod report;
pub mod sample;
pub mod source_logo;
pub mod source_logo_visual;
pub mod stage0;

pub use crate::post::overlay::{read_mask, OverlayMask};
pub use crate::vision::dino::{DinoOptions, DINO_MODEL_FILENAME, DINO_PROBE_FILENAME};
pub use batch::{discover_exports, run_folder, BatchOutcome, BatchSummary};
pub use face::{FaceOptions, FACE_MODEL_FILENAME};
pub use model::{
    DefectType, OverlayRegion, QaCheck, QaFinding, QaReport, QaStatus, Severity, Stage0Thresholds,
    Stage1FaceEvidence, Stage1LogoEvidence, Stage1LogoVisualEvidence, Stage1OcrEvidence,
};
pub use ocr::{OcrOptions, OCR_DET_FILENAME, OCR_REC_FILENAME};
pub use report::{read_report, report_path, write_report};
pub use sample::SegmentSpan;
pub use stage0::{run_stage0, Stage0Options, Stage0Result};

use crate::post::model::now_iso;
use crate::qa::model::{ExecutionMetrics, ModelsProvenance, QA_SCHEMA_VERSION};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct QaOptions {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub template_prefixes: Vec<String>,
    /// Répertoire de sortie explicite : jamais le dossier du MP4 par défaut.
    pub out_dir: PathBuf,
    /// Répertoire des modèles QA (YuNet, OCR).
    pub models_dir: Option<PathBuf>,
    /// Bibliothèque ONNX Runtime explicite.
    pub onnxruntime: Option<PathBuf>,
    /// Threads intra-op ONNX par session (None : défaut du runtime).
    pub intra_threads: Option<usize>,
    /// Active le contrôle OCR sous-titres.
    pub ocr_enabled: bool,
    /// Active le contrôle logos de la bande haute.
    pub logo_enabled: bool,
    /// Active le vérificateur visuel DINOv2 des candidats logos.
    pub logo_visual_enabled: bool,
    /// Active le contrôle visage YuNet.
    pub face_enabled: bool,
    /// Active l'annotation du logo source (registre spatial D-052). Opt-in :
    /// aucune décision, seulement un check `pass`/`info`.
    pub source_logo_enabled: bool,
    /// Active la confirmation visuelle locale (NCC/contours et stabilité des
    /// zones génériques). Les références restent externes.
    pub source_logo_visual: bool,
    /// Catalogue TOML explicite ; sinon `<models>/qa/source_logos.toml`.
    pub source_logo_catalog: Option<PathBuf>,
    /// Racine des crops RGBA de référence ; sinon `reference_root` du
    /// catalogue.
    pub source_logo_references: Option<PathBuf>,
    /// Sidecar de mission explicite. Sans valeur, il est recherché à côté
    /// du projet (`<docs>/<stem>_mission.json`).
    pub mission_sidecar: Option<PathBuf>,
    /// Écrit une frame WebP par finding sous `<out>/evidence/`.
    pub evidence_enabled: bool,
    /// 1 ou 3 échantillons par segment.
    pub per_segment: usize,
    pub video_id: Option<String>,
    pub folder_id: Option<String>,
    pub split: Option<String>,
}

impl Default for QaOptions {
    fn default() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            template_prefixes: crate::post::a6::default_prefixes(),
            out_dir: PathBuf::new(),
            models_dir: None,
            onnxruntime: None,
            intra_threads: None,
            ocr_enabled: true,
            logo_enabled: true,
            logo_visual_enabled: true,
            face_enabled: true,
            source_logo_enabled: false,
            source_logo_visual: false,
            source_logo_catalog: None,
            source_logo_references: None,
            mission_sidecar: None,
            evidence_enabled: true,
            per_segment: 1,
            video_id: None,
            folder_id: None,
            split: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QaOutcome {
    pub report: QaReport,
    pub qa_path: PathBuf,
}

/// Recherche le sidecar de mission correspondant à l'export ou au projet.
pub(crate) fn find_mission_sidecar(video: &Path, project: Option<&Path>) -> Option<PathBuf> {
    if let Some(project) = project {
        if let Some(path) = find_mission_sidecar_for_project(project) {
            return Some(path);
        }
    }
    let mut docs_dirs = Vec::new();
    if let Some(parent) = video.parent() {
        docs_dirs.push(parent.join("docs"));
    }
    if let Some(project) = project {
        if let Some(parent) = project.parent() {
            docs_dirs.push(parent.join("docs"));
            if parent
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "projets" || name == "projets_extraits")
            {
                if let Some(root) = parent.parent() {
                    docs_dirs.push(root.join("docs"));
                }
            }
        }
    }
    let video_key = workflow_core::mission::filename_key(
        video
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default(),
    );
    let video_key = strip_project_suffix(&video_key);
    let mut matches = Vec::new();
    for docs in docs_dirs {
        for path in mission_sidecars(&docs) {
            if let Some(value) = read_sidecar(&path) {
                let stem = value
                    .get("project_stem")
                    .and_then(serde_json::Value::as_str)
                    .map(workflow_core::mission::filename_key)
                    .unwrap_or_default();
                let stem = strip_project_suffix(&stem);
                let project_matches = project.is_some_and(|project| {
                    value
                        .get("project")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|candidate| Path::new(candidate) == project)
                });
                if stem == video_key || project_matches {
                    matches.push(path);
                }
            }
        }
    }
    matches.sort();
    matches.dedup();
    if matches.len() == 1 {
        Some(matches.remove(0))
    } else {
        None
    }
}

pub(crate) fn find_mission_sidecar_for_project(project: &Path) -> Option<PathBuf> {
    let parent = project.parent()?;
    let docs = if parent
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "projets" || name == "projets_extraits")
    {
        parent.parent()?.join("docs")
    } else {
        parent.join("docs")
    };
    let project_stem = project
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let project_key = strip_project_suffix(&workflow_core::mission::filename_key(project_stem));
    let mut matches = Vec::new();
    for path in mission_sidecars(&docs) {
        if let Some(value) = read_sidecar(&path) {
            let stem = value
                .get("project_stem")
                .and_then(serde_json::Value::as_str)
                .map(workflow_core::mission::filename_key)
                .unwrap_or_default();
            let stem = strip_project_suffix(&stem);
            let exact_project = value
                .get("project")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|candidate| Path::new(candidate) == project);
            if stem == project_key || exact_project {
                matches.push(path);
            }
        }
    }
    matches.sort();
    matches.dedup();
    if matches.len() == 1 {
        Some(matches.remove(0))
    } else {
        None
    }
}

fn mission_sidecars(docs: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(docs) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with("_mission.json"))
        })
        .collect();
    paths.sort();
    paths
}

fn read_sidecar(path: &Path) -> Option<serde_json::Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn strip_project_suffix(stem: &str) -> String {
    let mut stem = stem.trim_end_matches("_assist").to_string();
    stem = stem.trim_end_matches("_9x16").to_string();
    stem.trim_end_matches("_assist").to_string()
}

pub fn run_qa(video: &Path, project: Option<&Path>, options: &QaOptions) -> Result<QaOutcome> {
    if !video.is_file() {
        bail!("vidéo introuvable: {}", video.display());
    }
    if let Some(project) = project {
        if !project.is_file() {
            bail!("projet introuvable: {}", project.display());
        }
    }
    if options.out_dir.as_os_str().is_empty() {
        bail!("passer un répertoire --out explicite");
    }

    let started = Instant::now();
    let mission_sidecar = options
        .mission_sidecar
        .clone()
        .or_else(|| find_mission_sidecar(video, project));
    let stage0 = run_stage0(
        video,
        project,
        &Stage0Options {
            ffmpeg: options.ffmpeg.clone(),
            ffprobe: options.ffprobe.clone(),
            template_prefixes: options.template_prefixes.clone(),
            mission_sidecar,
            thresholds: Stage0Thresholds::default(),
        },
    )
    .with_context(|| format!("Stage 0 sur {}", video.display()))?;
    let export_sha256 = crate::post::detect::sha256_file(video)
        .with_context(|| format!("hachage SHA-256 de {}", video.display()))?;
    let cache_key = cache::fingerprint_from_export(&export_sha256, project, options)
        .with_context(|| format!("empreinte de cache QA de {}", video.display()))?;

    let mut checks = stage0.checks;
    let mut findings = stage0.findings;
    let mut ocr_evidence = None;
    let mut logo_evidence = None;
    let mut logo_visual_evidence = None;
    let mut ocr_model = None;
    let mut ocr_rec_model = None;
    let mut logo_visual_model = None;
    let mut ocr_ms = 0;
    let mut logo_ms = 0;
    let mut logo_visual_ms = 0;
    let mut face_evidence = None;
    let mut face_model = None;
    let mut face_ms = 0;

    let spans = stage0
        .segments
        .iter()
        .map(|segment| SegmentSpan {
            name: segment.name.clone(),
            start: segment.start,
            end: segment.end,
        })
        .collect::<Vec<_>>();

    if options.source_logo_enabled {
        checks.push(source_logo_check(video, project, &stage0.specs, options));
    }

    let mut ocr_options = if !options.ocr_enabled {
        checks.push(QaCheck::not_applicable(
            "subtitle_ocr",
            "contrôle OCR désactivé (--no-ocr)",
        ));
        None
    } else if let Some(models_dir) = &options.models_dir {
        let det_model = models_dir.join(OCR_DET_FILENAME);
        let rec_model = models_dir.join(OCR_REC_FILENAME);
        if !det_model.is_file() || !rec_model.is_file() {
            checks.push(QaCheck::not_applicable(
                "subtitle_ocr",
                format!(
                    "modèles OCR introuvables sous {} ({OCR_DET_FILENAME}, {OCR_REC_FILENAME})",
                    models_dir.display()
                ),
            ));
            None
        } else {
            Some(OcrOptions {
                det_model,
                rec_model,
                onnxruntime: options.onnxruntime.clone(),
                intra_threads: options.intra_threads,
                ffmpeg: options.ffmpeg.clone(),
                per_segment: options.per_segment,
                logo_visual: None,
            })
        }
    } else {
        checks.push(QaCheck::not_applicable(
            "subtitle_ocr",
            "modèles OCR non fournis (--models)",
        ));
        None
    };

    // Le masque des overlays déclarés est indispensable : sans projet, un OCR
    // de bande haute confond logo de chaîne et habillage éditorial.
    let mut logo_mask = None;
    if !options.logo_enabled {
        checks.push(QaCheck::not_applicable(
            "logo_residual",
            "contrôle logos désactivé (--no-logo)",
        ));
    } else if ocr_options.is_none() {
        checks.push(QaCheck::not_applicable(
            "logo_residual",
            "contrôle OCR désactivé ou modèles OCR absents",
        ));
    } else if let Some(project_path) = project {
        match crate::post::overlay::read_mask_with(project_path, &options.ffmpeg, &options.ffprobe)
        {
            Ok(mask) if mask.regions.is_empty() && !mask.skipped.is_empty() => {
                tracing::warn!(
                    skipped = mask.skipped.len(),
                    "aucun overlay exploitable, contrôle logos désactivé"
                );
                checks.push(QaCheck::not_applicable(
                    "logo_residual",
                    format!(
                        "overlays déclarés non exploitables: {}",
                        mask.skipped.join(" ; ")
                    ),
                ));
            }
            Ok(mask) => {
                tracing::info!(
                    overlays = mask.regions.len(),
                    skipped = mask.skipped.len(),
                    "masque d'overlays chargé"
                );
                logo_mask = Some(mask);
            }
            Err(error) => {
                tracing::warn!(
                    project = %project_path.display(),
                    error = %error,
                    "masque d'overlays illisible"
                );
                checks.push(QaCheck::not_applicable(
                    "logo_residual",
                    format!("masque d'overlays illisible: {error:#}"),
                ));
            }
        }
    } else {
        checks.push(QaCheck::not_applicable(
            "logo_residual",
            "projet .ecproj requis pour le masque d'overlays",
        ));
    }

    // Le vérificateur visuel est actif par défaut. Sans modèle et probe
    // présents, le contrôle reste explicitement `not_applicable` au lieu
    // d'échouer ; l'auto-approbation est alors refusée.
    let mut logo_visual_options = None;
    if options.logo_visual_enabled {
        if !options.logo_enabled || !options.ocr_enabled {
            checks.push(QaCheck::not_applicable(
                "logo_visual",
                "contrôle logos ou OCR désactivé",
            ));
        } else if logo_mask.is_none() {
            checks.push(QaCheck::not_applicable(
                "logo_visual",
                "projet .ecproj requis pour le masque d'overlays",
            ));
        } else if let Some(models_dir) = &options.models_dir {
            let model = models_dir.join(DINO_MODEL_FILENAME);
            let probe = models_dir.join(DINO_PROBE_FILENAME);
            if !model.is_file() || !probe.is_file() {
                checks.push(QaCheck::not_applicable(
                    "logo_visual",
                    format!(
                        "modèle DINOv2 ou probe introuvable sous {} ({DINO_MODEL_FILENAME}, {DINO_PROBE_FILENAME})",
                        models_dir.display()
                    ),
                ));
            } else {
                logo_visual_options = Some(DinoOptions {
                    model,
                    probe,
                    onnxruntime: options.onnxruntime.clone(),
                    intra_threads: options.intra_threads,
                });
            }
        } else {
            checks.push(QaCheck::not_applicable(
                "logo_visual",
                "modèles non fournis (--models)",
            ));
        }
    }
    if let Some(ref mut ocr_opts) = ocr_options {
        ocr_opts.logo_visual = logo_visual_options;
    }

    let face_options = if !options.face_enabled {
        checks.push(QaCheck::not_applicable(
            "face_cut",
            "contrôle visage désactivé (--no-face)",
        ));
        None
    } else if let Some(models_dir) = &options.models_dir {
        let model = models_dir.join(FACE_MODEL_FILENAME);
        if !model.is_file() {
            checks.push(QaCheck::not_applicable(
                "face_cut",
                format!("modèle YuNet introuvable: {}", model.display()),
            ));
            None
        } else {
            Some(FaceOptions {
                model,
                onnxruntime: options.onnxruntime.clone(),
                intra_threads: options.intra_threads,
                ffmpeg: options.ffmpeg.clone(),
                per_segment: options.per_segment,
            })
        }
    } else {
        checks.push(QaCheck::not_applicable(
            "face_cut",
            "modèle YuNet non fourni (--models)",
        ));
        None
    };

    // Les deux contrôles sont indépendants et CPU-bound : les lancer en
    // parallèle divise la latence du QA complet sur les exports longs.
    let (ocr_result, face_result) = match (&ocr_options, &face_options) {
        (Some(ocr_opts), Some(face_opts)) => {
            let logo_ref = logo_mask.as_ref();
            let (ocr_joined, face_joined) = std::thread::scope(|scope| {
                let ocr_handle =
                    scope.spawn(|| ocr::run_ocr(video, &stage0.specs, &spans, ocr_opts, logo_ref));
                let face_handle =
                    scope.spawn(|| face::run_face(video, &stage0.specs, &spans, face_opts));
                (ocr_handle.join(), face_handle.join())
            });
            (
                Some(join_stage(ocr_joined, "OCR")?),
                Some(join_stage(face_joined, "visage")?),
            )
        }
        (Some(ocr_opts), None) => (
            Some(ocr::run_ocr(
                video,
                &stage0.specs,
                &spans,
                ocr_opts,
                logo_mask.as_ref(),
            )?),
            None,
        ),
        (None, Some(face_opts)) => (
            None,
            Some(face::run_face(video, &stage0.specs, &spans, face_opts)?),
        ),
        (None, None) => (None, None),
    };

    if let Some(result) = ocr_result {
        checks.extend(result.checks);
        findings.extend(result.findings);
        ocr_evidence = Some(result.evidence);
        logo_evidence = result.logo_evidence;
        logo_visual_evidence = result.logo_visual_evidence;
        ocr_model = Some(result.det_model);
        ocr_rec_model = Some(result.rec_model);
        logo_visual_model = result.logo_visual_model;
        ocr_ms = result.timing_ms;
        logo_ms = result.logo_timing_ms;
        logo_visual_ms = result.logo_visual_timing_ms;
    }
    if let Some(result) = face_result {
        checks.extend(result.checks);
        findings.extend(result.findings);
        face_evidence = Some(result.evidence);
        face_model = Some(result.model);
        face_ms = result.timing_ms;
    }
    let evidence_ms = if options.evidence_enabled && !findings.is_empty() {
        let stats = evidence::write_evidence_frames(
            video,
            &mut findings,
            &options.out_dir,
            &options.ffmpeg,
            stage0.specs.duration_s,
        );
        if stats.errors > 0 {
            tracing::warn!(
                frames = stats.frames,
                errors = stats.errors,
                "certaines preuves WebP sont absentes"
            );
        }
        stats.timing_ms
    } else {
        0
    };

    let overall_status = aggregate::overall_status(&checks, &findings);
    let (auto_approve_eligible, auto_approve_reason) = aggregate::auto_approve(&checks, &findings);
    let mut timings = stage0.timings;
    timings.stage1_ocr_ms = ocr_ms;
    timings.stage1_logo_ms = logo_ms;
    timings.stage1_logo_visual_ms = logo_visual_ms;
    timings.stage1_face_ms = face_ms;
    timings.evidence_ms = evidence_ms;
    timings.total_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let mut provenance = ModelsProvenance::stage0_only();
    provenance.ocr_det = ocr_model;
    provenance.ocr_rec = ocr_rec_model;
    provenance.face = face_model;
    provenance.embedding = logo_visual_model;

    let video_id = options
        .video_id
        .clone()
        .unwrap_or_else(|| video_id_from_path(video));
    let report = QaReport {
        schema_version: QA_SCHEMA_VERSION.into(),
        generated_at: now_iso(),
        video_id,
        folder_id: options.folder_id.clone(),
        split: options.split.clone(),
        video: video.to_string_lossy().into_owned(),
        export_sha256,
        cache_key,
        project: project.map(|path| path.to_string_lossy().into_owned()),
        specs: stage0.specs,
        project_meta: stage0.project_meta,
        project_structure: stage0.project_structure,
        checks,
        findings,
        stage0_evidence: stage0.evidence,
        stage1_ocr_evidence: ocr_evidence,
        stage1_logo_evidence: logo_evidence,
        stage1_logo_visual_evidence: logo_visual_evidence,
        stage1_face_evidence: face_evidence,
        overall_status,
        auto_approve_eligible,
        auto_approve_reason,
        models_provenance: provenance,
        execution_metrics: ExecutionMetrics {
            timings_ms: timings,
        },
    };
    let qa_path = report::write_report(&options.out_dir, &report)?;
    Ok(QaOutcome { report, qa_path })
}

/// Résout et exécute l'annotation du logo source. Le contrôle est opt-in :
/// sans catalogue, il est explicitement `not_applicable` plutôt que bloquant.
fn source_logo_check(
    video: &Path,
    project: Option<&Path>,
    specs: &crate::qa::model::QaSpecs,
    options: &QaOptions,
) -> QaCheck {
    let Some(project) = project else {
        return QaCheck::not_applicable(
            "source_logo",
            "projet .ecproj requis pour projeter le logo source",
        );
    };
    let catalog = options.source_logo_catalog.clone().or_else(|| {
        options
            .models_dir
            .as_ref()
            .map(|directory| directory.join(crate::vision::source_logo::CATALOG_FILENAME))
    });
    let Some(catalog) = catalog else {
        return QaCheck::not_applicable(
            "source_logo",
            "catalogue non fourni (--models ou --source-logo-catalog)",
        );
    };
    if !catalog.is_file() {
        return QaCheck::not_applicable(
            "source_logo",
            format!("catalogue introuvable: {}", catalog.display()),
        );
    }
    let visual = options
        .source_logo_visual
        .then(|| source_logo_visual::VisualOptions {
            video,
            ffmpeg: &options.ffmpeg,
            frame_width: specs.width,
            frame_height: specs.height,
            references: options.source_logo_references.as_deref(),
        });
    match source_logo::run(project, &catalog, visual.as_ref()) {
        Ok(check) => check,
        Err(error) => {
            tracing::warn!(
                project = %project.display(),
                catalog = %catalog.display(),
                error = %error,
                "contrôle du logo source ignoré"
            );
            QaCheck::not_applicable(
                "source_logo",
                format!("registre de logos en échec: {error:#}"),
            )
        }
    }
}

/// Transforme le résultat d'un `JoinHandle` de Stage 1 en erreur applicative.
/// Un panic dans un worker ne doit pas laisser un `qa.json` partiel.
fn join_stage<T>(result: std::thread::Result<anyhow::Result<T>>, stage: &str) -> Result<T> {
    match result {
        Ok(result) => result,
        Err(_) => bail!("Stage 1 {stage} interrompu (panic du worker)"),
    }
}

fn video_id_from_path(video: &Path) -> String {
    video
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("export")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_id_falls_back_to_stem() {
        assert_eq!(
            video_id_from_path(Path::new("/tmp/UJUD - JOSEPH_9x16.mp4")),
            "UJUD - JOSEPH_9x16"
        );
    }

    #[test]
    fn source_logo_check_requires_a_project_and_a_catalog() {
        let mut options = QaOptions {
            source_logo_enabled: true,
            ..QaOptions::default()
        };
        let specs = crate::qa::model::QaSpecs {
            width: 1080,
            height: 1920,
            fps: 25.0,
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
            duration_s: 10.0,
            bit_rate: 1_000_000,
            size_bytes: 1_000_000,
        };
        assert_eq!(
            source_logo_check(Path::new("/tmp/export.mp4"), None, &specs, &options).status,
            QaStatus::NotApplicable
        );

        options.source_logo_catalog = Some(PathBuf::from("/nonexistent/source_logos.toml"));
        let project = Path::new("/tmp/projet.ecproj");
        assert_eq!(
            source_logo_check(
                Path::new("/tmp/export.mp4"),
                Some(project),
                &specs,
                &options
            )
            .status,
            QaStatus::NotApplicable
        );
    }
}
