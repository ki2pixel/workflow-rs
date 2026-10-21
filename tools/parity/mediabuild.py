#!/usr/bin/env python3
"""Construction du corpus de parité.

Les médias générés sont déterministes et reproductibles. Le dossier
`parity/cases/` est ignoré par git : il contient aussi une image extraite
d'une production réelle pour le cas de tracking, qui ne doit pas être publiée.
"""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

OLD_BASE = Path("/home/kidpixel/workflow_mediapipe")
NEW_BASE = Path(__file__).resolve().parents[2]
CASES_DIR = NEW_BASE / "parity" / "cases"

SHORT_SOURCE = OLD_BASE / "scripts/MediaSolution-EC/tests/fixture/Camille-test/docs/clip1.mp4"
FACE_FRAME = OLD_BASE / "scratch/inspector/frame000025_idx0_score0.70.png"
VOX_WAV = OLD_BASE / "scratch/bench_step3_der/voxconverse/wav/bwzyf.wav"
VOX_RTTM = OLD_BASE / "scratch/bench_step3_der/voxconverse/rttm/bwzyf.rttm"


def _run(args: list[str], timeout: int = 300) -> None:
    proc = subprocess.run(
        ["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", *args],
        capture_output=True, text=True, timeout=timeout,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"ffmpeg a échoué ({proc.returncode}): {' '.join(args)}\n{proc.stderr}")


def _probe(path: Path) -> dict[str, Any]:
    proc = subprocess.run(
        ["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)],
        capture_output=True, text=True, check=True, timeout=30,
    )
    payload = json.loads(proc.stdout)
    streams = []
    for stream in payload.get("streams", []):
        streams.append({
            key: stream.get(key)
            for key in (
                "codec_type", "codec_name", "width", "height", "pix_fmt",
                "r_frame_rate", "avg_frame_rate", "field_order", "nb_frames",
                "sample_rate", "channels", "duration",
            )
            if stream.get(key) is not None
        })
    return {
        "format": {
            "duration": payload.get("format", {}).get("duration"),
            "size": payload.get("format", {}).get("size"),
            "format_name": payload.get("format", {}).get("format_name"),
        },
        "streams": streams,
    }


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def build_short(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    if not SHORT_SOURCE.exists():
        raise FileNotFoundError(f"fixture introuvable: {SHORT_SOURCE}")
    shutil.copy2(SHORT_SOURCE, path)


def build_cuts(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    with tempfile.TemporaryDirectory(prefix="parity_cuts_") as tmp:
        tmp_dir = Path(tmp)
        segments = [
            ("testsrc2=size=1280x720:rate=25:duration=5", "hue=h=0"),
            ("smptebars=size=1280x720:rate=25:duration=5", None),
            ("testsrc=size=1280x720:rate=25:duration=5", "hue=h=120"),
            ("color=c=0x1e3a8a:size=1280x720:rate=25:duration=5", None),
        ]
        concat_lines = []
        for index, (source, video_filter) in enumerate(segments):
            segment = tmp_dir / f"seg{index}.mp4"
            args = ["-f", "lavfi", "-i", source]
            if video_filter:
                args += ["-vf", video_filter]
            args += [
                "-c:v", "libx264", "-preset", "ultrafast", "-crf", "18",
                "-pix_fmt", "yuv420p", "-an", str(segment),
            ]
            _run(args)
            concat_lines.append(f"file '{segment}'")
        concat_file = tmp_dir / "concat.txt"
        concat_file.write_text("\n".join(concat_lines) + "\n", encoding="utf-8")
        silent_video = tmp_dir / "video.mp4"
        _run(["-f", "concat", "-safe", "0", "-i", str(concat_file), "-c", "copy", str(silent_video)])
        _run([
            "-i", str(silent_video),
            "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=20",
            "-c:v", "copy", "-c:a", "aac", "-b:a", "128k", "-shortest", str(path),
        ])


def build_vfr(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    # Un vrai VFR ne s'obtient pas avec `setpts` + `-vsync` sur cette version
    # de ffmpeg : le muxeur réimpose le framerate. On concatène donc deux
    # segments de framerates différents en copie de flux. Le résultat a des
    # durées de frames hétérogènes et un avg_frame_rate différent du r_frame_rate.
    with tempfile.TemporaryDirectory(prefix="parity_vfr_") as tmp:
        tmp_dir = Path(tmp)
        segments = []
        for fps in (24, 30):
            segment = tmp_dir / f"seg{fps}.mp4"
            _run([
                "-f", "lavfi", "-i", f"testsrc2=size=640x360:rate={fps}:duration=5",
                "-c:v", "libx264", "-preset", "ultrafast", "-crf", "20",
                "-pix_fmt", "yuv420p", "-an", str(segment),
            ])
            segments.append(segment)
        concat_file = tmp_dir / "concat.txt"
        concat_file.write_text(
            "\n".join(f"file '{segment}'" for segment in segments) + "\n",
            encoding="utf-8",
        )
        _run(["-f", "concat", "-safe", "0", "-i", str(concat_file), "-c", "copy", str(path)])

    probe = _probe(path)
    video = next((s for s in probe["streams"] if s.get("codec_type") == "video"), {})
    if video.get("avg_frame_rate") == video.get("r_frame_rate"):
        raise RuntimeError(
            f"le fichier VFR est resté CFR: r={video.get('r_frame_rate')} avg={video.get('avg_frame_rate')}"
        )


def build_interlaced(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    _run([
        "-f", "lavfi", "-i", "testsrc2=size=1920x1080:rate=25:duration=10",
        "-flags", "+ilme+ildct", "-x264-params", "tff=1",
        "-c:v", "libx264", "-preset", "ultrafast", "-crf", "20",
        "-pix_fmt", "yuv420p", "-an", str(path),
    ])


def build_silent(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    _run([
        "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25:duration=5",
        "-c:v", "libx264", "-preset", "ultrafast", "-crf", "20",
        "-pix_fmt", "yuv420p", "-an", str(path),
    ])


def build_alpha(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    _run([
        "-f", "lavfi", "-i",
        "color=c=black@0.0:size=640x360:rate=25:duration=2,format=rgba,"
        "geq=r='X*255/W':g='Y*255/H':b='128':a='255*X/W'",
        "-c:v", "prores_ks", "-profile:v", "4444",
        "-pix_fmt", "yuva444p10le", "-vendor", "apl0", str(path),
    ])
    probe = _probe(path)
    pix_fmts = [s.get("pix_fmt", "") for s in probe["streams"] if s.get("codec_type") == "video"]
    if not any(fmt.startswith("yuva") for fmt in pix_fmts):
        raise RuntimeError(f"le MOV alpha n'a pas de canal alpha: {pix_fmts}")


def build_face(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    if not FACE_FRAME.exists():
        raise FileNotFoundError(f"image de visage introuvable: {FACE_FRAME}")
    _run([
        "-loop", "1", "-i", str(FACE_FRAME),
        "-vf",
        "scale=720:960:force_original_aspect_ratio=increase,crop=720:960,"
        "zoompan=z='min(zoom+0.0005,1.2)':x='iw/2-(iw/zoom/2)':y='ih/2-(ih/zoom/2)':"
        "d=250:s=720x960:fps=25",
        "-frames:v", "250", "-c:v", "libx264", "-preset", "medium", "-crf", "18",
        "-pix_fmt", "yuv420p", "-an", str(path),
    ])


def _read_rttm(path: Path) -> list[tuple[float, float, str]]:
    intervals: list[tuple[float, float, str]] = []
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        parts = line.split()
        if len(parts) < 9 or parts[0] != "SPEAKER":
            continue
        try:
            start = float(parts[3])
            duration = float(parts[4])
        except ValueError:
            continue
        intervals.append((start, start + duration, parts[7]))
    return intervals


def _choose_window(intervals: list[tuple[float, float, str]], duration: float = 30.0) -> tuple[float, list[str]]:
    candidates = sorted({round(start, 3) for start, _end, _speaker in intervals})
    for start in candidates:
        end = start + duration
        speakers = sorted({
            speaker for seg_start, seg_end, speaker in intervals
            if seg_start < end and seg_end > start
        })
        if len(speakers) >= 2:
            return start, speakers
    return 0.0, sorted({speaker for _s, _e, speaker in intervals})


def build_vox(path: Path, force: bool) -> None:
    if path.exists() and not force:
        return
    if not VOX_WAV.exists() or not VOX_RTTM.exists():
        raise FileNotFoundError(f"fixtures VoxConverse introuvables: {VOX_WAV}, {VOX_RTTM}")
    intervals = _read_rttm(VOX_RTTM)
    start, speakers = _choose_window(intervals, 30.0)
    with tempfile.TemporaryDirectory(prefix="parity_vox_") as tmp:
        excerpt = Path(tmp) / "excerpt.wav"
        _run(["-ss", str(start), "-t", "30", "-i", str(VOX_WAV), "-c:a", "pcm_s16le", str(excerpt)])
        _run([
            "-f", "lavfi", "-i", "testsrc2=size=640x360:rate=25:duration=30",
            "-i", str(excerpt),
            "-c:v", "libx264", "-preset", "ultrafast", "-crf", "23",
            "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "128k",
            "-shortest", str(path),
        ])
    sidecar = path.with_suffix(".json")
    sidecar.write_text(
        json.dumps({
            "source_wav": str(VOX_WAV),
            "source_rttm": str(VOX_RTTM),
            "window_start_s": start,
            "window_duration_s": 30.0,
            "speakers": speakers,
        }, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def build_all(force: bool = False, only: list[str] | None = None) -> dict[str, Any]:
    CASES_DIR.mkdir(parents=True, exist_ok=True)
    builders = {
        "short_2997.mp4": build_short,
        "synthetic_cuts.mp4": build_cuts,
        "synthetic_vfr.mp4": build_vfr,
        "synthetic_interlaced.mp4": build_interlaced,
        "synthetic_silent.mp4": build_silent,
        "synthetic_alpha.mov": build_alpha,
        "face_pan.mp4": build_face,
        "vox_2spk.mp4": build_vox,
    }
    manifest: dict[str, Any] = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "files": {},
    }
    lock_path = CASES_DIR / "corpus.lock.json"
    if lock_path.exists():
        try:
            previous = json.loads(lock_path.read_text(encoding="utf-8"))
            manifest["files"].update(previous.get("files", {}))
        except Exception:  # noqa: BLE001
            pass

    failures: list[str] = []
    for name, builder in builders.items():
        if only and name not in only:
            continue
        path = CASES_DIR / name
        try:
            builder(path, force)
            manifest["files"][name] = {
                "size": path.stat().st_size,
                "sha256": _sha256(path),
                "probe": _probe(path),
            }
            print(f"ok  {name}  {path.stat().st_size / 1024 / 1024:.1f} MiB")
        except Exception as exc:  # noqa: BLE001
            failures.append(f"{name}: {exc}")
            print(f"ko  {name}  {exc}")

    lock_path.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    if failures:
        raise SystemExit("Construction incomplète:\n- " + "\n- ".join(failures))
    return manifest


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Construit le corpus de parité")
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--only", nargs="*", help="Noms de fichiers à construire")
    args = parser.parse_args()
    build_all(force=args.force, only=args.only)
