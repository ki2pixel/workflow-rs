# Proposition D-058 — A1 : détecteur natif gardé, fallback `cropdetect`

- **Statut** : proposée (draft, non acceptée)
- **Date** : 2026-11-09
- **Portée** : A1 du post-STEP8, `crates/workflow-steps/src/post/{a1,detect}.rs`,
  `crates/workflow-core/src/config.rs` (`PostConfig`)
- **Voir aussi** : D-057, `docs/etudes/ETUDE-VSLETTERBOX-A1.md` (§ 10),
  `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample.md`,
  `docs/lots/LOT-9-POST-STEP8.md`

## TL;DR

Le détecteur natif **seul** n'est pas promu : sur 201 segments GOOD, il ne
produit aucun faux `apply` (0/123) et ses 25 applies sont à ≤ 2 px de
`cropdetect=24:2:0`, mais il rate **32/56 applies clairs (57,1 %)** et ne
résout **aucune** des 7 revues asymétriques. La décision proposée est un
mode **natif gardé** : `cropdetect` reste le détecteur primaire ; le natif ne
raffine un `apply` que si `cropdetect` a déjà conclu `apply` et que le
cluster natif est dominant (≥ 60 %). Sinon, `cropdetect` fait foi. Le mode
est opt-in, `cropdetect` reste le défaut, tous les vetos A1 sont conservés.

## Contexte

D-057 a ouvert un spike natif (EWMA 5σ, Sobel Y 3×3 ciblé, couverture de
colonnes, clustering temporel). Le spike initial sur 6 fixtures était
favorable : 305 à 81/79, 304 seg 9 en `noop` (Sobel rejette le bandeau),
305 seg 17 en `review`. L'évaluation à l'échelle sur le corpus GOOD
(65 `apply`, 116 `review`, 20 `noop`) révèle une limite structurelle de
rappel, principalement sur les barres basses sombres ou progressives.

## Résultats de l'évaluation (natif seul, `gray`)

| A1 \ natif | `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: |
| `apply` (65) | 25 | 26 | 14 |
| `review` (116) | 0 | 12 | 104 |
| `noop` (20) | 0 | 1 | 19 |

- **Faux `apply`** : 0/123 sur les segments A1 `review`/`noop`.
- **Faux négatifs** : 32/56 applies clairs (23 `review`, 9 `noop`), soit
  57,1 %. Le 89 concentre l'échec : 4/34 applies natifs, à cause de barres
  basses à couverture Sobel 0,45–0,59 ou sans déclenchement EWMA.
- **Faux vetos résolus** : 0/13 (0/7 asymétriques, 0/6 couverture). Le natif
  ne remplace pas le veto A1 sur ces cas.
- **Précision pixel** : sur les 25 applies natifs, 25/25 à ≤ 2 px de
  `cropdetect=24:2:0`, 24/25 à ≤ 1 px (MAE 0,84/0,68 px).
- **Coût** : natif médian 451 ms, ratio médian 1,12× `cropdetect=24:16:0`.

Le détail par projet : 305 **15/17**, 68 **6/7**, 89 **4/34**, 304/325/335/356
0/1–2. Le natif est donc excellent sur les letterbox francs et les barres
contrastées, mais pas sur les barres basses sombres ou progressives.

## Décision proposée

1. **Ne pas promouvoir le natif seul.** `a1_detector = "native_only"` reste
   un mode de spike, hors production.
2. **Introduire `a1_detector = "native"` en mode gardé**, opt-in :
   - `cropdetect=24:16:0` est exécuté d'abord, comme aujourd'hui ;
   - si la décision A1 issue de `cropdetect` est `apply` et passe les vetos,
     le natif est exécuté sur la même fenêtre ;
   - si le natif conclut `apply` avec un cluster dominant ≥ 60 %, ses barres,
     son `scale` et sa `Position Y` remplacent ceux de `cropdetect` ;
   - sinon (natif `review`/`noop`), la décision `cropdetect` est conservée ;
   - si `cropdetect` conclut `review`/`noop`, le natif ne peut pas créer un
     `apply` : la décision reste inchangée.
3. **Conserver `cropdetect` par défaut** et tous les vetos A1 : keyframes,
   asymétrie, `barred_frac`, `scale > 300 %`, `Position Y > 600`,
   couverture, différé A3.
4. **Préserver le contrat** `apply`/`review`/`noop`, `scale`, `Position Y`,
   `coverage`, `confidence` ; clés JSON additives seulement, schéma
   `post.json` inchangé.
5. **Limiter le coût** : le natif ne tourne que sur les segments où
   `cropdetect` applique déjà (65/2 609 segments dans le corpus GOOD), soit
   environ +0,45 s par apply, ~+2,5 % du temps A1 global.

## Justification du mode gardé

- **Sécurité** : 0 faux `apply` natif sur 123 segments `review`/`noop` ; le
  mode gardé ne crée jamais d'apply là où `cropdetect` n'en crée pas.
- **Rappel préservé** : les 40 applies clairs que le natif rate retombent sur
  `cropdetect` (dont les 30/34 du 89), donc aucune régression de rappel.
- **Gain de précision** : 25 applies raffinés (305 15, 68 6, 89 4) ; sur les
  63 applies comparables, l'erreur pixel projetée passe de 2,95/2,06 px
  (cropdetect 16) à **1,98/1,54 px**, et 53/63 segments sont à ≤ 2 px
  contre 37/63.
- **Coût maîtrisé** : le natif ne tourne que sur les applies, pas sur les
  2 609 segments.

## Limites assumées

- **Revues asymétriques non résolues** : 0/7. Le natif ne remplace pas le
  veto A1 ; une amélioration du critère d'asymétrie ou de la validation des
  barres basses est un spike séparé.
- **89 dépend du fallback** : le natif seul ne détecte que 4/34 applies ;
  le mode gardé conserve la recall par `cropdetect`, mais n'améliore pas la
  géométrie du 89.
- **304 seg 34** : natif `noop`, fallback `cropdetect` apply 211,5 % ;
  A3 doit rester le filet pour les keyframes.
- **Échantillon** : 201 segments, pas les 2 609 du corpus ; seuils 32/1020,
  couverture 2/3, ±2 px et 60 % non calibrés sur un plus grand ensemble.
- **Deux segments à une frame** (367 seg 33, 81 seg 23) : natif `review` par
  prudence ; ils sont déjà `review` A1, sans impact sur les applies.

## Plan de validation avant acceptation

1. Implémenter `A1Detector { Cropdetect, Native, NativeOnly }` et le mode
   gardé dans `decide_a1`, sans changer le chemin par défaut.
2. Tests unitaires : natif applique sur le 305 ; natif `noop` sur le 89
   retombe sur `cropdetect` ; `review`/`noop` A1 inchangés ; aucun nouvel
   `apply` sur les 20 `noop`.
3. Test d'intégration sur 305, 68, 89, 304 seg 34 : décisions, pixel,
   `coverage`, `Position Y`.
4. Mesurer le coût sur un projet réel (A1 actuel ~23,5 s) et vérifier
   l'absence de régression sur les 2 609 segments.
5. Activer `a1_detector = "native"` sur un seul projet pilote, puis décider
   de la généralisation.

## Références

- `docs/etudes/ETUDE-VSLETTERBOX-A1.md` (§ 9 spike, § 10 évaluation)
- `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample.md`
- `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample.json`
- `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample.csv`
- `crates/workflow-steps/examples/a1_letterbox_spike.rs`
- `crates/workflow-steps/src/post/{a1,detect}.rs`
- `crates/workflow-core/src/config.rs` (`PostConfig`)
- D-057, D-056
