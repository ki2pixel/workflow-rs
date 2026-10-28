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
use crate::template;
use anyhow::{bail, Context, Result};
use chrono::Local;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
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
    /// Répertoire du journal STEP2. `None` désactive le fichier (tests).
    pub logs_dir: Option<PathBuf>,
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
    /// `Some(prefix)` ignore les vidéos du template direct.
    pub template_prefix: Option<String>,
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
            logs_dir: Some(config.paths.resolve().logs.join("step2")),
            skip_preserved_mov: config.steps.step1.skip_preserved_mov,
            preserved_mov_names,
            template_prefix: config
                .steps
                .step8
                .post
                .direct
                .then(|| config.steps.step8.post.template_prefix.clone()),
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
    pub skipped_template: usize,
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

struct Step2Logger {
    file: Option<Mutex<BufWriter<File>>>,
}

impl Step2Logger {
    fn new(dir: Option<&Path>) -> Result<Self> {
        let file = if let Some(dir) = dir {
            fs::create_dir_all(dir)
                .with_context(|| format!("création du journal STEP2: {}", dir.display()))?;
            let name = format!(
                "transnet_pytorch_{}.log",
                Local::now().format("%Y%m%d_%H%M%S")
            );
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(&name))
                .with_context(|| format!("ouverture du journal STEP2: {name}"))?;
            Some(Mutex::new(BufWriter::new(file)))
        } else {
            None
        };
        Ok(Self { file })
    }

    fn line(&self, args: fmt::Arguments<'_>) {
        println!("{args}");
        let _ = std::io::stdout().flush();
        if let Some(file) = &self.file {
            if let Ok(mut writer) = file.lock() {
                let stamp = Local::now().format("%Y-%m-%d %H:%M:%S");
                let _ = writeln!(writer, "{stamp} - INFO - {args}");
                let _ = writer.flush();
            }
        }
    }
}

/// Exécute STEP2 sur `options.input_dir`.
pub fn run_step2(options: &Step2Options) -> Result<Step2Report> {
    validate_options(options)?;
    let logger = Arc::new(Step2Logger::new(options.logs_dir.as_deref())?);
    logger.line(format_args!(
        "CONFIG_EFFECTIVE: threshold={}, window={}, stride={}, padding={}, ffmpeg_threads={}, batch_size={}, use_tf32={}",
        options.threshold,
        options.window,
        options.stride,
        options.padding,
        options.ffmpeg_threads,
        options.batch_size,
        options.use_tf32
    ));

    let mut total_videos = 0;
    let mut skipped_existing = 0;
    let mut skipped_preserved = 0;
    let mut skipped_template = 0;
    let mut videos = Vec::new();
    for video in discover_videos(options)? {
        if let Some(prefix) = options.template_prefix.as_deref() {
            if template::matches_name(&video, prefix) {
                match template::probe_direct(&options.ffprobe, &video, prefix) {
                    Ok(Some(_)) => {
                        logger.line(format_args!("SKIP_TEMPLATE: {}", video.display()));
                        skipped_template += 1;
                        continue;
                    }
                    Ok(None) => {}
                    Err(error) => logger.line(format_args!(
                        "WARN: sonde template impossible pour {}: {error:#}",
                        video.display()
                    )),
                }
            }
        }
        let csv = video.with_extension("csv");
        if csv.exists() && !options.force {
            logger.line(format_args!("SKIP_EXISTING: {}", video.display()));
            skipped_existing += 1;
            continue;
        }
        if is_preserved_media(&video, options) {
            logger.line(format_args!("SKIP_PRESERVED: {}", video.display()));
            skipped_preserved += 1;
            continue;
        }
        total_videos += 1;
        videos.push(video);
    }
    logger.line(format_args!("TOTAL_VIDEOS_TO_PROCESS: {total_videos}"));
    let mut report = Step2Report {
        total_videos,
        processed: 0,
        skipped_existing,
        skipped_preserved,
        skipped_template,
        failures: Vec::new(),
    };
    if videos.is_empty() {
        logger.line(format_args!(
            "Analyse terminée: 0 réussie(s), 0 échec(s), {} ignorée(s).",
            skipped_existing + skipped_preserved + skipped_template
        ));
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
    let engine = match TransNetEngine::load(&engine_config) {
        Ok(engine) => Arc::new(Mutex::new(engine)),
        Err(error) => {
            logger.line(format_args!("Échec: initialisation TransNetV2: {error:#}"));
            return Err(error);
        }
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("création du runtime Tokio STEP2")?;

    for (index, video) in videos.iter().enumerate() {
        let csv = video.with_extension("csv");
        check_fps_conformance(video, options, &logger);
        logger.line(format_args!(
            "PROCESSING_VIDEO: {}/{}: {}",
            index + 1,
            videos.len(),
            video.file_name().unwrap_or_default().to_string_lossy()
        ));
        match runtime.block_on(process_video(
            video,
            &csv,
            Arc::clone(&engine),
            options,
            Arc::clone(&logger),
        )) {
            Ok(result) => {
                logger.line(format_args!(
                    "Succès: {} créé avec {} scènes ({} trames, {:.2} s)",
                    csv.file_name().unwrap_or_default().to_string_lossy(),
                    result.scenes,
                    result.frames,
                    result.elapsed.as_secs_f64()
                ));
                report.processed += 1;
            }
            Err(error) => {
                let message = format!("{error:#}");
                logger.line(format_args!("Échec: {}: {message}", video.display()));
                report.failures.push(Step2Failure {
                    video: video.clone(),
                    error: message,
                });
            }
        }
    }

    logger.line(format_args!(
        "Analyse terminée: {} réussie(s), {} échec(s), {} ignorée(s).",
        report.processed,
        report.failures.len(),
        report.skipped_existing + report.skipped_preserved + report.skipped_template
    ));
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
    logger: Arc<Step2Logger>,
) -> Result<VideoResult> {
    let started = std::time::Instant::now();
    let mut child = ffmpeg_command(video, options)
        .spawn()
        .with_context(|| format!("lancement de ffmpeg pour {}", video.display()))?;
    let _child_guard = crate::process::ChildGuard::register(child.id().unwrap_or(0));
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
                        &logger,
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
    logger.line(format_args!(
        "INTERNAL_PROGRESS: {total_frames}/{total_frames} frames (100%) - {video_name}"
    ));
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

fn log_progress(
    read_frames: u64,
    windows: u64,
    stride: usize,
    video_name: &str,
    logger: &Step2Logger,
) {
    let processed = (windows * stride as u64).min(read_frames);
    let percent = processed
        .saturating_mul(100)
        .checked_div(read_frames)
        .unwrap_or(0);
    logger.line(format_args!(
        "INTERNAL_PROGRESS: {processed}/{read_frames} frames ({percent}%) - {video_name}"
    ));
}

const FPS_TOLERANCE: f64 = 0.1;

fn check_fps_conformance(video: &Path, options: &Step2Options, logger: &Step2Logger) {
    let name = video
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    match probe_frame_rate(video, &options.ffprobe) {
        Ok(Some(fps)) if (fps - FPS_TARGET).abs() > FPS_TOLERANCE => {
            logger.line(format_args!(
                "WARNING: Fichier non normalisé détecté: {name} ({fps:.2} fps au lieu de {FPS_TARGET:.2}). Risque de désynchronisation frame/timecode en aval — lancer `extract_archives.py --normalize-only`."
            ));
        }
        Ok(_) => {}
        Err(error) => {
            logger.line(format_args!(
                "WARNING: Framerate indéterminable pour {name}: {error}"
            ));
        }
    }
}

fn probe_frame_rate(video: &Path, ffprobe: &Path) -> Result<Option<f64>> {
    let output = std::process::Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=avg_frame_rate,r_frame_rate,nb_frames,duration",
            "-of",
            "json",
        ])
        .arg(video)
        .output()
        .with_context(|| format!("ffprobe pour {}", video.display()))?;
    if !output.status.success() {
        bail!(
            "ffprobe a échoué: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("lecture du JSON ffprobe")?;
    let stream = payload
        .get("streams")
        .and_then(|value| value.as_array())
        .and_then(|streams| streams.first())
        .context("aucun flux vidéo ffprobe")?;
    let nb_frames = stream
        .get("nb_frames")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0);
    let duration = stream
        .get("duration")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0);
    let from_counts = nb_frames
        .zip(duration)
        .map(|(frames, duration)| frames / duration);
    let avg = stream
        .get("avg_frame_rate")
        .and_then(|value| value.as_str())
        .and_then(parse_frame_rate);
    let nominal = stream
        .get("r_frame_rate")
        .and_then(|value| value.as_str())
        .and_then(parse_frame_rate);
    for candidate in [from_counts, avg, nominal].into_iter().flatten() {
        if candidate > 0.0 && candidate <= 240.0 {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn parse_frame_rate(raw: &str) -> Option<f64> {
    let (numerator, denominator) = raw.split_once('/')?;
    let numerator: f64 = numerator.trim().parse().ok()?;
    let denominator: f64 = denominator.trim().parse().ok()?;
    if numerator <= 0.0 || denominator <= 0.0 {
        return None;
    }
    Some(numerator / denominator)
}

/// Détection conservatrice des `.mov` préservés (alpha ou nom configuré).
pub fn is_preserved_media(path: &Path, options: &Step2Options) -> bool {
    is_preserved_media_with(
        path,
        options.skip_preserved_mov,
        &options.preserved_mov_names,
        &options.ffprobe,
    )
}

/// Variante partagée par les étapes qui filtrent aussi les médias préservés.
pub fn is_preserved_media_with(
    path: &Path,
    skip_preserved_mov: bool,
    preserved_mov_names: &[String],
    ffprobe: &Path,
) -> bool {
    crate::media::is_preserved_media_with(path, skip_preserved_mov, preserved_mov_names, ffprobe)
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
            logs_dir: None,
            skip_preserved_mov: true,
            preserved_mov_names: vec!["m6+.mov".into()],
            template_prefix: None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn template_videos_are_skipped_before_engine_load() {
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("M6 - SDM - clip.mp4");
        std::fs::write(&video, b"video").unwrap();
        let ffprobe = dir.path().join("ffprobe");
        crate::test_support::write_executable(
            &ffprobe,
            r#"#!/bin/sh
printf '%s' '{"streams":[{"codec_type":"video","codec_name":"h264","width":1920,"height":1080,"sample_aspect_ratio":"1:1","avg_frame_rate":"25/1","r_frame_rate":"25/1","nb_frames":"531","duration":"21.240000"}]}'
"#,
        );
        let model = dir.path().join("model.onnx");
        std::fs::write(&model, b"model").unwrap();
        let mut options = base_options(dir.path());
        options.model = model;
        options.ffprobe = ffprobe;
        options.template_prefix = Some("M6 - SDM - ".into());
        let report = run_step2(&options).unwrap();
        assert_eq!(report.total_videos, 0);
        assert_eq!(report.skipped_template, 1);
        assert_eq!(report.processed, 0);
        assert!(!video.with_extension("csv").exists());
    }

    #[test]
    fn frame_rate_parser_handles_rationals_and_invalid_values() {
        assert_eq!(parse_frame_rate("25/1"), Some(25.0));
        assert!((parse_frame_rate("30000/1001").unwrap() - 29.970_029).abs() < 1e-5);
        assert_eq!(parse_frame_rate("0/0"), None);
        assert_eq!(parse_frame_rate("25"), None);
        assert_eq!(parse_frame_rate("x/1"), None);
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
