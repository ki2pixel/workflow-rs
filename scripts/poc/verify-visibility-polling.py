#!/usr/bin/env python3
"""Vérifie que les pollers opt-in sont suspendus quand la WebView est masquée.

Le script pilote une instance déjà lancée (serveur + client Tauri) : il
minimise la fenêtre, compte les requêtes `/api/system_monitor` et
`/api/csv_downloads_status` dans le journal du serveur, puis restaure la
fenêtre et vérifie que les requêtes reprennent.

Prérequis : `wmctrl`, `xdotool`, `xprop`, et un serveur démarré avec
`RUST_LOG=workflow_server=debug,tower_http=debug`.

Exemple :
    python3 scripts/poc/verify-visibility-polling.py \
        --server-log /tmp/opencode/mvp-frontend/server.log
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

MONITOR_PATH = "/api/system_monitor"
DOWNLOADS_PATH = "/api/csv_downloads_status"


def run(*command: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, check=True, text=True, capture_output=True)


def window_id(title: str) -> str:
    result = run("wmctrl", "-l")
    for line in result.stdout.splitlines():
        fields = line.split(None, 3)
        if len(fields) >= 4 and title in fields[3]:
            return fields[0]
    raise RuntimeError(f"fenêtre introuvable: {title!r}")


def is_hidden(wid: str) -> bool:
    result = run("xprop", "-id", wid, "_NET_WM_STATE")
    return "_NET_WM_STATE_HIDDEN" in result.stdout


def wait_hidden(wid: str, expected: bool, timeout: float = 5.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if is_hidden(wid) is expected:
            return
        time.sleep(0.1)
    raise RuntimeError(f"état caché={expected} non atteint pour {wid}")


def line_count(log_path: Path) -> int:
    with log_path.open("r", encoding="utf-8", errors="replace") as handle:
        return sum(1 for _ in handle)


def count_requests(log_path: Path, start_line: int) -> dict[str, int]:
    counts = {MONITOR_PATH: 0, DOWNLOADS_PATH: 0}
    with log_path.open("r", encoding="utf-8", errors="replace") as handle:
        for index, line in enumerate(handle, start=1):
            if index <= start_line:
                continue
            for path in counts:
                if f"path={path}" in line:
                    counts[path] += 1
    return counts


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--server-log", required=True, type=Path)
    parser.add_argument("--window-title", default="Workflow Manager")
    parser.add_argument("--hidden-seconds", type=float, default=15.0)
    parser.add_argument("--visible-seconds", type=float, default=8.0)
    parser.add_argument("--json-out", type=Path)
    args = parser.parse_args()

    if not args.server_log.is_file():
        print(f"ERREUR: journal absent: {args.server_log}", file=sys.stderr)
        return 2

    wid = window_id(args.window_title)
    before = line_count(args.server_log)

    run("xdotool", "windowminimize", wid)
    wait_hidden(wid, True)
    time.sleep(args.hidden_seconds)
    hidden_counts = count_requests(args.server_log, before)

    run("wmctrl", "-a", args.window_title)
    wait_hidden(wid, False)
    visible_start = line_count(args.server_log)
    time.sleep(args.visible_seconds)
    visible_counts = count_requests(args.server_log, visible_start)

    summary = {
        "window": wid,
        "hidden_seconds": args.hidden_seconds,
        "visible_seconds": args.visible_seconds,
        "hidden": hidden_counts,
        "visible": visible_counts,
    }
    print(json.dumps(summary, indent=2, ensure_ascii=False))
    if args.json_out:
        args.json_out.write_text(
            json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )

    failures = []
    if any(count != 0 for count in hidden_counts.values()):
        failures.append("des requêtes ont continué pendant le masquage")
    if any(count == 0 for count in visible_counts.values()):
        failures.append("les requêtes n'ont pas repris au focus")
    if failures:
        for failure in failures:
            print(f"ERREUR: {failure}", file=sys.stderr)
        return 1
    print("OK: pollers suspendus au masquage et restaurés au focus")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"ERREUR: {error}", file=sys.stderr)
        sys.exit(2)
