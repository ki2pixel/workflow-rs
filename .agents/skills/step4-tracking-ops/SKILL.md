---
name: step4-tracking-ops
description: Exploiter et diagnostiquer STEP4 tracking Rust (BlazeFace, FaceMesh, EfficientDet, One-Euro, parole).
---

# STEP4 Tracking Ops

> Ancien nom legacy : `step5-gpu-ops`. La numérotation legacy était décalée ;
> dans workflow-rs, le tracking est **STEP4**.

## Préparation

1. Lire `config/config.toml`, section `[steps.step4]`.
2. Vérifier les modèles :
   - `step4/blazeface_front_quantized_edgetpu.tflite`
   - `step4/facemesh_quantized_edgetpu.tflite`
   - `step4/face_blendshapes.tflite`
   - `step4/detect_edgetpu.tflite`
   - variantes CPU : `blazeface_front_quantized.tflite`,
     `facemesh_quantized.tflite`, `detect.tflite`
   - paire co-compilée sous `step4/cocompiled/`.
3. Vérifier `vendor/libtensorflowlite_c.so`, `vendor/libedgetpu.so.1`,
   `/dev/apex_0`.
4. Vérifier que `<stem>.csv` et `<stem>_audio.json` existent à côté de chaque
   vidéo (D-004).
5. Vérifier le verrou `/tmp/workflow-rs-hw.lock`.

## Exécution

```sh
cargo run --release -p workflow-cli -- --config config/config.toml step4 track \
  --input-dir runtime/projets_extraits \
  --device tpu
```

Options utiles :

- `--device cpu` : modèles CPU, mode de test/dépannage.
- `--no-object-detection` : visages seulement.
- `--object-min-missing`, `--face-reset-frames` : hystérésis.
- `--decode-width`, `--decode-height` : downscale explicite.
- `--allow-partial-speaking` : JSON partiel de test, jamais en production.
- `--tflite`, `--edgetpu`, `--logs-dir`.

## Architecture

- Décodage FFmpeg `rawvideo`, résolution source, `fps=metadata.fps`, `rgb24`.
- Thread producteur borné à 3 trames.
- BlazeFace 128×128, 896 ancres, NMS `score > 0.7`, `IoU <= 0.3`.
- Crop affine 192×192 depuis les yeux, FaceMesh 468 points.
- Blendshapes 136 indices MediaPipe, padding iris, sigmoïde float32, One-Euro
  float64 sur 52 dimensions ARKit.
- EfficientDet 300×300, `INTER_NEAREST`, score minimal 0.3, 5 objets max,
  hystérésis `object_min_missing` / `face_reset_frames`.
- Suivi centroïde le plus proche, seuil 100 px, IDs `obj_N`, oubli après
  10 frames, tri par `centroid_x`.
- Parole audio/visuelle et segments STEP2, champs `is_speaking`,
  `speaking_confidence`, `speaking_method`, `speaking_sources`.
- JSON streamé vers un temporaire puis renommé.

## Checklists

- [ ] `steps.step4.device = "tpu"`.
- [ ] `cocompiled = true` si la paire co-compilée est présente.
- [ ] `object_detection = true` si EfficientDet est attendu.
- [ ] `object_min_missing = 25`, `face_reset_frames = 1`.
- [ ] `cpu_threads = 1`.
- [ ] `jaw_open_threshold = 0.08`.
- [ ] CSV et audio présents.
- [ ] JSON atomique.
- [ ] Aucun `.mov` alpha/m6+ traité.

## Résolution d'incidents

| Symptôme | Action |
| --- | --- |
| `TPU non détecté` | Vérifier `/dev/apex_0`, `libedgetpu.so.1`, `pcie_aspm=off` |
| `signature edgetpu-custom-op absente` | Vérifier le modèle et le manifeste |
| `CSV/audio manquant` | Ajouter les entrées ou utiliser `--allow-partial-speaking` en test |
| `objets manquants` | Vérifier `object_detection`, l'hystérésis, le CSV STEP2 |
| `IDs instables` | Vérifier le suivi centroïde et l'ordre des frames |
| `blendshapes bruitées` | Vérifier One-Euro, `blendshapes_profile` |
| `parole incohérente` | Vérifier `jaw_open_threshold`, l'audio STEP3, les segments CSV |
| `JSON tronqué` | Vérifier l'écriture atomique et l'espace disque |
| `verrou refusé` | Une autre étape matérielle tourne |
| `RSS trop élevé` | Vérifier le décodage, le streaming JSON, les allocations par frame |

## Performance et limites

- 44,2 s sur `real_244`, 41,0 s sur `real_245` (mesures de référence).
- RSS d'environ 199,5 Mio sur `real_244`.
- Parité à `1e-6` sur `face_pan`, `real_244`, `real_245`.
- Le backend CPU sert aux tests, pas de fallback automatique.
- Chaque modèle TPU ouvre son propre délégué (différence volontaire avec
  Python).

## Références

- `.agents/skills/coral-tpu-system-orchestration/SKILL.md`
- `.agents/skills/coral-tpu-hybrid-algorithms/SKILL.md`
- `docs/LOT-3-STEP4.md`
- `docs/CONTRAT-ENVIRONNEMENT.md`
- `parity/DRIFT.md` (D-004, D-005, D-008)
- `resources/tracking_diagnostics.md`
