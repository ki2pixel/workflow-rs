#!/usr/bin/env python3
"""Expérience read-only : compare cropdetect A1 et l'algorithme vsletterbox.

Aucune écriture hors de /tmp/opencode. Les vidéos du corpus sont lues en
lecture seule. Le script implémente le coeur de vsletterbox :
  - signal `pre` = Y + 0.25*|U-128| + 0.25*|V-128| (YUV full range) ;
  - masque ExKirsch 5x5 (8 orientations, max, diviseur 1) ;
  - scan haut/bas avec EWMA alpha=0.05 et seuil mu + 5*max(sigma, 0.005) ;
  - validation de la ligne par densité de contours > 2/3.
"""
from __future__ import annotations

import argparse
import json
import subprocess
import sys
from dataclasses import dataclass

import numpy as np

ALPHA = 0.05
MIN_SIGMA = 0.005
REF_THR = 2.0 / 3.0

# 8 noyaux ExKirsch 5x5 (vsmasktools/edge/_5x5.py). Somme nulle -> diviseur 1.
KIRSCH = [
    [9, 9, 9, 9, 9, 9, 5, 5, 5, 9, -7, -3, 0, -3, -7, -7, -3, -3, -3, -7, -7, -7, -7, -7, -7],
    [9, 9, 9, 9, -7, 9, 5, 5, -3, -7, 9, 5, 0, -3, -7, 9, -3, -3, -3, -7, -7, -7, -7, -7, -7],
    [9, 9, -7, -7, -7, 9, 5, -3, -3, -7, 9, 5, 0, -3, -7, 9, 5, -3, -3, -7, 9, 9, -7, -7, -7],
    [-7, -7, -7, -7, -7, 9, -3, -3, -3, -7, 9, 5, 0, -3, -7, 9, 5, 5, -3, -7, 9, 9, 9, 9, -7],
    [-7, -7, -7, -7, -7, -7, -3, -3, -3, -7, -7, -3, 0, -3, -7, 9, 5, 5, 5, 9, 9, 9, 9, 9, 9],
    [-7, -7, -7, -7, -7, -7, -3, -3, -3, 9, -7, -3, 0, 5, 9, -7, -3, 5, 5, 9, -7, 9, 9, 9, 9],
    [-7, -7, -7, 9, 9, -7, -3, -3, 5, 9, -7, -3, 0, 5, 9, -7, -3, -3, 5, 9, -7, -7, -7, 9, 9],
    [-7, 9, 9, 9, 9, -7, -3, 5, 5, 9, -7, -3, 0, 5, 9, -7, -3, -3, -3, 9, -7, -7, -7, -7, -7],
]
KIRSCH = [np.asarray(k, dtype=np.float32).reshape(5, 5) for k in KIRSCH]


@dataclass
class Detection:
    top: int
    bottom: int
    top_edge: float | None
    bottom_edge: float | None
    top_jump: float | None
    bottom_jump: float | None
    top_trigger_mean: float | None
    bottom_trigger_mean: float | None


def extract_frames(video: str, start: float, duration: float, fps: float, width: int, height: int):
    """Extrait des frames yuv444p en full range, une par seconde."""
    cmd = [
        "ffmpeg", "-v", "error", "-ss", f"{start:.3f}", "-i", video,
        "-t", f"{duration:.3f}", "-vf", f"fps={fps},scale={width}:{height}:flags=bicubic",
        "-pix_fmt", "yuv444p", "-f", "rawvideo", "-",
    ]
    raw = subprocess.run(cmd, check=True, stdout=subprocess.PIPE).stdout
    frame_size = width * height * 3
    if len(raw) % frame_size:
        raise RuntimeError(f"taille brute inattendue: {len(raw)} % {frame_size}")
    count = len(raw) // frame_size
    frames = np.frombuffer(raw, dtype=np.uint8).reshape(count, 3, height, width)
    return frames


def pre_signal(frame: np.ndarray) -> np.ndarray:
    y = frame[0].astype(np.float32)
    u = frame[1].astype(np.float32) - 128.0
    v = frame[2].astype(np.float32) - 128.0
    return np.clip(y + 0.25 * np.abs(u) + 0.25 * np.abs(v), 0.0, 255.0)


def exkirsch(pre: np.ndarray) -> np.ndarray:
    """Convolution 5x5, 8 orientations, max ; clamp 8-bit."""
    # pad de 2 lignes/colonnes (bord reproduit), comme un filtre spatial.
    padded = np.pad(pre, 2, mode="edge")
    h, w = pre.shape
    out = np.full((h, w), -1e9, dtype=np.float32)
    for kernel in KIRSCH:
        acc = np.zeros((h, w), dtype=np.float32)
        for dy in range(5):
            for dx in range(5):
                weight = kernel[dy, dx]
                if weight:
                    acc += weight * padded[dy:dy + h, dx:dx + w]
        np.maximum(out, acc, out=out)
    return np.clip(out, 0.0, 255.0)


class Ewma:
    __slots__ = ("init", "mean", "var")

    def __init__(self):
        self.init = False
        self.mean = 0.0
        self.var = 0.0

    def add(self, value: float):
        if not self.init:
            self.init = True
            self.mean = value
            self.var = 0.0
        else:
            diff = value - self.mean
            incr = ALPHA * diff
            self.mean += incr
            self.var = (1.0 - ALPHA) * (self.var + diff * incr)

    def threshold(self) -> float:
        return self.mean + 5.0 * max(self.var ** 0.5, MIN_SIGMA)


def scan(pre: np.ndarray, edge: np.ndarray) -> Detection:
    h = pre.shape[0]
    row_pre = pre.mean(axis=1) / 255.0
    row_edge = edge.mean(axis=1) / 255.0

    top = 0
    top_edge = None
    top_jump = None
    top_trigger_mean = None
    stats = Ewma()
    for y in range(h):
        value = float(row_pre[y])
        if stats.init and value > stats.threshold():
            if row_edge[y] > REF_THR:
                top = y
                top_edge = float(row_edge[y])
                top_jump = value - stats.mean
                top_trigger_mean = value
                break
        stats.add(value)

    bottom = h - 1
    bottom_edge = None
    bottom_jump = None
    bottom_trigger_mean = None
    stats = Ewma()
    for y in range(h - 1, -1, -1):
        value = float(row_pre[y])
        if stats.init and value > stats.threshold():
            if row_edge[y] > REF_THR:
                bottom = y
                bottom_edge = float(row_edge[y])
                bottom_jump = value - stats.mean
                bottom_trigger_mean = value
                break
        stats.add(value)

    return Detection(top, bottom, top_edge, bottom_edge, top_jump, bottom_jump,
                     top_trigger_mean, bottom_trigger_mean)


def cropdetect(video: str, start: float, duration: float, limit: int = 24, round_to: int = 16):
    cmd = [
        "ffmpeg", "-hide_banner", "-ss", f"{start:.3f}", "-i", video,
        "-t", f"{duration:.3f}", "-vf", f"cropdetect={limit}:{round_to}:0",
        "-f", "null", "-",
    ]
    proc = subprocess.run(cmd, stderr=subprocess.PIPE, stdout=subprocess.DEVNULL, text=True)
    crops = []
    for line in proc.stderr.splitlines():
        if "crop=" in line:
            value = line.split("crop=")[-1].strip()
            try:
                w, h, x, y = (int(part) for part in value.split(":"))
                crops.append((w, h, x, y))
            except ValueError:
                pass
    return crops


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("video")
    parser.add_argument("--start", type=float, required=True)
    parser.add_argument("--duration", type=float, required=True)
    parser.add_argument("--fps", type=float, default=1.0)
    parser.add_argument("--width", type=int, default=1920)
    parser.add_argument("--height", type=int, default=1080)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    frames = extract_frames(args.video, args.start, args.duration, args.fps, args.width, args.height)
    rows = []
    for index, frame in enumerate(frames):
        pre = pre_signal(frame)
        edge = exkirsch(pre)
        det = scan(pre, edge)
        rows.append({
            "frame": index,
            "t": round(args.start + index / args.fps, 3),
            "top": det.top,
            "bottom": det.bottom,
            "top_bar": det.top,
            "bottom_bar": args.height - 1 - det.bottom,
            "top_edge": None if det.top_edge is None else round(det.top_edge, 3),
            "bottom_edge": None if det.bottom_edge is None else round(det.bottom_edge, 3),
            "top_jump": None if det.top_jump is None else round(det.top_jump, 3),
            "bottom_jump": None if det.bottom_jump is None else round(det.bottom_jump, 3),
        })

    crops = cropdetect(args.video, args.start, args.duration)
    result = {"video": args.video, "start": args.start, "duration": args.duration,
              "frames": rows, "cropdetect": crops}
    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(f"video={args.video} start={args.start} duration={args.duration} frames={len(rows)}")
        for row in rows:
            print(f"  t={row['t']:.3f} top={row['top']:4d} bottom={row['bottom']:4d} "
                  f"bars=({row['top_bar']},{row['bottom_bar']}) "
                  f"edge=({row['top_edge']},{row['bottom_edge']}) "
                  f"jump=({row['top_jump']},{row['bottom_jump']})")
        if crops:
            hs = sorted(c[1] for c in crops)
            print(f"  cropdetect: n={len(crops)} h_min={hs[0]} h_med={hs[len(hs)//2]} h_max={hs[-1]}")
            from collections import Counter
            print(f"  cropdetect top: {Counter(crops).most_common(3)}")
        else:
            print("  cropdetect: aucun crop")
    return 0


if __name__ == "__main__":
    sys.exit(main())
