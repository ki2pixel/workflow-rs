# Lot 10 — contrôle qualité read-only après export humain

> Statut : module Rust v1 démarré le 2026-10-30. Stage 0 déterministe,
> Stage 1 visage YuNet et agrégation intégrés ; OCR sous-titres et logos
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
- Pas de fallback Python : le module Rust est autonome pour Stage 0 et le
  visage.
- `auto_approve_eligible` reste **false** en v1. Les logos ne sont pas
  mesurés et l'OCR sous-titres n'est pas encore porté.
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
2. **Stage 1 — visage** : YuNet ONNX Runtime CPU sur des frames échantillonnées
   au milieu de chaque segment (ou 10/50/90 % avec `--per-segment 3`). Le
   détecteur tourne sur la frame et sur une version étendue de 15 % de chaque
   côté ; les deux jeux de boîtes sont fusionnés. Seuls les visages dont un
   landmark (yeux, nez, bouche) approche un bord gauche/droit sont retenus,
   après trois échantillons consécutifs. Un gros plan de profil qui coupe la
   nuque ou le menton sans couper les landmarks reste valide.
3. **Stage 1 — OCR** : marqué `not_applicable` tant que le portage Rust n'est
   pas fait. Le PoC Python a validé 12/12 sous-titres synthétiques et
   0 faux positif sur 35 GOOD.
4. **Stage 2 — agrégation** : un `fail` de check ou un défaut objectif
   (`spec_mismatch`, `template_logo_missing`) bloque ; tout autre finding ou
   check `review` demande une revue ; `info` et `not_applicable` ne dégradent
   pas le statut.
5. **Rapport** : `qa.json` avec specs, méta projet, checks, findings, preuves
   Stage 0, preuves visage, statut global et métriques.

## Seuils Stage 0

| Signal | Seuil finding | Justification |
| --- | ---: | --- |
| Noir | ≥ 3,0 s | les fondus de 1,5–2,2 s sont normaux |
| Gel | ≥ 5,0 s | les photos fixes et ralentis de 3,4 s sont normaux |
| Segment court | < 0,08 s | les flash-cuts de 0,24 s sont normaux |
| Couverture timeline | > 5 s | les cartons de fin de 3 s sont normaux |

Les intervalles noir/gel sont conservés dans `stage0_evidence` même quand ils
ne déclenchent pas de finding.

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
  --out "/media/kidpixel/WORK/qa-poc/reports/rust-face/300-joseph" \
  --models "/media/kidpixel/WORK/qa-poc/models" \
  --onnxruntime "/home/kidpixel/workflow_mediapipe/coral_env/lib/python3.11/site-packages/onnxruntime/capi/libonnxruntime.so.1.23.2" \
  --per-segment 1 \
  --video-id "300_UJUD - JOSEPH" \
  --folder-id 300 \
  --split dev
```

`--project` est optionnel : sans lui, `project_readable` est
`not_applicable` et l'échantillonnage retombe sur des fenêtres de 5 s.
`--models` est optionnel : sans lui, `face_cut` est `not_applicable`.
`--no-face` désactive explicitement le contrôle. `--per-segment` accepte 1
ou 3. `--docs` est accepté pour le contrat futur mais non utilisé en v1.
`--ffmpeg` et `--ffprobe` surchargent la configuration `steps.step8.post`.

Le binaire sort en code 1 si `overall_status = fail`, 0 sinon.

## Schéma `qa.json`

Champs principaux : `schema_version`, `generated_at`, `video_id`,
`folder_id`, `split`, `video`, `project`, `specs`, `project_meta`,
`project_structure`, `checks`, `findings`, `stage0_evidence`,
`stage1_face_evidence` (si le visage tourne), `overall_status`,
`auto_approve_eligible`, `auto_approve_reason`, `models_provenance`,
`execution_metrics.timings_ms`.

`stage1_face_evidence` contient le modèle, le nombre d'échantillons, les
compteurs `samples_with_face`, `samples_with_cut`, `frame_errors`, et les
paramètres (`per_segment`, score, NMS, padding). `timings_ms` ajoute
`stage1_face_ms` et `models_provenance.face` porte le SHA-256 et la licence.

Les findings Stage 0 utilisent `timecode_start_sec` / `timecode_end_sec`,
`severity`, `confidence`, `detected_by_stage` et `notes`. Les findings visage
ajoutent `bbox_xywh` et `detected_by_stage = stage1_yunet_face`. Le schéma
sera figé quand l'OCR et les logos seront portés.

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

### Stage 1 visage

- 35 exports, 1 124 échantillons, 745 avec visage, 25 échantillons avec un
  visage coupé, **0 épisode de 3 échantillons consécutifs**, donc 0 finding.
- **0 faux positif** sur les 35 GOOD ; comparaison PoC : 26 échantillons
  coupés isolés, 0 finding.
- Synthétiques : `left_cut` et `right_cut` détectés (1 finding chacun),
  `center` et `profile_shift` non flagués (0 finding).
- Temps visage médian 14,6 s et max 29,5 s par export ; QA complet médian
  28,3 s et max 50,3 s, sous la cible de 60 s.
- 34 tests unitaires `qa` (modèles, parsing noir/gel, specs, segments,
  template, agrégation, écriture atomique, letterbox, décodage YuNet, NMS,
  classification, padding).

## Limites v1

- OCR sous-titres non porté en Rust : `not_applicable`.
- Logos non mesurés ; le baseline OCR du PoC était insuffisant (15/35 flags,
  dont 13 habillages éditoriaux).
- Le modèle YuNet n'est pas versionné : il doit être fourni via `--models`.
- Pas de frames de preuve WebP pour l'instant ; seules les métadonnées,
  boîtes et intervalles sont écrits.
- Pas de cache, pas de batch `--folder`, pas de serveur.
- Le visage tourne sur CPU, deux inférences par échantillon (original +
  padded) ; le batch 4 workers n'est pas encore porté.
- Pas de `--docs` exploité : le contrôle A6 se fait par `sdm.png` dans le
  `.ecproj`.

## Prochaines étapes

1. Porter l'OCR sous-titres (PP-OCRv6 det/rec, bande basse) derrière
   `subtitle_ocr`.
2. Ajouter les preuves WebP, le hash de l'export et le cache par hash.
3. Ajouter un mode batch `qa run --folder` et borner les threads ONNX par
   worker.
4. Construire le masque des overlays projet avant DINOv2.
5. Figer `qa.json` et l'ajouter à `docs/README.md` / au contrat si le serveur
   l'expose un jour.
