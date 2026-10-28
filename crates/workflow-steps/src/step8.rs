//! STEP8 : génération des projets EffectCraft via `effectcraft-cli`.
//!
//! Le panel `MediaSolution.jsx` est concaténé à un driver puis exécuté en
//! headless. Aucun bridge, aucun lancement d'application : le préflight
//! `prefs.set` est exécuté par le CLI, dans la même session que le driver.

use crate::post::direct;
use crate::post::{run_post_for_folders, PostOptions, PostRunSummary};
use anyhow::{bail, Context, Result};
use chrono::Local;
use regex::Regex;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};
use workflow_schema::round::round2;

const TERMINATION_GRACE: Duration = Duration::from_secs(5);
static CURRENT_CHILD_PID: AtomicI32 = AtomicI32::new(0);
static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static SIGNAL_DONE: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone)]
pub struct Step8Options {
    pub manifest: Option<PathBuf>,
    pub folders: Vec<PathBuf>,
    pub cli: PathBuf,
    pub panel_jsx: PathBuf,
    /// Fichier `mediasolution_ec.json`. Vide ou absent : valeurs du panel.
    pub panel_config: Option<PathBuf>,
    pub docs_subfolder: String,
    pub logs_dir: PathBuf,
    pub cwd: PathBuf,
    pub dry_run: bool,
    pub max_drivers_kept: usize,
    /// 0 désactive le timeout interne.
    pub timeout_s: u64,
    /// Post-traitement A5/A6 (second `.ecproj` `_assist`).
    pub post: PostOptions,
}

#[derive(Debug, Clone, Serialize)]
pub struct Step8Outcome {
    pub success: bool,
    pub processed: usize,
    pub dry_run: bool,
    pub timed_out: bool,
    pub exit_code: i32,
    pub report: Option<String>,
    pub post: Option<PostRunSummary>,
}

#[derive(Debug, Clone, Serialize)]
struct Step8Report {
    schema: u32,
    timestamp: String,
    manifest: Option<String>,
    output_dir: Option<String>,
    folders: Vec<String>,
    panel_jsx: String,
    cli: String,
    mode: &'static str,
    bridge_port: Value,
    app_launched: bool,
    cli_version: Option<String>,
    driver: String,
    config_json: Option<String>,
    result: Value,
    cli_return_code: i32,
    timeout_s: u64,
    timed_out: bool,
    duration_s: f64,
    success: bool,
    direct_projects: usize,
    direct_created: usize,
    panel_skipped: bool,
    panel_folders: Vec<String>,
    post: Option<PostRunSummary>,
}

pub fn run_step8(options: &Step8Options) -> Result<Step8Outcome> {
    let started = Instant::now();
    let step7_dir = options.logs_dir.join("step7");
    let requested_manifest = options.manifest.as_ref().map(|path| expand_tilde(path));
    let manifest_path = if options.folders.is_empty() {
        let path = requested_manifest
            .clone()
            .or_else(|| latest_manifest(&step7_dir))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Aucun manifeste de l'étape 7 trouvé dans {} (lancez STEP7 ou passez --folder).",
                    step7_dir.display()
                )
            })?;
        if !path.is_file() {
            bail!("Manifeste introuvable: {}", path.display());
        }
        Some(path)
    } else {
        requested_manifest
    };

    let manifest = match manifest_path.as_deref() {
        Some(path) if path.is_file() => Some(load_manifest(path)?),
        _ => None,
    };

    let candidate_count = if options.folders.is_empty() {
        manifest_project_count(manifest.as_ref())
    } else {
        options.folders.len()
    };

    let config_path = resolve_config_path(options, manifest.as_ref());
    let (config_json, docs_subfolder) =
        load_panel_config(config_path.as_deref(), &options.docs_subfolder);
    // Le post-traitement et le bypass direct lisent le même `docs/` que le
    // panel, y compris quand `mediasolution_ec.json` change le nom du dossier.
    let mut post = options.post.clone();
    post.docs_subfolder = docs_subfolder.clone();
    let folders = select_folders(manifest.as_ref(), &options.folders, &docs_subfolder);
    if folders.is_empty() {
        if candidate_count == 0 {
            return Ok(Step8Outcome {
                success: true,
                processed: 0,
                dry_run: options.dry_run,
                timed_out: false,
                exit_code: 0,
                report: None,
                post: None,
            });
        }
        bail!(
            "Aucun dossier exploitable parmi {} projet(s) listés.",
            candidate_count
        );
    }

    let direct_summary = if post.direct {
        direct::ensure_for_folders(&folders, &post, options.dry_run)
    } else {
        direct::DirectSummary::default()
    };
    let panel_folders: Vec<String> = folders
        .iter()
        .filter(|folder| !direct_summary.pure_folders.contains(folder))
        .cloned()
        .collect();
    let panel_skipped = direct_summary.templates > 0 && panel_folders.is_empty();
    let mut config_json = config_json;
    if !direct_summary.names.is_empty() {
        config_json = inject_video_skip_names(config_json.as_deref(), &direct_summary.names);
    }

    tracing::info!(
        "Manifeste: {}",
        manifest_path
            .as_ref()
            .map_or_else(|| "aucun".into(), |path| path.display().to_string())
    );
    tracing::info!("{} dossier(s) à traiter:", folders.len());
    for folder in &folders {
        tracing::info!("  - {folder}");
    }
    if direct_summary.templates > 0 {
        tracing::info!(
            "Template direct: {} vidéo(s), {} brut(s) créé(s), {} déjà présent(s)",
            direct_summary.templates,
            direct_summary.created,
            direct_summary.existing
        );
    }
    if panel_skipped {
        tracing::info!("Panel MediaSolution ignoré: tous les dossiers sont des templates directs.");
    } else if panel_folders.len() != folders.len() {
        tracing::info!(
            "Panel MediaSolution limité à {} dossier(s) non-template.",
            panel_folders.len()
        );
    }

    let step8_dir = options.logs_dir.join("step8");
    let drivers_dir = step8_dir.join("drivers");
    fs::create_dir_all(&drivers_dir)
        .with_context(|| format!("création de {}", drivers_dir.display()))?;
    let stamp = Local::now().format("%Y%m%d_%H%M%S");
    let mut driver_path = PathBuf::new();
    let mut progress_path = PathBuf::new();
    let mut version = None;
    let mut panel_jsx_display = options.panel_jsx.to_string_lossy().to_string();
    let mut cli_display = options.cli.to_string_lossy().to_string();

    if !panel_skipped {
        let panel_jsx = resolve_panel_jsx(&options.panel_jsx)
            .context("Panel MediaSolution.jsx introuvable (dépôt et copie installée)")?;
        let cli_path = resolve_cli(&options.cli).with_context(|| {
            format!(
                "Exécutable EffectCraft introuvable: {}",
                options.cli.display()
            )
        })?;
        panel_jsx_display = panel_jsx.to_string_lossy().to_string();
        cli_display = cli_path.to_string_lossy().to_string();
        version = cli_version(&cli_path);
        tracing::info!(
            "Mode d'exécution: headless (effectcraft-cli {})",
            version.as_deref().unwrap_or("version inconnue")
        );
        tracing::info!("Panel: {}", panel_jsx.display());
        tracing::info!("CLI: {}", cli_path.display());

        driver_path = drivers_dir.join(format!("step8_driver_{stamp}.jsx"));
        progress_path = drivers_dir.join(format!("step8_progress_{stamp}.log"));
        let panel = fs::read_to_string(&panel_jsx)
            .with_context(|| format!("lecture du panel {}", panel_jsx.display()))?;
        let driver = build_driver_js(
            &panel,
            &panel_folders,
            config_json.as_deref(),
            &progress_path,
        );
        fs::write(&driver_path, driver)
            .with_context(|| format!("écriture du driver {}", driver_path.display()))?;
        tracing::info!("Driver généré: {}", driver_path.display());
    } else {
        tracing::info!("CLI: {}", cli_display);
    }

    if options.dry_run {
        tracing::info!("--dry-run: exécution EffectCraft ignorée.");
        println!("MS_BATCH_TOTAL: {}", panel_folders.len());
        for (index, folder) in panel_folders.iter().enumerate() {
            let name = Path::new(folder)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(folder);
            println!(
                "MS_BATCH_PROGRESS: {}/{} - {name}",
                index + 1,
                panel_folders.len()
            );
        }
        if panel_skipped {
            println!("DIRECT_TEMPLATE_TOTAL: {}", direct_summary.templates);
        }
        return Ok(Step8Outcome {
            success: true,
            processed: folders.len(),
            dry_run: true,
            timed_out: false,
            exit_code: 0,
            report: None,
            post: None,
        });
    }

    let duration;
    let mut return_code;
    let result;
    let timed_out;
    let mut success;
    if panel_skipped {
        duration = started.elapsed().as_secs_f64();
        return_code = i32::from(direct_summary.failed > 0);
        timed_out = false;
        success = direct_summary.failed == 0;
        if !success {
            tracing::error!(
                "Template direct: {} brut(s) non créé(s) dans les dossiers 100 % SDM.",
                direct_summary.failed
            );
        }
        result = Some(json!({
            "ok": direct_summary.failed == 0,
            "errors": [],
            "panel_skipped": "all_direct_templates",
            "direct_templates": direct_summary.templates,
        }));
    } else {
        let timeout = (options.timeout_s > 0).then(|| Duration::from_secs(options.timeout_s));
        let cli_outcome = run_cli(
            &resolve_cli(&options.cli).context("effectcraft-cli introuvable")?,
            &driver_path,
            &progress_path,
            &options.cwd,
            timeout,
        )?;
        duration = started.elapsed().as_secs_f64();
        return_code = cli_outcome.return_code;
        result = cli_outcome.result;
        timed_out = cli_outcome.timed_out;
        success = !timed_out && evaluate_result(result.as_ref());
        if timed_out {
            tracing::error!(
                "Batch interrompu après le timeout interne de {} s.",
                options.timeout_s
            );
        } else if result.is_none() {
            tracing::error!("Aucun résultat MS_BATCH_RESULT reçu du CLI (code {return_code}).");
        } else if !success {
            if let Some(error) = result.as_ref().and_then(|value| value.get("error")) {
                tracing::error!("Batch en échec: {error}");
            }
            if let Some(errors) = result
                .as_ref()
                .and_then(|value| value.get("errors"))
                .and_then(Value::as_array)
            {
                for error in errors {
                    tracing::error!("Échec projet: {error}");
                }
            }
        }
        if direct_summary.failed > 0 {
            tracing::error!(
                "Template direct: {} brut(s) non créé(s) ; lot marqué en échec.",
                direct_summary.failed
            );
            if success && return_code == 0 {
                return_code = 1;
            }
            success = false;
        }
        if success && return_code != 0 {
            tracing::warn!(
                "MS_BATCH_RESULT valide mais code de sortie CLI non nul ({return_code})."
            );
        }
    }

    let post_summary = if success && post.enabled {
        let summary = run_post_for_folders(&folders, &post);
        tracing::info!(
            "Post-traitement: {} projet(s), {} assist(s), {} apply, {} review, {} échec(s)",
            summary.projects,
            summary.assists,
            summary.apply,
            summary.review,
            summary.failures.len()
        );
        if !summary.failures.is_empty() {
            tracing::warn!(
                "Post-traitement en échec pour: {}",
                summary.failures.join(", ")
            );
        }
        Some(summary)
    } else {
        None
    };
    let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S").to_string();
    let report = Step8Report {
        schema: 1,
        timestamp,
        manifest: manifest_path
            .as_ref()
            .map(|path| path.to_string_lossy().to_string()),
        output_dir: manifest
            .as_ref()
            .and_then(|value| value.get("output_dir"))
            .and_then(Value::as_str)
            .map(str::to_string),
        folders: folders.clone(),
        panel_jsx: panel_jsx_display,
        cli: cli_display,
        mode: if panel_skipped { "direct" } else { "headless" },
        bridge_port: Value::Null,
        app_launched: false,
        cli_version: version.clone(),
        driver: driver_path.to_string_lossy().to_string(),
        config_json: config_json
            .as_ref()
            .and(config_path.as_ref())
            .map(|path| path.to_string_lossy().to_string()),
        result: result.unwrap_or(Value::Null),
        cli_return_code: return_code,
        timeout_s: options.timeout_s,
        timed_out,
        duration_s: round2(duration),
        success,
        direct_projects: direct_summary.templates,
        direct_created: direct_summary.created,
        panel_skipped,
        panel_folders: panel_folders.clone(),
        post: post_summary.clone(),
    };
    let report_path = write_report(&step8_dir, &report)?;
    tracing::info!("Rapport écrit: {}", report_path.display());
    cleanup_drivers(&drivers_dir, options.max_drivers_kept);
    tracing::info!(
        "--- Étape 8 terminée: {} ({:.1} s) ---",
        if success { "succès" } else { "échec" },
        duration
    );

    Ok(Step8Outcome {
        success,
        processed: folders.len(),
        dry_run: false,
        timed_out,
        exit_code: if success {
            0
        } else if timed_out {
            124
        } else {
            1
        },
        report: Some(report_path.to_string_lossy().to_string()),
        post: post_summary,
    })
}

fn expand_tilde(path: &Path) -> PathBuf {
    let Some(value) = path.to_str() else {
        return path.to_path_buf();
    };
    let Some(rest) = value.strip_prefix("~/") else {
        return path.to_path_buf();
    };
    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(rest))
        .unwrap_or_else(|| path.to_path_buf())
}

fn resolve_config_path(options: &Step8Options, manifest: Option<&Value>) -> Option<PathBuf> {
    if let Some(path) = options
        .panel_config
        .as_ref()
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Some(expand_tilde(path));
    }
    let output_dir = manifest
        .and_then(|value| value.get("output_dir"))
        .and_then(Value::as_str)
        .map(PathBuf::from)?;
    let candidate = output_dir.join("mediasolution_ec.json");
    candidate.is_file().then_some(candidate)
}

fn inject_video_skip_names(config_json: Option<&str>, names: &[String]) -> Option<String> {
    if names.is_empty() {
        return config_json.map(str::to_string);
    }
    let mut value = config_json
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let object = value.as_object_mut().expect("objet JSON");
    let entry = object
        .entry("videoSkipNames")
        .or_insert_with(|| json!(["m6+.mp4"]));
    if !entry.is_array() {
        *entry = json!(["m6+.mp4"]);
    }
    let array = entry.as_array_mut().expect("tableau videoSkipNames");
    // Le panel compare les noms de fichiers déjà passés en minuscules
    // (`isVideoSkipped`), donc toutes les entrées doivent l'être.
    for value in array.iter_mut() {
        if let Some(text) = value.as_str() {
            let lower = text.to_lowercase();
            if lower != text {
                *value = json!(lower);
            }
        }
    }
    for name in names {
        let lower = name.to_lowercase();
        if !array
            .iter()
            .any(|value| value.as_str() == Some(lower.as_str()))
        {
            array.push(json!(lower));
        }
    }
    serde_json::to_string(&value).ok()
}

fn load_panel_config(path: Option<&Path>, fallback_docs: &str) -> (Option<String>, String) {
    let Some(path) = path else {
        return (None, fallback_docs.to_string());
    };
    if !path.is_file() {
        return (None, fallback_docs.to_string());
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(
                "Config panel illisible '{}' ({error}) — valeurs par défaut.",
                path.display()
            );
            return (None, fallback_docs.to_string());
        }
    };
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                "Config panel invalide '{}' ({error}) — valeurs par défaut.",
                path.display()
            );
            return (None, fallback_docs.to_string());
        }
    };
    let docs = value
        .get("docsSubFolderName")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_docs)
        .to_string();
    match serde_json::to_string(&value) {
        Ok(json) => (Some(json), docs),
        Err(error) => {
            tracing::warn!(
                "Config panel non sérialisable '{}' ({error}).",
                path.display()
            );
            (None, fallback_docs.to_string())
        }
    }
}

fn select_folders(
    manifest: Option<&Value>,
    manual: &[PathBuf],
    docs_subfolder: &str,
) -> Vec<String> {
    let candidates: Vec<PathBuf> = if !manual.is_empty() {
        manual.iter().map(|path| expand_tilde(path)).collect()
    } else {
        manifest
            .and_then(|value| value.get("projects"))
            .and_then(Value::as_array)
            .map(|projects| {
                projects
                    .iter()
                    .filter_map(|entry| entry.get("path").and_then(Value::as_str))
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default()
    };

    let mut folders = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for candidate in candidates {
        if !candidate.is_dir() {
            tracing::warn!("Dossier ignoré (introuvable): {}", candidate.display());
            continue;
        }
        if !candidate.join(docs_subfolder).is_dir() {
            tracing::warn!(
                "Dossier ignoré (pas de '{}'): {}",
                docs_subfolder,
                candidate.display()
            );
            continue;
        }
        let Ok(resolved) = candidate.canonicalize() else {
            tracing::warn!(
                "Dossier ignoré (chemin non résolu): {}",
                candidate.display()
            );
            continue;
        };
        let resolved_string = resolved.to_string_lossy().to_string();
        if !seen.insert(resolved_string.clone()) {
            tracing::warn!("Dossier ignoré (doublon): {resolved_string}");
            continue;
        }
        folders.push(resolved_string);
    }
    folders
}

fn latest_manifest(step7_dir: &Path) -> Option<PathBuf> {
    if !step7_dir.is_dir() {
        return None;
    }
    let mut candidates: Vec<(SystemTime, PathBuf)> = fs::read_dir(step7_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        name.starts_with("finalized_projects_") && name.ends_with(".json")
                    })
        })
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (modified, path)
        })
        .collect();
    candidates.sort_by_key(|(modified, _)| *modified);
    candidates.pop().map(|(_, path)| path)
}

fn load_manifest(path: &Path) -> Result<Value> {
    let bytes =
        fs::read(path).with_context(|| format!("lecture du manifeste {}", path.display()))?;
    let value: Value = serde_json::from_slice(&bytes)
        .with_context(|| format!("manifeste JSON invalide {}", path.display()))?;
    if !value.is_object() {
        bail!(
            "Manifeste invalide (objet JSON attendu): {}",
            path.display()
        );
    }
    Ok(value)
}

fn manifest_project_count(manifest: Option<&Value>) -> usize {
    manifest
        .and_then(|value| value.get("projects"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len)
}

fn resolve_panel_jsx(candidate: &Path) -> Option<PathBuf> {
    let candidate = expand_tilde(candidate);
    let installed = std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("effectcraft")
            .join("Scripts")
            .join("ScriptUI Panels")
            .join("MediaSolution.jsx")
    });
    [Some(candidate), installed]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
}

fn resolve_cli(candidate: &Path) -> Option<PathBuf> {
    if candidate.as_os_str().is_empty() {
        return None;
    }
    let candidate = expand_tilde(candidate);
    let raw = candidate.to_string_lossy();
    if candidate.is_absolute() || raw.contains('/') || raw.contains('\\') {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let direct = directory.join(&candidate);
        if direct.is_file() {
            return Some(direct);
        }
        let with_suffix = directory.join(format!(
            "{}{}",
            candidate.to_string_lossy(),
            std::env::consts::EXE_SUFFIX
        ));
        if with_suffix.is_file() {
            return Some(with_suffix);
        }
    }
    None
}

fn cli_version(cli: &Path) -> Option<String> {
    let output = Command::new(cli).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    re_version()
        .find(&text)
        .map(|matched| matched.as_str().to_string())
}

fn build_driver_js(
    panel_js: &str,
    folders: &[String],
    config_json: Option<&str>,
    progress_path: &Path,
) -> String {
    let progress_literal =
        serde_json::to_string(&progress_path.to_string_lossy()).expect("sérialisation du chemin");
    let mut lines = vec![
        panel_js.trim_end().to_string(),
        String::new(),
        "// ---------------------------------------------------------------------------".to_string(),
        "// Driver headless généré par l'étape 8 du workflow (mode « Tout exécuter »).".to_string(),
        "// ---------------------------------------------------------------------------".to_string(),
        "// Le CLI active la préférence scripting avant `file.runScript` ; un script".to_string(),
        "// ne peut pas s'octroyer lui-même cette permission.".to_string(),
        format!("var __msProgressPath = {progress_literal};"),
        "(function () {".to_string(),
        "  var __msOriginalWriteLn = $.writeln;".to_string(),
        "  $.writeln = function (text) {".to_string(),
        "    try {".to_string(),
        "      var __msProgressFile = new File(__msProgressPath);".to_string(),
        "      __msProgressFile.encoding = \"UTF-8\";".to_string(),
        "      if (__msProgressFile.open(\"a\")) { __msProgressFile.writeln(text); __msProgressFile.close(); }".to_string(),
        "    } catch (__msProgressError) { }".to_string(),
        "    try { __msOriginalWriteLn(text); } catch (__msWriteError) { }".to_string(),
        "  };".to_string(),
        "})();".to_string(),
        String::new(),
    ];
    if let Some(config_json) = config_json {
        lines.push(format!(
            "mediasolutionEC.applyConfig({});",
            serde_json::to_string(config_json).expect("sérialisation d'une chaîne JSON")
        ));
        lines.push(String::new());
    }
    lines.extend([
        format!(
            "var MS_FOLDERS = {};",
            serde_json::to_string(folders).expect("sérialisation des dossiers")
        ),
        "var msResult = JSON.parse(mediasolutionEC.runBatch(JSON.stringify(MS_FOLDERS), null, 0));"
            .to_string(),
        "writeLn(\"MS_BATCH_RESULT: \" + JSON.stringify(msResult));".to_string(),
        "if (!msResult.ok || (msResult.errors && msResult.errors.length > 0)) {".to_string(),
        "  throw new Error(\"Batch Media Solution en échec\");".to_string(),
        "}".to_string(),
        "\"step8-ok\";".to_string(),
    ]);
    format!("{}\n", lines.join("\n"))
}

pub fn parse_panel_line(line: &str) -> Vec<String> {
    let mut emitted = Vec::new();
    if let Some(captures) = re_total().captures(line) {
        emitted.push(format!("MS_BATCH_TOTAL: {}", &captures[1]));
    }
    if let Some(captures) = re_progress().captures(line) {
        emitted.push(format!(
            "MS_BATCH_PROGRESS: {}/{} - {}",
            &captures[1], &captures[2], &captures[3]
        ));
    }
    if let Some(captures) = re_saved().captures(line) {
        emitted.push(format!("MS_BATCH_OK: {}", &captures[1]));
    }
    if let Some(captures) = re_unit_error().captures(line) {
        emitted.push(format!("MS_BATCH_FAIL: {}", &captures[1]));
    }
    if let Some(captures) = re_batch_end().captures(line) {
        emitted.push(format!("MS_BATCH_END: {}", &captures[1]));
    }
    emitted
}

fn evaluate_result(result: Option<&Value>) -> bool {
    result.is_some_and(|value| {
        value.get("ok").and_then(Value::as_bool) == Some(true)
            && value
                .get("errors")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
    })
}

struct CliOutcome {
    return_code: i32,
    result: Option<Value>,
    timed_out: bool,
}

fn run_cli(
    cli: &Path,
    driver: &Path,
    progress: &Path,
    cwd: &Path,
    timeout: Option<Duration>,
) -> Result<CliOutcome> {
    if let Some(parent) = progress.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(progress, b"").with_context(|| format!("création de {}", progress.display()))?;
    install_signal_handler();
    let prefs_params = json!({
        "key": "scripting.allowScriptsWriteFiles",
        "value": true
    });
    let script_params = json!({
        "path": driver.to_string_lossy(),
    });
    let mut command = Command::new(cli);
    command
        .arg("--empty")
        .arg("--json")
        .arg("run")
        .arg("prefs.set")
        .arg(serde_json::to_string(&prefs_params)?)
        .arg("file.runScript")
        .arg(serde_json::to_string(&script_params)?)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .spawn()
        .with_context(|| format!("lancement de {}", cli.display()))?;
    let child_pid = i32::try_from(child.id()).unwrap_or(i32::MAX);
    CURRENT_CHILD_PID.store(child_pid, Ordering::SeqCst);

    let finished = Arc::new(AtomicBool::new(false));
    let timed_out = Arc::new(AtomicBool::new(false));
    let watchdog = timeout.map(|duration| {
        let (done_tx, done_rx) = mpsc::channel();
        let finished_thread = Arc::clone(&finished);
        let timed_out_thread = Arc::clone(&timed_out);
        let handle = std::thread::spawn(move || {
            if done_rx.recv_timeout(duration).is_err() && !finished_thread.load(Ordering::SeqCst) {
                timed_out_thread.store(true, Ordering::SeqCst);
                terminate_process_group(child_pid, false);
                if done_rx.recv_timeout(TERMINATION_GRACE).is_err() {
                    terminate_process_group(child_pid, true);
                }
            }
        });
        (done_tx, handle)
    });

    let seen = Arc::new(Mutex::new(HashSet::new()));
    let tail_result = Arc::new(Mutex::new(None));
    let (stop_tail, tail_handle) = spawn_progress_tail(
        progress.to_path_buf(),
        Arc::clone(&seen),
        Arc::clone(&tail_result),
    );
    let output = child.wait_with_output();
    finished.store(true, Ordering::SeqCst);
    CURRENT_CHILD_PID.store(0, Ordering::SeqCst);
    if let Some((done_tx, handle)) = watchdog {
        let _ = done_tx.send(());
        let _ = handle.join();
    }
    stop_tail.store(true, Ordering::Relaxed);
    let _ = tail_handle.join();
    let output = output.context("attente d'effectcraft-cli")?;
    if INTERRUPTED.load(Ordering::SeqCst) {
        while !SIGNAL_DONE.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(50));
        }
        std::process::exit(143);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        println!("{stderr}");
        let _ = std::io::stdout().flush();
    }

    let payload: Option<Value> = serde_json::from_slice(&output.stdout).ok();
    let mut result = payload
        .as_ref()
        .and_then(|payload| consume_payload_output(payload, &seen));
    if result.is_none() {
        result = tail_result.lock().expect("verrou résultat").clone();
    }
    let timed_out = timed_out.load(Ordering::SeqCst);
    if timed_out {
        return Ok(CliOutcome {
            return_code: 124,
            result,
            timed_out: true,
        });
    }

    let payload = payload.with_context(|| {
        format!(
            "réponse JSON invalide d'effectcraft-cli (code {}): {}",
            output.status.code().unwrap_or(1),
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    let prefs_ok = command_result(&payload, "prefs.set")
        .and_then(|result| result.get("scripting.allowScriptsWriteFiles"))
        .and_then(Value::as_bool)
        == Some(true);
    if !prefs_ok {
        bail!("effectcraft-cli n'a pas activé scripting.allowScriptsWriteFiles");
    }

    let script_result = command_result(&payload, "file.runScript")
        .context("effectcraft-cli run: résultat file.runScript absent")?;
    if let Some(error) = script_result.get("error").filter(|error| !error.is_null()) {
        tracing::error!("file.runScript en échec: {error}");
    }

    Ok(CliOutcome {
        return_code: output.status.code().unwrap_or(1),
        result,
        timed_out: false,
    })
}

fn command_result<'a>(payload: &'a Value, command: &str) -> Option<&'a Value> {
    payload
        .as_array()?
        .iter()
        .find(|entry| entry.get("command").and_then(Value::as_str) == Some(command))?
        .get("result")
}

fn consume_payload_output(payload: &Value, seen: &Arc<Mutex<HashSet<String>>>) -> Option<Value> {
    let script_output = command_result(payload, "file.runScript")
        .and_then(|result| result.get("output"))
        .and_then(Value::as_str)?;
    let mut result = None;
    for line in script_output.lines() {
        let inserted = {
            let mut seen = seen.lock().expect("verrou lignes");
            seen.insert(line.to_string())
        };
        if inserted {
            println!("{line}");
            for normalized in parse_panel_line(line) {
                println!("{normalized}");
            }
        }
        if let Some(value) = parse_result_line(line) {
            result = Some(value);
        }
    }
    let _ = std::io::stdout().flush();
    result
}

#[cfg(unix)]
fn install_signal_handler() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        if let Err(error) = ctrlc::set_handler(|| {
            INTERRUPTED.store(true, Ordering::SeqCst);
            let pid = CURRENT_CHILD_PID.load(Ordering::SeqCst);
            if pid > 0 {
                terminate_process_group(pid, false);
                for _ in 0..50 {
                    if !process_group_exists(pid) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                terminate_process_group(pid, true);
            }
            SIGNAL_DONE.store(true, Ordering::SeqCst);
            if pid == 0 {
                std::process::exit(143);
            }
        }) {
            tracing::warn!("Installation du gestionnaire SIGTERM/SIGINT impossible: {error}");
        }
    });
}

#[cfg(not(unix))]
fn install_signal_handler() {}

#[cfg(unix)]
fn process_group_exists(pid: i32) -> bool {
    unsafe { libc::kill(-pid, 0) == 0 }
}

#[cfg(unix)]
fn terminate_process_group(pid: i32, force: bool) {
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    unsafe {
        libc::kill(-pid, signal);
    }
}

#[cfg(windows)]
fn terminate_process_group(pid: i32, _force: bool) {
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .status();
}

#[cfg(not(any(unix, windows)))]
fn terminate_process_group(_pid: i32, _force: bool) {}

fn spawn_progress_tail(
    path: PathBuf,
    seen: Arc<Mutex<HashSet<String>>>,
    result: Arc<Mutex<Option<Value>>>,
) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let handle = std::thread::spawn(move || {
        let mut offset = 0_usize;
        let mut pending = String::new();
        let emit = |line: &str| {
            if line.is_empty() {
                return;
            }
            let inserted = {
                let mut seen = seen.lock().expect("verrou lignes");
                seen.insert(line.to_string())
            };
            if inserted {
                println!("{line}");
                for normalized in parse_panel_line(line) {
                    println!("{normalized}");
                }
                let _ = std::io::stdout().flush();
            }
            if let Some(value) = parse_result_line(line) {
                *result.lock().expect("verrou résultat") = Some(value);
            }
        };
        loop {
            match fs::read(&path) {
                Ok(bytes) => {
                    if bytes.len() < offset {
                        offset = 0;
                        pending.clear();
                    }
                    if bytes.len() > offset {
                        pending.push_str(&String::from_utf8_lossy(&bytes[offset..]));
                        offset = bytes.len();
                    }
                }
                Err(_) => {
                    if stop_thread.load(Ordering::Relaxed) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
            }
            while let Some(position) = pending.find('\n') {
                let line: String = pending.drain(..=position).collect();
                emit(line.trim_end_matches(['\n', '\r']));
            }
            if stop_thread.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if !pending.trim().is_empty() {
            emit(pending.trim_end_matches(['\n', '\r']));
        }
    });
    (stop, handle)
}

fn parse_result_line(line: &str) -> Option<Value> {
    let captures = re_result().captures(line)?;
    serde_json::from_str(&captures[1]).ok()
}

fn write_report(log_dir: &Path, report: &Step8Report) -> Result<PathBuf> {
    fs::create_dir_all(log_dir).with_context(|| format!("création de {}", log_dir.display()))?;
    let path = log_dir.join(format!(
        "step8_run_{}.json",
        Local::now().format("%Y%m%d_%H%M%S")
    ));
    workflow_core::fsutil::write_atomic(&path, &serde_json::to_vec_pretty(report)?)
        .with_context(|| format!("publication de {}", path.display()))?;
    Ok(path)
}

fn cleanup_drivers(drivers_dir: &Path, keep: usize) {
    let Ok(entries) = fs::read_dir(drivers_dir) else {
        return;
    };
    let mut drivers: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        (name.starts_with("step8_driver_") && name.ends_with(".jsx"))
                            || (name.starts_with("step8_progress_") && name.ends_with(".log"))
                    })
        })
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            (modified, path)
        })
        .collect();
    drivers.sort_by_key(|(modified, _)| *modified);
    for (_, stale) in drivers.into_iter().rev().skip(keep) {
        if let Err(error) = fs::remove_file(&stale) {
            tracing::warn!(
                "Nettoyage du driver {} impossible: {error}",
                stale.display()
            );
        }
    }
}

fn re_total() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Batch démarré\s*:\s*(\d+)\s+projet\(s\)").expect("regex total"))
}

fn re_progress() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"---\s*(\d+)/(\d+)\s*:\s*(.+?)\s*$").expect("regex progression"))
}

fn re_saved() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Projet enregistré\s*:\s*(.+?)\s*\(").expect("regex projet"))
}

fn re_unit_error() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Erreur sur\s+(.+?)\s*:").expect("regex erreur"))
}

fn re_batch_end() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Batch terminé\s*:\s*(.+?)\s*$").expect("regex fin"))
}

fn re_result() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"MS_BATCH_RESULT:\s*(\{.*\})\s*$").expect("regex résultat"))
}

fn re_version() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d+\.\d+(?:\.\d+)?").expect("regex version"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_panel_lines() {
        assert_eq!(
            parse_panel_line("[INFO] 12:34:56 | Batch démarré : 12 projet(s) sur 3 dossier(s)"),
            vec!["MS_BATCH_TOTAL: 12"]
        );
        assert_eq!(
            parse_panel_line("[INFO] 12:34:57 | --- 3/12 : clip1.mp4"),
            vec!["MS_BATCH_PROGRESS: 3/12 - clip1.mp4"]
        );
        assert_eq!(
            parse_panel_line("[INFO] Projet enregistré : clip1_9x16.ecproj (12.3s)"),
            vec!["MS_BATCH_OK: clip1_9x16.ecproj"]
        );
        assert_eq!(
            parse_panel_line("[ERREUR] Erreur sur clip2.mp4 : media absent"),
            vec!["MS_BATCH_FAIL: clip2.mp4"]
        );
        assert!(parse_panel_line("[INFO] Batch terminé : 2 ok / 0 erreur")
            .iter()
            .any(|line| line == "MS_BATCH_END: 2 ok / 0 erreur"));
    }

    #[test]
    fn builds_driver_with_config_and_folders() {
        let driver = build_driver_js(
            "var mediasolutionEC = {};\n",
            &["/tmp/a".to_string(), "/tmp/b".to_string()],
            Some("{\"docsSubFolderName\":\"docs\"}"),
            Path::new("/tmp/step8-progress.log"),
        );
        assert!(driver.contains("mediasolutionEC.applyConfig("));
        assert!(driver.contains("var MS_FOLDERS = [\"/tmp/a\",\"/tmp/b\"];"));
        assert!(driver.contains("runBatch(JSON.stringify(MS_FOLDERS), null, 0)"));
        assert!(driver.contains("var __msProgressPath = \"/tmp/step8-progress.log\";"));
        assert!(!driver.contains("app.run(\"prefs.set\""));
        assert!(driver.ends_with("\"step8-ok\";\n"));
    }

    #[test]
    fn result_requires_ok_and_no_errors() {
        assert!(evaluate_result(Some(&json!({"ok": true, "errors": []}))));
        assert!(!evaluate_result(Some(
            &json!({"ok": true, "errors": ["boom"]})
        )));
        assert!(!evaluate_result(Some(&json!({"ok": false}))));
        assert!(!evaluate_result(None));
    }

    #[test]
    fn empty_manifest_finishes_without_report() {
        let dir = tempfile::tempdir().unwrap();
        let step7 = dir.path().join("logs").join("step7");
        fs::create_dir_all(&step7).unwrap();
        fs::write(
            step7.join("finalized_projects_20261022_120000.json"),
            "{\"schema\":1,\"output_dir\":\"/tmp/out\",\"projects\":[]}",
        )
        .unwrap();
        let options = Step8Options {
            manifest: None,
            folders: Vec::new(),
            cli: PathBuf::from("effectcraft-cli"),
            panel_jsx: PathBuf::from("assets/effectcraft/MediaSolution.jsx"),
            panel_config: None,
            docs_subfolder: "docs".into(),
            logs_dir: dir.path().join("logs"),
            cwd: dir.path().to_path_buf(),
            dry_run: false,
            max_drivers_kept: 10,
            timeout_s: 0,
            post: PostOptions::default(),
        };
        let outcome = run_step8(&options).unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.processed, 0);
        assert!(outcome.report.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_effectcraft_cli_and_writes_failure_report() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("projet");
        fs::create_dir_all(folder.join("docs")).unwrap();
        let panel = dir.path().join("MediaSolution.jsx");
        fs::write(&panel, "var mediasolutionEC = {};\n").unwrap();
        let config = dir.path().join("mediasolution_ec.json");
        fs::write(&config, "{\"docsSubFolderName\":\"docs\"}").unwrap();
        let cli = dir.path().join("effectcraft-cli");
        crate::test_support::write_executable(
            &cli,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'effectcraft-cli 0.13.0'; exit 0; fi\nsleep 20\nprintf '%s' '[]'\n",
        );

        let options = Step8Options {
            manifest: None,
            folders: vec![folder],
            cli,
            panel_jsx: panel,
            panel_config: Some(config),
            docs_subfolder: "docs".into(),
            logs_dir: dir.path().join("logs"),
            cwd: dir.path().to_path_buf(),
            dry_run: false,
            max_drivers_kept: 10,
            timeout_s: 1,
            post: PostOptions::default(),
        };
        let outcome = run_step8(&options).unwrap();
        assert!(!outcome.success);
        assert!(outcome.timed_out);
        assert_eq!(outcome.exit_code, 124);
        let report_path = PathBuf::from(outcome.report.unwrap());
        let report: Value = serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
        assert_eq!(report["success"], false);
        assert_eq!(report["timed_out"], true);
        assert_eq!(report["timeout_s"], 1);
        assert_eq!(report["cli_return_code"], 124);
    }

    #[cfg(unix)]
    #[test]
    fn runs_post_processing_after_effectcraft() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("projet");
        fs::create_dir_all(folder.join("docs")).unwrap();
        fs::create_dir_all(folder.join("projets")).unwrap();
        fs::write(
            folder.join("projets").join("clip_9x16.ecproj"),
            r#"{"next_id":20,"items":{"1":{"name":"clip.mp4","kind":{"type":"Footage","width":1920,"height":1080}},"2":{"name":"Comp","kind":{"type":"Comp","width":1080,"height":1920,"frame_rate":{"num":25,"den":1},"duration":2540160000000,"layers":[{"id":10,"name":"Segment 1","in_point":0,"out_point":2540160000000,"source":{"type":"Footage","item":1},"props":{"children":[{"name":"Effects","children":[]},{"name":"Transform","children":[{"name":"Anchor Point","value":{"v":[960.0,0.0,0.0]}},{"name":"Position","value":{"v":[540.0,0.0,0.0]}},{"name":"Scale","value":{"v":[180.0,180.0,0.0]}}]}]}]}}}"#,
        )
        .unwrap();
        let panel = dir.path().join("MediaSolution.jsx");
        fs::write(&panel, "var mediasolutionEC = {};\n").unwrap();
        let config = dir.path().join("mediasolution_ec.json");
        fs::write(&config, "{\"docsSubFolderName\":\"docs\"}").unwrap();
        let cli = dir.path().join("effectcraft-cli");
        crate::test_support::write_executable(
            &cli,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo 'effectcraft-cli 0.13.0'; exit 0; fi
printf '%s' '[{"command":"prefs.set","result":{"scripting.allowScriptsWriteFiles":true}},{"command":"file.runScript","result":{"ok":true,"error":null,"result":"step8-ok","output":"MS_BATCH_RESULT: {\"ok\": true, \"errors\": []}"}}]'
"#,
        );

        let options = Step8Options {
            manifest: None,
            folders: vec![folder.clone()],
            cli,
            panel_jsx: panel,
            panel_config: Some(config),
            docs_subfolder: "docs".into(),
            logs_dir: dir.path().join("logs"),
            cwd: dir.path().to_path_buf(),
            dry_run: false,
            max_drivers_kept: 10,
            timeout_s: 0,
            post: PostOptions::default(),
        };
        let outcome = run_step8(&options).unwrap();
        assert!(outcome.success);
        let post = outcome.post.expect("résumé de post-traitement");
        assert_eq!(post.projects, 1);
        assert_eq!(post.assists, 1);
        assert!(folder
            .join("projets")
            .join("clip_9x16_assist.ecproj")
            .is_file());
        assert!(folder
            .join("docs")
            .join("post")
            .join("clip_9x16_assist")
            .join("audit.json")
            .is_file());
    }

    #[cfg(unix)]
    #[test]
    fn runs_effectcraft_cli_and_writes_report() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("projet");
        fs::create_dir_all(folder.join("docs")).unwrap();
        let panel = dir.path().join("MediaSolution.jsx");
        fs::write(&panel, "var mediasolutionEC = {};\n").unwrap();
        let config = dir.path().join("mediasolution_ec.json");
        fs::write(&config, "{\"docsSubFolderName\":\"docs\"}").unwrap();
        let cli = dir.path().join("effectcraft-cli");
        crate::test_support::write_executable(
            &cli,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo 'effectcraft-cli 0.13.0'; exit 0; fi
printf '%s' '[{"command":"prefs.set","result":{"scripting.allowScriptsWriteFiles":true}},{"command":"file.runScript","result":{"ok":true,"error":null,"result":"step8-ok","output":"[INFO] 12:00:00 | Batch démarré : 1 projet(s)\n[INFO] 12:00:01 | --- 1/1 : projet\n[INFO] 12:00:02 | Projet enregistré : projet_9x16.ecproj (0.1s)\nMS_BATCH_RESULT: {\"ok\": true, \"errors\": []}"}}]'
"#,
        );

        let options = Step8Options {
            manifest: None,
            folders: vec![folder],
            cli,
            panel_jsx: panel,
            panel_config: Some(config),
            docs_subfolder: "docs".into(),
            logs_dir: dir.path().join("logs"),
            cwd: dir.path().to_path_buf(),
            dry_run: false,
            max_drivers_kept: 10,
            timeout_s: 0,
            post: PostOptions::default(),
        };
        let outcome = run_step8(&options).unwrap();
        assert!(outcome.success);
        assert_eq!(outcome.processed, 1);
        assert!(!outcome.timed_out);
        assert_eq!(outcome.exit_code, 0);
        let report_path = PathBuf::from(outcome.report.unwrap());
        let report: Value = serde_json::from_slice(&fs::read(report_path).unwrap()).unwrap();
        assert_eq!(report["success"], true);
        assert_eq!(report["mode"], "headless");
        assert_eq!(report["bridge_port"], Value::Null);
        assert_eq!(report["app_launched"], false);
        assert_eq!(report["cli_version"], "0.13.0");
        assert_eq!(report["result"]["ok"], true);
        assert_eq!(report["folders"].as_array().unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn direct_template_skips_panel_and_still_publishes_raw() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("340 Camille");
        fs::create_dir_all(folder.join("docs")).unwrap();
        fs::write(folder.join("docs/M6 - SDM - Leslie et Léo.mp4"), b"video").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let options = Step8Options {
            manifest: None,
            folders: vec![folder.clone()],
            cli: dir.path().join("effectcraft-cli-introuvable"),
            panel_jsx: dir.path().join("MediaSolution-introuvable.jsx"),
            panel_config: None,
            docs_subfolder: "docs".into(),
            logs_dir: dir.path().join("logs"),
            cwd: dir.path().to_path_buf(),
            dry_run: false,
            max_drivers_kept: 10,
            timeout_s: 0,
            post: PostOptions {
                direct: true,
                enabled: false,
                ffprobe,
                ..PostOptions::default()
            },
        };
        let outcome = run_step8(&options).unwrap();
        assert!(outcome.success);
        assert!(folder
            .join("projets/M6 - SDM - Leslie et Léo_9x16.ecproj")
            .is_file());
        let report: Value =
            serde_json::from_slice(&fs::read(outcome.report.unwrap()).unwrap()).unwrap();
        assert_eq!(report["mode"], "direct");
        assert_eq!(report["panel_skipped"], true);
        assert_eq!(report["direct_projects"], 1);
        assert_eq!(report["direct_created"], 1);
        assert_eq!(report["panel_folders"].as_array().unwrap().len(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn direct_write_failure_fails_step8() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("340 Camille");
        fs::create_dir_all(folder.join("docs")).unwrap();
        fs::write(folder.join("docs/M6 - SDM - Leslie et Léo.mp4"), b"video").unwrap();
        // Un fichier à la place du dossier `projets` force l'échec direct.
        fs::write(folder.join("projets"), b"obstacle").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let options = Step8Options {
            manifest: None,
            folders: vec![folder],
            cli: dir.path().join("effectcraft-cli-introuvable"),
            panel_jsx: dir.path().join("MediaSolution-introuvable.jsx"),
            panel_config: None,
            docs_subfolder: "docs".into(),
            logs_dir: dir.path().join("logs"),
            cwd: dir.path().to_path_buf(),
            dry_run: false,
            max_drivers_kept: 10,
            timeout_s: 0,
            post: PostOptions {
                direct: true,
                enabled: false,
                ffprobe,
                ..PostOptions::default()
            },
        };
        let outcome = run_step8(&options).unwrap();
        assert!(!outcome.success);
        assert_eq!(outcome.exit_code, 1);
        let report: Value =
            serde_json::from_slice(&fs::read(outcome.report.unwrap()).unwrap()).unwrap();
        assert_eq!(report["success"], false);
        assert_eq!(report["panel_skipped"], true);
    }

    #[cfg(unix)]
    #[test]
    fn mixed_folder_keeps_panel_and_skips_only_the_sdm() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("340 Camille");
        fs::create_dir_all(folder.join("docs")).unwrap();
        fs::write(folder.join("docs/M6 - SDM - Leslie et Léo.mp4"), b"video").unwrap();
        fs::write(folder.join("docs/Autre - Sujet.mp4"), b"video").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
case "$*" in
  *"M6 - SDM"*) printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}' ;;
  *) printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1280,"height":720,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"250","duration":"10.000000"}]}' ;;
esac
"#,
        );
        let panel = dir.path().join("MediaSolution.jsx");
        fs::write(&panel, "var mediasolutionEC = {};\n").unwrap();
        let config = dir.path().join("mediasolution_ec.json");
        fs::write(&config, "{\"docsSubFolderName\":\"docs\"}").unwrap();
        let cli = dir.path().join("effectcraft-cli");
        crate::test_support::write_executable(
            &cli,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo 'effectcraft-cli 0.13.0'; exit 0; fi
printf '%s' '[{"command":"prefs.set","result":{"scripting.allowScriptsWriteFiles":true}},{"command":"file.runScript","result":{"ok":true,"error":null,"result":"step8-ok","output":"MS_BATCH_RESULT: {\"ok\": true, \"errors\": []}"}}]'
"#,
        );
        let options = Step8Options {
            manifest: None,
            folders: vec![folder.clone()],
            cli,
            panel_jsx: panel,
            panel_config: Some(config),
            docs_subfolder: "docs".into(),
            logs_dir: dir.path().join("logs"),
            cwd: dir.path().to_path_buf(),
            dry_run: false,
            max_drivers_kept: 10,
            timeout_s: 0,
            post: PostOptions {
                direct: true,
                enabled: false,
                ffprobe,
                ..PostOptions::default()
            },
        };
        let outcome = run_step8(&options).unwrap();
        assert!(outcome.success);
        assert!(folder
            .join("projets/M6 - SDM - Leslie et Léo_9x16.ecproj")
            .is_file());
        let report: Value =
            serde_json::from_slice(&fs::read(outcome.report.unwrap()).unwrap()).unwrap();
        assert_eq!(report["panel_skipped"], false);
        assert_eq!(report["panel_folders"].as_array().unwrap().len(), 1);
        let driver = fs::read_to_string(report["driver"].as_str().unwrap()).unwrap();
        assert!(driver.contains("m6 - sdm - leslie et léo.mp4"));
        assert!(!driver.contains("M6 - SDM - Leslie et Léo.mp4"));
    }

    #[test]
    fn skip_names_are_injected_without_losing_panel_config() {
        let config = r#"{"docsSubFolderName":"docs","videoSkipNames":["M6+.MP4"]}"#;
        let injected =
            inject_video_skip_names(Some(config), &["M6 - SDM - Leslie et Léo.mp4".to_string()])
                .unwrap();
        let value: Value = serde_json::from_str(&injected).unwrap();
        assert_eq!(value["docsSubFolderName"], "docs");
        assert_eq!(value["videoSkipNames"].as_array().unwrap().len(), 2);
        assert_eq!(value["videoSkipNames"][0], "m6+.mp4");
        assert_eq!(value["videoSkipNames"][1], "m6 - sdm - leslie et léo.mp4");
        let injected = inject_video_skip_names(None, &["CLIP.mp4".to_string()]).unwrap();
        let value: Value = serde_json::from_str(&injected).unwrap();
        assert_eq!(value["videoSkipNames"][0], "m6+.mp4");
        assert_eq!(value["videoSkipNames"][1], "clip.mp4");
    }
}
