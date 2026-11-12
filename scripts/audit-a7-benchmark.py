#!/usr/bin/env python3
"""Consolide un audit `workflow-cli email audit --json` en tableaux Markdown.

Le corpus de référence est stratifié par période (avant/après une date de
bascule) et par format (HTML seul ou texte/plain). Aucun accès réseau ni base :
le script ne lit que le JSON produit par la commande d'audit.
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path

FIELDS = [
    "emails",
    "instructions",
    "no_logo",
    "with_logos",
    "unresolved",
    "tokens",
    "resolved",
    "known_absent",
    "missing",
    "ambiguous",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("audit", type=Path, help="JSON produit par email audit --json")
    parser.add_argument(
        "--split",
        default="2026-10",
        help="Premier mois de la période récente (défaut : 2026-10)",
    )
    return parser.parse_args()


def rate(values: dict[str, int]) -> str:
    denominator = values["tokens"] - values["known_absent"]
    if denominator <= 0:
        return "n/a"
    return f"{100.0 * values['resolved'] / denominator:.1f} %"


def add(target: dict[str, int], source: dict[str, int]) -> None:
    for field in FIELDS:
        target[field] += int(source.get(field, 0))


def table(title: str, groups: dict[str, dict[str, int]]) -> None:
    print(f"\n### {title}\n")
    print(
        "| Cohorte | Emails | Instructions | Logos requis | Tokens | "
        "Résolus | Absences volontaires | Manquants | Ambigus | Non résolus | Taux |"
    )
    print("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
    for name, values in groups.items():
        print(
            f"| {name} | {values['emails']} | {values['instructions']} | "
            f"{values['with_logos']} | {values['tokens']} | {values['resolved']} | "
            f"{values['known_absent']} | {values['missing']} | {values['ambiguous']} | "
            f"{values['unresolved']} | {rate(values)} |"
        )


def main() -> None:
    args = parse_args()
    data = json.loads(args.audit.read_text(encoding="utf-8"))
    format_cohorts = data.get("format_cohorts")
    if not format_cohorts:
        raise SystemExit("JSON sans format_cohorts : relancer email audit avec --json")

    history: dict[str, dict[str, int]] = {}
    recent: dict[str, dict[str, int]] = {}
    for key, values in sorted(format_cohorts.items()):
        month, _, fmt = key.partition("|")
        target = recent if month >= args.split else history
        bucket = target.setdefault(fmt, defaultdict(int))
        add(bucket, values)
    for target in (history, recent):
        target["total"] = defaultdict(int)
        for fmt in ("html_only", "text_plain"):
            if fmt in target:
                add(target["total"], target[fmt])
    total = defaultdict(int)
    for values in format_cohorts.values():
        add(total, values)

    table("Historique (avant " + args.split + ")", history)
    table("Récent (depuis " + args.split + ")", recent)
    print("\n### Corpus complet\n")
    print(
        f"- Emails : {total['emails']} (HTML seul : {data.get('html_only', 0)}, "
        f"texte/plain : {total['emails'] - data.get('html_only', 0)})"
    )
    print(f"- Lots : {data.get('lots', 0)}")
    print(
        f"- Instructions : {total['instructions']} ({total['no_logo']} sans logo, "
        f"{total['with_logos']} avec logos, {total['unresolved']} non résolues)"
    )
    print(
        f"- Tokens : {total['tokens']} ; résolus : {total['resolved']} ; "
        f"absences volontaires : {total['known_absent']} ; manquants : "
        f"{total['missing']} ; ambigus : {total['ambiguous']} ; taux : {rate(total)}"
    )
    print(f"- Politiques MOV : {json.dumps(data.get('policies', {}), ensure_ascii=False)}")

    top_missing = data.get("top_missing") or []
    if top_missing:
        print("\n### Principaux assets manquants\n")
        print("| Asset | Occurrences |")
        print("| --- | ---: |")
        for item in top_missing[:10]:
            print(f"| {item['name']} | {item['count']} |")


if __name__ == "__main__":
    main()
