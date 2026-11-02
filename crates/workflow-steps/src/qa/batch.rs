//! Mode lot du contrôle QA : un dossier d'exports, N workers.
//!
//! Le scan ne regarde que les `.mp4` à la racine du dossier et associe le
//! projet `projets/<stem>.ecproj` quand il existe, comme le corpus de
//! validation. Chaque export écrit son propre `<out>/<slug>/qa.json` ;
//! `summary.json` agrège les statuts, les findings et les latences.
//!
//! Les workers sont des threads qui exécutent `run_qa` en série : le Stage 1
//! d'un export lance déjà l'OCR et le visage en parallèle. Le nombre de
//! threads intra-op ONNX est réduit par worker pour éviter la sursouscription.

use crate::post::model::now_iso;
use crate::qa::{cache, report, run_qa, QaOptions};
use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use workflow_core::fsutil::write_atomic;

/// Un export découvert dans le dossier, avec son projet éventuel.
#[derive(Debug, Clone)]
pub struct BatchExport {
    pub video: PathBuf,
    pub project: Option<PathBuf>,
    pub video_id: String,
    pub folder_id: String,
    pub slug: String,
}

/// Résultat d'un export dans le lot.
#[derive(Debug, Clone, Serialize)]
pub struct BatchItem {
    pub video_id: String,
    pub folder_id: String,
    pub slug: String,
    pub video: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// `pass`, `info`, `review`, `fail`, `not_applicable` ou `error`.
    pub status: String,
    pub findings: usize,
    /// Le `qa.json` de cet export est auto-approbable sans revue humaine.
    pub auto_approve_eligible: bool,
    pub total_ms: u64,
    /// Rapport réutilisé sans réexécution (mêmes entrées).
    pub cached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub export_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qa_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchSummary {
    pub schema: u32,
    pub generated_at: String,
    pub folder: String,
    pub workers: usize,
    pub intra_threads: usize,
    pub videos: usize,
    pub counts: BTreeMap<String, usize>,
    pub findings_total: usize,
    /// Nombre d'exports dont le `qa.json` est auto-approbable.
    pub auto_approve_eligible: usize,
    pub items: Vec<BatchItem>,
}

impl BatchSummary {
    /// Un `fail` ou une erreur d'exécution doit faire sortir le CLI en erreur.
    pub fn has_failures(&self) -> bool {
        self.counts.get("fail").copied().unwrap_or(0) > 0
            || self.counts.get("error").copied().unwrap_or(0) > 0
    }
}

#[derive(Debug, Clone)]
pub struct BatchOutcome {
    pub summary_path: PathBuf,
    pub summary: BatchSummary,
}

/// Exécute le QA sur tous les `.mp4` de `folder`.
///
/// `base.out_dir` est le répertoire racine du lot : chaque export est écrit
/// dans un sous-répertoire `slug`. `base.video_id` et `base.folder_id` sont
/// ignorés ; `base.split` est conservé pour l'annotation.
pub fn run_folder(
    folder: &Path,
    base: &QaOptions,
    workers: usize,
    limit: Option<usize>,
    cache_enabled: bool,
) -> Result<BatchOutcome> {
    if !folder.is_dir() {
        anyhow::bail!("dossier introuvable: {}", folder.display());
    }
    if base.out_dir.as_os_str().is_empty() {
        anyhow::bail!("passer un répertoire --out explicite");
    }

    let mut exports = discover_exports(folder)?;
    if let Some(limit) = limit {
        exports.truncate(limit);
    }
    if exports.is_empty() {
        anyhow::bail!("aucun export .mp4 dans {}", folder.display());
    }

    let workers = workers.clamp(1, exports.len());
    let intra_threads = base.intra_threads.unwrap_or_else(|| {
        let cores = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        // Chaque worker lance l'OCR et le visage en parallèle : deux sessions
        // ONNX actives par worker.
        (cores / (workers * 2)).max(1)
    });
    let mut base = base.clone();
    base.intra_threads = Some(intra_threads);

    tracing::info!(
        folder = %folder.display(),
        videos = exports.len(),
        workers,
        intra_threads,
        cache = cache_enabled,
        "QA par lot démarré"
    );

    let queue = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(exports.len()));
    let started = Instant::now();

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = queue.fetch_add(1, Ordering::Relaxed);
                let Some(export) = exports.get(index) else {
                    break;
                };
                let item = run_export(export, &base, index, cache_enabled);
                if let Ok(mut guard) = results.lock() {
                    guard.push((index, item));
                }
            });
        }
    });

    let mut results = results
        .into_inner()
        .map_err(|_| anyhow::anyhow!("verrou des résultats QA empoisonné"))?;
    results.sort_by_key(|(index, _)| *index);
    let items = results
        .into_iter()
        .map(|(_, item)| item)
        .collect::<Vec<_>>();

    let mut counts = BTreeMap::new();
    for key in ["pass", "info", "review", "fail", "not_applicable", "error"] {
        counts.insert(key.to_string(), 0usize);
    }
    let mut findings_total = 0usize;
    let mut auto_approve_eligible = 0usize;
    for item in &items {
        *counts.entry(item.status.clone()).or_insert(0) += 1;
        findings_total += item.findings;
        auto_approve_eligible += usize::from(item.auto_approve_eligible);
    }

    let summary = BatchSummary {
        schema: 2,
        generated_at: now_iso(),
        folder: folder.to_string_lossy().to_string(),
        workers,
        intra_threads,
        videos: items.len(),
        counts,
        findings_total,
        auto_approve_eligible,
        items,
    };
    let summary_path = base.out_dir.join("summary.json");
    if let Some(parent) = summary_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("création du répertoire {}", parent.display()))?;
    }
    let mut bytes = serde_json::to_vec_pretty(&summary).context("sérialisation du résumé QA")?;
    bytes.push(b'\n');
    write_atomic(&summary_path, &bytes)
        .with_context(|| format!("écriture de {}", summary_path.display()))?;

    tracing::info!(
        videos = summary.videos,
        elapsed_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        summary = %summary_path.display(),
        "QA par lot terminé"
    );

    Ok(BatchOutcome {
        summary_path,
        summary,
    })
}

fn run_export(
    export: &BatchExport,
    base: &QaOptions,
    index: usize,
    cache_enabled: bool,
) -> BatchItem {
    if cache_enabled {
        if let Some(item) = cached_item(export, base, index) {
            return item;
        }
    }
    let mut options = base.clone();
    options.out_dir = base.out_dir.join(&export.slug);
    options.video_id = Some(export.video_id.clone());
    options.folder_id = Some(export.folder_id.clone());
    tracing::info!(
        index,
        video = %export.video.display(),
        project = ?export.project,
        "QA export démarré"
    );
    let started = Instant::now();
    let outcome = run_qa(&export.video, export.project.as_deref(), &options);
    let elapsed = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    match outcome {
        Ok(outcome) => {
            let report = outcome.report;
            let status = report.overall_status.as_str().to_string();
            tracing::info!(
                index,
                video_id = %export.video_id,
                status = %status,
                findings = report.findings.len(),
                total_ms = elapsed,
                "QA export terminé"
            );
            BatchItem {
                video_id: export.video_id.clone(),
                folder_id: export.folder_id.clone(),
                slug: export.slug.clone(),
                video: export.video.to_string_lossy().to_string(),
                project: export
                    .project
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                status,
                findings: report.findings.len(),
                auto_approve_eligible: report.auto_approve_eligible,
                total_ms: elapsed,
                cached: false,
                export_sha256: Some(report.export_sha256),
                qa_path: Some(outcome.qa_path.to_string_lossy().to_string()),
                error: None,
            }
        }
        Err(error) => {
            let message = format!("{error:#}");
            tracing::error!(
                index,
                video_id = %export.video_id,
                error = %message,
                "QA export en échec"
            );
            BatchItem {
                video_id: export.video_id.clone(),
                folder_id: export.folder_id.clone(),
                slug: export.slug.clone(),
                video: export.video.to_string_lossy().to_string(),
                project: export
                    .project
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                status: "error".into(),
                findings: 0,
                auto_approve_eligible: false,
                total_ms: elapsed,
                cached: false,
                export_sha256: None,
                qa_path: None,
                error: Some(message),
            }
        }
    }
}

/// Réutilise `<out>/<slug>/qa.json` si son empreinte correspond toujours aux
/// entrées. Un rapport absent, illisible ou d'une version antérieure est
/// simplement recalculé.
fn cached_item(export: &BatchExport, base: &QaOptions, index: usize) -> Option<BatchItem> {
    let qa_path = base.out_dir.join(&export.slug).join(report::QA_FILENAME);
    if !qa_path.is_file() {
        return None;
    }
    let report = match report::read_report(&qa_path) {
        Ok(report) => report,
        Err(error) => {
            tracing::warn!(
                index,
                path = %qa_path.display(),
                error = %error,
                "rapport en cache illisible, recalcul"
            );
            return None;
        }
    };
    if report.cache_key.is_empty() {
        return None;
    }
    let key = match cache::fingerprint(&export.video, export.project.as_deref(), base) {
        Ok(key) => key,
        Err(error) => {
            tracing::warn!(
                index,
                video_id = %export.video_id,
                error = %error,
                "empreinte de cache indisponible, recalcul"
            );
            return None;
        }
    };
    if report.cache_key != key {
        return None;
    }
    // Un rapport dont une preuve WebP a été supprimée doit être recalculé :
    // les chemins `evidence_frame` seraient sinon dangling.
    let output_dir = base.out_dir.join(&export.slug);
    let evidence_present = report.findings.iter().all(|finding| {
        finding
            .evidence_frame
            .as_ref()
            .is_none_or(|relative| output_dir.join(relative).is_file())
    });
    if !evidence_present {
        return None;
    }
    tracing::info!(
        index,
        video_id = %export.video_id,
        status = report.overall_status.as_str(),
        "QA export réutilisé depuis le cache"
    );
    Some(BatchItem {
        video_id: export.video_id.clone(),
        folder_id: export.folder_id.clone(),
        slug: export.slug.clone(),
        video: export.video.to_string_lossy().to_string(),
        project: export
            .project
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        status: report.overall_status.as_str().to_string(),
        findings: report.findings.len(),
        auto_approve_eligible: report.auto_approve_eligible,
        total_ms: 0,
        cached: true,
        export_sha256: Some(report.export_sha256),
        qa_path: Some(qa_path.to_string_lossy().to_string()),
        error: None,
    })
}

/// Liste les `.mp4` de la racine du dossier, triés par nom, et associe le
/// projet `projets/<stem>.ecproj`. Les fichiers cachés sont ignorés.
pub fn discover_exports(folder: &Path) -> Result<Vec<BatchExport>> {
    let mut videos = Vec::new();
    for entry in fs::read_dir(folder).with_context(|| format!("lecture de {}", folder.display()))? {
        let entry = entry.with_context(|| format!("entrée de {}", folder.display()))?;
        let path = entry.path();
        if !path.is_file() || is_hidden(&path) || !is_mp4(&path) {
            continue;
        }
        videos.push(path);
    }
    videos.sort();

    let mut used_slugs = HashSet::new();
    let mut exports = Vec::with_capacity(videos.len());
    for video in videos {
        let Some(stem) = video
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        let project_path = folder.join("projets").join(format!("{stem}.ecproj"));
        let project = project_path.is_file().then_some(project_path);
        let slug = unique_slug(&stem, &mut used_slugs);
        exports.push(BatchExport {
            video,
            project,
            video_id: stem,
            folder_id: folder_id_from_path(folder),
            slug,
        });
    }
    Ok(exports)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn is_mp4(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
}

/// Identifiant de dossier : premier jeton numérique du nom (`187 Camille_GOOD`
/// → `187`), sinon le nom complet.
fn folder_id_from_path(folder: &Path) -> String {
    let name = folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let first = name.split_whitespace().next().unwrap_or_default();
    if !first.is_empty() && first.chars().all(|character| character.is_ascii_digit()) {
        first.to_string()
    } else {
        name.to_string()
    }
}

fn unique_slug(stem: &str, used: &mut HashSet<String>) -> String {
    let base = slugify(stem);
    if base.is_empty() {
        let base = "export".to_string();
        return unique_slug(&base, used);
    }
    if used.insert(base.clone()) {
        return base;
    }
    let mut index = 2usize;
    loop {
        let candidate = format!("{base}-{index}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

/// Slug ASCII déterministe : minuscules, accents français translittérés,
/// toute autre suite de caractères remplacée par un tiret.
pub fn slugify(value: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for character in value.chars() {
        let lowered = character.to_lowercase().next().unwrap_or(character);
        let transliterated = match lowered {
            'à' | 'â' | 'ä' | 'á' | 'ã' | 'å' => Some('a'),
            'ç' => Some('c'),
            'è' | 'é' | 'ê' | 'ë' => Some('e'),
            'ì' | 'í' | 'î' | 'ï' => Some('i'),
            'ñ' => Some('n'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => Some('o'),
            'ù' | 'ú' | 'û' | 'ü' => Some('u'),
            'ý' | 'ÿ' => Some('y'),
            'œ' => Some('e'),
            'æ' => Some('e'),
            _ => None,
        };
        if let Some(letter) = transliterated {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            out.push(letter);
            pending_dash = false;
        } else if lowered.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            out.push(lowered);
            pending_dash = false;
        } else {
            pending_dash = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_transliterates_french_accents() {
        assert_eq!(
            slugify("303_À 68 ans, elle accède à sa terrasse !"),
            "303-a-68-ans-elle-accede-a-sa-terrasse"
        );
    }

    #[test]
    fn slug_collapses_separators() {
        assert_eq!(slugify("179_M6+MAX ANIME - EP33"), "179-m6-max-anime-ep33");
    }

    #[test]
    fn slug_is_unique_for_collisions() {
        let mut used = HashSet::new();
        assert_eq!(unique_slug("Clip A", &mut used), "clip-a");
        assert_eq!(unique_slug("clip_a", &mut used), "clip-a-2");
    }

    #[test]
    fn folder_id_uses_the_leading_number() {
        assert_eq!(
            folder_id_from_path(Path::new("/corpus/187 Camille_GOOD")),
            "187"
        );
        assert_eq!(folder_id_from_path(Path::new("/corpus/divers")), "divers");
    }

    #[test]
    fn discovery_matches_projects_and_ignores_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("projets")).unwrap();
        fs::write(root.join("300 Clip_9x16.mp4"), b"video").unwrap();
        fs::write(root.join("300 Clip_9x16.ecproj"), b"project").unwrap();
        fs::write(root.join("projets/300 Clip_9x16.ecproj"), b"project").unwrap();
        fs::write(root.join("notes.txt"), b"notes").unwrap();
        fs::write(root.join(".cache.mp4"), b"hidden").unwrap();

        let exports = discover_exports(root).unwrap();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].video_id, "300 Clip_9x16");
        assert_eq!(exports[0].slug, "300-clip-9x16");
        assert!(exports[0].project.is_some());
    }

    #[test]
    fn summary_reports_failures() {
        let mut counts = BTreeMap::new();
        counts.insert("pass".to_string(), 1);
        counts.insert("error".to_string(), 1);
        let summary = BatchSummary {
            schema: 2,
            generated_at: "2026-10-31T00:00:00Z".into(),
            folder: "/corpus".into(),
            workers: 4,
            intra_threads: 3,
            videos: 2,
            counts,
            findings_total: 0,
            auto_approve_eligible: 0,
            items: Vec::new(),
        };
        assert!(summary.has_failures());
    }
}
