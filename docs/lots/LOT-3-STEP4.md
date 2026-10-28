# Lot 3 : STEP4 tracking Coral

> Mise à jour lot 5 : STEP4 est branché à l'orchestrateur Rust
> (`workflow-server`, route `/run/STEP4`). Les mentions « pas encore branché »
> ci-dessous décrivent l'état livré au lot 3.

STEP4 est porté dans `workflow-steps::step4`. Le chemin nominal utilise le
Coral Edge TPU ; le backend CPU TFLite/XNNPACK emprunte exactement le même code
de préparation, de post-traitement et de suivi et sert uniquement aux tests.
Le script Python `workflow_scripts/step4/run_tracking_tpu.py` reste l'oracle de
production.

## Architecture

- Décodage FFmpeg en `rawvideo`, à la résolution source, avec
  `fps=metadata.fps` et `rgb24`. Un thread producteur borné à trois trames
  recouvre la lecture FFmpeg et l'inférence ; la mémoire reste ainsi linéaire.
- BlazeFace 128×128, 896 ancres et NMS `score > 0.7`, `IoU <= 0.3`, avec la
  même sémantique d'égalité que `numpy.argsort()[::-1]`.
- Crop affine 192×192 calculé depuis les deux yeux, puis FaceMesh 468 points.
- Blendshapes : 136 indices MediaPipe, padding des iris, sigmoïde float32 et
  filtre One-Euro float64 sur les 52 dimensions ARKit. Le filtre n'est mis à
  jour que sur les frames où un visage est détecté.
- EfficientDet 300×300 en `INTER_NEAREST`, post-traitement
  `[ymin, xmin, ymax, xmax]`, score minimal `0.3`, cinq objets au plus. Le
  détecteur est activé après `object_min_missing` frames sans visage et
  réinitialisé après `face_reset_frames` frames consécutives avec visage.
- Suivi par centroïde le plus proche, seuil 100 px, identifiants `obj_N` et
  désenregistrement après 10 frames sans détection. L'ordre de sortie est trié
  par `centroid_x`, comme Python.
- Détection de parole audio/visuelle et segments STEP2, avec les mêmes champs
  `is_speaking`, `speaking_confidence`, `speaking_method` et
  `speaking_sources`.
- JSON écrit en streaming dans un fichier temporaire, puis renommé sur le
  chemin final. Une interruption ne laisse donc pas de JSON tronqué.
- Le journal est écrit dans `runtime/logs/step4/tpu_tracking_<timestamp>.log`
  avec les lignes de progression attendues.

Les modèles sont résolus depuis `paths.models`. La paire co-compilée
BlazeFace/FaceMesh est sélectionnée avec `cocompiled = true` ; le modèle
Blendshapes reste sur CPU. Chaque modèle TPU ouvre son propre délégué Edge TPU.
Python partage un délégué entre BlazeFace et FaceMesh ; les sorties mesurées
sont identiques, mais cette différence d'implémentation est volontairement
explicite.

## Configuration

```toml
[steps.step4]
enabled = true
device = "tpu"
cocompiled = true
tflite = "/home/kidpixel/workflow-rs/vendor/libtensorflowlite_c.so"
edgetpu = "/usr/lib/x86_64-linux-gnu/libedgetpu.so.1"
blazeface = "step4/blazeface_front_quantized_edgetpu.tflite"
facemesh = "step4/facemesh_quantized_edgetpu.tflite"
blendshapes = "step4/face_blendshapes.tflite"
object_detector_path = "step4/detect_edgetpu.tflite"
blazeface_cpu = "step4/blazeface_front_quantized.tflite"
facemesh_cpu = "step4/facemesh_quantized.tflite"
object_detector_cpu = "step4/detect.tflite"
object_detection = true
object_min_missing = 25
face_reset_frames = 1
object_score_threshold = 0.3
object_max = 5
cpu_threads = 1
jaw_open_threshold = 0.08
decode_width = 0
decode_height = 0
```

Les chemins relatifs sont résolus sous `paths.models`. `tflite` et `edgetpu`
sont des chemins de bibliothèques système ; pour un paquet Coral extrait
localement, passer `--tflite` et/ou `--edgetpu` à la commande plutôt que de
modifier le TOML. `decode_width` et `decode_height` valent zéro pour conserver
la résolution source ; ils doivent être renseignés ensemble. Les champs `enabled`, `fast` et `object_detector`
(chaîne de rôle) restent acceptés pour les anciens TOML, mais le chemin Rust
utilise `object_detection`, `object_min_missing` et
`object_detector_path`. Les champs `blendshapes_throttle`,
`voluminous_exports` et `test_cpu_workers` sont également inertes : les
blendshapes sont calculées à chaque frame détectée, comme dans le script
Python.

## Commande

```sh
cd /home/kidpixel/workflow-rs
cargo build --release -p workflow-cli

./target/release/workflow-cli step4 track \
  --config config/config.toml \
  --input-dir /chemin/isolé/docs \
  --device tpu
```

La commande parcourt récursivement les vidéos `mp4`, `avi`, `mov`, `mkv` et
`webm`, ignore les `.mov` préservés (nom `m6+.mov` ou alpha), prend le verrou
matériel `hardware.lock_path` et réutilise une cascade chargée pour toutes les
vidéos. `<stem>.csv` et `<stem>_audio.json` sont requis à côté de la vidéo ;
`--allow-partial-speaking` autorise explicitement un JSON de test sans ces
entrées. `--no-object-detection` désactive EfficientDet. `--device cpu` bascule
sur les modèles CPU et n'est pas un fallback automatique.

Options utiles : `--object-min-missing`, `--face-reset-frames`,
`--decode-width`, `--decode-height`, `--logs-dir`, `--tflite`, `--edgetpu`.

## Parité

Comparaison sémantique complète, à `1e-6`, avec `metadata.video_path` ignoré.
Les identifiants, le nombre d'objets, les étiquettes, les scores, les champs de
parole et les métadonnées sont comparés, pas seulement les blendshapes.

| Cas | Trames | Objets | IDs différents | Bbox max | Blendshape max | JSON |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `face_pan` | 250 | 194 visages | 0 | 0 px | 2,98e-7 | identique |
| `real_244` | 4 920 | 5 873 | 0 | 0 px | 5,94e-7 | identique |
| `real_245` | 4 728 | 5 903 | 0 | 0 px | 5,96e-7 | identique |

`real_244` contient 2 522 sorties visage et 3 351 sorties objet ; `real_245`
contient 1 404 visages et 4 499 objets. L'IoU des objets appariés par
identifiant vaut 1,0 sur les trois cas, donc au-dessus de la cible 0,99.

Les écarts de blendshapes sont du bruit float32 résiduel inférieur à `6e-7`.
Les 468 landmarks ne sont pas exportés dans le JSON de production ; le crop
192×192 et son entrée FaceMesh sont identiques au bit près, et le modèle
FaceMesh a déjà été validé au bit près au lot 0. Les 136 points utilisés par le
modèle de blendshapes et les points d'iris sont identiques, ce qui couvre le
contrôle de landmarks sans publier de champ supplémentaire.

## Performance et mémoire

Mesures release sur la machine de production. Les temps incluent le chargement
des modèles et le décodage complet.

| Cas | Rust | Baseline Python | RSS Rust | RSS Python |
| --- | ---: | ---: | ---: | ---: |
| `face_pan` | 1,95 s | 2,425 s | 97,6 Mio | 409,8 Mio |
| `real_244` | 44,2 s | 40,516 s | 199,5 Mio | 593,3 Mio |
| `real_245` | 41,0 s | 39,572 s | 193,4 Mio | 589,5 Mio |

Le RSS est mesuré avec `/usr/bin/time -v` ; il reste sous la cible de 450 Mio.
Le pipeline est environ 3 à 9 % plus lent que Python sur les vidéos longues,
malgré le recouvrement décodage/inférence, car chaque modèle TPU a son propre
délégué et le chemin reste mono-inférence.

## Backend CPU de test

```sh
./target/release/workflow-cli step4 track \
  --config config/config.toml \
  --input-dir /tmp/step4-cpu \
  --device cpu \
  --no-object-detection \
  --allow-partial-speaking
```

Le backend CPU a traité `face_pan` en 13,8 s sur 250 trames. Les modèles CPU
ont des poids quantifiés différents : ce chemin valide le chargement TFLite,
les tenseurs, le suivi et l'écriture JSON, pas la parité TPU. Un test synthétique
de 50 trames a aussi exercé le détecteur d'objets CPU.

## Critères d'acceptation

| Critère | Cible | Résultat |
| --- | --- | --- |
| Erreur moyenne des landmarks | < 1 px | crop et entrée FaceMesh identiques au bit près ; FaceMesh validé au bit près au lot 0 |
| Écart blendshapes | < 1e-3 | 5,96e-7 au maximum |
| IoU détection d'objets | > 0,99 | 1,0 |
| RSS pic | < 450 Mio | 199,5 Mio sur `real_244` |
| Parité du schéma complet | 0 différence | 0 à `1e-6` sur les trois cas |

## Limites connues

- La cible de production est Linux. `workflow-steps::step4` et la commande CLI
  sont compilés uniquement sous Unix ; la compilation croisée Windows de
  `workflow-cli` reste valide, sans STEP4. La validation Windows/After Effects
  est abandonnée (D-022) : la production cible Linux.
- Le backend CPU est un chemin de test explicite, pas une solution de repli de
  production.
- Les options Python `--ndjson`, le profiling et l'export verbeux des landmarks
  ne sont pas repris ; ils ne changent pas le JSON de production.
- Le détecteur EfficientDet-Lite2 et les anciens champs de configuration sont
  conservés dans le manifeste ou acceptés pour compatibilité, mais le chemin
  STEP4 utilise `detect_edgetpu.tflite`.
- STEP4 n'est pas encore branché à l'orchestrateur Rust ; l'application Python
  legacy reste le workflow actif. La double exécution shadow et le branchement
  appartiennent aux lots suivants.
