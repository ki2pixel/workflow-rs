# Suivi de progression

## Statut actuel

- Migration Rust terminée pour les 8 étapes.
- Backend de production configuré : Rust `:5004` ; Python legacy `:5003` en
  rollback. Aucun serveur n'écoutait lors du bootstrap du 2026-10-24.
- Moniteur webhook Rust activé dans la configuration depuis le 2026-10-23
  (`webhook.enabled = true`).
- 326 tests workspace (pipeline + services + GUI), sans matériel.
- Chaîne STEP1 → STEP8 validée sur `1720 Camille.zip`, puis sur `1531 Camille`
  via le frontend Rust (8/8 étapes `completed`).
- Widget système Rust : CPU/RAM/disque et GPU `nvidia-smi` ; auto-ouverture
  des logs désactivée par défaut (toggle conservé).
- Journal serveur enrichi (configuration, requêtes, webhook, étapes,
  séquences), ANSI désactivé à l'écriture fichier et terminal limité à la
  session courante au démarrage.
- Shadow STEP5/6 `162/162` sur 20 projets.
- Sync réelle : 911 URL, 96 archives, 640 hash STEP7 (1,8 Go).
- Audit documentaire du 2026-10-24 : commandes de parité corrigées et état de
  production aligné sur Rust `:5004`.
- Remédiation de l'audit backend du 2026-10-24 : 33 constats corrigés,
  3 documentés (C3 → D-025, C4 → D-015, C8 → D-024). `cargo fmt --check`,
  clippy strict, 280 tests et `config check` verts.
- PoC Tauri A-Strict J1 (2026-10-25) : `workflow-desktop-services` et
  `workflow-gui-tauri` créés ; token lu dans le HTML Minijinja et vérifié par
  digest SHA-256 ; `invoke_system` et handler IPC natif neutralisés, aucune
  capability/plugin/commande ; audit `tauri=undefined`, `invoke`, `postMessage`,
  `rawIpc` et `nativeIpc` rejetés, `token-check=ok` ; baseline GUI RSS médian
  430 552 Kio, PSS 243 140 Kio, VRAM globale 190 Mio ; 299 tests workspace,
  fmt/clippy verts. Rapport `docs/audits/2026-10-25-poc-tauri-j1.md`.
- PoC Tauri A-Strict J2 (2026-10-25) : harness `scripts/poc/j2-stress.py` ;
  35 lancements, 25 balayages, 125 bascules ; `default` 20/20 stable et
  `disable-dmabuf` 5/5 stable (0 blanc, 0 SIGSEGV, 0 orphelin) ; canari
  `force-dmabuf` 5/5 solide avec 183 lignes `AcceleratedSurface` ;
  `force-dmabuf-shm` 5/5 normal ; `gsettings` restaurés, ports libres,
  token absent. Rapport `docs/audits/2026-10-25-poc-tauri-j2.md`.
- PoC Tauri A-Strict J3 (2026-10-25) : baseline Firefox snap, GUI Tauri et
  5 runs STEP2 CUDA sur vidéo `testsrc2` 30 s 1280×720 ; 5/5 CSV identiques,
  0 OOM, pic VRAM global 2 392 Mio, libre min 1 508 Mio ; GUI ~410 Mio RSS /
  ~227 Mio PSS, ratio ~0,32× Firefox ; aucun orphelin, ports libres, verrou
  non tenu, `gsettings` inchangés. Rapport
  `docs/audits/2026-10-25-poc-tauri-j3.md`.
- PoC Tauri A-Strict J4 (2026-10-25) : tray D-Bus
  (`libayatana-appindicator3`), `StatusWatcher` 1 Hz sur `watch`, matrice
  5 min (8/8 états, fermeture/restauration D-Bus), soak 2 h avec fenêtre
  minimisée et `systemd-inhibit` ; notifications/audio opt-in. Soak :
  dérive p95 0,310 ms, hôte +0,03 Mio, WebKitNetwork +22,06 Mio, WebKitWeb
  +3,35 Mio, PSS GUI +25,46 Mio, pente GUI 0,221 Mio/min, D-Bus stable,
  0 OOM. Acceptation conforme après recalibrage ratifié de l'enveloppe GUI
  (PSS ≤ 32 Mio, pente ≤ 0,267 Mio/min) ; l'hôte/watcher est plat, la
  croissance est portée par `WebKitNetworkProcess` et le polling du frontend
  legacy. Rapport `docs/audits/2026-10-25-poc-tauri-j4.md`.
- PoC Tauri A-Strict J5 (2026-10-25) : `.deb` `workflow-manager` construit
  par Tauri CLI 2.12.4 (5,59 Mio, 15 227 Kio installés), extraction sans
  root, `desktop-file-validate` et `systemd-analyze --user verify` verts ;
  unité systemd user et `switch-rust.sh` testés sur `:5005`, serveur PPID
  `systemd --user`, survit à la fermeture du GUI ; décision Tauri A-Strict
  pour le MVP. Rapports `docs/PACKAGING-GUI.md` et
  `docs/audits/2026-10-25-poc-tauri-j5.md`.
- Transition MVP (2026-10-25) : `PollingManager` suspend les pollers
  `systemMonitor` et `localDownloadsStatus` sur `visibilitychange`
  (`pauseWhenHidden`), avec reprise immédiate au focus ; imports
  `PollingManager.js?v=20261025a` et `Cache-Control: no-store` sur
  `/static/` pour contourner le cache de modules WebKit. Validation live :
  0 requête `system_monitor`/`csv_downloads` sur 8 s masquées, reprise au
  focus. Soak 10 min : 600 échantillons, 100 % masqué, dérive p95
  0,323 ms, hôte +4 Kio, GUI PSS +73 Kio, `WebKitNetworkProcess`
  **0 Kio** (contre +22,06 Mio en J4), watcher 602 ticks/0 erreur,
  acceptation `true`. Revue du crate `workflow-desktop-services` :
  3 corrections P2 (terminal obsolète, arrêt du watcher sans récepteur,
  identifiants dans l'URL), `#[non_exhaustive]`, 38 tests ; 319 tests
  workspace, fmt/clippy/build verts. Les exécutables factices des tests
  `workflow-steps` sont écrits par un processus `sh` séparé pour éviter
  `ETXTBSY`. Docs `docs/MVP-FRONTEND-POLLING.md` et
  `docs/audits/2026-10-25-revue-workflow-desktop-services.md`.
- MVP J2 (2026-10-26) : P1 `env_remove` (4 secrets), STEP8 sous unité
  systemd isolée (`.ecproj` 1080×1920, `ProtectSystem=full`, aucune écriture
  `/mnt/cache`/`runtime/`), soak 2 h `soak-2h-20261026` (7 200 échantillons,
  dérive p95 0,237 ms, **réseau 0 Kio**, GUI +1,68 Mio, serveur 0 Kio,
  0 OOM, tray Idle, D-Bus stable), bascule `workflow-server.service` avec
  webhook réel `running` (462 enregistrements, `planned=0`), secrets
  0600/0700, `systemctl --user restart` validé. 326 tests workspace,
  fmt/clippy/build verts. Rapports
  `docs/audits/2026-10-26-mvp-j2-soak-step8-production.md` et
  `docs/SWITCH-SYSTEMD.md`.
- Synchronisation documentaire du 2026-10-24 : statut `stopping`, en-têtes de
  sécurité, `server.allow_remote`, quotas STEP1, purge des résidus et index
  D-025 à D-027 alignés sur le code. Audit `.agents/` : `codingstandards.md` et
  les 22 `SKILL.md` alignés (tests 280, `max_workers` retiré, verrou STEP1,
  gate EffectCraft, legacy en lecture seule).
- Étude d'interface desktop Linux livrée (`docs/ETUDE-GUI-NATIVE-LINUX.md`) :
  inventaire du frontend, mesures, matrice Tauri/Slint/Iced/GTK4/egui,
  recommandation d'une coque Tauri v2 cliente de `:5004` et option native
  GTK4/Relm4. Aucun code fonctionnel modifié.
- Confrontation des deux études GUI
  (`docs/audits/2026-10-24-confrontation-etudes-gui-linux.md`) : consensus
  architectural, 14 points d'E2 corrigés ou nuancés, 7 corrections appliquées à E1
  (révision 1.1). Le choix du toolkit reste conditionnel à l'arbitrage
  « widgets natifs » ; règle de décision et PoC à deux pistes documentés.
- Analyse du rapport Deep Search GUI
  (`docs/audits/2026-10-24-analyse-rapport-deep-search-gui-linux.md`) :
  vérifications locales `MiniBrowser` 2.50.4 (profils par défaut,
  `DISABLE=1`, `FORCE_SHM`, calque de composition, cinq bascules d'espace de
  travail), R1 non reproduit sur le chemin par défaut mais reproduit en
  forçant `FORCE_DMABUF=1` (page vide, `AcceleratedSurface`), VRAM non
  mesurable par `--query-compute-apps`, corrections MSRV/AppIndicator/token,
  gates VRAM/RSS relatifs. Décision : PoC Tauri A-Strict 5 jours d'abord,
  GTK4 en repli, crate `workflow-desktop-services` dès J1.
- **PoC Tauri A-Strict J1–J5 terminé.** Prochain jalon : MVP — pause du
  polling page masquée, single-instance, installation réelle du `.deb` et
  runs STEP2/STEP8 sous l'unité systemd. Point de vigilance : croissance de
  `WebKitNetworkProcess` due au polling du frontend legacy en arrière-plan.

## Chronologie de migration

| Lot | Dates | Commits principaux | Livré | Validation |
| --- | --- | --- | --- | --- |
| 0 | 2026-10-22 | `e83b0a9`, `4440718` | Workspace, contrat, harnais de parité, spike TPU, modèles | 23 cas de parité, spike stable |
| 1 | 2026-10-22 | `d792e06`, `6530ba6`, `afdc93f`, `43c5fbf`, `e195a89`, `c0b6482`, `97ef1bc` | STEP5 audio/vidéo, STEP6, bridge, STEP7, STEP8, rétention | Parité 244/245/249/250, shadow 57/57, projet 250 |
| 2 | 2026-10-22 | `50722f1`, `d8f2ad2` | STEP2 TransNetV2 ONNX CUDA | CSV identiques sur 4 cas, VRAM sous baseline |
| 3 | 2026-10-22 | `b0983af`, `b7d2cbc` | STEP4 Coral tracking | Parité `1e-6` sur `face_pan`, `real_244`, `real_245` |
| 4 | 2026-10-22 | `f5d9e81` | STEP3 audio YAMNet/ECAPA/AHC | JSON identiques sur `vox_2spk`, `real_244`, `real_245` |
| 5 | 2026-10-23 | `ea10057`, `b257868` | Serveur axum `:5004`, séquence, arrêt, tests concurrence | 12/12 artefacts, 409, SIGTERM |
| 6 | 2026-10-23 | `b349ca6`, `85f3289` | STEP1 extraction/normalisation | ZIP/RAR/TAR, `.mov`, archive réelle |
| 7 | 2026-10-23 | `5f51ee2`, `34c6b97` | Intégration STEP2 → STEP8, bascule contrôlée | Séquence réelle, parité STEP5/6 |
| 8 | 2026-10-23 | `73b9312`, `f1fc509`, `e0b14b8` | Ingestion webhook, SQLite, R2/Dropbox, routes CSV | 911 URL, audit 0 changement, 462 enregistrements |
| Webhook | 2026-10-23 | `07ae8da` | Activation du moniteur Rust | Base non vide, un seul backend |
| Lanceur | 2026-10-23 | `f827d53` | `start-rust.sh`, secrets, Firefox | Lancement réel |
| Sync | 2026-10-23 | `28287af`, `6b6e5e1` | Fusion Python ↔ Rust | 911/96/640, 1,8 Go |
| Shadow | 2026-10-23/24 | `01fbbf2`, `b7b594b` | Corpus 153–159 et 1720 | 162/162 sur 20 projets |
| MVP GUI | 2026-10-25/26 | `9be0b02` + commit J1 | Polling masqué, `.deb` réel, single-instance D-Bus, cache CUDA systemd | 325 tests, `verify-single-instance.sh` OK sur binaire installé, STEP2 CSV identique |

## MVP client desktop (2026-10-26)

- Paquet final `workflow-manager_0.1.0_amd64.deb` : 6 595 548 octets,
  SHA-256 `66da7031…201439f8`, installé par `sudo apt install`.
- `desktop-file-validate` rc 0, `xdg-mime` `application/x-desktop`,
  `Gio.DesktopAppInfo` OK, icônes 32/128/256/512, entrée GNOME visible.
- Single-instance `org.workflow.desktop` / `org.freedesktop.Application` :
  second lancement 53 ms, un processus, fenêtre restaurée/focalisée, URL
  divergente rejetée en `InvalidArgs`, activation serveur arrêté.
- Unité systemd : `CacheDirectory=workflow-rs/cuda-compute`,
  `CUDA_CACHE_PATH=%C/workflow-rs/cuda-compute`,
  `CUDA_CACHE_MAXSIZE=1073741824`. STEP2 sous unité de test : 7 s puis 2 s
  après restart, CSV identique à `short_2997_step2`.
- Cache JIT : 4 096 o/0 fichier avant, 15 794 o/2 fichiers après sonde PTX
  systemd, hashes/mtimes identiques après restart et seconde sonde. STEP2
  n'ajoute aucun fichier (SASS `sm_75` précompilé).
- Rapport : `docs/audits/2026-10-26-mvp-j1-deb-single-instance-systemd.md`.

## Remédiation audit backend (2026-10-24)

| Lot | Contenu | Preuve |
| --- | --- | --- |
| 1 | P0 concurrence : `RunId`, `Stopping`, fin générationnelle, erreurs typées, `parking_lot` | 40 tests serveur, 409 après `/stop`, fin périmée ignorée |
| 2 | Cycle de vie : arrêt des groupes, verrou STEP1, isolation partagée, `allow_remote`, purges | test SIGTERM/SIGKILL de groupe, `bash -n switch-rust.sh` |
| 3 | Parité : `round_to` STEP3, RAR streamé | 0,1 % sur 1,8 s/3 600 s, faux `unrar` multi-Mio |
| 4 | Sécurité/contrats : CSP, constant-time, SSRF, quotas, symlinks, logs, fsync, timeout STEP7 | tests headers, IP privées, zip bomb, symlink hors logs |
| 5 | Robustesse : canal borné, writer bloquant, rotation, progression, purge orphelins, `media.rs`, regex, cache resize | ligne 64 Kio, rotation en cours, progression 3 étapes |
| 6 | Clôture : `unwrap` STEP2, TraceLayer, tests CLI/TPU, docs | 278 tests à la clôture, 280 après les deux commits suivants, clippy/fmt, D-025 à D-027 |

## Détails des validations

### STEP1 → STEP8

- Archive : `/mnt/cache/old/1720 Camille.zip` (1,7 Go, 4 MP4, 1 PNG).
- Configuration isolée sous `/tmp/opencode/step1-8`.
- Résultat : 8/8 étapes, 4 `.ecproj`, aucune écriture dans `/mnt/cache`.
- Durées : STEP1 3 s, STEP2 43 s, STEP3 1 min 40 s, STEP4 2 min 58 s,
  STEP5/6 < 1 s, STEP7 2 s, STEP8 4 s.

### Parité

- STEP2 : CSV identiques sur `cuts`, `short_2997`, `real_244`, `real_245`.
- STEP3 : JSON bruts identiques sur `vox_2spk`, `real_244`, `real_245`.
- STEP4 : JSON identiques à `1e-6` sur `face_pan`, `real_244`, `real_245`.
- STEP5/6 : shadow `162/162` sur 20 projets.
- STEP1 : validation ZIP/RAR/TAR et préservation `.mov`.

### Ingestion

- Import : 911 URL identiques à la base Python.
- Audit de normalisation : 0 changement.
- Webhook réel : 462 enregistrements.
- Téléchargement `276 Camille.zip` : R2 puis repli Dropbox, 183 450 047 octets,
  ZIP valide.
- Sync : 96 archives, 640 hash STEP7, 1,8 Go.

## Prochaines actions

1. **Push `main`** : attendre l'ordre explicite `GO PUSH MAIN <SHA>` sur le SHA
   final consolidé (option A), sans force.
2. **Surveillance production** : vérifier le moniteur webhook
   `workflow-server.service` après quelques jours, les téléchargements et la
   cohérence de l'historique.
3. **STEP7/STEP8 production** : produire un manifeste STEP7 courant avant de
   cliquer STEP8 ; le manifeste `20261024_160353` référence un projet absent
   de `/mnt/cache`.
4. **D-024** : diagnostiquer l'écart STEP4 Python sur `153 Camille`
   (4 977 objets contre 11 853). Ne pas utiliser `--prefer-raw` sur ce projet
   tant que la configuration d'origine n'est pas retrouvée.
5. **Reverse proxy** : aucun nginx/caddy/cloudflared n'est installé. La bascule
   publique reste à faire ; voir `docs/SWITCH-PUBLIC.md` et
   `docs/SWITCH-SYSTEMD.md`.
6. **Documentation** : maintenir `README.md`, `docs/LOT-*.md`,
   `docs/CONTRAT-ENVIRONNEMENT.md` et `parity/DRIFT.md` à jour.
7. **Memory-bank** : mettre à jour `activeContext.md` et ce fichier à chaque
   session significative.

## Politique de rétention

- Ce fichier conserve les tâches depuis octobre 2026.
- Les périodes plus anciennes seront résumées et déplacées vers
  `memory-bank/archives/` si le volume le justifie.
- L'historique Python legacy reste dans
  `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.
