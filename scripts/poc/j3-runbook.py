#!/usr/bin/env python3
"""Runbook J3 : baseline Firefox, GUI Tauri, 5 runs STEP2 CUDA et acceptance.

Le script isole tous les chemins sous `--out`, démarre le serveur de test
`:5005` avec le webhook désactivé, mesure Firefox puis le GUI, et exécute
STEP2 sur une vidéo de 30 s. Il ne touche ni `runtime/` ni `/mnt/cache`.
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
from pathlib import Path
from typing import Any

import psutil

REPO = Path(__file__).resolve().parents[2]
DEFAULT_GUI = REPO / "target/release/workflow-gui-tauri"
DEFAULT_SERVER = REPO / "target/release/workflow-server"
DEFAULT_CLI = REPO / "target/release/workflow-cli"
DEFAULT_AUDIT = REPO / "scripts/poc/audit.js"
DEFAULT_FIREFOX = REPO / "scripts/poc/j3-firefox-baseline.sh"
DEFAULT_MONITOR = REPO / "scripts/poc/j3-monitor.py"
DEFAULT_BASE_CONFIG = Path("/tmp/opencode/poc-gui/config.toml")
TITLE = "Workflow Manager"
SERVER_URL = "http://127.0.0.1:5005"
LOCK_PATH = Path("/tmp/workflow-rs-hw.lock")
BINARY_SUFFIXES = {
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".webp",
    ".mp4",
    ".sqlite",
    ".sqlite3",
    ".db",
    ".gz",
    ".zip",
    ".xz",
    ".bz2",
}

LOGGER = logging.getLogger("j3")


class RunFailure(RuntimeError):
    def __init__(self, message: str, record: dict[str, Any] | None = None) -> None:
        super().__init__(message)
        self.record = record or {}


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="milliseconds")


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


def run_cmd(cmd: list[str], timeout: float = 10) -> tuple[int, str, str]:
    try:
        proc = subprocess.run(
            cmd, capture_output=True, text=True, timeout=timeout, check=False
        )
        return proc.returncode, proc.stdout.strip(), proc.stderr.strip()
    except Exception as error:  # pragma: no cover - dépend de l'environnement
        return 1, "", str(error)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


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


def http_get(url: str, headers: dict[str, str] | None = None, timeout: float = 2) -> tuple[int, str]:
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    request = urllib.request.Request(url, headers=headers or {})
    with opener.open(request, timeout=timeout) as response:
        return response.status, response.read().decode("utf-8", "replace")


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


def process_rows() -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for proc in psutil.process_iter(["pid", "name", "cmdline"]):
        try:
            cmdline = proc.info["cmdline"] or []
            rows.append(
                {
                    "pid": proc.info["pid"],
                    "name": proc.info["name"] or "",
                    "cmdline": cmdline,
                    "names": proc_names(proc),
                }
            )
        except psutil.Error:
            pass
    return rows


def find_firefox() -> list[dict[str, Any]]:
    result = []
    for row in process_rows():
        first = Path(row["cmdline"][0]).name if row["cmdline"] else ""
        if row["name"] in ("firefox", "firefox-bin") or first in ("firefox", "firefox-bin"):
            result.append(row)
    return result


def find_gui() -> list[dict[str, Any]]:
    result = []
    for row in process_rows():
        if any(name.startswith("workflow-gui") for name in row["names"]):
            result.append(row)
    return result


def find_step2() -> list[dict[str, Any]]:
    result = []
    for row in process_rows():
        first = Path(row["cmdline"][0]).name if row["cmdline"] else ""
        if first.startswith("workflow-cli") and "step2" in row["cmdline"]:
            result.append(row)
    return result


def session_processes(sid: int) -> list[dict[str, Any]]:
    result = []
    for row in process_rows():
        try:
            if os.getsid(row["pid"]) == sid:
                result.append(row)
        except (ProcessLookupError, PermissionError, OSError):
            pass
    return result


def kill_group(sid: int | None, sig: int) -> None:
    if not sid:
        return
    try:
        os.killpg(sid, sig)
    except (ProcessLookupError, PermissionError):
        pass


def lock_is_held(path: Path = LOCK_PATH) -> bool:
    """Teste le verrou flock sans le créer ni le modifier."""
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


def gsettings_get(schema: str, key: str) -> str:
    rc, out, err = run_cmd(["gsettings", "get", schema, key], timeout=5)
    if rc != 0:
        return f"ERROR: {err}"
    return out.strip()


def current_desktop() -> int | None:
    rc, out, _ = run_cmd(["wmctrl", "-d"], timeout=5)
    if rc != 0:
        return None
    for line in out.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "*":
            try:
                return int(parts[0])
            except ValueError:
                return None
    return None


def gnome_snapshot() -> dict[str, Any]:
    return {
        "dynamic-workspaces": gsettings_get("org.gnome.mutter", "dynamic-workspaces"),
        "num-workspaces": gsettings_get(
            "org.gnome.desktop.wm.preferences", "num-workspaces"
        ),
        "current-desktop": current_desktop(),
    }


def find_window(title: str = TITLE) -> list[str]:
    rc, out, _ = run_cmd(["wmctrl", "-l"], timeout=5)
    if rc != 0:
        return []
    return [line.split()[0] for line in out.splitlines() if title in line]


def window_geometry(wid: str) -> dict[str, Any]:
    rc, out, err = run_cmd(["xwininfo", "-id", wid], timeout=5)
    result: dict[str, Any] = {"ok": rc == 0}
    if rc != 0:
        result["error"] = err
        return result
    for key, label in [
        ("width", "Width:"),
        ("height", "Height:"),
        ("x", "Absolute upper-left X:"),
        ("y", "Absolute upper-left Y:"),
    ]:
        for line in out.splitlines():
            if label in line:
                try:
                    result[key] = int(line.split(label, 1)[1].strip())
                except ValueError:
                    pass
    return result


def set_geometry(wid: str) -> None:
    rc, _, _ = run_cmd(
        ["wmctrl", "-i", "-r", wid, "-e", "0,240,109,1440,900"], timeout=5
    )
    if rc != 0:
        run_cmd(["xdotool", "windowsize", wid, "1440", "900"], timeout=5)
        run_cmd(["xdotool", "windowmove", wid, "240", "109"], timeout=5)
    run_cmd(["wmctrl", "-i", "-a", wid], timeout=5)


def capture(wid: str, path: Path) -> dict[str, Any]:
    path.parent.mkdir(parents=True, exist_ok=True)
    rc, _, err = run_cmd(["import", "-window", wid, str(path)], timeout=10)
    if rc != 0 or not path.exists():
        return {"ok": False, "path": str(path), "error": err}
    rc, out, err = run_cmd(
        [
            "identify",
            "-format",
            "%w %h %[fx:mean] %[fx:standard_deviation] %k",
            str(path),
        ],
        timeout=10,
    )
    if rc != 0:
        return {"ok": False, "path": str(path), "error": err}
    parts = out.strip().split()
    try:
        width, height = int(parts[0]), int(parts[1])
        mean, stddev, colors = float(parts[2]), float(parts[3]), int(parts[4])
    except (IndexError, ValueError) as error:
        return {"ok": False, "path": str(path), "error": f"identify: {error}: {out!r}"}
    stddev_value: float | None = None if math.isnan(stddev) else stddev
    blank = ""
    if stddev_value is None or colors <= 1:
        blank = "solid"
    elif mean > 0.95 and stddev_value < 0.02:
        blank = "white"
    elif mean < 0.05 and stddev_value < 0.02:
        blank = "black"
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


@dataclass
class RunbookArgs:
    out: Path
    base_config: Path
    gui_bin: Path
    server_bin: Path
    cli_bin: Path
    audit_js: Path
    firefox_script: Path
    monitor: Path
    runs: int
    gui_profile: str
    step2_batch: int
    cuda_lazy: bool
    firefox_seconds: int
    settle_seconds: int
    gui_baseline_seconds: int
    step2_timeout: int
    abort_vram: int
    display: str


class Runbook:
    def __init__(self, args: RunbookArgs) -> None:
        self.args = args
        self.out = args.out.resolve()
        self.state = self.out / "state"
        self.runs_dir = self.out / "runs"
        self.server_proc: subprocess.Popen[str] | None = None
        self.server_stdout: Any = None
        self.server_stderr: Any = None
        self.server_token = ""
        self.server_token_sha = ""
        self.gui_proc: subprocess.Popen[str] | None = None
        self.gui_stdout: Any = None
        self.gui_stderr: Any = None
        self.gui_pid: int | None = None
        self.gui_wid: str | None = None
        self.firefox: dict[str, Any] | None = None
        self.gui_baseline: dict[str, Any] | None = None
        self.records: list[dict[str, Any]] = []
        self.errors: list[str] = []
        self.cleanup_done = False
        self.idle_vram: int | None = None
        self.gui_baseline_vram: int | None = None
        self.reference_csv_sha: str | None = None
        self.gnome_before: dict[str, Any] = {}
        self.gnome_after: dict[str, Any] = {}
        self.preflight_data: dict[str, Any] = {}
        self.config_paths: dict[str, Any] = {}
        self.video_info: dict[str, Any] = {}
        self.started_at = utc_now()
        self.started_monotonic = time.monotonic()

    def log(self, message: str, *args: Any) -> None:
        LOGGER.info(message, *args)

    def preflight(self) -> None:
        errors: list[str] = []
        for name, path in [
            ("gui", self.args.gui_bin),
            ("server", self.args.server_bin),
            ("cli", self.args.cli_bin),
            ("audit", self.args.audit_js),
            ("firefox", self.args.firefox_script),
            ("monitor", self.args.monitor),
            ("base_config", self.args.base_config),
        ]:
            if not path.exists():
                errors.append(f"fichier absent ({name}): {path}")
        for tool in (
            "ffmpeg",
            "ffprobe",
            "nvidia-smi",
            "fuser",
            "wmctrl",
            "xdotool",
            "import",
            "identify",
            "xprop",
            "gsettings",
            "ss",
        ):
            if shutil.which(tool) is None:
                errors.append(f"outil absent: {tool}")
        if not os.environ.get("DISPLAY"):
            errors.append("DISPLAY absent")
        out_text = str(self.out)
        if out_text.startswith(str(REPO / "runtime")) or out_text.startswith("/mnt/cache"):
            errors.append(f"sortie interdite: {self.out}")

        rc, out, err = run_cmd(["ss", "-ltnp"], timeout=5)
        if rc != 0:
            errors.append(f"ss: {err}")
        else:
            for line in out.splitlines():
                if any(port in line for port in (":5003", ":5004", ":5005")):
                    errors.append(f"port occupé: {line.strip()}")

        for label, rows in [
            ("firefox", find_firefox()),
            ("gui", find_gui()),
            ("step2", find_step2()),
        ]:
            if rows:
                errors.append(f"processus {label} résiduel: {rows}")
        if lock_is_held():
            errors.append(f"verrou matériel tenu: {LOCK_PATH}")

        used, free, total, vram_error = query_vram()
        if vram_error:
            errors.append(f"nvidia-smi: {vram_error}")
        if free is not None and free < 2400:
            errors.append(f"VRAM libre insuffisante: {free} MiB")
        rc, compute_out, compute_err = run_cmd(
            [
                "nvidia-smi",
                "--query-compute-apps=pid,process_name,used_memory",
                "--format=csv,noheader",
            ],
            timeout=5,
        )
        if rc == 0 and compute_out.strip():
            errors.append(f"processus compute GPU résiduel: {compute_out.strip()}")
        if rc != 0:
            errors.append(f"nvidia-smi compute: {compute_err}")

        self.gnome_before = gnome_snapshot()
        self.preflight_data = {
            "timestamp": utc_now(),
            "out": str(self.out),
            "display": self.args.display,
            "vram": {"used_mib": used, "free_mib": free, "total_mib": total},
            "gnome_before": self.gnome_before,
            "errors": errors,
        }
        write_json(self.out / "preflight.json", self.preflight_data)
        if errors:
            raise RuntimeError("préflight J3 échoué:\n- " + "\n- ".join(errors))
        self.log("préflight ok (VRAM libre %s MiB)", free)

    def prepare(self) -> None:
        for directory in (
            self.out,
            self.state,
            self.state / "projects",
            self.state / "archives",
            self.state / "logs",
            self.state / "downloads",
            self.state / "cache",
            self.state / "finalized",
            self.out / "media",
            self.out / "gui",
            self.out / "firefox-baseline",
            self.runs_dir,
        ):
            directory.mkdir(parents=True, exist_ok=True)
        self.write_config()
        self.make_video()
        self.prepare_runs()
        write_json(
            self.out / "profile.json",
            {
                "timestamp": utc_now(),
                "gui_profile": self.args.gui_profile,
                "step2_batch": self.args.step2_batch,
                "cuda_lazy": self.args.cuda_lazy,
                "runs": self.args.runs,
                "abort_vram_mib": self.args.abort_vram,
                "firefox_seconds": self.args.firefox_seconds,
                "gui_baseline_seconds": self.args.gui_baseline_seconds,
            },
        )

    def write_config(self) -> None:
        base = self.args.base_config.read_text(encoding="utf-8")
        state_text = str(self.state)
        out_text = str(self.out)
        base = base.replace("/tmp/opencode/poc-gui/state", state_text)
        base = base.replace(
            'workdir = "/tmp/opencode/poc-gui"', f'workdir = "{out_text}"'
        )
        base = base.replace(
            'source_dir = "/home/kidpixel/workflow-rs/runtime/downloads"',
            f'source_dir = "{state_text}/downloads"',
        )
        base = re.sub(r"(?m)^port = \d+$", "port = 5005", base, count=1)
        # Le webhook doit rester désactivé pour tout le runbook.
        base = re.sub(
            r"(?ms)(\[webhook\].*?)(?=^\[|\Z)",
            lambda match: match.group(0)
            .replace("enabled = true", "enabled = false")
            .replace("dry_run = false", "dry_run = true"),
            base,
        )
        config_path = self.out / "config.toml"
        config_path.write_text(base, encoding="utf-8")
        lines = base.splitlines()
        bad = []
        for line in lines:
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
            ):
                if "/mnt/cache" in stripped or "workflow-rs/runtime" in stripped:
                    bad.append(stripped)
        self.config_paths = {
            "config": str(config_path),
            "state": state_text,
            "bad_assignments": bad,
            "webhook_disabled": "enabled = false" in base,
            "dry_run": "dry_run = true" in base,
        }
        write_json(self.out / "config-paths.json", self.config_paths)
        if bad:
            raise RuntimeError(f"chemins de production dans la config J3: {bad}")
        if not self.config_paths["webhook_disabled"] or not self.config_paths["dry_run"]:
            raise RuntimeError("webhook J3 non désactivé ou dry_run absent")

    def make_video(self) -> None:
        video = self.out / "media" / "clip-30s.mp4"
        rc, _, err = run_cmd(
            [
                "ffmpeg",
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x720:rate=25:duration=30",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                "18",
                "-pix_fmt",
                "yuv420p",
                str(video),
            ],
            timeout=120,
        )
        if rc != 0 or not video.exists():
            raise RuntimeError(f"ffmpeg testsrc2: {err}")
        rc, out, err = run_cmd(
            [
                "ffprobe",
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_entries",
                "stream=width,height,r_frame_rate,nb_frames,duration",
                "-of",
                "json",
                str(video),
            ],
            timeout=30,
        )
        if rc != 0:
            raise RuntimeError(f"ffprobe: {err}")
        payload = json.loads(out)
        stream = payload["streams"][0]
        info = {
            "path": str(video),
            "width": int(stream["width"]),
            "height": int(stream["height"]),
            "fps": stream.get("r_frame_rate"),
            "nb_frames": int(stream.get("nb_frames", 0)),
            "duration": float(stream.get("duration", 0)),
            "sha256": sha256_file(video),
        }
        if info["width"] != 1280 or info["height"] != 720:
            raise RuntimeError(f"vidéo J3 non 1280x720: {info}")
        if abs(info["duration"] - 30.0) > 0.2 or info["nb_frames"] != 750:
            raise RuntimeError(f"vidéo J3 non 30 s / 750 trames: {info}")
        self.video_info = info
        write_json(self.out / "video.json", info)
        self.log(
            "vidéo prête: %sx%s, %s trames, %.3f s",
            info["width"],
            info["height"],
            info["nb_frames"],
            info["duration"],
        )

    def prepare_runs(self) -> None:
        source = self.out / "media" / "clip-30s.mp4"
        for number in range(1, self.args.runs + 1):
            input_dir = self.runs_dir / f"run-{number:02d}" / "input"
            input_dir.mkdir(parents=True, exist_ok=True)
            target = input_dir / "clip.mp4"
            shutil.copyfile(source, target)
            csv_path = input_dir / "clip.csv"
            if csv_path.exists():
                csv_path.unlink()

    def start_server(self) -> None:
        token = os.urandom(32).hex()
        secret = os.urandom(32).hex()
        self.server_token = token
        self.server_token_sha = hashlib.sha256(token.encode()).hexdigest()
        env = os.environ.copy()
        for key in (
            "WORKFLOW_PATHS_BASE",
            "WORKFLOW_SERVER_PORT",
            "WORKFLOW_WEBHOOK_ENABLED",
            "WORKFLOW_WEBHOOK_URL",
        ):
            env.pop(key, None)
        env["WORKFLOW_SERVER_WORKER_TOKEN"] = token
        env["WORKFLOW_SERVER_SECRET_KEY"] = secret
        self.server_stdout = (self.out / "server.log").open("w", encoding="utf-8")
        self.server_stderr = (self.out / "server.err").open("w", encoding="utf-8")
        self.server_proc = subprocess.Popen(
            [str(self.args.server_bin), "--config", str(self.out / "config.toml")],
            cwd=str(REPO),
            env=env,
            stdout=self.server_stdout,
            stderr=self.server_stderr,
            text=True,
            start_new_session=True,
        )
        deadline = time.monotonic() + 30
        body = ""
        while time.monotonic() < deadline:
            if self.server_proc.poll() is not None:
                raise RuntimeError(
                    f"serveur arrêté (rc={self.server_proc.returncode}); "
                    f"voir {self.out / 'server.err'}"
                )
            try:
                status, body = http_get(SERVER_URL + "/", timeout=1)
                if status == 200:
                    break
            except (urllib.error.URLError, TimeoutError, OSError):
                pass
            time.sleep(0.2)
        else:
            raise RuntimeError("serveur: GET / indisponible dans les 30 s")
        match = re.search(r'<meta name="worker-token" content="([^"]+)"', body)
        if not match:
            raise RuntimeError("serveur: meta worker-token absente")
        if match.group(1) != token:
            raise RuntimeError("serveur: token HTML différent du token de lancement")
        try:
            status, ping = http_get(
                SERVER_URL + "/api/ping",
                headers={"X-Worker-Token": token},
                timeout=3,
            )
        except urllib.error.HTTPError as error:
            raise RuntimeError(f"serveur: /api/ping HTTP {error.code}") from error
        if status != 200 or '"runtime":"rust"' not in ping:
            raise RuntimeError(f"serveur: /api/ping inattendu: {status} {ping!r}")
        write_json(
            self.out / "server.json",
            {
                "timestamp": utc_now(),
                "pid": self.server_proc.pid,
                "url": SERVER_URL,
                "config": str(self.out / "config.toml"),
                "token_sha256": self.server_token_sha,
            },
        )
        self.log("serveur prêt sur %s (pid=%s)", SERVER_URL, self.server_proc.pid)

    def measure_idle(self) -> None:
        samples: list[int] = []
        for _ in range(10):
            used, free, total, error = query_vram()
            if error or used is None:
                raise RuntimeError(f"VRAM idle: {error}")
            samples.append(used)
            time.sleep(1)
        payload = {
            "timestamp": utc_now(),
            "used_mib": stats(samples),
            "samples": samples,
        }
        write_json(self.out / "vram-idle.json", payload)
        self.idle_vram = int(statistics.median(samples))
        self.log("VRAM idle médiane: %s MiB", self.idle_vram)

    def run_firefox(self) -> None:
        env = os.environ.copy()
        env["SERVER_URL"] = SERVER_URL + "/"
        env["BASELINE_SECONDS"] = str(self.args.firefox_seconds)
        env["SETTLE_SECONDS"] = str(self.args.settle_seconds)
        firefox_out = self.out / "firefox-baseline"
        proc = subprocess.run(
            [str(self.args.firefox_script), str(firefox_out)],
            cwd=str(REPO),
            env=env,
            capture_output=True,
            text=True,
            timeout=self.args.firefox_seconds + self.args.settle_seconds + 120,
            check=False,
        )
        (firefox_out / "console.log").write_text(
            proc.stdout + ("\n" if proc.stdout and not proc.stdout.endswith("\n") else "")
            + proc.stderr,
            encoding="utf-8",
        )
        if proc.returncode != 0:
            cleanup = read_json(firefox_out / "firefox-cleanup.json")
            raise RuntimeError(
                f"baseline Firefox rc={proc.returncode}; cleanup={cleanup}; "
                f"voir {firefox_out / 'console.log'}"
            )
        summary = read_json(firefox_out / "monitor" / "summary.json")
        cleanup = read_json(firefox_out / "firefox-cleanup.json")
        window = read_json(firefox_out / "firefox-window.json")
        if not summary or not cleanup or not window:
            raise RuntimeError("baseline Firefox: artefacts incomplets")
        if not cleanup.get("ok"):
            raise RuntimeError(f"baseline Firefox: nettoyage incomplet: {cleanup}")
        if window.get("screenshot", {}).get("blank"):
            raise RuntimeError(f"baseline Firefox: capture invalide: {window}")
        leftovers = find_firefox()
        if leftovers:
            raise RuntimeError(f"baseline Firefox: processus résiduels: {leftovers}")
        self.firefox = {
            "summary": summary,
            "cleanup": cleanup,
            "window": window,
            "vram_delta_mib": (
                summary["vram"]["used_mib"]["median"] - self.idle_vram
                if self.idle_vram is not None
                else None
            ),
        }
        write_json(self.out / "firefox-baseline.json", self.firefox)
        self.log(
            "Firefox baseline: RSS médian %.1f MiB, PSS médian %.1f MiB, VRAM pic %s MiB",
            summary["target"]["rss_kib"]["median"] / 1024,
            summary["target"]["pss_kib"]["median"] / 1024,
            summary["vram"]["peak_mib"],
        )

    def start_gui(self) -> None:
        env = os.environ.copy()
        env["DISPLAY"] = self.args.display
        env["GDK_BACKEND"] = "x11"
        if self.args.gui_profile == "disable-dmabuf":
            env["WEBKIT_DISABLE_DMABUF_RENDERER"] = "1"
        self.gui_stdout = (self.out / "gui" / "stdout.log").open("w", encoding="utf-8")
        self.gui_stderr = (self.out / "gui" / "stderr.log").open("w", encoding="utf-8")
        self.gui_proc = subprocess.Popen(
            [
                str(self.args.gui_bin),
                "--server-url",
                SERVER_URL,
                "--telemetry-dir",
                str(self.out / "gui"),
                "--ready-file",
                str(self.out / "gui" / "ready"),
                "--poc-driver",
                str(self.args.audit_js),
                "--log-level",
                "info",
            ],
            cwd=str(REPO),
            env=env,
            stdout=self.gui_stdout,
            stderr=self.gui_stderr,
            text=True,
            start_new_session=True,
        )
        self.gui_pid = self.gui_proc.pid
        deadline = time.monotonic() + 45
        wid = ""
        while time.monotonic() < deadline:
            if self.gui_proc.poll() is not None:
                raise RuntimeError(
                    f"GUI arrêté (rc={self.gui_proc.returncode}); "
                    f"voir {self.out / 'gui' / 'stderr.log'}"
                )
            windows = find_window()
            if len(windows) == 1:
                wid = windows[0]
                break
            if len(windows) > 1:
                raise RuntimeError(f"GUI: plusieurs fenêtres: {windows}")
            time.sleep(0.2)
        if not wid:
            raise RuntimeError("GUI: fenêtre Workflow Manager absente")
        self.gui_wid = wid
        set_geometry(wid)
        deadline = time.monotonic() + 45
        while time.monotonic() < deadline:
            if self.gui_proc.poll() is not None:
                raise RuntimeError(
                    f"GUI arrêté avant audit (rc={self.gui_proc.returncode})"
                )
            ready = (self.out / "gui" / "ready").exists()
            audit = (self.out / "gui" / "audit.json").exists()
            token_check = (self.out / "gui" / "token-check.json").exists()
            if ready and audit and token_check:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("GUI: ready/audit/token-check absents dans les 45 s")
        audit = read_json(self.out / "gui" / "audit.json")
        token_check = read_json(self.out / "gui" / "token-check.json")
        if not audit or not token_check:
            raise RuntimeError("GUI: audit ou token-check illisible")
        if token_check.get("status") != "ok":
            raise RuntimeError(f"GUI: token-check non ok: {token_check}")
        if audit.get("token_sha256") != self.server_token_sha:
            raise RuntimeError(f"GUI: digest token différent: {audit.get('token_sha256')}")
        time.sleep(1)
        screenshot = capture(wid, self.out / "gui" / "gui-final.png")
        if not screenshot.get("ok") or screenshot.get("blank"):
            raise RuntimeError(f"GUI: capture invalide: {screenshot}")
        payload = {
            "timestamp": utc_now(),
            "pid": self.gui_pid,
            "wid": wid,
            "geometry": window_geometry(wid),
            "screenshot": screenshot,
            "audit": audit,
            "token_check": token_check,
        }
        write_json(self.out / "gui-start.json", payload)
        self.log("GUI prêt (pid=%s, wid=%s)", self.gui_pid, wid)

    def run_gui_baseline(self) -> None:
        out = self.out / "gui-baseline" / "monitor"
        out.mkdir(parents=True, exist_ok=True)
        console = (self.out / "gui-baseline" / "console.log").open(
            "w", encoding="utf-8"
        )
        proc = subprocess.run(
            [
                sys.executable,
                str(self.args.monitor),
                "--role",
                "gui",
                "--root-pid",
                str(self.gui_pid),
                "--phase",
                "gui-baseline",
                "--duration",
                str(self.args.gui_baseline_seconds),
                "--interval",
                "1",
                "--abort-vram",
                str(self.args.abort_vram),
                "--out",
                str(out),
            ],
            cwd=str(REPO),
            stdout=console,
            stderr=subprocess.STDOUT,
            text=True,
            timeout=self.args.gui_baseline_seconds + 60,
            check=False,
        )
        console.close()
        summary = read_json(out / "summary.json")
        if proc.returncode != 0 or not summary:
            raise RuntimeError(
                f"baseline GUI rc={proc.returncode}; voir {self.out / 'gui-baseline'}"
            )
        if summary["gui"]["contract_ok"] is not True:
            raise RuntimeError(f"baseline GUI: contrat 3 processus non tenu: {summary['gui']}")
        screenshot = capture(self.gui_wid or "", self.out / "gui-baseline" / "gui-final.png")
        if not screenshot.get("ok") or screenshot.get("blank"):
            raise RuntimeError(f"baseline GUI: capture invalide: {screenshot}")
        self.gui_baseline = {
            "summary": summary,
            "screenshot": screenshot,
            "vram_delta_mib": (
                summary["vram"]["used_mib"]["median"] - self.idle_vram
                if self.idle_vram is not None
                else None
            ),
        }
        self.gui_baseline_vram = int(summary["vram"]["used_mib"]["median"])
        write_json(self.out / "gui-baseline.json", self.gui_baseline)
        self.log(
            "GUI baseline: RSS médian %.1f MiB, PSS médian %.1f MiB, VRAM médiane %s MiB",
            summary["target"]["rss_kib"]["median"] / 1024,
            summary["target"]["pss_kib"]["median"] / 1024,
            self.gui_baseline_vram,
        )

    def step2_command(self, input_dir: Path) -> list[str]:
        return [
            str(self.args.cli_bin),
            "--config",
            str(self.out / "config.toml"),
            "step2",
            "detect",
            "--input-dir",
            str(input_dir),
            "--force",
            "--device",
            "cuda",
            "--batch-size",
            str(self.args.step2_batch),
        ]

    def scan_oom(self, paths: list[Path]) -> tuple[bool, list[str]]:
        pattern = re.compile(
            r"CUDA_ERROR_OUT_OF_MEMORY|cudaErrorMemoryAllocation|out of memory|OOM",
            re.IGNORECASE,
        )
        matches: list[str] = []
        for path in paths:
            if not path.exists():
                continue
            try:
                for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
                    if pattern.search(line):
                        matches.append(f"{path}: {line}")
            except OSError:
                pass
        return bool(matches), matches

    def run_step2(self, number: int) -> dict[str, Any]:
        run_dir = self.runs_dir / f"run-{number:02d}"
        input_dir = run_dir / "input"
        run_dir.mkdir(parents=True, exist_ok=True)
        stdout_path = run_dir / "step2.stdout.log"
        stderr_path = run_dir / "step2.stderr.log"
        monitor_out = run_dir / "monitor"
        monitor_out.mkdir(parents=True, exist_ok=True)
        if self.gui_proc is None or self.gui_proc.poll() is not None:
            raise RunFailure("GUI absent avant STEP2")
        if not find_window():
            raise RunFailure("fenêtre GUI absente avant STEP2")

        env = os.environ.copy()
        env["DISPLAY"] = self.args.display
        env["GDK_BACKEND"] = "x11"
        for key in (
            "WORKFLOW_PATHS_BASE",
            "WORKFLOW_SERVER_PORT",
            "WORKFLOW_WEBHOOK_ENABLED",
            "WORKFLOW_WEBHOOK_URL",
        ):
            env.pop(key, None)
        if self.args.cuda_lazy:
            env["CUDA_MODULE_LOADING"] = "LAZY"
        command = self.step2_command(input_dir)
        started = time.monotonic()
        started_at = utc_now()
        cli_stdout = stdout_path.open("w", encoding="utf-8")
        cli_stderr = stderr_path.open("w", encoding="utf-8")
        cli_proc = subprocess.Popen(
            command,
            cwd=str(REPO),
            env=env,
            stdout=cli_stdout,
            stderr=cli_stderr,
            text=True,
            start_new_session=True,
        )
        monitor_console = (run_dir / "monitor-console.log").open("w", encoding="utf-8")
        monitor_cmd = [
            sys.executable,
            str(self.args.monitor),
            "--role",
            "step2",
            "--gui-pid",
            str(self.gui_pid),
            "--step2-pid",
            str(cli_proc.pid),
            "--phase",
            "step2",
            "--run",
            str(number),
            "--duration",
            "0",
            "--interval",
            "1",
            "--settle",
            "2",
            "--abort-vram",
            str(self.args.abort_vram),
            "--baseline-vram",
            str(self.gui_baseline_vram or 0),
            "--out",
            str(monitor_out),
        ]
        monitor_proc = subprocess.Popen(
            monitor_cmd,
            cwd=str(REPO),
            stdout=monitor_console,
            stderr=subprocess.STDOUT,
            text=True,
            start_new_session=True,
        )
        timeout = False
        try:
            cli_rc = cli_proc.wait(timeout=self.args.step2_timeout)
        except subprocess.TimeoutExpired:
            timeout = True
            cli_rc = -9
            kill_group(cli_proc.pid, signal.SIGTERM)
            time.sleep(2)
            kill_group(cli_proc.pid, signal.SIGKILL)
            try:
                cli_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
        cli_stdout.close()
        cli_stderr.close()
        try:
            monitor_rc = monitor_proc.wait(timeout=60)
        except subprocess.TimeoutExpired:
            monitor_proc.terminate()
            try:
                monitor_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                monitor_proc.kill()
                monitor_proc.wait(timeout=5)
            monitor_rc = -9
        monitor_console.close()
        duration = round(time.monotonic() - started, 3)
        summary = read_json(monitor_out / "summary.json")
        if not summary:
            raise RunFailure(f"run {number}: summary monitor absent", {"run": number})

        logs = sorted(
            (self.state / "logs" / "step2").glob("transnet_pytorch_*.log"),
            key=lambda path: path.stat().st_mtime,
        )
        step2_log = None
        if logs:
            step2_log = run_dir / "step2.log"
            shutil.copy2(logs[-1], step2_log)

        oom, oom_matches = self.scan_oom(
            [stdout_path, stderr_path, step2_log] if step2_log else [stdout_path, stderr_path]
        )
        csv_path = input_dir / "clip.csv"
        csv_sha = sha256_file(csv_path) if csv_path.exists() else ""
        csv_ok = False
        if csv_sha:
            if self.reference_csv_sha is None:
                self.reference_csv_sha = csv_sha
                csv_ok = True
            else:
                csv_ok = csv_sha == self.reference_csv_sha

        screenshot = capture(
            self.gui_wid or "", run_dir / f"gui-run-{number:02d}.png"
        )
        gui_alive = self.gui_proc.poll() is None and bool(find_window())
        leftovers = session_processes(cli_proc.pid)
        leftover_names = sorted({name for row in leftovers for name in row["names"]})
        if leftovers:
            kill_group(cli_proc.pid, signal.SIGKILL)
            time.sleep(0.5)
        time.sleep(2)
        cooldown_used, cooldown_free, cooldown_total, cooldown_error = query_vram()

        monitor_gui = summary.get("gui") or {}
        monitor_step2 = summary.get("step2") or {}
        vram_exceeded = bool(
            summary.get("vram_exceeded") or monitor_rc == 10
        )
        run_ok = bool(
            cli_rc == 0
            and not timeout
            and not oom
            and not vram_exceeded
            and monitor_rc == 0
            and monitor_gui.get("contract_ok") is True
            and gui_alive
            and screenshot.get("ok")
            and not screenshot.get("blank")
            and csv_path.exists()
            and csv_ok
        )
        record = {
            "run": number,
            "started_at": started_at,
            "finished_at": utc_now(),
            "duration_s": duration,
            "command": command,
            "env_cuda_lazy": self.args.cuda_lazy,
            "exit_code": cli_rc,
            "timeout": timeout,
            "monitor_rc": monitor_rc,
            "csv": {
                "path": str(csv_path),
                "sha256": csv_sha,
                "exists": csv_path.exists(),
                "matches_reference": csv_ok,
            },
            "oom": oom,
            "oom_matches": oom_matches,
            "vram_exceeded": vram_exceeded,
            "monitor": summary,
            "gui_alive": gui_alive,
            "gui_screenshot": screenshot,
            "leftovers": leftovers,
            "leftover_names": leftover_names,
            "cooldown_vram": {
                "used_mib": cooldown_used,
                "free_mib": cooldown_free,
                "total_mib": cooldown_total,
                "error": cooldown_error,
            },
            "run_ok": run_ok,
        }
        write_json(run_dir / "run.json", record)
        self.records.append(record)
        self.log(
            "run %02d: exit=%s csv=%s oom=%s pic_vram=%s MiB "
            "gui_rss_max=%.1f MiB step2_rss_max=%.1f MiB",
            number,
            cli_rc,
            "ok" if csv_ok else "FAIL",
            oom,
            summary["vram"]["peak_mib"],
            (monitor_gui.get("rss_kib") or {}).get("max", 0) / 1024,
            (monitor_step2.get("rss_kib") or {}).get("max", 0) / 1024,
        )
        if not run_ok:
            reasons = []
            if cli_rc != 0:
                reasons.append(f"exit={cli_rc}")
            if timeout:
                reasons.append("timeout")
            if oom:
                reasons.append("OOM")
            if vram_exceeded:
                reasons.append("VRAM>=3900")
            if monitor_rc != 0:
                reasons.append(f"monitor_rc={monitor_rc}")
            if monitor_gui.get("contract_ok") is not True:
                reasons.append("contrat GUI")
            if not gui_alive:
                reasons.append("GUI mort")
            if not screenshot.get("ok") or screenshot.get("blank"):
                reasons.append("capture GUI")
            if not csv_path.exists():
                reasons.append("CSV absent")
            elif not csv_ok:
                reasons.append("CSV différent")
            raise RunFailure(f"run {number}: " + ", ".join(reasons), record)
        return record

    def stop_gui(self) -> dict[str, Any]:
        result: dict[str, Any] = {"exit_code": None, "forced_kill": False}
        if self.gui_proc is None:
            return result
        if self.gui_wid:
            run_cmd(["wmctrl", "-i", "-c", self.gui_wid], timeout=5)
        try:
            result["exit_code"] = self.gui_proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            result["forced_kill"] = True
            kill_group(self.gui_pid, signal.SIGTERM)
            try:
                result["exit_code"] = self.gui_proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                kill_group(self.gui_pid, signal.SIGKILL)
                try:
                    result["exit_code"] = self.gui_proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    pass
        self.gui_proc = None
        self.gui_wid = None
        return result

    def stop_server(self) -> None:
        if self.server_proc is None:
            return
        if self.server_proc.poll() is None:
            kill_group(self.server_proc.pid, signal.SIGTERM)
            try:
                self.server_proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                kill_group(self.server_proc.pid, signal.SIGKILL)
                try:
                    self.server_proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    pass
        self.server_proc = None
        if self.server_stdout:
            self.server_stdout.close()
            self.server_stdout = None
        if self.server_stderr:
            self.server_stderr.close()
            self.server_stderr = None

    def cleanup(self) -> None:
        if self.cleanup_done:
            return
        self.cleanup_done = True
        errors: list[str] = []
        gui_result: dict[str, Any] = {}
        try:
            gui_result = self.stop_gui()
        except Exception as error:  # noqa: BLE001
            errors.append(f"stop_gui: {error}")
        try:
            self.stop_server()
        except Exception as error:  # noqa: BLE001
            errors.append(f"stop_server: {error}")
        if self.gui_stdout:
            self.gui_stdout.close()
            self.gui_stdout = None
        if self.gui_stderr:
            self.gui_stderr.close()
            self.gui_stderr = None
        time.sleep(1)
        rc, ports_out, ports_err = run_cmd(["ss", "-ltnp"], timeout=5)
        ports_busy = [
            line.strip()
            for line in ports_out.splitlines()
            if any(port in line for port in (":5003", ":5004", ":5005"))
        ]
        leftovers = find_gui() + find_step2() + find_firefox()
        self.gnome_after = gnome_snapshot()
        lock_held = lock_is_held()
        payload = {
            "timestamp": utc_now(),
            "errors": errors,
            "gui": gui_result,
            "ports_busy": ports_busy,
            "leftovers": leftovers,
            "hardware_lock_present": LOCK_PATH.exists(),
            "hardware_lock_held": lock_held,
            "gnome_before": self.gnome_before,
            "gnome_after": self.gnome_after,
            "gnome_unchanged": self.gnome_before == self.gnome_after,
        }
        write_json(self.out / "cleanup.json", payload)
        self.log(
            "cleanup: ports=%s orphelins=%s verrou_tenu=%s gsettings=%s",
            ports_busy,
            leftovers,
            lock_held,
            payload["gnome_unchanged"],
        )

    def token_absent(self) -> tuple[bool, str]:
        if not self.server_token:
            return True, ""
        for path in self.out.rglob("*"):
            if not path.is_file() or path.suffix.lower() in BINARY_SUFFIXES:
                continue
            try:
                if self.server_token in path.read_text(
                    encoding="utf-8", errors="ignore"
                ):
                    return False, str(path)
            except OSError:
                continue
        return True, ""

    def summarize(self) -> dict[str, Any]:
        firefox_summary = (self.firefox or {}).get("summary") or {}
        firefox_target = firefox_summary.get("target") or {}
        firefox_vram = firefox_summary.get("vram") or {}
        gui_summary = (self.gui_baseline or {}).get("summary") or {}
        gui_target = gui_summary.get("target") or {}
        gui_vram = gui_summary.get("vram") or {}

        firefox_rss_median = (firefox_target.get("rss_kib") or {}).get("median")
        firefox_pss_median = (firefox_target.get("pss_kib") or {}).get("median")
        firefox_vram_median = (firefox_vram.get("used_mib") or {}).get("median")
        gui_rss_median = (gui_target.get("rss_kib") or {}).get("median")
        gui_pss_median = (gui_target.get("pss_kib") or {}).get("median")
        gui_vram_median = (gui_vram.get("used_mib") or {}).get("median")

        run_gui_rss = [
            ((record.get("monitor") or {}).get("gui") or {}).get("rss_kib", {}).get("max")
            for record in self.records
        ]
        run_gui_pss = [
            ((record.get("monitor") or {}).get("gui") or {}).get("pss_kib", {}).get("max")
            for record in self.records
        ]
        run_step2_rss = [
            ((record.get("monitor") or {}).get("step2") or {}).get("rss_kib", {}).get("max")
            for record in self.records
        ]
        run_step2_pss = [
            ((record.get("monitor") or {}).get("step2") or {}).get("pss_kib", {}).get("max")
            for record in self.records
        ]
        run_step2_compute = [
            ((record.get("monitor") or {}).get("step2") or {})
            .get("compute_rss_kib", {})
            .get("max")
            for record in self.records
        ]
        run_ratios_rss = [
            ((record.get("monitor") or {}).get("ratios") or {}).get(
                "gui_over_step2_rss_median"
            )
            for record in self.records
        ]
        run_ratios_pss = [
            ((record.get("monitor") or {}).get("ratios") or {}).get(
                "gui_over_step2_pss_median"
            )
            for record in self.records
        ]

        gui_rss_peak = max((value for value in run_gui_rss if value is not None), default=None)
        gui_pss_peak = max((value for value in run_gui_pss if value is not None), default=None)
        step2_rss_peak = max(
            (value for value in run_step2_rss if value is not None), default=None
        )
        step2_pss_peak = max(
            (value for value in run_step2_pss if value is not None), default=None
        )
        step2_compute_peak = max(
            (value for value in run_step2_compute if value is not None), default=None
        )
        vram_peak = max(
            (
                ((record.get("monitor") or {}).get("vram") or {}).get("peak_mib", 0)
                for record in self.records
            ),
            default=0,
        )
        free_min = min(
            (
                ((record.get("monitor") or {}).get("vram") or {}).get("free_min_mib")
                for record in self.records
                if ((record.get("monitor") or {}).get("vram") or {}).get("free_min_mib")
                is not None
            ),
            default=None,
        )
        over_count = sum(
            int(
                ((record.get("monitor") or {}).get("vram") or {}).get(
                    "over_3900_samples", 0
                )
            )
            for record in self.records
        )
        oom = any(record.get("oom") for record in self.records)
        expected_runs = self.args.runs
        runs_ok = (
            len(self.records) == expected_runs
            and all(record.get("run_ok") for record in self.records)
        )
        csv_ok = (
            len(self.records) == expected_runs
            and all(
                record.get("csv", {}).get("matches_reference")
                for record in self.records
            )
        )
        gui_contract_ok = (
            len(self.records) == expected_runs
            and all(
                (((record.get("monitor") or {}).get("gui") or {}).get("contract_ok") is True)
                for record in self.records
            )
        )
        gui_no_crash = (
            len(self.records) == expected_runs
            and all(
                record.get("gui_alive")
                and (record.get("gui_screenshot") or {}).get("ok")
                and not (record.get("gui_screenshot") or {}).get("blank")
                for record in self.records
            )
        )
        orphans_ok = all(not record.get("leftovers") for record in self.records)
        cleanup = read_json(self.out / "cleanup.json") or {}
        ports_ok = not cleanup.get("ports_busy")
        leftovers_clean = not cleanup.get("leftovers")
        lock_free = not cleanup.get("hardware_lock_held")
        gsettings_ok = cleanup.get("gnome_unchanged") is True
        token_ok, token_path = self.token_absent()
        paths_ok = not self.config_paths.get("bad_assignments") and bool(
            self.config_paths.get("webhook_disabled")
        ) and bool(self.config_paths.get("dry_run"))
        firefox_ok = bool(
            self.firefox
            and (self.firefox.get("cleanup") or {}).get("ok")
            and not (self.firefox.get("window") or {}).get("screenshot", {}).get("blank")
        )
        gui_rss_ok = bool(
            firefox_rss_median
            and gui_rss_peak is not None
            and gui_rss_peak <= 2 * firefox_rss_median
        )
        vram_ok = bool(
            vram_peak and vram_peak < self.args.abort_vram
            and (free_min is None or free_min > 196)
        )
        oom_free = not oom

        acceptance = {
            "firefox_baseline_ok": firefox_ok,
            "runs_ok": runs_ok,
            "csv_ok": csv_ok,
            "oom_free": oom_free,
            "vram_ok": vram_ok,
            "gui_contract_ok": gui_contract_ok,
            "gui_rss_ok": gui_rss_ok,
            "gui_no_crash": gui_no_crash,
            "orphans_ok": orphans_ok and leftovers_clean,
            "ports_ok": ports_ok,
            "token_absent": token_ok,
            "lock_free": lock_free,
            "gsettings_ok": gsettings_ok,
            "paths_ok": paths_ok,
            "acceptance_ok": False,
        }
        acceptance["acceptance_ok"] = all(
            value for key, value in acceptance.items() if key != "acceptance_ok"
        )

        comparison = {
            "firefox_rss_median_kib": firefox_rss_median,
            "firefox_pss_median_kib": firefox_pss_median,
            "firefox_vram_median_mib": firefox_vram_median,
            "firefox_vram_delta_mib": (self.firefox or {}).get("vram_delta_mib"),
            "gui_baseline_rss_median_kib": gui_rss_median,
            "gui_baseline_pss_median_kib": gui_pss_median,
            "gui_baseline_vram_median_mib": gui_vram_median,
            "gui_baseline_vram_delta_mib": (self.gui_baseline or {}).get("vram_delta_mib"),
            "gui_step2_rss_peak_kib": gui_rss_peak,
            "gui_step2_pss_peak_kib": gui_pss_peak,
            "gui_step2_vram_peak_mib": vram_peak,
            "step2_rss_peak_kib": step2_rss_peak,
            "step2_pss_peak_kib": step2_pss_peak,
            "step2_compute_rss_peak_kib": step2_compute_peak,
            "gui_rss_over_firefox": (
                gui_rss_peak / firefox_rss_median
                if gui_rss_peak is not None and firefox_rss_median
                else None
            ),
            "gui_pss_over_firefox": (
                gui_pss_peak / firefox_pss_median
                if gui_pss_peak is not None and firefox_pss_median
                else None
            ),
            "step2_vram_delta_over_gui_mib": (
                vram_peak - self.gui_baseline_vram
                if self.gui_baseline_vram is not None and vram_peak
                else None
            ),
            "step2_vram_delta_over_idle_mib": (
                vram_peak - self.idle_vram
                if self.idle_vram is not None and vram_peak
                else None
            ),
        }

        summary = {
            "j3": {
                "started_at": self.started_at,
                "finished_at": utc_now(),
                "duration_s": round(time.monotonic() - self.started_monotonic, 3),
                "host": os.uname().nodename,
                "display": self.args.display,
                "out": str(self.out),
                "gui_profile": self.args.gui_profile,
                "step2_batch": self.args.step2_batch,
                "cuda_lazy": self.args.cuda_lazy,
                "total_runs": len(self.records),
            },
            "preflight": self.preflight_data,
            "config": self.config_paths,
            "video": self.video_info,
            "idle_vram": read_json(self.out / "vram-idle.json"),
            "firefox": self.firefox,
            "gui": {
                "start": read_json(self.out / "gui-start.json"),
                "baseline": self.gui_baseline,
            },
            "runs": self.records,
            "comparison": comparison,
            "vram": {
                "peak_mib": vram_peak,
                "free_min_mib": free_min,
                "over_3900_samples": over_count,
                "oom": oom,
                "abort_vram_mib": self.args.abort_vram,
            },
            "acceptance": acceptance,
            "token_path": token_path,
            "cleanup": cleanup,
            "errors": self.errors,
        }
        write_json(self.out / "summary.json", summary)
        self.write_csv()
        self.write_acceptance(summary)
        return summary

    def write_csv(self) -> None:
        fields = [
            "run",
            "exit_code",
            "duration_s",
            "csv_ok",
            "csv_sha256",
            "oom",
            "vram_peak_mib",
            "vram_free_min_mib",
            "vram_over_3900_samples",
            "gui_rss_median_kib",
            "gui_rss_max_kib",
            "gui_pss_median_kib",
            "gui_pss_max_kib",
            "step2_rss_median_kib",
            "step2_rss_max_kib",
            "step2_pss_median_kib",
            "step2_pss_max_kib",
            "step2_compute_rss_max_kib",
            "gui_contract_ok",
            "gui_proc_count",
            "step2_proc_count",
            "gui_over_step2_rss_median",
            "gui_over_step2_pss_median",
            "monitor_rc",
            "gui_alive",
            "gui_screenshot_blank",
            "leftovers",
        ]
        with (self.out / "runs.csv").open("w", newline="", encoding="utf-8") as handle:
            writer = csv.DictWriter(handle, fieldnames=fields)
            writer.writeheader()
            for record in self.records:
                monitor = record.get("monitor") or {}
                gui = monitor.get("gui") or {}
                step2 = monitor.get("step2") or {}
                vram = monitor.get("vram") or {}
                writer.writerow(
                    {
                        "run": record.get("run"),
                        "exit_code": record.get("exit_code"),
                        "duration_s": record.get("duration_s"),
                        "csv_ok": record.get("csv", {}).get("matches_reference"),
                        "csv_sha256": record.get("csv", {}).get("sha256"),
                        "oom": record.get("oom"),
                        "vram_peak_mib": vram.get("peak_mib"),
                        "vram_free_min_mib": vram.get("free_min_mib"),
                        "vram_over_3900_samples": vram.get("over_3900_samples"),
                        "gui_rss_median_kib": (gui.get("rss_kib") or {}).get("median"),
                        "gui_rss_max_kib": (gui.get("rss_kib") or {}).get("max"),
                        "gui_pss_median_kib": (gui.get("pss_kib") or {}).get("median"),
                        "gui_pss_max_kib": (gui.get("pss_kib") or {}).get("max"),
                        "step2_rss_median_kib": (step2.get("rss_kib") or {}).get("median"),
                        "step2_rss_max_kib": (step2.get("rss_kib") or {}).get("max"),
                        "step2_pss_median_kib": (step2.get("pss_kib") or {}).get("median"),
                        "step2_pss_max_kib": (step2.get("pss_kib") or {}).get("max"),
                        "step2_compute_rss_max_kib": (
                            step2.get("compute_rss_kib") or {}
                        ).get("max"),
                        "gui_contract_ok": gui.get("contract_ok"),
                        "gui_proc_count": len(gui.get("processes") or []),
                        "step2_proc_count": len(step2.get("processes") or []),
                        "gui_over_step2_rss_median": (
                            monitor.get("ratios") or {}
                        ).get("gui_over_step2_rss_median"),
                        "gui_over_step2_pss_median": (
                            monitor.get("ratios") or {}
                        ).get("gui_over_step2_pss_median"),
                        "monitor_rc": record.get("monitor_rc"),
                        "gui_alive": record.get("gui_alive"),
                        "gui_screenshot_blank": (
                            record.get("gui_screenshot") or {}
                        ).get("blank"),
                        "leftovers": "|".join(record.get("leftover_names") or []),
                    }
                )

    def write_acceptance(self, summary: dict[str, Any]) -> None:
        acceptance = summary["acceptance"]
        comparison = summary["comparison"]
        vram = summary["vram"]

        def mib(value: Any) -> str:
            if value is None:
                return "n/a"
            return f"{value / 1024:.1f}"

        lines = [
            "# J3 — STEP2 CUDA et coexistence GUI",
            "",
            f"- Début : {summary['j3']['started_at']}",
            f"- Fin : {summary['j3']['finished_at']}",
            f"- Durée : {summary['j3']['duration_s']} s",
            f"- Profil GUI : `{summary['j3']['gui_profile']}`",
            f"- Batch STEP2 : {summary['j3']['step2_batch']}",
            f"- CUDA_MODULE_LOADING=LAZY : {summary['j3']['cuda_lazy']}",
            f"- Lancements : {summary['j3']['total_runs']}",
            "",
            "## Tableau d'acceptation",
            "",
            "| Critère | Résultat |",
            "|---|---|",
            f"| Baseline Firefox | **{acceptance['firefox_baseline_ok']}** |",
            f"| {self.args.runs} runs STEP2 | **{acceptance['runs_ok']}** |",
            f"| CSV identiques | **{acceptance['csv_ok']}** |",
            f"| Aucun OOM CUDA | **{acceptance['oom_free']}** |",
            f"| VRAM totale < 3 900 MiB | **{acceptance['vram_ok']}** |",
            f"| Contrat 3 processus GUI | **{acceptance['gui_contract_ok']}** |",
            f"| GUI RSS ≤ 2× Firefox | **{acceptance['gui_rss_ok']}** |",
            f"| GUI sans crash/blanc | **{acceptance['gui_no_crash']}** |",
            f"| Aucun orphelin | **{acceptance['orphans_ok']}** |",
            f"| Ports libres | **{acceptance['ports_ok']}** |",
            f"| Token absent | **{acceptance['token_absent']}** |",
            f"| Verrou matériel libre | **{acceptance['lock_free']}** |",
            f"| gsettings inchangés | **{acceptance['gsettings_ok']}** |",
            f"| Chemins J3 isolés | **{acceptance['paths_ok']}** |",
            f"| **Acceptation globale** | **{acceptance['acceptance_ok']}** |",
            "",
            "## Firefox vs Tauri",
            "",
            "| Métrique | Firefox | Tauri baseline | Tauri pendant STEP2 |",
            "|---|---:|---:|---:|",
            f"| RSS (Mio) | {mib(comparison['firefox_rss_median_kib'])} | "
            f"{mib(comparison['gui_baseline_rss_median_kib'])} | "
            f"{mib(comparison['gui_step2_rss_peak_kib'])} |",
            f"| PSS (Mio) | {mib(comparison['firefox_pss_median_kib'])} | "
            f"{mib(comparison['gui_baseline_pss_median_kib'])} | "
            f"{mib(comparison['gui_step2_pss_peak_kib'])} |",
            f"| VRAM médiane (Mio) | {comparison['firefox_vram_median_mib']} | "
            f"{comparison['gui_baseline_vram_median_mib']} | "
            f"{comparison['gui_step2_vram_peak_mib']} (pic) |",
            f"| VRAM delta (Mio) | {comparison['firefox_vram_delta_mib']} | "
            f"{comparison['gui_baseline_vram_delta_mib']} | "
            f"{comparison['step2_vram_delta_over_gui_mib']} (sur baseline GUI) |",
            "",
            f"- GUI RSS / Firefox RSS : **{comparison['gui_rss_over_firefox']}**",
            f"- GUI PSS / Firefox PSS : **{comparison['gui_pss_over_firefox']}**",
            "",
            "## STEP2",
            "",
            "| Run | Exit | CSV | OOM | Pic VRAM (Mio) | Libre min (Mio) | GUI RSS max (Mio) | STEP2 RSS max (Mio) | Ratio GUI/STEP2 |",
            "|---:|---:|---|---:|---:|---:|---:|---:|---:|",
        ]
        for record in self.records:
            monitor = record.get("monitor") or {}
            gui = monitor.get("gui") or {}
            step2 = monitor.get("step2") or {}
            run_vram = monitor.get("vram") or {}
            ratio = (monitor.get("ratios") or {}).get("gui_over_step2_rss_median")
            lines.append(
                f"| {record['run']} | {record['exit_code']} | "
                f"{'ok' if record.get('csv', {}).get('matches_reference') else 'FAIL'} | "
                f"{record.get('oom')} | {run_vram.get('peak_mib')} | "
                f"{run_vram.get('free_min_mib')} | "
                f"{mib((gui.get('rss_kib') or {}).get('max'))} | "
                f"{mib((step2.get('rss_kib') or {}).get('max'))} | "
                f"{ratio} |"
            )
        lines += [
            "",
            f"- Pic VRAM global : **{vram['peak_mib']} MiB**",
            f"- VRAM libre minimale : **{vram['free_min_mib']} MiB**",
            f"- Échantillons ≥ 3 900 MiB : **{vram['over_3900_samples']}**",
            f"- OOM CUDA : **{vram['oom']}**",
            "",
            "## Artefacts bruts",
            "",
            f"```text\n{self.out}/\n"
            "  preflight.json\n"
            "  config.toml\n"
            "  server.log, server.err\n"
            "  video.json, media/clip-30s.mp4\n"
            "  vram-idle.json\n"
            "  firefox-baseline/{firefox-window.json,firefox-cleanup.json,monitor/}\n"
            "  gui-start.json, gui-baseline/{console.log,monitor/}\n"
            "  runs/run-01..05/{run.json,step2.log,step2.stdout.log,step2.stderr.log,monitor/}\n"
            "  runs.csv\n"
            "  summary.json\n"
            "  acceptance.md\n"
            "  cleanup.json\n"
            "```",
            "",
        ]
        (self.out / "acceptance.md").write_text(
            "\n".join(lines) + "\n", encoding="utf-8"
        )


def parse_args() -> RunbookArgs:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--base-config", type=Path, default=DEFAULT_BASE_CONFIG)
    parser.add_argument("--gui-bin", type=Path, default=DEFAULT_GUI)
    parser.add_argument("--server-bin", type=Path, default=DEFAULT_SERVER)
    parser.add_argument("--cli-bin", type=Path, default=DEFAULT_CLI)
    parser.add_argument("--audit-js", type=Path, default=DEFAULT_AUDIT)
    parser.add_argument("--firefox-script", type=Path, default=DEFAULT_FIREFOX)
    parser.add_argument("--monitor", type=Path, default=DEFAULT_MONITOR)
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument(
        "--gui-profile", choices=["default", "disable-dmabuf"], default="default"
    )
    parser.add_argument("--step2-batch", type=int, default=8)
    parser.add_argument("--cuda-lazy", action="store_true")
    parser.add_argument("--firefox-seconds", type=int, default=60)
    parser.add_argument("--settle-seconds", type=int, default=15)
    parser.add_argument("--gui-baseline-seconds", type=int, default=60)
    parser.add_argument("--step2-timeout", type=int, default=300)
    parser.add_argument("--abort-vram", type=int, default=3900)
    parser.add_argument("--display", default=os.environ.get("DISPLAY", ":0"))
    args = parser.parse_args()
    return RunbookArgs(
        out=args.out,
        base_config=args.base_config,
        gui_bin=args.gui_bin,
        server_bin=args.server_bin,
        cli_bin=args.cli_bin,
        audit_js=args.audit_js,
        firefox_script=args.firefox_script,
        monitor=args.monitor,
        runs=args.runs,
        gui_profile=args.gui_profile,
        step2_batch=args.step2_batch,
        cuda_lazy=args.cuda_lazy,
        firefox_seconds=args.firefox_seconds,
        settle_seconds=args.settle_seconds,
        gui_baseline_seconds=args.gui_baseline_seconds,
        step2_timeout=args.step2_timeout,
        abort_vram=args.abort_vram,
        display=args.display,
    )


def main() -> int:
    args = parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        handlers=[
            logging.StreamHandler(sys.stdout),
            logging.FileHandler(args.out / "runbook.log", encoding="utf-8"),
        ],
    )
    runbook = Runbook(args)

    def on_signal(signum: int, _frame: Any) -> None:
        runbook.log("signal %s reçu, nettoyage", signum)
        runbook.cleanup()
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGINT, on_signal)
    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGHUP, on_signal)

    exit_code = 0
    try:
        runbook.preflight()
        runbook.prepare()
        runbook.start_server()
        runbook.measure_idle()
        runbook.run_firefox()
        runbook.start_gui()
        runbook.run_gui_baseline()
        for number in range(1, args.runs + 1):
            runbook.run_step2(number)
    except RunFailure as error:
        runbook.errors.append(str(error))
        write_json(
            args.out / "contingency.json",
            {
                "timestamp": utc_now(),
                "reason": str(error),
                "run": error.record,
                "suggested_levels": [
                    "gui-profile=disable-dmabuf",
                    "step2-batch=4",
                    "step2-batch=4 + cudnn_algo_search=false",
                    "step2-batch=2 + --cuda-lazy",
                    "step2-batch=1 + --cuda-lazy",
                ],
            },
        )
        exit_code = 10
    except Exception as error:  # noqa: BLE001
        runbook.errors.append(str(error))
        LOGGER.exception("J3 interrompu")
        write_json(
            args.out / "contingency.json",
            {"timestamp": utc_now(), "reason": str(error), "run": None},
        )
        exit_code = 11
    finally:
        runbook.cleanup()

    try:
        summary = runbook.summarize()
    except Exception as error:  # noqa: BLE001
        runbook.errors.append(f"summarize: {error}")
        LOGGER.exception("synthèse J3 impossible")
        return 12

    LOGGER.info(
        "J3 terminé: acceptance=%s runs=%s oom=%s pic_vram=%s MiB",
        summary["acceptance"]["acceptance_ok"],
        summary["acceptance"]["runs_ok"],
        summary["vram"]["oom"],
        summary["vram"]["peak_mib"],
    )
    print(json.dumps(summary["acceptance"], indent=2, ensure_ascii=False))
    if summary["acceptance"]["acceptance_ok"]:
        return 0
    return exit_code or 10


if __name__ == "__main__":
    sys.exit(main())
