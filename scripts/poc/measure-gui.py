#!/usr/bin/env python3
"""Mesure RSS/PSS WebKit + VRAM globale pour le PoC Tauri A-Strict."""

from __future__ import annotations

import argparse
import csv
import json
import os
import signal
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

import psutil

STOP = False


def _stop(*_args) -> None:
    global STOP
    STOP = True


signal.signal(signal.SIGTERM, _stop)
signal.signal(signal.SIGINT, _stop)


def run(args: list[str], timeout: float = 5) -> tuple[int | None, str, str]:
    try:
        proc = subprocess.run(
            args, capture_output=True, text=True, timeout=timeout, check=False
        )
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except Exception as error:  # pragma: no cover - dépend de l'environnement
        return None, "", str(error)


def vram() -> tuple[int | None, int | None, str]:
    rc, out, err = run(
        [
            "nvidia-smi",
            "--query-gpu=memory.used,memory.total",
            "--format=csv,noheader,nounits",
            "--id=0",
        ]
    )
    if rc != 0 or not out:
        return None, None, err
    used, total = (int(value.strip()) for value in out.splitlines()[0].split(","))
    return used, total, ""


def tree(root: psutil.Process) -> dict[int, str]:
    procs: dict[int, str] = {}
    try:
        candidates = [root, *root.children(recursive=True)]
    except psutil.Error:
        candidates = [root]
    for proc in candidates:
        try:
            procs[proc.pid] = proc.name()
        except psutil.Error:
            pass

    # Filet de sécurité : WebKit peut être reparenté, mais reste dans la session.
    try:
        sid = os.getsid(root.pid)
        started = root.create_time() - 2
        for proc in psutil.process_iter(["pid", "name", "create_time"]):
            try:
                info = proc.info
                if info["pid"] in procs:
                    continue
                name = info["name"] or ""
                if (
                    name.startswith("WebKit")
                    and info["create_time"] >= started
                    and os.getsid(proc.pid) == sid
                ):
                    procs[info["pid"]] = name
            except psutil.Error:
                pass
    except psutil.Error:
        pass
    return procs


def rss_pss(procs: dict[int, str]) -> tuple[int, int | None]:
    rss = 0
    pss = 0
    pss_ok = True
    for pid in procs:
        try:
            proc = psutil.Process(pid)
            rss += proc.memory_info().rss // 1024
            try:
                pss += proc.memory_full_info().pss // 1024
            except psutil.Error:
                pss_ok = False
        except psutil.Error:
            pass
    return rss, (pss if pss_ok else None)


def firefox(pid: int | None) -> int:
    if pid is None:
        candidates = [
            proc
            for proc in psutil.process_iter(["pid", "name"])
            if (proc.info["name"] or "") == "firefox"
        ]
        if not candidates:
            return 0
        pid = max(candidates, key=lambda proc: proc.memory_info().rss).pid
    try:
        root = psutil.Process(pid)
        procs = {root.pid: root.name()}
        for proc in root.children(recursive=True):
            try:
                procs[proc.pid] = proc.name()
            except psutil.Error:
                pass
        rss, _ = rss_pss(procs)
        return rss
    except psutil.Error:
        return 0


def pmon(procs: dict[int, str]) -> str:
    rc, out, err = run(["nvidia-smi", "pmon", "-c", "1"])
    if rc != 0:
        return err
    keep = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 3 and parts[1].isdigit() and int(parts[1]) in procs:
            keep.append(line.strip())
    return " | ".join(keep)


def stats(values: list[int | None]) -> dict[str, float | int]:
    clean = [value for value in values if value is not None]
    if not clean:
        return {}
    return {
        "n": len(clean),
        "min": min(clean),
        "median": statistics.median(clean),
        "max": max(clean),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--phase", default="baseline")
    parser.add_argument("--out", required=True)
    parser.add_argument("--duration", type=float, default=60)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--ready-file")
    parser.add_argument("--ready-timeout", type=float, default=30)
    parser.add_argument("--firefox-pid", type=int)
    args = parser.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    if args.ready_file:
        deadline = time.time() + args.ready_timeout
        while not Path(args.ready_file).exists():
            if time.time() > deadline:
                sys.exit("timeout: GUI non prête")
            time.sleep(0.1)

    root = psutil.Process(args.pid)
    rows: list[dict[str, object]] = []
    start = time.monotonic()
    index = 0

    while not STOP and (
        args.duration <= 0 or time.monotonic() - start < args.duration
    ):
        try:
            procs = tree(root)
        except psutil.NoSuchProcess:
            break

        rss, pss = rss_pss(procs)
        used, total, vram_error = vram()
        firefox_rss = firefox(args.firefox_pid)
        pmon_line = pmon(procs) if index % 5 == 0 else ""

        if index % 5 == 0:
            rc, raw, err = run(["nvidia-smi", "pmon", "-c", "1"])
            with (out / "pmon.log").open("a", encoding="utf-8") as handle:
                handle.write(
                    f"# {datetime.now(timezone.utc).isoformat()} rc={rc}\n{raw}\n{err}\n"
                )
        if index % 10 == 0:
            for name, command in [
                ("fuser.log", ["fuser", "-v", "/dev/nvidia0"]),
                (
                    "compute-apps.log",
                    [
                        "nvidia-smi",
                        "--query-compute-apps=pid,process_name,used_memory",
                        "--format=csv,noheader",
                    ],
                ),
            ]:
                rc, stdout, stderr = run(command)
                with (out / name).open("a", encoding="utf-8") as handle:
                    handle.write(
                        f"# {datetime.now(timezone.utc).isoformat()} rc={rc}\n"
                        f"{stdout}\n{stderr}\n"
                    )

        rows.append(
            {
                "ts": datetime.now(timezone.utc).isoformat(),
                "elapsed_s": round(time.monotonic() - start, 3),
                "profile": args.profile,
                "phase": args.phase,
                "gui_pid": args.pid,
                "pids": "|".join(
                    f"{pid}:{name}" for pid, name in sorted(procs.items())
                ),
                "rss_kib": rss,
                "pss_kib": pss if pss is not None else "",
                "vram_used_mib": used if used is not None else "",
                "vram_total_mib": total if total is not None else "",
                "firefox_rss_kib": firefox_rss,
                "pmon": pmon_line,
                "note": vram_error,
            }
        )
        index += 1
        time.sleep(args.interval)

    fields = (
        list(rows[0].keys())
        if rows
        else [
            "ts",
            "elapsed_s",
            "profile",
            "phase",
            "gui_pid",
            "pids",
            "rss_kib",
            "pss_kib",
            "vram_used_mib",
            "vram_total_mib",
            "firefox_rss_kib",
            "pmon",
            "note",
        ]
    )
    with (out / "metrics.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)

    summary = {
        "profile": args.profile,
        "phase": args.phase,
        "gui_pid": args.pid,
        "duration_s": round(time.monotonic() - start, 3),
        "samples": len(rows),
        "rss_kib": stats([row["rss_kib"] for row in rows]),
        "pss_kib": stats([row["pss_kib"] for row in rows]),
        "vram_used_mib": stats([row["vram_used_mib"] for row in rows]),
        "vram_total_mib": stats([row["vram_total_mib"] for row in rows]),
        "firefox_rss_kib": stats([row["firefox_rss_kib"] for row in rows]),
        "pids": sorted(
            {pid for row in rows for pid in str(row["pids"]).split("|") if pid}
        ),
    }
    (out / "summary.json").write_text(
        json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(json.dumps(summary, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
