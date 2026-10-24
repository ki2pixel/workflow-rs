# Journal des décisions

Ce document enregistre les décisions architecturales et techniques durables de
workflow-rs. Les écarts de comportement legacy sont détaillés dans
`parity/DRIFT.md`.

> **Politique de conservation** : ce fichier conserve les décisions depuis le
> début de la migration Rust (2026-10-22). Les décisions legacy antérieures
> restent dans `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.

## 2026-10-24 — Bootstrap des agents et de la memory-bank

- **Contexte** : le dépôt Rust n'avait ni `.agents/` ni `memory-bank/`, alors
  que le legacy Python disposait d'une infrastructure d'agents éprouvée.
- **Décision** :
  1. Créer `.agents/rules/`, `.agents/skills/`, `.agents/workflows/` et
     `memory-bank/` dans le dépôt Rust.
  2. Renommer les skills legacy dont la numérotation était obsolète :
     `step4-audio-orchestrator` → `step3-audio-orchestrator`,
     `step5-gpu-ops` → `step4-tracking-ops`.
  3. Remplacer les skills legacy-only par des skills Rust :
     `opencv5-dnn-engine-expert` → `onnx-runtime-cuda-expert`,
     `cv5-multiprocessing-orchestrator` → `rust-concurrency-orchestrator`,
     `after-effects-cep-panel` → `after-effects-bridge`,
     `after-effects-scripts` → `after-effects-json-contract`.
  4. Conserver 5 fichiers canoniques de memory-bank, sans archives initiales.
  5. Pointer les arborescences depuis `AGENTS.md`.
- **Alternatives rejetées** :
  - Copier tel quel le contenu legacy : trop de commandes Python obsolètes.
  - Supprimer les skills sans équivalent Rust : perte de la frontière
    CEP/JSX/AE.
  - Renommer tous les skills : aurait cassé la correspondance avec le legacy.
- **Validation** : arborescence de 51 fichiers à l'époque, vérification des
  commandes et chemins, `cargo fmt`, `clippy`, `test`, commit unique.

## 2026-10-24 — Skill `rust-patterns`

- **Contexte** : ajout d'une skill générique sur les idiomes Rust.
- **Décision** : normaliser son format et l'intégrer à la matrice de routage en
  priorité 3, derrière les skills d'exploitation et de domaine.
- **Contenu adapté** : tests inline (pas de dossier `tests/`), commandes
  `cargo test --workspace` et `cargo clippy --workspace --all-targets`,
  références vers `codingstandards.md` et `rust-concurrency-orchestrator`.
- **Conséquence** : 22 skills, 52 fichiers dans `.agents/` + `memory-bank/`.
  `codingstandards.md` reste prioritaire en cas de conflit.

## Décisions de migration

### 2026-10-22 — Réécriture Rust sans fallback Python

- **Décision** : les huit étapes sont portées en Rust. `workflow-cli` est
  l'unique entrée ; aucun chemin Rust n'appelle `workflow_scripts/`.
- **Raison** : éviter un couplage permanent aux venvs Python et permettre la
  bascule contrôlée.
- **Conséquence** : le legacy reste la référence de parité et le rollback,
  mais n'est pas une dépendance d'exécution Rust.

### 2026-10-22 — Configuration TOML typée

- **Décision** : `config/config.toml` avec `deny_unknown_fields`, secrets par
  environnement, chemins résolus.
- **Alternatives rejetées** : parseur `.env` maison, JSON de config, variables
  d'environnement pour tout.
- **Conséquence** : les anciens noms d'environnement sont acceptés pendant la
  transition, mais le TOML est la source unique.

### 2026-10-22 — JSON contractuel et arrondis Python

- **Décision** : `serde_json` avec `preserve_order`, écritures atomiques,
  `round2`/`round4`/`round_to`.
- **Raison** : les scripts AE, le CEP et les fixtures dépendent de l'ordre des
  clés et des arrondis.
- **Conséquence** : toute modification de writer doit être couverte par le
  shadow STEP5/6 et `parity/DRIFT.md`.

### 2026-10-22 — Verrou matériel distinct

- **Décision** : `/tmp/workflow-rs-hw.lock` pour STEP2/3/4 ; la sync utilise
  `/tmp/workflow-sync.lock` et refuse un serveur actif.
- **Raison** : éviter toute collision avec le verrou Python
  `/tmp/workflow-hw.lock`.
- **Conséquence** : le serveur refuse le verrou Python et sérialise les étapes.

### 2026-10-23 — Serveur axum sur 5004

- **Décision** : orchestrateur Rust sur `:5004`, CLI en sous-processus,
  contrat de polling conservé.
- **Alternatives rejetées** : réutiliser Flask, lier la logique des étapes au
  serveur, partager la base Python.
- **Conséquence** : le serveur ne contient pas la logique métier ; il sérialise
  et expose l'état.

### 2026-10-23 — Runtime isolé et synchronisation explicite

- **Décision** : racines Rust séparées (`runtime/`), sync explicite avant
  bascule.
- **Raison** : un partage direct des racines aurait exposé Rust aux écritures
  Python et inversement.
- **Conséquence** : `workflow-cli sync` fusionne l'historique, la liste STEP1
  et les archives STEP7 ; les deux serveurs doivent être arrêtés.

### 2026-10-23 — Routes Python-only conservées

- **Décision** : cache, stats, diagnostics et `/api/audio/*` restent dans
  Python (D-015).
- **Raison** : éviter un portage spéculatif sans contrat ni besoin production.
- **Conséquence** : le reverse proxy devra router ces routes vers `:5003`.

### 2026-10-23 — Moniteur webhook Rust activé

- **Décision** : activer `webhook.enabled = true` après import de l'historique
  Python.
- **Préconditions** : 911 URL importées, audit à 0 changement, base non vide,
  `source_dir == downloads`.
- **Conséquence** : un seul moniteur à la fois ; Python doit être arrêté.

### 2026-10-23 — Lanceur Rust

- **Décision** : `scripts/start-rust.sh` pour le raccourci Bureau, avec
  génération des secrets, sync et ouverture Firefox.
- **Usage agent** : préférer `scripts/switch-rust.sh`.

### 2026-10-23 — Windows et CEP abandonnés

- **Décision** : ne pas porter la validation Windows, le JSX ou le CEP en Rust
  (D-022).
- **Raison** : la production cible Linux ; STEP8 est validé en headless avec
  `effectcraft-cli`.
- **Conséquence** : `workflow-bridge` reste un contrat disponible ; le CEP
  continue d'appeler Python.

## Écarts legacy (D-001 → D-024)

| ID | Sujet | Décision Rust |
| --- | --- | --- |
| D-001 | CSV de scènes | Conserver `<stem>.csv` ; l'archive utilise `_scenes.csv`. |
| D-002 | STEP3 sans audio | `SKIP_NO_AUDIO`, aucune sortie, code 0. |
| D-003 | Ordre des clés `_audio.json` | Écrire `fps`/`total_frames` avant `frames_analysis`. |
| D-004 | Dépendances STEP4 | CSV et audio requis ; `--allow-partial-speaking` en test. |
| D-005 | `metadata.video_path` | Conserver le chemin absolu observé. |
| D-006 | Commentaires inline `.env` | Loader TOML, pas de parseur naïf. |
| D-007 | `speaker_stats` STEP3/STEP5 | Projection STEP5 documentée. |
| D-008 | JSON STEP4 non atomique | Écriture atomique. |
| D-009 | Parité STEP2–6 | Fixtures de référence. |
| D-010 | MOV alpha | Préserver à l'octet près. |
| D-011 | VRAM | Distinguer processus et carte. |
| D-012 | Verrou legacy | Verrou Rust distinct. |
| D-013 | Parallélisme serveur | Sérialisation, 409. |
| D-014 | STEP1 manquant | Porté au lot 6. |
| D-015 | Routes Python-only | Cache, stats, diagnostics et `/api/audio/*` restent dans Python ; les routes CSV sont portées. |
| D-016 | Extensions STEP1 | Reproduire le scan effectif (pas `.tar.gz`). |
| D-017 | Noms TAR non UTF-8 | Décodage lossy. |
| D-018 | Ingestion manquante | Portée au lot 8. |
| D-019 | Dossier de téléchargement | `runtime/downloads` isolé. |
| D-020 | Concurrence téléchargements | Borne, statut final après repli. |
| D-021 | Historique vide | Import explicite obligatoire. |
| D-022 | Validation Windows/CEP | Abandonnée. |
| D-023 | Racines séparées | Sync explicite. |
| D-024 | STEP4 `153 Camille` | Ne pas utiliser `--prefer-raw` sur ce projet. |

## Références

- `parity/DRIFT.md` (détails et preuves)
- `docs/LOT-*.md`
- `docs/SYNC-BACKENDS.md`
- `docs/SWITCH-PUBLIC.md`
- `docs/RETENTION.md`
- `docs/VALIDATION-STEP1-8.md`
