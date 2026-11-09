# Lot B0 — exports ONNX et benchmark `ort` CUDA

- **Date** : 2026-11-09
- **Statut** : exécuté, hors production
- **Étude** : `docs/etudes/ETUDE-YOLO-ONNX-A2.md` §10
- **Binaire** : `crates/workflow-steps/examples/yolo_bench.rs`
- **Corpus** : 20 frames D-060 (8 références, 6 A2, 6 négatifs), 100 itérations

**Lot B1** (fine-tuning et évaluation open-set) : voir `B1.md` et les fichiers
`b1-*.json` du même dossier.

**Lot B2** (pilote `report_only`, annotation A2) : voir `B2.md` et
`b2-pilot.json` (copie assainie, chemins locaux et noms de fichiers vidéo
retirés).

## 1. Modèles exportés

| Modèle | Entrée | Sortie | Opset | FP32 | FP16 |
| --- | --- | --- | ---: | ---: | ---: |
| YOLOX-Nano | 1×3×416×416 | 1×3549×85 | 12 | 3,8 Mo | 2,0 Mo |
| YOLOX-Nano | 1×3×640×640 | 1×8400×85 | 12 | 3,9 Mo | 2,1 Mo |
| NanoDet-Plus-m | 1×3×416×416 | 1×3598×112 | 11 | 5,8 Mo | 3,5 Mo |

Sources : YOLOX `0.3.0` / poids `0.1.1rc0`, NanoDet `nanodet-plus-m_416.yml` /
poids `v1.0.0-alpha-1`. Les deux sont Apache-2.0. Les hashes, tailles et
paramètres de parité sont dans `export.json`.

Parité PyTorch → ONNX Runtime CPU :

| Modèle | Écart max | Écart moyen |
| --- | ---: | ---: |
| YOLOX 416 | 1,19e-2 px | 5,54e-6 px |
| YOLOX 640 | 1,12e-2 px | 6,95e-6 px |
| NanoDet 416 | 3,62e-5 | 6,65e-7 |

## 2. Latence GPU

GTX 1650 4 Gio, CUDA EP, batch 1, p50 / p95 en ms. « run » est la session
`ort` seule ; « total » ajoute le redimensionnement, la conversion CHW et
le décodage/NMS.

| Modèle | Entrée | Précision | Prétrait. | `run` | Post | Total | Δ VRAM |
| --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| YOLOX-Nano | 416 | FP32 | 3,03 / 3,18 | 3,90 / 5,29 | 0,04 / 0,05 | **7,01 / 8,40** | +107 Mo |
| YOLOX-Nano | 640 | FP32 | 6,84 / 7,10 | 9,46 / 9,78 | 0,08 / 0,10 | 16,36 / 16,80 | +161 Mo |
| NanoDet-Plus-m | 416 | FP32 | 5,16 / 5,45 | 5,28 / 6,48 | 0,43 / 0,47 | 10,82 / 12,16 | +109 Mo |
| YOLOX-Nano | 416 | FP16 | 2,91 / 3,10 | 3,50 / 5,18 | 0,04 / 0,05 | 6,52 / 8,19 | +112 Mo |
| YOLOX-Nano | 640 | FP16 | 6,81 / 7,08 | 6,77 / 8,09 | 0,08 / 0,10 | **13,69 / 15,00** | +107 Mo |
| NanoDet-Plus-m | 416 | FP16 | 5,18 / 5,35 | 4,46 / 6,04 | 0,43 / 0,47 | 10,11 / 11,64 | +134 Mo |

Le delta VRAM inclut le contexte CUDA ; il fluctue avec les autres
processus. Le pic absolu n'est pas comparable d'une série à l'autre.

Diagnostic CPU (10 frames, 20 itérations, non optimisé) :

| Modèle | Prétrait. | `run` | Post | Total |
| --- | ---: | ---: | ---: | ---: |
| YOLOX 416 | 4,40 / 4,52 | 27,45 / 35,28 | 0,02 / 0,03 | 31,96 / 40,30 |
| NanoDet 416 | 8,04 / 10,91 | 20,48 / 40,48 | 0,48 / 0,59 | 29,10 / 52,47 |

## 3. Parité FP16

Détections après seuil 0,25 et NMS 0,45, sur les 20 frames :

| Modèle | FP32 | FP16 | Appariées IoU ≥ 0,5 | Non appariées |
| --- | ---: | ---: | ---: | ---: |
| YOLOX 416 | 86 | 86 | 86 | 0 |
| YOLOX 640 | 94 | 93 | 93 | 1 (0,254) |
| NanoDet 416 | 181 | 127 | 118 | 63 |

YOLOX FP16 est stable. NanoDet FP16 perd 63 détections FP32, dont une à
0,663 : NanoDet reste en FP32.

## 4. Sanity check zéro-shot

COCO n'a ni `logo` ni `lower_third`. Sur les 20 frames, YOLOX 416 active au
moins un objet COCO sur 7/8 références, 6/6 bandeaux et 6/6 négatifs ; la
classe dominante est `person` (61/86). Ce n'est pas un taux de faux positifs
logo, seulement la preuve que le zéro-shot COCO ne suffit pas.

## 5. Reproduction

```sh
NV=/mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/nvidia
ORT=/mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/onnxruntime/capi
export LD_LIBRARY_PATH=$(find "$NV" -maxdepth 2 -type d -name lib | tr '\n' ':')$ORT:$LD_LIBRARY_PATH

cargo run --release -p workflow-steps --example yolo_bench -- \
  --model /tmp/opencode/yolo-a2/onnx/yolox_nano_416.onnx \
  --frames /tmp/opencode/yolo-a2/frames/selection.json \
  --kind yolox --size 416 --device cuda \
  --onnxruntime "$ORT/libonnxruntime.so.1.23.2" \
  --warmup 20 --iters 100 \
  --output /tmp/opencode/yolo-a2/bench/yolox416.json
```

Le JSON de frames contient `id`, `source`, `negative`, `width`, `height`,
`bgr`, `gt_logo`, `gt_band`, `gt_text`. `selection.json` liste les 20
frames ; les PNG D-060 sont convertis en BGR brut par un script labo non
versionné. Aucun média client n'est dans ce dossier.

## 6. Fichiers

- `bench-yolox416-fp32.json`, `bench-yolox416-fp16.json`
- `bench-yolox640-fp32.json`, `bench-yolox640-fp16.json`
- `bench-nanodet416-fp32.json`, `bench-nanodet416-fp16.json`
- `bench-yolox416-cpu.json`, `bench-nanodet416-cpu.json`
- `selection.json`, `export.json`

## 7. Limites

- 20 frames, une GPU, pas de mesure de qualité sur les 242 frames.
- Prétraitement bilinéaire scalaire : 6,8–7,1 ms à 640, d'où un total FP32 de
  16,80 ms. Le modèle seul reste sous 10 ms.
- Mesures Python FP16 non retenues : seules les mesures Rust Level3 font
  foi.
- INT8 non testé (GTX 1650 sans tensor cores, chemin TensorRT séparé).
- Les poids dérivés du dataset client ne doivent pas être redistribués.
