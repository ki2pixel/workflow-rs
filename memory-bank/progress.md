# Suivi de progression

## Statut actuel

| Composant | État au 2026-11-16 | Référence |
| --- | --- | --- |
| Pipeline STEP1 → STEP8 | Huit étapes portées et validées ; STEP7 publie et archive, STEP8 est headless (`effectcraft-cli`). | `docs/lots/LOT-*.md`, `parity/DRIFT.md` |
| Post-traitement A5/A6/A7 | A5/A6 en production depuis D-032 ; A7 résout les missions, la banque centrale, le masque QA MOV (D-064 → D-067) et reprend un asset local corrompu sur la banque (D-069). Brut jamais modifié ; `_assist.ecproj` publié hors `--report-only`. | `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-12*.md` |
| Bypass SDM/LPHDF | Bascule directe opt-in pour les profils 1920×1080 25 fps H.264 ; préfixes `M6 - SDM - `, `SDM - ` et `W9 - LPHDF` ; depuis D-080, une mission unique demandant exclusivement `sdm.png` déclenche aussi le template sans préfixe (lot 216). | D-034, D-035, D-061, D-080 |
| Ingestion | Webhook Rust, normalisation d'URL, historique SQLite, téléchargements R2/Dropbox ; un seul moniteur actif. Audit P1 remédié (D-070) : anti-SSRF sur IP littérales et redirections, `parking_lot`, SQLite hors workers Tokio, `fsync` des archives. Audit P2/P3 remédié (D-071) : chemins dérivés de `paths.base`, purge d'orphelins ciblée, PID du verrou. | `docs/lots/LOT-8-INGESTION.md`, `docs/validation/audits/2026-11-16-audit-backend-rust.md` |
| Serveur `:5004` | axum, une étape à la fois (409), arrêt `stopping` puis SIGTERM/SIGKILL ; Python `:5003` arrêté, rollback disponible. | `docs/lots/LOT-5-ORCHESTRATOR.md` |
| QA read-only | Schéma `1.5.0`, cache 12, masque des `.mov` alpha (12–24 frames), DINOv2 actif par défaut (opt-out). | D-049, D-051, D-067 |
| Parité | STEP2/3/4 à `1e-6`, shadow `162/162`, CSV/JSON contractuels préservés. | `parity/DRIFT.md`, `docs/validation/VALIDATION-STEP1-8.md` |
| Tests et outillage | 704 tests listés (702 exécutés + 2 ignorés), `fmt` et `clippy -D warnings` verts ; pas de CI. | `AGENTS.md` |
| Standards agents | `codingstandards.md` aligné sur `template`, `post/`, `qa/`, `vision/`, les sous-commandes CLI et les décisions D-062 → D-072. | D-072 |

- Backend de production : Rust `:5004` headless via `scripts/switch-rust.sh` ;
  interface web legacy `assets/web` uniquement (D-028).
- Webhook activé dans la configuration depuis le 2026-10-23 ; il ne tourne que
  si le serveur est démarré.
- Verrou matériel : `/tmp/workflow-rs-hw.lock` ; runtime `runtime/`, sortie
  finalisée `/mnt/cache`.
- 2026-11-16 : template SDM par mission (D-080) — lot 216 livré sans préfixe
  `SDM - ` mais avec `sdm.png` dans la mission unique. `SdmTemplateIndex`
  (workflow-core) indexe les couples lot/vidéo éligibles ; `Bypass` et A6 le
  consultent, STEP2 synchronise le cache une fois, STEP3–STEP7 le relisent.
  `mission_template = true` en production ; `template.trigger` et
  `template.mission` sont additifs, `template.prefix` reste vide. Repli strict
  sur le nommage si mission absente, ambiguë, multiple ou hors profil.
- 2026-11-16 : audit frontend Rust, Correctif 6 (AUD-FE-10, AUD-FE-12,
  D-079) — `#logs-column-global` porte `role="dialog"`, `aria-modal="true"` et
  `aria-labelledby="log-panel-title"`, sans modification de `popupManager.js`
  ni de `uiUpdater.js`. `layout.css` perd les règles mortes de
  `.local-downloads-section` et le grid compact `.steps-column` ;
  `controls.css` perd les règles écrasées de `#run-all-steps-button`.
  `components/downloads.css`, `components/steps.css` et
  `components/workflow-buttons.css` deviennent les sources uniques, avec
  `transition: opacity` (mode compact) et `min-height: var(--touch-target-min)`
  déplacées pour conserver les styles calculés. A/B Firefox 158 headless à
  1400 px et 700 px : styles et tailles identiques, seuls quatre longhands grid
  inertes diffèrent sous `display: flex` ; `tinycss2` 0 erreur, assertions ARIA
  dans le test de rendu Minijinja, tests serveur ciblés,
  `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés), `fmt`,
  clippy et `git diff --check` verts. Les douze constats AUD-FE-01 → AUD-FE-12
  sont clos.
- 2026-11-16 : audit frontend Rust, Correctif 5 (AUD-FE-07, AUD-FE-09,
  AUD-FE-11, D-078) — `WorkerManager` résout `null` sur une réponse périmée
  (chemin Worker et fallback synchrone) au lieu de rejeter, donc
  `unhandledrejection` n'affiche plus d'erreur pendant les rafraîchissements
  rapides de logs ; `destroy()` est attaché à `beforeunload` ;
  `updateSpecificLogUI`/`updateMainLogOutputUI` ignorent `null`. Les sept
  imports de `PollingManager.js` perdent le suffixe `?v=20261025a` (Axum force
  déjà la revalidation). L'aperçu CSV affiche les huit étapes canoniques et
  « étapes 1-8 ». `node --check` sur neuf modules, harnais Node 6/6 et 5/5,
  tests serveur ciblés, `cargo test --workspace`, `fmt`, clippy et
  `git diff --check` verts ; contrats HTTP/JSON/CSV et zéro-build inchangés.
- 2026-11-16 : audit frontend Rust, Correctif 4 (AUD-FE-08, D-077) — les six
  `box-shadow` de `steps.css` (survol, focus, sélection, statut `running`) et
  de `csv-workflow-prompt.css` (`.workflow-launch-btn` et son survol) passent
  de `rgba(var(--*-rgb), <alpha>)` à `rgb(var(--*-rgb) / <alpha>)`. La première
  forme se résout en `rgba(121 134 203, 0.3)`, invalide en CSS Color 4 et donc
  ignorée par le moteur ; les ombres sont désormais rendues. Aucune valeur
  d'alpha, aucun sélecteur, aucun contrat HTTP/JSON/CSV modifié, aucun
  préprocesseur ajouté. `grep` résiduel vide, `tinycss2` sans erreur, A/B
  Chromium sur les six déclarations, tests serveur ciblés,
  `cargo test --workspace`, `fmt` et clippy verts.
- 2026-11-16 : audit frontend Rust, Correctif 3 (AUD-FE-03, AUD-FE-04,
  AUD-FE-11, D-076) — `subscribeToProperty()` compare via `_areValuesEqual`
  (fin des notifications de `csvDownloads` à chaque tick `processInfo`) ;
  `getStateProperty()` clone profondément les lectures, avec préservation des
  références hôtes (nœud DOM du focus) ; `reset()` remplace l'état complet et
  vide réellement `processInfo`, `stepTimers` et `pollingIntervals`, avec 11
  clés `ui` et `theme: 'dark-pro'`. Harnais Node 15/15, `node --check`, tests
  serveur ciblés, `cargo test --workspace`, `fmt` et clippy verts ; contrats
  HTTP/JSON/CSV et architecture zéro-build inchangés.
- 2026-11-16 : audit frontend Rust, Correctif 2 (AUD-FE-02, AUD-FE-05,
  D-075) — `generatePerformanceReport()` appelle `errorHandler.getErrorStats()`
  (fin du `TypeError` périodique) ; `perf-memory-usage`,
  `perf-periodic-reporting` et `perf-optimizer-stats` passent
  `{ pauseWhenHidden: true }` ; `sendPerformanceReport()` utilise
  `this.originalFetch` et transporte `X-Worker-Token`, donc la télémétrie ne
  pollue plus `metrics.apiCalls`. Payload et handler serveur inchangés ; tests
  serveur ciblés, `cargo test --workspace`, `fmt` et clippy verts.
- 2026-11-16 : audit frontend Rust, Correctif 1 (AUD-FE-01, AUD-FE-06,
  D-074) — `DOMUpdateUtils.escapeHtml` échappe `&`, `<`, `>`, `"` et `'` par
  remplacements de chaînes sans allocation DOM ; `handleStepFailure` échappe
  `errorMessage` avant `updateMainLogOutputUI`. Aucun changement des sélecteurs
  DOM ni des contrats HTTP/JSON/CSV, aucune dépendance Node/npm. Tests serveur
  ciblés, `cargo test --workspace`, `fmt` et clippy verts.
- 2026-11-16 : alignement du standard de codage (D-072) — §2 structure des
  crates et sous-commandes, §3 `thiserror`/`anyhow` et sorties contractuelles,
  §4/§8 isolation `paths.base` et exposition réseau, §5 template direct,
  post-traitement A1–A7 et QA read-only. Aucun changement de comportement ni
  fichier de production modifié ; seuls des commentaires de module ont été
  alignés. `fmt`, clippy et 689 tests listés (687 + 2 ignorés) verts.
- 2026-11-16 : remédiation des constats P2/P3 de l'audit backend Rust (D-071) —
  parseur UTF-8 par caractères, chemins dérivés de `paths.base` (isolation
  complète par `WORKFLOW_PATHS_BASE`), purge d'orphelins ciblée, PID dans
  `HardwareLock`, runtime Tokio `current_thread` et `HashSet` dans le CLI,
  `secret_key` exigée avec `allow_remote = true`. Suite workspace : 689 tests
  listés (687 exécutés + 2 ignorés), 0 échec.
- 2026-11-16 : remédiation des constats P1 de l'audit backend Rust (D-070) —
  anti-SSRF sur IP littérales et chaîne de redirections, `parking_lot::Mutex`
  sans empoisonnement, SQLite déporté hors des workers Tokio dans
  `EmailProxyClient::sync`, `sync_all`/`sync_parent` des archives. Suite
  workspace : 678 tests listés (676 exécutés + 2 ignorés), 0 échec.
- 2026-11-16 : synchronisation documentaire post-D-069 (contrat `1.9`,
  `docs/README.md`, `activeContext` et ce fichier) ; aucun changement de code,
  de configuration ou de production.

## Synthèse des Lots

| Lot | Période | Livrable | Documentation | Décision |
| --- | --- | --- | --- | --- |
| 1 | 2026-10-22 | STEP5/6/7/8, bridge AE, shadow | `LOT-1-STEP5.md` … `LOT-1-SHADOW.md` | D-001 → D-027 (écarts) |
| 2 | 2026-10-22 | STEP2 TransNetV2 ONNX CUDA | `LOT-2-STEP2.md` | D-009, D-011 |
| 3 | 2026-10-22 | STEP4 tracking Coral | `LOT-3-STEP4.md` | D-004, D-008 |
| 4 | 2026-10-22 | STEP3 audio YAMNet/ECAPA/AHC | `LOT-4-STEP3.md` | D-002, D-003 |
| 5 | 2026-10-23 | Serveur axum `:5004`, séquence, arrêt | `LOT-5-ORCHESTRATOR.md` | D-013, D-025, D-027 |
| 6 | 2026-10-23 | STEP1 extraction et normalisation | `LOT-6-STEP1.md` | D-016, D-017 |
| 7 | 2026-10-23 | Intégration STEP2 → STEP8 | `LOT-7-INTEGRATION.md` | D-023, D-026 |
| 8 | 2026-10-23 | Ingestion webhook, SQLite, R2/Dropbox | `LOT-8-INGESTION.md` | D-018 → D-021 |
| 9 | 2026-10-27 → 2026-11-09 | Post-traitement A5/A6, bypass, multi-préfixes | `LOT-9-POST-STEP8.md` | D-030 → D-035, D-050, D-061 |
| 10 | 2026-10-30 → 2026-11-02 | QA read-only, Stage 0/1, DINOv2 | `LOT-10-QA.md` | D-036 → D-049 |
| 11 | 2026-11-11 | Veto latéral A1 (pillarbox) | `LOT-11-A1-VETO-LATERAL.md` | D-063 |
| 12 | 2026-11-12 | Logos de mission A7 | `LOT-12-A7-MISSIONS.md` | D-064 |
| 12.1 | 2026-11-12 | Parseur, alias, `tail_overlay` | `LOT-12.1-PARSEUR-ALIAS-TAIL.md` | D-065 |
| 12.2 | 2026-11-12 → 2026-11-13 | Banque centrale de repli A7, reprise sur asset local corrompu (D-069) | `LOT-12.2-BANQUE-CENTRALE.md` | D-066, D-069 |
| 12.3 | 2026-11-12 | Benchmark, masque QA MOV, canari | `LOT-12.3-BENCHMARK-QA-MOV.md` | D-067 |

Études hors lots : YOLO/ONNX B0 → B2.1 (`docs/etudes/ETUDE-YOLO-ONNX-A2.md`,
D-060/D-061), corpus 248 (`docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`,
D-062/D-063), VS letterbox A1 (`docs/etudes/ETUDE-VSLETTERBOX-A1.md`, D-058),
Grounding DINO (`docs/etudes/ETUDE-GROUNDING-DINO-A2.md`, D-060).

## Prochaines étapes

1. **D-024** : diagnostiquer l'écart STEP4 Python sur `153 Camille` (4 977
   objets contre 11 853) sans modifier la production ; ne pas utiliser
   `--prefer-raw` sur ce projet.
2. **Bascule publique** : installer le reverse proxy (aucun nginx/caddy/
   cloudflared) après lecture de `docs/exploitation/SWITCH-PUBLIC.md`.
3. **Surveillance** : suivre le moniteur webhook, les téléchargements et
   `history stats` après quelques jours d'activité.
4. **A5/A6** : exposer `review.json` en lecture seule dans l'UI, recalibrer
   A3/A4 et traiter les 179/287 références manquantes.
5. **QA logos** : faire signer les 12 labels dev et valider DINOv2 sur des
   logos inconnus et des masques partiels avant activation par défaut.
6. **D-056** : décider de l'activation de R2 (`a1_defer_on_a3`) et R6
   (`a3_relaxed_gap`) ou les laisser opt-in.
7. **Documentation** : maintenir `README.md`, `docs/README.md`,
   `docs/lots/LOT-*.md`, `CONTRAT-ENVIRONNEMENT.md`, `parity/DRIFT.md` et la
   memory-bank ; les archives restent en lecture seule.

## Historique et archives

- [Archives de progression (octobre 2026)](file:///home/kidpixel/workflow-rs/memory-bank/archives/progress-archive-2026-10.md)
  (`memory-bank/archives/progress-archive-2026-10.md`) : corps narratif
  complet du 2026-10-22 au 2026-11-12, déplacé verbatim par D-068.
- Ce fichier ne conserve que le statut par composant, la synthèse des lots et
  les jalons. Fenêtre de détail : 90 jours ; au-delà, résumer et déplacer vers
  `memory-bank/archives/`.
- L'historique Python legacy reste dans
  `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.
