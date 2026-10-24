# STEP3 Audio — triage TPU / ONNX / JSON

## 1. Vérifications rapides

```sh
ls -l /dev/apex_0 vendor/libedgetpu.so.1 vendor/libtensorflowlite_c.so
cat /proc/cmdline | grep pcie_aspm=off || true
ls -l assets/models/step3/
```

- `/dev/apex_0` présent.
- `libedgetpu.so.1` et `libtensorflowlite_c.so` lisibles.
- Les quatre modèles STEP3 présents.

## 2. Config

```sh
cargo run -p workflow-cli -- --config config/config.toml config show | \
  jq '.steps.step3'
```

- `device = "tpu"`.
- `yamnet_split = true`.
- `require_edgetpu = true`.
- `embedding_stride = 1`.
- `max_workers = 1`.
- `vad_threshold = 0.2`.
- `clustering_threshold = 0.32`.

## 3. Exécution de test

```sh
cargo run --release -p workflow-cli -- --config config/config.toml step3 diarize \
  --input-dir /tmp/opencode/step3-fixture \
  --edgetpu vendor/libedgetpu.so.1
```

- Utiliser une copie courte pour valider la stack.
- Pour un test sans TPU : `--device cpu` (mode explicite, pas production).

## 4. Signature Edge TPU

```sh
strings assets/models/step3/yamnet_split_edgetpu_compiled.tflite | \
  grep -c edgetpu-custom-op
```

- Attendu : au moins 1.
- Si 0 : le modèle n'est pas compilé pour l'Edge TPU ; vérifier le manifeste.

## 5. ONNX Runtime / ECAPA

```sh
ls -l /path/to/libonnxruntime.so*
```

- `steps.step3.onnxruntime` doit pointer vers une bibliothèque compatible.
- Vérifier que le modèle ECAPA est lisible et que `ecapa_batch_size` est
  cohérent avec la RAM.

## 6. Analyse des logs

```sh
ls -lt runtime/logs/step3/ | head
rg -n 'SKIP_NO_AUDIO|edgetpu|ECAPA|AHC|speaker|erreur' runtime/logs/step3/
```

- `SKIP_NO_AUDIO` : comportement attendu (D-002).
- `edgetpu-custom-op` : signature détectée.
- `délégué` : état du délégué matériel.
- `AHC` : clustering et seuil.

## 7. Actions correctives

| Symptôme | Action |
| --- | --- |
| TPU non détecté | Vérifier `/dev/apex_0`, le driver gasket, `pcie_aspm=off` |
| Signature absente | Recompiler le modèle ou restaurer le fichier du manifeste |
| ONNX Runtime absent | Renseigner le chemin ou installer la bibliothèque |
| ECAPA OOM | Réduire `ecapa_batch_size` ; vérifier le RSS |
| JSON différent | Vérifier le modèle, la mel matrix et la version de l'outil |
| Verrou refusé | Attendre la fin de STEP2/STEP4 ou arrêter l'autre exécution |
| Vidéo sans audio | Attendu : aucune sortie, code 0 |
| Segments incohérents | Vérifier la normalisation 25 fps de STEP1 |

> Documenter tout ajustement durable dans `memory-bank/decisionLog.md`.
