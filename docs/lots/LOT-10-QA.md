# Lot 10 — contrôle qualité read-only après export humain

> Statut : module Rust v1 démarré le 2026-10-30. Stage 0 déterministe,
> Stage 1 OCR PP-OCRv6 et visage YuNet, agrégation intégrés ; les logos
> restent à porter.
> Décision : scénario B de `docs/etudes/Architecture_QA_Vidéo_Locale.md`.
> Code : `crates/workflow-steps/src/qa/`, commande `workflow-cli qa run`.

## Problème

STEP8 produit un `.ecproj` brut, A5/A6 un projet `_assist` et un `audit.json`.
Il reste un contrôle **après export humain** : vérifier que le MP4 final
respecte les specs, ne contient pas de noir/gel anormal, que le template A6
est présent, et — à terme — qu'aucun sous-titre, visage coupé ou logo
résiduel n'a été oublié.

Ce contrôle est **read-only**. Il ne modifie ni le `.ecproj`, ni le MP4, ni
`docs/`. Il écrit uniquement `qa.json` sous un répertoire `--out` explicite.

## Invariants

- `qa.json` est distinct de l'`audit.json` de A5/A6. Il ne le remplace ni ne
  l'écrase.
- Aucune écriture dans le corpus, `runtime/` ou `/mnt/cache`.
- Pas de fallback Python : le module Rust est autonome pour Stage 0, l'OCR et
  le visage.
- `auto_approve_eligible` reste **false** en v1. Les logos ne sont pas
  mesurés et aucune validation humaine n'a encore eu lieu.
- Un modèle ou un projet absent produit `not_applicable`, pas un échec
  global.
- Les écritures JSON utilisent temporaire voisin + `rename`
  (`workflow_core::fsutil::write_atomic`).

## Pipeline v1

1. **Stage 0 — conformité**
   - specs ffprobe : 1080×1920, 25 fps, H.264, AAC ; bitrate 9–11,5 Mbit/s
     en information seulement ;
   - durée MP4 vs durée composition `.ecproj` (tolérance 1 s) ;
   - présence du template A6 (`sdm.png`) pour les projets `M6 - SDM - ` ;
   - structure de timeline : segments, segments courts, couverture ;
   - noir/gel : `ffmpeg blackdetect` + `freezedetect`, stockés en preuves.
2. **Stage 1 — OCR** : PP-OCRv6 det/rec sur ONNX Runtime CPU, uniquement sur
   la bande basse (25 % inférieurs). La détection DB renvoie des lignes de
   texte, la reconnaissance CTC donne texte et score, puis des filtres
   géométriques écartent logos, cartons et textes de scène. Un sous-titre
   n'est retenu qu'après trois échantillons consécutifs à hauteur stable.
3. **Stage 1 — visage** : YuNet ONNX Runtime CPU sur les mêmes instants. Le
   détecteur tourne sur la frame et sur une version étendue de 15 % de chaque
   côté ; les deux jeux de boîtes sont fusionnés. Seuls les visages dont un
   landmark (yeux, nez, bouche) approche un bord gauche/droit sont retenus,
   après trois échantillons consécutifs. Un gros plan de profil qui coupe la
   nuque ou le menton sans couper les landmarks reste valide.
4. **Stage 2 — agrégation** : un `fail` de check ou un défaut objectif
   (`spec_mismatch`, `template_logo_missing`) bloque ; tout autre finding ou
   check `review` demande une revue ; `info` et `not_applicable` ne dégradent
   pas le statut.
5. **Preuves** : hash SHA-256 de l'export, puis une frame WebP par finding
   sous `<out>/evidence/`, à mi-intervalle ou 250 ms après le début.
6. **Rapport** : `qa.json` avec specs, hash, méta projet, checks, findings,
   preuves Stage 0, preuves OCR et visage, statut global et métriques.

Les deux contrôles Stage 1 partagent le même échantillonnage et s'exécutent
dans deux threads : l'OCR et le visage sont CPU-bound et indépendants. Le
temps total est donc proche de `Stage 0 + max(OCR, visage)` plutôt que de la
somme.

## Seuils Stage 0

| Signal | Seuil finding | Justification |
| --- | ---: | --- |
| Noir | ≥ 3,0 s | les fondus de 1,5–2,2 s sont normaux |
| Gel | ≥ 5,0 s | les photos fixes et ralentis de 3,4 s sont normaux |
| Segment court | < 0,08 s | les flash-cuts de 0,24 s sont normaux |
| Couverture timeline | > 5 s | les cartons de fin de 3 s sont normaux |

Les intervalles noir/gel sont conservés dans `stage0_evidence` même quand ils
ne déclenchent pas de finding.

## Stage 1 OCR

| Paramètre | Valeur |
| --- | --- |
| Détection | `PP-OCRv6_det_small.onnx` (9,5 Mo) |
| Reconnaissance | `PP-OCRv6_rec_small.onnx` (21,2 Mo) |
| Licence | Apache-2.0 (RapidOCR / PaddleOCR) |
| Runtime | ONNX Runtime CPU via `ort` |
| Bande lue | 75 % à 98,5 % de la hauteur |
| Entrée détection | côté court 480 px, dimensions alignées sur 32, BGR 0–255 |
| Post-traitement DB | seuil 0,30, dilation 2×2, composantes 8-connexes, score ≥ 0,50, unclip 1,6 |
| Entrée reconnaissance | hauteur 48 px, lots de 6, CTC blank à l'index 0 |
| Score de reconnaissance | ≥ 0,55 |
| Texte | ≥ 2 caractères |
| Boîte | hauteur 24–140 px, largeur ≥ 40 px et ≤ 95 % du cadre |
| Forme | aspect ≥ 2,0, aire ≤ 15 % du cadre |
| Position | centre x dans 12–88 %, haut ≥ 76 %, bas ≤ 98,5 % |
| Agrégation | 3 échantillons consécutifs, dérive verticale ≤ 4 % |
| Sévérité | `medium` à 3 échantillons, `high` à partir de 4 |

Deux écarts au PoC sont assumés :

- **boîtes axis-aligned** : les sous-titres broadcast sont horizontaux. Cela
  évite d'embarquer un offsetter de polygones (pyclipper) et un calcul de
  rectangle minimal tourné. Les filtres du PoC utilisaient déjà la bbox
  axis-aligned des quatre points.
- **côté court 480 px** au lieu des 736 px par défaut de RapidOCR. La bande
  d'un export 1080×1920 fait 480 px de haut : la détection tourne à
  l'échelle native, sans suréchantillonnage 1,8×. Les 12 sous-titres
  synthétiques restent détectés avec les mêmes textes et des scores
  équivalents ; le corpus GOOD reste à 0 finding.

La liste des caractères est lue dans les métadonnées ONNX du modèle de
reconnaissance (`character`) : aucun fichier de dictionnaire externe n'est
nécessaire. Le classifieur d'orientation 180° de RapidOCR n'est pas utilisé :
les sous-titres sont horizontaux et la détection DB ne produit pas de
quadrilatères retournés sur ce corpus.

## Stage 1 visage

| Paramètre | Valeur |
| --- | --- |
| Modèle | `face_detection_yunet_2023mar.onnx` |
| Licence | MIT (OpenCV Zoo) |
| Taille | 0,23 Mo |
| Runtime | ONNX Runtime CPU via `ort` |
| Entrée | letterbox 640×640, valeurs 0–255, BGR |
| Score | ≥ 0,60 |
| NMS | IoU 0,30 |
| Padding | 15 % de chaque côté, bord répliqué |
| Taille minimale | 8 % de la largeur et de la hauteur |
| Marge de bord | 3 % de la largeur |
| Agrégation | 3 échantillons consécutifs |
| Sévérité | `medium` à 3 échantillons, `high` à partir de 4 |

Le padding est indispensable : sans lui, un visage dont l'œil sort du cadre
n'est pas détecté. Une détection padded qui recoupe une détection originale
est ignorée, sauf si elle révèle un landmark hors cadre — c'est précisément
le cas que le padding doit rattraper.

## CLI

```sh
workflow-cli qa run \
  --video "/media/.../300 Camille_GOOD/UJUD - JOSEPH_9x16.mp4" \
  --project "/media/.../300 Camille_GOOD/projets/UJUD - JOSEPH_9x16.ecproj" \
  --out "/media/kidpixel/WORK/qa-poc/reports/rust-qa/300-joseph" \
  --models "/media/kidpixel/WORK/qa-poc/models" \
  --onnxruntime "/home/kidpixel/workflow_mediapipe/coral_env/lib/python3.11/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2" \
  --per-segment 1 \
  --video-id "300_UJUD - JOSEPH" \
  --folder-id 300 \
  --split dev
```

`--project` est optionnel : sans lui, `project_readable` est
`not_applicable` et l'échantillonnage retombe sur des fenêtres de 5 s.
`--models` est optionnel : sans lui, `subtitle_ocr` et `face_cut` sont
`not_applicable`. `--no-ocr` et `--no-face` désactivent explicitement un
contrôle. `--per-segment` accepte 1 ou 3. `--no-evidence` n'écrit pas les
frames WebP ; `--no-cache` force le recalcul en mode lot. `--docs` est accepté pour le contrat futur mais non utilisé en
v1. `--ffmpeg` et `--ffprobe` surchargent la configuration
`steps.step8.post`. `--intra-threads` borne les threads intra-op ONNX par
session.

Le binaire sort en code 1 si `overall_status = fail`, 0 sinon.

### Mode lot

`--folder` remplace `--video` : les `.mp4` de la racine du dossier sont
traités en lot, avec le projet `projets/<stem>.ecproj` quand il existe.
`--out` devient la racine du lot ; chaque export écrit
`<out>/<slug>/qa.json` et le lot écrit `<out>/summary.json`.
`--workers` (défaut 4) fixe le nombre d'exports traités en parallèle,
`--limit` borne le lot pour un essai, et `--intra-threads` est calculé par
défaut pour ne pas sursouscrire les cœurs (deux sessions ONNX par worker).
Un export en erreur n'interrompt pas le lot : il est marqué `error` dans le
résumé, et le CLI sort en code 1 si le lot contient un `fail` ou une erreur.

Par défaut, `<out>/<slug>/qa.json` est réutilisé si son `cache_key` correspond
toujours à l'export, au projet, aux modèles et aux options qui changent le
résultat (`per_segment`, OCR, visage, preuves, seuils Stage 0). Le hit est
quasi instantané (0,23 s pour 2 exports mesurés) et marqué `cached: true` dans
le résumé ; `--no-cache` force le recalcul. Un contenu modifié, un modèle
différent ou une option changée invalide l'empreinte. Le cache vit sous
`--out` : aucun état global, aucun TTL.

```sh
workflow-cli qa run \
  --folder "/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/Corpus_Exemple/187 Camille_GOOD" \
  --out "/media/kidpixel/WORK/qa-poc/reports/rust-batch/187" \
  --models "/media/kidpixel/WORK/qa-poc/models" \
  --onnxruntime "/home/kidpixel/workflow_mediapipe/coral_env/lib/python3.11/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2" \
  --workers 4
```

`summary.json` contient `schema`, `generated_at`, `folder`, `workers`,
`intra_threads`, `videos`, `counts` (`pass`, `info`, `review`, `fail`,
`not_applicable`, `error`), `findings_total` et `items` (identifiants,
chemins, statut, nombre de findings, latence, `cached`, `export_sha256`,
`qa_path` ou `error`).

## Schéma `qa.json`

Champs principaux : `schema_version`, `generated_at`, `video_id`,
`folder_id`, `split`, `video`, `export_sha256`, `project`, `specs`,
`project_meta`,
`project_structure`, `checks`, `findings`, `stage0_evidence`,
`stage1_ocr_evidence`, `stage1_face_evidence` (si le contrôle tourne),
`overall_status`, `auto_approve_eligible`, `auto_approve_reason`,
`models_provenance`, `execution_metrics.timings_ms`.

`export_sha256` est le SHA-256 du MP4 contrôlé. `cache_key` est l'empreinte
des entrées qui ont produit le rapport ; vide dans les rapports antérieurs au
cache. Chaque finding peut porter
`evidence_frame`, chemin relatif au répertoire `--out` (par exemple
`evidence/001_face_cut_7.50.webp`). `timings_ms.evidence_ms` mesure
l'extraction des preuves ; un échec ffmpeg laisse le finding sans preuve et
n'invalide pas le rapport.

`stage1_ocr_evidence` contient les deux modèles, le nombre d'échantillons,
`samples_with_text`, `detections_total`, `findings_total`, `frame_errors`,
`per_segment`, le score minimal et le début de la bande. `timings_ms` ajoute
`stage1_ocr_ms` ; `models_provenance.ocr_det` porte le SHA-256 de la
détection et `models_provenance.ocr_rec` celui de la reconnaissance.

`stage1_face_evidence` contient le modèle, le nombre d'échantillons, les
compteurs `samples_with_face`, `samples_with_cut`, `frame_errors`, et les
paramètres (`per_segment`, score, NMS, padding). `timings_ms` ajoute
`stage1_face_ms` et `models_provenance.face` porte le SHA-256 et la licence.

Les findings Stage 0 utilisent `timecode_start_sec` / `timecode_end_sec`,
`severity`, `confidence`, `detected_by_stage` et `notes`. Les findings OCR
utilisent `detected_by_stage = stage1_ppocr_bottom` et résument dans `notes`
les textes lus, les segments, les scores moyen/max et le centre vertical. Les
findings visage ajoutent `bbox_xywh` et
`detected_by_stage = stage1_yunet_face`. Le schéma sera figé quand les logos
seront portés.

## Validation

### Stage 0

- 35 exports évaluables, 35 `pass`, 0 finding.
- Comparaison avec le Stage 0 Python : **0 écart** de statut, de nombre de
  findings et de statut des checks.
- Temps médian 12,3 s et max 21,8 s par export en release, sur 73,5 min de
  corpus.
- 295 : `template_a6 = pass` (`sdm.png` détecté).
- 298 : `pass` avec `freeze_frames = info` et `timeline_coverage = info`,
  comme le PoC Python.
- 179 sans projet : `project_readable = not_applicable`, `pass`.

### Stage 1 OCR

- 35 exports, 1 124 échantillons, 9 avec texte, 12 détections isolées,
  **0 épisode de 3 échantillons consécutifs**, donc 0 finding.
- **0 faux positif** sur les 35 GOOD. Le PoC Python, avec 3 échantillons par
  segment, trouvait 16 détections isolées sur les mêmes exports, également
  sans finding.
- Synthétiques : 12/12 sous-titres détectés (`center`, `two_lines`, `left`,
  `small`, `box`, `fast`, `high78`, `low94`, `small32`,
  `white_no_outline`, `yellow_outline`, `lower`) ; le contrôle `187_control`
  reste à 0 finding. Ces clips n'ont pas de piste audio : Stage 0 les passe
  en `review` sur `audio_codec`, indépendamment de l'OCR.
- Les textes lus aux mêmes instants sont identiques au PoC. `sub_fast`
  contient plusieurs phrases successives : la liste collectée dépend du
  nombre d'échantillons, mais le finding reste stable.
- Temps OCR médian 21,7 s et max 43,5 s par export ; QA complet médian
  35,4 s et max 63,6 s. La cible de 60 s est tenue en médiane et dépassée de
  3,6 s sur le pire export (`300_UJUD - KATIA`, 41,6 s d'OCR).
- 66 tests unitaires `qa` (modèles, parsing noir/gel, specs, segments,
  template, agrégation, écriture atomique, letterbox, décodage YuNet, NMS,
  padding, décodage CTC, composantes, filtres OCR, slugs, scan de lot,
  sélection de l'instant de preuve et empreinte de cache).

### Stage 1 visage

- 35 exports, 1 124 échantillons, 745 avec visage, 25 échantillons avec un
  visage coupé, **0 épisode de 3 échantillons consécutifs**, donc 0 finding.
- **0 faux positif** sur les 35 GOOD ; comparaison PoC : 26 échantillons
  coupés isolés, 0 finding.
- Synthétiques : `left_cut` et `right_cut` détectés (1 finding chacun),
  `center` et `profile_shift` non flagués (0 finding).

### Mode lot

- 35 exports, `--no-ocr --no-face --workers 4` : 140,5 s de mur, soit
  **4,01 s/export** (hash inclus), max 25,6 s par export → cible G1 tenue.
- 5 exports complets : `--workers 1` 156,4 s (31,3 s/export) contre
  `--workers 5` 71,5 s (14,3 s/export), soit **×2,19**. Les latences
  individuelles du résumé sont gonflées par la contention ; c'est le débit
  du lot qui compte.
- Un `.mp4` corrompu produit `status = error` sans interrompre le lot ; le
  CLI sort en code 1 et `summary.json` conserve l'erreur de l'export.

## Limites v1

- Logos non mesurés ; le baseline OCR du PoC était insuffisant (15/35 flags,
  dont 13 habillages éditoriaux).
- Détection OCR axis-aligned, sans classifieur d'orientation 180° : les
  sous-titres tournés ou verticaux ne sont pas couverts.
- Côté court de détection fixé à 480 px : réglé pour les exports 9:16
  1080×1920 du corpus. Un format très différent doit être revalidé.
- Les modèles YuNet et PP-OCRv6 ne sont pas versionnés : ils doivent être
  fournis via `--models`.
- Preuves limitées à une frame par finding ; pas de vignette de contexte ni
  de galerie.
- Cache local par empreinte, sans TTL ni purge automatique ; un rapport
  périmé est simplement recalculé. Pas de serveur.
- Chaque échantillon est extrait par un processus ffmpeg ; les deux contrôles
  Stage 1 tournent en parallèle dans chaque worker. Le cache évite de
  recalculer un export inchangé, mais un premier passage reste complet.
- Un lot ne supprime pas les `qa.json` des exports absents du dossier ; le
  `summary.json` fait foi pour le lot courant.
- Pas de `--docs` exploité : le contrôle A6 se fait par `sdm.png` dans le
  `.ecproj`.

## Prochaines étapes

1. Construire le masque des overlays projet et la galerie labellisée avant
   DINOv2 pour les logos.
2. Réserver le VLM local aux frames ambiguës (phase 4).
3. Figer `qa.json` et l'ajouter au contrat si le serveur l'expose un jour.
4. Faire relire les 12 labels dev pré-remplis avant d'étendre le corpus.
