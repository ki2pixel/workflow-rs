# Contexte actif

## Tâche en cours

**Modularisation des règles Antigravity et création de `.antigravityignore` (2026-11-16, terminé).**
Résolution de l'erreur IDE « You have exceeded the maximum token limit for this file » (D-073) :
création de `.antigravityignore` à la racine (exclusion de `target/`, `runtime/`, `assets/models/`, `assets/mission_overlays/*.mov`, etc.), scission de la section §5 de `.agents/rules/codingstandards.md` vers `.agents/rules/pipeline-standards.md`, et passage du frontmatter en `trigger: model_decision` pour respecter le budget de tokens Antigravity.

## Dernière session

- [2026-11-16] **Modularisation règles Antigravity & ignore (D-073)** :
  - Création de `.antigravityignore` pour filtrer les répertoires volumineux du contexte ;
  - Densification générale de l'ensemble des règles (`pipeline-standards.md` à 4,0 Ko, `v5.md` à 2,9 Ko, `prompt-injection-guard.md` à 3,2 Ko, `codingstandards.md` à 4,8 Ko) ;
  - 100 % des règles du dépôt sont désormais strictement sous 5,4 Ko, éliminant les alertes IDE de limite de tokens.
- [2026-11-16] **Alignement du standard de codage (D-072)** :
  - §2 : `workflow-steps` expose `template`, `post/`, `qa/`, `vision/` ;
    `workflow-cli` documente `step1`…`step8`, `post`, `qa`, `retention`, `sync`,
    `webhook`, `email`, `download`, `history`, `config` ;
  - §3 : répartition `thiserror`/`anyhow` vérifiée dans le code (`TpuError` en
    `Display` manuel), exceptions de sortie `POST_*`/JSON, champs additifs ;
  - §4/§8 : dérivation `paths.base` (D-071), purge `.part`/`.tmp`/`.*.tmp.*`/
    `_temp_*`, `allow_remote` avec `secret_key` robuste, interdits post/QA ;
  - §5 : template direct SDM/LPHDF, post A1–A7 (`_assist.ecproj`, sidecar
    `schema: 2`, `tail_overlay`, banque D-066), QA `1.5.0`/cache 12 (D-067) ;
  - `fmt`, clippy et 689 tests listés (687 + 2 ignorés) verts ; seuls des
    commentaires de module alignés, aucun artefact de production modifié.

- [2026-11-16] **Audit backend Rust — actions P2/P3 (D-071)** :
  - `mission.rs` : `strip_chars`/`char_indices()` pour les préfixes accentués
    (`Chaîne`, `Émission`, `Animé`), tests minuscules/majuscules ; `HashSet`
    dans `split_logo_tokens` et `extract_delivery_links` ;
  - `config/config.toml` : `projects`, `archives`, `logs`, `downloads`,
    `history_db` et `steps.step1.source_dir` dérivés de `paths.base` ;
    `Config::step1_source_dir()` consommé par STEP1, le serveur et le moniteur ;
  - `fsutil.rs` : purge limitée à `.part`, `.tmp` et `.*.tmp.*` ;
    `lock.rs` : PID écrit et synchronisé après `flock` ;
  - `workflow-cli` : suffixe `_tracking.json` insensible à la casse,
    `block_on_cli` current-thread, `audit_missions` en `HashSet` ;
  - `validate_server` : `allow_remote = true` exige une `secret_key` non vide
    et non préfixée `dev-` ; contrat d'environnement 1.10 ;
  - 11 tests ajoutés ; suite workspace 689 listés (687 + 2 ignorés) ;
  - `fmt`, clippy et `cargo test --workspace` verts ;
    `scripts/validate-step1.py` passe ; aucun fichier de production modifié.

- [2026-11-16] **Audit backend Rust — actions P1 (D-070)** :
  - `download.rs` : `validate_url_host` sur l'URL initiale, `probe` et
    `redirect_policy` personnalisée (3 sauts) ; refus typé `ErrorKind::Auth` ;
  - `history.rs`/`email.rs` : `parking_lot::Mutex`, suppression des
    `.expect("... verrouillé")`, `spawn_blocking` pour `needs_detail`/`upsert` ;
  - `download.rs` : `file.sync_all()` après `flush`, `sync_parent` après
    `rename` ;
  - 3 tests ajoutés (refus direct, refus de redirection, finalisation
    durable) ; 55 tests `workflow-ingest`, 678 listés au workspace ;
  - `fmt`, clippy et `cargo test --workspace` verts ; aucun fichier de
    production modifié.

- [2026-11-16] **Synchronisation documentaire post-D-069** :
  - contrat `1.8` → `1.9`, date `2026-11-13` ; contenu D-069 déjà présent ;
  - `docs/README.md` daté `2026-11-13`, renvoi D-068 clarifié ;
  - `activeContext` et `progress.md` alignés sur 675 tests ; `fmt`, clippy et
    `cargo test --workspace -- --list` vérifiés ;
  - aucun fichier de production modifié.
- [2026-11-13] **Repli A7 sur asset local corrompu (D-069)** :
  - `LogoResolver::resolve_fallback_target` (workflow-core) et reprise de
    `resolve_assets` (a7) après `asset_unreadable` / `asset_dimensions` ;
  - warning `local_asset_corrupted_fallback_used` dans `warnings` du sidecar,
    sans changer `reasons`, `missing`, ni `source: central_fallback` ;
  - échantillon réel de `183 Camille_brut.zip` sous `/tmp/opencode` (15 octets,
    SHA-256 `52ce014e…b063a4`) ; banque `LFAUIT.png` vérifiée 35 582 octets,
    SHA-256 `cc9a036f…d4e10a` ;
  - 5 tests ajoutés (1 mission, 4 A7), suite 675 listés (673 + 2 ignorés),
    `fmt` et clippy verts ;
  - aucun code JSX, `.ecproj`, `runtime/`, `config/config.toml` ni `/mnt/cache`
    modifié.
- [2026-11-12] **Benchmark, masque QA MOV et canari (D-067)** :
  - corpus 932 emails, 319 lots ; historique 785 emails (90,9 % hors absences),
    récent 147 emails (100 %) ; `format_cohorts` dans `email audit --json` ;
  - masque QA des `.mov` alpha (12–24 frames, `kind: "mov"`, MOV instable
    ignoré), `QA_SCHEMA_VERSION = 1.5.0`, `CACHE_VERSION = 12` ;
  - canari 20 missions réelles sous `/tmp/opencode` : 20/20 `apply`, 28 assets,
    0 doublon, 0 `_assist` écrit ;
  - correctif `stem_key` (espace avant `.mp4`).
- [2026-11-12] **Archivage de la memory-bank (D-068)** :
  - `memory-bank/archives/` créé ; `sessions-2026-10-to-11.md`,
    `decisionLog-archive-2026-10.md` et `progress-archive-2026-10.md`
    contiennent l'historique déplacé verbatim, avec provenance, plage de
    lignes et SHA-256 de la source ;
  - `activeContext.md` 1 166 → 134 lignes, `decisionLog.md` 2 234 → 1374,
    `progress.md` 649 → 76 ; aucune ligne source supprimée sans être
    présente à l'identique dans une archive (contrôle automatique) ;
  - index ajouté en tête du journal des décisions ; D-059 signalé comme
    numéro réservé (option C A1, non ouverte) ;
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
- 689 tests unitaires listés (687 exécutés + 2 ignorés) ; pas de CI.
- Audit backend du 2026-10-24 remédié : 33 corrigés, 3 documentés
  (C3 → D-025, C4 → D-015, C8 → D-024).
- Audit backend du 2026-11-16 : actions P1 (AUD-01, AUD-02, AUD-03) remédiées
  et testées (D-070) ; constats P2/P3 (AUD-04 → AUD-10) remédiés et testés
  (D-071).
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
