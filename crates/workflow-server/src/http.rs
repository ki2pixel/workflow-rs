//! Contrat HTTP de polling consommé par le frontend existant.

use crate::catalog::{frontend_config, LogKind, SpecificLog, StepKey};
use crate::error::WorkflowServerError;
use crate::progress::html_escape;
use crate::runner::Runner;
use crate::sequence::run_sequence;
use axum::body::Bytes;
use axum::extract::{Path, Request, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use glob::glob;
use minijinja::{context, Environment};
use regex::Regex;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;
use subtle::ConstantTimeEq;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;
use workflow_ingest::IngestionService;

pub struct AppState {
    pub runner: Arc<Runner>,
    pub ingestion: Arc<IngestionService>,
    template: Result<String, String>,
    cache_buster: String,
    static_dir: PathBuf,
    sound_design_dir: PathBuf,
}

impl AppState {
    pub fn new(runner: Arc<Runner>) -> anyhow::Result<Arc<Self>> {
        let config = runner.config();
        let ingestion = Arc::new(IngestionService::new(config, runner.db().repository())?);
        let template_path = config.server.template.clone();
        let static_dir = config.server.frontend_dir.clone();
        let sound_design_dir = config.server.sound_design_dir.clone();
        let template = fs::read_to_string(&template_path)
            .map_err(|error| format!("gabarit {}: {error}", template_path.display()));
        Ok(Arc::new(Self {
            runner,
            ingestion,
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
        }))
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
        .layer(middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn security_headers(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let mut response = next.run(request).await;
    if response.status().is_client_error() || response.status().is_server_error() {
        tracing::warn!(
            %method,
            path = %path,
            status = %response.status(),
            "requête HTTP en erreur"
        );
    }
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' blob:; connect-src 'self'; font-src 'self' data:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'",
        ),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    response
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
        Err(error) => return error.into_response(),
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
    let sequence_id = match state.runner.state().start_sequence(steps.clone(), "Custom") {
        Ok(sequence_id) => sequence_id,
        Err(error) => {
            let status = if error.is_conflict() {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            return status_error(status, &error.to_string());
        }
    };
    let runner = state.runner.clone();
    let count = steps.len();
    tokio::spawn(async move {
        run_sequence(runner, steps, "Custom".into(), sequence_id).await;
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
        Err(error) => match error {
            WorkflowServerError::StepNotRunning { step } => {
                status_error(StatusCode::OK, &format!("Step '{step}' is not running"))
            }
            other => other.into_response(),
        },
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
        Err(error) => error.into_response(),
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
        Err(error) => error.into_response(),
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
    let logs_root = state.runner.config().paths.resolve().logs;
    let log_for_read = log.clone();
    let (content, error, path) = match tokio::task::spawn_blocking(move || {
        read_log(&log_for_read, step, &logs_root)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => (
            format!("Lecture du journal interrompue: {error}"),
            Some("Erreur interne".into()),
            String::new(),
        ),
    };
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

async fn csv_downloads_status(State(state): State<Arc<AppState>>) -> Response {
    no_cache(Json(state.ingestion.downloads_status()).into_response())
}

async fn csv_monitor_status(State(state): State<Arc<AppState>>) -> Response {
    no_cache(Json(state.ingestion.monitor_status_json()).into_response())
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
    if received.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() != 1 {
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

fn read_log(
    log: &SpecificLog,
    step: StepKey,
    logs_root: &std::path::Path,
) -> (String, Option<String>, String) {
    let root = fs::canonicalize(logs_root).unwrap_or_else(|_| logs_root.to_path_buf());
    let path = match log.kind {
        LogKind::File => log.path.clone(),
        LogKind::DirectoryLatest => {
            let pattern = format!("{}/{}", log.path.display(), log.pattern);
            let mut candidates: Vec<PathBuf> = glob(&pattern)
                .map(|paths| {
                    paths
                        .filter_map(Result::ok)
                        .filter(|path| path.is_file())
                        .filter(|path| log_path_is_confined(path, &root))
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
                    if let Some(fallback) =
                        fallback.filter(|path| path.is_file() && log_path_is_confined(path, &root))
                    {
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
    if !log_path_is_confined(&path, &root) {
        return (
            format!("Accès refusé hors de paths.logs: {processed}"),
            Some("Accès refusé".into()),
            processed,
        );
    }
    match read_tail(&path, log.lines) {
        Ok(content) => (content, None, processed),
        Err(error) => (
            format!("Impossible de lire le fichier: {error}"),
            Some(format!("Erreur de lecture: {error}")),
            processed,
        ),
    }
}

fn log_path_is_confined(path: &std::path::Path, root: &std::path::Path) -> bool {
    fs::canonicalize(path)
        .map(|canonical| canonical.starts_with(root))
        .unwrap_or(false)
}

/// Lit uniquement la queue du fichier, sans charger un journal de plusieurs
/// centaines de Mio en mémoire.
fn read_tail(path: &std::path::Path, max_lines: usize) -> std::io::Result<String> {
    const MAX_TAIL_BYTES: u64 = 512 * 1024;
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    let mut position = length;
    let mut data = Vec::new();
    while position > 0 && data.len() < MAX_TAIL_BYTES as usize {
        let chunk = (position.min(64 * 1024)) as usize;
        position -= chunk as u64;
        file.seek(SeekFrom::Start(position))?;
        let mut buffer = vec![0_u8; chunk];
        file.read_exact(&mut buffer)?;
        buffer.extend_from_slice(&data);
        data = buffer;
        if data.iter().filter(|byte| **byte == b'\n').count() > max_lines + 1 {
            break;
        }
    }
    let text = String::from_utf8_lossy(&data);
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    if start == lines.len() {
        Ok("Fichier vide.".to_string())
    } else {
        Ok(lines[start..].join("\n"))
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
        test_app_with_script(
            "#!/bin/sh\necho 'TOTAL_JSON_TO_REDUCE: 1'\necho 'Succès: réduction JSON terminée pour demo'\n",
        )
    }

    fn test_app_with_script(script: &str) -> (tempfile::TempDir, Router) {
        let dir = tempfile::tempdir().unwrap();
        let cli = dir.path().join("fake-workflow-cli");
        fs::write(&cli, script).unwrap();
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
        let app = router(AppState::new(runner).unwrap());
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
    async fn given_large_log_when_requested_then_only_tail_is_returned() {
        let (dir, app) = test_app();
        let logs = dir.path().join("runtime").join("logs").join("step5");
        fs::create_dir_all(&logs).unwrap();
        let mut content = String::new();
        for index in 0..10_000 {
            content.push_str(&format!("ligne-{index}\n"));
        }
        fs::write(logs.join("run.log"), content).unwrap();
        let body = app
            .oneshot(
                Request::builder()
                    .uri("/get_specific_log/STEP5/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        let content = value["content"].as_str().unwrap();
        assert!(content.contains("ligne-9999"));
        assert!(!content.contains("ligne-0\n"));
        assert!(content.len() < 64 * 1024);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn given_symlink_outside_logs_when_requested_then_content_not_leaked() {
        use std::os::unix::fs::symlink;
        let (dir, app) = test_app();
        let logs = dir.path().join("runtime").join("logs").join("step5");
        fs::create_dir_all(&logs).unwrap();
        let secret = dir.path().join("secret.txt");
        fs::write(&secret, "contenu-secret-hors-logs").unwrap();
        symlink(&secret, logs.join("evil.log")).unwrap();
        let body = app
            .oneshot(
                Request::builder()
                    .uri("/get_specific_log/STEP5/0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert!(!value.to_string().contains("contenu-secret-hors-logs"));
        assert!(value["error"].is_string());
    }

    #[tokio::test]
    async fn given_index_when_served_then_security_headers_present() {
        let (_dir, app) = test_app();
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("content-security-policy"));
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    }

    #[tokio::test]
    async fn given_not_running_when_stop_then_contract_message_and_200() {
        let (_dir, app) = test_app();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/stop/STEP3")
                    .header("x-worker-token", "test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["status"], "error");
        assert_eq!(value["message"], "Step 'STEP3' is not running");
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
    async fn csv_routes_expose_the_ingestion_contract() {
        let (_dir, app) = test_app();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/csv_downloads_status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let downloads: Value = serde_json::from_slice(&body).unwrap();
        assert!(downloads.as_array().unwrap().is_empty());

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/csv_monitor_status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let monitor: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(monitor["data_source"], "webhook");
        assert_eq!(monitor["monitor_interval"], 15);
        assert_eq!(monitor["csv_monitor"]["status"], "stopped");
        assert!(monitor["failed_urls"].as_array().unwrap().is_empty());
        assert_eq!(monitor["webhook"]["available"], false);
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

    #[tokio::test]
    async fn a_second_step_is_rejected_while_one_is_running() {
        let (_dir, app) =
            test_app_with_script("#!/bin/sh\necho 'TOTAL_JSON_TO_REDUCE: 1'\nexec sleep 5\n");
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

        for _ in 0..100 {
            let status = step_status_value(&app, "STEP5").await;
            if status["status"] == "running" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/run/STEP6")
                    .header("x-worker-token", "test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert!(value["message"].as_str().unwrap().contains("en cours"));

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/stop/STEP5")
                    .header("x-worker-token", "test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn given_stopping_step_when_immediate_run_then_conflict() {
        let (_dir, app) = test_app_with_script("#!/bin/sh\ntrap '' TERM\nsleep 30\n");
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

        for _ in 0..100 {
            if step_status_value(&app, "STEP5").await["status"] == "running" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/stop/STEP5")
                    .header("x-worker-token", "test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let status = step_status_value(&app, "STEP5").await;
        assert_eq!(status["status"], "stopping");
        assert_eq!(status["is_stopping"], true);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/run/STEP6")
                    .header("x-worker-token", "test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/run_custom_sequence")
                    .header("x-worker-token", "test-token")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"steps":["STEP6"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn stopping_a_sequence_does_not_start_the_next_step() {
        let (_dir, app) =
            test_app_with_script("#!/bin/sh\necho 'TOTAL_JSON_TO_REDUCE: 1'\nexec sleep 5\n");
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/run_custom_sequence")
                    .header("x-worker-token", "test-token")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"steps":["STEP5","STEP6"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let mut current = String::new();
        for _ in 0..100 {
            let body = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/sequence/status")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes();
            let value: Value = serde_json::from_slice(&body).unwrap();
            current = value["current_step"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            if current == "STEP5" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(current, "STEP5");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/sequence/stop")
                    .header("x-worker-token", "test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/sequence/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let value: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["is_running"], false);
        assert_eq!(value["last_outcome"]["status"], "error");
        assert!(value["last_outcome"]["message"]
            .as_str()
            .unwrap()
            .contains("manually stopped"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/run_custom_sequence")
                    .header("x-worker-token", "test-token")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"steps":["STEP6"]}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);

        let mut status = step_status_value(&app, "STEP5").await;
        for _ in 0..100 {
            if status["status"] == "stopped" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            status = step_status_value(&app, "STEP5").await;
        }
        assert_eq!(status["status"], "stopped");
        assert_eq!(step_status_value(&app, "STEP6").await["status"], "idle");
    }

    async fn step_status_value(app: &Router, step: &str) -> Value {
        let body = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/status/{step}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        serde_json::from_slice(&body).unwrap()
    }
}
