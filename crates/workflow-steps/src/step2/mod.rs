//! STEP2 : détection de scènes TransNetV2 via ONNX Runtime.
//!
//! Le contrat de sortie reste celui de l'application Python : un CSV par
//! vidéo, nommé `<stem>.csv`, avec des timecodes à 25 fps et des segments
//! inclusifs. L'inférence est faite par lots de 8 fenêtres ; le flux FFmpeg est
//! lu en continu et le padding global est reconstruit sans charger la vidéo
//! entière en mémoire.

pub mod engine;
pub mod windows;

use crate::step2::engine::{EngineConfig, ExecutionDevice, TransNetEngine};
use crate::step2::windows::{
    predictions_to_scenes, write_scenes_csv, WindowStream, FPS_TARGET, FRAME_BYTES, FRAME_HEIGHT,
    FRAME_WIDTH,
};
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{mpsc, Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::mpsc as async_mpsc;
use walkdir::WalkDir;
use workflow_core::{Config, HardwareLock};

#[derive(Debug, Clone)]
pub struct Step2Options {
    pub input_dir: PathBuf,
    pub force: bool,
    pub model: PathBuf,
    pub device: ExecutionDevice,
    pub onnxruntime: Option<PathBuf>,
    pub cuda_lib_dirs: Vec<PathBuf>,
    pub auto_cuda_libs: bool,
    /// Verrou matériel à prendre avant le chargement du modèle. `None` est
    /// réservé aux tests unitaires sans GPU.
    pub lock_path: Option<PathBuf>,
    pub batch_size: usize,
    pub window: usize,
    pub stride: usize,
    pub padding: usize,
    pub threshold: f32,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg_threads: u32,
    pub cudnn_algo_search: bool,
    pub use_tf32: bool,
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
}

impl Step2Options {
    pub fn from_config(config: &Config, input_dir: PathBuf) -> Result<Self> {
        let step = &config.steps.step2;
        let model = if step.model.is_absolute() {
            step.model.clone()
        } else {
            config.paths.models.join(&step.model)
        };
        let preserved_mov_names = if config.steps.step1.preserved_mov_names.is_empty() {
            vec!["m6+.mov".to_string()]
        } else {
            config.steps.step1.preserved_mov_names.clone()
        };
        Ok(Self {
            input_dir,
            force: false,
            model,
            device: ExecutionDevice::parse(&step.device)?,
            onnxruntime: step
                .onnxruntime
                .clone()
                .filter(|path| !path.as_os_str().is_empty()),
            cuda_lib_dirs: step.cuda_lib_dirs.clone(),
            auto_cuda_libs: step.auto_cuda_libs,
            lock_path: Some(config.hardware.lock_path.clone()),
            batch_size: step.batch_size as usize,
            window: step.window as usize,
            stride: step.stride as usize,
            padding: step.padding as usize,
            threshold: step.threshold as f32,
            ffmpeg: step.ffmpeg.clone(),
            ffprobe: step.ffprobe.clone(),
            ffmpeg_threads: step.ffmpeg_threads,
            cudnn_algo_search: step.cudnn_algo_search,
            use_tf32: step.use_tf32,
            skip_preserved_mov: config.steps.step1.skip_preserved_mov,
            preserved_mov_names,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Step2Failure {
    pub video: PathBuf,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct Step2Report {
    pub total_videos: usize,
    pub processed: usize,
    pub skipped_existing: usize,
    pub skipped_preserved: usize,
    pub failures: Vec<Step2Failure>,
}

impl Step2Report {
    pub fn success(&self) -> bool {
        self.failures.is_empty()
    }
}

struct VideoResult {
    scenes: usize,
    frames: u64,
    elapsed: std::time::Duration,
}

/// Exécute STEP2 sur `options.input_dir`.
pub fn run_step2(options: &Step2Options) -> Result<Step2Report> {
    validate_options(options)?;

    let mut total_videos = 0;
    let mut skipped_existing = 0;
    let mut skipped_preserved = 0;
    let mut videos = Vec::new();
    for video in discover_videos(options)? {
        let csv = video.with_extension("csv");
        if csv.exists() && !options.force {
            println!("SKIP_EXISTING: {}", video.display());
            skipped_existing += 1;
            continue;
        }
        if is_preserved_media(&video, options) {
            println!("SKIP_PRESERVED: {}", video.display());
            skipped_preserved += 1;
            continue;
        }
        total_videos += 1;
        videos.push(video);
    }
    println!("TOTAL_VIDEOS_TO_PROCESS: {total_videos}");
    let mut report = Step2Report {
        total_videos,
        processed: 0,
        skipped_existing,
        skipped_preserved,
        failures: Vec::new(),
    };
    if videos.is_empty() {
        println!(
            "Analyse terminée: 0 réussie(s), 0 échec(s), {} ignorée(s).",
            skipped_existing + skipped_preserved
        );
        return Ok(report);
    }

    let _lock = options
        .lock_path
        .as_ref()
        .map(|path| {
            HardwareLock::acquire(path, false).with_context(|| {
                format!(
                    "verrou matériel STEP2 indisponible: {} (une autre étape GPU/Coral tourne peut-être)",
                    path.display()
                )
            })
        })
        .transpose()?;

    let engine_config = EngineConfig {
        model: options.model.clone(),
        device: options.device,
        onnxruntime: options.onnxruntime.clone(),
        cuda_lib_dirs: options.cuda_lib_dirs.clone(),
        auto_cuda_libs: options.auto_cuda_libs,
        batch_size: options.batch_size,
        window: options.window,
        padding: options.padding,
        cudnn_algo_search: options.cudnn_algo_search,
        use_tf32: options.use_tf32,
    };
    let engine = Arc::new(Mutex::new(TransNetEngine::load(&engine_config)?));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("création du runtime Tokio STEP2")?;

    for (index, video) in videos.iter().enumerate() {
        let csv = video.with_extension("csv");
        println!(
            "PROCESSING_VIDEO: {}/{}: {}",
            index + 1,
            videos.len(),
            video.file_name().unwrap_or_default().to_string_lossy()
        );
        match runtime.block_on(process_video(video, &csv, Arc::clone(&engine), options)) {
            Ok(result) => {
                println!(
                    "Succès: {} créé avec {} scènes ({} trames, {:.2} s)",
                    csv.file_name().unwrap_or_default().to_string_lossy(),
                    result.scenes,
                    result.frames,
                    result.elapsed.as_secs_f64()
                );
                report.processed += 1;
            }
            Err(error) => {
                let message = format!("{error:#}");
                println!("Échec: {}: {message}", video.display());
                report.failures.push(Step2Failure {
                    video: video.clone(),
                    error: message,
                });
            }
        }
    }

    println!(
        "Analyse terminée: {} réussie(s), {} échec(s), {} ignorée(s).",
        report.processed,
        report.failures.len(),
        report.skipped_existing + report.skipped_preserved
    );
    Ok(report)
}

fn validate_options(options: &Step2Options) -> Result<()> {
    if !options.input_dir.is_dir() {
        bail!(
            "répertoire d'entrée introuvable: {}",
            options.input_dir.display()
        );
    }
    if !options.model.is_file() {
        bail!("modèle TransNetV2 introuvable: {}", options.model.display());
    }
    if options.batch_size == 0 {
        bail!("batch_size doit être > 0");
    }
    if options.window == 0 || options.stride == 0 || options.padding * 2 >= options.window {
        bail!(
            "fenêtrage invalide: window={} stride={} padding={}",
            options.window,
            options.stride,
            options.padding
        );
    }
    if !(options.threshold >= 0.0 && options.threshold <= 1.0) {
        bail!("threshold doit être dans [0, 1]");
    }
    Ok(())
}

fn discover_videos(options: &Step2Options) -> Result<Vec<PathBuf>> {
    const EXTENSIONS: [&str; 5] = ["mp4", "avi", "mov", "mkv", "webm"];
    let mut videos = BTreeSet::new();
    for entry in WalkDir::new(&options.input_dir).follow_links(false) {
        let entry =
            entry.with_context(|| format!("parcours de {}", options.input_dir.display()))?;
        if !entry.path().is_file() {
            continue;
        }
        let path = entry.path();
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase());
        if extension
            .as_deref()
            .is_some_and(|value| EXTENSIONS.contains(&value))
        {
            videos.insert(path.to_path_buf());
        }
    }
    Ok(videos.into_iter().collect())
}

async fn process_video(
    video: &Path,
    csv: &Path,
    engine: Arc<Mutex<TransNetEngine>>,
    options: &Step2Options,
) -> Result<VideoResult> {
    let started = std::time::Instant::now();
    let mut child = ffmpeg_command(video, options)
        .spawn()
        .with_context(|| format!("lancement de ffmpeg pour {}", video.display()))?;
    let mut stdout = child.stdout.take().context("stdout ffmpeg indisponible")?;
    let mut stderr = child.stderr.take().context("stderr ffmpeg indisponible")?;
    let stderr_task = tokio::spawn(async move {
        let mut data = Vec::new();
        let _ = stderr.read_to_end(&mut data).await;
        data
    });

    let (window_tx, mut window_rx) = async_mpsc::channel::<Vec<Vec<u8>>>(2);
    let (result_tx, result_rx) = mpsc::channel::<Result<Vec<f32>, String>>();
    let inference = std::thread::spawn(move || {
        while let Some(batch) = window_rx.blocking_recv() {
            let result = engine
                .lock()
                .map_err(|_| "verrou du moteur TransNetV2 empoisonné".to_string())
                .and_then(|mut session| {
                    session.infer(&batch).map_err(|error| format!("{error:#}"))
                });
            let failed = result.is_err();
            if result_tx.send(result).is_err() {
                break;
            }
            if failed {
                break;
            }
        }
    });

    let mut stream = WindowStream::new(options.window, options.stride, options.padding);
    let mut batch: Vec<Vec<u8>> = Vec::with_capacity(options.batch_size);
    let mut windows_emitted: u64 = 0;
    let video_name = video
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut read_buffer = vec![0_u8; FRAME_BYTES * 64];
    let mut filled = 0;
    let mut send_error: Option<anyhow::Error> = None;

    loop {
        let read = stdout
            .read(&mut read_buffer[filled..])
            .await
            .context("lecture du flux ffmpeg")?;
        if read == 0 {
            break;
        }
        filled += read;
        let mut offset = 0;
        while filled - offset >= FRAME_BYTES {
            let frame = read_buffer[offset..offset + FRAME_BYTES].to_vec();
            offset += FRAME_BYTES;
            stream.push_frame(frame);
            for window in stream.drain(false) {
                windows_emitted += 1;
                batch.push(window);
                if batch.len() == options.batch_size {
                    if let Err(error) = window_tx.send(std::mem::take(&mut batch)).await {
                        send_error = Some(anyhow::anyhow!(
                            "le moteur d'inférence a arrêté la réception: {error}"
                        ));
                        break;
                    }
                    log_progress(
                        stream.raw_frames(),
                        windows_emitted,
                        options.stride,
                        &video_name,
                    );
                }
            }
            if send_error.is_some() {
                break;
            }
        }
        if let Some(error) = send_error {
            return Err(error);
        }
        if offset > 0 {
            read_buffer.copy_within(offset..filled, 0);
            filled -= offset;
        }
    }
    if filled != 0 {
        bail!(
            "flux ffmpeg tronqué: {} octets résiduels, multiple attendu {}",
            filled,
            FRAME_BYTES
        );
    }

    for window in stream.drain(true) {
        batch.push(window);
        if batch.len() == options.batch_size {
            if let Err(error) = window_tx.send(std::mem::take(&mut batch)).await {
                return Err(anyhow::anyhow!(
                    "le moteur d'inférence a arrêté la réception: {error}"
                ));
            }
        }
    }
    if !batch.is_empty() {
        if let Err(error) = window_tx.send(batch).await {
            return Err(anyhow::anyhow!(
                "le moteur d'inférence a arrêté la réception: {error}"
            ));
        }
    }
    drop(window_tx);

    let status = child
        .wait()
        .await
        .with_context(|| format!("attente de ffmpeg pour {}", video.display()))?;
    let stderr = stderr_task
        .await
        .context("lecture des erreurs ffmpeg")?
        .to_vec();
    if !status.success() {
        bail!(
            "ffmpeg a échoué pour {} (code {}): {}",
            video.display(),
            status.code().unwrap_or(-1),
            String::from_utf8_lossy(&stderr).trim()
        );
    }

    inference
        .join()
        .map_err(|_| anyhow::anyhow!("le thread d'inférence TransNetV2 a paniqué"))?;
    let mut predictions = Vec::new();
    for result in result_rx {
        predictions.extend(result.map_err(|error| anyhow::anyhow!(error))?);
    }
    let total_frames = stream.raw_frames();
    println!("INTERNAL_PROGRESS: {total_frames}/{total_frames} frames (100%) - {video_name}");
    let _ = std::io::stdout().flush();
    let total_frames_usize =
        usize::try_from(total_frames).context("nombre de trames trop grand")?;
    if predictions.len() < total_frames_usize {
        bail!(
            "prédictions TransNetV2 incomplètes: {} pour {} trames",
            predictions.len(),
            total_frames
        );
    }
    predictions.truncate(total_frames_usize);
    let scenes = predictions_to_scenes(&predictions, options.threshold);
    write_scenes_csv(csv, &scenes, FPS_TARGET)?;
    Ok(VideoResult {
        scenes: scenes.len(),
        frames: total_frames,
        elapsed: started.elapsed(),
    })
}

fn ffmpeg_command(video: &Path, options: &Step2Options) -> Command {
    let mut command = Command::new(&options.ffmpeg);
    command
        .arg("-nostdin")
        .arg("-v")
        .arg("error")
        .arg("-threads")
        .arg(options.ffmpeg_threads.to_string())
        .arg("-i")
        .arg(video)
        .arg("-vf")
        .arg(format!(
            "fps={FPS_TARGET},scale={FRAME_WIDTH}:{FRAME_HEIGHT}:flags=bilinear"
        ))
        .arg("-pix_fmt")
        .arg("rgb24")
        .arg("-f")
        .arg("rawvideo")
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

fn log_progress(read_frames: u64, windows: u64, stride: usize, video_name: &str) {
    let processed = (windows * stride as u64).min(read_frames);
    let percent = processed
        .saturating_mul(100)
        .checked_div(read_frames)
        .unwrap_or(0);
    println!("INTERNAL_PROGRESS: {processed}/{read_frames} frames ({percent}%) - {video_name}");
    let _ = std::io::stdout().flush();
}

/// Détection conservatrice des `.mov` préservés (alpha ou nom configuré).
pub fn is_preserved_media(path: &Path, options: &Step2Options) -> bool {
    if !options.skip_preserved_mov {
        return false;
    }
    if !path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("mov"))
    {
        return false;
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if options
        .preserved_mov_names
        .iter()
        .any(|preserved| preserved.eq_ignore_ascii_case(&name))
    {
        return true;
    }
    probe_mov_alpha(path, &options.ffprobe).unwrap_or(true)
}

fn probe_mov_alpha(path: &Path, ffprobe: &Path) -> Option<bool> {
    let output = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v",
            "-show_entries",
            "stream=codec_name,codec_tag_string,pix_fmt",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let streams = payload.get("streams")?.as_array()?;
    if streams.is_empty() {
        return None;
    }
    const ALPHA_PREFIXES: [&str; 9] = [
        "yuva", "gbrap", "ya8", "ya16", "ayuv", "rgba", "bgra", "argb", "abgr",
    ];
    const ALPHA_TAGS: [&str; 4] = ["ap4h", "ap4x", "rle", "png"];
    let mut missing_pix_fmt = false;
    for stream in streams {
        if let Some(pix_fmt) = stream.get("pix_fmt").and_then(|value| value.as_str()) {
            let normalized = pix_fmt.to_ascii_lowercase();
            if ALPHA_PREFIXES
                .iter()
                .any(|prefix| normalized.starts_with(prefix))
            {
                return Some(true);
            }
        } else {
            missing_pix_fmt = true;
        }
    }
    if !missing_pix_fmt {
        return Some(false);
    }
    for stream in streams {
        if stream.get("pix_fmt").is_some() {
            continue;
        }
        let tag = stream
            .get("codec_tag_string")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if ALPHA_TAGS.contains(&tag.as_str()) {
            return Some(true);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn base_options(dir: &Path) -> Step2Options {
        Step2Options {
            input_dir: dir.to_path_buf(),
            force: false,
            model: dir.join("model.onnx"),
            device: ExecutionDevice::Cpu,
            onnxruntime: None,
            cuda_lib_dirs: Vec::new(),
            auto_cuda_libs: false,
            lock_path: None,
            batch_size: 8,
            window: 100,
            stride: 50,
            padding: 25,
            threshold: 0.5,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            ffmpeg_threads: 0,
            cudnn_algo_search: true,
            use_tf32: false,
            skip_preserved_mov: true,
            preserved_mov_names: vec!["m6+.mov".into()],
        }
    }

    #[test]
    fn preserved_mov_name_is_skipped_without_probe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("M6+.MOV");
        fs::write(&path, b"not a movie").unwrap();
        let options = base_options(dir.path());
        assert!(is_preserved_media(&path, &options));
    }

    #[test]
    fn non_mov_is_never_preserved_by_step2() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.mp4");
        fs::write(&path, b"not a movie").unwrap();
        let options = base_options(dir.path());
        assert!(!is_preserved_media(&path, &options));
    }

    #[test]
    fn discovery_is_recursive_and_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("b.MP4"), b"").unwrap();
        fs::write(dir.path().join("nested/a.mkv"), b"").unwrap();
        fs::write(dir.path().join("notes.txt"), b"").unwrap();
        let options = base_options(dir.path());
        let videos = discover_videos(&options).unwrap();
        assert_eq!(videos.len(), 2);
        assert!(videos[0].ends_with("b.MP4"));
        assert!(videos[1].ends_with("nested/a.mkv"));
    }

    #[test]
    fn timecode_and_scene_helpers_are_reused() {
        assert_eq!(windows::frame_timecode(54, FPS_TARGET), "00:00:02.160");
        assert_eq!(
            predictions_to_scenes(&[0.1, 0.9, 0.1], 0.5),
            vec![(0, 1), (2, 2)]
        );
    }
}
