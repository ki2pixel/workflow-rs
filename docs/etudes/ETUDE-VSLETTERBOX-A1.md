# Étude d'opportunité — détecteur de letterbox `vapoursynth-letterbox` pour A1

| Champ | Valeur |
| --- | --- |
| Type | Étude d'opportunité et spike read-only |
| Statut | Référence (preuves dans `ETUDE-VSLETTERBOX-A1/`) |
| Date | 2026-11-08 |
| Portée | A1 du post-STEP8, `crates/workflow-steps/src/post/{a1,detect}.rs` |
| Dépôt étudié | `Akatmks/vapoursynth-letterbox`, commit `ac274fa` (2026-08-13), licence MIT |
| Décision | **D-057** : adapter le principe, pas intégrer le plugin ; spike natif opt-in |
| Voir aussi | D-056, `docs/lots/LOT-9-POST-STEP8.md`, `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md` |

---

## TL;DR

**Verdict : adapter, pas intégrer tel quel.** Le dépôt apporte deux idées
transposables — un détecteur différentiel EWMA à 5σ sur la luminance moyenne
des lignes, et une validation de la ligne frontière par densité de contours —
mais ce n'est pas un détecteur de letterbox décisionnel : c'est un filtre
VapourSynth **par frame**, orienté nettoyage, sans agrégation temporelle, sans
contrôle de symétrie et sans notion de crop.

Ce qui mérite un spike natif Rust :

1. l'EWMA de Tony Finch (`alpha = 0,05`) avec le seuil
   `μ + 5 × max(σ, 0,005)` : il rejette les gradients lents et les ciels
   nocturnes uniformes là où `cropdetect` fabrique une fausse barre ;
2. le scan ligne à ligne à 1 px près : sur le 305, il mesure 81/78 px de
   barres contre 86/82 px pour `cropdetect=24:16:0`, soit 9 px de hauteur
   active récupérés ;
3. l'idée de valider la frontière par un contour horizontal, à condition de
   la porter sur un opérateur **ciblé** (Sobel Y sur quelques lignes
   candidates) et non sur un ExKirsch 5×5 plein cadre.

Ce qui ne se transpose pas :

- le wrapper Python/VapourSynth, `vs-jetpack`, `vsmasktools` et le plugin C++ ;
- `clean_letterbox`, le deringing, `Morpho.minimum`, `fullblack_thr` et
  `_VSLETTERBOX_AREA_MULTIPLIER` : ce sont des garde-fous de **nettoyage**,
  pas de décision de recadrage ;
- le magnétisme `permanent` : il ne lisse pas le jitter temporel, il impose
  une barre **fournie par l'utilisateur** dès que la détection est au-dessus
  d'elle (tolérance 1 px vers le bas) ;
- l'ExKirsch 5×5 plein cadre : 8 orientations × 25 taps par pixel, coûteux et
  trop général pour une frontière qui est, par définition, horizontale.

**Recommandation** : conserver `cropdetect` comme défaut, ajouter un détecteur
natif **opt-in** (`a1_detector = "native"`), réutiliser l'EWMA, un Sobel
horizontal ciblé et un **clustering temporel** des frontières, puis valider sur
les fixtures 304/305/356 et un échantillon des 65 `apply` / 116 `review` A1
avant toute promotion. Aucun code de production n'est modifié par cette étude.

Le gain est **ciblé**, pas global : le détecteur ne corrige pas les 8 `apply`
non résolus de l'étude GOOD (keyframes/reframe, plans larges intentionnels) et
ne garantit pas les 7 revues asymétriques résolues. Il améliore la géométrie
des letterbox stables et réduit le bruit de revue ; A3 reste le levier des
recadrages dynamiques.

---

## 1. Méthode, périmètre et limites

### 1.1 Corpus et dépôt

- Dépôt : `https://github.com/Akatmks/vapoursynth-letterbox`, commit
  `ac274faf0804fa3c23265f107ae90c63ec926f2c`, 13 août 2026.
- Fichiers analysés : `vsletterboxc/vsletterbox.cpp` (SHA-256
  `1513a64f…d161`) et `vsletterbox/vsletterbox.py` (SHA-256
  `7653c81b…8035`).
- Dépendance de masque : `vsmasktools.ExKirsch` (dépôt
  `Jaded-Encoding-Thaumaturgy/vs-masktools`, commit `5a36970`), noyaux 5×5
  à 8 orientations.
- Corpus : `Corpus_Exemple_2`, projets `304 Camille`, `305 Camille` et
  `356 Camille`, lus en **lecture seule**. Les mesures A1 proviennent des
  `docs/post/<stem>_9x16_assist/post.json` existants.

### 1.2 Prototype et reproductibilité

Le prototype est un script Python d'étude, **hors production**, dans
[`ETUDE-VSLETTERBOX-A1/scripts/`](ETUDE-VSLETTERBOX-A1/scripts/) :

- extraction de frames `yuv444p` full range par `ffmpeg` ;
- signal `pre = Y + 0,25·|U−128| + 0,25·|V−128|`, comme le wrapper Python ;
- ExKirsch 5×5 (8 orientations, maximum, diviseur 1, clamp 8 bits) ;
- EWMA `alpha = 0,05`, seuil `μ + 5 × max(σ, 0,005)` ;
- validation de la ligne par moyenne du masque de contours `> 2/3` ;
- comparaison avec `cropdetect=24:16:0` (A1 actuel) et `24:2:0`
  (référence à 2 px).

Les résultats sont dans
[`ETUDE-VSLETTERBOX-A1/results/`](ETUDE-VSLETTERBOX-A1/results/). Les
commandes sont en § 8.

### 1.3 Limites

- Le prototype **n'est pas** une exécution VapourSynth : le clamp 8 bits de
  l'ExKirsch, la conversion de range et le redimensionnement chroma sont
  approximés. Les conclusions qualitatives sont robustes, les valeurs à
  ±1 px près ne sont pas contractuelles.
- L'échantillonnage est de 1 à 2 fps sur des segments de 1,3 à 4,4 s ; il ne
  reproduit pas exactement les 25 fps de `cropdetect`. C'est volontaire : A1
  n'a pas besoin de toutes les frames, mais le clustering temporel doit être
  calibré sur un nombre de frames suffisant.
- Les tests synthétiques sont des images non compressées : ils isolent le
  critère, ils ne mesurent pas l'effet du bruit H.264.
- L'étude ne réexécute pas A1 sur les 2 609 segments ; elle ne prouve pas une
  amélioration globale. C'est le rôle du spike de § 6.

---

## 2. Anatomie de `vapoursynth-letterbox`

### 2.1 Le plugin C++ `Find` : un détecteur par frame

Le plugin `vsletterbox.Find` reçoit deux clips de même géométrie : `clip`
(l'image) et `ref` (le masque de contours). Il parcourt les lignes depuis le
haut puis depuis le bas, et publie deux propriétés :
`VSLETTERBOX_TOP_ROW` (première ligne de l'image) et
`VSLETTERBOX_BOTTOM_ROW` (dernière ligne de l'image), toutes deux inclusives.

Le cœur statistique est `ExponentiallyWeightedStats`, d'après Tony Finch
(2009) :

```cpp
const double diff = data - _mean;
const double incr = alpha * diff;
_mean += incr;
_var = one_minus_alpha * (_var + diff * incr);
```

avec `alpha = 0,05`, `one_minus_alpha = 0,95`. La première ligne initialise
`mean = data` et `var = 0` ; les suivantes mettent à jour moyenne et variance.
Le seuil de rupture est :

```cpp
src_mean > *st_mean + 5 * std::max(*st_stddev, 0.005)
```

À 8 bits, le plancher `0,005` vaut `1,275` niveau, soit un écart minimal de
`5 × 0,005 × 255 = 6,375` niveaux, exactement la valeur documentée dans le
README. Le seuil est donc **adaptatif** : il monte avec le bruit de la barre
(`5σ`) et ne descend jamais sous 6,375 niveaux.

Quand une ligne déclenche le saut, le plugin calcule la moyenne du masque
`ref` sur cette ligne et ne valide la frontière que si elle dépasse
`ref_thr` (2/3 par défaut). Si la validation échoue, la ligne est **quand
même ajoutée aux statistiques** (le `stats.add_data` est après le `if`), ce
qui peut retarder ou masquer la vraie frontière suivante. Si aucune ligne ne
déclenche, `start_y` revient à 0 et `end_y` à `height − 1` : le plugin ne
distingue pas « pas de barre » de « détection manquée ».

Deux propriétés importantes pour A1 :

- **aucun état temporel** : chaque frame est indépendante ; il n'y a ni
  médiane, ni persistance, ni lissage entre frames ;
- **aucun magnétisme générique** : le C++ ne recolle pas les lignes. Le
  snap est fait dans le wrapper Python, uniquement vers les barres
  `permanent` fournies par l'utilisateur : toute détection au-dessus de la
  barre (tolérance 1 px vers le bas) est remplacée par la barre permanente.

### 2.2 La chaîne Python : prétraitement, masque, nettoyage

`vsletterbox/vsletterbox.py` remplit trois rôles.

**Prétraitement `pre`.** Pour un clip 3 plans, il convertit en 4:4:4 full
range puis calcule :

```python
pre = norm_expr(split(pre),
    "x y neutral - abs 0.25 * z neutral - abs 0.25 * + + 0 max mask_max min")
```

soit `Y + 0,25·|U−128| + 0,25·|V−128|`, borné à `[0, mask_max]`. Le détecteur
ne travaille donc pas sur la luma seule : une zone colorée mais sombre
(bandeau teal, logo) est artificiellement remontée. C'est utile pour le
nettoyage, mais c'est aussi ce qui peut faire passer un bandeau bas pour une
frontière.

**Masque `ExKirsch`.** `dynamic_ref = ExKirsch().edgemask` applique huit
noyaux 5×5 (les huit orientations de Kirsch étendues), prend le maximum
pixel-wise, puis `norm_expr`/`calc_mean` normalisent la ligne par le peak.
Le seuil `dynamic_ref_thr = 2/3` exige donc qu'une **large majorité** de la
ligne réponde au contour. C'est le critère qui rejette un titre blanc isolé
sur fond noir : le texte couvre peu de colonnes, la moyenne de ligne reste
sous 2/3. C'est aussi sa limite : un aplat clair large (bandeau, carton
plein écran) peut dépasser 2/3 et être pris pour le bord de l'image.

**Magnétisme `permanent`.** `find_letterbox` reçoit `permanent = [top,
bottom]` et recolle la ligne détectée sur la barre permanente si elle est
au-dessus d'elle (tolérance 1 px vers le bas) :

```python
VSLETTERBOX_TOP_ROW =
  x.VSLETTERBOX_TOP_ROW {permanent_top_row} 1 + <=
    {permanent_top_row} x.VSLETTERBOX_TOP_ROW ?
```

Ce n'est donc **pas** un filtre anti-jitter temporel : c'est un prior
utilisateur pour les barres connues. A1 n'a pas d'équivalent ; il faudra le
remplacer par une persistance temporelle, pas par un snap arbitraire.

**Nettoyage.** `letterbox_mask` calcule
`_VSLETTERBOX_AREA_MULTIPLIER = aire_détectée / aire_permanente /
fullblack_thr`, borné à `[0,1]`, avec `fullblack_thr = 1/5`. Quand l'aire
active devient très petite (image presque entièrement noire), le masque est
réduit pour ne pas effacer le bruit d'un fondu au noir. `clean_letterbox`
ajoute `thr = 0,030`, `transition = 0,015`, `Morpho.minimum` et le deringing
`border_y/u/v`. Tout cela est **hors périmètre A1** : A1 ne nettoie pas les
pixels, il décide d'un crop et d'une `Position Y`.

### 2.3 Ce que le dépôt ne fait pas

Le dépôt ne fournit :

- ni agrégation par segment (médiane, mode, percentile, persistance) ;
- ni contrôle de symétrie haut/bas ou gauche/droite ;
- ni décision `apply`/`review`/`noop` ;
- ni couverture après scale et compensation ;
- ni distinction entre « pas de barre » et « détection manquée ».

C'est un **primitif de mesure par frame**, pas un étage de décision. Toute
intégration A1 doit ajouter la couche de décision existante, et surtout ne pas
remplacer le p10 actuel par un p10 de mesures natives sans filtre
d'outliers (voir § 3.1 et § 4.2).

---

## 3. Adéquation aux cas limites de workflow-rs

### 3.1 Cas 1 — 304 « Cette voiture », segment 9 : ciel nocturne

A1 actuel (`cropdetect=24:16:0`, p10 des hauteurs) :

| Grandeur | Valeur |
| --- | --- |
| Crop dominant | `w=1664, h=432, x=122, y=642`, ratio 0,061 |
| Barres | haut **642 px**, bas **6 px** |
| Hauteur active p10 | 432 px |
| Décision | `review` « barres asymétriques » |
| Vérité terrain | pas de letterbox ; le haut est le ciel nocturne, le bas un bandeau |

`cropdetect=24:2:0` ne corrige pas le problème : le crop le plus fréquent est
`1806:604:114:476` (haut 476) et le p10 vaut `444` (`1676:444:116:636`, barres
636/0). Le défaut n'est pas l'arrondi, c'est le **seuil de luminance** de
`cropdetect`.

Prototype vsletterbox, 5 frames à 2 fps sur `start + 0,05 s` :

| Frame | top | bottom | Barres (haut, bas) | Interprétation |
| ---: | ---: | ---: | ---: | --- |
| 43,25 | 0 | 1079 | 0, 0 | pas de barre |
| 43,75 | 0 | 1079 | 0, 0 | pas de barre |
| 44,25 | 935 | 1003 | 935, 76 | faux haut = bandeau bas |
| 44,75 | 936 | 1003 | 936, 76 | faux haut = bandeau bas |
| 45,25 | 0 | 1079 | 0, 0 | pas de barre |

À 1 fps, les trois frames donnent `top = 0` ; à 2 fps, deux frames sur cinq
produisent une fausse barre haute de 935 px. Le basculement est instructif :

- le faux `top = 642` de `cropdetect` est rejeté par l'**EWMA** : le ciel est
  un gradient lent (`Δpre ≈ 0,001–0,004` par ligne, contour moyen 0,07–0,4),
  donc aucun saut à 5σ ; le masque de contours n'est pas le critère
  discriminant ;
- le faux `top = 935` est le **bandeau bas** (`GENDARMES DE NORMANDIE…`,
  `DUELS SUR LE BITUME`, logo) : la ligne a un contour moyen de 0,76–0,83,
  au-dessus de 2/3, et un saut de 0,034–0,066. Le masque Kirsch **ne rejette
  pas** un aplat large dans une scène texturée.

Conclusion : le dépôt élimine le faux positif du ciel **lisse**, mais pas
tous les faux positifs de scène sombre. Sur ce segment, A1 reste correct
grâce à son veto d'asymétrie (`642/6` ou `935/76`), pas grâce au détecteur
seul. Un clustering temporel `(0,0)` majoritaire (3/5) donnerait `noop`, ce
qui est la bonne décision ; un p10 des hauteurs actives donnerait 68 px et
une fausse revue. **Il ne faut donc pas brancher le détecteur natif sur le
p10 actuel sans agrégation robuste.**

### 3.2 Cas 2 — 305 « Chauffeuse », segment 1 : précision sub-16 px

Le segment 1 est un letterbox stable. Mesures :

| Méthode | top | bottom | Hauteur active | Scale exact | Scale ×1,015 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `cropdetect=24:16:0` (A1) | 86 | 82 | 912 | 210,5 % | **213,7 %** |
| `cropdetect=24:2:0` | 82 | 80 | 918 | 209,2 % | 212,3 % |
| Prototype vsletterbox | **81** | **78** | **921** | **208,5 %** | **211,6 %** |
| Choix humain GOOD | — | — | ≈914 | **210,0 %** | — |

Les `post.json` du corpus ont été générés avant D-056, avec la marge 1,04 :
le 218,9 % du 305 est donc l'**ancien** A1. Les colonnes « ×1,015 » sont
recalculées avec la marge actuelle.

L'étude GOOD résume « barre réelle 81 px, fit exact 210,5 % » ; 210,5 %
correspond à 912 px actifs (1080 − 2×84), pas à 81 px par côté. La mesure
1 px du prototype (81/78, actif 921) donne 208,5 % exact. C'est cette mesure
qui est utilisée ici.

Le prototype mesure `top = 81`, à 1 px de la référence 2 px (`top = 82`), et
`bottom = 78`, à 2 px de cette référence (`bottom = 80`) ; il reste stable
sur les segments 1 et 9 (`top = 81`, `bottom = 78–79`). Il récupère **4 à
5 px par barre** par rapport à l'arrondi 16 px, soit 9 px de hauteur active et
environ **2,1 points de scale**. Le passage de la marge 1,04 à 1,015 (D-056) apporte
lui environ **5,2 points** (218,9 → 213,7). Les deux effets sont donc
complémentaires :

- 218,9 % (ancien A1) → 216,8 % (détecteur exact, marge 1,04) → 213,7 %
  (A1 actuel) → **211,6 %** (détecteur exact, marge 1,015) ;
- l'humain est à 210,0 %. Le détecteur exact + 1,015 reste à **+1,6 point**
  du choix humain ; c'est la marge de sécurité, pas une erreur de mesure.
  Une marge de 1,007 sur la hauteur exacte donnerait 210,0 %.

**Réponse à la question posée** : l'estimation exacte ne « remplace » pas à
elle seule les 218,9 % par 210,0 %. Elle apporte ~2,1 points ; la marge
D-056 apporte ~5,2 points ; le reste est la marge résiduelle. Le vrai gain du
détecteur est la **précision géométrique** (pas de sur-zoom de 4–5 px par
barre) et la suppression du biais de quantification 16 px, pas la politique
de marge.

Le segment 17 (32,72–37,24 s) montre la limite du détecteur par frame :

| Frame | top | bottom | Barres |
| ---: | ---: | ---: | ---: |
| 32,77 | 81 | 1001 | 81, 78 |
| 33,77 | 81 | 1001 | 81, 78 |
| 34,77 | 29 | 1049 | 29, 30 |
| 35,77 | 0 | 1079 | 0, 0 |

La source passe d'un letterbox à un plein cadre. A1 avec p10 applique 912 px
(le plus petit actif). Un simple médian des `(top, bottom)` donnerait
`top = 55`, `bottom = 1025`, soit un actif de 971 px et un scale de ~197,7 % :
un `apply` sous-évalué, pas le 210 % humain. Un vote binaire 2/4 ne tranche
pas non plus. Le clustering dirait « deux clusters, aucun ≥ 60 % → review » ;
c'est ce comportement qu'il faut viser : **review sur transition**, pas
`apply` ni `noop` sec.

### 3.3 Cas 3 — titres, sous-titres et incrustations

Tests synthétiques (images 1920×1080, barres réelles 200/200, texte ou aplat
à `y = 50`, contenu actif à partir de `y = 200`) :

| Cas | top | bottom | Barres | Verdict |
| --- | ---: | ---: | ---: | --- |
| Aplat 20 % de largeur | 200 | 879 | 200, 200 | aplat rejeté, barre correcte |
| Aplat 60 % | 200 | 879 | 200, 200 | aplat rejeté, barre correcte |
| Aplat 70 % | **50** | 879 | **50**, 200 | faux haut : aplat large |
| Aplat 100 % | **50** | 879 | **50**, 200 | faux haut : ligne pleine |
| Texte 30 % (traits) | 200 | 879 | 200, 200 | texte rejeté |
| Texte 60 % (traits) | 200 | 879 | 200, 200 | texte rejeté |
| Texte 80 % (traits) | 200 | 879 | 200, 200 | texte rejeté |
| Ciel nocturne + nuage large | **280** | 339 | **280**, 740 | faux letterbox |
| Barre sombre, contraste 12/255 | 100 | 979 | 100, 100 | barre détectée |

Lecture :

- un **texte isolé** (traits, lettres) est bien rejeté, même à 80 % de
  largeur, parce que les contours sont fragmentés et la moyenne de ligne
  reste sous 2/3 ;
- un **aplat clair large** (bandeau, carton, lower-third plein) passe le
  seuil dès ~70 % de largeur et est pris pour le bord de l'image ;
- un **ciel nuageux** avec une bande claire large peut déclencher un faux
  letterbox : le nuage apporte à la fois le saut EWMA et la densité de
  contours. Le cas 304 est sauvé parce que le ciel est lisse, pas parce que
  l'algorithme serait immunisé ;
- une **barre sombre à faible contraste** (12/255) est détectée si le bord
  est franc : le plancher 6,375 niveaux suffit. Le risque est donc plutôt le
  bord **progressif** (fondu, flou), pas la barre sombre.

Sur le 356 « Pretty Little Liars », segment 2, le prototype donne `top = 0`
sur 3/3 frames, alors que `cropdetect` voit `haut 98, bas 6`. Le `noop` natif
est correct : c'est un cadre noir d'intro, pas un letterbox. C'est un gain de
précision, mais le veto A1 actuel était déjà correct.

### 3.4 Synthèse des cas

| Cas | A1 actuel | vsletterbox | Gain / limite |
| --- | --- | --- | --- |
| 304 seg 9, ciel lisse | `review` 642/6 | `top=0` (3/5), `935` (2/5) | faux haut supprimé sur ciel lisse ; bandeau large encore faux |
| 305 seg 1, letterbox stable | 86/82, 213,7 % | 81/78, 211,6 % | +9 px actifs, −2,1 points de scale |
| 305 seg 17, transition | p10 912, apply | 81/78 puis 0/0 | doit devenir `review`, pas apply/noop |
| 356 seg 2, cadre noir | `review` 98/6 | 0/0 | `noop` correct, gain de bruit |
| 304 seg 34, barre sombre | p10 944, apply 211,5 % | 0/0 | faux négatif du natif ; A3 doit prendre le relais |
| Titre texte | variable | rejeté | bon |
| Aplat large | variable | accepté à tort | limite structurelle |

---

## 4. Portage Rust : options et recommandation

### 4.1 Option A — étendre le filtergraph FFmpeg

Pistes : `cropdetect` avec d'autres `limit`/`round`, `blackframe`,
`signalstats`, `edgedetect` puis `cropdetect` sur le masque.

Limites :

- `cropdetect` reste un seuil de luminance avec arrondi ; même à 2 px, il
  voit le ciel du 304 comme une barre (`h ≈ 604`, `y ≈ 476`) ;
- `blackframe` et `signalstats` sont **frame-level** : ils ne donnent pas la
  densité de contours par ligne ;
- `edgedetect` est un filtre visuel, pas un métrique ligne à ligne ; le
  chaîner à `cropdetect` ne fait que déplacer le seuil ;
- un filtre FFmpeg custom serait du C à maintenir, pas plus simple qu'une
  routine Rust.

**Verdict** : insuffisant seul. Utile comme **cross-check** ou fallback
(`cropdetect=24:2:0`) dans le spike, pas comme détecteur principal.

### 4.2 Option B — routines Rust natives sur buffers d'échantillons

Pipeline recommandé :

```text
ffmpeg -ss <start> -t <durée> -vf fps=<n>,scale=<w>:<h>
  -pix_fmt gray        # luma seule ; ou yuv444p pour la variante fidèle
  -f rawvideo -
        │
        ▼
  pour chaque frame :
    1. moyennes de lignes (u32)                O(H·W), pas d'allocation par ligne
    2. EWMA alpha=0,05, seuil μ + 5·max(σ,0,005)
    3. lignes candidates = premiers sauts haut/bas
    4. pour chaque candidate : Sobel Y 3×3 ciblé sur ±2 lignes
       → couverture de colonnes / plus long run horizontal
    5. score de contour ≥ 2/3 et transition unique
        │
        ▼
  clustering temporel des couples (top, bottom), tolérance 1–2 px
  cluster dominant ≥ 60 % → Crop ; sinon review
        │
        ▼
  Vec<Crop> filtré par le cluster dominant → decide_segment() existant
  (p10, dominant, asymétrie, couverture, scale, Position Y)
```

Le clustering est appliqué **avant** `decide_segment` : seuls les crops du
cluster dominant sont transmis (ou un crop médian répété). Le p10 de
`decide_segment` ne voit alors plus d'outlier ; il reste utile pour le chemin
`cropdetect`, mais il ne doit jamais recevoir les mesures brutes du natif.

La mesure finale doit rester en **pleine résolution** (1920×1080 pour les
sources du corpus) : un downscale à 480 px ferait perdre la précision 1 px.
Un pré-scan grossier sur une miniature peut seulement proposer une fenêtre de
lignes, à revalider en pleine résolution.

Le prototype reproduit `pre` avec chroma pour rester fidèle au dépôt ; le
port A1 peut travailler en **luma seule** (`gray`), ce qui évite qu'un
bandeau coloré soit artificiellement remonté et réduit le coût. Le spike doit
comparer les deux variantes : chroma (fidèle au dépôt) et luma seule
(candidat A1).

Points de conception :

- **EWMA** : structure `EwmaStats { init: bool, mean: f64, var: f64 }`,
  `add` et `threshold` sans allocation. La formule doit être identique à
  celle du C++ (pas de `f64::round`, pas de variance naïve).
- **Sobel Y ciblé** : la frontière letterbox est horizontale ; un Sobel Y
  3×3 suffit. `detect.rs` contient déjà `sobel_row_mean` et `sobel_y` pour
  A2 ; réutiliser `sobel_row_mean` sur les lignes candidates évite
  d'introduire un nouvel opérateur. Un ExKirsch 5×5 plein cadre serait
  8×25 taps/pixel, soit ~30× plus coûteux pour un gain marginal.
- **Couverture de colonnes** : préférer `fraction de colonnes avec |G_y| >
  seuil` ou le plus long run horizontal à la moyenne de ligne. Un texte
  fragmenté a une moyenne < 2/3 mais peut avoir des runs courts ; un aplat
  large a un run long. C'est le critère qui manque au dépôt pour distinguer
  les deux.
- **Clustering temporel** : regrouper les `(top, bottom)` à ±1–2 px, compter
  les frames, choisir le cluster dominant. Si le dominant < 60 % (seuil à
  calibrer sur le spike), `review`. C'est plus robuste que le p10 actuel, qui
  privilégie les frames les plus croppées et transforme une frame à faux
  `top=935` en faux crop.
- **Pas de `permanent`** : A1 n'a pas de barre fournie. Utiliser la
  persistance temporelle, pas un snap à une valeur arbitraire.
- **Garde-fous conservés** : segment keyframé, `barred_frac`, asymétrie,
  `scale > 300 %`, `Position Y > 600`, couverture, différé A3. Le détecteur
  natif ne remplace aucun de ces vetos.
- **Asymétrie** : le code actuel utilise `|top−bottom| ≤ 0,5 × max(top,
  bottom)`, pas un seuil absolu de 8 px. Pour le natif, un seuil plus strict
  (`|top−bottom| > max(8, 0,25 × max)`) est à évaluer : le 304 seg 34
  (80/56) passe aujourd'hui en `apply` alors que l'humain keyframe.

### 4.3 Intégration et préservation du contrat

L'intégration minimale et sûre :

1. `A1Options` gagne `detector: A1Detector` (`Cropdetect` par défaut,
   `Native` opt-in) et éventuellement `native_samples`, `edge_coverage`,
   `cluster_frac`.
2. `detect.rs` gagne `letterbox_native(ffmpeg, video, start, duration,
   source_height, options) -> Result<Vec<Crop>>`, qui retourne des `Crop`
   de la même forme que `cropdetect` (`w = source_width`, `x = 0`,
   `y = top`, `h = bottom − top + 1`).
3. `decide_a1` choisit le détecteur ; `decide_segment` et `coverage` ne
   changent pas. Le chemin `cropdetect` reste **byte-identique**.
4. `PostOptions`/`PostConfig` gagnent `a1_detector: String` (ou enum
   sérialisé) avec `#[serde(default)]` et `deny_unknown_fields` respecté ;
   `config/config.toml` garde `cropdetect` tant que le spike n'a pas conclu.
5. Les détails JSON restent les mêmes ; on ajoute seulement, en mode natif,
   `detector`, `samples`, `edge_coverage`, `persistence`, `cluster_frac`.
   Le schéma `post.json` reste `1` (clés additives).

Contrat préservé :

- `Decision::apply/review/noop` et les champs `scale_target`,
  `position_y_target`, `position_y_delta`, `coverage`, `confidence` ;
- ordre d'insertion des clés `serde_json` (`preserve_order`) ;
- arrondis via `workflow_schema::round::round_to` ;
- aucun changement de l'Ancre ni des keyframes ;
- aucune dépendance Python/VapourSynth dans `workflow-cli` ;
- aucune écriture `/mnt/cache` ou `runtime/`.

### 4.4 Coût et performance

Mesures sur le corpus (lecture seule) :

| Chemin | Segment | Durée | Temps |
| --- | --- | ---: | ---: |
| `cropdetect=24:16:0` | 305 seg 1 | 2,18 s | **0,47 s** |
| `cropdetect=24:16:0` | 304 seg 9 | 2,66 s | **0,32 s** |
| Prototype Python (2 frames, ExKirsch plein cadre) | 305 seg 1 | 2,18 s | 2,61 s |
| Prototype Python (3 frames, ExKirsch plein cadre) | 304 seg 9 | 2,66 s | 2,92 s |

Le prototype Python n'est **pas** représentatif du coût Rust : il fait
8×25 taps/pixel en NumPy et paie l'extraction ffmpeg. Une routine Rust
ciblée (moyennes de lignes + Sobel Y sur quelques lignes candidates) est
estimée à **0,1–0,3 s** pour 6 frames, dominée par le décodage ffmpeg. Le
budget A2 OCR (8 threads, ~13 s sur 12 frames complètes) est un plafond très
large : A1 natif reste un calcul de convolution, pas un modèle neuronal.

Le coût mémoire peut rester bas : traiter les frames en flux, une frame
`gray` 1920×1080 = 2,07 Mo, ou 6 frames = 12,4 Mo. Ne pas charger la vidéo
entière.

---

## 5. Analyse comparative coûts / bénéfices

### 5.1 Précision et décisions

Gains attendus :

- **305** : hauteur active exacte à 1 px, scale +2,1 points plus proche du
  choix humain, suppression du biais 16 px ;
- **304 seg 9** : faux haut de 642 px supprimé sur les frames où le ciel est
  lisse ; le veto d'asymétrie reste nécessaire pour les frames à bandeau ;
- **356 seg 2** : `noop` au lieu de `review`, moins de bruit ;
- **revues de couverture / cropdetect vide** : une géométrie exacte peut
  réduire quelques faux alarms (6 couvertures résolues, 2 cropdetect vides
  dans l'étude GOOD).

Limites :

- les **8 applies non résolus** de l'étude GOOD (325, 304, 8, 89) sont
  majoritairement des keyframes/reframe ou des plans larges intentionnels,
  pas des erreurs de détection ; le détecteur natif ne les corrige pas. Le
  différé A3 (R2) est le bon levier ;
- les **7 revues asymétriques résolues** (l'humain a croppé) peuvent rester
  des revues si l'asymétrie est réelle ; le natif ne doit pas les convertir
  en apply sans preuve ;
- le **304 seg 34** montre un faux négatif du natif sur une barre sombre à
  bord faible : sans A3, il sous-croppe. C'est acceptable si A3 reste le
  filet, pas si le natif devient le seul détecteur.

Ordre de grandeur : le gain est **réel mais ciblé** (letterbox stable,
ciel lisse, bruit de revue), pas une refonte des 87 % de vetos corrects ni
des 8 sur-zooms. La décision doit se prendre sur un spike mesuré, pas sur
304/305 seuls.

### 5.2 Ressources

- `cropdetect` : 0,32–0,47 s par segment mesuré ; le chemin natif doit
  rester dans le même ordre de grandeur.
- Le décodage ffmpeg est inévitable dans les deux cas ; le natif ajoute
  quelques convolutions ciblées, pas un passage plein cadre.
- Le coût est linéaire en nombre de frames échantillonnées ; 6 frames par
  segment suffisent pour un clustering, contre ~25–100 frames pour
  `cropdetect` sur 1–4 s.

### 5.3 Maintenance

- Le chemin `cropdetect` est un standard FFmpeg, bien connu, mais son
  comportement est opaque (seuil de luminance, arrondi).
- Le chemin natif ajoute ~200–300 lignes Rust + tests, mais il est
  déterministe, sans modèle, et testable sur des images synthétiques
  (EWMA, contour, clustering). C'est plus de code, moins de boîte noire.
- Le garder **opt-in** et conserver `cropdetect` en fallback réduit le
  risque de régression ; la promotion ne se fait qu'après le spike.

### 5.4 Matrice de décision

| Option | Verdict |
| --- | --- |
| Intégrer le plugin/wrapper VapourSynth tel quel | **Non** : dépendance Python, par-frame, pas de décision, nettoyage hors périmètre |
| Remplacer `cropdetect` par le natif par défaut | **Non** : faux positifs sur aplats larges et ciels nuageux, faux négatifs sur bords faibles |
| Ajouter un détecteur natif opt-in, garder `cropdetect` | **Oui** : gain de précision mesurable, risque borné, contrat préservé |
| Rejeter l'idée | **Non** : l'EWMA 5σ et la validation de contour sont des primitives utiles |

---

## 6. Recommandations et feuille de route

### 6.1 Règles

1. **Ne pas intégrer** `vapoursynth-letterbox` comme dépendance, ni le
   plugin C++, ni le wrapper Python, ni `clean_letterbox`.
2. **Porter seulement** l'EWMA 5σ, le scan ligne à ligne et l'idée de
   validation par contour horizontal, avec un Sobel Y ciblé et un clustering
   temporel.
3. **Garder `cropdetect` par défaut** ; le mode natif est
   `a1_detector = "native"`, opt-in, sans changement du contrat A1.
4. **Conserver tous les vetos** : keyframes, asymétrie, `barred_frac`,
   `scale > 300 %`, `Position Y > 600`, couverture, différé A3.
5. **Ne pas utiliser le p10 actuel** pour agréger des mesures natives :
   cluster dominant ≥ 60 %, sinon `review`. Le p10 est conservé pour le
   chemin `cropdetect`.
6. **Documenter** toute promotion en D-058+ après le spike ; D-057 ne
   valide que l'expérimentation.

### 6.2 Spike de validation hors production

Périmètre minimal :

- 304 seg 9 (faux ciel), 305 seg 1/9/17 (letterbox + transition),
  356 seg 2 (cadre noir), 304 seg 34/36 (barre sombre/keyframes) ;
- un échantillon stratifié des 65 `apply` A1 et des 116 `review` segment
  de l'étude GOOD, avec au moins les 8 applies non résolus et les 7 revues
  asymétriques résolues ;
- 20 segments `noop` sans barre, pour mesurer les faux applies.

Métriques :

| Métrique | Cible |
| --- | --- |
| Erreur top/bottom vs `cropdetect=24:2:0` ou vérité manuelle | ≤ 2 px |
| Erreur de scale vs GOOD sur 305 | ≤ 1 point |
| Faux `apply` sur segments sans barre | 0 sur l'échantillon |
| Faux `noop` sur letterbox stable à bord franc | 0 sur l'échantillon |
| Accord de décision avec A1 actuel | ≥ 90 % hors cas ciblés |
| Temps par segment | ≤ 1,2 × `cropdetect` |
| Variance frame-à-frame du cluster | ≤ 1 px |

Les barres sombres à bord progressif (304 seg 34) sont hors de portée du
détecteur : elles doivent rester couvertes par A3/review, pas par une
promesse de rappel 100 %.

Le spike peut être un test `#[ignore]` ou un exemple
`crates/workflow-steps/examples/a1_letterbox_spike.rs`, qui lit le corpus en
lecture seule et écrit dans `/tmp/opencode/a1-letterbox-spike`. Il ne doit
pas être câblé dans `workflow-cli`.

### 6.3 Tests unitaires à ajouter avec le port

- EWMA : première valeur, mise à jour moyenne/variance, seuil 5σ, plancher
  0,005, non-régression sur une rampe lente.
- Scan : barre haute/basse, aucune barre, barre 1 px, barre asymétrique.
- Contour : ligne pleine, texte fragmenté, aplat 70 %, bruit.
- Clustering : cluster unique, deux clusters 50/50 → review, outlier isolé.
- Intégration : `decide_segment` inchangé sur les fixtures actuelles ;
  chemin `cropdetect` identique ; chemin natif additif.

### 6.4 Forces transposables

- **Détection différentielle 5σ** : l'EWMA compare chaque ligne à la
  prédiction des lignes précédentes, avec un plancher de bruit. C'est ce qui
  rejette les gradients lents et les ciels uniformes, là où un seuil absolu
  de luminance échoue.
- **Scan ligne à ligne à 1 px** : pas de quantification 16 px ; la frontière
  est mesurée là où elle est, ce qui récupère 4–5 px par barre sur le 305.
- **Validation croisée par contour** : une ligne de luminance seule ne suffit
  pas ; exiger un contour horizontal évite les faux positifs sur les zones
  sombres uniformes. Le principe est bon, l'opérateur doit être ciblé
  (Sobel Y) et complété par une couverture de colonnes.
- **Garde-fous de nettoyage comme inspiration** : `fullblack_thr` et
  `_VSLETTERBOX_AREA_MULTIPLIER` rappellent qu'une image presque entièrement
  noire ne doit pas être traitée comme un letterbox ; pour A1, cela se
  traduit par un `review` sur actif très petit, pas par un crop.

### 6.5 Éléments à laisser de côté

| Élément | Raison |
| --- | --- |
| Plugin C++ `vsletterbox` | Dépendance VapourSynth, API de frames, pas de décision |
| Wrapper Python `vsletterbox.py` | Python, `vs-jetpack`, `vsmasktools` |
| `clean_letterbox`, deringing, `Morpho.minimum` | Nettoyage pixel, hors A1 |
| `_VSLETTERBOX_AREA_MULTIPLIER`, `fullblack_thr` | Garde-fou de nettoyage, pas de crop |
| `permanent` (prior utilisateur) | Impose une barre fournie, pas un anti-jitter temporel |
| ExKirsch 5×5 plein cadre | Trop coûteux, trop général ; Sobel Y ciblé suffit |
| `border_y/u/v` | Deringing, hors périmètre |

---

## 7. Décision D-057

> **D-057 : A1 — détecteur de letterbox natif opt-in, pas d'intégration
> VapourSynth**
>
> - **Contexte** : étude du dépôt `Akatmks/vapoursynth-letterbox` (commit
>   `ac274fa`, 13 août 2026) et spike read-only sur 304/305/356, 8 novembre
>   2026.
> - **Constats** :
>   1. l'EWMA 5σ rejette le faux ciel nocturne lent du 304 (haut 642 → 0)
>      et le scan 1 px mesure 81/78 px sur le 305 contre 86/82 px pour
>      `cropdetect=24:16:0` ;
>   2. le masque ExKirsch seul ne rejette ni un ciel nuageux large ni un
>      bandeau bas large ; le dépôt est par-frame, sans agrégation,
>      symétrie ni décision ;
>   3. le magnétisme `permanent` n'est pas un anti-jitter temporel ;
>      `clean_letterbox` et le deringing sont hors A1.
> - **Décision** :
>   1. ne pas intégrer le plugin/wrapper VapourSynth ni `clean_letterbox` ;
>   2. expérimenter un détecteur natif Rust **opt-in**
>      (`a1_detector = "native"`), avec EWMA 5σ, Sobel Y ciblé, couverture
>      de colonnes et clustering temporel ;
>   3. conserver `cropdetect` par défaut et tous les vetos A1 ;
>   4. ne pas modifier le contrat `apply/review/noop`, `scale`,
>      `Position Y`, `coverage`, `confidence` ; clés additives seulement ;
>   5. aucun code de production modifié par cette étude.
> - **Validation** : `docs/etudes/ETUDE-VSLETTERBOX-A1.md`, scripts et
>   résultats `docs/etudes/ETUDE-VSLETTERBOX-A1/`.
> - **Références** : D-056, `docs/lots/LOT-9-POST-STEP8.md`,
>   `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md`,
>   `crates/workflow-steps/src/post/{a1,detect}.rs`.

---

## 8. Reproductibilité

Depuis la racine du dépôt :

```sh
cd docs/etudes/ETUDE-VSLETTERBOX-A1
export V="/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2"

# 305, letterbox stable
python3 scripts/analyze.py "$V/305 Camille/docs/6ter - LRDRLD -  Chauffeuse poids lourd et enceinte.mp4" \
  --start 0.050 --duration 2.180 --fps 1.0 --json > results/305-seg1.json

# 305, transition
python3 scripts/analyze.py "$V/305 Camille/docs/6ter - LRDRLD -  Chauffeuse poids lourd et enceinte.mp4" \
  --start 32.770 --duration 4.420 --fps 1.0 --json > results/305-seg17.json

# 304, ciel nocturne
python3 scripts/analyze.py "$V/304 Camille/docs/Cette voiture percute une maison !.mp4" \
  --start 43.250 --duration 2.660 --fps 2.0 --json > results/304-seg9.json

# 356, cadre noir d'intro
python3 scripts/analyze.py "$V/356 Camille/docs/M6+ - Pretty Little Liars - Extrait 6 - La mort de Maya.mp4" \
  --start 1.850 --duration 1.380 --fps 2.0 --json > results/356-seg2.json

# tests synthétiques
python3 scripts/synthetic.py > results/synthetic.txt

# référence cropdetect 2 px
ffmpeg -hide_banner -ss 0.050 -i "$V/305 Camille/docs/6ter - LRDRLD -  Chauffeuse poids lourd et enceinte.mp4" \
  -t 2.180 -vf cropdetect=24:2:0 -f null - 2>&1 | grep -o "crop=[0-9:]*" | sort | uniq -c | sort -rn
```

Dépendances : Python 3.10, `numpy`, `ffmpeg`/`ffprobe`. Les scripts lisent le
corpus en lecture seule et n'écrivent que sur `stdout` (ou dans le fichier de
redirection choisi). Aucun accès à `/mnt/cache`, `runtime/` ou au dépôt
Python legacy.

### 8.1 Fichiers de preuve

| Fichier | Contenu |
| --- | --- |
| `results/305-seg1.json` | 305 seg 1, 2 frames, cropdetect 16 px |
| `results/305-seg9.json` | 305 seg 9, 3 frames à 2 fps |
| `results/305-seg17.json` | 305 seg 17, transition letterbox → plein cadre |
| `results/305-seg18.json` | 305 seg 18, contrôle sans barre |
| `results/304-seg9.json` | 304 seg 9, 5 frames à 2 fps, faux haut |
| `results/304-seg34.json` | 304 seg 34, barre sombre non détectée |
| `results/356-seg2.json` | 356 seg 2, cadre noir d'intro |
| `results/synthetic.txt` | tests texte/aplat/ciel/contraste |
| `results/cropdetect.txt` | sorties `cropdetect` 16 px et 2 px |

### 8.2 Références

- `crates/workflow-steps/src/post/a1.rs` : `decide_a1`, `decide_segment`,
  `coverage`, percentile.
- `crates/workflow-steps/src/post/detect.rs` : `cropdetect`,
  `sobel_row_mean`, `sobel_y`.
- `crates/workflow-steps/src/post/run.rs` : `PostOptions`, appel A1.
- `crates/workflow-core/src/config.rs` : `PostConfig`, `a1_margin`.
- `docs/lots/LOT-9-POST-STEP8.md` : contrat A1.
- `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md` : métriques 65 applies,
  116 reviews, 8 non résolus, 7 asymétriques résolus.
- Dépôt étudié : `https://github.com/Akatmks/vapoursynth-letterbox`,
  commit `ac274faf0804fa3c23265f107ae90c63ec926f2c`.
- Masque : `Jaded-Encoding-Thaumaturgy/vs-masktools`,
  `vsmasktools/edge/_5x5.py` (`ExKirsch`), commit `5a36970`.

---

## 9. Spike Rust (D-057)

Le spike est implémenté dans
`crates/workflow-steps/examples/a1_letterbox_spike.rs`, **hors production** :
il n'est pas câblé dans `workflow-cli` et ne modifie ni A1 ni `cropdetect`.
Il reprend l'EWMA 5σ, un Sobel Y 3×3 ciblé sur les lignes candidates, une
couverture de colonnes ≥ 2/3 et un clustering temporel ±2 px avec cluster
dominant ≥ 60 %, sinon `review`. Le seuil de contour par défaut est 32/1020.
Les écritures sont limitées à `/tmp/opencode/a1-letterbox-spike`.

```sh
cargo run -p workflow-steps --release --example a1_letterbox_spike -- \
  --corpus "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2" \
  --pre both --runs 3
```

### 9.1 Résultats

Les deux variantes, luma seule (`gray`) et `pre` avec chroma (`chroma`),
donnent les mêmes décisions et les mêmes barres sur les six fixtures :

| Fixture | Décision | Cluster | Barres | cd16 | cd2 | Réf. | Δ px |
| --- | --- | ---: | ---: | ---: | ---: | --- | ---: |
| 305 seg 1 | `apply` | 2/2 (100 %) | 81/79 | 86/82 | 82/80 | prototype 81/78 | +0/+1 |
| 305 seg 9 | `apply` | 3/3 (100 %) | 81/79 | 86/82 | 82/80 | prototype 81/79 | +0/+0 |
| 305 seg 17 | `review` | 2/4 (50 %) | 81/79 | 86/82 | 82/80 | transition | — |
| 304 seg 9 | `noop` | 5/5 (100 %) | 0/0 | 642/6 | 636/0 | pas de barre | +0/+0 |
| 356 seg 2 | `noop` | 3/3 (100 %) | 0/0 | 98/6 | 92/0 | pas de barre | +0/+0 |
| 304 seg 34 | `noop` | 4/4 (100 %) | 0/0 | 80/56 | 74/52 | A1 80/56 | −80/−56 |

- **Précision** : 81/79 sur le 305 seg 1 et le 305 seg 9, soit 1 px d'écart
  maximum avec le prototype (81/78) et 3–5 px avec `cropdetect=24:16:0`
  (86/82). Le
  scale natif avec la marge 1,015 vaut 211,8 % (humain 210,0 %, +1,8 point) ;
  sans marge, la hauteur exacte donne 208,7 % (−1,3 point). L'écart résiduel
  est la marge de sécurité, pas une erreur de détection.
- **Faux positifs** : sur le 304 seg 9, les candidats 935/936 (bandeau bas)
  et 1002/1003 sont déclenchés par l'EWMA mais **rejetés par le Sobel Y** avec
  une couverture de 0,38–0,46, contre 0,76–0,83 pour l'ExKirsch du prototype
  Python. Le Sobel horizontal ciblé est donc plus robuste que le masque 5×5
  multi-orientations sur les bandeaux et logos. Le clustering `(0,0)` 5/5
  confirme `noop`.
- **Transition** : le 305 seg 17 donne deux clusters à 50 % (81/79 et 29/30),
  donc `review` ; le détecteur ne force ni `apply` ni `noop`.
- **Faux négatif connu** : le 304 seg 34 ne déclenche pas l'EWMA sur la barre
  réelle 80/56 (bord faible) ; les candidats de bandeau 935/936 sont rejetés
  par le Sobel, d'où `noop`. A3 doit rester le filet pour ce cas.

### 9.2 Coût

| Pré | Analyse Rust | Total natif | `cropdetect` 16 px | Ratio médian |
| --- | ---: | ---: | ---: | ---: |
| `gray` | 8–17 ms | 353–630 ms | 313–605 ms | **1,12×** |
| `chroma` | 18–34 ms | 398–734 ms | 314–591 ms | **1,24×** |

Le temps d'analyse Rust est négligeable (≤ 34 ms) ; le total est dominé par
le démarrage et le décodage `ffmpeg`. La variante luma seule respecte la
cible ≤ 1,2× `cropdetect` ; la variante chroma la dépasse sur plusieurs
fixtures sans gain de décision. **`gray` est le candidat A1.**

### 9.3 Limites et suite

- Six fixtures seulement ; l'échantillon stratifié des 65 `apply`, 116
  `review` et 20 `noop` de l'étude GOOD reste à exécuter.
- Les seuils (32/1020, couverture 2/3, tolérance ±2 px, 60 %) ne sont pas
  calibrés sur un corpus large ; ils sont des valeurs de spike.
- Le 304 seg 34 reste un faux négatif structurel du détecteur différentiel
  sur bord faible ; A3 doit conserver le relais.
- Aucune intégration production : pas de `a1_detector`, pas de modification
  de `PostConfig`, pas de changement du contrat A1.

La sortie brute du spike est conservée dans
[`ETUDE-VSLETTERBOX-A1/results/spike-rust.md`](ETUDE-VSLETTERBOX-A1/results/spike-rust.md).

## 10. Évaluation à l'échelle — 201 segments GOOD

Le spike a été exécuté en mode `gray` sur l'échantillon complet de l'étude
GOOD : **65 `apply`**, **116 `review`** segment et **20 `noop`** sans barre
(20 vidéos distinctes, premier `noop` sans barre et sans crop humain par
vidéo). Fenêtre A1 : `start = in + 0,05`, `duration = clamp(out − in − 0,1,
0,3, 4,0)`, échantillonnage `fps = clamp(2 / durée, 2, 8)` pour garantir au
moins deux frames. Une seule répétition par segment ; seuils 32/1020,
couverture 2/3, clustering ±2 px, dominant ≥ 60 %.

Matrice A1 (`cropdetect`) × natif :

| A1 \ natif | `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: |
| `apply` (65) | **25** | 26 | 14 |
| `review` (116) | **0** | 12 | 104 |
| `noop` (20) | **0** | 1 | 19 |

Détail des applies et revues :

| Sous-ensemble | n | natif `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: | ---: |
| apply accepté | 11 | 6 | 2 | 3 |
| apply ajusté | 45 | 18 | 21 | 6 |
| apply reverté | 9 | 1 | 3 | 5 |
| review veto correct | 92 | 0 | 2 | 90 |
| review résolue | 13 | 0 | 5 | 8 |
| — dont asymétrique | 7 | 0 | 2 | 5 |
| — dont couverture | 6 | 0 | 3 | 3 |
| review persistée / sans défaut | 11 | 0 | 5 | 6 |
| noop | 20 | 0 | 1 | 19 |

### 10.1 Constats

- **Précision excellente, rappel insuffisant.** Le natif seul ne produit
  **aucun faux `apply`** sur 123 segments A1 `review`/`noop` (0,0 %), et ses
  25 applies sont à **≤ 2 px** de `cropdetect=24:2:0` (24/25 à ≤ 1 px,
  MAE 0,84/0,68 px). Mais il rate **32/56 applies clairs (57,1 %)** :
  23 `review` et 9 `noop`.
- **Le 89 concentre l'échec.** Sur les 34 applies du film 89, le natif
  n'applique que **4/34** : la barre basse (140 px) a une couverture Sobel
  de 0,45–0,59, parfois aucun déclenchement EWMA, et le clustering voit des
  clusters 137/137, 137/0 et 0/0 sans majorité. Le 305 reste excellent
  (**15/17**), le 68 à **6/7**.
- **Aucun faux veto résolu.** Sur les 7 revues asymétriques résolues, le
  natif donne 5 `noop` (0/0) et 2 `review` (23/0) : il ne remplace pas le
  veto A1. Les 6 revues de couverture résolues donnent 3 `noop` et 3
  `review`. Le gain de revue est donc **nul** sur cet échantillon.
- **304 seg 34** reste un `noop` natif : la barre 80/56 ne déclenche pas
  l'EWMA (bord faible), alors que `cropdetect` applique 211,5 %. Le cas est
  cohérent avec la limite déjà documentée.
- **Coût** : natif médian 451 ms, ratio médian 1,12× `cropdetect=24:16:0`.
  L'analyse Rust reste marginale ; le décodage domine.

### 10.2 Conséquence : pas de promotion du natif seul

Le natif seul est **haute précision, bas rappel**. Il ne peut pas devenir
`a1_detector = "native"` au sens strict : il manquerait plus d'un apply
clair sur deux et ne résoudrait aucune revue asymétrique. Le mode sûr est un
**natif gardé** : `cropdetect` d'abord ; le natif ne raffine un `apply` que
si `cropdetect` a déjà conclu `apply` et que le cluster natif est dominant.
Sinon, on conserve la décision `cropdetect` (fallback). Le projeté de ce mode
sur les 63 applies comparables donne une erreur pixel de **1,98/1,54 px**
contre **2,95/2,06 px** pour `cropdetect=16 px`, avec **53/63** segments à
≤ 2 px (contre 37/63), sans nouveau `apply` sur les `review`/`noop`.

La calibration de la section 11 révise ce constat : le natif v2 récupère
33/34 applies du 89, mais conserve 5 faux négatifs clairs et 0/7 revues
asymétriques résolues.

Le draft D-058 est dans
[`docs/decisions/DECISION-A1-NATIVE-GARDE-D058.md`](../decisions/DECISION-A1-NATIVE-GARDE-D058.md).
La sortie complète est dans
[`ETUDE-VSLETTERBOX-A1/results/spike-good-sample.md`](ETUDE-VSLETTERBOX-A1/results/spike-good-sample.md)
et le CSV par segment dans
[`ETUDE-VSLETTERBOX-A1/results/spike-good-sample.csv`](ETUDE-VSLETTERBOX-A1/results/spike-good-sample.csv).

## 11. Calibration des barres sombres — natif v2

La section 10 laissait le natif seul à 4/34 applies sur le 89. Un diagnostic
ciblé et deux paramètres (Sobel 20 au lieu de 32, couverture 0,45 au lieu de
2/3 pour les barres très sombres) changent la conclusion.

### 11.1 Diagnostic des 30 échecs du 89

Les 30 segments `apply` du 89 ratés par la référence ont été analysés frame
par frame (±10 px autour de `cropdetect=24:2:0`), soit 113 frames et 226
côtés :

| Côté | frontière acceptée | couverture < 2/3 | EWMA sans déclenchement |
| --- | ---: | ---: | ---: |
| haut | 46 | **53** | 14 |
| bas | 26 | **79** | 8 |
| total | 72 (31,9 %) | **132 (58,4 %)** | 22 (9,7 %) |

La couverture est le blocage principal, pas l'EWMA. Les côtés bloqués ont
une couverture médiane de 0,385 (haut) et 0,418 (bas), avec un maximum de
0,653/0,656 : les valeurs sont juste sous 2/3.

### 11.2 Delta des paramètres

| Variante | apply natif total | A1 apply → apply | 89 apply natif | FN clairs | apply sur A1 review | apply sur A1 noop/veto | revues couv. résolues |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Référence (32 px, 2/3) | 25 | 25/65 | 4/34 | 32/56 | 0 | 0/112 | 0/6 |
| Sobel 20 seul | 42 | 40/65 | 17/34 | 19/56 | 2 | 0/112 | 2/6 |
| Sombre 0,45 seul (< 20) | 34 | 32/65 | 9/34 | 25/56 | 2 | 0/112 | 2/6 |
| **v2 (Sobel 20 + sombre 0,45 < 20)** | **65** | **56/65** | **33/34** | **5/56** | **9** | **0/112** | **4/6** |
| v2 durci (sombre < 10) | 65 | 56/65 | 33/34 | 5/56 | 9 | 0/112 | 4/6 |

Les deux paramètres sont **synergiques** : Sobel 20 seul récupère 17/34,
la couverture sombre seule 9/34, les deux ensemble 33/34. Le coût est
inchangé (analyse médiane 17,6 ms, total 445,6 ms, ratio 1,11×).

### 11.3 Matrice v2

| A1 \ natif | `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: |
| `apply` (65) | **56** | 2 | 7 |
| `review` (116) | **9** | 19 | 88 |
| `noop` (20) | **0** | 1 | 19 |

### 11.4 Le 89

| Élément | Référence | v2 |
| --- | ---: | ---: |
| 89 `apply` (34) → natif `apply` | 4 | **33** |
| 89 `review` (14) → natif `apply` | 0 | **9** |
| 89 total (48) → natif `apply` | 4 | **42** |

Les 30 échecs `apply` sont récupérés **29/30** ; le seul restant est le
segment 22 (`review`, cluster 1/2 = 50 %, barres 137/137), un seuil de
clustering sur 2 frames. La géométrie des 42 applies 89 est de **241,5 à
242,4 %** (médiane 242,1) contre **243,0 %** pour le GOOD humain, soit 0,6 à
1,5 point d'écart ; les scales A1 de ces segments étaient à 249,6–290,2 %.

Les 9 applies natifs sur A1 `review` sont **tous des 89 recadrés par
l'humain à 243,0 %** (segments 4, 5, 10, 14, 28, 37, 42, 43, 47). Pour les
5 cas persistés, le `cropdetect` A1 était faux (haut 202/216/230, bas
156/170), ce qui déclenchait les vetos couverture ou comp_y ; les 4 cas
résolus avaient `cd16 = 140/140`, à 2 px près. Ce ne sont donc pas des faux
`apply` : sur les 20 `noop` et les 92 `review` avec veto correct, le natif
ne produit **aucun `apply` (0/112)**. Le 5/123 de la matrice est un artefact
du label A1 `review` ; rapporté au GOOD humain, ces 5 cas sont des vrais
positifs.

### 11.5 Contrôle 304 seg 9

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

### 11.6 Limites et options

Les 5 faux négatifs sur les 56 applies clairs sont 305 seg 17 (`review`
4/8), 325 seg 15 (`noop`), 335 seg 55 (`noop`, barre < 8 px), 356 seg 8
(`noop`) et 89 seg 22 (`review` 1/2). À quoi s'ajoutent 304 seg 34/36
(bord faible, `noop` natif, humain 207,0), 0/7 revues asymétriques résolues
et 6 segments 89 non appliqués (5 revues, dont 4 à barre unique, et le
segment 22). Le 305 seg 1/9 conserve son
scale 211,8 % contre 210,0 % humain (+1,8 point, marge D-056).

| Option | Principe | Gain | Limite |
| --- | --- | --- | --- |
| **A — gardé** (draft D-058) | `cropdetect` d'abord ; natif raffine un `apply` déjà décidé | 89 apply 33/34 raffinés à 242,1 ; 305/68 précis ; 0 nouvel `apply` | ne récupère pas les 9 revues 89 |
| **B — natif v2 seul** | natif primaire | 42/48 au 89 ; 0 `apply` sur noop/veto | 5/56 FN clairs, 304 seg 34/36, 6 segments 89 non appliqués, 0/7 asymétriques |
| **C — v2 primaire + fallback `cropdetect`** | natif décide ; si `review`/`noop`, `cropdetect` décide avec les vetos A1 | 42/48 au 89 + fallback sur les 5 FN et 304 | promeut 9 revues A1 (toutes recadrées humain ici) ; vetos à réévaluer |

Le détail, les tableaux par segment et les commandes sont dans
[`ETUDE-VSLETTERBOX-A1/results/spike-good-sample-v2.md`](ETUDE-VSLETTERBOX-A1/results/spike-good-sample-v2.md),
avec le CSV [`spike-good-sample-v2.csv`](ETUDE-VSLETTERBOX-A1/results/spike-good-sample-v2.csv).

---

*Fin de l'étude. D-057 ouvre un spike opt-in ; D-058 propose un mode natif
gardé, et la calibration v2 ouvre l'option d'un natif primaire avec fallback
`cropdetect`. Aucune modification de production n'est incluse.*
