# Proposition D-058 — A1 : détecteur natif gardé ou natif v2 (arbitrage)

- **Statut** : proposée (draft, non acceptée)
- **Date** : 2026-11-09
- **Portée** : A1 du post-STEP8, `crates/workflow-steps/src/post/{a1,detect}.rs`,
  `crates/workflow-core/src/config.rs` (`PostConfig`)
- **Voir aussi** : D-057, D-056,
  `docs/etudes/ETUDE-VSLETTERBOX-A1.md` (§ 10–11),
  `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample.md`,
  `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample-v2.md`,
  `docs/lots/LOT-9-POST-STEP8.md`

## TL;DR

Le spike D-057 a d'abord montré un natif **haute précision, bas rappel** :
0 faux `apply` sur 123 segments A1 `review`/`noop`, mais 32/56 applies
clairs manqués (57,1 %) et 4/34 seulement sur le 89. La calibration v2
(Sobel 20/1020 + couverture 0,45 pour les barres très sombres) change la
donne : **33/34 applies du 89**, **5/56 faux négatifs clairs (8,9 %)**,
**0 `apply` sur les 20 `noop` et 92 vetos corrects**, et une géométrie 89 à
241,5–242,4 % contre 243,0 % pour le GOOD humain. Les 9 `apply` natifs sur
A1 `review` sont tous des 89 recadrés par l'humain ; ce ne sont pas des faux
positifs.

Trois options restent à arbitrer :

- **A — natif gardé** (draft initial) : `cropdetect` d'abord, natif
  seulement pour raffiner un `apply` déjà décidé. Sûr, mais ne récupère pas
  les 9 revues 89.
- **B — natif v2 seul** : natif primaire, sans fallback. 42/48 segments 89
  en `apply`, mais 5/56 FN clairs, 304 seg 34/36, 6 segments 89 non
  appliqués et 0/7 asymétriques.
- **C — v2 primaire + fallback `cropdetect`** : natif décide ; si
  `review`/`noop`, `cropdetect` décide avec les vetos A1. Récupère les 9
  revues 89 et les 5 FN clairs, mais promeut des revues A1 en `apply` et
  demande une réévaluation des vetos avec les barres natives.

Le mode reste **opt-in** ; `cropdetect` reste le défaut et tous les vetos A1
sont conservés.

## Contexte

D-057 a ouvert un spike natif (EWMA 5σ, Sobel Y 3×3 ciblé, couverture de
colonnes, clustering temporel). Le spike initial sur 6 fixtures était
favorable : 305 à 81/79, 304 seg 9 en `noop` (Sobel rejette le bandeau),
305 seg 17 en `review`. L'évaluation à l'échelle sur le corpus GOOD
(65 `apply`, 116 `review`, 20 `noop`) a révélé une limite de rappel sur les
barres basses sombres ou progressives, principalement dans le 89.

## Résultats — référence (Sobel 32, couverture 2/3)

| A1 \ natif | `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: |
| `apply` (65) | 25 | 26 | 14 |
| `review` (116) | 0 | 12 | 104 |
| `noop` (20) | 0 | 1 | 19 |

- **Faux `apply`** : 0/123 sur les segments A1 `review`/`noop`.
- **Faux négatifs** : 32/56 applies clairs (23 `review`, 9 `noop`), 57,1 %.
  Le 89 concentre l'échec : 4/34 applies natifs.
- **Faux vetos résolus** : 0/13 (0/7 asymétriques, 0/6 couverture).
- **Précision pixel** : 25/25 à ≤ 2 px de `cropdetect=24:2:0`, 24/25 à
  ≤ 1 px (MAE 0,84/0,68 px).
- **Coût** : natif médian 451 ms, ratio 1,12× `cropdetect=24:16:0`.

## Diagnostic des 30 échecs du 89

Sur 113 frames (226 côtés), la couverture est le blocage principal :

| Côté | frontière acceptée | couverture < 2/3 | EWMA sans déclenchement |
| --- | ---: | ---: | ---: |
| haut | 46 | **53** | 14 |
| bas | 26 | **79** | 8 |
| total | 72 (31,9 %) | **132 (58,4 %)** | 22 (9,7 %) |

Les côtés bloqués ont une couverture médiane de 0,385 (haut) et 0,418 (bas),
avec un maximum de 0,653/0,656 : les valeurs sont juste sous 2/3. Un seuil
à 0,45 pour les barres très sombres est donc le bon levier, et l'EWMA
n'explique que 22 côtés.

## Résultats — calibration v2

Paramètres : Sobel **20/1020**, couverture **2/3** (0,45 si les quatre
lignes côté barre ont une moyenne < 20/255), clustering ±2 px, dominant
≥ 60 %, marge 1,015.

| Variante | apply natif total | A1 apply → apply | 89 apply natif | FN clairs | apply sur A1 review | apply sur A1 noop/veto | revues couv. résolues |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Référence (32, 2/3) | 25 | 25/65 | 4/34 | 32/56 | 0 | 0/112 | 0/6 |
| Sobel 20 seul | 42 | 40/65 | 17/34 | 19/56 | 2 | 0/112 | 2/6 |
| Sombre 0,45 seul (< 20) | 34 | 32/65 | 9/34 | 25/56 | 2 | 0/112 | 2/6 |
| **v2 (les deux)** | **65** | **56/65** | **33/34** | **5/56** | **9** | **0/112** | **4/6** |
| v2 durci (sombre < 10) | 65 | 56/65 | 33/34 | 5/56 | 9 | 0/112 | 4/6 |

Matrice v2 :

| A1 \ natif | `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: |
| `apply` (65) | **56** | 2 | 7 |
| `review` (116) | **9** | 19 | 88 |
| `noop` (20) | **0** | 1 | 19 |

Les deux paramètres sont **synergiques** : 17/34 pour Sobel 20 seul, 9/34
pour la couverture sombre seule, 33/34 ensemble.

### Le 89

| Élément | Référence | v2 |
| --- | ---: | ---: |
| 89 `apply` (34) → natif `apply` | 4 | **33** |
| 89 `review` (14) → natif `apply` | 0 | **9** |
| 89 total (48) → natif `apply` | 4 | **42** |

- Les 30 échecs `apply` sont récupérés **29/30**. Le seul restant est le
  segment 22 (`review`, cluster 1/2 = 50 %, barres 137/137), un seuil de
  clustering sur 2 frames.
- Géométrie des 42 applies 89 : **241,5–242,4 %** (médiane 242,1) contre
  **243,0 %** pour le GOOD humain, soit 0,6 à 1,5 point d'écart. Les scales
  A1 de ces segments étaient à 249,6–290,2 %.
- Les 9 applies natifs sur A1 `review` sont **tous des 89 recadrés par
  l'humain à 243,0 %** (segments 4, 5, 10, 14, 28, 37, 42, 43, 47). Pour
  les 5 cas persistés, le `cropdetect` A1 était faux (haut 202/216/230, bas
  156/170), d'où les vetos couverture ou comp_y ; les 4 cas résolus avaient
  `cd16 = 140/140`, à 2 px près. Ce ne sont pas des faux positifs : sur les
  20 `noop` et les 92 `review` avec veto correct, le natif ne produit
  **aucun `apply` (0/112)**.
- **305 seg 3** : natif 81/79 → 211,8 %, humain 230,8 %. La barre est réelle,
  mais l'humain a recadré plus serré ; ce n'est pas un faux `apply`, c'est
  une limite de la cible de scale sur ce segment.
- **305 seg 1/9** : scale natif 211,8 % contre 210,0 % humain (+1,8 point),
  marge D-056 inchangée.

### Contrôle 304 seg 9

Le ciel nocturne reste **`noop` 5/5**, cluster 0/0 à 100 %. Les faux
candidats haut 935/936 (couverture 0,382–0,510) et bas 1002/1003
(0,158–0,372) sont rejetés :

- haut 935, frames 0/4 : lignes 931–934 à 12–18 (< 20), donc seuil 0,45,
  mais couverture 0,390/0,382 < 0,45 ;
- haut 936, frame 3 : couverture 0,510, mais lignes 932–935 à 35–46, donc
  seuil 2/3 ;
- bas 1002/1003 : côté barre clair (35–50), seuil 2/3, couverture ≤ 0,372.

Le durcissement `--dark-luma 10` donne le même `noop` et les mêmes
décisions d'`apply` sur les 201 segments ; il est plus conservateur sur les
ciels sombres et supprime 5 revues 309 (veto correct).

## Options d'arbitrage

| Option | Principe | Gain | Limite |
| --- | --- | --- | --- |
| **A — gardé** | `cropdetect` d'abord ; natif raffine un `apply` déjà décidé, cluster ≥ 60 % | 89 apply 33/34 raffinés à 242,1 ; 305/68 précis ; 0 nouvel `apply` sur review/noop | ne récupère pas les 9 revues 89 |
| **B — natif v2 seul** | natif primaire, pas de fallback | 42/48 au 89 ; 0 `apply` sur noop/veto | 5/56 FN clairs, 304 seg 34/36, 6 segments 89 non appliqués, 0/7 asymétriques |
| **C — v2 primaire + fallback `cropdetect`** | natif décide ; si `review`/`noop`, `cropdetect` décide avec les vetos A1 | 42/48 au 89 + fallback sur les 5 FN clairs et 304 | promeut 9 revues A1 (toutes recadrées humain ici) ; vetos à réévaluer |

**Recommandation de travail** : A est l'incrément sûr et immédiat (il corrige
la géométrie des 89 `apply` sans toucher au rappel des revues). C est la
cible si l'on veut automatiser les 9 revues 89 ; il faut d'abord un test sur
les 2 609 segments et une règle de réévaluation des vetos avec les barres
natives. B n'est pas promouvable en l'état à cause des 5 FN clairs et de
304 seg 34/36. Pour A et C, partir sur `dark_luma 10` : mêmes `apply` sur
l'échantillon, 5 revues 309 inutiles en moins, et plus sûr sur les ciels
sombres.

## Contrat à préserver

1. **Mode opt-in** : `cropdetect` reste le défaut ; `a1_detector` reste une
   clé de configuration additive.
2. **Contrat A1** : `apply`/`review`/`noop`, `scale`, `Position Y`,
   `coverage`, `confidence` inchangés ; clés JSON additives seulement,
   schéma `post.json` inchangé.
3. **Vetos conservés** : keyframes, asymétrie, `barred_frac`, `scale > 300 %`,
   `Position Y > 600`, couverture, différé A3.
4. **Coût** : le natif ne tourne que sur les segments où il est utile
   (65/2 609 dans le corpus GOOD pour le mode gardé), soit environ +0,45 s
   par apply, ~+2,5 % du temps A1 global.
5. **Écritures atomiques** et ordre des clés JSON préservés.

## Limites assumées

- **Revues asymétriques non résolues** : 0/7. Le natif ne remplace pas le
  veto A1 ; une amélioration du critère d'asymétrie ou de la validation des
  barres basses est un spike séparé.
- **5 faux négatifs clairs** : 305 seg 17 (`review` 4/8), 325 seg 15
  (`noop`), 335 seg 55 (`noop`, barre < 8 px), 356 seg 8 (`noop`),
  89 seg 22 (`review` 1/2). Le mode gardé les préserve par fallback ; le
  mode B les rate.
- **304 seg 34/36** : natif `noop`, `cropdetect` 80/56 et 78/58, humain
  207,0 ; bord faible, A3 doit rester le filet.
- **6 segments 89** non appliqués : 5 revues (1, 6, 17, 18, 35) et le
  segment 22 (`apply` A1, cluster 1/2) ; quatre revues ne voient qu'une
  barre. Ils sont recadrés par l'humain à 243,0.
- **Échantillon** : 201 segments, pas les 2 609 du corpus ; le contrôle
  304 seg 9 est ciblé, mais un ciel sombre et uniforme d'un autre projet
  pourrait passer la relaxation. `dark_luma 10` réduit ce risque.
- **Deux segments à une frame** (367 seg 33, 81 seg 23) : natif `review` par
  prudence ; ils sont déjà `review` A1, sans impact sur les applies.

## Plan de validation avant acceptation

1. Choisir l'option (A, B ou C) et figer les paramètres (`dark_luma` 20 ou
   10).
2. Implémenter `A1Detector { Cropdetect, Native, NativeOnly }` et le mode
   retenu dans `decide_a1`, sans changer le chemin par défaut.
3. Tests unitaires : natif applique sur le 305 ; natif `noop` sur le 89
   retombe sur `cropdetect` en mode A/C ; `review`/`noop` A1 inchangés ;
   aucun nouvel `apply` sur les 20 `noop`.
4. Test d'intégration sur 305, 68, 89, 304 seg 34 : décisions, pixel,
   `coverage`, `Position Y`.
5. Mesurer le coût sur un projet réel (A1 actuel ~23,5 s) et vérifier
   l'absence de régression sur les 2 609 segments.
6. Activer sur un seul projet pilote, puis décider de la généralisation.

## Références

- `docs/etudes/ETUDE-VSLETTERBOX-A1.md` (§ 9 spike, § 10 référence,
  § 11 calibration v2)
- `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample.md`
- `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample-v2.md`
- `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample-v2.csv`
- `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-good-sample-v2.json`
- `crates/workflow-steps/examples/a1_letterbox_spike.rs`
- `crates/workflow-steps/src/post/{a1,detect}.rs`
- `crates/workflow-core/src/config.rs` (`PostConfig`)
- D-057, D-056
