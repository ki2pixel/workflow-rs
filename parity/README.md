# Corpus et harnais de parité

Le harnais mesure l'application Python actuelle, fige ses sorties et compare la
future application Rust. Il n'est pas livré en production.

## Disposition

| Chemin | Rôle |
| --- | --- |
| `manifest.toml` | Cas, commandes, entrées, sorties attendues |
| `cases/` | Médias synthétiques générés, ignorés par git |
| `work/` | Copies de travail isolées, ignorées par git |
| `fixtures/<cas>/expected/` | Sorties de référence, ignorées par git |
| `fixtures/<cas>/actual/` | Dernières sorties mesurées |
| `fixtures/checksums.json` | SHA-256 des références, versionné |
| `baseline/<cas>/` | Résultats, consoles et diffs, ignorés par git |
| `BASELINE.md` | Synthèse lisible des dernières mesures |
| `DRIFT.md` | Écarts de comportement et décisions pour le portage |

Les fixtures sont ignorées par git car elles dérivent de productions réelles.
Le fichier `checksums.json` conserve l'empreinte des références et la source
exacte qui a permis de les produire.

## Commandes

```bash
cd /home/kidpixel/workflow-rs

# 1. Construire les médias synthétiques (idempotent)
coral_env/bin/python tools/parity/parity.py build-corpus

# 2. Vérifier le manifeste et les entrées
coral_env/bin/python tools/parity/parity.py verify --probe

# 3. Mesurer une étape
coral_env/bin/python tools/parity/parity.py run --case real_244_step6

# 4. Figer la première mesure d'un cas synthétique
coral_env/bin/python tools/parity/parity.py run --case cuts_step2 --snapshot

# 5. Comparer deux dossiers sans le harnais
coral_env/bin/python tools/parity/parity.py diff \
  --expected parity/fixtures/real_244_step6/expected \
  --actual parity/fixtures/real_244_step6/actual

# 6. Agréger
coral_env/bin/python tools/parity/parity.py report
```

`coral_env/bin/python` est utilisé pour `tomllib` (Python 3.11) et `psutil`.
Les étapes elles-mêmes tournent dans leurs venvs d'origine.

## Sécurité

- Avant STEP1 à STEP4, le harnais interroge `/sequence/status` sur le port 5003
  et refuse de démarrer si une séquence de production tourne.
- Il vérifie aussi `pgrep workflow_scripts/step*` pour détecter un processus
  orphelin.
- Il prend un verrou `flock` sur `$XDG_RUNTIME_DIR/workflow-hw.lock` pour les
  étapes matérielles. Le verrou est libéré à la fin, y compris sur erreur.
- `--allow-production` contourne le contrôle de production, pas le verrou.
- Les entrées réelles sont lues en lecture seule ; seules les copies de travail
  sont modifiées.

## Cas `real_244`

Le projet `/mnt/cache/244 Cvmille/docs` contient la chaîne complète d'une
production finalisée :

- `<stem>.mp4` : vidéo normalisée, 1920×1080, 25 fps, 4 920 frames ;
- `<stem>.csv` : sortie STEP2, 78 scènes ;
- `<stem>_audio.json` : sortie STEP3 ;
- `<stem>.json` : sortie STEP4, 8,5 Mo ;
- `<stem>_tracking.json` : sortie STEP5 ;
- `<stem>_ae.json` : sortie STEP6, 2,6 Mo.

Chaque cas copie les entrées dans `work/<cas>/<projet>/docs/`, avec un stem
simplifié `real_244`. La vidéo est liée par symlink. La seule clé ignorée est
`video_path` dans les métadonnées STEP4, qui contient le chemin absolu de la
production.

## Médias synthétiques

| Fichier | Ce qu'il vérifie |
| --- | --- |
| `short_2997.mp4` | 29,97 fps, 480×270, sans audio : normalisation STEP1 et STEP2 court |
| `synthetic_cuts.mp4` | 4 plans francs, 20 s, audio : frontières STEP2 |
| `synthetic_vfr.mp4` | timestamps irréguliers : normalisation VFR |
| `synthetic_interlaced.mp4` | 1080i : désentrelacement STEP1 |
| `synthetic_silent.mp4` | sans audio : comportement STEP3 |
| `synthetic_alpha.mov` | ProRes 4444 alpha : préservation du média |
| `face_pan.mp4` | deux visages, zoom lent : cascade STEP4 |
| `vox_2spk.mp4` | parole réelle VoxConverse, 30 s, ≥ 2 locuteurs : STEP3 |

`face_pan.mp4` est dérivé d'une frame de production. `cases/` ne doit pas être
publié ni versionné.

## Comparaison

- JSON : récursive, tolérance `1e-6` par défaut, timecodes à `1 ms`, clés
  ignorées par nom.
- CSV : en-têtes, lignes, cellules numériques et timecodes.
- Média : codec, résolution, pix_fmt, field_order, fps, durée, audio.
- Binaire : SHA-256.

Les différences sont comptées en totalité, mais seules les 100 premières sont
détaillées. `diff.txt` et `diff.json` sont écrits à côté du résultat.

## Résultats

Les 23 cas passent. Sur le projet réel 244 (1920×1080, 25 fps, 4 920 frames) :

| Étape | Durée | RSS max | VRAM étape | Diff |
| --- | ---: | ---: | ---: | --- |
| STEP2 | 12,2 s | 1 040 MiB | 2 224 MiB | 0 |
| STEP3 | 18,0 s | 965 MiB | 0 (Coral/CPU) | 0 après projection STEP5 |
| STEP4 | 40,5 s | 594 MiB | 0 (Coral/CPU) | 0 |
| STEP5 | 0,57 s | 18 MiB | 0 | 0 |
| STEP6 | 0,47 s | 29 MiB | 0 | 0 |

Le détail et les mesures des cas synthétiques sont dans `BASELINE.md`.
Les projets 245, 249 et 250 complètent le corpus : 245 est identique sur STEP2,
STEP3, STEP4, STEP5 et STEP6 ; 249 et 250 sont identiques sur STEP5 et STEP6.
Deux cas appliquent une transformation explicite :

- `real_244_step3` applique `step5_reduce_audio` après STEP3, car l'attendu de
  production est déjà réduit par STEP5 ;
- `real_244_step5` applique `step5_raw_audio` avant STEP5, car la copie de
  production a perdu l'ordre d'en-tête attendu par le parseur streaming ;
- `silent_step3` déclare `allow_empty_outputs = true`, car STEP3 sort en 0 sans
  produire d'artefact quand la vidéo n'a pas d'audio.

## Auto-test

Le comparateur est vérifié par mutation volontaire :

```bash
coral_env/bin/python tools/parity/selftest.py
```

Le script contrôle un dossier intact, puis injecte une différence JSON, CSV,
média et binaire. Il attend `rc=0` pour le contrôle et `rc=3` pour chaque
mutation.

## Limites connues

- Le comparateur JSON ne sait pas encore apparier des objets par identifiant
  quand les IDs de tracking changent. Le cas STEP4 permettra de décider si un
  comparateur dédié est nécessaire.
- Les embeddings STEP3 ne sont pas comparés avec une métrique cosinus ; le diff
  générique signale les écarts valeur par valeur.
- STEP7 et STEP8 ne sont pas encore instrumentés.
