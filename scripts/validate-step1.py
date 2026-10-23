#!/usr/bin/env python3
"""Valide STEP1 Rust contre le script Python sur une archive synthétique.

Le script Python est copié dans un arbre temporaire avec une configuration
minimale : aucune écriture dans `workflow_mediapipe`, aucun accès réseau. Les
deux implémentations traitent la même archive avec CPU et un seul worker, puis
les fichiers de `docs/` sont comparés au bit près.
"""
from __future__ import annotations

import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
PYTHON_FILES = (
    "workflow_scripts/step1/extract_archives.py",
    "utils/filename_security.py",
    "utils/media_normalizer.py",
    "utils/media_filters.py",
)
VIDEO_FILES = ("ready.mp4", "legacy.avi", "nonalpha.mov", "m6+.mov")
PROJECT_NAME = "20 Camille"


def run(command: list[str], *, cwd: Path | None = None, env: dict[str, str] | None = None) -> None:
    process = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )
    if process.returncode != 0:
        raise RuntimeError(
            f"{' '.join(command)} a échoué ({process.returncode})\n"
            f"{process.stdout}\n{process.stderr}"
        )


def generate_media(source: Path) -> None:
    common = ["-y", "-hide_banner", "-loglevel", "error", "-f", "lavfi"]
    run(
        [
            "ffmpeg",
            *common,
            "-i",
            "testsrc2=size=320x240:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440",
            "-t",
            "2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            str(source / "ready.mp4"),
        ]
    )
    run(
        [
            "ffmpeg",
            *common,
            "-i",
            "testsrc2=size=320x240:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=330",
            "-t",
            "2",
            "-c:v",
            "mpeg4",
            "-c:a",
            "libmp3lame",
            str(source / "legacy.avi"),
        ]
    )
    run(
        [
            "ffmpeg",
            *common,
            "-i",
            "testsrc2=size=320x240:rate=25",
            "-t",
            "2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            str(source / "nonalpha.mov"),
        ]
    )
    shutil.copy2(source / "nonalpha.mov", source / "m6+.mov")


def write_archive(source: Path, destination: Path) -> None:
    with zipfile.ZipFile(destination, "w", zipfile.ZIP_DEFLATED) as archive:
        for name in VIDEO_FILES:
            archive.write(source / name, arcname=name)


def prepare_python_tree(repository: Path, work: Path) -> Path:
    root = work / "python"
    for relative in PYTHON_FILES:
        target = root / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(repository / relative, target)
    (root / "config").mkdir(parents=True, exist_ok=True)
    (root / "config" / "__init__.py").write_text("", encoding="utf-8")
    (root / "config" / "settings.py").write_text(
        "class config:\n"
        "    STEP1_NORMALIZE_VIDEOS = True\n"
        "    STEP1_FPS_TARGET = 25.0\n"
        "    STEP1_USE_GPU = False\n"
        "    STEP1_QUALITY_CRF = 28\n"
        "    STEP1_MAX_GPU_WORKERS = 1\n",
        encoding="utf-8",
    )
    (root / "rarfile.py").write_text(
        "class BadRarFile(Exception):\n"
        "    pass\n"
        "class RarFile:\n"
        "    def __init__(self, *args, **kwargs):\n"
        "        raise BadRarFile('stub')\n",
        encoding="utf-8",
    )
    return root


def replace_step1(config: str, key: str, value: str) -> str:
    lines = config.splitlines()
    in_step1 = False
    for index, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_step1 = stripped == "[steps.step1]"
            continue
        if in_step1 and stripped.split("=", 1)[0].strip() == key:
            lines[index] = f"{key} = {value}"
    return "\n".join(lines) + "\n"


def write_rust_config(work: Path) -> Path:
    config = (ROOT / "config" / "config.toml").read_text(encoding="utf-8")
    runtime = work / "rust-runtime"
    config = config.replace(str(ROOT / "runtime"), str(runtime))
    config = replace_step1(config, "use_gpu", "false")
    config = replace_step1(config, "gpu_workers", "1")
    path = work / "config.toml"
    path.write_text(config, encoding="utf-8")
    return path


def digest(path: Path) -> str:
    return hashlib.md5(path.read_bytes()).hexdigest()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--python-repo",
        type=Path,
        default=Path("/home/kidpixel/workflow_mediapipe"),
    )
    parser.add_argument(
        "--cli",
        type=Path,
        default=ROOT / "target/release/workflow-cli",
    )
    args = parser.parse_args()

    if not args.cli.is_file():
        print(f"CLI introuvable: {args.cli}", file=sys.stderr)
        return 2
    if not (args.python_repo / PYTHON_FILES[0]).is_file():
        print(f"Script Python introuvable sous {args.python_repo}", file=sys.stderr)
        return 2
    for tool in ("ffmpeg", "ffprobe"):
        if shutil.which(tool) is None:
            print(f"{tool} introuvable dans PATH", file=sys.stderr)
            return 2

    with tempfile.TemporaryDirectory(prefix="workflow-step1-") as temporary:
        work = Path(temporary)
        media = work / "media"
        media.mkdir()
        generate_media(media)

        sources = {"python": work / "source-python", "rust": work / "source-rust"}
        for source in sources.values():
            source.mkdir()
            write_archive(media, source / "20_Camille_parity.zip")

        python_root = prepare_python_tree(args.python_repo, work)
        environment = {
            **os.environ,
            "PYTHONPATH": str(python_root),
            "SKIP_MOV_FILES": "true",
            "PRESERVED_MOV_NAMES": "m6+.mov",
        }
        run(
            [
                sys.executable,
                "workflow_scripts/step1/extract_archives.py",
                "--source-dir",
                str(sources["python"]),
            ],
            cwd=python_root,
            env=environment,
        )

        config = write_rust_config(work)
        run(
            [
                str(args.cli),
                "--config",
                str(config),
                "step1",
                "run",
                "--source-dir",
                str(sources["rust"]),
            ],
            cwd=ROOT,
            env={**os.environ, "WORKFLOW_PATHS_BASE": str(work / "rust-runtime")},
        )

        expected = python_root / "projets_extraits" / PROJECT_NAME / "docs"
        actual = work / "rust-runtime" / "projets_extraits" / PROJECT_NAME / "docs"
        if not expected.is_dir() or not actual.is_dir():
            print(f"Sortie absente: {expected} ou {actual}", file=sys.stderr)
            return 1

        expected_files = {path.name for path in expected.iterdir() if path.is_file()}
        actual_files = {path.name for path in actual.iterdir() if path.is_file()}
        checks = 0
        differences = 0
        for name in sorted(expected_files | actual_files):
            checks += 1
            if name not in expected_files or name not in actual_files:
                print(f"ABSENT: {name}")
                differences += 1
                continue
            if digest(expected / name) != digest(actual / name):
                print(f"DIFF: {name}")
                differences += 1

        print(f"Résultat lot 6: {checks - differences}/{checks} fichiers identiques")
        return 0 if differences == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
