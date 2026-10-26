# Lot 4 — STEP3 audio

| Champ | Valeur |
| --- | --- |
| Type | Lot |
| Statut | Référence |
| Date | 2026-10-22 |
| Portée | VAD YAMNet Split, ECAPA-TDNN, AHC et projection STEP5 audio. |
| Voir aussi | `docs/contracts/CONTRAT-ENVIRONNEMENT.md`, `docs/pipeline/LOT-1-STEP5.md`, `parity/DRIFT.md` |

## Objectif

Porter `workflow_scripts/step3/run_audio_diarization_tpu.py` en Rust, avec un
seul chemin d'implémentation :

- YAMNet Split INT8 compilé pour le Coral Edge TPU ;
- frontend log-mel CPU identique à `yamnet_frontend.py` ;
- ECAPA-TDNN float32 via ONNX Runtime CPU ;
- AHC average, distance cosinus, seuil 0,32 et réassignation des locuteurs
  mineurs ;
- sortie `<stem>_audio.json` compatible STEP5/STEP6.

Le backend CPU de YAMNet (`--device cpu`) est un mode de test explicite. Il
utilise `yamnet_split_quantized.tflite` et n'est jamais sélectionné par le
chemin de production.

## Commande

```sh
workflow-cli step3 diarize --input-dir <docs> \
  --edgetpu /path/to/libedgetpu.so.1
```

Le chemin nominal prend le verrou matériel, exige
`steps.step3.require_edgetpu = true` et refuse un modèle sans signature
`edgetpu-custom-op`. Le JSON est écrit dans un fichier temporaire voisin puis
renommé.

## Pipeline

1. Découverte récursive des vidéos et skip de `<stem>_audio.json` existant.
2. Extraction FFmpeg en PCM16 16 kHz mono.
3. VAD YAMNet : fenêtres de 15 360 échantillons (0,96 s), hop 7 680 (0,48 s),
   frontend log-mel (400/160, FFT 512, 64 mel), quantification INT8, médiane 5,
   seuil strict 0,20 et hangover 1,0 s.
4. ECAPA : segments de 3 s centrés sur chaque fenêtre de parole, log-mel
   80 × 300, CMN, batch 32, embeddings 192D.
5. AHC average sur les embeddings normalisés L2, puis réassignation des
   clusters de moins de 7 s au centroïde majeur le plus proche.
6. Fusion des segments adjacents et écriture des frames 1 à `total_frames` à
   25 fps, avec `speaker_stats` brut STEP3.

## Parité

| Cas | Frames | Segments | Locuteurs | Diff JSON brut |
| --- | ---: | ---: | ---: | ---: |
| `vox_2spk` | 750 | 6 | 3 | 0 |
| `real_244` | 4 921 | 29 | 4 | 0 |
| `real_245` | 4 729 | 41 | 6 | 0 |

Les références `real_244` et `real_245` ont été régénérées par le script Python
sur une copie isolée, avec le même YAMNet Split Edge TPU. La comparaison brute
porte sur `video_filename`, `total_frames`, `fps`, `speaker_stats`,
`frames_analysis`, les labels, le nombre de locuteurs et les timecodes.

Après projection STEP5 audio, les fixtures de production `real_244_step3` et
`real_245_step3` sont également identiques : 0 différence.

### Cas sans audio

`synthetic_silent.mp4` produit `SKIP_NO_AUDIO`, aucune sortie, code de sortie
0 et `skipped_no_audio = 1`. Le succès silencieux de Python (D-002) n'est pas
reproduit.

### Chemin CPU de test

`vox_2spk` exécuté avec `--device cpu` donne le même JSON que la fixture
Python. Ce chemin reste réservé aux tests ; il ne remplace pas le Coral.

## Performance

Mesures locales sur les copies isolées, avec le JSON brut :

| Cas | Durée Rust | RSS max | Durée Python |
| --- | ---: | ---: | ---: |
| `real_244` | 32,7 s | 1 146 Mio | 17,8 s |
| `real_245` | 33,0 s | 1 116 Mio | 17,4 s |

Le temps est dominé par ECAPA sur CPU (≈29,5 s par vidéo). Python utilise
OpenVINOExecutionProvider sur la même machine, alors que le port Rust utilise
le CPUExecutionProvider d'ONNX Runtime. La parité est exacte, mais le temps
Rust est environ 1,8× celui de Python sur ces deux extraits. Le RSS inclut le
pic transitoire d'ONNX Runtime ; il reste inférieur au pic mesuré du même
modèle ECAPA seul avec Python.

## Tests et limites

- `cargo test --workspace` passe, y compris les tests unitaires du frontend
  mel, du VAD, de l'AHC et du writer JSON.
- Le traitement des vidéos est séquentiel au lot 4, même si
  `steps.step3.max_workers` existe encore pour compatibilité.
- Le branchement à l'orchestrateur Rust et le basculement contrôlé restent
  hors de ce lot.
- Windows et After Effects restent hors scope ; la production cible Linux.
- Aucune donnée de production, aucun projet `/mnt/cache` et aucun JSX AE n'ont
  été modifiés pendant la validation.
