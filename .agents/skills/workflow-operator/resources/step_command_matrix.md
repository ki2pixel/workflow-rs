# Matrice des commandes — workflow-rs

> Lancer depuis la racine du dépôt. Toujours passer `--config
> config/config.toml` si le répertoire courant n'est pas la racine. Les étapes
> matérielles prennent `/tmp/workflow-rs-hw.lock`.

| Étape | Objectif | Commande standard | Logs | Prérequis | Options destructives |
| --- | --- | --- | --- | --- | --- |
| STEP1 | Extraction + normalisation MP4/H.264/progressif/25 fps | `workflow-cli step1 run --source-dir runtime/downloads` | `runtime/logs/step1/` | `ffmpeg`, `ffprobe`, `unrar` si RAR, source accessible | Supprime l'archive après succès ; `--keep-archives` pour conserver |
| STEP2 | TransNetV2 CUDA | `workflow-cli step2 detect --input-dir runtime/projets_extraits` | `runtime/logs/step2/` | ONNX Runtime CUDA, modèle `step2/transnetv2.onnx`, GPU | Aucune suppression ; `--force` réécrit les CSV |
| STEP3 | Diarisation YAMNet/ECAPA | `workflow-cli step3 diarize --input-dir runtime/projets_extraits --edgetpu vendor/libedgetpu.so.1` | `runtime/logs/step3/` | `/dev/apex_0`, `libtensorflowlite_c.so`, `libedgetpu.so.1`, modèles STEP3 | Aucune suppression ; `--force` réécrit les `_audio.json` |
| STEP4 | Tracking BlazeFace/FaceMesh/EfficientDet | `workflow-cli step4 track --input-dir runtime/projets_extraits --device tpu` | `runtime/logs/step4/tpu_tracking_*.log` | Modèles STEP4, `/dev/apex_0`, CSV et audio présents | Aucune suppression ; `--allow-partial-speaking` réservé aux tests |
| STEP5 | Réduction JSON | `workflow-cli step5 project --work-dir runtime/projets_extraits --keyword Camille` | `runtime/logs/step5/` | JSON STEP3/STEP4 | Réécrit les JSON réduits |
| STEP6 | Pré-traitement AE | `workflow-cli step6 project --work-dir runtime/projets_extraits --keyword Camille` | `runtime/logs/step6/` | `_tracking.json`, `_audio.json`, `<stem>.csv` | Réécrit les `_ae.json` |
| STEP7 | Finalisation + archivage | `workflow-cli step7 finalize --dry-run` puis sans `--dry-run` | `runtime/logs/step7/`, manifeste `finalized_projects_*.json` | `paths.finalized`, `paths.archives` accessibles | Supprime la source après publication ; `--no-delete` pour conserver |
| STEP8 | Projets EffectCraft | `workflow-cli step8 run --dry-run` puis `step8 run` | `runtime/logs/step8/`, `step8_run_*.json` | `effectcraft-cli`, manifeste STEP7, `assets/effectcraft/mediasolution_ec.json` | Réécrit les `.ecproj` ; `--dry-run` pour inspecter ; `--no-post` désactive A5/A6 |
| A5/A6 | Post-traitement déterministe | `workflow-cli post run --folder <dossier_projet>` | `docs/post/<projet_assisté>/` ; stdout du CLI | STEP8 réussi, `ffmpeg`/`ffprobe`, `[steps.step8.post]` | Publie `<video>_9x16_assist.ecproj` ; le brut reste intact ; `--report-only` pour ne rien publier |

## Rappels

- **Pas de `python3`** : le pipeline Rust n'appelle aucun script Python.
- **Pas de double moniteur** : un seul backend avec `webhook.enabled = true`.
- **Pas de double matériel** : STEP2/3/4 ne doivent pas tourner dans les deux
  stacks en même temps.
- **Pas d'écriture `/mnt/cache` en test** : utiliser `--work-dir`,
  `--output-dir`, `--archives-dir`, `--logs-dir` sous `/tmp/opencode`.
- **STEP1 n'a pas de dry-run** : vérifier la source et `--keep-archives` avant.
- **STEP7/8** : `--dry-run` d'abord, puis exécution réelle.
