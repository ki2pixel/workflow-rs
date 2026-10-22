#!/usr/bin/env python3
"""Validation shadow STEP5 + STEP6 sur des projets de production.

Le script lit uniquement les fichiers de production, écrit les sorties Rust
dans un répertoire séparé, puis compare les JSON avec le comparateur sémantique
du harnais de parité. Il refuse d'écrire sous `/mnt/cache`.
"""
from __future__ import annotations

import argparse
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools" / "parity"))
from compare import DiffResult, compare_json_files  # noqa: E402


@dataclass
class Check:
    label: str
    equal: bool
    diffs: int = 0
    seconds: float = 0.0
    details: list[str] = field(default_factory=list)


def run_cli(cli: Path, args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [str(cli), *args],
        capture_output=True,
        text=True,
        check=False,
    )


def compare_json(expected: Path, actual: Path) -> tuple[bool, int, list[str]]:
    result = DiffResult()
    compare_json_files(expected, actual, result)
    details = [
        f"{diff.path}: expected={diff.expected!r} actual={diff.actual!r} ({diff.kind})"
        for diff in result.differences
    ]
    return result.equal, result.diff_count, details


def check_file(
    cli: Path,
    label: str,
    args: list[str],
    expected: Path,
    actual: Path,
) -> Check:
    start = time.monotonic()
    process = run_cli(cli, args)
    elapsed = time.monotonic() - start
    if process.returncode != 0:
        return Check(
            label=label,
            equal=False,
            diffs=1,
            seconds=elapsed,
            details=[process.stderr.strip() or f"code {process.returncode}"],
        )
    if not actual.exists():
        return Check(
            label=label,
            equal=False,
            diffs=1,
            seconds=elapsed,
            details=[f"sortie absente: {actual}"],
        )
    equal, diffs, details = compare_json(expected, actual)
    return Check(label=label, equal=equal, diffs=diffs, seconds=elapsed, details=details)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("/mnt/cache"))
    parser.add_argument("--out", type=Path, default=Path("/tmp/workflow-shadow"))
    parser.add_argument("--cli", type=Path, default=ROOT / "target/release/workflow-cli")
    parser.add_argument("--projects", nargs="*", default=None)
    parser.add_argument("--limit", type=int, default=10)
    args = parser.parse_args()

    root = args.root.resolve()
    out = args.out.resolve()
    if out == root or root in out.parents:
        print(f"refus d'écrire dans la production: {out}", file=sys.stderr)
        return 2
    if not args.cli.exists():
        print(f"CLI introuvable: {args.cli}", file=sys.stderr)
        return 2
    if not root.is_dir():
        print(f"racine introuvable: {root}", file=sys.stderr)
        return 2

    candidates: list[tuple[str, Path]] = []
    for docs in sorted(root.glob("*/docs")):
        if not docs.is_dir():
            continue
        if not list(docs.glob("*_tracking.json")):
            continue
        if not list(docs.glob("*_ae.json")):
            continue
        project = docs.parent.name
        if args.projects and project not in args.projects:
            continue
        candidates.append((project, docs))
    candidates = candidates[: max(1, args.limit)]
    if not candidates:
        print("aucun projet de production compatible trouvé", file=sys.stderr)
        return 2

    out.mkdir(parents=True, exist_ok=True)
    failures = 0
    total = 0

    print(f"Shadow STEP5/6: {len(candidates)} projet(s) -> {out}")
    for project, docs in candidates:
        shadow = out / project
        shadow.mkdir(parents=True, exist_ok=True)
        print(f"\n== {project}")

        for tracking in sorted(docs.glob("*_tracking.json")):
            stem = tracking.name[: -len("_tracking.json")]
            audio = docs / f"{stem}_audio.json"
            production_ae = docs / f"{stem}_ae.json"
            csv = docs / f"{stem}.csv"
            if not audio.exists() or not production_ae.exists():
                print(f"  {stem}: audio ou _ae.json absent, ignoré")
                continue

            shadow_audio = shadow / f"{stem}_audio.json"
            shadow_tracking = shadow / f"{stem}_tracking.json"
            shadow_ae = shadow / f"{stem}_ae.json"

            checks = [
                check_file(
                    args.cli,
                    "step5_audio",
                    ["step5", "audio", "--input", str(audio), "--output", str(shadow_audio)],
                    audio,
                    shadow_audio,
                ),
                check_file(
                    args.cli,
                    "step5_video",
                    [
                        "step5",
                        "video",
                        "--input",
                        str(tracking),
                        "--output",
                        str(shadow_tracking),
                        "--audio",
                        str(audio),
                    ],
                    tracking,
                    shadow_tracking,
                ),
                check_file(
                    args.cli,
                    "step6",
                    [
                        "step6",
                        "build",
                        "--tracking",
                        str(shadow_tracking),
                        "--audio",
                        str(shadow_audio),
                        "--csv",
                        str(csv),
                        "--output",
                        str(shadow_ae),
                    ],
                    production_ae,
                    shadow_ae,
                ),
            ]

            for check in checks:
                total += 1
                status = "OK" if check.equal else f"DIFF({check.diffs})"
                print(f"  {stem[:48]:48} {check.label:12} {status} {check.seconds:.3f}s")
                if not check.equal:
                    failures += 1
                    for detail in check.details[:5]:
                        print(f"      {detail}")

    print(f"\nRésultat: {total - failures}/{total} vérifications identiques")
    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
