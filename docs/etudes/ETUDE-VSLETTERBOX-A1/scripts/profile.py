#!/usr/bin/env python3
"""Diagnostic des profils de lignes pour une frame donnée."""
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from analyze import exkirsch, pre_signal  # noqa: E402


def frame_at(video: str, t: float, width: int = 1920, height: int = 1080):
    cmd = [
        "ffmpeg", "-v", "error", "-ss", f"{t:.3f}", "-i", video,
        "-frames:v", "1", "-vf", f"scale={width}:{height}:flags=bicubic",
        "-pix_fmt", "yuv444p", "-f", "rawvideo", "-",
    ]
    raw = subprocess.run(cmd, check=True, stdout=subprocess.PIPE).stdout
    return np.frombuffer(raw, dtype=np.uint8).reshape(3, height, width)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("video")
    parser.add_argument("time", type=float)
    parser.add_argument("--from", dest="start", type=int, default=0)
    parser.add_argument("--to", dest="end", type=int, default=None)
    parser.add_argument("--top", type=int, default=25)
    args = parser.parse_args()

    frame = frame_at(args.video, args.time)
    height = frame.shape[1]
    end = height if args.end is None else min(height, args.end)
    pre = pre_signal(frame)
    edge = exkirsch(pre)
    row_pre = pre.mean(axis=1) / 255.0
    row_edge = edge.mean(axis=1) / 255.0
    # fraction de la ligne dont la réponse Kirsch dépasse 128 (mi-échelle)
    edge_frac = (edge > 128.0).mean(axis=1)
    print(f"t={args.time} rows {args.start}..{end - 1}")
    print(" y   pre    edge   frac>128  dpre")
    for y in range(max(0, args.start), end):
        dpre = row_pre[y] - row_pre[y - 1] if y > 0 else 0.0
        print(f"{y:4d} {row_pre[y]:.4f} {row_edge[y]:.4f} {edge_frac[y]:.4f} {dpre:+.4f}")
    print("\nTop edge rows:")
    order = np.argsort(row_edge)[::-1][: args.top]
    for y in sorted(int(v) for v in order):
        print(f"  y={y:4d} edge={row_edge[y]:.4f} frac>128={edge_frac[y]:.4f} pre={row_pre[y]:.4f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
