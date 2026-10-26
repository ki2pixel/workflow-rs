# Lot 1 — STEP6

| Champ | Valeur |
| --- | --- |
| Type | Lot |
| Statut | Référence |
| Date | 2026-10-24 |
| Portée | Construction de `<stem>_ae.json`, index tracking/audio et recommandations de segments. |
| Voir aussi | `docs/contracts/CONTRAT-ENVIRONNEMENT.md`, `docs/pipeline/LOT-1-STEP5.md`, `parity/DRIFT.md` |

## Périmètre livré

- Construction de `<stem>_ae.json` à partir de `<stem>_tracking.json`, `<stem>_audio.json` et `<stem>.csv`.
- Index `dataByFrame` filtré sur `face_landmarker` et `object_detector/person`, enrichi avec `is_speaking` et `tier`.
- Index `audioByFrame` et `audioMaxFrame`.
- Parse CSV (colonnes `Frame In`/`Frame Out`, repli timecodes) et `segment_recommendations`.
- Copie de `total_frames`, `tracking_analytics`, `temporal_alignment`, `expression_summary` et `speaker_embeddings` depuis STEP5.
- Écriture atomique, JSON indenté.

Le mode `analyze_manifest` du panneau AE et le pont Windows sont abandonnés
(D-022) : le panneau CEP continue d'appeler Python, et STEP8 est validé en
headless avec `effectcraft-cli`.

## Parité

| Cas | Entrée | Résultat |
| --- | --- | --- |
| 244 | tracking STEP5 + audio + CSV production | identique |
| 245 | tracking STEP5 + audio + CSV production | identique |
| 249 | tracking STEP5 + audio + CSV production | identique |
| 250 | tracking STEP5 + audio + CSV production | identique |

Les quatre sorties passent le comparateur sémantique avec zéro différence.

Points vérifiés :

- `maxFrame` inclut les frames sans objet retenu ; `dataByFrame` ne contient que les frames avec au moins un objet retenu.
- `audioByFrame` contient les 4 921 / 4 729 / 6 119 / 3 326 entrées audio.
- Les recommandations de segments sont identiques, y compris les cas sans candidat (`tier = 0`).
- `tracking_analytics` et `temporal_alignment` sont recopiés sans recalcul.

## Performance

Mesures release, mêmes entrées que la parité.

| Cas | Rust STEP6 | Baseline Python | Ratio | RSS Rust |
| --- | ---: | ---: | ---: | ---: |
| 244 | 0,108 s | 0,466 s | 4,3× | 44 Mio |
| 245 | 0,098 s | 0,521 s | 5,3× | < 44 Mio |
| 249 | 0,127 s | 0,517 s | 4,1× | < 44 Mio |
| 250 | 0,070 s | 0,319 s | 4,6× | < 44 Mio |

Le RSS maximum mesuré est de 44 Mio, sous la cible de 100 Mio. La cible de
débit 10× n'est pas atteinte sur STEP6 : le parseur `Value` charge le tracking
et l'audio complets, puis construit les index et le JSON de sortie. Une version
streaming du tracking et une sérialisation directe des index sont les pistes
d'optimisation.

## Reproduction

```sh
./target/release/workflow-cli step6 build \
  --tracking parity/fixtures/real_244_step5/expected/real_244_tracking.json \
  --audio parity/fixtures/real_244_step3/expected/real_244_audio.json \
  --csv "/mnt/cache/244 Cvmille/docs/<stem>.csv" \
  --output /tmp/real_244_ae.json

../workflow_mediapipe/coral_env/bin/python tools/parity/parity.py diff \
  --expected parity/fixtures/real_244_step6/expected \
  --actual /tmp
```
