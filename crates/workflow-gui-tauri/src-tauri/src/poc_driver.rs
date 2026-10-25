use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{WebviewWindow, Wry};

pub fn spawn(
    window: WebviewWindow<Wry>,
    js: String,
    out_dir: PathBuf,
    ready_file: Option<PathBuf>,
    expected_sha: String,
) {
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(500));
        if let Err(error) = window.eval(js) {
            write_json(
                &out_dir.join("driver-error.json"),
                &json!({"stage": "eval", "error": error.to_string()}),
            );
            return;
        }

        let deadline = Instant::now() + Duration::from_secs(30);
        let mut last_url = String::new();
        while Instant::now() < deadline {
            match window.url() {
                Ok(url) => {
                    let current = url.to_string();
                    if current != last_url {
                        last_url = current.clone();
                        append_line(&out_dir.join("driver-urls.log"), &current);
                        if let Some(fragment) = url.fragment() {
                            if let Some(raw) = fragment.strip_prefix("poc=") {
                                let decoded =
                                    percent_encoding::percent_decode_str(raw).decode_utf8_lossy();
                                match serde_json::from_str::<Value>(&decoded) {
                                    Ok(value) => {
                                        let actual = value
                                            .get("token_sha256")
                                            .and_then(Value::as_str)
                                            .unwrap_or("");
                                        let status = if actual == expected_sha.as_str() {
                                            "ok"
                                        } else {
                                            "mismatch"
                                        };
                                        write_json(
                                            &out_dir.join("token-check.json"),
                                            &json!({
                                                "status": status,
                                                "expected_sha256": expected_sha.clone(),
                                                "actual_sha256": actual,
                                            }),
                                        );
                                        write_json(&out_dir.join("audit.json"), &value);
                                        if let Some(path) = &ready_file {
                                            let _ = fs::write(path, format!("{current}\n"));
                                        }
                                        let _ = window.eval(
                                            "history.replaceState(null, '', location.pathname + location.search)",
                                        );
                                        tracing::info!(status, "audit A-Strict capturé");
                                        return;
                                    }
                                    Err(error) => {
                                        write_json(
                                            &out_dir.join("driver-error.json"),
                                            &json!({
                                                "stage": "json",
                                                "error": error.to_string(),
                                                "raw": decoded,
                                            }),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(error) => {
                    write_json(
                        &out_dir.join("driver-error.json"),
                        &json!({"stage": "url", "error": error.to_string()}),
                    );
                }
            }
            thread::sleep(Duration::from_millis(100));
        }

        write_json(
            &out_dir.join("driver-error.json"),
            &json!({"stage": "timeout", "error": "audit non reçu dans les 30 s"}),
        );
    });
}

fn write_json(path: &Path, value: &Value) {
    let tmp = path.with_extension("tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(value) {
        if fs::write(&tmp, bytes).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}

fn append_line(path: &Path, line: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}
