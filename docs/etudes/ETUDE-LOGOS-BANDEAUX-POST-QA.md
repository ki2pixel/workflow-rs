# Étude — détection de logos et de bandeaux fixes après STEP8

| Champ | Valeur |
| --- | --- |
| Type | Étude comparative et technique |
| Statut | Référence (non normative) |
| Date | 2026-11-07 |
| Portée | `post/a2.rs`, `qa/ocr.rs`, `qa/logo.rs`, `vision/dino.rs`, `post/overlay.rs` |
| Décision associée | **D-051** |
| Voir aussi | [`../lots/LOT-9-POST-STEP8.md`](../lots/LOT-9-POST-STEP8.md), [`../lots/LOT-10-QA.md`](../lots/LOT-10-QA.md), [`Architecture_QA_Vidéo_Locale.md`](Architecture_QA_Vidéo_Locale.md), D-032, D-049 |

## 1. Résumé exécutif

La pile Rust existante — OCR PP-OCRv6 pour le texte, DINOv2 ViT-S/14 pour
le visuel, masque alpha des overlays `.ecproj` et persistance temporelle —
est déjà plus adaptée au broadcast généraliste que les sept dépôts étudiés.
Aucun de ces dépôts ne combine découverte open-set, persistance vidéo,
masquage des habillages déclarés et exécution 100 % locale Rust/ONNX.

L'étude du corpus réel (`40 Camille`, `54 Camille`, `58 Camille`) met
néanmoins en évidence deux défauts précis, indépendants du benchmark :

1. **QA OCR — l'agrégation par moyenne de bloc masque les bandeaux fixes.**
   Dès qu'un sous-titre transitoire coexiste avec un bandeau persistant, la
   moyenne verticale du bloc dérive de plus de `MAX_Y_DRIFT = 0.04` et
   l'épisode entier est jeté. Mesuré : dérive `0.0642` sur `54`, `0.0637`
   sur `58`, `0.0562` sur `40 FOOTBALLEUR` ; les trois bandeaux persistants
   ne produisent aucun finding, alors que `40 ABHARTACH` (aucun sous-titre
   concurrent, dérive `0.0019`) est correctement signalé.
2. **A2 — la décision reste purement géométrique.** Un bandeau sombre
   (`médiane 37` sur `40 ABHARTACH`, `55` sur `40 FOOTBALLEUR`) ou une
   géométrie instable (`centre ±10 px`, `hauteur ±23 px`) part en `review`,
   même lorsque l'OCR lit le même texte fixe sur 96–100 % des échantillons.
   A2 ne sait donc pas exploiter la preuve sémantique déjà disponible.

La recommandation est de **ne pas importer** YOLO, Logodetect ni les
détecteurs morphologiques OpenCV, mais de :

- corriger l'agrégation QA par un **clustering par boîte stable** (le texte
  normalisé sert à mesurer la dominance) et d'ajouter un type
  `fixed_overlay` ;
- ajouter à A2 un **gate OCR conservateur** : l'OCR ne peut jamais contourner
  les garde-fous durs (largeur, hauteur, scale, keyframes, bande haute,
  conflit A1/A2) ; il peut confirmer un bandeau clair et stable, et surtout
  **veto** un recadrage quand le texte change trop vite (sous-titres) ;
- laisser les bandeaux sombres en `review` avec preuve OCR et scale proposé,
  tant qu'une politique spécifique n'a pas été validée ;
- extraire à terme les moteurs `frame`/`inference`/`ocr`/`dino` dans un
  module neutre `vision`, utilisé par `qa` et `post`, plutôt que faire
  dépendre `post` de `qa`.

Les phases 1 à 3 et les phases A/B/C du registre logo (D-052) ont modifié
le code de production. La suite de tests reste verte : **505 tests**, `fmt`
et `clippy -D warnings` OK.

## 2. Méthode et périmètre

### 2.1 Audit du code

Lecture intégrale de :

- `crates/workflow-steps/src/post/a2.rs` et `post/detect.rs` pour la
  génération des candidats, les seuils et la formule de scale ;
- `crates/workflow-steps/src/qa/ocr.rs`, `qa/logo.rs`,
  `vision/dino.rs` (anciennement `qa/dino.rs`), `post/overlay.rs`,
  `qa/mod.rs`, `qa/aggregate.rs`, `qa/model.rs`, `qa/cache.rs` pour les
  constantes, les tenseurs et l'agrégation ;
- `crates/workflow-cli/src/main.rs` pour le câblage `qa run` ;
- `config/config.toml`, `assets/models/qa/manifest.toml` et le manifeste
  local `/media/kidpixel/WORK/qa-poc/models/manifest.toml` pour les modèles.

### 2.2 Corpus et lecture seule

Le corpus disponible est celui des rushes, pas des exports finaux :

| Dossier | Projet `.ecproj` | Vidéo source | Durée | Segments |
| --- | --- | --- | ---: | ---: |
| `40 Camille` | `66M - ABHARTACH_9x16` | `66M - ABHARTACH.mp4` | 115,307 s | 27 |
| `40 Camille` | `66M - FOOTBALLEUR ESCROC_9x16` | `66M - FOOTBALLEUR ESCROC.mp4` | 199,147 s | 42 |
| `54 Camille` | `UJUD - CLIENT HOTEL_9x16` | `UJUD - CLIENT HOTEL.mp4` | 216,021 s | 56 |
| `58 Camille` | `UJUD - CROUSTY_9x16` | `UJUD - CROUSTY.mp4` | 96,000 s | 20 |

Les vidéos sont en **1920×1080**, alors que `qa run` attend un export final
**1080×1920**. Le contrôle Stage 0 renvoie donc `resolution: fail` ; c'est
attendu et sans rapport avec la détection OCR/logos. Les décisions A2, elles,
portent bien sur les compositions 1080×1920 des `.ecproj`.

Toutes les sorties QA ont été écrites sous
`/tmp/workflow_investigation/` ; aucune écriture n'a eu lieu dans
`/mnt/cache`. Les `post.json`, `review.json` et `audit.json` existants
(générés le 2026-11-07 à 11:40 UTC) ont été lus, pas réexécutés.

### 2.3 Commandes de diagnostic

```sh
target/release/workflow-cli --config config/config.toml qa run \
  --video "/mnt/cache/58 Camille/docs/UJUD - CROUSTY.mp4" \
  --project "/mnt/cache/58 Camille/projets/UJUD - CROUSTY_9x16.ecproj" \
  --out /tmp/workflow_investigation/qa/58 \
  --models /media/kidpixel/WORK/qa-poc/models \
  --onnxruntime /home/kidpixel/workflow_mediapipe/coral_env/lib/python3.11/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2 \
  --intra-threads 8 --folder-id 58
```

Les variantes `--no-logo --no-face --no-evidence` avec
`RUST_LOG=workflow_steps::qa::ocr=debug` ont fourni les boîtes OCR
échantillon par échantillon. Les frames témoins et les médianes temporelles
ont été extraites avec `ffmpeg` vers `/tmp/workflow_investigation/frames/`
et `/tmp/workflow_investigation/median/`.

## 3. Existant interne

### 3.1 A2 — bande basse pleine largeur

A2 fonctionne en deux temps : `post::detect::band_candidates` produit des
candidats sur une médiane temporelle, puis `post::a2::decide_from_candidates`
applique une politique conservatrice.

Constantes de décision (`post/a2.rs`) :

| Constante | Valeur | Rôle |
| --- | ---: | --- |
| `BANNER_MIN_WIDTH` | 0,50 | largeur minimale d'une bannière applicable |
| `LOGO_MAX_WIDTH` | 0,35 | en dessous : logo probable, `review` |
| `BANNER_MIN_HEIGHT` | 24 px (1080) | bande trop fine : `noop` |
| `BANNER_MAX_HEIGHT` | 200 px (1080) | décor statique probable : `review` |
| `DARK_MEDIAN` | 60 | bande trop sombre : `review` |
| `ANIM_STD` | 35 | écart temporel de luminance trop fort : `review` |
| `GEOM_STD` | 12 px | dérive du centre ou de la hauteur : `review` |

Génération des candidats (`post/detect.rs`) :

- miniature **480 px de large**, gris, **1 fps**, médiane par histogramme
  par pixel ;
- lignes « chaudes » si `sobel_row > 18` **ou** `bright_frac > 0.04`
  (pixels > 170) ; regroupement avec un trou de 4 lignes ;
- colonnes chaudes si plus de 5 % des pixels de la bande sont > 170 ;
- bandes sombres par paires de bords horizontaux (`sobel > 18`) espacées de
  24 à 200 px, largeur ≥ 35 %, médiane < 60 ;
- sortie : `BandCandidate` avec `width_frac`, `persistence`, `center_y_std`,
  `height_std`, `temporal_std`, `median_brightness`, `source`.

Ordre exact des décisions A2 :

1. vidéo absente → `review` ;
2. composition ≠ 1080×1920 → `review` ;
3. échec de détection → `review` ;
4. aucun candidat → `noop` ;
5. segment keyframé → `review` ;
6. échelles hétérogènes → `review` ;
7. bande haute plausible (`24–200 px`, largeur ≥ 35 %, persistance ≥ 60 %,
   aspect ≥ 3) → `review` ;
8. aucune bande basse → `noop` ;
9. sélection du meilleur candidat bas par
   `(largeur ≥ 50 %, hauteur plafonnée à 200, persistance, largeur)` ;
10. largeur < 35 % → `review` « logo probable » ;
11. hauteur > 200 px → `review` « décor statique probable » ;
12. hauteur < 24 px → `noop` ;
13. persistance < 60 % → `review` ;
14. largeur < 50 % → `review` « bannière partielle » ;
15. `center_y_std > 12` ou `height_std > 12` → `review` « animée » ;
16. `median_brightness < 60` → `review` « sombre » ;
17. `temporal_std > 35` → `review` « animée » ;
18. `active_src <= 0` → `review` ;
19. scale > 300 % → `review` « zoom excessif » ;
20. sinon `apply`.

Formule appliquée :

```text
active_src = source_height - height_src
scale      = comp_height / active_src * 100 * marge   (marge = 1,05)
```

A2 applique ce scale à **tous les calques dont le nom commence par
`Segment`**, sans toucher `Anchor Point` ni `Position`. La somme de contrôle
du projet brut est vérifiée après écriture ; un conflit A1/A2 bascule les
deux décisions en `review`.

### 3.2 QA OCR — sous-titres de la bande basse

`qa/ocr.rs` utilise PP-OCRv6 det + rec en ONNX Runtime CPU :

| Constante | Valeur |
| --- | ---: |
| bande basse | `BOTTOM_START = 0.75` |
| zone sous-titre | `SUBTITLE_TOP = 0.76`, `SUBTITLE_BOTTOM = 0.985` |
| texte minimal | 2 caractères, score ≥ 0,55 |
| boîte | hauteur 24–140 px, largeur ≥ 40 px, largeur ≤ 95 % frame |
| géométrie | aspect ≥ 2,0, aire ≤ 15 % frame, centre X ∈ [0,12 ; 0,88] |
| agrégation | 3 échantillons consécutifs, dérive Y ≤ 0,04 |
| détection | côté court 480 px, `thresh 0.3`, `box_thresh 0.5`, unclip 1,6 |
| reconnaissance | hauteur 48 px, largeur de base 320 px, batch 6 |

L'agrégation construit un `Episode` par suite d'échantillons non vides,
puis jette l'épisode si l'écart entre la moyenne Y minimale et maximale des
**blocs** dépasse `MAX_Y_DRIFT = 0.04`. C'est ce point qui masque les
bandeaux persistants lorsque des sous-titres apparaissent au-dessus d'eux.

### 3.3 QA logos — texte haut, DINOv2 et masque d'overlays

`qa/logo.rs` échantillonne **12 frames** réparties sur l'export, lit la
bande haute **0–32 %**, filtre les boîtes (hauteur 18–180 px, largeur ≥ 30,
aspect ≥ 1,2, score ≥ 0,50), puis conserve un texte présent sur au moins
**50 %** des échantillons. Les variantes OCR proches sont fusionnées par
distance d'édition et similarité de boîte.

`vision/dino.rs` (anciennement `qa/dino.rs`) ajoute un second étage
**open-set** :

- `dinov2_vits14_quantized.onnx`, entrée `pixel_values` 224×224, sortie
  `last_hidden_state` ; préprocessing Python : pad carré par la moyenne des
  bords, resize bicubique 256, crop central 224, normalisation ImageNet ;
- probe logistique versionné (`dinov2_logo_probe.json`, 384 poids + biais +
  seuil 0,50), score = sigmoïde(`w·CLS_normalisé + b`) ;
- géométrie : hauteur 18–220 px, largeur ≥ 30, aspect ≥ 1,2 ;
- persistance minimale **3/12**, déduplication avec les findings texte.

`post/overlay.rs` lit les calques image du `.ecproj`, réduit chaque PNG à sa
boîte alpha (seuil 8, pixels opaques > 200), applique les transformations
statiques et ignore les calques animés ou pivotés. Une détection est
masquée si **50 %** de la plus petite boîte est couverte par un overlay
déclaré. C'est ce qui évite de signaler le sticker SDM, le logo M6+ ou un
titre d'émission ajouté par le monteur. A2 utilise le même masque, avec un
seuil de couverture de 90 %, pour neutraliser son veto « logo seul » quand
le logo est caché.

### 3.4 Modèles et runtime

| Rôle | Fichier | Taille | SHA-256 | Licence |
| --- | --- | ---: | --- | --- |
| OCR det | `PP-OCRv6_det_small.onnx` | 9 929 594 | `090f04ab…1ff94f` | Apache-2.0 |
| OCR rec | `PP-OCRv6_rec_small.onnx` | 21 234 383 | `6f327246…3c14884` | Apache-2.0 |
| DINOv2 | `dinov2_vits14_quantized.onnx` | 24 446 700 | `c179f8f7…b197efb` | Apache-2.0 |
| Probe | `dinov2_logo_probe.json` | 11 052 | `e7fa8a7d…afcce4` | versionné |
| Visage | `face_detection_yunet_2023mar.onnx` | 232 589 | `8f2383e4…552fa4` | MIT |

Les binaires OCR/DINOv2 ne sont pas versionnés dans `assets/models/` ; le
probe et le manifeste DINOv2 le sont, les modèles OCR sont décrits dans le
manifeste local du PoC (`/media/kidpixel/WORK/qa-poc/models/manifest.toml`).
Le runtime utilisé est ONNX Runtime
**1.23.2** (bibliothèque du venv legacy), en CPU, avec un nombre de threads
intra-op borné. Les modèles OCR viennent de RapidOCR 3.10.0
(Apache-2.0) ; DINOv2 vient de `onnx-community/dinov2-small` (Apache-2.0).

### 3.5 Graphe de dépendances actuel

```text
post::detect  ◄── qa::dino, qa::stage0, qa::cache
post::ecproj  ◄── post::overlay, post::run, post::a6
post::overlay ◄── post::a2, qa::logo, qa::ocr, qa::source_logo
qa::frame, qa::inference ◄── qa::ocr, qa::dino, qa::face
post::a2     ──► post::detect, post::overlay
```

`post/a2.rs` n'a aujourd'hui aucune dépendance à `qa`. Faire dépendre `post`
de `qa` serait fonctionnel (même crate) mais inverserait la couche : le
post-traitement précède l'export, le QA le suit. C'est pourquoi l'étude
propose un module neutre `vision` plutôt qu'un import direct.

## 4. Benchmark des sept dépôts

### 4.1 Grille d'évaluation

| Critère | Question posée |
| --- | --- |
| Open-world | le dépôt découvre-t-il un logo/bandeau inconnu sans réentraînement ? |
| Temporel | exploite-t-il la persistance vidéo, pas seulement une image ? |
| Rust/ONNX | peut-il tourner localement en Rust/ONNX Runtime CPU, sans Python lourd ? |
| Précision broadcast | résiste-t-il aux scènes texturées, aux PiP et aux habillages ? |
| Coût | latence et empreinte sur la GTX 1650 / 24 threads CPU ? |
| Licence | usage commercial fermé possible sans contamination copyleft ? |

Légende : ● bon, ◐ partiel, ○ faible.

### 4.2 Matrice comparative

Métadonnées GitHub au 2026-11-07 (les étoiles ne sont pas un critère de
qualité) :

| Dépôt | Étoiles | Langage | Licence | Dernière mise à jour |
| --- | ---: | --- | --- | --- |
| `njc44/Logo_detection-with-YoloV7` | 4 | Python | aucune | 2025-07-23 |
| `Azmeer-59189/logo-detector-api` | 0 | Python | aucune | 2026-09-23 |
| `kunjmehta/logo-detection-and-classification` | 1 | Notebook | aucune | 2024-06-13 |
| `Heldenkombinat/Logodetect` | 73 | Notebook | AGPL-3.0 | 2026-09-02 |
| `timminator/VideOCR` | 858 | Python | MIT | 2026-11-06 |
| `KareemMoneeam/Video-Subtitles-Detector` | 2 | Notebook | aucune | 2026-09-22 |
| `Aalaa4444/Video-Subtitles-Detector` | 5 | Notebook | aucune | 2026-06-18 |

| Dépôt | Open-world | Temporel | Rust/ONNX | Précision broadcast | Coût | Licence | Verdict |
| --- | :---: | :---: | :---: | :---: | :---: | :---: | --- |
| YOLOv7 `njc44` | ○ | ○ | ○ | ○ | ◐ | ○ | Rejeté |
| YOLOv8 API `Azmeer` | ○ | ○ | ○ | ○ | ◐ | ○ | Rejeté |
| Open-set YOLOv5 `kunjmehta` | ◐ | ○ | ○ | ◐ | ◐ | ○ | Recherche only |
| Logodetect | ◐ | ● | ○ | ◐ | ○ | ○ (AGPL) | Cas asset connu |
| VideOCR | ◐ | ● | ◐ | ● | ◐ | ● (MIT) | Référence sous-titres |
| Video-Subtitles-Detector `Kareem` | ○ | ● | ● | ○ | ● | ○ | Rejeté |
| Video-Subtitles-Detector `Aalaa` | ○ | ● | ● | ○ | ● | ○ | Rejeté |
| **Pile Rust existante** | ● | ● | ● | ● | ● | ● | **Référence** |

### 4.3 Dépôts logos

#### 4.3.1 `njc44/Logo_detection-with-YoloV7`

Dépôt de démonstration Gradio autour de YOLOv7. Le code ne contient **ni
poids `logo_detection.pt` ni procédure d'entraînement reproductible** : il
appelle `attempt_load('logo_detection.pt')` et suppose un modèle déjà
présent. La dépendance est PyTorch + `yolov7-package`, avec CUDA recommandé.
Le README évoque « 5–10 images par classe, pas d'entraînement », mais le
dépôt ne fournit pas le pipeline qui produit ces poids. En pratique :

- détection **closed-set** : les classes sont figées au moment de
  l'entraînement, pas de découverte d'un bug de chaîne inconnu ;
- aucune persistance temporelle : chaque image est traitée isolément ;
- licence du dépôt absente ; YOLOv7 est GPL-3.0, donc copyleft ;
- intégration Rust/ONNX absente (PyTorch, Gradio).

Verdict : inutilisable pour le broadcast généraliste ; éventuellement utile
comme démonstrateur pour une marque déjà annotée.

#### 4.3.2 `Azmeer-59189/logo-detector-api`

API FastAPI/YOLOv8 qui sert un `best.pt` de 6,2 Mo. L'inspection des chaînes
du checkpoint donne cinq classes : **Apple Inc., Coca Cola, McDonalds
Corporation, PepsiCo Inc., Starbucks Corporation**. C'est un exemple
parfait de détection de marques fermée : le modèle ne peut pas reconnaître
un logo de chaîne TV, un bug de M6, un sticker de template ou un habillage
d'émission. Le service est image-only, avec un seuil de confiance 0,50 et
une limite de 8 Mo/1280 px. Licence du dépôt absente ; Ultralytics YOLOv8
est AGPL-3.0. Verdict : rejeté pour le besoin « bug de chaîne arbitraire ».

#### 4.3.3 `kunjmehta/logo-detection-and-classification`

Projet de cours Rutgers qui reproduit et étend « Open Set Logo Detection and
Retrieval » avec YOLOv5 sur Flickr-47. Le résultat revendiqué est une
amélioration de **24 points de mAP** en détection open-set, mais la
classification reste à **22,56 %** sur 47 classes. C'est une contribution de
recherche intéressante : elle confirme qu'un détecteur open-set doit être
couplé à une étape de reconnaissance/retrieval, et que la classification
fermée est le goulot. Le code est un notebook, sans licence, sans modèle
exporté ONNX, sans persistance vidéo. Verdict : référence académique, pas
une brique de production. Notre couple détection OCR + probe DINOv2 est une
version légère et opérationnelle de la même idée (détection puis
reconnaissance ouverte).

#### 4.3.4 `Heldenkombinat/Logodetect`

Bibliothèque one-shot (Faster R-CNN + embeddings/ResNet18) qui détecte un
logo à partir d'exemplaires fournis par l'utilisateur. Elle sait traiter
vidéo et images, et le README montre des logos de maillots et de terrain.
C'est la seule des quatre approches logos qui soit réellement **one-shot**.
Mais :

- il faut fournir l'asset exact à l'avance ; sans exemplaire, aucun logo
  inconnu n'est découvert ;
- la pile est PyTorch + Faster R-CNN, lourde pour la GTX 1650 et incompatible
  avec une exécution Rust/ONNX CPU sans portage ;
- la licence est **AGPL-3.0**, donc contamination copyleft pour un produit
  fermé ;
- pour les assets déclarés, `OverlayMask` est déjà plus précis : il lit le
  PNG alpha et la transformation statique du `.ecproj`, sans modèle.

Verdict : intérêt restreint aux cas « asset exact fourni », déjà couverts par
le masque d'overlays. Pas d'adoption.

### 4.4 Dépôts sous-titres et bandeaux

#### 4.4.1 `timminator/VideOCR`

Application Python (GUI + CLI) qui extrait les sous-titres incrustés avec
**PaddleOCR** en local, ou Google Lens en mode hybride cloud optionnel.
Licence MIT, 858 étoiles, mise à jour le 2026-11-06, Docker CPU/GPU. Le
README revendique 200+ langues et un traitement 100 % local avec PaddleOCR.
C'est la **validation externe la plus forte** du choix déjà fait dans
`workflow-rs` : PP-OCRv6 est de la même famille que PaddleOCR, et notre
implémentation Rust/ONNX reprend la détection DB + reconnaissance CTC.

Ce que VideOCR apporte comme idées, sans importer son code :

- fenêtrage temporel et déduplication des frames avant OCR (Fast-SSIM) ;
- sélection manuelle d'une zone de crop, puis OCR de cette zone ;
- post-traitement linguistique (wordninja, opencc) hors périmètre QA ;
- mode cloud **optionnel**, jamais obligatoire.

Ce qu'il ne faut pas importer : l'application est un GUI Python avec
PySimpleGUI, Nuitka, des dépendances Windows et un modèle PaddleOCR lourd.
Notre moteur ONNX Rust est plus léger et déjà intégré.

#### 4.4.2 `KareemMoneeam/Video-Subtitles-Detector`

Pipeline OpenCV classique : grayscale, seuillage adaptatif, opérations
morphologiques, contours. Il détecte les mots dans la partie haute, les
sous-titres dans la partie basse, et une « autre zone d'intérêt ». Aucun
OCR, aucune validation sémantique, aucune persistance temporelle (traitement
frame par frame). Licence absente. C'est très rapide, mais sur une scène
texturée (t-shirt, panneau, affiche, sous-titre incrusté dans le décor) le
taux de faux positifs est prohibitif. Le dépôt ne fournit pas de métriques
de précision/rappel ; il s'agit d'un projet de cours.

#### 4.4.3 `Aalaa4444/Video-Subtitles-Detector`

Même famille : OpenCV, gris, flou gaussien, seuillage, morphologie,
contours, reconstruction vidéo. Le README revendique explicitement
« without deep learning, machine learning, or OCR », et propose l'OCR
(Tesseract/EAST/CRNN) comme **extension future**. Licence absente. Verdict :
démonstrateur pédagogique, pas un détecteur de production. Il confirme
seulement qu'un filtre morphologique est un bon **pré-filtre** rapide, pas
un décideur.

### 4.5 Classement argumenté

1. **VideOCR** — meilleure référence sous-titres : PaddleOCR, temporel,
   MIT, actif. Il valide notre choix PP-OCRv6 et suggère le Fast-SSIM comme
   optimisation future. Il ne remplace pas notre moteur Rust.
2. **Logodetect** — seule approche one-shot crédible, mais AGPL, PyTorch,
   et limitée aux assets fournis. `OverlayMask` couvre déjà ce besoin pour
   les overlays déclarés.
3. **kunjmehta** — bonne référence open-set académique ; confirme qu'il
   faut détecter puis reconnaître, mais ne fournit pas de brique Rust.
4. **YOLOv7/YOLOv8** — adaptés aux marques connues, pas aux bugs de chaîne
   arbitraires ; licences copyleft ou absentes ; pas de temporel.
5. **Détecteurs morphologiques OpenCV** — rapides, mais sans OCR ni
   persistance, donc trop de faux positifs sur le broadcast. Ils ne
   distinguent pas un bandeau fixe d'un sous-titre.

La pile Rust existante n'est pas dans ce classement : elle est la
**référence** que les sept dépôts échouent à battre sur le cas d'usage
« broadcast/social généraliste, local, gratuit, sans réentraînement ».

## 5. Diagnostic du corpus Camille

### 5.1 Décisions A2 existantes

| Projet | Candidat `[x0,y0,x1,y1]` | Hauteur | Largeur | Persistance | Médiane | `temporal_std` | `center_y_std` | `height_std` | Décision |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 40 ABHARTACH | `[120,856,1084,1008]` | 156 | 0,504 | 0,878 | 37 | 16,6 | 7,4 | 14,6 | `review` animée |
| 40 FOOTBALLEUR | `[120,856,1288,1008]` | 156 | 0,610 | 1,000 | 55 | 43,7 | 10,5 | 22,7 | `review` animée |
| 54 CLIENT HOTEL | `[100,956,1228,1028]` | 76 | 0,590 | 1,000 | 97 | 18,5 | 1,0 | 3,8 | `apply` 200,8 % |
| 58 CROUSTY | `[100,912,1228,1028]` | 120 | 0,590 | 1,000 | 108 | 21,5 | 10,5 | 20,9 | `review` animée |

Lecture : les quatre candidats contiennent bien la bannière, mais leur
hauteur est gonflée par du contenu de scène au-dessus. `40` et `58` sont à
la limite ou au-delà de `GEOM_STD = 12` ; `40 FOOTBALLEUR` dépasse aussi
`ANIM_STD = 35`. Les deux `40` sont en plus sous `DARK_MEDIAN = 60` (37 et
55) : même si la géométrie était stabilisée, le gate « sombre » les
renverrait en `review`.

### 5.2 Preuve OCR par échantillon

Les logs `RUST_LOG=workflow_steps::qa::ocr=debug` donnent les boîtes et les
textes. Regroupement du prototype par texte normalisé, avec contrôle de
boîte :

| Projet | Texte dominant | Occurrences | `y0` médian | Hauteur | Dérive `y` | Score | Ratio dominant |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 40 ABHARTACH | `DERRY : GRAND FRISSON EN IRLANDE` | 26/27 | 925 | 32 | 7 px | 0,979 | 0,963 |
| 40 FOOTBALLEUR | `FOOTBALLEUR AMATEUR, PRO DE L'ARNAQUE ?` | 42/42 | 924 | 35 | 9 px | 0,987 | 1,000 |
| 54 CLIENT HOTEL | `FAUX CLIENTS, VRAIS ENQUÊTEURS : RIEN NE LEUR ÉCHAPPE` | 56/56 | 980 | 35 | 2 px | 0,987 | 1,000 |
| 58 CROUSTY | `FAUX CLIENTS, VRAIS ENQUÊTEURS : RIEN NE LEUR ÉCHAPPE` | 20/20 | 979 | 35 | 2 px | 0,986 | 1,000 |

Les textes transitoires (sous-titres ou textes de scène) sont bien présents,
mais à faible persistance :

- `40 FOOTBALLEUR` : 14 groupes à 1–2 occurrences (`FCV Dender EH`,
  `Transfert`, `Nimes B`, `il escroque les filles !`, etc.) ;
- `54` : 5 groupes à 1 occurrence (`Je regarde si le personnel`,
  `est sympatique, proactif,`, etc.) ;
- `58` : 2 groupes à 1 occurrence (`et pas d'eau au citron`,
  `comme sur la borne où j'ai commandé,`).

C'est exactement la signature d'un **bandeau fixe** : texte identique,
boîte stable, score élevé, persistance proche de 100 %. Un sous-titre
broadcast change de texte toutes les 1–3 secondes et n'atteint pas ce taux.

### 5.3 Cause racine n°1 — l'agrégation QA jette les bandeaux persistants

`qa/ocr.rs` calcule une seule moyenne `block_y_ratio` par échantillon, puis
jette l'épisode si `max - min > 0.04`. Voici les mesures sur les quatre
projets :

| Projet | Échantillons | `block_y` min | `block_y` max | Dérive | `> 0.04` ? | Résultat QA |
| --- | ---: | ---: | ---: | ---: | :---: | --- |
| 40 ABHARTACH | 27 | 0,8699 | 0,8718 | 0,0019 | non | finding sous-titre |
| 40 FOOTBALLEUR | 42 | 0,8352 | 0,8914 | 0,0562 | oui | **aucun finding** |
| 54 CLIENT HOTEL | 56 | 0,8594 | 0,9236 | 0,0642 | oui | **aucun finding** |
| 58 CROUSTY | 20 | 0,8599 | 0,9236 | 0,0637 | oui | **aucun finding** |

Dès qu'un sous-titre apparaît au-dessus du bandeau, la moyenne du bloc
monte de ~0,86 à ~0,92 ; l'écart dépasse 0,04 et l'épisode est entièrement
supprimé. `40 ABHARTACH`, sans sous-titre concurrent, est le seul cas
correctement signalé — et il est classé `subtitle_burned_in` alors que
c'est un bandeau de titre. Le défaut est donc double : **faux négatif** sur
trois cas et **fausse catégorie** sur le quatrième.

### 5.4 Cause racine n°2 — A2 ne peut pas utiliser la preuve OCR

A2 ne regarde que la géométrie et la luminance. Les deux bandeaux `40` sont
sombres (médiane 37 et 55) ; les candidats `40` et `58` dépassent les seuils
de géométrie. Or l'OCR lit le même texte fixe sur 96–100 % des échantillons. La décision actuelle est donc **conservatrice mais incomplète** :
elle protège le cadrage, au prix de quatre `review` dont deux au moins
pourraient être traités avec une preuve sémantique.

### 5.5 Cause racine n°3 — la contamination du bord haut du candidat

La médiane temporelle à 480 px et le seuil `sobel_row > 18` détectent des
bords de scène faibles au-dessus de la bannière. Exemple sur `58` : le
groupe de lignes chaudes commence à `y=912` alors que le texte persistant
commence à `y=979`, soit **67 px de contenu de scène** dans le candidat.
Sur `40 ABHARTACH`, le groupe commence à `856` pour un texte à `925`
(**69 px**). Le `height_std` élevé (14,6 / 20,9 / 22,7) vient en partie de
cette contamination, pas de la bannière elle-même.

Une piste naïve serait de relever le seuil Sobel ou le `bright_frac`.
Mesures sur la médiane :

- `40 ABHARTACH` : `sobel` reste > 18 jusqu'à `y=904` (46,4), puis 33,2 à
  `912`, 34,5 à `920`, 243,9 à `928` ; un seuil à 50 déplacerait le début
  à `928`, mais un bord de scène à `904` resterait détecté avec un seuil à 40.
- `58` : `sobel` = 18,1 à `912`, 35,5 à `928`, 39,5 à `960`, 367,9 à
  `968` ; un seuil à 40 déplacerait le début à `960`, encore 19 px au-dessus
  du texte.

Conclusion : **aucun seuil global unique ne sépare proprement scène et
bannière**. C'est la preuve sémantique (OCR) qui doit trancher, pas un
réglage de gradient.

### 5.6 Ce qui fonctionne déjà

- **A2 sur `54`** : l'application est propre. Le diff JSON brut/assist
  contient exactement **112 différences**, toutes des valeurs de `Scale`
  `180.0 → 200.8` sur les 56 segments ; aucune `Anchor Point`, aucune
  `Position`, aucun autre champ. L'invariant « le brut n'est pas modifié »
  est respecté.
- **DINOv2 sur `40 FOOTBALLEUR`** : détecte `VIDÉO TIKTOK` en haut à gauche
  sur 4/12 échantillons (score moyen 0,671, seuil 0,50), alors que l'OCR
  texte ne produit aucun finding persistant. C'est le sauvetage open-set
  attendu.
- **OCR sur les bandeaux** : scores 0,979–0,987, boîtes stables à ±2 px
  sur `54`/`58`. La qualité du modèle n'est pas le problème ; c'est
  l'agrégation qui l'exploite mal.
- **OverlayMask** : aucun faux positif de logo sur les bandes hautes des
  quatre projets ; les overlays déclarés ne sont pas signalés.

## 6. Architecture cible

### 6.1 Principes

1. **Conservatisme** : aucun faux positif ne doit modifier un cadrage
   valide. L'OCR ne peut jamais contourner les garde-fous durs de A2.
2. **Additif** : les artefacts `post.json`, `review.json`, `audit.json` et
   `qa.json` restent lisibles ; les nouveaux champs sont sous `details` ou
   dans une version de schéma explicite.
3. **Local et gratuit** : ONNX Runtime CPU, pas de cloud, pas d'API payante.
4. **Une seule source de vérité pour les moteurs** : `frame`, `inference`,
   `ocr`, `dino` ne doivent pas être dupliqués entre `qa` et `post`.
5. **Dégradation gracieuse** : modèle absent ou OCR en échec → comportement
   A2 actuel, pas d'échec du post.

### 6.2 Partage des moteurs : trois options

| Option | Description | Avantages | Inconvénients | Recommandation |
| --- | --- | --- | --- | --- |
| A. Import direct `post → qa` | `post/a2.rs` importe `qa::ocr` et `qa::frame` | effort minimal, même crate | inverse la couche, couple A2 à la logique QA, risque de cycle conceptuel | acceptable pour un spike, pas pour la production |
| B. Module neutre `vision` | déplacer `frame`, `inference`, `ocr`, `dino`, `text` dans `crates/workflow-steps/src/vision/` | layering propre, réutilisation réelle, tests partagés | refactor mécanique, imports à réécrire, validation QA complète | **recommandé** |
| C. Duplication dans `post` | recopier un OCR léger dans `post` | aucun refactor | deux préprocessings à maintenir, dérive de parité | rejeté |

Structure cible :

```text
crates/workflow-steps/src/
  vision/
    mod.rs
    frame.rs        # BgrFrame, grab_frame, resize_bilinear
    inference.rs    # init_ort, cpu_session
    ocr.rs          # DetEngine, RecEngine, decode CTC, filtres communs
    dino.rs         # DinoEngine, preprocessing, probe
    text.rs         # normalize_text, similarité, boîtes
    hash.rs         # sha256_file partagé
  qa/
    ocr.rs          # orchestration QA : bande basse, clusters, findings
    logo.rs         # orchestration QA : bande haute, overlays, DINOv2
  post/
    a2.rs           # orchestration A2 + gate OCR
```

Le déplacement doit être un lot séparé, **sans changement de comportement**,
validé par `cargo test --workspace` et un `qa run` réel sur les 35 exports
de référence. C'est un prérequis pour éviter que `post` dépende de `qa`.

**État au 2026-11-07** : option B implémentée. `vision/` contient `frame`,
`inference`, `ocr`, `dino`, `text` et `hash` ; `qa/frame.rs`,
`qa/inference.rs` et `qa/dino.rs` sont supprimés. Le rejeu QA de `54` donne
des findings, checks, preuves et modèles identiques à la version précédente
(seuls les temps changent) ; 467 tests `workflow-steps`, `fmt` et clippy
verts. La phase 3 (gate OCR A2) est aussi implémentée : voir §6.3.7.

### 6.3 Protocole A2 v3 — gate OCR conservateur

#### 6.3.1 Échantillonnage

- 12 timestamps uniformes sur la durée de la vidéo (même cadence que
  `qa/logo.rs`), ou 6 si le budget l'exige ; les cas du corpus sont à 100 %
  de persistance, donc 6 suffisent pour les valider, mais 12 restent plus
  sûrs pour un bandeau intermittent ;
- crop du candidat A2, avec 16 px de marge sur les quatre côtés, à la
  résolution source ; si le crop fait moins de 30 px de haut, le gate est
  ignoré ;
- OCR det + rec PP-OCRv6, CPU, threads bornés ;
- fusion des boîtes d'une même ligne (centres Y proches, hauteurs
  comparables, écart horizontal borné) avant l'agrégation : PP-OCR découpe
  parfois « PREMIERE PHRASE » en deux boîtes, et le mot constant « PHRASE »
  masquerait le changement de sous-titre ;
- texte normalisé d'au moins deux caractères ;
- si moins de 6 frames sont extraites, le gate est ignoré et la décision
  A2 actuelle s'applique ;
- si le crop est trop petit (< 30 px de haut) ou si l'OCR échoue, ignorer
  le gate et revenir au comportement A2 actuel.

#### 6.3.2 Features calculées

| Feature | Définition | Seuil proposé |
| --- | --- | ---: |
| `text_persistence` | échantillons avec une détection recouvrant le candidat / total | ≥ 0,60 |
| `dominant_ratio` | occurrences du texte normalisé dominant / total | ≥ 0,80 pour confirmer |
| `text_change_rate` | 1 − `dominant_ratio` | > 0,40 = sous-titres probables |
| `y_drift` | max − min du centre Y du groupe dominant, en px source | ≤ 12 px (1080) |
| `ocr_score` | médiane des scores du groupe dominant | ≥ 0,80 |
| `candidate_overlap` | intersection / plus petite boîte, médiane | ≥ 0,50 |
| `coverage_x` | largeur du groupe dominant / largeur du candidat | ≥ 0,25 |
| `median_brightness` | médiane du candidat (déjà calculée) | ≥ 60 pour le niveau 1 |

Les seuils `y_drift` et `GEOM_STD` sont exprimés en pixels 1080 et ramenés
à la hauteur source, comme les seuils A2 existants.

#### 6.3.3 Arbre de décision

```text
candidat bas sélectionné
  │
  ├── garde-fous durs (keyframe, échelles hétérogènes, bande haute,
  │   conflit A1/A2, largeur < 50 %, hauteur hors 24–200, persistance < 60 %,
  │   temporal_std > 35, scale > 300 %) → décision actuelle, OCR interdit
  │   d'y déroger
  │
  ├── OCR indisponible / texte trop rare (persistance < 60 %)
  │     └── décision A2 actuelle, sans dégradation
  │
  ├── texte persistant, dominant ≥ 80 %, y_drift ≤ 12 px, score ≥ 0,80,
  │   overlap ≥ 0,50, couverture ≥ 25 %
  │     ├── bande claire (médiane ≥ 60) et géométrie instable
  │     │   (amplitude ≤ 2× GEOM_STD)
  │     │     └── apply (override OCR), même formule de scale
  │     ├── bande sombre (médiane < 60)
  │     │     └── review enrichi : preuve OCR + scale proposé
  │     └── géométrie déjà valide
  │           └── apply (comme aujourd'hui)
  │
  ├── texte persistant mais dominant < 60 % (texte changeant)
  │     └── review « texte changeant, sous-titres probables ;
  │         pas de recadrage destructif »
  │
  └── preuve OCR ambiguë (dominant 60–80 %, y_drift > 12 px,
      score < 0,80, overlap < 0,50 ou couverture < 25 %)
        └── décision A2 actuelle ; si elle serait apply, downgrade review
```

La fusion des boîtes d'une même ligne précède l'agrégation : sans elle, un
mot commun aux sous-titres (« PHRASE ») dominerait à tort. Le tri du groupe
dominant est déterministe : nombre d'échantillons, puis largeur couverte,
puis texte. Le groupe dominant peut donc rester un fragment fréquent
(« 66 ») ; la couverture < 25 % le classe alors en preuve ambiguë, et
`details.ocr.texts` expose les autres lignes pour la revue.

Le **veto** est la partie la plus importante : si un candidat ressemble à
une bannière par sa géométrie mais que l'OCR lit des textes différents sur
les échantillons, A2 ne doit pas cropper. C'est la protection contre un
film sous-titré en continu, cas où la persistance géométrique serait
trompeuse.

#### 6.3.4 Politique des bandeaux sombres

Les deux bandeaux `40` ont une médiane de 37 et 55, mais un texte blanc
très contrasté (score 0,98). Deux niveaux sont proposés :

- **Niveau 1 (par défaut)** : `median_brightness < 60` → `review`, avec
  `details.ocr` (texte, persistance, score, boîte) et
  `details.scale_target` proposé. L'opérateur peut appliquer manuellement.
- **Niveau 2 (opt-in, non recommandé au premier lot)** : autoriser
  l'application si `dominant_ratio ≥ 0,90`, `ocr_score ≥ 0,90`,
  `text_persistence ≥ 0,90`, `y_drift ≤ 8 px`, `coverage_x ≥ 0,25` et
  `median_brightness < 60`. Ce niveau doit être validé sur un corpus
  dédié de lower-thirds sombres, car il change une politique validée.

Tant que le client n'a pas explicitement approuvé le niveau 2, les
bandeaux sombres restent en `review`. C'est le compromis retenu par D-051.

#### 6.3.5 Candidats étroits (< 35 %)

| Cas | Preuve OCR | Décision |
| --- | --- | --- |
| texte persistant ≥ 80 %, boîte stable | logo/watermark fixe | `review` « logo fixe » (jamais `apply`) |
| texte changeant (dominant < 60 %) | sous-titre mono-ligne | `noop` « sous-titre, pas une bannière » |
| aucun texte | nature indéterminée | `review` « zone étroite » |
| aucun texte + DINOv2 ≥ 0,50 sur ≥ 3/12 (option) | logo visuel | `review` « logo visuel » |

A2 ne doit **jamais** recadrer un candidat étroit : le crop global
détruirait le cadrage pour un logo de coin. L'OCR sert seulement à qualifier
la revue.

#### 6.3.6 Formule de scale et géométrie

La formule existante est conservée :

```text
scale = comp_height / (source_height - height_src) * 100 * 1,05
```

Elle traite implicitement le candidat comme une bande collée au bas de la
source. Dans le corpus, le bord visible après scale est :

| Projet | Scale | Bord source visible | Texte OCR `y0` | Bannière coupée ? |
| --- | ---: | ---: | ---: | :---: |
| 40 ABHARTACH | 218,2 % | 880 px | 925 | oui |
| 40 FOOTBALLEUR | 218,2 % | 880 px | 924 | oui |
| 54 CLIENT HOTEL | 200,8 % | 956 px | 980 | oui |
| 58 CROUSTY | 210,0 % | 914 px | 979 | oui |

Le bord visible est toujours au-dessus du texte, donc la bannière est bien
supprimée. Le prix est un sur-crop de 24–65 px de scène au-dessus. Une
formule plus précise utiliserait `y0` du texte, mais elle changerait les
valeurs validées sur 187 et n'est pas nécessaire pour la sécurité. Elle est
laissée à une étude ultérieure.

#### 6.3.7 Validation de la phase 3 (2026-11-07)

Le gate est implémenté dans `post/a2.rs` derrière `a2_ocr = false`. Les
modèles et le runtime sont configurables (`a2_ocr_det`, `a2_ocr_rec`,
`a2_ocr_onnxruntime`) ; les chemins relatifs sont résolus depuis
`paths.models`. Les moteurs sont ceux du module `vision` (phase 2).

Rejeu réel sur copies `/tmp`, avec `--report-only` puis run complet :

| Projet | Décision | Texte OCR dominant | Projet assisté |
| --- | --- | --- | --- |
| 54 CLIENT HOTEL | `apply` 200,8 % (inchangé) | « FAUX CLIENTS, VRAIS ENQUÊTEURS : RIEN NE LEUR ÉCHAPPE » (12/12, 100 %, couverture 80 %) | 112 diffs, toutes `Scale 180,0 → 200,8` ; audit `pass` |
| 58 CROUSTY | `review` → `apply` 210,0 % | même texte (12/12, 100 %, couverture 80 %) | 40 diffs, toutes `Scale 180,0 → 210,0` ; audit `pass` |
| 40 ABHARTACH | `review` conservé | fragment « 66 » (12/12, couverture 11 %) ; texte complet « DERRY: GRAND FRISSON EN IRLANDE » dans `texts` | aucun changement |
| 40 FOOTBALLEUR | `review` conservé | fragment « 66 » (12/12, couverture 9 %) ; texte complet dans `texts` | aucun changement |
| sub_fast synthétique | `review` « texte changeant » | 4 lignes de sous-titres, dominance 25 % | aucun changement |

Deux défauts ont été trouvés et corrigés pendant cette validation :

1. **Découpage OCR des lignes** : PP-OCR lit « PREMIERE » et « PHRASE »
   dans deux boîtes distinctes. Sans fusion, « PHRASE » est présent dans
   10/12 échantillons et confirme à tort un bandeau fixe. La fusion des
   boîtes d'une même ligne ramène la dominance à 25 % et déclenche le veto.
2. **Fragments et logos** : une boîte de logo plus haute que le texte
   (« UNDOC », 60 px contre 38 px) ne doit pas être fusionnée avec la
   bannière. La fusion exige un rapport de hauteur ≤ 1,35.

Le coût mesuré est de 15–25 s par projet (12 frames, 2 threads), sans
verrou matériel. Modèles absents ou OCR en échec : warning et décision A2
actuelle, jamais d'échec du post. La phase 3 est couverte par 26 tests
`post::a2` (features, seuils, veto, garde-fous, fusion de lignes).

### 6.4 QA v1.3 — clustering par détection

Le correctif QA est indépendant de A2 et peut être livré en premier.

#### 6.4.1 Algorithme

1. Pour chaque échantillon, conserver les détections filtrées
   (`text`, `score`, `bbox`, `time`).
2. Créer des clusters par **boîte stable** (containment, IoU ≥ 0,3, ou
   centres proches sur la même ligne avec des tailles comparables), comme
   `qa/logo.rs::bbox_similarity` mais sans exiger la même bande. Le texte
   normalisé n'est **pas** une clé de regroupement : deux sous-titres
   successifs partagent la même boîte et doivent rester dans le même
   cluster pour que leur dominance basse les classe en sous-titre.
3. Pour chaque cluster : `count`, `first`, `last`, `persistence =
   count/total`, `dominant_ratio` (occurrences du texte normalisé dominant
   / `count`), `y_drift`, `mean_score`.
4. Un cluster devient un finding si :
   - `count ≥ 3` **et** `y_drift ≤ 0.04 × height` ; et
   - soit `persistence ≥ 0.60` et `dominant_ratio ≥ 0.80` →
     `fixed_overlay` ;
   - soit `persistence < 0.60` ou `dominant_ratio < 0.80` →
     `subtitle_burned_in` (texte changeant).
5. Les clusters à 1–2 occurrences ne produisent pas de finding (bruit de
   décor, sous-titre isolé).

#### 6.4.2 Conséquences sur le corpus

| Projet | QA actuel | QA v1.3 |
| --- | --- | --- |
| 40 ABHARTACH | `subtitle_burned_in` (0,978) | `fixed_overlay` (review) |
| 40 FOOTBALLEUR | aucun finding sous-titre ; logo DINOv2 | `fixed_overlay` + logo DINOv2 |
| 54 CLIENT HOTEL | aucun finding sous-titre | `fixed_overlay` (review) + `subtitle_burned_in` (3 échantillons) |
| 58 CROUSTY | aucun finding sous-titre ; logo texte haut | `fixed_overlay` + logo texte haut |

Les sous-titres transitoires de `58` (1 occurrence) ne produisent toujours
pas de finding, ce qui est le comportement attendu.

#### 6.4.3 Contrat et versionnement

- `DefectType::FixedOverlay` avec `as_str = "fixed_overlay"`, non bloquant,
  `review`.
- `subtitle_ocr` reste `pass` si seuls des `fixed_overlay` sont trouvés ;
  un nouveau check `fixed_overlay` porte le `review`.
- `overall_status` reste `review` dès qu'un finding existe, donc
  `auto_approve_eligible = false` pour un bandeau fixe : l'intentionnalité
  (habillage de chaîne ou défaut) reste humaine.
- `QA_SCHEMA_VERSION` passe à **1.3.0**, `CACHE_VERSION` à **7** (le cache
  inclut déjà `schema`, mais l'incrément explicite documente le changement).
- `summary.json` n'a pas besoin de changer : les findings sont déjà
  sérialisés par export.

### 6.5 Impact performance

Mesures QA existantes (CPU, 8 threads, source 1920×1080) :

| Projet | Durée | OCR bas | Logos | DINOv2 | Visage | Total |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 40 ABHARTACH | 115 s | 37,9 s | 11,3 s | 0,8 s | 13,3 s | 66,1 s |
| 40 FOOTBALLEUR | 199 s | 57,6 s | 10,1 s | 0,6 s | 23,8 s | 91,5 s |
| 54 CLIENT HOTEL | 216 s | 71,4 s | 8,6 s | 0,3 s | 31,9 s | 106,8 s |
| 58 CROUSTY | 96 s | 18,1 s | 8,1 s | 0,7 s | 8,0 s | 37,9 s |

L'OCR bas coûte **~1,0–1,3 s par échantillon** : détection 0,5–0,7 s,
reconnaissance 0,1–0,3 s, extraction de frame 0,3–0,4 s. Le coût de
détection domine parce que le crop large est remis à l'échelle sur un côté
court de 480 px, ce qui produit un tenseur très large. Pour A2, 6–12 crops
coûteraient donc **~6–15 s** par projet, et A2 passerait de ~4,8 s à
~11–20 s. Le post total resterait sous ~40 s pour les projets du corpus.
Une extraction groupée des frames et un plafond de taille du tenseur
peuvent réduire ce coût, mais doivent être mesurés avant optimisation.
DINOv2 n'est pas nécessaire dans A2 au premier lot ; s'il est ajouté pour
les candidats étroits, compter 0,3–0,8 s de plus.

### 6.6 Impact contrat

- `post.json` : ajout de `details.ocr` (objet) et éventuellement
  `details.scale_target` sur les reviews sombres. Les clés existantes et
  leur ordre ne changent pas ; le schéma `post.json` reste `1`.
- `review.json` : la raison et les détails sont enrichis ; la sévérité
  reste `high` pour A1/A2.
- `qa.json` : nouveau `defect_type` et nouveau check, donc **schéma 1.3.0**
  explicite. Les consommateurs doivent accepter un type inconnu ou être
  mis à jour ; `auto_approve_eligible` reste faux.
- `config.toml` : nouveaux flags `a2_ocr` (défaut `false`), `a2_ocr_det`,
  `a2_ocr_rec` et `a2_ocr_onnxruntime` (optionnel). `deny_unknown_fields`
  impose de documenter ces clés dans le contrat. Le niveau 2 sombre n'est
  pas implémenté : aucune clé `a2_ocr_dark` n'existe.
- CLI : aucun changement d'interface ; `--no-ocr`/`--no-logo` restent
  spécifiques au QA.

### 6.7 Plan de tests

**Tests unitaires (sans modèle) :**

- fonction de décision A2 v3 avec des features synthétiques :
  - bandeau clair stable + géométrie instable → `apply` ;
  - bandeau sombre stable → `review` avec scale proposé ;
  - texte changeant → `review`/veto, jamais `apply` ;
  - candidat étroit texte stable → `review` logo ;
  - candidat étroit texte changeant → `noop` ;
  - OCR absent → décision A2 actuelle ;
  - scale > 300 → `review` ;
- clustering QA : bandeau fixe + sous-titres concurrents → un
  `fixed_overlay`, zéro `subtitle_burned_in` ; sous-titres seuls → un
  `subtitle_burned_in` ; texte changeant → `subtitle_burned_in`.

**Tests réels (hors CI, modèles requis) :**

- rejouer `qa run` sur les quatre projets et vérifier les findings ;
- rejouer `post run --report-only` sur des copies `/tmp` et comparer les
  décisions A2 avant/après ;
- vérifier que `54` reste `apply` à 200,8 %, que `58` passe à `apply`
  210,0 %, que `40` restent `review` avec preuve OCR ;
- vérifier l'invariant brut/assist : aucune différence hors `Scale` ;
- mesure de latence A2 avant/après.

**Critères d'acceptation :**

- 0 faux positif de recadrage sur les exports GOOD connus ;
- aucun `apply` sur un candidat dont le texte change ;
- `58` promu, `54` inchangé, `40` enrichis ;
- `qa.json` 1.3.0 valide, `auto_approve_eligible` faux pour tout
  `fixed_overlay` ;
- 442+ tests, `fmt`, `clippy -D warnings` verts.

## 7. Prototype et validation

### 7.1 Prototype analytique

Un script Python hors dépôt (`/tmp/workflow_investigation/analyze_ocr_a2.py`)
a parsé les logs OCR, regroupé les détections par texte normalisé et boîte,
et confronté chaque groupe au candidat A2. Il ne modifie aucun fichier du
dépôt et ne remplace pas un test Rust : il sert à vérifier que les features
proposées sont mesurables et discriminantes.

Résultats saillants :

- le texte dominant recouvre le candidat A2 à **100 %** dans les quatre
  cas (`overlapA2 = 1.00`) ;
- la persistance est **0,963–1,000**, la dérive Y **2–9 px**, le score
  **0,979–0,987** ;
- les sous-titres transitoires sont à **1–2 occurrences** sur 20–42
  échantillons, donc très en dessous du seuil de confirmation ;
- le ratio de textes distincts par échantillon est **1,00–1,33**, ce qui
  montre que le bandeau est le bloc dominant, même quand un sous-titre
  apparaît.

### 7.2 Décisions simulées A2 v3

| Projet | Actuel | Preuve OCR | Médiane | Décision v3 | Scale |
| --- | --- | --- | ---: | --- | ---: |
| 40 ABHARTACH | `review` | 96,3 %, dérive 7 px | 37 | `review` sombre + preuve | 218,2 % proposé |
| 40 FOOTBALLEUR | `review` | 100 %, dérive 9 px | 55 | `review` sombre + preuve | 218,2 % proposé |
| 54 CLIENT HOTEL | `apply` 200,8 % | 100 %, dérive 2 px | 97 | `apply` 200,8 % (inchangé) | 200,8 % |
| 58 CROUSTY | `review` | 100 %, dérive 2 px | 108 | **`apply` 210,0 %** | 210,0 % |

Cette simulation respecte la politique conservatrice : une seule nouvelle
application automatique (`58`), aucun recadrage sur les bandeaux sombres,
et deux revues enrichies avec un scale prêt à l'emploi. Si le client
approuve le niveau 2, `40` pourraient aussi être appliqués à 218,2 %, mais
ce n'est pas la recommandation par défaut.

### 7.3 Ce que le prototype ne prouve pas

- il ne remplace pas un test Rust du moteur OCR dans A2 ;
- il ne couvre que quatre projets, majoritairement de la même famille
  éditoriale (`66 MINUTES`, `UN JOUR UN DOC`) ;
- il ne teste pas les logos non textuels dans A2 ;
- il ne valide pas le niveau 2 sur des lower-thirds sombres variés ;
- il ne remplace pas une validation humaine des décisions `apply`.

### 7.4 Implémentation QA 1.3.0 (2026-11-07)

Le premier lot (QA v1.3) est implémenté dans `qa/ocr.rs`, `qa/model.rs` et
`qa/cache.rs` : agrégation par boîte stable, `DefectType::FixedOverlay`,
check `fixed_overlay`, schéma **1.3.0**, cache **7**. Le texte normalisé
sert à la dominance mais n'entre pas dans la clé de regroupement, sinon
deux sous-titres successifs dans la même boîte produiraient deux clusters
au lieu d'un `subtitle_burned_in` à dominance basse.

Rejeu réel sur les quatre rushes, modèles et ONNX Runtime de production,
sorties sous `/tmp/workflow_investigation/qa13b/` :

| Projet | `fixed_overlay` | `subtitle_burned_in` | Persistance / dominance |
| --- | :---: | :---: | --- |
| 40 ABHARTACH | 1 | 0 | 100 % / 96 %, score 0,978 |
| 40 FOOTBALLEUR | 1 | 0 | 100 % / 100 %, score 0,984 |
| 54 CLIENT HOTEL | 1 | 1 | 100 % / 100 %, score 0,984 ; sous-titre temporaire à 3 échantillons |
| 58 CROUSTY | 1 | 0 | 100 % / 100 %, score 0,983 |

Le `subtitle_burned_in` de `54` est un vrai positif supplémentaire : trois
échantillons lus au-dessus du bandeau (`est sympatique, proactif,` / `les
demandes des clients.` / `Pourquoi elle ne fait pas le café ici ?`), que
l'ancienne moyenne de bloc supprimait. Les clips synthétiques confirment le
partage : `sub_fast` (texte changeant toutes les 3 s) donne un
`subtitle_burned_in` à dominance 29 %, `181_sub_center` (texte fixe) donne
un `fixed_overlay`. Un texte statique est donc classé `fixed_overlay` même
s'il vient d'un sous-titre : la catégorie décrit la persistance, pas
l'intention éditoriale.

Lot de non-régression sur le corpus GOOD, OCR seul
(`--no-face --no-logo --no-logo-visual`, 4 workers, 36 exports hors un exclu) :
**36 `pass`, 0 finding, 0 `fixed_overlay`, 0 `subtitle_burned_in`**. Les
quelques textes isolés du corpus restent sous le seuil de trois échantillons
d'un même cluster. Sorties sous
`/tmp/workflow_investigation/corpus35-qa13/`.

### 7.5 Phases A/B/C — registre spatial des logos source (2026-11-08)

**Principe.** Le logo n'est pas recherché sur toute l'image : le client a
documenté, pour 29 émissions, une position quasi déterministe dans le canvas
source 1920×1080. Le pipeline répond d'abord à « où peut être le logo ? »
(registre spatial), puis seulement ensuite à « est-il réellement là ? »
(confirmation locale). La phase A annote cette position ; la phase B exclut
le texte du logo du groupe OCR dominant et interdit qu'il confirme seul une
bannière ; la phase C compare les pixels de l'export au crop de référence et
mesure la stabilité des zones génériques. Aucune des trois **n'agit jamais**
sur le recadrage : le pire cas est une revue.

**Catalogue.** `assets/models/qa/source_logos.toml` est versionné et généré
par `scripts/import-source-logos.py` depuis
`SELECTION_VIDEOS_SOURCES_LOGOS.csv` et les crops RGBA du client. Il contient
29 émissions (28 en bas-gauche, Piquantes ! en haut-gauche), leurs bboxes
demi-ouvertes, leurs variantes, 15 clean feeds sans logo et 5 émissions sans
logo à ignorer. Les crops restent hors dépôt ; le catalogue ne contient que
des métadonnées auditables. `sources` reprend les noms de fichiers du CSV :
il prime sur les alias, ce qui évite de classer « Kamila ne sait pas écrire
Marseillais » comme Les Marseillais alors que c'est Le Cross.

**Projection.** `Segment::project_bbox` reprend la formule A1/A5
(`comp = position + parent_dx + (source - anchor) * scale / 100`) et
interpole les clés de Position, Anchor et Scale. Un parent animé retourne
`None` : mieux vaut ne rien annoter qu'annoncer une visibilité fausse. La
visibilité est la part de la bbox **projetée** qui reste dans la composition,
et non le rapport des surfaces source/projetée.

**A2.** Derrière `a2_logo_prior = false`, la décision reste strictement
identique. Quand le flag est actif, `details.source_logo` documente le nom,
la chaîne, la bbox source, `logo_in_candidate`, `candidate_covered`,
`visible_in_comp` et la bbox projetée. Rejeu réel avec OCR : `54` reste
`apply` à 200,8 %, `58` reste `apply` à 210,0 %, `40 ABHARTACH` et
`40 FOOTBALLEUR` restent `review` avec le logo 66 Minutes documenté à 72,6 %
du candidat. Les seules différences sont le nouveau champ et les temps OCR.

**QA.** `qa run --source-logo` ajoute le check `source_logo` (`pass` ou
`info`, jamais bloquant) et exige un `.ecproj`. Sans émission reconnue, le
check rappelle les zones génériques du catalogue sans les confirmer. Rejeu
sur 35 exports GOOD avec leur `.ecproj` : 25 `pass`, 10 `info`, **0 nouveau
finding** ; `227` E=M6 est hors cadre, `270` GOAL est reconnu clean feed.
Deux exports (`286`, `298`) conservent un écart de durée MP4/comp de 3 s déjà
présent dans l'appariement projet/export, sans rapport avec le registre.

**Limites des phases A/B/C.** La confirmation visuelle est une corrélation
globale : un logo animé, une variante non cataloguée ou un reflet peuvent
rester non confirmés, et un fond statique peut déclencher la stabilité d'une
zone générique. Les seuils sont calibrés sur les références, les cas
synthétiques, quatre rendus H.264 construits depuis les rushes et les
transformations réelles, et les 35 GOOD ; ils ne sont pas encore confrontés
à des exports AE produits par l'opérateur. A2 lit désormais les overlays
déclarés (veto logo seul neutralisé si le logo est caché) mais ne consulte
pas la confirmation visuelle NCC ; DINOv2 sur les scores résiduels et une
confirmation A2 restent optionnels (phase D). Une rotation ou un parent
animé/redimensionné rend la projection non calculable : le QA répond alors
`not_applicable` au lieu de conclure « hors cadre », et A2 omet
`visible_in_comp` avec `projection = "unsupported"`.

**Phase B — exclusion OCR et veto logo seul (2026-11-08).** Quand le prior
est disponible, les détections OCR dont la boîte est majoritairement
(≥ 60 %) dans la bbox du logo sont retirées du groupe dominant. Les textes
retirés sont exposés sous `details.ocr.logo_texts`, avec
`logo_detections_total` et `logo_only` ; ces trois clés n'apparaissent que
si un prior est actif, pour ne pas modifier la sortie OCR sans registre. Si
la seule preuve textuelle est le logo (`logo_only`), A2 oppose un `review`
« texte du logo source uniquement » au lieu d'appliquer. Le reste de la
décision est inchangé : les garde-fous de géométrie, de largeur et de bande
sombre continuent de primer.

**Phase B — overlays déclarés (2026-11-08).** A2 lit le masque des overlays
image déclarés dans le `.ecproj` via le module partagé `post::overlay` (le
même que le QA). Si, aux douze instants du gate OCR, toutes les occurrences visibles du
logo projeté sont couvertes à ≥ 90 % par un overlay actif, `details.source_logo` ajoute
`overlay_covered` et `hidden_by_overlay`, passe `visible_in_comp` à `false`,
et `details.ocr` expose `logo_visible_samples`, `logo_overlay_samples` et
`logo_hidden_by_overlay`. Le veto `review` « logo seul » est alors remplacé
par la décision géométrique A2 : un logo que l'habillage cache ne doit pas
bloquer l'export. L'exclusion OCR du texte du logo reste inchangée — elle
porte sur la source, où l'overlay n'existe pas encore.

Rejeu sur `54` avec un habillage synthétique déclaré couvrant le logo :
`apply` 200,8 % inchangé, `visible_in_comp=false`, `overlay_covered=1.0`,
2/2 occurrences visibles masquées, `logo_hidden_by_overlay=true`. Sans
habillage, la sortie est identique à la phase B, clés d'overlay en moins.

Rejeu réel : `54` et `58` restent `apply` 200,8 % / 210,0 %, mais
`UNJOUR`/`UNDOC` passent de `texts` à `logo_texts` ; `40 ABHARTACH` et
`40 FOOTBALLEUR` restent `review` et passent du motif « bande animée » au
motif « bande sombre, texte persistant confirmé », avec la bannière réelle
dominante (91,7 %) et `scale_target` 218,2 % ; `sub_fast` reste vétoé à
25 %. Aucune action ni aucun scale de `54`/`58` ne change, les audits
restent `pass`. Le flag reste désactivé par défaut ; l'activation en
production attend la validation client.

**Phase C — confirmation locale (2026-11-08).** `--source-logo-visual`
(impliqué par `--source-logo-references`) compare jusqu'à six instants de
l'export au crop RGBA de référence, toutes variantes confondues. Le score
combine un NCC couleur calculé sur le masque alpha (65 %) et un NCC des
contours (35 %) ; un échantillon est fort si couleur ≥ 0,45, contours
≥ 0,10 et score ≥ 0,50, et il faut au moins deux échantillons forts pour
parler de logo confirmé. Les crops restent externes ; le chemin fourni peut
être le dossier `emissions` ou son parent `references_logos`. Si le format
de l'export diffère de la composition (rush 16:9 évalué avec une comp 9:16),
la comparaison est abandonnée avec une raison explicite.

Pour une source inconnue, le QA projette les zones génériques et mesure leur
stabilité temporelle : écart-type ≤ 12/255, au moins 5 % de pixels stables,
contours persistants ≥ 0,8 % et densité de contours ≥ 4 %. Une zone stable
produit un `info` « logo possible non identifié », jamais une identité ou un
`review`.

Validation : frame de référence 66 Minutes → NCC 0,97 (3/3) ; export 9:16
synthétique construit avec la transformation réelle du projet UJUD → NCC
0,96 (3/3) ; quatre rendus H.264 1080×1920 obtenus depuis les rushes de
production et les transformations réelles des projets (`54` à 148,68 s,
`58` à 49,16 s, `40 ABHARTACH` à 8,36 s, `40 FOOTBALLEUR` à 45,64 s) →
NCC 0,95 / 0,94 / 0,64 / 0,58, 3/3 à chaque fois ; le même rendu UJUD
évalué avec le prior 66 Minutes reste non confirmé à 0,07 ; image statique
inconnue → zone générique stable à 100 % (contours 19,6 %). Sur les 35
exports GOOD appariés : 25 `pass`, 10 `info`, **0 nouveau finding**,
aucune zone générique stable sur les sources inconnues. Un balayage
exhaustif de six des dix exports `info` (UJUD ×4, 66M ×2) à 1 i/s ne trouve
aucune frame au-dessus de 0,50 (meilleur 0,38 sur une bande de 24 px) :
pour ces six exports, l'échantillonnage n'est pas la cause. Les quatre
autres `info` (270 Gulli, 3 W9 LAA9) n'ont pas été balayés à 1 i/s.

Ces `info` ne signifient pas que le logo manque dans le rush : la
vérification manuelle des rushes UJUD et 66M le montre présent sur toute la
durée. Le check compare l'export final 1080×1920 au crop de référence du
logo **source**, à la position géométriquement projetée. Or le logo social
est ajouté manuellement en overlay ; la zone projetée peut donc ne pas
contenir le logo source (logo manuel ailleurs, recadrage, habillage). Un
`info` « logo non confirmé visuellement » signifie « logo source non
retrouvé à cette position dans l'export analysé », pas « logo absent du
rush » ni « export sans logo ». Le check ne valide pas l'overlay manuel.

**Critère pour la phase D.** Ajouter DINOv2 uniquement sur les scores
résiduels (0,35–0,55) et une confirmation A2 avant exclusion OCR seulement
si un corpus d'exports réellement porteurs du logo le justifie. Toujours
sans recadrage automatique.

## 8. Risques et points ouverts

1. **Rushes vs exports finaux.** Les replays A2/QA sur `54`, `58` et `40`
   utilisent les rushes 1920×1080, pas les exports sociaux 1080×1920 ; le
   corpus GOOD, lui, contient bien des exports 1080×1920. Un
   `resolution: fail` observé sur un rush n'est pas un défaut de production.
   Pour le check `source_logo`, l'image analysée est comparée au logo
   **source** projeté, pas à l'overlay ajouté manuellement.
2. **Bandeaux sombres.** La médiane de bande est un mauvais indicateur de
   contraste texte/fond. Le niveau 2 doit être validé sur un corpus dédié
   avant toute activation.
3. **Bandeaux sans texte.** Un logo purement graphique en bas de cadre ne
   sera pas confirmé par l'OCR. Il reste en `review`, ou peut être qualifié
   par DINOv2 en option. Ne jamais l'appliquer automatiquement.
4. **Sur-crop.** La formule actuelle enlève 24–65 px de scène au-dessus de
   la bannière. C'est acceptable pour la sécurité, mais une formule basée
   sur `y0` du texte pourrait réduire le zoom ; elle changerait les valeurs
   validées et doit être étudiée séparément.
5. **Multi-source.** Le corpus ne contient qu'une source par composition.
   A2 applique un scale global ; sur une composition avec PiP ou split
   screen, il faudrait vérifier que le scale ne déforme pas la seconde
   source. Le gate « échelles hétérogènes » ne couvre pas ce cas.
6. **Licences.** Les dépôts YOLO sont sans licence ou copyleft
   (GPL-3.0/AGPL-3.0), Logodetect est AGPL-3.0. Aucun code de ces dépôts
   ne doit être copié dans le dépôt propriétaire.
7. **Modèles hors dépôt.** Les binaires OCR/DINOv2 sont locaux et
   gitignorés ; un poste neuf sans `--models` verra `subtitle_ocr` et
   `logo_visual` en `not_applicable`. Le manifeste et les SHA-256 doivent
   rester à jour.
8. **Schéma QA.** L'ajout de `fixed_overlay` est une rupture de schéma
   mineure ; les consommateurs de `qa.json` doivent être vérifiés avant le
   passage en production.
9. **Coût CPU.** L'OCR A2 ajoute 1,5–3 s par projet ; sur un lot de 40
   projets, cela peut représenter 1–2 minutes. Le cache QA n'aide pas A2,
   qui s'exécute dans le post.

## 9. Recommandation et suite

### 9.1 Décision D-051

1. **Ne pas importer** les sept dépôts. La pile Rust existante reste la
   référence : PP-OCRv6 + DINOv2 + OverlayMask + persistance temporelle.
2. **QA v1.3** : corriger l'agrégation OCR par clustering par détection,
   ajouter `fixed_overlay`, schéma 1.3.0, cache 7. C'est le correctif le
   plus rentable et le plus sûr ; il ne touche pas A2.
3. **A2 v3** : ajouter un gate OCR derrière `a2_ocr = false` par défaut.
   Le gate peut confirmer un bandeau clair stable et opposer un veto aux
   textes changeants. Les bandeaux sombres restent en `review` avec preuve
   et scale proposé (niveau 1). Le niveau 2 est une option ultérieure.
4. **Refactor `vision`** : extraire `frame`, `inference`, `ocr`, `dino` et
   `text` dans un module neutre avant de brancher A2 sur l'OCR. Le faire
   sans changement de comportement, validé par la suite QA.
5. **Aucun changement de production** dans cette étude : les artefacts
   existants et la suite de tests (442 au moment de l'étude, 467 après les
   phases 1–3) restent la référence.

### 9.2 Phasage proposé

| Phase | Contenu | Critère de sortie |
| --- | --- | --- |
| 0 (fait) | Audit, benchmark, diagnostic corpus, prototype analytique | rapport + D-051 |
| 1 (fait) | QA v1.3 clustering + `fixed_overlay` | 4/4 bandeaux persistants classés `fixed_overlay` ; 36 GOOD sans finding |
| 2 (fait) | Module `vision` sans changement de comportement | 447 tests + QA réel identique |
| 3 (fait) | A2 v3 gate OCR, flag `a2_ocr=false` | `58` promu à 210,0 %, `54` inchangé à 200,8 %, `40` reviews enrichis, `sub_fast` veto ; audits `pass` |
| 4 (option) | Niveau 2 bandeaux sombres, DINOv2 candidats étroits | validation client + corpus dédié |
| 5 (fait, phase A) | Registre spatial des logos source, annotation A2/QA opt-in | 29 émissions + zones, `54`/`58` inchangés, `40` enrichi, 0 nouveau finding GOOD |
| 6 (fait, phase B) | Exclusion OCR du logo source, veto logo seul | `54`/`58` inchangés, `40` confirmé sombre avec scale 218,2 %, `sub_fast` toujours vétoé ; opt-in et validation client avant activation |
| 6 bis (fait) | A2 lit les overlays déclarés, veto logo seul neutralisé si le logo est caché | `54` de base inchangé ; avec habillage déclaré, `visible_in_comp=false` et 2/2 occurrences masquées |
| 7 (fait, phase C) | Confirmation NCC/contours et stabilité des zones génériques | synthétique 66M/UJUD confirmé, négatif 0,05–0,07, 35 GOOD sans nouveau finding |
| 8 (option) | DINOv2 sur scores résiduels, confirmation A2 avant exclusion OCR | corpus d'exports réellement porteurs, validation client, toujours sans recadrage |

### 9.3 Prochaines actions concrètes

- faire valider humainement les quatre cas du corpus (54, 58, 40 ×2) avant
  d'activer `a2_ocr` en production ; le flag reste `false` par défaut ;
- mesurer le coût du gate sur un lot complet (15–25 s par projet mesuré) et
  vérifier le timeout STEP8 ;
- décider du niveau 2 des bandeaux sombres et du volet logo avec le client ;
  les phases A, B et C du registre spatial sont implémentées mais restent
  opt-in (`a2_logo_prior = false`, `--source-logo`, `--source-logo-visual`) ;
- si le niveau 2 est retenu, constituer un corpus dédié de lower-thirds
  sombres et de logos bas, puis rouvrir une décision ;
- faire valider par le client l'exclusion du logo source du groupe OCR
  dominant et le veto logo seul, déjà mesurés sur `54`/`58`/`40`/`sub_fast` ;
  aucune régression d'action ni de scale sur `54`/`58` ;
- phase C : mesurer la confirmation visuelle sur des exports AE réellement
  produits par l'opérateur (les rendus H.264 construits depuis les rushes
  confirment déjà 0,58 à 0,95 ; les 35 GOOD ne présentent pas le logo source
  à la position projetée, le logo social étant ajouté manuellement en
  overlay), puis ajuster les seuils si nécessaire ;
- phase D optionnelle : DINOv2 sur les scores résiduels et confirmation A2
  avant exclusion OCR, après validation client ;
- conserver la parité QA : toute évolution du clustering ou des moteurs
  `vision` doit rejouer les 36 exports GOOD et les cas synthétiques.

## 10. Références

### 10.1 Code

- `crates/workflow-steps/src/post/a2.rs`
- `crates/workflow-steps/src/post/detect.rs`
- `crates/workflow-steps/src/post/run.rs`
- `crates/workflow-steps/src/qa/ocr.rs`
- `crates/workflow-steps/src/qa/logo.rs`
- `crates/workflow-steps/src/post/overlay.rs`
- `crates/workflow-steps/src/qa/aggregate.rs`
- `crates/workflow-steps/src/qa/model.rs`
- `crates/workflow-steps/src/qa/cache.rs`
- `crates/workflow-steps/src/vision/dino.rs`
- `crates/workflow-steps/src/vision/ocr.rs`
- `crates/workflow-steps/src/vision/source_logo.rs`
- `crates/workflow-steps/src/qa/source_logo.rs`
- `scripts/import-source-logos.py`, `assets/models/qa/source_logos.toml`
- `crates/workflow-cli/src/main.rs`
- `config/config.toml`, `assets/models/qa/manifest.toml`

### 10.2 Dépôts étudiés

- <https://github.com/njc44/Logo_detection-with-YoloV7>
- <https://github.com/Azmeer-59189/logo-detector-api>
- <https://github.com/kunjmehta/logo-detection-and-classification>
- <https://github.com/Heldenkombinat/Logodetect>
- <https://github.com/timminator/VideOCR>
- <https://github.com/KareemMoneeam/Video-Subtitles-Detector>
- <https://github.com/Aalaa4444/Video-Subtitles-Detector>

### 10.3 Modèles et licences

- RapidOCR / PP-OCRv6 small, Apache-2.0 :
  <https://github.com/RapidAI/RapidOCR>
- DINOv2 small quantifié, Apache-2.0 :
  <https://huggingface.co/onnx-community/dinov2-small>
- YuNet 2023mar, MIT :
  <https://github.com/opencv/opencv_zoo/tree/main/models/face_detection_yunet>

### 10.4 Preuves locales

- QA : `/tmp/workflow_investigation/qa/{40_abh,40_foot,54,58}/qa.json`
- OCR debug : `/tmp/workflow_investigation/ocrdebug/{40_abh,40_foot,54,58}/run.log`
- Frames : `/tmp/workflow_investigation/frames/`
- Médianes : `/tmp/workflow_investigation/median/`
- Prototype : `/tmp/workflow_investigation/analyze_ocr_a2.py`

Ces chemins sont des preuves de session, hors dépôt ; ils ne sont pas
contractuels et peuvent être supprimés.
