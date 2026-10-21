#!/usr/bin/env python3
"""Point d'entrée de parité pour la normalisation STEP1.

Le script STEP1 réel est inseparable de `BASE_DIR/projets_extraits` : il extrait
les archives dans le dépôt de production. Pour le harnais, on appelle donc la
même fonction `normalize_videos_in` sur un dossier isolé, avec le même
environnement et le même venv.
"""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: normalize_case.py <dossier> [--force]", file=sys.stderr)
        return 2
    work_dir = Path(sys.argv[1]).resolve()
    force = "--force" in sys.argv[2:]
    old_base = Path(os.environ.get("OLD_BASE", "/home/kidpixel/workflow_mediapipe"))
    sys.path.insert(0, str(old_base))

    from utils.media_normalizer import normalize_videos_in

    summary = normalize_videos_in(work_dir, force=force)
    payload = {
        "total": summary.total,
        "success": summary.success,
        "failed": summary.failed,
        "skipped": summary.skipped,
        "is_success": summary.is_success,
    }
    print("NORMALIZE_SUMMARY: " + json.dumps(payload, ensure_ascii=False), flush=True)
    return 0 if summary.is_success else 1


if __name__ == "__main__":
    raise SystemExit(main())
