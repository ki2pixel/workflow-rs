#!/usr/bin/env python3
"""Copie les modèles de production dans workflow-rs et écrit leur manifeste.

Les binaires restent locaux (ignorés par git) ; seul `manifest.toml` est
versionné. Le workspace ne dépend donc plus des chemins de l'ancienne
application à l'exécution, tout en gardant une trace vérifiable des sources.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
from pathlib import Path

OLD_ROOT = Path("/home/kidpixel/workflow_mediapipe")
NEW_ROOT = Path(__file__).resolve().parents[1]
DEST_ROOT = NEW_ROOT / "assets" / "models"

# (role, source relative à OLD_ROOT, destination relative à DEST_ROOT, usage)
MODELS = [
    ("step2.transnet", "assets/models/onnx/transnetv2.onnx",
     "step2/transnetv2.onnx", "TransNetV2 ONNX Runtime CUDA"),
    ("step3.yamnet", "assets/yamnet_split_edgetpu_compiled.tflite",
     "step3/yamnet_split_edgetpu_compiled.tflite", "YAMNet split Edge TPU"),
    ("step3.yamnet_fallback", "assets/yamnet_split_quantized.tflite",
     "step3/yamnet_split_quantized.tflite", "YAMNet split quantifié, repli"),
    ("step3.ecapa", "assets/ecapa_tdnn_speaker_float.onnx",
     "step3/ecapa_tdnn_speaker_float.onnx", "ECAPA-TDNN ONNX Runtime CPU"),
    ("step3.ecapa_data", "assets/ecapa_tdnn_speaker_float.onnx.data",
     "step3/ecapa_tdnn_speaker_float.onnx.data", "Données externes ECAPA"),
    ("step4.blazeface", "assets/blazeface_front_quantized_edgetpu.tflite",
     "step4/blazeface_front_quantized_edgetpu.tflite", "BlazeFace Edge TPU"),
    ("step4.facemesh", "assets/facemesh_quantized_edgetpu.tflite",
     "step4/facemesh_quantized_edgetpu.tflite", "FaceMesh Edge TPU"),
    ("step4.blendshapes", "assets/face_blendshapes.tflite",
     "step4/face_blendshapes.tflite", "Blendshapes TFLite"),
    ("step4.cocompiled.blazeface", "assets/cocompiled/blazeface_front_quantized_edgetpu.tflite",
     "step4/cocompiled/blazeface_front_quantized_edgetpu.tflite", "BlazeFace co-compilé"),
    ("step4.cocompiled.facemesh", "assets/cocompiled/facemesh_quantized_edgetpu.tflite",
     "step4/cocompiled/facemesh_quantized_edgetpu.tflite", "FaceMesh co-compilé"),
    ("step4.object_detector_fallback", "assets/detect_edgetpu.tflite",
     "step4/detect_edgetpu.tflite", "Détecteur d'objets Edge TPU, repli"),
    ("step4.object_detector_efficientdet_lite2",
     "workflow_scripts/step4/models/object_detectors/tflite/efficientdet_lite2_int8_mediapipe.tflite",
     "step4/efficientdet_lite2_int8_mediapipe.tflite", "EfficientDet-Lite2 INT8"),
]


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def quote(value: str) -> str:
    return json.dumps(value, ensure_ascii=False)


def main() -> int:
    parser = argparse.ArgumentParser(description="Copie les modèles de production")
    parser.add_argument("--force", action="store_true", help="Recopier même si la taille correspond")
    args = parser.parse_args()

    DEST_ROOT.mkdir(parents=True, exist_ok=True)
    entries = []
    failures = []
    total = 0

    for role, source_rel, dest_rel, purpose in MODELS:
        source = OLD_ROOT / source_rel
        destination = DEST_ROOT / dest_rel
        if not source.exists():
            failures.append(f"source absente: {source}")
            continue
        destination.parent.mkdir(parents=True, exist_ok=True)
        if args.force or not destination.exists() or destination.stat().st_size != source.stat().st_size:
            shutil.copy2(source, destination)
        digest = sha256(destination)
        size = destination.stat().st_size
        total += size
        entries.append(
            "\n[[model]]\n"
            f"role = {quote(role)}\n"
            f"path = {quote(dest_rel)}\n"
            f"source = {quote(source_rel)}\n"
            f"size = {size}\n"
            f"sha256 = {quote(digest)}\n"
            f"purpose = {quote(purpose)}\n"
        )
        print(f"ok  {role:40} {size / 1024 / 1024:8.1f} MiB")

    manifest = (
        "# Manifeste des modèles copiés dans workflow-rs.\n"
        "# Les binaires sont ignorés par git ; ce fichier et les SHA-256 sont versionnés.\n"
        "\nversion = 1\n"
        f"source_root = {quote(str(OLD_ROOT))}\n"
        + "".join(entries)
    )
    (DEST_ROOT / "manifest.toml").write_text(manifest, encoding="utf-8")

    if failures:
        print("Échecs :")
        for failure in failures:
            print(f"  - {failure}")
        return 1
    print(f"\n{len(entries)} modèles, {total / 1024 / 1024:.1f} MiB, manifeste: {DEST_ROOT / 'manifest.toml'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
