#!/usr/bin/env python3
"""Pilote B2 : annotation A2 par le détecteur YOLOX-Nano, sans décision.

Le script prépare deux copies strictement identiques de projets de production
finalisés (le `.ecproj` `_assist` n'est utilisé que lorsque son SHA-256 est
identique au brut, preuve qu'aucune transformation A1/A2/A6 n'a été publiée),
puis exécute `workflow-cli post run --report-only` avec et sans détecteur.

Il vérifie ensuite que toutes les actions, raisons, confiances et détails
(hors `logo_detector`) sont identiques, et résume les observations. Aucune
écriture n'est faite sous `/mnt/cache` : les vidéos sont liées par symlink.

Exemple :
  python3 scripts/pilot-b2-logo-detector.py --limit 20
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path("/home/kidpixel/workflow-rs")
CACHE = Path("/mnt/cache")
B1 = Path(
    "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Recadrage/2026/"
    "references_logos/training_yolo_b1"
)
DEFAULT_WORK = B1 / "pilot_b2"
CLI = ROOT / "target/release/workflow-cli"
CONFIG = ROOT / "config/config.toml"
MODEL = ROOT / "assets/models/qa/logo_yolox_nano_a2.onnx"
CUDA_ORT = Path(
    "/mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/"
    "onnxruntime/capi/libonnxruntime.so.1.23.2"
)
NVIDIA_ROOT = Path(
    "/mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/nvidia"
)

DETECTOR_BLOCK = """\
a2_logo_detector = true
a2_logo_detector_model = "qa/logo_yolox_nano_a2.onnx"
a2_logo_detector_onnxruntime = "{ort}"
a2_logo_detector_device = "cuda"
a2_logo_detector_size = 416
a2_logo_detector_conf = 0.30
a2_logo_detector_iou = 0.65
a2_logo_detector_min_persistence = 0.70
a2_logo_detector_roi = "full"
"""


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def find_docs_files(docs_dir, stem):
    """Fichiers annexes utiles au post-traitement, par nom exact."""
    names = [
        f"{stem}_ae.json",
        f"{stem}.csv",
        f"{stem}_audio.json",
        f"{stem}_tracking.json",
        f"{stem}.json",
    ]
    return [docs_dir / name for name in names if (docs_dir / name).is_file()]


def candidates():
    found = []
    for audit_path in sorted(CACHE.glob("*/docs/post/*/audit.json")):
        try:
            audit = json.loads(audit_path.read_text())
        except (OSError, json.JSONDecodeError):
            continue
        project = Path(audit.get("project", ""))
        assist_name = Path(audit.get("assist_project", "")).name
        raw_sha = audit.get("project_sha256")
        assist_sha = audit.get("assist_sha256")
        counts = audit.get("counts") or {}
        docs_dir = audit_path.parents[2]
        if not docs_dir.is_dir():
            continue
        # STEP7 supprime le brut et le dossier finalisé a pu être renommé :
        # on retrouve l'assist et la vidéo par leur nom dans le dossier réel.
        assist = docs_dir.parent / "projets" / assist_name
        if not assist.is_file():
            continue
        # Le brut est supprimé par STEP7 : l'assist n'est une copie fidèle que
        # lorsque les deux empreintes sont identiques (aucune transformation).
        if not raw_sha or raw_sha != assist_sha:
            continue
        if int(counts.get("apply") or 0) > 0:
            continue
        video = Path(audit.get("video", ""))
        if not video.is_file():
            candidate = docs_dir / video.name
            if candidate.is_file():
                video = candidate
        if not video.is_file():
            continue
        stem = project.stem
        if stem.endswith("_assist"):
            stem = stem[: -len("_assist")]
        found.append(
            {
                "audit": str(audit_path),
                "stem": stem,
                "project": project,
                "assist": assist,
                "video": video,
                "docs_dir": docs_dir,
                "source": docs_dir.parent.name,
                "raw_sha256": raw_sha,
            }
        )
    return found


def prepare_tree(work, name, entries):
    tree = work / name
    projects = tree / "projets"
    docs = tree / "docs"
    projects.mkdir(parents=True, exist_ok=True)
    docs.mkdir(parents=True, exist_ok=True)
    for entry in entries:
        target = projects / f"{entry['stem']}.ecproj"
        shutil.copyfile(entry["assist"], target)
        video_name = entry["video"].name
        link = docs / video_name
        if not link.exists():
            link.symlink_to(entry["video"])
        for extra in find_docs_files(entry["docs_dir"], entry["video"].stem):
            link = docs / extra.name
            if not link.exists():
                link.symlink_to(extra)
    return tree


def make_config(path, detector):
    text = CONFIG.read_text()
    header = "[steps.step8.post]"
    if header not in text:
        raise SystemExit(f"section {header} absente de {CONFIG}")
    if "a2_logo_detector" in text:
        raise SystemExit("config/config.toml contient déjà a2_logo_detector")
    # Les deux runs doivent utiliser le même runtime OCR : le détecteur
    # preload le runtime CUDA, donc la baseline aussi, sinon le diff pourrait
    # venir d'une différence de bibliothèque ONNX Runtime et non du détecteur.
    text, replaced = re.subn(
        r"^a2_ocr_onnxruntime\s*=.*$",
        f'a2_ocr_onnxruntime = "{CUDA_ORT}"',
        text,
        count=1,
        flags=re.MULTILINE,
    )
    if replaced != 1:
        raise SystemExit("a2_ocr_onnxruntime introuvable dans config/config.toml")
    if detector:
        block = DETECTOR_BLOCK.format(ort=CUDA_ORT)
        text = text.replace(header, header + "\n" + block, 1)
    path.write_text(text)


def environment():
    env = os.environ.copy()
    libs = [str(CUDA_ORT.parent)]
    for child in sorted(NVIDIA_ROOT.glob("*/lib")):
        libs.append(str(child))
    current = env.get("LD_LIBRARY_PATH")
    env["LD_LIBRARY_PATH"] = ":".join(libs + ([current] if current else []))
    return env


def run_post(tree, config, log_path):
    command = [
        str(CLI),
        "--config",
        str(config),
        "post",
        "run",
        "--folder",
        str(tree),
        "--report-only",
    ]
    result = subprocess.run(
        command,
        cwd=ROOT,
        env=environment(),
        capture_output=True,
        text=True,
        timeout=1800,
    )
    log_path.write_text(
        json.dumps(
            {
                "command": command,
                "returncode": result.returncode,
                "stdout": result.stdout,
                "stderr": result.stderr,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"post run a échoué ({result.returncode}) ; voir {log_path}"
        )
    return json.loads(result.stdout)


def artifacts(tree, stem):
    return tree / "docs" / "post" / f"{stem}_assist"


def load_json(path):
    return json.loads(path.read_text())


def strip_detector(value):
    if isinstance(value, dict):
        return {
            key: strip_detector(item)
            for key, item in value.items()
            if key != "logo_detector"
        }
    if isinstance(value, list):
        return [strip_detector(item) for item in value]
    return value


def action_key(action):
    return {
        "code": action.get("code"),
        "action": action.get("action"),
        "target": action.get("target"),
        "reason": action.get("reason"),
        "confidence": action.get("confidence"),
        "details": strip_detector(action.get("details")),
    }


def compare(baseline, detector):
    differences = []
    base_actions = [action_key(item) for item in baseline.get("actions", [])]
    det_actions = [action_key(item) for item in detector.get("actions", [])]
    if base_actions != det_actions:
        differences.append("actions")
    if baseline.get("summary") != detector.get("summary"):
        differences.append("summary")
    return differences


def review_items(tree, stem):
    path = artifacts(tree, stem) / "review.json"
    if not path.is_file():
        return None
    return strip_detector(load_json(path).get("items"))


def detector_summary(post):
    for action in post.get("actions", []):
        if action.get("code") != "A2":
            continue
        details = action.get("details") or {}
        block = details.get("logo_detector")
        if block is None:
            return None
        logo = block.get("logo") or {}
        return {
            "detected": logo.get("detected"),
            "persistence": logo.get("persistence"),
            "frames": logo.get("frames"),
            "bbox": logo.get("bbox"),
            "score": logo.get("score"),
            "unknown_logo": logo.get("unknown_logo"),
            "source_prior": logo.get("source_prior"),
            "source_logo_match": logo.get("source_logo_match"),
            "merged_with_lower_third": logo.get("merged_with_lower_third"),
            "clusters": block.get("clusters"),
            "detections_total": block.get("detections_total"),
            "detections_a2": block.get("detections_a2"),
            "frames_analyzed": block.get("frames_analyzed"),
            "latency_ms": block.get("latency_ms"),
            "top": block.get("top"),
        }
    return None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, default=20)
    parser.add_argument("--work-dir", type=Path, default=DEFAULT_WORK)
    args = parser.parse_args()

    if not CLI.is_file():
        raise SystemExit(f"binaire absent: {CLI} (cargo build --release -p workflow-cli)")
    if not MODEL.is_file():
        raise SystemExit(f"modèle absent: {MODEL}")
    if not CUDA_ORT.is_file():
        raise SystemExit(f"ONNX Runtime CUDA absent: {CUDA_ORT}")

    work = args.work_dir
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    (work / "configs").mkdir()
    (work / "logs").mkdir()

    entries = candidates()[: args.limit]
    if not entries:
        raise SystemExit("aucun projet candidat (brut == assist, apply=0)")
    print(f"{len(entries)} projet(s) candidat(s) sous {CACHE}")

    baseline_tree = prepare_tree(work, "baseline", entries)
    detector_tree = prepare_tree(work, "detector", entries)
    baseline_config = work / "configs/baseline.toml"
    detector_config = work / "configs/detector.toml"
    make_config(baseline_config, detector=False)
    make_config(detector_config, detector=True)

    run_post(
        baseline_tree,
        baseline_config,
        work / "logs/baseline.json",
    )
    run_post(
        detector_tree,
        detector_config,
        work / "logs/detector.json",
    )

    projects = []
    failures = []
    for entry in entries:
        stem = entry["stem"]
        base_path = artifacts(baseline_tree, stem) / "post.json"
        det_path = artifacts(detector_tree, stem) / "post.json"
        if not base_path.is_file() or not det_path.is_file():
            failures.append({"stem": stem, "reason": "post.json absent"})
            continue
        baseline = load_json(base_path)
        detector = load_json(det_path)
        diffs = compare(baseline, detector)
        if review_items(baseline_tree, stem) != review_items(detector_tree, stem):
            diffs.append("review")
        if diffs:
            failures.append({"stem": stem, "reason": "diff: " + ", ".join(diffs)})
        summary = detector_summary(detector)
        projects.append(
            {
                "stem": stem,
                "source": entry["source"],
                "video": entry["video"].name,
                "raw_sha256": entry["raw_sha256"],
                "action_diffs": diffs,
                "summary": summary,
                "timings_ms": detector.get("timings_ms"),
                "audit_verdict": load_json(
                    artifacts(detector_tree, stem) / "audit.json"
                ).get("verdict"),
            }
        )

    annotated = [item for item in projects if item["summary"] is not None]
    if not annotated:
        print(
            "aucune annotation logo_detector : vérifier le runtime CUDA et les logs",
            file=sys.stderr,
        )
        sys.exit(1)
    detected = [
        item
        for item in annotated
        if item["summary"].get("detected")
    ]
    unknown = [
        item
        for item in annotated
        if item["summary"].get("unknown_logo")
    ]
    diff_projects = [item for item in projects if item["action_diffs"]]

    report = {
        "schema": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "work_dir": str(work),
        "model": str(MODEL),
        "model_sha256": sha256(MODEL),
        "config": {
            "conf": 0.30,
            "iou": 0.65,
            "min_persistence": 0.70,
            "size": 416,
            "roi": "full",
            "device": "cuda",
            "report_only": True,
        },
        "projects_requested": len(entries),
        "projects_compared": len(projects),
        "annotated": len(annotated),
        "detected": len(detected),
        "unknown": len(unknown),
        "action_diff_projects": len(diff_projects),
        "failures": failures,
        "projects": projects,
    }
    out = work / "pilot-b2.json"
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2))
    print(
        f"{len(projects)} comparés, {len(annotated)} annotés, "
        f"{len(detected)} avec logo, {len(unknown)} inconnus, "
        f"{len(diff_projects)} diff d'action, {len(failures)} échec(s)"
    )
    print(f"résultat: {out}")
    if diff_projects or failures:
        sys.exit(1)


if __name__ == "__main__":
    main()
