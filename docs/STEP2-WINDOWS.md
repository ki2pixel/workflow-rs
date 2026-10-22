# Validation STEP2 sous Windows

État au 2026-10-22 : **compilation croisée validée, exécution non validée**.
`cargo check --target x86_64-pc-windows-msvc -p workflow-cli` passe depuis
Linux ; aucune session Windows avec GPU NVIDIA n'a encore été exécutée. Ce
document sert de protocole de recette, pas de preuve de fonctionnement.

## Prérequis

- Windows 10/11 x64, pilote NVIDIA récent.
- `ffmpeg.exe` et `ffprobe.exe` accessibles dans le `PATH`.
- ONNX Runtime 1.23 x64 avec provider CUDA, ou une version compatible avec le
  modèle `transnetv2.onnx`.
- CUDA/cuDNN correspondant à ONNX Runtime dans le `PATH` ou dans
  `steps.step2.cuda_lib_dirs`.
- Le modèle `assets/models/step2/transnetv2.onnx` et son entrée dans le
  manifeste SHA-256.

## Configuration

Utiliser des chemins avec des slashs (`C:/...`) ou des antislashs échappés
(`C:\\...`) dans `config/config.toml`. Pour un runtime CUDA installé hors du
`PATH` :

```toml
[steps.step2]
device = "cuda"
onnxruntime = "C:/onnxruntime/onnxruntime.dll"
auto_cuda_libs = false
cuda_lib_dirs = [
  "C:/onnxruntime",
  "C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.2/bin",
  "C:/Program Files/NVIDIA/CUDNN/v9/bin",
]
```

`auto_cuda_libs` ne découvre que les répertoires `nvidia/*/lib` d'un
`site-packages` Python ; il est sans effet sur une installation Windows
classique. Le chargement des DLL CUDA repose sur le `PATH` du processus et sur
`cuda_lib_dirs`.

## Recette

```powershell
cd C:\chemin\workflow-rs
cargo build --release -p workflow-cli
.\target\release\workflow-cli.exe config check

.\target\release\workflow-cli.exe step2 detect `
  --input-dir C:\chemin\parity\work\short_2997_step2 `
  --force --device cuda `
  --onnxruntime C:\onnxruntime\onnxruntime.dll
```

Contrôles attendus :

1. Le provider CUDA est enregistré ; aucune sortie ne doit indiquer un repli
   CPU silencieux.
2. `short_2997.mp4` produit un CSV identique octet pour octet à
   `parity/fixtures/short_2997_step2/expected/short_2997.csv`.
   Le fichier source est en 29,97 fps : le warning de non-conformité doit
   apparaître, mais le CSV reste à 25 fps.
3. Le journal `runtime/logs/step2/transnet_pytorch_<timestamp>.log` est créé et
   contient `TOTAL_VIDEOS_TO_PROCESS`, `PROCESSING_VIDEO`, `INTERNAL_PROGRESS`,
   `Succès` et `Analyse terminée`.
4. Mesurer le temps complet et le pic `nvidia-smi` ; comparer à la baseline
   Linux (2,32 s et ~2 132 Mio pour `short_2997`, à titre indicatif seulement).
5. Rejouer `real_244_step2` et `real_245_step2` pour vérifier les frontières
   sur des vidéos réelles.

Comparaison stricte du CSV :

```powershell
Get-FileHash .\parity\work\short_2997_step2\short_2997.csv -Algorithm SHA256
Get-FileHash .\parity\fixtures\short_2997_step2\expected\short_2997.csv -Algorithm SHA256
```

## Points de panne à documenter

- `onnxruntime.dll` ou `onnxruntime_providers_cuda.dll` introuvable.
- DLL CUDA/cuDNN manquante ou mauvaise version.
- Différence de sortie CSV, même minime, à traiter comme un échec de parité.
- Écart de performance ou de VRAM par rapport à la baseline.
- Comportement de `fs::rename` avec un CSV existant : le code supprime
  explicitement la cible sous Windows avant publication.

La validation doit être exécutée sur la machine Windows cible, puis consignée
dans ce document avec la version d'ONNX Runtime, le pilote NVIDIA, le résultat
SHA-256 et les mesures.
