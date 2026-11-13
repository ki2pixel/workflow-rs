# Lot 12 — logos de mission A7 (emails `email_proxy.php`)

> Statut : **implémenté et validé le 2026-11-12** (D-064) ; sous-lot 12.1
> (parseur, alias, `tail_overlay`, sidecar 2) validé le 2026-11-12 (D-065) ;
> sous-lot 12.2 (banque centrale de repli) validé le 2026-11-12 (D-066) ;
> sous-lot 12.3 (benchmark, masque QA MOV, canari) validé le 2026-11-12
> (D-067) ; résilience des assets locaux corrompus validée le 2026-11-13
> (D-069).
> Date : 2026-11-12.
> Décisions liées : D-064, D-065, D-066, D-067, D-069, D-041 (masque des
> overlays `.ecproj`), D-052 (registre des logos source), D-061 (template SDM
> multi-préfixes).
> Code visé : `crates/workflow-core/src/mission.rs`,
> `crates/workflow-ingest/src/email.rs`,
> `crates/workflow-steps/src/post/a7.rs`,
> `crates/workflow-steps/src/post/run.rs`,
> `crates/workflow-steps/src/qa/stage0.rs`,
> `crates/workflow-cli/src/main.rs`.

## Problème

Les emails de missions Média Solution contiennent, pour chaque lot, la liste
des vidéos et les logos à incruster (`Logos requis: …`), ainsi que les cas
`Pas de logo`. Ces informations étaient appliquées à la main après STEP8. Le
but de A7 est de les injecter dans le projet `_assist`, sans jamais modifier le
projet brut, avec une dégradation gracieuse stricte : un endpoint absent, lent,
en erreur ou vide ne doit rien changer au pipeline.

Le lot doit rester extractible du nom du dossier (`136 Cvmille`, `80 Camille`).
Un dossier sans numéro de lot, une mission ambiguë ou un asset absent ne
doivent jamais déclencher de placement deviné.

## Contrat

### Double interrupteur

| Niveau | Clé / flag | Effet |
| --- | --- | --- |
| Réseau + cache | `[email_proxy] enabled` | `false` : aucune requête, aucun accès au cache |
| Écriture A7 | `steps.step8.post.mission_overlay` | `false` : mission résolue et auditée, aucun calque inséré |
| Masque A2 | `steps.step8.post.a2_mission_mask` | `true` : A2 lit les calques mission comme masque de logos |
| Seuil end-card | `steps.step8.post.mission_end_card_max_s` | défaut `5.0`, borné `[0, 60]` |
| Isolation CLI | `post run --no-email` | aucune requête, aucune mission, aucun sidecar |
| Audit seul | `post run --no-mission-overlay` | résolution + sidecar + audit, `added: []` |

`step8 run` expose les mêmes échappements `--no-email` et
`--no-mission-overlay`. `email_proxy.enabled` est indépendant de
`mission_overlay` : le premier coupe le réseau et le cache, le second coupe
seulement l'écriture.

### Résolution

- `email_proxy.php` fournit un index borné (200 lots côté serveur, 50 en
  incrémental) et un détail par mission. Le cache SQLite dédié est
  `<paths.base>/email_missions.sqlite3` (ou `email_proxy.db`), jamais fusionné
  avec `download_history.sqlite3`.
- La résolution se fait à la demande au lancement de `step8`/`post run`, avec
  une synchronisation réseau non bloquante puis lecture du cache. Aucun poller
  serveur n'est ajouté.
- Une mission est choisie par lot exact. Plusieurs missions pour le même lot
  restent ambiguës : `review`, jamais « la plus récente ».
- L'expéditeur `@media-solution.fr` est exigé par défaut
  (`require_media_solution = true`).
- Les commandes hors ligne sont `email list`, `email show --id|--lot`,
  `email sync`, `email test`, `email match`, `email import`.

### Assets

Les assets de production sont **strictement 1080×1920 pleine toile avec
alpha**. Un PNG ou un `.mov` qui ne respecte pas ce profil n'est jamais placé :
la décision A7 passe en `review`, sans coordonnées devinées. Le placement est
fixe : centre `[540, 960]`, échelle `100`. Il n'existe pas de catalogue
`overlay_assets.toml` ni de recherche d'asset par similarité.

Depuis le sous-lot 12.1, la banque client compte **117 assets (109 PNG +
8 MOV)**. La résolution est ordonnée : fichier exact, nom normalisé unique,
extension implicite, puis table d'alias fermée (`22` acronymes officiels et
variantes historiques). `LRDLR.png` et `em6.png` sont prioritaires sur leurs
alias ; `MSL1` sans extension reste ambigu (PNG + MOV) et part en revue.
`lphdf.png` et `edc.png` sont des absences volontaires (`known_absent`),
documentées sans alerte d'oubli client.

Depuis le sous-lot 12.2, ces assets sont déployés dans
`assets/mission_overlays/` et servent de **repli central** quand un logo
demandé n'est pas dans `<projet>/docs`. L'ordre est : local exact → local
normalisé → banque exacte → banque normalisée → extension implicite → alias →
`known_absent` → revue. Un asset local valide gagne toujours. La banque est
optionnelle (`paths.mission_overlays`) ; le sidecar distingue
`source: local_docs` et `source: central_fallback`. Les MOV (2,2 Go) sont
gitignorés, les PNG, `manifest.toml` et `README.md` sont versionnés. Détail :
`docs/lots/LOT-12.2-BANQUE-CENTRALE.md` et
`docs/lots/LOT-12.1-PARSEUR-ALIAS-TAIL.md`.

Depuis D-069, un asset local présent mais illisible (corrompu, tronqué) ou
hors profil ne masque plus la banque : A7 re-sonde l'homologue central via
`LogoResolver::resolve_fallback_target`. Si le repli est sain, l'asset est
inséré en `central_fallback` et un warning additif
`local_asset_corrupted_fallback_used` est écrit dans le sidecar ; la décision
reste `apply`. Si le repli échoue aussi, l'erreur locale initiale est
conservée et la mission reste en `review`. Le brut, les assets de `docs/` et
`MediaSolution.jsx` ne sont jamais modifiés ; seul le sidecar A7 est ajouté.

### Classification des `.mov`

Pour une durée `d`, `main` = fin du dernier segment :

| Condition | Mode | Comportement |
| --- | --- | --- |
| `d >= main - 1 ms` | `full_duration` | calque de 0 à `main`, comp non prolongée |
| `d <= mission_end_card_max_s + 1 ms` et au moins un PNG valide | `end_card` | calque de `main` à `main + d`, comp prolongée de `d` |
| `.mov` déclaré `tail_overlay` et `d <= main + 1 ms` | `tail_overlay` | calque de `max(0, main - d)` à `main`, comp non prolongée, audio coupé |
| `.mov` déclaré `tail_overlay` et `d > main + 1 ms` | `review` | `short_video_for_tail_overlay`, aucun rognage |
| `.mov` court seul | `review` | `short_mov_without_png`, aucun MOV inséré |
| `.mov` intermédiaire | `review` | `intermediate_mov`, aucun MOV inséré |
| plus d'un `.mov` | `review` | `multiple_mov`, les PNG valides restent insérés |

Depuis le sous-lot 12.1, les huit MOV de la banque ont une politique explicite
(`MovPolicy`) : `m6+.mov` = `end_card`, `MOTION_FIN_EM6_V03.mov` =
`tail_overlay`, `MSL1.mov`, `9_16_RDV_PROGRAMME_EM6_V02_5MIN.mov`,
`the power.mov` et `hayu x below deck.mov` = `full_duration`, les deux bumpers
de 19,72 s (`m6+max anime 1ep.mov`, `W9_M6+MAX.mov`) = `review`. Aucun
déclenchement automatique de l'outro EM6 : le mode n'agit que si l'asset est
explicitement demandé. Un MOV inconnu conserve l'heuristique ci-dessus.

L'ordre d'insertion est : premier asset de l'email = calque le plus haut ;
end-card au-dessus des PNG. Un asset déjà présent (même nom normalisé, cas
`sdm.png` ajouté par A6) n'est jamais dupliqué : `skipped`, décision `noop`.

### `Pas de logo`

Une instruction `Pas de logo` ne crée aucun calque et ne supprime jamais un
calque humain ou A6. Si un template A6 a déjà ajouté `sdm.png`, A7 le signale
en `review` (`no_logo_conflict_a6`) sans le retirer. Le veto A2 « logo seul »
reste actif.

### Sorties

- Projet brut inchangé ; seul `<stem>_assist.ecproj` est publié.
- `post.json` conserve l'ordre des clés et ajoute `mission` **après**
  `summary` (bloc absent si aucune mission n'est chargée).
- Sidecar `<docs>/<stem>_mission.json` écrit seulement si le projet assisté est
  publié. Depuis le sous-lot 12.1 il est en `schema: 2` et contient
  `generated_at`, `project`, `project_stem`, `lot` et `mission` ; chaque entrée
  de `added` expose `requested` (token d'origine) et `source`
  (`local_docs` ou `central_fallback`). Les sidecars
  schéma 1 restent lisibles. Depuis D-069, `warnings` peut contenir en plus un
  avertissement `local_asset_corrupted_fallback_used` (même forme
  `code`/`line`/`detail` que les avertissements du parseur) ; aucune clé
  existante n'est renommée ou supprimée.
- `audit.json` ajoute les contrôles `mission_available`, `mission_assets`,
  `mission_known_absent`, `mission_layers`, `mission_no_logo`, `mission_modes`,
  `mission_overlay`, `mission_tail` et `mission_sidecar`.
- `timings_ms.a7_ms` n'est écrit que si des missions sont chargées.
- `workflow-cli email audit --json-dir <dir> --assets <dir> [--json]` audite un
  corpus hors ligne, sans réseau ni base, et agrège les résultats par cohorte
  mensuelle.

## Implémentation

- `workflow-core/src/mission.rs` : modèle `ClientMission`, `VideoInstruction`,
  normalisation NBSP/apostrophes/tirets, `filename_key` sans accents,
  sections urgentes, échéances (`13h45`, sans confondre `9:16`), parsing des
  logos, `Pas de logo`, `extract_lot`, liens de livraison, RFC 2047. Depuis le
  sous-lot 12.1 : lignes logiques (4 lignes / 600 caractères), parenthèses
  équilibrées, `\'` non échappé, formes `Logo <nom>` et `(yaq.png)`, table
  d'alias, `known_absent`, `LogoResolver` avec cache et recollage asset-aware.
  Depuis D-069 : `resolve_fallback_target` pour réinterroger la seule banque
  après l'échec d'une sonde locale.
- `workflow-ingest/src/email.rs` : client HTTP index/détail, store SQLite
  `email_missions` (WAL, purge de rétention), matching, import `.eml` MIME
  minimal (multipart, quoted-printable, base64).
- `workflow-ingest/src/history.rs` : table `download_origins` et hook
  d'enregistrement du nom d'origine au téléchargement réussi, pour la
  traçabilité des lots.
- `workflow-steps/src/post/a7.rs` : validation PNG/MOV, classification,
  planification, insertion, bloc `mission`. Depuis le sous-lot 12.1 :
  `PlacementMode::TailOverlay`, registre `MovPolicy` des huit MOV, `A7Added`
  avec `requested`/`source`, audio coupé en `tail_overlay`. Depuis D-069 :
  reprise sur la banque quand un asset `LocalDocs` échoue à `probe_asset`
  (`asset_unreadable` / `asset_dimensions`).
- `workflow-steps/src/post/run.rs` : A7 avant A1/A2 hors template (pour que le
  masque A2 voie les calques), après A6 pour un template éligible ; écriture
  du sidecar `schema: 2`, du bloc `mission` et des contrôles d'audit, dont
  `mission_known_absent`.
- `workflow-steps/src/post/overlay.rs` : résolution des chemins relatifs
  `../docs/...` depuis le dossier du projet, pas depuis le répertoire courant ;
  masque des habillages PNG et, depuis le sous-lot 12.3, échantillonnage alpha
  des MOV (`kind: "mov"`).
- `workflow-steps/src/qa/stage0.rs` : contrôle `mission_overlays` (assets
  manquants, `Pas de logo`, calques ajoutés ou déjà présents).
- `workflow-steps/src/qa/cache.rs` : `CACHE_VERSION = 12`, empreinte du
  sidecar de mission.
- `workflow-steps/src/qa/model.rs` : `QA_SCHEMA_VERSION = 1.5.0`.
- `workflow-steps/src/qa/mod.rs` : lecture du masque avec les outils
  `ffmpeg`/`ffprobe` configurés.
- `workflow-cli/src/main.rs` : commandes `email`, flags `--no-email` et
  `--no-mission-overlay`, chargement des missions par lot, audit
  `format_cohorts`.
- `config/config.toml` : `[email_proxy]` désactivé par défaut,
  `mission_overlay = false`, `mission_end_card_max_s = 5.0`,
  `a2_mission_mask = false`.

## Validation (2026-11-12)

Tous les essais ont utilisé des copies sous `/tmp/opencode` ; les projets et
médias de production n'ont pas été modifiés.

- **136 — end-card** : mission `m6+.png`, `the 100.png`, `m6+.mov` ; comp
  `44.4 s → 47.4 s`, 19 calques, MOV `44.4–47.4`, PNG statiques `0–44.4`.
  `effectcraft-cli 0.24.0 render-frame --time 45 --transparent` rend une frame
  1080×1920 pleine toile. Sidecar écrit, audit `mission_*` pass.
- **80 — plein durée et `Pas de logo`** : `MSL1.mov` inséré de `0` à `79.2 s`
  (= comp), audio conservé, aucune queue. `Leur belle-mère` : `no_logo`, 0
  calque, aucune suppression.
- **131 — déduplication A6** : `Raymond` reçoit `sdm.png` statique
  `0–28.16 s`. `SDM - Leslie et Léo` est éligible A6 ; A6 ajoute `sdm.png`,
  A7 le voit déjà présent, `added: []`, `skipped: ["sdm.png"]`, décision
  `noop`, aucun doublon (4 calques).
- **158 — `Pas de logo` réel** : 3 projets, 0 calque mission, sidecars écrits,
  audit `pass` (un `warn` de durée préexistant sur `CABINE`, sans lien A7).
  La variante live du lot 158 qui demande `teva.png` (asset absent) suit le
  même chemin que le cas 999 ci-dessous.
- **89 — PNG statique long** : `le seigneur des anneaux…png` inséré de `0` à
  `85.84 s`, 48 calques (47 segments + 1 logo), audit pass.
- **999 — asset manquant** : mission synthétique `teva.png` absente ; 0 calque,
  `decision: review`, `missing: ["teva.png"]`, audit `warn`, code de sortie 0,
  aucun crash.
- **Flags** : `--no-email` sur 136 : comp 44.4, 16 calques, aucun bloc
  `mission`, aucun sidecar, aucun `a7_ms`, aucune requête. `--no-mission-overlay` :
  comp 44.4, `mission.added: []`, `reasons: ["overlay_disabled"]`, sidecar
  écrit, audit `warn`, aucun calque.
- **QA** : `qa run` sur le projet 89 avec sidecar découvert automatiquement ;
  `mission_overlays: pass`, schéma `1.4.0` (à l'époque ; `1.5.0` depuis
  D-067). Le cache QA est passé en version
  11 et change quand le sidecar change.
- **Suite Rust** : `cargo test --workspace` = 634 tests exécutés, 0 échec,
  2 ignorés ; `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.

### Sous-lot 12.1 (2026-11-12)

- **Parseur** : 25 tests `workflow-core::mission` (lignes coupées, parenthèses
  de titre, note trailing, `\'`, formes `Logo <nom>` et `(yaq.png)`,
  continuations, CRLF, alias, résolveur, absences volontaires) ; 14 tests A7
  (registre MOV, `tail_overlay` 60 s, refus 8 s, `tail_s == 0`, audio coupé,
  comp non prolongée, `known_absent`).
- **Audit corpus** (lecture seule, `/tmp/opencode`) : 932 emails, 570 HTML-only,
  319 lots, 2 477 instructions, 2 275 avec logos, 39 non résolues
  (dont 28 instructions client « logo non trouvé/spécifié »),
  2 469 tokens, 2 228 résolus, 47 absences volontaires, 193 manquants,
  1 ambigu. Octobre 2026 : 234/242 tokens résolus (100 % hors absences
  volontaires) ; novembre 2026 : 57/57 (100 %).
- **Suite Rust** : 654 tests listés (652 exécutés + 2 ignorés), 0 échec ;
  `fmt` et clippy verts. Aucun `.ecproj`, `runtime/` ou `/mnt/cache` modifié.
- Détail et limites : `docs/lots/LOT-12.1-PARSEUR-ALIAS-TAIL.md`.

### Sous-lot 12.2 (2026-11-12)

- **Banque** : 117 assets déployés dans `assets/mission_overlays/`
  (109 PNG versionnés, 8 MOV gitignorés), `manifest.toml` avec dimensions,
  durée, codec, pix_fmt, audio, taille et SHA-256, `README.md` de déploiement.
- **Résolution** : local exact → local normalisé → banque exacte → banque
  normalisée → extension implicite → alias → `known_absent` → revue. Chaque
  asset central passe le même `probe_asset` 1080×1920 alpha.
- **Tests** : priorité locale sur la banque, banque seule
  (`source: central_fallback`, chemin `../../assets/mission_overlays/...`),
  banque absente → `review`, propagation `PostOptions`, validation
  `paths.mission_overlays`. `config check` vert sur la configuration réelle.
- **Suite Rust** : 661 tests listés (659 exécutés + 2 ignorés), 0 échec ;
  `fmt` et clippy verts. Aucun `.ecproj`, `runtime/` ou `/mnt/cache` modifié.
- Détail et limites : `docs/lots/LOT-12.2-BANQUE-CENTRALE.md`.

### Sous-lot 12.3 (2026-11-12)

- **Benchmark** : `email audit --json` expose `format_cohorts`
  (mois × HTML seul/texte) ; `scripts/audit-a7-benchmark.py` consolide
  historique 2025–2026 et récent oct.–nov. 2026. Corpus : 932 emails,
  2 469 tokens, 2 228 résolus, 47 absences volontaires, 193 manquants.
  Historique : 90,9 % hors absences ; récent : 100 % tous formats.
- **Masque QA MOV** : les calques `.mov` porteurs d'alpha sont échantillonnés
  en 12–24 frames (`ffmpeg` rawvideo RGBA), les boîtes stables autour de la
  médiane sont fusionnées (`kind: "mov"`), un MOV instable est ignoré avec un
  warning non bloquant. La fenêtre est ramenée au temps source pour un
  end-card placé tard dans la comp. `QA_SCHEMA_VERSION = 1.5.0`,
  `CACHE_VERSION = 12`.
- **Canari** : 20 missions réelles récentes rejouées sous `/tmp/opencode` avec
  la banque active et `--report-only` : 20/20 `apply`, 28 assets (18
  `central_fallback`, 10 `local_docs`), 0 doublon, 0 `_assist` écrit, exit 0.
  Cas limites : `known_absent`, manquant, ambigu, `tail_overlay`, déduplication.
- **Correctif** : `stem_key` est `trim()`é après retrait de l'extension, pour
  les emails avec un espace avant `.mp4` (lot 20 réel).
- **Suite Rust** : 670 tests listés (668 exécutés + 2 ignorés), 0 échec ;
  `fmt` et clippy verts. Aucun `.ecproj`, `runtime/` ou `/mnt/cache` modifié.
- Détail et limites : `docs/lots/LOT-12.3-BENCHMARK-QA-MOV.md`.

### Résilience asset local corrompu (2026-11-13, D-069)

- **Cas réel** : `docs/LFAUIT.png` de `183 Camille_brut.zip`, 15 octets ASCII
  sans signature PNG. Avant correctif : sonde `asset_unreadable`, sidecar
  `review` / `missing_asset`, aucun logo. Après correctif : repli sur
  `assets/mission_overlays/LFAUIT.png` (35 582 octets, SHA-256 conforme au
  manifeste), `decision: apply`, `added[0].source: central_fallback`, warning
  `local_asset_corrupted_fallback_used`, calque et item insérés avec le chemin
  `../../assets/mission_overlays/LFAUIT.png`.
- **Dégradation** : même PNG corrompu et banque vide/désactivée → `review`,
  `reasons: ["missing_asset"]`, `missing: ["LFAUIT.png"]`, décision
  `asset_unreadable`, aucun calque, exit 0.
- **Tests** : `fallback_target_bypasses_a_corrupt_local_asset`,
  `corrupt_local_asset_falls_back_to_central_bank`,
  `invalid_local_profile_falls_back_to_central_bank`,
  `corrupt_local_asset_without_fallback_degrades_to_review`,
  `corrupt_fallback_keeps_local_error_and_reviews`.
- **Suite Rust** : 675 tests listés (673 exécutés + 2 ignorés), 0 échec ;
  `fmt` et `clippy -D warnings` verts. Aucun `.ecproj`, `runtime/` ou
  `/mnt/cache` modifié.
- Détail : `docs/lots/LOT-12.2-BANQUE-CENTRALE.md`.

## Critères d'acceptation

- Endpoint indisponible, 5xx ou `[]` : pipeline identique, exit 0, aucune
  requête si `email_proxy.enabled = false`.
- `--no-email` : aucun accès réseau, aucun sidecar, aucun bloc `mission`.
- Asset non conforme ou absent : `review`, `warn` d'audit, exit 0.
- Asset local corrompu ou hors profil avec homologue central sain :
  `apply`, `source: central_fallback`, warning
  `local_asset_corrupted_fallback_used`, calque inséré ; sans repli
  exploitable : `review`, aucune régression.
- `.mov` classé sans ambiguïté, ou `review` explicite ; jamais de MOV deviné.
- `Pas de logo` : aucun calque mission, aucune suppression.
- Sidecar uniquement si l'assist est publié, `post.json` avec `mission` en
  dernier, ordre des clés préservé.
- Tests sans réseau avec fixtures `.eml` ; aucun test n'écrit dans
  `runtime/` ou `/mnt/cache`.

## Limites et risques

- **Masque MOV depuis 12.3** : les calques `.mov` alpha sont masqués, mais
  seules les frames stables sont fusionnées ; un logo animé ou multi-positions
  est ignoré et signalé, donc une review `logo_residual` non bloquante reste
  possible. Le masque PNG et l'échantillonnage alpha 12 frames restent en
  place.
- L'end-card exige au moins un PNG valide ; un MOV court seul part en revue.
- `tail_overlay` ne se déclenche jamais automatiquement : `MOTION_FIN_EM6_V03.mov`
  n'a aucune occurrence dans les 932 emails du corpus. Il faut le demander
  explicitement.
- Les deux bumpers `m6+max anime 1ep.mov` et `W9_M6+MAX.mov` (19,72 s) restent
  en `review` jusqu'à arbitrage humain.
- Les MOV de la banque centrale (2,2 Go) ne sont pas versionnés : un checkout
  neuf doit les recopier ; sans eux, un repli MOV part en `review`.
- Le benchmark est une mesure de résolution contre la banque actuelle ; il ne
  juge pas la conformité visuelle des assets.
- Le seuil `mission_end_card_max_s` est volontairement bas (5 s) : un MOV de
  duration moyenne est ambigu et doit être arbitré humainement.
- La résolution dépend du nom de dossier (`lot_from_folder`) et du nom de
  vidéo ; un renommage hors convention désactive A7 silencieusement, ce qui est
  le comportement conservateur attendu.
- Le cache est local à la machine ; la base `email_missions.sqlite3` doit être
  sauvegardée ou resynchronisée après un changement de machine.
- Aucune création automatique de calque pour un logo non fourni : le client
  doit transmettre l'asset, sinon la mission reste en revue.

## Hors périmètre

- Le rendu final des vidéos et l'upload ; A7 ne touche qu'au `.ecproj`.
- Le panneau CEP et `MediaSolution.jsx` (D-022) : le projet assisté reste
  consommé par EffectCraft.
- Les routes Python-only (D-015).
- Toute suppression ou déplacement d'un calque humain/A6.

## Références

- D-064, D-065, D-066 et D-067 dans `memory-bank/decisionLog.md`.
- D-041, D-052, D-061.
- `docs/lots/LOT-12.1-PARSEUR-ALIAS-TAIL.md`.
- `docs/lots/LOT-12.2-BANQUE-CENTRALE.md`.
- `docs/lots/LOT-12.3-BENCHMARK-QA-MOV.md`.
- `assets/mission_overlays/README.md`.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`.
- `crates/workflow-steps/src/post/a7.rs`.
- `crates/workflow-core/src/mission.rs`.
- `crates/workflow-ingest/src/email.rs`.
