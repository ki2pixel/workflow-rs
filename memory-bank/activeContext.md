# Contexte actif

## Tâche en cours

**Post-traitement A5/A6 intégré à la production.** STEP8 publie désormais
toujours un second `<stem>_assist.ecproj` à côté du brut intact, avec
`post.json`/`review.json`/`audit.json` sous `docs/post/`. A1 letterbox, A2
bannière, A3/A4 en revue, A6 template SDM. La commande `workflow-cli post`
permet de le relancer seul. 315 tests, fmt et clippy verts ; validation 295
au pixel (0/255) et parité de décisions avec le prototype sur 268/187/191/181.

## Dernière session

- [2026-10-28 01:32] **Premier batch production A5/A6 (`223 Camille`)** :
  2 vidéos, STEP2–8 7/7 `completed`, post 2 assist, 0 apply, 13 review,
  84 noop, audit `pass` ; brut/assist identiques et artefacts STEP4/5/6
  intacts. Revues A1/A2/A3/A4 attendues ; A6 non déclenché.
- [2026-10-27] **Post-traitement A5/A6 en production (D-032)** :
  - module Rust `crates/workflow-steps/src/post/` : parseur `.ecproj`,
    détecteurs ffprobe/cropdetect/bandes, A1, A2, A3, A4, A6, audit ;
  - hook après STEP8 réussi et commande `workflow-cli post run` ;
  - invariant : brut **octet pour octet inchangé**, `_assist.ecproj` toujours
    publié (copie exacte si aucune décision), artefacts additifs sous
    `docs/post/<projet_assisté>/` ;
  - A1/A2 ne déplacent jamais l'Anchor Point ; A3/A4 en revue ; A6 éligible
    court-circuite A1–A4 ; conflit A1/A2 → revue ; échec de post → STEP8
    sort en code non nul ;
  - validations : 295 assist identique à la référence et rendu 0/255 à
    0/10/30/50 s ; décisions A1/A2/A3/A4 identiques au prototype sur 268,
    187 DISNEYWORLD, 187 ECOLE, 191 et 181 ; 270/276/286/287 0 apply et
    assist identique au brut ; A2 synthétique 11/11 ; idempotence 187 ECOLE ;
  - docs : `docs/LOT-9-POST-STEP8.md`, `docs/LOT-1-STEP8.md`,
    `docs/CONTRAT-ENVIRONNEMENT.md`, `docs/ETUDE-STEP9-AGENTIQUE.md` §3.9,
    `docs/SPIKE-STEP9-PROTOCOLE.md` §14.1, `README.md`, `AGENTS.md` ;
  - prochaines étapes : exposer `review.json` en lecture seule dans l'UI,
    recalibrer A3/A4, traiter les références manquantes 179/287.
- [2026-10-27] **Spike STEP9 — A5 v2 et A6 intégré** :
  - hash vidéo **échantillonné** (début/milieu/fin, 3 × 4 Mio) dans
    `audit.json` ; `timings_ms` par étage dans `post.json`/`audit.json` ;
  - A1 : 10e percentile des hauteurs actives, couverture mathématique de tous
    les crops, keyframes en `review` ; 268 15 applies à 231,1 % + Y −287,9 px,
    191 37 applies à 249,6 % + Y −367,8 px enfant ; 181 Concierge 0 apply,
    10 barres asymétriques en `review` ;
  - A2 : scan haut+bas, bandes sombres par paires de bords, stabilité
    géométrique des bords (centre/hauteur > 12 px → `review`), logo/partiel/
    décor en `review` ; 187 DISNEYWORLD/ECOLE 206,6 %, 191 logo review,
    181 noop ; validation synthétique 11/11 (3 apply, 7 review, 1 noop) ;
  - A5 coûte 3,7–33,5 s par vidéo (0,07–0,24× la durée, sans rendu) ;
  - A6 : `a5_report.py --template sdm` émet la décision et le bloc
    `template` ; 295 apply/audit pass, 295 keyframé review, 187 noop ;
    l'écriture du projet reste dans `a6_apply.py --apply` ;
  - rapport : `/tmp/opencode/step9-spike/06-corpus/a5v2/RAPPORT-A5-V2.md` ;
  - prochaine étape : brancher A5 v2/A6 dans le post-traitement de
    production (hors panel), puis traiter les références manquantes 179/287.
- [2026-10-27] **Spike STEP9 — A5 et évaluation** :
  - `03-scenario-a/a5_report.py` : A0/A1/A2 + candidats A3/A4, écriture de
    `post.json`, `review.json`, `audit.json` sans modifier le `.ecproj` ;
  - 4 cas exécutés : 268, 187 DISNEYWORLD, 187 ECOLE, 191 cancer ;
  - A2 : 206,6 % vs 207 % sur 187 (0,4 %) ; décor statique de 187 ECOLE
    séparé ; logo de 191 en `review` ;
  - A1 : 231,1 % (268) dans la plage humaine 221,3–239,7 % ; 249,6 % vs
    255 % (191, 2,1 %) ; garde-fou de barres asymétriques (faux positif 187
    segment 14 évité) ;
  - validation visuelle sur copies relinkées : A2 206,6 % supprime la
    bannière de 187 aux timecodes 10/40/100 s ; A1 249,6 % sur 191 laisse
    19,5 % de barre en haut (ancre décentrée), mais la compensation
    Position Y de -417,8 px supprime les barres → A5 prescrit scale +
    compensation et signale le cadrage vertical en `review` ;
  - A3 : précision 0,29–0,60, rappel 0,20–0,43, jamais appliqué ;
  - A4 : cut dur intra-segment désactivé par défaut (32 faux positifs sur
    187), seuls segments courts et bornes CSV sont signalés ;
  - rapport : `/tmp/opencode/step9-spike/06-corpus/a5/RAPPORT-A5.md` ;
  - prochaine étape : finaliser le seuil de compensation A1, durcir A2
    (bannière sombre/animée, logo de coin), intégrer A6 avec le profil
    mesuré du GOOD 295.
- [2026-10-27] **Spike STEP9 — corpus élargi (19 paires / 32 vidéos)** :
  - 1059 segments bruts, 520 clés humaines, 145 segments keyframés ;
  - 278/520 clés (53,5 %) à ≤ 60 px d'un objet suivi ; 458/907 segments
    statiques (50,5 %) à ≤ 50 px du cadrage brut, 363/907 (40 %) à > 100 px ;
  - **A3 invalidé en auto** : précision 0,20, rappel 0,23 ; 35 faux positifs
    sur les vidéos « sans piège » → candidats en `review` ou flag projet ;
  - **A1 validé** sur le letterbox noir (191 : 249,6 % vs 255 % avec marge
    4 % ; 268 : 179,1/222,2 % par segment), mais bords non noirs de 268 non
    couverts ;
  - **A2 fragile** : 207,4 % correct sur 187 DISNEYWORLD, faux positif à
    260 % sur 187 ECOLE ; logos de coin (191) et logo de template (295,
    `sdm.png`) hors A2 ; **corrigé en v2 (D-031)** ;
  - **A6 SDM validé** sur le GOOD 295 reçu le 2026-10-27 : brut et GOOD
    partagent le même `_ae.json`/`_tracking.json` (SHA-256 identiques). Le
    template (fond 180 % flouté 50, panneau 84,3497774862575 % à y=171,032,
    panneau 56,294573830734535 % à y=1082,026, logo `sdm.png`) est reproduit
    par `06-corpus/a6_template.py` à **0/255** (exact, 0 pixel > 2) à 0, 10,
    30 et 50 s. Wrapper `03-scenario-a/a6_apply.py` : déclencheur configuré
    `M6 - SDM - `, 295 → apply/audit pass, 187 → noop/audit pass, 295
    keyframé → review sans projet ; rendu relinké 0/255. Reste à l'intégrer
    au post-traitement de production ;
  - réserves : 179 sans `.ecproj` GOOD, 287 avec 1 référence sur 3, 2CV
    exclue ; **dérive de baseline identifiée** : STEP4/5 tracking legacy vs
    Rust, pas STEP6/8. Le STEP6 Rust sur le `_tracking.json` legacy reproduit
    le `_GOOD/_ae.json` à l'identique (4355/4355 sur 187, 2979/2979 sur
    181) ; recette de régénération documentée, delta d'ancre médian vs GOOD
    ramené de 62,75 à 8,4 px sur 187 et de 44,01 à 2,43 px sur 181. Le
    commit legacy `0e9765b` du 2026-10-16 19:01 (`STEP4_TPU_FAST=1`,
    `object_min_missing=25`) est postérieur aux GOOD ; `--object-min-missing
    1` remonte la couverture personne à 1722 frames vs 1623 legacy, mais ne
    ramène pas les ancres (64,33 px) : le gate n'explique que la couverture.
    Diagnostic : `/tmp/opencode/step9-spike/06-corpus/DIAGNOSTIC-DERIVE.md` ;
  - **clamp du panel** : correct pour une position centrée
    (`[C/(2s), W-C/(2s)]`) ; les ancres hors plage des GOOD sont des
    déplacements manuels de `Position` couverts par l'intervalle généralisé
    `[p/s, W-(C-p)/s]`, sauf 181 segment 34 (vide de 36,7 px) à signaler en
    `review` ;
  - décision D-030 ; rapports `docs/ETUDE-STEP9-AGENTIQUE.md` §3.6/§3.8,
    `docs/SPIKE-STEP9-PROTOCOLE.md` §4.6/§5.7/§8.1,
    `/tmp/opencode/step9-spike/06-corpus/RAPPORT-CORPUS.md` et
    `RAPPORT-A6.md` ;
  - prochaine étape : A5, validation A1 synthétique, A2 durci, intégration A6.
- [2026-10-26] **Spike STEP9 — Phase 0 terminée** :
  - baseline STEP8 reproductible sur 298/300 (calques identiques, seuls les
    chemins médias diffèrent) ;
  - labels produits : 118 segments bruts, 120 finaux, 0 ancre modifiée,
    23 keyframés, 72 clés, 1 fusion, 3 splits ;
  - A0/A2 validé : aucune bande sur 298 (no-op), bande de 108 px sur 300 →
    scale 207,4 % vs 207 % humain ; A1 no-op (aucun letterbox) ;
  - A3 limité : 39 % des clés humaines à ≤ 60 px d'un objet suivi ; le
    re-cadrage statique de 300 (29/36 et 35/60 segments à > 100 px du brut)
    n'est pas déterministe avec `_ae.json` seul ;
  - rapport : `/tmp/opencode/step9-spike/01-baseline/RAPPORT-PHASE0.md` ;
    aucun code de production modifié, aucune écriture dans `/mnt/cache`.
- [2026-10-26] **Étude STEP9 agentique et décision D-029** :
  - étude `docs/ETUDE-STEP9-AGENTIQUE.md` et protocole
    `docs/SPIKE-STEP9-PROTOCOLE.md` ;
  - analyse des projets témoins `298 Camille_GOOD.zip` et
    `300 Camille_GOOD.zip` : 117 segments comparables, 0 ancre modifiée,
    23/120 calques keyframés, 72 clés, 92 segments à +27 % d'échelle,
    1 fusion et 3 splits, 2 logos + 1 jingle sur 298 ;
  - décision : consolidation déterministe (A0–A5 : profilage, letterbox,
    overlays source, keyframes, transitions, rapport) d'abord, agent QA
    read-only (B) ensuite, co-monteur (C) en laboratoire seulement ; logos,
    jingle et rendu restent humains. Aucun code de production modifié,
    aucune écriture dans `/mnt/cache` ou `runtime/`.
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

- Poursuivre le spike A sur copies isolées, selon
  `docs/SPIKE-STEP9-PROTOCOLE.md` : finaliser le seuil de compensation A1,
  durcir A2, intégrer A6 comme module opt-in avec le profil mesuré de 295.
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
- `docs/ETUDE-STEP9-AGENTIQUE.md`
- `docs/SPIKE-STEP9-PROTOCOLE.md`
- `parity/DRIFT.md`
- `memory-bank/progress.md`
- `memory-bank/decisionLog.md`
