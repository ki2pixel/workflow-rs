//! Évaluation temporaire D-058 sur le corpus GOOD complet (2 609 segments).
//!
//! Le corpus est lu uniquement. Les sorties vont sous `--out` (défaut /tmp).

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde_json::json;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;
use workflow_schema::round::round_to;
use workflow_steps::post::a1;
use workflow_steps::post::detect::{probe_video, Crop};
use workflow_steps::post::ecproj;
use workflow_steps::post::model::Action;

const DEFAULT_CORPUS: &str = "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2";
const DEFAULT_OUT: &str = "/tmp/opencode/a1-letterbox-full-eval";

#[derive(Debug, Clone)]
struct Project {
    folder: String,
    stem: String,
    project: PathBuf,
    video: PathBuf,
}

#[derive(Debug, Clone)]
struct SegmentEval {
    project: String,
    folder: String,
    stem: String,
    segment: String,
    start: f64,
    duration: f64,
    video: String,
    false_action: String,
    true_action: String,
    false_scale: Option<f64>,
    true_scale: Option<f64>,
    refined: bool,
    cd16_top: Option<i64>,
    cd16_bottom: Option<i64>,
    native_top: Option<u32>,
    native_bottom: Option<u32>,
    cd2_top: Option<u32>,
    cd2_bottom: Option<u32>,
    cd16_err_px: Option<f64>,
    native_err_px: Option<f64>,
    gain_px: Option<f64>,
}

fn main() -> Result<()> {
    let mut corpus = PathBuf::from(DEFAULT_CORPUS);
    let mut out = PathBuf::from(DEFAULT_OUT);
    let mut ffmpeg = PathBuf::from("ffmpeg");
    let mut ffprobe = PathBuf::from("ffprobe");
    let mut limit = 0usize;
    let mut only = String::new();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" => {
                index += 1;
                corpus = PathBuf::from(args.get(index).context("--corpus sans valeur")?);
            }
            "--out" => {
                index += 1;
                out = PathBuf::from(args.get(index).context("--out sans valeur")?);
            }
            "--ffmpeg" => {
                index += 1;
                ffmpeg = PathBuf::from(args.get(index).context("--ffmpeg sans valeur")?);
            }
            "--ffprobe" => {
                index += 1;
                ffprobe = PathBuf::from(args.get(index).context("--ffprobe sans valeur")?);
            }
            "--limit" => {
                index += 1;
                limit = args
                    .get(index)
                    .context("--limit sans valeur")?
                    .parse()
                    .context("--limit invalide")?;
            }
            "--only" => {
                index += 1;
                only = args.get(index).context("--only sans valeur")?.clone();
            }
            other => bail!("argument inconnu: {other}"),
        }
        index += 1;
    }
    if !out.starts_with("/tmp") {
        bail!("--out doit être sous /tmp : {}", out.display());
    }
    fs::create_dir_all(&out).with_context(|| format!("création de {}", out.display()))?;
    let mut projects = discover_projects(&corpus)?;
    if !only.is_empty() {
        projects.retain(|project| format!("{}/{}", project.folder, project.stem).contains(&only));
    }
    if limit > 0 {
        projects.truncate(limit);
    }
    eprintln!("{} projets bruts", projects.len());
    let mut dimensions: HashMap<PathBuf, (u32, u32)> = HashMap::new();
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut total_false_ms = 0.0;
    let mut total_true_ms = 0.0;
    let mut refined = 0usize;
    let mut false_applies = 0usize;
    let mut false_reviews = 0usize;
    let mut false_noops = 0usize;
    let mut true_applies = 0usize;
    let mut true_reviews = 0usize;
    let mut true_noops = 0usize;
    let mut decision_changes = 0usize;

    for (project_index, project) in projects.iter().enumerate() {
        let doc = match ecproj::load(&project.project) {
            Ok(doc) => doc,
            Err(error) => {
                errors.push(json!({"project": project.project.display().to_string(), "error": format!("{error:#}")}));
                continue;
            }
        };
        let meta = match ecproj::comp_meta(&doc) {
            Ok(meta) => meta,
            Err(error) => {
                errors.push(json!({"project": project.project.display().to_string(), "error": format!("{error:#}")}));
                continue;
            }
        };
        let segments = match ecproj::segments(&doc) {
            Ok(segments) => segments,
            Err(error) => {
                errors.push(json!({"project": project.project.display().to_string(), "error": format!("{error:#}")}));
                continue;
            }
        };
        if !project.video.is_file() {
            errors.push(json!({"project": project.project.display().to_string(), "error": format!("vidéo introuvable: {}", project.video.display())}));
            continue;
        }
        let (source_width, source_height) = match dimensions.get(&project.video) {
            Some(dim) => *dim,
            None => match probe_video(&ffprobe, &project.video) {
                Ok(specs) => {
                    let dim = (specs.width, specs.height);
                    dimensions.insert(project.video.clone(), dim);
                    dim
                }
                Err(error) => {
                    errors.push(json!({"project": project.project.display().to_string(), "error": format!("ffprobe: {error:#}")}));
                    continue;
                }
            },
        };
        let false_options = a1::A1Options::default();
        let true_options = a1::A1Options {
            refine_native: true,
            ..a1::A1Options::default()
        };
        let (false_decisions, false_ms, true_decisions, true_ms) = if project_index % 2 == 0 {
            let started = Instant::now();
            let false_decisions = a1::decide_a1(
                &segments,
                Some(&project.video),
                &ffmpeg,
                &meta,
                source_width,
                source_height,
                &false_options,
            );
            let false_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            let true_decisions = a1::decide_a1(
                &segments,
                Some(&project.video),
                &ffmpeg,
                &meta,
                source_width,
                source_height,
                &true_options,
            );
            let true_ms = started.elapsed().as_secs_f64() * 1000.0;
            (false_decisions, false_ms, true_decisions, true_ms)
        } else {
            let started = Instant::now();
            let true_decisions = a1::decide_a1(
                &segments,
                Some(&project.video),
                &ffmpeg,
                &meta,
                source_width,
                source_height,
                &true_options,
            );
            let true_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            let false_decisions = a1::decide_a1(
                &segments,
                Some(&project.video),
                &ffmpeg,
                &meta,
                source_width,
                source_height,
                &false_options,
            );
            let false_ms = started.elapsed().as_secs_f64() * 1000.0;
            (false_decisions, false_ms, true_decisions, true_ms)
        };
        total_false_ms += false_ms;
        total_true_ms += true_ms;
        let false_by_target: HashMap<&str, &workflow_steps::post::model::Decision> =
            false_decisions
                .iter()
                .filter(|decision| decision.code == "A1" && decision.target != "comp")
                .map(|decision| (decision.target.as_str(), decision))
                .collect();
        let true_by_target: HashMap<&str, &workflow_steps::post::model::Decision> = true_decisions
            .iter()
            .filter(|decision| decision.code == "A1" && decision.target != "comp")
            .map(|decision| (decision.target.as_str(), decision))
            .collect();
        for segment in &segments {
            let Some(false_decision) = false_by_target.get(segment.name.as_str()) else {
                continue;
            };
            let Some(true_decision) = true_by_target.get(segment.name.as_str()) else {
                continue;
            };
            match false_decision.action {
                Action::Apply => false_applies += 1,
                Action::Review => false_reviews += 1,
                Action::Noop => false_noops += 1,
            }
            match true_decision.action {
                Action::Apply => true_applies += 1,
                Action::Review => true_reviews += 1,
                Action::Noop => true_noops += 1,
            }
            if false_decision.action != true_decision.action {
                decision_changes += 1;
            }
            let is_refined = true_decision
                .details
                .get("native_refined")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            let duration = (segment.duration() - 0.1).clamp(0.3, 4.0);
            let cd16_top = false_decision
                .details
                .get("bars")
                .and_then(|bars| bars.get("top"))
                .and_then(|value| value.as_i64());
            let cd16_bottom = false_decision
                .details
                .get("bars")
                .and_then(|bars| bars.get("bottom"))
                .and_then(|value| value.as_i64());
            let native_top = true_decision
                .details
                .get("native")
                .and_then(|native| native.get("top"))
                .and_then(|value| value.as_u64())
                .map(|value| value as u32);
            let native_bottom = true_decision
                .details
                .get("native")
                .and_then(|native| native.get("bottom"))
                .and_then(|value| value.as_u64())
                .map(|value| value as u32);
            let (cd2_top, cd2_bottom) = if is_refined {
                match cropdetect_round(&ffmpeg, &project.video, segment.start + 0.05, duration, 2) {
                    Ok(crops) => summarize_cropdetect(&crops, source_height),
                    Err(_) => (None, None),
                }
            } else {
                (None, None)
            };
            let cd16_err_px = match (cd16_top, cd16_bottom, cd2_top, cd2_bottom) {
                (Some(top), Some(bottom), Some(ref_top), Some(ref_bottom)) => Some(
                    ((top - ref_top as i64).abs() as f64
                        + (bottom - ref_bottom as i64).abs() as f64)
                        / 2.0,
                ),
                _ => None,
            };
            let native_err_px = match (native_top, native_bottom, cd2_top, cd2_bottom) {
                (Some(top), Some(bottom), Some(ref_top), Some(ref_bottom)) => Some(
                    ((top as i64 - ref_top as i64).abs() as f64
                        + (bottom as i64 - ref_bottom as i64).abs() as f64)
                        / 2.0,
                ),
                _ => None,
            };
            let gain_px = match (cd16_err_px, native_err_px) {
                (Some(cd16), Some(native)) => Some(cd16 - native),
                _ => None,
            };
            if is_refined {
                refined += 1;
            }
            rows.push(SegmentEval {
                project: format!("{}/{}", project.folder, project.stem),
                folder: project.folder.clone(),
                stem: project.stem.clone(),
                segment: segment.name.clone(),
                start: round_to(segment.start + 0.05, 3),
                duration: round_to(duration, 3),
                video: project.video.display().to_string(),
                false_action: false_decision.action.as_str().into(),
                true_action: true_decision.action.as_str().into(),
                false_scale: false_decision
                    .details
                    .get("scale_target")
                    .and_then(|value| value.as_f64()),
                true_scale: true_decision
                    .details
                    .get("scale_target")
                    .and_then(|value| value.as_f64()),
                refined: is_refined,
                cd16_top,
                cd16_bottom,
                native_top,
                native_bottom,
                cd2_top,
                cd2_bottom,
                cd16_err_px,
                native_err_px,
                gain_px,
            });
        }
        eprintln!(
            "[{}/{}] {}/{}: {} segments, false {:.1}s, true {:.1}s",
            project_index + 1,
            projects.len(),
            project.folder,
            project.stem,
            segments.len(),
            false_ms / 1000.0,
            true_ms / 1000.0
        );
    }

    let refined_rows: Vec<&SegmentEval> = rows.iter().filter(|row| row.refined).collect();
    let gains: Vec<f64> = refined_rows.iter().filter_map(|row| row.gain_px).collect();
    let cd16_errors: Vec<f64> = refined_rows
        .iter()
        .filter_map(|row| row.cd16_err_px)
        .collect();
    let native_errors: Vec<f64> = refined_rows
        .iter()
        .filter_map(|row| row.native_err_px)
        .collect();
    let extra_ms = total_true_ms - total_false_ms;
    let summary = json!({
        "corpus": corpus.display().to_string(),
        "projects": projects.len(),
        "segments": rows.len(),
        "errors": errors,
        "false": {
            "apply": false_applies,
            "review": false_reviews,
            "noop": false_noops,
            "a1_ms": round_to(total_false_ms, 1),
            "a1_s": round_to(total_false_ms / 1000.0, 2)
        },
        "true": {
            "apply": true_applies,
            "review": true_reviews,
            "noop": true_noops,
            "a1_ms": round_to(total_true_ms, 1),
            "a1_s": round_to(total_true_ms / 1000.0, 2)
        },
        "refined_applies": refined,
        "false_applies": false_applies,
        "decision_changes": decision_changes,
        "pixel": {
            "n": refined_rows.len(),
            "n_with_reference": cd16_errors.len(),
            "mae_cd16_px": round_to(mean(&cd16_errors), 3),
            "mae_native_px": round_to(mean(&native_errors), 3),
            "mean_gain_px": round_to(mean(&gains), 3),
            "median_gain_px": round_to(median(&gains), 3),
            "max_gain_px": round_to(gains.iter().copied().fold(f64::NEG_INFINITY, f64::max), 3),
            "min_gain_px": round_to(gains.iter().copied().fold(f64::INFINITY, f64::min), 3),
            "gain_positive": gains.iter().filter(|gain| **gain > 0.0).count(),
            "gain_zero": gains.iter().filter(|gain| **gain == 0.0).count(),
            "gain_negative": gains.iter().filter(|gain| **gain < 0.0).count()
        },
        "time": {
            "extra_ms": round_to(extra_ms, 1),
            "extra_s": round_to(extra_ms / 1000.0, 2),
            "extra_pct": round_to(extra_ms / total_false_ms.max(0.001) * 100.0, 2),
            "extra_ms_per_refined": round_to(extra_ms / refined.max(1) as f64, 1)
        }
    });
    let json_path = out.join("summary.json");
    fs::write(&json_path, serde_json::to_string_pretty(&summary)?)
        .with_context(|| format!("écriture de {}", json_path.display()))?;
    let csv_path = out.join("segments.csv");
    write_segments_csv(&csv_path, &rows)?;
    let refined_csv_path = out.join("refined.csv");
    write_refined_csv(&refined_csv_path, &refined_rows)?;
    println!("{}", serde_json::to_string_pretty(&summary)?);
    println!("JSON  : {}", json_path.display());
    println!("CSV   : {}", csv_path.display());
    println!("Raffinés : {}", refined_csv_path.display());
    Ok(())
}

fn discover_projects(corpus: &Path) -> Result<Vec<Project>> {
    if !corpus.is_dir() {
        bail!("corpus introuvable: {}", corpus.display());
    }
    let mut projects = Vec::new();
    let mut folders: Vec<PathBuf> = fs::read_dir(corpus)
        .with_context(|| format!("lecture de {}", corpus.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_dir())
        .collect();
    folders.sort();
    for folder in folders {
        let projets = folder.join("projets");
        if !projets.is_dir() {
            continue;
        }
        let mut raws: Vec<PathBuf> = fs::read_dir(&projets)
            .with_context(|| format!("lecture de {}", projets.display()))?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.is_file()
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.ends_with("_9x16.ecproj")
                                && !name.ends_with("_assist.ecproj")
                                && !name.ends_with("_assist_GOOD.ecproj")
                        })
            })
            .collect();
        raws.sort();
        for raw in raws {
            let file_name = raw
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let stem = file_name.trim_end_matches("_9x16.ecproj").to_string();
            let docs = folder.join("docs");
            let video = resolve_video(&docs, &raw, &stem);
            projects.push(Project {
                folder: folder
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default()
                    .to_string(),
                stem,
                project: raw,
                video,
            });
        }
    }
    Ok(projects)
}

fn resolve_video(docs: &Path, project: &Path, stem: &str) -> PathBuf {
    let doc = ecproj::load(project).ok();
    let mut sources: Vec<String> = doc
        .as_ref()
        .and_then(|doc| ecproj::segments(doc).ok())
        .unwrap_or_default()
        .iter()
        .filter_map(|segment| segment.source.clone())
        .collect();
    sources.sort();
    sources.dedup();
    if sources.len() == 1 {
        let source = &sources[0];
        let direct = docs.join(source);
        if direct.is_file() {
            return direct;
        }
        let source_stem = Path::new(source)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(stem);
        if let Some(video) = find_video(docs, source_stem) {
            return video;
        }
    }
    find_video(docs, stem).unwrap_or_else(|| docs.join(format!("{stem}.mp4")))
}

fn find_video(docs: &Path, stem: &str) -> Option<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(docs)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file())
        .collect();
    entries.sort();
    entries.into_iter().find(|path| {
        path.file_stem().and_then(|value| value.to_str()) == Some(stem)
            && path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    ["mp4", "mov", "avi", "mkv", "webm"]
                        .iter()
                        .any(|candidate| extension.eq_ignore_ascii_case(candidate))
                })
    })
}

fn cropdetect_round(
    ffmpeg: &Path,
    video: &Path,
    start: f64,
    duration: f64,
    round: u32,
) -> Result<Vec<Crop>> {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-ss", &format!("{start:.3}"), "-i"])
        .arg(video)
        .args([
            "-t",
            &format!("{duration:.3}"),
            "-vf",
            &format!("cropdetect=24:{round}:0"),
            "-f",
            "null",
            "-",
        ])
        .output()
        .with_context(|| format!("ffmpeg cropdetect introuvable pour {}", video.display()))?;
    if !output.status.success() {
        bail!(
            "ffmpeg cropdetect: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let regex = crop_regex();
    Ok(stderr
        .lines()
        .filter_map(|line| {
            let captures = regex.captures(line)?;
            Some(Crop {
                w: captures.get(1)?.as_str().parse().ok()?,
                h: captures.get(2)?.as_str().parse().ok()?,
                x: captures.get(3)?.as_str().parse().ok()?,
                y: captures.get(4)?.as_str().parse().ok()?,
            })
        })
        .collect())
}

fn crop_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"crop=(\d+):(\d+):(\d+):(\d+)").expect("regex cropdetect"))
}

fn summarize_cropdetect(crops: &[Crop], source_height: u32) -> (Option<u32>, Option<u32>) {
    if crops.is_empty() {
        return (None, None);
    }
    let heights: Vec<u32> = crops.iter().map(|crop| crop.h).collect();
    let active = round_to(percentile(&heights, 0.10), 0) as u32;
    let mut counts: Vec<(Crop, usize)> = Vec::new();
    for crop in crops {
        if let Some(entry) = counts.iter_mut().find(|(candidate, _)| candidate == crop) {
            entry.1 += 1;
        } else {
            counts.push((*crop, 1));
        }
    }
    let mut dominant = counts[0];
    let mut dominant_key = (
        (dominant.0.h as i64 - active as i64).abs(),
        -(dominant.1 as i64),
    );
    for entry in counts.iter().skip(1) {
        let key = ((entry.0.h as i64 - active as i64).abs(), -(entry.1 as i64));
        if key < dominant_key {
            dominant = *entry;
            dominant_key = key;
        }
    }
    let crop = dominant.0;
    (
        Some(crop.y),
        Some(source_height.saturating_sub(crop.y + crop.h)),
    )
}

fn percentile(values: &[u32], quantile: f64) -> f64 {
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let index = round_to((ordered.len().saturating_sub(1)) as f64 * quantile, 0) as usize;
    ordered[index.min(ordered.len() - 1)] as f64
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut ordered = values.to_vec();
    ordered.sort_by(f64::total_cmp);
    let middle = ordered.len() / 2;
    if ordered.len() % 2 == 0 {
        (ordered[middle - 1] + ordered[middle]) / 2.0
    } else {
        ordered[middle]
    }
}

fn write_segments_csv(path: &Path, rows: &[SegmentEval]) -> Result<()> {
    let mut out = String::new();
    writeln!(
        out,
        "project,folder,stem,segment,start,duration,video,false_action,true_action,\
         false_scale,true_scale,refined,cd16_top,cd16_bottom,native_top,native_bottom,\
         cd2_top,cd2_bottom,cd16_err_px,native_err_px,gain_px"
    )?;
    for row in rows {
        writeln!(
            out,
            "{},{},{},{},{:.3},{:.3},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(&row.project),
            csv_escape(&row.folder),
            csv_escape(&row.stem),
            csv_escape(&row.segment),
            row.start,
            row.duration,
            csv_escape(&row.video),
            row.false_action,
            row.true_action,
            row.false_scale
                .map_or_else(String::new, |value| format!("{value:.1}")),
            row.true_scale
                .map_or_else(String::new, |value| format!("{value:.1}")),
            row.refined,
            row.cd16_top
                .map_or_else(String::new, |value| value.to_string()),
            row.cd16_bottom
                .map_or_else(String::new, |value| value.to_string()),
            row.native_top
                .map_or_else(String::new, |value| value.to_string()),
            row.native_bottom
                .map_or_else(String::new, |value| value.to_string()),
            row.cd2_top
                .map_or_else(String::new, |value| value.to_string()),
            row.cd2_bottom
                .map_or_else(String::new, |value| value.to_string()),
            row.cd16_err_px
                .map_or_else(String::new, |value| format!("{value:.3}")),
            row.native_err_px
                .map_or_else(String::new, |value| format!("{value:.3}")),
            row.gain_px
                .map_or_else(String::new, |value| format!("{value:.3}")),
        )?;
    }
    fs::write(path, out).with_context(|| format!("écriture de {}", path.display()))?;
    Ok(())
}

fn write_refined_csv(path: &Path, rows: &[&SegmentEval]) -> Result<()> {
    let mut out = String::new();
    writeln!(
        out,
        "project,segment,start,duration,false_scale,true_scale,cd16_top,cd16_bottom,\
         native_top,native_bottom,cd2_top,cd2_bottom,cd16_err_px,native_err_px,gain_px"
    )?;
    for row in rows {
        writeln!(
            out,
            "{},{},{:.3},{:.3},{},{},{},{},{},{},{},{},{},{},{}",
            csv_escape(&row.project),
            csv_escape(&row.segment),
            row.start,
            row.duration,
            row.false_scale
                .map_or_else(String::new, |value| format!("{value:.1}")),
            row.true_scale
                .map_or_else(String::new, |value| format!("{value:.1}")),
            row.cd16_top
                .map_or_else(String::new, |value| value.to_string()),
            row.cd16_bottom
                .map_or_else(String::new, |value| value.to_string()),
            row.native_top
                .map_or_else(String::new, |value| value.to_string()),
            row.native_bottom
                .map_or_else(String::new, |value| value.to_string()),
            row.cd2_top
                .map_or_else(String::new, |value| value.to_string()),
            row.cd2_bottom
                .map_or_else(String::new, |value| value.to_string()),
            row.cd16_err_px
                .map_or_else(String::new, |value| format!("{value:.3}")),
            row.native_err_px
                .map_or_else(String::new, |value| format!("{value:.3}")),
            row.gain_px
                .map_or_else(String::new, |value| format!("{value:.3}")),
        )?;
    }
    fs::write(path, out).with_context(|| format!("écriture de {}", path.display()))?;
    Ok(())
}

fn csv_escape(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\n' | '\r'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}
