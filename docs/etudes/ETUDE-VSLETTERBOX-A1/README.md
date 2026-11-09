# Preuves — étude `vapoursynth-letterbox` pour A1

Ce dossier conserve les scripts et résultats de l'étude
[`../ETUDE-VSLETTERBOX-A1.md`](../ETUDE-VSLETTERBOX-A1.md). Il ne contient
aucun code de production : les scripts sont des prototypes Python de mesure,
exécutés en lecture seule sur le corpus.

- **Dépôt étudié** : `Akatmks/vapoursynth-letterbox`, commit
  `ac274faf0804fa3c23265f107ae90c63ec926f2c` (2026-08-13).
- **Corpus** : `/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2`
  (projets 304, 305, 356), lecture seule.
- **Date** : 8 novembre 2026.
- **Décision** : D-057 (spike natif opt-in, `cropdetect` reste le défaut) ;
  D-058 **acceptée** (option A, natif gardé, `a1_refine_native = false` par
  défaut). L'option C est reportée à D-059.

## Contenu

```
scripts/analyze.py    extraction ffmpeg + EWMA 5σ + ExKirsch 5×5 + cropdetect
scripts/profile.py    profil ligne à ligne (pre, edge, fraction >128)
scripts/synthetic.py  tests texte/aplat/ciel/contraste
results/*.json        sorties analyze.py par segment
results/synthetic.txt sorties des tests synthétiques
results/cropdetect.txt sorties cropdetect 16 px et 2 px
results/spike-rust.md sortie du spike Rust EWMA + Sobel Y + clustering
results/spike-good-sample.md  matrice de confusion sur 201 segments GOOD
results/spike-good-sample.csv décisions par segment (65 apply / 116 review / 20 noop)
results/spike-good-sample-v2.md  calibration v2 (Sobel 20 + barres sombres)
results/spike-good-sample-v2.csv décisions par segment de la v2
results/spike-good-sample-v2.json matrice et taux de la v2
results/full-corpus-v2.md  validation du mode gardé sur 2 607 segments
scripts/confusion.py  calcul de la matrice depuis le CSV du spike
```

Le spike Rust est dans
`crates/workflow-steps/examples/a1_letterbox_spike.rs` ; il est hors
production et s'exécute avec `--out /tmp/...` uniquement. Le mode
`--manifest` lit une liste de segments (JSON) et écrit
`sample-report.{json,csv}`.

## Évaluation à l'échelle (201 segments)

L'échantillon GOOD complet a été exécuté en `gray` :

| A1 \ natif | `apply` | `review` | `noop` |
| --- | ---: | ---: | ---: |
| `apply` (65) | 25 | 26 | 14 |
| `review` (116) | 0 | 12 | 104 |
| `noop` (20) | 0 | 1 | 19 |

Le natif seul est haute précision (0 faux apply sur 123) mais bas rappel
(32/56 applies clairs manqués, 0/7 revues asymétriques résolues). Le 89
concentre l'échec (4/34) ; le 305 reste à 15/17 et le 68 à 6/7. D'où la
décision D-058 d'un **natif gardé** avec fallback `cropdetect`.
Détail : [`results/spike-good-sample.md`](results/spike-good-sample.md).

### Calibration v2 (2026-11-09)

Le diagnostic des 30 échecs du 89 montre que la couverture Sobel est le
blocage principal (132/226 côtés, 58,4 %), pas l'EWMA (22/226, 9,7 %). En
abaissant le seuil de contour à 20/1020 et la couverture à 0,45 pour les
barres dont les quatre lignes côté barre sont < 20/255 :

| Variante | apply natif total | 89 apply natif | FN clairs | apply sur A1 noop/veto |
| --- | ---: | ---: | ---: | ---: |
| Référence (32, 2/3) | 25 | 4/34 | 32/56 | 0/112 |
| Sobel 20 seul | 42 | 17/34 | 19/56 | 0/112 |
| Sombre 0,45 seul (< 20) | 34 | 9/34 | 25/56 | 0/112 |
| **v2 (les deux)** | **65** | **33/34** | **5/56** | **0/112** |
| v2 durci (sombre < 10) | 65 | 33/34 | 5/56 | 0/112 |

La géométrie des 42 applies 89 est à 241,5–242,4 % contre 243,0 % pour le
GOOD humain. Le contrôle 304 seg 9 reste `noop` 5/5 : les faux candidats
935/936 et 1002/1003 sont rejetés. Les 9 applies natifs sur A1 `review`
sont tous des 89 recadrés par l'humain (243,0 %), pas des faux positifs.
Détail : [`results/spike-good-sample-v2.md`](results/spike-good-sample-v2.md).

**Implémentation (D-058 option A, 2026-11-09)** : le mode gardé est dans
`crates/workflow-steps/src/post/{a1,detect}.rs`, derrière
`a1_refine_native = false`. Il ne raffine qu'un `apply` déjà décidé par
`cropdetect` ; un natif `review`/`noop`/instable conserve la décision
d'origine. Validation réelle sur le 305 : 16/17 applies à 211,8 % (un à
211,6 %) au lieu de 213,7 %.

### Validation sur le corpus complet (2026-11-09)

La passe Rust sur le corpus GOOD complet couvre **2 607 segments porteurs
d'une décision A1** (75 projets, 0 erreur). Le décompte 2 609 de l'inventaire
inclut deux couches template SDM de l'assist du projet 328, hors A1.

| Mesure | `refine_native = false` | `refine_native = true` |
| --- | ---: | ---: |
| `apply` | 65 | 65 |
| `review` | 116 | 116 |
| `noop` | 2 426 | 2 426 |
| Temps A1 | 1 102,4 s | 1 131,7 s |

- **0 changement d'action** ; **52/65 applies raffinés** (80,0 %).
- Gain pixel moyen vs `cropdetect=24:2:0` : **1,46 px** (médiane 1,75,
  p10/p90 1,00/2,00, min/max −2,00/+2,50 ; 47 améliorés, 1 neutre,
  4 dégradés sur le 89).
- MAE par côté : **2,23 px** (`cropdetect=24:16:0`) contre **0,77 px**
  (natif raffiné).
- Surcoût : **29,27 s** sur 2 607 segments, soit **2,66 %** du temps A1
  (563 ms par apply raffiné).
- 13 applies non raffinés : 9 natifs `noop`/`review` connus et 4 cas 89
  bloqués par le veto de couverture.

Détail et limites : [`results/full-corpus-v2.md`](results/full-corpus-v2.md).
L'activation pilote reste à faire ; l'option C est reportée à D-059.

## Méthode

Le prototype reproduit le cœur de `vsletterbox` :

1. extraction de frames `yuv444p` full range à `fps` et pleine résolution ;
2. `pre = Y + 0,25·|U−128| + 0,25·|V−128|` ;
3. ExKirsch 5×5, 8 orientations, maximum, diviseur 1, clamp 8 bits ;
4. scan haut/bas avec EWMA `alpha = 0,05` et seuil
   `μ + 5 × max(σ, 0,005)` ;
5. validation de la ligne par moyenne du masque `> 2/3` ; variante v2 :
   `> 0,45` si les quatre lignes côté barre sont < 20/255 ;
6. comparaison avec `cropdetect=24:16:0` et `24:2:0`.

Le prototype n'est pas une exécution VapourSynth : le clamp 8 bits, la
conversion de range et le redimensionnement chroma sont approximés. Les
conclusions qualitatives sont robustes ; les valeurs à ±1 px ne sont pas
contractuelles.

## Résultats principaux

### 305 seg 1 — letterbox stable

| Méthode | top | bottom | Actif | Scale exact | ×1,015 |
| --- | ---: | ---: | ---: | ---: | ---: |
| `cropdetect=24:16:0` | 86 | 82 | 912 | 210,5 % | 213,7 % |
| `cropdetect=24:2:0` | 82 | 80 | 918 | 209,2 % | 212,3 % |
| Prototype | 81 | 78 | 921 | 208,5 % | 211,6 % |
| Humain GOOD | — | — | ≈914 | 210,0 % | — |

Le prototype récupère 4–5 px par barre et ~2,1 points de scale par rapport à
l'arrondi 16 px. La marge D-056 (1,04 → 1,015) apporte ~5,2 points ; le reste
est la marge de sécurité.

### 304 seg 9 — ciel nocturne

| Méthode | top | bottom | Décision |
| --- | ---: | ---: | --- |
| `cropdetect=24:16:0` p10 | 642 | 6 | review asymétrique |
| `cropdetect=24:2:0` p10 | 636 | 0 | review asymétrique |
| Prototype 1 fps | 0 (3/3) | 0/77/0 | noop/review |
| Prototype 2 fps | 0, 0, 935, 936, 0 | 0, 0, 76, 76, 0 | cluster (0,0) 3/5 |

Le faux haut de 642 px est rejeté par l'EWMA (gradient lent). Le faux haut de
935 px est le bandeau bas : le masque Kirsch ne rejette pas un aplat large
dans une scène texturée. Le clustering `(0,0)` majoritaire donne `noop` ; un
p10 des hauteurs actives donnerait 68 px et une fausse revue.

### 304 seg 34 — barre sombre

`cropdetect=24:16:0` p10 = 944 (80/56), A1 apply 211,5 % (marge 1,04) ;
`cropdetect=24:2:0` p10 = 954 (74/52) ; prototype `top=0`, `bottom=1079`
(aucune barre détectée). Faux négatif du natif sur un bord faible : A3 doit
prendre le relais (keyframes humaines).

### 356 seg 2 — cadre noir d'intro

`cropdetect` voit 98/6 ; le prototype donne `top=0` sur 3/3 frames. Le `noop`
natif est correct : c'est un contenu noir, pas un letterbox.

### Tests synthétiques

| Cas | top | bottom | Barres | Verdict |
| --- | ---: | ---: | ---: | --- |
| Aplat 20 % | 200 | 879 | 200, 200 | rejeté |
| Aplat 60 % | 200 | 879 | 200, 200 | rejeté |
| Aplat 70 % | 50 | 879 | 50, 200 | faux haut |
| Aplat 100 % | 50 | 879 | 50, 200 | faux haut |
| Texte 30/60/80 % | 200 | 879 | 200, 200 | rejeté |
| Ciel + nuage large | 280 | 339 | 280, 740 | faux letterbox |
| Barre sombre 12/255 | 100 | 979 | 100, 100 | détectée |

Conclusion : le texte isolé est rejeté ; un aplat large (≥ ~70 % de largeur)
ou un nuage large peut être pris pour le bord de l'image. Le critère de
moyenne de ligne à 2/3 est un critère de **couverture**, pas un détecteur
sémantique.

## Reproductibilité

```sh
cd docs/etudes/ETUDE-VSLETTERBOX-A1
export V="/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2"

python3 scripts/analyze.py "$V/305 Camille/docs/6ter - LRDRLD -  Chauffeuse poids lourd et enceinte.mp4" \
  --start 0.050 --duration 2.180 --fps 1.0 --json > results/305-seg1.json
python3 scripts/analyze.py "$V/305 Camille/docs/6ter - LRDRLD -  Chauffeuse poids lourd et enceinte.mp4" \
  --start 32.770 --duration 4.420 --fps 1.0 --json > results/305-seg17.json
python3 scripts/analyze.py "$V/304 Camille/docs/Cette voiture percute une maison !.mp4" \
  --start 43.250 --duration 2.660 --fps 2.0 --json > results/304-seg9.json
python3 scripts/analyze.py "$V/356 Camille/docs/M6+ - Pretty Little Liars - Extrait 6 - La mort de Maya.mp4" \
  --start 1.850 --duration 1.380 --fps 2.0 --json > results/356-seg2.json
python3 scripts/synthetic.py > results/synthetic.txt
```

Dépendances : Python 3.10, `numpy`, `ffmpeg`/`ffprobe`. Aucune écriture dans
`/mnt/cache`, `runtime/` ou le dépôt Python legacy.

## Limites

- Prototype approximatif, pas VapourSynth ; pas de validation à 25 fps.
- Le prototype Python reste limité à 304/305/356 ; la passe Rust complète
  est dans [`results/full-corpus-v2.md`](results/full-corpus-v2.md).
- Images synthétiques non compressées : le bruit H.264 n'est pas modélisé.
- Les valeurs `cropdetect=24:2:0` restent une référence à 2 px, pas une
  vérité terrain manuelle.
