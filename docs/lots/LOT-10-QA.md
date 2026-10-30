# Lot 10 — contrôle qualité read-only après export humain

> Statut : module Rust v1 démarré le 2026-10-30. Stage 0 déterministe et
> agrégation intégrés ; OCR sous-titres et visage coupé validés dans le PoC
> Python mais pas encore portés en Rust.
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
- Pas de fallback Python : le module Rust est autonome pour Stage 0.
- `auto_approve_eligible` reste **false** en v1. Les logos ne sont pas
  mesurés et l'OCR/visage Rust n'est pas encore porté.
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
2. **Stage 1 — spécialistes** : OCR sous-titres et visage coupé marqués
   `not_applicable` tant que le portage Rust n'est pas fait. Le PoC Python a
   validé les algorithmes (0 faux positif sur 35 GOOD, 12/12 sous-titres et
   2/2 visages synthétiques).
3. **Stage 2 — agrégation** : un `fail` de check ou un défaut objectif
   (`spec_mismatch`, `template_logo_missing`) bloque ; tout autre finding ou
   check `review` demande une revue ; `info` et `not_applicable` ne dégradent
   pas le statut.
4. **Rapport** : `qa.json` avec specs, méta projet, checks, findings, preuves
   Stage 0, statut global et métriques.

## Seuils Stage 0

| Signal | Seuil finding | Justification |
| --- | ---: | --- |
| Noir | ≥ 3,0 s | les fondus de 1,5–2,2 s sont normaux |
| Gel | ≥ 5,0 s | les photos fixes et ralentis de 3,4 s sont normaux |
| Segment court | < 0,08 s | les flash-cuts de 0,24 s sont normaux |
| Couverture timeline | > 5 s | les cartons de fin de 3 s sont normaux |

Les intervalles noir/gel sont conservés dans `stage0_evidence` même quand ils
ne déclenchent pas de finding.

## CLI

```sh
workflow-cli qa run \
  --video "/media/.../300 Camille_GOOD/UJUD - JOSEPH_9x16.mp4" \
  --project "/media/.../300 Camille_GOOD/projets/UJUD - JOSEPH_9x16.ecproj" \
  --out "/media/kidpixel/WORK/qa-poc/reports/rust-stage0/300_joseph" \
  --video-id "300_UJUD - JOSEPH" \
  --folder-id 300 \
  --split dev
```

`--project` est optionnel : sans lui, `project_readable` est
`not_applicable`. `--docs` est accepté pour le contrat futur mais non utilisé
en v1. `--ffmpeg` et `--ffprobe` surchargent la configuration
`steps.step8.post`.

Le binaire sort en code 1 si `overall_status = fail`, 0 sinon.

## Schéma `qa.json`

Champs principaux : `schema_version`, `generated_at`, `video_id`,
`folder_id`, `split`, `video`, `project`, `specs`, `project_meta`,
`project_structure`, `checks`, `findings`, `stage0_evidence`,
`overall_status`, `auto_approve_eligible`, `auto_approve_reason`,
`models_provenance`, `execution_metrics.timings_ms`.

Les findings Stage 0 utilisent `timecode_start_sec` / `timecode_end_sec`,
`severity`, `confidence`, `detected_by_stage` et `notes`. Le schéma sera figé
quand OCR/visage seront portés.

## Validation

- 35 exports évaluables, 35 `pass`, 0 finding.
- Comparaison avec le Stage 0 Python : **0 écart** de statut, de nombre de
  findings et de statut des checks.
- Temps médian 12,3 s et max 21,8 s par export en release, sur 73,5 min de
  corpus.
- 295 : `template_a6 = pass` (`sdm.png` détecté).
- 298 : `pass` avec `freeze_frames = info` et `timeline_coverage = info`,
  comme le PoC Python.
- 179 sans projet : `project_readable = not_applicable`, `pass`.
- 21 tests unitaires `qa` (modèles, parsing noir/gel, specs, segments,
  template, agrégation, écriture atomique).

## Limites v1

- OCR sous-titres et visage coupé non portés en Rust : `not_applicable`.
- Logos non mesurés ; le baseline OCR du PoC était insuffisant (15/35 flags,
  dont 13 habillages éditoriaux).
- Pas de frames de preuve WebP pour l'instant ; seules les métadonnées et
  intervalles sont écrits.
- Pas de cache, pas de batch `--folder`, pas de serveur.
- Pas de `--docs` exploité : le contrôle A6 se fait par `sdm.png` dans le
  `.ecproj`.

## Prochaines étapes

1. Porter l'OCR sous-titres et YuNet/BlazeFace en Rust derrière les checks
   `subtitle_ocr` et `face_cut`.
2. Ajouter les preuves WebP et le hash de l'export.
3. Ajouter un mode batch `qa run --folder` et un cache par hash.
4. Construire le masque des overlays projet avant DINOv2.
5. Figer `qa.json` et l'ajouter à `docs/README.md` / au contrat si le serveur
   l'expose un jour.
