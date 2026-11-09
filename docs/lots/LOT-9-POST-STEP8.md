# Lot 9 — post-traitement A5/A6 après STEP8

> Statut : intégré à la production le 2026-10-27 ; bypass direct SDM ajouté
> le 2026-10-28, bypass archive-only LPHDF le 2026-10-29, gate OCR A2
> opt-in ajouté le 2026-11-07.
> Décisions : **D-030** (A3 en review, A1/A2 durcis), **D-031** (A5 v2,
> hash échantillonné, A6 mesuré), **D-032** (écriture du projet assisté),
> **D-034** (bypass direct des vidéos SDM), **D-051** (QA 1.3, module
> `vision`, gate OCR A2).
> Code : `crates/workflow-steps/src/post/`, hook dans
> `crates/workflow-steps/src/step8.rs`, commande `workflow-cli post`.

## Problème

STEP8 produit un projet brut `<vidéo>_9x16.ecproj`. Le travail humain qui
reste est majoritairement mécanique : retirer un letterbox noir (A1), retirer
une bannière basse pleine largeur (A2), appliquer le template SDM (A6). Ce
lot automatise ces gestes **sans jamais écraser le brut** ni les artefacts
STEP4/5/6.

Le post-traitement est volontairement conservateur : il préfère une revue à
une modification douteuse. A3 (keyframes) et A4 (transitions) ne sont jamais
appliqués.

## Invariants

- Le `.ecproj` brut est **octet pour octet inchangé**. Un second projet
  `<vidéo>_9x16_assist.ecproj` est **toujours** publié à côté. Sans décision
  applicable, c'est une copie exacte du brut.
- Les artefacts sont **additifs** : `post.json`, `review.json`, `audit.json`
  sous `<docs>/post/<projet_assisté>/`. Aucun `_ae.json`, `_audio.json`, CSV
  ou `.ecproj` brut n'est modifié.
- A1/A2 ne déplacent jamais l'**Anchor Point** STEP6 ; ils changent `Scale`
  et, pour A1, `Position Y`.
- A3 et A4 restent en `review` (D-030).
- Si A1 et A2 sont applicables en même temps, aucune des deux n'est
  appliquée : tout passe en `review` (`a1_a2_conflict`).
- Si A6 est éligible, il remplace le projet : A1–A4 ne sont pas exécutés.
  Si le préfixe SDM est présent mais la structure incompatible, A1–A4
  tournent quand même et A6 part en `review`.
- Le post-traitement ne modifie pas `MediaSolution.jsx` ; `assets/web` n'est
  touché que par l'affichage de sa progression (`progress_phase = "post"`).
  Les artefacts sont consultables en lecture seule sur disque.
- Les écritures JSON utilisent temporaire voisin + `rename`.

## Pipeline

1. Charger le projet, la composition, les segments, la vidéo et `ffprobe`.
2. Valider A6 (préfixe + structure).
3. Si A6 est éligible : appliquer le template, sauter A1–A4.
4. Sinon : contrôle de couverture source (A5), A1, A2, A3, A4, puis décision
   A6 (`review` si préfixe présent mais structure incompatible).
5. Résoudre le conflit A1/A2, appliquer A1 ou A2, publier le projet assisté.
6. Écrire `post.json`, `review.json`, `audit.json` et calculer le verdict.

`post.json` contient les décisions, les timings par étage, les specs vidéo,
la composition et le bloc `template` (seulement si le préfixe A6 correspond).
`review.json` ne contient que les décisions `review`, avec leur sévérité.
`audit.json` contient les contrôles structurels, les SHA-256 du brut et du
projet assisté, et un hash **échantillonné** de la vidéo (3 × 4 Mio au début,
au milieu et à la fin, plus taille et offsets).

## A1 — letterbox noir

- `cropdetect=24:16:0` par segment, sur `start + 0,05 s` et une durée bornée
  à `[0,3 s ; 4 s]`.
- Hauteur active = 10e percentile des hauteurs de crop du segment.
- Les seuils sont exprimés en pixels 1080 et ramenés à la hauteur source
  (`active_noop`, barres minimales, intermittence).
- `active >= active_noop` : pas de barres, `noop`.
- `review` si segment keyframé, barres intermittentes (≥ 10 % des crops),
  barres asymétriques (haut/bas ou gauche/droite), scale > 300 %, compensation
  Y > 600 px, ou couverture insuffisante (> 10 % des crops).
- `apply` sinon : `scale = 1920 / active × 100 × 1,015` (D-056), puis
  compensation `Position Y` pour centrer la zone active. La couverture
  mathématique de tous les crops est vérifiée après scale et compensation
  (tolérance 0,5 px).
- R2 (P2, `a1_defer_on_a3 = false` par défaut) : si A3 a marqué un candidat
  fort (`strong_candidate`), l'`apply` A1 du segment devient une `review`
  « compensation Y différée : keyframes probables détectées par A3 ». Aucun
  scale partiel n'est appliqué.

Validation historique (marge 1,04, avant D-056) : 268 → 15 segments à
**231,1 %** et Y **−287,9 px** ; 191 → 37 segments à **249,6 %** et Y
**−367,8 px** (cible enfant), soit −417,8 px en coordonnées composition. Avec
1,015, les cibles nominales deviennent **225,5 %** et **243,6 %** pour les
mêmes hauteurs actives ; les compensations Y sont à remesurer lors du prochain
run de validation. Les ancres restent identiques au brut.

D-057 (2026-11-08) : l'étude du détecteur `vapoursynth-letterbox` conclut à
un **spike natif opt-in**, pas à une intégration VapourSynth. `cropdetect`
reste le détecteur par défaut et tous les vetos ci-dessus sont conservés. Le
futur détecteur devra utiliser un clustering temporel, pas le p10 actuel,
pour agréger ses mesures. Rapport : `docs/etudes/ETUDE-VSLETTERBOX-A1.md`.

D-058 (2026-11-09) : l'option **A — natif gardé** est acceptée. Le flag
`a1_refine_native` (défaut `false`) active un raffinement du seul `apply`
A1 déjà décidé par `cropdetect` : natif v2 Sobel 20/1020, couverture 2/3
(0,45 si les quatre lignes côté barre sont sous 10/255), clustering
dominant ≥ 60 %, marge D-056 inchangée. Un natif `review`/`noop`, instable,
asymétrique ou qui échoue à la couverture conserve la décision
`cropdetect` ; le natif ne crée ni ne retire jamais un `apply`. Les détails
JSON additives sont `native_refined`, `native` et `cropdetect`. Validation
unitaire et réelle sur le 305, puis sur le corpus GOOD complet : 2 607
segments A1, 0 changement d'action, 52/65 applies raffinés, gain pixel moyen
1,46 px, surcoût 2,66 % du temps A1. Reste le contrôle visuel des cas 89
signalés et l'activation pilote. ADR :
`docs/decisions/DECISION-A1-NATIVE-GARDE-D058.md` ; preuve :
`docs/etudes/ETUDE-VSLETTERBOX-A1/results/full-corpus-v2.md`.

## A2 — bannière basse

- Médiane temporelle à 1 fps sur une miniature 480 px de large ; bandes
  claires et paires de bords sombres.
- Une bande **haute** plausible, un logo (< 35 % de largeur), une bande
  partielle (35–50 %) non confirmée par OCR, un décor (> 200 px), une bande
  sombre (médiane < 60) ou une bande dont la géométrie bouge
  (centre/hauteur > 12 px) partent en `review`.
- Une bannière qui change seulement de texte ou de luminosité reste
  applicable si sa géométrie est stable.
- Depuis D-056, une bande **basse** partielle de 35–50 % peut être appliquée
  si l'OCR est exécuté **avant** le gate de largeur et renvoie un verdict
  `Confirmed`, avec persistance ≥ 60 %, géométrie strictement stable
  (centre/hauteur ≤ 12 px), luminance ≥ 60, `temporal_std` ≤ 35 et scale
  ≤ 300 %. Les vetos `tall_bottom`, `top`, `logo` et `changing_text` restent
  prioritaires.
- `apply` si largeur ≥ 50 %, ou largeur ≥ 35 % pour une bande basse
  persistante confirmée par OCR, hauteur 24–200 px (seuils ramenés à la
  hauteur source), persistance ≥ 60 %, aspect ≥ 3, géométrie stable et
  scale ≤ 300 % :
  `scale = hauteur_comp / (hauteur_source − hauteur_bande) × 100 × 1,05`.
- R4 : la formule de scale reste basée sur la hauteur de bande, jamais sur la
  bbox OCR. Les applies partiels exposent `partial_width_ocr = true` et
  `scale_basis = "band_height"`.

Validation : 187 DISNEYWORLD et ECOLE → **206,6 %** (bannière de 104 px),
bannière supprimée à 10/40/100 s. Les 11 cas synthétiques donnent 3 `apply`,
7 `review`, 1 `noop`, conformes au prototype.

### Gate OCR A2 v3 (D-051, 2026-11-07)

Le gate est **opt-in** (`a2_ocr = false` par défaut) ; la production
l'active depuis le 2026-11-08 (D-055). Sans flag, la décision A2 reste
strictement géométrique et aucun modèle ONNX n'est ouvert. Quand il est
activé, 12 timestamps uniformes sont lus sur le crop du candidat (marge de
16 px sur les quatre côtés), PP-OCRv6 det/rec tourne en CPU avec
`a2_ocr_threads` (défaut 2, config de production 8), et les boîtes d'une
même ligne sont fusionnées avant l'agrégation. Le texte d'un seul caractère
est ignoré. Mesure sur `54` (12 échantillons) : 23,6 s à 2 threads, 13,2 s
à 8, 12,5 s à 12 ; A1 reste à ~23,5 s, donc 8 est le compromis retenu.

Features : `text_persistence` (échantillons avec texte / total),
`dominant_ratio`, `y_drift_px`, `score`, `overlap`, `coverage_x`.

**Activation (2026-11-08, D-055).** `a2_ocr = true` est actif en
production, avec `a2_logo_prior = false`, `a2_ocr_threads = 8`, les modèles
sous `paths.models/qa/` et le runtime
`vendor/libonnxruntime.so.1.23.2`. La séquence suivie a été : staging sur
copies de `40`, `54` et `58`, validation humaine, puis activation de
`a2_ocr` seul. Ne pas activer les deux flags ensemble : `a2_logo_prior`
reste en observation et sera évalué après une période de runs réels. Sans
modèles, le gate journalise un avertissement et retombe sur la décision
géométrique.

**Validation staging (2026-11-08).** Comparaison sur copies de `40`, `54`
et `58`, sans écriture dans `/mnt/cache` : baseline `a2_ocr = false` contre
staging `a2_ocr = true`, `a2_logo_prior = false`, `a2_ocr_threads = 8`.
`54` reste `apply` 200,8 % (confiance 0,90 → 0,95), `58` passe de `review`
à `apply` 210,0 %, `40 ABHARTACH` et `40 FOOTBALLEUR` restent `review` avec
les preuves OCR mais sans scale proposé : le texte dominant est encore
`66`/`MINUTES` (`coverage_x` 0,09–0,11 < 0,25). Tous les audits sont `pass`.
Coût du lot : 87,3 s en baseline, 130,7 s en staging (+50 %), soit 9–13 s
d'OCR par projet à 8 threads. Vérification avec la configuration de
production sur une copie de `54` : `apply` 200,8 %, OCR 13,0 s, post
43,9 s, audit `pass`, aucune erreur de chargement ONNX.

**Observation `a2_logo_prior` (2026-11-08).** Sur copies de `54` et `58`,
le flag ne change pas l'action : `apply` 200,8 % / 210,0 %, mêmes
`scale_target`. Il retire `UNJOUR`/`UNDOC` de `texts` et les place dans
`logo_texts` (`source_logo` visible, `logo_in_candidate` 0,65 / 0,93). Sur
`40 ABHARTACH` et `40 FOOTBALLEUR`, il ne change pas l'action (`review`)
mais remplace « bande basse animée, géométrie non fiable » (confiance 0,4,
sans scale) par « bande basse sombre, texte persistant confirmé »
(confiance 0,55, `scale_target` 218,2, dominance 0,9167). C'est un
changement de rapport réel, pas une simple annotation : le flag reste
`false` en production tant que ces nouveaux motifs n'ont pas été acceptés
par les opérateurs.

Décision :

- texte absent ou persistance < 60 % → décision A2 actuelle, sans
  dégradation ;
- texte changeant (dominance < 60 %) → `review` (veto), jamais `apply` ;
- texte persistant, dominant ≥ 80 %, score médian ≥ 0,80, `y_drift`
  ≤ 12 px (1080), overlap ≥ 0,50 et couverture ≥ 25 % :
  - bande claire et géométrie modérément instable → `apply` (override
    borné à 2× `GEOM_STD`) ;
  - bande sombre (médiane < 60) → `review` avec `details.scale_target` ;
  - géométrie stable → `apply` comme avant ;
- preuve ambiguë → décision A2 actuelle, mais un `apply` est dégradé en
  `review` ;
- garde-fous durs inchangés : keyframes, échelles hétérogènes, bande
  haute, largeur < 50 % sauf bande basse confirmée par OCR (D-056), hauteur
  hors 24–200, persistance < 60 %, `temporal_std` > 35 et scale > 300 % ;
- candidat étroit : texte persistant → `review` logo, texte changeant →
  `noop` sous-titre, sinon `review`.

`details.ocr` expose `samples`, `samples_with_text`, `detections_total`,
`frame_errors`, `text`, `bbox`, `text_persistence`, `dominant_ratio`,
`y_drift_px`, `score`, `overlap`, `coverage_x`, `texts` (trois premiers
groupes) et `timing_ms`.

Validation réelle du 2026-11-07 (config OCR dédiée, copies `/tmp`, run
complet pour `54`/`58`) :

- `54 CLIENT HOTEL` : `apply` **200,8 %**, texte « FAUX CLIENTS, VRAIS
  ENQUÊTEURS : RIEN NE LEUR ÉCHAPPE » persistant (12/12, dominance 100 %,
  couverture 80 %) ; 112 diffs de projet, toutes `Scale 180,0 → 200,8` ;
  audit `pass` ;
- `58 CROUSTY` : `review` → `apply` **210,0 %**, même texte ; 40 diffs,
  toutes `Scale 180,0 → 210,0` ; audit `pass` ;
- `40 ABHARTACH` et `40 FOOTBALLEUR` : restent `review` (géométrie
  instable) avec `details.ocr` enrichi ; le texte complet du bandeau
  apparaît dans `texts`, mais le groupe dominant reste le fragment « 66 »
  (couverture 9–11 %), donc la preuve est ambiguë ;
- `sub_fast` synthétique (sous-titres changeants) : `review` « texte
  changeant (dominance 25 %) », aucun recadrage.

Coût mesuré : 15–25 s par projet avec 2 threads, 13,2 s avec 8 threads sur
le Threadripper. Modèles absents ou OCR en échec → warning et retour à la
décision A2 actuelle, sans échec du post. Les fichiers
`PP-OCRv6_det_small.onnx` et `PP-OCRv6_rec_small.onnx` doivent être copiés
sous `paths.models/qa/`, ou renseignés en absolu.

### Registre spatial des logos source (D-052, phases A/B, 2026-11-08)

Le catalogue versionné contient 30 émissions avec logo (29 en bas-gauche,
Piquantes ! en haut-gauche), 21 clean feeds et 9 émissions sans logo ; la
30ᵉ entrée est *Les Reines de la Route* (6ter, bbox `[99, 848, 315, 1022]`
dans la zone `bottom_left`). Les crops RGBA restent externes.

`a2_logo_prior = false` par défaut : sans flag, aucune lecture de catalogue
et aucune clé JSON supplémentaire. Quand il est activé,
`assets/models/qa/source_logos.toml` résout l'émission de la source (nom de
fichier exact d'abord, alias ensuite) puis projette la bbox du logo par les
transformations du `.ecproj` (keyframes de Position, Anchor et Scale
interpolés ; parent animé refusé). `details.source_logo` expose `name`,
`channel`, `position`, `bbox`, `logo_in_candidate`, `candidate_covered`,
`visible_in_comp`, `visible_ratio` et `projected_bbox`. Une rotation ou un
parent animé/redimensionné rend la projection non calculable :
`projection = "unsupported"` remplace alors les champs de visibilité.
L'action A2 n'est **jamais** modifiée par ce champ.

Rejeu OCR réel : `54` reste `apply` 200,8 %, `58` reste `apply` 210,0 %,
`40 ABHARTACH` et `40 FOOTBALLEUR` restent `review` avec le logo 66 Minutes
documenté à 72,6 % du candidat ; les seules différences sont `source_logo`
et les temps OCR. Les crops de référence restent hors dépôt ; le catalogue
ne contient que des métadonnées.

Phase B (même flag) : les détections dont la boîte est majoritairement dans
la bbox du logo sont retirées du groupe OCR dominant. Les clés
`details.ocr.logo_texts`, `logo_detections_total` et `logo_only`
n'apparaissent que si un prior est actif ; sans prior, la sortie OCR est
inchangée. Si le logo est la seule preuve textuelle, A2 répond `review` au
lieu d'appliquer. Rejeu : `54`/`58` conservent leur action et leur scale
(`UNJOUR`/`UNDOC` passent de `texts` à `logo_texts`), `40` reste `review`
mais devient « bande sombre, texte persistant confirmé » avec la bannière
réelle dominante (91,7 %) et `scale_target` 218,2 %, `sub_fast` reste vétoé
à 25 %.

Phase B — overlays déclarés (2026-11-08) : A2 lit le masque des overlays
image du `.ecproj` via le module partagé `post::overlay`. Si, aux douze
instants du gate OCR, toutes les occurrences visibles du logo projeté
sont couvertes à ≥ 90 %,
`visible_in_comp` passe à `false`, `overlay_covered` et
`hidden_by_overlay` documentent la couverture, et le veto « logo seul » est
remplacé par la décision géométrique A2. Rejeu sur `54` avec un habillage
synthétique déclaré couvrant le logo : `apply` 200,8 % inchangé,
`visible_in_comp=false`, `overlay_covered=1.0`, 2/2 occurrences visibles
masquées ; sans habillage, la sortie reste identique à la phase B. La
confirmation visuelle NCC/contours et la stabilité de la zone générique
sont implémentées côté QA (`--source-logo-visual`, LOT-10) ; A2 ne les
consulte pas et garde ses garde-fous géométriques.

### Grounding DINO (D-060, hors production, 2026-11-09)

Le spike `docs/etudes/ETUDE-GROUNDING-DINO-A2.md` évalue
`IDEA-Research/grounding-dino-tiny` sur 242 frames (122 logos du catalogue,
60 bandeaux A2, 60 négatifs). Au seuil 0,20, le prompt composite
`logo . text . watermark . lower third . banner` donne 96,7 % de logos à
IoU ≥ 0,50, 98,3 % de bandeaux et 97,2 % de texte IoU ≥ 0,30, mais 25 % de
faux positifs **par frame** sur les négatifs. Le clustering temporel
(IoU ≥ 0,50, centre ≤ 10 px, persistance ≥ 70 %) laisse 0/5 clip négatif
persistant et conserve 5/5 logos positifs. Coût local : 1,279 s/frame à
480 px, 1,47 Gio de VRAM, 15,3 s pour 12 frames.

**Aucun flag Grounding DINO n'est ajouté, aucun appel de ce modèle dans
A2.** Le détecteur reste un outil de laboratoire pour proposer des logos
inconnus au catalogue et qualifier les revues. Si un pilote était autorisé,
il serait `review` seulement : branche logo serrée (largeur ≤ 0,35),
branche texte (largeur ≥ 0,20, `y ≥ 0,60`), persistance ≥ 70 %,
`details.grounding_dino` additif, échec → décision A2 inchangée, verrou
matériel obligatoire. Détails, seuils et conditions de réouverture :
`docs/decisions/DECISION-GROUNDING-DINO-A2.md`. La suite est un détecteur
local ONNX/YOLOX-Nano ou NanoDet-Plus exécuté nativement en Rust via `ort`,
sans sidecar : `docs/etudes/ETUDE-YOLO-ONNX-A2.md`. Le **Lot B0** est
exécuté (exports FP32/FP16, benchmark CUDA : YOLOX 416 FP32 à 8,40 ms p95
au total) ; les lots B1 et B2 ci-dessous prennent la suite.

### Détecteur YOLOX-Nano B1/B2 (annotation seule, 2026-11-09)

Le **lot B1** a fine-tuné YOLOX-Nano 416 depthwise sur 1 495 logos et
228 logos externes. Le checkpoint époque 5 est figé après un arrêt manuel à
l'époque 15 (la val synthétique montait, D-060 baissait). Point de
fonctionnement : seuil 0,30, NMS 0,65, 1 classe `logo`, persistance 0,70.
Métriques : rappel D-060 95,5 %, présence lower-third 96,7 %, boîte logo A2
91,7 %, open-set 90,3 %, FP négatifs 3,3 %, 8,01 ms p95 et +107 Mio de
VRAM. Détails : `results/B1.md`.

Le **lot B2** ajoute une annotation passive, désactivée par défaut :

```toml
a2_logo_detector = false
a2_logo_detector_model = "qa/logo_yolox_nano_a2.onnx"
a2_logo_detector_onnxruntime = ""   # vide = runtime système
a2_logo_detector_device = "cuda"    # cpu = diagnostic
a2_logo_detector_size = 416         # 640 possible
a2_logo_detector_conf = 0.30
a2_logo_detector_iou = 0.65
a2_logo_detector_min_persistence = 0.70
a2_logo_detector_roi = "full"       # bottom, bottom_left = essais
```

Quand il est actif, `post run` échantillonne 12 frames à la résolution
source (les mêmes instants que le gate OCR A2, mais `a2.rs` reste intact),
filtre les détections sur le candidat A2 et les zones logo, agrège les
clusters (IoU ≥ 0,50, centre ≤ 10 px), puis ajoute
`details.logo_detector` aux décisions A2. Le bloc expose le modèle, son
SHA-256, les seuils, `detections_total`/`detections_a2`, `clusters`,
`logo` (`detected`, `persistence`, `frames`, `bbox`, `score`, `unknown_logo`,
`source_logo_match`, `merged_with_lower_third`) et `latency_ms`. Aucune
action, raison, confiance ou `scale_target` n'est modifiée. Une erreur
(modèle absent, verrou occupé, OOM, frame illisible) journalise un warning
et laisse la décision intacte.

Le verrou `/tmp/workflow-rs-hw.lock` est acquis sans attente avant le
chargement de la session et relâché après destruction de la session CUDA.
Le runtime CUDA est initialisé avant le gate OCR A2 pour que le `OnceLock`
ONNX Runtime ne reste pas sur le runtime CPU de production ; le pilote
utilise le runtime du venv TransNet et `LD_LIBRARY_PATH` avec les
répertoires `nvidia/*/lib`. Le binaire ONNX reste hors dépôt ; le manifeste
`assets/models/qa/manifest.toml` fige taille, SHA-256, licence et opset.

Le recadrage ROI a été mesuré sur D-060 : `bottom_left` porte la boîte logo
A2 de 91,7 % à 98,3 %, mais fait chuter les références de 95,5 % à 84,0 %.
Le pilote B2 garde donc `full` ; une variante à deux passes reste la piste
B1.2. Le pilote `report_only`, ses résultats et ses limites sont décrits
dans `docs/etudes/ETUDE-YOLO-ONNX-A2/results/B2.md`. **Aucune activation
de production n'est autorisée par ce lot** : le détecteur ne participe
toujours pas à la décision A2.

Résultats du pilote (2026-11-09) : 13 projets finalisés dont le brut est
byte-identique à l'assist (STEP7 supprime les bruts ; 2 `apply` exclus),
**0 différence d'action** et de `review.json`, aucun audit `fail`
(2 `warn` `source_coverage` préexistants),
2/13 logos persistants (11/12), tous deux hors catalogue D-052, deux
logos animés à 7/12 sous le seuil, latence p50 9,28–10,86 ms / p95
10,77–12,25 ms, coût 4,45 s/projet. Aucun `apply` créé.

La consolidation **B2.1** (2026-11-09) ne change ni le modèle ni la
décision. Le clustering tente d'abord l'association stricte (IoU ≥ 0,50,
centre ≤ 10 px), puis une association animée (IoU ≥ 0,35, centre ≤ 25 px,
ratio de taille 0,5–2,0) ; les seuils sont exposés dans
`details.logo_detector.clustering`. Sur D-060, aucun clip négatif ne devient
persistant et cinq groupes de références supplémentaires sont récupérés,
tous corrects. Le test synthétique reprenant les deux boîtes de
« Manifestations » donne 10/12 ; sur le pilote réel, le sous-cluster reste
séparé (9/12 + 3/12, 0,75, `detected=true`). Le pilote B2.1 conserve
**0 différence d'action** et de `review.json`, aucun `apply`, et fait
passer les logos persistants de 2/13 à **5/13**, tous `unknown_logo`
(latence p95 10,92–14,28 ms, coût 4 535 ms/projet). Les 12 frames source
sont mises en cache (`vision/frame.rs`) : quand l'OCR A2 est actif, il les
extrait et le détecteur les relit, ce qui ramène le second passage de
4,1–4,7 s à 0 ms. Aucun des 13 projets du pilote ne déclenche l'OCR : le
cache n'a donc pas réduit le coût en conditions réelles. Le coût résiduel
du détecteur reste la session CUDA (~1,4–1,5 s) et l'inférence (~0,1 s).
Détails : `docs/etudes/ETUDE-YOLO-ONNX-A2/results/B2.1.md`.

## A3/A4/A5 — revue et rapport seulement

### A3 — candidats de keyframes

- A3 lit `_ae.json` et `segment_recommendations`, calcule l'amplitude et le
  trou de suivi, et signale des candidats de keyframes en `review`. Jamais
  d'application (R7).
- R5 (D-056) : `noop` si `amplitude < 250 px` **et** `candidate_keys <= 2`,
  avec le motif « mouvement faible ou nombre de clés insuffisant ». Dans le
  corpus GOOD, ce filtre retire 171 revues (37,5 %) pour 4 vrais positifs.
- `high_confidence = true` si `amplitude >= 700 px` ou
  `candidate_keys == 6` ; la raison contient « haute confiance » et
  `review.json` passe la sévérité à `high`.
- R6 (P2, `a3_relaxed_gap = false` par défaut) : en revue seulement,
  `gap < 0,70` est accepté si `amplitude >= 200 px`. Le détail
  `gap_relaxed = true` documente le recours au seuil assoupli.
- `strong_candidate = true` si `candidate_keys >= 4` ou
  (`amplitude >= 300 px` et `gap < 0,30`) ; ce marqueur alimente R2.
- Aucun recadrage statique automatique : la recommandation STEP4 ne
  correspond au choix humain que dans 46 % des cas.

### A4 — segments courts

- A4 signale les segments courts, les fusions CSV et les bornes non alignées
  en `review`. La détection de cuts durs intra-segment reste désactivée
  (trop de faux positifs).
- R8 (D-056) : un segment de durée `< 0,3 s` sans continuité de source des
  deux côtés est tagué `flash_probable = true`. La continuité exige le même
  `source` et des intervalles qui se touchent à 2 ms près ; le détail
  `source_continuity` expose les deux côtés. Sur le corpus, les 7 `none`
  < 0,3 s sont des segments de bord, alors que les 4 `extend` sont intérieurs
  et continus. Aucune fusion automatique.

### A5 — couverture source

- A5 reste report-only : chaque revue de couverture porte
  `resolution = "resolved"` et `report_only = true`.
- Le contrôle d'audit `source_coverage` indique « résolus par le monteur
  (report-only) ». Les 17 revues du corpus sont toutes résolues par le
  recadrage humain ; elles ne sont pas comptées comme faux positifs.

## A6 — template SDM

Déclencheur explicite : nom de la source (ou, à défaut, nom du projet sans
`_9x16`) préfixé par l'un des `template_prefixes` configurés. Les deux
nommages livrés en production sont `M6 - SDM - ` et `SDM - `. Le contenu ne
sert jamais à deviner le template ; le préfixe effectivement matché est
enregistré dans `template.prefix` (`post.json`) et dans les détails de la
décision A6.

Contrôles structurels : un seul segment, aucune keyframe, composition
1080×1920, segment couvrant toute la composition, parent Null absent ou
statique, source 1920×1080. Le logo `sdm.png` est optionnel.

Profil exact mesuré sur le GOOD 295 :

| Élément | Valeur |
| --- | --- |
| Fond | 180 %, flou gaussien 50, position (540,0950433039733 ; 0) |
| Panneau principal | 84,3497774862575 %, position (540,095… ; 171,03210271024682) |
| Panneau secondaire | 56,294573830734535 %, position (540,095… ; 1082,0260859403052) |
| Logo | `sdm.png` 100 %, position (540 ; 960) |
| Ordre des calques | logo, secondaire, principal, fond |
| Ancre des panneaux | (960 ; 0) |

Validation : le projet assisté de 295 est **identique au JSON de référence**
et le rendu relinké donne **0/255** (0 pixel > 2) à 0, 10, 30 et 50 s par
rapport au GOOD. Un projet keyframé ou multi-segment part en `review` et le
projet assisté reste une copie du brut.

## Bypass direct des vidéos SDM

A6 ne consomme que le `.ecproj` brut, la vidéo source et `sdm.png`. Les
vidéos SDM n'ont donc pas besoin de STEP2 (cuts), STEP3 (audio), STEP4
(tracking), STEP5/6 (réduction) ni du panel `MediaSolution.jsx`. Quand
`steps.step8.post.direct = true`, une vidéo SDM valide est court-circuitée.

Prédicat volontairement conservateur : nom préfixé par l'un des
`template_prefixes`, `.mp4`, H.264, 1920×1080, SAR 1:1, 25 fps CFR, durée de
flux et `nb_frames` valides. Un nom SDM qui ne respecte pas ce profil repart
dans le pipeline normal.

- **STEP2 à STEP6** : `SKIP_TEMPLATE` est appliqué avant le chargement des
  modèles et le verrou matériel. Les rapports exposent `skipped_template`.
- **STEP7** : une vidéo SDM valide suffit en mode `lenient` ; le mode
  `strict` conserve sa sémantique (scènes + tracking réels). Le CSV
  mono-segment est synthétisé depuis `nb_frames` avec le format STEP2
  (en-tête `No,Timecode In,Timecode Out,Frame In,Frame Out`, CRLF, frames
  1-based incluses, `Timecode Out` = début de la dernière frame). Il est
  écrit dans l'archive sous `<stem>_scenes.csv` et dans la copie finale
  sous `<stem>.csv`. Le projet source n'est pas modifié ; un CSV existant
  n'est jamais écrasé. `metadata.json` reçoit `scenes_origin` (`template`,
  `step2` ou `null`).
- **STEP8** : le brut `<stem>_9x16.ecproj` est construit depuis l'asset
  embarqué `assets/effectcraft/sdm_raw_template.json`, sans
  `MediaSolution.jsx`, puis A6 publie `<stem>_9x16_assist.ecproj`. Le brut
  est créé même avec `--no-post` et un brut existant n'est jamais écrasé.
- **Dossiers** : un dossier qui ne contient que des SDM n'appelle pas le
  panel. Un dossier mixte l'appelle avec les noms SDM injectés en minuscules
  dans `videoSkipNames` ; le panel ignore les SDM et traite les non-SDM
  normalement.
- **Post seul** : `workflow-cli post run --direct` crée le brut pour un
  lancement manuel, mais pas en `--report-only`.

Validation :

| Cas | Résultat |
| --- | --- |
| 295 | CSV STEP7 identique au CSV STEP2 (`1,00:00:00.000,00:00:52.640,1,1317`) ; chaîne STEP7 → STEP8 ; rendu 0/10/30/50 s identique au pixel à la référence Rust acceptée |
| 340 | CSV `1,00:00:00.000,00:00:21.200,1,531` ; brut et assist rendus, logo et panneaux conformes |
| Lot mixte 303 + 340 | non-SDM `.ecproj` et `_assist.ecproj` identiques octet pour octet à l'ancien pipeline (chemins normalisés) ; rendu identique à 10/60 s ; SDM `apply` A6, audit `pass` |
| 116 Camille, nom `SDM - Liliane et josé` (2026-11-09) | rejeu réel sous `/tmp/test-sdm` : préfixe `SDM - ` reconnu, brut + assist, 4 calques A6 (180 %, 84,3497774862575 %, 56,294573830734535 %, logo 100 %), `template.prefix = "SDM - "`, audit `pass`, aucune décision A1–A4 |
| 116 Camille renommé `M6 - SDM - Liliane et josé` | même résultat avec `template.prefix = "M6 - SDM - "`, aucune régression |

## Bypass archive-only (LPHDF)

La famille `W9 - LPHDF` n'a besoin d'aucun `.ecproj` : son template sans
découpe est géré manuellement. Quand `direct = true` et que
`archive_only_prefixes` contient `W9 - LPHDF`, la vidéo suit un bypass encore
plus radical que SDM.

- **STEP2 à STEP6** : même prédicat que SDM (MP4 H.264 1920×1080, SAR 1:1,
  25 fps CFR, `nb_frames` valide), donc mêmes gains et mêmes rapports
  `skipped_template`.
- **STEP7** : readiness `lenient`, CSV mono-segment synthétisé et archivé
  sous `<stem>_scenes.csv`, puis copié sous `<stem>.csv` dans la sortie
  finale. La copie `/mnt/cache` et la suppression de source restent
  inchangées : la vidéo reste disponible pour le montage manuel.
- **STEP8** : aucun `.ecproj` n'est créé. Le skip est **basé sur le nom**,
  indépendamment du profil : même un fichier hors profil ne produit jamais
  de projet. Un dossier 100 % LPHDF n'appelle pas le panel ; un dossier
  mixte l'appelle en injectant les noms LPHDF en minuscules dans
  `videoSkipNames`.
- **Post-traitement** : les dossiers 100 % LPHDF et les projets résiduels
  dont le stem correspond à une vidéo LPHDF sont ignorés.
- **Hors profil** : le fichier traverse STEP2–7 normalement pour produire un
  vrai CSV, mais STEP8 l'ignore quand même.

Validation :

| Cas | Résultat |
| --- | --- |
| 341 LPHDF (4 vidéos) | 4 CSV archivés (`5259`, `5293`, `5240`, `5253` frames), `scenes_origin = "template"`, copie docs complète, aucun `projets/`, aucun appel `effectcraft-cli` |
| 341 + 303 (mixte) | seul le projet ordinaire est produit ; `videoSkipNames` contient le nom LPHDF en minuscules ; projet ordinaire identique à l'ancien pipeline hors version EffectCraft et chemins |

## Configuration

```toml
[steps.step8.post]
enabled = true
# Bypass direct SDM (D-034) et archive-only LPHDF (D-035).
# Validé sur 295/340/341 et des lots mixtes.
direct = true
suffix = "_assist"
projects_subfolder = "projets"
artifacts_subfolder = "post"
# Les deux nommages SDM coexistent (`M6 - SDM - ` et `SDM - `).
template_prefixes = ["M6 - SDM - ", "SDM - "]
# Familles sans `.ecproj` : CSV de scènes archivé uniquement.
archive_only_prefixes = ["W9 - LPHDF"]
ffmpeg = "ffmpeg"
ffprobe = "ffprobe"
# Marge A1 D-056 : médiane humaine 1,0125, 51/65 cas <= 1,015.
a1_margin = 1.015
# P2 validé sur copies (2026-11-08) : différer A1 sur candidat A3 fort (R2).
a1_defer_on_a3 = false
# D-058 option A : raffiner un apply A1 avec le natif v2 (désactivé par défaut).
a1_refine_native = false
a2_margin = 1.05
# P2 validé sur copies (2026-11-08) : gap < 0,7 en revue pour amplitude >= 200 px (R6).
a3_relaxed_gap = false
# Gate OCR A2 (D-051), activé en production le 2026-11-08 (D-055).
a2_ocr = true
a2_ocr_det = "qa/PP-OCRv6_det_small.onnx"
a2_ocr_rec = "qa/PP-OCRv6_rec_small.onnx"
a2_ocr_threads = 8
# a2_ocr_onnxruntime = "/chemin/vers/libonnxruntime.so"
# Registre spatial des logos source (D-052), annotation seule.
a2_logo_prior = false
a2_logo_catalog = "qa/source_logos.toml"
```

`template_prefixes` remplace le champ scalaire `template_prefix`, encore
accepté comme alias Serde pour les configurations existantes. `config check`
refuse un préfixe vide et tout chevauchement entre préfixes de template ou
avec `archive_only_prefixes`.

Le timeout `steps.step8.timeout_s` couvre EffectCraft **et** le
post-traitement lorsque STEP8 est lancé par le serveur. Le coût mesuré du
post-traitement est de 0,07–0,24× la durée de la vidéo (sans rendu).

## Commandes

```sh
# Hook automatique après STEP8 (désactivable).
workflow-cli step8 run --folder /tmp/projet
workflow-cli step8 run --folder /tmp/projet --no-post

# Bypass direct SDM + archive-only (ou via [steps.step8.post].direct = true).
workflow-cli step7 finalize --direct --work-dir /tmp/projets
workflow-cli step8 run --direct --folder /tmp/projet
workflow-cli post run --direct --folder /tmp/projet

# Post-traitement seul, sur un dossier projet.
workflow-cli post run --folder /tmp/projet

# Sur un projet isolé, avec un dossier docs explicite.
workflow-cli post run \
  --project /tmp/projet/projets/video_9x16.ecproj \
  --docs /tmp/projet/docs

# Calcul des rapports sans publier le projet assisté.
workflow-cli post run --folder /tmp/projet --report-only
```

## Progression STEP8/post-STEP8

Le hook STEP8 ne se contente plus d'attendre la fin du post-traitement : il
émet des marqueurs de progression lus par le serveur.

```
POST_TOTAL: N
POST_CURRENT: i/N - <projet>
POST_PROGRESS: i/N - <projet>
POST_DONE: N - <échecs>
```

- `POST_TOTAL` ajoute les `N` projets A5/A6 au total EffectCraft déjà publié
  par `MS_BATCH_TOTAL` ; la barre ne repart pas de zéro.
- `POST_CURRENT` est émis avant chaque projet et expose le texte
  `Post-traitement A5/A6 : i/N — <projet>` ; la barre reste au nombre de
  projets déjà terminés.
- `POST_PROGRESS` est émis après chaque projet et avance la barre.
- `POST_DONE` clôt la phase avec le nombre d'échecs éventuels.
- Le serveur publie `progress_phase = "post"` ; `assets/web` affiche la barre
  en orange, le statut « Post-traitement » et le texte sans réapposer le
  compteur global.
- Les marqueurs partagent `stdout` avec `MS_BATCH_*` dans le hook STEP8 pour
  préserver l'ordre ; `workflow-cli post run` les écrit sur `stderr` pour
  garder son JSON intact sur `stdout`. La granularité est le projet, pas les
  étages A1/A2/A3/A4/A6.

## Validation de production

Les lignes 268 et 191 ont été mesurées avec la marge A1 historique de 1,04,
avant D-056 ; elles restent la référence de parité comportementale.

| Cas | Résultat |
| --- | --- |
| 295 SDM | 1 `apply` A6, audit `pass`, assist identique à la référence, rendu 0/255 |
| 295 keyframé | 0 `apply`, 3 `review`, assist copie exacte |
| 268 | 15 A1 à 231,1 %, 9 `review`, 53 `noop`, audit `pass` (marge 1,04) |
| 187 DISNEYWORLD | 1 A2 à 206,6 %, 14 `review`, audit `pass` |
| 187 ECOLE | 1 A2 à 206,6 %, 2 `review`, audit `pass` |
| 191 | 37 A1 à 249,6 %, 12 `review`, audit `pass` (marge 1,04) |
| 181 Concierge | 0 `apply`, 20 `review`, assist octet pour octet identique |
| 270 / 276 / 286 / 287 | 0 `apply`, 7 projets, assist identiques au brut, audit `pass` |
| 295 direct | CSV STEP7 = STEP2, rendu 0/10/30/50 s identique au pixel à la référence Rust |
| 340 direct | CSV 531 frames, brut + assist, rendu conforme |
| Lot mixte 303 + 340 | non-SDM identique octet pour octet à l'ancien pipeline (chemins normalisés), rendu identique |
| A2 synthétique | 11/11 conformes (3 `apply`, 7 `review`, 1 `noop`) |
| A2 v3 OCR (54/58) | 54 `apply` 200,8 % inchangé, 58 `review` → `apply` 210,0 %, diffs limitées à `Scale`, audits `pass` |
| A2 v3 OCR (40) | 2 `review` conservés, `details.ocr` et `texts` enrichis |
| A2 v3 OCR (sub_fast) | sous-titres changeants → `review` « texte changeant », aucun recadrage |
| A2 registre logos (D-052) | `54`/`58` actions et scales identiques, `40` enrichis sans changement d'action, audits `pass` |
| D-056 A1 | marge 1,015 : `243,6 %` sur la fixture 800 px ; marge explicite 1,04 : `249,6 %` |
| D-056 A2 | bande basse 40 % + OCR `Confirmed` → `apply` 206,6 % ; sans OCR, Sparse, Changing ou LogoOnly → `review` |
| D-056 A3 | `noop` sous 250 px / 2 clés ; `high_confidence` à ≥ 700 px ou 6 clés ; `gap_relaxed` opt-in ; jamais `apply` |
| D-056 A4 | 7 flashes de bord < 0,3 s tagués, 4 `extend` intérieurs continus non tagués |
| D-056 A5 | revues de couverture `resolution = "resolved"`, `report_only = true` |

Avant D-056, les décisions A1/A2/A3/A4 de 268, 187 DISNEYWORLD, 187 ECOLE,
191 et 181 étaient **identiques au prototype Python** (code, action, cible,
raison et détails). Cette parité et les lignes 268/191 ont été mesurées avec
la marge 1,04 ; D-056 change les scales A1 et filtre certaines revues A3,
sans toucher aux invariants A6. Les GOOD 270/276/286 ne changent aucune
échelle ; 287 n'a qu'une référence GOOD sur trois, donc la comparaison y est
partielle.

### Validation R2/R6 sur copies (2026-11-08)

Trois projets du corpus ont été copiés sous `/tmp/opencode/r2r6` puis rejoués
avec deux configurations identiques (`a1_margin = 1.015`, `a2_ocr = false`,
`direct = false`), seule la paire `a1_defer_on_a3`/`a3_relaxed_gap` change.
L'OCR A2 est désactivé pour isoler R2/R6 ; les décisions A2 restent
identiques entre baseline et variante. Aucune écriture sous `/mnt/cache`,
`runtime/` ou le corpus.

- **68 — `6ter - De l'autre côté du périph` (R2)** : 7 `apply` / 2 `review` /
  7 `noop` en baseline, 6 / 3 / 7 en variante. Le Segment 7
  (25,72–38,88 s), seul `apply` A1 partagé avec un candidat A3 fort
  (317 px, gap 0,261, 4 clés), passe en `review` avec
  `a1_deferred_on_a3 = true`. Les six autres `apply` A1 restent identiques ;
  l'assist variante conserve le transform brut du Segment 7 (scale 180 %,
  position Y 50) au lieu de 243,6 % / −305,4 px. Le diff de l'assist se limite
  à ces deux valeurs.
- **54 — `UJUD - CLIENT HOTEL` (R6)** : 1 `apply` / 15 `review` / 97 `noop`
  en baseline, 1 / 24 / 88 en variante. Neuf A3 `noop` passent en `review`
  avec `gap_relaxed = true` : Segments 2 (262 px, 0,585), 5 (491, 0,508),
  14 (235, 0,490), 17 (480, 0,625), 19 (207, 0,456), 23 (763, 0,602,
  `high_confidence`), 31 (371, 0,545), 40 (306, 0,457) et 49 (262, 0,316).
  Les Segments 3 (197 px) et 29 (348 px, gap 0,785) restent `noop`. Aucun
  `apply` A3 ; l'assist est identique octet pour octet.
- **356 — `Pretty Little Liars - Extrait 6` (R6 et non-régression R2)** :
  1 / 9 / 21 en baseline, 1 / 12 / 18 en variante. Les Segments 9 (327 px,
  0,362), 11 (295 px, 0,585) et 13 (485 px, 0,440) passent en `review` ;
  l'`apply` A1 du Segment 8 (196,5 %) n'est pas différé car le candidat A3
  fort est sur le Segment 2. L'assist est identique octet pour octet.
- **Haute confiance** : dans la variante 54, `review.json` porte
  `severity = "high"` pour les Segments 10, 23 et 27 (6 clés / amplitude
  ≥ 700 px), et l'audit `a3_disabled` indique « 3 haute(s) confiance ».
- **Qualité sur les GOOD** : le Segment 7 de 68 porte 18 keyframes de
  Position dans la référence humaine (scale statique 249,6 %), ce qui
  confirme le candidat A3 et le report R2. Sur les 12 nouvelles revues R6,
  5 segments sont keyframés dans les GOOD (54 : 5, 17, 23, 49 ; 356 : 9) et
  54/19 a un repositionnement statique humain, soit 6/12 revues utiles ;
  6 sont sans changement humain (54 : 2, 14, 31, 40 ; 356 : 11, 13). R6
  reste review-only : il ajoute du bruit de revue sans modifier les projets.
- **Portée** : sur les 75 `post.json` du corpus, le projet 68 est le seul où
  un `apply` A1 et un candidat A3 fort visent le même segment. R2 est donc
  rare mais son unique déclenchement est confirmé par les keyframes GOOD ;
  R6 ajoute des revues sans jamais produire d'`apply`. Les deux flags restent
  opt-in jusqu'à décision d'activation.

## Limites

- A3/A4 restent en revue, A5 en rapport ; aucun keyframe, cut ou recadrage
  statique n'est appliqué. R2 et R6 sont validés sur copies (68/54/356,
  2026-11-08) mais restent opt-in jusqu'à activation en production.
- A1 ne couvre pas les bords non noirs ni les recadrages sémantiques ; ces cas
  restent en `review`.
- A2 OCR est **activé explicitement** (`a2_ocr = true` en production depuis
  D-055 ; défaut du code `false`) et coûte 15–25 s par projet avec 2
  threads, 13,2 s avec 8 ; il ne couvre que le texte, pas les logos
  purement visuels. Les bandeaux sombres restent en `review` (niveau 1), les
  logos (< 35 %) ne sont jamais recadrés et les bandes partielles 35–50 % ne
  sont appliquées que si l'OCR confirme un texte persistant et stable.
- A6 est un template SDM mesuré ; un projet multi-segment ou keyframé part en
  `review`. Le profil est volontairement figé.
- Les artefacts ne sont pas encore exposés dans l'interface web ou le panneau
  CEP ; ils sont consultables sous `docs/post/`.
- Le post-traitement ajoute du temps au STEP8 ; le timeout serveur doit en
  tenir compte sur les vidéos très longues.
- Le bypass direct est limité au profil MP4 H.264 1920×1080 25 fps CFR ; les
  autres formats ou cadences repartent dans le pipeline normal. Un brut
  existant n'est jamais remplacé, même s'il n'est pas compatible A6.
- 179 n'a pas de `.ecproj` GOOD et 287 n'a qu'une référence sur trois : la
  validation corpus reste partielle sur ces deux projets.
- Les projets **multi-source** ne sont pas traités par A1/A2 : la vidéo
  n'est pas choisie arbitrairement, les décisions partent en `review`.

## Tests

```sh
cargo test -p workflow-steps post:: --lib
cargo test -p workflow-steps post::a2 --lib
cargo test -p workflow-steps step8 --lib
cargo test -p workflow-steps template --lib
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```
