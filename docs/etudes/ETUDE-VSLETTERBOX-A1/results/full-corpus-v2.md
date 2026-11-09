# A1 natif gardé — validation sur le corpus GOOD complet

- **Date** : 2026-11-09
- **Statut** : preuve pour D-058 option A, hors production ; corpus en
  lecture seule.
- **Corpus** : `/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2`
  (75 projets, 2 609 segments inventoriés).
- **Périmètre évalué** : **2 607 segments porteurs d'une décision A1**.
  Le décompte 2 609 de l'inventaire GOOD inclut les couches `[small]` et
  `[main]` ajoutées par le template SDM dans l'assist du projet 328 ; la
  composition brute de ce projet ne contient qu'un seul `Segment`, hors A1.
- **Mode** : `a1_refine_native = true`, Sobel 20/1020, couverture 2/3
  (0,45 sous 10/255), clustering ±2 px, dominant ≥ 60 %, marge 1,015.
- **Référence pixel** : `cropdetect=24:2:0` sur la même fenêtre
  `start + 0,05`, durée `segment - 0,1` bornée à [0,3 ; 4,0].

## Commande

```sh
cargo run --release -p workflow-steps --example a1_full_eval -- \
  --out /tmp/opencode/a1-letterbox-full-eval
```

Le harnais appelle le vrai `post::a1::decide_a1` deux fois par projet
(`refine_native = false` puis `true`, ordre alterné pour limiter l'effet du
cache), mesure le temps A1, puis exécute `cropdetect=24:2:0` sur chaque
apply raffiné. Sorties : `summary.json`, `segments.csv`, `refined.csv`.

## 1. Décisions A1

| Mode | `apply` | `review` | `noop` | total |
| --- | ---: | ---: | ---: | ---: |
| `refine_native = false` | 65 | 116 | 2 426 | 2 607 |
| `refine_native = true` | 65 | 116 | 2 426 | 2 607 |

- **0 changement d'action** : le natif ne crée ni ne retire aucun `apply`.
- **52 applies raffinés sur 65** (80,0 %).
- Répartition : 305 **16**, 68 **7**, 89 **29**.
- 13 applies non raffinés : 9 cas où le natif est `noop`/`review`
  (304 seg 34/36, 305 seg 17, 325 seg 15/19, 335 seg 55, 356 seg 8,
  8 seg 16, 89 seg 22) et 4 cas 89 (seg 13, 20, 31, 32) où le natif voit
  un apply mais où le veto de couverture sur les crops `cropdetect`
  d'origine le rejette.

## 2. Gain pixel

Sur les 52 applies raffinés, erreur moyenne par côté vs
`cropdetect=24:2:0` :

| Mesure | Valeur |
| --- | ---: |
| MAE `cropdetect=24:16:0` | **2,231 px** |
| MAE natif raffiné | **0,769 px** |
| Gain moyen | **1,462 px** |
| Gain médian | **1,750 px** |
| Gain p10 / p90 | 1,000 / 2,000 px |
| Gain min / max | −2,000 / 2,500 px |
| Améliorés / neutres / dégradés | **47 / 1 / 4** |

- Le gain est positif sur **47/52** applies.
- Les **4 dégradations** sont toutes sur le 89 (seg 3, 27, 38, 40) :
  `cropdetect=24:16:0` y est déjà exact à 140/140, le natif donne
  137–138/138, soit 0,5 à 2 px d'écart. Elles sont faibles et bornées.
- Le scale natif baisse de **1,62 point en moyenne** (médiane 1,65,
  min −2,10, max −1,20) par rapport à `cropdetect=24:16:0`.
- Les 4 cas 89 non raffinés malgré un natif `apply` (13, 20, 31, 32) ont un
  `cropdetect=24:2:0` lui-même bruité (bas 248, haut 184, haut 160,
  haut 174) : le veto de couverture est conservateur, mais il protège
  contre ces mesures instables. À instruire visuellement avant tout
  assouplissement.

## 3. Coût

| Mesure | Valeur |
| --- | ---: |
| A1 `refine_native = false` | **1 102,4 s** |
| A1 `refine_native = true` | **1 131,7 s** |
| Surcoût global | **29,27 s** |
| Surcoût relatif | **2,66 %** |
| Surcoût par apply raffiné | **563 ms** |

Le surcoût reste marginal : le natif ne tourne que sur les 65 applies, pas
sur les 2 426 `noop` ni les 116 `review`.

## 4. Conclusion pour la bascule

Le mode gardé est **sûr** : 0 changement d'action sur 2 607 segments,
0 erreur, et un gain pixel moyen de **1,46 px** pour **2,66 %** de temps A1
en plus. Les limites sont documentées :

- 4 régressions de 0,5 à 2 px sur le 89, où `cropdetect=24:16:0` est déjà
  exact ;
- 4 applies 89 non raffinés par le veto de couverture, avec une référence
  `24:2:0` bruitée ;
- 9 faux négatifs/natifs `noop`/`review` déjà connus (304 seg 34/36,
  305 seg 17, 325 seg 15/19, 335 seg 55, 356 seg 8, 8 seg 16, 89 seg 22).

Recommandation : activation **pilote** sur un projet, puis bascule par
défaut si le rendu visuel confirme les 4 cas 89 et si le coût de 2,66 %
reste acceptable en production. Ne pas passer directement à l'option C
(D-059) sans traiter ces 4 cas.
