#!/usr/bin/env python3
"""Moniteur d'endurance J4 : RSS/PSS, RAM système, VRAM et fenêtre cachée.

Échantillonnage à 1 Hz corrigé en dérive, écriture ligne par ligne, agrégats
numériques seulement. Aucun accès à la WebView.
"""

from __future__ import annotations

import argparse
import csv
import glob
import json
import logging
import math
import os
import re
import signal
import statistics
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from logging.handlers import RotatingFileHandler
from pathlib import Path
from typing import Any

import psutil

STOP = threading.Event()
OOM_RE = re.compile(
    r"CUDA_ERROR_OUT_OF_MEMORY|cudaErrorMemoryAllocation|out of memory|\bOOM\b",
    re.IGNORECASE,
)


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def run(cmd: list[str], timeout: float = 5) -> tuple[int, str, str]:
    try:
        proc = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout, check=False
        )
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except Exception as error:  # pragma: no cover - environnement
        return 1, "", str(error)


def proc_names(proc: psutil.Process) -> set[str]:
    names: set[str] = set()
    try:
        names.add(proc.name())
    except psutil.Error:
        pass
    try:
        cmdline = proc.cmdline()
        if cmdline:
            names.add(Path(cmdline[0]).name)
    except psutil.Error:
        pass
    return {name for name in names if name}


def session_members(sid: int) -> list[tuple[psutil.Process, set[str]]]:
    members: list[tuple[psutil.Process, set[str]]] = []
    for proc in psutil.process_iter(["pid", "name", "cmdline"]):
        try:
            if os.getsid(proc.info["pid"]) != sid:
                continue
            members.append((proc, proc_names(proc)))
        except (psutil.Error, ProcessLookupError, PermissionError, OSError):
            continue
    return members


def classify_gui(
    members: list[tuple[psutil.Process, set[str]]]
) -> dict[str, list[tuple[psutil.Process, set[str]]]]:
    groups: dict[str, list[tuple[psutil.Process, set[str]]]] = {
        "host": [],
        "network": [],
        "web": [],
        "extra": [],
    }
    for proc, names in members:
        if any(name.startswith("workflow-gui") for name in names):
            groups["host"].append((proc, names))
        elif any(name.startswith("WebKitNetwork") for name in names):
            groups["network"].append((proc, names))
        elif any(name.startswith("WebKitWeb") for name in names):
            groups["web"].append((proc, names))
        elif any(name.startswith("WebKit") for name in names):
            groups["extra"].append((proc, names))
    return groups


def find_server(pid: int | None) -> psutil.Process | None:
    if pid:
        try:
            proc = psutil.Process(pid)
            if proc.is_running():
                return proc
        except psutil.Error:
            return None
    for proc in psutil.process_iter(["pid", "name", "cmdline"]):
        try:
            names = proc_names(proc)
            if any(name.startswith("workflow-server") for name in names):
                return proc
        except psutil.Error:
            continue
    return None


def read_mem(proc: psutil.Process | None) -> tuple[int | None, int | None]:
    if proc is None:
        return None, None
    try:
        rss = proc.memory_info().rss // 1024
    except psutil.Error:
        return None, None
    try:
        pss = proc.memory_full_info().pss // 1024
    except psutil.Error:
        pss = None
    return rss, pss


def sum_mem(
    entries: list[tuple[psutil.Process, set[str]]]
) -> tuple[int, int | None, bool, list[int]]:
    rss = 0
    pss = 0
    complete = True
    pids: list[int] = []
    for proc, _names in entries:
        pids.append(proc.pid)
        proc_rss, proc_pss = read_mem(proc)
        if proc_rss is None:
            complete = False
            continue
        rss += proc_rss
        if proc_pss is None:
            complete = False
        else:
            pss += proc_pss
    return rss, (pss if complete else None), complete, pids


def window_state(wid: str) -> tuple[bool | None, str]:
    if not wid:
        return None, "wid absent"
    rc, out, err = run(["xprop", "-id", wid, "_NET_WM_STATE"], timeout=5)
    if rc != 0:
        return None, err or "xprop échec"
    hidden = "_NET_WM_STATE_HIDDEN" in out
    rc, out2, err2 = run(["xwininfo", "-id", wid], timeout=5)
    map_state = "inconnu"
    if rc == 0:
        for line in out2.splitlines():
            if "Map State:" in line:
                map_state = line.split("Map State:", 1)[1].strip()
                break
    else:
        map_state = err2 or "xwininfo échec"
    return hidden, map_state


def hide_window(wid: str) -> None:
    """Re-cache la fenêtre ; Mutter exige une minimisation réelle."""
    run(["wmctrl", "-i", "-r", wid, "-b", "add,hidden"], timeout=5)
    time.sleep(0.3)
    hidden, _ = window_state(wid)
    if hidden is not True:
        run(["xdotool", "windowminimize", wid], timeout=5)


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def log_size(paths: list[str]) -> int:
    total = 0
    for pattern in paths:
        for name in glob.glob(pattern):
            try:
                total += os.path.getsize(name)
            except OSError:
                pass
    return total


def scan_oom(paths: list[Path]) -> tuple[bool, list[str]]:
    matches: list[str] = []
    for path in paths:
        if not path.exists():
            continue
        try:
            with path.open("r", encoding="utf-8", errors="replace") as handle:
                for line in handle:
                    if OOM_RE.search(line):
                        if len(matches) < 20:
                            matches.append(f"{path}: {line.strip()[:300]}")
        except OSError:
            continue
    return bool(matches), matches


def stats(values: list[float]) -> dict[str, float | int | None]:
    if not values:
        return {"n": 0, "min": None, "median": None, "max": None, "p95": None}
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, math.ceil(0.95 * len(ordered)) - 1))
    return {
        "n": len(values),
        "min": min(values),
        "median": statistics.median(values),
        "max": max(values),
        "p95": ordered[index],
    }


def slope_per_min(values: list[float], interval_s: float) -> float | None:
    n = len(values)
    if n < 2 or interval_s <= 0:
        return None
    sum_x = n * (n - 1) / 2.0
    sum_y = sum(values)
    sum_xy = sum(index * value for index, value in enumerate(values))
    sum_xx = (n - 1) * n * (2 * n - 1) / 6.0
    denominator = n * sum_xx - sum_x * sum_x
    if denominator == 0:
        return None
    slope_per_sample = (n * sum_xy - sum_x * sum_y) / denominator
    return slope_per_sample * 60.0 / interval_s


def delta_first_last(values: list[float], window: int) -> float | None:
    if not values:
        return None
    size = min(window, max(1, len(values) // 2))
    first = statistics.median(values[:size])
    last = statistics.median(values[-size:])
    return last - first


class VramSampler(threading.Thread):
    def __init__(self, path: Path) -> None:
        super().__init__(daemon=True)
        self.path = path
        self.lock = threading.Lock()
        self.latest: tuple[int, int, int] | None = None
        self.peak = 0
        self.peak_ts = ""
        self.free_min: int | None = None
        self.total: int | None = None
        self.error = ""
        self.proc: subprocess.Popen[str] | None = None

    def run(self) -> None:
        cmd = [
            "nvidia-smi",
            "--query-gpu=memory.used,memory.free,memory.total",
            "--format=csv,noheader,nounits",
            "--id=0",
            "-lms",
            "200",
        ]
        try:
            with self.path.open("w", encoding="utf-8", newline="") as handle:
                handle.write("ts,used_mib,free_mib,total_mib\n")
                handle.flush()
                self.proc = subprocess.Popen(
                    cmd,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                    bufsize=1,
                )
                assert self.proc.stdout is not None
                for line in self.proc.stdout:
                    if STOP.is_set():
                        break
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        used, free, total = (
                            int(value.strip()) for value in line.split(",")
                        )
                    except (ValueError, IndexError):
                        continue
                    ts = utc_now()
                    handle.write(f"{ts},{used},{free},{total}\n")
                    handle.flush()
                    with self.lock:
                        self.latest = (used, free, total)
                        if used > self.peak:
                            self.peak = used
                            self.peak_ts = ts
                        if self.free_min is None or free < self.free_min:
                            self.free_min = free
                        self.total = total
        except Exception as error:  # pragma: no cover - environnement
            self.error = str(error)
            (self.path.parent / "vram-sampler.err").write_text(
                f"{utc_now()} {error}\n", encoding="utf-8"
            )

    def stop(self) -> None:
        if self.proc is not None and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=2)


class Monitor:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.out = Path(args.out)
        self.out.mkdir(parents=True, exist_ok=True)
        self.sampler = VramSampler(self.out / "vram-200ms.csv")
        self.rows = 0
        self.drift: list[float] = []
        self.host_rss: list[float] = []
        self.host_pss: list[float] = []
        self.network_rss: list[float] = []
        self.network_pss: list[float] = []
        self.web_rss: list[float] = []
        self.web_pss: list[float] = []
        self.gui_rss: list[float] = []
        self.gui_pss: list[float] = []
        self.server_rss: list[float] = []
        self.server_pss: list[float] = []
        self.mem_available: list[float] = []
        self.swap_used: list[float] = []
        self.hidden_values: list[bool] = []
        self.gui_alive_values: list[bool] = []
        self.pss_complete_values: list[bool] = []
        self.window_violations = 0
        self.server_failures = 0
        self.server_failure_streak = 0
        self.tray_changes = 0
        self.tray_last_visual: tuple[Any, ...] | None = None
        self.tray_final: dict[str, Any] | None = None
        self.watcher_last: dict[str, Any] | None = None
        self.monitor_rss_start = self.self_rss()
        self.started = time.monotonic()
        self.started_at = utc_now()

    def self_rss(self) -> int | None:
        try:
            return psutil.Process().memory_info().rss // 1024
        except psutil.Error:
            return None

    def tray_visual(self, payload: dict[str, Any] | None) -> tuple[Any, ...] | None:
        if not payload:
            return None
        status = payload.get("status") or {}
        progress = status.get("progress") or {}
        return (
            status.get("state"),
            status.get("headline"),
            status.get("detail"),
            progress.get("percent") if isinstance(progress, dict) else None,
        )

    def check_server(self) -> bool:
        rc, out, _ = run(
            [
                "curl",
                "-fsS",
                "--max-time",
                "2",
                f"{self.args.server_url.rstrip('/')}/sequence/status",
            ],
            timeout=4,
        )
        ok = rc == 0 and bool(out)
        if ok:
            self.server_failure_streak = 0
        else:
            self.server_failures += 1
            self.server_failure_streak += 1
        return ok

    def tick(self, index: int, writer: csv.DictWriter[str], handle: Any) -> None:
        now = time.monotonic()
        deadline = self.started + index * self.args.interval
        drift_ms = (now - deadline) * 1000.0
        self.drift.append(drift_ms)

        hidden: bool | None = None
        map_state = ""
        if index % 60 == 0:
            hidden, map_state = window_state(self.args.window_wid)
            if hidden is False:
                self.window_violations += 1
                hide_window(self.args.window_wid)
                hidden, map_state = window_state(self.args.window_wid)
            if self.args.server_url:
                self.check_server()
        else:
            hidden = self.hidden_values[-1] if self.hidden_values else None
            map_state = "échantillon précédent"

        groups: dict[str, list[tuple[psutil.Process, set[str]]]] = {
            "host": [],
            "network": [],
            "web": [],
            "extra": [],
        }
        if self.args.gui_pid:
            try:
                sid = os.getsid(self.args.gui_pid)
                groups = classify_gui(session_members(sid))
            except OSError:
                pass
        host_rss, host_pss, host_complete, host_pids = sum_mem(groups["host"])
        net_rss, net_pss, net_complete, net_pids = sum_mem(groups["network"])
        web_rss, web_pss, web_complete, web_pids = sum_mem(groups["web"])
        extra_rss, extra_pss, extra_complete, extra_pids = sum_mem(groups["extra"])
        gui_rss = host_rss + net_rss + web_rss + extra_rss
        pss_complete = (
            host_complete
            and net_complete
            and web_complete
            and extra_complete
            and bool(host_pids)
            and bool(net_pids)
            and bool(web_pids)
        )
        gui_pss = (
            (host_pss or 0) + (net_pss or 0) + (web_pss or 0) + (extra_pss or 0)
            if pss_complete
            else None
        )
        gui_alive = bool(host_pids) and bool(net_pids) and bool(web_pids)

        server = find_server(self.args.server_pid)
        server_rss, server_pss = read_mem(server)

        memory = psutil.virtual_memory()
        swap = psutil.swap_memory()

        with self.sampler.lock:
            latest = self.sampler.latest
        vram_used = latest[0] if latest else None
        vram_free = latest[1] if latest else None

        if index % 60 == 0:
            self.watcher_last = read_json(Path(self.args.watcher_stats_file))
            tray_payload = read_json(Path(self.args.tray_state_file))
            visual = self.tray_visual(tray_payload)
            if visual is not None:
                if self.tray_last_visual is not None and visual != self.tray_last_visual:
                    self.tray_changes += 1
                self.tray_last_visual = visual
            if tray_payload:
                self.tray_final = tray_payload

        watcher = (self.watcher_last or {}).get("watcher") or {}
        tray_status = (self.tray_final or {}).get("status") or {}

        row = {
            "tick": index,
            "ts": utc_now(),
            "elapsed_s": round(now - self.started, 3),
            "drift_ms": round(drift_ms, 3),
            "window_hidden": "" if hidden is None else hidden,
            "window_state": map_state,
            "host_pids": "|".join(str(pid) for pid in host_pids),
            "host_rss_kib": host_rss,
            "host_pss_kib": host_pss if host_pss is not None else "",
            "network_pids": "|".join(str(pid) for pid in net_pids),
            "network_rss_kib": net_rss,
            "network_pss_kib": net_pss if net_pss is not None else "",
            "web_pids": "|".join(str(pid) for pid in web_pids),
            "web_rss_kib": web_rss,
            "web_pss_kib": web_pss if web_pss is not None else "",
            "extra_pids": "|".join(str(pid) for pid in extra_pids),
            "gui_proc_count": len(host_pids) + len(net_pids) + len(web_pids) + len(extra_pids),
            "gui_rss_kib": gui_rss,
            "gui_pss_kib": gui_pss if gui_pss is not None else "",
            "pss_complete": pss_complete,
            "gui_alive": gui_alive,
            "server_pid": server.pid if server else "",
            "server_rss_kib": server_rss if server_rss is not None else "",
            "server_pss_kib": server_pss if server_pss is not None else "",
            "mem_total_kib": memory.total // 1024,
            "mem_available_kib": memory.available // 1024,
            "swap_total_kib": swap.total // 1024,
            "swap_used_kib": swap.used // 1024,
            "vram_used_mib": vram_used if vram_used is not None else "",
            "vram_free_mib": vram_free if vram_free is not None else "",
            "watcher_ticks": watcher.get("ticks", ""),
            "watcher_errors": watcher.get("errors", ""),
            "watcher_latency_ms": watcher.get("last_latency_ms", ""),
            "tray_state": tray_status.get("state", ""),
            "tray_headline": tray_status.get("headline", ""),
            "tray_detail": tray_status.get("detail", ""),
            "note": "",
        }
        writer.writerow(row)
        handle.flush()
        self.rows += 1

        self.host_rss.append(host_rss)
        if host_pss is not None:
            self.host_pss.append(host_pss)
        self.network_rss.append(net_rss)
        if net_pss is not None:
            self.network_pss.append(net_pss)
        self.web_rss.append(web_rss)
        if web_pss is not None:
            self.web_pss.append(web_pss)
        self.gui_rss.append(gui_rss)
        if gui_pss is not None:
            self.gui_pss.append(gui_pss)
        if server_rss is not None:
            self.server_rss.append(server_rss)
        if server_pss is not None:
            self.server_pss.append(server_pss)
        self.mem_available.append(memory.available // 1024)
        self.swap_used.append(swap.used // 1024)
        if hidden is not None:
            self.hidden_values.append(hidden)
        self.gui_alive_values.append(gui_alive)
        self.pss_complete_values.append(pss_complete)

        if index % 60 == 0:
            logging.info(
                "t=%5.0fs drift=%6.1fms hidden=%s host=%.1fMio gui=%.1fMio "
                "vram=%sMio watcher=%s/%s tray=%s",
                now - self.started,
                drift_ms,
                hidden,
                host_rss / 1024,
                gui_rss / 1024,
                vram_used,
                watcher.get("ticks", "?"),
                watcher.get("errors", "?"),
                tray_status.get("state", "?"),
            )

    def summary(self) -> dict[str, Any]:
        interval = self.args.interval
        expected = int(self.args.duration / interval)
        # Dernière lecture après la boucle : les compteurs du watcher et l'état
        # du tray peuvent avoir progressé depuis le dernier échantillon 60 s.
        fresh_watcher = read_json(Path(self.args.watcher_stats_file))
        if fresh_watcher:
            self.watcher_last = fresh_watcher
        fresh_tray = read_json(Path(self.args.tray_state_file))
        if fresh_tray:
            self.tray_final = fresh_tray
            visual = self.tray_visual(fresh_tray)
            if visual is not None:
                if (
                    self.tray_last_visual is not None
                    and visual != self.tray_last_visual
                ):
                    self.tray_changes += 1
                self.tray_last_visual = visual
        hidden_ratio = (
            sum(1 for value in self.hidden_values if value) / len(self.hidden_values)
            if self.hidden_values
            else 0.0
        )
        gui_alive_ratio = (
            sum(1 for value in self.gui_alive_values if value)
            / len(self.gui_alive_values)
            if self.gui_alive_values
            else 0.0
        )
        pss_ratio = (
            sum(1 for value in self.pss_complete_values if value)
            / len(self.pss_complete_values)
            if self.pss_complete_values
            else 0.0
        )
        drift_stats = stats(self.drift)
        host_delta = delta_first_last(self.host_rss, 600)
        gui_delta = delta_first_last(self.gui_rss, 600)
        pss_delta = delta_first_last(self.gui_pss, 600)
        network_delta = delta_first_last(self.network_rss, 600)
        web_delta = delta_first_last(self.web_rss, 600)
        server_delta = delta_first_last(self.server_rss, 600)
        mem_delta = delta_first_last(self.mem_available, 600)
        swap_delta = delta_first_last(self.swap_used, 600)
        host_slope = slope_per_min(self.host_rss, interval)
        gui_slope = slope_per_min(self.gui_rss, interval)

        host_log_bytes = log_size([str(self.out / "host.log*")])
        server_log_bytes = log_size(
            [str(self.out / "server.log*"), str(self.out / "server.err*")]
        )
        progress_bytes = log_size([str(self.out / "progress.log*")])
        gui_stdout_bytes = log_size([str(self.out / "gui.stdout.log*")])
        gui_stderr_bytes = log_size([str(self.out / "gui.stderr.log*")])
        console_bytes = log_size([str(self.out / "console.log*")])
        csv_bytes = log_size([str(self.out / "endurance.csv")])
        vram_csv_bytes = log_size([str(self.out / "vram-200ms.csv")])
        oom_paths: list[Path] = []
        for pattern in (
            "host.log*",
            "server.log*",
            "server.err*",
            "progress.log*",
            "gui.stdout.log*",
            "gui.stderr.log*",
            "console.log*",
        ):
            oom_paths.extend(Path(name) for name in glob.glob(str(self.out / pattern)))
        oom, oom_matches = scan_oom(oom_paths)
        tray_state = ((self.tray_final or {}).get("status") or {}).get("state")
        watcher = (self.watcher_last or {}).get("watcher") or {}

        # Enveloppe GUI recalibrée le 2026-10-25 (ratifiée) : le plafond PSS
        # est aligné sur le plafond RSS de 32 Mio, et la pente maximale en
        # découle (32 Mio / 120 min = 0,267 Mio/min). Les autres seuils sont
        # inchangés.
        gui_slope_limit_kib = (32.0 / 120.0) * 1024.0
        host_rss_delta_ok = host_delta is not None and host_delta <= 12 * 1024
        host_rss_slope_ok = host_slope is None or host_slope <= 0.08 * 1024
        gui_rss_delta_ok = gui_delta is not None and gui_delta <= 32 * 1024
        gui_rss_slope_ok = gui_slope is None or gui_slope <= gui_slope_limit_kib
        network_rss_ok = network_delta is None or network_delta <= 32 * 1024
        web_rss_ok = web_delta is None or web_delta <= 32 * 1024

        acceptance = {
            "samples_ok": abs(self.rows - expected) <= 5,
            "drift_ok": bool(
                drift_stats["p95"] is not None
                and drift_stats["p95"] <= 50
                and drift_stats["max"] <= 250
            ),
            "hidden_ok": hidden_ratio >= 0.99,
            "gui_alive_ok": gui_alive_ratio >= 0.999,
            "host_rss_delta_ok": host_rss_delta_ok,
            "host_rss_slope_ok": host_rss_slope_ok,
            "host_rss_ok": bool(host_rss_delta_ok and host_rss_slope_ok),
            "gui_rss_delta_ok": gui_rss_delta_ok,
            "gui_rss_slope_ok": gui_rss_slope_ok,
            "gui_rss_ok": bool(gui_rss_delta_ok and gui_rss_slope_ok),
            "gui_pss_ok": bool(pss_delta is not None and pss_delta <= 32 * 1024),
            "network_rss_ok": network_rss_ok,
            "web_rss_ok": web_rss_ok,
            "webkit_ok": bool(network_rss_ok and web_rss_ok),
            "server_rss_ok": bool(server_delta is None or server_delta <= 12 * 1024),
            "memory_ok": bool(
                (mem_delta is None or mem_delta >= -256 * 1024)
                and (swap_delta is None or swap_delta <= 16 * 1024)
            ),
            "logs_ok": bool(
                host_log_bytes <= 16 * 1024 * 1024
                and server_log_bytes <= 16 * 1024 * 1024
                and progress_bytes <= 8 * 1024 * 1024
                and gui_stdout_bytes <= 8 * 1024 * 1024
                and gui_stderr_bytes <= 8 * 1024 * 1024
                and console_bytes <= 8 * 1024 * 1024
                and csv_bytes <= 8 * 1024 * 1024
            ),
            "tray_ok": bool(tray_state == "Idle" and self.tray_changes <= 20),
            "pss_complete_ok": pss_ratio >= 0.999,
            "vram_ok": bool(
                not self.sampler.error
                and self.sampler.peak < self.args.abort_vram
                and (
                    self.sampler.free_min is None
                    or self.sampler.free_min > 196
                )
            ),
            "oom_free": not oom,
            "server_ok": self.server_failures == 0,
        }
        acceptance["acceptance_ok"] = all(acceptance.values())

        return {
            "started_at": self.started_at,
            "finished_at": utc_now(),
            "duration_s": round(time.monotonic() - self.started, 3),
            "interval_s": interval,
            "expected_samples": expected,
            "samples": self.rows,
            "drift_ms": drift_stats,
            "window": {
                "wid": self.args.window_wid,
                "hidden_ratio": hidden_ratio,
                "violations": self.window_violations,
                "samples": len(self.hidden_values),
            },
            "gui_alive_ratio": gui_alive_ratio,
            "pss_complete_ratio": pss_ratio,
            "rss_kib": {
                "host": stats(self.host_rss),
                "network": stats(self.network_rss),
                "web": stats(self.web_rss),
                "gui": stats(self.gui_rss),
                "server": stats(self.server_rss),
            },
            "pss_kib": {
                "host": stats(self.host_pss),
                "network": stats(self.network_pss),
                "web": stats(self.web_pss),
                "gui": stats(self.gui_pss),
                "server": stats(self.server_pss),
            },
            "deltas_kib": {
                "host": host_delta,
                "gui": gui_delta,
                "gui_pss": pss_delta,
                "network": network_delta,
                "web": web_delta,
                "server": server_delta,
                "mem_available": mem_delta,
                "swap_used": swap_delta,
            },
            "slopes_kib_per_min": {
                "host": host_slope,
                "gui": gui_slope,
            },
            "memory": {
                "mem_total_kib": self.mem_available and psutil.virtual_memory().total // 1024,
                "mem_available": stats(self.mem_available),
                "swap_used": stats(self.swap_used),
            },
            "vram": {
                "peak_mib": self.sampler.peak,
                "peak_ts": self.sampler.peak_ts,
                "free_min_mib": self.sampler.free_min,
                "total_mib": self.sampler.total,
                "sampler_error": self.sampler.error,
            },
            "watcher": {
                "last": watcher,
                "server_failures": self.server_failures,
            },
            "tray": {
                "changes": self.tray_changes,
                "final_state": tray_state,
                "final": self.tray_final,
            },
            "logs_bytes": {
                "host": host_log_bytes,
                "server": server_log_bytes,
                "progress": progress_bytes,
                "gui_stdout": gui_stdout_bytes,
                "gui_stderr": gui_stderr_bytes,
                "console": console_bytes,
                "endurance_csv": csv_bytes,
                "vram_csv": vram_csv_bytes,
            },
            "oom": oom,
            "oom_matches": oom_matches,
            "monitor_rss_kib": {
                "start": self.monitor_rss_start,
                "end": self.self_rss(),
            },
            "acceptance": acceptance,
        }

    def run(self) -> int:
        log_path = self.out / "progress.log"
        handler = RotatingFileHandler(
            log_path, maxBytes=8 * 1024 * 1024, backupCount=2, encoding="utf-8"
        )
        handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(message)s"))
        logging.basicConfig(
            level=logging.INFO,
            format="%(asctime)s %(levelname)s %(message)s",
            handlers=[logging.StreamHandler(sys.stdout), handler],
        )
        fields = [
            "tick",
            "ts",
            "elapsed_s",
            "drift_ms",
            "window_hidden",
            "window_state",
            "host_pids",
            "host_rss_kib",
            "host_pss_kib",
            "network_pids",
            "network_rss_kib",
            "network_pss_kib",
            "web_pids",
            "web_rss_kib",
            "web_pss_kib",
            "extra_pids",
            "gui_proc_count",
            "gui_rss_kib",
            "gui_pss_kib",
            "pss_complete",
            "gui_alive",
            "server_pid",
            "server_rss_kib",
            "server_pss_kib",
            "mem_total_kib",
            "mem_available_kib",
            "swap_total_kib",
            "swap_used_kib",
            "vram_used_mib",
            "vram_free_mib",
            "watcher_ticks",
            "watcher_errors",
            "watcher_latency_ms",
            "tray_state",
            "tray_headline",
            "tray_detail",
            "note",
        ]
        self.sampler.start()
        index = 0
        try:
            with (self.out / "endurance.csv").open(
                "w", encoding="utf-8", newline=""
            ) as handle:
                writer = csv.DictWriter(handle, fieldnames=fields)
                writer.writeheader()
                handle.flush()
                while not STOP.is_set() and index < int(
                    self.args.duration / self.args.interval
                ):
                    deadline = self.started + index * self.args.interval
                    while not STOP.is_set():
                        remaining = deadline - time.monotonic()
                        if remaining <= 0:
                            break
                        time.sleep(min(remaining, 0.2))
                    if STOP.is_set():
                        break
                    self.tick(index, writer, handle)
                    index += 1
        finally:
            self.sampler.stop()
        summary = self.summary()
        (self.out / "summary.json").write_text(
            json.dumps(summary, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )
        logging.info(
            "fin: samples=%s drift_p95=%sms host_delta=%sKio gui_delta=%sKio "
            "acceptance=%s",
            summary["samples"],
            summary["drift_ms"]["p95"],
            summary["deltas_kib"]["host"],
            summary["deltas_kib"]["gui"],
            summary["acceptance"]["acceptance_ok"],
        )
        return 0 if summary["acceptance"]["acceptance_ok"] else 10


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True)
    parser.add_argument("--gui-pid", type=int, required=True)
    parser.add_argument("--window-wid", required=True)
    parser.add_argument("--server-url", default="http://127.0.0.1:5005")
    parser.add_argument("--server-pid", type=int)
    parser.add_argument("--tray-state-file", required=True)
    parser.add_argument("--watcher-stats-file", required=True)
    parser.add_argument("--duration", type=float, default=7200)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--abort-vram", type=int, default=3900)
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    def stop(_signum: int, _frame: Any) -> None:
        STOP.set()

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGHUP, stop)
    return Monitor(args).run()


if __name__ == "__main__":
    sys.exit(main())
