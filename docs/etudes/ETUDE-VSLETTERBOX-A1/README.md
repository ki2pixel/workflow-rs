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
- **Décision** : D-057 (spike natif opt-in, `cropdetect` reste le défaut).

## Contenu

```
scripts/analyze.py    extraction ffmpeg + EWMA 5σ + ExKirsch 5×5 + cropdetect
scripts/profile.py    profil ligne à ligne (pre, edge, fraction >128)
scripts/synthetic.py  tests texte/aplat/ciel/contraste
results/*.json        sorties analyze.py par segment
results/synthetic.txt sorties des tests synthétiques
results/cropdetect.txt sorties cropdetect 16 px et 2 px
```

## Méthode

Le prototype reproduit le cœur de `vsletterbox` :

1. extraction de frames `yuv444p` full range à `fps` et pleine résolution ;
2. `pre = Y + 0,25·|U−128| + 0,25·|V−128|` ;
3. ExKirsch 5×5, 8 orientations, maximum, diviseur 1, clamp 8 bits ;
4. scan haut/bas avec EWMA `alpha = 0,05` et seuil
   `μ + 5 × max(σ, 0,005)` ;
5. validation de la ligne par moyenne du masque `> 2/3` ;
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
- Échantillon 304/305/356, pas les 2 609 segments.
- Images synthétiques non compressées : le bruit H.264 n'est pas modélisé.
- Les valeurs `cropdetect=24:2:0` restent une référence à 2 px, pas une
  vérité terrain manuelle.
