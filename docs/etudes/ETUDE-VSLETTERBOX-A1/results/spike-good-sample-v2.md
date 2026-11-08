# Spike A1 natif — calibration v2 sur l'échantillon GOOD

- **Date** : 2026-11-09
- **Statut** : preuve pour D-058, hors production ; aucun code de production modifié.
- **Corpus** : `/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2`
  (lecture seule).
- **Échantillon** : 65 `apply`, 116 `review` segment, 20 `noop` sans barre
  (201 segments, 20 vidéos distinctes pour les `noop`).
- **Variante** : `--pre gray`, 1 passe, Sobel 20/1020, couverture 2/3
  (0,45 si les quatre lignes côté barre ont une moyenne < 20/255), clustering
  ±2 px, cluster dominant ≥ 60 %, marge A1 1,015.

Commande de référence :

```sh
cargo run --release -p workflow-steps --example a1_letterbox_spike -- \
  --manifest /tmp/opencode/a1-letterbox-sample/manifest.json \
  --pre gray --edge-threshold 20 --dark-coverage 0.45 --dark-luma 20 \
  --runs 1 --out /tmp/opencode/a1-letterbox-sample/run-dark20
```

## 1. Diagnostic des 30 échecs du 89

Les 30 segments `apply` du 89 que le natif ratait en référence (Sobel 32,
couverture 2/3) ont été analysés frame par frame, en comparant la frontière
native à `cropdetect=24:2:0` (±10 px). Il y a 113 frames, donc 226 côtés.

| Côté | frontière acceptée | couverture < 2/3 | EWMA sans déclenchement |
| --- | ---: | ---: | ---: |
| haut | 46 | **53** | 14 |
| bas | 26 | **79** | 8 |
| total | 72 (31,9 %) | **132 (58,4 %)** | 22 (9,7 %) |

- La couverture est le blocage principal, pas l'EWMA : 132 côtés sur 226
  (58,4 %) ont un déclenchement EWMA mais une couverture Sobel sous 2/3.
- Couverture des côtés bloqués : haut médiane **0,385** (max 0,653), bas
  médiane **0,418** (max 0,656). Les valeurs sont juste sous 2/3, ce qui
  explique qu'un seuil à 0,45 récupère une grande partie.
- L'EWMA ne déclenche pas sur 22 côtés (9,7 %), surtout en bas (8) ; ces cas
  ne sont pas corrigés par les deux paramètres.

## 2. Delta des paramètres

| Variante | apply natif total | A1 apply → apply | 89 apply natif | FN clairs | apply sur A1 review | dont 89 recadrés | apply sur A1 noop/veto | revues couv. résolues | natif ms méd. | ratio méd. |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Référence (32 px, 2/3) | 25 | 25/65 | 4/34 | 32/56 | 0 | 0 | 0/112 | 0/6 | 450,7 | 1,12× |
| Sobel 20 seul | 42 | 40/65 | 17/34 | 19/56 | 2 | 2 | 0/112 | 2/6 | 446,1 | 1,10× |
| Sombre 0,45 seul (< 20) | 34 | 32/65 | 9/34 | 25/56 | 2 | 2 | 0/112 | 2/6 | 438,4 | 1,11× |
| **v2 (Sobel 20 + sombre 0,45 < 20)** | **65** | **56/65** | **33/34** | **5/56** | **9** | **9** | **0/112** | **4/6** | **445,6** | **1,11×** |
| v2 durci (sombre < 10) | 65 | 56/65 | 33/34 | 5/56 | 9 | 9 | 0/112 | 4/6 | 446,1 | 1,11× |

Les deux paramètres sont **synergiques** : Sobel 20 seul récupère 17/34
applies du 89, la couverture sombre seule 9/34, et les deux ensemble 33/34.
Le durcissement `< 10` ne change aucune décision d'`apply` sur cet
échantillon ; il transforme seulement 5 revues 309 (veto correct) en
`noop`, ce qui est plus sûr.

## 3. Matrice v2

| A1 \ natif | `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: |
| `apply` (65) | **56** | 2 | 7 |
| `review` (116) | **9** | 19 | 88 |
| `noop` (20) | **0** | 1 | 19 |

Détail :

| Sous-ensemble | n | natif `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: | ---: |
| apply accepté | 11 | 7 | 1 | 3 |
| apply ajusté | 45 | 44 | 1 | 0 |
| apply reverté | 9 | 5 | 0 | 4 |
| review veto correct | 92 | 0 | 10 | 82 |
| review résolue | 13 | 4 | 4 | 5 |
| — dont asymétrique | 7 | 0 | 2 | 5 |
| — dont couverture | 6 | 4 | 2 | 0 |
| review persistée / sans défaut | 11 | 5 | 5 | 1 |
| noop | 20 | 0 | 1 | 19 |

## 4. Le 89 : rappel et géométrie

| Élément | Référence | v2 |
| --- | ---: | ---: |
| 89 `apply` (34) → natif `apply` | 4 | **33** |
| 89 `review` (14) → natif `apply` | 0 | **9** |
| 89 total (48) → natif `apply` | 4 | **42** |
| 89 total → natif `review` | 29 | 6 |
| 89 total → natif `noop` | 15 | 0 |

- Les 30 échecs `apply` sont récupérés **29/30**. Le seul restant est le
  segment 22 (`adjusted_house`) : natif `review` avec cluster **1/2 (50 %)**,
  barres 137/137, donc un problème de seuil de clustering sur 2 frames, pas
  de couverture.
- Les 6 segments 89 non appliqués sont 5 revues (1, 6, 17, 18, 35) et le
  segment 22 (`apply` A1, cluster 1/2). Quatre des cinq revues ne voient
  qu'une barre (138/0 ou 0/138) ; le segment 18 voit 137/136 mais son
  cluster n'est que 1/2. Ils sont tous recadrés par l'humain
  (`good_scale = 243,0`). Les 5 revues ne faisaient pas partie des 30
  échecs `apply` ; le segment 22 est le seul échec restant.
- Géométrie des 42 applies 89 : barres 137/138 ou 138/138, **scale natif
  241,5–242,4 %** (médiane 242,1) contre **243,0 %** pour le GOOD humain,
  soit **0,6 à 1,5 point** d'écart. C'est très supérieur aux scales A1
  (`cropdetect`) de ces segments (249,6 à 290,2 %) et au scale assist 180 %.
- **305 seg 3** (apply A1 reverté) : natif 81/79 → 211,8 %, humain 230,8 %.
  La barre est réelle, mais l'humain a recadré plus serré ; ce n'est pas un
  faux `apply`, c'est une limite de la cible de scale sur ce segment.

### Les 9 applies natifs sur A1 `review`

Les 9 sont des segments 89, tous recadrés par l'humain à `good_scale = 243,0`
(assist 180 %, ratio 1,35) :

| Segment | A1 | `cd16` | Natif | Scale natif |
| --- | --- | ---: | ---: | ---: |
| 4 | review coverage persistée | 140/156 | 138/138 | 242,4 |
| 5 | review coverage persistée | 202/142 | 138/138 | 242,4 |
| 10 | review comp_y persistée | 216/144 | 138/137 | 242,1 |
| 14 | review coverage résolue | 140/140 | 137/137 | 241,8 |
| 28 | review coverage persistée | 142/170 | 138/138 | 242,4 |
| 37 | review coverage résolue | 140/140 | 137/138 | 242,1 |
| 42 | review coverage résolue | 140/140 | 138/138 | 242,4 |
| 43 | review coverage résolue | 140/140 | 137/137 | 241,8 |
| 47 | review comp_y persistée | 230/146 | 138/138 | 242,4 |

Ce ne sont donc **pas des faux `apply`** : pour les 5 cas persistés, le
`cropdetect` A1 était faux (haut 202/216/230, bas 156/170), ce qui
déclenchait les vetos couverture ou comp_y ; les 4 cas résolus avaient
`cd16 = 140/140`, à 2 px près, et le natif resserre à 137–138. Le natif
retrouve la barre 138/138 et le scale humain 243,0. Sur les 20 `noop` et les
92 `review` avec veto correct, le natif ne produit **aucun `apply`
(0/112)**. Le taux de 5/123 de la matrice est un artefact du label A1
`review` utilisé comme vérité terrain ; rapporté au GOOD humain, ces 5 cas
sont des vrais positifs.

## 5. Contrôle 304 seg 9 (ciel nocturne)

Le contrôle `304-seg9` reste **`noop` 5/5**, cluster 0/0 à 100 %, avec les
deux paramètres. Les faux candidats :

| Frame | Haut (couverture) | Bas (couverture) |
| --- | --- | --- |
| 0 | 935 (0,390), 936 (0,391) | 1002 (0,199), 1003 (0,188) |
| 1 | 935 (0,444), 936 (0,432) | 1002 (0,255), 1003 (0,244) |
| 2 | 936 (0,479) | 1002 (0,295), 1003 (0,261) |
| 3 | 936 (0,510) | 1002 (0,372), 1003 (0,345) |
| 4 | 935 (0,382), 936 (0,382) | 1002 (0,172), 1003 (0,158) |

La relaxation sombre ne s'applique que si les quatre lignes côté barre sont
< 20/255 :

- haut 935, frames 0 et 4 : lignes 931–934 à 12–18, donc seuil 0,45, mais
  couverture 0,390/0,382 < 0,45 ;
- haut 936, frame 3 : couverture 0,510, mais lignes 932–935 à 35–46, donc
  seuil 2/3, rejeté ;
- bas 1002/1003 : côté barre clair (35–50), seuil 2/3, couverture ≤ 0,372,
  rejeté.

Le durcissement `--dark-luma 10` donne le même `noop` et est encore plus
conservateur (les lignes 931–934 du ciel, à 16–18, ne sont plus « sombres »).
La variante `dark_luma 10` conserve exactement les mêmes décisions d'`apply`
sur les 201 segments et transforme 5 revues 309 (veto correct) en `noop`.

## 6. Faux négatifs et limites

Les 5 faux négatifs sur les 56 applies clairs :

| Segment | A1 | Natif | Bars / `cd16` | Cause |
| --- | --- | --- | ---: | --- |
| 305 seg 17 | accepté | review 4/8 (50 %) | 81/79 | transition letterbox → plein cadre |
| 325 seg 15 | accepté | noop 0/0 | 156/124 | barre non détectée |
| 335 seg 55 | accepté | noop 0/2 | 16/8 | barre basse < 8 px, `active ≥ 1072` |
| 356 seg 8 | accepté | noop 0/0 | 30/58 | barre non détectée |
| 89 seg 22 | ajusté | review 1/2 (50 %) | 137/137 | cluster 1/2 sur 2 frames |

À quoi s'ajoutent :

- **304 seg 34/36** : natif `noop`, `cropdetect` 80/56 et 78/58, humain
  `good_scale = 207,0` ; bord faible, faux négatif structurel déjà connu.
  A3 doit rester le filet.
- **325 seg 19 et 8 seg 16** : natif `noop`, humain `good_scale = 180`
  (pas de recadrage) ; le `noop` est correct, même si A1 avait appliqué.
- **0/7 revues asymétriques résolues** : le natif ne remplace pas le veto
  asymétrique. Les 2 cas 78 restent `review` (23/0).
- **6 segments 89** non appliqués : 5 revues (1, 6, 17, 18, 35) et le
  segment 22 (`apply` A1, cluster 1/2) ; quatre revues ne voient qu'une
  barre.
- **Échantillon** : 201 segments, pas les 2 609 du corpus. Le ciel nocturne
  304 seg 9 est le contrôle ciblé, mais un ciel sombre et uniforme d'un autre
  projet pourrait passer la relaxation ; `dark_luma 10` réduit ce risque.
- **305 seg 1/9** : scale natif 211,8 % contre 210,0 % humain (+1,8 point).
  C'est la marge D-056 (1,015), pas la détection ; la calibration v2 ne
  change pas ce segment.

## 7. Coût

- Analyse native : médiane **17,6 ms**, p95 **38,1 ms**.
- Total natif : médiane **445,6 ms** ; `cropdetect=24:16:0` : 398,1 ms ;
  ratio médian par segment **1,11×** (1,12× sur les médianes).
- Le décodage domine (427,7 ms) ; les deux paramètres ne coûtent rien.

## 8. Options pour D-058

| Option | Principe | Gain | Limite |
| --- | --- | --- | --- |
| **A — gardé** (draft actuel) | `cropdetect` d'abord ; natif raffine seulement un `apply` déjà décidé, cluster ≥ 60 % | 89 apply 33/34 raffinés à 242,1 ; 305/68 précis ; aucun nouvel `apply` sur review/noop | ne récupère pas les 9 revues 89 recadrées par l'humain |
| **B — natif v2 seul** | natif primaire, pas de fallback | 42/48 segments 89 en `apply` (dont 9 revues) ; 0 `apply` sur les 20 `noop`/92 vetos | 5/56 FN clairs, 304 seg 34/36, 6 segments 89 non appliqués, 0/7 asymétriques |
| **C — v2 primaire + fallback `cropdetect`** | natif décide ; si `review`/`noop`, `cropdetect` décide avec les vetos A1 | 42/48 au 89 + fallback sur les 5 FN clairs et 304 ; 0 nouvel `apply` sur noop/veto | promeut 9 revues A1 en `apply` (toutes recadrées humain ici) ; il faut réévaluer les vetos avec les barres natives et valider sur plus grand |

Recommandation de travail : **A** est l'incrément sûr et immédiat (il corrige
la géométrie des 89 `apply` sans toucher au rappel des revues). **C** est la
cible si l'on veut automatiser les 9 revues 89 ; il faut d'abord un test sur
les 2 609 segments et une règle de réévaluation des vetos avec les barres
natives. **B** n'est pas promouvable en l'état à cause des 5 FN clairs et de
304 seg 34/36.

Le durcissement `dark_luma 10` est équivalent sur les `apply` de cet
échantillon, plus sûr sur les ciels sombres et supprime 5 revues 309
inutiles ; c'est la valeur de départ la plus sûre. Garder `20` si l'on veut
conserver ces revues.

## Références

- [`../../ETUDE-VSLETTERBOX-A1.md`](../../ETUDE-VSLETTERBOX-A1.md) (§ 10–11)
- [`spike-good-sample.md`](spike-good-sample.md) (référence 32 px / 2/3)
- [`spike-good-sample-v2.csv`](spike-good-sample-v2.csv) (201 segments, v2)
- [`spike-good-sample-v2.json`](spike-good-sample-v2.json) (matrice et taux)
- [`../../../decisions/DECISION-A1-NATIVE-GARDE-D058.md`](../../../decisions/DECISION-A1-NATIVE-GARDE-D058.md)
- `crates/workflow-steps/examples/a1_letterbox_spike.rs`
