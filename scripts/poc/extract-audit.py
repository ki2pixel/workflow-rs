#!/usr/bin/env python3
"""Extrait l'audit A-Strict depuis driver-urls.log ou ready, en fallback."""

from __future__ import annotations

import json
import sys
import urllib.parse
from pathlib import Path


def main() -> None:
    base = Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/opencode/poc-gui/j1")
    urls_path = base / "driver-urls.log"
    ready_path = base / "ready"

    candidates: list[str] = []
    if urls_path.exists():
        candidates.extend(urls_path.read_text(encoding="utf-8").splitlines())
    if ready_path.exists():
        candidates.append(ready_path.read_text(encoding="utf-8").strip())

    url = next(
        (candidate for candidate in reversed(candidates) if "#poc=" in candidate),
        None,
    )
    if url is None:
        raise SystemExit("aucune URL #poc= dans driver-urls.log/ready")

    fragment = urllib.parse.urlparse(url).fragment
    if not fragment.startswith("poc="):
        raise SystemExit(f"fragment inattendu: {fragment!r}")

    payload = json.loads(urllib.parse.unquote(fragment[len("poc=") :]))
    (base / "audit.json").write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    print(json.dumps(payload, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
