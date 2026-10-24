# Contexte produit — workflow-rs

## Objectif

`workflow-rs` est la réécriture Rust du pipeline `workflow_mediapipe`. Il
automatise l'ingestion, l'extraction, l'analyse vidéo/audio, la réduction JSON,
le pré-traitement After Effects, la finalisation et la génération de projets
EffectCraft.

Le dépôt Python reste la référence de production et le chemin de rollback. La
migration n'a pas pour objectif de modifier le legacy : elle doit produire des
artefacts compatibles et permettre une bascule contrôlée.

## Périmètre

- **8 étapes** : STEP1 extraction/normalisation, STEP2 transitions, STEP3 audio,
  STEP4 tracking, STEP5 réduction, STEP6 pré-traitement AE, STEP7 finalisation,
  STEP8 Media Solution.
- **Serveur** : orchestrateur axum sur `:5004`, contrat de polling, séquences,
  arrêt, logs, SQLite et verrou isolés.
- **Ingestion** : moniteur webhook, normalisation d'URL, historique SQLite,
  cooldown, téléchargement R2/Dropbox reprenable.
- **Synchronisation** : fusion explicite de l'historique, de la liste STEP1 et
  des archives STEP7 avant une bascule Python ↔ Rust.
- **Pont AE** : `workflow-bridge` pour les cuts CSV et l'analyse de manifeste.
- **Frontend** : copie de `assets/web`, servie par Minijinja.
- **EffectCraft** : STEP8 headless avec `effectcraft-cli`.

## Non-objectifs

- Réécrire le panneau CEP ou les scripts JSX After Effects (D-022).
- Porter les routes Python-only (cache, stats, diagnostics, `/api/audio/*`)
  sans décision explicite (D-015).
- Modifier `/home/kidpixel/workflow_mediapipe`.
- Partager les racines de production Python et Rust (D-019, D-023).
- Reproduire les quirks D-001, D-002, D-004, D-016 sans les documenter.

## Architecture

| Crate | Rôle |
| --- | --- |
| `workflow-core` | Config TOML typée, chemins résolus, verrou matériel. |
| `workflow-schema` | Schémas JSON, arrondis Python. |
| `workflow-steps` | STEP1–STEP8, rétention. |
| `workflow-cli` | Entrée opérationnelle des étapes, ingestion, sync. |
| `workflow-server` | Orchestrateur axum `:5004`. |
| `workflow-ingest` | Webhook, URL, SQLite, téléchargements. |
| `workflow-sync` | Fusion Python ↔ Rust. |
| `workflow-bridge` | Pont JSON After Effects. |
| `workflow-tpu` | FFI TFLite/Edge TPU et spike. |

## Pipeline

1. **STEP1** : scan des archives, extraction sécurisée, assainissement,
   normalisation MP4/H.264/progressif/25 fps, préservation `.mov`, suppression
   de l'archive après succès.
2. **STEP2** : TransNetV2 ONNX Runtime CUDA, fenêtres 100/50/25, CSV de scènes.
3. **STEP3** : YAMNet Split Edge TPU, frontend log-mel CPU, ECAPA-TDNN ONNX CPU,
   AHC, `_audio.json`.
4. **STEP4** : BlazeFace/FaceMesh/EfficientDet, One-Euro, suivi, parole,
   `_tracking.json`.
5. **STEP5** : réduction streaming audio/vidéo, analytics, alignement temporel.
6. **STEP6** : `_ae.json`, index par frame, recommandations de segments.
7. **STEP7** : archivage SHA-256, copie finalisée, manifeste
   `finalized_projects_<ts>.json`.
8. **STEP8** : génération `<video>_9x16.ecproj` via `effectcraft-cli`.

## Intégrations

- **Webhook** : `https://webhook.kidpixel.fr/webhook_proxy.php`.
- **R2 / Dropbox** : téléchargement reprenable, repli R2 → Dropbox.
- **Coral Edge TPU** : `/dev/apex_0`, `libedgetpu.so.1`,
  `libtensorflowlite_c.so`.
- **GPU** : ONNX Runtime CUDA pour STEP2.
- **EffectCraft** : `effectcraft-cli`, panel `MediaSolution.jsx`.
- **After Effects** : pont `cuts`/manifeste ; CEP et JSX restent Python.

## Interface

- Timeline 8 étapes, sélection de séquence, progression, logs.
- Overlay de logs avec focus trap et auto-ouverture configurable.
- Polling via `PollingManager`.
- Frontend copié du legacy ; pas de logique Rust spécifique.

## Modèle de production

- Rust est configuré sur `:5004` ; Python legacy sur `:5003` pour le rollback.
- Un seul backend avec le webhook actif.
- Le moniteur Rust est activé depuis le 2026-10-23
  (`webhook.enabled = true`) ; il ne tourne que si le serveur est démarré.
- Les données Rust vivent sous `runtime/` ; `/mnt/cache` est la sortie
  finalisée partagée.
- Le verrou matériel Rust est `/tmp/workflow-rs-hw.lock` ; la sync utilise
  `/tmp/workflow-sync.lock`.
- `scripts/switch-rust.sh` et `scripts/sync-backends.sh` gèrent la bascule.

## État de validation

- 233 tests unitaires, sans matériel.
- Parité STEP2/3/4/5/6 sur les corpus de référence.
- Chaîne STEP1 → STEP8 validée sur `1720 Camille.zip`.
- Shadow STEP5/6 `162/162` sur 20 projets.
- Sync réelle : 911 URL, 96 archives, 640 hash STEP7.
- D-024 : le STEP4 Python régénéré sur `153 Camille` diffère de l'archive ;
  ne pas utiliser `--prefer-raw` sur ce projet.

## Références

- `README.md`
- `AGENTS.md`
- `docs/CONTRAT-ENVIRONNEMENT.md`
- `docs/LOT-*.md`
- `parity/DRIFT.md`
- `memory-bank/systemPatterns.md`
- `memory-bank/decisionLog.md`
