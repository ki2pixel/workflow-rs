# workflow-rs

Réécriture Rust du pipeline `workflow_mediapipe`.

Le dépôt Python reste la référence de production jusqu'au cutover. Le lot 0 a
validé le pont matériel Coral, le contrat d'environnement et le harnais de
parité. Le lot 1 est en cours : le socle Rust, la configuration TOML, STEP5
audio/vidéo, STEP6 (`_ae.json`), le pont After Effects (`cuts` et
`analyze_manifest`), STEP7 (finalisation) et STEP8 (EffectCraft via
`effectcraft-cli`) sont opérationnels. La compilation Windows du pont, le
basculement du JSX et la validation STEP8 sur la machine Windows restent à
faire.

## État

- Lot 0.1 validé : `libtensorflowlite_c.so` compilé, BlazeFace et FaceMesh identiques au bit près aux références Python, 10 000 invocations stables.
- Lot 0.2 terminé : contrat d'environnement et table de correspondance des clés.
- Lot 0.3 terminé : 23 cas de parité, 14 étapes réelles sur quatre projets (244, 245, 249, 250) identiques à la production, 9 cas synthétiques déterministes.
- Lot 1.1 terminé : workspace `workflow-core` / `workflow-schema` /
  `workflow-steps` / `workflow-cli`, configuration TOML typée, CLI de
  validation, réducteur audio STEP5 avec en-tête avant le tableau.
- Lot 1.2 terminé : réducteur vidéo STEP5 en streaming, analytics et alignement
  temporel, chemin rapide typé pour les STEP4 bruts.
- STEP5 audio et vidéo sont identiques à la production sur les quatre projets
  (244, 245, 249, 250). STEP4 brut pour 244/245 ; idempotence sur 249/250.
- Performance release : 0,096 s (5,9×) sur 244, 0,101 s sur 245, 0,072 s sur
  249, 0,039 s sur 250, RSS 16 Mio. Détail et décision ouverte dans
  `docs/LOT-1-STEP5.md`.
- Lot 1.3 terminé : STEP6 `_ae.json` avec index tracking/audio, enrichissement
  `is_speaking`/`tier`, parse CSV et recommandations de segments. Identique à la
  production sur les quatre projets ; 0,108 s et 44 Mio sur 244.
- Shadow STEP5/6 : 12/12 vérifications identiques sur les quatre projets de
  production disponibles (244, 245, 249, 250), sans écriture dans `/mnt/cache`.
  La cible de dix projets reste ouverte, faute de projets complets sur la
  machine à cette date.
- Lot 1.4 terminé : `workflow-bridge` pour les cuts CSV et `analyze_manifest`.
  Parité sur 273 calques, les quatre entrées `_ae.json` et les overrides de
  configuration ; cuts 21× plus rapide que Python.
- Lot 1.5 terminé : STEP7 archive par SHA-256, copie vers `/mnt/cache`,
  normalisation `docs/`, manifeste lu par STEP8. Test réel sur une copie du
  projet 250 : `docs/` identique, empreinte vidéo identique.
- Lot 1.6 terminé : STEP8 pilote `MediaSolution.jsx` en headless avec
  `effectcraft-cli`, sans pont Python ni application. Le projet 250 a produit
  un `.ecproj` ouvrable (82 segments, 1080×1920, 25 fps, 133 s) et la
  progression `MS_BATCH_*` est relayée en direct.
- Modèles de production copiés dans `assets/models/` avec manifeste SHA-256 ;
  les binaires restent locaux, seuls le manifeste et le README sont versionnés.
- `crates/workflow-tpu` : chargement dynamique de `libtensorflowlite_c.so` et de
  `libedgetpu.so.1`, session transactionnelle, binaire de spike.

## Documents

- `docs/CONTRAT-ENVIRONNEMENT.md` : routes, états, schémas, fichiers, intégrations.
- `config/config.toml` : configuration typée ; secrets par variables d'environnement.
- `config/env.map.toml` : correspondance entre l'ancien `.env` et le futur `config.toml`.
- `assets/models/manifest.toml` : rôles, tailles et SHA-256 des modèles copiés.
- `docs/LOT-1-STEP5.md` : parité, performance et limites du port STEP5.
- `docs/LOT-1-STEP6.md` : parité, performance et limites du port STEP6.
- `docs/LOT-1-SHADOW.md` : validation shadow STEP5/STEP6 et limite des projets disponibles.
- `docs/LOT-1-BRIDGE.md` : parité et limites du pont After Effects.
- `docs/LOT-1-STEP7.md` : archivage, publication et manifeste STEP7.
- `docs/LOT-1-STEP8.md` : pilotage headless EffectCraft et gate scripting.
- `parity/README.md` : corpus, commandes et sécurité du harnais.
- `parity/BASELINE.md` : mesures de référence.
- `parity/DRIFT.md` : écarts de comportement à ne pas reproduire.

## CLI

```sh
cargo run -p workflow-cli -- config check
cargo run -p workflow-cli -- config show
cargo run -p workflow-cli -- step5 audio \
  --input path/to/audio.json \
  --output path/to/audio_reduced.json

cargo run -p workflow-cli -- step5 video \
  --input path/to/real_244.json \
  --output path/to/real_244_tracking.json \
  --audio path/to/real_244_audio.json

cargo run -p workflow-cli -- step6 build \
  --tracking path/to/real_244_tracking.json \
  --audio path/to/real_244_audio.json \
  --csv path/to/real_244.csv \
  --output path/to/real_244_ae.json

cargo run -p workflow-cli -- step7 finalize --dry-run
cargo run -p workflow-cli -- step7 finalize --no-delete \
  --work-dir /tmp/work --output-dir /tmp/out \
  --archives-dir /tmp/archives --logs-dir /tmp/logs

cargo run -p workflow-cli -- step8 run --dry-run
cargo run -p workflow-cli -- step8 run --folder /tmp/projet
```

Sans `--output` audio, le fichier d'entrée est réécrit de façon atomique, comme
dans la production. Sans `--output` vidéo, la CLI écrit
`<stem>_tracking.json` (ou réécrit un fichier déjà nommé `_tracking.json`).
L'en-tête `fps` / `total_frames` est écrit avant `frames_analysis` ; une seconde
réduction conserve donc les métadonnées.

## Pont After Effects

```sh
cargo build --release -p workflow-bridge
./target/release/workflow-bridge \
  --manifest_path /tmp/ms_manifest.json \
  --output_path /tmp/ms_results.json
```

Le binaire traite `mode = "cuts"` (CSV de scènes) et les manifestes de tracking
(`layers`). Le format de sortie direct ou `{"error": ...}` est conservé ; le JSX
de production n'est pas modifié dans ce lot.

## Parité

```sh
coral_env/bin/python tools/parity/parity.py verify --probe
coral_env/bin/python tools/parity/parity.py run --case real_244_step6
coral_env/bin/python tools/parity/parity.py run-all --tag synthetic
coral_env/bin/python tools/parity/parity.py report
```

Les étapes réelles STEP2 à STEP6 des projets `244` et `245` sont identiques à la
production, ainsi que STEP5 et STEP6 de `249` et `250`. STEP2 mesure 12,2 s et
2,2 GiB de VRAM ; STEP4 mesure 40,5 s sur 4 920 frames. STEP5 et STEP6 prennent
moins d'une seconde.

## Prérequis

- Rust 1.86 ou plus récent.
- `python3 scripts/import-models.py` pour copier les modèles de production.
- `libtensorflowlite_c.so`, produit par `scripts/build-tflite-c.sh`.
- `libedgetpu.so.1` (paquet Coral) et un `/dev/apex_0` accessible.

## Spike

```sh
cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model assets/models/step4/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu ../workflow_mediapipe/tmp_lib/usr/lib/x86_64-linux-gnu/libedgetpu.so.1 \
  --iterations 1000
```

Le binaire vérifie la signature `edgetpu-custom-op`, exécute l'inférence et
affiche les dimensions, la quantization, la latence et une empreinte des sorties.
