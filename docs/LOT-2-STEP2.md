# Lot 2 : STEP2 TransNetV2

STEP2 est porté dans `workflow-steps::step2`. La production Python reste
l'oracle ; le binaire Rust écrit le même CSV, à côté de chaque vidéo, sans
modifier le reste du pipeline.

## Architecture

- Décodage FFmpeg en `rawvideo` via `tokio::process` :
  `fps=25,scale=48:27:flags=bilinear`, `rgb24`, `-threads 0`.
- Fenêtres de 100 trames, pas de 50, padding global de 25 trames. Le flux est
  consommé par `WindowStream`, qui ne garde qu'environ 150 trames en mémoire et
  applique le padding gauche/droit au fil de l'eau.
- Inférence `ort` 2.0 avec `CUDAExecutionProvider` (cuDNN exhaustif, TF32
  désactivé), lots de 8 fenêtres. Le tenseur est `uint8 [8,100,27,48,3]` et la
  sortie est lue dans `transition_probabilities [8,100,1]`.
- Une seule sigmoïde est appliquée aux logits de la tranche centrale
  `[25,75)`, comme dans `zero_copy_pipeline.py`.
- `predictions_to_scenes` conserve la sémantique Python : segments inclusifs,
  coupure si `probabilité > 0.5`, CSV en `\r\n` avec
  `No,Timecode In,Timecode Out,Frame In,Frame Out`.
- Le backend CPU existe dans le même chemin de code (`device = "cpu"`) pour les
  tests sans GPU. Ce n'est pas un fallback silencieux : le mode CUDA échoue si
  le provider CUDA ne peut pas être enregistré.

## Configuration

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

`model` est relatif à `paths.models`. `onnxruntime` peut être vide pour laisser
le loader système chercher la bibliothèque. Avec `auto_cuda_libs = true`, les
répertoires `nvidia/*/lib` voisins d'un `site-packages` Python sont ajoutés et
préchargés en `RTLD_GLOBAL` avant ONNX Runtime ; le loader glibc fige
`LD_LIBRARY_PATH` au démarrage et ne suffit donc pas seul.

## Commande

```sh
cargo run --release -p workflow-cli -- step2 detect \
  --input-dir /chemin/vers/docs \
  --force
```

La commande prend le verrou matériel (`hardware.lock_path`) avant de charger le
modèle. Elle parcourt récursivement le dossier, ignore les CSV existants sauf
avec `--force`, et ignore les `.mov` préservés (nom `m6+.mov` ou couche alpha
détectée par `ffprobe`). Le total affiché ne compte que les vidéos réellement
traitées, comme le script Python.

Options utiles : `--device cpu|cuda`, `--model`, `--onnxruntime`,
`--cuda-lib-dir`, `--no-auto-cuda-libs`, `--batch-size`, `--window`, `--stride`,
`--padding`, `--threshold`, `--ffmpeg`, `--ffmpeg-threads`.

## Parité

Les quatre cas STEP2 du corpus sont identiques octet pour octet au CSV de
production :

| Cas | Scènes | Trames | CSV |
| --- | ---: | ---: | --- |
| `cuts_step2` | 4 | 500 | identique |
| `short_2997_step2` | 1 | 75 | identique |
| `real_244_step2` | 78 | 4 920 | identique |
| `real_245_step2` | 54 | 4 728 | identique |

Le backend CPU a aussi produit le CSV exact de `short_2997_step2` et de
`real_244_step2` ; il sert uniquement de chemin de test.

## Performance et VRAM

Mesures release sur la machine de production, processus complet (chargement du
modèle compris), comparées à `parity/BASELINE.md` :

| Cas | Rust | Baseline Python | RSS Rust | VRAM étape Rust | VRAM étape baseline |
| --- | ---: | ---: | ---: | ---: | ---: |
| `short_2997_step2` | 2,32 s | 3,27 s | ~879 Mio | ~2 132 Mio | 2 224 Mio |
| `cuts_step2` | 2,96 s | 3,83 s | ~883 Mio | ~2 131 Mio | 2 226 Mio |
| `real_244_step2` | 10,92 s | 12,15 s | ~881 Mio | ~2 203 Mio | 2 224 Mio |
| `real_245_step2` | 10,66 s | 12,11 s | ~882 Mio | ~2 127 Mio | 2 224 Mio |

La VRAM est mesurée avec `nvidia-smi` (total GPU moins le bureau, 524–527 Mio),
pas avec les compteurs internes d'ONNX Runtime. La mesure est un pic
échantillonné à 100 ms ; elle peut varier de quelques dizaines de Mio. Le débit est supérieur à la
baseline sur les quatre cas et la VRAM pic reste sous la baseline.

## Limites connues

- La validation CUDA est faite sous Linux uniquement. Le chemin Windows
  (`PATH`, provider CUDA) n'est pas encore exercé.
- Le préchargement CUDA est spécifique au loader Linux. Sur une installation
  système classique, renseigner `cuda_lib_dirs` ou laisser le système résoudre
  les bibliothèques.
- Le fallback MediaInfo de détection alpha n'est pas porté ; si `ffprobe` ne
  donne pas de `pix_fmt`, le `.mov` est préservé par précaution (comme Python
  en cas d'indétermination).
- Le contrôle non bloquant de framerate (`check_fps_conformance`) n'émet pas
  encore le warning Python. Il ne change pas le CSV : FFmpeg force 25 fps.
- Le port Rust écrit la progression sur stdout/stderr, mais ne crée pas encore
  le fichier `logs/step2/transnet_*.log` ni la ligne `CONFIG_EFFECTIVE` du
  script Python. Le branchement à l'orchestrateur les ajoutera.
- STEP2 n'est pas encore branché à l'orchestrateur Flask ; la double exécution
  shadow et le cutover restent au lot 5.
