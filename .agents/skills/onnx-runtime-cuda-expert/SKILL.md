---
name: onnx-runtime-cuda-expert
description: Moteur STEP2 TransNetV2 Rust via ONNX Runtime CUDA (ort, FFmpeg rawvideo, fenêtres, CSV, VRAM).
---

# ONNX Runtime CUDA Expert — STEP2

> Remplace la skill legacy `opencv5-dnn-engine-expert`. OpenCV 5.0 DNN n'est
> pas utilisé dans workflow-rs ; STEP2 utilise `ort` avec CUDA.

## Rôle

Porter et exploiter le moteur TransNetV2 de STEP2 :

- décodage FFmpeg `rawvideo` via `tokio::process` ;
- fenêtres de 100 frames, pas de 50, padding 25 ;
- inférence `ort` 2.0 avec `CUDAExecutionProvider` ;
- écriture du CSV de scènes au format Python ;
- mode CPU explicite pour les tests.

## Stack

| Élément | Valeur |
| --- | --- |
| Runtime | `ort = 2.0.0-rc.10` (épinglé) |
| Provider | `CUDAExecutionProvider` |
| cuDNN | `CuDNNConvAlgorithmSearch::Exhaustive` |
| TF32 | désactivé (`use_tf32 = false`) |
| Modèle | `step2/transnetv2.onnx` |
| Entrée | `uint8 [8, 100, 27, 48, 3]` |
| Sortie | `transition_probabilities [8, 100, 1]` |
| CSV | `\r\n`, `No,Timecode In,Timecode Out,Frame In,Frame Out` |

## Architecture

1. FFmpeg produit `fps=25,scale=48:27:flags=bilinear,rgb24`.
2. `WindowStream` conserve environ 150 frames et applique le padding
   gauche/droit.
3. Les fenêtres sont regroupées par lots de 8.
4. Une seule sigmoïde est appliquée aux logits de la tranche centrale
   `[25, 75)`.
5. `predictions_to_scenes` conserve la sémantique Python : segments inclusifs,
   coupure si probabilité > 0,5.
6. Le CSV est écrit à côté de la vidéo.

## Config

```toml
[steps.step2]
cudnn_algo_search = true
batch_size = 8
window = 100
stride = 50
padding = 25
threshold = 0.5
device = "cuda"
model = "step2/transnetv2.onnx"
onnxruntime = "/mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2"
cuda_lib_dirs = []
auto_cuda_libs = true
ffmpeg = "ffmpeg"
ffprobe = "ffprobe"
ffmpeg_threads = 0
use_tf32 = false
```

- `model` est relatif à `paths.models`.
- `onnxruntime` vide laisse le loader système chercher la bibliothèque.
- `auto_cuda_libs = true` précharge les répertoires `nvidia/*/lib` voisins.
- Le préchargement est nécessaire car `LD_LIBRARY_PATH` est figé au démarrage.

## Commandes

```sh
cargo run --release -p workflow-cli -- --config config/config.toml step2 detect \
  --input-dir runtime/projets_extraits --force
```

Options : `--device cpu|cuda`, `--model`, `--onnxruntime`, `--cuda-lib-dir`,
`--no-auto-cuda-libs`, `--batch-size`, `--window`, `--stride`, `--padding`,
`--threshold`, `--ffmpeg`, `--ffmpeg-threads`.

## Diagnostics

| Symptôme | Piste |
| --- | --- |
| `CUDA provider absent` | Vérifier ONNX Runtime CUDA et `cuda_lib_dirs` |
| `libonnxruntime` introuvable | Renseigner `steps.step2.onnxruntime` |
| `cuDNN` erreur | Vérifier la version CUDA/cuDNN et `cudnn_algo_search` |
| VRAM insuffisante | Réduire `batch_size` ; vérifier la VRAM des autres processus |
| CSV différent | Vérifier le modèle, le seuil, le padding et le format `\r\n` |
| CPU utilisé sans demande | Le provider CUDA n'a pas été enregistré ; échouer explicitement |
| Lent | Vérifier le préchargement CUDA, le batch et le décodage FFmpeg |

## Parité

- CSV identiques sur `cuts`, `short_2997`, `real_244`, `real_245`.
- Le backend CPU produit les mêmes CSV sur `short_2997` et `real_244`, mais
  reste un mode de test.
- Mesures : 2,3–10,9 s selon le cas ; VRAM pic 2,1–2,2 GiB.

## Règles

- Ne pas remplacer `ort` par OpenCV DNN ou PyTorch.
- Ne pas activer TF32 pour « accélérer » sans validation de parité.
- Ne pas ajouter de fallback CPU silencieux.
- Ne pas modifier le format CSV.
- Ne pas lancer STEP2 en parallèle de STEP3/STEP4 matérielles.

## Références

- `docs/LOT-2-STEP2.md`
- `docs/STEP2-WINDOWS.md`
- `crates/workflow-steps/src/step2/`
- `parity/DRIFT.md`
- `.agents/skills/rust-concurrency-orchestrator/SKILL.md`
