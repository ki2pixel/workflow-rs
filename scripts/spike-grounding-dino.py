#!/usr/bin/env python3
"""Spike hors production : Grounding DINO pour A2 et les logos source.

Le script ne touche ni `runtime/`, ni `/mnt/cache`, ni le dépôt Python. Il lit
les références client et le corpus GOOD, extrait des frames dans
`/tmp/opencode/gdino/frames`, puis écrit un manifeste et des métriques sous
`docs/etudes/ETUDE-GROUNDING-DINO-A2/results/`.

Deux usages :

    # Construire le manifeste (lecture seule des corpus)
    /mnt/venv_ext4/audio_env/bin/python scripts/spike-grounding-dino.py \
        build-dataset --frames-dir /tmp/opencode/gdino/frames

    # Exécuter l'évaluation locale (transformers + CUDA)
    PYTHONPATH=/tmp/opencode/gdino/site HF_HOME=/tmp/opencode/gdino/hf \
    /mnt/venv_ext4/audio_env/bin/python scripts/spike-grounding-dino.py \
        run --manifest docs/etudes/ETUDE-GROUNDING-DINO-A2/results/manifest.json

La clé Replicate n'est jamais écrite : elle est lue depuis
`REPLICATE_API_TOKEN` uniquement, et n'est jamais journalisée.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

try:
    import tomllib  # Python >= 3.11
except ModuleNotFoundError:  # pragma: no cover - l'environnement torch est 3.10
    import tomli as tomllib  # type: ignore[no-redef]

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / "assets/models/qa/source_logos.toml"
REFERENCES = Path(
    "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Recadrage/2026/"
    "references_logos/emissions"
)
CORPUS = Path(
    "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2"
)
RESULTS_DIR = ROOT / "docs/etudes/ETUDE-GROUNDING-DINO-A2/results"
DEFAULT_FRAMES = Path("/tmp/opencode/gdino/frames")
MODEL_ID = "IDEA-Research/grounding-dino-tiny"

A2_SAMPLES = 12
PROMPT_LIST = [
    "logo",
    "text",
    "logo . text",
    "lower third",
    "television logo",
    "banner",
    "logo . text . watermark . lower third . banner",
]
BEST_PROMPT = "logo . text . watermark . lower third . banner"

# Cinq clips A2 avec bandeau bas persistant : le logo est connu et la bande
# textuelle est documentée par le post.json de production.
A2_CLIPS = [
    {
        "id": "54_ujud_hotel",
        "project": "54 Camille",
        "video": "UJUD - CLIENT HOTEL.mp4",
        "post": "UJUD - CLIENT HOTEL_9x16_assist/post.json",
        "logo": "Un Jour Un Doc",
    },
    {
        "id": "58_ujud_crousty",
        "project": "58 Camille",
        "video": "UJUD - CROUSTY.mp4",
        "post": "UJUD - CROUSTY_9x16_assist/post.json",
        "logo": "Un Jour Un Doc",
    },
    {
        "id": "40_66m_citrouilles",
        "project": "40 Camille",
        "video": "66M - CITROUILLES.mp4",
        "post": "66M - CITROUILLES_9x16_assist/post.json",
        "logo": "66 Minutes",
    },
    {
        "id": "40_66m_abhartach",
        "project": "40 Camille",
        "video": "66M - ABHARTACH.mp4",
        "post": "66M - ABHARTACH_9x16_assist/post.json",
        "logo": "66 Minutes",
    },
    {
        "id": "40_66m_footballeur",
        "project": "40 Camille",
        "video": "66M - FOOTBALLEUR ESCROC.mp4",
        "post": "66M - FOOTBALLEUR ESCROC_9x16_assist/post.json",
        "logo": "66 Minutes",
    },
]

NEGATIVE_VIDEOS = [
    ("wanted_soiree", CORPUS / "37 Camille/docs/WANTED - SOIREE EM5 rs1.mp4"),
    (
        "pll_extrait5",
        CORPUS
        / "355 Camille/docs/M6+ - Pretty Little Liars - Extrait 5 - Quand Caleb vient s’excuser auprès de Spencer.mp4",
    ),
    (
        "younger_bar",
        CORPUS / "45 Camille/docs/M6+ - Younger – Extrait 1 - Liza et Josh au bar.mp4",
    ),
    (
        "lotr_troll",
        CORPUS
        / "89 Camille/docs/M6 - Le seigneur des anneaux les anneaux de pouvoir - Troll des neiges.mp4",
    ),
    (
        "6ter_petite_maison",
        CORPUS
        / "40 Camille/docs/6ter - La petite maison dans la prairie - Il tombe amoureux de cette belle inconnue.mp4",
    ),
]


# ---------------------------------------------------------------------------
# Utilitaires
# ---------------------------------------------------------------------------


def log(message: str) -> None:
    print(f"[spike-gdino] {message}", flush=True)


def log_err(message: str) -> None:
    print(f"[spike-gdino] {message}", file=sys.stderr, flush=True)


def slug(value: str) -> str:
    value = re.sub(r"[^A-Za-z0-9._-]+", "_", value)
    return value.strip("_")[:100]


def now_iso() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def ffprobe_duration(path: Path) -> float:
    out = subprocess.check_output(
        [
            "ffprobe",
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            str(path),
        ],
        text=True,
    )
    return float(out.strip())


def extract_frame(video: Path, time_s: float, output: Path) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-ss",
            f"{time_s:.3f}",
            "-i",
            str(video),
            "-frames:v",
            "1",
            "-q:v",
            "2",
            "-y",
            str(output),
        ],
        check=True,
    )


def frame_times(duration_s: float, count: int) -> list[float]:
    return [duration_s * (index + 0.5) / count for index in range(count)]


def bbox_xyxy_to_list(values: Iterable[float]) -> list[float]:
    return [round(float(value), 2) for value in values]


def iou_xyxy(a: list[float], b: list[float]) -> float:
    x1 = max(a[0], b[0])
    y1 = max(a[1], b[1])
    x2 = min(a[2], b[2])
    y2 = min(a[3], b[3])
    if x2 <= x1 or y2 <= y1:
        return 0.0
    inter = (x2 - x1) * (y2 - y1)
    area_a = max(0.0, a[2] - a[0]) * max(0.0, a[3] - a[1])
    area_b = max(0.0, b[2] - b[0]) * max(0.0, b[3] - b[1])
    union = area_a + area_b - inter
    return inter / union if union > 0 else 0.0


def center_inside(inner: list[float], outer: list[float], margin: float = 0.0) -> bool:
    cx = (inner[0] + inner[2]) / 2.0
    cy = (inner[1] + inner[3]) / 2.0
    return (
        outer[0] - margin <= cx <= outer[2] + margin
        and outer[1] - margin <= cy <= outer[3] + margin
    )


def clip_bbox(bbox: list[float], width: int, height: int) -> list[float]:
    return [
        max(0.0, min(float(width), bbox[0])),
        max(0.0, min(float(height), bbox[1])),
        max(0.0, min(float(width), bbox[2])),
        max(0.0, min(float(height), bbox[3])),
    ]


def contains(haystack: str, needles: Iterable[str]) -> bool:
    haystack = haystack.lower()
    return any(needle in haystack for needle in needles)


# ---------------------------------------------------------------------------
# Manifeste
# ---------------------------------------------------------------------------


def load_catalog() -> dict[str, Any]:
    with CATALOG.open("rb") as handle:
        return tomllib.load(handle)


def catalog_entries(catalog: dict[str, Any]) -> dict[str, dict[str, Any]]:
    result: dict[str, dict[str, Any]] = {}
    for emission in catalog.get("emissions", []):
        result[emission["name"]] = emission
    return result


def emission_gt_boxes(emission: dict[str, Any]) -> list[list[float]]:
    boxes = [bbox_xyxy_to_list(emission["bbox"])]
    for variant in emission.get("variants", []):
        boxes.append(bbox_xyxy_to_list(variant["bbox"]))
    unique: list[list[float]] = []
    for box in boxes:
        if box not in unique:
            unique.append(box)
    return unique


def load_post_a2(post_path: Path) -> dict[str, Any] | None:
    if not post_path.is_file():
        return None
    payload = json.loads(post_path.read_text(encoding="utf-8"))
    actions = payload.get("actions", payload.get("decisions", []))
    for action in actions:
        if action.get("code") == "A2":
            return action
    return None


def build_dataset(args: argparse.Namespace) -> None:
    frames_dir = Path(args.frames_dir)
    frames_dir.mkdir(parents=True, exist_ok=True)
    catalog = load_catalog()
    entries = catalog_entries(catalog)
    frames: list[dict[str, Any]] = []

    # 1. Références logos : toutes les captures non-crop sont positives. La
    # première de chaque émission sert au comparatif de prompts, les autres à
    # la validation.
    for emission in catalog.get("emissions", []):
        folder = REFERENCES / emission["dossier"]
        if not folder.is_dir():
            continue
        pngs = [
            path
            for path in sorted(folder.glob("*.png"))
            if path.name.lower() != "crop.png"
        ]
        boxes = emission_gt_boxes(emission)
        for index, path in enumerate(pngs):
            try:
                with Image.open(path) as image:
                    width, height = image.size
            except Exception as error:  # noqa: BLE001 - données externes
                log(f"image illisible {path}: {error}")
                continue
            frames.append(
                {
                    "id": f"ref_{slug(emission['dossier'])}_{index:02d}",
                    "source": "reference_logo",
                    "path": str(path),
                    "width": width,
                    "height": height,
                    "emission": emission["name"],
                    "channel": emission.get("channel"),
                    "position": emission.get("position"),
                    "split": "prompt" if index == 0 else "validation",
                    "negative": False,
                    "gt_logo": boxes,
                    "gt_text": None,
                    "gt_band": None,
                    "clip": None,
                    "frame_index": None,
                }
            )

    # 2. Clips A2 : 12 frames aux mêmes instants que le gate OCR. La bande et
    # le texte viennent du post.json réel ; le texte n'est renseigné que quand
    # le groupe OCR dominant est bien le texte de bannière (pas le logo).
    for clip in A2_CLIPS:
        project_dir = CORPUS / clip["project"]
        video_path = project_dir / "docs" / clip["video"]
        post_path = project_dir / "docs/post" / clip["post"]
        if not video_path.is_file():
            log(f"vidéo A2 absente: {video_path}")
            continue
        action = load_post_a2(post_path)
        if action is None:
            log(f"A2 absent dans {post_path}")
            continue
        details = action.get("details", {})
        band = bbox_xyxy_to_list(details["bbox"])
        ocr = details.get("ocr") or {}
        ocr_bbox = ocr.get("bbox")
        ocr_text = ocr.get("text") or ""
        gt_text = None
        if ocr_bbox and len(ocr_bbox) == 4:
            # Le bbox OCR A2 est [x, y, largeur, hauteur].
            x, y, w, h = [float(value) for value in ocr_bbox]
            candidate_text = [x, y, x + w, y + h]
            # "66" / "MINUTES" / "UJUD" sont le logo, pas la bannière.
            if len(ocr_text.strip()) >= 8:
                gt_text = candidate_text
        emission = entries.get(clip["logo"])
        logo_boxes = emission_gt_boxes(emission) if emission else []
        duration = ffprobe_duration(video_path)
        for index, time_s in enumerate(frame_times(duration, A2_SAMPLES)):
            output = frames_dir / f"{clip['id']}_{index:02d}.jpg"
            if not output.is_file() or args.force:
                extract_frame(video_path, time_s, output)
            with Image.open(output) as image:
                width, height = image.size
            frames.append(
                {
                    "id": f"a2_{clip['id']}_{index:02d}",
                    "source": "a2_lower_third",
                    "path": str(output),
                    "width": width,
                    "height": height,
                    "emission": clip["logo"],
                    "channel": None,
                    "position": "bottom_left",
                    "split": "prompt" if index < 2 else "validation",
                    "negative": False,
                    "gt_logo": logo_boxes,
                    "gt_text": [bbox_xyxy_to_list(gt_text)] if gt_text else None,
                    "gt_band": [band],
                    "clip": clip["id"],
                    "frame_index": index,
                    "time_s": round(time_s, 3),
                    "video": str(video_path),
                    "post_action": action.get("action"),
                }
            )

    # 3. Clean feeds et scènes sans logo : contrôle de faux positifs.
    for name, video_path in NEGATIVE_VIDEOS:
        if not video_path.is_file():
            log(f"négatif absent: {video_path}")
            continue
        duration = ffprobe_duration(video_path)
        for index, time_s in enumerate(frame_times(duration, A2_SAMPLES)):
            output = frames_dir / f"neg_{name}_{index:02d}.jpg"
            if not output.is_file() or args.force:
                extract_frame(video_path, time_s, output)
            with Image.open(output) as image:
                width, height = image.size
            frames.append(
                {
                    "id": f"neg_{name}_{index:02d}",
                    "source": "negative",
                    "path": str(output),
                    "width": width,
                    "height": height,
                    "emission": name,
                    "channel": None,
                    "position": "none",
                    "split": "prompt" if index < 2 else "validation",
                    "negative": True,
                    "gt_logo": [],
                    "gt_text": None,
                    "gt_band": None,
                    "clip": name,
                    "frame_index": index,
                    "time_s": round(time_s, 3),
                    "video": str(video_path),
                }
            )

    manifest = {
        "schema": 1,
        "generated_at": now_iso(),
        "model_id": MODEL_ID,
        "catalog_sha256": sha256_file(CATALOG),
        "a2_samples": A2_SAMPLES,
        "prompts": PROMPT_LIST,
        "best_prompt": BEST_PROMPT,
        "frames": frames,
        "counts": {
            "total": len(frames),
            "reference_logo": sum(1 for f in frames if f["source"] == "reference_logo"),
            "a2_lower_third": sum(1 for f in frames if f["source"] == "a2_lower_third"),
            "negative": sum(1 for f in frames if f["source"] == "negative"),
            "prompt": sum(1 for f in frames if f["split"] == "prompt"),
            "validation": sum(1 for f in frames if f["split"] == "validation"),
            "clips": len({f["clip"] for f in frames if f["clip"]}),
        },
    }
    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    manifest_path = Path(args.manifest)
    manifest_path.parent.mkdir(parents=True, exist_ok=True)
    manifest_path.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    log(f"manifeste: {manifest_path} ({len(frames)} frames)")
    log(json.dumps(manifest["counts"], ensure_ascii=False))


# ---------------------------------------------------------------------------
# Inférence locale
# ---------------------------------------------------------------------------


@dataclass
class Detection:
    score: float
    label: str
    bbox: list[float]
    bbox_norm: list[float]

    def to_json(self) -> dict[str, Any]:
        return {
            "score": round(self.score, 4),
            "label": self.label,
            "bbox": [round(value, 2) for value in self.bbox],
            "bbox_norm": [round(value, 6) for value in self.bbox_norm],
        }


class LocalBackend:
    def __init__(self, model_id: str, size: int, device: str = "cuda") -> None:
        import torch
        from transformers import AutoModelForZeroShotObjectDetection, AutoProcessor

        self.torch = torch
        self.device = device
        self.size = size
        self.model_id = model_id
        started = time.perf_counter()
        self.processor = AutoProcessor.from_pretrained(model_id)
        self.processor.image_processor.size = {
            "shortest_edge": size,
            "longest_edge": int(size * 1333 / 800),
        }
        self.model = (
            AutoModelForZeroShotObjectDetection.from_pretrained(model_id)
            .to(device)
            .eval()
        )
        self.load_s = time.perf_counter() - started
        self.calls = 0
        self.inference_s = 0.0
        self.peak_vram_gb = 0.0
        if device == "cuda":
            torch.cuda.reset_peak_memory_stats()

    def detect(self, image: Image.Image, prompt: str) -> list[Detection]:
        import torch

        self.calls += 1
        inputs = self.processor(
            images=image, text=prompt, return_tensors="pt"
        ).to(self.device)
        started = time.perf_counter()
        with torch.inference_mode(), torch.autocast(
            "cuda" if self.device == "cuda" else "cpu",
            dtype=torch.float16 if self.device == "cuda" else torch.bfloat16,
        ):
            outputs = self.model(**inputs)
        self.inference_s += time.perf_counter() - started
        if self.device == "cuda":
            self.peak_vram_gb = max(
                self.peak_vram_gb, torch.cuda.max_memory_allocated() / 1e9
            )
        result = self.processor.post_process_grounded_object_detection(
            outputs,
            input_ids=inputs.input_ids,
            threshold=0.05,
            text_threshold=0.05,
            target_sizes=[image.size[::-1]],
        )[0]
        width, height = image.size
        detections: list[Detection] = []
        # `labels` deviendra un tableau d'ids dans transformers >= 4.58 ;
        # `text_labels` porte les chaînes. On garde le repli pour 4.57.
        labels = result.get("text_labels") or result["labels"]
        for score, label, bbox in zip(
            result["scores"].tolist(),
            labels,
            result["boxes"].tolist(),
        ):
            x1, y1, x2, y2 = clip_bbox(bbox, width, height)
            if x2 - x1 < 2 or y2 - y1 < 2:
                continue
            detections.append(
                Detection(
                    score=float(score),
                    label=str(label).strip(),
                    bbox=[x1, y1, x2, y2],
                    bbox_norm=[x1 / width, y1 / height, x2 / width, y2 / height],
                )
            )
        detections.sort(key=lambda item: item.score, reverse=True)
        return detections

    def stats(self) -> dict[str, Any]:
        return {
            "backend": "local",
            "model_id": self.model_id,
            "size": self.size,
            "device": self.device,
            "load_s": round(self.load_s, 3),
            "calls": self.calls,
            "inference_s": round(self.inference_s, 3),
            "mean_inference_s": round(self.inference_s / self.calls, 4)
            if self.calls
            else 0.0,
            "peak_vram_gb": round(self.peak_vram_gb, 3),
        }


class ReplicateBackend:
    """Client minimal, jamais utilisé sans `REPLICATE_API_TOKEN`.

    Le schéma d'entrée de `adirik/grounding-dino` est `image`, `query`,
    `box_threshold`, `text_threshold`. La version doit être fournie par
    `REPLICATE_GROUNDING_DINO_VERSION` (ou `REPLICATE_MODEL_VERSION`) : elle
    n'est pas devinée et n'est jamais écrite dans le dépôt.
    """

    def __init__(self, model_id: str, size: int) -> None:
        import requests

        self.requests = requests
        self.token = os.environ.get("REPLICATE_API_TOKEN")
        if not self.token:
            raise RuntimeError("REPLICATE_API_TOKEN absent")
        self.version = os.environ.get("REPLICATE_GROUNDING_DINO_VERSION") or os.environ.get(
            "REPLICATE_MODEL_VERSION"
        )
        if not self.version:
            raise RuntimeError(
                "REPLICATE_GROUNDING_DINO_VERSION absent (version hash du modèle)"
            )
        self.model_id = "adirik/grounding-dino"
        self.size = size
        self.calls = 0
        self.inference_s = 0.0
        self.load_s = 0.0

    def detect(self, image: Image.Image, prompt: str) -> list[Detection]:
        buffer = tempfile.SpooledTemporaryFile(max_size=8 * 1024 * 1024)
        image.save(buffer, format="JPEG", quality=92)
        encoded = base64.b64encode(buffer.getvalue()).decode("ascii")
        # Le wrapper Replicate attend des requêtes séparées par des virgules,
        # alors que le modèle transformers utilise des points.
        query = prompt.replace(" . ", ", ")
        payload = {
            "version": self.version,
            "input": {
                "image": f"data:image/jpeg;base64,{encoded}",
                "query": query,
                "box_threshold": 0.05,
                "text_threshold": 0.05,
            },
        }
        started = time.perf_counter()
        response = self.requests.post(
            "https://api.replicate.com/v1/predictions",
            headers={
                "Authorization": f"Bearer {self.token}",
                "Content-Type": "application/json",
            },
            json=payload,
            timeout=180,
        )
        response.raise_for_status()
        prediction = response.json()
        while prediction.get("status") in {"starting", "processing"}:
            time.sleep(1.0)
            poll = self.requests.get(
                prediction["urls"]["get"],
                headers={"Authorization": f"Bearer {self.token}"},
                timeout=60,
            )
            poll.raise_for_status()
            prediction = poll.json()
        if prediction.get("status") != "succeeded":
            raise RuntimeError(f"Replicate: {prediction.get('error')}")
        self.inference_s += time.perf_counter() - started
        self.calls += 1
        output = prediction.get("output") or {}
        width, height = image.size
        detections: list[Detection] = []
        for item in output.get("detections", []):
            # Sortie Replicate: [ymin, xmin, ymax, xmax] normalisé.
            y1, x1, y2, x2 = [float(value) for value in item["bbox"]]
            box = [x1 * width, y1 * height, x2 * width, y2 * height]
            box = clip_bbox(box, width, height)
            detections.append(
                Detection(
                    score=float(item.get("score", 0.0)),
                    label=str(item.get("label", "")).strip(),
                    bbox=box,
                    bbox_norm=[x1, y1, x2, y2],
                )
            )
        detections.sort(key=lambda item: item.score, reverse=True)
        return detections

    def stats(self) -> dict[str, Any]:
        return {
            "backend": "replicate",
            "model_id": self.model_id,
            "size": self.size,
            "calls": self.calls,
            "inference_s": round(self.inference_s, 3),
            "mean_inference_s": round(self.inference_s / self.calls, 4)
            if self.calls
            else 0.0,
        }


# ---------------------------------------------------------------------------
# Métriques
# ---------------------------------------------------------------------------


def in_logo_roi(det: Detection, width: int, height: int, position: str) -> bool:
    x1, y1, x2, y2 = det.bbox
    cx = (x1 + x2) / 2.0 / width
    cy = (y1 + y2) / 2.0 / height
    if position == "top_left":
        return cx <= 0.40 and cy <= 0.30
    return cx <= 0.40 and cy >= 0.55


def in_bottom_band(det: Detection, width: int, height: int) -> bool:
    _, y1, _, y2 = det.bbox
    cy = (y1 + y2) / 2.0 / height
    return cy >= 0.60


def best_logo_match(
    detections: list[Detection], gt_boxes: list[list[float]], width: int, height: int, position: str
) -> tuple[float, Detection | None]:
    best_iou = 0.0
    best: Detection | None = None
    for det in detections:
        if not in_logo_roi(det, width, height, position):
            continue
        for gt in gt_boxes:
            value = iou_xyxy(det.bbox, gt)
            if value > best_iou:
                best_iou = value
                best = det
    return best_iou, best


def best_text_match(
    detections: list[Detection],
    gt_text: list[list[float]] | None,
    gt_band: list[list[float]] | None,
    width: int,
    height: int,
) -> tuple[float, bool, Detection | None]:
    """Retourne (IoU texte, présence dans la bande, meilleure détection)."""
    best_iou = 0.0
    best: Detection | None = None
    band_hit = False
    for det in detections:
        if not in_bottom_band(det, width, height):
            continue
        if gt_text:
            for gt in gt_text:
                value = iou_xyxy(det.bbox, gt)
                if value > best_iou:
                    best_iou = value
                    best = det
        if gt_band:
            for band in gt_band:
                if center_inside(det.bbox, band) and (det.bbox[2] - det.bbox[0]) >= 0.20 * width:
                    band_hit = True
                    if best is None:
                        best = det
    return best_iou, band_hit, best


def frame_metrics(
    frame: dict[str, Any], detections: list[Detection], threshold: float
) -> dict[str, Any]:
    width = frame["width"]
    height = frame["height"]
    kept = [det for det in detections if det.score >= threshold]
    result: dict[str, Any] = {
        "id": frame["id"],
        "source": frame["source"],
        "threshold": threshold,
        "detections": len(kept),
        "raw_detections": len(detections),
    }

    if frame["negative"]:
        logo_fp = [
            det
            for det in kept
            if contains(det.label, ["logo"])
            and (
                in_logo_roi(det, width, height, "bottom_left")
                or in_logo_roi(det, width, height, "top_left")
            )
        ]
        text_fp = [
            det
            for det in kept
            if contains(det.label, ["text", "lower", "banner", "third", "subtitle"])
            and in_bottom_band(det, width, height)
        ]
        result.update(
            {
                "logo_fp": len(logo_fp) > 0,
                "logo_fp_count": len(logo_fp),
                "text_fp": len(text_fp) > 0,
                "text_fp_count": len(text_fp),
                "logo_fp_score": round(max((d.score for d in logo_fp), default=0.0), 4),
            }
        )
        return result

    if frame["gt_logo"]:
        best_iou, best = best_logo_match(
            kept, frame["gt_logo"], width, height, frame.get("position") or "bottom_left"
        )
        center = bool(
            best and any(center_inside(best.bbox, gt) for gt in frame["gt_logo"])
        )
        result.update(
            {
                "logo_iou": round(best_iou, 4),
                "logo_hit_050": best_iou >= 0.50,
                "logo_hit_030": best_iou >= 0.30,
                "logo_center": center,
                "logo_score": round(best.score, 4) if best else 0.0,
                "logo_bbox": [round(value, 2) for value in best.bbox] if best else None,
            }
        )
        spurious = [
            det
            for det in kept
            if contains(det.label, ["logo"])
            and in_logo_roi(det, width, height, frame.get("position") or "bottom_left")
            and all(iou_xyxy(det.bbox, gt) < 0.10 for gt in frame["gt_logo"])
        ]
        result["logo_spurious"] = len(spurious)

    if frame["gt_band"] or frame["gt_text"]:
        text_iou, band_hit, best_text = best_text_match(
            kept, frame["gt_text"], frame["gt_band"], width, height
        )
        result.update(
            {
                "text_gt_available": bool(frame["gt_text"]),
                "text_iou": round(text_iou, 4),
                "text_hit_030": text_iou >= 0.30,
                "band_hit": band_hit,
                "text_score": round(best_text.score, 4) if best_text else 0.0,
                "text_bbox": [round(value, 2) for value in best_text.bbox]
                if best_text
                else None,
            }
        )
    return result


def aggregate_metrics(
    frames: list[dict[str, Any]], results: dict[str, list[Detection]], thresholds: list[float]
) -> dict[str, Any]:
    by_threshold: dict[str, Any] = {}
    for threshold in thresholds:
        rows = [frame_metrics(frame, results[frame["id"]], threshold) for frame in frames]
        logo_rows = [row for row in rows if row.get("logo_iou") is not None]
        text_rows = [row for row in rows if row.get("band_hit") is not None]
        text_iou_rows = [row for row in text_rows if row.get("text_gt_available")]
        negative_rows = [row for row in rows if row["source"] == "negative"]
        logo_ious = [row["logo_iou"] for row in logo_rows]
        text_ious = [row["text_iou"] for row in text_iou_rows]
        by_threshold[f"{threshold:.2f}"] = {
            "threshold": threshold,
            "frames": len(rows),
            "logo_frames": len(logo_rows),
            "logo_recall_iou050": round(
                sum(1 for row in logo_rows if row.get("logo_hit_050")) / len(logo_rows), 4
            )
            if logo_rows
            else 0.0,
            "logo_recall_iou030": round(
                sum(1 for row in logo_rows if row.get("logo_hit_030")) / len(logo_rows), 4
            )
            if logo_rows
            else 0.0,
            "logo_center_rate": round(
                sum(1 for row in logo_rows if row.get("logo_center")) / len(logo_rows), 4
            )
            if logo_rows
            else 0.0,
            "logo_iou_median": round(statistics.median(logo_ious), 4) if logo_ious else 0.0,
            "logo_iou_p10": round(percentile(logo_ious, 10), 4) if logo_ious else 0.0,
            "logo_spurious_per_frame": round(
                sum(row.get("logo_spurious", 0) for row in logo_rows) / len(logo_rows), 3
            )
            if logo_rows
            else 0.0,
            "text_frames": len(text_rows),
            "text_iou_frames": len(text_iou_rows),
            "text_recall_iou030": round(
                sum(1 for row in text_iou_rows if row.get("text_hit_030"))
                / len(text_iou_rows),
                4,
            )
            if text_iou_rows
            else 0.0,
            "band_recall": round(
                sum(1 for row in text_rows if row.get("band_hit")) / len(text_rows), 4
            )
            if text_rows
            else 0.0,
            "text_iou_median": round(statistics.median(text_ious), 4) if text_ious else 0.0,
            "negative_frames": len(negative_rows),
            "negative_logo_fp_rate": round(
                sum(1 for row in negative_rows if row.get("logo_fp")) / len(negative_rows), 4
            )
            if negative_rows
            else 0.0,
            "negative_text_fp_rate": round(
                sum(1 for row in negative_rows if row.get("text_fp")) / len(negative_rows), 4
            )
            if negative_rows
            else 0.0,
            "negative_logo_fp_per_frame": round(
                sum(row.get("logo_fp_count", 0) for row in negative_rows) / len(negative_rows), 3
            )
            if negative_rows
            else 0.0,
        }
        by_threshold[f"{threshold:.2f}"]["rows"] = rows
    return by_threshold


def percentile(values: list[float], percent: float) -> float:
    if not values:
        return 0.0
    ordered = sorted(values)
    if len(ordered) == 1:
        return ordered[0]
    rank = (len(ordered) - 1) * percent / 100.0
    lower = int(rank)
    upper = min(lower + 1, len(ordered) - 1)
    fraction = rank - lower
    return ordered[lower] * (1 - fraction) + ordered[upper] * fraction


def cluster_detections(
    items: list[tuple[int, Detection | None]],
    iou_threshold: float = 0.50,
    center_px: float = 10.0,
) -> list[dict[str, Any]]:
    clusters: list[dict[str, Any]] = []
    for frame_index, det in items:
        if det is None:
            continue
        cx = (det.bbox[0] + det.bbox[2]) / 2.0
        cy = (det.bbox[1] + det.bbox[3]) / 2.0
        assigned = None
        for cluster in clusters:
            ref = cluster["detections"][0][1]
            rcx = (ref.bbox[0] + ref.bbox[2]) / 2.0
            rcy = (ref.bbox[1] + ref.bbox[3]) / 2.0
            distance = ((cx - rcx) ** 2 + (cy - rcy) ** 2) ** 0.5
            if iou_xyxy(det.bbox, ref.bbox) >= iou_threshold and distance <= center_px:
                assigned = cluster
                break
        if assigned is None:
            assigned = {"detections": []}
            clusters.append(assigned)
        assigned["detections"].append((frame_index, det))
    summary: list[dict[str, Any]] = []
    for cluster in clusters:
        detections = [det for _, det in cluster["detections"]]
        boxes = [det.bbox for det in detections]
        centers_x = [(box[0] + box[2]) / 2.0 for box in boxes]
        centers_y = [(box[1] + box[3]) / 2.0 for box in boxes]
        summary.append(
            {
                "frames": len(detections),
                "score_median": round(statistics.median([d.score for d in detections]), 4),
                "bbox_median": [
                    round(statistics.median([box[index] for box in boxes]), 2)
                    for index in range(4)
                ],
                "center_std_px": round(
                    (
                        statistics.pstdev(centers_x) ** 2
                        + statistics.pstdev(centers_y) ** 2
                    )
                    ** 0.5,
                    2,
                )
                if len(boxes) > 1
                else 0.0,
                "frame_indices": [index for index, _ in cluster["detections"]],
            }
        )
    summary.sort(key=lambda item: item["frames"], reverse=True)
    return summary


def persistence_metrics(
    frames: list[dict[str, Any]], results: dict[str, list[Detection]], threshold: float
) -> dict[str, Any]:
    clips: dict[str, list[dict[str, Any]]] = {}
    for frame in frames:
        if frame["source"] != "a2_lower_third":
            continue
        clips.setdefault(frame["clip"], []).append(frame)
    output: dict[str, Any] = {}
    for clip, clip_frames in sorted(clips.items()):
        # La phase comparative ne contient que deux frames par clip : elle ne
        # permet pas de conclure sur la persistance temporelle.
        if len(clip_frames) < 6:
            continue
        clip_frames.sort(key=lambda item: item["frame_index"])
        logo_items: list[tuple[int, Detection | None]] = []
        text_items: list[tuple[int, Detection | None]] = []
        for frame in clip_frames:
            detections = [det for det in results[frame["id"]] if det.score >= threshold]
            width = frame["width"]
            height = frame["height"]
            _, logo_best = best_logo_match(
                detections,
                frame["gt_logo"],
                width,
                height,
                frame.get("position") or "bottom_left",
            )
            _, band_hit, text_best = best_text_match(
                detections, frame["gt_text"], frame["gt_band"], width, height
            )
            logo_items.append((frame["frame_index"], logo_best))
            text_items.append(
                (frame["frame_index"], text_best if band_hit else None)
            )
        logo_clusters = cluster_detections(logo_items)
        text_clusters = cluster_detections(text_items)
        total = len(clip_frames)
        output[clip] = {
            "frames": total,
            "threshold": threshold,
            "logo_clusters": logo_clusters,
            "text_clusters": text_clusters,
            "logo_persistence": round(
                logo_clusters[0]["frames"] / total if logo_clusters else 0.0, 4
            ),
            "text_persistence": round(
                text_clusters[0]["frames"] / total if text_clusters else 0.0, 4
            ),
            "logo_persistent": bool(logo_clusters and logo_clusters[0]["frames"] / total >= 0.70),
            "text_persistent": bool(text_clusters and text_clusters[0]["frames"] / total >= 0.70),
            "logo_frames_6": round(
                sum(
                    1
                    for index, det in logo_items
                    if index % 2 == 0 and det is not None
                )
                / max(1, len([index for index, _ in logo_items if index % 2 == 0])),
                4,
            ),
            "text_frames_6": round(
                sum(
                    1
                    for index, det in text_items
                    if index % 2 == 0 and det is not None
                )
                / max(1, len([index for index, _ in text_items if index % 2 == 0])),
                4,
            ),
        }
    return output


# ---------------------------------------------------------------------------
# Exécution
# ---------------------------------------------------------------------------


def run_plan(args: argparse.Namespace) -> None:
    manifest_path = Path(args.manifest)
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    frames = manifest["frames"]
    if getattr(args, "only_source", None):
        frames = [frame for frame in frames if frame["source"] == args.only_source]
    if getattr(args, "only_split", None):
        frames = [frame for frame in frames if frame["split"] == args.only_split]
    requested = [frame for frame in frames if Path(frame["path"]).is_file()]
    missing = len(frames) - len(requested)
    if missing:
        log(f"{missing} frames absentes, ignorées")
    log(f"{len(requested)} frames à évaluer")

    if args.backend == "replicate":
        backend: Any = ReplicateBackend(manifest["model_id"], args.size)
    else:
        backend = LocalBackend(manifest["model_id"], args.size, args.device)

    prompt_list = manifest.get("prompts", PROMPT_LIST)
    if getattr(args, "prompts", None):
        prompt_list = args.prompts
    best_prompt = args.prompt or manifest.get("best_prompt", BEST_PROMPT)
    phases = set(args.phases.split(","))
    results: dict[str, dict[str, list[Detection]]] = {}
    stats: dict[str, Any] = {}

    def run_prompt(prompt: str, subset: list[dict[str, Any]], key: str) -> None:
        log(f"run {key}: {len(subset)} frames, prompt={prompt!r}")
        started = time.perf_counter()
        store: dict[str, list[Detection]] = {}
        for index, frame in enumerate(subset, start=1):
            with Image.open(frame["path"]) as image:
                image = image.convert("RGB")
                store[frame["id"]] = backend.detect(image, prompt)
            if index % 20 == 0 or index == len(subset):
                log(f"  {index}/{len(subset)}")
        results[key] = store
        stats[key] = {
            "prompt": prompt,
            "frames": len(subset),
            "elapsed_s": round(time.perf_counter() - started, 3),
        }

    prompt_frames = [frame for frame in requested if frame["split"] == "prompt"]
    validation_frames = requested
    if "prompts" in phases:
        for prompt in prompt_list:
            run_prompt(prompt, prompt_frames, f"prompt::{prompt}")
    if "validation" in phases:
        run_prompt(best_prompt, validation_frames, f"validation::{best_prompt}")

    backend_stats = backend.stats()
    payload = {
        "schema": 1,
        "generated_at": now_iso(),
        "manifest": str(manifest_path),
        "model_id": manifest["model_id"],
        "prompt_list": prompt_list,
        "best_prompt": best_prompt,
        "backend": backend_stats,
        "runs": {
            key: {frame_id: [det.to_json() for det in dets] for frame_id, dets in store.items()}
            for key, store in results.items()
        },
        "frame_stats": stats,
    }
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    log(f"résultats: {output_path}")
    log(json.dumps(backend_stats, ensure_ascii=False))
    write_report(manifest, payload, output_path.with_suffix(".md"))


def serve(args: argparse.Namespace) -> None:
    """Prototype de sidecar local : une requête JSON par ligne sur stdin.

    Contrat minimal pour le futur adaptateur Rust :

        {"id": "...", "image": "/chemin/frame.jpg", "prompt": "logo . text",
         "box_threshold": 0.25, "roi_y_min": 0.60}

    Réponse sur stdout, une ligne JSON par requête. Les journaux restent sur
    stderr pour ne pas polluer le protocole.
    """
    model_id = args.model or MODEL_ID
    backend = LocalBackend(model_id, args.size, args.device)
    log_err(f"sidecar prêt: {model_id}, size={args.size}, device={args.device}")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
            image_path = Path(request["image"])
            prompt = request.get("prompt", BEST_PROMPT)
            box_threshold = float(request.get("box_threshold", 0.25))
            roi_y_min = request.get("roi_y_min")
            started = time.perf_counter()
            with Image.open(image_path) as image:
                image = image.convert("RGB")
                detections = backend.detect(image, prompt)
            width, height = image.size
            filtered = []
            for det in detections:
                if det.score < box_threshold:
                    continue
                if roi_y_min is not None:
                    center_y = (det.bbox[1] + det.bbox[3]) / 2.0 / height
                    if center_y < float(roi_y_min):
                        continue
                filtered.append(det.to_json())
            response = {
                "id": request.get("id"),
                "detections": filtered,
                "timing_ms": round((time.perf_counter() - started) * 1000.0, 1),
            }
            print(json.dumps(response, ensure_ascii=False), flush=True)
        except Exception as error:  # noqa: BLE001 - le sidecar doit rester vivant
            print(
                json.dumps(
                    {"id": None, "error": str(error), "detections": []},
                    ensure_ascii=False,
                ),
                flush=True,
            )


def load_run_detections(payload: dict[str, Any], key: str) -> dict[str, list[Detection]]:
    store = payload["runs"][key]
    return {
        frame_id: [
            Detection(
                score=item["score"],
                label=item["label"],
                bbox=item["bbox"],
                bbox_norm=item["bbox_norm"],
            )
            for item in dets
        ]
        for frame_id, dets in store.items()
    }


def write_report(manifest: dict[str, Any], payload: dict[str, Any], output: Path) -> None:
    frames = {frame["id"]: frame for frame in manifest["frames"]}
    thresholds = [0.10, 0.15, 0.20, 0.25, 0.30, 0.35]
    lines: list[str] = []
    lines.append("# Spike Grounding DINO — résultats bruts")
    lines.append("")
    lines.append(f"- Généré : {payload['generated_at']}")
    lines.append(f"- Modèle : `{payload['model_id']}`")
    lines.append(f"- Backend : `{payload['backend']}`")
    lines.append("")
    prompt_metrics: dict[str, Any] = {}
    for key, store in payload["runs"].items():
        if not key.startswith("prompt::") and not key.startswith("validation::"):
            continue
        subset = [frame for frame in manifest["frames"] if frame["id"] in store]
        aggregated = aggregate_metrics(subset, load_run_detections(payload, key), thresholds)
        prompt_metrics[key] = aggregated
        prompt = payload["frame_stats"][key]["prompt"]
        lines.append(f"## {key}")
        lines.append("")
        lines.append(f"Prompt : `{prompt}` — {len(subset)} frames")
        lines.append("")
        lines.append(
            "| seuil | rappel logo IoU≥0,5 | rappel logo IoU≥0,3 | centre dans GT | IoU médian | FP logo clean | rappel bande | IoU texte médian |"
        )
        lines.append("| --- | --- | --- | --- | --- | --- | --- | --- |")
        for value in thresholds:
            row = aggregated[f"{value:.2f}"]
            lines.append(
                "| {threshold:.2f} | {logo_recall_iou050:.1%} | {logo_recall_iou030:.1%} | "
                "{logo_center_rate:.1%} | {logo_iou_median:.3f} | {negative_logo_fp_rate:.1%} | "
                "{band_recall:.1%} | {text_iou_median:.3f} |".format(**row)
            )
        lines.append("")
        # Détail par source au seuil 0,25 : les logos du catalogue et les
        # bandeaux A2 n'ont pas la même taille ni le même contexte.
        lines.append("Détail par source au seuil 0,25 :")
        lines.append("")
        lines.append(
            "| source | frames | rappel logo IoU≥0,5 | centre | IoU médian | rappel bande | rappel texte IoU≥0,3 | FP logo clean |"
        )
        lines.append("| --- | --- | --- | --- | --- | --- | --- | --- |")
        run_dets = load_run_detections(payload, key)
        for source in ("reference_logo", "a2_lower_third", "negative"):
            source_frames = [frame for frame in subset if frame["source"] == source]
            if not source_frames:
                continue
            row = aggregate_metrics(source_frames, run_dets, [0.25])["0.25"]
            lines.append(
                "| {source} | {frames} | {logo_recall_iou050:.1%} | {logo_center_rate:.1%} | "
                "{logo_iou_median:.3f} | {band_recall:.1%} | {text_recall_iou030:.1%} | "
                "{negative_logo_fp_rate:.1%} |".format(source=source, **row)
            )
        lines.append("")
        persistence = persistence_metrics(
            subset,
            load_run_detections(payload, key),
            0.20,
        )
        if persistence:
            lines.append("### Persistance temporelle (seuil 0,20)")
            lines.append("")
            lines.append(
                "| clip | frames | persistance logo | persistance texte | logo 6f | texte 6f |"
            )
            lines.append("| --- | --- | --- | --- | --- | --- |")
            for clip, info in persistence.items():
                lines.append(
                    f"| {clip} | {info['frames']} | {info['logo_persistence']:.1%} | "
                    f"{info['text_persistence']:.1%} | {info['logo_frames_6']:.1%} | "
                    f"{info['text_frames_6']:.1%} |"
                )
            lines.append("")
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines) + "\n", encoding="utf-8")
    log(f"rapport: {output}")


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    build = sub.add_parser("build-dataset", help="construire le manifeste")
    build.add_argument(
        "--manifest",
        default=str(RESULTS_DIR / "manifest.json"),
        help="chemin du manifeste",
    )
    build.add_argument("--frames-dir", default=str(DEFAULT_FRAMES))
    build.add_argument("--force", action="store_true", help="réextraire les frames")
    build.set_defaults(func=build_dataset)

    run = sub.add_parser("run", help="exécuter l'inférence et les métriques")
    run.add_argument("--manifest", default=str(RESULTS_DIR / "manifest.json"))
    run.add_argument("--output", default=str(RESULTS_DIR / "results.json"))
    run.add_argument("--backend", choices=["local", "replicate"], default="local")
    run.add_argument("--device", default="cuda")
    run.add_argument("--size", type=int, default=480)
    run.add_argument(
        "--phases",
        default="prompts,validation",
        help="phases séparées par virgule: prompts,validation",
    )
    run.add_argument("--prompt", default=None)
    run.add_argument(
        "--prompts",
        nargs="*",
        default=None,
        help="liste de prompts pour la phase comparative",
    )
    run.add_argument("--only-source", default=None)
    run.add_argument("--only-split", default=None)
    run.set_defaults(func=run_plan)

    report = sub.add_parser("report", help="recalculer le rapport depuis results.json")
    report.add_argument("--manifest", default=str(RESULTS_DIR / "manifest.json"))
    report.add_argument("--results", default=str(RESULTS_DIR / "results.json"))

    def report_existing(args: argparse.Namespace) -> None:
        manifest = json.loads(Path(args.manifest).read_text(encoding="utf-8"))
        payload = json.loads(Path(args.results).read_text(encoding="utf-8"))
        write_report(manifest, payload, Path(args.results).with_suffix(".md"))

    report.set_defaults(func=report_existing)

    serve_parser = sub.add_parser(
        "serve", help="prototype de sidecar local (JSON Lines sur stdin/stdout)"
    )
    serve_parser.add_argument("--model", default=MODEL_ID)
    serve_parser.add_argument("--device", default="cuda")
    serve_parser.add_argument("--size", type=int, default=480)
    serve_parser.set_defaults(func=serve)

    args = parser.parse_args()
    return args.func(args) or 0


if __name__ == "__main__":
    sys.exit(main())
