#!/usr/bin/env python3
"""Stress-test J2 du PoC Tauri A-Strict.

Protocole ratifié :
- 20 lancements `default` avec 5 bascules chacun ;
- 5 lancements `disable-dmabuf` avec 5 bascules chacun ;
- 5 lancements `force-dmabuf` sans balayage (canari) ;
- 5 lancements `force-dmabuf-shm` sans balayage ;
soit 35 lancements, 25 balayages et 125 bascules.

Le script sauvegarde et restaure `dynamic-workspaces` et `num-workspaces`,
isole chaque GUI dans sa session, capture les stderr et vérifie l'absence de
processus WebKit orphelins après chaque lancement.
"""

from __future__ import annotations

import argparse
import atexit
import csv
import hashlib
import json
import logging
import math
import os
import re
import resource
import secrets
import shutil
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import psutil

REPO = Path(__file__).resolve().parents[2]
DEFAULT_GUI = REPO / "target/release/workflow-gui-tauri"
DEFAULT_SERVER = REPO / "target/release/workflow-server"
DEFAULT_CONFIG = Path("/tmp/opencode/poc-gui/config.toml")
DEFAULT_AUDIT = REPO / "scripts/poc/audit.js"
TITLE = "Workflow Manager"
SERVER_URL = "http://127.0.0.1:5005"
DISPLAY = os.environ.get("DISPLAY", ":0")

SETTINGS = {
    "dynamic-workspaces": ("org.gnome.mutter", "dynamic-workspaces"),
    "num-workspaces": ("org.gnome.desktop.wm.preferences", "num-workspaces"),
}

LOGGER = logging.getLogger("j2")


@dataclass(frozen=True)
class Profile:
    name: str
    env: dict[str, str]
    runs: int
    sweep: bool


PROFILES = [
    Profile("default", {}, 20, True),
    Profile("disable-dmabuf", {"WEBKIT_DISABLE_DMABUF_RENDERER": "1"}, 5, True),
    Profile("force-dmabuf", {"WEBKIT_FORCE_DMABUF_RENDERER": "1"}, 5, False),
    Profile(
        "force-dmabuf-shm",
        {
            "WEBKIT_FORCE_DMABUF_RENDERER": "1",
            "WEBKIT_DMABUF_RENDERER_FORCE_SHM": "1",
        },
        5,
        False,
    ),
]

SMOKE_PROFILES = [
    Profile("default", {}, 1, True),
    Profile("disable-dmabuf", {"WEBKIT_DISABLE_DMABUF_RENDERER": "1"}, 1, True),
    Profile("force-dmabuf", {"WEBKIT_FORCE_DMABUF_RENDERER": "1"}, 1, False),
    Profile(
        "force-dmabuf-shm",
        {
            "WEBKIT_FORCE_DMABUF_RENDERER": "1",
            "WEBKIT_DMABUF_RENDERER_FORCE_SHM": "1",
        },
        1,
        False,
    ),
]


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def run(
    cmd: list[str],
    *,
    timeout: float = 10,
    env: dict[str, str] | None = None,
    cwd: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
            env=env,
            cwd=str(cwd) if cwd else None,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout or ""
        stderr = exc.stderr or ""
        if isinstance(stdout, bytes):
            stdout = stdout.decode("utf-8", "replace")
        if isinstance(stderr, bytes):
            stderr = stderr.decode("utf-8", "replace")
        return subprocess.CompletedProcess(cmd, 124, stdout, stderr + "\nTIMEOUT")


def run_ok(cmd: list[str], **kwargs: Any) -> bool:
    return run(cmd, **kwargs).returncode == 0


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    tmp.replace(path)


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def gsettings_get(schema: str, key: str) -> str:
    proc = run(["gsettings", "get", schema, key], timeout=5)
    if proc.returncode != 0:
        raise RuntimeError(f"gsettings get {schema} {key}: {proc.stderr.strip()}")
    return proc.stdout.strip()


def gsettings_set(schema: str, key: str, value: str) -> None:
    proc = run(["gsettings", "set", schema, key, value], timeout=5)
    if proc.returncode != 0:
        raise RuntimeError(f"gsettings set {schema} {key} {value}: {proc.stderr.strip()}")


def gsettings_writable(schema: str, key: str) -> bool:
    proc = run(["gsettings", "writable", schema, key], timeout=5)
    return proc.returncode == 0 and proc.stdout.strip() == "true"


def current_desktop() -> int | None:
    proc = run(["wmctrl", "-d"], timeout=5)
    if proc.returncode != 0:
        return None
    for line in proc.stdout.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "*":
            try:
                return int(parts[0])
            except ValueError:
                return None
    return None


def desktop_count() -> int:
    proc = run(["wmctrl", "-d"], timeout=5)
    if proc.returncode != 0:
        return 0
    return sum(1 for line in proc.stdout.splitlines() if line.split() and line.split()[0].isdigit())


def set_desktop(index: int) -> tuple[bool, str]:
    proc = run(["wmctrl", "-s", str(index)], timeout=5)
    method = "wmctrl"
    time.sleep(0.25)
    if proc.returncode != 0 or current_desktop() != index:
        proc = run(["xdotool", "set_desktop", str(index)], timeout=5)
        method = "xdotool"
        time.sleep(0.25)
    return current_desktop() == index, method


def find_window() -> str | None:
    proc = run(["wmctrl", "-l"], timeout=5)
    if proc.returncode != 0:
        return None
    for line in proc.stdout.splitlines():
        parts = line.split(None, 3)
        if len(parts) >= 4 and TITLE in parts[3]:
            return parts[0]
    return None


def window_geometry(wid: str) -> dict[str, Any]:
    proc = run(["xwininfo", "-id", wid], timeout=5)
    result: dict[str, Any] = {"ok": proc.returncode == 0}
    if proc.returncode == 0:
        for key, label in [
            ("width", "Width:"),
            ("height", "Height:"),
            ("x", "Absolute upper-left X:"),
            ("y", "Absolute upper-left Y:"),
        ]:
            for line in proc.stdout.splitlines():
                if label in line:
                    try:
                        result[key] = int(line.split(label, 1)[1].strip())
                    except ValueError:
                        pass
    else:
        result["error"] = proc.stderr.strip()
    return result


def set_sticky(wid: str) -> bool:
    activate(wid)
    time.sleep(0.3)
    for _ in range(3):
        run(["wmctrl", "-i", "-r", wid, "-b", "add,sticky"], timeout=5)
        time.sleep(0.4)
        proc = run(["xprop", "-id", wid, "_NET_WM_STATE"], timeout=5)
        if proc.returncode == 0 and "_NET_WM_STATE_STICKY" in proc.stdout:
            return True
    return False


def activate(wid: str) -> None:
    run(["wmctrl", "-i", "-a", wid], timeout=5)


def capture(wid: str, path: Path) -> dict[str, Any]:
    path.parent.mkdir(parents=True, exist_ok=True)
    proc = run(["import", "-window", wid, str(path)], timeout=10)
    if proc.returncode != 0 or not path.exists():
        return {"ok": False, "path": str(path), "error": proc.stderr.strip()}
    proc = run(
        [
            "identify",
            "-format",
            "%w %h %[fx:mean] %[fx:standard_deviation] %k",
            str(path),
        ],
        timeout=10,
    )
    if proc.returncode != 0:
        return {"ok": False, "path": str(path), "error": proc.stderr.strip()}
    parts = proc.stdout.strip().split()
    try:
        width, height = int(parts[0]), int(parts[1])
        mean, stddev = float(parts[2]), float(parts[3])
        colors = int(parts[4])
    except (IndexError, ValueError) as error:
        return {"ok": False, "path": str(path), "error": f"identify: {error}: {proc.stdout!r}"}
    stddev_value: float | None = None if math.isnan(stddev) else stddev
    blank = ""
    if mean > 0.95 and stddev_value is not None and stddev_value < 0.02:
        blank = "white"
    elif mean < 0.05 and stddev_value is not None and stddev_value < 0.02:
        blank = "black"
    elif colors <= 1 or stddev_value is None or stddev_value < 0.005:
        blank = "solid"
    return {
        "ok": True,
        "path": str(path),
        "width": width,
        "height": height,
        "mean": mean,
        "stddev": stddev_value,
        "colors": colors,
        "blank": blank,
    }


def session_processes(sid: int) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    for proc in psutil.process_iter(["pid", "name", "create_time"]):
        try:
            info = proc.info
            if os.getsid(info["pid"]) == sid:
                result.append(info)
        except (psutil.Error, ProcessLookupError, PermissionError, OSError):
            pass
    return result


def memory_for_session(sid: int) -> tuple[int, int | None]:
    rss = 0
    pss = 0
    pss_ok = True
    for info in session_processes(sid):
        try:
            proc = psutil.Process(info["pid"])
            rss += proc.memory_info().rss // 1024
            try:
                pss += proc.memory_full_info().pss // 1024
            except psutil.Error:
                pss_ok = False
        except psutil.Error:
            pass
    return rss, (pss if pss_ok else None)


def kill_session(sid: int, sig: int) -> None:
    try:
        os.killpg(sid, sig)
    except ProcessLookupError:
        pass
    except PermissionError:
        pass


def http_get(url: str, headers: dict[str, str] | None = None, timeout: float = 2) -> tuple[int, str]:
    request = urllib.request.Request(url, headers=headers or {})
    with urllib.request.urlopen(request, timeout=timeout) as response:
        return response.status, response.read().decode("utf-8", "replace")


class Harness:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.out_dir = Path(args.out)
        self.out_dir.mkdir(parents=True, exist_ok=True)
        self.server_proc: subprocess.Popen[str] | None = None
        self.server_sid: int | None = None
        self.server_token = ""
        self.server_token_sha = ""
        self.gui_proc: subprocess.Popen[str] | None = None
        self.gui_sid: int | None = None
        self.gui_wid: str | None = None
        self.records: list[dict[str, Any]] = []
        self.settings_before: dict[str, Any] = {}
        self.settings_after: dict[str, Any] = {}
        self.settings_restored = False
        self.cleaned = False
        self.started_at = utc_now()
        self.started_monotonic = time.monotonic()
        self.errors: list[str] = []

    def log(self, message: str, *args: Any) -> None:
        LOGGER.info(message, *args)

    def gui_processes(self) -> list[dict[str, Any]]:
        result: list[dict[str, Any]] = []
        for proc in psutil.process_iter(["pid", "name", "cmdline"]):
            try:
                info = proc.info
                name = info.get("name") or ""
                cmdline = info.get("cmdline") or []
                is_gui = name.startswith("workflow-gui")
                if not is_gui and cmdline:
                    is_gui = Path(cmdline[0]).name == "workflow-gui-tauri"
                if is_gui:
                    result.append(
                        {
                            "pid": info["pid"],
                            "name": name,
                            "cmd": " ".join(cmdline),
                        }
                    )
            except (psutil.Error, OSError):
                pass
        return result

    def ensure_clean(self) -> None:
        if find_window() is not None:
            raise RuntimeError("une fenêtre 'Workflow Manager' existe déjà")
        leftovers = self.gui_processes()
        if leftovers:
            raise RuntimeError(f"processus GUI résiduel: {leftovers}")

    def check_ports(self) -> None:
        proc = run(["ss", "-ltnp"], timeout=5)
        for line in proc.stdout.splitlines():
            if any(port in line for port in (":5003", ":5004", ":5005")):
                raise RuntimeError(f"port occupé avant J2: {line.strip()}")

    def save_settings(self) -> None:
        for name, (schema, key) in SETTINGS.items():
            if not gsettings_writable(schema, key):
                raise RuntimeError(f"gsettings non inscriptible: {schema} {key}")
            self.settings_before[name] = gsettings_get(schema, key)
        self.settings_before["current-desktop"] = current_desktop()
        write_json(self.out_dir / "gnome-settings-before.json", self.settings_before)

    def apply_settings(self) -> None:
        gsettings_set(*SETTINGS["dynamic-workspaces"], "false")
        gsettings_set(*SETTINGS["num-workspaces"], "5")
        time.sleep(0.5)
        if gsettings_get(*SETTINGS["dynamic-workspaces"]) != "false":
            raise RuntimeError("dynamic-workspaces n'a pas pris la valeur false")
        if gsettings_get(*SETTINGS["num-workspaces"]) != "5":
            raise RuntimeError("num-workspaces n'a pas pris la valeur 5")
        if desktop_count() < 5:
            time.sleep(1.0)
        if desktop_count() < 5:
            raise RuntimeError(f"wmctrl ne voit que {desktop_count()} bureaux")
        if current_desktop() != 0:
            ok, method = set_desktop(0)
            if not ok:
                raise RuntimeError(f"impossible de revenir au bureau 0 ({method})")

    def restore_settings(self) -> None:
        errors: list[str] = []
        for name, (schema, key) in SETTINGS.items():
            original = self.settings_before.get(name)
            if original is None:
                continue
            try:
                gsettings_set(schema, key, str(original))
            except Exception as error:  # noqa: BLE001 - on consigne tout
                errors.append(str(error))
        time.sleep(0.5)
        after: dict[str, Any] = {}
        for name, (schema, key) in SETTINGS.items():
            try:
                after[name] = gsettings_get(schema, key)
            except Exception as error:  # noqa: BLE001
                after[name] = f"ERROR: {error}"
                errors.append(str(error))
        after["current-desktop"] = current_desktop()
        original_desktop = self.settings_before.get("current-desktop")
        if isinstance(original_desktop, int) and original_desktop >= 0:
            if current_desktop() != original_desktop:
                try:
                    set_desktop(original_desktop)
                except Exception as error:  # noqa: BLE001
                    errors.append(f"restauration bureau: {error}")
        after["current-desktop"] = current_desktop()
        self.settings_after = after
        self.settings_restored = (
            after.get("dynamic-workspaces") == self.settings_before.get("dynamic-workspaces")
            and after.get("num-workspaces") == self.settings_before.get("num-workspaces")
            and not errors
        )
        write_json(
            self.out_dir / "gnome-settings-after.json",
            {"after": after, "errors": errors, "restored": self.settings_restored},
        )

    def start_server(self) -> None:
        self.check_ports()
        token = secrets.token_hex(32)
        secret = secrets.token_hex(32)
        self.server_token = token
        self.server_token_sha = hashlib.sha256(token.encode()).hexdigest()
        env = os.environ.copy()
        env["WORKFLOW_SERVER_WORKER_TOKEN"] = token
        env["WORKFLOW_SERVER_SECRET_KEY"] = secret
        stdout = (self.out_dir / "server.log").open("w", encoding="utf-8")
        stderr = (self.out_dir / "server.err").open("w", encoding="utf-8")
        self.server_proc = subprocess.Popen(
            [
                str(self.args.server_bin),
                "--config",
                str(self.args.server_config),
            ],
            cwd=str(REPO),
            env=env,
            stdout=stdout,
            stderr=stderr,
            text=True,
            start_new_session=True,
        )
        self.server_sid = self.server_proc.pid
        deadline = time.monotonic() + 30
        html = ""
        while time.monotonic() < deadline:
            if self.server_proc.poll() is not None:
                raise RuntimeError(
                    f"serveur arrêté au démarrage (rc={self.server_proc.returncode}); "
                    f"voir {self.out_dir / 'server.err'}"
                )
            try:
                status, html = http_get(SERVER_URL + "/", timeout=1)
                if status == 200:
                    break
            except (urllib.error.URLError, TimeoutError, OSError):
                pass
            time.sleep(0.2)
        else:
            raise RuntimeError("serveur : GET / non disponible dans les 30 s")

        match = re.search(r'<meta name="worker-token" content="([^"]+)"', html)
        if not match:
            raise RuntimeError("serveur : meta worker-token absente")
        if match.group(1) != token:
            raise RuntimeError("serveur : worker-token HTML différent du token de lancement")
        try:
            status, body = http_get(
                SERVER_URL + "/api/ping",
                headers={"X-Worker-Token": token},
                timeout=3,
            )
        except urllib.error.HTTPError as error:
            raise RuntimeError(f"serveur : /api/ping HTTP {error.code}") from error
        if status != 200 or '"runtime":"rust"' not in body:
            raise RuntimeError(f"serveur : /api/ping inattendu: {status} {body!r}")
        self.log("serveur prêt sur %s (pid=%s)", SERVER_URL, self.server_proc.pid)

    def stop_server(self) -> None:
        if self.server_proc is None:
            return
        if self.server_proc.poll() is None:
            kill_session(self.server_sid or self.server_proc.pid, signal.SIGTERM)
            try:
                self.server_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                kill_session(self.server_sid or self.server_proc.pid, signal.SIGKILL)
                try:
                    self.server_proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    pass
        self.server_proc = None
        self.server_sid = None

    def cleanup_gui(self) -> dict[str, Any]:
        proc = self.gui_proc
        sid = self.gui_sid
        wid = self.gui_wid
        result: dict[str, Any] = {
            "exit_code": None,
            "forced_kill": False,
            "orphan_count": 0,
            "orphan_pids": [],
        }
        if proc is None:
            self.gui_wid = None
            return result

        if wid:
            run(["wmctrl", "-i", "-c", wid], timeout=5)
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            result["forced_kill"] = True
            kill_session(sid or proc.pid, signal.SIGTERM)
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                kill_session(sid or proc.pid, signal.SIGKILL)
                try:
                    proc.wait(timeout=2)
                except subprocess.TimeoutExpired:
                    pass
        result["exit_code"] = proc.returncode

        if sid is not None:
            deadline = time.monotonic() + 5.0
            gui_leftovers: list[dict[str, Any]] = []
            while time.monotonic() < deadline:
                leftovers = session_processes(sid)
                gui_leftovers = [
                    info
                    for info in leftovers
                    if str(info.get("name", "")).startswith("workflow-gui")
                    or str(info.get("name", "")).startswith("WebKit")
                ]
                if not gui_leftovers:
                    break
                time.sleep(0.2)
            result["orphan_count"] = len(gui_leftovers)
            result["orphan_pids"] = [info["pid"] for info in gui_leftovers]
            for info in gui_leftovers:
                try:
                    os.kill(info["pid"], signal.SIGKILL)
                except (ProcessLookupError, PermissionError):
                    pass
            if gui_leftovers:
                time.sleep(0.3)
                remaining = [
                    info
                    for info in session_processes(sid)
                    if str(info.get("name", "")).startswith("workflow-gui")
                    or str(info.get("name", "")).startswith("WebKit")
                ]
                result["orphan_pids_after_kill"] = [info["pid"] for info in remaining]

        if find_window() is not None:
            leftover_wid = find_window()
            if leftover_wid:
                run(["wmctrl", "-i", "-c", leftover_wid], timeout=5)

        self.gui_proc = None
        self.gui_sid = None
        self.gui_wid = None
        return result

    def cleanup(self) -> None:
        if self.cleaned:
            return
        self.cleaned = True
        try:
            self.cleanup_gui()
        except Exception as error:  # noqa: BLE001
            self.errors.append(f"cleanup_gui: {error}")
        try:
            self.stop_server()
        except Exception as error:  # noqa: BLE001
            self.errors.append(f"stop_server: {error}")
        try:
            self.restore_settings()
        except Exception as error:  # noqa: BLE001
            self.errors.append(f"restore_settings: {error}")
        write_json(
            self.out_dir / "cleanup.json",
            {
                "timestamp": utc_now(),
                "errors": self.errors,
                "gsettings_restored": self.settings_restored,
                "settings_before": self.settings_before,
                "settings_after": self.settings_after,
            },
        )
        self.log("cleanup terminé (gsettings restaurés=%s)", self.settings_restored)

    def wait_for_ready(self, run_dir: Path, proc: subprocess.Popen[str]) -> tuple[bool, str]:
        deadline = time.monotonic() + self.args.timeout
        ready = run_dir / "ready"
        audit = run_dir / "audit.json"
        while time.monotonic() < deadline:
            if ready.exists() and audit.exists():
                return True, "ok"
            if proc.poll() is not None:
                return False, f"gui_exited:{proc.returncode}"
            time.sleep(0.1)
        return False, "timeout"

    def wait_for_window(self, proc: subprocess.Popen[str]) -> str | None:
        deadline = time.monotonic() + self.args.timeout
        while time.monotonic() < deadline:
            wid = find_window()
            if wid:
                return wid
            if proc.poll() is not None:
                return None
            time.sleep(0.1)
        return None

    def stderr_scan(self, run_dir: Path) -> dict[str, Any]:
        stderr_path = run_dir / "gui.stderr.log"
        stdout_path = run_dir / "gui.stdout.log"
        text = ""
        for path in (stderr_path, stdout_path):
            if path.exists():
                text += path.read_text(encoding="utf-8", errors="replace") + "\n"
        patterns = {
            "accelerated_surface": r"AcceleratedSurface",
            "drm": r"\bDRM\b",
            "gbm": r"\bGBM\b",
            "egl": r"\bEGL\b",
            "sigsegv": r"SIGSEGV|segfault|Segmentation fault",
            "critical": r"\bCRITICAL\b",
            "error": r"\bERROR\b|\bErreur\b",
        }
        counts: dict[str, int] = {}
        for name, pattern in patterns.items():
            counts[name] = len(re.findall(pattern, text, flags=re.IGNORECASE))
        accelerated = [
            line
            for line in text.splitlines()
            if "AcceleratedSurface" in line
        ]
        if accelerated:
            (run_dir / "accelerated-surface.txt").write_text(
                "\n".join(accelerated) + "\n", encoding="utf-8"
            )
        return {
            "counts": counts,
            "accelerated_surface": counts["accelerated_surface"],
            "accelerated_lines": accelerated[:200],
            "segfault_text": counts["sigsegv"] > 0,
        }

    def run_sweep(self, wid: str, run_dir: Path) -> list[dict[str, Any]]:
        switches: list[dict[str, Any]] = []
        sticky = set_sticky(wid)
        activate(wid)
        if current_desktop() != 0:
            set_desktop(0)
        for index in range(1, 6):
            target = index % 5  # 1, 2, 3, 4, 0
            moved = False
            if not sticky:
                run(["wmctrl", "-i", "-r", wid, "-t", str(target)], timeout=5)
                moved = True
                time.sleep(0.2)
            before = current_desktop()
            ok, method = set_desktop(target)
            time.sleep(self.args.sweep_delay)
            activate(wid)
            capture_result = capture(wid, run_dir / f"ws-{index}.png")
            after = current_desktop()
            switches.append(
                {
                    "n": index,
                    "before": before,
                    "target": target,
                    "after": after,
                    "ok": bool(ok and after == target),
                    "method": method,
                    "sticky": sticky,
                    "window_moved": moved,
                    "capture": capture_result,
                }
            )
        if current_desktop() != 0:
            set_desktop(0)
        if not sticky:
            run(["wmctrl", "-i", "-r", wid, "-t", "0"], timeout=5)
        return switches

    def pre_run_leftovers(self) -> list[dict[str, Any]]:
        return self.gui_processes()

    def run_one(self, profile: Profile, index: int) -> dict[str, Any]:
        run_dir = self.out_dir / profile.name / f"run-{index:02d}"
        run_dir.mkdir(parents=True, exist_ok=True)
        started_at = utc_now()
        started = time.monotonic()
        pre_leftovers = self.pre_run_leftovers()
        for info in pre_leftovers:
            try:
                os.kill(info["pid"], signal.SIGKILL)
            except (ProcessLookupError, PermissionError):
                pass
        if pre_leftovers:
            time.sleep(0.5)
        env = os.environ.copy()
        env.update(profile.env)
        env["DISPLAY"] = DISPLAY
        env["GDK_BACKEND"] = "x11"
        stdout = (run_dir / "gui.stdout.log").open("w", encoding="utf-8")
        stderr = (run_dir / "gui.stderr.log").open("w", encoding="utf-8")
        record: dict[str, Any] = {
            "profile": profile.name,
            "run": index,
            "run_dir": str(run_dir),
            "started_at": started_at,
            "profile_env": profile.env,
            "sweep": profile.sweep,
            "pre_run_leftovers": pre_leftovers,
            "launch_ok": False,
            "window_found": False,
            "sticky": False,
            "ready_status": "not_started",
            "audit_ok": False,
            "token_check": None,
            "final": None,
            "sweeps": 0,
            "switches": [],
            "switches_ok": 0,
            "blank_switches": 0,
            "rss_kib": None,
            "pss_kib": None,
            "exit_code": None,
            "forced_kill": False,
            "orphan_count": 0,
            "orphan_pids": [],
            "accelerated_surface": 0,
            "accelerated_lines": [],
            "sigsegv": False,
            "crashed": False,
            "stderr_scan": {},
        }
        try:
            self.gui_proc = subprocess.Popen(
                [
                    str(self.args.gui_bin),
                    "--server-url",
                    SERVER_URL,
                    "--telemetry-dir",
                    str(run_dir),
                    "--ready-file",
                    str(run_dir / "ready"),
                    "--poc-driver",
                    str(self.args.audit_js),
                    "--log-level",
                    "info",
                ],
                cwd=str(REPO),
                env=env,
                stdout=stdout,
                stderr=stderr,
                text=True,
                start_new_session=True,
            )
            self.gui_sid = self.gui_proc.pid
            record["launch_ok"] = True
            wid = self.wait_for_window(self.gui_proc)
            self.gui_wid = wid
            record["window_found"] = wid is not None
            if wid:
                record["geometry"] = window_geometry(wid)
                record["sticky"] = set_sticky(wid)
                activate(wid)
            ready_ok, ready_status = self.wait_for_ready(run_dir, self.gui_proc)
            record["ready_status"] = ready_status
            audit = read_json(run_dir / "audit.json")
            token_check = read_json(run_dir / "token-check.json")
            record["token_check"] = token_check
            record["audit_ok"] = bool(
                ready_ok
                and token_check
                and token_check.get("status") == "ok"
                and audit
                and audit.get("token_sha256") == self.server_token_sha
            )
            if wid:
                time.sleep(0.8)
                activate(wid)
                record["final"] = capture(wid, run_dir / "final.png")
                if profile.sweep:
                    switches = self.run_sweep(wid, run_dir)
                    record["switches"] = switches
                    record["sweeps"] = 1
                    record["switches_ok"] = sum(1 for switch in switches if switch["ok"])
                    record["blank_switches"] = sum(
                        1
                        for switch in switches
                        if switch["capture"].get("blank")
                    )
                record["rss_kib"], record["pss_kib"] = memory_for_session(
                    self.gui_sid or self.gui_proc.pid
                )
        except Exception as error:  # noqa: BLE001 - on consigne et on continue
            record["error"] = str(error)
            self.log("ERREUR run %s/%s: %s", profile.name, index, error)
        finally:
            close = self.cleanup_gui()
            record.update(
                {
                    "exit_code": close.get("exit_code"),
                    "forced_kill": close.get("forced_kill", False),
                    "orphan_count": close.get("orphan_count", 0),
                    "orphan_pids": close.get("orphan_pids", []),
                    "orphan_pids_after_kill": close.get("orphan_pids_after_kill", []),
                }
            )
            scan = self.stderr_scan(run_dir)
            record["stderr_scan"] = scan
            record["accelerated_surface"] = scan["accelerated_surface"]
            record["accelerated_lines"] = scan["accelerated_lines"]
            record["sigsegv"] = bool(
                scan["segfault_text"]
                or (isinstance(record.get("exit_code"), int) and record["exit_code"] in (-11, 139))
            )
            record["crashed"] = bool(
                isinstance(record.get("exit_code"), int)
                and record["exit_code"] not in (0, None)
            )
            record["duration_s"] = round(time.monotonic() - started, 3)
            record["finished_at"] = utc_now()
            stdout.close()
            stderr.close()
            write_json(run_dir / "run.json", record)
            self.records.append(record)
            final = record.get("final") or {}
            self.log(
                "run %-16s %02d window=%s audit=%s final=%s switches=%s/%s orphan=%s accel=%s exit=%s",
                profile.name,
                index,
                record["window_found"],
                record["audit_ok"],
                final.get("blank") or "ok",
                record["switches_ok"],
                len(record["switches"]),
                record["orphan_count"],
                record["accelerated_surface"],
                record["exit_code"],
            )
        return record

    def summarize(self) -> dict[str, Any]:
        profiles: dict[str, Any] = {}
        for profile in self.args.profiles:
            rows = [row for row in self.records if row["profile"] == profile.name]
            profiles[profile.name] = {
                "runs": len(rows),
                "audit_ok": sum(1 for row in rows if row["audit_ok"]),
                "window_found": sum(1 for row in rows if row["window_found"]),
                "sticky_runs": sum(1 for row in rows if row["sticky"]),
                "sweeps": sum(row["sweeps"] for row in rows),
                "switches": sum(len(row["switches"]) for row in rows),
                "switches_ok": sum(row["switches_ok"] for row in rows),
                "blank_runs": sum(1 for row in rows if (row.get("final") or {}).get("blank")),
                "blank_switches": sum(row["blank_switches"] for row in rows),
                "sigsegv_runs": sum(1 for row in rows if row["sigsegv"]),
                "orphan_events": sum(
                    1
                    for row in rows
                    if row["orphan_count"] > 0 or row["forced_kill"]
                ),
                "accelerated_surface_runs": sum(
                    1 for row in rows if row["accelerated_surface"] > 0
                ),
                "crashed_runs": sum(1 for row in rows if row["crashed"]),
                "pre_run_leftovers": sum(1 for row in rows if row["pre_run_leftovers"]),
            }
        default = profiles.get("default", {})
        disable = profiles.get("disable-dmabuf", {})
        force = profiles.get("force-dmabuf", {})
        force_shm = profiles.get("force-dmabuf-shm", {})
        acceptance = {
            "default_ok": bool(
                default.get("runs") == 20
                and default.get("audit_ok") == 20
                and default.get("window_found") == 20
                and default.get("sticky_runs") == 20
                and default.get("sweeps") == 20
                and default.get("switches_ok") == 100
                and default.get("blank_runs") == 0
                and default.get("blank_switches") == 0
                and default.get("sigsegv_runs") == 0
                and default.get("orphan_events") == 0
                and default.get("pre_run_leftovers") == 0
            ),
            "disable_ok": bool(
                disable.get("runs") == 5
                and disable.get("audit_ok") == 5
                and disable.get("window_found") == 5
                and disable.get("sticky_runs") == 5
                and disable.get("sweeps") == 5
                and disable.get("switches_ok") == 25
                and disable.get("blank_runs") == 0
                and disable.get("blank_switches") == 0
                and disable.get("sigsegv_runs") == 0
                and disable.get("orphan_events") == 0
                and disable.get("pre_run_leftovers") == 0
            ),
            "canary_detected": bool(
                force.get("accelerated_surface_runs", 0) > 0
                or force.get("blank_runs", 0) > 0
                or force.get("sigsegv_runs", 0) > 0
                or force.get("crashed_runs", 0) > 0
            ),
            "force_shm_documented": bool(force_shm.get("runs") == 5),
            "gsettings_restored": self.settings_restored,
            "ports_clear": self.ports_clear(),
            "token_absent": not self.token_leak(),
        }
        summary = {
            "j2": {
                "started_at": self.started_at,
                "finished_at": utc_now(),
                "duration_s": round(time.monotonic() - self.started_monotonic, 3),
                "host": os.uname().nodename,
                "display": DISPLAY,
                "gui_bin": str(self.args.gui_bin),
                "server_bin": str(self.args.server_bin),
                "server_config": str(self.args.server_config),
                "server_token_sha256": self.server_token_sha,
                "total_runs": len(self.records),
                "total_switches": sum(len(row["switches"]) for row in self.records),
            },
            "gnome": {
                "before": self.settings_before,
                "after": self.settings_after,
                "restored": self.settings_restored,
            },
            "profiles": profiles,
            "acceptance": acceptance,
            "errors": self.errors,
        }
        write_json(self.out_dir / "summary.json", summary)
        self.write_csv()
        self.write_acceptance(summary)
        self.write_accelerated_log()
        return summary

    def ports_clear(self) -> bool:
        proc = run(["ss", "-ltnp"], timeout=5)
        return not any(
            port in proc.stdout for port in (":5003", ":5004", ":5005")
        )

    def token_leak(self) -> bool:
        if not self.server_token:
            return False
        for path in self.out_dir.rglob("*"):
            if not path.is_file():
                continue
            if path.suffix.lower() in {".png", ".jpg", ".jpeg", ".sqlite", ".db", ".gz", ".zip"}:
                continue
            try:
                if self.server_token in path.read_text(encoding="utf-8", errors="ignore"):
                    return True
            except OSError:
                continue
        return False

    def write_csv(self) -> None:
        fields = [
            "profile",
            "run",
            "launch_ok",
            "window_found",
            "sticky",
            "ready_status",
            "audit_ok",
            "token_check",
            "sweeps",
            "switches_ok",
            "blank_switches",
            "final_blank",
            "final_mean",
            "final_stddev",
            "final_colors",
            "rss_kib",
            "pss_kib",
            "exit_code",
            "forced_kill",
            "orphan_count",
            "orphan_pids",
            "accelerated_surface",
            "sigsegv",
            "crashed",
            "pre_run_leftovers",
            "duration_s",
        ]
        with (self.out_dir / "runs.csv").open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=fields)
            writer.writeheader()
            for row in self.records:
                final = row.get("final") or {}
                token_check = row.get("token_check") or {}
                writer.writerow(
                    {
                        "profile": row["profile"],
                        "run": row["run"],
                        "launch_ok": row["launch_ok"],
                        "window_found": row["window_found"],
                        "sticky": row["sticky"],
                        "ready_status": row["ready_status"],
                        "audit_ok": row["audit_ok"],
                        "token_check": token_check.get("status", ""),
                        "sweeps": row["sweeps"],
                        "switches_ok": row["switches_ok"],
                        "blank_switches": row["blank_switches"],
                        "final_blank": final.get("blank", ""),
                        "final_mean": final.get("mean", ""),
                        "final_stddev": final.get("stddev", ""),
                        "final_colors": final.get("colors", ""),
                        "rss_kib": row["rss_kib"] if row["rss_kib"] is not None else "",
                        "pss_kib": row["pss_kib"] if row["pss_kib"] is not None else "",
                        "exit_code": row["exit_code"] if row["exit_code"] is not None else "",
                        "forced_kill": row["forced_kill"],
                        "orphan_count": row["orphan_count"],
                        "orphan_pids": "|".join(str(pid) for pid in row["orphan_pids"]),
                        "accelerated_surface": row["accelerated_surface"],
                        "sigsegv": row["sigsegv"],
                        "crashed": row["crashed"],
                        "pre_run_leftovers": len(row["pre_run_leftovers"]),
                        "duration_s": row["duration_s"],
                    }
                )

    def write_acceptance(self, summary: dict[str, Any]) -> None:
        lines = [
            "# J2 — Stress-test de rendu",
            "",
            f"- Début : {summary['j2']['started_at']}",
            f"- Fin : {summary['j2']['finished_at']}",
            f"- Durée : {summary['j2']['duration_s']} s",
            f"- Lancements : {summary['j2']['total_runs']}",
            f"- Bascules : {summary['j2']['total_switches']}",
            "",
            "## Tableau d'acceptation",
            "",
            "| Profil | Lancements | Audits ok | Sticky | Balayages | Bascules ok | Blancs (runs) | Blancs (bascules) | SIGSEGV | Orphelins | AcceleratedSurface |",
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|",
        ]
        for name, data in summary["profiles"].items():
            lines.append(
                f"| `{name}` | {data['runs']} | {data['audit_ok']} | {data['sticky_runs']} | "
                f"{data['sweeps']} | {data['switches_ok']}/{data['switches']} | "
                f"{data['blank_runs']} | {data['blank_switches']} | {data['sigsegv_runs']} | "
                f"{data['orphan_events']} | {data['accelerated_surface_runs']} |"
            )
        acceptance = summary["acceptance"]
        lines += [
            "",
            "## Critères",
            "",
            f"- `default` stable : **{acceptance['default_ok']}**",
            f"- `disable-dmabuf` stable : **{acceptance['disable_ok']}**",
            f"- Canari `force-dmabuf` détecté : **{acceptance['canary_detected']}**",
            f"- `force-dmabuf-shm` documenté : **{acceptance['force_shm_documented']}**",
            f"- `gsettings` restaurés : **{acceptance['gsettings_restored']}**",
            f"- Ports 5003/5004/5005 libres : **{acceptance['ports_clear']}**",
            f"- Token absent des artefacts : **{acceptance['token_absent']}**",
            "",
            "## GNOME",
            "",
            f"- Avant : `{summary['gnome']['before']}`",
            f"- Après : `{summary['gnome']['after']}`",
            "",
            "## Notes",
            "",
            "- Les profils `force-dmabuf` et `force-dmabuf-shm` ne font pas de balayage :",
            "  ils servent de canaris et sont attendus en échec de rendu.",
            "- Les lignes exactes `AcceleratedSurface` sont dans `accelerated-surface.log`",
            "  et dans chaque `gui.stderr.log`.",
            "",
        ]
        (self.out_dir / "acceptance.md").write_text("\n".join(lines) + "\n", encoding="utf-8")

    def write_accelerated_log(self) -> None:
        lines = ["# Lignes AcceleratedSurface par run", ""]
        for row in self.records:
            if not row["profile"].startswith("force-"):
                continue
            lines.append(
                f"## profile={row['profile']} run={row['run']:02d} "
                f"exit={row['exit_code']} window={row['window_found']} "
                f"blank={(row.get('final') or {}).get('blank') or 'non'} "
                f"crashed={row['crashed']}"
            )
            if row["accelerated_lines"]:
                lines.extend(row["accelerated_lines"])
            else:
                lines.append("(aucune ligne AcceleratedSurface; extrait stderr)")
                stderr_path = Path(row["run_dir"]) / "gui.stderr.log"
                if stderr_path.exists():
                    excerpt = stderr_path.read_text(
                        encoding="utf-8", errors="replace"
                    ).splitlines()[-80:]
                    lines.extend(excerpt)
            lines.append("")
        if len(lines) == 2:
            lines.append("(aucun run force-* dans ce jeu de données)")
        (self.out_dir / "accelerated-surface.log").write_text(
            "\n".join(lines) + "\n", encoding="utf-8"
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, help="dossier d'artefacts J2")
    parser.add_argument("--gui-bin", type=Path, default=DEFAULT_GUI)
    parser.add_argument("--server-bin", type=Path, default=DEFAULT_SERVER)
    parser.add_argument("--server-config", type=Path, default=DEFAULT_CONFIG)
    parser.add_argument("--audit-js", type=Path, default=DEFAULT_AUDIT)
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("--sweep-delay", type=float, default=0.75)
    parser.add_argument("--smoke", action="store_true", help="1 lancement par profil")
    args = parser.parse_args()
    args.profiles = SMOKE_PROFILES if args.smoke else PROFILES
    return args


def main() -> int:
    args = parse_args()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        handlers=[
            logging.StreamHandler(sys.stdout),
            logging.FileHandler(out_dir / "harness.log", encoding="utf-8"),
        ],
    )
    resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
    harness = Harness(args)

    def on_signal(signum: int, _frame: Any) -> None:
        harness.log("signal %s reçu, nettoyage", signum)
        harness.cleanup()
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGHUP, on_signal)
    atexit.register(harness.cleanup)

    try:
        for path in (args.gui_bin, args.server_bin, args.server_config, args.audit_js):
            if not path.exists():
                raise RuntimeError(f"fichier absent: {path}")
        for tool in ("wmctrl", "xdotool", "import", "identify", "xwininfo", "gsettings", "ss"):
            if shutil.which(tool) is None:
                raise RuntimeError(f"outil absent: {tool}")
        if not os.environ.get("DISPLAY"):
            raise RuntimeError("DISPLAY absent")
        harness.ensure_clean()
        harness.check_ports()
        harness.save_settings()
        harness.apply_settings()
        harness.start_server()
        for profile in args.profiles:
            for index in range(1, profile.runs + 1):
                harness.run_one(profile, index)
    except Exception as error:  # noqa: BLE001
        harness.errors.append(str(error))
        harness.log("ERREUR FATALE: %s", error)
    finally:
        harness.cleanup()

    summary = harness.summarize()
    LOGGER.info(
        "J2 terminé: default_ok=%s disable_ok=%s canary=%s gsettings=%s ports=%s",
        summary["acceptance"]["default_ok"],
        summary["acceptance"]["disable_ok"],
        summary["acceptance"]["canary_detected"],
        summary["acceptance"]["gsettings_restored"],
        summary["acceptance"]["ports_clear"],
    )
    print(json.dumps(summary["acceptance"], indent=2, ensure_ascii=False))
    if not summary["acceptance"]["default_ok"] or not summary["acceptance"]["disable_ok"]:
        return 1
    if not summary["acceptance"]["gsettings_restored"]:
        return 2
    if not summary["acceptance"]["ports_clear"]:
        return 3
    return 0


if __name__ == "__main__":
    sys.exit(main())
