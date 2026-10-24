# Journal des décisions

Ce document enregistre les décisions architecturales et techniques durables de
workflow-rs. Les écarts de comportement legacy sont détaillés dans
`parity/DRIFT.md`.

> **Politique de conservation** : ce fichier conserve les décisions depuis le
> début de la migration Rust (2026-10-22). Les décisions legacy antérieures
> restent dans `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.

## 2026-10-24 — Journal serveur Rust enrichi et sans ANSI

- **Contexte** : le lanceur affichait dans le terminal les 40 dernières lignes
  du journal, dont d'anciennes erreurs webhook du 2026-10-23 ; l'opérateur les
  a prises pour des erreurs du batch courant. Le journal serveur était aussi
  plus pauvre que `logs/app.log` du legacy et contenait des séquences ANSI.
- **Décision** :
  1. Désactiver ANSI quand stderr n'est pas un terminal (cas du lanceur), tout
     en conservant la couleur en exécution manuelle ; même règle pour le CLI.
  2. Enrichir `workflow-server.log` : résumé de configuration, requêtes
     mutantes/4xx-5xx avec durée, démarrage/arrêt et heartbeat du moniteur
     webhook, téléchargements planifiés, étapes avec PID et séquences avec
     durée/résultat.
  3. Faire démarrer `tail` après la dernière ligne existante dans
     `start-rust.sh`, pour n'afficher que la session courante.
- **Alternatives rejetées** :
  - Tout passer en `debug` par défaut : trop de bruit toutes les 15 s.
  - Supprimer les anciennes lignes du journal : perte de traçabilité.
- **Conséquences** : `RUST_LOG=debug` reste le levier pour les requêtes GET et
  le détail des passages webhook ; les anciennes lignes ANSI déjà écrites ne
  sont pas réécrites.
- **Validation** : serveur lancé avec stderr redirigé, zéro octet ESC dans le
  nouveau journal, résumé de configuration et téléchargement dry-run visibles.

## 2026-10-24 — Widget système Rust et auto-ouverture des logs

- **Contexte** : un batch STEP1 → STEP8 lancé depuis le frontend Rust a montré
  deux écarts d'interface : le widget Moniteur Système n'affichait pas la GPU,
  et l'overlay de logs s'ouvrait automatiquement à chaque lancement, alors que
  l'opérateur legacy travaillait avec l'auto-ouverture désactivée.
- **Décision** :
  1. `/api/system_monitor` lit la première carte NVIDIA avec `nvidia-smi` et
     expose le contrat GPU existant (`name`, `utilization_percent`, `memory`,
     `temperature_c`), avec `null` si l'outil est absent et
     `{"error": "GPU data not available"}` en cas d'échec.
  2. La copie frontend Rust passe `ui.autoOpenLogOverlay` à `false` par
     défaut ; le toggle « Auto-ouverture des logs » reste disponible et la
     valeur persistée dans `localStorage` reste prioritaire.
- **Alternatives rejetées** :
  - Continuer à router `/api/system_monitor` vers Python : le serveur Rust
    direct `:5004` affichait CPU/RAM sans GPU et dépendait d'un second
    processus pour une métrique machine.
  - Supprimer le toggle : l'opérateur doit pouvoir réactiver l'auto-ouverture.
- **Conséquences** : `/api/system_monitor` peut être routé vers le backend
  actif (voir `docs/SWITCH-PUBLIC.md`) ; la divergence de valeur par défaut
  est consignée dans `systemPatterns.md`.
- **Validation** : endpoint réel sur GTX 1650 (30 %, 0,41/4,0 Go, 44 °C),
  tests serveur GPU, page rendue avec la case décochée.

## 2026-10-24 — Audit croisé du rapport Gemini Web

- **Contexte** : trois rapports d'audit Gemini Web (retirés du dépôt après
  synthèse), produits à partir d'un export Repomix partiellement tronqué.
- **Décision** :
  1. Ne retenir un constat Gemini qu'après reproduction à HEAD ; ne pas
     importer ses sévérités telles quelles.
  2. Retenir cinq nouveaux constats : C9 (arrondi STEP3, P1), R7 (RAR en RAM,
     P1), S9 (secrets d'export, P2), R8 (allocations/regex, P2) et R9
     (duplication, P2).
  3. Enrichir S2 (verrou par défaut et CLI sans `validate_isolation`), A4,
     R1, R6, S7 et C4.
  4. Requalifier SEC-01 et STP1-01 en P2 ; rejeter AUD-06 (déjà corrigé) ;
     classer AUD-01 et AUD-07 hors périmètre.
- **Alternatives rejetées** :
  - Reprendre le P0 SEC-01 : le fichier est ignoré, en mode 600 et le serveur
    est loopback ; la rotation reste une action d'hygiène.
  - Corriger STP1-01 comme une traversée : sous Linux, les antislashs restent
    un nom de fichier littéral.
- **Conséquences** : le rapport passe à 36 constats (1 P0, 6 P1, 29 P2) ; la
  matrice de priorisation est corrigée (l'ancien total de 23 P2 était
  arithmétiquement faux). Aucun code n'est modifié.
- **Validation** : revérification par grep/lecture à HEAD `ae2098d`, simulation
  numérique Python vs formule Rust pour C9, contrôle des fixtures
  `vox_step3`, du frontend et de `parity/README.md`.

## 2026-10-24 — Audit backend Rust

- **Contexte** : audit statique et opérationnel demandé sur les neuf crates,
  sans exécution matérielle ni modification des données de production.
- **Décision** :
  1. Consigner le rapport dans
     `docs/audits/2026-10-24-audit-backend-rust.md` ; les audits futurs suivent
     la même convention `docs/audits/AAAA-MM-JJ-<sujet>.md`.
  2. Classer les constats en P0 (bloquant), P1 (majeur) et P2 (qualité), avec
     preuve `fichier:ligne` et test de non-régression attendu.
  3. Traiter en priorité la course `stop` → `reserve` / `finish` (A1), qui
     autorise deux étapes simultanées et peut écraser l'état de la nouvelle
     exécution.
- **Alternatives rejetées** :
  - Corriger directement pendant l'audit : mélange diagnostic et remédiation,
    et empêche de mesurer l'état initial.
  - Documenter uniquement dans la conversation : perte de traçabilité.
- **Conséquences** : les correctifs P0/P1 (sérialisation générationnelle,
  arrêt propre des sous-processus, portée du verrou, isolation/secret) devront
  être consignés dans ce journal au moment de leur implémentation. Le rapport
  ne modifie ni `runtime/`, ni `/mnt/cache`, ni le dépôt Python legacy.
- **Validation** : `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (233/233), `cargo tree -d`.

## 2026-10-24 — Commandes de parité et état de production

- **Contexte** : les commandes du harnais citaient `coral_env/bin/python` depuis
  la racine du dépôt Rust, où ce venv n'existe pas. Le README présentait aussi
  Python comme le workflow legacy actif, alors que Rust `:5004` est le backend
  de production depuis le 2026-10-23.
- **Décision** :
  1. Les commandes de parité du dépôt Rust utilisent
     `../workflow_mediapipe/coral_env/bin/python`.
  2. Les documents d'état (`README.md`, `CONTRAT-ENVIRONNEMENT.md`,
     `SWITCH-PUBLIC.md`) présentent Rust comme backend de production et Python
     comme référence de parité et rollback.
- **Conséquence** : ne pas recréer `coral_env/` dans ce dépôt ; toute nouvelle
  commande de parité doit pointer vers le venv legacy.

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

## Remédiation de l'audit backend du 2026-10-24

- [2026-10-24 15:00:00] - **Statut `stopping` exposé sans libérer le créneau**
  - `WorkflowState` attribue un `RunId` à chaque exécution ; `StepPhase::Stopping`
    reste actif jusqu'à la fin réelle du sous-processus.
  - `finish_step(step, run_id, …)` ignore une finalisation périmée : un arrêt
    tardif ne peut plus écraser l'état d'une exécution suivante.
  - Le frontend mappe `stopping` sur « Arrêt en cours » et le traite comme un
    état actif.
- [2026-10-24 15:03:00] - **Suppression des clés TOML mortes**
  - Retirées : `server.debug`, `server.polling_interval_ms`,
    `server.system_monitor_interval_ms`, `server.snapshot_interval_s`,
    `server.slow_api_threshold_ms`, `server.gpu_monitoring`,
    `download.chunk_bytes`, `steps.step3.max_workers`, `hardware.tpu_device`,
    `hardware.gpu_mem_limit_mb`, `hardware.require_tpu`.
  - Rupture assumée : `deny_unknown_fields` rejette désormais une ancienne
    configuration ; `config/config.toml` et `config/env.map.toml` sont alignés.
- [2026-10-24 15:06:00] - **`thiserror` aux frontières d'ingestion et de sync**
  - `IngestError` et `SyncError` exposent des résultats typés ; les détails
    internes restent en `anyhow`.
  - Les steps STEP2/3/4/7/8 conservent `anyhow` en interne : dérogation
    documentée D-026, car leurs erreurs traversent le serveur comme code de
    sortie.
- [2026-10-24 15:09:00] - **Verrou matériel unique et écoute explicite**
  - Défaut `/tmp/workflow-rs-hw.lock`, distinct du verrou Python ; tout
    basename `workflow-hw.lock` est refusé, où que soit le chemin.
  - `server.host` non-loopback exige `server.allow_remote = true` ; le
    lancement journalise un avertissement dans ce cas.
- [2026-10-24 15:12:00] - **Isolation partagée et arrêt des groupes**
  - `workflow_core::validate_isolation` est appelé par la CLI et le serveur.
  - Chaque CLI d'étape tourne dans son groupe ; l'arrêt serveur envoie
    `SIGTERM` puis `SIGKILL` au groupe, et `switch-rust.sh stop` tue aussi les
    descendants orphelins.
- [2026-10-24 15:13:18] - **Traversée `/sound-design` documentée**
  - `ServeDir` renvoie 404 au lieu du 403 du contrat Python ; aucune fuite.
    Décision D-025 : conserver le comportement standard et documenter l'écart.

## Compléments A4, A9 et R8 (2026-10-24)

- [2026-10-24 15:25:00] - **Journaux hors du runtime Tokio**
  - Les lignes de journal sont transmises à un writer dédié exécuté via
    `spawn_blocking` ; un disque lent ne bloque plus un worker HTTP.
  - Le canal du writer est borné (512) et le canal amont reste à 1 024.
  - `Runner` journalise début, fin, code de retour, durée et chemin de
    journal ; le middleware HTTP signale les réponses 4xx/5xx.
- [2026-10-24 15:28:00] - **Gestionnaire de signaux pour STEP1–4**
  - `workflow_steps::process` suit jusqu'à 64 enfants longs dans une table
    atomique ; `ffmpeg` et `unrar p` y sont enregistrés.
  - La CLI installe `SIGTERM`/`SIGINT` uniquement pour STEP1–4 : les enfants
    reçoivent `SIGTERM`, puis le CLI sort en `128 + signal`.
  - STEP8 garde son gestionnaire `ctrlc` à double signal ; aucun remplacement.
- [2026-10-24 15:31:00] - **Cache de resize STEP4**
  - Les coefficients `xofs`/`xalpha` et les lignes `row0`/`row1` sont calculés
    une fois par thread et par couple de dimensions, puis réutilisés.
  - L'allocation de l'image de sortie reste inhérente au retour d'un
    `RgbImage` possédé ; elle est la seule allocation par frame conservée.
