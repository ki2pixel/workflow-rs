#!/usr/bin/env python3
"""Scan the 75 (71 available) exports for final geometry and black bars.

Read-only on the corpus. cropdetect runs at 1 fps; the full decode is needed
because cropdetect is a filter, but 4 workers keep the wall time reasonable.
"""
from __future__ import annotations

import csv
import json
import re
import subprocess
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

import common

OUT = common.out_dir()
LOGS = OUT / "export_logs"
LOGS.mkdir(parents=True, exist_ok=True)

CROP_RE = re.compile(r"crop=(\d+):(\d+):(\d+):(\d+)")
FPS = 1


def probe(path: Path) -> dict:
    cmd = [
        "ffprobe",
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,r_frame_rate,duration,nb_frames",
        "-of",
        "json",
        str(path),
    ]
    try:
        raw = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        data = json.loads(raw.stdout or "{}")
        stream = (data.get("streams") or [{}])[0]
        return {
            "width": stream.get("width"),
            "height": stream.get("height"),
            "fps": stream.get("r_frame_rate"),
            "duration": stream.get("duration"),
            "frames": stream.get("nb_frames"),
        }
    except Exception as exc:  # noqa: BLE001
        return {"error": str(exc)}


def scan(path: Path) -> dict:
    log_path = LOGS / (path.name.replace("/", "_") + ".log")
    cmd = [
        "ffmpeg",
        "-hide_banner",
        "-nostats",
        "-threads",
        "4",
        "-i",
        str(path),
        "-an",
        "-vf",
        f"fps={FPS},cropdetect=24:2:0",
        "-f",
        "null",
        "-",
    ]
    try:
        proc = subprocess.run(
            cmd, capture_output=True, text=True, timeout=1800
        )
        log_path.write_text(proc.stderr, encoding="utf-8")
        crops = CROP_RE.findall(proc.stderr)
    except Exception as exc:  # noqa: BLE001
        return {"error": str(exc), "crops": []}
    modes: dict[tuple[int, int, int, int], int] = {}
    for w, h, x, y in crops:
        key = (int(w), int(h), int(x), int(y))
        modes[key] = modes.get(key, 0) + 1
    total = len(crops)
    barred = [
        (key, count)
        for key, count in modes.items()
        if key[0] < 1080 or key[1] < 1920 or key[2] > 0 or key[3] > 0
    ]
    barred_frames = sum(count for _, count in barred)
    top = max(modes.items(), key=lambda item: item[1]) if modes else None
    return {
        "crops": [{"w": k[0], "h": k[1], "x": k[2], "y": k[3], "n": v} for k, v in sorted(modes.items(), key=lambda i: -i[1])],
        "samples": total,
        "barred_samples": barred_frames,
        "barred_frac": round(barred_frames / total, 3) if total else None,
        "dominant": {"w": top[0][0], "h": top[0][1], "x": top[0][2], "y": top[0][3], "n": top[1]} if top else None,
        "max_top": max((k[3] for k in modes), default=None),
        "max_bottom": max((1920 - (k[1] + k[3]) for k in modes), default=None),
        "max_left": max((k[2] for k in modes), default=None),
        "max_right": max((1080 - (k[0] + k[2]) for k in modes), default=None),
        "log": str(log_path),
    }


def main() -> None:
    projects = [p for p in common.discover() if p["export_video"].is_file()]
    rows = []
    with ThreadPoolExecutor(max_workers=4) as pool:
        futures = {
            pool.submit(run_one, project): project for project in projects
        }
        for future in as_completed(futures):
            project = futures[future]
            try:
                row = future.result()
            except Exception as exc:  # noqa: BLE001
                row = {"project": f"{project['folder']}/{project['stem']}", "error": str(exc)}
            rows.append(row)
            print(row.get("project"), row.get("barred_frac"), row.get("dominant"), flush=True)
    rows.sort(key=lambda r: r.get("project") or "")
    columns = [
        "project",
        "folder",
        "stem",
        "width",
        "height",
        "fps",
        "duration",
        "filesize",
        "samples",
        "barred_samples",
        "barred_frac",
        "max_top",
        "max_bottom",
        "max_left",
        "max_right",
        "dominant",
        "crops",
        "log",
        "error",
    ]
    with (OUT / "exports_scan.csv").open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            row = dict(row)
            if isinstance(row.get("dominant"), dict):
                row["dominant"] = json.dumps(row["dominant"], ensure_ascii=False)
            if isinstance(row.get("crops"), list):
                row["crops"] = json.dumps(row["crops"], ensure_ascii=False)
            writer.writerow(row)
    print(f"Wrote {OUT/'exports_scan.csv'} ({len(rows)} exports)")


def run_one(project: dict) -> dict:
    path = project["export_video"]
    info = probe(path)
    scan_info = scan(path)
    return {
        "project": f"{project['folder']}/{project['stem']}",
        "folder": project["folder"],
        "stem": project["stem"],
        "filesize": path.stat().st_size,
        **info,
        **scan_info,
    }


if __name__ == "__main__":
    main()
