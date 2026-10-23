#!/usr/bin/env python3
"""Valide les modes batch STEP5/STEP6 du lot 5 sans écrire dans la production.

Les projets sont copiés dans un répertoire temporaire, les vidéos sont
remplacées par des fichiers vides (seul le nom sert à la découverte), puis les
sorties Rust sont comparées sémantiquement aux JSON de production.
"""
from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def copy_project(source: Path, destination: Path) -> Path:
    docs = source / "docs"
    if not docs.is_dir():
        raise FileNotFoundError(docs)
    target = destination / source.name / "docs"
    target.mkdir(parents=True)
    for pattern in ("*.json", "*.csv"):
        for path in docs.glob(pattern):
            shutil.copy2(path, target / path.name)
    for path in docs.iterdir():
        if path.suffix.lower() in {".mp4", ".mov", ".avi", ".mkv", ".webm"}:
            (target / path.name).touch()
    return target


def run_cli(cli: Path, command: list[str]) -> None:
    process = subprocess.run(
        [str(cli), *command],
        cwd=ROOT,
        text=True,
        capture_output=True,
        check=False,
    )
    if process.returncode != 0:
        raise RuntimeError(
            f"{' '.join(command)} a échoué ({process.returncode})\n"
            f"{process.stdout}\n{process.stderr}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("/mnt/cache/old"))
    parser.add_argument(
        "--projects",
        nargs="*",
        default=["244 Cvmille", "245 Cvmille", "249 Cvmille", "250 Cvmille"],
    )
    parser.add_argument("--keyword", default="Cvmille")
    parser.add_argument("--cli", type=Path, default=ROOT / "target/release/workflow-cli")
    args = parser.parse_args()

    root = args.root.resolve()
    if not args.cli.is_file():
        print(f"CLI introuvable: {args.cli}", file=sys.stderr)
        return 2

    checks = 0
    differences = 0
    with tempfile.TemporaryDirectory(prefix="workflow-lot5-") as temp:
        work = Path(temp)
        for project in args.projects:
            source = root / project
            if not (source / "docs").is_dir():
                print(f"{project}: docs absent, ignoré")
                continue
            target_docs = copy_project(source, work / "projects")
            run_cli(
                args.cli,
                [
                    "--config",
                    "config/config.toml",
                    "step5",
                    "project",
                    "--work-dir",
                    str(work / "projects"),
                    "--keyword",
                    args.keyword,
                ],
            )
            run_cli(
                args.cli,
                [
                    "--config",
                    "config/config.toml",
                    "step6",
                    "project",
                    "--work-dir",
                    str(work / "projects"),
                    "--keyword",
                    args.keyword,
                ],
            )
            for suffix in ("_audio.json", "_tracking.json", "_ae.json"):
                for expected in (source / "docs").glob(f"*{suffix}"):
                    actual = target_docs / expected.name
                    checks += 1
                    if not actual.exists():
                        print(f"{project}: ABSENT {expected.name}")
                        differences += 1
                        continue
                    left = json.loads(expected.read_text())
                    right = json.loads(actual.read_text())
                    if left != right:
                        print(f"{project}: DIFF {expected.name}")
                        differences += 1
            print(f"{project}: terminé")

    print(f"Résultat lot 5: {checks - differences}/{checks} JSON identiques")
    return 0 if differences == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
