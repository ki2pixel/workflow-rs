#!/usr/bin/env python3
"""Moniteur J3 : RSS/PSS des processus GUI/WebKit et STEP2, VRAM globale.

Deux rythmes :
- 1 Hz pour les processus et l'écriture de `metrics.csv` ;
- 200 ms pour `nvidia-smi`, le pic VRAM et l'alerte 3 900 MiB.

Le script ne modifie aucun état du pipeline. Il lit `/proc` via psutil et
`nvidia-smi`, et écrit uniquement dans `--out`.
"""

from __future__ import annotations

import argparse
import csv
import json
import os
import re
import signal
import statistics
import subprocess
import sys
import threading
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import psutil

STOP = threading.Event()
GUI_CATEGORIES = ("gui", "network", "web")
STEP2_CATEGORIES = ("workflow-cli", "ffmpeg")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def log(message: str) -> None:
    print(message, flush=True)


def run_cmd(cmd: list[str], timeout: float = 5) -> tuple[int, str, str]:
    try:
        proc = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout, check=False
        )
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except Exception as error:  # pragma: no cover - dépend de l'environnement
        return 1, "", str(error)


def query_vram() -> tuple[int | None, int | None, int | None, str]:
    rc, out, err = run_cmd(
        [
            "nvidia-smi",
            "--query-gpu=memory.used,memory.free,memory.total",
            "--format=csv,noheader,nounits",
            "--id=0",
        ]
    )
    if rc != 0 or not out:
        return None, None, None, err
    try:
        used, free, total = (int(value.strip()) for value in out.splitlines()[0].split(","))
    except (ValueError, IndexError) as error:
        return None, None, None, f"parse: {error}: {out!r}"
    return used, free, total, ""


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
    for proc in psutil.process_iter(["pid", "name", "cmdline", "create_time"]):
        try:
            if os.getsid(proc.info["pid"]) != sid:
                continue
            members.append((proc, proc_names(proc)))
        except (psutil.Error, ProcessLookupError, PermissionError, OSError):
            continue
    return members


def classify_gui(
    members: list[tuple[psutil.Process, set[str]]]
) -> list[tuple[psutil.Process, set[str], str]]:
    result: list[tuple[psutil.Process, set[str], str]] = []
    for proc, names in members:
        category = ""
        if any(name.startswith("workflow-gui") for name in names):
            category = "gui"
        elif any(name.startswith("WebKitNetwork") for name in names):
            category = "network"
        elif any(name.startswith("WebKitWeb") for name in names):
            category = "web"
        elif any(name.startswith("WebKit") for name in names):
            category = "extra"
        if category:
            result.append((proc, names, category))
    return result


def classify_step2(
    members: list[tuple[psutil.Process, set[str]]]
) -> list[tuple[psutil.Process, set[str], str]]:
    result: list[tuple[psutil.Process, set[str], str]] = []
    for proc, names in members:
        category = ""
        if any(name.startswith("workflow-cli") for name in names):
            category = "workflow-cli"
        elif any(name.startswith("ffmpeg") for name in names):
            category = "ffmpeg"
        if category:
            result.append((proc, names, category))
    return result


def read_memory(
    entries: list[tuple[psutil.Process, set[str], str]]
) -> tuple[int, int | None, list[dict[str, Any]], bool]:
    rss = 0
    pss = 0
    pss_complete = True
    details: list[dict[str, Any]] = []
    for proc, names, category in entries:
        try:
            rss_value = proc.memory_info().rss // 1024
            rss += rss_value
            try:
                pss_value = proc.memory_full_info().pss // 1024
                pss += pss_value
            except psutil.Error:
                pss_complete = False
                pss_value = None
            details.append(
                {
                    "pid": proc.pid,
                    "names": sorted(names),
                    "category": category,
                    "rss_kib": rss_value,
                    "pss_kib": pss_value,
                    "missing": False,
                }
            )
        except psutil.Error:
            pss_complete = False
            details.append(
                {
                    "pid": proc.pid,
                    "names": sorted(names),
                    "category": category,
                    "rss_kib": 0,
                    "pss_kib": None,
                    "missing": True,
                }
            )
    return rss, (pss if pss_complete else None), details, pss_complete


def detail_string(details: list[dict[str, Any]]) -> str:
    return "|".join(
        f"{item['pid']}:{item['category']}:{','.join(item['names'])}"
        for item in details
    )


def pid_string(details: list[dict[str, Any]]) -> str:
    return "|".join(str(item["pid"]) for item in details)


def stats(values: list[int | float | None]) -> dict[str, float | int]:
    clean = [value for value in values if value is not None]
    if not clean:
        return {}
    return {
        "n": len(clean),
        "min": min(clean),
        "median": statistics.median(clean),
        "max": max(clean),
    }


def category_flags(details: list[dict[str, Any]]) -> dict[str, bool]:
    flags = {category: False for category in GUI_CATEGORIES}
    for item in details:
        category = item.get("category")
        if category in flags and not item.get("missing"):
            flags[category] = True
    return flags


def pid_alive(pid: int | None) -> bool:
    if not pid:
        return False
    try:
        proc = psutil.Process(pid)
        return proc.is_running() and proc.status() != psutil.STATUS_ZOMBIE
    except psutil.Error:
        return False


def kill_group(sid: int | None, sig: int) -> None:
    if not sid:
        return
    try:
        os.killpg(sid, sig)
    except (ProcessLookupError, PermissionError):
        pass


class VramSampler(threading.Thread):
    def __init__(self, path: Path, threshold: int, alerts_path: Path) -> None:
        super().__init__(daemon=True)
        self.path = path
        self.threshold = threshold
        self.alerts_path = alerts_path
        self.lock = threading.Lock()
        self.latest: tuple[int, int, int] | None = None
        self.values: list[tuple[int, int, int]] = []
        self.peak = 0
        self.peak_ts = ""
        self.free_min: int | None = None
        self.total: int | None = None
        self.over_count = 0
        self.first_over_ts = ""
        self.abort = threading.Event()
        self.alerts: list[dict[str, Any]] = []
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
                        self.values.append((used, free, total))
                        if used > self.peak:
                            self.peak = used
                            self.peak_ts = ts
                        if self.free_min is None or free < self.free_min:
                            self.free_min = free
                        self.total = total
                        if used >= self.threshold:
                            self.over_count += 1
                            if not self.first_over_ts:
                                self.first_over_ts = ts
                            self.abort.set()
                            if (
                                not self.alerts
                                or time.monotonic() - self.alerts[-1]["monotonic"] >= 1.0
                            ):
                                alert = {
                                    "ts": ts,
                                    "used_mib": used,
                                    "free_mib": free,
                                    "total_mib": total,
                                    "threshold_mib": self.threshold,
                                    "monotonic": time.monotonic(),
                                }
                                self.alerts.append(alert)
                                with self.alerts_path.open("a", encoding="utf-8") as alert_file:
                                    alert_file.write(
                                        f"{ts},used={used},free={free},"
                                        f"total={total},threshold={self.threshold}\n"
                                    )
        except Exception as error:  # pragma: no cover - dépend de l'environnement
            self.error = str(error)
            with (self.path.parent / "vram-sampler.err").open(
                "a", encoding="utf-8"
            ) as handle:
                handle.write(f"{utc_now()} {error}\n")

    def stop(self) -> None:
        if self.proc is not None and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                try:
                    self.proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    pass


class Monitor:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.out = Path(args.out)
        self.out.mkdir(parents=True, exist_ok=True)
        self.metrics_path = self.out / "metrics.csv"
        self.pmon_path = self.out / "pmon.log"
        self.fuser_path = self.out / "fuser.log"
        self.compute_path = self.out / "compute-apps.log"
        self.alerts_path = self.out / "alerts.log"
        self.rows: list[dict[str, Any]] = []
        self.started = time.monotonic()
        self.sampler = VramSampler(
            self.out / "vram-200ms.csv", args.abort_vram, self.alerts_path
        )
        self.aborted_vram = False
        self.killed_step2 = False

    def collect(self) -> tuple[
        list[tuple[psutil.Process, set[str], str]],
        list[tuple[psutil.Process, set[str], str]],
        list[tuple[psutil.Process, set[str], str]],
    ]:
        role = self.args.role
        if role == "firefox":
            if not pid_alive(self.args.root_pid):
                return [], [], []
            try:
                sid = os.getsid(self.args.root_pid)
            except OSError:
                return [], [], []
            members = session_members(sid)
            if self.args.profile_path:
                marker = self.args.profile_path
                for proc in psutil.process_iter(["pid", "cmdline"]):
                    try:
                        cmdline = " ".join(proc.info["cmdline"] or [])
                        if marker in cmdline:
                            if not any(item[0].pid == proc.info["pid"] for item in members):
                                members.append((proc, proc_names(proc)))
                    except (psutil.Error, OSError):
                        pass
            entries = [(proc, names, "firefox") for proc, names in members]
            return entries, [], []
        if role == "gui":
            if not pid_alive(self.args.root_pid):
                return [], [], []
            try:
                sid = os.getsid(self.args.root_pid)
            except OSError:
                return [], [], []
            gui = classify_gui(session_members(sid))
            return gui, gui, []
        if role == "step2":
            gui: list[tuple[psutil.Process, set[str], str]] = []
            step2: list[tuple[psutil.Process, set[str], str]] = []
            if self.args.gui_pid and pid_alive(self.args.gui_pid):
                try:
                    gui_sid = os.getsid(self.args.gui_pid)
                    gui = classify_gui(session_members(gui_sid))
                except OSError:
                    pass
            if self.args.step2_pid and pid_alive(self.args.step2_pid):
                try:
                    step2_sid = os.getsid(self.args.step2_pid)
                    step2 = classify_step2(session_members(step2_sid))
                except OSError:
                    pass
            return gui + step2, gui, step2
        return [], [], []

    def tick(self, index: int) -> dict[str, Any]:
        target_entries, gui_entries, step2_entries = self.collect()
        target_rss, target_pss, target_detail, _ = read_memory(target_entries)
        gui_rss, gui_pss, gui_detail, _ = read_memory(gui_entries)
        step2_rss, step2_pss, step2_detail, _ = read_memory(step2_entries)

        compute_entries = [
            item for item in step2_detail if item.get("category") == "workflow-cli"
        ]
        compute_rss = sum(int(item["rss_kib"]) for item in compute_entries)
        compute_pss_values = [
            int(item["pss_kib"]) for item in compute_entries if item.get("pss_kib") is not None
        ]
        compute_pss = sum(compute_pss_values) if compute_pss_values else None

        flags = category_flags(gui_detail)
        contract_ok = (
            all(flags.values())
            if self.args.role in ("gui", "step2")
            else None
        )
        extra_pids = [
            item["pid"] for item in gui_detail if item.get("category") == "extra"
        ]

        with self.sampler.lock:
            latest = self.sampler.latest
        if latest is None:
            used, free, total, vram_error = query_vram()
        else:
            used, free, total, vram_error = *latest, ""
        delta = ""
        if used is not None and self.args.baseline_vram is not None:
            delta = used - self.args.baseline_vram

        combined_rss = gui_rss + step2_rss if self.args.role == "step2" else target_rss
        combined_pss: int | None
        if self.args.role == "step2":
            if gui_pss is None or step2_pss is None:
                combined_pss = None
            else:
                combined_pss = gui_pss + step2_pss
        else:
            combined_pss = target_pss

        ratio_rss = ""
        ratio_pss = ""
        if self.args.role == "step2" and step2_rss > 0:
            ratio_rss = round(gui_rss / step2_rss, 4)
        if (
            self.args.role == "step2"
            and gui_pss is not None
            and step2_pss is not None
            and step2_pss > 0
        ):
            ratio_pss = round(gui_pss / step2_pss, 4)

        pmon = ""
        fuser = ""
        if index % 5 == 0:
            rc, out, err = run_cmd(["nvidia-smi", "pmon", "-c", "1"], timeout=5)
            pmon = " | ".join(line.strip() for line in out.splitlines() if line.strip())
            with self.pmon_path.open("a", encoding="utf-8") as handle:
                handle.write(f"# {utc_now()} rc={rc}\n{out}\n{err}\n")
            rc, out, err = run_cmd(["fuser", "-v", "/dev/nvidia0"], timeout=5)
            fuser = " | ".join(line.strip() for line in out.splitlines() if line.strip())
            with self.fuser_path.open("a", encoding="utf-8") as handle:
                handle.write(f"# {utc_now()} rc={rc}\n{out}\n{err}\n")
            rc, out, err = run_cmd(
                [
                    "nvidia-smi",
                    "--query-compute-apps=pid,process_name,used_memory",
                    "--format=csv,noheader",
                ],
                timeout=5,
            )
            with self.compute_path.open("a", encoding="utf-8") as handle:
                handle.write(f"# {utc_now()} rc={rc}\n{out}\n{err}\n")

        row: dict[str, Any] = {
            "ts": utc_now(),
            "elapsed_s": round(time.monotonic() - self.started, 3),
            "phase": self.args.phase,
            "run": self.args.run,
            "role": self.args.role,
            "target_pids": pid_string(target_detail),
            "target_names": detail_string(target_detail),
            "target_rss_kib": target_rss,
            "target_pss_kib": target_pss if target_pss is not None else "",
            "target_proc_count": len(target_detail),
            "gui_pids": pid_string(gui_detail),
            "gui_names": detail_string(gui_detail),
            "gui_rss_kib": gui_rss,
            "gui_pss_kib": gui_pss if gui_pss is not None else "",
            "gui_proc_count": len(gui_detail),
            "gui_contract_ok": contract_ok,
            "gui_extra_pids": "|".join(str(pid) for pid in extra_pids),
            "step2_pids": pid_string(step2_detail),
            "step2_names": detail_string(step2_detail),
            "step2_rss_kib": step2_rss,
            "step2_pss_kib": step2_pss if step2_pss is not None else "",
            "step2_proc_count": len(step2_detail),
            "step2_compute_rss_kib": compute_rss,
            "step2_compute_pss_kib": compute_pss if compute_pss is not None else "",
            "combined_rss_kib": combined_rss,
            "combined_pss_kib": combined_pss if combined_pss is not None else "",
            "gui_over_step2_rss": ratio_rss,
            "gui_over_step2_pss": ratio_pss,
            "vram_used_mib": used if used is not None else "",
            "vram_free_mib": free if free is not None else "",
            "vram_total_mib": total if total is not None else "",
            "vram_delta_mib": delta,
            "pmon": pmon,
            "fuser": fuser,
            "note": vram_error,
        }
        return row

    def run(self) -> int:
        log(f"moniteur {self.args.role} phase={self.args.phase} run={self.args.run}")
        self.sampler.start()
        fields = [
            "ts",
            "elapsed_s",
            "phase",
            "run",
            "role",
            "target_pids",
            "target_names",
            "target_rss_kib",
            "target_pss_kib",
            "target_proc_count",
            "gui_pids",
            "gui_names",
            "gui_rss_kib",
            "gui_pss_kib",
            "gui_proc_count",
            "gui_contract_ok",
            "gui_extra_pids",
            "step2_pids",
            "step2_names",
            "step2_rss_kib",
            "step2_pss_kib",
            "step2_proc_count",
            "step2_compute_rss_kib",
            "step2_compute_pss_kib",
            "combined_rss_kib",
            "combined_pss_kib",
            "gui_over_step2_rss",
            "gui_over_step2_pss",
            "vram_used_mib",
            "vram_free_mib",
            "vram_total_mib",
            "vram_delta_mib",
            "pmon",
            "fuser",
            "note",
        ]
        gone_ticks = 0
        index = 0
        with self.metrics_path.open("w", encoding="utf-8", newline="") as handle:
            writer = csv.DictWriter(handle, fieldnames=fields)
            writer.writeheader()
            handle.flush()
            while not STOP.is_set():
                elapsed = time.monotonic() - self.started
                if self.args.duration > 0 and elapsed >= self.args.duration:
                    break
                if self.args.role == "step2" and self.args.step2_pid:
                    if not pid_alive(self.args.step2_pid):
                        gone_ticks += 1
                        if gone_ticks >= self.args.settle:
                            break
                    else:
                        gone_ticks = 0
                row = self.tick(index)
                writer.writerow(row)
                handle.flush()
                self.rows.append(row)
                if (
                    self.args.role == "step2"
                    and self.sampler.abort.is_set()
                    and not self.aborted_vram
                ):
                    self.aborted_vram = True
                    self.killed_step2 = True
                    kill_group(self.args.step2_pid, signal.SIGTERM)
                    with self.alerts_path.open("a", encoding="utf-8") as alert_file:
                        alert_file.write(
                            f"{utc_now()},action=SIGTERM_STEP2,"
                            f"used={row['vram_used_mib']},threshold={self.args.abort_vram}\n"
                        )
                if self.args.role == "firefox":
                    log(
                        f"[{row['elapsed_s']:7.2f}s] "
                        f"firefox={row['target_rss_kib'] / 1024:7.1f}/"
                        f"{row['target_pss_kib'] if row['target_pss_kib'] != '' else 0:7.1f} MiB "
                        f"({row['target_proc_count']}) "
                        f"vram={row['vram_used_mib']}/{row['vram_total_mib']} MiB"
                    )
                else:
                    log(
                        f"[{row['elapsed_s']:7.2f}s] "
                        f"gui={row['gui_rss_kib'] / 1024:7.1f}/"
                        f"{row['gui_pss_kib'] if row['gui_pss_kib'] != '' else 0:7.1f} MiB "
                        f"({row['gui_proc_count']}) "
                        f"step2={row['step2_rss_kib'] / 1024:7.1f}/"
                        f"{row['step2_pss_kib'] if row['step2_pss_kib'] != '' else 0:7.1f} MiB "
                        f"({row['step2_proc_count']}) "
                        f"vram={row['vram_used_mib']}/{row['vram_total_mib']} MiB"
                    )
                index += 1
                time.sleep(self.args.interval)
        self.sampler.stop()
        return self.summarize()

    def summarize(self) -> int:
        with self.sampler.lock:
            vram_values = list(self.sampler.values)
            peak = self.sampler.peak
            peak_ts = self.sampler.peak_ts
            free_min = self.sampler.free_min
            total = self.sampler.total
            over_count = self.sampler.over_count
            first_over_ts = self.sampler.first_over_ts
            alerts = list(self.sampler.alerts)
            sampler_error = self.sampler.error

        used_values = [value[0] for value in vram_values]
        free_values = [value[1] for value in vram_values]
        total_values = [value[2] for value in vram_values]

        target_rss = [row["target_rss_kib"] for row in self.rows]
        target_pss = [row["target_pss_kib"] for row in self.rows if row["target_pss_kib"] != ""]
        gui_rss = [row["gui_rss_kib"] for row in self.rows]
        gui_pss = [row["gui_pss_kib"] for row in self.rows if row["gui_pss_kib"] != ""]
        step2_rss = [row["step2_rss_kib"] for row in self.rows]
        step2_pss = [row["step2_pss_kib"] for row in self.rows if row["step2_pss_kib"] != ""]
        combined_rss = [row["combined_rss_kib"] for row in self.rows]
        combined_pss = [row["combined_pss_kib"] for row in self.rows if row["combined_pss_kib"] != ""]

        contract_values = [
            row["gui_contract_ok"] for row in self.rows if row["gui_contract_ok"] is not None
        ]
        contract_ok_all = all(contract_values) if contract_values else None
        contract_fail_samples = sum(1 for value in contract_values if not value)
        extra_pids = sorted(
            {
                int(pid)
                for row in self.rows
                for pid in str(row["gui_extra_pids"]).split("|")
                if pid
            }
        )

        ratios_rss = [
            float(row["gui_over_step2_rss"])
            for row in self.rows
            if row["gui_over_step2_rss"] != ""
        ]
        ratios_pss = [
            float(row["gui_over_step2_pss"])
            for row in self.rows
            if row["gui_over_step2_pss"] != ""
        ]

        vram_peak = peak if peak else (max(used_values) if used_values else 0)
        vram_free_min = free_min if free_min is not None else (min(free_values) if free_values else None)
        vram_total = total if total is not None else (max(total_values) if total_values else None)
        vram_exceeded = bool(
            over_count > 0 or (vram_peak and vram_peak >= self.args.abort_vram)
        )

        summary = {
            "role": self.args.role,
            "phase": self.args.phase,
            "run": self.args.run,
            "started_at": datetime.fromtimestamp(
                time.time() - (time.monotonic() - self.started), timezone.utc
            ).isoformat(timespec="milliseconds"),
            "finished_at": utc_now(),
            "duration_s": round(time.monotonic() - self.started, 3),
            "samples": len(self.rows),
            "target": {
                "rss_kib": stats(target_rss),
                "pss_kib": stats(target_pss),
                "processes": sorted(
                    {
                        pid
                        for row in self.rows
                        for pid in str(row["target_pids"]).split("|")
                        if pid
                    }
                ),
            },
            "gui": {
                "contract_ok": contract_ok_all,
                "contract_fail_samples": contract_fail_samples,
                "extra_pids": extra_pids,
                "rss_kib": stats(gui_rss),
                "pss_kib": stats(gui_pss),
                "processes": sorted(
                    {
                        pid
                        for row in self.rows
                        for pid in str(row["gui_pids"]).split("|")
                        if pid
                    }
                ),
            }
            if self.args.role in ("gui", "step2")
            else None,
            "step2": {
                "rss_kib": stats(step2_rss),
                "pss_kib": stats(step2_pss),
                "compute_rss_kib": stats(
                    [row["step2_compute_rss_kib"] for row in self.rows]
                ),
                "compute_pss_kib": stats(
                    [
                        row["step2_compute_pss_kib"]
                        for row in self.rows
                        if row["step2_compute_pss_kib"] != ""
                    ]
                ),
                "processes": sorted(
                    {
                        pid
                        for row in self.rows
                        for pid in str(row["step2_pids"]).split("|")
                        if pid
                    }
                ),
            }
            if self.args.role == "step2"
            else None,
            "combined": {
                "rss_kib": stats(combined_rss),
                "pss_kib": stats(combined_pss),
            },
            "ratios": {
                "gui_over_step2_rss_median": statistics.median(ratios_rss)
                if ratios_rss
                else None,
                "gui_over_step2_pss_median": statistics.median(ratios_pss)
                if ratios_pss
                else None,
            },
            "vram": {
                "used_mib": stats(used_values),
                "free_mib": stats(free_values),
                "total_mib": vram_total,
                "peak_mib": vram_peak,
                "peak_ts": peak_ts,
                "free_min_mib": vram_free_min,
                "over_3900_samples": over_count,
                "first_over_ts": first_over_ts,
                "sampler_error": sampler_error,
            },
            "vram_exceeded": vram_exceeded,
            "aborted_vram": self.aborted_vram,
            "killed_step2": self.killed_step2,
            "alerts": alerts,
            "oom": False,
        }
        (self.out / "summary.json").write_text(
            json.dumps(summary, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
        )
        log(
            f"résumé {self.args.role}/{self.args.phase}/{self.args.run}: "
            f"échantillons={len(self.rows)} pic_vram={vram_peak} MiB "
            f"libre_min={vram_free_min} MiB dépassements={over_count} "
            f"contrat_gui={contract_ok_all}"
        )
        if vram_exceeded:
            return 10
        if contract_ok_all is False:
            return 11
        return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--role", choices=["gui", "step2", "firefox"], required=True)
    parser.add_argument("--root-pid", type=int)
    parser.add_argument("--gui-pid", type=int)
    parser.add_argument("--step2-pid", type=int)
    parser.add_argument("--profile-path")
    parser.add_argument("--phase", required=True)
    parser.add_argument("--run", type=int, default=0)
    parser.add_argument("--duration", type=float, default=60)
    parser.add_argument("--interval", type=float, default=1.0)
    parser.add_argument("--settle", type=float, default=2.0)
    parser.add_argument("--abort-vram", type=int, default=3900)
    parser.add_argument("--baseline-vram", type=int)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    if args.role in ("gui", "firefox") and not args.root_pid:
        parser.error("--root-pid est requis pour gui/firefox")
    if args.role == "step2" and not args.step2_pid:
        parser.error("--step2-pid est requis pour step2")
    return args


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
