#!/usr/bin/env python3
"""Per-project extraction for the GOOD corpus study.

The corpus is read-only. Each project is loaded one at a time.
"""
from __future__ import annotations

import argparse
import json
import math
import re
import sys
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any

import common


OUT = common.out_dir()
PROJECTS_OUT = OUT / "projects"
PROJECTS_OUT.mkdir(parents=True, exist_ok=True)

TICKS = common.TICKS_PER_SECOND
COMP_RECT = (0.0, 0.0, common.COMP_W, common.COMP_H)


def tick_seconds(value: Any) -> float:
    if isinstance(value, (int, float)):
        return float(value) / TICKS
    return 0.0


def interval_overlap(a: dict[str, Any], b: dict[str, Any]) -> float:
    return min(a["out"], b["out"]) - max(a["in"], b["in"])


def source_match(a: dict[str, Any], b: dict[str, Any]) -> bool:
    if a.get("source_item") is None or b.get("source_item") is None:
        return True
    return a.get("source_item") == b.get("source_item")


def same_interval(a: dict[str, Any], b: dict[str, Any], tol: float = 0.002) -> bool:
    return abs(a["in"] - b["in"]) <= tol and abs(a["out"] - b["out"]) <= tol


def find_matches(a: dict[str, Any], candidates: list[dict[str, Any]]) -> list[dict[str, Any]]:
    exact = [
        b
        for b in candidates
        if (b["name"] == a["name"] or b["base"] == a["base"])
        and source_match(a, b)
        and interval_overlap(a, b) > -0.002
    ]
    if exact:
        return sorted(exact, key=lambda b: (b["in"], b["out"], str(b["id"])))
    # Fallback: same source and time overlap, nearest name.
    fallback = [
        b
        for b in candidates
        if source_match(a, b) and interval_overlap(a, b) > 0.05
    ]
    return sorted(
        fallback,
        key=lambda b: (
            -interval_overlap(a, b),
            abs(b["in"] - a["in"]),
            str(b["id"]),
        ),
    )[:4]


def relationship(a: dict[str, Any], matches: list[dict[str, Any]]) -> str:
    if not matches:
        return "deleted"
    if len(matches) == 1:
        b = matches[0]
        if same_interval(a, b):
            return "same"
        if b["in"] >= a["in"] - 0.002 and b["out"] <= a["out"] + 0.002:
            return "trim"
        if b["in"] <= a["in"] + 0.002 and b["out"] >= a["out"] - 0.002:
            return "extend"
        if abs((b["out"] - b["in"]) - (a["out"] - a["in"])) <= 0.002:
            return "shift"
        return "partial"
    sorted_m = sorted(matches, key=lambda b: (b["in"], b["out"]))
    exact_dupes = sum(1 for b in sorted_m if same_interval(a, b))
    if exact_dupes >= 2:
        return "duplicate"
    # Tiling: intervals cover the assist interval with small gaps/overlaps.
    total = sum(max(0.0, b["out"] - b["in"]) for b in sorted_m)
    span = max(b["out"] for b in sorted_m) - min(b["in"] for b in sorted_m)
    gaps = sum(
        max(0.0, sorted_m[i]["in"] - sorted_m[i - 1]["out"])
        for i in range(1, len(sorted_m))
    )
    overlaps = sum(
        max(0.0, sorted_m[i - 1]["out"] - sorted_m[i]["in"])
        for i in range(1, len(sorted_m))
    )
    if gaps <= 0.06 and overlaps <= 0.06 and total >= (a["out"] - a["in"]) * 0.92:
        return "split"
    if span <= (a["out"] - a["in"]) * 1.15 and overlaps > 0.06:
        return "overlap"
    return "mixed"


def merged_into_neighbor(a: dict[str, Any], good_infos: list[dict[str, Any]]) -> dict[str, Any] | None:
    for b in good_infos:
        if b["base"] == a["base"]:
            continue
        if b["in"] <= a["in"] + 0.05 and b["out"] >= a["out"] - 0.05:
            return b
    return None


def category_a1(reason: str) -> str:
    r = reason.lower()
    if "pas de barres" in r:
        return "noop"
    if "asymétrique" in r:
        return "asymmetric"
    if "intermittent" in r or "variable" in r:
        return "intermittent"
    if "keyfram" in r:
        return "keyframed"
    if "compensation y" in r:
        return "comp_y"
    if "ne couvre pas" in r or "coverage" in r:
        return "coverage"
    if "sans résultat" in r:
        return "crop_empty"
    if "vidéo source" in r or "vidéo" in r:
        return "video"
    return "other"


def category_a2(reason: str) -> str:
    r = reason.lower()
    if "aucune bande" in r:
        return "noop"
    if "bande haute" in r:
        return "top"
    if "zone basse haute" in r or "décor statique" in r:
        return "tall_bottom"
    if "largeur" in r and "<" in r:
        return "partial_width"
    if "animée" in r or "instable" in r:
        return "animated"
    if "logo" in r or "watermark" in r:
        return "logo"
    if "texte changeant" in r or "sous-titre" in r:
        return "changing_text"
    if "sombre" in r:
        return "dark"
    if "bannière" in r or "banniere" in r:
        return "banner"
    return "other"


def category_a4(reason: str) -> str:
    r = reason.lower()
    if "segment court" in r or "fusion" in r:
        return "short_segment"
    if "borne" in r or "align" in r:
        return "bounds"
    if "cut" in r or "raccord" in r:
        return "cut"
    return "other"


def load_ae_track(path: Path) -> dict[int, list[dict[str, Any]]]:
    """Return frame -> tracked objects with centroids and metadata."""
    if not path.is_file():
        return {}
    try:
        doc = common.load_json(path)
    except Exception:
        return {}
    data = doc.get("dataByFrame")
    if not isinstance(data, dict):
        return {}
    out: dict[int, list[dict[str, Any]]] = {}
    for frame_key, objects in data.items():
        try:
            frame = int(frame_key)
        except (TypeError, ValueError):
            continue
        if not isinstance(objects, list):
            continue
        row: list[dict[str, Any]] = []
        for obj in objects:
            if not isinstance(obj, dict):
                continue
            oid = obj.get("id")
            cx = obj.get("centroid_x")
            if oid is None or not isinstance(cx, (int, float)):
                continue
            row.append(
                {
                    "id": str(oid),
                    "cx": float(cx),
                    "cy": float(obj.get("centroid_y") or 0.0),
                    "bbox_w": float(obj.get("bbox_width") or 0.0),
                    "bbox_h": float(obj.get("bbox_height") or 0.0),
                    "tier": obj.get("tier"),
                    "label": obj.get("label"),
                    "speaking": bool(obj.get("is_speaking")),
                    "source": obj.get("source"),
                }
            )
        out[frame] = row
    return out


def load_recommendations(path: Path) -> list[dict[str, Any]]:
    if not path.is_file():
        return []
    try:
        doc = common.load_json(path)
    except Exception:
        return []
    raw = doc.get("segment_recommendations")
    if not isinstance(raw, dict):
        return []
    out: list[dict[str, Any]] = []
    for item in raw.values():
        if not isinstance(item, dict):
            continue
        start = item.get("start_frame")
        end = item.get("end_frame")
        if not isinstance(start, (int, float)) or not isinstance(end, (int, float)):
            continue
        out.append(
            {
                "start_frame": int(start),
                "end_frame": int(end),
                "object": item.get("recommended_object_id"),
                "target_anchor_x": item.get("target_anchor_x"),
                "spread_x": item.get("spread_x"),
                "tier": item.get("tier"),
                "needs_review": item.get("needs_review"),
            }
        )
    out.sort(key=lambda r: (r["start_frame"], r["end_frame"]))
    return out


def pick_recommendation(
    recs: list[dict[str, Any]], f0: int, f1: int
) -> dict[str, Any] | None:
    best = None
    for rec in recs:
        if rec["start_frame"] > f1 or rec["end_frame"] < f0:
            continue
        overlap = min(f1, rec["end_frame"]) - max(f0, rec["start_frame"])
        if best is None or overlap > best[0]:
            best = (overlap, rec)
    return best[1] if best else None


def objects_at(
    frames: dict[int, list[dict[str, Any]]], frame: int, window: int = 2
) -> list[dict[str, Any]]:
    for delta in range(0, window + 1):
        for candidate in (frame - delta, frame + delta):
            if candidate in frames and frames[candidate]:
                return frames[candidate]
    return []


def pearson(xs: list[float], ys: list[float]) -> float | None:
    if len(xs) < 2 or len(xs) != len(ys):
        return None
    mx = sum(xs) / len(xs)
    my = sum(ys) / len(ys)
    num = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    dx = math.sqrt(sum((x - mx) ** 2 for x in xs))
    dy = math.sqrt(sum((y - my) ** 2 for y in ys))
    if dx <= 0 or dy <= 0:
        return None
    return num / (dx * dy)


def track_correlation(
    key_stats: dict[str, Any],
    action: dict[str, Any] | None,
    ae_track: dict[int, list[dict[str, Any]]],
    fps: float,
) -> dict[str, Any]:
    if not action or not key_stats.get("keys"):
        return {"correlation": None, "mae": None, "mae_centered": None, "n": 0}
    details = action.get("details") or {}
    object_id = details.get("object")
    start = details.get("start")
    end = details.get("end")
    if not isinstance(start, (int, float)) or not isinstance(end, (int, float)):
        return {"correlation": None, "mae": None, "mae_centered": None, "n": 0}
    frames = [f for f in ae_track if start * fps - 1 <= f <= end * fps + 1]
    frames.sort()
    track = []
    for frame in frames:
        row = ae_track[frame]
        if object_id is not None:
            for obj in row:
                if obj["id"] == str(object_id):
                    track.append((frame / fps if fps else 0.0, obj["cx"]))
                    break
        elif row:
            track.append((frame / fps if fps else 0.0, row[0]["cx"]))
    if len(track) < 2:
        return {"correlation": None, "mae": None, "mae_centered": None, "n": 0}
    keys = key_stats["keys"]
    source_xs = key_stats.get("source_xs") or []
    if len(source_xs) != len(keys):
        return {"correlation": None, "mae": None, "mae_centered": None, "n": 0}
    xs: list[float] = []
    ys: list[float] = []
    for key, sx in zip(keys, source_xs):
        t = key["t"]
        # Linear interpolation in the track.
        if t <= track[0][0]:
            tx = track[0][1]
        elif t >= track[-1][0]:
            tx = track[-1][1]
        else:
            tx = track[-1][1]
            for (t0, x0), (t1, x1) in zip(track, track[1:]):
                if t0 <= t <= t1:
                    span = t1 - t0
                    tx = x0 + (x1 - x0) * ((t - t0) / span if span else 0.0)
                    break
        xs.append(sx)
        ys.append(tx)
    corr = pearson(xs, ys)
    mae = common.mean(abs(a - b) for a, b in zip(xs, ys))
    diffs = [a - b for a, b in zip(xs, ys)]
    offset = common.median(diffs)
    mae_centered = common.mean(abs(d - offset) for d in diffs)
    return {
        "correlation": corr,
        "mae": mae,
        "mae_centered": mae_centered,
        "offset": offset,
        "n": len(xs),
        "track_points": len(track),
        "object": object_id,
    }


def summarize_layer(
    project: common.Project,
    info: dict[str, Any],
    active_rect: tuple[float, float, float, float] | None,
) -> dict[str, Any]:
    layer = project.by_id.get(info["id"])
    if layer is None:
        return {}
    times = common.sample_times(layer, 5)
    return project.summary(layer, times, active_rect)


def combined_key_stats(
    project: common.Project, infos: list[dict[str, Any]]
) -> dict[str, Any]:
    all_keys: list[dict[str, Any]] = []
    all_source: list[float] = []
    all_comp: list[float] = []
    interp: set[str] = set()
    amplitude = 0.0
    monotonic = True
    for info in infos:
        layer = project.by_id.get(info["id"])
        if layer is None:
            continue
        stats = project.keyframe_stats(layer)
        if stats["n"]:
            all_keys.extend(stats["keys"])
            all_source.extend(stats.get("source_xs") or [])
            all_comp.extend(stats.get("comp_xs") or [])
            interp.update(stats.get("interp") or [])
            amplitude = max(amplitude, stats.get("amplitude_source") or 0.0)
            if stats.get("monotonic") is False:
                monotonic = False
    all_keys.sort(key=lambda k: k["t"])
    return {
        "n": len(all_keys),
        "keys": all_keys,
        "source_xs": all_source,
        "comp_xs": all_comp,
        "amplitude_source": amplitude,
        "monotonic": monotonic if all_keys else None,
        "interp": sorted(interp),
    }


def project_main(project: dict[str, Any], verbose: bool = False) -> dict[str, Any] | None:
    if not all(
        project[key].is_file()
        for key in ("raw", "assist", "good", "post", "review", "audit")
    ):
        return None
    raw = common.Project(common.load_json(project["raw"]))
    assist = common.Project(common.load_json(project["assist"]))
    good = common.Project(common.load_json(project["good"]))
    post = common.load_json(project["post"])
    review = common.load_json(project["review"])
    audit = common.load_json(project["audit"])

    specs = post.get("specs") or {}
    src_w = float(specs.get("width") or 1920)
    src_h = float(specs.get("height") or 1080)
    comp_meta = common.comp_meta(assist.doc)
    fps = comp_meta.get("fps") or 25.0

    actions = list(post.get("actions") or [])
    review_items = list(review.get("items") or [])
    actions_by_target: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for action in actions:
        actions_by_target[(action.get("code"), action.get("target"))].append(action)
    review_by_target: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for item in review_items:
        review_by_target[(item.get("code"), item.get("target"))].append(item)

    assist_infos = [
        common.layer_info(layer, assist.items)
        for layer in assist.comp.get("layers", [])
        if str(layer.get("name", "")).startswith("Segment")
    ]
    good_infos = [
        common.layer_info(layer, good.items)
        for layer in good.comp.get("layers", [])
        if str(layer.get("name", "")).startswith("Segment")
    ]
    raw_infos = [
        common.layer_info(layer, raw.items)
        for layer in raw.comp.get("layers", [])
        if str(layer.get("name", "")).startswith("Segment")
    ]

    # A1 lookups by segment name.
    a1_by_target: dict[str, dict[str, Any]] = {}
    a1_comp: list[dict[str, Any]] = []
    a2_actions: list[dict[str, Any]] = []
    a3_by_target: dict[str, dict[str, Any]] = {}
    a4_by_target: dict[str, dict[str, Any]] = {}
    a5_by_target: dict[str, dict[str, Any]] = {}
    a6_actions: list[dict[str, Any]] = []
    for action in actions:
        code = action.get("code")
        target = action.get("target") or ""
        if code == "A1":
            if target.startswith("Segment"):
                a1_by_target[target] = action
            else:
                a1_comp.append(action)
        elif code == "A2":
            a2_actions.append(action)
        elif code == "A3" and target.startswith("Segment"):
            a3_by_target[target] = action
        elif code == "A4" and target.startswith("Segment"):
            a4_by_target[target] = action
        elif code == "A5" and target.startswith("Segment"):
            a5_by_target[target] = action
        elif code == "A6":
            a6_actions.append(action)

    # Active source rectangle from A1 crop, used for geometry comparisons.
    def active_rect_for(target: str) -> tuple[float, float, float, float] | None:
        action = a1_by_target.get(target)
        if not action:
            return None
        details = action.get("details") or {}
        crop = details.get("crop") or {}
        if not isinstance(crop, dict):
            return None
        x = crop.get("x")
        y = crop.get("y")
        w = crop.get("w")
        h = crop.get("h")
        if not all(isinstance(v, (int, float)) for v in (x, y, w, h)):
            return None
        return (float(x), float(y), float(x) + float(w), float(y) + float(h))

    # House-style zoom is measured from RAW to GOOD, not ASSIST to GOOD:
    # STEP6 already applies the house zoom in ASSIST on untouched segments.
    # On A1 apply segments, ASSIST contains A1's own scale, so using
    # GOOD/ASSIST would wrongly report a human "unzoom".
    a1_targets = {
        name
        for name, action in a1_by_target.items()
        if action.get("action") == "apply"
    }
    ratios: list[float] = []
    for info in assist_infos:
        if info["name"] in a1_targets:
            continue
        matches = find_matches(info, good_infos)
        if len(matches) != 1:
            continue
        rel = relationship(info, matches)
        if rel not in ("same", "trim", "shift"):
            continue
        r_matches = find_matches(info, raw_infos)
        a_layer = assist.by_id.get(info["id"])
        g_layer = good.by_id.get(matches[0]["id"])
        if a_layer is None or g_layer is None or len(r_matches) != 1:
            continue
        r_layer = raw.by_id.get(r_matches[0]["id"])
        if r_layer is None:
            continue
        a_sum = summarize_layer(assist, info, None)
        g_sum = summarize_layer(good, matches[0], None)
        r_sum = summarize_layer(raw, r_matches[0], None)
        if a_sum.get("scale") and g_sum.get("scale") and r_sum.get("scale"):
            ratios.append(g_sum["scale"] / r_sum["scale"])
    rounded = Counter(round(r, 3) for r in ratios if r > 0)
    baseline = 1.0
    baseline_mode = None
    if rounded:
        mode, count = rounded.most_common(1)[0]
        if count >= max(5, int(0.20 * len(ratios))):
            baseline = mode
            baseline_mode = mode
        else:
            baseline = common.median(ratios)
    # For A2 apply projects, the assist already contains the A2 scale. The
    # useful baseline is therefore GOOD/ASSIST, not GOOD/RAW: the mode is
    # 1.0 when the monteur kept the A2 correction, and >1.0 if he added a
    # house-style zoom on top.

    rows: list[dict[str, Any]] = []
    decision_rows: list[dict[str, Any]] = []
    events: list[dict[str, Any]] = []
    matched_good_ids: set[Any] = set()
    ae_track = load_ae_track(project["ae"]) if a3_by_target else {}
    ae_recs = load_recommendations(project["ae"]) if a3_by_target else []

    for info in assist_infos:
        matches = find_matches(info, good_infos)
        rel = relationship(info, matches)
        for match in matches:
            matched_good_ids.add(match["id"])
        active_rect = active_rect_for(info["name"])
        a_layer = assist.by_id.get(info["id"])
        a_sum = summarize_layer(assist, info, active_rect) if a_layer else {}
        r_matches = find_matches(info, raw_infos)
        r_sum = summarize_layer(raw, r_matches[0], active_rect) if len(r_matches) == 1 else {}
        g_sums = []
        for match in matches:
            layer = good.by_id.get(match["id"])
            if layer is not None:
                g_sums.append((match, summarize_layer(good, match, active_rect)))
        primary_match = None
        primary_sum = {}
        if g_sums:
            exact = [
                (m, s)
                for m, s in g_sums
                if m["name"] == info["name"] and same_interval(info, m)
            ]
            if exact:
                primary_match, primary_sum = exact[0]
            else:
                primary_match, primary_sum = max(
                    g_sums, key=lambda pair: interval_overlap(info, pair[0])
                )
        g_key_stats = combined_key_stats(good, matches)

        scale_ratio = (
            primary_sum.get("scale", 0) / a_sum["scale"]
            if a_sum.get("scale") and primary_sum.get("scale")
            else None
        )
        raw_scale = (r_sum or {}).get("scale")
        raw_to_assist = (
            a_sum.get("scale", 0) / raw_scale
            if raw_scale and a_sum.get("scale")
            else None
        )
        raw_to_good = (
            primary_sum.get("scale", 0) / raw_scale
            if raw_scale and primary_sum.get("scale")
            else None
        )
        a1_excess = (
            raw_to_assist - raw_to_good
            if raw_to_assist is not None and raw_to_good is not None
            else None
        )
        zoom_rel = scale_ratio / baseline if scale_ratio is not None and baseline else None
        active_dx = (
            primary_sum.get("active_center_x") - a_sum.get("active_center_x")
            if primary_sum.get("active_center_x") is not None
            and a_sum.get("active_center_x") is not None
            else None
        )
        active_dy = (
            primary_sum.get("active_center_y") - a_sum.get("active_center_y")
            if primary_sum.get("active_center_y") is not None
            and a_sum.get("active_center_y") is not None
            else None
        )
        full_dx = (
            primary_sum.get("center_x") - a_sum.get("center_x")
            if primary_sum.get("center_x") is not None and a_sum.get("center_x") is not None
            else None
        )
        full_dy = (
            primary_sum.get("center_y") - a_sum.get("center_y")
            if primary_sum.get("center_y") is not None and a_sum.get("center_y") is not None
            else None
        )
        raw_dx = (
            a_sum.get("center_x") - r_sum.get("center_x")
            if r_sum.get("center_x") is not None and a_sum.get("center_x") is not None
            else None
        )
        raw_dy = (
            a_sum.get("center_y") - r_sum.get("center_y")
            if r_sum.get("center_y") is not None and a_sum.get("center_y") is not None
            else None
        )

        # Where is the comp center in source coordinates? This is the point
        # the editor chose to center, and it can be compared with STEP4
        # object centroids independently of parent/Null offsets.
        mid_t = (info["in"] + info["out"]) / 2.0
        a_layer = assist.by_id.get(info["id"])
        g_layer = good.by_id.get(primary_match["id"]) if primary_match else None
        r_layer = raw.by_id.get(r_matches[0]["id"]) if len(r_matches) == 1 else None
        assist_src_x = (
            assist.inverse_source_x(a_layer, mid_t, common.COMP_W / 2.0)
            if a_layer is not None
            else None
        )
        good_src_x = (
            good.inverse_source_x(g_layer, mid_t, common.COMP_W / 2.0)
            if g_layer is not None
            else None
        )
        raw_src_x = (
            raw.inverse_source_x(r_layer, mid_t, common.COMP_W / 2.0)
            if r_layer is not None
            else None
        )
        f_mid = int(round(mid_t * fps)) + 1
        f0 = int(round(info["in"] * fps)) + 1
        f1 = int(round(info["out"] * fps))
        rec = pick_recommendation(ae_recs, f0, f1) if ae_recs else None
        obj_agg: dict[str, dict[str, Any]] = {}
        if ae_track:
            for frame in range(f0, f1 + 1):
                for obj in ae_track.get(frame, []):
                    agg = obj_agg.setdefault(
                        obj["id"],
                        {
                            "id": obj["id"],
                            "xs": [],
                            "ys": [],
                            "bbox_w": obj["bbox_w"],
                            "bbox_h": obj["bbox_h"],
                            "tier": obj["tier"],
                            "label": obj["label"],
                            "source": obj["source"],
                            "speaking": False,
                        },
                    )
                    agg["xs"].append(obj["cx"])
                    agg["ys"].append(obj["cy"])
                    agg["speaking"] = agg["speaking"] or obj["speaking"]
        obj_rows = []
        for agg in obj_agg.values():
            obj_rows.append(
                {
                    "id": agg["id"],
                    "cx": round(common.median(agg["xs"]), 2),
                    "cy": round(common.median(agg["ys"]), 2),
                    "n": len(agg["xs"]),
                    "bbox_w": agg["bbox_w"],
                    "bbox_h": agg["bbox_h"],
                    "area": agg["bbox_w"] * agg["bbox_h"],
                    "tier": agg["tier"],
                    "label": agg["label"],
                    "source": agg["source"],
                    "speaking": agg["speaking"],
                }
            )
        near_human = (
            min(obj_rows, key=lambda o: abs(o["cx"] - good_src_x))
            if obj_rows and good_src_x is not None
            else None
        )
        near_assist = (
            min(obj_rows, key=lambda o: abs(o["cx"] - assist_src_x))
            if obj_rows and assist_src_x is not None
            else None
        )
        rec_obj = None
        if rec and rec.get("object") is not None:
            rec_obj = next(
                (o for o in obj_rows if o["id"] == str(rec["object"])), None
            )
        human_obj_dist = (
            abs(good_src_x - near_human["cx"]) if near_human is not None else None
        )
        assist_obj_dist = (
            abs(assist_src_x - near_assist["cx"]) if near_assist is not None else None
        )
        rec_human_dist = (
            abs(good_src_x - rec_obj["cx"]) if rec_obj is not None else None
        )
        rec_assist_dist = (
            abs(assist_src_x - rec_obj["cx"]) if rec_obj is not None else None
        )

        a1_action = a1_by_target.get(info["name"])
        a1_details = (a1_action or {}).get("details") or {}
        bars = a1_details.get("bars") if isinstance(a1_details.get("bars"), dict) else {}
        active_height = a1_details.get("active_height")
        # A1 only acts on vertical letterbox. `active_height` is the crop
        # height picked by cropdetect; 1072 is the no-bar reference for a
        # 1920x1080 source. A noop can still report tiny top/bottom edges
        # (4 px) and horizontal side bars, which are out of A1 scope.
        if isinstance(active_height, (int, float)):
            bars_present = float(active_height) < 1072.0
        else:
            bars_present = bool(bars) and (
                float(bars.get("top") or 0) + float(bars.get("bottom") or 0)
            ) > 8
        a1_assist_removed = None
        a1_good_removed = None
        a1_raw_removed = None
        a1_good_residual = None
        a1_assist_residual = None
        if bars:
            a_times = common.sample_times(assist.by_id[info["id"]], 5)
            a1_assist_residual = common.bars_residual(
                assist, assist.by_id[info["id"]], a_times, src_w, src_h, bars
            )
            a1_assist_removed = common.bars_removed(
                assist, assist.by_id[info["id"]], a_times, src_w, src_h, bars
            )
            if r_matches:
                r_layer = raw.by_id.get(r_matches[0]["id"])
                if r_layer is not None:
                    a1_raw_removed = common.bars_removed(
                        raw, r_layer, common.sample_times(r_layer, 5), src_w, src_h, bars
                    )
            if g_sums:
                a1_good_residual = max(
                    common.bars_residual(
                        good,
                        good.by_id[match["id"]],
                        common.sample_times(good.by_id[match["id"]], 5),
                        src_w,
                        src_h,
                        bars,
                    )
                    for match, _ in g_sums
                    if match["id"] in good.by_id
                )
                a1_good_removed = common.bars_removed(
                    good,
                    good.by_id[g_sums[0][0]["id"]],
                    common.sample_times(good.by_id[g_sums[0][0]["id"]], 5),
                    src_w,
                    src_h,
                    bars,
                )

        # A2 banner resolution (same bbox for all segments, comp-wide).
        a2_rows: list[dict[str, Any]] = []
        for a2 in a2_actions:
            details = a2.get("details") or {}
            bbox = details.get("bbox")
            if not (isinstance(bbox, list) and len(bbox) == 4 and all(isinstance(v, (int, float)) for v in bbox)):
                continue
            rect = tuple(float(v) for v in bbox)
            a_times = common.sample_times(assist.by_id[info["id"]], 5)
            a_removed = common.banner_removed(
                assist, assist.by_id[info["id"]], a_times, rect
            )
            g_removed = None
            if g_sums:
                g_removed = all(
                    common.banner_removed(
                        good,
                        good.by_id[match["id"]],
                        common.sample_times(good.by_id[match["id"]], 5),
                        rect,
                    )
                    for match, _ in g_sums
                    if match["id"] in good.by_id
                )
            r_removed = None
            if r_matches:
                r_layer = raw.by_id.get(r_matches[0]["id"])
                if r_layer is not None:
                    r_removed = common.banner_removed(
                        raw, r_layer, common.sample_times(r_layer, 5), rect
                    )
            a2_rows.append(
                {
                    "action": a2.get("action"),
                    "reason": a2.get("reason"),
                    "category": category_a2(a2.get("reason") or ""),
                    "confidence": a2.get("confidence"),
                    "bbox": rect,
                    "height": details.get("height"),
                    "width_frac": details.get("width_frac"),
                    "median_brightness": details.get("median_brightness"),
                    "ocr": details.get("ocr"),
                    "assist_removed": a_removed,
                    "good_removed": g_removed,
                    "raw_removed": r_removed,
                    "scale_target": details.get("scale_target"),
                }
            )

        # A3 human keyframe / static reframe.
        a3_action = a3_by_target.get(info["name"])
        a3_eval: dict[str, Any] = {}
        if a3_action:
            track = track_correlation(g_key_stats, a3_action, ae_track, fps)
            if g_key_stats.get("n"):
                if track.get("correlation") is not None and track["correlation"] >= 0.6 and (track.get("mae_centered") or 9999) <= 100:
                    a3_res = "tracking"
                elif (g_key_stats.get("amplitude_source") or 0) <= 60:
                    a3_res = "stabilization"
                else:
                    a3_res = "keyframe_other"
            elif rel in ("deleted", "split", "duplicate", "mixed"):
                a3_res = "structural"
            elif zoom_rel is not None and (zoom_rel >= 1.05 or zoom_rel <= 0.95):
                a3_res = "static_reframe"
            elif active_dx is not None and abs(active_dx) >= 20:
                a3_res = "static_reframe"
            elif active_dy is not None and abs(active_dy) >= 20:
                a3_res = "static_reframe"
            else:
                a3_res = "none"
            a3_eval = {
                "review": True,
                "resolution": a3_res,
                "human_keys": g_key_stats.get("n") or 0,
                "key_amplitude": g_key_stats.get("amplitude_source") or 0,
                "key_monotonic": g_key_stats.get("monotonic"),
                "key_interp": g_key_stats.get("interp") or [],
                "correlation": track.get("correlation"),
                "mae": track.get("mae"),
                "mae_centered": track.get("mae_centered"),
                "track_points": track.get("track_points"),
                "amplitude_px": (a3_action.get("details") or {}).get("amplitude_px"),
                "candidate_keys": (a3_action.get("details") or {}).get("candidate_keys"),
            }

        # A4 event on the target.
        a4_action = a4_by_target.get(info["name"])
        a4_eval: dict[str, Any] = {}
        if a4_action:
            if rel == "deleted":
                neighbor = merged_into_neighbor(info, good_infos)
                a4_event = "merged" if neighbor is not None else "deleted"
            elif rel == "split":
                a4_event = "split"
            elif rel == "duplicate":
                a4_event = "duplicate"
            elif rel == "trim":
                a4_event = "trim"
            elif rel == "extend":
                a4_event = "extend"
            elif rel == "shift":
                a4_event = "shift"
            else:
                a4_event = "none"
            a4_eval = {
                "review": True,
                "event": a4_event,
                "acted": a4_event in ("merged", "deleted", "split", "trim", "extend"),
                "duration": info["out"] - info["in"],
            }

        # A5 coverage.
        a5_action = a5_by_target.get(info["name"])
        a5_eval: dict[str, Any] = {}
        if a5_action:
            g_contains = None
            if g_sums:
                g_contains = True
                for match, _ in g_sums:
                    layer = good.by_id.get(match["id"])
                    if layer is None:
                        continue
                    for t in common.sample_times(layer, 5):
                        rect = good.project_rect(layer, (0.0, 0.0, src_w, src_h), t)
                        if not common.rect_contains(COMP_RECT, rect):
                            g_contains = False
                            break
                    if g_contains is False:
                        break
            a5_eval = {
                "review": True,
                "good_covers_comp": g_contains,
                "reason": a5_action.get("reason"),
            }

        row = {
            "project": f"{project['folder']}/{project['stem']}",
            "name": info["name"],
            "base": info["base"],
            "source_item": info["source_item"],
            "in": info["in"],
            "out": info["out"],
            "duration": info["out"] - info["in"],
            "relationship": rel,
            "n_good": len(matches),
            "good_ids": [m["id"] for m in matches],
            "good_names": [m["name"] for m in matches],
            "assist": a_sum,
            "raw": r_sum,
            "good": primary_sum,
            "good_all": [s for _, s in g_sums],
            "scale_ratio": scale_ratio,
            "raw_to_assist": raw_to_assist,
            "raw_to_good": raw_to_good,
            "a1_excess": a1_excess,
            "baseline": baseline,
            "zoom_rel": zoom_rel,
            "full_dx": full_dx,
            "full_dy": full_dy,
            "active_dx": active_dx,
            "active_dy": active_dy,
            "raw_dx": raw_dx,
            "raw_dy": raw_dy,
            "assist_src_x": assist_src_x,
            "good_src_x": good_src_x,
            "raw_src_x": raw_src_x,
            "objects": obj_rows,
            "recommendation": rec,
            "human_obj_dist": human_obj_dist,
            "assist_obj_dist": assist_obj_dist,
            "rec_human_dist": rec_human_dist,
            "rec_assist_dist": rec_assist_dist,
            "keyframes": g_key_stats,
            "a1": {
                "action": (a1_action or {}).get("action"),
                "reason": (a1_action or {}).get("reason"),
                "category": category_a1((a1_action or {}).get("reason") or ""),
                "confidence": (a1_action or {}).get("confidence"),
                "bars": bars,
                "active_height": active_height,
                "bars_present": bars_present,
                "assist_removed": a1_assist_removed,
                "good_removed": a1_good_removed,
                "good_residual": a1_good_residual,
                "assist_residual": a1_assist_residual,
                "raw_removed": a1_raw_removed,
                "scale_target": a1_details.get("scale_target"),
                "position_y_target": a1_details.get("position_y_target"),
            },
            "a2": a2_rows,
            "a3": a3_eval,
            "a4": a4_eval,
            "a5": a5_eval,
        }
        rows.append(row)

        # Decision rows, one per action.
        if a1_action:
            decision_rows.append(
                make_decision_row(project, a1_action, info["name"], row, code="A1")
            )
        if a3_action:
            decision_rows.append(
                make_decision_row(project, a3_action, info["name"], row, code="A3")
            )
        if a4_action:
            decision_rows.append(
                make_decision_row(project, a4_action, info["name"], row, code="A4")
            )
        if a5_action:
            decision_rows.append(
                make_decision_row(project, a5_action, info["name"], row, code="A5")
            )

    # A2 is comp-wide: one decision row per action, evaluated over all segments.
    for action in a2_actions:
        decision_rows.append(make_a2_decision_row(project, action, rows))

    # A1 comp reviews.
    for action in a1_comp:
        decision_rows.append(
            make_comp_decision_row(project, action, rows, code="A1")
        )
    for action in a6_actions:
        decision_rows.append(
            make_comp_decision_row(project, action, rows, code="A6")
        )

    # Events: split/duplicate/trim/delete/add/merge.
    for row in rows:
        rel = row["relationship"]
        if rel == "deleted":
            neighbor = merged_into_neighbor(
                {"base": row["base"], "in": row["in"], "out": row["out"]},
                good_infos,
            )
            events.append(
                {
                    "project": row["project"],
                    "type": "merge" if neighbor is not None else "delete",
                    "target": row["name"],
                    "in": row["in"],
                    "out": row["out"],
                    "neighbor": neighbor["name"] if neighbor else None,
                }
            )
        elif rel in ("split", "duplicate", "mixed", "overlap"):
            events.append(
                {
                    "project": row["project"],
                    "type": rel,
                    "target": row["name"],
                    "in": row["in"],
                    "out": row["out"],
                    "parts": row["good_names"],
                }
            )
        elif rel in ("trim", "extend", "shift", "partial"):
            events.append(
                {
                    "project": row["project"],
                    "type": rel,
                    "target": row["name"],
                    "in": row["in"],
                    "out": row["out"],
                    "good_in": row["good"].get("in") if isinstance(row["good"], dict) else None,
                }
            )

    # Added GOOD layers with no assist match.
    for match in good_infos:
        if match["id"] in matched_good_ids:
            continue
        events.append(
            {
                "project": f"{project['folder']}/{project['stem']}",
                "type": "add",
                "target": match["name"],
                "in": match["in"],
                "out": match["out"],
                "parts": [],
            }
        )

    # Overlays in GOOD (non-Segment layers).
    overlays = []
    for layer in good.comp.get("layers", []):
        name = str(layer.get("name", ""))
        if name.startswith("Segment"):
            continue
        info = common.layer_info(layer, good.items)
        if info.get("source_type") == "Null":
            continue
        try:
            mid = (info["in"] + info["out"]) / 2.0
            rect = good.project_rect(layer, (0.0, 0.0, src_w, src_h), mid)
        except Exception:
            rect = None
        overlays.append(
            {
                "name": name,
                "source_type": info.get("source_type"),
                "in": info["in"],
                "out": info["out"],
                "duration": info["out"] - info["in"],
                "rect": rect,
                "covers_comp": bool(rect and common.rect_contains(COMP_RECT, rect)),
            }
        )

    # Project payload.
    payload = {
        "folder": project["folder"],
        "stem": project["stem"],
        "project": f"{project['folder']}/{project['stem']}",
        "paths": {
            "raw": str(project["raw"]),
            "assist": str(project["assist"]),
            "good": str(project["good"]),
            "post": str(project["post"]),
            "review": str(project["review"]),
            "audit": str(project["audit"]),
            "source_video": str(project["source_video"]),
            "export_video": str(project["export_video"]),
            "ae": str(project["ae"]),
            "csv": str(project["csv"]),
        },
        "meta": comp_meta,
        "specs": specs,
        "baseline": {
            "value": baseline,
            "mode": baseline_mode,
            "median": common.median(ratios) if ratios else None,
            "ratios": len(ratios),
            "distribution": dict(Counter(round(r, 2) for r in ratios).most_common(20)),
        },
        "counts": {
            "assist_segments": len(assist_infos),
            "good_segments": len(good_infos),
            "raw_segments": len(raw_infos),
            "actions": len(actions),
            "reviews": len(review_items),
            "overlays": len(overlays),
        },
        "rows": rows,
        "decisions": decision_rows,
        "events": events,
        "overlays": overlays,
        "audit_verdict": audit.get("verdict"),
    }
    if verbose:
        print(
            f"{payload['project']}: {len(rows)} segments, {len(decision_rows)} decisions, "
            f"baseline={baseline:.3f}, events={len(events)}",
            file=sys.stderr,
        )
    return payload


def make_a2_decision_row(
    project: dict[str, Any],
    action: dict[str, Any],
    rows: list[dict[str, Any]],
) -> dict[str, Any]:
    details = action.get("details") or {}
    reason = action.get("reason") or ""
    per_segment = []
    for row in rows:
        for a2 in row.get("a2", []):
            if a2.get("reason") != reason:
                continue
            removed = a2.get("good_removed")
            if row.get("relationship") == "deleted":
                removed = True
            per_segment.append(
                {
                    "name": row["name"],
                    "relationship": row.get("relationship"),
                    "good_removed": removed,
                    "assist_removed": a2.get("assist_removed"),
                    "raw_removed": a2.get("raw_removed"),
                    "scale_target": a2.get("scale_target"),
                }
            )
    removed_values = [p["good_removed"] for p in per_segment if p["good_removed"] is not None]
    good_removed_all = bool(removed_values) and all(removed_values)
    evaluation = {
        "category": category_a2(reason),
        "n_segments": len(per_segment),
        "n_removed": sum(1 for p in per_segment if p["good_removed"]),
        "good_removed": good_removed_all,
        "assist_removed": all(p["assist_removed"] for p in per_segment if p["assist_removed"] is not None)
        if per_segment
        else None,
        "raw_removed": all(p["raw_removed"] for p in per_segment if p["raw_removed"] is not None)
        if per_segment
        else None,
        "segments": per_segment,
        "bbox": details.get("bbox"),
        "height": details.get("height"),
        "width_frac": details.get("width_frac"),
        "scale_target": details.get("scale_target"),
        "ocr": details.get("ocr"),
    }
    if action.get("action") == "apply":
        evaluation["apply_acceptance"] = "accepted" if good_removed_all else "reverted"
    elif action.get("action") == "review":
        if good_removed_all:
            evaluation["review_outcome"] = "resolved"
        elif evaluation["category"] in (
            "logo",
            "changing_text",
            "tall_bottom",
            "animated",
            "dark",
            "top",
        ):
            evaluation["review_outcome"] = "veto_correct"
        else:
            evaluation["review_outcome"] = "persisted"
    return {
        "project": f"{project['folder']}/{project['stem']}",
        "code": "A2",
        "action": action.get("action"),
        "target": action.get("target"),
        "target_name": "comp",
        "reason": reason,
        "confidence": action.get("confidence"),
        "details": details,
        "evaluation": evaluation,
        "relationship": "comp",
        "baseline": None,
    }


def make_decision_row(
    project: dict[str, Any],
    action: dict[str, Any],
    target_name: str,
    segment_row: dict[str, Any],
    code: str,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    details = action.get("details") or {}
    evaluation: dict[str, Any] = {}
    if code == "A1":
        residual = segment_row["a1"].get("good_residual")
        evaluation = {
            "bars_present": segment_row["a1"].get("bars_present"),
            "good_removed": segment_row["a1"].get("good_removed"),
            "good_residual": residual,
            "good_cleared": bool(residual is not None and residual <= 8.0),
            "assist_removed": segment_row["a1"].get("assist_removed"),
            "assist_residual": segment_row["a1"].get("assist_residual"),
            "raw_removed": segment_row["a1"].get("raw_removed"),
            "scale_target": details.get("scale_target"),
            "position_y_target": details.get("position_y_target"),
            "scale_ratio": segment_row.get("scale_ratio"),
            "raw_to_assist": segment_row.get("raw_to_assist"),
            "raw_to_good": segment_row.get("raw_to_good"),
            "a1_excess": segment_row.get("a1_excess"),
            "baseline": segment_row.get("baseline"),
            "zoom_rel": segment_row.get("zoom_rel"),
            "active_dy": segment_row.get("active_dy"),
        }
        if action.get("action") == "apply":
            cleared = evaluation.get("good_cleared")
            if cleared:
                excess = evaluation.get("a1_excess")
                if excess is None:
                    evaluation["apply_acceptance"] = "accepted"
                elif abs(excess) <= 0.02:
                    evaluation["apply_acceptance"] = "accepted"
                elif 0.02 < excess <= 0.07:
                    evaluation["apply_acceptance"] = "adjusted_house"
                else:
                    evaluation["apply_acceptance"] = "overcropped"
            else:
                evaluation["apply_acceptance"] = "reverted"
        elif action.get("action") == "review":
            if evaluation.get("bars_present") is False:
                evaluation["review_outcome"] = "no_defect"
            elif evaluation.get("good_cleared"):
                evaluation["review_outcome"] = "resolved"
            elif (action.get("reason") or "").lower().find("asymétrique") >= 0:
                evaluation["review_outcome"] = "veto_correct"
            else:
                evaluation["review_outcome"] = "persisted"
    elif code == "A2":
        a2_rows = (extra or {}).get("a2") or []
        evaluation = {
            "a2_rows": a2_rows,
            "good_removed": any(r.get("good_removed") for r in a2_rows) if a2_rows else None,
            "assist_removed": any(r.get("assist_removed") for r in a2_rows) if a2_rows else None,
            "raw_removed": any(r.get("raw_removed") for r in a2_rows) if a2_rows else None,
            "category": (a2_rows[0] or {}).get("category") if a2_rows else None,
        }
        if action.get("action") == "apply":
            evaluation["apply_acceptance"] = (
                "accepted" if evaluation.get("good_removed") else "reverted"
            )
        elif action.get("action") == "review":
            if evaluation.get("good_removed"):
                evaluation["review_outcome"] = "resolved"
            else:
                cat = evaluation.get("category")
                if cat in ("logo", "changing_text", "tall_bottom", "animated", "dark", "top"):
                    evaluation["review_outcome"] = "veto_correct"
                else:
                    evaluation["review_outcome"] = "persisted"
    elif code == "A3":
        evaluation = dict(segment_row.get("a3") or {})
    elif code == "A4":
        evaluation = dict(segment_row.get("a4") or {})
    elif code == "A5":
        evaluation = dict(segment_row.get("a5") or {})
    return {
        "project": f"{project['folder']}/{project['stem']}",
        "code": code,
        "action": action.get("action"),
        "target": action.get("target"),
        "target_name": target_name,
        "reason": action.get("reason"),
        "confidence": action.get("confidence"),
        "details": details,
        "evaluation": evaluation,
        "relationship": segment_row.get("relationship"),
        "baseline": segment_row.get("baseline"),
    }


def make_comp_decision_row(
    project: dict[str, Any],
    action: dict[str, Any],
    rows: list[dict[str, Any]],
    code: str,
) -> dict[str, Any]:
    details = action.get("details") or {}
    evaluation: dict[str, Any] = {}
    if code == "A1":
        # Comp review: median Y compensation across segments.
        deltas = [
            row["active_dy"]
            for row in rows
            if row.get("active_dy") is not None
        ]
        evaluation = {
            "median_active_dy": common.median(deltas) if deltas else None,
            "n_segments": len(deltas),
            "good_action": bool(deltas and abs(common.median(deltas)) > 10),
        }
    elif code == "A6":
        evaluation = {"a6_apply": action.get("action") == "apply"}
    return {
        "project": f"{project['folder']}/{project['stem']}",
        "code": code,
        "action": action.get("action"),
        "target": action.get("target"),
        "target_name": "comp",
        "reason": action.get("reason"),
        "confidence": action.get("confidence"),
        "details": details,
        "evaluation": evaluation,
        "relationship": "comp",
        "baseline": None,
    }


def write_csv(path: Path, rows: list[dict[str, Any]], columns: list[str]) -> None:
    import csv

    with path.open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def flatten_row(row: dict[str, Any]) -> dict[str, Any]:
    out = {
        "project": row["project"],
        "name": row["name"],
        "base": row["base"],
        "in": round(row["in"], 3),
        "out": round(row["out"], 3),
        "duration": round(row["duration"], 3),
        "relationship": row["relationship"],
        "n_good": row["n_good"],
        "assist_scale": round((row["assist"] or {}).get("scale") or 0, 3),
        "good_scale": round((row["good"] or {}).get("scale") or 0, 3),
        "scale_ratio": round(row["scale_ratio"], 4) if row.get("scale_ratio") is not None else None,
        "baseline": round(row.get("baseline") or 0, 4),
        "zoom_rel": round(row["zoom_rel"], 4) if row.get("zoom_rel") is not None else None,
        "active_dx": round(row["active_dx"], 2) if row.get("active_dx") is not None else None,
        "active_dy": round(row["active_dy"], 2) if row.get("active_dy") is not None else None,
        "full_dx": round(row["full_dx"], 2) if row.get("full_dx") is not None else None,
        "full_dy": round(row["full_dy"], 2) if row.get("full_dy") is not None else None,
        "assist_src_x": round(row["assist_src_x"], 2) if row.get("assist_src_x") is not None else None,
        "good_src_x": round(row["good_src_x"], 2) if row.get("good_src_x") is not None else None,
        "raw_src_x": round(row["raw_src_x"], 2) if row.get("raw_src_x") is not None else None,
        "human_obj_dist": round(row["human_obj_dist"], 2) if row.get("human_obj_dist") is not None else None,
        "assist_obj_dist": round(row["assist_obj_dist"], 2) if row.get("assist_obj_dist") is not None else None,
        "rec_human_dist": round(row["rec_human_dist"], 2) if row.get("rec_human_dist") is not None else None,
        "rec_assist_dist": round(row["rec_assist_dist"], 2) if row.get("rec_assist_dist") is not None else None,
        "n_objects": len(row.get("objects") or []),
        "raw_to_assist": round(row["raw_to_assist"], 4) if row.get("raw_to_assist") is not None else None,
        "raw_to_good": round(row["raw_to_good"], 4) if row.get("raw_to_good") is not None else None,
        "a1_excess": round(row["a1_excess"], 4) if row.get("a1_excess") is not None else None,
        "a1_action": (row.get("a1") or {}).get("action"),
        "a1_category": (row.get("a1") or {}).get("category"),
        "a1_bars_present": (row.get("a1") or {}).get("bars_present"),
        "a1_good_removed": (row.get("a1") or {}).get("good_removed"),
        "a3_resolution": (row.get("a3") or {}).get("resolution"),
        "a3_human_keys": (row.get("a3") or {}).get("human_keys"),
        "a3_correlation": (row.get("a3") or {}).get("correlation"),
        "a4_event": (row.get("a4") or {}).get("event"),
        "a5_covers": (row.get("a5") or {}).get("good_covers_comp"),
        "human_keyframes": (row.get("keyframes") or {}).get("n"),
        "human_key_amplitude": round((row.get("keyframes") or {}).get("amplitude_source") or 0, 2),
    }
    return out


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--only", type=str, default="")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()
    projects = common.discover()
    if args.only:
        projects = [p for p in projects if args.only.lower() in f"{p['folder']}/{p['stem']}".lower()]
    if args.limit:
        projects = projects[: args.limit]

    all_rows: list[dict[str, Any]] = []
    all_decisions: list[dict[str, Any]] = []
    all_events: list[dict[str, Any]] = []
    index = []
    for project in projects:
        payload = project_main(project, verbose=args.verbose)
        if payload is None:
            print(f"SKIP {project['folder']}/{project['stem']}", file=sys.stderr)
            continue
        filename = re.sub(r"[^A-Za-z0-9._-]+", "_", f"{project['folder']}__{project['stem']}")[:180]
        path = PROJECTS_OUT / f"{filename}.json"
        path.write_text(json.dumps(payload, ensure_ascii=False, indent=1), encoding="utf-8")
        index.append(
            {
                "project": payload["project"],
                "file": path.name,
                "segments": payload["counts"]["assist_segments"],
                "decisions": payload["counts"]["actions"],
                "events": len(payload["events"]),
                "baseline": payload["baseline"]["value"],
            }
        )
        for row in payload["rows"]:
            all_rows.append(flatten_row(row))
        all_decisions.extend(payload["decisions"])
        for event in payload["events"]:
            all_events.append(
                {
                    "project": event.get("project"),
                    "type": event.get("type"),
                    "target": event.get("target"),
                    "in": round(event.get("in") or 0, 3),
                    "out": round(event.get("out") or 0, 3),
                    "neighbor": event.get("neighbor"),
                    "parts": " | ".join(event.get("parts") or []),
                }
            )

    (OUT / "projects_index.json").write_text(
        json.dumps(index, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    write_csv(OUT / "segments.csv", all_rows, list(all_rows[0].keys()) if all_rows else [])
    decision_flat = []
    for decision in all_decisions:
        ev = decision.get("evaluation") or {}
        decision_flat.append(
            {
                "project": decision.get("project"),
                "code": decision.get("code"),
                "action": decision.get("action"),
                "target": decision.get("target"),
                "reason": decision.get("reason"),
                "confidence": decision.get("confidence"),
                "relationship": decision.get("relationship"),
                "baseline": decision.get("baseline"),
                "eval_apply_acceptance": ev.get("apply_acceptance"),
                "eval_review_outcome": ev.get("review_outcome"),
                "eval_good_removed": ev.get("good_removed"),
                "eval_good_residual": ev.get("good_residual"),
                "eval_good_cleared": ev.get("good_cleared"),
                "eval_bars_present": ev.get("bars_present"),
                "eval_scale_target": ev.get("scale_target"),
                "eval_zoom_rel": ev.get("zoom_rel"),
                "eval_active_dy": ev.get("active_dy"),
                "eval_a3_resolution": ev.get("resolution"),
                "eval_a3_human_keys": ev.get("human_keys"),
                "eval_a3_correlation": ev.get("correlation"),
                "eval_a4_event": ev.get("event"),
                "eval_a5_covers": ev.get("good_covers_comp"),
                "eval_median_active_dy": ev.get("median_active_dy"),
                "eval_good_action": ev.get("good_action"),
            }
        )
    write_csv(OUT / "decisions.csv", decision_flat, list(decision_flat[0].keys()) if decision_flat else [])
    write_csv(OUT / "events.csv", all_events, list(all_events[0].keys()) if all_events else [])
    print(f"projects={len(index)} segments={len(all_rows)} decisions={len(all_decisions)} events={len(all_events)}")


if __name__ == "__main__":
    main()
