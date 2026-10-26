# Journal des décisions

Ce document enregistre les décisions architecturales et techniques durables de
workflow-rs. Les écarts de comportement legacy sont détaillés dans
`parity/DRIFT.md`.

> **Politique de conservation** : ce fichier conserve les décisions depuis le
> début de la migration Rust (2026-10-22). Les décisions legacy antérieures
> restent dans `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.

## 2026-10-24 — Rapport Deep Search GUI : R1 non reproduit, PoC Tauri corrigé

- **Contexte** : le rapport de recherche externe
  `docs/audits/Analyse_Architecturale_GUI_Linux.md` (E3) a été analysé et
  confronté au code, aux études E1/E2/C et à des mesures locales. L'analyse
  critique et les vérifications sont consignées dans
  `docs/audits/2026-10-24-analyse-rapport-deep-search-gui-linux.md`.
- **Décision** :
  1. L'**architecture reste inchangée** : modèle 1, serveur headless
     autoritaire, GUI client, Tauri A-Strict en phase 1, GTK4/Relm4 en repli,
     C2 rejeté, socle C3 (`workflow-desktop-services`) mutualisé.
  2. Le **PoC Tauri passe en premier, 5 jours**, au lieu d'un PoC à deux
     pistes de 15–20 jours. GTK4 n'est financé que si Tauri échoue sur une
     porte mesurable.
  3. **R1 (écran blanc/SIGSEGV DMA-BUF) est reclassé** : le bug amont
     `block/buzz#3654` est réel, mais il n'a pas été reproduit sur le chemin
     par défaut de WebKitGTK 2.50.4/Ubuntu 22.04/NVIDIA 535, y compris avec
     `WEBKIT_DISABLE_DMABUF_RENDERER=1`, un calque de composition et cinq
     bascules d'espace de travail. En forçant le chemin DMA-BUF
     (`WEBKIT_FORCE_DMABUF_RENDERER=1`), le contenu devient vide avec des
     erreurs `AcceleratedSurface` : le risque est réel sur ce chemin, mais le
     chemin par défaut patché est stable. Le patch Ubuntu rend `FORCE_SHM`
     inopérant (`isNVIDIA()` retourne avant sa lecture). Le PoC garde le
     profil forcé comme canari et le profil par défaut comme mode livré, et
     confirme sur 20 cycles et 2 h.
  4. Les **seuils VRAM/RSS deviennent relatifs** : aucun OOM CUDA sur 5 runs
     de STEP2 et VRAM totale < 3 900 Mio, plutôt qu'un seuil absolu de
     50 Mio. La baseline est Firefox + bureau (~1 030 Mio RSS, ~1 000 Mio
     VRAM), pas une application native idéale.
  5. Les **10 000 lignes de logs** sont un lot **serveur** (SSE/fichier), pas
     un critère de choix toolkit. AT-SPI n'est une exigence que si
     l'opérateur utilise Orca quotidiennement.
  6. La **MSRV est portée par le crate GUI** : `workflow-gui` peut exiger
     1.90 (Tauri 2.12) ou 1.93 (Relm4 0.11) sans changer la MSRV 1.86 des
     neuf crates existantes. Si une exception est interdite, le repli correct
     est `gtk4 0.10.3` (1.83) + `relm4 0.10.0` (1.85), et non
     `gtk4 0.11.5`/`relm4 0.10.1` (1.92) comme l'écrit E3.
  7. Le **token A-Strict** doit être spécifié : lecture de `/` et parsing de
     la balise `worker-token`, ou route de session dédiée ; pas de lecture de
     `config/config.toml` depuis le GUI.
- **Alternatives rejetées** :
  - Financer GTK4 en parallèle sans preuve d'échec Tauri : coût 70–95 j-h
    contre 12–18 j-h, alors que le test local ne montre pas de blocage.
  - Conserver les gates absolus de E3 (50 Mio VRAM, 300 Mio RSS) : ils
    rejettent une solution déjà meilleure que la baseline Firefox.
  - Traiter les 10 000 lignes comme un argument GTK4 : le serveur borne à
    300 lignes, le changement est serveur dans les deux cas.
  - Reprendre les MSRV de repli de E3 : versions fausses, vérifiées sur
    crates.io.
- **Conséquences** :
  - Nouveau document d'analyse :
    `docs/audits/2026-10-24-analyse-rapport-deep-search-gui-linux.md`.
  - Le PoC Tauri doit mesurer la VRAM avec `nvidia-smi pmon`, la VRAM totale
    et `fuser /dev/nvidia0`, pas avec `--query-compute-apps` (contextes
    compute uniquement).
  - Le crate `workflow-desktop-services` est créé dès J1, pour éviter une
    réécriture si GTK4 est choisi plus tard.
  - Aucun code fonctionnel, aucun contrat HTTP, aucun accès à `runtime/` ou
    `/mnt/cache` n'a été modifié.
- **Validation** : `MiniBrowser` 2.50.4, trois profils, page statique et page
  avec calque de composition, cinq bascules d'espace de travail ; versions
  vérifiées par `dpkg-query`, `apt-cache policy`, `crates.io` et le changelog
  Debian. Le test local est un test de fumée, pas une preuve de parité ou de stabilité
  longue durée.

## 2026-10-24 — Confrontation des études GUI : faits corrigés, choix conditionnel

- **Contexte** : une étude indépendante
  (`docs/audits/Migration-Architecture-GUI-Linux-workflow-rs.md`, E2) recommande
  GTK4/Relm4 et conteste l'orientation Tauri de
  `docs/ETUDE-GUI-NATIVE-LINUX.md` (E1). Les deux documents ont été confrontés
  ligne à ligne et vérifiés contre le code à HEAD ; le résultat est consigné
  dans `docs/audits/2026-10-24-confrontation-etudes-gui-linux.md`.
- **Décision** :
  1. L'**architecture est acquise** : modèle 1 (client GUI découplé), serveur
     headless autoritaire, GUI jamais parent du serveur, aucun accès
     CUDA/TPU/verrou depuis le processus graphique. Les deux études convergent.
  2. Le **choix du toolkit reste conditionnel** à une exigence à arbitrer :
     « natif » signifie-t-il *application de bureau* (Tauri T1 suffit) ou
     *widgets natifs + AT-SPI + VRAM nulle* (GTK4/Relm4) ?
  3. La **règle de décision** est fixée : délai court et parité visuelle →
     Tauri T1 (12–18 j) ; widgets natifs, isolation VRAM et logs virtualisés →
     GTK4/Relm4 (70–95 j). Un PoC à deux pistes (15–20 j) est recommandé si le
     budget le permet.
  4. Les **corrections factuelles** sont appliquées à E1 (révision 1.1) :
     `GtkFileLauncher` remplacé par `gio::AppInfo` (GTK 4.6), RSS GTK4 révisé,
     `GSK_RENDERER=cairo`, NVML en phase 2, badges `/mnt/cache` et
     `/dev/apex_0`, modèle `LogEntry` avec coalescence 16 ms, SSE en phase 3.
  5. Les **affirmations erronées d'E2** ne sont pas reprises :
     `/api/system_monitor` existe bien en Rust (`http.rs:82`, `614-669`) ;
     le frontend n'appelle jamais `/sequence/status` ; le polling réel est de
     2 req/s par étape active (le chemin 200 ms n'est jamais activé) ;
     `clear_disk_cache` est du code mort dans `downloadsListManager.js` ;
     les tokens CSS web ne sont pas réutilisables tels quels avant GTK 4.16 ;
     Iced n'a pas d'AT-SPI (issue #552) ; une parité GTK4 ne tient pas en
     4 semaines.
- **Alternatives rejetées** :
  - Suivre E2 telle quelle : les prémisses fausses (D-015, 5 req/s,
    `/sequence/status`) justifient à tort une réécriture complète.
  - Suivre E1 sans révision : `GtkFileLauncher` est indisponible sur Ubuntu
    22.04 et les optimisations Cairo/NVML/SSE manquaient.
  - Trancher la techno sans arbitrer l'exigence : aucun des deux documents ne
    peut désigner un gagnant unique, le classement dépend de la pondération.
- **Conséquences** :
  - E1 passe en version 1.1 ; la recommandation par défaut reste Tauri T1,
    mais elle est explicitement conditionnelle.
  - Le prochain jalon est l'arbitrage « widgets natifs : exigence ou
    confort ? », puis le PoC correspondant (5–8 j Tauri ou 8–10 j GTK4).
  - Aucun code fonctionnel, aucun contrat HTTP, aucun accès à `runtime/` ou
    `/mnt/cache` n'a été modifié par la confrontation.
- **Validation** : vérifications à HEAD (greps, versions de paquets, docs GTK,
  issues amont) listées en annexe A du document de confrontation. Aucune
  mesure de prototype supplémentaire.

## 2026-10-24 — Interface desktop Linux : coque native, client du serveur

- **Contexte** : étude de faisabilité demandée pour remplacer l'interface web
  `assets/web` (rendue par Minijinja, servie sur `:5004`) par une application
  desktop Linux X11/Wayland. Contraintes : ne pas casser l'architecture
  serveur, ne pas modifier le legacy Python, préserver les contrats
  JSON/CSV/HTTP, ne pas écrire dans `/mnt/cache`. Le constat de départ contient
  trois hypothèses fausses : débit de logs de « milliers de lignes/s », usage
  de `POST /run_custom_sequence` par le frontend, et existence d'un
  gestionnaire de cache Rust.
- **Décision** (orientation, subordonnée aux critères de sortie du PoC) :
  1. La cible est une application desktop **cliente** de `workflow-server`
     (modèle 1). L'orchestrateur n'est pas fusionné dans le binaire GUI :
     une fenêtre n'est pas un démon, et un crash graphique ne doit pas emporter
     le webhook, l'ingestion ni les séquences.
  2. Le premier lot utilise **Tauri v2 en mode URL distante** (fenêtre sur
     `:5004`), avec les fonctions de bureau côté Rust (tray, instance unique,
     notifications, `xdg-open`). Aucun changement de contrat HTTP, aucun
     changement de `assets/web`, aucune dépendance npm.
  3. L'option native de repli est **GTK4 + gtk4-rs/Relm4**, évaluée seulement
     si le PoC échoue (artefacts WebKitGTK/NVIDIA, RSS > 500 Mo, latence
     d'entrée > 100 ms, crash pendant STEP2).
  4. Iced et egui sont écartés pour cette UI (accessibilité Linux insuffisante,
     widgets timeline/logs à écrire). Slint reste documenté mais non retenu
     (réécriture complète, MSRV 1.92, licence à trancher).
  5. Les séquences doivent passer côté serveur (`POST /run_custom_sequence`,
     `GET /sequence/status`, `POST /sequence/stop`) pour survivre à la
     fermeture de la fenêtre ; c'est un changement de comportement volontaire
     à couvrir par un test.
  6. Le serveur reste supervisé indépendamment du GUI (`switch-rust.sh start`
     pour le PoC, systemd user pour le MVP) ; le GUI n'est jamais son parent.
- **Alternatives rejetées** :
  - Modèle monolithique (GUI + `Runner` dans le même processus) : duplication
    du cycle de vie, double moniteur webhook possible, perte de l'isolation
    des crashs, réimplémentation du verrou et des groupes de processus.
  - Tauri T2 (assets embarqués) : le template dépend de Minijinja
    (`steps_config`, `worker_token`), il faudrait dupliquer le rendu, ajouter
    CORS et élargir la CSP.
  - Tauri T3 (commandes IPC à la place de `fetch`) : pousse mécaniquement vers
    le modèle monolithique.
  - Iced/egui pour la parité visuelle : accessibilité et composants
    personnalisés insuffisants.
  - Navigateur en mode `--app` : moins d'une heure de travail, mais pas de
    tray, pas d'instance unique, pas de notifications fiables, dépendance au
    profil Firefox.
- **Conséquences** :
  - Nouvelle crate `workflow-desktop` prévue ; les neuf crates existantes et
    `assets/web` restent inchangés en phase 1.
  - Le MSRV du workspace devra passer de 1.86 à 1.90 (Tauri 2.12) si le PoC
    est retenu ; la décision du 2026-10-24 sur le rapport Deep Search précise
    qu'elle peut être portée par le seul crate GUI.
  - Les paquets de développement (`libwebkit2gtk-4.1-dev`, `libxdo-dev`,
    `libayatana-appindicator3-dev`) sont absents de la machine mais
    installables ; les bibliothèques runtime sont déjà présentes.
  - La documentation de référence est `docs/ETUDE-GUI-NATIVE-LINUX.md`.
- **Validation** : aucune exécution de prototype à ce stade. Les mesures
  (logs 5,4 l/s max, 300 lignes bornées, `nvidia-smi` 10–20 ms, VRAM bureau
  ~933 Mio) et les versions de frameworks sont consignées dans l'étude. Le
  PoC devra démontrer les seuils du §8.3 avant toute décision définitive.

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
  `cargo test --workspace` (233/233 à l'audit), `cargo tree -d`.

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

## Écarts legacy (D-001 → D-027)

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
| D-025 | `/sound-design` hors racine | 404 standard de `ServeDir`, pas de middleware dédié. |
| D-026 | Frontières d'erreur | `thiserror` pour ingest/sync ; `anyhow` interne STEP2/3/4/7/8. |
| D-027 | Arrêt manuel CLI | Groupes de processus, SIGTERM puis SIGKILL ; STEP8 garde son gestionnaire. |

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

## 2026-10-25 — A-Strict Tauri : neutraliser aussi le handler IPC natif de wry

- **Contexte** : le stub `Builder::invoke_system` neutralise
  `window.__TAURI_INTERNALS__.postMessage`, mais wry expose en plus
  `window.ipc` (objet gelé par `Object.defineProperty`) et
  `window.webkit.messageHandlers.ipc`. L'audit J1 a montré que
  `window.ipc.postMessage(...)` résolvait encore, alors que `invoke(...)`
  était déjà rejeté.
- **Décision** : dans le script d'initialisation `A_STRICT_SCRUB`, remplacer
  `window.webkit.messageHandlers.ipc.postMessage` par une fonction qui lève
  `IPC natif désactivé (A-Strict)`. L'objet `window.ipc` étant gelé et non
  configurable, on ne peut pas le supprimer ; on neutralise son handler natif.
- **Conséquences** : `window.__TAURI_INTERNALS__`, `window.ipc` et
  `window.isTauri` restent présents (injection wry non configurable), mais
  `invoke`, `postMessage`, `window.ipc.postMessage` et le handler natif
  échouent tous. Aucune capability, commande ou plugin n'est exposé.
- **Validation** : audit J1 réel sur `:5005` ; les quatre chemins d'envoi sont
  rejetés, `tauri=undefined`, `token-check=ok`. Rapport
  `docs/audits/2026-10-25-poc-tauri-j1.md`.

## 2026-10-25 — J2 : rendu stable, canari DMA-BUF et `FORCE_SHM`

- **Contexte** : stress-test de rendu du PoC Tauri A-Strict sur 35 lancements
  et 125 bascules de bureaux GNOME, quatre profils WebKit, avec vérification
  sticky et restauration `gsettings`.
- **Constat** :
  1. `default` (20 lancements, 100 bascules) et `disable-dmabuf`
     (5 lancements, 25 bascules) sont stables : aucun écran blanc, noir ou
     solide, aucun SIGSEGV, aucun orphelin.
  2. `force-dmabuf` est un canari reproductible : capture finale solide
     (`mean=0,172549`, une couleur) et 183 lignes
     `AcceleratedSurface was unable to construct a complete framebuffer`.
     La détection par capture et par stderr fonctionne.
  3. `force-dmabuf-shm` rend normalement dans ce run, sans erreur
     `AcceleratedSurface`. Le constat E1 « `FORCE_SHM` inopérant » n'est donc
     pas reproduit ici ; il reste à confirmer en J3/J4 et ne justifie pas de
     changer le profil de production.
  4. Le profil par défaut reste le mode livré ; les profils forcés restent des
     canaris de diagnostic.
- **Conséquences** : le harness `scripts/poc/j2-stress.py` devient la référence
  du stress-test ; il vérifie `_NET_WM_STATE_STICKY` après un délai, isole
  chaque GUI dans sa session et restaure `dynamic-workspaces` et
  `num-workspaces` dans un `trap`. Les captures ne doivent pas être considérées
  comme valides si `sticky=false`.
- **Validation** : run final `j2-20261025-021328` ; 35/35 audits
  `token-check=ok`, 35/35 sticky, 125/125 bascules, 0 orphelin, `gsettings`
  avant/après identiques, ports libres. Rapport
  `docs/audits/2026-10-25-poc-tauri-j2.md`.

## 2026-10-25 — J3 : Tauri + STEP2 CUDA sur 4 Gio

- **Contexte** : 5 runs STEP2 TransNetV2 CUDA avec le GUI Tauri ouvert, sur
  une vidéo `testsrc2` 30 s 1280×720, après une baseline Firefox snap
  fraîche et une baseline GUI.
- **Mesures** :
  1. Firefox : RSS médian 1 283,4 Mio, PSS 706,1 Mio, VRAM +174 Mio.
  2. Tauri : RSS médian 409,5 Mio, PSS 226,0 Mio, VRAM +2 Mio.
  3. STEP2 : RSS max 803,6 Mio, PSS 798,7 Mio.
  4. Pic VRAM global 2 392 Mio, libre min 1 508 Mio, 0 OOM, 0 échantillon
     ≥ 3 900 Mio.
- **Constats** :
  - le GUI n’est pas le facteur limitant VRAM ;
  - les 5 CSV sont identiques octet pour octet (`7d3897…`) ;
  - le contrat des trois processus GUI tient sur tous les échantillons ;
  - le profil Firefox snap doit vivre sous
    `/home/kidpixel/snap/firefox/common/` car `/tmp/opencode` n’est pas
    visible dans son namespace ;
  - le fichier `/tmp/workflow-rs-hw.lock` persiste après STEP2 : l’acceptation
    teste `flock`, pas l’existence du fichier.
- **Conséquences** : aucun levier de contingence n’est nécessaire ; les
  niveaux `disable-dmabuf`, batch 4, cuDNN heuristique et batch 2/1 restent
  documentés pour J4/J5. Le monitor J3 devient la référence pour les mesures
  RAM/VRAM.
- **Validation** : run `j3-20261025-025201`, acceptance globale `true`.
  Rapport `docs/audits/2026-10-25-poc-tauri-j3.md`.

## 2026-10-25 — J4 : tray D-Bus, StatusWatcher 1 Hz et endurance 2 h

- **Contexte** : tray natif sous GNOME, polling d'état à 1 Hz sans solliciter
  la WebView, matrice d'états, fermeture vers le tray et soak de 2 h.
- **Décisions** :
  1. La politique du tray (`TrayState`, `TrayStatus`, `TrayVisualKey`,
     `TrayPresenter`) reste dans `workflow-desktop-services`, sans toolkit ;
     l'adaptateur `tauri::tray` / `tray-icon` / `libayatana-appindicator3`
     reste dans l'hôte Tauri.
  2. `StatusWatcher` publie des `StatusSnapshot` dans un
     `tokio::sync::watch` à un slot ; tick de 1 s ancré sur une échéance
     absolue, `max_errors = None` pour le tray, statistiques ticks/succès/
     erreurs/skipped.
  3. Fermeture vers le tray par défaut ; `--no-close-to-tray` restaure la
     fermeture native pour les scripts ; `--no-tray` utilise `NoopTray`.
  4. Notifications et audio opt-in (`--notifications`, `--audio`,
     `--sound-dir`) ; `TransitionGate` déduplique par `terminal_seq` ;
     `RodioAudio` lit les MP3 locaux.
  5. Single-instance différé au MVP.
  6. `libayatana-appindicator3-dev` installé (0.5.90) ; l'extension
     `ubuntu-appindicators@ubuntu.com` et `org.kde.StatusNotifierWatcher`
     restent les prérequis.
- **Mesures** :
  1. Matrice 5 min `j4-matrix-final-20261025-050025` : Idle, Running,
     Success, Error, Stopping, Offline, reconnexion et fermeture/restauration
     par le menu D-Bus validés ; durée 299,028 s ; item D-Bus
     `:1.1662/org/ayatana/NotificationItem/tray_icon_tray_app_main`.
  2. Soak 7 200 s `j4-soak-20261025-025624` : dérive p95 0,310 ms, hôte
     +0,03 Mio, WebKitNetwork +22,06 Mio, WebKitWeb +3,35 Mio, PSS GUI
     +25,46 Mio, pente GUI 0,221 Mio/min, VRAM pic 199 Mio, 0 OOM, 0 erreur
     watcher, D-Bus stable.
- **Décision ratifiée (2026-10-25)** : l'enveloppe GUI est recalibrée pour
  être cohérente. Le plafond PSS passe de 24 à 32 Mio, aligné sur le plafond
  RSS ; la pente maximale passe de 0,20 à 0,267 Mio/min (32 Mio / 120 min).
  Justification : pour une croissance privée, PSS ≈ RSS ; un plafond PSS plus
  strict que le plafond RSS est incohérent, et la pente doit dériver du même
  budget. Les autres seuils (hôte, WebKit par processus, serveur, mémoire,
  logs, tray, VRAM) sont inchangés.
- **Constat** : l'hôte (watcher/tray) est plat ; la croissance est portée par
  `WebKitNetworkProcess` (+22,1 Mio) et attribuée au polling du frontend
  legacy qui continue en arrière-plan (`/api/system_monitor` 5 s,
  téléchargements 1 s).
- **Conséquences** : le tray, le watcher et l'enveloppe GUI recalibrée sont
  conformes ; l'acceptation est recalculée sur les mêmes mesures, sans
  modification du code, du frontend ni des données. La croissance réseau
  WebKit reste un point de vigilance MVP (pause du polling page masquée à
  évaluer en J5). Le single-instance reste différé au MVP.
- **Validation** : matrice `matrix_ok=true`, soak recalculé `soak_ok=true`
  avec les seuils recalibrés, `cleanup_ok=true` ; 316 tests workspace
  (280 pipeline + 36 services), fmt/clippy verts, build release.
  Rapport `docs/audits/2026-10-25-poc-tauri-j4.md`.

## 2026-10-25 — J5 : paquet .deb, supervision systemd user et décision Tauri

- **Contexte** : clôture du PoC Tauri A-Strict par le packaging `.deb`, la
  validation de l'extraction sans root et la supervision du serveur en PoC
  (`switch-rust.sh`) et en MVP (`systemd --user`), sans que le GUI soit
  jamais le parent du serveur.
- **Décision** : **Tauri A-Strict est conservé pour le MVP.** Le client reste
  un client HTTP de `workflow-server` ; le serveur, les modèles et les
  dépendances CUDA/TPU restent hors du paquet GUI.
- **Paquet** : `workflow-manager_0.1.0_amd64.deb`, construit par Tauri CLI
  2.12.4, 5 857 424 octets (5,59 Mio), 15 227 Kio installés. Binaire
  `/usr/bin/workflow-gui-tauri` strippé, `.desktop`
  `workflow-manager.desktop`, icônes 32/128/256/512, dépendances
  `libasound2`, `libssl3`, `libayatana-appindicator3-1`,
  `libwebkit2gtk-4.1-0`, `libgtk-3-0`. Aucun serveur, config, secret ou
  chemin de production dans le paquet.
- **Télémétrie** : défaut conforme XDG
  (`$XDG_STATE_HOME/workflow-manager` ou
  `~/.local/state/workflow-manager`) au lieu de `/tmp/opencode/poc-gui`.
- **Supervision** :
  1. PoC : `scripts/switch-rust.sh` (check/start/stop), serveur lancé avec
     `setsid`, PID file, `/api/ping` ; testé sur `:5005`.
  2. MVP : `packaging/systemd/workflow-server.service.in` et
     `scripts/install-systemd-user.sh` ; secrets 0600 par `EnvironmentFile`,
     préflight port/webhook, `Restart=on-failure`, `KillMode=mixed`,
     `ProtectSystem=full`, `PrivateTmp=false` (verrou matériel partagé),
     `ProtectHome` désactivé (caches CUDA/EffectCraft).
- **Validation** : extraction et exécution sans root, `desktop-file-validate`
  rc=0, `systemd-analyze --user verify` rc=0, GUI chargé avec tray `Idle`,
  serveur PPID 1644 (`systemd --user`), serveur toujours actif après
  fermeture du GUI, préflight port occupé et secrets 0644 refusés,
  désinstallation idempotente, 316 tests, fmt/clippy/build verts.
- **Conséquences** : le PoC J1–J5 est clos. Le MVP devra traiter la
  croissance `WebKitNetworkProcess` (polling du frontend legacy en
  arrière-plan), le single-instance, l'installation réelle du `.deb` et les
  runs STEP2/STEP8 sous l'unité systemd.
- **Validation documentaire** : `docs/PACKAGING-GUI.md` et
  `docs/audits/2026-10-25-poc-tauri-j5.md`.

## 2026-10-25 — MVP : polling masqué et gel de workflow-desktop-services

- **Contexte** : le soak J4 avait mesuré +22,06 Mio sur
  `WebKitNetworkProcess`, causés par `/api/system_monitor` (5 s) et
  `/api/csv_downloads_status` (1 s) qui continuaient quand la WebView était
  minimisée dans le tray. Le crate partagé devait être revu avant d'être figé
  comme socle du client Tauri et d'un futur repli GTK4.
- **Décision 1 — suspension opt-in** : `PollingManager` accepte
  `pauseWhenHidden`. Seuls `systemMonitor` et `localDownloadsStatus`
  l'activent ; les pollers d'étapes et de séquence restent actifs pour ne pas
  bloquer l'auto-mode. Les intervalles et backoffs sont suspendus, puis
  l'intervalle est recréé avec un rafraîchissement immédiat au focus.
- **Décision 2 — cache** : `Cache-Control: no-store` sur `/static/` et
  imports `PollingManager.js?v=20261025a` dans ses sept consommateurs. Le
  `cache_buster` de `main.js` ne couvre pas les imports ES internes ; un
  premier essai avec `no-cache` seul a montré que WebKit réutilisait encore
  l'ancien module après un HEAD 200. Changer la clé d'URL est la garantie
  effective ; incrémenter le suffixe à chaque modification du module.
- **Décision 3 — gel du crate** : `workflow-desktop-services` 0.1.0 est
  gelé. Trois corrections P2 appliquées avant gel :
  1. `tray.rs` : une séquence active prime sur `last_terminal` pour éviter
     un faux « Succès » entre deux étapes ;
  2. `polling.rs` : le watcher s'arrête si le récepteur `watch` disparaît ;
  3. `client.rs` : les identifiants dans l'URL loopback sont refusés.
  `ServicesError` devient `#[non_exhaustive]` pour autoriser des variantes
  additives. Toute évolution de `polling.rs`/`tray.rs` rejoue la matrice J4
  et, si la charge change, le soak 2 h.
- **Validation** : live 0 requête `system_monitor`/`csv_downloads` sur 8 s
  masquées et reprise au focus ; soak 10 min `mvp-frontend-soak10-20261025`
  (600 échantillons, 100 % masqué, dérive p95 0,323 ms, hôte +4 Kio, GUI PSS
  +73 Kio, réseau 0 Kio, watcher 602 ticks/0 erreur, VRAM pic 368 Mio,
  acceptation `true`) ; 38 tests services, 319 tests workspace, fmt/clippy/
  build verts.
- **Conséquences** : la cause de la dérive J4 est neutralisée ; le soak 2 h
  de confirmation reste à rejouer avant la bascule publique. Le
  single-instance, l'installation réelle du `.deb` et les runs STEP2/STEP8
  sous systemd restent au MVP.
- **Validation documentaire** : `docs/MVP-FRONTEND-POLLING.md`,
  `docs/audits/2026-10-25-revue-workflow-desktop-services.md`,
  `memory-bank/systemPatterns.md`.

## 2026-10-26 — MVP J1 : `.deb`, single-instance, cache CUDA

- **Contexte** : le PoC J5 avait validé le paquet sans installation réelle, et
  le single-instance, l'installation du `.deb` et les runs CUDA sous unité
  systemd restaient au MVP. Les quatre choix du protocole ont été ratifiés par
  la direction.
- **Décision 1 — identité D-Bus** : nom `org.workflow.desktop`, objet
  `/org/workflow/desktop`, interface `org.freedesktop.Application`,
  `DBusActivatable=false`. Le `.desktop` reste `workflow-manager.desktop` ;
  l'activation par le bus exigerait un renommage et un service D-Bus, différé.
- **Décision 2 — arguments** : le second lancement transmet `argv`, `cwd`,
  `server-url` et `explicit-server-url`. Même URL : focus strict. URL
  explicitement divergente : `InvalidArgs` immédiat. Sans URL explicite, le
  lancement depuis le menu est accepté même si l'instance active utilise un
  port de test.
- **Décision 3 — cache CUDA** : `CacheDirectory=workflow-rs/cuda-compute`,
  `CUDA_CACHE_PATH=%C/workflow-rs/cuda-compute`,
  `CUDA_CACHE_MAXSIZE=1073741824` (1 Gio). `%C` vaut `$XDG_CACHE_HOME` ou
  `~/.cache`. Il n'existe pas de `NVVM_CACHE_PATH` : le cache NVVM est le
  cache JIT CUDA.
- **Décision 4 — cycle du paquet** : le `.deb` de référence J5 a été installé,
  validé (GIO/GNOME), puis désinstallé. Le paquet MVP final a été reconstruit,
  installé et validé sur le binaire `/usr/bin/workflow-gui-tauri`.
- **Décision 5 — focus X11** : `present_with_time(0)` de GTK ne déminimise pas
  de façon fiable sous GNOME. `x11_activation.rs` envoie `_NET_ACTIVE_WINDOW`
  avec l'indication « pager » (2), `check()` + `sync()`, après un délai de
  500 ms. Le repli Wayland reste `set_focus()`.
- **Validation** : paquet 6 595 548 octets / 17 159 Kio, SHA-256
  `66da7031601e66591a27c9fa03bbb9cee1ae9f46a5022d3381dc5640201439f8` ;
  introspection 1 091 octets ; second lancement 53 ms, un processus, fenêtre
  focalisée ; URL divergente `InvalidArgs` ; activation serveur arrêté ;
  STEP2 7 s puis 2 s, CSV identique ; cache 4 096 o/0 → 15 794 o/2 fichiers
  après sonde PTX, persistant après restart ; 325 tests, fmt/clippy/build
  verts.
- **Conséquences** : le jalon MVP J1 est clos. Restent le soak 2 h, le run
  STEP8 sous unité, puis l'installation de l'unité de production lors de la
  bascule publique.
- **Validation documentaire** : `docs/PACKAGING-GUI.md`,
  `docs/audits/2026-10-26-mvp-j1-deb-single-instance-systemd.md`,
  `memory-bank/activeContext.md`, `memory-bank/progress.md`.

## 2026-10-26 — MVP J2 : soak, STEP8 systemd, bascule production

- **Contexte** : clôturer le MVP avant le push de `main`. Les quatre arbitrages
  du protocole J2 ont été ratifiés : P1 obligatoire, push sur le SHA final
  (option A), `systemctl --user restart` au lieu d'un reboot, codification des
  runbooks.
- **Décision 1 — secrets** : `runner.rs` retire
  `WORKFLOW_SERVER_WORKER_TOKEN`, `WORKFLOW_SERVER_SECRET_KEY`,
  `FLASK_SECRET_KEY` et `INTERNAL_WORKER_COMMS_TOKEN` de l'environnement du
  `workflow-cli` enfant. Les variables CUDA/cache restent transmises.
- **Décision 2 — STEP8** : le run de validation utilise une unité
  `workflow-server-step8-test.service` isolée, un port `5007`, un manifeste
  STEP7 et un verrou matériel sous `/tmp/opencode`. Le `.ecproj` est ouvert
  par `effectcraft-cli` pour vérifier 1080×1920.
- **Décision 3 — soak** : gate GO MVP strict `delta WebKitNetworkProcess
  ≤ 0 Kio` en plus des gates J4. Le gate `gui_proc_count == 1` a été remplacé
  par « un seul PID hôte + membres de session stables » : le CSV compte le
  processus hôte et les deux processus WebKit (3).
- **Décision 4 — production** : smoke avec `[webhook] enabled = false`, puis
  installation de l'unité avec `config/config.toml` (`enabled = true`,
  `dry_run = false`). Secrets `~/.config/workflow-rs` 0700 et
  `launcher.env` 0600. `systemctl --user enable` et `Linger=yes`.
- **Validation** : soak 7 200 échantillons, réseau 0 Kio, GUI +1,68 Mio,
  serveur 0 Kio, 0 OOM ; STEP8 `.ecproj` 26 726 octets ; webhook réel
  `running`, 462 enregistrements, `planned=0` ; 326 tests, fmt/clippy/build
  verts.
- **Conséquences** : `main` peut être poussé après `GO PUSH MAIN <SHA>`.
  Le manifeste STEP7 de production reste à régénérer avant un STEP8 réel.
- **Validation documentaire** : `docs/SWITCH-SYSTEMD.md`,
  `docs/audits/2026-10-26-mvp-j2-soak-step8-production.md`,
  `docs/PACKAGING-GUI.md`, `docs/SWITCH-PUBLIC.md`,
  `memory-bank/activeContext.md`, `memory-bank/progress.md`.
