//! Contrat HTTP de polling consommé par le frontend existant.

use crate::catalog::{frontend_config, LogKind, SpecificLog, StepKey};
use crate::progress::html_escape;
use crate::runner::Runner;
use crate::sequence::run_sequence;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use glob::glob;
use minijinja::{context, Environment};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;
use tower_http::services::ServeDir;

pub struct AppState {
    pub runner: Arc<Runner>,
    template: Result<String, String>,
    cache_buster: String,
    static_dir: PathBuf,
    sound_design_dir: PathBuf,
}

impl AppState {
    pub fn new(runner: Arc<Runner>) -> Arc<Self> {
        let config = runner.config();
        let template_path = config.server.template.clone();
        let static_dir = config.server.frontend_dir.clone();
        let sound_design_dir = config.server.sound_design_dir.clone();
        let template = fs::read_to_string(&template_path)
            .map_err(|error| format!("gabarit {}: {error}", template_path.display()));
        Arc::new(Self {
            runner,
            template,
            cache_buster: format!(
                "{}",
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0)
            ),
            static_dir,
            sound_design_dir,
        })
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let static_dir = state.static_dir.clone();
    let sound_dir = state.sound_design_dir.clone();
    Router::new()
        .route("/", get(index))
        .route("/favicon.ico", get(favicon))
        .route("/run/{step}", post(run_step))
        .route("/run_custom_sequence", post(run_custom_sequence))
        .route("/status/{step}", get(step_status))
        .route("/stop/{step}", post(stop_step))
        .route("/cancel/{step}", post(cancel_step))
        .route("/get_specific_log/{step}/{index}", get(specific_log))
        .route("/sequence/status", get(sequence_status))
        .route("/sequence/stop", post(stop_sequence))
        .route("/api/ping", get(ping))
        .route("/api/step_status/{step}", get(api_step_status))
        .route("/api/system_monitor", get(system_monitor))
        .route("/api/csv_downloads_status", get(csv_downloads_status))
        .route("/api/csv_monitor_status", get(csv_monitor_status))
        .route("/api/performance/frontend", post(performance_frontend))
        .nest_service("/static", ServeDir::new(static_dir))
        .nest_service("/sound-design", ServeDir::new(sound_dir))
        .with_state(state)
}

async fn index(State(state): State<Arc<AppState>>) -> Response {
    let template = match &state.template {
        Ok(template) => template,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!(
                    "<h1>Gabarit indisponible</h1><p>{}</p>",
                    html_escape(error)
                )),
            )
                .into_response();
        }
    };
    let frontend = frontend_config(state.runner.config());
    let frontend_json = serde_json::to_string(&frontend).unwrap_or_else(|_| "{}".into());
    let source = preprocess_template(template);
    let token = html_escape(&state.runner.config().server.worker_token);
    let result = (|| -> Result<String, minijinja::Error> {
        let mut environment = Environment::new();
        environment.add_template("index", &source)?;
        let template = environment.get_template("index")?;
        template.render(context! {
            steps_config => frontend,
            steps_config_json => frontend_json,
            worker_token => token,
            cache_buster => state.cache_buster,
        })
    })();
    match result {
        Ok(html) => no_cache(Html(html).into_response()),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!(
                "<h1>Rendu impossible</h1><p>{}</p>",
                html_escape(&error.to_string())
            )),
        )
            .into_response(),
    }
}

async fn favicon() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn ping(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }
    Json(json!({"status": "pong", "worker_type": "ubuntu_new", "runtime": "rust"})).into_response()
}

async fn run_step(
    State(state): State<Arc<AppState>>,
    Path(step): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }
    let Some(step) = StepKey::parse(&step) else {
        return status_error(StatusCode::NOT_FOUND, "Étape inconnue");
    };
    if let Some(reason) = step.unavailable_reason() {
        return status_error(StatusCode::BAD_REQUEST, reason);
    }
    let control = match state.runner.state().reserve_step(step) {
        Ok(control) => control,
        Err(message) => {
            let code = if message.contains("en cours") {
                StatusCode::CONFLICT
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return status_error(code, &message);
        }
    };
    let display_name = step.display_name();
    let runner = state.runner.clone();
    tokio::spawn(async move {
        runner.execute_reserved(step, control).await;
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "initiated",
            "message": format!("Lancement de: {display_name}")
        })),
    )
        .into_response()
}

async fn run_custom_sequence(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }
    let body: Value = match serde_json::from_slice(&body) {
        Ok(body) => body,
        Err(_) => return status_error(StatusCode::BAD_REQUEST, "Invalid steps list"),
    };
    let Some(values) = body.get("steps").and_then(Value::as_array) else {
        return status_error(StatusCode::BAD_REQUEST, "Invalid steps list");
    };
    let mut steps = Vec::with_capacity(values.len());
    for value in values {
        let Some(raw) = value.as_str() else {
            return status_error(StatusCode::BAD_REQUEST, "Invalid steps list");
        };
        let Some(step) = StepKey::parse(raw) else {
            return status_error(StatusCode::BAD_REQUEST, &format!("Étape inconnue : {raw}"));
        };
        if let Some(reason) = step.unavailable_reason() {
            return status_error(StatusCode::BAD_REQUEST, reason);
        }
        steps.push(step);
    }
    if let Err(message) = state.runner.state().start_sequence("Custom") {
        let code = if message.contains("en cours") {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        };
        return status_error(code, &message);
    }
    let runner = state.runner.clone();
    let count = steps.len();
    tokio::spawn(async move {
        run_sequence(runner, steps, "Custom".into()).await;
    });
    (
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "initiated",
            "message": format!("Séquence personnalisée lancée avec {count} étapes.")
        })),
    )
        .into_response()
}

async fn step_status(State(state): State<Arc<AppState>>, Path(step): Path<String>) -> Response {
    let Some(step) = StepKey::parse(&step) else {
        return status_error(StatusCode::NOT_FOUND, "Étape inconnue");
    };
    match state.runner.state().step_status(step, true) {
        Some(status) => no_cache(Json(status).into_response()),
        None => status_error(StatusCode::NOT_FOUND, "Étape inconnue"),
    }
}

async fn api_step_status(State(state): State<Arc<AppState>>, Path(step): Path<String>) -> Response {
    let Some(step) = StepKey::parse(&step) else {
        return raw_error(StatusCode::NOT_FOUND, "Étape inconnue");
    };
    match state.runner.state().step_status(step, false) {
        Some(status) => no_cache(Json(status).into_response()),
        None => raw_error(StatusCode::NOT_FOUND, "Étape inconnue"),
    }
}

async fn stop_step(
    State(state): State<Arc<AppState>>,
    Path(step): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }
    let Some(step) = StepKey::parse(&step) else {
        return status_error(StatusCode::NOT_FOUND, "Step introuvable");
    };
    match state.runner.state().stop_step(step) {
        Ok(()) => Json(json!({
            "status": "success",
            "message": format!("Step '{}' stopped successfully", step.as_str())
        }))
        .into_response(),
        Err(message) => Json(json!({"status": "error", "message": message})).into_response(),
    }
}

async fn cancel_step(
    State(state): State<Arc<AppState>>,
    Path(step): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }
    let Some(step) = StepKey::parse(&step) else {
        return status_error(StatusCode::BAD_REQUEST, "Step introuvable");
    };
    match state.runner.state().stop_step(step) {
        Ok(()) => Json(json!({
            "status": "success",
            "message": format!("Step '{}' stopped successfully", step.as_str())
        }))
        .into_response(),
        Err(message) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"status": "error", "message": message})),
        )
            .into_response(),
    }
}

async fn sequence_status(State(state): State<Arc<AppState>>) -> Response {
    no_cache(Json(state.runner.state().sequence_status()).into_response())
}

async fn stop_sequence(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(response) = authorize(&state, &headers) {
        return response;
    }
    match state.runner.state().stop_sequence() {
        Ok(()) => Json(json!({
            "status": "success",
            "message": "Sequence stopped. Stopped steps: current"
        }))
        .into_response(),
        Err(message) if message.contains("Aucune") => status_error(StatusCode::CONFLICT, &message),
        Err(message) => status_error(StatusCode::INTERNAL_SERVER_ERROR, &message),
    }
}

async fn specific_log(
    State(state): State<Arc<AppState>>,
    Path((step, index)): Path<(String, usize)>,
) -> Response {
    let Some(step) = StepKey::parse(&step) else {
        return raw_error(StatusCode::NOT_FOUND, "Step not found");
    };
    let logs = step.specific_logs(state.runner.config());
    let Some(log) = logs.get(index) else {
        return raw_error(StatusCode::NOT_FOUND, "Log index out of range");
    };
    let (content, error, path) = read_log(log, step);
    Json(json!({
        "content": content,
        "error": error,
        "path": path,
        "type": match log.kind {
            LogKind::File => "file",
            LogKind::DirectoryLatest => "directory_latest",
        },
        "lines_requested": log.lines,
    }))
    .into_response()
}

async fn system_monitor(State(state): State<Arc<AppState>>) -> Response {
    let _ = state;
    Json(system_metrics()).into_response()
}

async fn csv_downloads_status() -> Response {
    Json(json!([])).into_response()
}

async fn csv_monitor_status() -> Response {
    Json(json!({
        "csv_monitor": {"status": "stopped", "last_check": null, "error": null},
        "data_source": "webhook",
        "monitor_interval": 15,
        "failed_urls": [],
        "failed_urls_count": 0,
        "webhook": {"available": false, "last_fetch_ts": null, "error": "legacy Python", "records": 0}
    }))
    .into_response()
}

async fn performance_frontend() -> Response {
    Json(json!({"status": "ok"})).into_response()
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Option<Response> {
    let expected = state.runner.config().server.worker_token.trim();
    if expected.is_empty() {
        return Some(raw_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Authentication not configured",
        ));
    }
    let Some(received) = headers
        .get("x-worker-token")
        .and_then(|value| value.to_str().ok())
    else {
        return Some(raw_error(
            StatusCode::UNAUTHORIZED,
            "Missing authentication token",
        ));
    };
    if received != expected {
        return Some(raw_error(
            StatusCode::UNAUTHORIZED,
            "Invalid authentication token",
        ));
    }
    None
}

fn status_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"status": "error", "message": message}))).into_response()
}

fn raw_error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"error": message}))).into_response()
}

fn no_cache(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, header::HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert(header::EXPIRES, header::HeaderValue::from_static("0"));
    response
}

fn read_log(log: &SpecificLog, step: StepKey) -> (String, Option<String>, String) {
    let path = match log.kind {
        LogKind::File => log.path.clone(),
        LogKind::DirectoryLatest => {
            let pattern = format!("{}/{}", log.path.display(), log.pattern);
            let mut candidates: Vec<PathBuf> = glob(&pattern)
                .map(|paths| {
                    paths
                        .filter_map(Result::ok)
                        .filter(|path| path.is_file())
                        .collect()
                })
                .unwrap_or_default();
            candidates.sort_by_key(|path| {
                fs::metadata(path)
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH)
            });
            match candidates.pop() {
                Some(path) => path,
                None => {
                    // STEP5/6/7 n'écrivent pas de journal propre : le journal
                    // principal de l'orchestrateur reste consultable.
                    let fallback = log
                        .path
                        .parent()
                        .map(|root| root.join(format!("step_{}.log", step.as_str())));
                    if let Some(fallback) = fallback.filter(|path| path.is_file()) {
                        fallback
                    } else {
                        return (
                            format!(
                                "Aucun fichier correspondant au pattern '{}' dans {}",
                                log.pattern,
                                log.path.display()
                            ),
                            Some("Aucun fichier trouvé".into()),
                            format!("{}/{}", log.path.display(), log.pattern),
                        );
                    }
                }
            }
        }
    };
    let processed = path.display().to_string();
    if !path.exists() {
        return (
            format!("Fichier non trouvé: {processed}"),
            Some("Fichier non trouvé".into()),
            processed,
        );
    }
    match fs::read_to_string(&path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let start = lines.len().saturating_sub(log.lines);
            let tail = if start == lines.len() {
                "Fichier vide.".to_string()
            } else {
                lines[start..].join("\n")
            };
            (tail, None, processed)
        }
        Err(error) => (
            format!("Impossible de lire le fichier: {error}"),
            Some(format!("Erreur de lecture: {error}")),
            processed,
        ),
    }
}

fn preprocess_template(source: &str) -> String {
    static URL_FOR: OnceLock<Regex> = OnceLock::new();
    let url_for = URL_FOR.get_or_init(|| {
        Regex::new(r"\{\{\s*url_for\('static',\s*filename='([^']+)'\)\s*\}\}").expect("url_for")
    });
    let source = url_for.replace_all(source, "/static/$1");
    source
        .replace("steps_config.items()", "steps_config|items")
        .replace("{{ csrf_token() if csrf_token else '' }}", "")
        .replace(
            "{{ worker_token if worker_token else '' }}",
            "{{ worker_token }}",
        )
        .replace(
            "{{ steps_config | tojson | safe }}",
            "{{ steps_config_json | safe }}",
        )
}

fn system_metrics() -> Value {
    let cpus = std::thread::available_parallelism()
        .map(|value| value.get() as f64)
        .unwrap_or(1.0);
    let load = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|text| text.split_whitespace().next()?.parse::<f64>().ok())
        .unwrap_or(0.0);
    let cpu_percent = (load / cpus * 100.0).clamp(0.0, 100.0);
    let mut total_kb: f64 = 0.0;
    let mut available_kb: f64 = 0.0;
    if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
        for line in meminfo.lines() {
            if let Some(value) = line.strip_prefix("MemTotal:") {
                total_kb = value
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0);
            } else if let Some(value) = line.strip_prefix("MemAvailable:") {
                available_kb = value
                    .split_whitespace()
                    .next()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0.0);
            }
        }
    }
    let total_gb = total_kb / 1024.0 / 1024.0;
    let available_gb = available_kb / 1024.0 / 1024.0;
    let used_gb = (total_gb - available_gb).max(0.0);
    let percent = if total_gb > 0.0 {
        used_gb / total_gb * 100.0
    } else {
        0.0
    };
    let (disk_percent, disk_used_gb, disk_total_gb, disk_free_gb) = disk_metrics();
    json!({
        "cpu_percent": cpu_percent,
        "memory": {
            "percent": percent,
            "used_gb": used_gb,
            "total_gb": total_gb,
            "available_gb": available_gb
        },
        "gpu": Value::Null,
        "disk": {
            "percent": disk_percent,
            "used_gb": disk_used_gb,
            "total_gb": disk_total_gb,
            "free_gb": disk_free_gb
        },
        "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        "snapshot_age_s": 0.0
    })
}

fn disk_metrics() -> (f64, f64, f64, f64) {
    let Ok(path) = std::ffi::CString::new("/") else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let block = stat.f_frsize.max(1) as f64;
    let total = stat.f_blocks as f64 * block / 1024.0 / 1024.0 / 1024.0;
    let free = stat.f_bavail as f64 * block / 1024.0 / 1024.0 / 1024.0;
    let used = (total - free).max(0.0);
    let percent = if total > 0.0 {
        used / total * 100.0
    } else {
        0.0
    };
    (percent, used, total, free)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::WorkflowState;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tower::ServiceExt;
    use workflow_core::Config;

    fn test_app() -> (tempfile::TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fake-workflow-cli");
        fs::write(
            &cli,
            "#!/bin/sh\necho 'TOTAL_JSON_TO_REDUCE: 1'\necho 'Succès: réduction JSON terminée pour demo'\n",
        )
        .unwrap();
        let mut permissions = fs::metadata(&cli).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&cli, permissions).unwrap();
        fs::create_dir_all(dir.path().join("static")).unwrap();
        fs::write(dir.path().join("static/app.js"), "console.log('ok')").unwrap();
        fs::write(
            dir.path().join("index.html"),
            "{% for key, config in steps_config.items() %}{{ key }}={{ config.display_name }};{% endfor %}{{ worker_token }}",
        )
        .unwrap();
        let mut config = Config::default();
        config.paths.base = dir.path().join("runtime");
        config.paths.projects = Some(config.paths.base.join("projets_extraits"));
        config.paths.logs = Some(config.paths.base.join("logs"));
        config.paths.history_db = Some(config.paths.base.join("history.sqlite3"));
        config.server.workflow_cli = cli;
        config.server.workdir = dir.path().to_path_buf();
        config.server.frontend_dir = dir.path().join("static");
        config.server.template = dir.path().join("index.html");
        config.server.sound_design_dir = dir.path().join("sound");
        config.server.worker_token = "test-token".into();
        config.hardware.lock_path = dir.path().join("hardware.lock");
        let runner = Runner::new(
            config,
            dir.path().join("config.toml"),
            Arc::new(WorkflowState::new()),
        )
        .unwrap();
        let app = router(AppState::new(runner));
        (dir, app)
    }

    #[test]
    fn real_index_template_renders_with_existing_frontend_syntax() {
        let source = include_str!("../../../assets/web/templates/index_new.html");
        let source = preprocess_template(source);
        let frontend = frontend_config(&Config::default());
        let frontend_json = serde_json::to_string(&frontend).unwrap();
        let mut environment = Environment::new();
        environment.add_template("index", &source).unwrap();
        let html = environment
            .get_template("index")
            .unwrap()
            .render(context! {
                steps_config => frontend,
                steps_config_json => frontend_json,
                worker_token => "token",
                cache_buster => "42",
            })
            .unwrap();
        for step in StepKey::ALL {
            assert!(html.contains(&format!("id=\"step-{}\"", step.as_str())));
        }
        assert!(html.contains("/static/css/base.css"));
        assert!(!html.contains("url_for("));
        assert!(!html.contains("steps_config | tojson"));
    }

    #[tokio::test]
    async fn index_renders_without_a_template_engine_specific_filter() {
        let (_dir, app) = test_app();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("STEP1=1. Extraction"));
    }

    #[tokio::test]
    async fn mutating_routes_require_the_worker_token() {
        let (_dir, app) = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/run/STEP5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn run_and_status_follow_the_polling_contract() {
        let (_dir, app) = test_app();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/run/STEP5")
                    .header("x-worker-token", "test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/status/STEP5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["step"], "STEP5");
        assert!(value["log"].is_array());
    }
}
