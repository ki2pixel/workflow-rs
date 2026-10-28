//! STEP4 : cascade BlazeFace → FaceMesh → Blendshapes sur Google Coral.
//!
//! Le chemin nominal est l'Edge TPU ; le backend CPU TFLite/XNNPACK utilise le
//! même code de préparation, de post-traitement et de suivi pour les tests.
//! La sortie reste un JSON par vidéo, avec le schéma de l'application Python.

pub mod engine;
pub mod face;
pub mod filter;
pub mod image;
pub mod json;
pub mod object;
pub mod speaking;
pub mod tracking;

use anyhow::{bail, Context, Result};
use chrono::Local;
use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;
use walkdir::WalkDir;
use workflow_core::{Config, HardwareLock};

use crate::step4::engine::{ExecutionDevice, FaceCascade, ModelPaths};
use crate::step4::face::{detect_face, extract_blendshapes, extract_landmarks};
use crate::step4::filter::OneEuroFilter;
use crate::step4::image::{warp_affine_linear, AffineTransform, RgbImage};
use crate::step4::json::{Metadata, Resolution, StreamingJson};
use crate::step4::object::{detect_objects, ObjectDetection};
use crate::step4::speaking::SpeakingDetector;
use crate::step4::tracking::{BlendFilter, Detection, Tracker};
use crate::template;

const FACE_CROP_SIZE: usize = 192;
const TRACKING_METHOD: &str = "kdtree_tpu";
const ENGINE_NAME: &str = "blazeface_tpu";
const PROGRESS_INTERVAL: u64 = 25;

#[derive(Debug, Clone)]
pub struct Step4Options {
    pub input_dir: PathBuf,
    pub models: ModelPaths,
    pub device: ExecutionDevice,
    pub cpu_threads: i32,
    pub object_detection: bool,
    pub object_min_missing: u32,
    pub face_reset_frames: u32,
    pub object_score_threshold: f64,
    pub object_max: usize,
    pub blend_filter: BlendFilter,
    pub jaw_open_threshold: f64,
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub decode_width: u32,
    pub decode_height: u32,
    /// Verrou matériel partagé avec STEP2/STEP3. `None` pour les tests isolés.
    pub lock_path: Option<PathBuf>,
    /// Journal STEP4. `None` désactive le fichier.
    pub logs_dir: Option<PathBuf>,
    pub skip_preserved_mov: bool,
    pub preserved_mov_names: Vec<String>,
    /// Autorise un JSON partiel quand le CSV de scènes ou l'audio manquent.
    /// Désactivé par défaut : la production exige les deux entrées.
    pub allow_partial_speaking: bool,
    /// Préfixes actifs du bypass direct (SDM + archive-only).
    pub bypass: template::Bypass,
}

impl Step4Options {
    /// Re-sélectionne les modèles quand la CLI force un backend différent
    /// de celui du TOML.
    pub fn set_device(&mut self, config: &Config, device: ExecutionDevice) {
        self.device = device;
        let step = &config.steps.step4;
        let models_dir = &config.paths.models;
        let resolve = |path: &Path| -> PathBuf {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                models_dir.join(path)
            }
        };
        match device {
            ExecutionDevice::Cpu => {
                self.models.blazeface = resolve(&step.blazeface_cpu);
                self.models.facemesh = resolve(&step.facemesh_cpu);
                self.models.object_detector = step
                    .object_detection
                    .then(|| resolve(&step.object_detector_cpu));
            }
            ExecutionDevice::Tpu => {
                let mut blazeface = resolve(&step.blazeface);
                let mut facemesh = resolve(&step.facemesh);
                if step.cocompiled {
                    let cocompiled_blazeface = models_dir
                        .join("step4/cocompiled/blazeface_front_quantized_edgetpu.tflite");
                    let cocompiled_facemesh =
                        models_dir.join("step4/cocompiled/facemesh_quantized_edgetpu.tflite");
                    if cocompiled_blazeface.is_file() && cocompiled_facemesh.is_file() {
                        blazeface = cocompiled_blazeface;
                        facemesh = cocompiled_facemesh;
                    }
                }
                self.models.blazeface = blazeface;
                self.models.facemesh = facemesh;
                self.models.object_detector = step
                    .object_detection
                    .then(|| resolve(&step.object_detector_path));
            }
        }
    }

    pub fn from_config(config: &Config, input_dir: PathBuf) -> Result<Self> {
        let step = &config.steps.step4;
        let models_dir = &config.paths.models;
        let resolve = |path: &Path| -> PathBuf {
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                models_dir.join(path)
            }
        };

        let mut blazeface = resolve(&step.blazeface);
        let mut facemesh = resolve(&step.facemesh);
        if step.cocompiled && step.device.eq_ignore_ascii_case("tpu") {
            let cocompiled_blazeface =
                models_dir.join("step4/cocompiled/blazeface_front_quantized_edgetpu.tflite");
            let cocompiled_facemesh =
                models_dir.join("step4/cocompiled/facemesh_quantized_edgetpu.tflite");
            if cocompiled_blazeface.is_file() && cocompiled_facemesh.is_file() {
                blazeface = cocompiled_blazeface;
                facemesh = cocompiled_facemesh;
            }
        }

        let device = ExecutionDevice::parse(&step.device)?;
        let (blazeface, facemesh, object_detector) = match device {
            ExecutionDevice::Tpu => (
                blazeface,
                facemesh,
                step.object_detection
                    .then(|| resolve(&step.object_detector_path)),
            ),
            ExecutionDevice::Cpu => (
                resolve(&step.blazeface_cpu),
                resolve(&step.facemesh_cpu),
                step.object_detection
                    .then(|| resolve(&step.object_detector_cpu)),
            ),
        };

        let preserved_mov_names = if config.steps.step1.preserved_mov_names.is_empty() {
            vec!["m6+.mov".to_string()]
        } else {
            config.steps.step1.preserved_mov_names.clone()
        };

        Ok(Self {
            input_dir,
            models: ModelPaths {
                tflite: step.tflite.clone(),
                edgetpu: step.edgetpu.clone(),
                blazeface,
                facemesh,
                blendshapes: resolve(&step.blendshapes),
                object_detector,
            },
            device,
            cpu_threads: i32::try_from(step.cpu_threads).unwrap_or(i32::MAX).max(1),
            object_detection: step.object_detection,
            object_min_missing: step.object_min_missing,
            face_reset_frames: step.face_reset_frames,
            object_score_threshold: step.object_score_threshold,
            object_max: usize::try_from(step.object_max).unwrap_or(usize::MAX),
            blend_filter: BlendFilter {
                profile: step.blendshapes_profile.clone(),
                export_keys: step.blendshapes_export.clone(),
                include_tongue: step.blendshapes_include_tongue,
            },
            jaw_open_threshold: step.jaw_open_threshold,
            ffmpeg: step.ffmpeg.clone(),
            ffprobe: step.ffprobe.clone(),
            decode_width: step.decode_width,
            decode_height: step.decode_height,
            lock_path: Some(config.hardware.lock_path.clone()),
            logs_dir: Some(config.paths.resolve().logs.join("step4")),
            skip_preserved_mov: config.steps.step1.skip_preserved_mov,
            preserved_mov_names,
            allow_partial_speaking: false,
            bypass: if config.steps.step8.post.direct {
                template::Bypass::from_config(&config.steps.step8.post)
            } else {
                template::Bypass::default()
            },
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Step4Failure {
    pub video: PathBuf,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct Step4Report {
    pub total_videos: usize,
    pub processed: usize,
    pub skipped_preserved: usize,
    pub skipped_template: usize,
    pub failures: Vec<Step4Failure>,
}

impl Step4Report {
    pub fn success(&self) -> bool {
        self.failures.is_empty()
    }
}

struct Step4Logger {
    file: Option<Mutex<BufWriter<File>>>,
}

impl Step4Logger {
    fn new(dir: Option<&Path>) -> Result<Self> {
        let file = if let Some(dir) = dir {
            fs::create_dir_all(dir)
                .with_context(|| format!("création du journal STEP4: {}", dir.display()))?;
            let name = format!("tpu_tracking_{}.log", Local::now().format("%Y%m%d_%H%M%S"));
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join(&name))
                .with_context(|| format!("ouverture du journal STEP4: {name}"))?;
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

#[derive(Debug, Clone)]
struct VideoMetadata {
    width: usize,
    height: usize,
    fps: f64,
    total_frames: Option<u64>,
}

/// Exécute STEP4 sur `options.input_dir`.
pub fn run_step4(options: &Step4Options) -> Result<Step4Report> {
    validate_options(options)?;
    let logger = Step4Logger::new(options.logs_dir.as_deref())?;
    let mut videos = Vec::new();
    let mut skipped_preserved = 0;
    let mut skipped_template = 0;
    for video in discover_videos(&options.input_dir)? {
        match options.bypass.probe(&options.ffprobe, &video) {
            Ok(Some(_)) => {
                logger.line(format_args!("SKIP_TEMPLATE: {}", video.display()));
                skipped_template += 1;
                continue;
            }
            Ok(None) => {}
            Err(error) => logger.line(format_args!(
                "WARN: sonde bypass impossible pour {}: {error:#}",
                video.display()
            )),
        }
        if is_preserved_media(&video, options) {
            logger.line(format_args!("SKIP_PRESERVED: {}", video.display()));
            skipped_preserved += 1;
            continue;
        }
        videos.push(video);
    }
    logger.line(format_args!("TOTAL_VIDEOS_TO_PROCESS: {}", videos.len()));
    let mut report = Step4Report {
        total_videos: videos.len(),
        processed: 0,
        skipped_preserved,
        skipped_template,
        failures: Vec::new(),
    };
    if videos.is_empty() {
        logger.line(format_args!("--- Analyse terminée. 0/0 réussie(s). ---"));
        return Ok(report);
    }

    let _lock = options
        .lock_path
        .as_ref()
        .map(|path| {
            HardwareLock::acquire(path, false).with_context(|| {
                format!(
                    "verrou matériel STEP4 indisponible: {} (une autre étape GPU/Coral tourne peut-être)",
                    path.display()
                )
            })
        })
        .transpose()?;

    let mut cascade = match FaceCascade::open(
        options.device,
        &options.models,
        options.cpu_threads,
        options.object_detection,
    ) {
        Ok(cascade) => cascade,
        Err(error) => {
            logger.line(format_args!(
                "Échec: initialisation de la cascade STEP4: {error:#}"
            ));
            return Err(error);
        }
    };

    for video in &videos {
        logger.line(format_args!(
            "PROCESSING_VIDEO: {}",
            video.file_name().unwrap_or_default().to_string_lossy()
        ));
        match process_video(video, &mut cascade, options, &logger) {
            Ok(frame_count) => {
                let output = video.with_extension("json");
                logger.line(format_args!(
                    "Succès: {} créé avec {} frames.",
                    output.file_name().unwrap_or_default().to_string_lossy(),
                    frame_count
                ));
                report.processed += 1;
            }
            Err(error) => {
                let message = format!("{error:#}");
                logger.line(format_args!(
                    "Échec de l'analyse tracking pour {}: {message}",
                    video.file_name().unwrap_or_default().to_string_lossy()
                ));
                report.failures.push(Step4Failure {
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

fn validate_options(options: &Step4Options) -> Result<()> {
    if !options.input_dir.is_dir() {
        bail!(
            "répertoire d'entrée introuvable: {}",
            options.input_dir.display()
        );
    }
    if !options.models.tflite.is_file() {
        bail!(
            "bibliothèque TFLite C introuvable: {}",
            options.models.tflite.display()
        );
    }
    if options.device == ExecutionDevice::Tpu && !options.models.edgetpu.is_file() {
        bail!(
            "bibliothèque Edge TPU introuvable: {}",
            options.models.edgetpu.display()
        );
    }
    for (role, path) in [
        ("BlazeFace", &options.models.blazeface),
        ("FaceMesh", &options.models.facemesh),
        ("Blendshapes", &options.models.blendshapes),
    ] {
        if !path.is_file() {
            bail!("modèle {role} introuvable: {}", path.display());
        }
    }
    if let Some(path) = &options.models.object_detector {
        if !path.is_file() {
            bail!("détecteur d'objets introuvable: {}", path.display());
        }
    }
    if options.object_min_missing == 0 || options.face_reset_frames == 0 {
        bail!("les seuils de suivi STEP4 doivent être > 0");
    }
    if options.object_max == 0 {
        bail!("object_max doit être > 0");
    }
    if (options.decode_width == 0) != (options.decode_height == 0) {
        bail!("decode_width et decode_height doivent être renseignés ensemble");
    }
    Ok(())
}

fn discover_videos(input_dir: &Path) -> Result<Vec<PathBuf>> {
    const EXTENSIONS: [&str; 5] = ["mp4", "avi", "mov", "mkv", "webm"];
    let mut videos = BTreeSet::new();
    for entry in WalkDir::new(input_dir).follow_links(false) {
        let entry = entry.with_context(|| format!("parcours de {}", input_dir.display()))?;
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
            .is_some_and(|value| EXTENSIONS.contains(&value))
        {
            videos.insert(entry.path().to_path_buf());
        }
    }
    Ok(videos.into_iter().collect())
}

fn process_video(
    video: &Path,
    cascade: &mut FaceCascade,
    options: &Step4Options,
    logger: &Step4Logger,
) -> Result<u64> {
    let started = Instant::now();
    let metadata = probe_video(video, &options.ffprobe)?;
    let (width, height) = if options.decode_width > 0 && options.decode_height > 0 {
        (
            options.decode_width as usize,
            options.decode_height as usize,
        )
    } else {
        (metadata.width, metadata.height)
    };
    if width == 0 || height == 0 {
        bail!("résolution vidéo invalide pour {}", video.display());
    }

    let csv_path = video.with_extension("csv");
    let audio_path = video.with_file_name(format!(
        "{}_audio.json",
        video
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    ));
    if !options.allow_partial_speaking {
        if !csv_path.is_file() {
            bail!(
                "CSV de scènes introuvable pour {}: {} (utiliser --allow-partial-speaking pour un test sans audio/CSV)",
                video.display(),
                csv_path.display()
            );
        }
        if !audio_path.is_file() {
            bail!(
                "JSON audio introuvable pour {}: {} (utiliser --allow-partial-speaking pour un test sans audio/CSV)",
                video.display(),
                audio_path.display()
            );
        }
    } else if !csv_path.is_file() || !audio_path.is_file() {
        logger.line(format_args!(
            "AVERTISSEMENT STEP4: entrées de parole incomplètes pour {} (CSV={}, audio={})",
            video.display(),
            csv_path.is_file(),
            audio_path.is_file()
        ));
    }
    let speaking_detector = SpeakingDetector::load(
        video,
        options.jaw_open_threshold,
        options.allow_partial_speaking,
    )?;
    let output = video.with_extension("json");
    let json_metadata = Metadata {
        video_path: video.to_string_lossy().to_string(),
        original_fps: metadata.fps,
        resolution: Resolution {
            width: metadata.width,
            height: metadata.height,
        },
        total_frames_processed: metadata.total_frames,
        engine: ENGINE_NAME.to_string(),
        tracking_method: TRACKING_METHOD.to_string(),
    };
    let mut stream = StreamingJson::create(&output, &json_metadata)?;

    let mut child = ffmpeg_command(video, width, height, metadata.fps, options)
        .spawn()
        .with_context(|| format!("lancement de ffmpeg pour {}", video.display()))?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            kill_child(&mut child);
            bail!("stdout ffmpeg indisponible pour {}", video.display());
        }
    };

    enum DecodedFrame {
        Frame(Vec<u8>),
        End(Result<(), String>),
    }

    let frame_size = width * height * 3;
    let video_display = video.display().to_string();
    let (frame_tx, frame_rx) = std::sync::mpsc::sync_channel::<DecodedFrame>(3);
    let decoder = std::thread::spawn(move || {
        let _guard = crate::process::ChildGuard::register(child.id());
        let mut stdout = stdout;
        let mut child = child;
        loop {
            let mut buffer = vec![0u8; frame_size];
            let mut filled = 0usize;
            let mut read_error = None;
            while filled < frame_size {
                match stdout.read(&mut buffer[filled..]) {
                    Ok(0) => break,
                    Ok(read) => filled += read,
                    Err(error) => {
                        read_error = Some(error);
                        break;
                    }
                }
            }
            if let Some(error) = read_error {
                let _ = frame_tx.send(DecodedFrame::End(Err(format!(
                    "lecture du flux ffmpeg pour {video_display}: {error}"
                ))));
                kill_child(&mut child);
                let _ = child.wait();
                return;
            }
            if filled == 0 {
                break;
            }
            if filled < frame_size {
                break;
            }
            if frame_tx.send(DecodedFrame::Frame(buffer)).is_err() {
                kill_child(&mut child);
                let _ = child.wait();
                return;
            }
        }
        drop(stdout);
        let status = child.wait();
        let result = match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(format!(
                "ffmpeg a échoué pour {video_display} (code {})",
                status.code().unwrap_or(-1)
            )),
            Err(error) => Err(format!("attente de ffmpeg pour {video_display}: {error}")),
        };
        let _ = frame_tx.send(DecodedFrame::End(result));
    });

    let mut frame_index: u64 = 0;
    let mut frames_without_face: u32 = 0;
    let mut consecutive_face_frames: u32 = 0;
    let mut tracker = Tracker::new();
    let mut temporal_filter = OneEuroFilter::new(metadata.fps);
    let video_name = video
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mut result: Result<()> = Ok(());
    let mut decoder_error: Option<String> = None;
    while let Ok(message) = frame_rx.recv() {
        match message {
            DecodedFrame::End(status) => {
                if let Err(error) = status {
                    decoder_error = Some(error);
                }
                break;
            }
            DecodedFrame::Frame(frame_buffer) => {
                let frame_number = frame_index + 1;
                let image = RgbImage::from_raw(width, height, frame_buffer);
                let mut detections = Vec::new();
                let mut face_detected = false;

                match detect_face(&mut cascade.blazeface, &image) {
                    Ok(Some(face)) => {
                        face_detected = true;
                        // NumPy effectue xmax - xmin en float32 avant la promotion
                        // en float64 par la multiplication avec la largeur entière.
                        let bbox_xmin = (face.bbox[1] as f64 * width as f64) as i64;
                        let bbox_ymin = (face.bbox[0] as f64 * height as f64) as i64;
                        let bbox_width =
                            ((face.bbox[3] - face.bbox[1]) as f64 * width as f64).max(1.0) as i64;
                        let bbox_height =
                            ((face.bbox[2] - face.bbox[0]) as f64 * height as f64).max(1.0) as i64;
                        let centroid = (bbox_xmin + bbox_width / 2, bbox_ymin + bbox_height / 2);

                        // Le chemin Python applique toujours l'affine dès qu'un visage
                        // est détecté, même si les deux yeux sont confondus (dist=0) :
                        // le crop devient alors quasi noir, ce qui influence le filtre.
                        let affine = AffineTransform::from_eyes(
                            face.right_eye,
                            face.left_eye,
                            FACE_CROP_SIZE,
                        );
                        let crop = warp_affine_linear(
                            &image,
                            affine.matrix(width, height),
                            FACE_CROP_SIZE,
                        );

                        let landmarks = match extract_landmarks(&mut cascade.facemesh, &crop) {
                            Ok(landmarks) => landmarks,
                            Err(error) => {
                                result = Err(error.context("extraction FaceMesh"));
                                break;
                            }
                        };
                        let raw_blendshapes =
                            match extract_blendshapes(&mut cascade.blendshapes, &landmarks) {
                                Ok(values) => values,
                                Err(error) => {
                                    result = Err(error.context("extraction Blendshapes"));
                                    break;
                                }
                            };
                        let smoothed = temporal_filter.update(&raw_blendshapes);
                        let filtered = options.blend_filter.filter(&smoothed);
                        detections.push(Detection {
                            centroid,
                            bbox: (bbox_xmin, bbox_ymin, bbox_width, bbox_height),
                            blendshapes: filtered,
                            source_detector: "face_landmarker".into(),
                            label: "face".into(),
                            confidence: 1.0,
                        });
                    }
                    Ok(None) => {}
                    Err(error) => {
                        result = Err(error.context("détection BlazeFace"));
                        break;
                    }
                }

                if face_detected {
                    consecutive_face_frames += 1;
                    if consecutive_face_frames >= options.face_reset_frames {
                        frames_without_face = 0;
                    }
                } else {
                    consecutive_face_frames = 0;
                    frames_without_face += 1;
                }

                if !face_detected
                    && frames_without_face >= options.object_min_missing
                    && options.object_detection
                {
                    if let Some(object_session) = &mut cascade.object_detector {
                        match detect_objects(
                            object_session,
                            &image,
                            width,
                            height,
                            options.object_score_threshold,
                            options.object_max,
                        ) {
                            Ok(objects) => {
                                for object in objects {
                                    detections.push(object_to_detection(object));
                                }
                            }
                            Err(error) => {
                                result = Err(error.context("détection d'objets"));
                                break;
                            }
                        }
                    }
                }

                let objects = tracker.update(&detections, frame_number, &speaking_detector);
                if let Err(error) = stream.push(frame_number, &objects) {
                    result = Err(error);
                    break;
                }
                frame_index = frame_number;
                if let Some(total) = metadata.total_frames {
                    if total > 0 && frame_number % PROGRESS_INTERVAL == 0 {
                        let percent = frame_number.saturating_mul(100) / total;
                        logger.line(format_args!(
                            "INTERNAL_PROGRESS: {frame_number}/{total} frames ({percent}%) - {video_name}"
                        ));
                    }
                }
            }
        }
    }
    drop(frame_rx);
    if decoder.join().is_err() && result.is_ok() {
        result = Err(anyhow::anyhow!(
            "le thread de décodage FFmpeg a paniqué pour {}",
            video.display()
        ));
    }
    if result.is_ok() {
        if let Some(error) = decoder_error {
            result = Err(anyhow::anyhow!(error));
        }
    }

    if let Err(error) = result {
        let temp = stream.path().to_path_buf();
        drop(stream);
        let _ = fs::remove_file(temp);
        return Err(error);
    }

    if let Some(total) = metadata.total_frames {
        if total > 0 && frame_index > 0 && frame_index < total {
            let percent = frame_index.saturating_mul(100) / total;
            logger.line(format_args!(
                "INTERNAL_PROGRESS: {frame_index}/{total} frames ({percent}%) - {video_name}"
            ));
        } else if total > 0 && frame_index == total && total % PROGRESS_INTERVAL != 0 {
            logger.line(format_args!(
                "INTERNAL_PROGRESS: {total}/{total} frames (100%) - {video_name}"
            ));
        }
    }
    stream.finish(&output)?;
    logger.line(format_args!(
        "STEP4 terminé pour {} en {:.2} s ({} frames)",
        video_name,
        started.elapsed().as_secs_f64(),
        frame_index
    ));
    Ok(frame_index)
}

fn object_to_detection(object: ObjectDetection) -> Detection {
    Detection {
        centroid: object.centroid,
        bbox: object.bbox,
        blendshapes: Some(Vec::new()),
        source_detector: "object_detector".into(),
        label: object.label,
        confidence: object.confidence,
    }
}

fn probe_video(video: &Path, ffprobe: &Path) -> Result<VideoMetadata> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,nb_frames",
            "-of",
            "json",
        ])
        .arg(video)
        .output()
        .with_context(|| format!("ffprobe pour {}", video.display()))?;
    if !output.status.success() {
        bail!(
            "ffprobe a échoué pour {}: {}",
            video.display(),
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
    let width = stream
        .get("width")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(1280);
    let height = stream
        .get("height")
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(720);
    let fps = stream
        .get("r_frame_rate")
        .and_then(|value| value.as_str())
        .and_then(parse_frame_rate)
        .filter(|value| *value > 0.0)
        .unwrap_or(25.0);
    let total_frames = stream
        .get("nb_frames")
        .and_then(|value| value.as_str())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0);
    Ok(VideoMetadata {
        width,
        height,
        fps,
        total_frames,
    })
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

fn ffmpeg_command(
    video: &Path,
    width: usize,
    height: usize,
    fps: f64,
    options: &Step4Options,
) -> Command {
    let mut command = Command::new(&options.ffmpeg);
    command
        .arg("-nostdin")
        .arg("-v")
        .arg("error")
        .arg("-i")
        .arg(video)
        .arg("-f")
        .arg("image2pipe")
        .arg("-pix_fmt")
        .arg("rgb24")
        .arg("-s")
        .arg(format!("{width}x{height}"))
        .arg("-r")
        .arg(format!("{fps}"))
        .arg("-vcodec")
        .arg("rawvideo")
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    command
}

fn kill_child(child: &mut Child) {
    let _ = child.kill();
}

fn is_preserved_media(path: &Path, options: &Step4Options) -> bool {
    crate::media::is_preserved_media_with(
        path,
        options.skip_preserved_mov,
        &options.preserved_mov_names,
        &options.ffprobe,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fractional_frame_rates() {
        assert!((parse_frame_rate("25/1").unwrap() - 25.0).abs() < 1e-9);
        assert!((parse_frame_rate("30000/1001").unwrap() - 29.97002997).abs() < 1e-6);
        assert!(parse_frame_rate("0/0").is_none());
    }

    #[test]
    fn alpha_pixel_formats_are_strict() {
        use crate::media::pix_fmt_has_alpha;
        assert!(pix_fmt_has_alpha("yuva420p"));
        assert!(pix_fmt_has_alpha("rgba64le"));
        assert!(pix_fmt_has_alpha("ya8"));
        assert!(!pix_fmt_has_alpha("yuv420p"));
        assert!(!pix_fmt_has_alpha("gray"));
        assert!(!pix_fmt_has_alpha("pal8"));
        assert!(!pix_fmt_has_alpha(""));
    }

    #[test]
    fn preserved_mov_names_are_case_insensitive() {
        let options = Step4Options {
            input_dir: PathBuf::new(),
            models: ModelPaths {
                tflite: PathBuf::new(),
                edgetpu: PathBuf::new(),
                blazeface: PathBuf::new(),
                facemesh: PathBuf::new(),
                blendshapes: PathBuf::new(),
                object_detector: None,
            },
            device: ExecutionDevice::Cpu,
            cpu_threads: 1,
            object_detection: false,
            object_min_missing: 25,
            face_reset_frames: 1,
            object_score_threshold: 0.3,
            object_max: 5,
            blend_filter: BlendFilter {
                profile: "full".into(),
                export_keys: Vec::new(),
                include_tongue: false,
            },
            jaw_open_threshold: 0.08,
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            decode_width: 0,
            decode_height: 0,
            lock_path: None,
            logs_dir: None,
            skip_preserved_mov: true,
            preserved_mov_names: vec!["m6+.mov".into()],
            allow_partial_speaking: false,
            bypass: template::Bypass::default(),
        };
        assert!(is_preserved_media(Path::new("M6+.MOV"), &options));
    }
}
