#!/usr/bin/env python3
"""Applique la réduction audio de STEP5 à un `_audio.json` brut.

STEP3 produit un schéma brut (video_filename, speaker_stats par durée,
num_distinct_speakers_audio). STEP5 le projette vers le schéma consommé par
STEP6. La copie de production ayant déjà été réduite, le cas de parité STEP3
doit appliquer la même projection avant comparaison.
"""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) < 2:
        print("usage: reduce_audio.py <audio.json>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1]).resolve()
    old_base = Path(os.environ.get("OLD_BASE", "/home/kidpixel/workflow_mediapipe"))
    sys.path.insert(0, str(old_base))

    from workflow_scripts.step5.json_reducer import stream_reduce_audio_json

    result = stream_reduce_audio_json(path, path)
    print("REDUCE_AUDIO: " + json.dumps(result or {}, ensure_ascii=False), flush=True)
    return 0 if result else 1


if __name__ == "__main__":
    raise SystemExit(main())
