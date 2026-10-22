use anyhow::{Context, Result};
use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use super::tracking::OutputObject;

#[derive(Debug, Clone, Serialize)]
pub struct Resolution {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Metadata {
    pub video_path: String,
    pub original_fps: f64,
    pub resolution: Resolution,
    pub total_frames_processed: Option<u64>,
    pub engine: String,
    pub tracking_method: String,
}

#[derive(Serialize)]
struct FrameRef<'a> {
    frame: u64,
    tracked_objects: &'a [OutputObject],
}

/// Écrit le JSON STEP4 au fil de l'eau, sans matérialiser les frames.
pub struct StreamingJson {
    writer: BufWriter<File>,
    temp: TempPath,
    first: bool,
}

struct TempPath {
    path: PathBuf,
    published: bool,
}

impl Drop for TempPath {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.path);
        }
    }
}

impl StreamingJson {
    pub fn create(path: &Path, metadata: &Metadata) -> Result<Self> {
        let temp = TempPath {
            path: temp_path(path),
            published: false,
        };
        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temp.path)
            .with_context(|| format!("création de {}", temp.path.display()))?;
        let mut writer = BufWriter::with_capacity(256 * 1024, file);
        writer.write_all(b"{\n  \"metadata\": ")?;
        serde_json::to_writer_pretty(&mut writer, metadata).context("sérialisation metadata")?;
        writer.write_all(b",\n  \"frames\": [\n")?;
        Ok(Self {
            writer,
            temp,
            first: true,
        })
    }

    pub fn push(&mut self, frame: u64, objects: &[OutputObject]) -> Result<()> {
        if !self.first {
            self.writer.write_all(b",\n")?;
        }
        self.first = false;
        self.writer.write_all(b"    ")?;
        serde_json::to_writer(
            &mut self.writer,
            &FrameRef {
                frame,
                tracked_objects: objects,
            },
        )
        .context("sérialisation frame STEP4")?;
        Ok(())
    }

    pub fn finish(mut self, destination: &Path) -> Result<()> {
        self.writer.write_all(b"\n  ]\n}\n")?;
        self.writer.flush()?;
        let file = self
            .writer
            .into_inner()
            .map_err(|error| error.into_error())
            .context("fermeture du JSON STEP4")?;
        file.sync_all().context("synchronisation du JSON STEP4")?;
        drop(file);
        replace_file(&self.temp.path, destination)?;
        self.temp.published = true;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.temp.path
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("tracking.json");
    path.with_file_name(format!(".{name}.tmp"))
}

fn replace_file(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    if destination.exists() {
        fs::remove_file(destination)
            .with_context(|| format!("remplacement de {}", destination.display()))?;
    }
    fs::rename(source, destination)
        .with_context(|| format!("publication de {}", destination.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_valid_streaming_json() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tracking.json");
        let metadata = Metadata {
            video_path: "video.mp4".into(),
            original_fps: 25.0,
            resolution: Resolution {
                width: 1280,
                height: 720,
            },
            total_frames_processed: Some(0),
            engine: "blazeface_tpu".into(),
            tracking_method: "kdtree_tpu".into(),
        };
        let writer = StreamingJson::create(&path, &metadata).unwrap();
        writer.finish(&path).unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(value["metadata"]["engine"], "blazeface_tpu");
        assert!(value["frames"].as_array().unwrap().is_empty());
    }

    #[test]
    fn dropping_unfinished_writer_removes_temp_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("tracking.json");
        let metadata = Metadata {
            video_path: "video.mp4".into(),
            original_fps: 25.0,
            resolution: Resolution {
                width: 1280,
                height: 720,
            },
            total_frames_processed: Some(0),
            engine: "blazeface_tpu".into(),
            tracking_method: "kdtree_tpu".into(),
        };
        let writer = StreamingJson::create(&path, &metadata).unwrap();
        let temp = writer.path().to_path_buf();
        assert!(temp.is_file());
        drop(writer);
        assert!(!temp.exists());
        assert!(!path.exists());
    }
}
