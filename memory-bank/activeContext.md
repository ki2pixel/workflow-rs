# Contexte actif

## Tâche en cours

**MVP J2 — soak 2 h, STEP8 systemd et bascule production validés.** Le
correctif P1 retire les quatre secrets HTTP de l'environnement des
sous-processus (`runner.rs`, test unitaire dédié). STEP8 tourne sous une unité
systemd isolée (`ProtectSystem=full`, `NoNewPrivileges=yes`, cache CUDA
`%C/workflow-rs/cuda-compute`) et produit un `.ecproj` 1080×1920 valide, sans
écriture dans `/mnt/cache` ni `runtime/`. Le soak de 7 200 s donne 7 200/7 200
échantillons, dérive p95 0,237 ms, **delta `WebKitNetworkProcess` 0 Kio**,
GUI +1,68 Mio, serveur 0 Kio, 0 OOM, tray Idle. L'unité de production
`workflow-server.service` est installée, activée, `Linger=yes`, secrets
0600/0700, webhook réel `running` (462 enregistrements, `planned=0`) et
redémarrage validé. 326 tests workspace, fmt/clippy/build verts.

**Prochain jalon** : push de `main` après l'ordre explicite
`GO PUSH MAIN <SHA>`, puis surveillance du moniteur webhook en production.
Toute évolution de `polling.rs`/`tray.rs` rejoue J4 ; toute évolution du
single-instance rejoue `scripts/poc/verify-single-instance.sh`.

## Dernière session

- [2026-10-26] **Restructuration documentaire de `docs/`** :
  - 23 documents racine déplacés dans `contracts/`, `pipeline/`, `ops/` et
    `desktop/` ; `audits/` conservé ;
  - hubs `docs/README.md` et `docs/audits/README.md` ; 37 documents indexés
    avec les statuts Actif/Prêt/Référence/Preuve/Historique ;
  - 217 références analysées dans 44 fichiers avec une allowlist (156
    réécrites, 61 vers `docs/audits/` inchangées), 7 renvois
    `fichier:ligne` recalés, 1 orphelin résolu ; template systemd et unité
    installée mis à jour sans redémarrage ; fmt/clippy/326 tests verts.
- [2026-10-26] **MVP J2 — soak, STEP8 systemd, bascule production** :
  - P1 `env_remove` pour `WORKFLOW_SERVER_WORKER_TOKEN`,
    `WORKFLOW_SERVER_SECRET_KEY`, `FLASK_SECRET_KEY` et
    `INTERNAL_WORKER_COMMS_TOKEN` ; test unitaire d'étanchéité, wrapper STEP8
    avec les quatre secrets à `<unset>` et `CUDA_CACHE_PATH` conservé ;
  - STEP8 sous `workflow-server-step8-test.service` : rapport `success=true`,
    mode `headless`, `bridge_port=null`, `.ecproj` 26 726 octets 1080×1920,
    `effectcraft-cli 0.17.0`, permissions `ProtectSystem=full` vérifiées,
    aucune écriture `/mnt/cache`/`runtime/`, unité retirée ;
  - soak 2 h `soak-2h-20261026` : 7 200 échantillons, durée 7 199,131 s,
    dérive p95 0,237 ms / max 1,201 ms, réseau 0 Kio, GUI RSS +1 716 Kio,
    GUI PSS +1 701 Kio, WebKitWeb +1 692 Kio, serveur 0 Kio, MemAvailable
    -115,56 Mio, VRAM pic 222 Mio, 0 OOM, watcher 7 202/0, D-Bus stable,
    nettoyage complet ;
  - bascule : `sync-backends.sh --apply` (911 URL, 97 archives, 1 008
    doublons STEP7, sauvegarde `runtime/backups/sync/20261026_035127`),
    smoke webhook désactivé, puis unité de production avec webhook réel
    `running` (462 enregistrements, `planned=0`), `:5004` loopback, un seul
    processus, `systemctl --user restart` 175295 → 175480, secrets
    0600/0700, unité `enabled` ;
  - validation : fmt/clippy rc 0, 326 tests workspace, build release
    inchangé ; rapport
    `docs/audits/2026-10-26-mvp-j2-soak-step8-production.md` et procédure
    `docs/ops/SWITCH-SYSTEMD.md`.
- [2026-10-26] **MVP J1 — `.deb`, single-instance, cache CUDA** :
  - paquet final 6,29 Mio / 17 159 Kio installé, `desktop-file-validate`
    rc 0, `xdg-mime` `application/x-desktop`, `Gio.DesktopAppInfo` OK,
    icônes 32/128/256/512, entrée GNOME visible ;
  - `single_instance.rs` (zbus 5.19) : `org.workflow.desktop`,
    `/org/workflow/desktop`, `org.freedesktop.Application`, payload `a{sv}`,
    `DoNotQueue`, validation de taille, `--no-single-instance` ;
  - `x11_activation.rs` : `_NET_ACTIVE_WINDOW` source 2, `check()` + `sync()`,
    activation différée après `show()`/`unminimize()` ;
  - preuves : introspection 1 091 octets, second lancement 53 ms,
    `gui_processes=1`, état `_NET_WM_STATE_FOCUSED`, URL divergente
    `InvalidArgs`, activation serveur arrêté, nom libéré à l'arrêt ;
  - unité systemd : `CacheDirectory`, `CUDA_CACHE_PATH=%C/...`,
    `CUDA_CACHE_MAXSIZE=1 Gio`, env serveur et `workflow-cli` enfant
    vérifiés ; STEP2 7 s puis 2 s après restart, CSV identique à la fixture ;
  - cache : 4 096 octets/0 fichier avant, 15 794 octets/2 fichiers après
    sonde PTX systemd, hashes/mtimes identiques après restart et seconde
    sonde ; STEP2 n'ajoute aucun fichier (SASS précompilé) ;
  - 325 tests workspace (281 pipeline + 38 services + 6 GUI), fmt/clippy/
    build release/.deb verts ; docs `docs/desktop/PACKAGING-GUI.md` et
    `docs/audits/2026-10-26-mvp-j1-deb-single-instance-systemd.md`.
- [2026-10-25] **Transition MVP — polling masqué + gel du crate** :
  - `PollingManager` : option `pauseWhenHidden`, suspension des intervalles
    et des backoffs, reprise immédiate, nettoyage `stopPolling`/`destroy`,
    statistiques `paused`/`totalPaused` ;
  - `main.js` : opt-in pour `systemMonitor` et `localDownloadsStatus` ; les
    pollers d'étapes/séquence restent actifs pour l'auto-mode ;
  - `workflow-server` : `Cache-Control: no-store` sur `/static/` ; imports
    `PollingManager.js?v=20261025a` dans les sept consommateurs pour que
    WebKit recharge le module après mise à jour ;
  - validation live `scripts/poc/verify-visibility-polling.py` : 0/0 en
    masqué, reprise au focus ;
  - soak 10 min `mvp-frontend-soak10-20261025` : 600 échantillons, dérive
    p95 0,323 ms, hôte +4 Kio, GUI PSS +73 Kio, réseau 0 Kio, watcher
    602/0/0, VRAM pic 368 Mio, acceptation `true` ;
  - revue `workflow-desktop-services` : terminal obsolète corrigé, watcher
    arrêté si le récepteur `watch` disparaît, identifiants d'URL refusés,
    `#[non_exhaustive]` ; 38 tests ;
  - 319 tests workspace, fmt/clippy/build verts ; docs
    `docs/desktop/MVP-FRONTEND-POLLING.md` et
    `docs/audits/2026-10-25-revue-workflow-desktop-services.md`.
- [2026-10-25] **PoC Tauri A-Strict — J5** :
  - bundle `deb` Tauri : `productName=workflow-manager`, icônes 32/128/256/512,
    `.desktop` `workflow-manager.desktop`, dépendances `libasound2`,
    `libssl3`, AppIndicator/WebKit/GTK ;
  - `strip = "symbols"` pour le paquet GUI ; télémétrie par défaut XDG ;
  - `packaging/systemd/workflow-server.service.in` et
    `scripts/install-systemd-user.sh` (préflight port/webhook, secrets 0600,
    `--start`, `--uninstall`) ;
  - `.deb` 5,59 Mio / 15 227 Kio installés, extraction sans root, `--help`
    et `--version` OK, `desktop-file-validate` rc=0 ;
  - unité systemd test `:5005` : `systemd-analyze verify` rc=0, serveur
    PPID 1644, GUI chargé, tray Idle, GUI tué, serveur actif ;
  - `switch-rust.sh` test `:5005` : check/start/stop rc=0, serveur PPID 1644,
    GUI tué, serveur actif ;
  - préflight : port occupé refusé avant installation, secrets 0644 refusés,
    désinstallation idempotente ;
  - décision : Tauri A-Strict conservé pour le MVP ;
  - rapports `docs/audits/2026-10-25-poc-tauri-j5.md` et
    `docs/desktop/PACKAGING-GUI.md`.
- [2026-10-25] **PoC Tauri A-Strict — J4** :
  - `workflow-desktop-services::tray` : politique pure du tray, réducteur
    `StatusSnapshot` → état visuel, `TrayPresenter`/`NoopTray` ;
  - `workflow-desktop-services::polling` : `StatusWatcher` à 1 Hz sur canal
    `watch`, tick ancré, `max_errors = None`, statistiques et
    `terminal_seq` pour les notifications ;
  - `workflow-gui-tauri` : adaptateur `tauri::tray`/`tray-icon`/
    `libayatana-appindicator3`, six icônes, menu natif, fermeture vers le
    tray, restauration par événement D-Bus ;
  - notifications et audio opt-in : `TransitionGate`, `NotifyRustNotifier`,
    `RodioAudio` (MP3 locaux) ;
  - scripts `j4-runbook.py` et `j4-monitor.py` : matrice, soak 2 h, RSS/PSS
    1 Hz, VRAM 200 ms, `_NET_WM_STATE_HIDDEN`, CSV streamé ;
  - matrice 5 min `j4-matrix-final-20261025-050025` : 8/8 états, fermeture
    `IsUnMapped`, restauration D-Bus `IsViewable`, durée 299,028 s ;
  - soak `j4-soak-20261025-025624` : 7 200 échantillons, dérive p95 0,310 ms,
    hôte +0,03 Mio, WebKitNetwork +22,06 Mio, WebKitWeb +3,35 Mio, PSS GUI
    +25,46 Mio, pente GUI 0,221 Mio/min, D-Bus stable, 0 OOM ;
  - acceptation conforme après recalibrage ratifié de l'enveloppe GUI :
    PSS ≤ 32 Mio (aligné sur le plafond RSS) et pente ≤ 0,267 Mio/min
    (32 Mio / 120 min) ; PSS 25,46 Mio et pente 0,221 Mio/min conformes.
    L'hôte et le watcher sont plats, la croissance est portée par
    `WebKitNetworkProcess` et le polling du frontend legacy ;
  - rapport `docs/audits/2026-10-25-poc-tauri-j4.md`.
- [2026-10-25] **PoC Tauri A-Strict — J3** :
  - `scripts/poc/j3-monitor.py` : RSS/PSS à 1 Hz, VRAM à 200 ms, contrat des
    trois processus GUI, suivi STEP2 + ffmpeg, alerte 3 900 MiB ;
  - `scripts/poc/j3-firefox-baseline.sh` : profil snap isolé, un onglet,
    capture et nettoyage ;
  - `scripts/poc/j3-runbook.py` : serveur `:5005`, vidéo `testsrc2` 30 s
    1280×720, baseline Firefox, baseline GUI, 5 runs STEP2 CUDA, acceptance ;
  - 5/5 runs exit 0, 5/5 CSV identiques (`7d3897…`), 0 OOM, pic VRAM
    2 392 Mio, libre min 1 508 Mio ;
  - Firefox : RSS médian 1 283,4 Mio, PSS 706,1 Mio, VRAM +174 Mio ;
  - Tauri : RSS médian 409,5 Mio, PSS 226,0 Mio, VRAM +2 Mio ;
  - STEP2 : RSS max 803,6 Mio, PSS 798,7 Mio ; ratio GUI/STEP2 ~0,513 RSS ;
  - rapport `docs/audits/2026-10-25-poc-tauri-j3.md`.
- [2026-10-25] **PoC Tauri A-Strict — J2** :
  - harness `scripts/poc/j2-stress.py` : sessions dédiées, sticky vérifié,
    trap `gsettings`, capture par bureau, contrôle des orphelins ;
  - 35 lancements, 25 balayages, 125 bascules ; 35/35 audits `token-check=ok`,
    35/35 sticky, 125/125 bascules ;
  - `default` : 20/20, 0 blanc, 0 SIGSEGV, 0 orphelin ; RSS médian ~410,8 Mio,
    PSS ~226,7 Mio ;
  - `disable-dmabuf` : 5/5, 25/25, 0 blanc, 0 orphelin ;
  - `force-dmabuf` : 5/5, 5 captures solides (`mean=0,172549`, 1 couleur),
    183 lignes `AcceleratedSurface was unable to construct a complete framebuffer` ;
  - `force-dmabuf-shm` : 5/5, rendu normal, 0 `AcceleratedSurface` ;
  - `gsettings` avant/après identiques (`dynamic-workspaces=true`,
    `num-workspaces=4`), ports libres, token absent ; rapport
    `docs/audits/2026-10-25-poc-tauri-j2.md`.
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
    `docs/desktop/ETUDE-GUI-NATIVE-LINUX.md` (E1) et
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
  - livrable `docs/desktop/ETUDE-GUI-NATIVE-LINUX.md` : inventaire réel du frontend,
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

- **J4** : 2 h de polling, tray D-Bus, notifications et audio opt-in.
- **J5** : packaging `.deb`, extraction sans root, décision go/no-go.
- Ratifier le passage des séquences côté serveur (`/run_custom_sequence`,
  `/sequence/status`) et ajouter le test associé.
- Diagnostiquer D-024 sans modifier les données de production.
- Lire `docs/ops/SWITCH-PUBLIC.md` avant toute bascule publique.
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
- `docs/README.md`
- `docs/contracts/CONTRAT-ENVIRONNEMENT.md`
- `docs/pipeline/LOT-*.md`
- `parity/DRIFT.md`
- `memory-bank/progress.md`
- `memory-bank/decisionLog.md`
