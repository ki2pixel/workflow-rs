# Suivi de progression

## Statut actuel

- Migration Rust terminée pour les 8 étapes.
- Backend de production : Rust `:5004` headless via
  `scripts/switch-rust.sh` ; Python legacy `:5003` arrêté (rollback
  disponible).
- Interface : page web legacy `assets/web` uniquement ; client desktop Tauri
  abandonné (D-028).
- Rollback au commit `a93a554` validé : 280 tests au checkpoint, 282 après
  correctifs, fmt/clippy, `config check` et build release.
- Moniteur webhook Rust activé dans la configuration depuis le 2026-10-23
  (`webhook.enabled = true`).
- 315 tests unitaires, sans matériel.
- Chaîne STEP1 → STEP8 validée sur `1720 Camille.zip`, puis sur `1531 Camille`
  via le frontend Rust (8/8 étapes `completed`).
- Widget système Rust : CPU/RAM/disque et GPU `nvidia-smi` ; auto-ouverture
  des logs désactivée par défaut (toggle conservé).
- Journal serveur enrichi (configuration, requêtes, webhook, étapes,
  séquences), ANSI désactivé à l'écriture fichier et terminal limité à la
  session courante au démarrage.
- Lanceur Rust : la fermeture de la fenêtre de terminal arrête désormais le
  serveur `:5004` (SIGHUP intercepté, `cleanup` robuste au pty détruit).
- Shadow STEP5/6 `162/162` sur 20 projets.
- Sync réelle : 911 URL, 96 archives, 640 hash STEP7 (1,8 Go).
- Audit documentaire du 2026-10-24 : commandes de parité corrigées et état de
  production aligné sur Rust `:5004`.
- Remédiation de l'audit backend du 2026-10-24 : 33 constats corrigés,
  3 documentés (C3 → D-025, C4 → D-015, C8 → D-024). `cargo fmt --check`,
  clippy strict, 280 tests et `config check` verts.
- Synchronisation documentaire du 2026-10-24 : statut `stopping`, en-têtes de
  sécurité, `server.allow_remote`, quotas STEP1, purge des résidus et index
  D-025 à D-027 alignés sur le code. Audit `.agents/` : `codingstandards.md` et
  les 22 `SKILL.md` alignés (tests 280, `max_workers` retiré, verrou STEP1,
  gate EffectCraft, legacy en lecture seule).
- Étude STEP9 agentique terminée (D-029) : consolidation déterministe (A0–A5 :
  profilage, letterbox, overlays source, keyframes, transitions, rapport)
  d'abord, agent QA read-only (B) ensuite, co-monteur (C) en laboratoire.
  Logos, jingle et rendu restent humains. Aucun code de production modifié.
- **Post-traitement A5/A6 intégré (D-032)** : après un STEP8 réussi, le brut
  reste intact et `<stem>_assist.ecproj` est toujours publié, avec
  `post.json`/`review.json`/`audit.json` sous `docs/post/`. A1 letterbox, A2
  bannière, A3/A4 en revue, A6 template SDM. Commande `workflow-cli post`.
  Validation 295 au pixel (0/255), parité de décisions avec le prototype sur
  268/187/191/181, 270/276/286/287 sans changement d'échelle, A2 synthétique
  11/11, 315 tests, fmt/clippy verts. Reste : exposer `review.json` dans
  l'UI, recalibrer A3/A4 et traiter 179/287.

## Synchronisation documentaire (2026-10-28)

- Audit du workflow `docs-updater` : 315 tests (283 `#[test]` + 32
  `#[tokio::test]`), `clippy -D warnings` et `cargo fmt --check` verts, arbre
  git propre, aucun TODO/FIXME/HACK dans `crates/`.
- Comptes de tests corrigés de 282 à 315 dans `memory-bank/systemPatterns.md`,
  `memory-bank/productContext.md`, `.agents/rules/test-strategy.md`,
  `.agents/skills/tests-suite-guardian/`, `.agents/skills/workflow-operator/`
  et `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`.
- `parity/DRIFT.md` D-015 : `/api/system_monitor` sert des métriques réelles
  CPU/RAM/disque/GPU ; seul `/api/performance/frontend` reste un stub.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md` passe en version 1.5 :
  nomenclature STEP8 avec `_assist.ecproj` et `docs/post/`, panneau
  `assets/effectcraft/MediaSolution.jsx`, 161 clés TOML et sections réelles
  (`[legacy]`, `[steps.filter]`, `[steps.step8.post]`, `[retention]`).
- `docs/lots/LOT-1-STEP8.md` pointe la copie versionnée du panneau ;
  `docs/lots/LOT-8-INGESTION.md` date ses comptes de tests de clôture.
- `.agents/skills/workflow-operator/` documente le hook A5/A6 obligatoire
  après STEP8 (`--no-post`, `post run`, `--report-only`) et les artefacts
  `_assist`.

## Post-traitement A5/A6 (2026-10-27)

- Code : `crates/workflow-steps/src/post/` ; hook dans `step8.rs` ;
  commande `workflow-cli post run`.
- Invariants : brut inchangé, `_assist.ecproj` toujours publié (copie exacte
  sans décision), artefacts additifs, A1/A2 sans déplacement d'Anchor Point,
  A3/A4 en revue, A6 éligible court-circuite A1–A4, conflit A1/A2 → revue.
- Validation : 295 assist identique à la référence et rendu 0/255 à
  0/10/30/50 s ; décisions identiques au prototype sur 268, 187 DISNEYWORLD,
  187 ECOLE, 191 et 181 ; 270/276/286/287 0 apply et assist identique au
  brut ; A2 synthétique 11/11 ; idempotence 187 ECOLE.
- Configuration : `[steps.step8.post]` ; `--no-post` pour désactiver ;
  `post run --report-only` pour ne pas publier d'assist.
- Documentation : `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-1-STEP8.md`,
  `docs/architecture/CONTRAT-ENVIRONNEMENT.md`, étude §3.9, protocole §14.1.
- Limites : pas d'exposition UI/CEP, A3/A4 en revue, bords non noirs et
  recadrages sémantiques en revue, A6 multi-segment/keyframé en revue,
  timeout serveur STEP8 incluant le post, 179/287 partiels.

## Validation production A5/A6 (2026-10-28)

- Batch STEP2–8 sur `223 Camille` (2 vidéos) : 7/7 étapes `completed`,
  STEP8 2/2 projets, post 2 assist, 0 apply, 13 review, 84 noop, 0 échec,
  audit `pass` sur les deux projets.
- Brut et assist **octet pour octet identiques** (aucune action auto) ;
  `_ae.json`, `_audio.json` et CSV non modifiés (mtimes antérieurs au post).
- Revues : A1 barres asymétriques haut 6 / bas 82 (2 segments Audi, 1
  Peugeot), A2 bande animée (Audi) / logo étroit (Peugeot), A3 7 candidats
  (désactivé), A4 segment court 0,32 s (Peugeot). A6 non déclenché.
- Clamps panneau Peugeot segments 16/19/21 (300/1620 px) : couverture exacte,
  `source_coverage` pass, pas de revue A5.
- Le STEP8 isolé de 00:28 avait échoué sur l'ancien manifeste
  `/mnt/cache/309 Camille` (dossier introuvable), avant le batch ; sans lien.

## Étude STEP9 agentique (2026-10-26)

- Documents : `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`,
  `docs/etudes/SPIKE-STEP9-PROTOCOLE.md`.
- Corpus : `298 Camille_GOOD.zip` et `300 Camille_GOOD.zip`, comparés aux
  sorties STEP8 brutes.
- Constats : 117 segments comparables, 0 ancre STEP6 modifiée, 23/120 calques
  keyframés (72 clés), 92 segments à +27 % d'échelle sur 300, 1 fusion et
  3 splits, 2 logos + 1 jingle sur 298.
- Décision D-029 : A0–A5 maintenant, B en PoC, C en laboratoire ; pas d'agent
  en écriture sur la production, pas de dépendance cloud bloquante ; logos,
  jingle et rendu restent humains.
- Prochaine étape : spike A sur copies isolées, sans toucher aux 8 étapes.

### Phase 0 du spike (2026-10-26)

- Baseline STEP8 reproductible : calques identiques sur 298/300, seuls les
  chemins médias changent.
- Labels : 118 segments bruts, 120 finaux, 0 ancre modifiée, 23 keyframés,
  72 clés, 1 fusion, 3 splits, Null parent 118 → 0.
- A0/A2 : bande de 108 px détectée sur 300 → scale 207,4 % (humain 207 %) ;
  no-op sur 298 ; A1 no-op (aucun letterbox).
- A3 : 39 % des clés humaines à ≤ 60 px d'un objet suivi ; règle de sélection
  précision 0,62 (298), 0,29 (JOSEPH), 0,20 (KATIA). Re-cadrage statique de
  300 non déterministe avec `_ae.json` seul.
- Rapport : `/tmp/opencode/step9-spike/01-baseline/RAPPORT-PHASE0.md`.
- Prochaine étape : A2 sur copie (rendu de contrôle), prototype A3 conservateur
  sur 298, A4 splits/transitions.

### Corpus élargi du spike (2026-10-27)

- 19 paires / 32 vidéos exploitables : 179, 181, 187, 191, 208, 211, 221,
  223, 227, 266, 268, 270, 276, 286, 287, 295, 298, 300, 303.
- 1059 segments bruts, 520 clés humaines, 145 segments keyframés.
- 278/520 clés (53,5 %) à ≤ 60 px d'un objet suivi ; 458/907 segments
  statiques (50,5 %) à ≤ 50 px du brut, 363/907 (40 %) à > 100 px.
- A3 invalidé en auto : précision 0,20, rappel 0,23 ; 35 faux positifs sur
  les vidéos « sans piège ». Décision D-030 : candidats en `review` ou flag
  projet.
- A1 validé sur le letterbox noir (191 : 249,6 % vs 255 % avec marge 4 % ;
  268 : 179,1/222,2 % par segment) ; bords non noirs de 268 en `review`.
- A2 fragile : 207,4 % correct sur 187 DISNEYWORLD, faux positif à 260 % sur
  187 ECOLE ; logos de coin (191) et logo de template (295, `sdm.png`) hors
  A2 ; corrigé en v2 (D-031).
- A6 SDM validé sur le GOOD 295 reçu le 2026-10-27 : brut et GOOD partagent
  le même `_ae.json`/`_tracking.json` (SHA-256 identiques). Template fond
  180 % flouté 50 + panneau 84,3497774862575 % à y=171,032 + panneau
  56,294573830734535 % à y=1082,026 + logo `sdm.png`, reproduit par
  `06-corpus/a6_template.py` à 0/255 (exact, 0 pixel > 2) à 0, 10, 30 et
  50 s. Wrapper `03-scenario-a/a6_apply.py` : déclencheur configuré
  `M6 - SDM - `, 295 → apply/audit pass, 187 → noop/audit pass, 295
  keyframé → review sans projet ; rendu relinké 0/255. Reste à l'intégrer
  au post-traitement de production.
- Réserves : 179 sans `.ecproj` GOOD, 287 avec 1 référence sur 3, 2CV
  exclue ; dérive de baseline identifiée comme STEP4/5 (tracking legacy
  vs Rust), pas STEP6/8. Le STEP6 Rust sur le `_tracking.json` legacy
  reproduit le `_GOOD/_ae.json` à l'identique (4355/4355 sur 187,
  2979/2979 sur 181) ; delta d'ancre médian vs GOOD ramené de 62,75 à
  8,4 px sur 187 et de 44,01 à 2,43 px sur 181 avec la recette legacy.
  Le commit legacy `0e9765b` du 2026-10-16 19:01 (`STEP4_TPU_FAST=1`,
  `object_min_missing=25`) est postérieur aux GOOD ; `--object-min-missing
  1` remonte la couverture personne à 1722 frames vs 1623 legacy, mais ne
  ramène pas les ancres (64,33 px) : le gate n'explique que la couverture.
  Diagnostic : `/tmp/opencode/step9-spike/06-corpus/DIAGNOSTIC-DERIVE.md`.
- Clamp du panel : correct pour une position centrée (`[C/(2s),
  W-C/(2s)]`) ; les ancres hors plage des GOOD sont des déplacements
  manuels de `Position` couverts par l'intervalle généralisé
  `[p/s, W-(C-p)/s]`, sauf 181 segment 34 (vide de 36,7 px) à signaler en
  `review`.
- Rapports : `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` §3.6/§3.8,
  `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` §4.6/§5.7/§8.1,
  `/tmp/opencode/step9-spike/06-corpus/RAPPORT-CORPUS.md`,
  `RAPPORT-A6.md` et `DIAGNOSTIC-DERIVE.md`.
- Prochaine étape : A5 (`post.json`, `review.json`, `audit.json`), validation
  A1 sur letterbox synthétique, puis A2 durci et intégration A6.

### A5 v2 et A6 intégré (2026-10-27)

- `a5_report.py` v2 : hash vidéo échantillonné (3 × 4 Mio), `timings_ms`,
  A1 10e percentile + couverture mathématique + keyframes en `review`, A2
  haut/bas + sombre + géométrie + logo/partiel/décor, `--template sdm`.
- A1 : 268 15 applies à 231,1 % + Y −287,9 px ; 191 37 applies à 249,6 % +
  Y −367,8 px enfant ; 181 Concierge 0 apply, 10 barres asymétriques en
  `review`. Rendu 268 : 194/170 px de barres → 0/0 à 2,5/5/9/24,5/35/54 s.
- A2 : 187 DISNEYWORLD/ECOLE 206,6 %, géométrie 1,3/2,6 et 0/0 px ; 191
  logo review ; 268/181 noop. Validation synthétique 11/11 : 3 apply,
  7 review, 1 noop.
- A6 : `a5_report.py --template sdm` émet la décision et le bloc `template` ;
  295 apply/audit pass, 295 keyframé review, 187 noop. `a6_apply.py --apply`
  reste le seul writer du projet.
- Coût A5 : 3,7–33,5 s par vidéo (0,07–0,24× la durée, sans rendu).
- Rapport : `/tmp/opencode/step9-spike/06-corpus/a5v2/RAPPORT-A5-V2.md` ;
  décision D-031.
- Prochaine étape : brancher A5/A6 dans le post-traitement de production,
  hors panel ; références 179/287 toujours manquantes.

### Prototype A5 (2026-10-27)

- `03-scenario-a/a5_report.py` : rapport déterministe A0–A4, écrit
  `post.json`/`review.json`/`audit.json` sans modifier le `.ecproj`.
- 4 cas : 268, 187 DISNEYWORLD, 187 ECOLE, 191 cancer.
- A2 : 206,6 % vs 207 % sur 187 (0,4 %) ; décor de 187 ECOLE séparé ; logo
  de 191 en `review`.
- A1 : 231,1 % (268) dans la plage 221,3–239,7 % ; 249,6 % vs 255 % (191,
  2,1 %). Validation visuelle : A2 propre ; A1 laisse 19,5 % de barre en haut
  sur 191 (ancre décentrée), mais la compensation Position Y de -417,8 px
  supprime les barres. A5 prescrit scale + compensation et signale le cadrage
  vertical en `review`.
- A3 : précision 0,29–0,60, rappel 0,20–0,43, jamais appliqué.
- A4 : cut dur intra-segment désactivé par défaut (32 faux positifs sur 187).
- Rapport : `/tmp/opencode/step9-spike/06-corpus/a5/RAPPORT-A5.md`.
- Prochaine étape : contrôle de couverture A1 (Position Y), A2 durci,
  intégration A6 opt-in avec le profil mesuré.

## Rollback Tauri (2026-10-26)

- Décision D-028 : abandon du client desktop Tauri et retour à la stack Rust
  headless avec `assets/web`.
- `git reset --hard` sur `a93a554c4c5ed5e17805575cf15ac533d7cd50a3` ; branche
  de sauvegarde `backup/tauri-abandon-20261026` sur `1613b17`.
- Crates desktop, paquet `workflow-manager`, unité `workflow-server.service`,
  scripts PoC et documentation desktop retirés ; `target/` purgé (3,8 Gio).
- Validation : 280 tests au checkpoint, 282 après correctifs, fmt/clippy,
  `config check`, build release, serveur `:5004` seul.
- Correctifs réappliqués : `514e2d6` (ETXTBSY), `4ddb91b` (secrets, 281)
  et `713b6bc` (polling web masqué, 282). La réorg docs reste dans la
  branche de sauvegarde.

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

## Réorganisation documentaire (2026-10-28)

- `docs/` passe de 23 documents à plat à six domaines : `architecture/`,
  `lots/`, `exploitation/`, `decisions/`, `etudes/`, `validation/`.
- 24 fichiers déplacés par `git mv` en deux commits : move pur détecté à
  100 %, puis réécriture des renvois.
- `docs/README.md` devient l'index : taxonomie, statuts, parcours par profil,
  table par étape et correspondance ancien → nouveau.
- 211 renvois réécrits dans 30 fichiers (README, AGENTS, règles, skills,
  workflow, memory-bank, parity) et 18 renvois internes normalisés en chemins
  repo-relatifs.
- Convention audits : `docs/validation/audits/AAAA-MM-JJ-<sujet>.md` (D-033).
- Les chemins applicatifs `<projet>/docs/` et `docs/post/` sont inchangés ;
  `cargo fmt`, `clippy -D warnings` et les 315 tests restent verts.

## Prochaines actions

1. **D-024** : diagnostiquer l'écart STEP4 Python sur `153 Camille`
   (4 977 objets contre 11 853). Ne pas utiliser `--prefer-raw` sur ce projet
   tant que la configuration d'origine n'est pas retrouvée.
2. **Reverse proxy** : aucun nginx/caddy/cloudflared n'est installé. La bascule
   publique reste à faire ; voir `docs/exploitation/SWITCH-PUBLIC.md`.
3. **Surveillance production** : vérifier le moniteur webhook Rust après
   quelques jours d'activité, les téléchargements et la cohérence de
   l'historique.
4. **Documentation** : maintenir `README.md`, `docs/README.md`,
   `docs/lots/LOT-*.md`, `docs/architecture/CONTRAT-ENVIRONNEMENT.md` et
   `parity/DRIFT.md` à jour.
5. **Memory-bank** : mettre à jour `activeContext.md` et ce fichier à chaque
   session significative.
6. **Interface** : ne pas réintroduire de client desktop Tauri/GTK/WebKit ;
   D-028 reste la référence.

## Politique de rétention

- Ce fichier conserve les tâches depuis octobre 2026.
- Les périodes plus anciennes seront résumées et déplacées vers
  `memory-bank/archives/` si le volume le justifie.
- L'historique Python legacy reste dans
  `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.
