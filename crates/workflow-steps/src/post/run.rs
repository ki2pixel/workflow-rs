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
use crate::post::model::{
    now_iso, verdict, Action, AuditCheck, Decision, PostProjectOutcome, PostRunSummary, PostSummary,
};
use crate::post::{a1, a2, a3, a4, a6, detect, direct};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::ser::PrettyFormatter;
use serde_json::{json, Map, Serializer, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use workflow_core::fsutil::write_atomic;
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
    pub template_prefix: String,
    pub a1_margin: f64,
    pub a2_margin: f64,
    /// Laboratoire uniquement : calcule les rapports sans publier `_assist`.
    pub report_only: bool,
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
            template_prefix: a6::DEFAULT_PREFIX.into(),
            a1_margin: 1.04,
            a2_margin: 1.05,
            report_only: false,
        }
    }
}

impl PostOptions {
    pub fn from_config(config: &workflow_core::config::PostConfig, docs_subfolder: &str) -> Self {
        Self {
            enabled: config.enabled,
            direct: config.direct,
            ffmpeg: config.ffmpeg.clone(),
            ffprobe: config.ffprobe.clone(),
            suffix: config.suffix.clone(),
            projects_subfolder: config.projects_subfolder.clone(),
            docs_subfolder: docs_subfolder.to_string(),
            artifacts_subfolder: config.artifacts_subfolder.clone(),
            template_prefix: config.template_prefix.clone(),
            a1_margin: config.a1_margin,
            a2_margin: config.a2_margin,
            report_only: false,
        }
    }
}

pub fn run_post_for_folders(folders: &[String], options: &PostOptions) -> PostRunSummary {
    let mut summary = PostRunSummary::default();
    if !options.enabled {
        return summary;
    }
    for folder in folders {
        let folder_summary = run_post_dir(options, Path::new(folder));
        merge_summary(&mut summary, folder_summary);
    }
    summary
}

pub fn run_post_dir(options: &PostOptions, folder: &Path) -> PostRunSummary {
    let mut summary = PostRunSummary::default();
    if !options.enabled {
        return summary;
    }
    if options.direct && !options.report_only {
        let direct = direct::ensure_folder(
            folder,
            &options.docs_subfolder,
            &options.projects_subfolder,
            &options.ffprobe,
            &options.template_prefix,
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
        if direct.failed > 0 {
            summary
                .failures
                .push(format!("template direct: {}", folder.display()));
        }
    }
    let projects_dir = folder.join(&options.projects_subfolder);
    if !projects_dir.is_dir() {
        tracing::debug!(
            "Post-traitement: pas de dossier {} pour {}",
            projects_dir.display(),
            folder.display()
        );
        return summary;
    }
    let mut projects = Vec::new();
    let Ok(entries) = fs::read_dir(&projects_dir) else {
        tracing::warn!(
            "Post-traitement: lecture de {} impossible",
            projects_dir.display()
        );
        return summary;
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
    if projects.is_empty() {
        tracing::debug!(
            "Post-traitement: aucun .ecproj brut dans {}",
            projects_dir.display()
        );
        return summary;
    }
    tracing::info!(
        "Post-traitement: {} projet(s) dans {}",
        projects.len(),
        folder.display()
    );
    for project in projects {
        let outcome = run_post_project(options, &project, None);
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
        summary.push(outcome);
    }
    summary
}

pub fn run_post_project(
    options: &PostOptions,
    project: &Path,
    docs_override: Option<&Path>,
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
        a6::validate(&doc, &segments, &meta, &stem, &options.template_prefix);
    let mut decisions = Vec::new();
    let mut a6_ms = a6_start.elapsed().as_secs_f64() * 1000.0;
    let mut a1_ms = 0.0;
    let mut a2_ms = 0.0;
    let mut a3_ms = 0.0;
    let mut a4_ms = 0.0;
    if template.eligible != Some(true) {
        let (source_width, source_height) = specs
            .as_ref()
            .map(|specs| (specs.width, specs.height))
            .unwrap_or((1920, 1080));
        decisions.extend(coverage_reviews(
            &segments,
            &meta,
            source_width,
            source_height,
        ));
        let start = Instant::now();
        decisions.extend(a1::decide_a1(
            &segments,
            video.as_deref(),
            &options.ffmpeg,
            &meta,
            source_width,
            source_height,
            options.a1_margin,
        ));
        a1_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        decisions.extend(a2::decide_a2(
            &segments,
            video.as_deref(),
            &options.ffmpeg,
            &options.ffprobe,
            &meta,
            source_height,
            options.a2_margin,
        ));
        a2_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        decisions.extend(a3::decide_a3(&segments, ae.as_deref(), fps));
        a3_ms = start.elapsed().as_secs_f64() * 1000.0;
        let start = Instant::now();
        let rows = csv.as_deref().map(a4::parse_csv).unwrap_or_default();
        decisions.extend(a4::decide_a4(&segments, &rows, fps));
        a4_ms = start.elapsed().as_secs_f64() * 1000.0;
    }
    decisions.extend(a6_decisions);
    timings.insert("a1_ms".into(), json!(round_to(a1_ms, 1)));
    timings.insert("a2_ms".into(), json!(round_to(a2_ms, 1)));
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

    let mut output_doc = doc.clone();
    let mut transform_error = None;
    let mut transformed = false;
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
    timings.insert("a6_ms".into(), json!(round_to(a6_ms, 1)));

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
        &meta,
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
            };
            if options.report_only {
                checks.push(AuditCheck::pass(
                    "output_layers",
                    format!("{layers} calques calculés (report-only)"),
                ));
            } else if transform_error.is_none() && assist.is_file() {
                checks.push(AuditCheck::pass(
                    "output_written",
                    format!("{}, {layers} calques", assist.display()),
                ));
                if layers == expected {
                    checks.push(AuditCheck::pass(
                        "output_layers",
                        format!("{expected} calques"),
                    ));
                } else {
                    checks.push(AuditCheck::warn(
                        "output_layers",
                        format!("{layers} calques au lieu de {expected}"),
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
        &meta,
        &template,
        &timings,
        &decisions,
        &summary,
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
        &meta,
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

fn merge_summary(target: &mut PostRunSummary, source: PostRunSummary) {
    target.projects += source.projects;
    target.assists += source.assists;
    target.apply += source.apply;
    target.review += source.review;
    target.noop += source.noop;
    target.failures.extend(source.failures);
    target.items.extend(source.items);
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

/// Contrôle structurel de la couverture source.
///
/// Le panel borne l'ancre pour que la source couvre la composition. Un projet
/// édité à la main peut sortir de cette plage : A5 émet alors une revue au
/// lieu de laisser un vide dans le rendu.
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
                    }
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
    if meta.duration > 0.0 && (duration - meta.duration).abs() > 1.0_f64.max(0.01 * meta.duration) {
        checks.push(AuditCheck::warn(
            "duration_coherent",
            format!(
                "somme segments {duration:.2}s vs comp {:.2}s",
                meta.duration
            ),
        ));
    } else {
        checks.push(AuditCheck::pass(
            "duration_coherent",
            format!("{duration:.2}s"),
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
    checks.push(AuditCheck::pass(
        "a3_disabled",
        "A3 en review uniquement (D-030)",
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
            format!("{coverage_reviews} segment(s) hors couverture, en review"),
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
}
