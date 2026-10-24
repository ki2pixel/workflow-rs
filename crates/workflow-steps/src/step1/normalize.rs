//! Normalisation vidéo STEP1 : MP4 / H.264 / progressif / cadence cible.
//!
//! Reproduit `utils/media_normalizer.py` : analyse ffprobe (fps réel, codec,
//! entrelacement), décision de conformité, encodage GPU avec repli CPU et
//! protection des `.mov` préservés.

use super::{RunLogger, Step1Options};
use crate::step2::is_preserved_media_with;
use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

const FPS_TOLERANCE: f64 = 0.1;
const VIDEO_EXTENSIONS: [&str; 7] = ["mp4", "mov", "avi", "mkv", "webm", "flv", "wmv"];
const DEINTERLACE_FILTER: &str = "yadif";
const INTERLACE_PROBE_SECONDS: u32 = 1;
const AUDIO_COPY_CODECS: [&str; 3] = ["aac", "mp3", "ac3"];
const TEMP_SUFFIX: &str = ".temp_normalize.mp4";
const SKIP_NAME_TOKENS: [&str; 4] = [
    "_temp_conversion",
    "_converted",
    ".temp_compress",
    ".temp_normalize",
];
const MAX_PROBE_WORKERS: usize = 10;

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub path: PathBuf,
    pub fps: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub interlaced: Option<bool>,
    pub error: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NormalizationSummary {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub skipped: usize,
}

impl NormalizationSummary {
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }
}

pub fn parse_ffprobe_fraction(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.is_empty() || value == "0/0" {
        return None;
    }
    if let Some((numerator, denominator)) = value.split_once('/') {
        let numerator = numerator.trim().parse::<f64>().ok()?;
        let denominator = denominator.trim().parse::<f64>().ok()?;
        if denominator == 0.0 {
            return None;
        }
        Some(numerator / denominator)
    } else {
        value.parse::<f64>().ok()
    }
}

pub fn parse_ffprobe_fps(payload: &Value) -> Option<f64> {
    let streams = payload.get("streams")?.as_array()?;
    let video = streams
        .iter()
        .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("video"))?;

    let avg_fps = video
        .get("avg_frame_rate")
        .and_then(Value::as_str)
        .and_then(parse_ffprobe_fraction);
    let r_fps = video
        .get("r_frame_rate")
        .and_then(Value::as_str)
        .and_then(parse_ffprobe_fraction);

    let nb_frames = video
        .get("nb_frames")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && *value != "N/A")
        .and_then(|value| value.parse::<f64>().ok());
    let duration = video
        .get("duration")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && *value != "N/A")
        .and_then(|value| value.parse::<f64>().ok());

    let fps_from_counts = match (nb_frames, duration) {
        (Some(frames), Some(duration)) if frames > 0.0 && duration > 0.0 => Some(frames / duration),
        _ => None,
    };

    [fps_from_counts, avg_fps, r_fps]
        .into_iter()
        .flatten()
        .find(|candidate| *candidate > 0.0 && *candidate <= 240.0)
}

pub fn analyze_video(video_path: &Path, ffprobe: &Path) -> VideoInfo {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,avg_frame_rate,r_frame_rate,nb_frames,duration",
            "-of",
            "json",
        ])
        .arg(video_path)
        .output();

    let output = match output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            return VideoInfo {
                path: video_path.to_path_buf(),
                fps: None,
                video_codec: None,
                audio_codec: None,
                interlaced: None,
                error: Some(format!(
                    "ffprobe a échoué: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )),
            };
        }
        Err(error) => {
            return VideoInfo {
                path: video_path.to_path_buf(),
                fps: None,
                video_codec: None,
                audio_codec: None,
                interlaced: None,
                error: Some(format!("ffprobe indisponible: {error}")),
            };
        }
    };

    let payload: Value = match serde_json::from_slice(&output.stdout) {
        Ok(payload) => payload,
        Err(error) => {
            return VideoInfo {
                path: video_path.to_path_buf(),
                fps: None,
                video_codec: None,
                audio_codec: None,
                interlaced: None,
                error: Some(format!("JSON ffprobe invalide: {error}")),
            };
        }
    };

    let fps = parse_ffprobe_fps(&payload);
    let mut video_codec = None;
    let mut audio_codec = None;
    if let Some(streams) = payload.get("streams").and_then(Value::as_array) {
        for stream in streams {
            match stream.get("codec_type").and_then(Value::as_str) {
                Some("video") if video_codec.is_none() => {
                    video_codec = stream
                        .get("codec_name")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                Some("audio") if audio_codec.is_none() => {
                    audio_codec = stream
                        .get("codec_name")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                _ => {}
            }
        }
    }

    let interlaced = probe_interlaced(video_path, ffprobe);
    VideoInfo {
        path: video_path.to_path_buf(),
        fps,
        video_codec,
        audio_codec,
        interlaced,
        error: None,
    }
}

pub fn probe_interlaced(video_path: &Path, ffprobe: &Path) -> Option<bool> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-read_intervals",
            &format!("%+{INTERLACE_PROBE_SECONDS}"),
            "-show_entries",
            "frame=interlaced_frame",
            "-of",
            "csv=p=0",
        ])
        .arg(video_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut flags = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.split(',').next().unwrap_or_default().trim());
    let mut has_flag = false;
    for flag in flags.by_ref() {
        has_flag = true;
        if flag == "1" {
            return Some(true);
        }
    }
    has_flag.then_some(false)
}

pub fn is_conformant(info: &VideoInfo, target_fps: f64) -> bool {
    if info.error.is_some() {
        return false;
    }
    if info
        .path
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| !value.eq_ignore_ascii_case("mp4"))
    {
        return false;
    }
    if info
        .video_codec
        .as_deref()
        .is_none_or(|codec| !codec.eq_ignore_ascii_case("h264"))
    {
        return false;
    }
    let Some(fps) = info.fps else {
        return false;
    };
    if info.interlaced == Some(true) {
        return false;
    }
    (fps - target_fps).abs() <= FPS_TOLERANCE
}

pub fn discover_videos(root: &Path, options: &Step1Options) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut candidates = Vec::new();
    let mut preserved = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let Ok(entry) = entry else {
            continue;
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy();
        if SKIP_NAME_TOKENS
            .iter()
            .any(|token| file_name.contains(token))
        {
            continue;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        if !extension
            .as_deref()
            .is_some_and(|extension| VIDEO_EXTENSIONS.contains(&extension))
        {
            continue;
        }
        let path = entry.path().to_path_buf();
        if is_preserved_media_with(
            &path,
            options.skip_preserved_mov,
            &options.preserved_mov_names,
            &options.ffprobe,
        ) {
            preserved.push(path);
        } else {
            candidates.push(path);
        }
    }
    candidates.sort();
    preserved.sort();
    (candidates, preserved)
}

pub fn normalize_video(
    info: &VideoInfo,
    options: &Step1Options,
    use_gpu: bool,
    logger: &RunLogger,
) -> bool {
    let video_path = &info.path;
    if let Some(error) = &info.error {
        logger.line(format!(
            "Normalisation impossible pour {}: {error}",
            video_path.display()
        ));
        return false;
    }
    if is_preserved_media_with(
        video_path,
        options.skip_preserved_mov,
        &options.preserved_mov_names,
        &options.ffprobe,
    ) {
        logger.line(format!(
            "Média .mov préservé ignoré par le worker: {}",
            video_path.display()
        ));
        return true;
    }

    let interlaced = info.interlaced == Some(true);
    let use_gpu = use_gpu && !interlaced;
    let worker_type = if use_gpu { "GPU" } else { "CPU" };
    let temp_output_path = with_suffix(video_path, TEMP_SUFFIX);

    let fps_label = info
        .fps
        .map(|fps| format!("{fps:.2}"))
        .unwrap_or_else(|| "Inconnu".into());
    logger.line(format!(
        "Normalisation ({worker_type}) démarrée pour {} (FPS={fps_label}, Audio={}{})",
        video_path.display(),
        info.audio_codec.as_deref().unwrap_or("None"),
        if interlaced {
            ", désentrelacement"
        } else {
            ""
        }
    ));

    let mut filters = Vec::new();
    if interlaced {
        filters.push(DEINTERLACE_FILTER.to_string());
    }
    let needs_fps_fix = info
        .fps
        .is_some_and(|fps| (fps - options.fps).abs() > FPS_TOLERANCE);
    if needs_fps_fix {
        filters.push(format!("fps={}", options.fps));
    }

    let mut command = Command::new(&options.ffmpeg);
    command.args(["-y", "-hide_banner"]);
    if use_gpu {
        command.args(["-hwaccel", "cuda"]);
    }
    command.arg("-i").arg(video_path);
    if !filters.is_empty() {
        command.arg("-vf").arg(filters.join(","));
    }
    if use_gpu {
        command.args([
            "-c:v",
            "h264_nvenc",
            "-preset",
            "p5",
            "-tune",
            "hq",
            "-cq",
            &options.crf.to_string(),
            "-pix_fmt",
            "yuv420p",
        ]);
    } else {
        command.args([
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            &options.crf.to_string(),
            "-pix_fmt",
            "yuv420p",
        ]);
    }
    if info
        .audio_codec
        .as_deref()
        .is_some_and(|codec| AUDIO_COPY_CODECS.contains(&codec.to_ascii_lowercase().as_str()))
    {
        command.args(["-c:a", "copy"]);
    } else {
        command.args(["-c:a", "aac", "-b:a", "192k"]);
    }
    command.arg(&temp_output_path);

    let result = command.output();
    match result {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            logger.line(format!(
                "Erreur FFmpeg ({worker_type}) pour {}.\nStderr: {}",
                video_path.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
            remove_if_exists(&temp_output_path);
            return false;
        }
        Err(error) => {
            logger.line(format!(
                "Erreur FFmpeg ({worker_type}) pour {}: {error}",
                video_path.display()
            ));
            remove_if_exists(&temp_output_path);
            return false;
        }
    }

    let final_destination = video_path.with_extension("mp4");
    if final_destination != *video_path && final_destination.exists() {
        logger.line(format!(
            "Collision de nom pour {} : {} existe déjà, aucun écrasement (fichier laissé en échec).",
            video_path.display(),
            final_destination.display()
        ));
        remove_if_exists(&temp_output_path);
        return false;
    }
    if final_destination != *video_path {
        remove_if_exists(video_path);
    }
    if let Err(error) = move_file(&temp_output_path, &final_destination) {
        logger.line(format!(
            "Déplacement impossible de {} vers {}: {error}",
            temp_output_path.display(),
            final_destination.display()
        ));
        remove_if_exists(&temp_output_path);
        return false;
    }
    logger.line(format!(
        "Succès ({worker_type}): {} normalisée (MP4/H.264/{:.1} fps).",
        video_path.display(),
        options.fps
    ));
    true
}

pub fn normalize_video_with_fallback(
    info: &VideoInfo,
    options: &Step1Options,
    logger: &RunLogger,
) -> bool {
    if !options.use_gpu {
        return normalize_video(info, options, false, logger);
    }
    if normalize_video(info, options, true, logger) {
        return true;
    }
    logger.line(format!(
        "Fallback CPU déclenché pour {}",
        info.path.display()
    ));
    normalize_video(info, options, false, logger)
}

pub fn normalize_videos_in(
    root: &Path,
    options: &Step1Options,
    logger: &RunLogger,
) -> NormalizationSummary {
    logger.line(format!(
        "Recherche de vidéos ({}) dans {}...",
        VIDEO_EXTENSIONS
            .iter()
            .map(|extension| format!(".{extension}"))
            .collect::<Vec<_>>()
            .join(", "),
        root.display()
    ));

    let (candidates, preserved) = discover_videos(root, options);
    for logo in &preserved {
        logger.line(format!(
            "--- Média .mov préservé (non normalisé, alpha/liste noire): {} ---",
            logo.display()
        ));
    }
    if !preserved.is_empty() {
        logger.line(format!(
            "{} média(s) .mov préservé(s) ignoré(s) par la normalisation (alpha conservé jusqu'à la finalisation).",
            preserved.len()
        ));
    }

    let analyzed = analyze_many(&candidates, options);
    let analysis_failure_count = analyzed.iter().filter(|info| info.error.is_some()).count();
    for info in analyzed.iter().filter(|info| info.error.is_some()) {
        logger.line(format!(
            "Analyse impossible, fichier laissé en l'état: {} ({})",
            info.path.display(),
            info.error.as_deref().unwrap_or_default()
        ));
    }

    let mut usable: Vec<VideoInfo> = analyzed
        .into_iter()
        .filter(|info| info.error.is_none())
        .collect();
    let mut conformant = Vec::new();
    if !options.force_normalize {
        let (skipped, remaining): (Vec<VideoInfo>, Vec<VideoInfo>) = usable
            .into_iter()
            .partition(|info| is_conformant(info, options.fps));
        conformant = skipped;
        usable = remaining;
        for info in &conformant {
            logger.line(format!("Déjà conforme, ignorée: {}", info.path.display()));
        }
    }

    let total = usable.len();
    let skipped = conformant.len() + preserved.len();
    logger.line(format!("TOTAL_VIDEOS_TO_PROCESS: {total}"));

    if total == 0 {
        logger.line("Aucune vidéo à normaliser.");
        return NormalizationSummary {
            total: 0,
            success: 0,
            failed: analysis_failure_count,
            skipped,
        };
    }

    logger.line(format!(
        "Lancement du pool de normalisation ({} workers max) pour {} vidéo(s).",
        options.gpu_workers, total
    ));

    let completed = AtomicUsize::new(0);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.gpu_workers.max(1))
        .build();
    let process = |info: &VideoInfo| {
        let index = completed.fetch_add(1, Ordering::SeqCst) + 1;
        let percent = index * 100 / total;
        logger.line(format!(
            "--- Normalisation ({index}/{total}): {} ---",
            info.path.display()
        ));
        logger.line(format!(
            "INTERNAL_PROGRESS: {index}/{total} vidéos ({percent}%) - {}",
            info.path.display()
        ));
        normalize_video_with_fallback(info, options, logger)
    };

    let success_count = match pool {
        Ok(pool) => pool.install(|| usable.par_iter().filter(|info| process(info)).count()),
        Err(_) => usable.iter().filter(|info| process(info)).count(),
    };

    let failures = analysis_failure_count + (total - success_count);
    logger.line(format!(
        "--- Normalisation terminée : {success_count}/{total} réussie(s) ---"
    ));
    NormalizationSummary {
        total,
        success: success_count,
        failed: failures,
        skipped,
    }
}

fn analyze_many(paths: &[PathBuf], options: &Step1Options) -> Vec<VideoInfo> {
    if paths.is_empty() {
        return Vec::new();
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(options.probe_workers.clamp(1, MAX_PROBE_WORKERS))
        .build();
    match pool {
        Ok(pool) => pool.install(|| {
            paths
                .par_iter()
                .map(|path| analyze_video(path, &options.ffprobe))
                .collect()
        }),
        Err(_) => paths
            .iter()
            .map(|path| analyze_video(path, &options.ffprobe))
            .collect(),
    }
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut candidate = path.to_path_buf();
    candidate.set_extension(suffix.trim_start_matches('.'));
    candidate
}

fn remove_if_exists(path: &Path) {
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn move_file(source: &Path, destination: &Path) -> io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            fs::copy(source, destination)?;
            fs::remove_file(source)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn fps_prefers_counted_frames() {
        let payload = json!({
            "streams": [{
                "codec_type": "video",
                "avg_frame_rate": "25/1",
                "r_frame_rate": "25/1",
                "nb_frames": "2318",
                "duration": "182.20"
            }]
        });
        let fps = parse_ffprobe_fps(&payload).unwrap();
        assert!(fps > 12.6 && fps < 12.8);
    }

    #[test]
    fn fps_falls_back_to_average_rate() {
        let payload = json!({
            "streams": [{
                "codec_type": "video",
                "avg_frame_rate": "25/1",
                "r_frame_rate": "50/1"
            }]
        });
        assert_eq!(parse_ffprobe_fps(&payload), Some(25.0));
    }

    #[test]
    fn invalid_fractions_are_rejected() {
        assert_eq!(parse_ffprobe_fraction(""), None);
        assert_eq!(parse_ffprobe_fraction("0/0"), None);
        assert_eq!(parse_ffprobe_fraction("25/0"), None);
        assert_eq!(parse_ffprobe_fraction("25/2"), Some(12.5));
    }

    fn info(path: &str, fps: f64, codec: &str, interlaced: bool) -> VideoInfo {
        VideoInfo {
            path: PathBuf::from(path),
            fps: Some(fps),
            video_codec: Some(codec.into()),
            audio_codec: None,
            interlaced: Some(interlaced),
            error: None,
        }
    }

    #[test]
    fn conformance_requires_mp4_h264_and_target_fps() {
        assert!(is_conformant(&info("clip.mp4", 25.0, "h264", false), 25.0));
        assert!(!is_conformant(&info("clip.mkv", 25.0, "h264", false), 25.0));
        assert!(!is_conformant(
            &info("clip.mp4", 25.0, "mpeg4", false),
            25.0
        ));
        assert!(!is_conformant(&info("clip.mp4", 30.0, "h264", false), 25.0));
        assert!(!is_conformant(&info("clip.mp4", 25.0, "h264", true), 25.0));
    }

    #[test]
    fn conformance_rejects_unknown_fps_and_errors() {
        let mut unknown = info("clip.mp4", 25.0, "h264", false);
        unknown.fps = None;
        assert!(!is_conformant(&unknown, 25.0));
        let mut failed = info("clip.mp4", 25.0, "h264", false);
        failed.error = Some("boom".into());
        assert!(!is_conformant(&failed, 25.0));
    }

    #[test]
    fn suffix_replaces_only_the_extension() {
        assert_eq!(
            with_suffix(Path::new("/tmp/clip.avi"), TEMP_SUFFIX),
            PathBuf::from("/tmp/clip.temp_normalize.mp4")
        );
    }

    #[cfg(unix)]
    fn fake_ffmpeg(temp: &tempfile::TempDir, exit_code: u8) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = temp.path().join("fake-ffmpeg");
        fs::write(
            &path,
            format!(
                "#!/bin/sh\nfor last; do :; done\nprintf converted > \"$last\"\nexit {exit_code}\n"
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn test_options(temp: &tempfile::TempDir, ffmpeg: PathBuf) -> Step1Options {
        Step1Options {
            source_dir: temp.path().join("downloads"),
            projects_dir: temp.path().join("projects"),
            logs_dir: temp.path().join("logs"),
            keyword: "Camille".into(),
            docs_subfolder: "docs".into(),
            delete_archive: false,
            normalize: true,
            force_normalize: false,
            fps: 25.0,
            use_gpu: false,
            crf: 28,
            gpu_workers: 1,
            probe_workers: 1,
            ffmpeg,
            ffprobe: PathBuf::from("/nonexistent/ffprobe"),
            unrar: PathBuf::from("/nonexistent/unrar"),
            skip_preserved_mov: true,
            preserved_mov_names: vec!["m6+.mov".into()],
            log_file: None,
            lock_path: None,
            max_extracted_bytes: 100 * 1024 * 1024 * 1024,
            max_compression_ratio: 200,
        }
    }

    #[cfg(unix)]
    #[test]
    fn normalize_replaces_the_source_and_removes_the_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let video = temp.path().join("clip.avi");
        fs::write(&video, b"source").unwrap();
        let options = test_options(&temp, fake_ffmpeg(&temp, 0));
        let info = info(video.to_str().unwrap(), 30.0, "mpeg4", false);
        let logger = RunLogger::new(None);
        assert!(normalize_video(&info, &options, false, &logger));
        assert!(temp.path().join("clip.mp4").exists());
        assert!(!video.exists());
        assert!(!temp.path().join("clip.temp_normalize.mp4").exists());
    }

    #[cfg(unix)]
    #[test]
    fn normalize_never_overwrites_an_existing_mp4() {
        let temp = tempfile::tempdir().unwrap();
        let video = temp.path().join("clip.avi");
        fs::write(&video, b"source").unwrap();
        fs::write(temp.path().join("clip.mp4"), b"precious").unwrap();
        let options = test_options(&temp, fake_ffmpeg(&temp, 0));
        let info = info(video.to_str().unwrap(), 30.0, "mpeg4", false);
        let logger = RunLogger::new(None);
        assert!(!normalize_video(&info, &options, false, &logger));
        assert_eq!(fs::read(temp.path().join("clip.mp4")).unwrap(), b"precious");
        assert!(video.exists());
        assert!(!temp.path().join("clip.temp_normalize.mp4").exists());
    }

    #[cfg(unix)]
    #[test]
    fn ffmpeg_failure_removes_the_temp_file() {
        let temp = tempfile::tempdir().unwrap();
        let video = temp.path().join("clip.avi");
        fs::write(&video, b"source").unwrap();
        let options = test_options(&temp, fake_ffmpeg(&temp, 1));
        let info = info(video.to_str().unwrap(), 30.0, "mpeg4", false);
        let logger = RunLogger::new(None);
        assert!(!normalize_video(&info, &options, false, &logger));
        assert!(video.exists());
        assert!(!temp.path().join("clip.temp_normalize.mp4").exists());
    }

    #[test]
    fn discovery_separates_preserved_mov_and_skips_temp_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let docs = temp.path().join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("ready.mp4"), b"video").unwrap();
        fs::write(docs.join("clip.avi"), b"video").unwrap();
        fs::write(docs.join("logo.mov"), b"video").unwrap();
        fs::write(docs.join("m6+.mov"), b"video").unwrap();
        fs::write(docs.join("clip.temp_normalize.mp4"), b"partial").unwrap();
        let options = test_options(&temp, PathBuf::from("ffmpeg"));
        let (candidates, preserved) = discover_videos(&docs, &options);
        let candidate_names: Vec<_> = candidates
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        let preserved_names: Vec<_> = preserved
            .iter()
            .filter_map(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(candidate_names, vec!["clip.avi", "ready.mp4"]);
        assert_eq!(preserved_names, vec!["logo.mov", "m6+.mov"]);
    }
}
