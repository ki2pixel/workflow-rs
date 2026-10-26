# Contexte actif

## Tâche en cours

**Aucune.** Le rollback du client desktop Tauri est terminé et validé. La
production est revenue à la stack Rust headless avec l'interface web legacy.

## Dernière session

- [2026-10-26] **Rollback Tauri et retour au headless (D-028)** :
  - abandon du client desktop Tauri, des crates
    `workflow-desktop-services`/`workflow-gui-tauri`, du paquet
    `workflow-manager`, de l'unité `workflow-server.service` et des scripts
    PoC ;
  - dépôt ramené à `a93a554` ; branche de sauvegarde
    `backup/tauri-abandon-20261026` sur `1613b17` ;
  - validation : 280 tests au rollback strict, 282 après correctifs ;
    fmt/clippy, `config check`, build release ;
  - production : Rust `:5004` relancé via `scripts/switch-rust.sh`, Python
    `:5003` arrêté, aucun client desktop ;
  - correctifs réappliqués : `514e2d6` (ETXTBSY), `4ddb91b` (secrets,
    281 tests) et `713b6bc` (polling web masqué, 282 tests) ; réorg docs
    conservée dans la branche de sauvegarde.
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

- Backend de production : Rust `:5004` headless via
  `scripts/switch-rust.sh` ; Python legacy `:5003` arrêté (rollback
  disponible).
- Interface : page web legacy `assets/web` uniquement ; aucun client desktop
  natif (D-028).
- Webhook Rust activé dans la configuration depuis le 2026-10-23
  (`enabled = true`) ; il ne tourne que si le serveur est démarré.
- 282 tests unitaires ; pas de CI.
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
- Ne pas réintroduire de client desktop Tauri/GTK/WebKit/zbus (D-028).
- Le serveur headless est lancé par `scripts/switch-rust.sh` ; ne pas
  réinstaller `workflow-server.service` sans nouvelle décision.
- Lire `.agents/rules/codingstandards.md` avant toute modification de code.

## Références

- `AGENTS.md`
- `README.md`
- `docs/CONTRAT-ENVIRONNEMENT.md`
- `docs/LOT-*.md`
- `parity/DRIFT.md`
- `memory-bank/progress.md`
- `memory-bank/decisionLog.md`
