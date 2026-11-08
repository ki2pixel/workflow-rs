#!/usr/bin/env python3
"""Matrice de confusion du spike A1 natif (D-057/D-058).

Lit le CSV produit par `a1_letterbox_spike --manifest` et calcule :
  - la matrice A1 (cropdetect) x natif ;
  - les taux de faux apply, faux négatif et faux veto résolu ;
  - l'accord avec cropdetect 16 px / 2 px ;
  - l'erreur pixel des applies natifs vs cropdetect 2 px ;
  - le coût médian.

Aucune écriture hors de --out. Le CSV d'entrée est produit dans /tmp par le
spike Rust ; le corpus n'est jamais lu par ce script.
"""
from __future__ import annotations

import argparse
import collections
import csv
import json
import statistics
from pathlib import Path
from typing import Any

ACTIONS = ("apply", "review", "noop")


def as_int(value: str) -> int | None:
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def as_float(value: str) -> float | None:
    try:
        return float(value)
    except (TypeError, ValueError):
        return None


def matrix(rows: list[dict[str, str]]) -> dict[str, dict[str, int]]:
    counts = collections.Counter((row["kind"], row["native"]) for row in rows)
    return {a1: {native: counts[(a1, native)] for native in ACTIONS} for a1 in ACTIONS}


def subset(rows: list[dict[str, str]], **filters: str) -> list[dict[str, str]]:
    out = []
    for row in rows:
        if all(row.get(key) == value for key, value in filters.items()):
            out.append(row)
    return out


def native_counts(rows: list[dict[str, str]]) -> dict[str, int]:
    counts = collections.Counter(row["native"] for row in rows)
    return {action: counts[action] for action in ACTIONS}


def pixel_errors(rows: list[dict[str, str]], top_key: str, bottom_key: str) -> dict[str, Any]:
    errors = []
    for row in rows:
        if row["cd2"] != "apply":
            continue
        top = as_int(row[top_key])
        bottom = as_int(row[bottom_key])
        ref_top = as_int(row["cd2_top"])
        ref_bottom = as_int(row["cd2_bottom"])
        if None in (top, bottom, ref_top, ref_bottom):
            continue
        errors.append((abs(top - ref_top), abs(bottom - ref_bottom)))
    if not errors:
        return {"n": 0}
    return {
        "n": len(errors),
        "mae_top": round(statistics.mean(top for top, _ in errors), 2),
        "mae_bottom": round(statistics.mean(bottom for _, bottom in errors), 2),
        "max": max(max(pair) for pair in errors),
        "le_1_px": sum(1 for pair in errors if max(pair) <= 1),
        "le_2_px": sum(1 for pair in errors if max(pair) <= 2),
        "le_4_px": sum(1 for pair in errors if max(pair) <= 4),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("sample", type=Path, help="sample-report.csv du spike")
    parser.add_argument("--out", type=Path, required=True, help="dossier de sortie")
    args = parser.parse_args()

    with args.sample.open(encoding="utf-8", newline="") as handle:
        rows = list(csv.DictReader(handle))
    if not rows:
        raise SystemExit("CSV vide")

    mat = matrix(rows)
    clear_apply = [
        row
        for row in rows
        if row["kind"] == "apply"
        and row["apply_acceptance"] in ("accepted", "adjusted_house")
    ]
    accepted_apply = subset(rows, kind="apply", apply_acceptance="accepted")
    adjusted_apply = subset(rows, kind="apply", apply_acceptance="adjusted_house")
    reverted_apply = subset(rows, kind="apply", apply_acceptance="reverted")
    veto_reviews = subset(rows, kind="review", review_outcome="veto_correct")
    resolved_reviews = subset(rows, kind="review", review_outcome="resolved")
    other_reviews = [
        row
        for row in rows
        if row["kind"] == "review"
        and row["review_outcome"] not in ("veto_correct", "resolved")
    ]
    noop_rows = subset(rows, kind="noop")

    clear_counts = native_counts(clear_apply)
    resolved_counts = native_counts(resolved_reviews)
    resolved_asym = [row for row in resolved_reviews if row["category"] == "asymmetric"]
    resolved_cov = [row for row in resolved_reviews if row["category"] == "coverage"]
    false_apply_rows = veto_reviews + other_reviews + noop_rows
    false_apply = sum(1 for row in false_apply_rows if row["native"] == "apply")
    false_negative = clear_counts["review"] + clear_counts["noop"]

    cd16_clear = [row for row in rows if row["cd16"] == "apply"]
    cd2_clear = [row for row in rows if row["cd2"] == "apply"]

    refined = [row for row in rows if row["kind"] == "apply" and row["native"] == "apply"]
    fallback = [row for row in rows if row["kind"] == "apply" and row["native"] != "apply"]

    by_project: dict[str, dict[str, Any]] = {}
    for row in rows:
        if row["kind"] != "apply":
            continue
        project = row["project"].split("/", 1)[0]
        entry = by_project.setdefault(
            project, {"n": 0, "native_apply": 0, "acceptance": collections.Counter()}
        )
        entry["n"] += 1
        entry["acceptance"][row["apply_acceptance"]] += 1
        if row["native"] == "apply":
            entry["native_apply"] += 1

    ratios = []
    native_ms = []
    for row in rows:
        native = as_float(row["native_ms"])
        cd16 = as_float(row["cd16_ms"])
        if native is not None:
            native_ms.append(native)
        if native is not None and cd16:
            ratios.append(native / cd16)

    native_only = {
        "matrix": mat,
        "clear_apply": len(clear_apply),
        "clear_native": clear_counts,
        "accepted": len(accepted_apply),
        "accepted_native": native_counts(accepted_apply),
        "adjusted": len(adjusted_apply),
        "adjusted_native": native_counts(adjusted_apply),
        "false_negative": false_negative,
        "false_negative_rate": round(false_negative / len(clear_apply), 3)
        if clear_apply
        else None,
        "false_apply": false_apply,
        "false_apply_denominator": len(false_apply_rows),
        "false_apply_rate": round(false_apply / len(false_apply_rows), 3)
        if false_apply_rows
        else None,
        "reverted_apply": len(reverted_apply),
        "reverted_native": native_counts(reverted_apply),
        "resolved_reviews": len(resolved_reviews),
        "resolved_native": resolved_counts,
        "resolved_asym": len(resolved_asym),
        "resolved_asym_native": native_counts(resolved_asym),
        "resolved_coverage": len(resolved_cov),
        "resolved_coverage_native": native_counts(resolved_cov),
        "veto_reviews": len(veto_reviews),
        "veto_native": native_counts(veto_reviews),
        "other_reviews": len(other_reviews),
        "other_native": native_counts(other_reviews),
        "noop": len(noop_rows),
        "noop_native": native_counts(noop_rows),
        "cd16_clear": len(cd16_clear),
        "cd16_native": native_counts(cd16_clear),
        "cd2_clear": len(cd2_clear),
        "cd2_native": native_counts(cd2_clear),
        "pixels_native": pixel_errors(refined, "top", "bottom"),
        "pixels_cropdetect16": pixel_errors(
            [row for row in rows if row["kind"] == "apply"], "cd16_top", "cd16_bottom"
        ),
        "pixels_hybrid": pixel_errors(
            [
                row
                if row["native"] == "apply"
                else {**row, "top": row["cd16_top"], "bottom": row["cd16_bottom"]}
                for row in rows
                if row["kind"] == "apply"
            ],
            "top",
            "bottom",
        ),
        "refined": len(refined),
        "fallback": len(fallback),
        "by_project": by_project,
        "timing": {
            "native_ms_median": round(statistics.median(native_ms), 1) if native_ms else None,
            "ratio_median": round(statistics.median(ratios), 2) if ratios else None,
        },
    }

    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / "confusion.json").write_text(
        json.dumps(native_only, ensure_ascii=False, indent=1), encoding="utf-8"
    )

    md = render_markdown(native_only)
    (args.out / "confusion.md").write_text(md, encoding="utf-8")
    print(md)
    return 0


def render_markdown(data: dict[str, Any]) -> str:
    mat = data["matrix"]
    lines = [
        "# Matrice de confusion — spike A1 natif (gray)\n",
        "| A1 \\ natif | apply | review | noop |",
        "| --- | ---: | ---: | ---: |",
    ]
    for action in ACTIONS:
        row = mat[action]
        lines.append(
            f"| {action} | {row['apply']} | {row['review']} | {row['noop']} |"
        )
    lines += [
        "",
        "## Taux",
        "",
        f"- Faux `apply` natifs sur A1 review/noop : "
        f"**{data['false_apply']}/{data['false_apply_denominator']}** "
        f"({(data['false_apply_rate'] or 0) * 100:.1f} %).",
        f"- Faux négatifs natifs sur les {data['clear_apply']} applies clairs : "
        f"**{data['false_negative']}/{data['clear_apply']}** "
        f"({(data['false_negative_rate'] or 0) * 100:.1f} %).",
        f"- Faux vetos résolus : **{data['resolved_native']['apply']}/"
        f"{data['resolved_reviews']}** ; asymétriques "
        f"**{data['resolved_asym_native']['apply']}/{data['resolved_asym']}** ; "
        f"couverture **{data['resolved_coverage_native']['apply']}/"
        f"{data['resolved_coverage']}**.",
        f"- Accord `cropdetect=24:16:0` : **{data['cd16_native']['apply']}/"
        f"{data['cd16_clear']}** ; `24:2:0` : **{data['cd2_native']['apply']}/"
        f"{data['cd2_clear']}**.",
        "",
        "## Détail",
        "",
        "| Sous-ensemble | n | apply | review | noop |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for label, n_key, counts_key in [
        ("apply accepted", "accepted", "accepted_native"),
        ("apply adjusted", "adjusted", "adjusted_native"),
        ("apply reverted", "reverted_apply", "reverted_native"),
        ("review veto", "veto_reviews", "veto_native"),
        ("review resolved", "resolved_reviews", "resolved_native"),
        ("review asym résolues", "resolved_asym", "resolved_asym_native"),
        ("review couverture résolues", "resolved_coverage", "resolved_coverage_native"),
        ("review autres", "other_reviews", "other_native"),
        ("noop", "noop", "noop_native"),
    ]:
        if n_key is None:
            continue
        counts = data[counts_key]
        lines.append(
            f"| {label} | {data[n_key]} | {counts['apply']} | "
            f"{counts['review']} | {counts['noop']} |"
        )
    # accepted/adjusted are not stored separately; derive from matrix/clear.
    lines += [
        "",
        "## Pixel (applies natifs vs cropdetect 2 px)",
        "",
        f"- Natif : {data['pixels_native']}",
        f"- cropdetect 16 px : {data['pixels_cropdetect16']}",
        f"- Hybride projeté (natif si apply, sinon cd16) : {data['pixels_hybrid']}",
        "",
        "## Coût",
        "",
        f"- Natif médian : {data['timing']['native_ms_median']} ms ; "
        f"ratio médian vs cropdetect 16 px : {data['timing']['ratio_median']}×.",
        f"- Applies raffinés par le natif : {data['refined']} ; "
        f"fallback cropdetect : {data['fallback']}.",
    ]
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    raise SystemExit(main())
