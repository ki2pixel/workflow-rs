#!/usr/bin/env python3
"""Tests synthétiques du critère EWMA + ExKirsch de vsletterbox.

But : vérifier le comportement sur titres/incrustations qui empiètent sur une
barre noire, et sur des scènes sombres sans barre. Aucune vidéo, aucun fichier
de production : tout est calculé en mémoire.
"""
from __future__ import annotations

import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from analyze import exkirsch, scan  # noqa: E402

H, W = 1080, 1920


def base(active_value: float = 80.0, active_top: int = 200, active_bottom: int = 879,
         noise: float = 12.0, seed: int = 7) -> np.ndarray:
    rng = np.random.default_rng(seed)
    frame = np.zeros((H, W), dtype=np.float32)
    frame[active_top:active_bottom + 1, :] = active_value
    frame[active_top:active_bottom + 1, :] += rng.normal(0, noise, (active_bottom - active_top + 1, W))
    return np.clip(frame, 0, 255)


def solid_band(frame: np.ndarray, y: int, coverage: float, value: float = 255.0, height: int = 18):
    width = int(W * coverage)
    x0 = (W - width) // 2
    frame[y:y + height, x0:x0 + width] = value
    return frame


def text_band(frame: np.ndarray, y: int, coverage: float, value: float = 255.0, height: int = 18):
    """Strokes verticaux espacés : simule des lettres, pas un aplat."""
    width = int(W * coverage)
    x0 = (W - width) // 2
    for x in range(x0, x0 + width, 14):
        frame[y:y + height, x:x + 5] = value
    return frame


def night_gradient(seed: int = 11) -> np.ndarray:
    rng = np.random.default_rng(seed)
    frame = np.zeros((H, W), dtype=np.float32)
    # ciel nocturne : dégradé lent, bruit faible, un nuage plus clair
    for y in range(H):
        frame[y, :] = 4.0 + 10.0 * (y / H)
    frame += rng.normal(0, 1.5, (H, W))
    # nuage large mais peu contrasté
    frame[280:340, 300:1500] += 25.0
    # zone de route plus claire en bas
    frame[800:, :] += 30.0
    return np.clip(frame, 0, 255)


def run(name: str, frame: np.ndarray):
    edge = exkirsch(frame)
    det = scan(frame, edge)
    top_bar = det.top
    bottom_bar = H - 1 - det.bottom
    print(f"{name:24s} top={det.top:4d} bottom={det.bottom:4d} "
          f"bars=({top_bar:4d},{bottom_bar:4d}) "
          f"edge=({det.top_edge},{det.bottom_edge}) "
          f"jump=({det.top_jump},{det.bottom_jump})")


def main() -> int:
    run("solid20", solid_band(base(), 50, 0.20))
    run("solid60", solid_band(base(), 50, 0.60))
    run("solid70", solid_band(base(), 50, 0.70))
    run("solid100", solid_band(base(), 50, 1.00))
    run("text30", text_band(base(), 50, 0.30))
    run("text60", text_band(base(), 50, 0.60))
    run("text80", text_band(base(), 50, 0.80))
    run("night_gradient", night_gradient())
    run("low_contrast_12", base(active_value=12.0, active_top=100, active_bottom=979,
                                noise=1.0))
    run("low_contrast_20", base(active_value=20.0, active_top=100, active_bottom=979,
                                noise=1.0))
    return 0


if __name__ == "__main__":
    sys.exit(main())
