# STEP4 Tracking — diagnostics et bascules

## 1. Matrice des backends

| Backend | Config | Modèles | Usage |
| --- | --- | --- | --- |
| `tpu` | `device = "tpu"` | BlazeFace/FaceMesh/EfficientDet Edge TPU, co-compilés si présents | Production |
| `cpu` | `device = "cpu"` | BlazeFace/FaceMesh/detect CPU | Tests et dépannage |

Le backend CPU n'est pas un fallback automatique : il doit être demandé
explicitement.

## 2. Vérifications rapides

```sh
ls -l /dev/apex_0 vendor/libedgetpu.so.1 vendor/libtensorflowlite_c.so
ls -l assets/models/step4/
cargo run -p workflow-cli -- --config config/config.toml config show | \
  jq '.steps.step4'
```

- [ ] `/dev/apex_0` présent.
- [ ] Bibliothèques lisibles.
- [ ] Modèles présents.
- [ ] `device = "tpu"`.
- [ ] `cocompiled = true` si la paire est utilisée.

## 3. Validation des modèles

```sh
cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model assets/models/step4/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu vendor/libedgetpu.so.1 --iterations 10
```

- Le spike vérifie `edgetpu-custom-op`.
- Pour FaceMesh et EfficientDet, adapter le chemin du modèle.

## 4. Exécution de test

```sh
cargo run --release -p workflow-cli -- --config config/config.toml step4 track \
  --input-dir /tmp/opencode/step4-fixture \
  --device tpu \
  --logs-dir /tmp/opencode/logs/step4
```

- Utiliser une copie courte.
- Ne pas écrire dans `runtime/` ni `/mnt/cache`.
- Vérifier que `<stem>.csv` et `<stem>_audio.json` existent.

## 5. Densité JSON

```sh
jq '{total_frames, fps, frames: (.frames|length)}' \
  /tmp/opencode/step4-fixture/<stem>_tracking.json

jq '[.frames[] | .frame_index] | {first: .[0], last: .[-1], count: length}' \
  /tmp/opencode/step4-fixture/<stem>_tracking.json
```

- `total_frames` doit correspondre à `ffprobe`.
- Les `frame_index` doivent être continus de 1 à `total_frames`.

## 6. Objets et parole

```sh
jq '[.frames[] | .tracked_objects[]? | .id] | unique' \
  /tmp/opencode/step4-fixture/<stem>_tracking.json

jq '[.frames[] | select(.is_speaking == true)] | length' \
  /tmp/opencode/step4-fixture/<stem>_tracking.json
```

- Les IDs doivent être `obj_N`, stables et triés par `centroid_x`.
- `speaking_sources.audio` et `.segment` doivent être présents si le CSV et
  l'audio sont fournis.
- Sans CSV/audio : le JSON doit être refusé, sauf
  `--allow-partial-speaking`.

## 7. Actions correctives

| Symptôme | Action |
| --- | --- |
| TPU non détecté | Vérifier le driver gasket, `/dev/apex_0`, ASPM |
| Modèle non compilé | Restaurer le fichier du manifeste ou recompiler |
| CSV/audio manquant | Ajouter les entrées STEP2/STEP3 |
| Objets absents | Vérifier `object_detection`, le CSV, l'hystérésis |
| IDs instables | Vérifier le suivi et l'ordre des frames |
| Blendshapes bruitées | Vérifier One-Euro et le profil |
| Parole incohérente | Vérifier le seuil `jawOpen`, l'audio, les segments |
| Verrou refusé | Attendre STEP2/STEP3 ou arrêter l'autre exécution |
| JSON tronqué | Vérifier l'écriture atomique et le disque |

> Toute modification durable de modèle, de seuil ou de backend doit être
> consignée dans `memory-bank/decisionLog.md` et, si elle affecte la parité,
> dans `parity/DRIFT.md`.
