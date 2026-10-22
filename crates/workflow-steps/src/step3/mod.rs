//! STEP3 : diarisation audio YAMNet Split + ECAPA-TDNN + AHC.
//!
//! Le contrat observable est celui de
//! `workflow_scripts/step3/run_audio_diarization_tpu.py` :
//!
//! - découverte récursive des vidéos et skip de `<stem>_audio.json` existant ;
//! - extraction PCM16 16 kHz mono par FFmpeg ;
//! - VAD YAMNet Split sur l'Edge TPU, fenêtres de 0,96 s et hop 0,48 s ;
//! - embeddings ECAPA-TDNN 192D sur CPU via ONNX Runtime ;
//! - AHC average, cosinus, seuil 0,32 et réassignation des locuteurs mineurs ;
//! - JSON streamé avec `speaker_stats` et une entrée par frame à 25 fps.
//!
//! `device = "cpu"` est un chemin de test explicite, jamais un repli
//! silencieux. Le chemin nominal exige le Coral.

pub mod cluster;
pub mod ecapa;
pub mod mel;
pub mod vad;
pub mod yamnet;

use crate::step3::cluster::{ClusterConfig, DiarizationSegment};
use crate::step3::ecapa::EcapaEngine;
use crate::step3::mel::{
    EcapaFrontend, MelMatrix, YamnetFrontend, ECAPA_SEGMENT_SAMPLES, SAMPLE_RATE,
    YAMNET_SEGMENT_SAMPLES,
};
use crate::step3::vad::{hop_for_overlap, postfilter, speech_segments, window_starts};
use crate::step3::yamnet::YamnetEngine;
use anyhow::{bail, Context, Result};
use chrono::Local;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Instant;
use walkdir::WalkDir;
use workflow_core::{Config, HardwareLock};

const FPS: f64 = 25.0;
const VIDEO_EXTENSIONS: [&str; 5] = ["mp4", "avi", "mov", "mkv", "webm"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step3Device {
    Tpu,
    Cpu,
}

impl Step3Device {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "tpu" | "coral" => Ok(Self::Tpu),
            "cpu" | "xnnpack" => Ok(Self::Cpu),
            other => bail!("device STEP3 inconnu: {other} (attendu tpu ou cpu)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Step3Options {
    pub input_dir: PathBuf,
    pub force: bool,
    pub device: Step3Device,
    pub require_edgetpu: bool,
    pub yamnet_split: bool,
    pub yamnet: PathBuf,
    pub yamnet_fallback: PathBuf,
    pub mel_matrix: PathBuf,
    pub ecapa: PathBuf,
    pub tflite: PathBuf,
    pub edgetpu: PathBuf,
    pub onnxruntime: Option<PathBuf>,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg_threads: u32,
    pub cpu_threads: i32,
    pub vad_threshold: f32,
    pub hop_overlap: f64,
    pub median_filter_size: usize,
    pub hangover_sec: f64,
    pub max_speakers: usize,
    pub embedding_stride: usize,
    pub adaptive_stride: bool,
    pub adaptive_threshold: usize,
    pub clustering_threshold: f64,
    pub min_speaker_speech_duration: f64,
    pub min_cluster_embeddings: usize,
    pub ecapa_batch_size: usize,
    pub max_workers: usize,
    pub lock_path: Option<PathBuf>,
    pub logs_dir: Option<PathBuf>,
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
}

impl Step3Options {
    pub fn from_config(config: &Config, input_dir: PathBuf) -> Result<Self> {
        let step = &config.steps.step3;
        let models = &config.paths.models;
        let resolve_model = |path: &PathBuf| -> PathBuf {
            if path.is_absolute() {
                path.clone()
            } else {
                models.join(path)
            }
        };
        let preserved_mov_names = if config.steps.step1.preserved_mov_names.is_empty() {
            vec!["m6+.mov".to_string()]
        } else {
            config.steps.step1.preserved_mov_names.clone()
        };
        Ok(Self {
            input_dir,
            force: false,
            device: Step3Device::parse(&step.device)?,
            require_edgetpu: step.require_edgetpu,
            yamnet_split: step.yamnet_split,
            yamnet: resolve_model(&step.yamnet),
            yamnet_fallback: resolve_model(&step.yamnet_fallback),
            mel_matrix: resolve_model(&step.mel_matrix),
            ecapa: resolve_model(&step.ecapa),
            tflite: step.tflite.clone(),
            edgetpu: step.edgetpu.clone(),
            onnxruntime: step
                .onnxruntime
                .clone()
                .filter(|path| !path.as_os_str().is_empty()),
            ffmpeg: step.ffmpeg.clone(),
            ffprobe: step.ffprobe.clone(),
            ffmpeg_threads: step.ffmpeg_threads,
            cpu_threads: i32::try_from(step.cpu_threads).unwrap_or(i32::MAX),
            vad_threshold: step.vad_threshold as f32,
            hop_overlap: step.hop_overlap,
            median_filter_size: step.median_filter_size as usize,
            hangover_sec: step.hangover_sec,
            max_speakers: step.max_speakers as usize,
            embedding_stride: step.embedding_stride as usize,
            adaptive_stride: step.adaptive_stride,
            adaptive_threshold: step.adaptive_threshold as usize,
            clustering_threshold: step.clustering_threshold,
            min_speaker_speech_duration: step.min_speaker_speech_duration,
            min_cluster_embeddings: step.min_cluster_embeddings as usize,
            ecapa_batch_size: step.ecapa_batch_size as usize,
            max_workers: step.max_workers as usize,
            lock_path: Some(config.hardware.lock_path.clone()),
            logs_dir: Some(config.paths.resolve().logs.join("step3")),
            skip_preserved_mov: config.steps.step1.skip_preserved_mov,
            preserved_mov_names,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Step3Failure {
    pub video: PathBuf,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct Step3Report {
    pub total_videos: usize,
    pub processed: usize,
    pub skipped_existing: usize,
    pub skipped_no_audio: usize,
    pub skipped_preserved: usize,
    pub failures: Vec<Step3Failure>,
}

impl Step3Report {
    pub fn success(&self) -> bool {
        self.failures.is_empty()
    }
}

struct Step3Logger {
    file: Option<Mutex<BufWriter<File>>>,
}

impl Step3Logger {
    fn new(dir: Option<&Path>) -> Result<Self> {
        let file = if let Some(dir) = dir {
            fs::create_dir_all(dir)
                .with_context(|| format!("création du journal STEP3: {}", dir.display()))?;
            let name = format!(
                "tpu_audio_diarization_{}.log",
                Local::now().format("%Y%m%d_%H%M%S")
            );
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(&name))
                .with_context(|| format!("ouverture du journal STEP3: {name}"))?;
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

struct VideoOutcome {
    segments: usize,
    frames: i64,
    elapsed: std::time::Duration,
}

/// Exécute STEP3 sur `options.input_dir`.
pub fn run_step3(options: &Step3Options) -> Result<Step3Report> {
    validate_options(options)?;
    let logger = Step3Logger::new(options.logs_dir.as_deref())?;
    logger.line(format_args!(
        "CONFIG: vad_threshold={}, hop_overlap={}, median_filter={}, hangover_sec={}, max_speakers={}",
        options.vad_threshold,
        options.hop_overlap,
        options.median_filter_size,
        options.hangover_sec,
        options.max_speakers
    ));
    logger.line(format_args!(
        "--- Démarrage de l'analyse Audio TPU (YAMNet INT8 - Audit Camille) ---"
    ));

    let mut skipped_existing = 0usize;
    let mut skipped_preserved = 0usize;
    let mut videos = Vec::new();
    for video in discover_videos(options)? {
        if is_preserved_media(&video, options) {
            logger.line(format_args!("SKIP_PRESERVED: {}", video.display()));
            skipped_preserved += 1;
            continue;
        }
        let output = audio_output_path(&video);
        if output.exists() && !options.force {
            logger.line(format_args!("SKIP_EXISTING: {}", video.display()));
            skipped_existing += 1;
            continue;
        }
        videos.push(video);
    }
    let total_videos = videos.len();
    logger.line(format_args!("TOTAL_VIDEOS_TO_PROCESS: {total_videos}"));
    if videos.is_empty() {
        logger.line(format_args!(
            "--- Analyse terminée. 0/{total_videos} réussie(s). ---"
        ));
        return Ok(Step3Report {
            total_videos,
            processed: 0,
            skipped_existing,
            skipped_no_audio: 0,
            skipped_preserved,
            failures: Vec::new(),
        });
    }

    let _lock = options
        .lock_path
        .as_ref()
        .map(|path| {
            HardwareLock::acquire(path, false).with_context(|| {
                format!(
                    "verrou matériel STEP3 indisponible: {} (une autre étape GPU/Coral tourne peut-être)",
                    path.display()
                )
            })
        })
        .transpose()?;

    let matrix = MelMatrix::load(&options.mel_matrix)?;
    let yamnet_frontend = YamnetFrontend::new(matrix)?;
    let ecapa_frontend = EcapaFrontend::new();
    let mut yamnet = YamnetEngine::load(
        options.device,
        &options.yamnet,
        &options.yamnet_fallback,
        &options.tflite,
        &options.edgetpu,
        options.cpu_threads,
    )?;
    if options.device == Step3Device::Tpu {
        if let Some(version) = yamnet.edgetpu_version() {
            logger.line(format_args!("Délégué Edge TPU actif: version {version}."));
        }
        logger.line(format_args!(
            "YAMNet Split Edge TPU chargé: {}",
            options.yamnet.display()
        ));
    } else {
        logger.line(format_args!(
            "YAMNet Split CPU chargé (test explicite): {}",
            options.yamnet_fallback.display()
        ));
    }
    let mut ecapa = EcapaEngine::load(&options.ecapa, options.onnxruntime.as_deref())?;

    let mut report = Step3Report {
        total_videos,
        processed: 0,
        skipped_existing,
        skipped_no_audio: 0,
        skipped_preserved,
        failures: Vec::new(),
    };
    for video in &videos {
        logger.line(format_args!(
            "PROCESSING_VIDEO: {}",
            video.file_name().unwrap_or_default().to_string_lossy()
        ));
        match process_video(
            video,
            options,
            &logger,
            &yamnet_frontend,
            &ecapa_frontend,
            &mut yamnet,
            &mut ecapa,
        ) {
            Ok(Some(result)) => {
                logger.line(format_args!(
                    "Succès: {}.{} créé ({} segments, {} frames, {:.2} s)",
                    video.file_stem().unwrap_or_default().to_string_lossy(),
                    "json",
                    result.segments,
                    result.frames,
                    result.elapsed.as_secs_f64()
                ));
                report.processed += 1;
            }
            Ok(None) => {
                logger.line(format_args!("SKIP_NO_AUDIO: {}", video.display()));
                report.skipped_no_audio += 1;
            }
            Err(error) => {
                let message = format!("{error:#}");
                logger.line(format_args!("Échec: {}: {message}", video.display()));
                report.failures.push(Step3Failure {
                    video: video.clone(),
                    error: message,
                });
            }
        }
    }
    logger.line(format_args!(
        "--- Analyse terminée. {}/{} réussie(s). ---",
        report.processed, report.total_videos
    ));
    Ok(report)
}

fn validate_options(options: &Step3Options) -> Result<()> {
    if !options.input_dir.is_dir() {
        bail!(
            "répertoire d'entrée introuvable: {}",
            options.input_dir.display()
        );
    }
    if !options.yamnet_split {
        bail!("STEP3 ne supporte que YAMNet Split (steps.step3.yamnet_split = true)");
    }
    if options.median_filter_size == 0 {
        bail!("median_filter_size doit être > 0");
    }
    if options.embedding_stride == 0 || options.adaptive_threshold == 0 {
        bail!("embedding_stride et adaptive_threshold doivent être > 0");
    }
    if options.ecapa_batch_size == 0 {
        bail!("ecapa_batch_size doit être > 0");
    }
    if !options.mel_matrix.is_file() {
        bail!("matrice mel introuvable: {}", options.mel_matrix.display());
    }
    if !options.ecapa.is_file() {
        bail!("modèle ECAPA introuvable: {}", options.ecapa.display());
    }
    if !options.tflite.is_file() {
        bail!(
            "bibliothèque TFLite introuvable: {}",
            options.tflite.display()
        );
    }
    match options.device {
        Step3Device::Tpu => {
            if !options.yamnet.is_file() {
                bail!("modèle YAMNet introuvable: {}", options.yamnet.display());
            }
            if !options.edgetpu.exists() {
                bail!(
                    "bibliothèque Edge TPU introuvable: {}",
                    options.edgetpu.display()
                );
            }
            if !options.require_edgetpu {
                bail!("STEP3 exige require_edgetpu = true pour le chemin nominal");
            }
        }
        Step3Device::Cpu => {
            if !options.yamnet_fallback.is_file() {
                bail!(
                    "modèle YAMNet CPU introuvable: {}",
                    options.yamnet_fallback.display()
                );
            }
        }
    }
    Ok(())
}

fn discover_videos(options: &Step3Options) -> Result<Vec<PathBuf>> {
    let mut videos = BTreeSet::new();
    for entry in WalkDir::new(&options.input_dir).follow_links(false) {
        let entry =
            entry.with_context(|| format!("parcours de {}", options.input_dir.display()))?;
        if !entry.path().is_file() {
            continue;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase());
        if extension
            .as_deref()
            .is_some_and(|value| VIDEO_EXTENSIONS.contains(&value))
        {
            videos.insert(entry.path().to_path_buf());
        }
    }
    Ok(videos.into_iter().collect())
}

fn audio_output_path(video: &Path) -> PathBuf {
    let stem = video
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audio");
    video.with_file_name(format!("{stem}_audio.json"))
}

fn is_preserved_media(video: &Path, options: &Step3Options) -> bool {
    if !options.skip_preserved_mov {
        return false;
    }
    if !video
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("mov"))
    {
        return false;
    }
    let name = video
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
    probe_mov_alpha(video, &options.ffprobe).unwrap_or(true)
}

fn probe_mov_alpha(path: &Path, ffprobe: &Path) -> Option<bool> {
    let output = Command::new(ffprobe)
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
    let payload: Value = serde_json::from_slice(&output.stdout).ok()?;
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
        if let Some(pix_fmt) = stream.get("pix_fmt").and_then(Value::as_str) {
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
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if ALPHA_TAGS.contains(&tag.as_str()) {
            return Some(true);
        }
    }
    None
}

fn process_video(
    video: &Path,
    options: &Step3Options,
    logger: &Step3Logger,
    yamnet_frontend: &YamnetFrontend,
    ecapa_frontend: &EcapaFrontend,
    yamnet: &mut YamnetEngine,
    ecapa: &mut EcapaEngine,
) -> Result<Option<VideoOutcome>> {
    let started = Instant::now();
    let audio_started = Instant::now();
    let Some(audio) = extract_audio(video, options)? else {
        return Ok(None);
    };
    log_stage(logger, "audio", audio_started, video);
    if audio.samples.is_empty() {
        return Ok(None);
    }
    let total_samples = audio.samples.len();
    let hop = hop_for_overlap(YAMNET_SEGMENT_SAMPLES, options.hop_overlap)?;
    let starts = window_starts(total_samples, YAMNET_SEGMENT_SAMPLES, hop);
    let hop_sec = hop as f64 / SAMPLE_RATE as f64;
    let mut probabilities = Vec::with_capacity(starts.len());
    if !starts.is_empty() {
        let logmel_started = Instant::now();
        let log_mel = yamnet_frontend.log_mel(&audio.samples);
        log_stage(logger, "yamnet_logmel", logmel_started, video);
        let patches = yamnet_frontend.patches(&log_mel, &starts);
        let patch_size = mel::YAMNET_PATCH_FRAMES * mel::YAMNET_MELS;
        let yamnet_started = Instant::now();
        for (index, _) in starts.iter().enumerate() {
            let start = index * patch_size;
            probabilities.push(yamnet.infer_patch(&patches[start..start + patch_size])?);
        }
        log_stage(logger, "yamnet", yamnet_started, video);
    }
    let mask = postfilter(
        &probabilities,
        options.vad_threshold,
        hop_sec,
        options.hangover_sec,
        options.median_filter_size,
    );
    let segments = speech_segments(&starts, &mask);
    let speech_starts: Vec<usize> = starts
        .iter()
        .zip(mask.iter())
        .filter_map(|(start, selected)| selected.then_some(*start))
        .collect();
    logger.line(format_args!(
        "VAD: {} segments de parole détectés",
        segments.len()
    ));

    let ecapa_started = Instant::now();
    let embeddings = if segments.is_empty() {
        Vec::new()
    } else {
        extract_embeddings(
            &audio.samples,
            &speech_starts,
            options,
            logger,
            ecapa_frontend,
            ecapa,
            video,
        )?
    };
    log_stage(logger, "ecapa", ecapa_started, video);
    let clustering_started = Instant::now();
    let diarization = cluster::diarize(
        &embeddings,
        &segments,
        ClusterConfig {
            threshold: options.clustering_threshold,
            min_speaker_speech_duration: options.min_speaker_speech_duration,
            max_speakers: options.max_speakers,
        },
    )?;
    log_stage(logger, "clustering", clustering_started, video);
    let duration = audio
        .duration
        .max(total_samples as f64 / SAMPLE_RATE as f64);
    let output = audio_output_path(video);
    let json_started = Instant::now();
    write_audio_json(
        &output,
        video
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .as_ref(),
        &diarization,
        duration,
        logger,
    )?;
    log_stage(logger, "json_write", json_started, video);
    let total_frames = total_frames_for(duration, &diarization);
    Ok(Some(VideoOutcome {
        segments: diarization.len(),
        frames: total_frames,
        elapsed: started.elapsed(),
    }))
}

fn log_stage(logger: &Step3Logger, stage: &str, started: Instant, video: &Path) {
    logger.line(format_args!(
        "STAGE_TIMING: {stage}={:.1}ms video={}",
        started.elapsed().as_secs_f64() * 1000.0,
        video.file_name().unwrap_or_default().to_string_lossy()
    ));
}

fn extract_embeddings(
    samples: &[i16],
    speech_starts: &[usize],
    options: &Step3Options,
    logger: &Step3Logger,
    frontend: &EcapaFrontend,
    ecapa: &mut EcapaEngine,
    video: &Path,
) -> Result<Vec<Vec<f32>>> {
    let stride = embedding_stride(options, speech_starts.len());
    let mut positions: Vec<usize> = (0..speech_starts.len()).step_by(stride).collect();
    if positions.last().copied() != Some(speech_starts.len() - 1) {
        positions.push(speech_starts.len() - 1);
    }
    let selected_starts: Vec<usize> = positions
        .iter()
        .map(|index| speech_starts[*index])
        .collect();
    logger.line(format_args!(
        "ECAPA-TDNN: extraction de {} d-vectors 192D (stride={}, {} fenêtres de parole)...",
        selected_starts.len(),
        stride,
        speech_starts.len()
    ));
    let mut selected_embeddings = Vec::with_capacity(selected_starts.len());
    let batch_size = options.ecapa_batch_size;
    let mut offset = 0;
    let mut logmel_ns = 0u128;
    let mut onnx_ns = 0u128;
    while offset < selected_starts.len() {
        let end = (offset + batch_size).min(selected_starts.len());
        let mut batch_mels =
            Vec::with_capacity((end - offset) * mel::ECAPA_MELS * mel::ECAPA_TARGET_FRAMES);
        let logmel_started = Instant::now();
        for start in &selected_starts[offset..end] {
            let center = *start + YAMNET_SEGMENT_SAMPLES / 2;
            let segment = extract_segment(samples, center);
            batch_mels.extend(frontend.log_mel(&segment)?);
        }
        logmel_ns += logmel_started.elapsed().as_nanos();
        let onnx_started = Instant::now();
        let batch_outputs = ecapa.infer(&batch_mels, end - offset)?;
        onnx_ns += onnx_started.elapsed().as_nanos();
        for chunk in batch_outputs.chunks_exact(192) {
            selected_embeddings.push(chunk.to_vec());
        }
        offset = end;
        let percent = offset * 100 / selected_starts.len().max(1);
        logger.line(format_args!(
            "ECAPA_PROGRESS: {offset}/{} embeddings ({percent}%) - {}",
            selected_starts.len(),
            video.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    logger.line(format_args!(
        "ECAPA_TIMING: logmel={:.1}ms onnx={:.1}ms - {}",
        logmel_ns as f64 / 1_000_000.0,
        onnx_ns as f64 / 1_000_000.0,
        video.file_name().unwrap_or_default().to_string_lossy()
    ));
    if stride == 1 {
        return Ok(selected_embeddings);
    }
    let nearest = nearest_indices(speech_starts, &selected_starts);
    Ok(nearest
        .into_iter()
        .map(|index| selected_embeddings[index].clone())
        .collect())
}

fn embedding_stride(options: &Step3Options, speech_windows: usize) -> usize {
    if options.adaptive_stride {
        if speech_windows >= options.adaptive_threshold {
            2
        } else {
            1
        }
    } else {
        options.embedding_stride.max(1)
    }
}

fn nearest_indices(all_times: &[usize], selected_times: &[usize]) -> Vec<usize> {
    if selected_times.is_empty() {
        return Vec::new();
    }
    all_times
        .iter()
        .map(|time| {
            let right = selected_times.partition_point(|value| *value < *time);
            let right = right.min(selected_times.len() - 1);
            let left = right.saturating_sub(1);
            if time.abs_diff(selected_times[left]) <= selected_times[right].abs_diff(*time) {
                left
            } else {
                right
            }
        })
        .collect()
}

fn extract_segment(samples: &[i16], center_sample: usize) -> Vec<i16> {
    let half = ECAPA_SEGMENT_SAMPLES / 2;
    let start = center_sample as isize - half as isize;
    let end = center_sample as isize + half as isize;
    let mut output = vec![0_i16; ECAPA_SEGMENT_SAMPLES];
    let source_start = start.max(0) as usize;
    let source_end = end.min(samples.len() as isize).max(0) as usize;
    if source_end > source_start {
        let destination_start = (source_start as isize - start) as usize;
        output[destination_start..destination_start + (source_end - source_start)]
            .copy_from_slice(&samples[source_start..source_end]);
    }
    output
}

struct AudioData {
    samples: Vec<i16>,
    duration: f64,
}

fn extract_audio(video: &Path, options: &Step3Options) -> Result<Option<AudioData>> {
    if !has_audio_stream(video, &options.ffprobe)? {
        return Ok(None);
    }
    let mut command = Command::new(&options.ffmpeg);
    command.args(["-nostdin", "-v", "error"]);
    if options.ffmpeg_threads > 0 {
        command
            .arg("-threads")
            .arg(options.ffmpeg_threads.to_string());
    }
    let output = command
        .arg("-i")
        .arg(video)
        .args([
            "-vn",
            "-acodec",
            "pcm_s16le",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-f",
            "s16le",
            "pipe:1",
        ])
        .output()
        .with_context(|| format!("lancement de FFmpeg pour {}", video.display()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("does not contain any stream")
            || stderr.contains("Output file does not contain any stream")
            || stderr.contains("matches no streams")
        {
            return Ok(None);
        }
        bail!(
            "FFmpeg a échoué pour {} (code {}): {}",
            video.display(),
            output.status.code().unwrap_or(-1),
            stderr.trim()
        );
    }
    if output.stdout.len() < 2 {
        return Ok(None);
    }
    if output.stdout.len() % 2 != 0 {
        bail!("flux PCM16 tronqué pour {}", video.display());
    }
    let samples: Vec<i16> = output
        .stdout
        .chunks_exact(2)
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();
    let duration = probe_duration(video, &options.ffprobe)
        .unwrap_or_else(|| samples.len() as f64 / SAMPLE_RATE as f64);
    Ok(Some(AudioData { samples, duration }))
}

fn has_audio_stream(video: &Path, ffprobe: &Path) -> Result<bool> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_type",
            "-of",
            "json",
        ])
        .arg(video)
        .output()
        .with_context(|| format!("ffprobe audio pour {}", video.display()))?;
    if !output.status.success() {
        // Laisser FFmpeg produire le diagnostic final.
        return Ok(true);
    }
    let payload: Value = serde_json::from_slice(&output.stdout).context("JSON ffprobe audio")?;
    Ok(payload
        .get("streams")
        .and_then(Value::as_array)
        .is_some_and(|streams| !streams.is_empty()))
}

fn probe_duration(video: &Path, ffprobe: &Path) -> Option<f64> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(video)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()
}

fn total_frames_for(duration: f64, diarization: &[DiarizationSegment]) -> i64 {
    let mut total = if duration > 0.0 {
        (duration * FPS).round_ties_even() as i64
    } else {
        -1
    };
    if total < 0 {
        let max_frame = diarization
            .iter()
            .map(|segment| (segment.end * FPS) as i64)
            .max()
            .unwrap_or(0);
        total = max_frame;
    }
    total.max(1)
}

fn write_audio_json(
    output: &Path,
    video_filename: &str,
    diarization: &[DiarizationSegment],
    duration: f64,
    logger: &Step3Logger,
) -> Result<()> {
    let total_frames = total_frames_for(duration, diarization);
    let total_frames_usize = usize::try_from(total_frames).context("total_frames négatif")?;
    let mut timeline: Vec<BTreeSet<String>> = vec![BTreeSet::new(); total_frames_usize];
    for segment in diarization {
        let start_frame = ((segment.start * FPS) as i64).max(1);
        let end_frame = (segment.end * FPS) as i64;
        if end_frame < 1 || end_frame < start_frame {
            continue;
        }
        let start = (start_frame - 1) as usize;
        let end = (end_frame - 1) as usize;
        for frame in start..=end {
            if let Some(labels) = timeline.get_mut(frame) {
                labels.insert(segment.speaker.clone());
            }
        }
    }
    let speaker_stats = speaker_stats(diarization);
    let temp = output.with_file_name(format!(
        "{}.tmp",
        output
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("audio")
    ));
    let result = (|| -> Result<()> {
        let file = File::create(&temp)
            .with_context(|| format!("création du JSON audio {}", temp.display()))?;
        let mut writer = BufWriter::new(file);
        write!(writer, "{{\n  \"video_filename\": ")?;
        serde_json::to_writer(&mut writer, &Value::String(video_filename.to_string()))?;
        write!(
            writer,
            ",\n  \"total_frames\": {total_frames},\n  \"fps\": "
        )?;
        serde_json::to_writer(&mut writer, &json!(FPS))?;
        if !speaker_stats.is_empty() {
            write!(writer, ",\n  \"speaker_stats\": ")?;
            serde_json::to_writer(&mut writer, &speaker_stats)?;
        }
        write!(writer, ",\n  \"frames_analysis\": [\n")?;
        for frame in 1..=total_frames_usize {
            if frame > 1 {
                writeln!(writer, ",")?;
            }
            let labels: Vec<&str> = timeline[frame - 1].iter().map(String::as_str).collect();
            let timecode = round_to((frame - 1) as f64 / FPS, 3);
            let payload = json!({
                "frame": frame,
                "audio_info": {
                    "is_speech_present": !labels.is_empty(),
                    "num_distinct_speakers_audio": labels.len(),
                    "active_speaker_labels": labels,
                    "timecode_sec": timecode,
                }
            });
            serde_json::to_writer(&mut writer, &payload)?;
            if frame % 500 == 0 || frame == total_frames_usize {
                let percent = frame * 100 / total_frames_usize.max(1);
                logger.line(format_args!(
                    "INTERNAL_PROGRESS: {frame}/{total_frames_usize} frames ({percent}%) - {video_filename}"
                ));
            }
        }
        write!(writer, "\n  ]\n}}\n")?;
        writer.flush().context("flush du JSON audio")?;
        fs::rename(&temp, output)
            .with_context(|| format!("renommage atomique de {}", output.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn speaker_stats(diarization: &[DiarizationSegment]) -> Map<String, Value> {
    let mut times: BTreeMap<String, f64> = BTreeMap::new();
    for segment in diarization {
        *times.entry(segment.speaker.clone()).or_insert(0.0) += segment.end - segment.start;
    }
    let total: f64 = times.values().sum();
    let mut stats = Map::new();
    for (speaker, seconds) in times {
        let percentage = if total > 0.0 {
            round_to(seconds / total * 100.0, 1)
        } else {
            0.0
        };
        stats.insert(
            speaker,
            json!({
                "total_time_sec": round_to(seconds, 3),
                "percentage": percentage,
            }),
        );
    }
    stats
}

fn round_to(value: f64, decimals: u32) -> f64 {
    let factor = 10_f64.powi(decimals as i32);
    (value * factor).round_ties_even() / factor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_frames_uses_duration_rounding() {
        assert_eq!(total_frames_for(196.84, &[]), 4921);
        assert_eq!(total_frames_for(0.0, &[]), 1);
    }

    #[test]
    fn timeline_marks_inclusive_frames() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("x_audio.json");
        let diarization = vec![DiarizationSegment {
            speaker: "SPEAKER_00".into(),
            start: 0.0,
            end: 0.08,
        }];
        write_audio_json(&output, "x.mp4", &diarization, 0.12, &test_logger()).unwrap();
        let value: Value = serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
        let frames = value["frames_analysis"].as_array().unwrap();
        assert_eq!(frames.len(), 3);
        assert_eq!(
            frames[0]["audio_info"]["active_speaker_labels"],
            json!(["SPEAKER_00"])
        );
        assert_eq!(
            frames[1]["audio_info"]["active_speaker_labels"],
            json!(["SPEAKER_00"])
        );
        assert_eq!(frames[2]["audio_info"]["active_speaker_labels"], json!([]));
    }

    #[test]
    fn round_to_uses_python_like_rounding() {
        assert_eq!(round_to(0.125, 2), 0.12);
        assert_eq!(round_to(0.135, 2), 0.14);
        assert_eq!(round_to(196.84 * 25.0, 0), 4921.0);
    }

    #[test]
    fn segment_extraction_pads_silence() {
        let samples = vec![100_i16; 10];
        let segment = extract_segment(&samples, 5);
        assert_eq!(segment.len(), ECAPA_SEGMENT_SAMPLES);
        assert_eq!(segment[0], 0);
    }

    #[test]
    fn nearest_index_prefers_left_on_tie() {
        let all = [10, 20, 30];
        let selected = [10, 30];
        assert_eq!(nearest_indices(&all, &selected), vec![0, 0, 1]);
    }

    fn test_logger() -> Step3Logger {
        Step3Logger { file: None }
    }
}
