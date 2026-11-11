# Lot 11 — veto latéral A1 (ticket de durcissement)

> Statut : **proposé, non planifié, non implémenté**.
> Date : 2026-11-11.
> Source : `docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`, sections 3.5 et 5.6.
> Décisions liées : D-056 (marge A1 1,015), D-057/D-058 (détecteur natif
> opt-in), D-062 (activation pilote de `a2_logo_prior`).
> Code visé : `crates/workflow-steps/src/post/a1.rs`.

## Problème

A1 applique un zoom sur la hauteur active détectée par `cropdetect` sans
vérifier les **barres latérales**. Sur une source pillarboxée, le cropdetect
trouve bien des barres haut/bas (parfois aidé par une scène sombre), A1 zoome
sur la hauteur, mais la largeur active ne couvre pas la composition : le
monteur, lui, a laissé 180 %.

La variable `horizontal` est calculée dans `a1.rs` (symétrie gauche/droite)
puis **jamais utilisée comme veto**. Elle ne suffit d'ailleurs pas : sa
définition exige les **deux** côtés et un équilibre à 50 %, alors que le cas
`134 Camille — Segment 24` n'a qu'une barre gauche (64 px, droite 0).

## Preuve (corpus 248, lecture seule)

7 faux applies sur 380 applies appariés (1,84 %), tous pillarboxés :

| Projet | Segment | A1 | GOOD | Barres h/b/g/d (px) | Cause |
| --- | --- | ---: | ---: | --- | --- |
| 134 Camille — Greg veut s'expliquer | 24 | 259,1 | 180 | 218/110/64/0 | barre gauche seule + scène nocturne |
| 134 Camille — Greg veut s'expliquer | 31 | 225,6 | 180 | 96/120/164/412 | barres latérales |
| 134 Camille — Maeva et Océane embrassent | 55 | 238,8 | 180 | 142/122/162/606 | barres latérales |
| 134 Camille — Maeva et Océane s'ouvrent | 38 | 221,5 | 180 | 82/118/162/366 | barres latérales |
| 136 Camille — Maeva est toujours vexée | 44 | 206,4 | 180 | 84/52/164/428 | barres latérales |
| 182 Camille — Paga et Benjy | 26 | 225,6 | 180 | 96/120/166/458 | barres latérales |
| 210 Camille — Extrait 2 | 16 | 187,4 | 180 | 18/22/120/24 | barres latérales |

Sur les 380 applies appariés, 34 ont une barre latérale ≥ 8 px : les 7 faux
applies et 27 applies conservés. Compromis mesuré d'un veto
`gauche ≥ seuil ou droite ≥ seuil → review` :

| Seuil latéral | Applies avec barre | Faux applies couverts | Bons applies rétrogradés | Dont à ±2 pts |
| ---: | ---: | ---: | ---: | ---: |
| ≥ 8 px | 34 | **7/7** | 27 | 12 |
| ≥ 32 px | 26 | **7/7** | 19 | 12 |
| ≥ 64 px | 25 | **7/7** | 18 | 12 |
| ≥ 100 px | 20 | 6/7 | 14 | 8 |

Le seuil **32 px** (≈ 3 % de 1080) couvre les 7 cas et ne rétrograde que
19 bons applies, dont 12 à ±2 points (surtout les 243,6 % de `13 Camille`).
Le coût est de 19 revues supplémentaires, sans perte de zoom : le monteur
confirme ou ajuste. Un seuil de 100 px laisse passer `134/Segment 24`
(64 px) et ne couvre que 6/7.

## Correctif proposé

1. Dans `a1.rs`, avant l'action `apply`, calculer un seuil latéral
   `lateral_min = arrondi(32 × hauteur_source / 1080)`, minimum 1 px.
2. Si `bars.left >= lateral_min` **ou** `bars.right >= lateral_min`,
   rétrograder A1 en `review` avec un motif explicite :
   « barres latérales (gauche X px, droite Y px) : recadrage non fiable ».
   Ne pas réutiliser `horizontal` tel quel : il exige les deux côtés.
3. Laisser `noop` et `review` inchangés ; le veto ne s'applique qu'à
   `apply`.
4. Exposer le seuil dans les détails A1 (`details.lateral_min`,
   `details.bars`) pour l'audit et le rejeu.

Le brut reste inchangé et les autres décisions A1 ne bougent pas ; seule la
décision destructive est évitée. Aucun changement de schéma `post.json` en
dehors des détails A1.

## Critères d'acceptation

- 7/7 faux applies du corpus 248 passent en `review` ; 0 faux apply restant.
- ≤ 19 bons applies rétrogradés sur 380, dont 12 à ±2 pts ; aucun apply
  perdu sur les 346 segments sans barre latérale ≥ 32 px.
- Tests inline `a1.rs` : barre gauche seule `64/0`, barres `164/412`,
  barres `120/24`, aucune barre, seuil normalisé sur une source non 1080.
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`,
  `cargo test -p workflow-steps a1 --lib`.
- Rejeu du corpus 248 en `--report-only` sur un miroir (jamais le corpus
  d'origine) et comparaison aux 7 cas ; contrôle visuel des 7 frames.
- Pas de régression sur le corpus 75 GOOD ni sur les applies A1 conservés.

## Risques et limites

- **19 rétrogradations** de zooms légitimes, dont 12 à ±2 points : le coût
  est une revue, pas une perte de qualité. À valider visuellement avant
  généralisation.
- Les scènes naturellement pillarboxées (archives 4:3, split screen) seront
  revues plus souvent ; c'est le comportement conservateur attendu.
- Le seuil est dépendant de la hauteur source : le normaliser, ne pas coder
  32 px en dur.
- Le cropdetect peut produire des barres intermittentes ; ignorer les
  valeurs < 8 px pour ne pas déclencher sur le bruit d'encodage.

## Hors périmètre

- Les flags A2 `a2_logo_prior` / `a2_logo_detector` (D-062).
- STEP8, le parser `.ecproj`, A2–A6 et la QA.
- Toute modification de `config/config.toml` : le veto est un comportement
  A1, il ne nécessite pas de nouvelle clé de configuration.

## Références

- `docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md` (3.5, 5.6, annexe A).
- `crates/workflow-steps/src/post/a1.rs` (`horizontal`, `bars`).
- D-056, D-057, D-058, D-062.
