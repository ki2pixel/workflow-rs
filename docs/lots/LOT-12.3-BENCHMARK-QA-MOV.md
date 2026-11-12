# Lot 12.3 — benchmark stratifié, masque QA MOV et canari

> Statut : **implémenté et validé le 2026-11-12** (D-067).
> Date : 2026-11-12.
> Décisions liées : D-064 (A7), D-065 (parseur, alias, `tail_overlay`), D-066
> (banque centrale de repli).
> Code : `crates/workflow-steps/src/post/overlay.rs`,
> `crates/workflow-steps/src/qa/model.rs`,
> `crates/workflow-steps/src/qa/cache.rs`,
> `crates/workflow-steps/src/qa/mod.rs`,
> `crates/workflow-steps/src/post/run.rs`,
> `crates/workflow-core/src/mission.rs`,
> `crates/workflow-cli/src/main.rs`,
> `scripts/audit-a7-benchmark.py`.

## Problème

Le Lot 12.2 a livré une banque de 117 assets et une résolution locale/centrale
validée par tests. Il restait trois angles morts : un benchmark consolidé du
corpus complet, un masque QA des habillages `.mov` (le QA ne masquait que les
PNG), et une validation de bout en bout sur des missions réelles récentes.

## Benchmark stratifié

### Méthode

L'audit est en lecture seule, sans réseau, base ou écriture :

```sh
cargo run -q -p workflow-cli -- email audit \
  --json-dir "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/mbox-to-json-2.0.0/emails_json" \
  --assets assets/mission_overlays \
  --json > /tmp/opencode/audit-12.3-bank.json

python3 scripts/audit-a7-benchmark.py /tmp/opencode/audit-12.3-bank.json --split 2026-10
```

Le JSON expose deux stratifications **additives** :

- `cohorts` : un bucket par mois (`%Y-%m`) ;
- `format_cohorts` : un bucket par mois et par format
  (`YYYY-MM|html_only` ou `YYYY-MM|text_plain`).

`html_only` signifie que le corps ne contient pas de partie `[text/plain]`.
Le taux de résolution est `resolved / (tokens - known_absent)` : les absences
volontaires ne pénalisent pas le taux, les ambiguïtés et les assets manquants
restent dans le dénominateur.

### Corpus complet

| Indicateur | Valeur |
| --- | ---: |
| Emails | 932 |
| HTML seul | 570 |
| Texte/plain | 362 |
| Lots | 319 |
| Instructions | 2 477 |
| Sans logo | 163 |
| Avec logos | 2 275 |
| Instructions non résolues | 39 |
| Tokens de logos | 2 469 |
| Résolus | 2 228 |
| Absences volontaires | 47 |
| Manquants | 193 |
| Ambigus | 1 |
| Taux hors absences volontaires | 92,0 % |

Les 39 instructions non résolues sont majoritairement des notes client
« logo non trouvé/spécifié » et ne portent pas de token exploitable.

### Historique (mars 2025 – septembre 2026)

| Cohorte | Emails | Instructions | Logos requis | Tokens | Résolus | Absences | Manquants | Ambigus | Non résolus | Taux |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| HTML seul | 504 | 1 545 | 1 441 | 1 567 | 1 441 | 25 | 101 | 0 | 13 | 93,5 % |
| Texte/plain | 281 | 644 | 579 | 603 | 496 | 14 | 92 | 1 | 25 | 84,2 % |
| **Total** | **785** | **2 189** | **2 020** | **2 170** | **1 937** | **39** | **193** | **1** | **38** | **90,9 %** |

### Récent (octobre – novembre 2026)

| Cohorte | Emails | Instructions | Logos requis | Tokens | Résolus | Absences | Manquants | Ambigus | Non résolus | Taux |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| HTML seul | 66 | 159 | 138 | 155 | 151 | 4 | 0 | 0 | 1 | 100,0 % |
| Texte/plain | 81 | 129 | 117 | 144 | 140 | 4 | 0 | 0 | 0 | 100,0 % |
| **Total** | **147** | **288** | **255** | **299** | **291** | **8** | **0** | **0** | **1** | **100,0 %** |

La période récente est intégralement résolue hors absences volontaires, tous
formats confondus. L'unique instruction non résolue est une note client sans
token. L'historique conserve 193 manquants, concentrés sur des émissions qui ne
sont plus dans la banque actuelle (`nouveau jour.png`, `en famille.png`,
`dressing vip by maeva.png`, `les 50 la finale.png`, etc.). Ces manques sont
normaux pour une banque de 117 assets et ne bloquent jamais le pipeline.

### Politiques MOV observées

Sur les 2 469 tokens, 82 correspondent à des `.mov` reconnus par le registre
fermé : 61 `end_card`, 14 `full_duration`, 7 `review`. Les 7 revues sont les
deux bumpers de 19,72 s (`m6+max anime 1ep.mov`, `W9_M6+MAX.mov`) et les cas
non classables : elles restent des décisions humaines, sans MOV deviné.

## Masque QA MOV

### Détection

`post::overlay` ne considère un calque `.mov` comme habillage que si son flux
vidéo porte un canal alpha (`yuva*`, `rgba*`, `bgra*`, `argb*`, `abgr*`, `ya*`).
Une vidéo opaque est du contenu : elle est ignorée silencieusement. Un `.mov`
dont l'asset est absent est aussi ignoré sans bruit, car le contrôle
`mission_overlays` du QA couvre déjà les assets de mission manquants.

### Échantillonnage

Pour chaque calque MOV exploitable :

- 12 à 24 frames sont extraites entre le début et la fin du calque, en une
  seule passe `ffmpeg` rawvideo RGBA (`fps` calculé sur la durée) ;
- la boîte alpha est calculée par frame avec les seuils historiques
  (`> 8` pour la boîte, `≥ 200` pour les pixels opaques) ;
- la boîte médiane sert de référence ; les frames dont l'IoU avec cette
  médiane est `< 0,70` sont écartées ;
- au moins 75 % des frames visibles doivent être stables, sinon le MOV est
  ignoré et signalé (`MOV instable: n/m échantillons stables`) ;
- la région finale est l'union des boîtes stables, avec
  `kind: "mov"` et `opaque_pixels` = maximum observé.

Le placement temporel est ramené au temps source : un end-card de 3 s placé à
44 s dans la composition est échantillonné sur `0–3 s`, pas sur `44–47 s`.
Sans cette conversion, `ffmpeg` ne renvoyait aucune frame et le masque était
perdu. La fenêtre reste `[in_point, out_point]` pour le contrôle QA
(`start_s`/`end_s`), seule la lecture de l'asset est recalée.

Un MOV animé ou multi-positions est donc **sous-masqué**, jamais sur-masqué :
le finding `logo_residual` reste non bloquant (`warn`, jamais de veto).

### Contrat et cache

- `OverlayRegion.kind` vaut `image` (PNG/JPG/WebP) ou `mov`.
- Le champ est ajouté en fin d'objet et `#[serde(default)]` : un ancien
  `qa.json` sans `kind` se relit comme `image`.
- `QA_SCHEMA_VERSION` passe à **1.5.0** ; `CACHE_VERSION` passe à **12**.
  Les anciens `qa.json` sont invalidés une fois.
- Le statut reste non bloquant : le masque écarte des détections, il ne
  provoque ni `fail` ni veto.

## Canari 20 lots récents

### Méthode

Vingt missions réelles d'octobre–novembre 2026 (une vidéo par email) ont été
rejouées sur des copies du projet brut 179 sous `/tmp/opencode/canary/lots`.
Chaque copie reprend les sidecars d'analyse réels (CSV, audio, tracking) et
pointe la même vidéo source en lecture seule sous `/mnt/cache`. La banque est
active, `a2_mission_mask = true`, et le store email est un fichier temporaire
`/tmp/opencode/canary/email_missions.sqlite3` ; le proxy pointe sur
`127.0.0.1:9` pour échouer immédiatement sans réseau. Les contrôles A2 OCR et
registre source sont désactivés pour isoler A7 ; le masque mission reste actif.

Commande :

```sh
workflow-cli --config /tmp/opencode/canary/config.toml post run \
  --folder "/tmp/opencode/canary/lots/166 Canari 01" \
  ... --report-only
```

### Résultats

| Indicateur | Valeur |
| --- | ---: |
| Lots | 20 |
| Décisions `apply` | 20 |
| Assets ajoutés | 28 |
| Provenance `central_fallback` | 18 |
| Provenance `local_docs` | 10 |
| Assets manquants | 0 |
| Absences volontaires | 0 |
| Doublons | 0 |
| `_assist.ecproj` écrits | 0 |
| Code de sortie | 0 |
| Durée | ~54 s |

Exemples représentatifs :

- lot 166 : `pp.png` en `local_docs` ;
- lot 355 : `m6+.png` local, `m6+.mov` et `pretty little liars.png` en
  `central_fallback`, end-card `tail_s = 3` ;
- lot 78 : `MSL1.mov` en `full_duration`, provenance centrale ;
- lot 20 : `m6+.mov`/`e=m6.png` centraux, `m6+.png` local.

Le seul avertissement du run est le masque A2 de `MSL1.mov` : 7/24 frames
stables, donc MOV ignoré par le masque. C'est le comportement conservateur
attendu pour un logo animé ; la mission reste `apply`.

En `--report-only`, le contrôle `mission_sidecar` de l'audit reste `warn`
(aucun `.ecproj` assisté publié) ; les contrôles `mission_assets`,
`mission_known_absent`, `mission_layers` et `mission_modes` passent sur les 20
lots.

### Cas limites contrôlés

Cinq lots supplémentaires (901–905) valident les branches de dégradation :

| Cas | Mission | Résultat |
| --- | --- | --- |
| `known_absent` | `lphdf.png` | `review`, `known_absent`, 0 calque, audit `pass` sur `mission_known_absent`, exit 0 |
| manquant | `nouveau jour.png` | `review`, `missing_asset`, 0 calque, audit `warn`, exit 0 |
| ambigu | `MSL1` | `review`, `missing_asset`, 0 calque, audit `warn`, exit 0 |
| `tail_overlay` | `MOTION_FIN_EM6_V03.mov` | `apply`, `mode: tail_overlay`, `119,76–131,76`, `tail_s = 0`, audio coupé |
| déduplication | `sdm.png` déjà présent | `noop`, `skipped: ["sdm.png"]`, 0 ajout, audit `pass` sur `mission_layers` |

### Bug trouvé et corrigé

Le canari a révélé un cas réel : un email écrit
`Saurez-vous trouver les 3 trompe-l'œil .mp4` (espace avant l'extension).
`stem_key` conservait cet espace après retrait de `.mp4`, donc
`match_video` ne trouvait pas le projet. `stem_key` est désormais
`trim()`é après le retrait de l'extension ; un test
`matches_filename_with_space_before_extension` couvre le cas. La mission lot 20
passe de `review`/`video_not_in_mission` à `apply` avec ses trois logos.

## Validation

- `cargo test -p workflow-steps post::overlay --lib` : 18 tests, dont MOV
  alpha généré, placement tardif, MOV opaque, MOV absent, frame aberrante et
  MOV instable.
- `cargo test -p workflow-core mission --lib` : 28 tests, dont l'espace avant
  extension.
- `cargo test --workspace` : **670 tests listés (668 exécutés + 2 ignorés),
  0 échec**.
- `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.
- `config check` vert sur la configuration réelle.
- Aucun `.ecproj`, `runtime/` ou `/mnt/cache` modifié ; le canari est
  entièrement sous `/tmp/opencode`.

## Limites

- Le benchmark mesure la résolution contre la banque **actuelle** ; il ne
  prouve pas la conformité visuelle des 117 assets, déjà contrôlée en 12.2.
- Les 193 manquants historiques ne sont pas des régressions : ce sont des
  émissions absentes de la banque.
- Le masque MOV ne fusionne que les frames stables ; un logo qui traverse
  l'écran ou change de taille est ignoré, avec un warning non bloquant.
- La fenêtre source est déduite de la durée du média et de la durée du calque.
  Un calque volontairement trimé sur une autre portion du MOV peut être
  sous-masqué ; c'est acceptable pour un contrôle non bloquant.
- Le canari réutilise un projet source réel et des missions réelles, mais les
  copies de projet sont synthétiques : il valide A7/banque/provenance, pas la
  production d'un `.ecproj` After Effects complet.
- Les MOV de la banque restent gitignorés (2,2 Go) et doivent être recopiés
  après un checkout.

## Références

- D-064, D-065, D-066 et D-067 dans `memory-bank/decisionLog.md`.
- `docs/lots/LOT-12-A7-MISSIONS.md`.
- `docs/lots/LOT-12.1-PARSEUR-ALIAS-TAIL.md`.
- `docs/lots/LOT-12.2-BANQUE-CENTRALE.md`.
- `docs/lots/LOT-10-QA.md`.
- `assets/mission_overlays/README.md`.
- `scripts/audit-a7-benchmark.py`.
