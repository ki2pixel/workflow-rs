---
description: Standards obligatoires de workflow-rs (Rust), réécriture du pipeline workflow_mediapipe
alwaysApply: true
---

# workflow-rs — standards de codage et d'architecture

> Toute déviation doit être justifiée dans `memory-bank/decisionLog.md` et, si elle
> concerne la parité, dans `parity/DRIFT.md`. Le dépôt Python
> `/home/kidpixel/workflow_mediapipe` reste la référence de production et le
> chemin de rollback : il ne se modifie pas depuis ce dépôt.

## 1. Stack et configuration

- **Langage** : Rust 1.86 minimum, edition 2021. Workspace de neuf crates.
- **Serveur** : `axum` 0.8 sur le port **5004**. Il sérialise les exécutions et
  délègue aux sous-processus `workflow-cli` ; il ne contient pas la logique des
  étapes.
- **CLI** : `workflow-cli` est l'unique point d'entrée opérationnel des huit
  étapes. Le serveur et les scripts l'invoquent. Aucun fallback Python.
- **Config** : `config/config.toml`, types TOML avec `deny_unknown_fields`.
  Les secrets viennent de l'environnement (`WORKFLOW_SERVER_SECRET_KEY`,
  `WORKFLOW_SERVER_WORKER_TOKEN`), jamais du TOML. `--config` est global.
- **Données** : SQLite via `rusqlite` pour l'ingestion ; JSON via `serde_json`
  avec la feature `preserve_order` ; arrondis via
  `workflow_schema::round::{round2, round4, round_to}`.
- **Matériel** : TFLite C et Edge TPU chargés dynamiquement par
  `workflow-tpu` ; ONNX Runtime via `ort` pour STEP2 ; FFmpeg/ffprobe en
  sous-processus.
- **Frontend** : `assets/web` est une copie du frontend Python legacy. Le
  serveur Rust ne le modifie pas ; il adapte à l'inférence Minijinja `url_for`,
  `steps_config.items()`, `csrf_token`, le jeton worker et la sérialisation
  `tojson`.

## 2. Structure du dépôt

| Crate | Responsabilité |
| --- | --- |
| `workflow-core` | Config typée, chemins résolus, verrou matériel. |
| `workflow-schema` | Schémas JSON et arrondis compatibles Python. |
| `workflow-steps` | STEP1 à STEP8, rétention. |
| `workflow-cli` | Entrée opérationnelle des étapes, de l'ingestion et de la sync. |
| `workflow-server` | Orchestrateur axum `:5004`, état, séquence, logs. |
| `workflow-ingest` | Webhook, URL, historique SQLite, téléchargements R2/Dropbox. |
| `workflow-sync` | Fusion Python ↔ Rust avant bascule. |
| `workflow-bridge` | Pont JSON After Effects (`cuts`, manifeste). |
| `workflow-tpu` | FFI TFLite/Edge TPU et binaire de spike. |

- Une étape vit dans `crates/workflow-steps/src/stepN` (dossier ou fichier).
- Le câblage CLI va dans `crates/workflow-cli/src/main.rs`.
- Le mapping serveur va dans `crates/workflow-server/src/catalog.rs`.
- Ne pas créer de dossier `tests/` d'intégration : les tests sont inline dans
  les modules, sous `#[cfg(test)]`.

## 3. Style de code

- `cargo fmt` est obligatoire ; `cargo clippy --workspace --all-targets --
  -D warnings` doit passer.
- Erreurs : `thiserror` dans les bibliothèques, `anyhow` aux frontières CLI,
  serveur et scripts. Pas de `unwrap()` en production. Un `expect()` n'est
  toléré que pour un invariant local, avec un message explicite.
- Pas de `println!` de debug dans les étapes ou le serveur : utiliser `tracing`.
  Les sorties machine contractuelles (`TOTAL_*`, `MS_BATCH_*`, le passthrough
  `effectcraft-cli`) et `config check`/`config show` restent des exceptions.
- Commentaires et documentation en français, orientés « pourquoi ». Les
  identifiants et les commandes restent en anglais.
- Préférer les emprunts aux clones ; justifier tout `clone()` dans un chemin
  chaud. Éviter les allocations par frame dans STEP2/3/4.
- Les écritures de fichiers contractualisés sont atomiques : fichier
  temporaire voisin, `flush`, puis `rename`.
- Les JSON contractuels conservent l'ordre d'insertion. Ne jamais désactiver
  `preserve_order`.

## 4. Patrons transverses

- **Config** : charger une fois, valider avec `validate_common` côté CLI et
  `validate_server` côté serveur, puis résoudre les chemins via
  `config.paths.resolve()`. Ne pas recalculer les chemins à la main.
- **Isolation** : `paths.base`, `history_db`, `projects`, `archives`, `logs` et
  `downloads` restent sous la racine Rust. Le serveur refuse une base hors
  `paths.base`, le port 5003, le verrou Python `/tmp/workflow-hw.lock` et les
  jetons par défaut.
- **Verrou matériel** : `HardwareLock` sur `hardware.lock_path`
  (`/tmp/workflow-rs-hw.lock`) autour de STEP2/3/4. Jamais le verrou Python.
- **Serveur** : une seule étape à la fois, deuxième lancement refusé en 409 ;
  une séquence s'arrête au premier échec ; `stop`/`cancel` envoient SIGTERM et
  attendent la fin.
- **Sous-processus** : le serveur et STEP2 utilisent `tokio::process` avec
  `kill_on_drop`, sorties pompées vers les logs et timeout par étape. STEP1/3/4
  et STEP8 utilisent `std::process` avec threads dédiés et arrêt explicite
  (groupe de processus et watchdog pour EffectCraft). Codes de retour et
  progression `progress_patterns` conservés.
- **Ingestion** : un seul moniteur actif (Python ou Rust). Le moniteur Rust
  refuse une base vide (`require_history`) et exige
  `steps.step1.source_dir == paths.downloads`.
- **Sync** : `workflow-cli sync` fusionne l'historique, la liste STEP1 et les
  analyses STEP7. Toujours faire un dry-run avant `--apply`, et arrêter les
  deux serveurs.

## 5. Pipeline

### STEP1 — extraction et normalisation

- Scan non récursif de `steps.step1.source_dir` ; extensions réellement
  sélectionnées : `.zip`, `.zipx`, `.rar`, `.tgz`, `.7z`, `.tar` (D-016).
- Assainir chaque chemin de membre et refuser les traversées.
- Conserver la liste mensuelle `processed_archives.txt` et supprimer l'archive
  après succès (`--keep-archives` pour déroger).
- Normaliser en MP4 / H.264 / yuv420p / **progressif** / 25 fps ; `yadif` pour
  l'entrelacé, repli CPU `libx264`.
- Préserver les `.mov` alpha et `m6+.mov` à l'octet près (D-010).

### STEP2 — TransNetV2

- FFmpeg `rawvideo` 48×27, 25 fps ; fenêtres 100, pas 50, padding 25.
- `ort` avec `CUDAExecutionProvider`, cuDNN exhaustif, TF32 désactivé.
- CSV `\r\n` avec l'en-tête Python ; pas de fallback CPU silencieux.

### STEP3 — audio

- YAMNet Split INT8 sur Edge TPU ; frontend log-mel CPU ; ECAPA-TDNN ONNX CPU ;
  AHC moyenne, distance cosinus, seuil 0,32, réassignation des mineurs.
- Sans audio : `SKIP_NO_AUDIO`, aucune sortie, code 0 (D-002).

### STEP4 — tracking

- BlazeFace 128×128, FaceMesh 192×192, EfficientDet 300×300, One-Euro 52D.
- `<stem>.csv` et `<stem>_audio.json` sont requis par défaut (D-004) ;
  `--allow-partial-speaking` est réservé aux fixtures.
- JSON streamé vers un temporaire puis renommé.

### STEP5/6 — réduction et pré-traitement AE

- STEP5 écrit `fps`/`total_frames` **avant** `frames_analysis` (D-003).
- STEP6 produit `<stem>_ae.json` avec `dataByFrame`, `audioByFrame`,
  `segment_recommendations`, `is_speaking` et `tier`.
- Les deux étapes sont streaming et doivent rester sous 100 Mio de RSS sur les
  cas de production.

### STEP7/8 — finalisation et EffectCraft

- STEP7 archive par SHA-256 de la vidéo, copie vers `paths.finalized`, écrit le
  manifeste `finalized_projects_<ts>.json`, puis supprime la source
  (`--no-delete`/`--dry-run` pour les tests).
- STEP8 est headless : `effectcraft-cli script`, panel `MediaSolution.jsx`
  concaténé à un driver, lignes `MS_BATCH_*`, SIGTERM vers le groupe enfant.

## 6. Frontend et After Effects

- `assets/web` : copie de `workflow_mediapipe`. Ne pas y introduire de
  logique Rust ; synchroniser depuis le legacy puis relancer les tests de
  rendu du crate `workflow-server`.
- `assets/effectcraft` : copie du panel de production. Le wrapper ne modifie
  pas le JSX ; le driver est généré à l'exécution.
- Le panneau CEP et les scripts JSX continuent d'appeler Python (D-022). Le
  binaire `workflow-bridge` est un contrat Rust disponible, pas un basculement
  de production.
- Ne pas « corriger » les formats legacy : `_ae.json`, `_tracking.json`,
  `_audio.json`, CSV et `MS_BATCH_*` sont contractuels.

## 7. Qualité et processus

- Commandes de référence :
  ```sh
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cargo build --release -p workflow-server -p workflow-cli
  ```
- Il n'y a pas de CI : exécuter ces commandes manuellement avant de terminer.
- Les tests unitaires ne couvrent ni les modèles, ni le GPU, ni le Coral. Une
  suite verte ne prouve pas la parité.
- La parité réelle se mesure avec `scripts/shadow-step56.py`,
  `scripts/validate-lot5.py` et `scripts/validate-step1.py`, sur des copies,
  jamais en écrivant dans `/mnt/cache`.
- Utiliser `--release` pour STEP2/3/4, STEP8 et le serveur.
- Commits : Conventional Commits en anglais, scope réel (`step1`…`step8`,
  `server`, `ingest`, `sync`, `config`, `launcher`, `docs`).

## 8. Anti-patrons

- Réintroduire un fallback Python ou appeler `workflow_scripts/` depuis Rust.
- Modifier `/home/kidpixel/workflow_mediapipe` ou y écrire.
- Lancer les moniteurs Python et Rust en même temps.
- Lancer STEP2/3/4 matérielles des deux stacks simultanément.
- Écrire ou supprimer dans `/mnt/cache` depuis un test.
- Utiliser `runtime/` comme espace de brouillon : c'est la production locale.
- Remplacer `round_to` par `f64::round` ou désactiver `preserve_order`.
- Charger un JSON de tracking complet en RAM quand un réducteur streaming
  suffit.
- Porter les routes Python-only (cache, stats, diagnostics, `/api/audio/*`)
  sans décision explicite (D-015).
- Nettoyer les quirks documentés D-001, D-002, D-004, D-016.
- Recompiler ou remplacer un modèle Edge TPU sans mise à jour du manifeste
  SHA-256 et validation du spike.
- Utiliser des canaux non bornés ou un parallélisme non contrôlé sur le TPU.
- Laisser un `unwrap()` sur une entrée utilisateur, un chemin ou un fichier.

> Maintenir ce fichier concis. Les détails de lot vivent dans `docs/LOT-*.md`,
> les écarts dans `parity/DRIFT.md`, l'état courant dans `memory-bank/`.
