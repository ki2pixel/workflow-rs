//! Spike A1 — détecteur de letterbox natif (D-057).
//!
//! Cet exemple est **hors production** : il ne modifie ni le contrat A1 ni le
//! chemin `cropdetect`. Il compare, sur des fixtures en lecture seule :
//!
//! - un scan ligne à ligne avec EWMA 5σ (`alpha = 0,05`, plancher `0,005`) ;
//! - une validation de frontière par Sobel Y 3×3 ciblé et couverture de
//!   colonnes (au lieu de l'ExKirsch 5×5 plein cadre) ;
//! - un clustering temporel des couples `(top, bottom)` avec cluster dominant
//!   ≥ 60 %, sinon `review` ;
//! - `cropdetect=24:16:0` (A1 actuel) et `cropdetect=24:2:0` (référence 2 px).
//!
//! Usage :
//!
//! ```sh
//! cargo run -p workflow-steps --release --example a1_letterbox_spike -- \
//!   --corpus "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2" \
//!   --out /tmp/opencode/a1-letterbox-spike
//!
//! # échantillon complet (manifeste JSON de segments)
//! cargo run -p workflow-steps --release --example a1_letterbox_spike -- \
//!   --manifest /tmp/opencode/a1-letterbox-sample/manifest.json \
//!   --pre gray --runs 1 --out /tmp/opencode/a1-letterbox-sample/run
//! ```
//!
//! Les écritures sont limitées à `--out`, qui doit être sous `/tmp`. Le corpus
//! est uniquement lu par `ffmpeg`/`ffprobe`.

use anyhow::{bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Instant;
use workflow_schema::round::round_to;
use workflow_steps::post::detect::{probe_video, Crop};

const ALPHA: f64 = 0.05;
const MIN_SIGMA: f64 = 0.005;
const REF_THR: f64 = 2.0 / 3.0;
const CLUSTER_TOL: i64 = 2;
const CLUSTER_MIN_FRAC: f64 = 0.60;
const A1_MARGIN: f64 = 1.015;
const ACTIVE_NOOP: u32 = 1072;
const INTERMITTENT_DELTA: i64 = 24;
const OUTLIER_FRAC: f64 = 0.10;
const DEFAULT_EDGE_THRESHOLD: f32 = 32.0;
const DEFAULT_RUNS: usize = 3;
/// Seuil de couverture relaxé pour une barre très sombre (moyenne < 20/255).
/// `REF_THR` désactive la relaxation.
const DARK_COVERAGE: f64 = 0.45;
const DARK_LUMA: f32 = 20.0;

const V305: &str = "305 Camille/docs/6ter - LRDRLD -  Chauffeuse poids lourd et enceinte.mp4";
const V304: &str = "304 Camille/docs/Cette voiture percute une maison !.mp4";
const V356: &str = "356 Camille/docs/M6+ - Pretty Little Liars - Extrait 6 - La mort de Maya.mp4";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreMode {
    Gray,
    Chroma,
}

impl PreMode {
    fn parse(value: &str) -> Result<Self> {
        match value {
            "gray" => Ok(Self::Gray),
            "chroma" => Ok(Self::Chroma),
            other => bail!("mode --pre inconnu: {other} (gray|chroma|both)"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Gray => "gray",
            Self::Chroma => "chroma",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Fixture {
    name: &'static str,
    video: &'static str,
    start: f64,
    duration: f64,
    fps: f64,
    expected_decision: &'static str,
    reference_top: Option<u32>,
    reference_bottom: Option<u32>,
    reference_label: &'static str,
    note: &'static str,
    known_false_negative: bool,
}

const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "305-seg1",
        video: V305,
        start: 0.050,
        duration: 2.180,
        fps: 1.0,
        expected_decision: "apply",
        reference_top: Some(81),
        reference_bottom: Some(78),
        reference_label: "prototype Python 81/78 (cropdetect2 82/80)",
        note: "letterbox stable, précision 1 px",
        known_false_negative: false,
    },
    Fixture {
        name: "305-seg9",
        video: V305,
        start: 15.970,
        duration: 1.340,
        fps: 2.0,
        expected_decision: "apply",
        reference_top: Some(81),
        reference_bottom: Some(79),
        reference_label: "prototype Python 81/79",
        note: "letterbox stable, contrôle de stabilité",
        known_false_negative: false,
    },
    Fixture {
        name: "305-seg17",
        video: V305,
        start: 32.770,
        duration: 4.420,
        fps: 1.0,
        expected_decision: "review",
        reference_top: None,
        reference_bottom: None,
        reference_label: "transition, pas de référence fixe",
        note: "letterbox → plein cadre : le clustering doit refuser d'appliquer",
        known_false_negative: false,
    },
    Fixture {
        name: "304-seg9",
        video: V304,
        start: 43.250,
        duration: 2.660,
        fps: 2.0,
        expected_decision: "noop",
        reference_top: Some(0),
        reference_bottom: Some(0),
        reference_label: "pas de letterbox (ciel nocturne + bandeau)",
        note: "faux haut 935 px attendu sur 2/5 frames, rejeté par le clustering",
        known_false_negative: false,
    },
    Fixture {
        name: "356-seg2",
        video: V356,
        start: 1.850,
        duration: 1.380,
        fps: 2.0,
        expected_decision: "noop",
        reference_top: Some(0),
        reference_bottom: Some(0),
        reference_label: "cadre noir d'intro, pas de letterbox",
        note: "cropdetect voit 98/6 ; le natif doit rester noop",
        known_false_negative: false,
    },
    Fixture {
        name: "304-seg34",
        video: V304,
        start: 149.300,
        duration: 3.900,
        fps: 1.0,
        expected_decision: "noop",
        reference_top: Some(80),
        reference_bottom: Some(56),
        reference_label: "A1 cropdetect16 80/56 (faux négatif natif attendu)",
        note: "barre sombre à bord faible : A3 doit rester le filet",
        known_false_negative: true,
    },
];

#[derive(Debug, Clone, Copy)]
struct NativeConfig {
    edge_threshold: f32,
    /// Seuil de couverture pour les barres très sombres ; `REF_THR` désactive.
    dark_coverage: f64,
    /// Luminance moyenne (0–255) sous laquelle une barre est « très sombre ».
    dark_luma: f32,
}

#[derive(Debug, Clone)]
struct SampleSpec {
    start: f64,
    duration: f64,
    fps: f64,
    width: usize,
    height: usize,
    mode: PreMode,
    edge_threshold: f32,
    /// Seuil de couverture pour les barres très sombres ; `REF_THR` désactive.
    dark_coverage: f64,
    /// Luminance moyenne (0–255) sous laquelle une barre est « très sombre ».
    dark_luma: f32,
}

#[derive(Debug, Clone, Copy)]
struct Ewma {
    init: bool,
    mean: f64,
    var: f64,
}

impl Ewma {
    fn new() -> Self {
        Self {
            init: false,
            mean: 0.0,
            var: 0.0,
        }
    }

    fn add(&mut self, data: f64) {
        if self.init {
            let diff = data - self.mean;
            let incr = ALPHA * diff;
            self.mean += incr;
            self.var = (1.0 - ALPHA) * (self.var + diff * incr);
        } else {
            self.init = true;
            self.mean = data;
            self.var = 0.0;
        }
    }

    fn threshold(&self) -> Option<f64> {
        self.init
            .then(|| self.mean + 5.0 * self.var.sqrt().max(MIN_SIGMA))
    }

    /// Retourne la moyenne EWMA si `value` franchit le seuil 5σ.
    fn trigger(&self, value: f64) -> Option<f64> {
        self.threshold()
            .filter(|threshold| value > *threshold)
            .map(|_| self.mean)
    }
}

#[derive(Debug, Clone)]
struct Boundary {
    row: u32,
    coverage: f64,
    jump: f64,
    longest_run: usize,
    rejected: Vec<RejectedCandidate>,
}

#[derive(Debug, Clone, Serialize)]
struct RejectedCandidate {
    row: u32,
    coverage: f64,
    jump: f64,
    longest_run: usize,
}

#[derive(Debug, Clone, Serialize)]
struct FrameMeasure {
    index: usize,
    t: f64,
    top_row: u32,
    bottom_row: u32,
    top_bar: u32,
    bottom_bar: u32,
    top_coverage: f64,
    bottom_coverage: f64,
    top_jump: f64,
    bottom_jump: f64,
    top_run: usize,
    bottom_run: usize,
    top_rejected: Vec<RejectedCandidate>,
    bottom_rejected: Vec<RejectedCandidate>,
}

#[derive(Debug, Clone, Serialize)]
struct Cluster {
    top: u32,
    bottom: u32,
    count: usize,
    fraction: f64,
}

#[derive(Debug, Clone, Serialize)]
struct DecisionJson {
    kind: String,
    reason: String,
    active: u32,
    scale: Option<f64>,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct CropJson {
    w: u32,
    h: u32,
    x: u32,
    y: u32,
}

impl From<Crop> for CropJson {
    fn from(crop: Crop) -> Self {
        Self {
            w: crop.w,
            h: crop.h,
            x: crop.x,
            y: crop.y,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct CropSummary {
    n_crops: usize,
    active_p10: u32,
    crop: Option<CropJson>,
    top: i64,
    bottom: i64,
    ratio: f64,
    scale: Option<f64>,
    decision: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
struct TimingJson {
    native_decode_ms: f64,
    native_analysis_ms: f64,
    native_total_ms: f64,
    cropdetect16_ms: f64,
    cropdetect2_ms: f64,
    ratio_total_vs_cropdetect16: f64,
}

#[derive(Debug, Clone, Serialize)]
struct ReferenceJson {
    top: Option<u32>,
    bottom: Option<u32>,
    label: String,
    expected_decision: String,
    error_top: Option<i64>,
    error_bottom: Option<i64>,
    scale_error: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct FixtureReport {
    fixture: String,
    video: String,
    start: f64,
    duration: f64,
    fps: f64,
    pre: String,
    n_frames: usize,
    frames: Vec<FrameMeasure>,
    clusters: Vec<Cluster>,
    dominant: Option<Cluster>,
    native: DecisionJson,
    cropdetect16: CropSummary,
    cropdetect2: CropSummary,
    timings: TimingJson,
    reference: ReferenceJson,
    notes: String,
    known_false_negative: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct TimingSamples {
    decode_ms: f64,
    analysis_ms: f64,
    total_ms: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ManifestEntry {
    name: String,
    project: String,
    segment: String,
    video: PathBuf,
    start: f64,
    duration: f64,
    fps: f64,
    kind: String,
    category: String,
    review_outcome: String,
    apply_acceptance: String,
    bars_present: String,
    human_keyframes: u32,
    good_removed: String,
    good_cleared: String,
    zoom_rel: String,
    scale_target: String,
    reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct SampleReport {
    name: String,
    project: String,
    segment: String,
    video: String,
    start: f64,
    duration: f64,
    fps: f64,
    kind: String,
    category: String,
    review_outcome: String,
    apply_acceptance: String,
    bars_present: String,
    human_keyframes: u32,
    good_removed: String,
    good_cleared: String,
    zoom_rel: String,
    scale_target: String,
    reason: String,
    n_frames: usize,
    native: DecisionJson,
    dominant: Option<Cluster>,
    clusters: Vec<Cluster>,
    cropdetect16: CropSummary,
    cropdetect2: CropSummary,
    timings: TimingJson,
    frames: Vec<FrameMeasure>,
}

fn main() -> Result<()> {
    let mut corpus = PathBuf::from(std::env::var("CORPUS").unwrap_or_else(|_| {
        "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2".into()
    }));
    let mut out = PathBuf::from("/tmp/opencode/a1-letterbox-spike");
    let mut pre_modes = vec![PreMode::Gray, PreMode::Chroma];
    let mut edge_threshold = DEFAULT_EDGE_THRESHOLD;
    let mut dark_coverage = REF_THR;
    let mut dark_luma = DARK_LUMA;
    let mut runs = DEFAULT_RUNS;
    let mut only: Option<String> = None;
    let mut manifest: Option<PathBuf> = None;

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
            "--pre" => {
                index += 1;
                let value = args.get(index).context("--pre sans valeur")?;
                pre_modes = match value.as_str() {
                    "both" => vec![PreMode::Gray, PreMode::Chroma],
                    other => vec![PreMode::parse(other)?],
                };
            }
            "--edge-threshold" => {
                index += 1;
                edge_threshold = args
                    .get(index)
                    .context("--edge-threshold sans valeur")?
                    .parse()
                    .context("--edge-threshold invalide")?;
            }
            "--dark-coverage" => {
                index += 1;
                dark_coverage = args
                    .get(index)
                    .context("--dark-coverage sans valeur")?
                    .parse()
                    .context("--dark-coverage invalide")?;
            }
            "--dark-luma" => {
                index += 1;
                dark_luma = args
                    .get(index)
                    .context("--dark-luma sans valeur")?
                    .parse()
                    .context("--dark-luma invalide")?;
            }
            "--runs" => {
                index += 1;
                runs = args
                    .get(index)
                    .context("--runs sans valeur")?
                    .parse()
                    .context("--runs invalide")?;
            }
            "--fixture" => {
                index += 1;
                only = Some(args.get(index).context("--fixture sans valeur")?.clone());
            }
            "--manifest" => {
                index += 1;
                manifest = Some(PathBuf::from(
                    args.get(index).context("--manifest sans valeur")?,
                ));
            }
            "--help" | "-h" => {
                println!(
                    "Usage: a1_letterbox_spike [--corpus DIR] [--out DIR] \
                     [--pre gray|chroma|both] [--edge-threshold N] [--runs N] \
                     [--dark-coverage N] [--dark-luma N] \
                     [--fixture NAME] [--manifest FILE]\n\
                     --dark-coverage : seuil de couverture des barres sombres \
                     (défaut désactivé ; candidat {DARK_COVERAGE})\n\
                     --dark-luma : luminance moyenne 0–255 d'une barre sombre \
                     (défaut {DARK_LUMA})"
                );
                return Ok(());
            }
            other => bail!("argument inconnu: {other}"),
        }
        index += 1;
    }

    if !out.starts_with("/tmp") {
        bail!(
            "--out doit être sous /tmp (spike read-only) : {}",
            out.display()
        );
    }
    if !corpus.is_dir() {
        bail!("corpus introuvable: {}", corpus.display());
    }
    if runs == 0 {
        bail!("--runs doit être ≥ 1");
    }
    if !(0.0..=1.0).contains(&dark_coverage) {
        bail!("--dark-coverage doit être dans [0, 1]");
    }
    if !(0.0..=255.0).contains(&dark_luma) {
        bail!("--dark-luma doit être dans [0, 255]");
    }
    fs::create_dir_all(&out).with_context(|| format!("création de {}", out.display()))?;
    let native = NativeConfig {
        edge_threshold,
        dark_coverage,
        dark_luma,
    };

    let ffmpeg = std::env::var("FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
    let ffprobe = std::env::var("FFPROBE").unwrap_or_else(|_| "ffprobe".into());
    let ffmpeg = PathBuf::from(ffmpeg);
    let ffprobe = PathBuf::from(ffprobe);

    if let Some(manifest_path) = manifest {
        if only.is_some() {
            bail!("--manifest et --fixture sont exclusifs");
        }
        if pre_modes.len() != 1 || pre_modes[0] != PreMode::Gray {
            bail!("--manifest exige --pre gray (candidat A1)");
        }
        run_manifest(&ffmpeg, &ffprobe, &manifest_path, &out, runs, native)?;
        return Ok(());
    }

    let selected: Vec<&Fixture> = FIXTURES
        .iter()
        .filter(|fixture| only.as_deref().is_none_or(|name| name == fixture.name))
        .collect();
    if selected.is_empty() {
        bail!("aucune fixture ne correspond à --fixture");
    }

    let mut reports = Vec::new();
    for fixture in selected {
        let video = corpus.join(fixture.video);
        if !video.is_file() {
            bail!("vidéo introuvable: {}", video.display());
        }
        let specs = probe_video(&ffprobe, &video)?;
        if specs.height != 1080 {
            bail!(
                "fixture {} non 1080p ({}), le scale A1 n'est pas comparable",
                fixture.name,
                specs.height
            );
        }
        for mode in &pre_modes {
            let sample = SampleSpec {
                start: fixture.start,
                duration: fixture.duration,
                fps: fixture.fps,
                width: specs.width as usize,
                height: specs.height as usize,
                mode: *mode,
                edge_threshold: native.edge_threshold,
                dark_coverage: native.dark_coverage,
                dark_luma: native.dark_luma,
            };
            eprintln!(
                "spike {} [{}] {}x{} @ {:.3} fps…",
                fixture.name,
                mode.as_str(),
                sample.width,
                sample.height,
                sample.fps
            );
            let report = run_fixture(&ffmpeg, &video, fixture, &sample, runs, specs.height)?;
            println!(
                "{:<11} {:<6} native={:<6} cluster={}/{} ({:.0}%) bars={}/{} \
                 cd16={}/{} cd2={}/{} t={:.0}ms ratio={:.2}",
                report.fixture,
                report.pre,
                report.native.kind,
                report.dominant.as_ref().map_or(0, |cluster| cluster.count),
                report.n_frames,
                report
                    .dominant
                    .as_ref()
                    .map_or(0.0, |cluster| cluster.fraction)
                    * 100.0,
                report.dominant.as_ref().map_or(0, |cluster| cluster.top),
                report.dominant.as_ref().map_or(0, |cluster| cluster.bottom),
                report.cropdetect16.top,
                report.cropdetect16.bottom,
                report.cropdetect2.top,
                report.cropdetect2.bottom,
                report.timings.native_total_ms,
                report.timings.ratio_total_vs_cropdetect16
            );
            reports.push(report);
        }
    }

    let report_json = serde_json::json!({
        "spike": "a1_letterbox_spike",
        "decision": "D-057",
        "corpus": corpus.display().to_string(),
        "edge_threshold": native.edge_threshold,
        "dark_coverage": native.dark_coverage,
        "dark_luma": native.dark_luma,
        "min_coverage": REF_THR,
        "cluster_tol_px": CLUSTER_TOL,
        "cluster_min_frac": CLUSTER_MIN_FRAC,
        "runs": runs,
        "fixtures": reports,
    });
    let json_path = out.join("report.json");
    fs::write(&json_path, serde_json::to_string_pretty(&report_json)?)
        .with_context(|| format!("écriture de {}", json_path.display()))?;

    let markdown = render_markdown(&reports, native, runs);
    let md_path = out.join("report.md");
    fs::write(&md_path, markdown).with_context(|| format!("écriture de {}", md_path.display()))?;

    println!("\nRapport JSON : {}", json_path.display());
    println!("Rapport Markdown : {}", md_path.display());
    Ok(())
}

fn run_manifest(
    ffmpeg: &Path,
    ffprobe: &Path,
    manifest: &Path,
    out: &Path,
    runs: usize,
    native: NativeConfig,
) -> Result<()> {
    let payload = fs::read_to_string(manifest)
        .with_context(|| format!("lecture du manifeste {}", manifest.display()))?;
    let entries: Vec<ManifestEntry> =
        serde_json::from_str(&payload).context("manifeste JSON invalide")?;
    if entries.is_empty() {
        bail!("manifeste vide");
    }
    let mut dimensions: HashMap<PathBuf, (u32, u32)> = HashMap::new();
    let mut reports = Vec::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let (width, height) = match dimensions.get(&entry.video) {
            Some(dimension) => *dimension,
            None => {
                let specs = probe_video(ffprobe, &entry.video)?;
                let dimension = (specs.width, specs.height);
                dimensions.insert(entry.video.clone(), dimension);
                dimension
            }
        };
        if height != 1080 {
            bail!(
                "segment {} non 1080p ({}), le scale A1 n'est pas comparable",
                entry.name,
                height
            );
        }
        let sample = SampleSpec {
            start: entry.start,
            duration: entry.duration,
            fps: entry.fps,
            width: width as usize,
            height: height as usize,
            mode: PreMode::Gray,
            edge_threshold: native.edge_threshold,
            dark_coverage: native.dark_coverage,
            dark_luma: native.dark_luma,
        };
        let (frames, timing) = measure_native(ffmpeg, &entry.video, &sample, runs)?;
        let clusters = cluster_frames(&frames);
        let dominant = clusters.first().cloned();
        let native = decide_native(&dominant, frames.len(), height);
        let (crops16, cd16_ms) =
            measure_cropdetect(ffmpeg, &entry.video, entry.start, entry.duration, 16, runs)?;
        let (crops2, cd2_ms) =
            measure_cropdetect(ffmpeg, &entry.video, entry.start, entry.duration, 2, runs)?;
        let cropdetect16 = summarize_cropdetect(&crops16, height);
        let cropdetect2 = summarize_cropdetect(&crops2, height);
        let timings = TimingJson {
            native_decode_ms: round_to(timing.decode_ms, 1),
            native_analysis_ms: round_to(timing.analysis_ms, 1),
            native_total_ms: round_to(timing.total_ms, 1),
            cropdetect16_ms: round_to(cd16_ms, 1),
            cropdetect2_ms: round_to(cd2_ms, 1),
            ratio_total_vs_cropdetect16: round_to(timing.total_ms / cd16_ms.max(0.001), 2),
        };
        let label: String = entry.name.chars().take(72).collect();
        eprintln!(
            "[{}/{}] {:<5} {:<72} -> {} ({} frames)",
            index + 1,
            entries.len(),
            entry.kind,
            label,
            native.kind,
            frames.len()
        );
        reports.push(SampleReport {
            name: entry.name.clone(),
            project: entry.project.clone(),
            segment: entry.segment.clone(),
            video: entry.video.display().to_string(),
            start: round_to(entry.start, 3),
            duration: round_to(entry.duration, 3),
            fps: round_to(entry.fps, 3),
            kind: entry.kind.clone(),
            category: entry.category.clone(),
            review_outcome: entry.review_outcome.clone(),
            apply_acceptance: entry.apply_acceptance.clone(),
            bars_present: entry.bars_present.clone(),
            human_keyframes: entry.human_keyframes,
            good_removed: entry.good_removed.clone(),
            good_cleared: entry.good_cleared.clone(),
            zoom_rel: entry.zoom_rel.clone(),
            scale_target: entry.scale_target.clone(),
            reason: entry.reason.clone(),
            n_frames: frames.len(),
            native,
            dominant,
            clusters,
            cropdetect16,
            cropdetect2,
            timings,
            frames,
        });
    }
    let json_path = out.join("sample-report.json");
    fs::write(&json_path, serde_json::to_string_pretty(&reports)?)
        .with_context(|| format!("écriture de {}", json_path.display()))?;
    let csv_path = out.join("sample-report.csv");
    write_sample_csv(&csv_path, &reports)?;
    println!("\n{} segments analysés", reports.len());
    println!("JSON : {}", json_path.display());
    println!("CSV  : {}", csv_path.display());
    Ok(())
}

fn write_sample_csv(path: &Path, reports: &[SampleReport]) -> Result<()> {
    let mut out = String::new();
    writeln!(
        out,
        "name,project,segment,kind,category,review_outcome,apply_acceptance,\
         bars_present,human_keyframes,good_removed,good_cleared,zoom_rel,\
         scale_target,start,duration,fps,n_frames,native,cluster_count,\
         cluster_fraction,top,bottom,cd16,cd16_top,cd16_bottom,cd2,cd2_top,\
         cd2_bottom,native_scale,native_ms,cd16_ms,cd2_ms,reason"
    )?;
    for report in reports {
        let (count, fraction, top, bottom) =
            report.dominant.as_ref().map_or((0, 0.0, 0, 0), |cluster| {
                (cluster.count, cluster.fraction, cluster.top, cluster.bottom)
            });
        let native_scale = report
            .native
            .scale
            .map_or_else(String::new, |scale| format!("{scale:.1}"));
        writeln!(
            out,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{:.3},{:.3},{:.3},{},{},{},{:.3},{},{},{},{},{},{},{},{},{},{:.1},{:.1},{:.1},{}",
            csv_escape(&report.name),
            csv_escape(&report.project),
            csv_escape(&report.segment),
            report.kind,
            report.category,
            report.review_outcome,
            report.apply_acceptance,
            report.bars_present,
            report.human_keyframes,
            report.good_removed,
            report.good_cleared,
            report.zoom_rel,
            report.scale_target,
            report.start,
            report.duration,
            report.fps,
            report.n_frames,
            report.native.kind,
            count,
            fraction,
            top,
            bottom,
            report.cropdetect16.decision,
            report.cropdetect16.top,
            report.cropdetect16.bottom,
            report.cropdetect2.decision,
            report.cropdetect2.top,
            report.cropdetect2.bottom,
            native_scale,
            report.timings.native_total_ms,
            report.timings.cropdetect16_ms,
            report.timings.cropdetect2_ms,
            csv_escape(&report.reason)
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

fn run_fixture(
    ffmpeg: &Path,
    video: &Path,
    fixture: &Fixture,
    sample: &SampleSpec,
    runs: usize,
    source_height: u32,
) -> Result<FixtureReport> {
    let (frames, timing) = measure_native(ffmpeg, video, sample, runs)?;
    let clusters = cluster_frames(&frames);
    let dominant = clusters.first().cloned();
    let native = decide_native(&dominant, frames.len(), source_height);

    let (crops16, cd16_ms) =
        measure_cropdetect(ffmpeg, video, sample.start, sample.duration, 16, runs)?;
    let (crops2, cd2_ms) =
        measure_cropdetect(ffmpeg, video, sample.start, sample.duration, 2, runs)?;
    let cropdetect16 = summarize_cropdetect(&crops16, source_height);
    let cropdetect2 = summarize_cropdetect(&crops2, source_height);

    let reference = build_reference(fixture, &native, dominant.as_ref());
    let timings = TimingJson {
        native_decode_ms: round_to(timing.decode_ms, 1),
        native_analysis_ms: round_to(timing.analysis_ms, 1),
        native_total_ms: round_to(timing.total_ms, 1),
        cropdetect16_ms: round_to(cd16_ms, 1),
        cropdetect2_ms: round_to(cd2_ms, 1),
        ratio_total_vs_cropdetect16: round_to(timing.total_ms / cd16_ms.max(0.001), 2),
    };

    Ok(FixtureReport {
        fixture: fixture.name.into(),
        video: video.display().to_string(),
        start: round_to(sample.start, 3),
        duration: round_to(sample.duration, 3),
        fps: round_to(sample.fps, 3),
        pre: sample.mode.as_str().into(),
        n_frames: frames.len(),
        frames,
        clusters,
        dominant,
        native,
        cropdetect16,
        cropdetect2,
        timings,
        reference,
        notes: fixture.note.into(),
        known_false_negative: fixture.known_false_negative,
    })
}

fn build_reference(
    fixture: &Fixture,
    native: &DecisionJson,
    dominant: Option<&Cluster>,
) -> ReferenceJson {
    let top = dominant.map(|cluster| cluster.top);
    let bottom = dominant.map(|cluster| cluster.bottom);
    let error_top = match (top, fixture.reference_top) {
        (Some(actual), Some(expected)) => Some(actual as i64 - expected as i64),
        _ => None,
    };
    let error_bottom = match (bottom, fixture.reference_bottom) {
        (Some(actual), Some(expected)) => Some(actual as i64 - expected as i64),
        _ => None,
    };
    let scale_error = if fixture.name == "305-seg1" {
        native.scale.map(|scale| round_to(scale - 210.0, 1))
    } else {
        None
    };
    ReferenceJson {
        top: fixture.reference_top,
        bottom: fixture.reference_bottom,
        label: fixture.reference_label.into(),
        expected_decision: fixture.expected_decision.into(),
        error_top,
        error_bottom,
        scale_error,
    }
}

fn measure_native(
    ffmpeg: &Path,
    video: &Path,
    sample: &SampleSpec,
    runs: usize,
) -> Result<(Vec<FrameMeasure>, TimingSamples)> {
    let mut last_frames = Vec::new();
    let mut decode = Vec::with_capacity(runs);
    let mut analysis = Vec::with_capacity(runs);
    let mut total = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        let raw = extract_frames(ffmpeg, video, sample)?;
        let decode_ms = started.elapsed().as_secs_f64() * 1000.0;
        let analysis_started = Instant::now();
        let mut frames = Vec::with_capacity(raw.len());
        for (index, frame) in raw.iter().enumerate() {
            frames.push(analyze_frame(frame, index, sample));
        }
        let analysis_ms = analysis_started.elapsed().as_secs_f64() * 1000.0;
        last_frames = frames;
        decode.push(decode_ms);
        analysis.push(analysis_ms);
        total.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    Ok((
        last_frames,
        TimingSamples {
            decode_ms: median(&mut decode),
            analysis_ms: median(&mut analysis),
            total_ms: median(&mut total),
        },
    ))
}

fn extract_frames(ffmpeg: &Path, video: &Path, sample: &SampleSpec) -> Result<Vec<Vec<u8>>> {
    let pix_fmt = match sample.mode {
        PreMode::Gray => "gray",
        PreMode::Chroma => "yuv444p",
    };
    let planes = match sample.mode {
        PreMode::Gray => 1,
        PreMode::Chroma => 3,
    };
    let frame_size = sample.width * sample.height * planes;
    let mut child = Command::new(ffmpeg)
        .args(["-v", "error", "-ss", &format!("{:.3}", sample.start), "-i"])
        .arg(video)
        .args([
            "-t",
            &format!("{:.3}", sample.duration),
            "-vf",
            &format!("fps={}", sample.fps),
            "-f",
            "rawvideo",
            "-pix_fmt",
            pix_fmt,
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("ffmpeg extraction introuvable pour {}", video.display()))?;
    let stderr = child.stderr.take().context("stderr ffmpeg absent")?;
    let stderr_handle = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let mut reader = stderr;
        let _ = reader.read_to_end(&mut buffer);
        buffer
    });
    let stdout = child.stdout.take().context("stdout ffmpeg absent")?;
    let mut reader = BufReader::new(stdout);
    let mut frames = Vec::new();
    loop {
        let mut buffer = vec![0u8; frame_size];
        match reader.read_exact(&mut buffer) {
            Ok(()) => frames.push(buffer),
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error).context("lecture des frames ffmpeg"),
        }
    }
    drop(reader);
    let status = child.wait().context("attente ffmpeg extraction")?;
    let stderr = stderr_handle.join().unwrap_or_default();
    if !status.success() {
        bail!(
            "ffmpeg extraction: {}",
            String::from_utf8_lossy(&stderr).trim()
        );
    }
    if frames.is_empty() {
        bail!("ffmpeg extraction sans frame");
    }
    Ok(frames)
}

fn analyze_frame(frame: &[u8], index: usize, sample: &SampleSpec) -> FrameMeasure {
    let pre = build_pre(frame, sample);
    let means = row_means(&pre, sample.width, sample.height);
    let top = scan_top(&pre, &means, sample);
    let bottom = scan_bottom(&pre, &means, sample);
    let height = sample.height as u32;
    FrameMeasure {
        index,
        t: round_to(sample.start + index as f64 / sample.fps, 3),
        top_row: top.row,
        bottom_row: bottom.row,
        top_bar: top.row,
        bottom_bar: height - 1 - bottom.row,
        top_coverage: round_to(top.coverage, 3),
        bottom_coverage: round_to(bottom.coverage, 3),
        top_jump: round_to(top.jump, 4),
        bottom_jump: round_to(bottom.jump, 4),
        top_run: top.longest_run,
        bottom_run: bottom.longest_run,
        top_rejected: top.rejected,
        bottom_rejected: bottom.rejected,
    }
}

fn build_pre(frame: &[u8], sample: &SampleSpec) -> Vec<f32> {
    let pixels = sample.width * sample.height;
    match sample.mode {
        PreMode::Gray => frame[..pixels].iter().map(|value| *value as f32).collect(),
        PreMode::Chroma => {
            let (y, rest) = frame.split_at(pixels);
            let (u, v) = rest.split_at(pixels);
            (0..pixels)
                .map(|index| {
                    let u = u[index] as f32 - 128.0;
                    let v = v[index] as f32 - 128.0;
                    (y[index] as f32 + 0.25 * u.abs() + 0.25 * v.abs()).clamp(0.0, 255.0)
                })
                .collect()
        }
    }
}

fn row_means(pre: &[f32], width: usize, height: usize) -> Vec<f64> {
    (0..height)
        .map(|y| {
            let row = &pre[y * width..(y + 1) * width];
            row.iter().map(|value| *value as f64).sum::<f64>() / width as f64 / 255.0
        })
        .collect()
}

fn scan_top(pre: &[f32], means: &[f64], sample: &SampleSpec) -> Boundary {
    let mut stats = Ewma::new();
    let mut rejected = Vec::new();
    for (y, value) in means.iter().enumerate() {
        if let Some(mean) = stats.trigger(*value) {
            let threshold = coverage_threshold(means, y, sample, true);
            let (coverage, longest_run) = edge_coverage(pre, sample, y, sample.edge_threshold);
            if coverage >= threshold {
                return Boundary {
                    row: y as u32,
                    coverage,
                    jump: *value - mean,
                    longest_run,
                    rejected,
                };
            }
            rejected.push(RejectedCandidate {
                row: y as u32,
                coverage: round_to(coverage, 3),
                jump: round_to(*value - mean, 4),
                longest_run,
            });
        }
        stats.add(*value);
    }
    Boundary {
        row: 0,
        coverage: 0.0,
        jump: 0.0,
        longest_run: 0,
        rejected,
    }
}

fn scan_bottom(pre: &[f32], means: &[f64], sample: &SampleSpec) -> Boundary {
    let mut stats = Ewma::new();
    let mut rejected = Vec::new();
    for y in (0..means.len()).rev() {
        let value = means[y];
        if let Some(mean) = stats.trigger(value) {
            let threshold = coverage_threshold(means, y, sample, false);
            let (coverage, longest_run) = edge_coverage(pre, sample, y, sample.edge_threshold);
            if coverage >= threshold {
                return Boundary {
                    row: y as u32,
                    coverage,
                    jump: value - mean,
                    longest_run,
                    rejected,
                };
            }
            rejected.push(RejectedCandidate {
                row: y as u32,
                coverage: round_to(coverage, 3),
                jump: round_to(value - mean, 4),
                longest_run,
            });
        }
        stats.add(value);
    }
    Boundary {
        row: sample.height.saturating_sub(1) as u32,
        coverage: 0.0,
        jump: 0.0,
        longest_run: 0,
        rejected,
    }
}

fn edge_coverage(pre: &[f32], sample: &SampleSpec, y: usize, threshold: f32) -> (f64, usize) {
    let width = sample.width;
    let height = sample.height;
    let ym1 = y.saturating_sub(1);
    let yp1 = (y + 1).min(height - 1);
    let mut hits = 0usize;
    let mut run = 0usize;
    let mut longest_run = 0usize;
    for x in 0..width {
        let xm1 = x.saturating_sub(1);
        let xp1 = (x + 1).min(width - 1);
        let gradient =
            -pre[ym1 * width + xm1] - 2.0 * pre[ym1 * width + x] - pre[ym1 * width + xp1]
                + pre[yp1 * width + xm1]
                + 2.0 * pre[yp1 * width + x]
                + pre[yp1 * width + xp1];
        if gradient.abs() > threshold {
            hits += 1;
            run += 1;
            longest_run = longest_run.max(run);
        } else {
            run = 0;
        }
    }
    (hits as f64 / width as f64, longest_run)
}

/// Seuil de couverture applicable à la ligne `y`.
///
/// Une barre est considérée « très sombre » quand la moyenne des quatre lignes
/// situées côté barre est inférieure à `dark_luma` (0–255). On relaxe alors la
/// couverture à `dark_coverage` (candidat 0,45) au lieu de `REF_THR` (2/3).
/// Le ciel nocturne du 304 seg 9 n'est pas un barreau uniforme : ses lignes
/// restent au-dessus du seuil ou leur couverture reste sous 0,45.
fn coverage_threshold(means: &[f64], y: usize, sample: &SampleSpec, top: bool) -> f64 {
    if sample.dark_coverage >= REF_THR {
        return REF_THR;
    }
    let (start, end) = if top {
        (y.saturating_sub(4), y)
    } else {
        ((y + 1).min(means.len()), (y + 5).min(means.len()))
    };
    if start >= end {
        return REF_THR;
    }
    let mean_luma = means[start..end].iter().sum::<f64>() / (end - start) as f64 * 255.0;
    if mean_luma < sample.dark_luma as f64 {
        sample.dark_coverage
    } else {
        REF_THR
    }
}

fn cluster_frames(frames: &[FrameMeasure]) -> Vec<Cluster> {
    let mut clusters: Vec<Cluster> = Vec::new();
    for frame in frames {
        if let Some(cluster) = clusters.iter_mut().find(|cluster| {
            (cluster.top as i64 - frame.top_bar as i64).abs() <= CLUSTER_TOL
                && (cluster.bottom as i64 - frame.bottom_bar as i64).abs() <= CLUSTER_TOL
        }) {
            cluster.count += 1;
        } else {
            clusters.push(Cluster {
                top: frame.top_bar,
                bottom: frame.bottom_bar,
                count: 1,
                fraction: 0.0,
            });
        }
    }
    for cluster in &mut clusters {
        cluster.fraction = round_to(cluster.count as f64 / frames.len().max(1) as f64, 3);
    }
    clusters.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(a.top.cmp(&b.top))
            .then(a.bottom.cmp(&b.bottom))
    });
    clusters
}

fn decide_native(dominant: &Option<Cluster>, n_frames: usize, source_height: u32) -> DecisionJson {
    let Some(cluster) = dominant else {
        return DecisionJson {
            kind: "review".into(),
            reason: "aucune mesure".into(),
            active: source_height,
            scale: None,
        };
    };
    if cluster.fraction < CLUSTER_MIN_FRAC || n_frames < 2 {
        return DecisionJson {
            kind: "review".into(),
            reason: format!(
                "clusters instables : dominant {}/{} ({:.0}%)",
                cluster.count,
                n_frames,
                cluster.fraction * 100.0
            ),
            active: source_height
                .saturating_sub(cluster.top)
                .saturating_sub(cluster.bottom),
            scale: None,
        };
    }
    if cluster.top == 0 && cluster.bottom == 0 {
        return DecisionJson {
            kind: "noop".into(),
            reason: "pas de barres".into(),
            active: source_height,
            scale: None,
        };
    }
    let active = source_height
        .saturating_sub(cluster.top)
        .saturating_sub(cluster.bottom);
    if active >= ACTIVE_NOOP {
        return DecisionJson {
            kind: "noop".into(),
            reason: "barres < 8 px".into(),
            active,
            scale: None,
        };
    }
    let top = cluster.top as i64;
    let bottom = cluster.bottom as i64;
    let bar_min = 8;
    let symmetric = top >= bar_min
        && bottom >= bar_min
        && (top - bottom).abs() as f64 <= 0.5 * top.max(bottom) as f64;
    if !symmetric {
        return DecisionJson {
            kind: "review".into(),
            reason: format!("barres asymétriques ({top}/{bottom})"),
            active,
            scale: None,
        };
    }
    let scale = round_to(1920.0 / active as f64 * 100.0 * A1_MARGIN, 1);
    DecisionJson {
        kind: "apply".into(),
        reason: format!(
            "cluster dominant {}/{} ({:.0}%)",
            cluster.count,
            n_frames,
            cluster.fraction * 100.0
        ),
        active,
        scale: Some(scale),
    }
}

fn measure_cropdetect(
    ffmpeg: &Path,
    video: &Path,
    start: f64,
    duration: f64,
    round: u32,
    runs: usize,
) -> Result<(Vec<Crop>, f64)> {
    let mut last = Vec::new();
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        last = run_cropdetect(ffmpeg, video, start, duration, round)?;
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    Ok((last, median(&mut times)))
}

fn run_cropdetect(
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

fn summarize_cropdetect(crops: &[Crop], source_height: u32) -> CropSummary {
    if crops.is_empty() {
        return CropSummary {
            n_crops: 0,
            active_p10: 0,
            crop: None,
            top: 0,
            bottom: 0,
            ratio: 0.0,
            scale: None,
            decision: "review".into(),
        };
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
    let (crop, crop_count) = dominant;
    let ratio = round_to(crop_count as f64 / crops.len() as f64, 3);
    let top = crop.y as i64;
    let bottom = source_height as i64 - (crop.y + crop.h) as i64;
    let barred = crops
        .iter()
        .filter(|crop| (crop.h as i64) < active as i64 - INTERMITTENT_DELTA)
        .count();
    let barred_frac = barred as f64 / crops.len() as f64;
    let bar_min = 8;
    let vertical = top >= bar_min
        && bottom >= bar_min
        && (top - bottom).abs() as f64 <= 0.5 * top.max(bottom) as f64;
    let scale = round_to(1920.0 / active as f64 * 100.0 * A1_MARGIN, 1);
    let decision = if active >= ACTIVE_NOOP {
        "noop"
    } else if barred_frac >= OUTLIER_FRAC || !vertical || scale > 300.0 {
        "review"
    } else {
        "apply"
    };
    CropSummary {
        n_crops: crops.len(),
        active_p10: active,
        crop: Some(crop.into()),
        top,
        bottom,
        ratio,
        scale: Some(scale),
        decision: decision.into(),
    }
}

fn percentile(values: &[u32], quantile: f64) -> f64 {
    let mut ordered = values.to_vec();
    ordered.sort_unstable();
    let index = round_to((ordered.len().saturating_sub(1)) as f64 * quantile, 0) as usize;
    ordered[index.min(ordered.len() - 1)] as f64
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn render_markdown(reports: &[FixtureReport], native: NativeConfig, runs: usize) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# Spike A1 — détecteur natif (D-057)\n");
    let _ = writeln!(
        out,
        "Prototype hors production. EWMA `alpha = 0,05`, seuil \
         `μ + 5 × max(σ, 0,005)`, Sobel Y 3×3 ciblé, couverture de colonnes \
         ≥ {:.3}, seuil de contour {:.0}/1020, clustering ±{} px avec cluster \
         dominant ≥ {:.0} %, sinon `review`. {} répétition(s) par mesure.\n",
        REF_THR,
        native.edge_threshold,
        CLUSTER_TOL,
        CLUSTER_MIN_FRAC * 100.0,
        runs
    );
    if native.dark_coverage < REF_THR {
        let _ = writeln!(
            out,
            "Relaxation barres sombres : couverture ≥ {:.2} si la moyenne des \
             quatre lignes côté barre est < {:.0}/255 ; sinon ≥ {:.3}.\n",
            native.dark_coverage, native.dark_luma, REF_THR
        );
    } else {
        let _ = writeln!(out, "Relaxation barres sombres : désactivée.\n");
    }

    let _ = writeln!(out, "## Résultats par fixture\n");
    let _ = writeln!(
        out,
        "| Fixture | Pré | Natif | Cluster | Barres | cd16 | cd2 | Réf. | Δ px | Scale | Attendu |"
    );
    let _ = writeln!(
        out,
        "| --- | --- | --- | ---: | ---: | ---: | ---: | --- | ---: | ---: | --- |"
    );
    for report in reports {
        let cluster = report.dominant.as_ref().map_or_else(
            || "—".to_string(),
            |cluster| {
                format!(
                    "{}/{} ({:.0} %)",
                    cluster.count,
                    report.n_frames,
                    cluster.fraction * 100.0
                )
            },
        );
        let bars = report.dominant.as_ref().map_or_else(
            || "—".to_string(),
            |cluster| format!("{}/{}", cluster.top, cluster.bottom),
        );
        let delta = match (report.reference.error_top, report.reference.error_bottom) {
            (Some(top), Some(bottom)) => format!("{top:+}/{bottom:+}"),
            _ => "—".to_string(),
        };
        let scale = report
            .native
            .scale
            .map_or_else(|| "—".to_string(), |scale| format!("{scale:.1} %"));
        let expected = if report.reference.expected_decision == report.native.kind {
            format!("✅ {}", report.reference.expected_decision)
        } else {
            format!(
                "❌ {} (attendu {})",
                report.native.kind, report.reference.expected_decision
            )
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {}/{} | {}/{} | {} | {} | {} | {} |",
            report.fixture,
            report.pre,
            report.native.kind,
            cluster,
            bars,
            report.cropdetect16.top,
            report.cropdetect16.bottom,
            report.cropdetect2.top,
            report.cropdetect2.bottom,
            report.reference.label,
            delta,
            scale,
            expected
        );
    }

    let _ = writeln!(out, "\n## Temps\n");
    let _ = writeln!(
        out,
        "| Fixture | Pré | Décodage | Analyse | Total natif | cd16 | cd2 | Total / cd16 |"
    );
    let _ = writeln!(
        out,
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |"
    );
    for report in reports {
        let _ = writeln!(
            out,
            "| {} | {} | {:.0} ms | {:.0} ms | {:.0} ms | {:.0} ms | {:.0} ms | {:.2}× |",
            report.fixture,
            report.pre,
            report.timings.native_decode_ms,
            report.timings.native_analysis_ms,
            report.timings.native_total_ms,
            report.timings.cropdetect16_ms,
            report.timings.cropdetect2_ms,
            report.timings.ratio_total_vs_cropdetect16
        );
    }

    let _ = writeln!(out, "\n## Robustesse frame à frame\n");
    for name in ["305-seg17", "304-seg9", "304-seg34"] {
        for report in reports.iter().filter(|report| report.fixture == name) {
            let _ = writeln!(out, "### {} — {}\n", report.fixture, report.pre);
            let _ = writeln!(
                out,
                "| t | top | bottom | barres | cov. haut | cov. bas | saut haut | saut bas |"
            );
            let _ = writeln!(
                out,
                "| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |"
            );
            for frame in &report.frames {
                let _ = writeln!(
                    out,
                    "| {:.2} | {} | {} | {}/{} | {:.3} | {:.3} | {:.4} | {:.4} |",
                    frame.t,
                    frame.top_row,
                    frame.bottom_row,
                    frame.top_bar,
                    frame.bottom_bar,
                    frame.top_coverage,
                    frame.bottom_coverage,
                    frame.top_jump,
                    frame.bottom_jump
                );
            }
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "## Candidats rejetés\n");
    for name in ["304-seg9", "304-seg34", "305-seg17"] {
        for report in reports.iter().filter(|report| report.fixture == name) {
            let mut lines = Vec::new();
            for frame in &report.frames {
                for candidate in &frame.top_rejected {
                    lines.push((frame.t, "haut", candidate));
                }
                for candidate in &frame.bottom_rejected {
                    lines.push((frame.t, "bas", candidate));
                }
            }
            if lines.is_empty() {
                continue;
            }
            let _ = writeln!(out, "### {} — {}\n", report.fixture, report.pre);
            let _ = writeln!(out, "| t | sens | ligne | couverture | saut | run |");
            let _ = writeln!(out, "| ---: | --- | ---: | ---: | ---: | ---: |");
            for (t, direction, candidate) in lines {
                let _ = writeln!(
                    out,
                    "| {t:.2} | {direction} | {} | {:.3} | {:.4} | {} |",
                    candidate.row, candidate.coverage, candidate.jump, candidate.longest_run
                );
            }
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "## Synthèse\n");
    let matches = reports
        .iter()
        .filter(|report| report.reference.expected_decision == report.native.kind)
        .count();
    let false_apply = reports
        .iter()
        .filter(|report| {
            report.reference.expected_decision == "noop" && report.native.kind == "apply"
        })
        .count();
    let false_noop = reports
        .iter()
        .filter(|report| {
            report.reference.expected_decision == "apply" && report.native.kind == "noop"
        })
        .count();
    let known_fn = reports
        .iter()
        .filter(|report| report.known_false_negative && report.native.kind == "noop")
        .count();
    let max_error = reports
        .iter()
        .filter(|report| !report.known_false_negative)
        .filter_map(
            |report| match (report.reference.error_top, report.reference.error_bottom) {
                (Some(top), Some(bottom)) => Some(top.abs().max(bottom.abs())),
                _ => None,
            },
        )
        .max()
        .unwrap_or(0);
    let _ = writeln!(
        out,
        "- Décisions conformes à l'attendu natif : **{matches}/{}**, dont **{known_fn}** \
         faux négatif(s) connu(s) (304 seg 34, barre sombre).",
        reports.len()
    );
    let _ = writeln!(
        out,
        "- Faux `apply` sur segments sans barre : **{false_apply}**."
    );
    let _ = writeln!(
        out,
        "- Faux `noop` sur letterbox attendu : **{false_noop}**."
    );
    let _ = writeln!(
        out,
        "- Erreur pixel max vs référence, hors faux négatif connu : **{max_error} px**."
    );
    for mode in ["gray", "chroma"] {
        let mut ratios: Vec<f64> = reports
            .iter()
            .filter(|report| report.pre == mode)
            .map(|report| report.timings.ratio_total_vs_cropdetect16)
            .collect();
        if !ratios.is_empty() {
            let ratio_median = median(&mut ratios);
            let _ = writeln!(
                out,
                "- Ratio temps natif / `cropdetect=24:16:0` ({mode}) : \
                 **{ratio_median:.2}×**."
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ewma_first_value_and_slow_ramp() {
        let mut stats = Ewma::new();
        stats.add(0.1);
        assert_eq!(stats.threshold().map(|value| value > 0.1), Some(true));
        let threshold = stats.threshold().expect("seuil");
        assert!((threshold - 0.125).abs() < 1e-12);
        for value in [0.101, 0.102, 0.103, 0.104] {
            stats.add(value);
        }
        assert!(stats.threshold().expect("seuil") > 0.104);
    }

    #[test]
    fn edge_coverage_distinguishes_full_line_and_text() {
        let width = 64;
        let height = 8;
        let mut full = vec![0f32; width * height];
        for y in 4..height {
            for x in 0..width {
                full[y * width + x] = 200.0;
            }
        }
        let sample = SampleSpec {
            start: 0.0,
            duration: 1.0,
            fps: 1.0,
            width,
            height,
            mode: PreMode::Gray,
            edge_threshold: 32.0,
            dark_coverage: REF_THR,
            dark_luma: DARK_LUMA,
        };
        let (full_coverage, full_run) = edge_coverage(&full, &sample, 4, 32.0);
        assert!(full_coverage > 0.95);
        assert_eq!(full_run, width);

        let mut text = vec![0f32; width * height];
        for y in 4..height {
            for x in (0..width).step_by(8) {
                text[y * width + x] = 200.0;
            }
        }
        let (text_coverage, _) = edge_coverage(&text, &sample, 4, 32.0);
        assert!(text_coverage < REF_THR);
    }

    #[test]
    fn coverage_threshold_relaxes_dark_bar_only() {
        let width = 64;
        let height = 8;
        let sample = SampleSpec {
            start: 0.0,
            duration: 1.0,
            fps: 1.0,
            width,
            height,
            mode: PreMode::Gray,
            edge_threshold: 20.0,
            dark_coverage: DARK_COVERAGE,
            dark_luma: DARK_LUMA,
        };
        // Barre noire en haut (lignes 0-3), image claire à partir de 4.
        let mut means = vec![0.0; height];
        for value in means.iter_mut().skip(4) {
            *value = 200.0 / 255.0;
        }
        assert_eq!(coverage_threshold(&means, 4, &sample, true), DARK_COVERAGE);
        // Côté bas, la « barre » est claire : pas de relaxation.
        assert_eq!(coverage_threshold(&means, 3, &sample, false), REF_THR);
        // Ciel nocturne sombre mais non uniforme : moyenne > 20/255.
        let mut sky = vec![40.0 / 255.0; height];
        for value in sky.iter_mut().skip(4) {
            *value = 200.0 / 255.0;
        }
        assert_eq!(coverage_threshold(&sky, 4, &sample, true), REF_THR);
        // La relaxation est désactivée quand dark_coverage vaut REF_THR.
        let disabled = SampleSpec {
            dark_coverage: REF_THR,
            ..sample
        };
        assert_eq!(coverage_threshold(&means, 4, &disabled, true), REF_THR);
    }

    #[test]
    fn clustering_rejects_split_vote() {
        let frames = vec![
            measure(81, 78),
            measure(81, 78),
            measure(29, 30),
            measure(0, 0),
        ];
        let clusters = cluster_frames(&frames);
        assert_eq!(clusters[0].count, 2);
        assert_eq!(clusters[0].fraction, 0.5);
        let decision = decide_native(&clusters.first().cloned(), frames.len(), 1080);
        assert_eq!(decision.kind, "review");
    }

    #[test]
    fn clustering_accepts_dominant_noop() {
        let frames = vec![
            measure(0, 0),
            measure(0, 0),
            measure(935, 76),
            measure(936, 76),
            measure(0, 0),
        ];
        let clusters = cluster_frames(&frames);
        assert_eq!(clusters[0].count, 3);
        assert_eq!(clusters[0].fraction, 0.6);
        let decision = decide_native(&clusters.first().cloned(), frames.len(), 1080);
        assert_eq!(decision.kind, "noop");
    }

    fn measure(top: u32, bottom: u32) -> FrameMeasure {
        FrameMeasure {
            index: 0,
            t: 0.0,
            top_row: top,
            bottom_row: 1079 - bottom,
            top_bar: top,
            bottom_bar: bottom,
            top_coverage: 1.0,
            bottom_coverage: 1.0,
            top_jump: 0.0,
            bottom_jump: 0.0,
            top_run: 0,
            bottom_run: 0,
            top_rejected: Vec::new(),
            bottom_rejected: Vec::new(),
        }
    }
}
