#!/usr/bin/env python3
"""Harnais de parité de la migration Rust.

Le harnais n'est pas livré avec l'application : il sert à mesurer l'ancienne
implémentation, à figer ses sorties et à comparer la nouvelle implémentation.

Commandes principales :

    coral_env/bin/python tools/parity/parity.py build-corpus
    coral_env/bin/python tools/parity/parity.py verify
    coral_env/bin/python tools/parity/parity.py run --case real_244_step6
    coral_env/bin/python tools/parity/parity.py run-all --tag real
    coral_env/bin/python tools/parity/parity.py report
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import threading
import time
import tomllib
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

if sys.version_info < (3, 11):
    raise SystemExit("Utiliser coral_env/bin/python (Python >= 3.11) pour tomllib.")

import fcntl

TOOLS_DIR = Path(__file__).resolve().parent
NEW_BASE = TOOLS_DIR.parents[1]
PARITY_DIR = NEW_BASE / "parity"
DEFAULT_MANIFEST = PARITY_DIR / "manifest.toml"
FIXTURES_DIR = PARITY_DIR / "fixtures"
BASELINE_DIR = PARITY_DIR / "baseline"
WORK_DIR = PARITY_DIR / "work"

TEMPLATE_RE = re.compile(r"\{([A-Za-z0-9_.]+)\}")
PROGRESS_RE = re.compile(
    r"^(INTERNAL_PROGRESS|TOTAL_VIDEOS_TO_PROCESS|TOTAL_VIDEO_TO_PROCESS|"
    r"TOTAL_JSON_TO_REDUCE|PROCESSING_VIDEO|Succès|Échec|MS_BATCH_OK|MS_BATCH_FAIL)"
)


# ---------------------------------------------------------------------------
# Manifeste et contexte


def load_manifest(path: Path = DEFAULT_MANIFEST) -> dict[str, Any]:
    with open(path, "rb") as handle:
        return tomllib.load(handle)


def resolve_case(case: dict[str, Any], manifest: dict[str, Any]) -> dict[str, Any]:
    family_name = case.get("family")
    base: dict[str, Any] = {}
    if family_name:
        base = dict(manifest.get("families", {}).get(family_name, {}))
    base.update(case)
    return base


def expand(text: str, ctx: dict[str, Any]) -> str:
    def replace(match: re.Match[str]) -> str:
        value: Any = ctx
        for part in match.group(1).split("."):
            if not isinstance(value, dict) or part not in value:
                raise KeyError(f"clé inconnue dans le modèle: {match.group(1)}")
            value = value[part]
        return str(value)

    return TEMPLATE_RE.sub(replace, str(text))


def case_context(case: dict[str, Any], manifest: dict[str, Any]) -> dict[str, Any]:
    project_cfg = manifest["project"]
    case_id = case["id"]
    stem = case.get("stem", case_id)
    layout = case.get("layout", "flat")
    project = case.get("project", f"{case_id} Camille")
    case_dir = WORK_DIR / case_id
    if layout == "docs":
        work_dir = case_dir / project
        docs_dir = work_dir / "docs"
        outputs_dir = docs_dir
    else:
        work_dir = case_dir
        docs_dir = case_dir
        outputs_dir = case_dir
    logs = case_dir / "logs"
    return {
        "old_base": project_cfg["old_base"],
        "new_base": project_cfg.get("new_base", str(NEW_BASE)),
        "parity": str(PARITY_DIR),
        "case": case_id,
        "stem": stem,
        "project": project,
        "case_dir": str(case_dir),
        "work_dir": str(work_dir),
        "docs_dir": str(docs_dir),
        "outputs_dir": str(outputs_dir),
        "logs": str(logs),
        "python": project_cfg["python"],
        "source_dir": case.get("source_dir", ""),
        "source_stem": case.get("source_stem", ""),
    }


def resolve_path(root: Path, text: str, ctx: dict[str, Any]) -> Path:
    expanded = expand(text, ctx)
    path = Path(expanded)
    return path if path.is_absolute() else root / path


# ---------------------------------------------------------------------------
# Vérification


def verify(manifest: dict[str, Any], probe: bool = False, check_sources: bool = False) -> int:
    errors: list[str] = []
    old_base = Path(manifest["project"]["old_base"])
    if not old_base.is_dir():
        errors.append(f"old_base introuvable: {old_base}")
    for step, python in manifest["project"]["python"].items():
        if not Path(python).exists():
            errors.append(f"python {step} introuvable: {python}")

    lock_path = PARITY_DIR / "cases" / "corpus.lock.json"
    if lock_path.exists():
        try:
            lock = json.loads(lock_path.read_text(encoding="utf-8"))
            for name, info in lock.get("files", {}).items():
                path = PARITY_DIR / "cases" / name
                if path.exists() and sha256(path) != info.get("sha256"):
                    errors.append(f"corpus {name}: checksum différent de corpus.lock.json")
        except Exception as exc:  # noqa: BLE001
            errors.append(f"corpus.lock.json illisible: {exc}")

    if check_sources:
        checksums_path = FIXTURES_DIR / "checksums.json"
        if checksums_path.exists():
            try:
                checksums = json.loads(checksums_path.read_text(encoding="utf-8"))
                for case_id, files in checksums.items():
                    for name, info in files.items():
                        source = info.get("source")
                        if not source:
                            continue
                        source_path = Path(source)
                        if not source_path.exists():
                            print(f"AVERTISSEMENT: source absente {case_id}/{name}: {source}")
                            continue
                        if sha256(source_path) != info.get("sha256"):
                            print(f"AVERTISSEMENT: source modifiée {case_id}/{name}: {source}")
            except Exception as exc:  # noqa: BLE001
                errors.append(f"checksums.json illisible: {exc}")

    print(f"{'cas':<28} {'étape':<7} {'layout':<6} {'entrées':>7} {'attendu':<8}")
    print("-" * 68)
    for raw_case in manifest.get("case", []):
        case = resolve_case(raw_case, manifest)
        ctx = case_context(case, manifest)
        step_name = case.get("step")
        step = manifest.get("steps", {}).get(step_name)
        if step is None:
            errors.append(f"{case['id']}: étape inconnue {step_name!r}")
            continue
        input_count = 0
        for entry in case.get("inputs", []):
            source = resolve_path(PARITY_DIR, entry["source"], ctx)
            input_count += 1
            if not source.exists():
                errors.append(f"{case['id']}: entrée introuvable {source}")
        expected_ok = "-"
        expected_source = case.get("expected_source")
        if expected_source:
            source = resolve_path(PARITY_DIR, expected_source, ctx)
            if not source.exists():
                errors.append(f"{case['id']}: attendu introuvable {source}")
                expected_ok = "manquant"
            else:
                expected_ok = "oui"
                if probe:
                    _probe_quiet(source)
        print(
            f"{case['id']:<28} {step_name:<7} {case.get('layout', 'flat'):<6} "
            f"{input_count:>7} {expected_ok:<8}"
        )

    if errors:
        print("\nErreurs:")
        for error in errors:
            print(f"  - {error}")
        return 1
    print("\nContrat de corpus valide.")
    return 0


def _probe_quiet(path: Path) -> None:
    subprocess.run(
        ["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)],
        capture_output=True, timeout=30,
    )


# ---------------------------------------------------------------------------
# Préparation


def _transform_step5_raw_audio(docs_dir: Path, stem: str) -> None:
    """Reconstruit l'ordre du JSON STEP3 brut.

    La copie de production `_audio.json` est déjà passée par STEP5 : `fps` et
    `total_frames` sont écrits après `frames_analysis`. Or le parseur streaming
    de STEP5 s'arrête au début du tableau et ne voit alors plus le fps. On
    replace donc l'en-tête avant le tableau, comme le produit STEP3.
    """
    audio_path = docs_dir / f"{stem}_audio.json"
    if not audio_path.exists():
        return
    data = json.loads(audio_path.read_text(encoding="utf-8"))
    frames = data.get("frames_analysis") or data.get("frames") or []
    payload: dict[str, Any] = {
        "video_filename": f"{stem}.mp4",
        "total_frames": data.get("total_frames"),
        "fps": data.get("fps"),
    }
    if data.get("speaker_stats"):
        payload["speaker_stats"] = data["speaker_stats"]
    payload["frames_analysis"] = frames
    if data.get("speaker_embeddings"):
        payload["speaker_embeddings"] = data["speaker_embeddings"]
    audio_path.write_text(
        json.dumps(payload, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def prepare(case: dict[str, Any], manifest: dict[str, Any]) -> dict[str, Any]:
    ctx = case_context(case, manifest)
    case_dir = Path(ctx["case_dir"])
    if case_dir.exists():
        shutil.rmtree(case_dir)
    docs_dir = Path(ctx["docs_dir"])
    docs_dir.mkdir(parents=True, exist_ok=True)
    Path(ctx["logs"]).mkdir(parents=True, exist_ok=True)

    for entry in case.get("inputs", []):
        source = resolve_path(PARITY_DIR, entry["source"], ctx)
        if not source.exists():
            raise FileNotFoundError(f"entrée introuvable: {source}")
        relative = expand(entry["as"], ctx)
        destination = docs_dir / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        mode = entry.get("mode", "copy")
        if mode == "link":
            os.symlink(source, destination)
        else:
            shutil.copy2(source, destination)

    transform = case.get("transform")
    if transform == "step5_raw_audio":
        _transform_step5_raw_audio(docs_dir, str(ctx["stem"]))
    elif transform:
        raise ValueError(f"transformation inconnue: {transform}")
    return ctx


# ---------------------------------------------------------------------------
# Sécurité d'exécution


def production_busy() -> str | None:
    try:
        with urllib.request.urlopen("http://127.0.0.1:5003/sequence/status", timeout=2) as response:
            payload = json.load(response)
        if payload.get("is_running"):
            return "séquence de production en cours"
    except Exception:  # noqa: BLE001
        pass

    proc = subprocess.run(
        ["pgrep", "-af", r"workflow_scripts/step[1-8]"],
        capture_output=True, text=True,
    )
    for line in proc.stdout.splitlines():
        if "parity" in line or "pgrep" in line:
            continue
        return line.strip()
    return None


def acquire_hardware_lock(wait: bool = False) -> int | None:
    lock_dir = Path(os.environ.get("XDG_RUNTIME_DIR", "/tmp"))
    lock_path = lock_dir / "workflow-hw.lock"
    descriptor = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o644)
    flags = fcntl.LOCK_EX | (0 if wait else fcntl.LOCK_NB)
    try:
        fcntl.flock(descriptor, flags)
    except BlockingIOError:
        os.close(descriptor)
        return None
    os.write(descriptor, f"{os.getpid()}\n".encode())
    return descriptor


def release_hardware_lock(descriptor: int | None) -> None:
    if descriptor is None:
        return
    try:
        fcntl.flock(descriptor, fcntl.LOCK_UN)
    finally:
        os.close(descriptor)


# ---------------------------------------------------------------------------
# Environnement des sous-processus


def load_dotenv(path: Path, env: dict[str, str]) -> None:
    if not path.exists():
        return
    for raw_line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        if line.startswith("export "):
            line = line[len("export "):]
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
            value = value[1:-1]
        else:
            # Les .env du projet contiennent des commentaires inline
            # (`STEP5_INCLUDE_TRACKING_ANALYTICS=1  # ...`). python-dotenv les
            # retire ; le harnais doit faire pareil.
            value = value.split("#", 1)[0].strip()
        env.setdefault(key, value)


def build_env(case: dict[str, Any], step: dict[str, Any], ctx: dict[str, Any]) -> dict[str, str]:
    old_base = Path(ctx["old_base"])
    env = dict(os.environ)
    load_dotenv(old_base / ".env", env)

    env.update({
        "BASE_PATH_SCRIPTS": str(old_base),
        "BASE_PATH_SCRIPTS_ENV": str(old_base),
        "ROOT_SCAN_DIR": ctx["docs_dir"],
        "FOLDER_KEYWORD": case.get("keyword", "Camille"),
        "SUBFOLDER_NAME": "docs",
        "PYTHONPATH": f"{old_base}:{TOOLS_DIR}",
        "PYTHONIOENCODING": "UTF-8",
        "PYTHONUTF8": "1",
        "PYTHONUNBUFFERED": "1",
        "OLD_BASE": str(old_base),
    })

    ld_paths = [
        str(old_base / "coral_env" / "lib"),
        str(old_base / "tmp_lib" / "usr" / "lib" / "x86_64-linux-gnu"),
    ]
    existing = env.get("LD_LIBRARY_PATH")
    if existing:
        ld_paths.append(existing)
    python = Path(expand(step["command"][0], ctx))
    venv_root = python.parent.parent
    for candidate in (venv_root / "lib" / "python3.10" / "site-packages", venv_root / "lib"):
        if candidate.exists():
            for nvidia_dir in candidate.glob("nvidia/*/lib"):
                ld_paths.insert(0, str(nvidia_dir))
    env["LD_LIBRARY_PATH"] = ":".join(ld_paths)

    for key, value in step.get("env", {}).items():
        env[key] = expand(str(value), ctx)
    return env


# ---------------------------------------------------------------------------
# Mesure et exécution


def _sample_rss(pid: int) -> tuple[int, float, float]:
    import psutil

    process = psutil.Process(pid)
    rss = process.memory_info().rss
    cpu_user, cpu_system = process.cpu_times().user, process.cpu_times().system
    for child in process.children(recursive=True):
        try:
            rss += child.memory_info().rss
            child_times = child.cpu_times()
            cpu_user += child_times.user
            cpu_system += child_times.system
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue
    return rss, cpu_user, cpu_system


def _gpu_global() -> tuple[int, int] | None:
    try:
        proc = subprocess.run(
            ["nvidia-smi", "--query-gpu=utilization.gpu,memory.used", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=2,
        )
        if proc.returncode != 0:
            return None
        first = proc.stdout.strip().splitlines()[0]
        util, memory = (part.strip() for part in first.split(",", 1))
        return int(float(util)), int(float(memory))
    except Exception:  # noqa: BLE001
        return None


def _gpu_process_memory(pids: set[int]) -> int:
    try:
        proc = subprocess.run(
            ["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
            capture_output=True, text=True, timeout=2,
        )
        if proc.returncode != 0:
            return 0
        total = 0
        for line in proc.stdout.splitlines():
            parts = [part.strip() for part in line.split(",")]
            if len(parts) < 2:
                continue
            try:
                pid = int(parts[0])
                memory = int(float(parts[1]))
            except ValueError:
                continue
            if pid in pids:
                total += memory
        return total
    except Exception:  # noqa: BLE001
        return 0


def _monitor(pid: int, stop: threading.Event, metrics: dict[str, Any]) -> None:
    import psutil

    max_rss = 0
    max_gpu_util = 0
    max_gpu_memory = 0
    max_gpu_total = 0
    last_gpu = 0.0
    while not stop.is_set():
        pids: set[int] = {pid}
        try:
            rss, cpu_user, cpu_system = _sample_rss(pid)
            max_rss = max(max_rss, rss)
            metrics["cpu_user_s"] = cpu_user
            metrics["cpu_sys_s"] = cpu_system
            process = psutil.Process(pid)
            pids |= {child.pid for child in process.children(recursive=True)}
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            break
        now = time.monotonic()
        if now - last_gpu >= 0.5:
            sample = _gpu_global()
            if sample is not None:
                util, total_memory = sample
                max_gpu_util = max(max_gpu_util, util)
                max_gpu_total = max(max_gpu_total, total_memory)
            max_gpu_memory = max(max_gpu_memory, _gpu_process_memory(pids))
            last_gpu = now
        stop.wait(0.25)
    metrics["max_rss_bytes"] = max_rss
    metrics["gpu_util_peak"] = max_gpu_util
    metrics["gpu_memory_peak_mb"] = max_gpu_memory
    metrics["gpu_total_peak_mb"] = max_gpu_total


def _kill_group(pid: int, sig: int) -> None:
    try:
        os.killpg(pid, sig)
    except ProcessLookupError:
        pass


def run_process(
    command: list[str],
    cwd: Path,
    env: dict[str, str],
    timeout_s: int,
    console_path: Path,
    quiet: bool,
) -> dict[str, Any]:
    console_path.parent.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    started_at = datetime.now(timezone.utc).isoformat()
    process = subprocess.Popen(
        command,
        cwd=str(cwd),
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        encoding="utf-8",
        errors="replace",
        bufsize=1,
        start_new_session=True,
    )

    progress: list[dict[str, Any]] = []
    reader_done = threading.Event()

    def read_output() -> None:
        assert process.stdout is not None
        with open(console_path, "w", encoding="utf-8") as console:
            for line in process.stdout:
                console.write(line)
                console.flush()
                if not quiet:
                    print(line, end="")
                if PROGRESS_RE.match(line.strip()):
                    progress.append({
                        "t_s": round(time.monotonic() - started, 3),
                        "line": line.rstrip(),
                    })
        reader_done.set()

    reader = threading.Thread(target=read_output, name="parity-reader", daemon=True)
    reader.start()

    metrics: dict[str, Any] = {
        "max_rss_bytes": 0,
        "cpu_user_s": 0.0,
        "cpu_sys_s": 0.0,
        "gpu_util_peak": 0,
        "gpu_memory_peak_mb": 0,
        "gpu_total_peak_mb": 0,
    }
    stop = threading.Event()
    monitor = threading.Thread(target=_monitor, args=(process.pid, stop, metrics), daemon=True)
    monitor.start()

    timed_out = False
    try:
        process.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        timed_out = True
        _kill_group(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            _kill_group(process.pid, signal.SIGKILL)
            process.wait()
    finally:
        stop.set()
        monitor.join(timeout=2)
        reader.join(timeout=30)

    wall_s = time.monotonic() - started
    return {
        "command": command,
        "cwd": str(cwd),
        "exit_code": process.returncode,
        "timed_out": timed_out,
        "wall_s": round(wall_s, 3),
        "max_rss_mb": round(metrics["max_rss_bytes"] / (1024 * 1024), 2),
        "cpu_user_s": round(metrics["cpu_user_s"], 3),
        "cpu_sys_s": round(metrics["cpu_sys_s"], 3),
        "gpu_util_peak": metrics["gpu_util_peak"],
        "gpu_memory_peak_mb": metrics["gpu_memory_peak_mb"],
        "gpu_total_peak_mb": metrics["gpu_total_peak_mb"],
        "started_at": started_at,
        "ended_at": datetime.now(timezone.utc).isoformat(),
        "progress": progress[-50:],
    }


# ---------------------------------------------------------------------------
# Sorties et fixtures


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def collect_outputs(
    case: dict[str, Any],
    step: dict[str, Any],
    ctx: dict[str, Any],
) -> tuple[list[dict[str, Any]], list[str]]:
    outputs_dir = Path(ctx["outputs_dir"])
    case_id = case["id"]
    actual_dir = FIXTURES_DIR / case_id / "actual"
    if actual_dir.exists():
        shutil.rmtree(actual_dir)
    actual_dir.mkdir(parents=True, exist_ok=True)

    records: list[dict[str, Any]] = []
    warnings: list[str] = []
    patterns = case.get("outputs", step.get("outputs", []))
    for pattern in patterns:
        expanded = expand(pattern, ctx)
        matches = sorted(outputs_dir.glob(expanded))
        if not matches:
            warnings.append(f"aucune sortie pour {expanded!r}")
        for path in matches:
            if not path.is_file():
                continue
            relative = path.relative_to(outputs_dir)
            destination = actual_dir / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, destination)
            records.append({
                "path": str(relative),
                "size": path.stat().st_size,
                "sha256": sha256(path),
            })
    return records, warnings


def _expected_dir(case_id: str) -> Path:
    return FIXTURES_DIR / case_id / "expected"


def ensure_expected(case: dict[str, Any], ctx: dict[str, Any]) -> Path | None:
    expected_dir = _expected_dir(case["id"])
    if expected_dir.exists() and any(expected_dir.iterdir()):
        return expected_dir

    expected_source = case.get("expected_source")
    if not expected_source:
        return None
    source = resolve_path(PARITY_DIR, expected_source, ctx)
    if not source.exists():
        return None
    output_name = Path(expand(case["outputs"][0], ctx)).name
    expected_dir.mkdir(parents=True, exist_ok=True)
    destination = expected_dir / output_name
    shutil.copy2(source, destination)
    _record_checksum(case["id"], output_name, destination, str(source))
    return expected_dir


def _checksums_path() -> Path:
    return FIXTURES_DIR / "checksums.json"


def _record_checksum(case_id: str, name: str, path: Path, source: str | None = None) -> None:
    checksums: dict[str, Any] = {}
    path_checksums = _checksums_path()
    if path_checksums.exists():
        try:
            checksums = json.loads(path_checksums.read_text(encoding="utf-8"))
        except Exception:  # noqa: BLE001
            checksums = {}
    checksums.setdefault(case_id, {})[name] = {
        "sha256": sha256(path),
        "size": path.stat().st_size,
        "source": source,
    }
    path_checksums.parent.mkdir(parents=True, exist_ok=True)
    path_checksums.write_text(
        json.dumps(checksums, indent=2, ensure_ascii=False, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def snapshot(case: dict[str, Any], manifest: dict[str, Any]) -> int:
    ctx = case_context(case, manifest)
    expected = ensure_expected(case, ctx)
    if expected is None:
        actual = FIXTURES_DIR / case["id"] / "actual"
        if not actual.exists() or not any(actual.iterdir()):
            if case.get("allow_empty_outputs"):
                print(f"{case['id']}: aucune sortie, comportement explicitement attendu.")
                return 0
            print(f"{case['id']}: aucune sortie réelle à figer. Lancer la mesure d'abord.")
            return 1
        expected = _expected_dir(case["id"])
        if expected.exists():
            shutil.rmtree(expected)
        shutil.copytree(actual, expected)
        for path in expected.rglob("*"):
            if path.is_file():
                _record_checksum(case["id"], str(path.relative_to(expected)), path)
    print(f"{case['id']}: attendu disponible dans {expected}")
    return 0


# ---------------------------------------------------------------------------
# Exécution d'un cas


def run_case(
    case: dict[str, Any],
    manifest: dict[str, Any],
    args: argparse.Namespace,
) -> int:
    ctx = prepare(case, manifest)
    step_name = case["step"]
    step = manifest["steps"][step_name]
    expected_dir = ensure_expected(case, ctx)

    hardware = bool(step.get("hardware", False))
    busy = production_busy()
    if busy and hardware and not args.allow_production:
        print(f"REFUS: ressource matérielle occupée ({busy}). Utiliser --allow-production pour passer outre.")
        return 2
    if busy and not hardware:
        print(f"Note: production active ({busy}), cas CPU exécuté en parallèle.")

    lock = None
    if hardware:
        lock = acquire_hardware_lock(wait=args.wait_lock)
        if lock is None:
            print("REFUS: verrou matériel workflow-hw.lock déjà tenu.")
            return 2

    try:
        command = [expand(part, ctx) for part in step["command"]]
        cwd = Path(expand(step["cwd"], ctx))
        env = build_env(case, step, ctx)
        timeout_s = int(case.get("timeout_s", step.get("timeout_s", 1800)))
        baseline_case_dir = BASELINE_DIR / case["id"]
        baseline_case_dir.mkdir(parents=True, exist_ok=True)
        console_path = baseline_case_dir / "console.log"

        print(f"\n=== {case['id']} / {step_name} ===")
        print("commande:", " ".join(command))
        print("cwd:", cwd)
        metrics = run_process(command, cwd, env, timeout_s, console_path, args.quiet)

        postprocess_payload: dict[str, Any] | None = None
        postprocess = case.get("postprocess")
        if postprocess == "step5_reduce_audio":
            audio_path = Path(ctx["docs_dir"]) / f"{ctx['stem']}_audio.json"
            helper = TOOLS_DIR / "reduce_audio.py"
            started = time.monotonic()
            proc = subprocess.run(
                [expand("{python.step5}", ctx), str(helper), str(audio_path)],
                cwd=ctx["old_base"],
                env=env,
                capture_output=True,
                text=True,
                timeout=300,
            )
            postprocess_payload = {
                "name": postprocess,
                "exit_code": proc.returncode,
                "wall_s": round(time.monotonic() - started, 3),
                "stdout": proc.stdout[-2000:],
                "stderr": proc.stderr[-2000:],
            }
            if proc.returncode != 0:
                print(f"postprocess {postprocess} en échec: {proc.stderr[-500:]}")
                return 1
        elif postprocess:
            raise ValueError(f"postprocess inconnu: {postprocess}")

        outputs, warnings = collect_outputs(case, step, ctx)
        empty_output_expected = bool(case.get("allow_empty_outputs")) and not outputs

        diff_payload: dict[str, Any] | None = None
        if empty_output_expected:
            diff_payload = {
                "equal": True,
                "expected_files": 0,
                "actual_files": 0,
                "missing": [],
                "extra": [],
                "diff_count": 0,
                "differences": [],
                "notes": ["sortie vide attendue"],
            }
        elif expected_dir is not None:
            import compare

            actual_dir = FIXTURES_DIR / case["id"] / "actual"
            result = compare.compare_dirs(
                expected_dir,
                actual_dir,
                tolerance=args.tolerance,
                time_tolerance=args.time_tolerance,
                ignore_keys=case.get("ignore_keys", []),
                max_diffs=args.max_diffs,
            )
            (baseline_case_dir / "diff.txt").write_text(compare.render_text(result), encoding="utf-8")
            diff_payload = result.to_dict()
            (baseline_case_dir / "diff.json").write_text(
                json.dumps(diff_payload, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )

        result_payload = {
            "case": case["id"],
            "family": case.get("family"),
            "step": step_name,
            "stem": case.get("stem"),
            **metrics,
            "outputs": outputs,
            "warnings": warnings,
            "note": case.get("note"),
            "postprocess": postprocess_payload,
            "diff": diff_payload,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        }
        (baseline_case_dir / "result.json").write_text(
            json.dumps(result_payload, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )

        print(
            f"code={metrics['exit_code']} durée={metrics['wall_s']}s "
            f"rss={metrics['max_rss_mb']}MiB gpu_étape={metrics['gpu_memory_peak_mb']}MiB "
            f"gpu_total={metrics['gpu_total_peak_mb']}MiB util={metrics['gpu_util_peak']}%"
        )
        for warning in warnings:
            print("avertissement:", warning)
        if diff_payload is not None:
            print(
                f"diff: equal={diff_payload['equal']} "
                f"différences={diff_payload['diff_count']}"
            )
            if not diff_payload["equal"]:
                print((baseline_case_dir / "diff.txt").read_text(encoding="utf-8"))
        else:
            print("diff: aucun attendu figé (utiliser --snapshot pour le créer)")

        if metrics["timed_out"]:
            return 1
        if metrics["exit_code"] != 0:
            return 1
        if diff_payload is not None and not diff_payload["equal"]:
            return 3
        return 0
    finally:
        release_hardware_lock(lock)


# ---------------------------------------------------------------------------
# Rapport


def report() -> int:
    results: list[dict[str, Any]] = []
    if BASELINE_DIR.exists():
        for path in sorted(BASELINE_DIR.glob("*/result.json")):
            try:
                results.append(json.loads(path.read_text(encoding="utf-8")))
            except Exception:  # noqa: BLE001
                continue
    if not results:
        print("Aucun résultat. Lancer au moins un cas.")
        return 1

    lines = [
        "# Baseline de parité",
        "",
        f"Généré le {datetime.now(timezone.utc).isoformat()}.",
        "",
        "| Cas | Étape | Code | Durée (s) | RSS max (MiB) | GPU étape (MiB) | GPU total (MiB) | Diff | Sorties |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | --- | ---: |",
    ]
    for result in results:
        diff = result.get("diff")
        if diff is None:
            diff_text = "non figé"
        elif diff.get("equal"):
            diff_text = "identique"
        else:
            diff_text = f"{diff.get('diff_count')} diff."
        lines.append(
            f"| `{result['case']}` | {result['step']} | {result['exit_code']} | "
            f"{result['wall_s']} | {result['max_rss_mb']} | {result.get('gpu_memory_peak_mb', 0)} | "
            f"{result.get('gpu_total_peak_mb', 0)} | {diff_text} | {len(result.get('outputs', []))} |"
        )

    by_step: dict[str, list[dict[str, Any]]] = {}
    for result in results:
        by_step.setdefault(result["step"], []).append(result)
    lines += ["", "## Synthèse par étape", ""]
    for step, step_results in sorted(by_step.items()):
        durations = [item["wall_s"] for item in step_results]
        rss = [item["max_rss_mb"] for item in step_results]
        lines.append(
            f"- **{step}** : {len(step_results)} cas, durée {min(durations):.2f}–{max(durations):.2f} s, "
            f"RSS max {max(rss):.1f} MiB."
        )

    notes = [(result["case"], result.get("note")) for result in results if result.get("note")]
    if notes:
        lines += ["", "## Notes", ""]
        for case, note in notes:
            lines.append(f"- `{case}` : {note}")

    output_path = PARITY_DIR / "BASELINE.md"
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Rapport écrit: {output_path}")
    return 0


# ---------------------------------------------------------------------------
# CLI


def _add_common_run_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--snapshot", action="store_true", help="Figer les sorties comme attendu")
    parser.add_argument("--allow-production", action="store_true", help="Autoriser malgré une production active")
    parser.add_argument("--wait-lock", action="store_true", help="Attendre le verrou matériel")
    parser.add_argument("--quiet", action="store_true", help="Ne pas recopier la console")
    parser.add_argument("--tolerance", type=float, default=1e-6)
    parser.add_argument("--time-tolerance", type=float, default=0.001)
    parser.add_argument("--max-diffs", type=int, default=100)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Harnais de parité workflow-rs")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    subparsers = parser.add_subparsers(dest="command", required=True)

    verify_parser = subparsers.add_parser("verify", help="Vérifier le manifeste et les entrées")
    verify_parser.add_argument("--probe", action="store_true")
    verify_parser.add_argument("--check-sources", action="store_true", help="Comparer les sources de production aux checksums")

    build_parser = subparsers.add_parser("build-corpus", help="Construire les médias synthétiques")
    build_parser.add_argument("--force", action="store_true")
    build_parser.add_argument("--only", nargs="*")

    prepare_parser = subparsers.add_parser("prepare", help="Préparer le dossier de travail d'un cas")
    prepare_parser.add_argument("--case", required=True)

    run_parser = subparsers.add_parser("run", help="Exécuter un cas")
    run_parser.add_argument("--case", required=True)
    _add_common_run_args(run_parser)

    all_parser = subparsers.add_parser("run-all", help="Exécuter tous les cas d'un tag")
    all_parser.add_argument("--tag", action="append", default=[])
    all_parser.add_argument("--keep-going", action="store_true")
    _add_common_run_args(all_parser)

    snapshot_parser = subparsers.add_parser("snapshot", help="Figer les sorties attendues")
    snapshot_parser.add_argument("--case", required=True)

    report_parser = subparsers.add_parser("report", help="Agréger les mesures")
    report_parser.add_argument("--quiet", action="store_true")

    diff_parser = subparsers.add_parser("diff", help="Comparer deux dossiers")
    diff_parser.add_argument("--expected", type=Path, required=True)
    diff_parser.add_argument("--actual", type=Path, required=True)
    diff_parser.add_argument("--tolerance", type=float, default=1e-6)
    diff_parser.add_argument("--time-tolerance", type=float, default=0.001)
    diff_parser.add_argument("--ignore-key", action="append", default=[])
    diff_parser.add_argument("--max-diffs", type=int, default=100)

    args = parser.parse_args(argv)
    manifest = load_manifest(args.manifest)

    if args.command == "verify":
        return verify(manifest, probe=args.probe, check_sources=args.check_sources)

    if args.command == "build-corpus":
        import mediabuild

        mediabuild.build_all(force=args.force, only=args.only)
        return verify(manifest)

    if args.command == "prepare":
        case = _find_case(manifest, args.case)
        ctx = prepare(case, manifest)
        print(ctx["docs_dir"])
        return 0

    if args.command == "run":
        case = _find_case(manifest, args.case)
        status = run_case(case, manifest, args)
        if args.snapshot and status in {0, 3}:
            snapshot(case, manifest)
        return status

    if args.command == "run-all":
        statuses: list[int] = []
        for raw_case in manifest.get("case", []):
            case = resolve_case(raw_case, manifest)
            tags = set(case.get("tags", []))
            if args.tag and not (tags & set(args.tag)):
                continue
            status = run_case(case, manifest, args)
            statuses.append(status)
            if args.snapshot and status in {0, 3}:
                snapshot(case, manifest)
            if status not in {0, 3} and not args.keep_going:
                return status
        if not statuses:
            print("Aucun cas ne correspond aux tags.")
            return 1
        return max(statuses)

    if args.command == "snapshot":
        case = _find_case(manifest, args.case)
        return snapshot(case, manifest)

    if args.command == "report":
        return report()

    if args.command == "diff":
        import compare

        result = compare.compare_dirs(
            args.expected,
            args.actual,
            tolerance=args.tolerance,
            time_tolerance=args.time_tolerance,
            ignore_keys=args.ignore_key,
            max_diffs=args.max_diffs,
        )
        print(compare.render_text(result))
        return 0 if result.equal else 3

    return 2


def _find_case(manifest: dict[str, Any], case_id: str) -> dict[str, Any]:
    for raw_case in manifest.get("case", []):
        if raw_case["id"] == case_id:
            return resolve_case(raw_case, manifest)
    raise SystemExit(f"cas inconnu: {case_id}")


if __name__ == "__main__":
    raise SystemExit(main())
