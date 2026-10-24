# Lot 1 — STEP5 audio et vidéo

## Périmètre livré

- Réduction audio : `frames_analysis` filtré sur `frame` et `audio_info`, comptage des locuteurs, conservation de `speaker_embeddings`, écriture atomique.
- Réduction vidéo : lecture streaming des frames STEP4, réduction des objets, `tracking_analytics`, `temporal_alignment`, écriture atomique.
- En-tête (`fps`, `total_frames`) écrit avant `frames_analysis` ; une seconde réduction conserve les métadonnées.
- Chemin rapide typé pour le cas de production (`expression_summary = false`) : les `blendshapes` et les champs non consommés sont ignorés sans allocation.
- Chemin `Value` conservé pour `expression_summary = true`.
- `workflow-cli step5 audio` et `workflow-cli step5 video`.

## Parité

| Cas | Entrée | Sortie | Résultat |
| --- | --- | --- | --- |
| 244 | STEP4 brut `real_244.json` + `real_244_audio.json` | STEP5 production | identique |
| 245 | STEP4 brut `real_245.json` + `real_245_audio.json` | STEP5 production | identique |
| 249 | STEP5 production réduit + audio production | STEP5 production | identique (idempotence) |
| 250 | STEP5 production réduit + audio production | STEP5 production | identique (idempotence) |

Les quatre comparaisons passent le comparateur sémantique de `tools/parity/compare.py` avec zéro différence.

Points de parité corrigés pendant le port :

- `objects_total` compte les identifiants uniques, pas les instances.
- `presence_ratio` est `presence_frames / frames_total`.
- `avg_objects_per_frame` est divisé par `frames_with_objects`, pas par `frames_total`.
- `round(x, n)` de Python est reproduit par formatage décimal ; les cas `0.60005 → 0.6`, `2919.125 → 2919.12` sont couverts par des tests.
- `temporal_alignment` conserve `video_fps = null` quand STEP4 ne fournit pas `fps`.

## Performance

Mesures release, serveur Flask arrêté, mêmes fichiers d'entrée que la parité.

| Cas | Rust STEP5 | Baseline Python | Ratio | RSS Rust |
| --- | ---: | ---: | ---: | ---: |
| 244 | 0,096 s | 0,567 s | 5,9× | 16 Mio |
| 245 | 0,101 s | 0,521 s | 5,2× | 16 Mio |
| 249 | 0,072 s | 0,691 s | 9,6× | 16 Mio |
| 250 | 0,039 s | 0,381 s | 9,8× | 16 Mio |

Le RSS maximum mesuré est de 16 Mio, sous la cible de 100 Mio. Le débit dépasse
la cible de 10× sur les entrées déjà réduites (249, 250) et s'en approche sur
244/245. Le coût restant sur 244/245 est le scan des `blendshapes` du STEP4 brut
(8,5 Mio de JSON) ; le chemin rapide les ignore sans les allouer, mais doit
encore les parcourir.

Décision ouverte : accepter ce niveau (5–6× sur les STEP4 bruts, 10× sur les
sorties réduites) ou investir dans un parseur spécialisé pour dépasser 10× sur
les STEP4 bruts.

## Reproduction

```sh
cargo build --release -p workflow-cli

./target/release/workflow-cli step5 audio \
  --input parity/fixtures/real_244_step3/expected/real_244_audio.json \
  --output /tmp/audio.json

./target/release/workflow-cli step5 video \
  --input parity/fixtures/real_244_step4/expected/real_244.json \
  --output /tmp/real_244_tracking.json \
  --audio parity/fixtures/real_244_step3/expected/real_244_audio.json

../workflow_mediapipe/coral_env/bin/python tools/parity/parity.py diff \
  --expected parity/fixtures/real_244_step5/expected \
  --actual /tmp
```
