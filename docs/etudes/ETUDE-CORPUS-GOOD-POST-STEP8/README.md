# Preuves visuelles — étude du corpus GOOD (A1–A6)

Ce dossier conserve les preuves de l'étude
[`../ETUDE-CORPUS-GOOD-POST-STEP8.md`](../ETUDE-CORPUS-GOOD-POST-STEP8.md) :
planches de contrôle source/export, rendus synthétiques et données agrégées.

- **Corpus** : `/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2`
  (37 dossiers, 75 triplets `.ecproj`, 2 609 segments, 71 exports).
- **Date** : 8 novembre 2026.
- **Périmètre** : lecture seule, aucun code de production modifié.
- **Poids** : 5,0 Mo de JPEG. Les 176 Mo de frames PNG brutes ne sont pas
  conservés : ils sont reproductibles par `scripts/make_frames.py`.

## Contenu

```
images/    planches de contrôle et rendus synthétiques (JPEG q88)
scripts/   chaîne d'analyse complète (Python 3.10, PIL, OpenCV, ffmpeg)
data/      summary.json, inventory.csv, exports_scan.csv
```

Les grosses tables intermédiaires (`segments.csv` 834 Ko, `decisions.csv`
1,1 Mo, `projects/*.json`) ne sont pas versionnées : elles se régénèrent avec
`scripts/extract.py` à partir du corpus.

## Index des planches

La plupart des planches montrent, pour trois instants, la **source**
(1920×1080, colonne de gauche) et l'**export** (1080×1920, colonne de droite).
Les fichiers `*-rendu-*` et `A1-305-assist-vs-good` sont des compositions
synthétiques (transformation du projet appliquée à la source), utilisées pour
le 305 qui n'a pas d'export.

| Fichier | Cas | Section | Ce que la planche prouve |
|---|---|---|---|
| [`A1-305-assist-vs-good.jpg`](images/A1-305-assist-vs-good.jpg) | 305 Chauffeuse | § 4.6 | Même frame : A1 218,9 % recadre 4 % de plus que l'humain à 210 % ; aucun bord noir dans les deux cas |
| [`A1-68-apply-accepte-keyframes.jpg`](images/A1-68-apply-accepte-keyframes.jpg) | 68 Périph | § 4.6 | Apply A1 conservé, suivi humain par keyframes ensuite |
| [`A1-304-veto-ciel-nocturne.jpg`](images/A1-304-veto-ciel-nocturne.jpg) | 304 Cette voiture | § 4.6 | Le « haut 642 px » est le ciel nocturne : le veto A1 est correct |
| [`A2-54-bandeau-applique.jpg`](images/A2-54-bandeau-applique.jpg) | 54 CLIENT HOTEL | § 5.4 | Apply A2 200,8 %, bandeau retiré, échelle conservée |
| [`A2-77-bandeau-partiel-39.jpg`](images/A2-77-bandeau-partiel-39.jpg) | 77 CHAT | § 5.3 | Bandeau 39 % non appliqué par A2, retiré par l'humain à 207 % |
| [`A2-80-bandeau-partiel-37.jpg`](images/A2-80-bandeau-partiel-37.jpg) | 80 belle-mère | § 5.3 | Idem à 37 % : apply manqué |
| [`A2-zooms-bandeaux-bas.jpg`](images/A2-zooms-bandeaux-bas.jpg) | 40 / 77 / 80 / 81 | § 5.2 | Zoom sur le bas de frame : texte présent dans la source, absent de l'export |
| [`A2-325-logo-veto.jpg`](images/A2-325-logo-veto.jpg) | 325 Océane | § 5.4 | Logo toujours visible après recadrage : `n_removed` trompeur, veto correct |
| [`A2-40-padding-bande.jpg`](images/A2-40-padding-bande.jpg) | 40 CITROUILLES | § 5.2 | 45 px de padding de bande subsistent sans texte : ne pas durcir la formule |
| [`A2-81-apply-conserve.jpg`](images/A2-81-apply-conserve.jpg) | 81 belle-mère | § 5.2 | Apply A2 conservé sur 23/23 segments |
| [`A3-313-recadrage-statique-noop.jpg`](images/A3-313-recadrage-statique-noop.jpg) | 313 W9 LAA9 | § 6.8 | Recadrage statique humain massif qu'A3 laisse en `noop` |
| [`A3-54-tracking-tp.jpg`](images/A3-54-tracking-tp.jpg) | 54 CLIENT HOTEL | § 6.8 | Keyframes A3 suivies par l'humain, corrélation 0,99 |
| [`A4-79-split-040s.jpg`](images/A4-79-split-040s.jpg) | 79 Gulli MSL1 | § 7.3 | Segment de 0,40 s que l'humain découpe en trois |
| [`A4-8-flash-012s.jpg`](images/A4-8-flash-012s.jpg) | 8 Totally Spies | § 7.3 | Flash de 0,12 s laissé tel quel : veto correct |
| [`A5-62-couverture-source.jpg`](images/A5-62-couverture-source.jpg) | 62 SHORT_oléagineux | § 8.1 | Source qui ne couvre pas le comp : avertissement, pas défaut |
| [`A6-328-template-sdm.jpg`](images/A6-328-template-sdm.jpg) | 328 SDM | § 8.2 | Template 3 panneaux + logo conservé tel quel |
| `A1-305-rendu-*.jpg` (2) | 305, rendus GOOD/ASSIST segment 1 | § 4.6 | Transformation GOOD appliquée à la source : contrôle du bord noir |
| [`A1-356-rendu-good-seg2.jpg`](images/A1-356-rendu-good-seg2.jpg) | 356 PLL | § 4.5 | Cadre noir d'intro conservé : ce n'est pas un letterbox |
| [`A1-68-rendu-good-seg1.jpg`](images/A1-68-rendu-good-seg1.jpg) | 68 Périph | § 4.6 | Géométrie GOOD après apply A1 |
| [`A2-40-rendu-good-seg1.jpg`](images/A2-40-rendu-good-seg1.jpg) | 40 CITROUILLES | § 5.2 | Géométrie GOOD après apply A2 |

## Reproductibilité

Les scripts se trouvent dans `scripts/` et n'écrivent que dans `ETUDE_OUT`
(défaut : `scripts/out/`, ignoré par git). Le corpus reste en lecture seule.

```sh
cd docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8
export ETUDE_OUT=/tmp/etude-good-out
export ETUDE_CORPUS="/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple_2"

python3 scripts/inventory.py     # inventaire + contrôles review/post
python3 scripts/extract.py       # projects/*.json, segments, decisions, events
python3 scripts/scan_exports.py  # ffprobe + cropdetect sur 71 exports
python3 scripts/analyze.py       # summary.json (métriques du rapport)
python3 scripts/make_frames.py   # planches images/ (JPEG bruts en PNG)
python3 scripts/render_good.py   # rendus synthétiques 305/356/40/68
```

Dépendances : Python 3.10, `Pillow`, `opencv-python`, `ffmpeg`/`ffprobe`.

Notes de méthode :

- `scan_exports.py` utilise `cropdetect=24:2:0`. Un premier passage en
  arrondi 16 px (`24:16:0`) fabriquait de faux bords de 4 px (1080 → 1072) :
  il a été refait. Au pas de 2 px, 67/71 exports sont pleine frame et les
  4 autres n'ont que des anomalies d'intro/transition.
- Les hashes d'`audit.json` ne sont pas exploités (fichiers renommés/corrigés) :
  la validation est structurelle.
- `make_frames.py` et `render_good.py` codent en dur les cas publiés ; adapter
  la liste `CASES` pour d'autres projets.
