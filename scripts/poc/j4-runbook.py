#!/usr/bin/env python3
"""Runbook J4 : matrice d'états du tray puis soak d'endurance.

Deux instances isolées :
- matrice : serveur `:5005`, GUI, vidéo 120 s, transitions d'état ;
- soak : serveur `:5005`, GUI, projet vide, 2 h de polling à 1 Hz.
Aucun accès à `runtime/` ni `/mnt/cache`.
"""

from __future__ import annotations

import argparse
import csv
import fcntl
import hashlib
import json
import logging
import math
import os
import re
import shutil
import signal
import statistics
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from logging.handlers import RotatingFileHandler
from pathlib import Path
from typing import Any

import psutil

REPO = Path(__file__).resolve().parents[2]
DEFAULT_GUI = REPO / "target/release/workflow-gui-tauri"
DEFAULT_SERVER = REPO / "target/release/workflow-server"
DEFAULT_CLI = REPO / "target/release/workflow-cli"
DEFAULT_AUDIT = REPO / "scripts/poc/audit.js"
DEFAULT_MONITOR = REPO / "scripts/poc/j4-monitor.py"
DEFAULT_BASE_CONFIG = Path("/tmp/opencode/poc-gui/config.toml")
SERVER_URL = "http://127.0.0.1:5005"
LOCK_PATH = Path("/tmp/workflow-rs-hw.lock")
TITLE = "Workflow Manager"
TEXT_SUFFIXES = {
    ".json",
    ".jsonl",
    ".log",
    ".txt",
    ".md",
    ".csv",
    ".toml",
    ".err",
    ".out",
}

LOGGER = logging.getLogger("j4")


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f".{path.name}.tmp")
    tmp.write_text(json.dumps(value, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    tmp.replace(path)


def read_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def run(cmd: list[str], timeout: float = 10) -> tuple[int, str, str]:
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, check=False)
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except Exception as error:  # pragma: no cover - environnement
        return 1, "", str(error)


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode()).hexdigest()


def gsettings_get(schema: str, key: str) -> str:
    rc, out, err = run(["gsettings", "get", schema, key], timeout=5)
    return out.strip() if rc == 0 else f"ERROR: {err}"


def gnome_snapshot() -> dict[str, Any]:
    return {
        "dynamic-workspaces": gsettings_get("org.gnome.mutter", "dynamic-workspaces"),
        "num-workspaces": gsettings_get("org.gnome.desktop.wm.preferences", "num-workspaces"),
    }


def lock_held(path: Path = LOCK_PATH) -> bool:
    if not path.exists():
        return False
    try:
        fd = os.open(path, os.O_RDWR)
    except OSError:
        return True
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return True
        else:
            fcntl.flock(fd, fcntl.LOCK_UN)
            return False
    finally:
        os.close(fd)


def process_rows() -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for proc in psutil.process_iter(["pid", "name", "cmdline"]):
        try:
            cmdline = proc.info["cmdline"] or []
            names = {proc.info["name"] or ""}
            if cmdline:
                names.add(Path(cmdline[0]).name)
            rows.append(
                {
                    "pid": proc.info["pid"],
                    "name": proc.info["name"] or "",
                    "cmdline": cmdline,
                    "names": {name for name in names if name},
                }
            )
        except psutil.Error:
            pass
    return rows


def find_residuals() -> dict[str, list[dict[str, Any]]]:
    result = {"firefox": [], "gui": [], "step2": [], "webkit": []}
    for row in process_rows():
        names = row["names"]
        if "firefox" in names or "firefox-bin" in names:
            result["firefox"].append(row)
        if any(name.startswith("workflow-gui") for name in names):
            result["gui"].append(row)
        if any(name.startswith("workflow-cli") for name in names) and "step2" in row["cmdline"]:
            result["step2"].append(row)
        if any(name.startswith("WebKit") for name in names):
            result["webkit"].append(row)
    return result


def window_ids() -> list[str]:
    rc, out, _ = run(["wmctrl", "-l"], timeout=5)
    if rc != 0:
        return []
    return [line.split()[0] for line in out.splitlines() if TITLE in line]


def window_state(wid: str) -> dict[str, Any]:
    rc, out, err = run(["xprop", "-id", wid, "_NET_WM_STATE"], timeout=5)
    hidden = None
    if rc == 0:
        hidden = "_NET_WM_STATE_HIDDEN" in out
    rc2, out2, err2 = run(["xwininfo", "-id", wid], timeout=5)
    map_state = ""
    if rc2 == 0:
        for line in out2.splitlines():
            if "Map State:" in line:
                map_state = line.split("Map State:", 1)[1].strip()
                break
    return {
        "hidden": hidden,
        "map_state": map_state,
        "xprop_error": err if rc != 0 else "",
        "xwininfo_error": err2 if rc2 != 0 else "",
    }


def set_geometry(wid: str) -> None:
    rc, _, _ = run(["wmctrl", "-i", "-r", wid, "-e", "0,240,109,1440,900"], timeout=5)
    if rc != 0:
        run(["xdotool", "windowsize", wid, "1440", "900"], timeout=5)
        run(["xdotool", "windowmove", wid, "240", "109"], timeout=5)
    run(["wmctrl", "-i", "-a", wid], timeout=5)


def capture(wid: str, path: Path) -> dict[str, Any]:
    path.parent.mkdir(parents=True, exist_ok=True)
    rc, _, err = run(["import", "-window", wid, str(path)], timeout=10)
    if rc != 0 or not path.exists():
        return {"ok": False, "error": err}
    rc, out, err = run(
        ["identify", "-format", "%w %h %[fx:mean] %[fx:standard_deviation] %k", str(path)],
        timeout=10,
    )
    if rc != 0:
        return {"ok": False, "error": err}
    parts = out.split()
    try:
        width, height = int(parts[0]), int(parts[1])
        mean, stddev, colors = float(parts[2]), float(parts[3]), int(parts[4])
    except (ValueError, IndexError) as error:
        return {"ok": False, "error": f"identify: {error}: {out}"}
    stddev_value = None if math.isnan(stddev) else stddev
    blank = ""
    if stddev_value is None or colors <= 1:
        blank = "solid"
    elif mean > 0.95 and stddev_value < 0.02:
        blank = "white"
    elif mean < 0.05 and stddev_value < 0.02:
        blank = "black"
    return {
        "ok": True,
        "width": width,
        "height": height,
        "mean": mean,
        "stddev": stddev_value,
        "colors": colors,
        "blank": blank,
    }


def status_notifier_items() -> dict[str, Any]:
    rc, out, err = run(
        [
            "gdbus",
            "call",
            "--session",
            "--dest",
            "org.kde.StatusNotifierWatcher",
            "--object-path",
            "/StatusNotifierWatcher",
            "--method",
            "org.freedesktop.DBus.Properties.Get",
            "org.kde.StatusNotifierWatcher",
            "RegisteredStatusNotifierItems",
        ],
        timeout=5,
    )
    items = re.findall(r"'([^']+)'", out)
    tray_items = [
        item
        for item in items
        if "tray_icon" in item.lower() or "workflow" in item.lower()
    ]
    return {
        "rc": rc,
        "raw": out,
        "error": err,
        "items": items,
        "item_path": tray_items[0] if tray_items else "",
        "registered": rc == 0 and bool(tray_items),
    }


@dataclass
class Args:
    out: Path
    base_config: Path
    gui_bin: Path
    server_bin: Path
    cli_bin: Path
    audit_js: Path
    monitor: Path
    matrix_hold: int
    soak_seconds: int
    soak_interval: float
    abort_vram: int
    skip_matrix: bool
    skip_soak: bool
    smoke: bool
    display: str


class Runbook:
    def __init__(self, args: Args) -> None:
        self.args = args
        self.out = args.out.resolve()
        self.out.mkdir(parents=True, exist_ok=True)
        self.server_url = SERVER_URL
        self.token = ""
        self.token_sha = ""
        self.server: subprocess.Popen[str] | None = None
        self.server_logs: tuple[Any, Any] | None = None
        self.gui: subprocess.Popen[str] | None = None
        self.gui_logs: tuple[Any, Any] | None = None
        self.gui_pid: int | None = None
        self.gui_wid = ""
        self.gui_dir: Path | None = None
        self.gnome_before = gnome_snapshot()
        self.matrix: dict[str, Any] = {}
        self.soak: dict[str, Any] = {}
        self.cleanup_data: dict[str, Any] = {}
        self.dbus: dict[str, Any] = {}
        self.preflight_data: dict[str, Any] = {}
        self.errors: list[str] = []
        self.started_at = utc_now()
        self.started = time.monotonic()

    def log(self, message: str, *args: Any) -> None:
        LOGGER.info(message, *args)

    def preflight(self) -> None:
        errors: list[str] = []
        for name, path in [
            ("gui", self.args.gui_bin),
            ("server", self.args.server_bin),
            ("cli", self.args.cli_bin),
            ("audit", self.args.audit_js),
            ("monitor", self.args.monitor),
            ("base_config", self.args.base_config),
        ]:
            if not path.exists():
                errors.append(f"fichier absent ({name}): {path}")
        for tool in (
            "ffmpeg",
            "ffprobe",
            "nvidia-smi",
            "wmctrl",
            "xdotool",
            "import",
            "identify",
            "xprop",
            "gsettings",
            "ss",
            "curl",
            "systemd-inhibit",
            "gdbus",
        ):
            if shutil.which(tool) is None:
                errors.append(f"outil absent: {tool}")
        if not os.environ.get("DISPLAY"):
            errors.append("DISPLAY absent")
        out_text = str(self.out)
        if out_text.startswith(str(REPO / "runtime")) or out_text.startswith("/mnt/cache"):
            errors.append(f"sortie interdite: {self.out}")

        rc, out, err = run(["pkg-config", "--modversion", "ayatana-appindicator3-0.1"], timeout=5)
        if rc != 0:
            errors.append(f"libayatana-appindicator3-dev absent: {err}")
        rc, out, _ = run(["gnome-extensions", "info", "ubuntu-appindicators@ubuntu.com"], timeout=5)
        if rc != 0 or "ENABLED" not in out:
            errors.append("extension ubuntu-appindicators non active")
        rc, _, err = run(
            [
                "gdbus",
                "introspect",
                "--session",
                "--dest",
                "org.kde.StatusNotifierWatcher",
                "--object-path",
                "/StatusNotifierWatcher",
            ],
            timeout=5,
        )
        if rc != 0:
            errors.append(f"StatusNotifierWatcher absent: {err}")

        rc, out, _ = run(["ss", "-ltnp"], timeout=5)
        if rc == 0:
            for line in out.splitlines():
                if any(port in line for port in (":5003", ":5004", ":5005")):
                    errors.append(f"port occupé: {line.strip()}")
        else:
            errors.append("ss indisponible")

        residuals = find_residuals()
        for kind, rows in residuals.items():
            if rows:
                errors.append(f"processus {kind} résiduel: {[row['pid'] for row in rows]}")
        if lock_held():
            errors.append(f"verrou matériel tenu: {LOCK_PATH}")

        rc, out, _ = run(
            [
                "nvidia-smi",
                "--query-gpu=memory.used,memory.free,memory.total",
                "--format=csv,noheader,nounits",
                "--id=0",
            ],
            timeout=5,
        )
        vram: dict[str, Any] = {}
        if rc == 0 and out:
            try:
                used, free, total = (int(value.strip()) for value in out.split(","))
                vram = {"used_mib": used, "free_mib": free, "total_mib": total}
                if free < 2400:
                    errors.append(f"VRAM libre insuffisante: {free} MiB")
            except ValueError:
                errors.append(f"nvidia-smi illisible: {out}")
        else:
            errors.append("nvidia-smi indisponible")

        self.preflight_data = {
            "timestamp": utc_now(),
            "out": str(self.out),
            "vram": vram,
            "gnome_before": self.gnome_before,
            "ayatana_pkgconfig": out if rc == 0 else "",
            "errors": errors,
        }
        write_json(self.out / "preflight.json", self.preflight_data)
        if errors:
            raise RuntimeError("préflight J4 échoué:\n- " + "\n- ".join(errors))
        self.log("préflight ok (VRAM libre %s MiB)", vram.get("free_mib"))

    def write_config(self, state_dir: Path, name: str) -> Path:
        base = self.args.base_config.read_text(encoding="utf-8")
        state = str(state_dir)
        base = base.replace("/tmp/opencode/poc-gui/state", state)
        base = base.replace(
            'workdir = "/tmp/opencode/poc-gui"',
            f'workdir = "{self.out}"',
        )
        base = base.replace(
            'source_dir = "/home/kidpixel/workflow-rs/runtime/downloads"',
            f'source_dir = "{state}/downloads"',
        )
        base = re.sub(r"(?m)^port = \d+$", "port = 5005", base, count=1)
        base = base.replace("enabled = true", "enabled = false")
        base = base.replace("dry_run = false", "dry_run = true")
        config = self.out / f"config-{name}.toml"
        config.write_text(base, encoding="utf-8")
        bad = []
        for line in base.splitlines():
            stripped = line.strip()
            if stripped.startswith(
                (
                    "base =",
                    "projects =",
                    "archives =",
                    "logs =",
                    "downloads =",
                    "history_db =",
                    "cache_root =",
                    "finalized =",
                    "source_dir =",
                    "workdir =",
                )
            ) and ("/mnt/cache" in stripped or "workflow-rs/runtime" in stripped):
                bad.append(stripped)
        if bad:
            raise RuntimeError(f"chemins de production dans {config}: {bad}")
        if "enabled = false" not in base or "dry_run = true" not in base:
            raise RuntimeError(f"webhook non désactivé dans {config}")
        for directory in ("projects", "archives", "logs", "downloads", "cache", "finalized"):
            (state_dir / directory).mkdir(parents=True, exist_ok=True)
        return config

    def start_server(self, config: Path, log_dir: Path, prefix: str = "server") -> None:
        log_dir.mkdir(parents=True, exist_ok=True)
        self.stop_server()
        token = os.urandom(32).hex()
        secret = os.urandom(32).hex()
        self.token = token
        self.token_sha = sha256_text(token)
        env = os.environ.copy()
        for key in (
            "WORKFLOW_PATHS_BASE",
            "WORKFLOW_SERVER_PORT",
            "WORKFLOW_WEBHOOK_ENABLED",
            "WORKFLOW_WEBHOOK_URL",
            "WORKFLOW_STEP1_SOURCE_DIR",
        ):
            env.pop(key, None)
        env["WORKFLOW_SERVER_WORKER_TOKEN"] = token
        env["WORKFLOW_SERVER_SECRET_KEY"] = secret
        stdout = (log_dir / f"{prefix}.log").open("w", encoding="utf-8")
        stderr = (log_dir / f"{prefix}.err").open("w", encoding="utf-8")
        self.server = subprocess.Popen(
            [str(self.args.server_bin), "--config", str(config)],
            cwd=str(REPO),
            env=env,
            stdout=stdout,
            stderr=stderr,
            text=True,
            start_new_session=True,
        )
        self.server_logs = (stdout, stderr)
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if self.server.poll() is not None:
                raise RuntimeError(
                    f"serveur arrêté rc={self.server.returncode}; voir {log_dir / (prefix + '.err')}"
                )
            try:
                status, body = self.http("GET", "/", timeout=1)
            except (urllib.error.URLError, TimeoutError, OSError):
                time.sleep(0.2)
                continue
            if status == 200:
                match = re.search(r'<meta name="worker-token" content="([^"]+)"', body)
                if not match or match.group(1) != token:
                    raise RuntimeError("serveur: token HTML absent ou différent")
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("serveur: GET / indisponible dans les 30 s")
        write_json(
            self.out / "server.json",
            {
                "timestamp": utc_now(),
                "pid": self.server.pid,
                "url": self.server_url,
                "config": str(config),
                "token_sha256": self.token_sha,
            },
        )
        self.log("serveur prêt sur %s (pid=%s)", self.server_url, self.server.pid)

    def stop_server(self) -> None:
        if self.server is not None and self.server.poll() is None:
            try:
                os.killpg(self.server.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                self.server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(self.server.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    self.server.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    pass
        self.server = None
        if self.server_logs:
            for handle in self.server_logs:
                handle.close()
            self.server_logs = None

    def http(
        self,
        method: str,
        path: str,
        token: str | None = None,
        body: Any = None,
        timeout: float = 5,
    ) -> tuple[int, str]:
        url = self.server_url.rstrip("/") + path
        data = None if body is None else json.dumps(body).encode()
        headers = {"Content-Type": "application/json"}
        if token:
            headers["X-Worker-Token"] = token
        request = urllib.request.Request(url, data=data, headers=headers, method=method)
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        try:
            with opener.open(request, timeout=timeout) as response:
                return response.status, response.read().decode("utf-8", "replace")
        except urllib.error.HTTPError as error:
            return error.code, error.read().decode("utf-8", "replace")

    def start_gui(self, telemetry_dir: Path) -> None:
        telemetry_dir.mkdir(parents=True, exist_ok=True)
        self.stop_gui()
        env = os.environ.copy()
        env["DISPLAY"] = self.args.display
        env["GDK_BACKEND"] = "x11"
        stdout = (telemetry_dir / "gui.stdout.log").open("w", encoding="utf-8")
        stderr = (telemetry_dir / "gui.stderr.log").open("w", encoding="utf-8")
        command = [
            str(self.args.gui_bin),
            "--server-url",
            self.server_url,
            "--telemetry-dir",
            str(telemetry_dir),
            "--ready-file",
            str(telemetry_dir / "ready"),
            "--poc-driver",
            str(self.args.audit_js),
            "--log-level",
            "info",
            "--log-file",
            str(telemetry_dir / "host.log"),
        ]
        self.gui = subprocess.Popen(
            command,
            cwd=str(REPO),
            env=env,
            stdout=stdout,
            stderr=stderr,
            text=True,
            start_new_session=True,
        )
        self.gui_logs = (stdout, stderr)
        self.gui_pid = self.gui.pid
        self.gui_dir = telemetry_dir
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if self.gui.poll() is not None:
                raise RuntimeError(f"GUI arrêté rc={self.gui.returncode}")
            ids = window_ids()
            if len(ids) == 1:
                self.gui_wid = ids[0]
                break
            if len(ids) > 1:
                raise RuntimeError(f"GUI: plusieurs fenêtres {ids}")
            time.sleep(0.2)
        if not self.gui_wid:
            raise RuntimeError("GUI: fenêtre Workflow Manager absente")
        set_geometry(self.gui_wid)
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if self.gui.poll() is not None:
                raise RuntimeError(f"GUI arrêté avant audit rc={self.gui.returncode}")
            ready = (telemetry_dir / "ready").exists()
            audit = read_json(telemetry_dir / "audit.json")
            token_check = read_json(telemetry_dir / "token-check.json")
            if ready and audit and token_check:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("GUI: ready/audit/token-check absents")
        audit = read_json(telemetry_dir / "audit.json") or {}
        token_check = read_json(telemetry_dir / "token-check.json") or {}
        if token_check.get("status") != "ok":
            raise RuntimeError(f"token-check non ok: {token_check}")
        if audit.get("token_sha256") != self.token_sha:
            raise RuntimeError("audit: digest token différent")
        self.log("GUI prêt (pid=%s, wid=%s)", self.gui_pid, self.gui_wid)

    def stop_gui(self) -> None:
        if self.gui is not None and self.gui.poll() is None:
            try:
                os.killpg(self.gui.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                self.gui.wait(timeout=5)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(self.gui.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                try:
                    self.gui.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    pass
        self.gui = None
        self.gui_pid = None
        self.gui_wid = ""
        if self.gui_logs:
            for handle in self.gui_logs:
                handle.close()
            self.gui_logs = None

    def tray_state(self) -> dict[str, Any] | None:
        if self.gui_dir is None:
            return None
        return read_json(self.gui_dir / "tray-state.json")

    def tray_state_name(self) -> str | None:
        payload = self.tray_state()
        if not payload:
            return None
        return (payload.get("status") or {}).get("state")

    def wait_tray(self, expected: set[str], timeout: float) -> tuple[str | None, list[str]]:
        deadline = time.monotonic() + timeout
        seen: list[str] = []
        while time.monotonic() < deadline:
            if self.gui is not None and self.gui.poll() is not None:
                raise RuntimeError(f"GUI arrêté pendant l'attente tray rc={self.gui.returncode}")
            state = self.tray_state_name()
            if state and (not seen or seen[-1] != state):
                seen.append(state)
            if state in expected:
                return state, seen
            time.sleep(0.1)
        return None, seen

    def collect_tray(self, duration: float) -> list[str]:
        deadline = time.monotonic() + duration
        seen: list[str] = []
        while time.monotonic() < deadline:
            state = self.tray_state_name()
            if state and (not seen or seen[-1] != state):
                seen.append(state)
            time.sleep(0.1)
        return seen

    def save_tray(self, directory: Path, name: str) -> dict[str, Any] | None:
        payload = self.tray_state()
        if payload:
            write_json(directory / f"state-{name}.json", payload)
        return payload

    def find_step_child(self, step: str) -> int | None:
        """PID du `workflow-cli` lancé par le serveur pour cette étape."""
        if self.server is None:
            return None
        try:
            parent = psutil.Process(self.server.pid)
        except psutil.Error:
            return None
        for child in parent.children(recursive=True):
            try:
                joined = " ".join(child.cmdline()).lower()
            except psutil.Error:
                continue
            if "workflow-cli" in joined and step.lower() in joined:
                return child.pid
        return None

    def hide_window(self, wid: str) -> tuple[dict[str, Any], str]:
        """Cache la fenêtre et privilégie `_NET_WM_STATE_HIDDEN`.

        `wmctrl -b add,hidden` suffit sur certains WMs ; Mutter exige une
        minimisation réelle pour publier `_NET_WM_STATE_HIDDEN`.
        """
        run(["wmctrl", "-i", "-r", wid, "-b", "add,hidden"], timeout=5)
        time.sleep(2)
        state = window_state(wid)
        method = "wmctrl-hidden"
        if state.get("hidden") is not True:
            run(["xdotool", "windowminimize", wid], timeout=5)
            time.sleep(1)
            state = window_state(wid)
            method = "xdotool-minimize"
        return state, method

    def show_window(self, wid: str) -> dict[str, Any]:
        # Une fenêtre minimisée reste gérée par le WM : `wmctrl -a` la restaure.
        run(["wmctrl", "-i", "-r", wid, "-b", "remove,hidden"], timeout=5)
        run(["wmctrl", "-i", "-a", wid], timeout=5)
        time.sleep(1)
        state = window_state(wid)
        if state.get("hidden") is not False or state.get("map_state") != "IsViewable":
            run(["xdotool", "windowmap", wid], timeout=5)
            run(["wmctrl", "-i", "-a", wid], timeout=5)
            time.sleep(1)
            state = window_state(wid)
        return state

    def tray_toggle(self, item_path: str) -> dict[str, Any]:
        """Déclenche l'item « afficher/masquer » du menu D-Bus.

        L'item 6 correspond à `toggle` dans le menu construit par l'hôte
        Tauri ; l'événement `clicked` est le contrat dbusmenu standard.
        """
        if not item_path or "/" not in item_path:
            return {"ok": False, "error": f"item D-Bus invalide: {item_path!r}"}
        bus, path = item_path.split("/", 1)
        menu = f"/{path}/Menu"
        rc, out, err = run(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                bus,
                "--object-path",
                menu,
                "--method",
                "com.canonical.dbusmenu.Event",
                "6",
                "clicked",
                "<0>",
                "0",
            ],
            timeout=5,
        )
        return {
            "ok": rc == 0,
            "rc": rc,
            "out": out,
            "error": err,
            "bus": bus,
            "menu": menu,
        }

    def matrix_phase(self) -> dict[str, Any]:
        directory = self.out / "matrix"
        directory.mkdir(parents=True, exist_ok=True)
        state_dir = self.out / "state-matrix"
        config = self.write_config(state_dir, "matrix")
        video = state_dir / "projects" / "clip-120s.mp4"
        rc, _, err = run(
            [
                "ffmpeg",
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x720:rate=25:duration=120",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "23",
                "-pix_fmt",
                "yuv420p",
                str(video),
            ],
            timeout=180,
        )
        if rc != 0 or not video.exists():
            raise RuntimeError(f"ffmpeg matrice: {err}")

        self.dbus["before_matrix"] = status_notifier_items()
        self.start_server(config, directory, prefix="server")
        self.start_gui(directory / "gui")
        self.dbus["after_matrix_gui"] = status_notifier_items()
        if not self.dbus["after_matrix_gui"].get("registered"):
            raise RuntimeError("tray non enregistré auprès de StatusNotifierWatcher")
        idle, idle_seen = self.wait_tray({"Idle"}, 30)
        if idle != "Idle":
            raise RuntimeError(f"matrice: Idle non atteint, vu {idle_seen}")
        self.save_tray(directory, "idle")
        self.log("matrice: Idle")

        # Sonde de masquage : valide la méthode utilisée par le soak.
        hidden_state, hidden_method = self.hide_window(self.gui_wid)
        hidden_ok = hidden_state.get("hidden") is True
        hidden_restored = self.show_window(self.gui_wid)
        hidden_restored_ok = (
            hidden_restored.get("hidden") is False
            and hidden_restored.get("map_state") == "IsViewable"
        )
        self.log(
            "matrice: masquage=%s méthode=%s restauré=%s",
            hidden_ok,
            hidden_method,
            hidden_restored_ok,
        )

        # Running puis Success.
        bad = state_dir / "projects" / "bad.mp4"
        csv_path = state_dir / "projects" / "clip-120s.csv"
        if bad.exists():
            bad.unlink()
        if csv_path.exists():
            csv_path.unlink()
        status, body = self.http("POST", "/run/STEP2", token=self.token)
        if status not in (200, 202):
            raise RuntimeError(f"matrice: run STEP2 HTTP {status}: {body[:200]}")
        running, running_seen = self.wait_tray({"Running"}, 20)
        success, success_seen = self.wait_tray({"Success", "Error"}, 180)
        self.save_tray(directory, "running-success")
        self.log("matrice: running=%s success=%s", running, success)

        # Error avec un faux MP4.
        if video.exists():
            video.unlink()
        if csv_path.exists():
            csv_path.unlink()
        bad.write_text("not a video\n", encoding="utf-8")
        status, body = self.http("POST", "/run/STEP2", token=self.token)
        if status not in (200, 202):
            raise RuntimeError(f"matrice: run STEP2 error HTTP {status}: {body[:200]}")
        error_state, error_seen = self.wait_tray({"Error"}, 90)
        self.save_tray(directory, "error")
        self.log("matrice: error=%s", error_state)

        # Stopping : la phase est transitoire et dure moins d'un tick à 1 Hz.
        # On fige le CLI avec SIGSTOP, on demande l'arrêt, on laisse le watcher
        # observer `stopping`, puis SIGCONT pour terminer proprement.
        if bad.exists():
            bad.unlink()
        if csv_path.exists():
            csv_path.unlink()
        if not video.exists():
            rc, _, err = run(
                [
                    "ffmpeg",
                    "-y",
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "testsrc2=size=1280x720:rate=25:duration=120",
                    "-c:v",
                    "libx264",
                    "-preset",
                    "veryfast",
                    "-crf",
                    "23",
                    "-pix_fmt",
                    "yuv420p",
                    str(video),
                ],
                timeout=180,
            )
            if rc != 0:
                raise RuntimeError(f"ffmpeg matrice stopping: {err}")
        status, body = self.http("POST", "/run/STEP2", token=self.token)
        if status not in (200, 202):
            raise RuntimeError(f"matrice: run STEP2 stopping HTTP {status}: {body[:200]}")
        self.wait_tray({"Running"}, 20)
        child_pid = self.find_step_child("step2")
        frozen = False
        if child_pid:
            try:
                os.kill(child_pid, signal.SIGSTOP)
                frozen = True
                time.sleep(0.5)
            except ProcessLookupError:
                frozen = False
        status, body = self.http("POST", "/stop/STEP2", token=self.token)
        if status not in (200, 202):
            raise RuntimeError(f"matrice: stop STEP2 HTTP {status}: {body[:200]}")
        stopping, stopping_seen = self.wait_tray({"Stopping"}, 10)
        self.save_tray(directory, "stopping")
        self.log(
            "matrice: stopping=%s (figé=%s, pid=%s, vu=%s)",
            stopping,
            frozen,
            child_pid,
            stopping_seen,
        )
        if frozen and child_pid:
            try:
                os.kill(child_pid, signal.SIGCONT)
            except ProcessLookupError:
                pass
        self.wait_tray({"Idle", "Error"}, 60)

        # Offline puis reconnexion.
        self.stop_server()
        offline, offline_seen = self.wait_tray({"Offline"}, 20)
        self.save_tray(directory, "offline")
        self.log("matrice: offline=%s", offline)
        self.start_server(config, directory, prefix="server-restart")
        reconnect, reconnect_seen = self.wait_tray({"Idle"}, 30)
        self.save_tray(directory, "reconnect")
        self.log("matrice: reconnect=%s", reconnect)

        if self.args.matrix_hold > 0:
            time.sleep(self.args.matrix_hold)
            self.save_tray(directory, "hold")

        # Fermeture vers le tray : dernier test, car `window.hide()` retire la
        # fenêtre de la liste du WM et seule l'application peut la remettre.
        run(["wmctrl", "-i", "-c", self.gui_wid], timeout=5)
        time.sleep(2)
        closed_state = window_state(self.gui_wid)
        closed_away = closed_state.get("hidden") is True or closed_state.get(
            "map_state"
        ) == "IsUnMapped"
        close_to_tray = bool(
            self.gui is not None and self.gui.poll() is None and closed_away
        )
        item_path = self.dbus["after_matrix_gui"].get("item_path", "")
        toggle = self.tray_toggle(item_path)
        time.sleep(1)
        restored_state = window_state(self.gui_wid)
        restored_visible = (
            restored_state.get("hidden") is False
            and restored_state.get("map_state") == "IsViewable"
        )
        close_to_tray = close_to_tray and toggle.get("ok", False) and restored_visible
        self.log(
            "matrice: close-to-tray=%s (closed=%s, vivant=%s, toggle=%s, restauré=%s)",
            close_to_tray,
            closed_state,
            self.gui is not None and self.gui.poll() is None,
            toggle,
            restored_state,
        )

        self.dbus["after_matrix"] = status_notifier_items()
        matrix = {
            "duration_s": None,
            "idle": idle == "Idle",
            "idle_seen": idle_seen,
            "running": running == "Running",
            "running_seen": running_seen,
            "success": success == "Success",
            "success_seen": success_seen,
            "error": error_state == "Error",
            "error_seen": error_seen,
            "stopping": stopping == "Stopping",
            "stopping_seen": stopping_seen,
            "stopping_frozen": frozen,
            "stopping_pid": child_pid,
            "offline": offline == "Offline",
            "offline_seen": offline_seen,
            "reconnect": reconnect == "Idle",
            "reconnect_seen": reconnect_seen,
            "close_to_tray": close_to_tray,
            "closed_state": closed_state,
            "restored_state": restored_state,
            "tray_toggle": toggle,
            "hidden_probe": {
                "ok": hidden_ok,
                "method": hidden_method,
                "state": hidden_state,
                "restored_ok": hidden_restored_ok,
                "restored_state": hidden_restored,
            },
            "dbus_item": self.dbus["after_matrix_gui"].get("item_path"),
            "matrix_ok": bool(
                idle == "Idle"
                and running == "Running"
                and success == "Success"
                and error_state == "Error"
                and stopping == "Stopping"
                and offline == "Offline"
                and reconnect == "Idle"
                and close_to_tray
                and hidden_ok
                and hidden_restored_ok
                and self.dbus["after_matrix_gui"].get("registered")
            ),
        }
        self.stop_gui()
        self.stop_server()
        write_json(directory / "states.json", matrix)
        self.matrix = matrix
        self.log("matrice terminée: ok=%s", matrix["matrix_ok"])
        return matrix

    def soak_phase(self) -> dict[str, Any]:
        directory = self.out / "soak"
        monitor_dir = directory / "monitor"
        monitor_dir.mkdir(parents=True, exist_ok=True)
        state_dir = self.out / "state-soak"
        config = self.write_config(state_dir, "soak")
        self.dbus["before_soak"] = status_notifier_items()
        self.start_server(config, monitor_dir, prefix="server")
        self.start_gui(monitor_dir)
        idle, idle_seen = self.wait_tray({"Idle"}, 30)
        if idle != "Idle":
            raise RuntimeError(f"soak: Idle non atteint, vu {idle_seen}")
        self.dbus["soak_start"] = status_notifier_items()
        if not self.dbus["soak_start"].get("registered"):
            raise RuntimeError("soak: tray non enregistré")

        initial_window, hidden_method = self.hide_window(self.gui_wid)
        if initial_window.get("hidden") is not True:
            raise RuntimeError(f"soak: fenêtre non cachée: {initial_window}")
        self.log(
            "soak: fenêtre cachée (%s, méthode=%s)",
            initial_window.get("map_state"),
            hidden_method,
        )

        command = [
            "systemd-inhibit",
            "--what=idle:sleep",
            "--who=poc-j4",
            "--why=endurance",
            "--mode=block",
            sys.executable,
            str(self.args.monitor),
            "--out",
            str(monitor_dir),
            "--gui-pid",
            str(self.gui_pid),
            "--window-wid",
            self.gui_wid,
            "--server-url",
            self.server_url,
            "--server-pid",
            str(self.server.pid) if self.server else "",
            "--tray-state-file",
            str(monitor_dir / "tray-state.json"),
            "--watcher-stats-file",
            str(monitor_dir / "watcher-stats.json"),
            "--duration",
            str(self.args.soak_seconds),
            "--interval",
            str(self.args.soak_interval),
            "--abort-vram",
            str(self.args.abort_vram),
        ]
        console = (monitor_dir / "console.log").open("w", encoding="utf-8")
        monitor_proc = subprocess.Popen(
            command,
            cwd=str(REPO),
            stdout=console,
            stderr=subprocess.STDOUT,
            text=True,
            start_new_session=True,
        )
        self.log("soak: monitor lancé pour %s s", self.args.soak_seconds)
        timed_out = False
        try:
            monitor_proc.wait(timeout=self.args.soak_seconds + 600)
        except subprocess.TimeoutExpired:
            timed_out = True
            try:
                os.killpg(monitor_proc.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                monitor_proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(monitor_proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        console.close()

        final_window = self.show_window(self.gui_wid)
        time.sleep(1)
        screenshot = capture(self.gui_wid, monitor_dir / "gui-final.png")
        self.dbus["soak_end"] = status_notifier_items()

        summary = read_json(monitor_dir / "summary.json") or {}
        soak_ok = bool(
            summary.get("acceptance", {}).get("acceptance_ok")
            and screenshot.get("ok")
            and not screenshot.get("blank")
            and not timed_out
        )
        self.stop_gui()
        self.stop_server()
        soak = {
            "duration_s": summary.get("duration_s"),
            "samples": summary.get("samples"),
            "expected_samples": summary.get("expected_samples"),
            "drift_ms": summary.get("drift_ms"),
            "deltas_kib": summary.get("deltas_kib"),
            "slopes_kib_per_min": summary.get("slopes_kib_per_min"),
            "rss_kib": summary.get("rss_kib"),
            "pss_kib": summary.get("pss_kib"),
            "window": summary.get("window"),
            "gui_alive_ratio": summary.get("gui_alive_ratio"),
            "pss_complete_ratio": summary.get("pss_complete_ratio"),
            "vram": summary.get("vram"),
            "watcher": summary.get("watcher"),
            "tray": summary.get("tray"),
            "logs_bytes": summary.get("logs_bytes"),
            "oom": summary.get("oom"),
            "acceptance": summary.get("acceptance"),
            "initial_window": initial_window,
            "hidden_method": hidden_method,
            "final_window": final_window,
            "screenshot": screenshot,
            "timed_out": timed_out,
            "dbus_item_start": self.dbus.get("soak_start", {}).get("item_path"),
            "dbus_item_end": self.dbus.get("soak_end", {}).get("item_path"),
            "dbus_stable": bool(
                self.dbus.get("soak_start", {}).get("item_path")
                and self.dbus.get("soak_start", {}).get("item_path")
                == self.dbus.get("soak_end", {}).get("item_path")
            ),
            "soak_ok": soak_ok,
        }
        write_json(directory / "summary.json", soak)
        self.soak = soak
        self.log(
            "soak terminé: ok=%s samples=%s drift_p95=%s host_delta=%s gui_delta=%s",
            soak_ok,
            soak.get("samples"),
            (soak.get("drift_ms") or {}).get("p95"),
            (soak.get("deltas_kib") or {}).get("host"),
            (soak.get("deltas_kib") or {}).get("gui"),
        )
        return soak

    def cleanup(self) -> dict[str, Any]:
        self.stop_gui()
        self.stop_server()
        time.sleep(1)
        rc, out, _ = run(["ss", "-ltnp"], timeout=5)
        ports_busy = [
            line.strip()
            for line in out.splitlines()
            if any(port in line for port in (":5003", ":5004", ":5005"))
        ]
        residuals = find_residuals()
        gnome_after = gnome_snapshot()
        token_absent = True
        token_path = ""
        if self.token:
            for path in self.out.rglob("*"):
                if not path.is_file() or path.suffix.lower() not in TEXT_SUFFIXES:
                    continue
                try:
                    if self.token in path.read_text(encoding="utf-8", errors="ignore"):
                        token_absent = False
                        token_path = str(path)
                        break
                except OSError:
                    continue
        cleanup = {
            "timestamp": utc_now(),
            "ports_busy": ports_busy,
            "residuals": residuals,
            "hardware_lock_held": lock_held(),
            "gnome_before": self.gnome_before,
            "gnome_after": gnome_after,
            "gnome_unchanged": self.gnome_before == gnome_after,
            "token_absent": token_absent,
            "token_path": token_path,
        }
        cleanup["cleanup_ok"] = bool(
            not ports_busy
            and not any(residuals.values())
            and not cleanup["hardware_lock_held"]
            and cleanup["gnome_unchanged"]
            and token_absent
        )
        write_json(self.out / "cleanup.json", cleanup)
        self.cleanup_data = cleanup
        self.log("cleanup: ok=%s ports=%s", cleanup["cleanup_ok"], ports_busy)
        return cleanup

    def write_acceptance(self, summary: dict[str, Any]) -> None:
        matrix = summary.get("matrix") or {}
        soak = summary.get("soak") or {}
        cleanup = summary.get("cleanup") or {}
        drift = soak.get("drift_ms") or {}
        deltas = soak.get("deltas_kib") or {}
        slopes = soak.get("slopes_kib_per_min") or {}
        vram = soak.get("vram") or {}
        logs = soak.get("logs_bytes") or {}
        tray = soak.get("tray") or {}
        acceptance = soak.get("acceptance") or {}

        def mib(value: Any) -> str:
            return "n/a" if value is None else f"{value / 1024:.1f}"

        lines = [
            "# J4 — Tray D-Bus, polling 1 Hz et endurance 2 h",
            "",
            f"- Début : {summary['started_at']}",
            f"- Fin : {summary['finished_at']}",
            f"- Durée : {summary['duration_s']} s",
            f"- Soak : {soak.get('duration_s')} s, {soak.get('samples')} échantillons",
            "",
            "## Matrice d'états",
            "",
            "| État | Observé | OK |",
            "|---|---|---|",
            f"| Idle | {matrix.get('idle_seen')} | {matrix.get('idle')} |",
            f"| Running | {matrix.get('running_seen')} | {matrix.get('running')} |",
            f"| Success | {matrix.get('success_seen')} | {matrix.get('success')} |",
            f"| Error | {matrix.get('error_seen')} | {matrix.get('error')} |",
            f"| Stopping | {matrix.get('stopping_seen')} | {matrix.get('stopping')} |",
            f"| Offline | {matrix.get('offline_seen')} | {matrix.get('offline')} |",
            f"| Reconnexion Idle | {matrix.get('reconnect_seen')} | {matrix.get('reconnect')} |",
            f"| Fermeture vers tray | {matrix.get('close_to_tray')} | {matrix.get('close_to_tray')} |",
            "",
            f"- Item D-Bus : `{matrix.get('dbus_item')}`",
            f"- Matrice OK : **{matrix.get('matrix_ok')}**",
            "",
            "## Soak",
            "",
            "| Critère | Valeur | Seuil | OK |",
            "|---|---:|---:|---|",
            f"| Échantillons | {soak.get('samples')} | {soak.get('expected_samples')} ±5 | {acceptance.get('samples_ok')} |",
            f"| Dérive p95 | {drift.get('p95')} ms | ≤ 50 ms | {acceptance.get('drift_ok')} |",
            f"| Dérive max | {drift.get('max')} ms | ≤ 250 ms | {acceptance.get('drift_ok')} |",
            f"| Fenêtre cachée | {(soak.get('window') or {}).get('hidden_ratio')} | ≥ 0,99 | {acceptance.get('hidden_ok')} |",
            f"| GUI vivant | {soak.get('gui_alive_ratio')} | ≥ 0,999 | {acceptance.get('gui_alive_ok')} |",
            f"| Delta RSS hôte | {mib(deltas.get('host'))} Mio | ≤ +12 Mio | {acceptance.get('host_rss_delta_ok', acceptance.get('host_rss_ok'))} |",
            f"| Pente RSS hôte | {mib(slopes.get('host'))} Mio/min | ≤ 0,08 | {acceptance.get('host_rss_slope_ok', acceptance.get('host_rss_ok'))} |",
            f"| Delta RSS GUI | {mib(deltas.get('gui'))} Mio | ≤ +32 Mio | {acceptance.get('gui_rss_delta_ok', acceptance.get('gui_rss_ok'))} |",
            f"| Pente RSS GUI | {mib(slopes.get('gui'))} Mio/min | ≤ 0,267 | {acceptance.get('gui_rss_slope_ok', acceptance.get('gui_rss_ok'))} |",
            f"| Delta PSS GUI | {mib(deltas.get('gui_pss'))} Mio | ≤ +32 Mio | {acceptance.get('gui_pss_ok')} |",
            f"| Delta RSS WebKitNetwork | {mib(deltas.get('network'))} Mio | ≤ +32 Mio | {acceptance.get('network_rss_ok', acceptance.get('webkit_ok'))} |",
            f"| Delta RSS WebKitWeb | {mib(deltas.get('web'))} Mio | ≤ +32 Mio | {acceptance.get('web_rss_ok', acceptance.get('webkit_ok'))} |",
            f"| Delta RSS serveur | {mib(deltas.get('server'))} Mio | ≤ +12 Mio | {acceptance.get('server_rss_ok')} |",
            f"| Delta MemAvailable | {mib(deltas.get('mem_available'))} Mio | ≥ -256 Mio | {acceptance.get('memory_ok')} |",
            f"| Delta swap | {mib(deltas.get('swap_used'))} Mio | ≤ +16 Mio | {acceptance.get('memory_ok')} |",
            f"| Logs hôte | {logs.get('host')} octets | ≤ 16 Mio | {acceptance.get('logs_ok')} |",
            f"| Logs serveur | {logs.get('server')} octets | ≤ 16 Mio | {acceptance.get('logs_ok')} |",
            f"| CSV endurance | {logs.get('endurance_csv')} octets | ≤ 8 Mio | {acceptance.get('logs_ok')} |",
            f"| Changements tray | {tray.get('changes')} | ≤ 20 | {acceptance.get('tray_ok')} |",
            f"| État final tray | {tray.get('final_state')} | Idle | {acceptance.get('tray_ok')} |",
            f"| Pic VRAM | {vram.get('peak_mib')} MiB | < 3900 MiB | {acceptance.get('vram_ok')} |",
            f"| VRAM libre min | {vram.get('free_min_mib')} MiB | > 196 MiB | {acceptance.get('vram_ok')} |",
            f"| OOM | {soak.get('oom')} | non | {acceptance.get('oom_free')} |",
            f"| PSS complet | {soak.get('pss_complete_ratio')} | ≥ 0,999 | {acceptance.get('pss_complete_ok')} |",
            f"| Serveur joignable | {acceptance.get('server_ok')} | oui | {acceptance.get('server_ok')} |",
            "",
            f"- Item D-Bus début soak : `{soak.get('dbus_item_start')}`",
            f"- Item D-Bus fin soak : `{soak.get('dbus_item_end')}`",
            f"- Icône D-Bus stable : **{soak.get('dbus_stable')}**",
            f"- Soak OK : **{soak.get('soak_ok')}**",
            "",
            "## Nettoyage",
            "",
            f"- Ports occupés : {cleanup.get('ports_busy')}",
            f"- Processus résiduels : {cleanup.get('residuals')}",
            f"- Verrou tenu : {cleanup.get('hardware_lock_held')}",
            f"- gsettings inchangés : {cleanup.get('gnome_unchanged')}",
            f"- Token absent : {cleanup.get('token_absent')}",
            f"- Nettoyage OK : **{cleanup.get('cleanup_ok')}**",
            "",
            f"## Acceptation globale : **{summary.get('acceptance_ok')}**",
            "",
        ]
        (self.out / "acceptance.md").write_text("\n".join(lines) + "\n", encoding="utf-8")

    def run(self) -> int:
        self.preflight()
        if not self.args.skip_matrix:
            self.matrix_phase()
        if not self.args.skip_soak:
            self.soak_phase()
        self.cleanup()
        acceptance_ok = bool(
            (self.args.skip_matrix or self.matrix.get("matrix_ok"))
            and (self.args.skip_soak or self.soak.get("soak_ok"))
            and self.cleanup_data.get("cleanup_ok")
        )
        summary = {
            "started_at": self.started_at,
            "finished_at": utc_now(),
            "duration_s": round(time.monotonic() - self.started, 3),
            "preflight": self.preflight_data,
            "dbus": self.dbus,
            "matrix": self.matrix,
            "soak": self.soak,
            "cleanup": self.cleanup_data,
            "acceptance_ok": acceptance_ok,
            "errors": self.errors,
        }
        write_json(self.out / "summary.json", summary)
        self.write_acceptance(summary)
        self.log(
            "J4 terminé: acceptance=%s matrix=%s soak=%s cleanup=%s",
            acceptance_ok,
            self.matrix.get("matrix_ok"),
            self.soak.get("soak_ok"),
            self.cleanup_data.get("cleanup_ok"),
        )
        if acceptance_ok or self.args.smoke:
            return 0
        return 10


def parse_args() -> Args:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--base-config", type=Path, default=DEFAULT_BASE_CONFIG)
    parser.add_argument("--gui-bin", type=Path, default=DEFAULT_GUI)
    parser.add_argument("--server-bin", type=Path, default=DEFAULT_SERVER)
    parser.add_argument("--cli-bin", type=Path, default=DEFAULT_CLI)
    parser.add_argument("--audit-js", type=Path, default=DEFAULT_AUDIT)
    parser.add_argument("--monitor", type=Path, default=DEFAULT_MONITOR)
    parser.add_argument("--matrix-hold", type=int, default=180)
    parser.add_argument("--soak-seconds", type=int, default=7200)
    parser.add_argument("--soak-interval", type=float, default=1.0)
    parser.add_argument("--abort-vram", type=int, default=3900)
    parser.add_argument("--skip-matrix", action="store_true")
    parser.add_argument("--skip-soak", action="store_true")
    parser.add_argument("--smoke", action="store_true")
    parser.add_argument("--display", default=os.environ.get("DISPLAY", ":0"))
    values = parser.parse_args()
    return Args(
        out=values.out,
        base_config=values.base_config,
        gui_bin=values.gui_bin,
        server_bin=values.server_bin,
        cli_bin=values.cli_bin,
        audit_js=values.audit_js,
        monitor=values.monitor,
        matrix_hold=values.matrix_hold,
        soak_seconds=values.soak_seconds,
        soak_interval=values.soak_interval,
        abort_vram=values.abort_vram,
        skip_matrix=values.skip_matrix,
        skip_soak=values.skip_soak,
        smoke=values.smoke,
        display=values.display,
    )


def main() -> int:
    args = parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        handlers=[
            logging.StreamHandler(sys.stdout),
            RotatingFileHandler(
                args.out / "runbook.log",
                maxBytes=8 * 1024 * 1024,
                backupCount=2,
                encoding="utf-8",
            ),
        ],
    )
    runbook = Runbook(args)

    def on_signal(signum: int, _frame: Any) -> None:
        runbook.log("signal %s reçu, nettoyage", signum)
        try:
            runbook.cleanup()
        finally:
            raise SystemExit(128 + signum)

    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGHUP, on_signal)
    try:
        return runbook.run()
    except Exception as error:  # noqa: BLE001
        runbook.errors.append(str(error))
        LOGGER.exception("J4 interrompu")
        try:
            runbook.cleanup()
        except Exception:  # noqa: BLE001
            LOGGER.exception("nettoyage après erreur impossible")
        write_json(
            args.out / "contingency.json",
            {"timestamp": utc_now(), "reason": str(error), "errors": runbook.errors},
        )
        return 11


if __name__ == "__main__":
    sys.exit(main())
