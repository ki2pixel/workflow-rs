//! Stage 0 — contrôles déterministes de l'export final.
//!
//! Le stage ne juge pas le contenu éditorial. Il vérifie les specs, la durée
//! face au `.ecproj`, la structure de timeline, le template A6 et les
//! séquences noires ou figées. Les fondus, photos fixes et flash-cuts sont
//! stockés comme preuves, pas condamnés : le corpus GOOD en contient.

use crate::post::detect;
use crate::post::ecproj;
use crate::qa::model::{
    DefectType, Interval, ProjectStructure, QaCheck, QaFinding, QaProjectMeta, QaSpecs, QaStatus,
    Severity, Stage0Evidence, Stage0Thresholds, Timings,
};
use anyhow::{bail, Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Instant;
use workflow_schema::round::round_to;

#[derive(Debug, Clone)]
pub struct Stage0Options {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
    pub template_prefix: String,
    pub thresholds: Stage0Thresholds,
}

impl Default for Stage0Options {
    fn default() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            template_prefix: crate::post::a6::DEFAULT_PREFIX.into(),
            thresholds: Stage0Thresholds::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stage0Result {
    pub specs: QaSpecs,
    pub project_meta: Option<QaProjectMeta>,
    pub project_structure: Option<ProjectStructure>,
    pub checks: Vec<QaCheck>,
    pub findings: Vec<QaFinding>,
    pub evidence: Stage0Evidence,
    pub timings: Timings,
}

pub fn run_stage0(
    video: &Path,
    project: Option<&Path>,
    options: &Stage0Options,
) -> Result<Stage0Result> {
    let started = Instant::now();
    let mut timings = Timings::default();

    let probe_started = Instant::now();
    let specs = probe_specs(&options.ffprobe, video)?;
    timings.ffprobe_ms = elapsed_ms(probe_started);

    let mut checks = Vec::new();
    let mut findings = Vec::new();
    let (spec_checks, spec_findings) = check_specs(&specs);
    checks.extend(spec_checks);
    findings.extend(spec_findings);

    let black_started = Instant::now();
    let (black_intervals, freeze_intervals) =
        run_black_freeze(&options.ffmpeg, video, specs.duration_s)?;
    timings.black_freeze_ms = elapsed_ms(black_started);

    let (bf_checks, bf_findings) =
        check_black_freeze(&black_intervals, &freeze_intervals, &options.thresholds);
    checks.extend(bf_checks);
    findings.extend(bf_findings);

    let mut project_meta = None;
    let mut project_structure = None;
    let project_started = Instant::now();
    if let Some(project) = project {
        let project_check = check_project(
            project,
            &specs,
            video,
            &options.template_prefix,
            &options.thresholds,
        )?;
        checks.extend(project_check.checks);
        findings.extend(project_check.findings);
        project_meta = project_check.meta;
        project_structure = project_check.structure;
    } else {
        checks.push(QaCheck::not_applicable(
            "project_readable",
            "aucun .ecproj fourni",
        ));
    }
    timings.project_ms = elapsed_ms(project_started);
    timings.total_ms = elapsed_ms(started);

    Ok(Stage0Result {
        specs,
        project_meta,
        project_structure,
        checks,
        findings,
        evidence: Stage0Evidence {
            black_intervals,
            freeze_intervals,
            thresholds: options.thresholds.clone(),
        },
        timings,
    })
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn probe_specs(ffprobe: &Path, video: &Path) -> Result<QaSpecs> {
    let video_specs = detect::probe_video(ffprobe, video)?;
    let audio_codec = probe_audio_codec(ffprobe, video)?;
    Ok(QaSpecs {
        width: video_specs.width,
        height: video_specs.height,
        fps: video_specs.fps,
        video_codec: video_specs.codec,
        audio_codec,
        duration_s: video_specs.duration_s,
        bit_rate: video_specs.bit_rate,
        size_bytes: video_specs.size_bytes,
    })
}

fn probe_audio_codec(ffprobe: &Path, video: &Path) -> Result<String> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(video)
        .output()
        .with_context(|| format!("ffprobe audio en échec pour {}", video.display()))?;
    if !output.status.success() {
        return Ok("none".into());
    }
    let codec = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if codec.is_empty() {
        "none".into()
    } else {
        codec
    })
}

pub fn check_specs(specs: &QaSpecs) -> (Vec<QaCheck>, Vec<QaFinding>) {
    let mut checks = Vec::new();
    let mut findings = Vec::new();
    let mut mismatch = false;

    if specs.width != 1080 || specs.height != 1920 {
        mismatch = true;
        checks.push(QaCheck::fail(
            "resolution",
            format!("{}x{} au lieu de 1080x1920", specs.width, specs.height),
        ));
    } else {
        checks.push(QaCheck::pass("resolution", "1080x1920"));
    }

    if (specs.fps - 25.0).abs() > 0.01 {
        mismatch = true;
        checks.push(QaCheck::fail(
            "fps",
            format!("{} fps au lieu de 25", specs.fps),
        ));
    } else {
        checks.push(QaCheck::pass("fps", "25 fps"));
    }

    if specs.video_codec != "h264" {
        mismatch = true;
        checks.push(QaCheck::fail(
            "video_codec",
            format!("{} au lieu de h264", specs.video_codec),
        ));
    } else {
        checks.push(QaCheck::pass("video_codec", "h264"));
    }

    if specs.audio_codec != "aac" {
        checks.push(QaCheck::warn(
            "audio_codec",
            format!("{} au lieu de aac", specs.audio_codec),
        ));
    } else {
        checks.push(QaCheck::pass("audio_codec", "aac"));
    }

    if specs.bit_rate > 0 && !(9_000_000..=11_500_000).contains(&specs.bit_rate) {
        checks.push(QaCheck::info(
            "bitrate",
            format!(
                "{:.2} Mbit/s hors plage 9,0–11,5",
                specs.bit_rate as f64 / 1e6
            ),
        ));
    } else {
        let detail = if specs.bit_rate > 0 {
            format!("{:.2} Mbit/s", specs.bit_rate as f64 / 1e6)
        } else {
            "non mesuré".into()
        };
        checks.push(QaCheck::pass("bitrate", detail));
    }

    if mismatch {
        findings.push(QaFinding::new(
            DefectType::SpecMismatch,
            0.0,
            None,
            Severity::High,
            1.0,
            "stage0",
            "specs de sortie non conformes",
        ));
    }
    (checks, findings)
}

fn run_black_freeze(
    ffmpeg: &Path,
    video: &Path,
    duration_s: f64,
) -> Result<(Vec<Interval>, Vec<Interval>)> {
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-nostats",
            "-v",
            "info",
            "-threads",
            "4",
            "-i",
        ])
        .arg(video)
        .args([
            "-vf",
            "blackdetect=d=0.2:pix_th=0.10,freezedetect=n=-60dB:d=1.0",
            "-an",
            "-f",
            "null",
            "-",
        ])
        .output()
        .with_context(|| format!("ffmpeg blackdetect en échec pour {}", video.display()))?;
    if !output.status.success() {
        bail!(
            "ffmpeg blackdetect: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Ok(parse_black_freeze(&stderr, duration_s))
}

pub fn parse_black_freeze(log: &str, duration_s: f64) -> (Vec<Interval>, Vec<Interval>) {
    let blacks = black_regex()
        .captures_iter(log)
        .filter_map(|captures| {
            let start = captures.get(1)?.as_str().parse::<f64>().ok()?;
            let end = captures.get(2)?.as_str().parse::<f64>().ok()?;
            let duration = captures.get(3)?.as_str().parse::<f64>().ok()?;
            Some(Interval {
                start: round_to(start, 3),
                end: Some(round_to(end, 3)),
                duration: round_to(duration, 3),
            })
        })
        .collect::<Vec<_>>();

    let starts = freeze_start_regex()
        .captures_iter(log)
        .filter_map(|captures| captures.get(1)?.as_str().parse::<f64>().ok())
        .collect::<Vec<_>>();
    let ends = freeze_end_regex()
        .captures_iter(log)
        .filter_map(|captures| captures.get(1)?.as_str().parse::<f64>().ok())
        .collect::<Vec<_>>();
    let durations = freeze_duration_regex()
        .captures_iter(log)
        .filter_map(|captures| captures.get(1)?.as_str().parse::<f64>().ok())
        .collect::<Vec<_>>();

    let freezes = starts
        .iter()
        .enumerate()
        .map(|(index, start)| {
            let end = ends
                .get(index)
                .copied()
                .or_else(|| durations.get(index).map(|duration| start + duration));
            let duration = durations
                .get(index)
                .copied()
                .or_else(|| end.map(|end| (end - start).max(0.0)))
                .unwrap_or_else(|| (duration_s - start).max(0.0));
            Interval {
                start: round_to(*start, 3),
                end: end.map(|end| round_to(end, 3)),
                duration: round_to(duration.max(0.0), 3),
            }
        })
        .collect::<Vec<_>>();

    (blacks, freezes)
}

fn black_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"black_start:\s*([0-9.]+)\s+black_end:\s*([0-9.]+)\s+black_duration:\s*([0-9.]+)",
        )
        .expect("regex blackdetect")
    })
}

fn freeze_start_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"freeze_start:\s*([0-9.]+)").expect("regex freeze_start"))
}

fn freeze_end_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"freeze_end:\s*([0-9.]+)").expect("regex freeze_end"))
}

fn freeze_duration_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"freeze_duration:\s*([0-9.]+)").expect("regex freeze_duration"))
}

pub fn check_black_freeze(
    blacks: &[Interval],
    freezes: &[Interval],
    thresholds: &Stage0Thresholds,
) -> (Vec<QaCheck>, Vec<QaFinding>) {
    let mut checks = Vec::new();
    let mut findings = Vec::new();

    let max_black = blacks
        .iter()
        .map(|interval| interval.duration)
        .fold(0.0_f64, f64::max);
    if blacks.is_empty() {
        checks.push(QaCheck::pass("black_frames", "aucune"));
    } else {
        let status = if max_black >= thresholds.black_finding_s {
            QaStatus::Review
        } else {
            QaStatus::Info
        };
        checks.push(QaCheck::new(
            "black_frames",
            status,
            format!(
                "{} séquence(s) noire(s), max {:.2} s",
                blacks.len(),
                max_black
            ),
        ));
    }
    for interval in blacks {
        if interval.duration >= thresholds.black_finding_s {
            findings.push(QaFinding::new(
                DefectType::BlackFrame,
                interval.start,
                interval.end,
                if interval.duration >= 5.0 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                1.0,
                "stage0",
                format!(
                    "noir de {:.2} s (seuil {:.1} s)",
                    interval.duration, thresholds.black_finding_s
                ),
            ));
        }
    }

    let max_freeze = freezes
        .iter()
        .map(|interval| interval.duration)
        .fold(0.0_f64, f64::max);
    if freezes.is_empty() {
        checks.push(QaCheck::pass("freeze_frames", "aucun"));
    } else {
        let status = if max_freeze >= thresholds.freeze_finding_s {
            QaStatus::Review
        } else {
            QaStatus::Info
        };
        checks.push(QaCheck::new(
            "freeze_frames",
            status,
            format!("{} gel(s), max {:.2} s", freezes.len(), max_freeze),
        ));
    }
    for interval in freezes {
        if interval.duration >= thresholds.freeze_finding_s {
            findings.push(QaFinding::new(
                DefectType::Freeze,
                interval.start,
                interval.end,
                if interval.duration >= 10.0 {
                    Severity::High
                } else {
                    Severity::Medium
                },
                1.0,
                "stage0",
                format!(
                    "gel de {:.2} s (seuil {:.1} s)",
                    interval.duration, thresholds.freeze_finding_s
                ),
            ));
        }
    }

    (checks, findings)
}

#[derive(Debug, Clone)]
struct ProjectCheck {
    checks: Vec<QaCheck>,
    findings: Vec<QaFinding>,
    meta: Option<QaProjectMeta>,
    structure: Option<ProjectStructure>,
}

fn check_project(
    project: &Path,
    specs: &QaSpecs,
    video: &Path,
    template_prefix: &str,
    thresholds: &Stage0Thresholds,
) -> Result<ProjectCheck> {
    let doc = match ecproj::load(project) {
        Ok(doc) => doc,
        Err(error) => {
            return Ok(ProjectCheck {
                checks: vec![QaCheck::fail(
                    "project_readable",
                    format!("projet illisible: {error}"),
                )],
                findings: vec![QaFinding::new(
                    DefectType::SpecMismatch,
                    0.0,
                    None,
                    Severity::High,
                    1.0,
                    "stage0",
                    "projet .ecproj illisible",
                )],
                meta: None,
                structure: None,
            });
        }
    };
    let meta = match ecproj::comp_meta(&doc) {
        Ok(meta) => meta,
        Err(error) => {
            return Ok(ProjectCheck {
                checks: vec![QaCheck::fail(
                    "project_readable",
                    format!("composition illisible: {error}"),
                )],
                findings: vec![QaFinding::new(
                    DefectType::SpecMismatch,
                    0.0,
                    None,
                    Severity::High,
                    1.0,
                    "stage0",
                    "composition .ecproj illisible",
                )],
                meta: None,
                structure: None,
            });
        }
    };
    let segments = ecproj::segments(&doc).unwrap_or_default();
    let has_sdm_logo = ecproj::find_item_by_name(&doc, "sdm.png", "Footage").is_some();
    let mut checks = vec![QaCheck::pass(
        "project_readable",
        format!(
            "{} segments, comp {}x{}",
            segments.len(),
            meta.width,
            meta.height
        ),
    )];
    let mut findings = Vec::new();

    let delta = (specs.duration_s - meta.duration).abs();
    if delta > 1.0 {
        checks.push(QaCheck::fail(
            "duration_coherent",
            format!(
                "MP4 {:.2}s vs comp {:.2}s (Δ {:.2}s)",
                specs.duration_s, meta.duration, delta
            ),
        ));
        findings.push(QaFinding::new(
            DefectType::SpecMismatch,
            0.0,
            None,
            Severity::High,
            1.0,
            "stage0",
            format!("durée MP4/comp incohérente ({delta:.2}s)"),
        ));
    } else if delta > 0.5 {
        checks.push(QaCheck::info("duration_coherent", format!("Δ {delta:.2}s")));
    } else {
        checks.push(QaCheck::pass("duration_coherent", format!("Δ {delta:.3}s")));
    }

    let (segment_checks, segment_findings) = check_segments(&segments, meta.duration, thresholds);
    checks.extend(segment_checks);
    findings.extend(segment_findings);

    let stem = video
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let matched = !template_prefix.is_empty() && stem.starts_with(template_prefix);
    if !matched {
        checks.push(QaCheck::not_applicable(
            "template_a6",
            "projet hors template SDM",
        ));
    } else if has_sdm_logo {
        checks.push(QaCheck::pass("template_a6", "sdm.png présent"));
    } else {
        checks.push(QaCheck::fail(
            "template_a6",
            "sdm.png absent alors que le préfixe SDM est présent",
        ));
        findings.push(QaFinding::new(
            DefectType::TemplateLogoMissing,
            0.0,
            None,
            Severity::High,
            1.0,
            "stage0",
            "template SDM sans sdm.png",
        ));
    }

    let structure = ProjectStructure {
        first_segment_start: segments.first().map(|segment| round_to(segment.start, 3)),
        last_segment_end: segments
            .iter()
            .map(|segment| segment.end)
            .reduce(f64::max)
            .map(|end| round_to(end, 3)),
        shortest_segment_s: segments
            .iter()
            .map(|segment| segment.duration())
            .reduce(f64::min)
            .map(|duration| round_to(duration, 3)),
        has_sdm_logo,
    };
    let project_meta = QaProjectMeta {
        comp_id: meta.id,
        width: meta.width,
        height: meta.height,
        fps: meta.fps,
        duration_s: meta.duration,
        n_layers: meta.n_layers,
        n_segments: segments.len(),
    };
    Ok(ProjectCheck {
        checks,
        findings,
        meta: Some(project_meta),
        structure: Some(structure),
    })
}

fn check_segments(
    segments: &[ecproj::Segment],
    comp_duration: f64,
    thresholds: &Stage0Thresholds,
) -> (Vec<QaCheck>, Vec<QaFinding>) {
    let mut checks = Vec::new();
    let mut findings = Vec::new();
    if segments.is_empty() {
        checks.push(QaCheck::fail("segments_present", "aucun segment"));
        return (checks, findings);
    }
    checks.push(QaCheck::pass(
        "segments_present",
        format!("{} segments", segments.len()),
    ));

    let short = segments
        .iter()
        .filter(|segment| segment.duration() < thresholds.short_segment_info_s)
        .collect::<Vec<_>>();
    let very_short = short
        .iter()
        .copied()
        .filter(|segment| segment.duration() < thresholds.short_segment_finding_s)
        .collect::<Vec<_>>();
    if short.is_empty() {
        checks.push(QaCheck::pass(
            "short_segments",
            format!("aucun segment < {:.1} s", thresholds.short_segment_info_s),
        ));
    } else {
        let status = if very_short.is_empty() {
            QaStatus::Info
        } else {
            QaStatus::Review
        };
        checks.push(QaCheck::new(
            "short_segments",
            status,
            format!(
                "{} segment(s) < {:.1} s{}",
                short.len(),
                thresholds.short_segment_info_s,
                if very_short.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", dont {} < {:.2} s",
                        very_short.len(),
                        thresholds.short_segment_finding_s
                    )
                }
            ),
        ));
    }
    for segment in very_short {
        findings.push(QaFinding::new(
            DefectType::CutContinuity,
            round_to(segment.start, 3),
            Some(round_to(segment.end, 3)),
            Severity::Low,
            0.8,
            "stage0",
            format!(
                "segment de {:.2} s (< {:.2} s)",
                segment.duration(),
                thresholds.short_segment_finding_s
            ),
        ));
    }

    let last_end = segments
        .iter()
        .map(|segment| segment.end)
        .reduce(f64::max)
        .unwrap_or(0.0);
    let coverage = (comp_duration - last_end).abs();
    let status = if coverage > thresholds.coverage_warn_s {
        QaStatus::Review
    } else if coverage > 1.0 {
        QaStatus::Info
    } else {
        checks.push(QaCheck::pass(
            "timeline_coverage",
            format!(
                "dernier segment à {last_end:.2}s, comp {comp_duration:.2}s (Δ {coverage:.2}s)"
            ),
        ));
        return (checks, findings);
    };
    checks.push(QaCheck::new(
        "timeline_coverage",
        status,
        format!("dernier segment à {last_end:.2}s, comp {comp_duration:.2}s (Δ {coverage:.2}s)"),
    ));
    (checks, findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qa::model::QaStatus;
    use serde_json::{json, Value};
    use std::fs;
    use tempfile::tempdir;

    fn specs() -> QaSpecs {
        QaSpecs {
            width: 1080,
            height: 1920,
            fps: 25.0,
            video_codec: "h264".into(),
            audio_codec: "aac".into(),
            duration_s: 10.0,
            bit_rate: 10_000_000,
            size_bytes: 12_500_000,
        }
    }

    fn project_fixture() -> Value {
        json!({
            "items": {
                "1": {"name": "clip.mp4", "kind": {"type": "Footage", "width": 1920, "height": 1080}},
                "2": {
                    "name": "Comp",
                    "kind": {
                        "type": "Comp",
                        "width": 1080,
                        "height": 1920,
                        "frame_rate": {"num": 25, "den": 1},
                        "duration": 2540160000000_i64,
                        "layers": [
                            {
                                "id": 10,
                                "name": "Null Segments",
                                "source": {"type": "Null"},
                                "props": {"children": [{"name": "Transform", "children": [
                                    {"name": "Anchor Point", "value": {"v": [50.0, 50.0, 0.0]}},
                                    {"name": "Position", "value": {"v": [540.0, 0.0, 0.0]}},
                                    {"name": "Scale", "value": {"v": [100.0, 100.0, 100.0]}}
                                ]}]}
                            },
                            {
                                "id": 11,
                                "name": "Segment 1",
                                "parent": 10,
                                "in_point": 0,
                                "out_point": 2540160000000_i64,
                                "source": {"type": "Footage", "item": 1},
                                "props": {"children": [{"name": "Transform", "children": [
                                    {"name": "Anchor Point", "value": {"v": [960.0, 0.0, 0.0]}},
                                    {"name": "Position", "value": {"v": [540.0, 0.0, 0.0]}},
                                    {"name": "Scale", "value": {"v": [180.0, 180.0, 0.0]}}
                                ]}]}
                            }
                        ]
                    }
                }
            }
        })
    }

    #[test]
    fn parses_black_and_freeze_log() {
        let log = "\
[blackdetect @ 0x1] black_start:4.28 black_end:5.8 black_duration:1.52
[freezedetect @ 0x2] lavfi.freezedetect.freeze_start: 148.8
[freezedetect @ 0x2] lavfi.freezedetect.freeze_duration: 1.36
[freezedetect @ 0x2] lavfi.freezedetect.freeze_end: 150.16
";
        let (blacks, freezes) = parse_black_freeze(log, 200.0);
        assert_eq!(blacks.len(), 1);
        assert_eq!(blacks[0].start, 4.28);
        assert_eq!(blacks[0].duration, 1.52);
        assert_eq!(freezes.len(), 1);
        assert_eq!(freezes[0].start, 148.8);
        assert_eq!(freezes[0].end, Some(150.16));
    }

    #[test]
    fn open_freeze_uses_video_end() {
        let log = "[freezedetect @ 0x2] lavfi.freezedetect.freeze_start: 98.8\n";
        let (_, freezes) = parse_black_freeze(log, 100.0);
        assert_eq!(freezes.len(), 1);
        assert_eq!(freezes[0].end, None);
        assert!((freezes[0].duration - 1.2).abs() < 1e-6);
    }

    #[test]
    fn specs_pass_on_reference_profile() {
        let (checks, findings) = check_specs(&specs());
        assert!(checks.iter().all(|check| check.status == QaStatus::Pass));
        assert!(findings.is_empty());
    }

    #[test]
    fn specs_fail_on_wrong_resolution() {
        let mut wrong = specs();
        wrong.width = 1920;
        wrong.height = 1080;
        let (checks, findings) = check_specs(&wrong);
        assert!(checks
            .iter()
            .any(|check| check.id == "resolution" && check.status == QaStatus::Fail));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].defect_type, DefectType::SpecMismatch);
    }

    #[test]
    fn high_black_and_freeze_become_findings() {
        let thresholds = Stage0Thresholds::default();
        let blacks = vec![Interval {
            start: 10.0,
            end: Some(14.0),
            duration: 4.0,
        }];
        let freezes = vec![Interval {
            start: 20.0,
            end: Some(26.0),
            duration: 6.0,
        }];
        let (checks, findings) = check_black_freeze(&blacks, &freezes, &thresholds);
        assert!(checks
            .iter()
            .any(|check| check.id == "black_frames" && check.status == QaStatus::Review));
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].defect_type, DefectType::BlackFrame);
        assert_eq!(findings[1].defect_type, DefectType::Freeze);
    }

    #[test]
    fn short_but_normal_segments_are_info_only() {
        let segments = vec![ecproj::Segment {
            name: "Segment 1".into(),
            layer_id: 1,
            parent: None,
            start: 0.0,
            end: 10.0,
            source: None,
            anchor: [0.0; 3],
            scale: 100.0,
            position: [0.0; 3],
            position_keys: Vec::new(),
            scale_keys: Vec::new(),
            anchor_keys: Vec::new(),
            comp_x: 0.0,
            parent_dy: 0.0,
            null_parent: false,
            parent_keyframed: false,
        }];
        let (checks, findings) = check_segments(&segments, 10.0, &Stage0Thresholds::default());
        assert!(checks
            .iter()
            .any(|check| check.id == "timeline_coverage" && check.status == QaStatus::Pass));
        assert!(findings.is_empty());
    }

    #[test]
    fn project_check_reads_meta_and_template() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("clip_9x16.ecproj");
        fs::write(&path, serde_json::to_vec(&project_fixture()).unwrap()).unwrap();
        let check = check_project(
            &path,
            &specs(),
            Path::new("clip_9x16.mp4"),
            crate::post::a6::DEFAULT_PREFIX,
            &Stage0Thresholds::default(),
        )
        .unwrap();
        assert!(check.meta.is_some());
        assert!(check
            .checks
            .iter()
            .any(|item| item.id == "project_readable" && item.status == QaStatus::Pass));
        assert!(check
            .checks
            .iter()
            .any(|item| item.id == "template_a6" && item.status == QaStatus::NotApplicable));
    }

    #[test]
    fn missing_sdm_logo_is_blocking() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("M6 - SDM - test_9x16.ecproj");
        fs::write(&path, serde_json::to_vec(&project_fixture()).unwrap()).unwrap();
        let check = check_project(
            &path,
            &specs(),
            Path::new("M6 - SDM - test_9x16.mp4"),
            crate::post::a6::DEFAULT_PREFIX,
            &Stage0Thresholds::default(),
        )
        .unwrap();
        assert!(check
            .checks
            .iter()
            .any(|item| item.id == "template_a6" && item.status == QaStatus::Fail));
        assert!(check
            .findings
            .iter()
            .any(|item| item.defect_type == DefectType::TemplateLogoMissing));
    }
}
