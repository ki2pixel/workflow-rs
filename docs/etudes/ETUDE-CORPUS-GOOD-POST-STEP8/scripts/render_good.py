#!/usr/bin/env python3
"""Render a GOOD/ASSIST layer transform from the source frame.

Used to validate geometry on projects without an export (305) and to compare
the project transform with the actual export.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFont

import common

OUT = common.out_dir() / "render"
OUT.mkdir(parents=True, exist_ok=True)


def grab(video: Path, t: float) -> np.ndarray | None:
    cmd = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-ss",
        f"{max(0.0, t):.3f}",
        "-i",
        str(video),
        "-frames:v",
        "1",
        "-f",
        "image2pipe",
        "-vcodec",
        "png",
                "-",
    ]
    proc = subprocess.run(cmd, capture_output=True)
    if proc.returncode != 0 or not proc.stdout:
        return None
    arr = np.frombuffer(proc.stdout, np.uint8)
    return cv2.imdecode(arr, cv2.IMREAD_COLOR)


def render(project: common.Project, layer: dict, src: np.ndarray, t: float) -> np.ndarray:
    aff = project.affine_at(layer, t)
    matrix = np.array(
        [
            [aff["sx"], aff["cross_x"], aff["ox"]],
            [aff["cross_y"], aff["sy"], aff["oy"]],
        ],
        dtype=np.float64,
    )
    return cv2.warpAffine(
        src,
        matrix,
        (int(common.COMP_W), int(common.COMP_H)),
        flags=cv2.INTER_LINEAR,
        borderMode=cv2.BORDER_CONSTANT,
        borderValue=(0, 0, 0),
    )


def label(image: np.ndarray, text: str) -> Image.Image:
    img = Image.fromarray(cv2.cvtColor(image, cv2.COLOR_BGR2RGB))
    canvas = Image.new("RGB", (img.width, img.height + 30), (15, 15, 15))
    canvas.paste(img, (0, 30))
    draw = ImageDraw.Draw(canvas)
    try:
        font = ImageFont.truetype(
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 16
        )
    except Exception:
        font = ImageFont.load_default()
    draw.text((6, 6), text, fill=(255, 220, 120), font=font)
    return canvas


def find_layer(project: common.Project, name: str) -> dict:
    for layer in project.comp.get("layers", []):
        if layer.get("name") == name:
            return layer
    raise KeyError(name)


def compose_305() -> None:
    """Compose la comparaison ASSIST/GOOD du 305 (pas d'export disponible)."""
    left = OUT / "305 Camille_1_1.0_assist.png"
    right = OUT / "305 Camille_1_1.0_good.png"
    if not (left.is_file() and right.is_file()):
        return
    height = 1422
    tiles = []
    for path in (left, right):
        image = Image.open(path).convert("RGB")
        width = round(image.width * height / image.height)
        tiles.append(image.resize((width, height), Image.LANCZOS))
    gap = 30
    sheet = Image.new(
        "RGB", (tiles[0].width + tiles[1].width + gap, height + 60), (16, 16, 18)
    )
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.truetype(
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 22
    )
    draw.text(
        (12, 8),
        "305 Chauffeuse — segment 1, même frame source : aucun bord noir dans les deux cas",
        fill=(255, 255, 255),
        font=font,
    )
    draw.text(
        (12, 34), "ASSIST 218,9 % (A1) — 4 % de cadre en moins", fill=(255, 190, 120), font=font
    )
    draw.text(
        (tiles[0].width + 24, 34),
        "GOOD 210 % (monteur) — cadrage conservé",
        fill=(120, 220, 255),
        font=font,
    )
    sheet.paste(tiles[0], (10, 60))
    sheet.paste(tiles[1], (tiles[0].width + 20, 60))
    sheet.save(OUT / "A1-305-assist-vs-good.jpg", "JPEG", quality=90, optimize=True)
    print(OUT / "A1-305-assist-vs-good.jpg")


def main() -> None:
    cases = [
        ("305 Camille", "6ter - LRDRLD -  Chauffeuse poids lourd et enceinte", 1.0, 1),
        ("305 Camille", "6ter - LRDRLD -  Chauffeuse poids lourd et enceinte", 1.0, 18),
        ("68 Camille", "6ter - De l'autre côté du périph", 1.0, 1),
        ("356 Camille", "M6+ - Pretty Little Liars - Extrait 6 - La mort de Maya", 2.6, 2),
        ("40 Camille", "66M - CITROUILLES", 1.0, 1),
    ]
    projects = {f"{p['folder']}/{p['stem']}": p for p in common.discover()}
    tiles = []
    for folder, stem, t, seg in cases:
        p = projects[f"{folder}/{stem}"]
        a = common.Project(common.load_json(p["assist"]))
        g = common.Project(common.load_json(p["good"]))
        names_a = [
            l.get("name")
            for l in a.comp.get("layers", [])
            if str(l.get("name", "")).startswith("Segment")
        ]
        names_g = [
            l.get("name")
            for l in g.comp.get("layers", [])
            if str(l.get("name", "")).startswith("Segment")
        ]
        name_a = names_a[seg - 1]
        # Match GOOD by base + same in/out when possible.
        base = common.base_name(name_a)
        name_g = next((n for n in names_g if common.base_name(n) == base), names_g[seg - 1])
        la = find_layer(a, name_a)
        lg = find_layer(g, name_g)
        src_t = t - float(la.get("start_time") or 0) / common.TICKS_PER_SECOND
        src = grab(p["source_video"], src_t)
        if src is None:
            print("grab failed", folder, stem, t)
            continue
        ra = render(a, la, src, t)
        rg = render(g, lg, src, t)
        tiles.append(label(ra, f"{folder}/{stem} — ASSIST seg {seg} t={t}"))
        tiles.append(label(rg, f"{folder}/{stem} — GOOD seg {seg} t={t}"))
        cv2.imwrite(str(OUT / f"{folder}_{seg}_{t}_assist.png"), ra)
        cv2.imwrite(str(OUT / f"{folder}_{seg}_{t}_good.png"), rg)
    if not tiles:
        return
    compose_305()
    width = sum(tile.width for tile in tiles) + 10 * (len(tiles) + 1)
    height = max(tile.height for tile in tiles) + 20
    sheet = Image.new("RGB", (width, height), (12, 12, 12))
    x = 10
    for tile in tiles:
        sheet.paste(tile, (x, 10))
        x += tile.width + 10
    sheet.save(OUT / "render_compare.png")
    print(OUT / "render_compare.png")


if __name__ == "__main__":
    main()
