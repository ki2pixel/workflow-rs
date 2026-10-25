# Contexte actif

## Tâche en cours

**PoC Tauri A-Strict — J1 terminé.** Le scaffold
`workflow-desktop-services` + `workflow-gui-tauri`, l'authentification par
parsing HTML, l'hôte A-Strict et l'audit DOM sont validés. La baseline GUI est
mesurée (RSS médian 430 552 Kio, PSS 243 140 Kio, VRAM globale 190 Mio).

**Prochain jalon** : J2 — stress-test de rendu sur les quatre profils WebKit,
35 lancements au total et 25 balayages de 5 bureaux GNOME (125 bascules).

## Dernière session

- [2026-10-25] **PoC Tauri A-Strict — J1** :
  - ajout de `crates/workflow-desktop-services` (client HTTP typé, auth,
    modèles, polling, logs, notifications, télémétrie) et de
    `crates/workflow-gui-tauri/src-tauri` (Tauri 2.12.5, `invoke_system`
    neutralisé, `build.rs` garde-fou capabilities) ;
  - `tauri.conf.json` A-Strict : URL distante, `withGlobalTauri=false`, aucune
    capability/plugin/commande, `create=false`, `dragDropEnabled=false`,
    `devtools=false`, `freezePrototype=true` ;
  - token lu dans le HTML Minijinja, digest SHA-256 vérifié dans le DOM ;
    aucun token brut dans l'URL ou les artefacts ;
  - audit : `tauri=undefined`, `invoke`, `postMessage`, `rawIpc` et
    `nativeIpc` rejetés, digest `ok`, screenshot non blanc (1440×900,
    stddev 0,104) ;
  - `cargo test --workspace` : 299 tests ; `cargo fmt --all --check` et
    `cargo clippy --workspace --all-targets -- -D warnings` verts ;
  - serveur de test isolé sur `:5005`, webhook désactivé, chemins `/tmp` ;
    aucun accès `runtime/` ni `/mnt/cache` ; rapport
    `docs/audits/2026-10-25-poc-tauri-j1.md`.
- [2026-10-24] **Analyse du rapport Deep Search GUI** :
  - `docs/audits/Analyse_Architecturale_GUI_Linux.md` (E3) analysé et
    confronté au code, à E1/E2/C et à des mesures locales ; résultat dans
    `docs/audits/2026-10-24-analyse-rapport-deep-search-gui-linux.md` ;
  - test de fumée `MiniBrowser` 2.50.4 : page statique et page avec calque de
    composition, profils par défaut, `DISABLE_DMABUF_RENDERER=1` et
    `FORCE_SHM`, cinq bascules d'espace de travail ; aucun écran blanc, aucun
    SIGSEGV sur le chemin par défaut ;
  - en forçant `WEBKIT_FORCE_DMABUF_RENDERER=1`, page vide et erreurs
    `AcceleratedSurface` : R1 est réel sur le chemin DMA-BUF, mais le chemin
    par défaut patché par Ubuntu reste stable ;
  - VRAM non mesurable par `--query-compute-apps` (WebKit est graphique) ;
    RSS total 305 Mio contre ~1 030 Mio pour Firefox ;
  - corrections : `system_monitor` à 5 s, MSRV de repli (`gtk4 0.10.3` 1.83,
    `relm4 0.10.0` 1.85), identifiant AppIndicator Ubuntu, token A-Strict,
    gates VRAM/RSS relatifs, 10 000 lignes = lot serveur ;
  - décision : PoC Tauri A-Strict 5 jours d'abord, GTK4 seulement en cas
    d'échec ; crate `workflow-desktop-services` dès J1 ; aucun code
    fonctionnel modifié, aucun accès à `runtime/` ni `/mnt/cache`.
- [2026-10-24] **Confrontation des deux études GUI** :
  - `docs/audits/2026-10-24-confrontation-etudes-gui-linux.md` compare
    `docs/ETUDE-GUI-NATIVE-LINUX.md` (E1) et
    `docs/audits/Migration-Architecture-GUI-Linux-workflow-rs.md` (E2) ;
  - consensus architectural (modèle 1, serveur headless) ; désaccord limité au
    toolkit et à l'effort ;
  - 14 points d'E2 corrigés ou nuancés, dont 5 contredits par le code
    (`/api/system_monitor`, `/sequence/status`, 200 ms/5 req/s,
    `clear_disk_cache`, tokens CSS), plus l'accessibilité Iced et l'effort de
    4 semaines ; 7 corrections appliquées à E1 (révision 1.1), dont
    `GtkFileLauncher` → `gio::AppInfo`, `GSK_RENDERER=cairo`, NVML, SSE,
    `LogEntry` ;
  - règle de décision : Tauri T1 si le délai prime, GTK4/Relm4 si « widgets
    natifs » et 0 Mio VRAM sont exigés ; PoC à deux pistes recommandé ;
  - décision consignée dans `decisionLog.md` ; aucun code fonctionnel modifié,
    aucun accès à `runtime/` ni `/mnt/cache`.
- [2026-10-24] **Étude d'interface desktop Linux** :
  - livrable `docs/ETUDE-GUI-NATIVE-LINUX.md` : inventaire réel du frontend,
    mesures (logs 5,4 l/s max, 300 lignes bornées, VRAM bureau ~933 Mio),
    matrice pondérée Tauri/Slint/Iced/GTK4/egui, modèles d'intégration ;
  - recommandation : coque Tauri v2 en mode URL distante (client de `:5004`),
    fonctions de bureau côté Rust, serveur supervisé indépendamment ;
  - option native de repli : GTK4/Relm4 ; Iced et egui écartés ;
  - trois corrections au cadrage : débit de logs réel, séquence pilotée par le
    client (pas `/run_custom_sequence`), gestionnaire de cache absent du
    frontend Rust (D-015) ;
  - décision d'orientation consignée dans `decisionLog.md` ; aucun code
    fonctionnel modifié, aucun accès à `runtime/` ni `/mnt/cache`.
- [2026-10-24] **Synchronisation documentaire** :
  - chiffre de tests aligné sur la mesure
    (`cargo test --workspace -- --list`) : 280 ;
  - statut `stopping`/`is_stopping`, en-têtes de sécurité, `server.allow_remote`,
    `download.allow_private_hosts`, quotas STEP1 et `steps.step7.timeout_s`
    documentés dans le contrat et les docs de lot ;
  - index D-025 à D-027 complété dans `decisionLog.md` ;
  - purge des résidus au démarrage ajoutée à `RETENTION.md` ;
  - audit de `.agents/rules/codingstandards.md` et des 22 `SKILL.md` :
    compte de tests 280, clé `steps.step3.max_workers` retirée, verrou STEP1,
    arrêt `stopping`, gate `effectcraft-cli`, écriture legacy interdite dans les
    skills frontend.
- [2026-10-24] **Journal serveur enrichi** :
  - `workflow-server.log` inclut désormais la configuration au démarrage, les
    requêtes mutantes et erreurs HTTP avec durée, le cycle webhook, les étapes
    avec PID et les séquences.
  - ANSI désactivé automatiquement quand stderr est redirigé vers un fichier ;
    `RUST_LOG=debug` pour les GET et le détail webhook.
  - `start-rust.sh` n'affiche plus les 40 anciennes lignes au démarrage, ce qui
    évite de confondre les erreurs webhook du 2026-10-23 avec un incident
    courant.
- [2026-10-24] **Widget système et auto-ouverture des logs** :
  - `/api/system_monitor` expose la GPU via `nvidia-smi` (première carte,
    contrat `null`/`error` conservé) ; le batch STEP1 → STEP8 du jour a servi
    de validation d'exploitation.
  - L'auto-ouverture de l'overlay de logs est désactivée par défaut côté
    frontend Rust ; le toggle Settings et la préférence persistée restent
    effectifs.
  - Logs du batch `1531 Camille` vérifiés : 8/8 étapes `completed`, aucun
    échec d'étape ; seules les erreurs webhook du 2026-10-23 restent dans le
    journal serveur.
- [2026-10-24] **Remédiation de l'audit backend (36 constats)** : six lots
  appliqués sur les neuf crates. Orchestrateur : `RunId`, `StepPhase::Stopping`,
  fin générationnelle, canal de logs borné, rotation en cours d'exécution,
  erreurs HTTP typées, arrêt des groupes de processus. Configuration :
  `allow_remote`, verrou `/tmp/workflow-rs-hw.lock`, clés TOML mortes
  supprimées, `steps.step7.timeout_s`. Sécurité : SSRF public-only, token
  constant-time, en-têtes CSP, quotas ZIP/RAR, symlinks STEP7 refusés, logs
  confinés. Parité : `round_to` partagé STEP3, RAR streamé. Persistance :
  `fsync` avant publication, temporaires cachés, purge des orphelins.
  Documentation : D-025 (404 `/sound-design`), D-026 (frontières typées),
  D-027 (arrêt manuel CLI). Suite : 280 tests, fmt/clippy verts.
- [2026-10-24] **Bootstrap `.agents/` et `memory-bank/`** :
  - création de `.agents/rules/` (10 règles), `.agents/skills/` (21 skills +
    9 ressources) et `.agents/workflows/` (6 workflows) ;
  - renommage des skills legacy obsolètes vers la numérotation Rust
    (`step3-audio-orchestrator`, `step4-tracking-ops`) ;
  - remplacement des skills legacy-only par des skills Rust
    (`onnx-runtime-cuda-expert`, `rust-concurrency-orchestrator`,
    `after-effects-bridge`, `after-effects-json-contract`) ;
  - initialisation de `memory-bank/` avec 5 fichiers à partir de l'historique
    git (31 commits) et des docs de lot ;
  - ajout d'un pointeur dans `AGENTS.md`.
- [2026-10-24] **Intégration de `rust-patterns`** :
  - normalisation du frontmatter et du Markdown (table, commandes, structure) ;
  - ajout du routage priorité 3 dans `skills-integration.md` ;
  - mise à jour du décompte : 22 skills, 52 fichiers dans `.agents/` +
    `memory-bank/`.
- [2026-10-24] **Audit documentaire** :
  - correction des commandes de parité : le venv legacy est
    `../workflow_mediapipe/coral_env/bin/python`, pas `coral_env/` ;
  - alignement de l'état de production dans `README.md`, `SWITCH-PUBLIC.md` et
    `CONTRAT-ENVIRONNEMENT.md` : Rust `:5004`, Python `:5003` en rollback ;
  - `.agents/workflows/repomix-bundle.md` est supprimé dans l'arbre de travail
    (non commité) ; le décompte courant est 51 fichiers dans `.agents/` +
    `memory-bank/`.

## État du système

- Backend de production : Rust `:5004` ; Python legacy `:5003` en rollback.
  Aucun serveur n'écoutait lors du bootstrap du 2026-10-24.
- Webhook Rust activé dans la configuration depuis le 2026-10-23
  (`enabled = true`) ; il ne tourne que si le serveur est démarré.
- 280 tests unitaires ; pas de CI.
- Audit backend du 2026-10-24 remédié : 33 corrigés, 3 documentés
  (C3 → D-025, C4 → D-015, C8 → D-024).
- Dernière validation : STEP1 → STEP8 sur `1720 Camille`, shadow `162/162`.
- Verrou matériel : `/tmp/workflow-rs-hw.lock`.
- Runtime : `runtime/` ; sortie finalisée : `/mnt/cache`.

## Blocages / points ouverts

1. **D-024** : le STEP4 Python régénéré sur `153 Camille` diffère de l'archive
   (4 977 objets contre 11 853). Ne pas utiliser `--prefer-raw` sur ce projet.
2. **Reverse proxy** : aucun nginx/caddy/cloudflared installé ; la bascule
   publique reste à faire.
3. **Surveillance production** : vérifier le moniteur webhook et les
   téléchargements après quelques jours.
4. **Parité STEP4** : la configuration d'origine de `153` reste à retrouver.
5. **Signaux** : les étapes 1 à 4 installent un gestionnaire `SIGTERM`/`SIGINT`
   qui termine leurs enfants ; STEP8 conserve son propre gestionnaire. Voir
   D-027.

## Prochaines actions

- **J2** : stress-test de rendu sur les quatre profils WebKit, 35 lancements
  et 25 balayages de 5 bureaux GNOME (125 bascules).
- **J3** : 5 runs STEP2 sur projet isolé, baseline Firefox fraîche, VRAM
  mesurée par `nvidia-smi pmon`, `fuser /dev/nvidia0` et VRAM globale.
- **J4** : 2 h de polling, tray D-Bus, notifications et audio opt-in.
- **J5** : packaging `.deb`, extraction sans root, décision go/no-go.
- Ratifier le passage des séquences côté serveur (`/run_custom_sequence`,
  `/sequence/status`) et ajouter le test associé.
- Diagnostiquer D-024 sans modifier les données de production.
- Lire `docs/SWITCH-PUBLIC.md` avant toute bascule publique.
- Vérifier `history stats`, `webhook probe --plan` et les logs du moniteur.
- Maintenir `memory-bank/progress.md` et `decisionLog.md` à jour.

## Rappels

- Ne pas modifier `/home/kidpixel/workflow_mediapipe`.
- Ne pas écrire dans `/mnt/cache` depuis un test.
- Ne pas lancer les moniteurs Python et Rust simultanément.
- Ne pas lancer STEP2/3/4 matérielles des deux stacks en même temps.
- Ne pas nettoyer les quirks D-001, D-002, D-004, D-016.
- Lire `.agents/rules/codingstandards.md` avant toute modification de code.

## Références

- `AGENTS.md`
- `README.md`
- `docs/CONTRAT-ENVIRONNEMENT.md`
- `docs/LOT-*.md`
- `parity/DRIFT.md`
- `memory-bank/progress.md`
- `memory-bank/decisionLog.md`
