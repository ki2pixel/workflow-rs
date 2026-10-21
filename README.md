# workflow-rs

Réécriture Rust du pipeline `workflow_mediapipe`.

Le dépôt Python reste la référence de production jusqu'au cutover. Ce workspace
contient le lot 0 : validation du pont matériel Coral, contrat d'environnement
et harnais de parité. Le lot 1 (socle, schéma, STEP5-8, pont Windows) est la
prochaine étape.

## État

- Lot 0.1 validé : `libtensorflowlite_c.so` compilé, BlazeFace et FaceMesh identiques au bit près aux références Python, 10 000 invocations stables.
- Lot 0.2 terminé : contrat d'environnement et table de correspondance des clés.
- Lot 0.3 terminé : 23 cas de parité, 14 étapes réelles sur quatre projets (244, 245, 249, 250) identiques à la production, 9 cas synthétiques déterministes.
- `crates/workflow-tpu` : chargement dynamique de `libtensorflowlite_c.so` et de
  `libedgetpu.so.1`, session transactionnelle, binaire de spike.

## Documents

- `docs/CONTRAT-ENVIRONNEMENT.md` : routes, états, schémas, fichiers, intégrations.
- `config/env.map.toml` : correspondance entre l'ancien `.env` et le futur `config.toml`.
- `parity/README.md` : corpus, commandes et sécurité du harnais.
- `parity/BASELINE.md` : mesures de référence.
- `parity/DRIFT.md` : écarts de comportement à ne pas reproduire.

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
- `libtensorflowlite_c.so`, produit par `scripts/build-tflite-c.sh`.
- `libedgetpu.so.1` (paquet Coral) et un `/dev/apex_0` accessible.

## Spike

```sh
cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model ../workflow_mediapipe/assets/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu ../workflow_mediapipe/tmp_lib/usr/lib/x86_64-linux-gnu/libedgetpu.so.1 \
  --iterations 1000
```

Le binaire vérifie la signature `edgetpu-custom-op`, exécute l'inférence et
affiche les dimensions, la quantization, la latence et une empreinte des sorties.
