# workflow-rs

Réécriture Rust du pipeline `workflow_mediapipe`.

Le dépôt Python reste la référence de production jusqu'au cutover. Le lot 0 a
validé le pont matériel Coral, le contrat d'environnement et le harnais de
parité. Le lot 1 est en cours : le socle Rust, la configuration TOML et la
réduction audio de STEP5 sont opérationnels ; la réduction vidéo de STEP5,
STEP6 et le pont Windows restent à porter.

## État

- Lot 0.1 validé : `libtensorflowlite_c.so` compilé, BlazeFace et FaceMesh identiques au bit près aux références Python, 10 000 invocations stables.
- Lot 0.2 terminé : contrat d'environnement et table de correspondance des clés.
- Lot 0.3 terminé : 23 cas de parité, 14 étapes réelles sur quatre projets (244, 245, 249, 250) identiques à la production, 9 cas synthétiques déterministes.
- Lot 1.1 terminé : workspace `workflow-core` / `workflow-schema` /
  `workflow-steps` / `workflow-cli`, configuration TOML typée, CLI de
  validation, réducteur audio STEP5 avec en-tête avant le tableau.
- Le réducteur audio est identique à la production sur les quatre projets
  (244, 245, 249, 250), y compris lorsque `fps` et `total_frames` sont placés
  après `frames_analysis`.
- Modèles de production copiés dans `assets/models/` avec manifeste SHA-256 ;
  les binaires restent locaux, seuls le manifeste et le README sont versionnés.
- `crates/workflow-tpu` : chargement dynamique de `libtensorflowlite_c.so` et de
  `libedgetpu.so.1`, session transactionnelle, binaire de spike.

## Documents

- `docs/CONTRAT-ENVIRONNEMENT.md` : routes, états, schémas, fichiers, intégrations.
- `config/config.toml` : configuration typée ; secrets par variables d'environnement.
- `config/env.map.toml` : correspondance entre l'ancien `.env` et le futur `config.toml`.
- `assets/models/manifest.toml` : rôles, tailles et SHA-256 des modèles copiés.
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
```

Sans `--output`, le fichier d'entrée est réécrit de façon atomique, comme dans
la production. L'en-tête `fps` / `total_frames` est écrit avant
`frames_analysis` ; une seconde réduction conserve donc les métadonnées.

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
