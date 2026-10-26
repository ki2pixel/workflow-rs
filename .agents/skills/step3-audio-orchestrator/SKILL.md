---
name: step3-audio-orchestrator
description: Exploiter et diagnostiquer STEP3 audio Rust (YAMNet Split Edge TPU, ECAPA-TDNN ONNX, AHC, cas sans audio).
---

# STEP3 Audio Orchestrator

> Ancien nom legacy : `step4-audio-orchestrator`. La numérotation legacy était
> décalée ; dans workflow-rs, l'audio est **STEP3**.

## Préparation

1. Lire `config/config.toml`, section `[steps.step3]`.
2. Vérifier les modèles :
   - `step3/yamnet_split_edgetpu_compiled.tflite`
   - `step3/yamnet_split_quantized.tflite`
   - `step3/yamnet_mel_matrix.npy`
   - `step3/ecapa_tdnn_speaker_float.onnx`
3. Vérifier `vendor/libtensorflowlite_c.so`, `vendor/libedgetpu.so.1` et
   `/dev/apex_0`.
4. Vérifier ONNX Runtime (`steps.step3.onnxruntime`) et `ffmpeg`/`ffprobe`.
5. Vérifier que le verrou `/tmp/workflow-rs-hw.lock` n'est pas tenu par une
   autre étape.

## Exécution

```sh
cargo run --release -p workflow-cli -- --config config/config.toml step3 diarize \
  --input-dir runtime/projets_extraits \
  --edgetpu vendor/libedgetpu.so.1
```

Options utiles :

- `--device cpu` : mode de test explicite, jamais le chemin de production.
- `--yamnet`, `--yamnet-fallback`, `--ecapa`, `--mel-matrix` : surcharges.
- `--tflite`, `--edgetpu`, `--onnxruntime` : surcharges de bibliothèques.
- `--logs-dir` : répertoire de logs.
- `--force` : réécrit un `_audio.json` existant.

## Pipeline

1. Découverte récursive des vidéos, skip des `_audio.json` existants.
2. Extraction FFmpeg PCM16 16 kHz mono.
3. YAMNet Split : fenêtres 15 360 échantillons (0,96 s), hop 7 680
   (0,48 s), frontend log-mel CPU (400/160, FFT 512, 64 mel), quantification
   INT8, médiane 5, seuil strict 0,20, hangover 1,0 s.
4. ECAPA-TDNN : segments de 3 s, log-mel 80×300, CMN, batch 32, embeddings
   192D.
5. AHC moyenne, distance cosinus, seuil 0,32, réassignation des clusters de
   moins de 7 s.
6. Fusion des segments, frames 1 à `total_frames`, `speaker_stats` brut STEP3,
   écriture atomique.

## Checklists

- [ ] `steps.step3.device = "tpu"`.
- [ ] `steps.step3.yamnet_split = true`.
- [ ] `steps.step3.require_edgetpu = true` en production.
- [ ] `steps.step3.embedding_stride = 1` par défaut (zéro régression).
- [ ] `steps.step3.adaptive_stride = false` par défaut.
- [ ] Traitement séquentiel : aucune parallélisation des vidéos ni des
      sessions YAMNet (pas de clé `max_workers` dans le TOML).
- [ ] `/dev/apex_0` accessible.
- [ ] Le modèle contient `edgetpu-custom-op`.
- [ ] Le JSON est écrit atomiquement.
- [ ] Le cas sans audio produit `SKIP_NO_AUDIO`, aucune sortie, code 0.

## Résolution d'incidents

| Symptôme | Action |
| --- | --- |
| `YAMNet sans signature edgetpu-custom-op` | Recompiler le modèle ou vérifier le manifeste |
| `délégué Edge TPU absent` | Vérifier `libedgetpu.so.1`, `/dev/apex_0`, `pcie_aspm=off` |
| `ONNX Runtime introuvable` | Renseigner `steps.step3.onnxruntime` ou installer la bibliothèque |
| `ECAPA lent` | Vérifier `ecapa_batch_size`, `cpu_threads`, l'absence de surcharge CPU |
| `ModuleNotFoundError` | Impossible : le chemin Rust n'appelle pas Python ; vérifier la config |
| `SKIP_NO_AUDIO` | Comportement attendu (D-002), pas une erreur |
| `JSON différent` | Comparer avec la fixture et vérifier le modèle/mel matrix |
| `OOM` | Réduire `ecapa_batch_size`, `cpu_threads` ; vérifier le RSS |
| `verrou refusé` | Une autre étape matérielle tourne ; attendre ou arrêter |
| `date de frames incohérente` | Vérifier la normalisation STEP1 (25 fps) |

## Performance et limites

- ECAPA sur CPU domine le temps (environ 30 s par vidéo sur `real_244`).
- Le JSON brut est identique sur `vox_2spk`, `real_244`, `real_245`.
- Le chemin CPU YAMNet est un mode de test, pas un fallback.
- Le traitement des vidéos est séquentiel.

## Références

- `.agents/skills/coral-tpu-system-orchestration/SKILL.md`
- `.agents/skills/coral-tpu-hybrid-algorithms/SKILL.md`
- `docs/pipeline/LOT-4-STEP3.md`
- `docs/contracts/CONTRAT-ENVIRONNEMENT.md`
- `parity/DRIFT.md` (D-002, D-007)
- `resources/audio_triage.md`
