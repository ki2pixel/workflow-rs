# Étude — post-STEP8 sur 248 projets finalisés (Corpus_Exemple_3)

> Statut : étude en lecture seule ; décision D-062 appliquée ensuite
> (`a2_logo_prior`). Aucun code de production modifié.
> Date : 2026-11-11. Corpus : `Corpus_Exemple_3`, 248 projets, 92 dossiers,
> 9 779 segments. Sorties d'analyse :
> `/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/benchmark_corpus3/`.
> Décisions liées : D-030, D-032, D-051, D-052, D-055, D-056, D-058, D-060,
> D-062.
> Lots liés : LOT-9, LOT-10, LOT-11, B2/B2.1.

## 1. Objet et résumé exécutif

La question posée est double :

1. mesurer la **conformité globale** de la post-STEP8 actuelle (A1, A2, A3,
   A4, A6) au travail du monteur, sur un corpus massif de projets finalisés ;
2. objectiver l'**activation en production** de `a2_logo_prior` (catalogue
   D-052) et `a2_logo_detector` (YOLOX-Nano B2.1, annotation seule), pour
   savoir s'ils qualifient sûrement les revues A2 sans créer d'erreur.

Le corpus contient 248 triplets complets `_9x16.ecproj` /
`_9x16_assist.ecproj` / `_9x16_assist_GOOD.ecproj`, soit 744 projets
EffectCraft et 9 779 segments bruts. Tous les triplets sont présents, ainsi
que les 248 `post.json`, `audit.json` et `review.json`. Le corpus est resté
en lecture seule ; les vidéos et JSON annexes sont liés par symlink dans un
miroir de travail sous `/media/kidpixel/WORK/`, et seuls les 248 `.ecproj`
bruts (160 Mo) ont été copiés.

### 1.1 Résultats clés

| Mesure | Valeur | Lecture |
| --- | ---: | --- |
| Segments bruts / appariés au GOOD | 9 779 / 9 359 (95,7 %) | 420 non mesurables, concentrés sur 8 projets renommés |
| Accord strict de scale (`±0,01` point) | 58,93 % | 5 474 segments sans zoom + 41 cas A2 exacts |
| Matrice de zoom (TP / FP / FN / TN) | 660 / 7 / 3 218 / 5 474 | précision 98,95 %, rappel 17,02 % |
| MAE globale des scales | 19,32 pts | dominée par les reframes sémantiques du monteur |
| MAE conditionnelle « les deux zooment » | 9,87 pts (p95 55,53) | comparaison la plus juste |
| A1 `apply` conservés par le monteur | 373 / 380 (98,16 %) | 7 faux applies (1,84 %) |
| A2 `apply` conservés | 287 / 287 segments (100 %) | 0 faux apply, 0 sur-zoom |
| A2 `review` avec bandeau humain | 35 / 175 (20,0 %) | 13 recadrages « cible A2 » manqués (7,4 %), dont 1 récupéré par le prior |
| A3 `review` avec clés ajoutées | 211 / 667 (31,6 %) | au-dessus du taux de base 12,1 % |
| A4 `review` avec action structurelle | 4 / 67 (6,0 %) | revues très peu suivies |
| A5 `review` résolues par le zoom humain | 111 / 111 | report-only par contrat |
| Shadow 248 `prior + detector` — `apply` | 1 créé / 0 perdu | `257 Camille` 218,2 % contre 215,3 % humain |
| Shadow — changements d'action | 3 | 1 `apply` correct, 2 fausses revues (`108`, `243`) |
| Shadow — actions de revue confirmées | 41 / 199 | toutes étroites ; aucune action de bandeau confirmée |
| Shadow — coût du détecteur | 2 313 ms/projet | 573,6 s pour 248 projets, max 5,34 s |

La post-STEP8 est **très conservative et globalement sûre**, mais elle n'est
pas « zéro faux apply » : 7 segments A1 sur 380 (1,84 %) ont été zoomés par
l'automate alors que le monteur a laissé 180 %. Ces 7 cas partagent une
signature nette — une **barre latérale** (gauche 64–166 px ou droite
24–606 px) que le veto A1 actuel ne regarde pas. Le rappel est faible
(17,02 %) parce que le monteur zoome aussi pour des raisons sémantiques :
39,4 % des segments A1 `noop` sont zoomés dans le GOOD, contre 20,2 % des
segments A1 `review`.

A2 est le module le plus solide : 9 `apply`, 287 segments, aucun revert,
aucun sur-zoom, et une échelle toujours **inférieure ou égale** à celle du
monteur. A2 rate en revanche 13 recadrages de bandeau sur 175 revues
mesurables (7,4 %), dont deux bandes sombres où le code calcule déjà le
`scale_target` exact du monteur.

La suite du document détaille la méthode, les matrices, les cas, puis la
shadow evaluation des deux détecteurs A2. **Décision D-062** : `a2_logo_prior`
est activé en pilote contrôlé (1 `apply` correct, 0 faux apply, 2 fausses
revues sur 248 projets) et `a2_logo_detector` reste `false` (annotation seule,
aucun gain de décision, 2,3 s/projet).

## 2. Corpus et méthode

### 2.1 Structure réelle

Le corpus n'est pas 248 dossiers mais **92 dossiers clients** contenant
248 vidéos. Chaque vidéo est un projet :

```
<client>/projets/<stem>_9x16.ecproj              # brut STEP8
<client>/projets/<stem>_9x16_assist.ecproj       # sortie de la post actuelle
<client>/projets/<stem>_9x16_assist_GOOD.ecproj  # vérité terrain du monteur
<client>/docs/post/<stem>_9x16_assist/post.json  # décisions A1–A6
<client>/docs/post/<stem>_9x16_assist/audit.json
<client>/docs/post/<stem>_9x16_assist/review.json
<client>/docs/<stem>.mp4                         # rush source
<client>/docs/<stem>_ae.json, <stem>.csv         # analyses STEP4/STEP2
```

L'inventaire `python3 scripts/benchmark-corpus3.py index` donne :

| Élément | Présent | Manquant |
| --- | ---: | ---: |
| Projets bruts | 248 | 0 |
| `_9x16_assist.ecproj` | 248 | 0 |
| `_9x16_assist_GOOD.ecproj` | 248 | 0 |
| `post.json` | 248 | 0 |
| `audit.json` | 248 | 0 |
| `review.json` | 248 | 0 |

Les 248 audits sont `pass` (245) ou `warn` (3, uniquement
`source_coverage` préexistant). Aucun `fail`. Les trois `warn` sont des
projets pour lesquels la source ne couvre pas toute la composition ; A5 les
déclare `resolved` / `report_only`.

### 2.2 Indexation des décisions

Le parseur relit la composition qui contient le plus de calques
`Segment…` (même règle que `post::ecproj::comp_id` en Rust) et extrait pour
chaque segment :

- `Scale` (vecteur, on garde X), `Position`, `Anchor Point`, le nombre de
  clés de chaque propriété, les bornes `in_point` / `out_point` ;
- la position **dans la composition** (`comp_x`, `comp_y`) en appliquant le
  parent Null comme le fait `post::ecproj::parent_info`. C'est indispensable :
  le brut garde un parent Null (`Position 540,0`, ancre `50,50`) alors que le
  GOOD a souvent « cuit » l'offset dans le calque. Comparer les `Position`
  locales donnerait 100 % de faux mouvements.

Les décisions sont relues depuis `post.json` :

- A1 par segment via `details.start` (les 25 décisions A1 `target = comp`
  « compensation Y médiane … » sont traitées à part) ;
- A2 au niveau projet : `apply` si au moins une décision A2 est `apply`,
  sinon `review` si au moins une est `review`, sinon `noop` ;
- A3/A4/A5 par décision, avec leur cible.

Tous les bruts ont `Scale = 180 %` (9 779/9 779). Le zoom est donc défini
comme `|scale − 180| > 0,01` point. Cette définition est exacte pour ce
corpus : aucune échelle initiale différente de 180.

### 2.3 Appariement brut → GOOD

Le GOOD n'est pas toujours une copie structurelle du brut :

- 151 projets ont **plus** de calques GOOD que de segments bruts
  (duplications `… 2`, `… 3` par le monteur) ;
- 20 projets en ont **moins** (fusions ou suppressions) ;
- 77 projets ont exactement le même nombre ;
- 182 projets ont des noms de calques différents (suffixes de duplication,
  renommage complet).

L'appariement est déterministe, par ordre de priorité :

| Règle | Segments | Part |
| --- | ---: | ---: |
| Nom exact | 9 179 | 93,87 % |
| Bornes `in_point`/`out_point` exactes | 155 | 1,58 % |
| Bornes à 40 ms | 7 | 0,07 % |
| Recouvrement temporel ≥ 0,80 | 18 | 0,18 % |
| Non apparié | 420 | 4,29 % |

Les 420 non appariés sont concentrés sur **8 projets** dont le GOOD a
renommé tous les calques en `<source>.mp4` (dossiers `227`, `255`, `303`) :
344 des 420 viennent de ces 8 projets. Les 34 autres projets ont 1 à 15
segments manquants (fusions, suppressions). Les projets dont la couverture
d'appariement est < 50 % sont marqués **non mesurables** et exclus des taux
d'action projet A2 ; ils restent comptés dans l'inventaire.

### 2.4 Définitions

- **Accord strict** : `|scale_assist − scale_GOOD| ≤ 0,01` point.
- **MAE** : moyenne des `|scale_assist − scale_GOOD|` en points de scale.
- **p95** : 95ᵉ centile des écarts absolus, en points.
- **Zoom** : scale > 180,01. **Pas de zoom** : scale ≤ 180,01.
- **Bandeau humain (`banner_like`)** : échelle dominante du GOOD comprise
  entre 190 et 230 points, couvrant ≥ 50 % des segments appariés. Cette
  plage correspond aux `apply` A2 observés (197,6–206,6) et à la formule
  `1920 / (1080 − h) × 100 × 1,05` pour une bande de 24–200 px.
- **Punch-in sémantique** : échelle dominante > 230 points (le monteur
  recadre pour le cadre, pas pour retirer une bande). 2 644 segments sont
  dans cette classe, dont beaucoup à 243 %.
- **Revue « cible A2 »** : projet `banner_like` dont l'échelle dominante est à
  ±8 points de la cible A2. Le filtre `banner_like` évite de compter un zoom
  mixte dont la valeur dominante est minoritaire (cas `112 Camille — SOSIE
  CELINE`, couverture 6 %). C'est la définition d'un recadrage de bandeau
  manqué, distinct d'un reframe sémantique.
- **Faux apply** : l'automate zoome (`assist > 180,01`) et le GOOD reste à
  180,00 sur le même segment.

### 2.5 Limites de mesure

1. **Le GOOD n'est pas dérivé de l'assist courant.** Il a été finalisé par
   le monteur sur une version antérieure du pipeline. Les bornes de segments
   peuvent différer de quelques images ; c'est pourquoi l'appariement
   tolère 40 ms et 4,29 % de segments restent non appariés.
2. **Le monteur zoome massivement pour des raisons sémantiques.** Le taux de
   zoom humain sur les segments A1 `noop` est de 39,43 % (3 468/8 796).
   Toute MAE non stratifiée mélange donc le letterbox et le reframe
   créatif ; les tableaux par étage sont la seule lecture valide.
3. **Les positions locales ne sont pas comparables** entre brut et GOOD à
   cause du parent Null ; les calculs utilisent `comp_x`/`comp_y`.
4. **8 projets ne sont pas mesurables au segment** (GOOD renommé). Ils
   restent dans les totaux de décisions, mais sont exclus des taux d'action
   projet A2.
5. **Le corpus est mono-monteur et mono-période.** Les 248 GOOD viennent de
   la même chaîne de finalisation ; les conclusions ne sont pas une vérité
   universelle, seulement une mesure de convergence sur ce corpus.

## 3. Phase 2 — Conformité de la post-STEP8

### 3.1 Matrice de confusion globale

Au niveau segment, en comparant le zoom de l'automate et celui du monteur :

| | GOOD zoome | GOOD ne zoome pas | Total |
| --- | ---: | ---: | ---: |
| **Assist zoome** | 660 (TP) | 7 (FP) | 667 |
| **Assist ne zoome pas** | 3 218 (FN) | 5 474 (TN) | 8 692 |
| Total | 3 878 | 5 481 | 9 359 |

- **Précision** : 660 / 667 = **98,95 %**.
- **Rappel** : 660 / 3 878 = **17,02 %**.
- **Accord strict de scale** : 5 515 / 9 359 = **58,93 %**
  (5 474 TN + 41 cas A2 exactement égaux).
- **MAE globale** : **19,32 points**, médiane 0, p95 **73,70**, p99 101,48,
  max 422,54.
- **Écart signé moyen** : **−18,73 points** (l'automate zoome moins que le
  monteur), p05 −73,33, p95 0.
- **MAE conditionnelle « les deux zooment »** (660 segments) :
  **9,87 points**, médiane 2,26, p95 **55,53**, p99 69,07.

Le rappel de 17 % ne signifie pas que l'automate rate 83 % des letterbox :
il signifie que **le monteur zoome aussi en dehors du périmètre A1/A2**. La
matrice par étage ci-dessous isole ce qui relève de chaque module.

### 3.2 A1 — letterbox

#### 3.2.1 Décisions

| Décision A1 | Total | Appariés GOOD |
| --- | ---: | ---: |
| `apply` | 382 | 380 |
| `review` segment | 207 | 183 |
| `review` comp « compensation Y médiane » | 25 | — |
| `noop` | 9 190 | 8 796 |

Les 25 revues `comp` sont ajoutées dès qu'au moins un `apply` A1 porte une
compensation Y ; elles n'annulent pas les `apply`. Elles expliquent pourquoi
certains projets ont à la fois des `apply` et une revue « cadrage vertical à
valider ».

#### 3.2.2 `apply` : conservation et précision

| Indicateur | Valeur |
| --- | ---: |
| `apply` appariés | 380 |
| Conservés (GOOD zoome) | **373 (98,16 %)** |
| Revertés (GOOD reste 180) | **7 (1,84 %)** |
| À ±1 point du GOOD | 225 / 373 (60,3 %) |
| À ±2 points | 237 / 373 (63,5 %) |
| À ±5 points | 299 / 373 (80,2 %) |
| MAE (conservés) | **9,09 pts**, médiane 0,6, p95 59,76 |
| MAE avec marge retirée (`scale/1,015`) | 11,36 pts, médiane 3,0 |

La marge D-056 de 1,015 est **mieux calibrée que 1,0** : retirer la marge
dégrade la MAE de 9,09 à 11,36 points et décale la médiane de +0,6 à
−3,0 points. La distribution est très asymétrique : 63,5 % des `apply` sont
à ±2 points, mais la p95 est à 59,76 parce que le monteur ajoute souvent un
punch-in sémantique par-dessus le letterbox.

La compensation Y est également validée :

- médiane de l'écart `Y_assist − Y_GOOD` : **0,6 px** ;
- 228 / 380 (60,0 %) à ±20 px ;
- 366 / 380 (96,3 %) des `apply` ont un mouvement Y humain ;
- 14 / 380 seulement restent à Y constant.

#### 3.2.3 `review` : opportunités manquées ?

| Indicateur | Valeur |
| --- | ---: |
| Revues segment appariées | 183 |
| Le monteur zoome | **37 (20,22 %)** |
| Le monteur ne zoome pas | 146 (79,78 %) |
| Zoom humain proche de la cible A1 (`±3 pts`) | **3 (1,64 %)** |
| Delta humain moyen quand il zoome | **+50,7 pts** (médiane 44,9) |

Les revues A1 ne sont donc **pas** un gisement de letterbox manqués :
seules 3 revues sur 183 (1,6 %) correspondent à un zoom proche de la cible
A1. Les 34 autres zooms humains sont des reframes sémantiques de +26 à
+102 points. Le taux d'action sur les revues (20,2 %) est même **inférieur**
au taux de zoom humain sur les `noop` (39,4 %) : le monteur agit moins sur
les segments déjà signalés, pas plus.

#### 3.2.4 `noop` : le bruit de fond sémantique

3 468 / 8 796 segments `noop` (39,43 %) sont zoomés dans le GOOD. Ces
segments n'ont pas de barres noires (`active_height ≥ 1 072`), donc A1 ne
peut pas les revendiquer. C'est la **ligne de base du reframe créatif** :
toute métrique globale non stratifiée la compte comme un faux négatif A1,
ce qui serait faux.

### 3.3 A2 — bandeaux bas

#### 3.3.1 `apply` : conservation et sur-zoom

| Indicateur | Valeur |
| --- | ---: |
| Projets `apply` | 9 |
| Segments appariés couverts par l'`apply` | 287 |
| Segments conservés (GOOD ≥ assist − 2 pts) | **287 / 287 (100 %)** |
| Segments revertés | **0** |
| Sur-zoom > 2 points | **0** |
| Écart `assist − GOOD` max | **+0,3 pt** (l'automate est toujours ≤ humain) |
| Écart `assist − GOOD` moyen | −10,80 pts (le monteur zoome plus) |
| Médiane / p95 de l'écart | −10,44 / −19,86 |

Détail par projet :

| Projet | A2 | Monteur (dominante) | Écart |
| --- | ---: | ---: | ---: |
| 41 Camille — UJUD ACHETEUSE COMPULSIVE | 206,6 | **206,61** | +0,01 |
| 98 Camille — UJUD PRODUITS BEBE | 206,6 | 206,30 | −0,30 |
| 47 Camille — UJUD MENAGE | 200,8 | 216,30 | −15,5 |
| 98 Camille — UJUD DESTOCKEUR | 206,6 | 213,17 | −6,6 |
| 98 Camille — UJUD COUPONS | 206,6 | 217,04 | −10,4 |
| 197 Camille — PARFUMS | 204,0 | 217,73 | −13,7 |
| 227 Camille — ARNAQUE VOITURE | 199,2 | 213,02 | −13,8 |
| 261 Camille — Lamborghini Revuelto | 206,6 | 222,57 | −16,0 |
| 284 Camille — sommeil | 197,6 | 217,46 | −19,9 |

Deux projets sur neuf sont à ≤ 0,3 point ; les sept autres sont **sous-zoomés
de 6,6 à 19,9 points** par rapport au monteur, qui ajoute un punch-in
supplémentaire. Aucun cas d'automate plus agressif que l'humain : la règle
« zéro faux apply » est respectée pour A2.

#### 3.3.2 `review` : ce que le monteur a réellement fait

Sur 180 projets `review`, 175 sont mesurables (5 projets au GOOD renommé) :

| Indicateur | Valeur |
| --- | ---: |
| Revues mesurables | 175 |
| Zoom humain (toutes raisons) | 158 (90,3 %) |
| **Bandeau humain (`banner_like`)** | **35 (20,0 %)** |
| **Recadrage « cible A2 » (`±8 pts`)** | **13 (7,4 %)** |
| Pas de bandeau humain | 140 (80,0 %) |

Le zoom humain est donc très fréquent (158/175, 90,3 %), mais il est
majoritairement sémantique : 35 projets (20,0 %) ont une échelle de bandeau,
et 13 (7,4 %) une échelle compatible avec la cible A2 du candidat. **80 % des
revues A2 sont donc justifiées** : le monteur n'a pas retiré de bandeau.
Parmi les 140 projets sans bandeau, 123 ont néanmoins zoomé pour des raisons
de reframe.

Détail par motif de revue (175 mesurables) :

| Motif | n | Bandeau humain | Cible A2 ±8 (banner) | Zoom humain |
| --- | ---: | ---: | ---: | ---: |
| Logo étroit | 50 | 1 (2,0 %) | 0 | 43 |
| Décor bas (> 200 px) | 50 | 5 (10,0 %) | 0 | 46 |
| Bande haute | 28 | 4 (14,3 %) | 2 | 23 |
| Bande animée | 24 | 6 (25,0 %) | 4 | 23 |
| Sous-titres changeants | 7 | 5 (71,4 %) | 1 | 7 |
| Bande partielle (< 50 %) | 5 | 5 (100 %) | 1 | 5 |
| Autre | 6 | 5 (83,3 %) | 3 | 6 |
| Bande non persistante | 3 | 2 (66,7 %) | 0 | 3 |
| Bande sombre | 2 | 2 (100 %) | **2** | 2 |

Les deux **bandes sombres** sont le cas le plus net : le code calcule déjà
un `scale_target` (218,2 et 203,2) et le monteur a appliqué **220,4 et
205,8**, soit +2,2 et +2,6 points. Le veto « bande sombre » est le seul
blocage ; la cible est juste.

Les 13 recadrages « cible A2 » manqués sont listés en annexe A. Les motifs
sont : 2 bandes sombres, 4 bandes animées, 2 bandes hautes, 1 bande
partielle, 1 texte changeant (sous-titres), 2 preuves OCR ambiguës et 1 bande
très instable. Plusieurs sont des vetos volontaires (bande haute,
sous-titres, largeur < 50 %) ; les autres sont des seuils de géométrie ou de
persistance qui pourraient être réévalués.

#### 3.3.3 `noop`

59 projets `noop`, 56 mesurables, **2 avec bandeau humain brut (3,6 %)**.
Après retrait des projets dont l'échelle dominante est expliquée par un
`apply` A1 (15 segments A1 à 203 % pour `57 Camille`), il ne reste qu'**un
seul cas non expliqué** (`261 Camille`, 1,8 %) : le monteur zoome à 227,41
alors qu'aucun candidat A2 n'est retenu. A2 ne détecte donc quasiment aucun
bandeau manqué sur les `noop` ; c'est cohérent avec un module conservateur.

### 3.4 Revues A1–A5 : taux d'action humain

| Étage | Revues (appariées) | Action humaine | Taux | Lecture |
| --- | ---: | ---: | ---: | --- |
| A1 | 183 | 37 | **20,2 %** | 3 seulement proches de la cible A1 |
| A2 | 175 | 35 | **20,0 %** | 13 proches de la cible A2 |
| A3 | 667 | 450 (dont 211 clés) | **67,5 %** (31,6 % clés) | enrichissement 2,6× vs base 12,1 % |
| A4 | 67 | 4 | **6,0 %** | revues peu suivies |
| A5 | 111 | 111 | **100 %** | report-only, résolu par le zoom |

A3 mérite une lecture prudente : 67,5 % des segments revus ont un
changement de transformation (position ou scale), mais **31,6 % seulement**
ont des clés ajoutées. Le taux de base de keyframes dans le GOOD est de
12,1 % (1 132/9 359) ; les segments A3 sont donc **2,6× plus souvent
keyframés** que la moyenne. Cela confirme la valeur de A3 comme filet de
revue, sans prouver que le monteur suive la recommandation de clés : D-056
mesurait 15,1 % de précision stricte (36,6 % en tolérant), ce qui reste
l'estimation de référence.

A4 est très peu suivi (4/67, 6,0 %) : les segments courts signalés ne sont
pas fusionnés par le monteur dans ce corpus. A5 est report-only par
contrat ; les 111 revues sont toutes résolues par le recadrage humain et ne
doivent pas être comptées comme des faux positifs.

### 3.5 Les 7 faux applies A1

| Projet | Segment | A1 | GOOD | Barres (h/b/g/d) | Cause |
| --- | --- | ---: | ---: | --- | --- |
| 134 Camille — Greg veut s'expliquer | 24 | 259,1 | 180 | 218/110/64/0 | barre latérale + scène nocturne |
| 134 Camille — Greg veut s'expliquer | 31 | 225,6 | 180 | 96/120/164/412 | barres latérales |
| 134 Camille — Maeva et Océane embrassent | 55 | 238,8 | 180 | 142/122/162/606 | barres latérales |
| 134 Camille — Maeva et Océane s'ouvrent | 38 | 221,5 | 180 | 82/118/162/366 | barres latérales |
| 136 Camille — Maeva est toujours vexée | 44 | 206,4 | 180 | 84/52/164/428 | barres latérales |
| 182 Camille — Paga et Benjy | 26 | 225,6 | 180 | 96/120/166/458 | barres latérales |
| 210 Camille — Extrait 2 | 16 | 187,4 | 180 | 18/22/120/24 | barres latérales |

Les 7 cas (1,84 % des `apply` appariés, 7/382 décisions) sont tous des
sources **pillarboxées** : une ou deux barres latérales sont présentes, mais
le code ne teste que les barres verticales (`vertical`) avant d'appliquer.
La variable `horizontal` est calculée dans `a1.rs` puis **jamais utilisée
comme veto**. Le cropdetect continue de trouver des barres haut/bas (parfois
la scène sombre aide), et A1 zoome sur la hauteur sans vérifier que la
largeur active couvre la composition.

Sur les 380 `apply` appariés, 34 ont une barre latérale ≥ 8 px : les 7 faux
applies et 27 applies conservés. Le tableau ci-dessous mesure le compromis
d'un veto `gauche ≥ seuil ou droite ≥ seuil → review` avant l'`apply` :

| Seuil latéral | Apply avec barre | Faux applies couverts | Bons applies rétrogradés | Dont à ±2 pts |
| ---: | ---: | ---: | ---: | ---: |
| ≥ 8 px | 34 | **7/7** | 27 | 12 |
| ≥ 32 px | 26 | **7/7** | 19 | 12 |
| ≥ 64 px | 25 | **7/7** | 18 | 12 |
| ≥ 100 px | 20 | 6/7 | 14 | 8 |

Un seuil de **32 px** (3 % de la hauteur 1080) couvre les 7 faux applies et
ne rétrograde que 19 bons applies, dont 12 sont à ±2 points du monteur
(principalement les 243,6 % de `13 Camille`). Le coût est donc de 19 revues
supplémentaires, sans perte de zoom : le monteur confirme ou ajuste. Le
bénéfice est la suppression des 7 seuls zooms destructifs du corpus. Un
seuil de 100 px laisserait passer le cas `134 Camille — Segment 24`
(max 64 px) et ne couvrirait que 6/7 faux applies. Le seuil 32 est le
compromis retenu : il couvre tous les cas sans descendre dans le bruit
d'encodage (8–16 px).

### 3.6 Synthèse par étage

| Étage | Décisions | Risque principal | Preuve |
| --- | ---: | --- | --- |
| A1 | 382 `apply` (380 appariés) | 7 faux applies pillarbox (1,84 %) | GOOD 180 sur 7 segments, frames extraites |
| A2 | 9 `apply` (287 segments) | aucun faux apply ; 13 bandeaux manqués sur 175 revues | GOOD 100 % conservé, scale ≤ humaine |
| A3 | 667 revues | bruit de revue ; 31,6 % de clés ajoutées | taux de base 12,1 % |
| A4 | 67 revues | 6 % d'action ; peu utile | aucune fusion humaine |
| A5 | 111 revues | report-only, pas un risque | 111/111 résolues |
| A6 | 0 projet | non déclenché sur ce corpus | aucun template SDM |

Aucun projet A6 n'est présent dans ce corpus : les 248 vidéos sont des
extraits ordinaires, pas des templates SDM/LPHDF.

## 4. Phase 3 — Shadow evaluation `a2_logo_prior` et `a2_logo_detector`

### 4.1 Protocole

Les 248 projets du miroir ont été rejoués une seule fois avec les deux flags
actifs :

- `a2_logo_prior = true`, catalogue `qa/source_logos.toml` (D-052) ;
- `a2_logo_detector = true`, YOLOX-Nano B2.1 `qa/logo_yolox_nano_a2.onnx`,
  CUDA, 416 px, confiance 0,30, NMS 0,65, persistance 0,70, ROI `full` ;
- `direct = false`, `a2_ocr = true`, même runtime OCR que la production ;
- `post run --report-only`, donc aucun `.ecproj` assisté publié.

Le baseline est le `post.json` du corpus, pas un second run : la comparaison
des quatre projets de `102 Camille` avec leurs `post.json` d'origine donne des
décisions identiques (action, cible, raison, confiance, détails hors
`logo_detector`/`source_logo`), seul `ocr.timing_ms` varie. Le run shadow ne
modifie que `mirror/<dossier>/docs/post/<projet>_assist/` ; le corpus reste en
lecture seule.

Le run a produit 248/248 variantes en 1 h 52 min. Les audits
sont `245 pass`, `3 warn`, `0 fail` ;
les 3 `warn` sont les `source_coverage` déjà connus (projets renommés).
Les variantes sont comparées au baseline après retrait des annotations
`source_logo` et `logo_detector` : la signature est le couple
`(action, scale_target)` et, à action constante, la liste des `reason`.

### 4.2 Effet sur les décisions A2

| A2 projet | Baseline | Variante |
| --- | ---: | ---: |
| `apply` | 9 | 10 |
| `review` | 180 | 181 |
| `noop` | 59 | 57 |

- **`apply` créés / perdus** : 1 / 0.
- **Échelles modifiées** : 1.
- **3 changements d'action** (détail ci-dessous).
- **15 changements de motif** à action constante :
  15 revues réparties sur 11 dossiers (`11 Camille` ×4, `141 Camille` ×2, `174
  Camille`, `210 Camille`, `228 Camille`, `230 Camille`, `27 Camille`, `31
  Camille`, `49 Camille`, `71 Camille`, `73 Camille`) : motif « texte
  persistant / bande animée » remplacé par « texte du logo source uniquement
  » (aucune action modifiée).

Trois changements d'action ont été observés :

- **`257 Camille — 66M VENTES` : `review` → `apply` 218,2 %.** Le baseline
  laissait A2 en revue parce que l'OCR dominant était le texte du logo
  « 66 » (couverture 11 %) et la géométrie instable. Le prior retire les
  textes du logo `66`/`MINUTES`, l'OCR dominant devient « LES NOUVEAUX
  MAGASINS ANTI-CRISE » (couverture 83 %, dominance 0,917), la bannière est
  confirmée et A2 applique 218,2 %. Le monteur a laissé une échelle dominante
  de 215,3 % : l'écart est de +2,9 points, dans la tolérance ±8 de la cible
  A2. C'est le seul `apply` créé par les flags. Le
  détecteur étant annotation seule, ce gain est imputable au prior.
- **`108 Camille — Le chef Etchebest…` et `243 Camille — Ce restaurateur…` :
  `noop` → `review`.** Le prior reconnaît le logo « Cauchemar en cuisine » et
  ne laisse aucun texte de bannière ; A2 renvoie une revue « logo source
  uniquement ». Le monteur a laissé 180 % : ce sont deux **fausses revues**,
  pas des faux applies. Elles disent d'ailleurs « jamais recadré ».

Les 15 changements de motif à action constante concernent des revues déjà
présentes, reformulées « texte du logo source uniquement » ou « pas une bannière »
(principalement les projets `M6 - ADP`, plus `49 Camille — COLOC MAISON`).
Ils sont exacts sur le fond — le logo *L'amour est dans le pré* est bien
persistant à 12/12 — mais ne changent ni l'action ni la cible.

### 4.3 `detector_any` n'est pas `detector_candidate`

Le détecteur publie un logo dans les zones génériques bas-gauche ou haut-gauche
via le repli `in_a2_roi` de `post/logo.rs`. Sur un bandeau haut, un logo
bas-gauche est conservé et `detections_a2` peut être non nul : `detector_any`
mesure donc « un logo persistant existe quelque part », pas « le candidat A2
est un logo ». La shadow evaluation distingue :

- `detector_any` : détection, persistance ≥ 0,70, ni `wide` ni
  `merged_with_lower_third` ;
- `detector_candidate` : en plus, candidat **étroit** (largeur ≤ 0,35,
  hauteur ≤ 220 px) et logo majoritairement **dans** le candidat
  (`logo_inside ≥ 0,5`). Seul ce second critère peut qualifier une revue.

| Indicateur | Valeur |
| --- | ---: |
| Actions de revue A2 dans la variante | 199 |
| `prior_ok` | 7 |
| `detector_any` | 158 |
| `detector_candidate` | 41 |
| Revues confirmées (`prior_ok` ou `detector_candidate`) | 41 |
| dont confirmées « étroites » | 41 |
| dont `prior` + `detector` | 7 |
| dont `detector` seul | 34 |
| dont `prior` seul | 0 |
| `detector_any` sans candidat ni prior | 117 |
| Revues confirmées avec GOOD à 180 | 39 |
| Revues confirmées sur un projet `banner_like` | 0 |

La lecture est nette : 158 actions de revue sur 199 ont un logo
persistant quelque part, mais 41 seulement le placent dans un candidat
étroit. Les 117 autres ne qualifient pas la bande A2 (logo
générique, candidat large ou boîte hors candidat) ; les utiliser fermerait des
revues à tort.

Le plafond est structurel : sur les 198 actions de revue
du baseline, 46 (23,2 %) sont étroites. Les
38 actions des 35 projets `banner_like`
sont **toutes larges** (largeur > 0,35 ou hauteur > 220) : aucune ne peut être
confirmée par la règle de confirmation. L'une d'elles (`257 Camille — 66M
VENTES`) est récupérée par le prior sous forme d'`apply` (voir 4.2) ; les
autres restent en revue. Le résultat de l'hypothèse
(0 faux noop) n'est donc pas une preuve que le détecteur reconnaît
un bandeau ; c'est la conséquence mécanique du seuil d'étroitesse. La règle ne
peut pas, par construction, fermer une revue de bandeau — et c'est précisément
la population qui compte.

Cas d'école : `105 Camille — Gulli - Ma Super Ligue 1`. Les deux revues du
projet ont `detector_any = true` avec une persistance de 1,0, mais
`detector_candidate = false` (largeur 0,84 et 0,77, hauteur 192 et 32 px). Le
monteur a zoomé à 223,9 %, dans la plage `banner_like`. Si un consommateur
utilisait `detector_any` pour résoudre la revue en `noop`, il fermerait un vrai
bandeau.

À l'inverse, 39 des 41 revues confirmées ont un GOOD à 180. 1 à 269,1 % a un
zoom humain ; 1 n'est pas mesurable (projet au GOOD renommé). Ces revues
prouvent l'**absence de bandeau**, pas la présence d'un recadrage à faire.

Le prior récupère
`257 Camille — 66M VENTES` (bannière 66 Minutes à 53 % de largeur) par exclusion
OCR, pas par la règle de confirmation. Les 12 autres cibles A2 manquées
(annexe A) restent hors de portée : ce sont des bandes larges, hautes,
sombres, animées ou des sous-titres, dont aucune n'est un logo étroit.

Hypothèse de résolution automatique (toutes les revues confirmées → `noop`) :
40 projets corrects, 0 faux noop, 0 partiels.
À ces 40 noops corrects s'ajoute l'`apply` `257 Camille — 66M
VENTES`, également correct ; il n'est pas compté dans l'hypothèse `noop`
puisqu'il change d'action. Le chiffre de faux noop doit être lu avec la
réserve ci-dessus : sa valeur (0) s'explique parce que les bandeaux
sont hors du domaine de confirmation, pas parce que le détecteur les rejette.

### 4.4 Coût

Le détecteur est le seul coût significatif. Sur 248 projets mesurés :

| Indicateur | Valeur |
| --- | ---: |
| Médiane | 1 858,5 ms |
| Moyenne | 2 313,0 ms |
| p95 | 4 727,1 ms |
| Maximum | 5 338,4 ms |
| Projets avec cache OCR (< 1 s) | 124 |
| Projets avec décodage complet (≥ 1 s) | 124 |
| Total sur le run | 573,6 s |

Le cache mono-projet de `vision/frame.rs` n'est exercé que lorsque le gate OCR
A2 échantillonne les mêmes 12 frames : dans ce cas le détecteur coûte
~250–270 ms. Sans candidat OCR, il décode lui-même 12 frames et coûte
4,9–5,3 s. Le corpus se partage exactement en deux : 124 projets sous
1 s et 124 en décodage complet, ce qui explique une médiane de
1 858,5 ms et une moyenne de 2 313,0 ms/projet (573,6 s
sur le run). Le prior est négligeable en CPU, mais il modifie le regroupement
OCR et donc les motifs.

### 4.5 Lecture

1. **Le détecteur est strictement additif** : `post/logo.rs` insère
   `details.logo_detector` après la décision A2 ; aucune action, raison,
   confiance ou `scale_target` n'en dépend. Sur ce corpus, 0 changement
   d'action lui est imputable.
2. **Le prior n'est pas strictement additif** : il retire le texte du logo du
   groupe OCR dominant, remplace le veto « logo seul » par la géométrie quand
   un overlay déclaré cache le logo, et peut donc changer un motif ou une
   action. Sur ce corpus, il crée 1 `apply` correct
   (`257 Camille — 66M VENTES`, 218,2 % contre 215,3 % humain) et deux
   fausses revues (`108`, `243`), pour 15 reformulations de motif.
3. **La confirmation est étroite** : 41/199 actions de
   revue seulement, toutes des candidats étroits, majoritairement déjà classés
   « logo étroit » par A2. Le détecteur confirme l'absence de bandeau sur ces
   revues ; il n'ouvre pas les 38 actions de bandeau, dont 37 restent en
   revue.
4. **Le bénéfice du prior est réel mais limité** : il récupère 1 des 13
   cibles A2 (`257`) et confirme 7 revues ; le coût est négligeable. Le
   détecteur apporte 34 confirmations supplémentaires, mais aucune décision,
   pour 2 313,0 ms/projet en moyenne (jusqu'à 5,3 s sans cache).
5. **Les 13 cibles A2 manquées du baseline** : 1 est récupérée par le prior
   (`257`), 12 restent hors de portée. Aucune des 12 n'est un logo étroit :
   ce sont des bandes larges, hautes, sombres, animées ou des sous-titres. Le
   levier correct pour les 12 restantes reste la révision des vetos A2
   (bandes sombres, largeur 35–50 %, bandes hautes), pas le logo.

## 5. Décision D-062 (ADR)

### 5.1 Contexte

- D-052 a livré le catalogue de logos source et le flag `a2_logo_prior`,
  désactivé en production depuis D-055.
- D-060 a écarté Grounding DINO ; B1/B2/B2.1 ont livré YOLOX-Nano 416 en
  annotation seule, `a2_logo_detector = false`, avec un pilote de 13 projets.
- La présente étude porte sur 248 projets finalisés, 9 359 segments appariés.
  A2 est sûr (9 `apply`, 287 segments, 0 revert, 0 sur-zoom) mais rate 13
  recadrages de bandeau sur 175 revues mesurables (7,4 %) ; 35 revues (20 %)
  cachent un bandeau humain.
- La shadow evaluation montre que le prior récupère **1 des 13 cibles**
  (`257 Camille — 66M VENTES`, `apply` 218,2 % contre 215,3 % humain), avec
  0 faux apply et 2 fausses revues. Le détecteur ne change aucune action.
- Question : activer `a2_logo_prior` et/ou `a2_logo_detector` en production ?

### 5.2 Décision D-062

**`a2_logo_prior = true` en production, en pilote contrôlé ;
`a2_logo_detector = false`.** La décision D-062 (2026-11-11) applique ce
choix : `config/config.toml` active le flag, aucun code de production n'est
modifié.

Le prior est le seul flag qui change une décision : il transforme
`257` en `apply` correct (218,2 % contre 215,3 %) et ajoute deux revues
« logo source uniquement » (`108`, `243`). Le détecteur est annotation seule :
il confirme 41 revues, mais ne change aucune action, pour
2 313,0 ms/projet en moyenne. Il reste un outil de QA en
`--report-only`, pas un décideur de production.

Si un consommateur exploite un jour l'annotation du détecteur, il ne peut
utiliser que `detector_candidate` (candidat étroit + logo majoritairement
dedans), jamais `detector_any`. Et il doit traiter `detector_candidate` comme
une confirmation d'**absence de bandeau**, pas comme une preuve de recadrage.

### 5.3 Justification

1. **Le prior apporte un gain réel mais limité.** Il crée
   1 `apply` correct (`257`, +2,9 points du monteur, dans
   la tolérance ±8), sans aucun apply perdu ni faux apply. Il ajoute deux
   fausses revues (`108`, `243`) et 15 reformulations de motif. Le solde de
   revues est +1 action (198 → 199) : le gain de rappel A2 est réel, le coût
   de revue reste contenu. Il récupère 1 des 13 cibles ; les 12 autres
   restent hors de portée.
2. **Le détecteur n'apporte aucun gain de décision.** 0 changement d'action,
   0 apply, 0 revue créée. Ses 41 confirmations (dont
   34 sans le prior) annotent des revues déjà étroites ; elles ne
   peuvent pas fermer une revue sans une règle aval non implémentée. Pour
   2 313,0 ms/projet, le détecteur ne justifie pas une activation de
   production.
3. **La confirmation du détecteur est étroite et tautologique.**
   41/199 actions de revue seulement, toutes des candidats
   étroits ; les 38 actions de bandeau sont larges et 37
   restent en revue. Le faible taux de faux noop (0) est la
   conséquence mécanique du seuil d'étroitesse, pas une preuve que le
   détecteur reconnaît un bandeau. Il ne peut pas récupérer les 12 cibles
   restantes.
4. **Le risque du prior est mesuré et pilotable.** Son effet OCR peut créer
   un `apply` ou une revue ; sur 248 projets, il crée 1 apply correct et 2
   fausses revues, aucun faux apply. Ces deux revues disent « jamais
   recadré » et se ferment sans action. Un pilote avec surveillance des
   `apply` A2 et des revues permet de confirmer ce comportement avant
   généralisation.
5. **Le coût du prior est négligeable.** Le catalogue et la projection de
   bbox sont locaux ; le surcoût est dominé par l'OCR déjà présent. Le
   détecteur, lui, coûte 2 313,0 ms/projet en moyenne, jusqu'à 5,3 s sans
   cache OCR, pour une annotation.
6. **Le risque d'usage du détecteur est réel.** `detector_any` est vrai sur
   158 des 199 actions de revue, y compris le bandeau
   `105 Camille` à persistance 1,0. Un consommateur pressé qui confond `any` et `candidate`
   fermerait de vrais bandeaux. Le garder `false` supprime ce risque au
   niveau production.

### 5.4 Conséquences

- `config/config.toml` est passé à `a2_logo_prior = true` (pilote, D-062) ;
  `a2_logo_detector` reste `false` (défaut du code).
- D-052 devient une brique active en pilote ; B2.1 reste un pilote
  `report_only`.
- Les runs de QA peuvent activer le détecteur sur des copies, avec
  `--report-only`, pour produire `details.logo_detector`.
- Le pilote doit surveiller les `apply` A2 (dont `257`) et les revues
  « logo source uniquement » ; un faux apply ou une dérive de revue déclenche
  le retour à `false`.
- Les 12 cibles A2 restantes ne sont pas résolues par cette activation : elles
  relèvent des vetos A2 (bandes sombres, largeur 35–50 %, bandes hautes).

### 5.5 Pilotage et conditions de réouverture

- **Prior** : activé en pilote (D-062) ; mesurer sur un lot réel les `apply`
  créés, les revues ajoutées et les faux applies. Retour à `false` si un faux
  apply apparaît ou si la charge de revue dépasse le gain. Le corpus 248
  fournit la référence : 1 apply correct, 2 fausses revues, 0 faux apply.
- **Détecteur** : ne le réouvrir comme décideur que si une règle aval utilise
  la confirmation pour fermer des revues, avec 0 faux noop sur un corpus
  indépendant contenant des **bandeaux étroits** (≤ 0,35 de largeur,
  ≤ 220 px) et au moins 20 clips négatifs ; et si son coût descend à
  ≤ 0,5 s/projet avec cache, ou nul quand l'OCR ne tourne pas.
- **A1** : le veto barres latérales ≥ 32 px reste une décision séparée
  (voir 5.6).

### 5.6 Décision séparée : le veto A1 barres latérales

Le seul défaut destructif mesuré n'est pas dans A2 mais dans A1 : 7 faux
applies sur 380 (1,84 %), tous des sources pillarboxées. `a1.rs` calcule
`horizontal` mais ne l'utilise pas comme veto. Un veto
`gauche ≥ 32 px ou droite ≥ 32 px → review` couvre 7/7 faux applies et ne
rétrograde que 19 bons applies, dont 12 à ±2 points. Cette correction est
**hors périmètre** de la présente décision sur les flags A2 ; elle est
formalisée dans `docs/lots/LOT-11-A1-VETO-LATERAL.md` pour une passe de
durcissement séparée, avec validation visuelle et critères d'acceptation.

### 5.7 Statut

**Décision D-062 acceptée le 2026-11-11.** `config/config.toml` active
`a2_logo_prior` ; aucun changement de `crates/workflow-steps`, du runtime ou
de `/mnt/cache`. Le ticket A1 `LOT-11-A1-VETO-LATERAL.md` a été implémenté
et validé le 2026-11-11 sous D-063 : veto latéral ≥ 32 px normalisé,
7/7 faux applies couverts, 19 bons applies rétrogradés (12 à ±2 pts).

## 6. Reproductibilité

### 6.1 Scripts

| Script | Rôle |
| --- | --- |
| `benchmark_corpus3/scripts/prepare-mirror.py` | copie les 248 `.ecproj` bruts, lie vidéos/`_ae.json`/`.csv` par symlink, crée `docs/post` |
| `benchmark_corpus3/scripts/benchmark-corpus3.py index` | inventaire et intégrité des triplets |
| `... analyze` | Phase 2 : parse les 744 projets, écrit `summary.json`, `segments.csv`, `projects.csv`, `reviews.csv`, `false_applies.json` |
| `... shadow` | Phase 3 : compare le run `prior + detector` au baseline et simule la résolution des revues |
| `report-shadow-metrics.py` | agrège `shadow.json` et les `post.json` du corpus en `shadow-report.json` |
| `fill-section45.py` | remplit les sections 4 et 5 à partir de `shadow-report.json` |
| `merge-section45.py` | fusionne les sections générées dans ce rapport |

Les scripts sont hors dépôt, sous
`/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/benchmark_corpus3/`.
Aucune écriture sous `/tmp`, dans le home ou dans le corpus.

### 6.2 Commandes

```sh
BENCH="/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/benchmark_corpus3"

python3 "$BENCH/scripts/prepare-mirror.py"
python3 "$BENCH/scripts/benchmark-corpus3.py" index
python3 "$BENCH/scripts/benchmark-corpus3.py" analyze

# Run shadow (rapport seul, aucune publication d'assist) :
export LD_LIBRARY_PATH="/mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/onnxruntime/capi:$(ls -d /mnt/venv_ext4/transnet_cv5_env/lib/python3.10/site-packages/nvidia/*/lib | paste -sd:):$LD_LIBRARY_PATH"
mapfile -t FOLDERS < <(python3 "$BENCH/scripts/prepare-mirror.py" --list-folders)
ARGS=(); for f in "${FOLDERS[@]}"; do ARGS+=(--folder "$f"); done
./target/release/workflow-cli --config "$BENCH/configs/both.toml" \
  post run "${ARGS[@]}" --report-only

python3 "$BENCH/scripts/benchmark-corpus3.py" shadow
python3 "$BENCH/scripts/report-shadow-metrics.py"
python3 "$BENCH/scripts/fill-section45.py"
python3 "$BENCH/scripts/merge-section45.py"
```

### 6.3 Configuration shadow

Le fichier `benchmark_corpus3/configs/both.toml` est une copie de
`config/config.toml` avec :

- `direct = false` (évite tout bypass) ;
- `a2_logo_prior = true` ;
- `a2_logo_detector = true`, modèle `qa/logo_yolox_nano_a2.onnx`, device
  `cuda`, taille 416, conf 0,30, NMS 0,65, persistance 0,70, ROI `full` ;
- `a2_ocr_onnxruntime` forcé sur le runtime CUDA du venv TransNet, pour que
  baseline et variante utilisent la même bibliothèque.

Le run baseline n'a pas été rejoué sur les 248 projets : la comparaison des
4 projets de `102 Camille` avec leurs `post.json` d'origine donne des
décisions identiques (code, action, cible, raison, confiance, détails hors
`logo_detector`/`source_logo`), seul `ocr.timing_ms` varie. Les `post.json`
du corpus servent donc de baseline, et le run shadow est comparé à ces
fichiers après retrait des annotations.

### 6.4 Sorties

| Fichier | Contenu |
| --- | --- |
| `out/index.json` | inventaire |
| `out/summary.json` | toutes les métriques Phase 2 |
| `out/segments.csv` | 9 359 lignes, une par segment apparié |
| `out/projects.csv` | 248 lignes, une par projet |
| `out/reviews.csv` | 1 028 lignes, une par revue A1/A2/A3/A4/A5 |
| `out/false_applies.json` | 7 cas A1 avec barres et cause |
| `out/shadow.json` | Phase 3 brute |
| `out/shadow-report.json` | Phase 3 agrégée (décisions, confirmations, coûts, audits) |
| `out/section4-5.md` | sections 4 et 5 générées |
| `out/frames/*.png` | frames de contrôle des faux applies |

## Annexe A — Les 13 recadrages A2 manqués (cible ±8 pts)

`cible` est la scale A2 théorique `1920 / (1080 − h) × 100 × 1,05` ;
`humain` est l'échelle dominante du GOOD ; `Δ` = humain − cible.

| Projet | Motif de la revue | h (px) | Largeur | Cible | Humain | Δ |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 49 Camille — COLOC ALEXANDRE | bande basse partielle animée | 156 | 0,43 | 218,2 | 217,4 | −0,8 |
| 221 Camille — chute de 4 mètres | bande haute 108 px | 108 | 0,49 | 207,4 | 209,2 | +1,8 |
| 256 Camille — CHAMBRE BVLGARI | **bande basse sombre** (médiane 52) | 156 | 0,66 | 218,2 | 220,4 | +2,2 |
| 26 Camille — officialiser sa relation | texte changeant (58 %) | 172 | 0,81 | 222,0 | 224,5 | +2,5 |
| 221 Camille — python à tête noire | **bande basse sombre** (médiane 40) | 88 | 0,46 | 203,2 | 205,8 | +2,6 |
| 257 Camille — 66M VENTES | bande basse animée | 156 | 0,53 | 218,2 | 215,3 | −2,9 |
| 105 Camille — Super Ligue 1 | bande haute 192 px | 192 | 0,84 | 227,0 | 223,9 | −3,1 |
| 49 Camille — COLOC MAISON | largeur 43 % < 50 % | 156 | 0,43 | 218,2 | 222,3 | +4,2 |
| 41 Camille — DRAGUE PLAGE | bande basse animée | 164 | 0,91 | 220,1 | 225,5 | +5,4 |
| 175 Camille — RETRAITES_TAT_M6 | OCR ambigu | 88 | 0,68 | 203,2 | 209,3 | +6,0 |
| 36 Camille — SALADES REFUSEES | OCR ambigu | 116 | 0,89 | 209,1 | 215,6 | +6,4 |
| 236 Camille — EXTRAIT POSTDIFF 2 | bande basse animée | 104 | 0,59 | 206,6 | 213,0 | +6,5 |
| 16 Camille — UJUD APPLIS PROMOS | bande basse très instable | 152 | 0,61 | 217,2 | 210,0 | −7,2 |

Les deux bandes sombres sont les cas les plus exploitables : la cible est
déjà dans `details.scale_target` (218,2 et 203,2) et le monteur a appliqué
220,4 et 205,8. Les bandes hautes et les sous-titres restent des vetos
volontaires : les compter comme « manqués » serait une erreur de lecture.
En shadow, `a2_logo_prior` récupère `257 Camille` (`apply` à 218,2 %) ; les
12 autres restent manquées et relèvent des vetos A2, pas du logo.

Une 14e revue, `112 Camille — 66M - SOSIE CELINE`, a une échelle dominante à
2,3 points de sa cible (215,9 contre 218,2) mais n'est pas `banner_like` : la
valeur dominante ne couvre que 6 % des segments, le zoom humain est mixte
(180–280 %). Elle n'est donc pas comptée comme un bandeau manqué.

## Annexe B — Glossaire

- **GOOD** : projet finalisé et validé par le monteur, vérité terrain.
- **Assist** : projet produit par la post-STEP8, sans intervention humaine.
- **Brut** : projet STEP8 avant post-traitement, octet pour octet inchangé.
- **`banner_like`** : échelle dominante GOOD 190–230, couverture ≥ 50 %.
- **`punch-in`** : échelle dominante GOOD > 230, reframe sémantique.
- **TP/FP/FN/TN** : zoom automatique × zoom humain, matrice 2×2.
