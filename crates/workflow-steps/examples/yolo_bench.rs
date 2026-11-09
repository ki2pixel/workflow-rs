//! Microbenchmark labo pour les détecteurs YOLOX-Nano et NanoDet-Plus.
//!
//! Ce binaire n'est **pas** utilisé par le CLI, le serveur ou A2. Il charge un
//! modèle ONNX exporté hors production, lit des frames brutes BGR `1920×1080`
//! (`.bgr`) décrites par un JSON, puis mesure séparément le prétraitement, la
//! session `ort` et le post-traitement.
//!
//! Exemple :
//!
//! ```sh
//! cargo run --release -p workflow-steps --example yolo_bench -- \
//!   --model /tmp/opencode/yolo-a2/onnx/yolox_nano_640.onnx \
//!   --frames /tmp/opencode/yolo-a2/frames/selection.json \
//!   --kind yolox --size 640 --device cuda \
//!   --onnxruntime /mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2 \
//!   --warmup 20 --iters 100 --output /tmp/opencode/yolo-a2/bench/yolox640.json
//! ```
//!
//! Sur cette machine, exporter `LD_LIBRARY_PATH` avec les paquets `nvidia-*`
//! et le dossier `capi` d'ONNX Runtime avant de lancer (voir
//! `docs/etudes/ETUDE-YOLO-ONNX-A2/results/results.md`).

use anyhow::{bail, Context, Result};
use ort::execution_providers::cuda::{CUDAExecutionProvider, CuDNNConvAlgorithmSearch};
use ort::execution_providers::CPUExecutionProvider;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const YOLOX_CLASSES: usize = 80;
const NANODET_CLASSES: usize = 80;
const NANODET_BINS: usize = 8;
const NANODET_STRIDE: usize = NANODET_CLASSES + 4 * NANODET_BINS;

#[derive(Debug, Deserialize)]
struct FrameSpec {
    id: String,
    source: String,
    negative: bool,
    width: usize,
    height: usize,
    bgr: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Device {
    Cuda,
    Cpu,
}

impl Device {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cuda" => Ok(Self::Cuda),
            "cpu" => Ok(Self::Cpu),
            other => bail!("device inconnu: {other} (attendu: cuda ou cpu)"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Cpu => "cpu",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Yolox,
    Nanodet,
}

impl Kind {
    fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "yolox" => Ok(Self::Yolox),
            "nanodet" => Ok(Self::Nanodet),
            other => bail!("kind inconnu: {other} (attendu: yolox ou nanodet)"),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Yolox => "yolox",
            Self::Nanodet => "nanodet",
        }
    }
}

struct Args {
    model: PathBuf,
    frames: PathBuf,
    output: PathBuf,
    kind: Kind,
    size: usize,
    classes: usize,
    device: Device,
    onnxruntime: Option<PathBuf>,
    warmup: usize,
    iters: usize,
    conf: f32,
    iou: f32,
    limit: Option<usize>,
}

impl Args {
    fn parse() -> Result<Self> {
        let mut model = None;
        let mut frames = None;
        let mut output = None;
        let mut kind = None;
        let mut size = 416usize;
        let mut classes = YOLOX_CLASSES;
        let mut device = Device::Cuda;
        let mut onnxruntime = None;
        let mut warmup = 20usize;
        let mut iters = 100usize;
        let mut conf = 0.25f32;
        let mut iou = 0.45f32;
        let mut limit = None;
        let mut args = std::env::args().skip(1);

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--model" => model = Some(PathBuf::from(next_value(&mut args, "--model")?)),
                "--frames" => frames = Some(PathBuf::from(next_value(&mut args, "--frames")?)),
                "--output" => output = Some(PathBuf::from(next_value(&mut args, "--output")?)),
                "--kind" => kind = Some(Kind::parse(&next_value(&mut args, "--kind")?)?),
                "--size" => size = parse_usize(&next_value(&mut args, "--size")?, "--size")?,
                "--classes" => {
                    classes = parse_usize(&next_value(&mut args, "--classes")?, "--classes")?
                }
                "--device" => device = Device::parse(&next_value(&mut args, "--device")?)?,
                "--onnxruntime" => {
                    onnxruntime = Some(PathBuf::from(next_value(&mut args, "--onnxruntime")?))
                }
                "--warmup" => {
                    warmup = parse_usize(&next_value(&mut args, "--warmup")?, "--warmup")?
                }
                "--iters" => iters = parse_usize(&next_value(&mut args, "--iters")?, "--iters")?,
                "--conf" => conf = parse_f32(&next_value(&mut args, "--conf")?, "--conf")?,
                "--iou" => iou = parse_f32(&next_value(&mut args, "--iou")?, "--iou")?,
                "--limit" => {
                    limit = Some(parse_usize(&next_value(&mut args, "--limit")?, "--limit")?)
                }
                "--help" | "-h" => {
                    print_usage();
                    std::process::exit(0);
                }
                other => bail!("argument inconnu: {other}"),
            }
        }

        let model = model.context("--model est obligatoire")?;
        let frames = frames.context("--frames est obligatoire")?;
        let output = output.unwrap_or_else(|| PathBuf::from("/tmp/yolo_bench.json"));
        let kind = match kind {
            Some(kind) => kind,
            None if model.to_string_lossy().contains("nanodet") => Kind::Nanodet,
            None => Kind::Yolox,
        };
        if size == 0 || classes == 0 || warmup == 0 || iters == 0 {
            bail!("--size, --classes, --warmup et --iters doivent être > 0");
        }
        if !(0.0..=1.0).contains(&conf) || !(0.0..=1.0).contains(&iou) {
            bail!("--conf et --iou doivent être dans [0, 1]");
        }

        Ok(Self {
            model,
            frames,
            output,
            kind,
            size,
            classes,
            device,
            onnxruntime,
            warmup,
            iters,
            conf,
            iou,
            limit,
        })
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String> {
    args.next()
        .with_context(|| format!("valeur manquante pour {flag}"))
}

fn parse_usize(value: &str, flag: &str) -> Result<usize> {
    value
        .parse()
        .with_context(|| format!("{flag} invalide: {value}"))
}

fn parse_f32(value: &str, flag: &str) -> Result<f32> {
    value
        .parse()
        .with_context(|| format!("{flag} invalide: {value}"))
}

fn print_usage() {
    println!(
        "yolo_bench --model <onnx> --frames <selection.json> [options]\n\
         \n\
         Options :\n\
           --kind yolox|nanodet   type de modèle (auto-détecté si absent)\n\
           --size 416|640         taille d'entrée carrée (défaut 416)\n\
           --classes N            nombre de classes YOLOX (défaut 80)\n\
           --device cuda|cpu      provider (défaut cuda)\n\
           --onnxruntime <lib>    bibliothèque ONNX Runtime explicite\n\
           --warmup N             itérations de chauffe (défaut 20)\n\
           --iters N              itérations mesurées par frame (défaut 100)\n\
           --conf F               seuil de score (défaut 0.25)\n\
           --iou F                seuil NMS (défaut 0.45)\n\
           --limit N              limite le nombre de frames\n\
           --output <json>        sortie JSON (défaut /tmp/yolo_bench.json)"
    );
}

#[derive(Clone)]
struct Detection {
    class_id: usize,
    score: f32,
    bbox: [f32; 4],
}

struct Preprocessed {
    data: Vec<f32>,
    scale_x: f32,
    scale_y: f32,
    pad_x: f32,
    pad_y: f32,
}

struct ResizeSpec {
    src_w: usize,
    src_h: usize,
    dst_w: usize,
    dst_h: usize,
    out_w: usize,
    out_h: usize,
}

struct DecodeParams<'a> {
    width: usize,
    height: usize,
    scale_x: f32,
    scale_y: f32,
    pad_x: f32,
    pad_y: f32,
    conf: f32,
    iou: f32,
    classes: usize,
    centers: &'a [(f32, f32)],
    strides: &'a [f32],
}

fn main() -> Result<()> {
    let args = Args::parse()?;
    run(args)
}

fn run(args: Args) -> Result<()> {
    let mut frames: Vec<FrameSpec> = serde_json::from_str(
        &fs::read_to_string(&args.frames)
            .with_context(|| format!("lecture de {}", args.frames.display()))?,
    )
    .with_context(|| format!("JSON de frames invalide: {}", args.frames.display()))?;
    if let Some(limit) = args.limit {
        frames.truncate(limit);
    }
    if frames.is_empty() {
        bail!("aucune frame dans {}", args.frames.display());
    }

    let model_bytes =
        fs::read(&args.model).with_context(|| format!("lecture de {}", args.model.display()))?;
    let model_sha256 = format!("{:x}", Sha256::digest(&model_bytes));
    let model_size = model_bytes.len();
    drop(model_bytes);

    init_runtime(args.onnxruntime.as_deref())?;
    let mut session = build_session(&args)?;
    let output_name = session
        .outputs
        .first()
        .map(|output| output.name.clone())
        .context("le modèle n'a aucune sortie")?;
    let input_shape: Vec<usize> = session
        .inputs
        .first()
        .and_then(|input| input.input_type.tensor_shape())
        .map(|shape| {
            shape
                .iter()
                .map(|dim| usize::try_from(*dim).unwrap_or(0))
                .collect()
        })
        .unwrap_or_default();
    let expected = match args.kind {
        Kind::Yolox => [1, 3, args.size, args.size],
        Kind::Nanodet => [1, 3, args.size, args.size],
    };
    if !input_shape.is_empty() && input_shape != expected {
        bail!(
            "forme d'entrée {} incompatible avec --size {} (attendu {:?})",
            format_shape(&input_shape),
            args.size,
            expected
        );
    }

    let (centers, strides) = match args.kind {
        Kind::Nanodet => nanodet_centers(args.size),
        Kind::Yolox => (Vec::new(), Vec::new()),
    };

    // Chauffe les noyaux CUDA avec la première frame avant toute mesure.
    let first = &frames[0];
    let first_bgr = read_frame(first)?;
    let first_input = preprocess(&first_bgr, first, &args)?;
    for _ in 0..args.warmup {
        let tensor =
            Tensor::from_array(([1usize, 3, args.size, args.size], first_input.data.clone()))
                .context("création du tenseur de chauffe")?;
        let _ = session
            .run(ort::inputs![tensor])
            .context("inférence de chauffe")?;
    }

    let mut results = Vec::with_capacity(frames.len());
    let mut all_pre = Vec::with_capacity(frames.len() * args.iters);
    let mut all_run = Vec::with_capacity(frames.len() * args.iters);
    let mut all_post = Vec::with_capacity(frames.len() * args.iters);
    let mut all_total = Vec::with_capacity(frames.len() * args.iters);

    for frame in &frames {
        let bgr = read_frame(frame)?;
        let mut pre_ms = Vec::with_capacity(args.iters);
        let mut run_ms = Vec::with_capacity(args.iters);
        let mut post_ms = Vec::with_capacity(args.iters);
        let mut total_ms = Vec::with_capacity(args.iters);
        let mut detections = Vec::new();

        for iteration in 0..args.iters {
            let start = Instant::now();
            let input = preprocess(&bgr, frame, &args)?;
            let tensor = Tensor::from_array(([1usize, 3, args.size, args.size], input.data))
                .context("création du tenseur d'entrée")?;
            let pre = start.elapsed().as_secs_f64() * 1000.0;

            let start = Instant::now();
            let outputs = session
                .run(ort::inputs![tensor])
                .context("inférence du détecteur")?;
            let run = start.elapsed().as_secs_f64() * 1000.0;

            let start = Instant::now();
            let value = outputs
                .get(&output_name)
                .with_context(|| format!("sortie {output_name} absente"))?;
            let (shape, data) = value
                .try_extract_tensor::<f32>()
                .context("lecture de la sortie du détecteur")?;
            let decoded = decode(
                data,
                shape,
                &DecodeParams {
                    width: frame.width,
                    height: frame.height,
                    scale_x: input.scale_x,
                    scale_y: input.scale_y,
                    pad_x: input.pad_x,
                    pad_y: input.pad_y,
                    conf: args.conf,
                    iou: args.iou,
                    classes: args.classes,
                    centers: &centers,
                    strides: &strides,
                },
                args.kind,
            )?;
            let post = start.elapsed().as_secs_f64() * 1000.0;
            let total = pre + run + post;

            if iteration == 0 {
                detections = decoded;
            }
            pre_ms.push(pre);
            run_ms.push(run);
            post_ms.push(post);
            total_ms.push(total);
            all_pre.push(pre);
            all_run.push(run);
            all_post.push(post);
            all_total.push(total);
        }

        results.push(json!({
            "id": frame.id,
            "source": frame.source,
            "negative": frame.negative,
            "width": frame.width,
            "height": frame.height,
            "timings_ms": {
                "preprocess": summarize(&mut pre_ms),
                "run": summarize(&mut run_ms),
                "postprocess": summarize(&mut post_ms),
                "total": summarize(&mut total_ms),
            },
            "detections": detections.iter().map(detection_json).collect::<Vec<_>>(),
        }));
    }

    let summary = json!({
        "model": args.model.display().to_string(),
        "model_sha256": model_sha256,
        "model_size_bytes": model_size,
        "kind": args.kind.label(),
        "input_size": args.size,
        "classes": args.classes,
        "input_shape": input_shape,
        "device": args.device.label(),
        "frames": frames.len(),
        "warmup": args.warmup,
        "iterations": args.iters,
        "conf": args.conf,
        "iou": args.iou,
        "timings_ms": {
            "preprocess": summarize(&mut all_pre),
            "run": summarize(&mut all_run),
            "postprocess": summarize(&mut all_post),
            "total": summarize(&mut all_total),
        },
        "per_frame": results,
    });

    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent).with_context(|| format!("création de {}", parent.display()))?;
    }
    fs::write(&args.output, serde_json::to_vec_pretty(&summary)?)
        .with_context(|| format!("écriture de {}", args.output.display()))?;

    let timings = &summary["timings_ms"];
    println!(
        "{} size={} device={} frames={} iters={}",
        args.kind.label(),
        args.size,
        args.device.label(),
        frames.len(),
        args.iters
    );
    for key in ["preprocess", "run", "postprocess", "total"] {
        let value = &timings[key];
        println!(
            "  {key:12} p50={:.2} ms  p95={:.2} ms  mean={:.2} ms",
            value["p50_ms"].as_f64().unwrap_or_default(),
            value["p95_ms"].as_f64().unwrap_or_default(),
            value["mean_ms"].as_f64().unwrap_or_default()
        );
    }
    println!("résultat: {}", args.output.display());
    Ok(())
}

fn init_runtime(runtime: Option<&Path>) -> Result<()> {
    if let Some(runtime) = runtime {
        ort::init_from(runtime.to_string_lossy().to_string())
            .commit()
            .with_context(|| format!("chargement ONNX Runtime: {}", runtime.display()))?;
    } else {
        ort::init()
            .commit()
            .context("initialisation ONNX Runtime système")?;
    }
    Ok(())
}

fn build_session(args: &Args) -> Result<Session> {
    if !args.model.is_file() {
        bail!("modèle introuvable: {}", args.model.display());
    }
    let mut builder = Session::builder()?
        .with_optimization_level(GraphOptimizationLevel::Level3)?
        .with_parallel_execution(false)?
        .with_memory_pattern(true)?;
    let providers = match args.device {
        Device::Cuda => {
            let cuda = CUDAExecutionProvider::default()
                .with_device_id(0)
                .with_conv_algorithm_search(CuDNNConvAlgorithmSearch::Heuristic)
                .with_tf32(false)
                .build()
                .error_on_failure();
            vec![cuda, CPUExecutionProvider::default().build()]
        }
        Device::Cpu => vec![CPUExecutionProvider::default().build()],
    };
    builder = builder
        .with_execution_providers(providers)
        .context("enregistrement des execution providers ONNX Runtime")?;
    builder
        .commit_from_file(&args.model)
        .with_context(|| format!("ouverture du modèle {}", args.model.display()))
}

fn read_frame(frame: &FrameSpec) -> Result<Vec<u8>> {
    let bytes =
        fs::read(&frame.bgr).with_context(|| format!("lecture de {}", frame.bgr.display()))?;
    let expected = frame.width * frame.height * 3;
    if bytes.len() != expected {
        bail!(
            "frame {}: {} octets au lieu de {}",
            frame.id,
            bytes.len(),
            expected
        );
    }
    Ok(bytes)
}

fn preprocess(bgr: &[u8], frame: &FrameSpec, args: &Args) -> Result<Preprocessed> {
    match args.kind {
        Kind::Yolox => {
            let scale =
                (args.size as f32 / frame.width as f32).min(args.size as f32 / frame.height as f32);
            let new_w = (frame.width as f32 * scale).round().max(1.0) as usize;
            let new_h = (frame.height as f32 * scale).round().max(1.0) as usize;
            let mut padded = vec![114u8; args.size * args.size * 3];
            resize_bilinear_into(
                bgr,
                &mut padded,
                &ResizeSpec {
                    src_w: frame.width,
                    src_h: frame.height,
                    dst_w: args.size,
                    dst_h: args.size,
                    out_w: new_w,
                    out_h: new_h,
                },
            )?;
            Ok(Preprocessed {
                data: to_chw(&padded, args.size, None, None),
                scale_x: scale,
                scale_y: scale,
                pad_x: 0.0,
                pad_y: 0.0,
            })
        }
        Kind::Nanodet => {
            let mut resized = vec![0u8; args.size * args.size * 3];
            resize_bilinear_into(
                bgr,
                &mut resized,
                &ResizeSpec {
                    src_w: frame.width,
                    src_h: frame.height,
                    dst_w: args.size,
                    dst_h: args.size,
                    out_w: args.size,
                    out_h: args.size,
                },
            )?;
            let mean = [103.53 / 255.0, 116.28 / 255.0, 123.675 / 255.0];
            let std = [57.375 / 255.0, 57.12 / 255.0, 58.395 / 255.0];
            Ok(Preprocessed {
                data: to_chw(&resized, args.size, Some(mean), Some(std)),
                scale_x: frame.width as f32 / args.size as f32,
                scale_y: frame.height as f32 / args.size as f32,
                pad_x: 0.0,
                pad_y: 0.0,
            })
        }
    }
}

fn resize_bilinear_into(src: &[u8], dst: &mut [u8], spec: &ResizeSpec) -> Result<()> {
    let ResizeSpec {
        src_w,
        src_h,
        dst_w,
        dst_h,
        out_w,
        out_h,
    } = *spec;
    if src.len() != src_w * src_h * 3 {
        bail!("source BGR de taille invalide");
    }
    if dst.len() != dst_w * dst_h * 3 || out_w > dst_w || out_h > dst_h {
        bail!("destination BGR de taille invalide");
    }
    // Convention demi-pixel d'OpenCV (`INTER_LINEAR`), identique à
    // `vision::frame::resize_bilinear` et aux prétraitements Python.
    let scale_x = src_w as f32 / out_w as f32;
    let scale_y = src_h as f32 / out_h as f32;
    for y in 0..out_h {
        let sy = (y as f32 + 0.5) * scale_y - 0.5;
        let y0 = sy.floor().max(0.0) as usize;
        let y1 = (y0 + 1).min(src_h - 1);
        let wy = (sy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..out_w {
            let sx = (x as f32 + 0.5) * scale_x - 0.5;
            let x0 = sx.floor().max(0.0) as usize;
            let x1 = (x0 + 1).min(src_w - 1);
            let wx = (sx - x0 as f32).clamp(0.0, 1.0);
            let di = (y * dst_w + x) * 3;
            let p00 = (y0 * src_w + x0) * 3;
            let p10 = (y0 * src_w + x1) * 3;
            let p01 = (y1 * src_w + x0) * 3;
            let p11 = (y1 * src_w + x1) * 3;
            for channel in 0..3 {
                let top = src[p00 + channel] as f32 * (1.0 - wx) + src[p10 + channel] as f32 * wx;
                let bottom =
                    src[p01 + channel] as f32 * (1.0 - wx) + src[p11 + channel] as f32 * wx;
                dst[di + channel] =
                    (top * (1.0 - wy) + bottom * wy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Ok(())
}

fn to_chw(bgr: &[u8], size: usize, mean: Option<[f32; 3]>, std: Option<[f32; 3]>) -> Vec<f32> {
    let plane = size * size;
    let mut out = vec![0f32; plane * 3];
    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) * 3;
            for channel in 0..3 {
                let mut value = bgr[index + channel] as f32;
                if let (Some(mean), Some(std)) = (mean, std) {
                    value = (value / 255.0 - mean[channel]) / std[channel];
                }
                out[channel * plane + y * size + x] = value;
            }
        }
    }
    out
}

fn nanodet_centers(size: usize) -> (Vec<(f32, f32)>, Vec<f32>) {
    let mut centers = Vec::new();
    let mut strides = Vec::new();
    for stride in [8usize, 16, 32, 64] {
        let cells = size.div_ceil(stride);
        for y in 0..cells {
            for x in 0..cells {
                centers.push(((x * stride) as f32, (y * stride) as f32));
                strides.push(stride as f32);
            }
        }
    }
    (centers, strides)
}

fn decode(
    data: &[f32],
    shape: &[i64],
    params: &DecodeParams<'_>,
    kind: Kind,
) -> Result<Vec<Detection>> {
    if shape.len() != 3 {
        bail!("sortie de détecteur de rang {} au lieu de 3", shape.len());
    }
    let anchors = shape[1] as usize;
    let stride = shape[2] as usize;
    match kind {
        Kind::Yolox if stride != 5 + params.classes => {
            bail!(
                "sortie YOLOX de largeur {stride} au lieu de {}",
                5 + params.classes
            )
        }
        Kind::Nanodet if stride != NANODET_STRIDE => {
            bail!("sortie NanoDet de largeur {stride} au lieu de {NANODET_STRIDE}")
        }
        _ => {}
    }
    if data.len() < anchors * stride {
        bail!("sortie de détecteur tronquée");
    }
    match kind {
        Kind::Yolox => Ok(decode_yolox(data, anchors, params)),
        Kind::Nanodet => Ok(decode_nanodet(data, anchors, params)),
    }
}

fn decode_yolox(data: &[f32], anchors: usize, params: &DecodeParams<'_>) -> Vec<Detection> {
    let mut detections = Vec::new();
    let stride = 5 + params.classes;
    for anchor in 0..anchors {
        let base = anchor * stride;
        let objectness = data[base + 4];
        if objectness < params.conf {
            continue;
        }
        let mut class_id = 0;
        let mut class_score = 0.0f32;
        for class in 0..params.classes {
            let score = data[base + 5 + class];
            if score > class_score {
                class_score = score;
                class_id = class;
            }
        }
        let score = objectness * class_score;
        if score < params.conf {
            continue;
        }
        let center_x = data[base];
        let center_y = data[base + 1];
        let width = data[base + 2];
        let height = data[base + 3];
        let bbox = [
            center_x - width / 2.0,
            center_y - height / 2.0,
            center_x + width / 2.0,
            center_y + height / 2.0,
        ];
        if let Some(bbox) = map_box(bbox, params) {
            detections.push(Detection {
                class_id,
                score,
                bbox,
            });
        }
    }
    nms(detections, params.iou)
}

fn decode_nanodet(data: &[f32], anchors: usize, params: &DecodeParams<'_>) -> Vec<Detection> {
    if params.centers.len() < anchors {
        return Vec::new();
    }
    let mut detections = Vec::new();
    for anchor in 0..anchors {
        let base = anchor * NANODET_STRIDE;
        let mut class_id = 0;
        let mut score = 0.0f32;
        for class in 0..NANODET_CLASSES {
            let value = data[base + class];
            if value > score {
                score = value;
                class_id = class;
            }
        }
        if score < params.conf {
            continue;
        }
        let stride = params.strides.get(anchor).copied().unwrap_or(8.0);
        let mut distance = [0f32; 4];
        for (side, value) in distance.iter_mut().enumerate() {
            let offset = base + NANODET_CLASSES + side * NANODET_BINS;
            let mut max = data[offset];
            for bin in 1..NANODET_BINS {
                if data[offset + bin] > max {
                    max = data[offset + bin];
                }
            }
            let mut sum = 0.0f32;
            let mut normalizer = 0.0f32;
            for bin in 0..NANODET_BINS {
                let weight = (data[offset + bin] - max).exp();
                sum += bin as f32 * weight;
                normalizer += weight;
            }
            *value = (sum / normalizer) * stride;
        }
        let (center_x, center_y) = params.centers[anchor];
        let bbox = [
            center_x - distance[0],
            center_y - distance[1],
            center_x + distance[2],
            center_y + distance[3],
        ];
        if let Some(bbox) = map_box(bbox, params) {
            detections.push(Detection {
                class_id,
                score,
                bbox,
            });
        }
    }
    nms(detections, params.iou)
}

fn map_box(mut bbox: [f32; 4], params: &DecodeParams<'_>) -> Option<[f32; 4]> {
    for value in bbox.iter_mut() {
        if !value.is_finite() {
            return None;
        }
    }
    bbox[0] = (bbox[0] - params.pad_x) / params.scale_x;
    bbox[2] = (bbox[2] - params.pad_x) / params.scale_x;
    bbox[1] = (bbox[1] - params.pad_y) / params.scale_y;
    bbox[3] = (bbox[3] - params.pad_y) / params.scale_y;
    bbox[0] = bbox[0].clamp(0.0, params.width as f32);
    bbox[1] = bbox[1].clamp(0.0, params.height as f32);
    bbox[2] = bbox[2].clamp(0.0, params.width as f32);
    bbox[3] = bbox[3].clamp(0.0, params.height as f32);
    if bbox[2] <= bbox[0] || bbox[3] <= bbox[1] {
        return None;
    }
    Some(bbox)
}

fn nms(mut detections: Vec<Detection>, threshold: f32) -> Vec<Detection> {
    detections.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
    });
    let mut kept: Vec<Detection> = Vec::new();
    'candidate: for detection in detections {
        for previous in &kept {
            if previous.class_id == detection.class_id
                && iou(&previous.bbox, &detection.bbox) > threshold
            {
                continue 'candidate;
            }
        }
        kept.push(detection);
    }
    kept
}

fn iou(left: &[f32; 4], right: &[f32; 4]) -> f32 {
    let x1 = left[0].max(right[0]);
    let y1 = left[1].max(right[1]);
    let x2 = left[2].min(right[2]);
    let y2 = left[3].min(right[3]);
    let intersection = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let left_area = (left[2] - left[0]).max(0.0) * (left[3] - left[1]).max(0.0);
    let right_area = (right[2] - right[0]).max(0.0) * (right[3] - right[1]).max(0.0);
    let union = left_area + right_area - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn detection_json(detection: &Detection) -> Value {
    json!({
        "class_id": detection.class_id,
        "score": detection.score,
        "bbox": detection.bbox,
    })
}

fn summarize(values: &mut [f64]) -> Value {
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let count = values.len();
    let mean = if count == 0 {
        0.0
    } else {
        values.iter().sum::<f64>() / count as f64
    };
    json!({
        "count": count,
        "p50_ms": percentile(values, 0.50),
        "p95_ms": percentile(values, 0.95),
        "mean_ms": mean,
        "min_ms": values.first().copied().unwrap_or(0.0),
        "max_ms": values.last().copied().unwrap_or(0.0),
    })
}

fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let index = ((values.len() - 1) as f64 * fraction).round() as usize;
    values[index]
}

fn format_shape(shape: &[usize]) -> String {
    format!(
        "[{}]",
        shape
            .iter()
            .map(|dim| dim.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )
}
