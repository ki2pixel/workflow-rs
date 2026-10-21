#!/usr/bin/env python3
"""Auto-test du comparateur de parité.

Vérifie qu'une différence injectée est bien détectée pour chaque type de
sortie, et qu'un dossier intact reste égal. À lancer avec le Python du harnais
(coral_env) après avoir exécuté la baseline.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CLI = ROOT / "tools" / "parity" / "parity.py"
FIXTURES = ROOT / "parity" / "fixtures"


def run_diff(expected: Path, actual: Path) -> tuple[int, str]:
    proc = subprocess.run(
        [
            sys.executable, str(CLI), "diff",
            "--expected", str(expected),
            "--actual", str(actual),
            "--max-diffs", "5",
        ],
        capture_output=True,
        text=True,
    )
    return proc.returncode, proc.stdout + proc.stderr


def main() -> int:
    required = [
        FIXTURES / "real_244_step6" / "expected" / "real_244_ae.json",
        FIXTURES / "real_244_step6" / "actual" / "real_244_ae.json",
        FIXTURES / "cuts_step2" / "expected" / "synthetic_cuts.csv",
        FIXTURES / "short_2997_step1" / "expected" / "short_2997.mp4",
        FIXTURES / "vfr_step1" / "expected" / "synthetic_vfr.mp4",
    ]
    missing = [str(path) for path in required if not path.exists()]
    if missing:
        print("Fixtures manquantes, lancer la baseline d'abord :")
        for path in missing:
            print(f"  - {path}")
        return 2

    tmp = Path(tempfile.mkdtemp(prefix="parity_selftest_"))
    failures: list[str] = []

    def check(name: str, condition: bool, detail: str) -> None:
        status = "PASS" if condition else "FAIL"
        print(f"[{status}] {name} — {detail}")
        if not condition:
            failures.append(name)

    try:
        # Contrôle : fixtures intactes.
        expected = FIXTURES / "real_244_step6" / "expected"
        actual = FIXTURES / "real_244_step6" / "actual"
        rc, output = run_diff(expected, actual)
        check("control", rc == 0 and "equal: True" in output, f"rc={rc}")

        # Mutation JSON.
        json_expected = tmp / "json_expected"
        json_actual = tmp / "json_actual"
        json_expected.mkdir()
        json_actual.mkdir()
        source = expected / "real_244_ae.json"
        shutil.copy2(source, json_expected / source.name)
        payload = json.loads(source.read_text(encoding="utf-8"))
        payload["total_frames"] = int(payload.get("total_frames", 0)) + 1
        (json_actual / source.name).write_text(
            json.dumps(payload, ensure_ascii=False), encoding="utf-8"
        )
        rc, output = run_diff(json_expected, json_actual)
        check("json", rc == 3 and "total_frames" in output, f"rc={rc}")

        # Mutation CSV.
        csv_expected = tmp / "csv_expected"
        csv_actual = tmp / "csv_actual"
        csv_expected.mkdir()
        csv_actual.mkdir()
        source = FIXTURES / "cuts_step2" / "expected" / "synthetic_cuts.csv"
        shutil.copy2(source, csv_expected / source.name)
        rows = source.read_text(encoding="utf-8").splitlines()
        cells = rows[-1].split(",")
        cells[-1] = str(int(cells[-1]) + 1)
        rows[-1] = ",".join(cells)
        (csv_actual / source.name).write_text("\n".join(rows) + "\n", encoding="utf-8")
        rc, output = run_diff(csv_expected, csv_actual)
        check("csv", rc == 3 and "row[" in output, f"rc={rc}")

        # Mutation média : deux vidéos de propriétés différentes.
        media_expected = tmp / "media_expected"
        media_actual = tmp / "media_actual"
        media_expected.mkdir()
        media_actual.mkdir()
        shutil.copy2(
            FIXTURES / "short_2997_step1" / "expected" / "short_2997.mp4",
            media_expected / "media.mp4",
        )
        shutil.copy2(
            FIXTURES / "vfr_step1" / "expected" / "synthetic_vfr.mp4",
            media_actual / "media.mp4",
        )
        rc, output = run_diff(media_expected, media_actual)
        detected = any(token in output for token in ("width", "height", "r_frame_rate", "duration"))
        check("media", rc == 3 and detected, f"rc={rc}")

        # Mutation binaire.
        bin_expected = tmp / "bin_expected"
        bin_actual = tmp / "bin_actual"
        bin_expected.mkdir()
        bin_actual.mkdir()
        (bin_expected / "x.bin").write_bytes(b"alpha")
        (bin_actual / "x.bin").write_bytes(b"beta")
        rc, output = run_diff(bin_expected, bin_actual)
        check("binary", rc == 3 and "sha256" in output, f"rc={rc}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if failures:
        print(f"\nÉchecs: {', '.join(failures)}")
        return 1
    print("\nAuto-test réussi.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
