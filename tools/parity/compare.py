#!/usr/bin/env python3
"""Comparaison sémantique des sorties de parité.

Aucune dépendance externe. Le module compare des arbres de fichiers :
- JSON : récursif, tolérances numériques, timecodes, clés ignorées ;
- CSV : en-têtes, lignes, cellules numériques et timecodes ;
- média : propriétés ffprobe (codec, résolution, fps, durée, audio) ;
- binaire : SHA-256.

Il est utilisé par `parity.py` et peut être appelé seul via
`python tools/parity/parity.py diff ...`.
"""
from __future__ import annotations

import csv
import fnmatch
import hashlib
import json
import math
import re
import subprocess
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Mapping

TIMECODE_RE = re.compile(r"^\d{1,2}:\d{2}:\d{2}[.,]\d{1,3}$")
IGNORED_NAMES = {"__pycache__", ".DS_Store"}


@dataclass
class Difference:
    path: str
    expected: Any
    actual: Any
    kind: str = "value"


@dataclass
class DiffResult:
    equal: bool = True
    expected_files: int = 0
    actual_files: int = 0
    missing: list[str] = field(default_factory=list)
    extra: list[str] = field(default_factory=list)
    differences: list[Difference] = field(default_factory=list)
    diff_count: int = 0
    max_diffs: int = 100
    notes: list[str] = field(default_factory=list)

    def add(self, path: str, expected: Any, actual: Any, kind: str = "value") -> None:
        self.equal = False
        self.diff_count += 1
        if len(self.differences) < self.max_diffs:
            self.differences.append(Difference(path, expected, actual, kind))

    def note(self, message: str) -> None:
        self.notes.append(message)

    def to_dict(self) -> dict[str, Any]:
        return {
            "equal": self.equal,
            "expected_files": self.expected_files,
            "actual_files": self.actual_files,
            "missing": self.missing,
            "extra": self.extra,
            "diff_count": self.diff_count,
            "differences": [
                {
                    "path": d.path,
                    "expected": _short(d.expected),
                    "actual": _short(d.actual),
                    "kind": d.kind,
                }
                for d in self.differences
            ],
            "notes": self.notes,
        }


def _short(value: Any, limit: int = 300) -> Any:
    if isinstance(value, str) and len(value) > limit:
        return value[:limit] + "…"
    if isinstance(value, (list, dict)):
        text = json.dumps(value, ensure_ascii=False, default=str)
        return text[:limit] + ("…" if len(text) > limit else "")
    return value


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def is_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def timecode_to_seconds(value: str) -> float | None:
    if not TIMECODE_RE.match(value.strip()):
        return None
    text = value.strip().replace(",", ".")
    hours, minutes, seconds = text.split(":")
    return int(hours) * 3600 + int(minutes) * 60 + float(seconds)


def _path_join(base: str, key: Any) -> str:
    key = str(key)
    if not base:
        return key
    if re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", key):
        return f"{base}.{key}"
    return f"{base}[{key!r}]"


def _tolerance_for(path: str, default: float, overrides: Mapping[str, float] | None) -> float:
    if not overrides:
        return default
    for pattern, value in overrides.items():
        if fnmatch.fnmatch(path, pattern):
            return value
    return default


def _diff_value(
    expected: Any,
    actual: Any,
    path: str,
    result: DiffResult,
    tolerance: float,
    time_tolerance: float,
    tolerances: Mapping[str, float] | None,
    ignore_keys: set[str],
    max_diffs: int,
) -> None:
    if result.diff_count >= max_diffs:
        # On continue à compter sans mémoriser les détails.
        pass

    if isinstance(expected, dict) and isinstance(actual, dict):
        expected_keys = set(expected) - ignore_keys
        actual_keys = set(actual) - ignore_keys
        for key in sorted(expected_keys - actual_keys):
            result.add(_path_join(path, key), expected[key], "<missing>", "key")
        for key in sorted(actual_keys - expected_keys):
            result.add(_path_join(path, key), "<missing>", actual[key], "key")
        for key in sorted(expected_keys & actual_keys):
            _diff_value(
                expected[key], actual[key], _path_join(path, key), result,
                tolerance, time_tolerance, tolerances, ignore_keys, max_diffs,
            )
        return

    if isinstance(expected, list) and isinstance(actual, list):
        if len(expected) != len(actual):
            result.add(path, len(expected), len(actual), "length")
        for index in range(min(len(expected), len(actual))):
            _diff_value(
                expected[index], actual[index], _path_join(path, index), result,
                tolerance, time_tolerance, tolerances, ignore_keys, max_diffs,
            )
        return

    if is_number(expected) and is_number(actual):
        atol = _tolerance_for(path, tolerance, tolerances)
        if isinstance(expected, int) and isinstance(actual, int):
            if expected != actual:
                result.add(path, expected, actual, "int")
            return
        if not math.isclose(float(expected), float(actual), rel_tol=atol, abs_tol=atol):
            result.add(path, expected, actual, "number")
        return

    if isinstance(expected, str) and isinstance(actual, str):
        if expected == actual:
            return
        exp_tc = timecode_to_seconds(expected)
        act_tc = timecode_to_seconds(actual)
        if exp_tc is not None and act_tc is not None:
            if abs(exp_tc - act_tc) <= time_tolerance:
                return
            result.add(path, expected, actual, "timecode")
            return
        result.add(path, expected, actual, "string")
        return

    if expected != actual:
        result.add(path, expected, actual, "value")


def compare_json_files(
    expected_path: Path,
    actual_path: Path,
    result: DiffResult,
    tolerance: float = 1e-6,
    time_tolerance: float = 0.001,
    tolerances: Mapping[str, float] | None = None,
    ignore_keys: Iterable[str] = (),
) -> None:
    try:
        expected = json.loads(expected_path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        result.add(str(expected_path.name), "<json invalide>", str(exc), "json")
        return
    try:
        actual = json.loads(actual_path.read_text(encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001
        result.add(str(actual_path.name), "<attendu>", f"<json invalide: {exc}>", "json")
        return
    _diff_value(
        expected, actual, "", result, tolerance, time_tolerance,
        tolerances, set(ignore_keys), result.max_diffs,
    )


def _cell_equal(expected: str, actual: str, tolerance: float, time_tolerance: float) -> bool:
    if expected == actual:
        return True
    exp_tc = timecode_to_seconds(expected)
    act_tc = timecode_to_seconds(actual)
    if exp_tc is not None and act_tc is not None:
        return abs(exp_tc - act_tc) <= time_tolerance
    try:
        exp_num = float(expected)
        act_num = float(actual)
    except (TypeError, ValueError):
        return False
    return math.isclose(exp_num, act_num, rel_tol=tolerance, abs_tol=tolerance)


def compare_csv_files(
    expected_path: Path,
    actual_path: Path,
    result: DiffResult,
    tolerance: float = 1e-6,
    time_tolerance: float = 0.001,
) -> None:
    try:
        with open(expected_path, newline="", encoding="utf-8-sig") as handle:
            expected_rows = list(csv.reader(handle))
        with open(actual_path, newline="", encoding="utf-8-sig") as handle:
            actual_rows = list(csv.reader(handle))
    except Exception as exc:  # noqa: BLE001
        result.add(str(expected_path.name), "<csv>", str(exc), "csv")
        return

    if expected_rows and actual_rows:
        if expected_rows[0] != actual_rows[0]:
            result.add("header", expected_rows[0], actual_rows[0], "csv")
    if len(expected_rows) != len(actual_rows):
        result.add("rows", len(expected_rows), len(actual_rows), "csv")
    for row_index in range(min(len(expected_rows), len(actual_rows))):
        expected_row = expected_rows[row_index]
        actual_row = actual_rows[row_index]
        if len(expected_row) != len(actual_row):
            result.add(f"row[{row_index}]", len(expected_row), len(actual_row), "csv")
        for col_index in range(min(len(expected_row), len(actual_row))):
            if not _cell_equal(expected_row[col_index], actual_row[col_index], tolerance, time_tolerance):
                result.add(
                    f"row[{row_index}].col[{col_index}]",
                    expected_row[col_index],
                    actual_row[col_index],
                    "csv",
                )


def _fps(value: str | None) -> float | None:
    if not value:
        return None
    try:
        if "/" in value:
            num, den = value.split("/", 1)
            return float(num) / float(den) if float(den) else None
        return float(value)
    except (TypeError, ValueError, ZeroDivisionError):
        return None


def _probe(path: Path) -> dict[str, Any] | None:
    try:
        proc = subprocess.run(
            ["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)],
            capture_output=True, text=True, check=True, timeout=30,
        )
        return json.loads(proc.stdout)
    except Exception:  # noqa: BLE001
        return None


def compare_media_files(
    expected_path: Path,
    actual_path: Path,
    result: DiffResult,
    tolerance: float = 1e-6,
) -> None:
    expected = _probe(expected_path)
    actual = _probe(actual_path)
    if expected is None or actual is None:
        result.add(str(expected_path.name), "<probe>", "<probe impossible>", "media")
        return

    def streams(payload: dict[str, Any], kind: str) -> list[dict[str, Any]]:
        return [s for s in payload.get("streams", []) if s.get("codec_type") == kind]

    exp_video = streams(expected, "video")
    act_video = streams(actual, "video")
    if len(exp_video) != len(act_video):
        result.add("video_streams", len(exp_video), len(act_video), "media")
    for index in range(min(len(exp_video), len(act_video))):
        exp = exp_video[index]
        act = act_video[index]
        for key in ("codec_name", "width", "height", "pix_fmt", "field_order"):
            if exp.get(key) != act.get(key):
                result.add(f"video[{index}].{key}", exp.get(key), act.get(key), "media")
        for key in ("r_frame_rate", "avg_frame_rate"):
            exp_fps = _fps(exp.get(key))
            act_fps = _fps(act.get(key))
            if exp_fps is not None and act_fps is not None:
                if not math.isclose(exp_fps, act_fps, rel_tol=0.01, abs_tol=0.011):
                    result.add(f"video[{index}].{key}", exp.get(key), act.get(key), "media")
            elif exp.get(key) != act.get(key):
                result.add(f"video[{index}].{key}", exp.get(key), act.get(key), "media")
        for key in ("duration", "nb_frames"):
            exp_value = exp.get(key)
            act_value = act.get(key)
            if exp_value is None or act_value is None:
                continue
            try:
                delta = abs(float(exp_value) - float(act_value))
            except (TypeError, ValueError):
                continue
            frame_tolerance = 1.5 / max(_fps(exp.get("r_frame_rate")) or 25.0, 1.0)
            if delta > max(frame_tolerance, 0.05):
                result.add(f"video[{index}].{key}", exp_value, act_value, "media")

    exp_audio = streams(expected, "audio")
    act_audio = streams(actual, "audio")
    if len(exp_audio) != len(act_audio):
        result.add("audio_streams", len(exp_audio), len(act_audio), "media")
    for index in range(min(len(exp_audio), len(act_audio))):
        exp = exp_audio[index]
        act = act_audio[index]
        for key in ("codec_name", "sample_rate", "channels"):
            if exp.get(key) != act.get(key):
                result.add(f"audio[{index}].{key}", exp.get(key), act.get(key), "media")

    exp_duration = expected.get("format", {}).get("duration")
    act_duration = actual.get("format", {}).get("duration")
    if exp_duration and act_duration:
        try:
            if abs(float(exp_duration) - float(act_duration)) > 0.25:
                result.add("format.duration", exp_duration, act_duration, "media")
        except (TypeError, ValueError):
            pass


def compare_binary_files(expected_path: Path, actual_path: Path, result: DiffResult) -> None:
    exp_hash = sha256(expected_path)
    act_hash = sha256(actual_path)
    if exp_hash != act_hash:
        result.add(str(expected_path.name), exp_hash, act_hash, "sha256")


def compare_files(
    expected_path: Path,
    actual_path: Path,
    result: DiffResult,
    tolerance: float,
    time_tolerance: float,
    tolerances: Mapping[str, float] | None,
    ignore_keys: Iterable[str],
) -> None:
    suffix = expected_path.suffix.lower()
    if suffix == ".json":
        compare_json_files(
            expected_path, actual_path, result, tolerance, time_tolerance,
            tolerances, ignore_keys,
        )
    elif suffix == ".csv":
        compare_csv_files(expected_path, actual_path, result, tolerance, time_tolerance)
    elif suffix in {".mp4", ".mov", ".mkv", ".avi", ".webm", ".m4v"}:
        compare_media_files(expected_path, actual_path, result, tolerance)
    elif suffix in {".txt", ".log", ".md"}:
        exp_text = expected_path.read_text(encoding="utf-8", errors="replace")
        act_text = actual_path.read_text(encoding="utf-8", errors="replace")
        if exp_text != act_text:
            result.add(str(expected_path.name), "<texte différent>", "<texte différent>", "text")
    else:
        compare_binary_files(expected_path, actual_path, result)


def _relative_files(root: Path) -> list[Path]:
    files: list[Path] = []
    if not root.exists():
        return files
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        if any(part in IGNORED_NAMES for part in path.parts):
            continue
        if path.suffix.lower() in {".log", ".tmp", ".part"}:
            continue
        files.append(path.relative_to(root))
    return sorted(files)


def compare_dirs(
    expected_dir: Path,
    actual_dir: Path,
    tolerance: float = 1e-6,
    time_tolerance: float = 0.001,
    tolerances: Mapping[str, float] | None = None,
    ignore_keys: Iterable[str] = (),
    max_diffs: int = 100,
) -> DiffResult:
    result = DiffResult(max_diffs=max_diffs)
    expected_files = _relative_files(expected_dir)
    actual_files = _relative_files(actual_dir)
    result.expected_files = len(expected_files)
    result.actual_files = len(actual_files)

    expected_set = set(expected_files)
    actual_set = set(actual_files)
    for missing in sorted(expected_set - actual_set):
        result.missing.append(str(missing))
        result.equal = False
    for extra in sorted(actual_set - expected_set):
        result.extra.append(str(extra))
        result.equal = False

    for relative in sorted(expected_set & actual_set):
        compare_files(
            expected_dir / relative,
            actual_dir / relative,
            result,
            tolerance,
            time_tolerance,
            tolerances,
            ignore_keys,
        )
    return result


def render_text(result: DiffResult) -> str:
    lines = [
        f"equal: {result.equal}",
        f"files: expected={result.expected_files} actual={result.actual_files}",
        f"differences: {result.diff_count}",
    ]
    if result.missing:
        lines.append("missing:")
        lines.extend(f"  - {name}" for name in result.missing)
    if result.extra:
        lines.append("extra:")
        lines.extend(f"  - {name}" for name in result.extra)
    if result.differences:
        lines.append("details:")
        for diff in result.differences:
            lines.append(
                f"  {diff.path or '<root>'}: expected={_short(diff.expected)!r} "
                f"actual={_short(diff.actual)!r} ({diff.kind})"
            )
        if result.diff_count > len(result.differences):
            lines.append(f"  … {result.diff_count - len(result.differences)} différences supplémentaires")
    if result.notes:
        lines.append("notes:")
        lines.extend(f"  - {note}" for note in result.notes)
    return "\n".join(lines) + "\n"
