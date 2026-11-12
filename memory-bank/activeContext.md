# Contexte actif

## Tâche en cours

**Lot 12.3 — benchmark, masque QA MOV et canari (2026-11-12, implémenté et
validé, D-067).**
Le benchmark consolidé du corpus (932 emails, 319 lots) distingue l'historique
2025–2026 (785 emails, 90,9 % de résolution hors absences) du récent
oct.–nov. 2026 (147 emails, 100 %) et les formats HTML seul/texte.
`email audit --json` expose `format_cohorts` ; `scripts/audit-a7-benchmark.py`
produit les tableaux. Le masque QA couvre désormais les calques `.mov` alpha :
12–24 frames échantillonnées en rawvideo RGBA, boîtes stables autour de la
médiane fusionnées (`kind: "mov"`), MOV instable ignoré avec warning non
bloquant. La fenêtre est ramenée au temps source pour un end-card placé tard ;
`QA_SCHEMA_VERSION = 1.5.0`, `CACHE_VERSION = 12`. Le canari de 20 missions
réelles récentes sous `/tmp/opencode` donne 20/20 `apply`, 28 assets (18
`central_fallback`, 10 `local_docs`), 0 doublon, 0 `_assist` écrit. Cas
limites validés : `known_absent`, manquant, ambigu, `tail_overlay`,
déduplication. Correctif : `stem_key` est `trim()`é après retrait de
l'extension (espace avant `.mp4`, lot 20). Suite workspace : 670 tests listés
(668 exécutés + 2 ignorés), `fmt` et clippy verts. Détail :
`docs/lots/LOT-12.3-BENCHMARK-QA-MOV.md` ; décision D-067. Aucun `.ecproj`,
`runtime/` ou `/mnt/cache` modifié.

## Dernière session

- [2026-11-12] **Archivage de la memory-bank (D-068)** :
  - `memory-bank/archives/` créé ; `sessions-2026-10-to-11.md`,
    `decisionLog-archive-2026-10.md` et `progress-archive-2026-10.md`
    contiennent l'historique déplacé verbatim, avec provenance, plage de
    lignes et SHA-256 de la source ;
  - `activeContext.md` 1 166 → 134 lignes, `decisionLog.md` 2 234 → 1374,
    `progress.md` 649 → 76 ; aucune ligne source supprimée sans être
    présente à l'identique dans une archive (contrôle automatique) ;
  - index ajouté en tête du journal des décisions ; D-059 signalé comme
    numéro réservé (option C A1, non ouverte) ; compteur de tests porté à
    670 (668 exécutés + 2 ignorés) ;
  - aucun code, `.ecproj`, `runtime/`, `config/config.toml` ni `/mnt/cache`
    modifié ; aucun commit.
- [2026-11-12] **Synchronisation documentaire post-D-067** : README, AGENTS,
  contrat 1.8, skills et aide CLI alignés sur A5/A6/A7 et 670 tests ; aucun
  changement de comportement. Détail dans l'archive des sessions.

## État du système

- Backend de production : Rust `:5004` headless via
  `scripts/switch-rust.sh` ; Python legacy `:5003` arrêté (rollback
  disponible).
- Interface : page web legacy `assets/web` uniquement ; aucun client desktop
  natif (D-028).
- Webhook Rust activé dans la configuration depuis le 2026-10-23
  (`enabled = true`) ; il ne tourne que si le serveur est démarré.
- 670 tests unitaires listés (668 exécutés + 2 ignorés) ; pas de CI.
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

1. **D-024** : diagnostiquer l'écart STEP4 Python sur `153 Camille` (4 977
   objets contre 11 853) sans modifier les données de production ; ne pas
   utiliser `--prefer-raw` sur ce projet.
2. **Bascule publique** : installer le reverse proxy (aucun nginx/caddy/
   cloudflared) après lecture de `docs/exploitation/SWITCH-PUBLIC.md`.
3. **Surveillance production** : suivre le moniteur webhook, les
   téléchargements et `history stats` après quelques jours d'activité.
4. **A5/A6** : exposer `review.json` en lecture seule dans l'UI, recalibrer
   A3/A4 et traiter les 179/287 références manquantes
   (`docs/lots/LOT-9-POST-STEP8.md`).
5. **QA logos** : faire signer les 12 labels dev (`review_status = done`) et
   valider DINOv2 sur des logos inconnus et des masques partiels avant toute
   activation par défaut.
6. **D-056** : décider de l'activation de R2 (`a1_defer_on_a3`) et R6
   (`a3_relaxed_gap`) ou les laisser opt-in.
7. **Documentation** : maintenir `README.md`, `docs/README.md`,
   `docs/lots/LOT-*.md`, `CONTRAT-ENVIRONNEMENT.md`, `parity/DRIFT.md` et la
   memory-bank ; les archives restent en lecture seule.

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

## Archives et traçabilité

- [Archives des sessions 2026-10 à 2026-11](file:///home/kidpixel/workflow-rs/memory-bank/archives/sessions-2026-10-to-11.md)
  (`memory-bank/archives/sessions-2026-10-to-11.md`) : lots B2 → 12.2, étude
  corpus 248, Grounding DINO, registre des logos et sessions du 2026-10-24 au
  2026-11-12.
- [Archives des décisions historiques (octobre 2026)](file:///home/kidpixel/workflow-rs/memory-bank/archives/decisionLog-archive-2026-10.md)
  (`memory-bank/archives/decisionLog-archive-2026-10.md`) : D-028 → D-040,
  brouillons D-058, bootstrap, migration et audits d'octobre.
- [Archives de progression (octobre 2026)](file:///home/kidpixel/workflow-rs/memory-bank/archives/progress-archive-2026-10.md)
  (`memory-bank/archives/progress-archive-2026-10.md`) : spikes STEP9,
  prototypes A5/A6, rollback Tauri, chronologie de migration et validations.
- L'index des lots et de leurs décisions vit dans `memory-bank/progress.md`
  § Synthèse des Lots ; les fiches détaillées dans `docs/lots/LOT-*.md`.
- Les archives sont en lecture seule : une nouvelle décision se consigne dans
  le fichier actif avec un renvoi vers l'archive.

## Références

- `AGENTS.md`
- `README.md`
- `docs/README.md`
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`
- `docs/lots/LOT-*.md`
- `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`
- `docs/etudes/SPIKE-STEP9-PROTOCOLE.md`
- `parity/DRIFT.md`
- `memory-bank/progress.md`
- `memory-bank/decisionLog.md`
