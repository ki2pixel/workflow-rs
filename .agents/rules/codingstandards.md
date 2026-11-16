---
trigger: model_decision
description: Standards obligatoires de workflow-rs (Rust) : stack, structure des crates, style, isolation, patrons transverses, qualité et anti-patrons.
---

# workflow-rs — standards de codage et d'architecture

> Référence legacy : `/home/kidpixel/workflow_mediapipe` (rollback strict, aucune modification directe).
> Déviations de parité : `parity/DRIFT.md`. Décisions durables : `memory-bank/decisionLog.md`.

## 1. Stack et conventions Rust

- **Langage** : Rust 1.86 minimum, edition 2021. Workspace de 9 crates.
- **CLI & Serveur** : `workflow-cli` est l'unique point d'entrée opérationnel (`step1`…`step8`, `post`, `qa`, `retention`, `sync`, `webhook`, `email`, `download`, `history`, `config`). `workflow-server` (axum :5004) sérialise et délègue au CLI sans fallback Python.
- **Configuration** : `config/config.toml`, `deny_unknown_fields`. Secrets par environnement (`WORKFLOW_SERVER_SECRET_KEY`, `WORKFLOW_SERVER_WORKER_TOKEN`), jamais dans le TOML.
- **Erreurs** : `thiserror` pour les types publics de bibliothèque (`ConfigError`, `MissionError`, `StepError`, `ArchiveError`, `IngestError`, `SyncError`, `WorkflowServerError`) ; `TpuError` en `Display` manuel. `anyhow` aux frontières CLI/serveur/scripts et en interne des étapes lourdes STEP2/3/4/7/8 (D-026).
- **Zéro `unwrap()`** en production ; `expect()` réservé aux invariants locaux documentés.
- **Logs** : `tracing` sur stderr ; aucun `println!` de debug. Exceptions contractuelles : `TOTAL_*`, `MS_BATCH_*`, `POST_*`, `DIRECT_TEMPLATE_TOTAL`, `ARCHIVE_ONLY_TOTAL`, le passthrough `effectcraft-cli` et les sorties JSON des commandes CLI.
- **Performance** : emprunts prioritaires sur les clones. Éviter les allocations par frame dans STEP2/3/4. Déduplication par `HashSet` (D-071).

## 2. Intégrité des données et JSON

- **Écritures atomiques** : fichier temporaire voisin, `flush`, puis `rename` (applicable aux étapes, au post et au QA).
- **JSON contractuel** : conserver strictement l'ordre d'insertion (`preserve_order`). Nouveaux champs additifs en fin d'objet avec `#[serde(default)]` (`OverlayRegion.kind`, `source`, `known_absent`).
- **Arrondis** : utiliser `workflow_schema::round::{round2, round4, round_to}` (compatible Python), jamais `f64::round`.

## 3. Patrons transverses et isolation (D-071)

- **Isolation stricte** : `paths.base` est la racine de travail (`WORKFLOW_PATHS_BASE`). `projects`, `archives`, `logs`, `downloads`, `history_db` et `step1.source_dir` en dérivent.
- **Garde réseau** : `server.allow_remote = true` exige une `server.secret_key` non vide, non blanche et non préfixée `dev-`.
- **Lecture seule des entrées** : le projet `.ecproj` brut, les MP4 sources et `docs/` ne sont jamais modifiés. Seul `<stem>_assist.ecproj` est publié.
- **Verrou matériel** : `HardwareLock` sur `hardware.lock_path` (`/tmp/workflow-rs-hw.lock`) pour STEP1 et STEP2/3/4. Verrou Python interdit.
- **Purge d'orphelins** : strictement limitée aux motifs pipeline `.part`, `.tmp`, `.*.tmp.*` et `_temp_*`.

## 4. Pipeline et étapes

Les spécifications détaillées sont réparties dans les documents et modules dédiés :
- **Spécifications normatives du pipeline** : `.agents/rules/pipeline-standards.md` (détail STEP1 à STEP8, template bypass SDM/LPHDF, post-traitement A5/A6/A7, sidecars mission schema 2, repli central overlays, contrôle qualité QA 1.5.0 et masques MOV alpha).
- **Skills d'exploitation** : `workflow-operator`, `step3-audio-orchestrator`, `step4-tracking-ops`, `onnx-runtime-cuda-expert`, `csv-monitoring-sme`, `after-effects-bridge`.
- **Documentation de lot** : fiches `docs/lots/LOT-*.md` et écarts intentionnels `parity/DRIFT.md`.

## 5. Qualité et anti-patrons absolus

- **Validation obligatoire avant fin de tâche** :
  ```sh
  cargo fmt --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  cargo build --release -p workflow-server -p workflow-cli
  ```
- **Anti-patrons stricts** :
  - Modifier ou écrire dans `/home/kidpixel/workflow_mediapipe`.
  - Écrire ou supprimer dans `/mnt/cache` depuis un test, ou utiliser `runtime/` comme brouillon.
  - Exécuter simultanément les moniteurs Python (:5003) et Rust (:5004) ou les étapes matérielles.
  - Écraser, renommer ou altérer le `.ecproj` brut pendant un post-traitement.
  - Faire écrire le QA dans le MP4, le `.ecproj` ou `docs/`, ou l'exécuter dans STEP8.
  - Porter les routes Python-only (`/api/audio/*`, cache, stats, diagnostics) sans décision explicite (D-015).
  - Élargir la purge des temporaires hors des motifs pipeline ou utiliser des chemins absolus neutralisant `WORKFLOW_PATHS_BASE`.
  - Nettoyer les quirks documentés dans `parity/DRIFT.md` (D-001, D-002, D-004, D-016).
