#!/usr/bin/env python3
"""Build labelled source/export contact sheets for the qualitative review."""
from __future__ import annotations

import subprocess
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

import common

OUT = common.out_dir() / "frames"
OUT.mkdir(parents=True, exist_ok=True)

CASES = [
    {
        "id": "a1_305",
        "folder": "305 Camille",
        "stem": "6ter - LRDRLD -  Chauffeuse poids lourd et enceinte",
        "times": [1.0, 20.0, 33.0],
        "note": "A1 apply (17 segments), human 210% vs A1 218.9%, residual 4.6 px",
    },
    {
        "id": "a1_68",
        "folder": "68 Camille",
        "stem": "6ter - De l'autre côté du périph",
        "times": [1.0, 4.0, 15.0],
        "note": "A1 apply accepted + human keyframes",
    },
    {
        "id": "a1_304_review",
        "folder": "304 Camille",
        "stem": "Cette voiture percute une maison !",
        "times": [44.0, 57.0, 65.0],
        "note": "A1 review asymetrique / top 642 px",
    },
    {
        "id": "a2_54_apply",
        "folder": "54 Camille",
        "stem": "UJUD - CLIENT HOTEL",
        "times": [1.0, 5.0, 30.0],
        "note": "A2 apply accepted, banner 76 px",
    },
    {
        "id": "a2_77_partial",
        "folder": "77 Camille",
        "stem": "UJUD - CHAT HYPOALLERGENIQUE",
        "times": [1.0, 4.0, 20.0],
        "note": "A2 review partial width 39%, human removed",
    },
    {
        "id": "a2_80_partial",
        "folder": "80 Camille",
        "stem": "Leur belle-mère leur pourrit la vie !",
        "times": [1.0, 4.0, 20.0],
        "note": "A2 review partial width 37%, human removed",
    },
    {
        "id": "a2_325_logo",
        "folder": "325 Camille",
        "stem": "Océane découvre que Cédric a embrassé Charlotte sur le front et elle est très en colère contre lui",
        "times": [1.0, 4.0, 20.0],
        "note": "A2 review logo / watermark, veto",
    },
    {
        "id": "a2_40_citrouilles",
        "folder": "40 Camille",
        "stem": "66M - CITROUILLES",
        "times": [1.0, 5.0, 20.0],
        "note": "A2 apply reverted, 11/48 segments",
    },
    {
        "id": "a2_81_reverted",
        "folder": "81 Camille",
        "stem": "Ils pensent que leur belle-mère n'est là que pour l'argent !",
        "times": [1.0, 4.0, 20.0],
        "note": "A2 apply reverted, 0/23 segments",
    },
    {
        "id": "a3_313_static",
        "folder": "313 Camille",
        "stem": "W9 - LAA9 - post-diff 1 EP38 – Les Gris perdent face à l’abandon d’Emi",
        "times": [0.4, 4.0, 14.0],
        "note": "A3 noop, human static reframe -1094/+493 px",
    },
    {
        "id": "a3_54_tracking",
        "folder": "54 Camille",
        "stem": "UJUD - CLIENT HOTEL",
        "times": [24.5, 50.0, 55.0],
        "note": "A3 review, human keyframes, tracking",
    },
    {
        "id": "a4_79_split",
        "folder": "79 Camille",
        "stem": "Gulli - MSL1 - Ripart fait une surprise à ces deux supporters",
        "times": [30.4, 30.8, 31.2],
        "note": "A4 review 0.40 s, human split (3 parts)",
    },
    {
        "id": "a4_8_flash",
        "folder": "8 Camille",
        "stem": "Gulli - Totally Spies - Volley ball",
        "times": [0.04, 0.12, 0.36],
        "note": "A4 review 0.12 s, human left it",
    },
    {
        "id": "a5_62",
        "folder": "62 Camille",
        "stem": "SHORT_oléagineux",
        "times": [1.0, 4.0, 12.0],
        "note": "A5 review source ne couvre pas, human reframe",
    },
    {
        "id": "a6_328",
        "folder": "328 Camille",
        "stem": "M6 - SDM - Leslie et Léo",
        "times": [2.0, 10.0, 18.0],
        "note": "A6 template SDM apply",
    },
]


def frame(video: Path, t: float, dest: Path) -> bool:
    cmd = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-ss",
        f"{t:.3f}",
        "-i",
        str(video),
        "-frames:v",
        "1",
        "-q:v",
        "2",
        "-y",
        str(dest),
    ]
    return subprocess.run(cmd, capture_output=True).returncode == 0


def fit(image: Image.Image, height: int) -> Image.Image:
    ratio = height / image.height
    return image.resize((int(image.width * ratio), height), Image.LANCZOS)


def main() -> None:
    project_index = {
        f"{p['folder']}/{p['stem']}": p for p in common.discover()
    }
    for case in CASES:
        key = f"{case['folder']}/{case['stem']}"
        project = project_index[key]
        rows = []
        for t in case["times"]:
            src = OUT / f"{case['id']}_{t}_source.png"
            exp = OUT / f"{case['id']}_{t}_export.png"
            frame(project["source_video"], t, src)
            frame(project["export_video"], t, exp)
            rows.append((t, src, exp))
        width = 1400
        height = 70 + len(rows) * 440
        sheet = Image.new("RGB", (width, height), (18, 18, 20))
        draw = ImageDraw.Draw(sheet)
        try:
            font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 18)
            small = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 15)
        except Exception:
            font = ImageFont.load_default()
            small = font
        draw.text((12, 8), f"{case['id']} — {key}", fill=(255, 255, 255), font=font)
        draw.text((12, 32), case["note"], fill=(200, 200, 200), font=small)
        y = 66
        for t, src, exp in rows:
            if src.is_file():
                img = fit(Image.open(src).convert("RGB"), 360)
                sheet.paste(img, (12, y + 28))
            if exp.is_file():
                img = fit(Image.open(exp).convert("RGB"), 360)
                sheet.paste(img, (740, y + 28))
            draw.text((12, y + 4), f"source t={t:.2f}s", fill=(255, 200, 120), font=small)
            draw.text((740, y + 4), f"export t={t:.2f}s", fill=(120, 220, 255), font=small)
            y += 440
        dest = OUT / f"{case['id']}.png"
        sheet.save(dest)
        print(dest)


if __name__ == "__main__":
    main()
