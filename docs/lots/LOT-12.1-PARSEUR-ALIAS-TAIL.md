# Lot 12.1 — parseur de missions, alias et `tail_overlay`

> Statut : **implémenté et validé le 2026-11-12** (D-065).
> Date : 2026-11-12.
> Décisions liées : D-064 (A7 initial), D-041 (masque des overlays),
> D-052 (registre des logos source), D-061 (templates SDM).
> Code : `crates/workflow-core/src/mission.rs`,
> `crates/workflow-steps/src/post/a7.rs`,
> `crates/workflow-steps/src/post/run.rs`,
> `crates/workflow-steps/src/qa/stage0.rs`,
> `crates/workflow-cli/src/main.rs`.

## Problème

A7 (D-064) fonctionnait sur les emails récents, mais le corpus historique était
fragile :

- 570 des 932 emails du corpus client n'ont **que du HTML** ; le texte doit
  être converti avant analyse ;
- 569 emails contiennent des apostrophes échappées par l'export mbox
  (`l\'ile`, `c\'est la famille.png`) ;
- des noms de vidéos et des listes de logos sont coupés par le client mail sur
  plusieurs lignes physiques, parfois au milieu d'une parenthèse ;
- certaines parenthèses ne sont pas équilibrées (`((logo gulliprime)`), ou
  contiennent un titre (`… l'avion (2).mp4`) ;
- des instructions utilisent une forme libre (`Logo chaîne : gulli.png`,
  `(yaq.png)`) au lieu de `Logos requis: …` ;
- les alias historiques (`LRDLR`, `EM6`, `CEC`, `SI ON SE RENCONTRAIT`) et les
  assets volontairement absents (`lphdf.png`, `edc.png`) n'étaient pas
  distingués des oublis client ;
- le mode `tail_overlay` n'existait pas alors que la banque contient
  `MOTION_FIN_EM6_V03.mov` (12 s).

## Contrat

### Lignes logiques

`logical_lines` fusionne jusqu'à **4 lignes physiques / 600 caractères** quand
l'instruction est incomplète :

- extension vidéo absente de la ligne ;
- parenthèses non équilibrées ;
- ligne de continuation `Logo …` quand l'instruction n'a pas encore de
  parenthèse ;
- ligne de continuation commençant par `(` quand l'instruction n'en a pas.

Les lignes vides et les en-têtes de section coupent la fusion. Les notes qui
suivent une instruction complète restent des lignes distinctes et alimentent
`is_logo_note`. Les fins de ligne CRLF et CR seules sont normalisées.

### Parenthèses et tokens

- `split_parenthetical` trouve la fermeture par **équilibrage**, pas par
  `rfind(')')` : `(2)` dans un titre et une note trailing
  (`(Logos requis: …) (inclure signature…)`) sont préservés.
- `parse_logos` exige un séparateur `:` ou `=`.
- `parse_lenient_logos` accepte `Logo <nom>`, `Logo chaîne : <nom>`,
  `Logo émission : <nom>` et les libellés `programme`, `demandé`, `animé`.
- `parse_bare_asset_list` accepte une parenthèse composée uniquement de tokens
  en forme d'asset (`(yaq.png)`, `(a.png, b.mov)`). Un titre ou une note libre
  n'est jamais converti en logo.
- `unescape_mission_text` remplace `\'` par `'` avant l'analyse.

### Alias et absences

La table `ALIASES` est une `const` fermée dans `workflow-core::mission`. Elle
couvre les 22 acronymes officiels, les variantes historiques et deux absences
volontaires. La résolution est strictement ordonnée :

1. fichier exact dans `docs/` ;
2. nom normalisé (casse, accents, ponctuation) si unique ;
3. extension implicite `.png` puis `.mov` si le token n'en a pas ;
4. alias de la table ;
5. `known_absent` ou `missing`.

`LRDLR.png` et `em6.png` sont résolus directement s'ils existent ; l'alias vers
`Les reines de la route.png` / `e=m6.png` ne sert que de repli. `MSL1` sans
extension reste ambigu (PNG + MOV) et part en revue. `lphdf.png` et `edc.png`
sont des absences volontaires, documentées comme telles, jamais des alertes
d'oubli client.

Un token coupé peut être recollé sur 2 ou 3 tokens avec `, ` ou ` et `
(`lorie, derrière la positive attitude.png`) **uniquement si le nom reconstitué
existe réellement**.

### `tail_overlay`

Pour un MOV explicitement nommé et déclaré `TailOverlay` :

| Condition | Comportement |
| --- | --- |
| `d <= main + 1 ms` | calque de `max(0, main - d)` à `main`, `tail_s = 0`, comp non prolongée, audio coupé |
| `d > main + 1 ms` | `review` (`short_video_for_tail_overlay`), aucun rognage, aucun calque MOV |

Aucun déclenchement automatique par `stem_contains("EM6")`. L'outro n'est
jouée que si le fichier est explicitement demandé dans la mission (ou pendant
les tests).

### Registre des MOV

Table `const` dans `a7.rs` :

| Asset | Politique |
| --- | --- |
| `m6+.mov` | `end_card` |
| `MOTION_FIN_EM6_V03.mov` | `tail_overlay` |
| `MSL1.mov` | `full_duration` |
| `9_16_RDV_PROGRAMME_EM6_V02_5MIN.mov` | `full_duration` |
| `the power.mov` | `full_duration` |
| `hayu x below deck.mov` | `full_duration` |
| `m6+max anime 1ep.mov` | `review` (bumper 19,72 s non arbitré) |
| `W9_M6+MAX.mov` | `review` (bumper 19,72 s non arbitré) |

Un MOV inconnu conserve l'heuristique D-064 (plein durée, end-card ou revue
selon la durée et les PNG).

### Sidecar `schema: 2`

Le sidecar `<docs>/<stem>_mission.json` passe en schéma 2. Chaque entrée de
`added` gagne deux champs additifs :

- `requested` : token d'origine de l'email, avant alias ou recollage ;
- `source` : provenance de l'asset (`local_docs` en 12.1,
  `central_fallback` en 12.2).

Le bloc `mission` gagne `known_absent` (liste `{name, note}`) et
`audit.json` ajoute `mission_known_absent`. Les absences volontaires ne
déclenchent ni `warn` d'asset manquant ni revue QA.

### Audit hors ligne

```sh
cargo run -p workflow-cli -- email audit \
  --json-dir <emails_json> --assets <logos_overlay> [--json]
```

La commande ne fait aucun accès réseau, n'ouvre pas la base de missions et
n'écrit rien. Elle expose par cohorte mensuelle : emails, `html_only`,
instructions, `Pas de logo`, tokens, résolutions, absences volontaires, assets
manquants, ambiguïtés, politiques MOV et top des manquants. `--json` produit le
rapport complet, `failures` étant borné aux 100 premiers cas non résolus.

## Implémentation

- `mission.rs` : `logical_lines`, `instruction_incomplete`,
  `parentheses_balanced`, `unescape_mission_text`, `parse_bare_asset_list`,
  `parse_lenient_logos`, `split_logo_tokens`, `ALIASES`, `resolve_alias`,
  `LogoResolver`, `resolve_logo_file`.
- `a7.rs` : `PlacementMode::TailOverlay`, `MovPolicy`, `mov_policy`,
  `A7Added { requested, source }`, `resolve_token`, `plan_assets` par
  politique, audio coupé pour `tail_overlay`, `known_absent` dans le bloc
  mission.
- `run.rs` : sidecar `schema: 2`, contrôle `mission_known_absent`, filtrage
  des absences volontaires dans `mission_assets`.
- `stage0.rs` : QA neutre pour les absences volontaires.
- `main.rs` : sous-commande `email audit`, rapport texte et JSON.

## Validation

- `cargo test -p workflow-core mission --lib` : 25 tests, dont lignes
  coupées, parenthèses de titre, note trailing, `\'`, forme libre, forme nue,
  continuation `Logo`/`(`, CRLF, alias, résolveur et absences.
- `cargo test -p workflow-steps post::a7 --lib` : 14 tests, dont
  `tail_overlay` à 60 s, refus à 8 s, invariant `tail_s == 0`, audio coupé,
  comp non prolongée, registre MOV et `known_absent`.
- `cargo test --workspace` : 654 tests listés (652 exécutés + 2 ignorés),
  0 échec ; `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.
- Audit réel du corpus (lecture seule, aucun accès réseau) :
  932 emails, 570 HTML-only, 319 lots, 2 477 instructions,
  2 275 avec logos, 39 non résolues (dont 28 instructions client
  « logo non trouvé/spécifié »), 2 469 tokens,
  2 228 résolus, 47 absences volontaires, 193 manquants, 1 ambigu.
  Cohorte octobre 2026 : 234/242 tokens résolus (100 % hors absences
  volontaires) ; novembre 2026 : 57/57 (100 %).
- Les mesures et rapports sont sous `/tmp/opencode` ; aucun `.ecproj`,
  `runtime/` ou `/mnt/cache` n'a été modifié.

## Limites

- La banque centrale de repli (`assets/mission_overlays`) et la provenance
  `central_fallback` sont le Lot 12.2, pas encore implémentées.
- `MOTION_FIN_EM6_V03.mov` n'a aucune occurrence dans les 932 emails : le mode
  existe, mais aucun déclenchement automatique n'est justifié.
- Les deux bumpers de 19,72 s restent en `review` jusqu'à arbitrage humain.
- Les emails historiques dont le logo n'est réellement pas spécifié
  (`Logo demandé mais non trouvé/spécifié`) restent en revue : aucune
  inférence depuis le programme.
- Le masque QA des pixels MOV et le benchmark stratifié complet sont le
  Lot 12.3.

## Rollback

Le parseur et le mode `tail_overlay` sont additifs. Un retour à D-064 consiste
à retirer `TailOverlay`/`MovPolicy` de `a7.rs`, à revenir au sidecar schéma 1 et
à supprimer la sous-commande `email audit`. Aucune donnée de production n'est
migrée ; les sidecars schéma 1 restent lisibles par le QA (champs additifs
absents).

## Références

- D-065 dans `memory-bank/decisionLog.md`.
- `docs/lots/LOT-12-A7-MISSIONS.md`.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`.
