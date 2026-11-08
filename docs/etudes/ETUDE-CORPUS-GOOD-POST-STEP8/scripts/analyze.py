#!/usr/bin/env python3
"""Consolidated metrics for the ETUDE-CORPUS-GOOD-POST-STEP8 report."""
from __future__ import annotations

import csv
import json
import statistics
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

import common

OUT = common.out_dir()
PROJECTS = [json.load(p.open(encoding="utf-8")) for p in sorted((OUT / "projects").glob("*.json"))]
SEGMENTS = list(csv.DictReader((OUT / "segments.csv").open(encoding="utf-8")))
DECISIONS = list(csv.DictReader((OUT / "decisions.csv").open(encoding="utf-8")))
EVENTS = list(csv.DictReader((OUT / "events.csv").open(encoding="utf-8")))
EXPORTS = list(csv.DictReader((OUT / "exports_scan.csv").open(encoding="utf-8")))


def f(value: Any) -> float | None:
    try:
        if value in ("", "None", None):
            return None
        return float(value)
    except (TypeError, ValueError):
        return None


def pct(num: int, den: int) -> float:
    return round(100.0 * num / den, 1) if den else 0.0


def segments_by_project() -> dict[tuple[str, str], dict[str, Any]]:
    out = {}
    for project in PROJECTS:
        for row in project["rows"]:
            out[(project["project"], row["name"])] = row
    return out


SEG = segments_by_project()


def decision_rows(code: str) -> list[dict[str, Any]]:
    return [d for p in PROJECTS for d in p["decisions"] if d["code"] == code]


def row_for(project: str, target: str) -> dict[str, Any] | None:
    return SEG.get((project, target))


def project_rows(code: str) -> list[tuple[dict[str, Any], dict[str, Any]]]:
    out = []
    for project in PROJECTS:
        for decision in project["decisions"]:
            if decision["code"] != code:
                continue
            row = row_for(project["project"], decision["target"])
            if row is not None:
                out.append((decision, row))
    return out


def a1_metrics() -> dict[str, Any]:
    rows = project_rows("A1")
    applies = [(d, r) for d, r in rows if d["action"] == "apply"]
    reviews = [(d, r) for d, r in rows if d["action"] == "review"]
    noops = [(d, r) for d, r in rows if d["action"] == "noop"]

    margins = []
    scale_ratios = []
    residual_8 = residual_16 = residual_more = 0
    keyframed = 0
    for decision, row in applies:
        details = decision.get("details") or {}
        active = f(details.get("active_height"))
        good_scale = f((row.get("good") or {}).get("scale"))
        raw_scale = f((row.get("raw") or {}).get("scale"))
        if active and good_scale:
            margins.append(good_scale / (1920.0 / active * 100.0))
        if good_scale and (row.get("assist") or {}).get("scale"):
            scale_ratios.append(good_scale / row["assist"]["scale"])
        residual = f((decision.get("evaluation") or {}).get("good_residual"))
        if residual is not None:
            if residual <= 8:
                residual_8 += 1
            elif residual <= 16:
                residual_16 += 1
            else:
                residual_more += 1
        if (row.get("keyframes") or {}).get("n"):
            keyframed += 1

    margin_bins = Counter()
    for value in margins:
        if value < 1.005:
            margin_bins["<1.005"] += 1
        elif value < 1.02:
            margin_bins["1.005-1.02"] += 1
        elif value < 1.03:
            margin_bins["1.02-1.03"] += 1
        elif value < 1.05:
            margin_bins["1.03-1.05"] += 1
        else:
            margin_bins[">=1.05"] += 1

    ratio_bins = Counter()
    for value in scale_ratios:
        if value < 0.90:
            ratio_bins["<0.90"] += 1
        elif value < 0.98:
            ratio_bins["0.90-0.98"] += 1
        elif value <= 1.02:
            ratio_bins["0.98-1.02"] += 1
        else:
            ratio_bins[">1.02"] += 1

    review_cat = Counter()
    review_out = defaultdict(Counter)
    for decision, _ in reviews:
        reason = decision.get("reason") or ""
        if decision.get("target") == "comp":
            cat = "comp"
        elif "asym" in reason:
            cat = "asym"
        elif "couvre" in reason:
            cat = "coverage"
        elif "sans résultat" in reason:
            cat = "crop_empty"
        elif "compensation" in reason:
            cat = "comp_y"
        else:
            cat = "other"
        review_cat[cat] += 1
        review_out[cat][(decision.get("evaluation") or {}).get("review_outcome")] += 1

    noop_vertical_bars = 0
    noop_side_bars = 0
    for decision, _ in noops:
        details = decision.get("details") or {}
        bars = details.get("bars") or {}
        top = f(bars.get("top")) or 0.0
        bottom = f(bars.get("bottom")) or 0.0
        left = f(bars.get("left")) or 0.0
        right = f(bars.get("right")) or 0.0
        if top > 8 or bottom > 8:
            noop_vertical_bars += 1
        if left > 8 or right > 8:
            noop_side_bars += 1

    return {
        "counts": {
            "apply": len(applies),
            "review": len(reviews) + 6,
            "noop": len(noops),
            "review_segment": len(reviews),
            "review_comp": 6,
        },
        "apply": {
            "margin_median": round(statistics.median(margins), 4) if margins else None,
            "margin_bins": dict(margin_bins),
            "scale_ratio_median": round(statistics.median(scale_ratios), 4) if scale_ratios else None,
            "scale_ratio_bins": dict(ratio_bins),
            "residual_le_8": residual_8,
            "residual_8_16": residual_16,
            "residual_gt_16": residual_more,
            "human_keyframed": keyframed,
            "projects": dict(Counter(d["project"] for d, _ in applies).most_common()),
        },
        "review": {
            "categories": dict(review_cat),
            "outcomes": {cat: dict(out) for cat, out in review_out.items()},
        },
        "noop": {
            "vertical_bars": noop_vertical_bars,
            "side_bars": noop_side_bars,
        },
    }


def a2_metrics() -> dict[str, Any]:
    decisions = decision_rows("A2")
    applies = [d for d in decisions if d["action"] == "apply"]
    reviews = [d for d in decisions if d["action"] == "review"]

    apply_rows = []
    for decision in applies:
        project = decision["project"]
        project_data = next(p for p in PROJECTS if p["project"] == project)
        ratios = []
        for candidate in project_data["rows"]:
            if candidate["relationship"] not in ("same", "trim"):
                continue
            assist = f((candidate.get("assist") or {}).get("scale"))
            good = f((candidate.get("good") or {}).get("scale"))
            if assist and good:
                ratios.append(good / assist)
        exact = sum(1 for x in ratios if abs(x - 1.0) <= 0.01)
        more = sum(1 for x in ratios if x > 1.03)
        less = sum(1 for x in ratios if x < 0.97)
        apply_rows.append(
            {
                "project": project,
                "action": decision["action"],
                "reason": decision.get("reason"),
                "category": (decision.get("evaluation") or {}).get("category"),
                "median_ratio": round(statistics.median(ratios), 4) if ratios else None,
                "exact": exact,
                "more": more,
                "less": less,
                "n": len(ratios),
            }
        )

    review_rows = []
    cat_out = defaultdict(Counter)
    for decision in reviews:
        evaluation = decision.get("evaluation") or {}
        cat = evaluation.get("category")
        cat_out[cat][evaluation.get("review_outcome")] += 1
        review_rows.append(
            {
                "project": decision["project"],
                "category": cat,
                "reason": decision.get("reason"),
                "outcome": evaluation.get("review_outcome"),
                "n_removed": evaluation.get("n_removed"),
                "n_segments": evaluation.get("n_segments"),
                "bbox": evaluation.get("bbox"),
            }
        )

    return {
        "counts": {
            "apply": len(applies),
            "review": len(reviews),
            "noop": sum(1 for d in decisions if d["action"] == "noop"),
        },
        "applies": apply_rows,
        "reviews": review_rows,
        "review_categories": {cat: dict(out) for cat, out in cat_out.items()},
    }


def a3_metrics() -> dict[str, Any]:
    rows = project_rows("A3")
    review = [(d, r) for d, r in rows if d["action"] == "review"]
    noop = [(d, r) for d, r in rows if d["action"] == "noop"]

    def stats(pairs):
        keys = static_strong = structural = none = 0
        for decision, row in pairs:
            evaluation = decision.get("evaluation") or {}
            has_keys = (evaluation.get("human_keys") or 0) > 0
            assist_x = f(row.get("assist_src_x"))
            good_x = f(row.get("good_src_x"))
            strong = (
                assist_x is not None
                and good_x is not None
                and abs(good_x - assist_x) >= 40
            )
            is_structural = row.get("relationship") not in ("same", "trim", "shift")
            if has_keys:
                keys += 1
            elif strong:
                static_strong += 1
            elif is_structural:
                structural += 1
            else:
                none += 1
        return {
            "n": len(pairs),
            "keys": keys,
            "static_strong": static_strong,
            "structural": structural,
            "none": none,
            "tolerant": len(pairs) - none,
            "key_precision": pct(keys, len(pairs)),
            "tolerant_precision": pct(len(pairs) - none, len(pairs)),
        }

    review_stats = stats(review)
    noop_stats = stats(noop)
    strict_tp = review_stats["keys"]
    strict_fp = review_stats["n"] - strict_tp
    strict_fn = noop_stats["keys"]
    strict_tn = noop_stats["n"] - strict_fn
    tolerant_tp = review_stats["tolerant"]
    tolerant_fp = review_stats["n"] - tolerant_tp
    tolerant_fn = noop_stats["tolerant"]
    tolerant_tn = noop_stats["n"] - tolerant_fn

    amp_bins = defaultdict(Counter)
    key_bins = defaultdict(Counter)
    for decision, row in review:
        evaluation = decision.get("evaluation") or {}
        amp = f(evaluation.get("amplitude_px")) or 0.0
        count = evaluation.get("candidate_keys") or 0
        resolution = evaluation.get("resolution")
        if amp < 150:
            band = "100-150"
        elif amp < 250:
            band = "150-250"
        elif amp < 400:
            band = "250-400"
        elif amp < 700:
            band = "400-700"
        else:
            band = "700+"
        amp_bins[band][resolution] += 1
        key_bins[count][resolution] += 1

    noop_reasons = defaultdict(Counter)
    noop_amp = defaultdict(lambda: {"n": 0, "keys": 0})
    noop_gap = defaultdict(lambda: {"n": 0, "keys": 0})
    for decision, row in noop:
        evaluation = decision.get("evaluation") or {}
        has_keys = (evaluation.get("human_keys") or 0) > 0
        reason = decision.get("reason") or ""
        if "segment < 1" in reason:
            cat = "short"
        else:
            cat = "movement"
        noop_reasons[cat]["n"] += 1
        if has_keys:
            noop_reasons[cat]["keys"] += 1
        amp = f(evaluation.get("amplitude_px")) or 0.0
        if amp < 50:
            band = "<50"
        elif amp < 100:
            band = "50-100"
        elif amp < 200:
            band = "100-200"
        elif amp < 400:
            band = "200-400"
        else:
            band = "400+"
        noop_amp[band]["n"] += 1
        noop_amp[band]["keys"] += 1 if has_keys else 0
        details = decision.get("details") or {}
        gap = f(details.get("gap"))
        if amp >= 100 and gap is not None:
            if gap < 0.3:
                gband = "<30"
            elif gap < 0.5:
                gband = "30-50"
            elif gap < 0.7:
                gband = "50-70"
            elif gap < 0.9:
                gband = "70-90"
            else:
                gband = "90-100"
            noop_gap[gband]["n"] += 1
            noop_gap[gband]["keys"] += 1 if has_keys else 0

    correlations = [
        f((d.get("evaluation") or {}).get("correlation"))
        for d, _ in review
    ]
    correlations = [x for x in correlations if x is not None]
    return {
        "counts": {"review": len(review), "noop": len(noop)},
        "strict": {
            "tp": strict_tp,
            "fp": strict_fp,
            "fn": strict_fn,
            "tn": strict_tn,
            "precision": pct(strict_tp, strict_tp + strict_fp),
            "recall": pct(strict_tp, strict_tp + strict_fn),
        },
        "tolerant": {
            "tp": tolerant_tp,
            "fp": tolerant_fp,
            "fn": tolerant_fn,
            "tn": tolerant_tn,
            "precision": pct(tolerant_tp, tolerant_tp + tolerant_fp),
            "recall": pct(tolerant_tp, tolerant_tp + tolerant_fn),
        },
        "review_stats": review_stats,
        "noop_stats": noop_stats,
        "amplitude_bins": {band: dict(counter) for band, counter in amp_bins.items()},
        "candidate_bins": {str(k): dict(v) for k, v in sorted(key_bins.items())},
        "noop_reasons": {k: dict(v) for k, v in noop_reasons.items()},
        "noop_amplitude": {k: dict(v) for k, v in noop_amp.items()},
        "noop_gap": {k: dict(v) for k, v in noop_gap.items()},
        "correlation": {
            "n": len(correlations),
            "median": round(statistics.median(correlations), 3) if correlations else None,
            "ge_06": sum(1 for x in correlations if x >= 0.6),
        },
    }


def a4_metrics() -> dict[str, Any]:
    rows = project_rows("A4")
    acted = [d for d, _ in rows if (d.get("evaluation") or {}).get("acted")]
    none = [d for d, _ in rows if not (d.get("evaluation") or {}).get("acted")]
    bins = defaultdict(Counter)
    for decision, _ in rows:
        evaluation = decision.get("evaluation") or {}
        duration = f(evaluation.get("duration")) or 0.0
        event = evaluation.get("event")
        if duration < 0.2:
            band = "<0.2"
        elif duration < 0.3:
            band = "0.2-0.3"
        elif duration < 0.4:
            band = "0.3-0.4"
        else:
            band = "0.4-0.5"
        bins[band][event] += 1
    veto_correct = sum(
        1
        for d, _ in rows
        if not (d.get("evaluation") or {}).get("acted")
        and (f((d.get("evaluation") or {}).get("duration")) or 0.0) < 0.3
    )
    return {
        "n": len(rows),
        "acted": len(acted),
        "none": len(none),
        "strict_precision": pct(len(acted), len(rows)),
        "tolerant_precision": pct(len(acted) + veto_correct, len(rows)),
        "veto_correct": veto_correct,
        "durations": {band: dict(counter) for band, counter in bins.items()},
        "events": dict(Counter((d.get("evaluation") or {}).get("event") for d, _ in rows)),
    }


def a5_a6_metrics() -> dict[str, Any]:
    a5 = decision_rows("A5")
    a6 = decision_rows("A6")
    return {
        "a5": {
            "n": len(a5),
            "covers": Counter(
                (d.get("evaluation") or {}).get("good_covers_comp") for d in a5
            ),
            "projects": Counter(d["project"] for d in a5),
        },
        "a6": {
            "n": len(a6),
            "actions": Counter(d["action"] for d in a6),
            "projects": Counter(d["project"] for d in a6),
        },
    }


def timeline_metrics() -> dict[str, Any]:
    return {
        "events": len(EVENTS),
        "types": dict(Counter(e["type"] for e in EVENTS)),
        "projects_with_events": len({e["project"] for e in EVENTS}),
    }


def overlay_metrics() -> dict[str, Any]:
    overlays = []
    for project in PROJECTS:
        for overlay in project["overlays"]:
            overlays.append((project["project"], overlay))
    return {
        "projects_with_overlays": len({p for p, _ in overlays}),
        "layers": len(overlays),
        "types": dict(Counter(o.get("source_type") for _, o in overlays)),
        "covers_comp": sum(1 for _, o in overlays if o.get("covers_comp")),
        "durations": {
            "min": round(min((o.get("duration") or 0) for _, o in overlays), 2) if overlays else None,
            "max": round(max((o.get("duration") or 0) for _, o in overlays), 2) if overlays else None,
        },
    }


def baseline_metrics() -> dict[str, Any]:
    project_values = []
    for project in PROJECTS:
        value = (project.get("baseline") or {}).get("value")
        if value:
            project_values.append(round(float(value), 3))
    return {
        "projects": dict(Counter(project_values).most_common()),
        "median": round(statistics.median(project_values), 3) if project_values else None,
    }


def export_metrics() -> dict[str, Any]:
    rows = EXPORTS
    return {
        "n": len(rows),
        "resolution": {f"{k[0]}x{k[1]}": v for k, v in Counter((r["width"], r["height"]) for r in rows).items()},
        "fps": dict(Counter(r["fps"] for r in rows)),
        "barred_frac": dict(Counter(r["barred_frac"] for r in rows)),
        "max_top": dict(Counter(r["max_top"] for r in rows)),
        "max_bottom": dict(Counter(r["max_bottom"] for r in rows)),
        "max_left": dict(Counter(r["max_left"] for r in rows).most_common(8)),
        "max_right": dict(Counter(r["max_right"] for r in rows).most_common(8)),
        "top_anomalies": [
            {
                "project": r["project"],
                "max_top": r["max_top"],
                "max_bottom": r["max_bottom"],
                "max_left": r["max_left"],
                "max_right": r["max_right"],
            }
            for r in rows
            if r["max_top"] not in ("0", "None")
            or r["max_bottom"] not in ("0", "None")
            or r["max_left"] not in ("0", "None")
            or r["max_right"] not in ("0", "None")
        ],
    }


def segment_metrics() -> dict[str, Any]:
    return {
        "n": len(SEGMENTS),
        "relationships": dict(Counter(r["relationship"] for r in SEGMENTS)),
        "a1_actions": dict(Counter(r["a1_action"] or "none" for r in SEGMENTS)),
        "human_keyframes": {
            "segments": sum(1 for r in SEGMENTS if int(r["human_keyframes"] or 0) > 0),
            "keys": sum(int(r["human_keyframes"] or 0) for r in SEGMENTS),
            "distribution": dict(
                Counter(int(r["human_keyframes"] or 0) for r in SEGMENTS).most_common(10)
            ),
        },
        "strong_static": sum(
            1
            for r in SEGMENTS
            if f(r["assist_src_x"]) is not None
            and f(r["good_src_x"]) is not None
            and abs(f(r["good_src_x"]) - f(r["assist_src_x"])) >= 40
        ),
    }


def main() -> None:
    summary = {
        "corpus": {
            "projects": len(PROJECTS),
            "folders": len({p["folder"] for p in PROJECTS}),
            "segments": len(SEGMENTS),
            "exports": len(EXPORTS),
        },
        "segments": segment_metrics(),
        "baseline": baseline_metrics(),
        "a1": a1_metrics(),
        "a2": a2_metrics(),
        "a3": a3_metrics(),
        "a4": a4_metrics(),
        "a5_a6": a5_a6_metrics(),
        "timeline": timeline_metrics(),
        "overlays": overlay_metrics(),
        "exports": export_metrics(),
    }
    (OUT / "summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
