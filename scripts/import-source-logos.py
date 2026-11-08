#!/usr/bin/env python3
"""Génère le registre spatial des logos source (D-052).

Le script lit le CSV de référence du client et les crops RGBA associés,
calcule les boîtes alpha (demi-ouvertes) et écrit un catalogue TOML
versionnable. Il ne copie aucune image : les crops restent des assets locaux,
pointés par `emission.variants[].crop` relativement à `reference_root`.

Exemple :
  python3 scripts/import-source-logos.py \
    --csv "/media/.../SELECTION_VIDEOS_SOURCES_LOGOS.csv" \
    --references "/media/.../references_logos/emissions" \
    --out assets/models/qa/source_logos.toml
"""

from __future__ import annotations

import argparse
import csv
import json
import os
from collections import OrderedDict
from datetime import date
from pathlib import Path

import numpy as np
from PIL import Image

# Alias de reconnaissance du nom de fichier. Un alias terminé par « - » est
# un préfixe ; les autres sont des sous-chaînes normalisées. Ces indices ne
# remplacent jamais la confirmation visuelle.
ALIASES = {
    "66 Minutes": ["66M -", "66M-", "66 minutes"],
    "Un Jour Un Doc": ["UJUD -", "UJUD-", "Un Jour Un Doc"],
    "Cauchemar en cuisine": ["M6 - CEC -", "M6 -CEC -", "Cauchemar en cuisine"],
    "L'amour est dans le pré": ["M6 - ADP -", "ADP -", "L'amour est dans le pré"],
    "Mariés au premier regard": ["M6 - MAPR -", "MAPR -", "Mariés au premier regard"],
    "Les Traîtres": ["M6 - TRAITRES -", "TRAITRES -", "Les Traîtres"],
    "Le Meilleur Combat des Régions": [
        "M6 - LMCR -",
        "LMCR -",
        "Le Meilleur Combat des Régions",
    ],
    "Top Chef": ["M6 - Top Chef -", "Top Chef"],
    "Pékin Express": ["M6 - Pékin Express -", "Pékin Express", "Pekin Express"],
    "Ma Super Ligue 1": ["Gulli - Ma Super Ligue 1", "Ma Super Ligue 1", "MSL1"],
    "Piquantes !": ["Piquantes", "TEVA_Piquantes"],
    "Les Grosses Têtes": [
        "PP - Les grosses têtes",
        "Les Grosses Têtes",
        "Les Grosses Tetes",
    ],
    "Les Mamans": ["6ter - Les mamans", "Les Mamans"],
    "Norbert commis d'office": ["6TER_norbert", "Norbert commis"],
    "E=M6": ["M6 - EM6", "EM6_", "E=M6"],
    "Enquête d'action": ["Enquête d'action", "Enquete d'action"],
    "Enquête Exclusive": ["Enquête Exclusive", "Enquete Exclusive"],
    "Capital": ["Capital"],
    "Zone Interdite": ["Zone Interdite"],
    "Les Marseillais": ["Marseillais"],
    "Les Apprentis Aventuriers": ["Apprentis Aventuriers", "LAA9", "LAA"],
    "Le Reste du Monde": ["Reste du Monde", "RDM"],
    "Le Cross": ["Le Cross"],
    "Les Ch'tis": ["Ch'tis", "Chtis"],
    "Cougar Island": ["Cougar Island"],
    "L'Île de la tentation": ["Île de la tentation", "Ile de la tentation"],
    "Qui veut épouser mon fils ?": ["Qui veut épouser mon fils", "QVEMF"],
    "C'est la famille": ["C'est la famille", "Cest la famille"],
    "Le Maillon Faible": ["Maillon Faible"],
}


def alpha_bbox(path: Path) -> tuple[tuple[int, int, int, int], int] | None:
    with Image.open(path) as image:
        alpha = np.array(image.getchannel("A"))
    ys, xs = np.where(alpha > 0)
    if len(xs) == 0:
        return None
    bbox = (int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1)
    return bbox, int((alpha > 0).sum())


def union_bbox(boxes: list[tuple[int, int, int, int]]) -> tuple[int, int, int, int]:
    return (
        min(box[0] for box in boxes),
        min(box[1] for box in boxes),
        max(box[2] for box in boxes),
        max(box[3] for box in boxes),
    )


def toml_value(value) -> str:
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return repr(value)
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    raise TypeError(f"valeur TOML non prise en charge: {type(value)!r}")


def write_catalog(path: Path, catalog: dict) -> None:
    lines: list[str] = []
    lines.append("# Registre spatial des logos de programme (D-052).")
    lines.append("# Les bboxes sont demi-ouvertes, en pixels du canvas source 1920x1080.")
    lines.append("# Les crops de référence restent hors dépôt ; ce fichier ne")
    lines.append("# contient que des métadonnées auditables.")
    lines.append("")
    for key in (
        "schema",
        "generated_at",
        "source",
        "source_resolution",
        "reference_root",
    ):
        lines.append(f"{key} = {toml_value(catalog[key])}")
    lines.append("")
    lines.append("[zones]")
    for key, value in catalog["zones"].items():
        lines.append(f"{key} = {toml_value(value)}")
    lines.append("")

    for emission in catalog["emissions"]:
        lines.append("[[emissions]]")
        for key in ("name", "channel", "position", "dossier", "bbox", "aliases", "sources", "notes"):
            lines.append(f"{key} = {toml_value(emission[key])}")
        for variant in emission["variants"]:
            lines.append("")
            lines.append("[[emissions.variants]]")
            for key in ("crop", "bbox", "opaque_pixels"):
                lines.append(f"{key} = {toml_value(variant[key])}")
        lines.append("")

    for feed in catalog["clean_feeds"]:
        lines.append("[[clean_feeds]]")
        for key in ("emission", "channel", "video"):
            lines.append(f"{key} = {toml_value(feed[key])}")
        lines.append("")

    for emission in catalog["no_logo_emissions"]:
        lines.append("[[no_logo_emissions]]")
        for key in ("name", "channel", "dossier"):
            lines.append(f"{key} = {toml_value(emission[key])}")
        lines.append("")

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--csv",
        type=Path,
        default=Path(
            "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Recadrage/2026/"
            "SELECTION_VIDEOS_SOURCES_LOGOS.csv"
        ),
    )
    parser.add_argument(
        "--references",
        type=Path,
        default=Path(
            "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Recadrage/2026/"
            "references_logos/emissions"
        ),
    )
    parser.add_argument(
        "--out",
        type=Path,
        default=Path("assets/models/qa/source_logos.toml"),
    )
    args = parser.parse_args()

    rows = list(csv.DictReader(args.csv.open(encoding="utf-8-sig")))
    emissions: OrderedDict[str, dict] = OrderedDict()
    clean_feeds: list[dict] = []
    no_logo_emissions: list[dict] = []

    for row in rows:
        status = row["statut"]
        emission = row["emission"].strip()
        channel = row["chaine"].strip()
        if status in ("VALIDE_AVEC_LOGO", "VARIANTE_CROP_DETAIL"):
            if emission not in emissions:
                dossier = row["dossier_reference"].strip()
                folder = args.references / dossier
                crops = sorted(folder.glob("crop*.png"))
                if not crops:
                    raise SystemExit(f"aucun crop pour {emission} dans {folder}")
                variants = []
                boxes = []
                for crop in crops:
                    result = alpha_bbox(crop)
                    if result is None:
                        continue
                    bbox, opaque = result
                    boxes.append(bbox)
                    variants.append(
                        {
                            "crop": f"{dossier}/{crop.name}",
                            "bbox": list(bbox),
                            "opaque_pixels": opaque,
                        }
                    )
                if not variants:
                    raise SystemExit(f"aucun pixel alpha pour {emission}")
                emissions[emission] = {
                    "name": emission,
                    "channel": channel,
                    "position": "top_left"
                    if row["position_ecran"].startswith("Haut")
                    else "bottom_left",
                    "dossier": dossier,
                    "bbox": list(union_bbox(boxes)),
                    "aliases": ALIASES.get(emission, []),
                    "sources": [],
                    "notes": row["notes"].strip(),
                    "variants": variants,
                }
            video = os.path.basename(row["chemin_relatif_video"].strip())
            if video and video not in emissions[emission]["sources"]:
                emissions[emission]["sources"].append(video)
        elif status == "CLEAN_FEED_SANS_LOGO":
            video = os.path.basename(row["chemin_relatif_video"].strip())
            clean_feeds.append(
                {"emission": emission, "channel": channel, "video": video}
            )
        elif status == "EMISSION_SANS_LOGO_A_IGNORER":
            no_logo_emissions.append(
                {
                    "name": emission,
                    "channel": channel,
                    "dossier": row["dossier_reference"].strip(),
                }
            )

    bottom = [e for e in emissions.values() if e["position"] == "bottom_left"]
    top = [e for e in emissions.values() if e["position"] == "top_left"]
    zones = {}
    if bottom:
        zones["bottom_left"] = list(union_bbox([tuple(e["bbox"]) for e in bottom]))
    if top:
        zones["top_left"] = list(union_bbox([tuple(e["bbox"]) for e in top]))

    catalog = {
        "schema": 1,
        "generated_at": date.today().isoformat(),
        "source": args.csv.name,
        "source_resolution": [1920, 1080],
        "reference_root": "",
        "zones": zones,
        "emissions": list(emissions.values()),
        "clean_feeds": clean_feeds,
        "no_logo_emissions": no_logo_emissions,
    }
    write_catalog(args.out, catalog)
    print(
        f"{args.out}: {len(emissions)} émissions, "
        f"{len(clean_feeds)} clean feeds, {len(no_logo_emissions)} sans logo"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
