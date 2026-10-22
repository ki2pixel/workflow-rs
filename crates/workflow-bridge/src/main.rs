mod cuts;

use anyhow::{Context, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "workflow-bridge",
    version,
    about = "Pont JSON pour le panneau After Effects"
)]
struct Args {
    #[arg(long = "manifest_path")]
    manifest_path: PathBuf,
    #[arg(long = "output_path")]
    output_path: PathBuf,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("[PY] Error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    let payload = match fs::read_to_string(&args.manifest_path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(manifest) => dispatch(&manifest),
            Err(error) => json!({"error": error.to_string()}),
        },
        Err(error) => json!({"error": error.to_string()}),
    };

    write_json_atomically(&args.output_path, &payload)
        .with_context(|| format!("écriture de {}", args.output_path.display()))?;
    Ok(())
}

fn dispatch(manifest: &Value) -> Value {
    let result = match manifest.get("mode").and_then(Value::as_str) {
        Some("cuts") => cuts::analyze(manifest).map_err(|error| error.to_string()),
        Some(mode) => Err(format!("mode inconnu: {mode}")),
        None if manifest.get("layers").is_some() => {
            workflow_steps::step6::analyze_manifest_value(manifest)
                .map_err(|error| error.to_string())
        }
        None => Err("manifest.mode absent".into()),
    };
    match result {
        Ok(payload) => payload,
        Err(error) => json!({"error": error}),
    }
}

fn write_json_atomically(path: &Path, payload: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    let result = (|| -> Result<()> {
        let file = File::create(&temp_path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, payload)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        fs::rename(&temp_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}
