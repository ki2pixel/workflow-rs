#!/usr/bin/env python3
"""Inventory and structural validation of the GOOD corpus (read-only)."""
from __future__ import annotations

import csv
import json
from collections import Counter
from pathlib import Path

import common


OUT = common.out_dir()
OUT.mkdir(parents=True, exist_ok=True)


def channel_of(stem: str) -> str:
    s = stem.lower()
    if "ujud" in s:
        return "UJUD / M6"
    if s.startswith("66m") or "66 minutes" in s:
        return "66 Minutes"
    if "sdm" in s:
        return "M6 SDM"
    if s.startswith("gulli") or "totally spies" in s:
        return "Gulli"
    if "laa9" in s:
        return "W9 LAA9"
    if "wanted" in s:
        return "WANTED"
    if s.startswith("m6+"):
        return "M6+ série"
    if "le seigneur des anneaux" in s:
        return "M6 cinéma"
    if s.startswith("6ter") or "lrldr" in s or "lrdlr" in s or "lrdrld" in s:
        return "6ter"
    if "e=m6" in s or "tat_m6" in s or "em6" in s or "extrait" in s:
        return "M6"
    if s.startswith("pp -"):
        return "PP"
    if "template" in s:
        return "Template"
    if "reco" in s:
        return "Gulli"
    return "Autre"


def main() -> None:
    projects = common.discover()
    rows = []
    problems = []
    for project in projects:
        row = {
            "folder": project["folder"],
            "stem": project["stem"],
            "channel": channel_of(project["stem"]),
            "raw": project["raw"].is_file(),
            "assist": project["assist"].is_file(),
            "good": project["good"].is_file(),
            "post": project["post"].is_file(),
            "review": project["review"].is_file(),
            "audit": project["audit"].is_file(),
            "source_video": project["source_video"].is_file(),
            "ae": project["ae"].is_file(),
            "csv": project["csv"].is_file(),
            "export_video": project["export_video"].is_file(),
        }
        for key in ("raw", "assist", "good", "post", "review", "audit"):
            if not row[key]:
                problems.append(f"{project['folder']}/{project['stem']}: {key} manquant")
        if all(row[k] for k in ("assist", "post", "review")):
            assist = common.load_json(project["assist"])
            post = common.load_json(project["post"])
            review = common.load_json(project["review"])
            audit = common.load_json(project["audit"])
            meta = common.comp_meta(assist)
            actions = post.get("actions") or []
            reviews = review.get("items") or []
            row.update(
                {
                    "segments": meta["n_segments"],
                    "duration_s": round(meta["duration"], 3),
                    "fps": meta["fps"],
                    "actions": len(actions),
                    "reviews": len(reviews),
                    "a1_apply": sum(1 for a in actions if a.get("code") == "A1" and a.get("action") == "apply"),
                    "a2_apply": sum(1 for a in actions if a.get("code") == "A2" and a.get("action") == "apply"),
                    "a6_apply": sum(1 for a in actions if a.get("code") == "A6" and a.get("action") == "apply"),
                    "a3_review": sum(1 for a in actions if a.get("code") == "A3" and a.get("action") == "review"),
                    "audit_verdict": audit.get("verdict")
                    or (
                        "fail"
                        if any(c.get("status") == "fail" for c in audit.get("checks", []))
                        else "warn"
                        if any(c.get("status") == "warn" for c in audit.get("checks", []))
                        else "pass"
                    ),
                }
            )
            # Structural consistency: review items must correspond to review actions.
            review_actions = {
                (a.get("code"), a.get("target"))
                for a in actions
                if a.get("action") == "review"
            }
            review_items = {(r.get("code"), r.get("target")) for r in reviews}
            if not review_items <= review_actions:
                problems.append(
                    f"{project['folder']}/{project['stem']}: review.json non inclus dans post.json"
                )
            if len(reviews) != len(review_actions):
                # A1/A2 comp decisions can be duplicated? In practice counts match.
                problems.append(
                    f"{project['folder']}/{project['stem']}: {len(reviews)} reviews vs {len(review_actions)} actions review"
                )
            # Apply targets must exist in the assist project.
            names = {
                layer.get("name")
                for layer in common.comp_of(assist).get("layers", [])
                if str(layer.get("name", "")).startswith("Segment")
            }
            for action in actions:
                if action.get("action") == "apply" and str(action.get("target", "")).startswith("Segment"):
                    if action.get("target") not in names:
                        problems.append(
                            f"{project['folder']}/{project['stem']}: cible apply absente {action.get('target')}"
                        )
        else:
            row.update({"segments": None, "duration_s": None, "fps": None, "actions": None, "reviews": None})
        rows.append(row)

    columns = list(rows[0].keys()) if rows else []
    with (OUT / "inventory.csv").open("w", encoding="utf-8", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=columns)
        writer.writeheader()
        writer.writerows(rows)

    summary = {
        "projects": len(rows),
        "folders": len({r["folder"] for r in rows}),
        "channels": dict(Counter(r["channel"] for r in rows).most_common()),
        "segments": sum(r.get("segments") or 0 for r in rows),
        "reviews": sum(r.get("reviews") or 0 for r in rows),
        "a1_apply": sum(r.get("a1_apply") or 0 for r in rows),
        "a2_apply": sum(r.get("a2_apply") or 0 for r in rows),
        "a6_apply": sum(r.get("a6_apply") or 0 for r in rows),
        "a3_review": sum(r.get("a3_review") or 0 for r in rows),
        "with_export": sum(1 for r in rows if r["export_video"]),
        "with_source": sum(1 for r in rows if r["source_video"]),
        "without_review": [
            f"{r['folder']}/{r['stem']}" for r in rows if r.get("reviews") == 0
        ],
        "problems": problems,
    }
    (OUT / "inventory.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    print(json.dumps(summary, ensure_ascii=False, indent=2))
    print(f"\nÉcrit: {OUT/'inventory.csv'}")


if __name__ == "__main__":
    main()
