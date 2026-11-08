"""Common helpers for the GOOD post-STEP8 corpus study.

Read-only on the corpus. Outputs go under ETUDE_OUT (par défaut
scripts/out/ à côté de ce fichier).
"""
from __future__ import annotations

import json
import math
import os
import re
from pathlib import Path
from typing import Any, Iterable

CORPUS = Path(
    os.environ.get(
        "ETUDE_CORPUS",
        "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2",
    )
)
TICKS_PER_SECOND = 254_016_000_000.0
COMP_W = 1080.0
COMP_H = 1920.0
EPS_T = 1e-6
BASE_RE = re.compile(r"^(Segment \d+ \([^)]*\))( \d+)?$")


def out_dir() -> Path:
    """Sortie des scripts ; surchargeable par ETUDE_OUT."""
    return Path(
        os.environ.get("ETUDE_OUT", str(Path(__file__).resolve().parent / "out"))
    )


def base_name(name: str) -> str:
    match = BASE_RE.match(name or "")
    if match:
        return match.group(1)
    return name or ""


def load_json(path: Path) -> Any:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def discover() -> list[dict[str, Any]]:
    projects: list[dict[str, Any]] = []
    if not CORPUS.is_dir():
        raise SystemExit(f"corpus introuvable: {CORPUS}")
    for folder in sorted(CORPUS.iterdir(), key=lambda p: p.name):
        if not folder.is_dir():
            continue
        projets = folder / "projets"
        if not projets.is_dir():
            continue
        raws = sorted(projets.glob("*_9x16.ecproj"))
        for raw in raws:
            if raw.name.endswith("_assist.ecproj") or raw.name.endswith(
                "_assist_GOOD.ecproj"
            ):
                continue
            stem = raw.name[: -len("_9x16.ecproj")]
            assist = projets / f"{stem}_9x16_assist.ecproj"
            good = projets / f"{stem}_9x16_assist_GOOD.ecproj"
            artifacts = folder / "docs" / "post" / f"{stem}_9x16_assist"
            projects.append(
                {
                    "folder": folder.name,
                    "stem": stem,
                    "raw": raw,
                    "assist": assist,
                    "good": good,
                    "artifacts": artifacts,
                    "post": artifacts / "post.json",
                    "review": artifacts / "review.json",
                    "audit": artifacts / "audit.json",
                    "source_video": folder / "docs" / f"{stem}.mp4",
                    "ae": folder / "docs" / f"{stem}_ae.json",
                    "csv": folder / "docs" / f"{stem}.csv",
                    "export_video": folder / f"{stem}_9x16.mp4",
                }
            )
    return projects


def comp_of(doc: dict[str, Any]) -> dict[str, Any]:
    best: tuple[int, dict[str, Any]] | None = None
    items = doc.get("items")
    if not isinstance(items, dict):
        raise ValueError("projet sans items")
    for item in items.values():
        kind = item.get("kind")
        if not isinstance(kind, dict) or kind.get("type") != "Comp":
            continue
        count = sum(
            1
            for layer in kind.get("layers", [])
            if str(layer.get("name", "")).startswith("Segment")
        )
        if best is None or count > best[0]:
            best = (count, kind)
    if best is None:
        raise ValueError("aucune composition")
    return best[1]


def comp_meta(doc: dict[str, Any]) -> dict[str, Any]:
    comp = comp_of(doc)
    rate = comp.get("frame_rate") or {}
    fps = 0.0
    if isinstance(rate, dict):
        num = rate.get("num")
        den = rate.get("den")
        if isinstance(num, (int, float)) and isinstance(den, (int, float)) and den:
            fps = float(num) / float(den)
    duration_ticks = comp.get("duration")
    duration = (
        float(duration_ticks) / TICKS_PER_SECOND
        if isinstance(duration_ticks, (int, float))
        else 0.0
    )
    layers = comp.get("layers") or []
    return {
        "width": comp.get("width"),
        "height": comp.get("height"),
        "fps": fps,
        "duration": duration,
        "n_layers": len(layers),
        "n_segments": sum(
            1 for layer in layers if str(layer.get("name", "")).startswith("Segment")
        ),
    }


def transform(layer: dict[str, Any]) -> dict[str, Any] | None:
    props = layer.get("props") or {}
    children = props.get("children") or []
    for child in children:
        if child.get("name") == "Transform":
            out: dict[str, Any] = {}
            for prop in child.get("children") or []:
                name = prop.get("name")
                if name:
                    out[name] = prop
            return out
    return None


def prop_value(prop: dict[str, Any] | None, t: float) -> list[float] | float | None:
    if not prop:
        return None
    keys = prop.get("keys")
    if isinstance(keys, list) and keys:
        parsed: list[tuple[float, list[float] | float]] = []
        for key in keys:
            time = key.get("time")
            value = key.get("value", {}).get("v")
            if not isinstance(time, (int, float)) or value is None:
                continue
            parsed.append((float(time) / TICKS_PER_SECOND, value))
        if not parsed:
            return None
        parsed.sort(key=lambda item: item[0])
        if t <= parsed[0][0]:
            return parsed[0][1]
        if t >= parsed[-1][0]:
            return parsed[-1][1]
        for left, right in zip(parsed, parsed[1:]):
            if left[0] <= t <= right[0]:
                span = right[0] - left[0]
                if span <= 0:
                    return right[1]
                ratio = (t - left[0]) / span
                lv, rv = left[1], right[1]
                if isinstance(lv, list) and isinstance(rv, list):
                    return [
                        float(a) + (float(b) - float(a)) * ratio
                        for a, b in zip(lv, rv)
                    ]
                if isinstance(lv, (int, float)) and isinstance(rv, (int, float)):
                    return float(lv) + (float(rv) - float(lv)) * ratio
                return lv
    value = prop.get("value", {}).get("v")
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, list):
        return [float(v) for v in value]
    return None


def prop_keys(prop: dict[str, Any] | None) -> list[dict[str, Any]]:
    if not prop:
        return []
    keys = prop.get("keys")
    if not isinstance(keys, list):
        return []
    out: list[dict[str, Any]] = []
    for key in keys:
        time = key.get("time")
        value = key.get("value", {}).get("v")
        if not isinstance(time, (int, float)) or value is None:
            continue
        out.append(
            {
                "t": float(time) / TICKS_PER_SECOND,
                "v": value,
                "in_interp": key.get("in_interp"),
                "out_interp": key.get("out_interp"),
            }
        )
    out.sort(key=lambda item: item["t"])
    return out


def scalar(value: Any, index: int = 0) -> float | None:
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, list) and len(value) > index and isinstance(
        value[index], (int, float)
    ):
        return float(value[index])
    return None


def layer_info(layer: dict[str, Any], items: dict[str, Any]) -> dict[str, Any]:
    source = layer.get("source") or {}
    item_id = source.get("item")
    source_name = None
    if isinstance(item_id, (int, float)):
        item = items.get(str(int(item_id)))
        if isinstance(item, dict):
            source_name = item.get("name")
    in_point = layer.get("in_point")
    out_point = layer.get("out_point")
    return {
        "id": layer.get("id"),
        "name": layer.get("name") or "",
        "base": base_name(layer.get("name") or ""),
        "parent": layer.get("parent"),
        "source_item": item_id,
        "source_name": source_name,
        "source_type": source.get("type"),
        "in": float(in_point) / TICKS_PER_SECOND
        if isinstance(in_point, (int, float))
        else 0.0,
        "out": float(out_point) / TICKS_PER_SECOND
        if isinstance(out_point, (int, float))
        else 0.0,
        "in_ticks": in_point,
        "out_ticks": out_point,
        "opacity": scalar(prop_value(transform(layer).get("Opacity") if transform(layer) else None, 0.0), 0) if transform(layer) else None,
    }


class Project:
    def __init__(self, doc: dict[str, Any]):
        self.doc = doc
        self.items = doc.get("items") or {}
        self.comp = comp_of(doc)
        self.layers = list(self.comp.get("layers") or [])
        self.by_id = {
            layer.get("id"): layer
            for layer in self.layers
            if isinstance(layer.get("id"), (int, float))
        }

    def transform_of(self, layer: dict[str, Any]) -> dict[str, Any] | None:
        return transform(layer)

    def affine_at(self, layer: dict[str, Any], t: float) -> dict[str, float]:
        """Compose the parent chain. Returns x/y scale and offset.

        x_comp = offset_x + scale_x * x_source
        y_comp = offset_y + scale_y * y_source
        No rotation is expected on Segment layers; overlays are not projected
        with this helper.
        """
        def apply(point: tuple[float, float], current: dict[str, Any], time: float) -> tuple[float, float]:
            tr = self.transform_of(current)
            if not tr:
                return point
            position = prop_value(tr.get("Position"), time)
            anchor = prop_value(tr.get("Anchor Point"), time)
            scale = prop_value(tr.get("Scale"), time)
            px = scalar(position, 0) or 0.0
            py = scalar(position, 1) or 0.0
            ax = scalar(anchor, 0) or 0.0
            ay = scalar(anchor, 1) or 0.0
            sx = (scalar(scale, 0) or 100.0) / 100.0
            sy = (scalar(scale, 1) or scalar(scale, 0) or 100.0) / 100.0
            x = px + (point[0] - ax) * sx
            y = py + (point[1] - ay) * sy
            parent_id = current.get("parent")
            if isinstance(parent_id, (int, float)) and parent_id in self.by_id:
                return apply((x, y), self.by_id[parent_id], time)
            return x, y

        ox, oy = apply((0.0, 0.0), layer, t)
        xx, xy = apply((1.0, 0.0), layer, t)
        yx, yy = apply((0.0, 1.0), layer, t)
        return {
            "ox": ox,
            "oy": oy,
            "sx": xx - ox,
            "sy": yy - oy,
            "cross_x": xy - oy,
            "cross_y": yx - ox,
        }

    def project_point(self, layer: dict[str, Any], x: float, y: float, t: float) -> tuple[float, float]:
        aff = self.affine_at(layer, t)
        return (
            aff["ox"] + aff["sx"] * x + aff["cross_x"] * y,
            aff["oy"] + aff["cross_y"] * x + aff["sy"] * y,
        )

    def project_rect(
        self, layer: dict[str, Any], rect: tuple[float, float, float, float], t: float
    ) -> tuple[float, float, float, float]:
        x0, y0, x1, y1 = rect
        points = [
            self.project_point(layer, x0, y0, t),
            self.project_point(layer, x1, y0, t),
            self.project_point(layer, x1, y1, t),
            self.project_point(layer, x0, y1, t),
        ]
        xs = [p[0] for p in points]
        ys = [p[1] for p in points]
        return min(xs), min(ys), max(xs), max(ys)

    def summary(
        self,
        layer: dict[str, Any],
        samples: Iterable[float],
        active_rect: tuple[float, float, float, float] | None = None,
    ) -> dict[str, Any]:
        times = list(samples) or [0.0]
        scales: list[float] = []
        centers: list[tuple[float, float]] = []
        active_centers: list[tuple[float, float]] = []
        active_sizes: list[tuple[float, float]] = []
        full_rects: list[tuple[float, float, float, float]] = []
        for t in times:
            aff = self.affine_at(layer, t)
            scales.append(abs(aff["sx"]) * 100.0 if aff["sx"] else 0.0)
            rect = self.project_rect(layer, (0.0, 0.0, 1920.0, 1080.0), t)
            full_rects.append(rect)
            centers.append(((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0))
            if active_rect is not None:
                act = self.project_rect(layer, active_rect, t)
                active_centers.append(((act[0] + act[2]) / 2.0, (act[1] + act[3]) / 2.0))
                active_sizes.append((act[2] - act[0], act[3] - act[1]))
        tr = self.transform_of(layer) or {}
        position = prop_value(tr.get("Position"), times[len(times) // 2])
        anchor = prop_value(tr.get("Anchor Point"), times[len(times) // 2])
        scale = prop_value(tr.get("Scale"), times[len(times) // 2])
        return {
            "scale": median(scales),
            "center_x": median([c[0] for c in centers]),
            "center_y": median([c[1] for c in centers]),
            "full_rect": full_rects[len(full_rects) // 2],
            "active_center_x": median([c[0] for c in active_centers]) if active_centers else None,
            "active_center_y": median([c[1] for c in active_centers]) if active_centers else None,
            "active_w": median([s[0] for s in active_sizes]) if active_sizes else None,
            "active_h": median([s[1] for s in active_sizes]) if active_sizes else None,
            "position": position,
            "anchor": anchor,
            "scale_raw": scale,
            "keyframes": {
                "position": len(prop_keys(tr.get("Position"))),
                "scale": len(prop_keys(tr.get("Scale"))),
                "anchor": len(prop_keys(tr.get("Anchor Point"))),
            },
        }

    def inverse_source_x(self, layer: dict[str, Any], t: float, comp_x: float = 540.0) -> float | None:
        aff = self.affine_at(layer, t)
        if abs(aff["sx"]) < 1e-9:
            return None
        # Ignore cross terms: Segment layers have no rotation in this corpus.
        return (comp_x - aff["ox"]) / aff["sx"]

    def keyframe_stats(self, layer: dict[str, Any]) -> dict[str, Any]:
        tr = self.transform_of(layer) or {}
        pos_keys = prop_keys(tr.get("Position"))
        if not pos_keys:
            return {
                "n": 0,
                "keys": [],
                "amplitude_source": 0.0,
                "monotonic": None,
                "interp": [],
            }
        source_xs: list[float] = []
        comp_xs: list[float] = []
        for key in pos_keys:
            t = key["t"]
            aff = self.affine_at(layer, t)
            # The source point that the keyframe places at the comp center.
            # This is the point the monteur is following/centering. Comparing
            # it to the STEP4 object centroid is meaningful; comparing the
            # raw Position value is not (it is in parent coordinates).
            src = self.inverse_source_x(layer, t, COMP_W / 2.0)
            if src is not None:
                source_xs.append(src)
            comp_xs.append(src if src is not None else 0.0)
        diffs = [b - a for a, b in zip(source_xs, source_xs[1:])]
        monotonic = None
        if diffs:
            monotonic = all(d >= -1e-6 for d in diffs) or all(d <= 1e-6 for d in diffs)
        return {
            "n": len(pos_keys),
            "keys": [
                {
                    "t": key["t"],
                    "v": key["v"],
                    "in_interp": key["in_interp"],
                    "out_interp": key["out_interp"],
                }
                for key in pos_keys
            ],
            "comp_xs": comp_xs,
            "source_xs": source_xs,
            "amplitude_source": (max(source_xs) - min(source_xs)) if source_xs else 0.0,
            "monotonic": monotonic,
            "max_step": max((abs(d) for d in diffs), default=0.0),
            "interp": sorted(
                {
                    str(key.get("in_interp"))
                    for key in pos_keys
                    if key.get("in_interp")
                }
                | {
                    str(key.get("out_interp"))
                    for key in pos_keys
                    if key.get("out_interp")
                }
            ),
        }


def median(values: Iterable[float]) -> float:
    vals = sorted(float(v) for v in values if v is not None and math.isfinite(float(v)))
    if not vals:
        return 0.0
    n = len(vals)
    if n % 2:
        return vals[n // 2]
    return (vals[n // 2 - 1] + vals[n // 2]) / 2.0


def mean(values: Iterable[float]) -> float:
    vals = [float(v) for v in values if v is not None and math.isfinite(float(v))]
    return sum(vals) / len(vals) if vals else 0.0


def rect_intersects(rect: tuple[float, float, float, float], other: tuple[float, float, float, float]) -> bool:
    return not (
        rect[2] <= other[0]
        or rect[0] >= other[2]
        or rect[3] <= other[1]
        or rect[1] >= other[3]
    )


def rect_contains(inner: tuple[float, float, float, float], outer: tuple[float, float, float, float]) -> bool:
    return (
        inner[0] >= outer[0] - 0.5
        and inner[1] >= outer[1] - 0.5
        and inner[2] <= outer[2] + 0.5
        and inner[3] <= outer[3] + 0.5
    )


def bar_rects(width: float, height: float, bars: dict[str, Any]) -> list[tuple[float, float, float, float]]:
    out: list[tuple[float, float, float, float]] = []
    top = float(bars.get("top") or 0)
    bottom = float(bars.get("bottom") or 0)
    left = float(bars.get("left") or 0)
    right = float(bars.get("right") or 0)
    if top > 0:
        out.append((0.0, 0.0, width, top))
    if bottom > 0:
        out.append((0.0, height - bottom, width, height))
    if left > 0:
        out.append((0.0, 0.0, left, height))
    if right > 0:
        out.append((width - right, 0.0, width, height))
    return out


def bars_residual(
    project: Project,
    layer: dict[str, Any],
    times: Iterable[float],
    width: float,
    height: float,
    bars: dict[str, Any],
) -> float:
    """Largest bar overlap with the comp, in comp pixels.

    A1's cropdetect has a few pixels of uncertainty on real footage; 0 px is
    the strict contract, but a 4-8 px residue is often a detection edge, not
    a visible bar. Callers decide the tolerance.
    """
    rects = bar_rects(width, height, bars)
    if not rects:
        return 0.0
    worst = 0.0
    for t in times:
        for rect in rects:
            projected = project.project_rect(layer, rect, t)
            overlap_w = min(projected[2], COMP_W) - max(projected[0], 0.0)
            overlap_h = min(projected[3], COMP_H) - max(projected[1], 0.0)
            if overlap_w > 0 and overlap_h > 0:
                worst = max(worst, min(overlap_w, overlap_h))
    return worst


def bars_removed(
    project: Project,
    layer: dict[str, Any],
    times: Iterable[float],
    width: float,
    height: float,
    bars: dict[str, Any],
    tolerance: float = 0.5,
) -> bool:
    rects = bar_rects(width, height, bars)
    if not rects:
        return False
    for t in times:
        for rect in rects:
            projected = project.project_rect(layer, rect, t)
            overlap_w = min(projected[2], COMP_W) - max(projected[0], 0.0)
            overlap_h = min(projected[3], COMP_H) - max(projected[1], 0.0)
            if overlap_w > tolerance and overlap_h > tolerance:
                return False
    return True


def banner_removed(
    project: Project,
    layer: dict[str, Any],
    times: Iterable[float],
    bbox: tuple[float, float, float, float],
) -> bool:
    for t in times:
        projected = project.project_rect(layer, bbox, t)
        overlap_w = min(projected[2], COMP_W) - max(projected[0], 0.0)
        overlap_h = min(projected[3], COMP_H) - max(projected[1], 0.0)
        if overlap_w > 0.5 and overlap_h > 0.5:
            return False
    return True


def sample_times(layer: dict[str, Any], count: int = 5) -> list[float]:
    start = layer.get("in_point")
    end = layer.get("out_point")
    if not isinstance(start, (int, float)) or not isinstance(end, (int, float)):
        return [0.0]
    start = float(start) / TICKS_PER_SECOND
    end = float(end) / TICKS_PER_SECOND
    if end <= start:
        return [start]
    if count <= 1:
        return [(start + end) / 2.0]
    margin = min(0.05, (end - start) * 0.05)
    low = start + margin
    high = end - margin
    if high <= low:
        return [(start + end) / 2.0]
    return [low + (high - low) * i / (count - 1) for i in range(count)]
