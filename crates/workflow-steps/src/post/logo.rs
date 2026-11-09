//! Annotation A2 par le détecteur de logos B1/B2 (rapport seul).
//!
//! Le module ne prend aucune décision : il échantillonne 12 frames, agrège
//! les détections en clusters temporels et insère `details.logo_detector`
//! dans les décisions A2. `action`, `reason`, `confidence` et `scale_target`
//! ne sont jamais modifiés. Toute erreur (modèle absent, verrou GPU occupé,
//! OOM, décodage) laisse la décision A2 strictement intacte.
//!
//! Le verrou matériel n'est tenu que pendant le chargement et l'inférence :
//! la session CUDA est détruite avant de le relâcher, pour ne pas laisser de
//! VRAM résiduelle entre deux projets.

use crate::post::detect::VideoSpecs;
use crate::post::model::Decision;
use crate::post::run::PostOptions;
use crate::vision::frame::grab_frame;
use crate::vision::inference::init_ort;
use crate::vision::logo_detector::{iou, Detection, DetectorConfig, ExecutionDevice, LogoDetector};
use crate::vision::source_logo::SourceLogoPrior;

use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use workflow_core::HardwareLock;
use workflow_schema::round::round_to;

const LOGO_DETECTOR_SAMPLES: usize = 12;
const LOGO_DETECTOR_MIN_TOP_PERSISTENCE: f64 = 0.25;

/// Annotateur paresseux, créé une fois par lot `post run`.
pub(crate) struct LogoAnnotation {
    settings: Option<Settings>,
    disabled: bool,
}

struct Settings {
    detector: DetectorConfig,
    min_persistence: f64,
    lock_path: PathBuf,
}

impl LogoAnnotation {
    pub(crate) fn new(options: &PostOptions) -> Self {
        if !options.a2_logo_detector {
            return Self {
                settings: None,
                disabled: false,
            };
        }
        if !options.a2_logo_detector_model.is_file() {
            tracing::warn!(
                "A2: modèle de détection introuvable ({}), annotation ignorée",
                options.a2_logo_detector_model.display()
            );
            return Self {
                settings: None,
                disabled: true,
            };
        }
        let detector = DetectorConfig {
            model: options.a2_logo_detector_model.clone(),
            device: options.a2_logo_detector_device,
            onnxruntime: options.a2_logo_detector_onnxruntime.clone(),
            input_size: options.a2_logo_detector_size,
            conf_threshold: options.a2_logo_detector_conf,
            iou_threshold: options.a2_logo_detector_iou,
            class_names: vec!["logo".into()],
            roi: options.a2_logo_detector_roi,
        };
        // Initialise ONNX Runtime avant A2 OCR : sans cela, le gate OCR
        // chargerait le runtime CPU de production et le détecteur CUDA ne
        // pourrait plus changer de bibliothèque dans le même processus.
        if let Err(error) = init_ort(options.a2_logo_detector_onnxruntime.as_deref()) {
            tracing::warn!("A2: détecteur de logos désactivé, ONNX Runtime: {error}");
            return Self {
                settings: None,
                disabled: true,
            };
        }
        Self {
            settings: Some(Settings {
                detector,
                min_persistence: options.a2_logo_detector_min_persistence,
                lock_path: options.hardware_lock.clone(),
            }),
            disabled: false,
        }
    }

    /// Annote les décisions A2 et retourne la durée totale (chargement,
    /// décodage ffmpeg, inférence, agrégation) en millisecondes.
    pub(crate) fn annotate(
        &mut self,
        decisions: &mut [Decision],
        video: Option<&Path>,
        specs: Option<&VideoSpecs>,
        ffmpeg: &Path,
        source_logo: Option<&SourceLogoPrior>,
    ) -> Option<f64> {
        if self.disabled {
            return None;
        }
        let settings = self.settings.as_ref()?;
        let (Some(video), Some(specs)) = (video, specs) else {
            return None;
        };
        if specs.width == 0 || specs.height == 0 || specs.duration_s <= 0.0 {
            return None;
        }
        let started = Instant::now();
        let lock = match settings.detector.device {
            ExecutionDevice::Cuda => match HardwareLock::acquire(&settings.lock_path, false) {
                Ok(lock) => Some(lock),
                Err(error) => {
                    tracing::warn!(
                        "A2: détecteur de logos ignoré, verrou matériel {} indisponible: {error}",
                        settings.lock_path.display()
                    );
                    return None;
                }
            },
            ExecutionDevice::Cpu => None,
        };
        let mut detector = match LogoDetector::load(settings.detector.clone()) {
            Ok(detector) => detector,
            Err(error) => {
                tracing::warn!("A2: détecteur de logos indisponible: {error:#}");
                self.disabled = true;
                return None;
            }
        };
        let mut observations = Vec::new();
        let mut latencies = Vec::new();
        for index in 0..LOGO_DETECTOR_SAMPLES {
            let time = specs.duration_s * (index as f64 + 0.5) / LOGO_DETECTOR_SAMPLES as f64;
            let frame = match grab_frame(
                ffmpeg,
                video,
                time,
                specs.width as usize,
                specs.height as usize,
            ) {
                Ok(Some(frame)) => frame,
                Ok(None) => continue,
                Err(error) => {
                    tracing::debug!("A2: frame de détection ignorée à {time:.2}s: {error:#}");
                    continue;
                }
            };
            let start = Instant::now();
            match detector.detect(&frame) {
                Ok(detections) => {
                    latencies.push(start.elapsed().as_secs_f64() * 1000.0);
                    observations.push((index, detections));
                }
                Err(error) => {
                    tracing::warn!("A2: inférence de logos en échec à {time:.2}s: {error:#}");
                }
            }
        }
        if observations.is_empty() {
            tracing::warn!(
                "A2: aucune frame exploitable pour le détecteur de logos ({})",
                video.display()
            );
            return None;
        }
        let details = build_details(
            DetectorMeta {
                model: detector
                    .config()
                    .model
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("logo_detector"),
                sha256: detector.model_sha256(),
                input_size: detector.config().input_size,
                roi: detector.config().roi.as_str(),
                conf: detector.config().conf_threshold,
                iou: detector.config().iou_threshold,
                min_persistence: settings.min_persistence,
            },
            &observations,
            specs,
            source_logo,
            a2_bbox(decisions),
            &latencies,
        );
        drop(detector);
        drop(lock);
        apply_to_a2(decisions, details);
        Some(started.elapsed().as_secs_f64() * 1000.0)
    }
}

struct DetectorMeta<'a> {
    model: &'a str,
    sha256: &'a str,
    input_size: u32,
    roi: &'a str,
    conf: f32,
    iou: f32,
    min_persistence: f64,
}

#[derive(Debug, Clone)]
struct Cluster {
    bbox: [f32; 4],
    best_score: f32,
    score_sum: f64,
    score_count: usize,
    frames: BTreeSet<usize>,
}

impl Cluster {
    fn new(detection: &Detection, frame: usize) -> Self {
        let mut frames = BTreeSet::new();
        frames.insert(frame);
        Self {
            bbox: detection.bbox,
            best_score: detection.score,
            score_sum: detection.score as f64,
            score_count: 1,
            frames,
        }
    }

    fn add(&mut self, detection: &Detection, frame: usize) {
        if detection.score > self.best_score {
            self.best_score = detection.score;
            self.bbox = detection.bbox;
        }
        self.score_sum += detection.score as f64;
        self.score_count += 1;
        self.frames.insert(frame);
    }

    fn persistence(&self, frames_analyzed: usize) -> f64 {
        if frames_analyzed == 0 {
            0.0
        } else {
            self.frames.len() as f64 / frames_analyzed as f64
        }
    }

    fn mean_score(&self) -> f64 {
        if self.score_count == 0 {
            0.0
        } else {
            self.score_sum / self.score_count as f64
        }
    }
}

/// Agrège les détections en clusters stables : IoU ≥ 0,50 et centre ≤ 10 px.
fn cluster_detections(observations: &[(usize, Vec<Detection>)]) -> Vec<Cluster> {
    let mut clusters: Vec<Cluster> = Vec::new();
    for (frame, detections) in observations {
        let mut sorted = detections.clone();
        sorted.sort_by(|left, right| {
            right
                .score
                .partial_cmp(&left.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for detection in &sorted {
            let center = bbox_center(detection.bbox);
            let matched = clusters.iter_mut().find(|cluster| {
                iou(&cluster.bbox, &detection.bbox) >= 0.50
                    && center_distance(&bbox_center(cluster.bbox), &center) <= 10.0
            });
            if let Some(cluster) = matched {
                cluster.add(detection, *frame);
            } else {
                clusters.push(Cluster::new(detection, *frame));
            }
        }
    }
    clusters.sort_by(|left, right| {
        right
            .persistence(observations.len())
            .partial_cmp(&left.persistence(observations.len()))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .best_score
                    .partial_cmp(&left.best_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    clusters
}

fn build_details(
    meta: DetectorMeta<'_>,
    observations: &[(usize, Vec<Detection>)],
    specs: &VideoSpecs,
    source_logo: Option<&SourceLogoPrior>,
    a2_bbox: Option<[f64; 4]>,
    latencies: &[f64],
) -> Value {
    let frames_analyzed = observations.len();
    let mut detections_total = 0usize;
    let mut filtered: Vec<(usize, Vec<Detection>)> = Vec::new();
    for (frame, detections) in observations {
        detections_total += detections.len();
        let kept: Vec<Detection> = detections
            .iter()
            .filter(|detection| in_a2_roi(detection.bbox, specs.width, specs.height, a2_bbox))
            .cloned()
            .collect();
        filtered.push((*frame, kept));
    }
    let detections_a2 = filtered.iter().map(|(_, dets)| dets.len()).sum::<usize>();
    let clusters = cluster_detections(&filtered);
    let persistent = clusters
        .iter()
        .filter(|cluster| cluster.persistence(frames_analyzed) >= meta.min_persistence)
        .count();
    let dominant = clusters.first();
    let source_logo_match = dominant
        .and_then(|cluster| source_logo.map(|prior| matches_prior(cluster.bbox, prior)))
        .unwrap_or(false);
    let detected = dominant
        .map(|cluster| cluster.persistence(frames_analyzed) >= meta.min_persistence)
        .unwrap_or(false);

    let mut object = Map::new();
    object.insert("backend".into(), json!("onnx"));
    object.insert("model".into(), json!(meta.model));
    object.insert("model_sha256".into(), json!(meta.sha256));
    object.insert("input_size".into(), json!(meta.input_size));
    object.insert("roi".into(), json!(meta.roi));
    object.insert("conf".into(), json!(round_to(meta.conf as f64, 3)));
    object.insert("iou".into(), json!(round_to(meta.iou as f64, 3)));
    object.insert(
        "min_persistence".into(),
        json!(round_to(meta.min_persistence, 3)),
    );
    object.insert("samples_requested".into(), json!(LOGO_DETECTOR_SAMPLES));
    object.insert("frames_analyzed".into(), json!(frames_analyzed));
    object.insert("detections_total".into(), json!(detections_total));
    object.insert("detections_a2".into(), json!(detections_a2));
    object.insert("clusters".into(), json!(persistent));

    let mut logo = Map::new();
    logo.insert("detected".into(), json!(detected));
    if let Some(cluster) = dominant {
        let persistence = cluster.persistence(frames_analyzed);
        logo.insert("persistence".into(), json!(round_to(persistence, 4)));
        logo.insert("frames".into(), json!(cluster.frames.len()));
        logo.insert("bbox".into(), json!(rounded_bbox(cluster.bbox)));
        logo.insert("score".into(), json!(round_to(cluster.mean_score(), 3)));
        logo.insert(
            "score_max".into(),
            json!(round_to(cluster.best_score as f64, 3)),
        );
        let width_frac =
            ((cluster.bbox[2] - cluster.bbox[0]) as f64 / specs.width as f64).clamp(0.0, 1.0);
        logo.insert("width_frac".into(), json!(round_to(width_frac, 4)));
        logo.insert("wide".into(), json!(width_frac > 0.35));
        logo.insert("source_prior".into(), json!(source_logo.is_some()));
        logo.insert("source_logo_match".into(), json!(source_logo_match));
        logo.insert("unknown_logo".into(), json!(detected && !source_logo_match));
        logo.insert(
            "merged_with_lower_third".into(),
            json!(
                detected
                    && width_frac > 0.35
                    && a2_bbox
                        .map(|band| overlap_ratio(cluster.bbox, band) >= 0.50)
                        .unwrap_or(false)
            ),
        );
    } else {
        logo.insert("persistence".into(), json!(0.0));
        logo.insert("frames".into(), json!(0));
        logo.insert("bbox".into(), Value::Null);
        logo.insert("score".into(), Value::Null);
        logo.insert("score_max".into(), Value::Null);
        logo.insert("width_frac".into(), Value::Null);
        logo.insert("wide".into(), json!(false));
        logo.insert("source_prior".into(), json!(source_logo.is_some()));
        logo.insert("source_logo_match".into(), json!(false));
        logo.insert("unknown_logo".into(), json!(false));
        logo.insert("merged_with_lower_third".into(), json!(false));
    }
    object.insert("logo".into(), Value::Object(logo));

    let top: Vec<Value> = clusters
        .iter()
        .filter(|cluster| cluster.persistence(frames_analyzed) >= LOGO_DETECTOR_MIN_TOP_PERSISTENCE)
        .take(3)
        .map(|cluster| {
            let width_frac =
                ((cluster.bbox[2] - cluster.bbox[0]) as f64 / specs.width as f64).clamp(0.0, 1.0);
            json!({
                "bbox": rounded_bbox(cluster.bbox),
                "persistence": round_to(cluster.persistence(frames_analyzed), 4),
                "frames": cluster.frames.len(),
                "score": round_to(cluster.mean_score(), 3),
                "score_max": round_to(cluster.best_score as f64, 3),
                "width_frac": round_to(width_frac, 4),
                "wide": width_frac > 0.35,
            })
        })
        .collect();
    object.insert("top".into(), Value::Array(top));
    object.insert("latency_ms".into(), latency_json(latencies));
    Value::Object(object)
}

fn apply_to_a2(decisions: &mut [Decision], details: Value) {
    for decision in decisions
        .iter_mut()
        .filter(|decision| decision.code == "A2")
    {
        if let Value::Object(object) = &mut decision.details {
            object.insert("logo_detector".into(), details.clone());
        }
    }
}

fn a2_bbox(decisions: &[Decision]) -> Option<[f64; 4]> {
    let values = decisions
        .iter()
        .find(|decision| decision.code == "A2")?
        .details
        .get("bbox")?
        .as_array()?;
    if values.len() != 4 {
        return None;
    }
    Some([
        values[0].as_f64()?,
        values[1].as_f64()?,
        values[2].as_f64()?,
        values[3].as_f64()?,
    ])
}

fn in_a2_roi(bbox: [f32; 4], width: u32, height: u32, a2_bbox: Option<[f64; 4]>) -> bool {
    let width = width.max(1) as f64;
    let height = height.max(1) as f64;
    let center = bbox_center(bbox);
    if let Some(band) = a2_bbox {
        let margin_x = 0.05 * width;
        let margin_y = 0.10 * height;
        if center[0] as f64 >= band[0] - margin_x
            && center[0] as f64 <= band[2] + margin_x
            && center[1] as f64 >= band[1] - margin_y
            && center[1] as f64 <= band[3] + margin_y
        {
            return true;
        }
    }
    let fx = center[0] as f64 / width;
    let fy = center[1] as f64 / height;
    (fx <= 0.40 && fy >= 0.55) || (fx <= 0.40 && fy <= 0.30)
}

fn matches_prior(bbox: [f32; 4], prior: &SourceLogoPrior) -> bool {
    let prior_bbox = [
        prior.bbox[0] as f32,
        prior.bbox[1] as f32,
        prior.bbox[2] as f32,
        prior.bbox[3] as f32,
    ];
    let intersection = intersection_area(bbox, prior_bbox);
    let det_area = bbox_area(bbox);
    let prior_area = bbox_area(prior_bbox);
    let union = det_area + prior_area - intersection;
    let iou = if union > 0.0 {
        intersection / union
    } else {
        0.0
    };
    let min_area = det_area.min(prior_area).max(1.0);
    let prior_center = bbox_center(prior_bbox);
    iou >= 0.50 || intersection / min_area >= 0.50 || point_in_bbox(prior_center, bbox)
}

fn bbox_center(bbox: [f32; 4]) -> [f32; 2] {
    [(bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0]
}

fn center_distance(left: &[f32; 2], right: &[f32; 2]) -> f32 {
    ((left[0] - right[0]).powi(2) + (left[1] - right[1]).powi(2)).sqrt()
}

fn bbox_area(bbox: [f32; 4]) -> f32 {
    (bbox[2] - bbox[0]).max(0.0) * (bbox[3] - bbox[1]).max(0.0)
}

fn intersection_area(left: [f32; 4], right: [f32; 4]) -> f32 {
    let x1 = left[0].max(right[0]);
    let y1 = left[1].max(right[1]);
    let x2 = left[2].min(right[2]);
    let y2 = left[3].min(right[3]);
    (x2 - x1).max(0.0) * (y2 - y1).max(0.0)
}

fn overlap_ratio(bbox: [f32; 4], band: [f64; 4]) -> f32 {
    let band = [
        band[0] as f32,
        band[1] as f32,
        band[2] as f32,
        band[3] as f32,
    ];
    let intersection = intersection_area(bbox, band);
    intersection / bbox_area(bbox).max(1.0)
}

fn point_in_bbox(point: [f32; 2], bbox: [f32; 4]) -> bool {
    point[0] >= bbox[0] && point[0] <= bbox[2] && point[1] >= bbox[1] && point[1] <= bbox[3]
}

fn rounded_bbox(bbox: [f32; 4]) -> Value {
    json!([
        round_to(bbox[0] as f64, 1),
        round_to(bbox[1] as f64, 1),
        round_to(bbox[2] as f64, 1),
        round_to(bbox[3] as f64, 1),
    ])
}

fn latency_json(latencies: &[f64]) -> Value {
    if latencies.is_empty() {
        return Value::Null;
    }
    let mut sorted = latencies.to_vec();
    sorted.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let percentile = |value: f64| {
        let index = ((sorted.len() - 1) as f64 * value).round() as usize;
        sorted[index.min(sorted.len() - 1)]
    };
    json!({
        "p50": round_to(percentile(0.50), 2),
        "p95": round_to(percentile(0.95), 2),
        "mean": round_to(sorted.iter().sum::<f64>() / sorted.len() as f64, 2),
        "samples": sorted.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn specs() -> VideoSpecs {
        VideoSpecs {
            width: 1920,
            height: 1080,
            fps: 25.0,
            codec: "h264".into(),
            duration_s: 60.0,
            bit_rate: 1_000_000,
            size_bytes: 1_000_000,
        }
    }

    fn meta() -> DetectorMeta<'static> {
        DetectorMeta {
            model: "logo_detector",
            sha256: "abc",
            input_size: 416,
            roi: "full",
            conf: 0.30,
            iou: 0.65,
            min_persistence: 0.70,
        }
    }

    fn detection(bbox: [f32; 4], score: f32) -> Detection {
        Detection {
            class_id: 0,
            label: "logo".into(),
            score,
            bbox,
        }
    }

    fn decision(action: crate::post::model::Action, details: Value) -> Decision {
        Decision::new("A2", action, "comp", "raison", 0.5, details)
    }

    #[test]
    fn persistence_boundaries_follow_the_70_percent_rule() {
        // Given: un cluster présent 8 frames sur 12 puis 9 sur 12.
        // When:  la persistance est calculée.
        // Then:  8/12 reste sous le seuil, 9/12 le dépasse.
        let mut observations = Vec::new();
        for frame in 0..12 {
            let dets = if frame < 8 {
                vec![detection([100.0, 900.0, 300.0, 1000.0], 0.8)]
            } else {
                Vec::new()
            };
            observations.push((frame, dets));
        }
        let details = build_details(meta(), &observations, &specs(), None, None, &[]);
        assert_eq!(details["logo"]["detected"], false);
        assert_eq!(details["logo"]["persistence"], 0.6667);

        let mut observations = Vec::new();
        for frame in 0..12 {
            let dets = if frame < 9 {
                vec![detection([100.0, 900.0, 300.0, 1000.0], 0.8)]
            } else {
                Vec::new()
            };
            observations.push((frame, dets));
        }
        let details = build_details(meta(), &observations, &specs(), None, None, &[]);
        assert_eq!(details["logo"]["detected"], true);
        assert_eq!(details["logo"]["persistence"], 0.75);
        assert_eq!(details["clusters"], 1);
    }

    #[test]
    fn a2_roi_accepts_bottom_left_and_a2_band_only() {
        // Given: une frame 1920x1080 et une bande A2 centrée.
        // When:  des centres sont testés.
        // Then:  le coin bas-gauche et la bande passent, le coin haut-droit non.
        let band = Some([422.0, 928.0, 1498.0, 1032.0]);
        assert!(in_a2_roi([100.0, 900.0, 300.0, 1000.0], 1920, 1080, band));
        assert!(in_a2_roi([900.0, 950.0, 1100.0, 1020.0], 1920, 1080, band));
        assert!(in_a2_roi([100.0, 100.0, 300.0, 200.0], 1920, 1080, band));
        assert!(!in_a2_roi([1700.0, 100.0, 1900.0, 200.0], 1920, 1080, band));
    }

    #[test]
    fn annotation_preserves_the_a2_decision() {
        // Given: une décision A2 avec action/reason/confidence/scale.
        // When:  le bloc détecteur est inséré.
        // Then:  seuls les details changent.
        let mut decisions = vec![decision(
            crate::post::model::Action::Review,
            json!({"bbox": [100, 900, 300, 1000], "scale_target": 125.0}),
        )];
        let original = decisions[0].clone();
        apply_to_a2(&mut decisions, json!({"logo": {"detected": true}}));
        assert_eq!(decisions[0].action, original.action);
        assert_eq!(decisions[0].reason, original.reason);
        assert_eq!(decisions[0].confidence, original.confidence);
        assert_eq!(
            decisions[0].details["scale_target"],
            original.details["scale_target"]
        );
        assert_eq!(
            decisions[0].details["logo_detector"]["logo"]["detected"],
            true
        );
    }

    #[test]
    fn source_prior_match_accepts_containment_and_rejects_iou_below_half() {
        let prior = SourceLogoPrior {
            name: "Test".into(),
            channel: "W9".into(),
            position: "bottom_left".into(),
            bbox: [100.0, 900.0, 300.0, 1000.0],
            variants: Vec::new(),
        };
        assert!(matches_prior([100.0, 900.0, 300.0, 1000.0], &prior));
        assert!(matches_prior([120.0, 910.0, 280.0, 990.0], &prior));
        assert!(!matches_prior([700.0, 100.0, 900.0, 200.0], &prior));
    }

    #[test]
    fn cluster_splits_when_center_moves_more_than_ten_pixels() {
        // Given: deux détections de même taille décalées de 12 px.
        // When:  elles sont agrégées.
        // Then:  deux clusters distincts sont créés.
        let observations = vec![
            (0, vec![detection([100.0, 900.0, 300.0, 1000.0], 0.8)]),
            (1, vec![detection([112.0, 900.0, 312.0, 1000.0], 0.8)]),
        ];
        let clusters = cluster_detections(&observations);
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn unknown_logo_is_true_without_a_catalog_prior() {
        let observations: Vec<(usize, Vec<Detection>)> = (0..12)
            .map(|frame| (frame, vec![detection([100.0, 900.0, 300.0, 1000.0], 0.8)]))
            .collect();
        let details = build_details(meta(), &observations, &specs(), None, None, &[]);
        assert_eq!(details["logo"]["detected"], true);
        assert_eq!(details["logo"]["source_prior"], false);
        assert_eq!(details["logo"]["unknown_logo"], true);
        assert_eq!(details["logo"]["source_logo_match"], false);
    }

    #[test]
    fn disabled_detector_does_not_annotate() {
        let options = PostOptions {
            a2_logo_detector: false,
            ..PostOptions::default()
        };
        let mut annotation = LogoAnnotation::new(&options);
        let mut decisions = vec![decision(
            crate::post::model::Action::Review,
            json!({"bbox": [100, 900, 300, 1000]}),
        )];
        let result = annotation.annotate(
            &mut decisions,
            Some(Path::new("/inexistant.mp4")),
            Some(&specs()),
            Path::new("ffmpeg"),
            None,
        );
        assert!(result.is_none());
        assert!(decisions[0].details.get("logo_detector").is_none());
    }

    #[test]
    fn unavailable_model_leaves_a2_intact() {
        let options = PostOptions {
            a2_logo_detector: true,
            a2_logo_detector_model: PathBuf::from("/inexistant/logo.onnx"),
            a2_logo_detector_device: ExecutionDevice::Cpu,
            ..PostOptions::default()
        };
        let mut annotation = LogoAnnotation::new(&options);
        let mut decisions = vec![decision(
            crate::post::model::Action::Review,
            json!({"bbox": [100, 900, 300, 1000], "scale_target": 125.0}),
        )];
        let original = decisions[0].clone();
        let result = annotation.annotate(
            &mut decisions,
            Some(Path::new("/inexistant.mp4")),
            Some(&specs()),
            Path::new("ffmpeg"),
            None,
        );
        assert!(result.is_none());
        assert_eq!(decisions[0].action, original.action);
        assert_eq!(decisions[0].reason, original.reason);
        assert_eq!(decisions[0].confidence, original.confidence);
        assert_eq!(decisions[0].details["scale_target"], 125.0);
        assert!(decisions[0].details.get("logo_detector").is_none());
    }
}
