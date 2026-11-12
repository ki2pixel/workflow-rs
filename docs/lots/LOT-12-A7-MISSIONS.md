# Lot 12 — logos de mission A7 (emails `email_proxy.php`)

> Statut : **implémenté et validé le 2026-11-12** (D-064).
> Date : 2026-11-12.
> Décisions liées : D-064, D-041 (masque des overlays `.ecproj`), D-052
> (registre des logos source), D-061 (template SDM multi-préfixes).
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

### Classification des `.mov`

Pour une durée `d`, `main` = fin du dernier segment :

| Condition | Mode | Comportement |
| --- | --- | --- |
| `d >= main - 1 ms` | `full_duration` | calque de 0 à `main`, comp non prolongée |
| `d <= mission_end_card_max_s + 1 ms` et au moins un PNG valide | `end_card` | calque de `main` à `main + d`, comp prolongée de `d` |
| `.mov` court seul | `review` | `short_mov_without_png`, aucun MOV inséré |
| `.mov` intermédiaire | `review` | `intermediate_mov`, aucun MOV inséré |
| plus d'un `.mov` | `review` | `multiple_mov`, les PNG valides restent insérés |

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
  publié. Il contient `schema`, `generated_at`, `project`, `project_stem`,
  `lot` et `mission` ; il permet le QA hors ligne.
- `audit.json` ajoute les contrôles `mission_available`, `mission_assets`,
  `mission_layers`, `mission_no_logo`, `mission_modes`, `mission_overlay`,
  `mission_tail` et `mission_sidecar`.
- `timings_ms.a7_ms` n'est écrit que si des missions sont chargées.

## Implémentation

- `workflow-core/src/mission.rs` : modèle `ClientMission`, `VideoInstruction`,
  normalisation NBSP/apostrophes/tirets, `filename_key` sans accents,
  sections urgentes, échéances (`13h45`, sans confondre `9:16`), parsing des
  logos, `Pas de logo`, `extract_lot`, liens de livraison, RFC 2047.
- `workflow-ingest/src/email.rs` : client HTTP index/détail, store SQLite
  `email_missions` (WAL, purge de rétention), matching, import `.eml` MIME
  minimal (multipart, quoted-printable, base64).
- `workflow-ingest/src/history.rs` : table `download_origins` et hook
  d'enregistrement du nom d'origine au téléchargement réussi, pour la
  traçabilité des lots.
- `workflow-steps/src/post/a7.rs` : validation PNG/MOV, classification,
  planification, insertion, bloc `mission`.
- `workflow-steps/src/post/run.rs` : A7 avant A1/A2 hors template (pour que le
  masque A2 voie les calques), après A6 pour un template éligible ; écriture
  du sidecar, du bloc `mission` et des contrôles d'audit.
- `workflow-steps/src/post/overlay.rs` : résolution des chemins relatifs
  `../docs/...` depuis le dossier du projet, pas depuis le répertoire courant.
- `workflow-steps/src/qa/stage0.rs` : contrôle `mission_overlays` (assets
  manquants, `Pas de logo`, calques ajoutés ou déjà présents).
- `workflow-steps/src/qa/cache.rs` : `CACHE_VERSION = 11`, empreinte du
  sidecar de mission.
- `workflow-steps/src/qa/model.rs` : `QA_SCHEMA_VERSION = 1.4.0`.
- `workflow-cli/src/main.rs` : commandes `email`, flags `--no-email` et
  `--no-mission-overlay`, chargement des missions par lot.
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
  `mission_overlays: pass`, schéma `1.4.0`. Le cache QA est passé en version
  11 et change quand le sidecar change.
- **Suite Rust** : `cargo test --workspace` = 634 tests exécutés, 0 échec,
  2 ignorés ; `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.

## Critères d'acceptation

- Endpoint indisponible, 5xx ou `[]` : pipeline identique, exit 0, aucune
  requête si `email_proxy.enabled = false`.
- `--no-email` : aucun accès réseau, aucun sidecar, aucun bloc `mission`.
- Asset non conforme ou absent : `review`, `warn` d'audit, exit 0.
- `.mov` classé sans ambiguïté, ou `review` explicite ; jamais de MOV deviné.
- `Pas de logo` : aucun calque mission, aucune suppression.
- Sidecar uniquement si l'assist est publié, `post.json` avec `mission` en
  dernier, ordre des clés préservé.
- Tests sans réseau avec fixtures `.eml` ; aucun test n'écrit dans
  `runtime/` ou `/mnt/cache`.

## Limites et risques

- **v1 sans masque MOV** : le QA ne masque pas encore les pixels d'un MOV de
  mission ; une review `logo_residual` non bloquante reste possible. Le masque
  PNG et l'échantillonnage alpha 12 frames sont en place.
- L'end-card exige au moins un PNG valide ; un MOV court seul part en revue.
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

- D-064 dans `memory-bank/decisionLog.md`.
- D-041, D-052, D-061.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`.
- `crates/workflow-steps/src/post/a7.rs`.
- `crates/workflow-core/src/mission.rs`.
- `crates/workflow-ingest/src/email.rs`.
