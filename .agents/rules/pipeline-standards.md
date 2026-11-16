---
trigger: model_decision
description: Spécifications normatives du pipeline workflow-rs (STEP1 à STEP8, bypass template SDM/LPHDF, post-traitement A5/A6/A7, QA)
globs:
---

# workflow-rs — standards normatifs du pipeline

Complément technique à `.agents/rules/codingstandards.md` détaillant les invariants des étapes et des sous-systèmes.

## 1. Étapes 1 à 4 (Ingestion, Scènes, Audio, Tracking)

- **STEP1 (Extraction & Normalisation)** : scan non récursif de `step1.source_dir` (`.zip`, `.zipx`, `.rar`, `.tgz`, `.7z`, `.tar`, D-016). Assainissement anti-traversée. Liste mensuelle `processed_archives.txt` et suppression archive post-succès. Normalisation MP4 H.264 progressif 25 fps yuv420p (`yadif` si entrelacé, repli `libx264`). Préservation des `.mov` alpha à l'octet près (D-010).
- **STEP2 (TransNetV2)** : FFmpeg `rawvideo` 48×27 25 fps, fenêtres 100/pas 50/padding 25. `ort` avec `CUDAExecutionProvider`, cuDNN exhaustif, TF32 désactivé. CSV `\r\n` avec en-tête Python ; aucun repli CPU silencieux.
- **STEP3 (Audio)** : YAMNet Split INT8 sur Edge TPU ; log-mel CPU ; ECAPA-TDNN ONNX CPU. AHC moyenne, cosinus, seuil 0,32. Sans audio : `SKIP_NO_AUDIO`, aucune sortie, code 0 (D-002).
- **STEP4 (Tracking)** : BlazeFace 128×128, FaceMesh 192×192, EfficientDet 300×300, One-Euro 52D. `<stem>.csv` et `<stem>_audio.json` requis par défaut (D-004). JSON streamé vers temporaire puis renommé.

## 2. Étapes 5 à 8 (Réduction, AE, Finalisation, EffectCraft)

- **STEP5/STEP6** : streaming (< 100 Mio RSS). STEP5 écrit `fps`/`total_frames` avant `frames_analysis` (D-003). STEP6 produit `<stem>_ae.json` avec `dataByFrame`, `audioByFrame`, `segment_recommendations`, `is_speaking`, `tier`.
- **STEP7 (Finalisation)** : archive par SHA-256 vidéo vers `paths.finalized`, manifeste `finalized_projects_<ts>.json`, suppression source (`--no-delete`/`--dry-run` en test).
- **STEP8 (EffectCraft headless)** : `effectcraft-cli run prefs.set … file.runScript` (`scripting.allowScriptsWriteFiles` activé), panel `MediaSolution.jsx` + driver, signaux `MS_BATCH_*`, SIGTERM vers groupe enfant.

## 3. Template direct et bypass (D-034, D-035, D-061, D-080)

- Profil strict : MP4 H.264 1920×1080, pixel aspect 1, 25 fps (±0,1). Requiert `ffprobe`.
- `template_prefixes` (`M6 - SDM - `, `SDM - `) : saute STEP2–6, STEP7 synthétise un `_scenes.csv` mono-segment, STEP8 génère brut puis assist A6.
- `mission_template = true` : second déclencheur, consulté seulement après échec des préfixes. Une mission unique du lot doit demander exclusivement `sdm.png` (ni `Pas de logo`, ni logo non résolu, ni second logo). Lot ambigu, cache absent ou vidéo hors profil : repli sur le nommage et pipeline normal.
- `archive_only_prefixes` (`W9 - LPHDF`) : STEP7 archive le CSV, STEP8 ignore la vidéo sans `.ecproj`. Le mode archive-only reste strictement basé sur le nom. Préfixes vides ou chevauchants interdits.
- A6 trace `template.trigger` (`name` / `mission` / `none`) et `template.mission` ; `template.prefix` reste vide pour un déclenchement mission.

## 4. Post-traitement A5/A6/A7 (D-030 → D-069)

- **Immutabilité du brut** : le `.ecproj` brut n'est jamais modifié. Publication de `<stem>_assist.ecproj`, sidecar `<stem>_mission.json` (`schema: 2`) et rapports `post.json`, `review.json`, `audit.json` sous `<docs>/post/<projet_assisté>/`.
- **A1** : veto pillarbox latéral : barre gauche/droite ≥ `round(32 × h_source / 1080)` rétrograde l'`apply` en `review` (confiance 0,4, D-063).
- **A2** : `a2_logo_prior` (registre spatial, D-062). Conflit A1/A2 arbitré en `review`.
- **A7 (Missions)** : `requested`, `source` (`local_docs` / `central_fallback`), `known_absent` additifs. Local `docs/` prioritaire, repli sur `assets/mission_overlays` (D-066). `tail_overlay` conserve `tail_s = 0` et coupe l'audio. Durée > main + 1 ms bascule en `review` (D-065).
- Échec du post = échec STEP8. Options : `--no-post`, `--no-email`, `post run --report-only`.

## 5. Contrôle Qualité QA read-only (D-036 à D-049, D-067)

- Exécution post-export humain uniquement (`qa run`), distincte de STEP8. Lecture seule sur MP4, `.ecproj` et `docs/`.
- Sorties sous `--out` : `qa.json` (`QA_SCHEMA_VERSION: 1.5.0`, `CACHE_VERSION: 12`), frames de preuve WebP (`evidence/`), et `summary.json` en mode dossier.
- Masque overlays : analyse les calques `.mov` avec alpha (12–24 frames), ignore les MOV instables avec warning non-bloquant sans provoquer de `fail`.
