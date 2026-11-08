# Contexte actif

## Tâche en cours

**Registre spatial des logos source — phases A/B (2026-11-08, D-052).** Le
catalogue versionné `assets/models/qa/source_logos.toml` contient les 29
émissions documentées par le client (position, bbox d'enveloppe, variantes,
sources exactes), 15 clean feeds et 5 émissions sans logo ; les crops RGBA
restent externes. `vision/source_logo.rs` résout la source et projette la
bbox par les transformations du `.ecproj` (keyframes compris). A2 ajoute
`details.source_logo` derrière `a2_logo_prior = false` et, quand un prior
est actif, retire du groupe OCR dominant les détections majoritairement
dans la bbox du logo (`details.ocr.logo_texts`, `logo_detections_total`,
`logo_only`) ; un logo seul part en `review`. Sans prior, ces clés restent
absentes. Le QA ajoute `source_logo` (`pass`/`info`) derrière
`--source-logo`. Le catalogue est généré par
`scripts/import-source-logos.py` depuis le CSV client. Rejeu réel :
`54`/`58` inchangés (`apply` 200,8 % / 210,0 %) et `UNJOUR`/`UNDOC` déplacés
dans `logo_texts` ; `40` reste `review` mais devient « bande sombre, texte
persistant confirmé » avec la bannière réelle à 91,7 % et un scale proposé
de 218,2 % ; `sub_fast` reste vétoé à 25 %. QA sur 35 exports GOOD avec
`.ecproj` : 25 `pass`, 10 `info`, 0 nouveau finding. Phase C (confirmation
visuelle NCC/contours, stabilité temporelle de la zone générique) non
implémentée ; activation production toujours soumise à validation client.

**QA 1.3.0 — clustering OCR (2026-11-07, D-051).** La phase 1 est
implémentée : `qa/ocr.rs` regroupe les détections par **boîte stable** (le
texte normalisé sert à la dominance), ajoute `DefectType::FixedOverlay`, le
check `fixed_overlay` et `fixed_overlays_total`, passe `QA_SCHEMA_VERSION` à
**1.3.0** et `CACHE_VERSION` à **7**. Un bandeau persistant (≥ 60 %) et
dominant (≥ 80 %) devient `fixed_overlay = review` ; un texte changeant
reste `subtitle_burned_in`. Validation réelle sous
`/tmp/workflow_investigation/qa13b/` : un `fixed_overlay` sur chacun des
quatre rushes `40 ABHARTACH`, `40 FOOTBALLEUR`, `54`, `58`, plus un vrai
`subtitle_burned_in` sur `54` auparavant manqué ; `sub_fast` donne un
sous-titre (dominance 29 %), `181_sub_center` un `fixed_overlay`. Tests
workspace, `fmt` et `clippy -D warnings` verts ; aucune écriture dans
`/mnt/cache`.

**Module `vision` (D-051, phase 2).** Les moteurs `frame`, `inference`,
`ocr`, `dino`, `text` et `hash` sont sortis de `qa` vers
`crates/workflow-steps/src/vision/` ; `qa/ocr.rs` ne garde que
l'orchestration des sous-titres et `qa/logo.rs` celle des logos. `post` peut
désormais réutiliser PP-OCRv6 sans dépendre de `qa`. Rejeu QA de `54` :
findings, checks, preuves et provenance identiques, seuls les temps
changent.

**Gate OCR A2 (D-051, phase 3).** `post/a2.rs` expose `A2Options` (marge +
OCR optionnel) et un gate derrière `a2_ocr = false`. Il échantillonne 12
frames, lit le crop du candidat avec PP-OCRv6, fusionne les boîtes d'une
même ligne, mesure persistance/dominance/dérive/score/recouvrement et
oppose un veto au texte changeant. Garde-fous durs inchangés, override de
géométrie borné à 2× `GEOM_STD`, bandeaux sombres en `review` avec scale
proposé, texte trop rare → décision A2 actuelle. Validation : `54` reste
`apply` 200,8 %, `58` passe `review` → `apply` 210,0 %, `40` restent
`review` avec `details.ocr`, `sub_fast` est vétoé (dominance 25 %) ; les
projets assistés ne changent que par `Scale`. Détail :
`docs/etudes/ETUDE-LOGOS-BANDEAUX-POST-QA.md` et
`docs/lots/LOT-9-POST-STEP8.md`.

**Progression STEP8/post-STEP8 dans l'interface.**
La carte STEP8 prolonge sa barre pendant le post-traitement A5/A6 : le CLI
émet `POST_TOTAL`, `POST_CURRENT`, `POST_PROGRESS` et `POST_DONE` sur
`stdout` dans le hook STEP8 (même flux que `MS_BATCH_*`) et sur `stderr`
pour `post run`, le serveur ajoute ces projets au total STEP8 et expose
`progress_phase = "post"`, et
`assets/web` affiche la phase en orange avec le statut « Post-traitement ».
Le détail est dans D-050.

**Contexte QA (production, D-049).**
Le module `crates/workflow-steps/src/qa/` et `workflow-cli qa run` sont intégrés
pour le Stage 0 (specs, durée MP4/composition, timeline, template A6,
noir/gel), le Stage 1 OCR PP-OCRv6 (sous-titres et logos) et le Stage 1 visage
YuNet, tous en ONNX Runtime CPU. `qa.json` est distinct de l'`audit.json`
A5/A6 et s'écrit uniquement sous `--out` ; il porte le hash SHA-256 de
l'export et, par finding, une frame de preuve WebP. Validation Stage 0 sur
les 35 exports : 35 `pass`, 0 écart avec le PoC Python, médiane 12,3 s.
Validation OCR : 1 124 échantillons, 12 détections isolées, 0 finding sur
35 GOOD, 12/12 sous-titres synthétiques. Validation logos : masque alpha des
overlays `.ecproj`, 30/32 exports évalués `pass`, 2 exports `review` (270,
branding source, 4 findings), 264/386 détections masquées ; galerie
synthétique 60/60 sur 22 fonds (IC 95 % 94,0–100 %) et 0/16 faux positifs ;
assets réels 34/39 (87,2 %, IC 73,3–94,4 %), 0/17 faux positifs, 13/13
masqués, 11/13 à l'échelle de production. Validation visage : 745
échantillons avec visage, 0 finding, 2/2 recadrages synthétiques. Les
sous-titres et les logos partagent le thread OCR ; QA complet 298 en 32,1 s. Le mode lot `--folder` répartit les exports sur
4 workers (Stage 0 : 4,01 s/export) et réutilise les
`qa.json` dont l'empreinte correspond. Le vérificateur visuel DINOv2 ViT-S/14
est actif par défaut, avec `--no-logo-visual` en opt-out : il complète l'OCR
sur les candidats géométriques et atteint 39/39 en union sur la galerie
synthétique, sans nouveau faux positif sur les exports GOOD. Validation
réelle par ablation : 14/14 exports avec overlay non déclaré détectés
(26 findings), 0/14 avec le projet d'origine ; 298 « The 100 » est sauvé par
DINOv2 (2/2). Les overlays et les deux exports 270 (split screen) sont
confirmés intentionnels. Passage en production (D-049) : `auto_approve_eligible`
est vrai pour un `pass` complet avec les contrôles requis exécutés, faux pour
tout `review`, `fail` ou contrôle requis `not_applicable`.
Le post-traitement A5/A6 et les bypass SDM/LPHDF restent en production (D-032,
D-034, D-035).

## Dernière session

- [2026-11-07] **Gate OCR A2 (D-051, phase 3)** :
  - `PostConfig`/`PostOptions` : `a2_ocr` (défaut `false`), `a2_ocr_det`,
    `a2_ocr_rec`, `a2_ocr_onnxruntime` ; `A2Options` regroupe la marge et
    l'OCR, `decide_a2` garde 7 arguments ;
  - 12 frames, crop 16 px sur quatre côtés, PP-OCRv6 2 threads, fusion des
    boîtes d'une même ligne (rapport de hauteur ≤ 1,35, écart borné), texte
    d'au moins deux caractères ;
  - arbre : garde-fous durs d'abord, texte trop rare → décision actuelle,
    veto si dominance < 60 %, confirmation si persistance ≥ 60 %,
    dominance ≥ 80 %, score ≥ 0,80, `y_drift` ≤ 12 px, overlap ≥ 0,50,
    couverture ≥ 25 % ; override de géométrie borné à 2× `GEOM_STD`,
    `temporal_std` reste dur, bandeaux sombres en `review` + scale ;
  - `details.ocr` expose texte, bbox, features, `texts` (3 groupes) et
    timing ; `post.json` reste schéma 1 ;
  - validation : `54` apply 200,8 % (112 diffs `Scale 180→200,8`), `58`
    review → apply 210,0 % (40 diffs `Scale 180→210`), `40` reviews
    enrichis, `sub_fast` veto « texte changeant 25 % », audits `pass` ;
  - 26 tests `post::a2`, 467 tests workspace, `fmt` et clippy verts.

- [2026-11-07] **Module `vision` (D-051, phase 2)** :
  - `qa/frame.rs`, `qa/inference.rs` et `qa/dino.rs` déplacés vers
    `vision/` ; ajout de `vision/text.rs` (normalisation, distance,
    similarité de boîtes) et `vision/hash.rs` (`sha256_file`) ;
  - `vision/ocr.rs` porte `DetEngine`, `RecEngine`, le décodage CTC et les
    filtres communs ; `qa/ocr.rs` ne garde que la bande basse, le
    clustering et les findings ; `qa/logo.rs` importe les moteurs depuis
    `vision` ;
  - `ModelInfo` déplacé dans `vision/mod.rs` et ré-exporté par
    `qa/model.rs` ; `post::detect` ré-exporte `sha256_file` ;
  - rejeu QA de `54` : findings, checks, preuves et provenance identiques
    (écart uniquement sur les timings) ; `cargo test -p workflow-steps`
    295 passed, `fmt` et clippy verts.

- [2026-11-07] **Implémentation QA 1.3.0 (D-051, phase 1)** :
  - `qa/ocr.rs` : agrégation par boîte (`boxes_similar` : containment,
    IoU ≥ 0,3, ou centres proches sur la même ligne), variantes de texte
    suivies pour la dominance, `MIN_CLUSTER_COUNT = 3`,
    `FIXED_PERSISTENCE = 0.60`, `FIXED_DOMINANT_RATIO = 0.80` ;
  - `qa/model.rs` : `DefectType::FixedOverlay`, `QA_SCHEMA_VERSION = 1.3.0`,
    `fixed_overlays_total` dans `Stage1OcrEvidence` ;
  - `qa/cache.rs` : `CACHE_VERSION = 7` ; `subtitle_ocr` reste `pass` si
    seuls des bandeaux fixes existent, le check `fixed_overlay` porte le
    `review` ;
  - rejeu réel des quatre rushes : 4/4 `fixed_overlay`, 0 régression,
    `54` gagne un `subtitle_burned_in` réel (3 échantillons au-dessus du
    bandeau) ; `sub_fast` → sous-titre (dominance 29 %), `181_sub_center` →
    `fixed_overlay` ; sorties sous `/tmp/workflow_investigation/qa13b/` ;
  - lot de régression OCR sur 36 exports GOOD :
    `/tmp/workflow_investigation/corpus35-qa13/` → 36 `pass`, 0 finding,
    aucun `fixed_overlay` ni `subtitle_burned_in` ;

- [2026-11-07] **Étude logos et bandeaux fixes (D-051)** :
  - audit de `post/a2.rs` et de `qa/ocr.rs`, `qa/logo.rs`, `qa/dino.rs`,
    `qa/overlay.rs` ; benchmark des sept dépôts fournis (YOLOv7, YOLOv8 API,
    open-set YOLOv5, Logodetect, VideOCR, deux détecteurs morphologiques) ;
  - diagnostic en lecture seule de `40`, `54` et `58 Camille` : l'agrégation
    QA par moyenne de bloc manque trois bandeaux persistants sur quatre
    (dérive verticale 0,0562–0,0642 > `MAX_Y_DRIFT = 0.04`) et A2 ne peut
    pas exploiter l'OCR (bandeaux sombres, géométrie de scène instable) ;
  - décision : QA 1.3.0 avec clustering par détection et `fixed_overlay` ;
    A2 v3 avec gate OCR conservateur (`a2_ocr = false`), garde-fous durs
    inchangés, veto sur texte changeant, bandeaux sombres en `review` ;
    extraction ultérieure d'un module neutre `vision` ;
  - application A2 de `54` vérifiée propre : 112 différences JSON, toutes
    des `Scale` 180,0 → 200,8, aucune ancre ni position modifiée ;
  - 442 tests, `fmt` et `clippy -D warnings` verts ; aucune écriture dans
    `/mnt/cache`, preuves sous `/tmp/workflow_investigation/` ;
  - rapport : `docs/etudes/ETUDE-LOGOS-BANDEAUX-POST-QA.md`.

- [2026-11-02] **Progression post-STEP8 dans l'interface (D-050)** :
  - `post/run.rs` émet `POST_TOTAL`, `POST_CURRENT`, `POST_PROGRESS` et
    `POST_DONE` avant/après chaque projet A5/A6, sur `stdout` avec
    `MS_BATCH_*` dans le hook STEP8 et sur `stderr` pour `post run` ;
  - `progress.rs` ajoute ces projets au total EffectCraft et publie
    `progress_phase = "post"` ;
  - `uiUpdater.js` affiche la barre en orange, le badge « Post-traitement »
    et le texte `i/N` du serveur sans réapposer le compteur global ;
  - tests parser avec et sans base EffectCraft, marqueurs vérifiés sur deux
    projets, 442 tests workspace, `fmt`, `clippy` et `node --check` verts ;
  - harnais navigateur sur le vrai `uiUpdater.js` : barre orange, statut
    « Post-traitement », texte `i/N` brut, complétion verte, échec rouge et
    retour idle sans `data-progress-phase`.

- [2026-11-02] **Passage en production du QA (D-049)** :
  - `auto_approve_eligible` calculé par `aggregate::auto_approve` : vrai
    seulement sur un `pass` complet avec `project_readable`, `subtitle_ocr`,
    `logo_residual` et `face_cut` exécutés ; `logo_visual` doit être
    `pass`/`info` quand DINOv2 est actif ;
  - DINOv2 actif par défaut, `--no-logo-visual` en opt-out ; l'ancien
    `--logo-visual` reste accepté sans effet ;
  - schéma `qa.json` 1.2.0, `CACHE_VERSION = 6` ;
  - `summary.json` passe en schéma 2 avec `auto_approve_eligible` par export
    et le compte d'exports auto-approbables ;
  - tests d'agrégation ajoutés (pass complet, review/fail, contrôle requis
    absent ou `not_applicable`, `logo_visual` non exécuté).

- [2026-11-02] **Vérificateur visuel DINOv2 (D-048)** :
  - `qa/dino.rs` : DINOv2 ViT-S/14 quantifié ONNX CPU, probe logistique
    versionné, préprocessing Python (pad carré, bicubique 256, crop 224,
    ImageNet), batch 12 ;
  - `qa/logo.rs` : second étage sur les boîtes OCR 18–220 px, persistance
    3/12, déduplication texte, findings `stage1_dinov2_top` ;
  - intégré en opt-in à cette date, puis passé en opt-out par D-049 ;
    `logo_visual` `not_applicable` si modèle/probe absents ;
    `stage1_logo_visual_evidence`, `models_provenance.embedding`,
    `stage1_logo_visual_ms` ; schéma final `qa.json` 1.2.0, cache 6 ;
  - galerie 56 clips : union **39/39** (34 texte + 5 visuels : Gulli 0,5,
    Les 100 1,0, M6+ ×3), 0/13 masqués, 0/4 négatifs ;
  - corpus 32 exports avec projet : 0 nouveau `pass` signalé, 270 toujours
    `review` ; 736 candidats, 465 au-dessus du seuil ;
  - validation réelle par ablation : 14 GOOD avec 16 overlays image, projet
    copié sans les calques image ; 0/14 finding avec le projet d'origine,
    **14/14** avec la déclaration retirée (26 findings, 19 texte, 7 DINOv2),
    298 « The 100 » sauvé par DINOv2 (2/2) ; overlays et 270 confirmés
    intentionnels par le client ; détail dans
    `/media/kidpixel/WORK/qa-poc/reports/real-ablation/` ;
  - temps DINOv2 1,25–1,31 s par export isolé, jusqu'à 5,9 s sous 4 workers ;
  - 439 tests workspace (407 `#[test]` + 32 `#[tokio::test]`), `fmt` et
    `clippy -D warnings` verts ;
  - docs : `LOT-10-QA.md`, note d'architecture, `POC-RESULTS.md`
    §2.10–2.11, `assets/models/qa/`, D-048 et D-049.

- [2026-11-01/02] **Stage 1 logos, masque overlays et galeries (D-041 à D-044)** :
  - `qa/overlay.rs` : calques image du `.ecproj`, bbox alpha via `png`,
    transformations statiques, résolution `docs/`, canonicalisation des
    liens symboliques ;
  - `qa/logo.rs` : OCR bande haute 0–32 %, 12 frames, persistance ≥ 50 %,
    masque si ≥ 50 % de la détection est couverte, findings `logo_residual` ;
  - `--no-logo`, `overlays_skipped`, `stage1_logo_evidence` et
    `stage1_logo_ms` ; le cache passe en `cache_version=2` ;
  - lot 35 exports : 30 `pass`, 3 `not_applicable` (sans projet), 2 exports
    `review` (270, 4 findings), 264/386 détections masquées, 6,25 % de faux
    positifs connus ; logo confirmé dans les rushes ;
  - synthétiques : logo persistant détecté, contrôle masqué propre, texte
    transitoire non flagué ;
  - galerie texte 76 clips sur 22 fonds : 60/60 positifs (24–96 px, IC 95 %
    94,0–100 %), 0/16 faux positifs (8 masqués, 4 transitoires, 4 négatifs),
    177,2 s ;
  - galerie assets réels 56 clips (13 PNG × 3 échelles, split dev/holdout
    7/6) : 34/39 positifs (87,2 %, IC 73,3–94,4 %), 0/17 faux positifs,
    13/13 masqués ; 1,0 : 11/13, 0,75 : 12/13, 0,5 : 11/13 ;
  - regroupement spatial et containment (D-044, D-047) : M6+/Gulli à 0,75
    et e=m6 à 0,5 récupérés ; M6+ (toutes échelles), Les 100 à 1,0 et
    Gulli à 0,5 restent non détectés ;
  - test 736 rejeté (D-045) : 22/26 contre 24/26 à 480, boîtes fusionnées ;
  - labels dev régénérés depuis `reports/rust-logo` (plus de baseline) ;
  - QA complet 298 : 32,1 s (OCR 17,2 s, logos 7,4 s, visage 14,2 s) ;
  - 98 tests `qa`, suite 439 tests, `fmt` et `clippy -D warnings` verts ;
  - docs : `LOT-10-QA.md`, note d'architecture, POC-RESULTS,
    D-041 à D-047.

- [2026-11-01] **Stage 1 OCR, lot, preuves et cache (D-038 à D-040)** :
  - `qa/ocr.rs` : détection DB et reconnaissance CTC sur la bande basse
    75–98,5 %, CPU via `ort`, liste de caractères lue dans les métadonnées ;
  - détection côté court 480 px, boîtes axis-aligned, unclip 1,6, filtres
    géométriques du PoC et agrégation sur 3 échantillons consécutifs ;
  - `qa run --no-ocr`, `--models` fournit det/rec ; sans modèle,
    `subtitle_ocr` est `not_applicable` ;
  - Stage 1 OCR et visage en parallèle, échantillonnage partagé
    (`qa/sample.rs`, `qa/frame.rs`, `qa/inference.rs`) ;
  - 35 GOOD : 12 détections isolées, 0 finding, 0 faux positif ; 12/12
    synthétiques, contrôle `187_control` propre ; QA complet médian 35,4 s,
    max 63,6 s ;
  - mode lot `--folder` : scan, workers, `summary.json`, threads ONNX
    bornés ; Stage 0 35 exports en 4 workers = 4,01 s/export ;
  - `export_sha256`, `--no-evidence` et une frame WebP par finding
    (`<out>/evidence/`) ; échec ffmpeg non bloquant ;
  - cache local `cache_key` : 2 exports en 0,23 s sur hit, `--no-cache` pour
    forcer le recalcul ;
  - 66 tests `qa`, suite 407 tests, `fmt` et `clippy -D warnings` verts ;
  - docs : `LOT-10-QA.md`, note d'architecture, README, POC-RESULTS, D-038,
    D-039 et D-040.

- [2026-10-30] **Stage 1 visage YuNet (D-037)** :
  - `qa/face.rs` : YuNet ONNX CPU, letterbox 640×640, frame originale +
    padding 15 %, fusion conditionnelle des détections padded ;
  - critère landmarks à 3 % des bords, taille minimale 8 %, score 0,60,
    NMS 0,30, agrégation sur 3 échantillons consécutifs ;
  - `qa run --models --onnxruntime --per-segment`, `--no-face` pour désactiver ;
  - 35 GOOD : 1 124 échantillons, 745 avec visage, 25 coupés isolés, 0 finding,
    0 faux positif ; synthétiques 2/2 détectés, 0/2 faux positifs ;
  - QA complet médian 28,3 s, max 50,3 s ; 34 tests `qa`, suite 375 tests,
    `fmt` et `clippy -D warnings` verts ;
  - docs : `LOT-10-QA.md`, note d'architecture, README et D-037.

- [2026-10-30] **Contrôle QA read-only — Stage 0 Rust (D-036)** :
  - module `qa/` (`model`, `stage0`, `aggregate`, `report`) et commande
    `workflow-cli qa run` ; `qa.json` séparé, écriture atomique sous `--out` ;
  - Stage 0 : ffprobe, specs, durée MP4/composition, timeline, template A6,
    `blackdetect`/`freezedetect`, seuils hauts (noir 3 s, gel 5 s, segment
    0,08 s, couverture 5 s) ;
  - 35 exports : 35 `pass`, 0 finding, 0 écart vs PoC Python ; médiane 12,3 s,
    max 21,8 s en release séquentiel ;
  - 21 tests `qa`, suite 362 tests, `fmt` et `clippy -D warnings` verts ;
  - OCR/visage/logos `not_applicable`, `auto_approve_eligible = false` ;
  - docs : `docs/lots/LOT-10-QA.md`, note d'architecture, README, index docs.

- [2026-10-29] **Bypass archive-only LPHDF (D-035)** :
  - `archive_only_prefixes = ["W9 - LPHDF"]` sous `[steps.step8.post]` ;
  - STEP2–STEP6 : même prédicat que SDM, `SKIP_TEMPLATE` avant modèles ;
  - STEP7 : CSV mono-segment archivé (`_scenes.csv`) et copié (`.csv`),
    copie `/mnt/cache` et suppression de source inchangées ;
  - STEP8 : skip par nom, aucun `.ecproj`, panel ignoré pour un dossier
    100 % LPHDF, noms injectés en minuscules dans un dossier mixte ;
  - post-traitement : dossiers/projets LPHDF résiduels ignorés ;
  - validation 341 : 4 CSV (`5259`, `5293`, `5240`, `5253` frames), aucun
    `projets/` ; mixte 303 + LPHDF : seul le projet ordinaire est produit,
    identique hors version EffectCraft et chemins ;
  - 341 tests, `fmt`, `clippy -D warnings` et `config check` verts.

- [2026-10-28] **Bypass direct SDM (D-034)** :
  - `template.rs` : sonde ffprobe, prédicat MP4 H.264 1920×1080 SAR 1:1
    25 fps CFR, CSV mono-segment au format STEP2, brut depuis
    `assets/effectcraft/sdm_raw_template.json` ;
  - STEP2–STEP6 : `SKIP_TEMPLATE` avant modèles et verrou matériel ;
  - STEP7 : SDM prête en `lenient` (strict inchangé), CSV synthétique dans l'archive
    (`_scenes.csv`) et la copie finale (`.csv`), `scenes_origin` dans
    `metadata.json`, source intacte ;
  - STEP8 : brut direct + assist A6, panel évité pour un dossier 100 % SDM,
    `videoSkipNames` en minuscules pour un dossier mixte ; brut créé même
    avec `--no-post` ;
  - validations : 295 STEP7→STEP8 rendu identique au pixel à 0/10/30/50 s ;
    340 CSV 531 frames et rendu conforme ; lot mixte 303 + 340 non-SDM
    identique octet pour octet (chemins normalisés) et rendu identique ;
  - config `direct = true` après validation ; docs LOT-9, README et D-034.

- [2026-10-28] **Synchronisation documentaire post-audit** :
  comptes de tests ramenés à 315 (283 `#[test]` + 32 `#[tokio::test]`) dans
  la memory-bank et `.agents/` ; `parity/DRIFT.md` D-015 corrigé
  (`/api/system_monitor` est réel, `/api/performance/frontend` reste un stub) ;
  contrat en version 1.5 (nomenclature `_assist.ecproj`, panneau
  `assets/effectcraft/MediaSolution.jsx`, 161 clés TOML et sections réelles) ;
  skill `workflow-operator` complétée pour le hook A5/A6. Aucun code ni
  artefact de production modifié.
- [2026-10-28] **Documentation réorganisée en six domaines (D-033)** :
  `docs/` passe de 23 fichiers à plat à `architecture/`, `lots/`,
  `exploitation/`, `decisions/`, `etudes/` et `validation/`, avec un index
  `docs/README.md`. 24 fichiers déplacés par `git mv`, 211 renvois réécrits
  dans 30 fichiers. Les chemins applicatifs `<projet>/docs/` et `docs/post/`
  restent inchangés.
- [2026-10-28 02:16] **Fermeture du terminal = arrêt du serveur** :
  `start-rust.sh` pose son trap avant le préflight et arrête le serveur avant
  toute écriture sur le pty. Sous `setsid`, `workflow-server` survivait à la
  fermeture de la fenêtre : le `echo` de `cleanup` échouait (EIO) et `set -e`
  interrompait le trap avant `switch-rust.sh stop`. Validé en pty réel :
  fermeture (SIGHUP) et Ctrl+C envoient SIGTERM et nettoient le PID ; GNOME
  Terminal conserve son dialogue `confirm-close`.
- [2026-10-28 01:32] **Premier batch production A5/A6 (`223 Camille`)** :
  2 vidéos, STEP2–8 7/7 `completed`, post 2 assist, 0 apply, 13 review,
  84 noop, audit `pass` ; brut/assist identiques et artefacts STEP4/5/6
  intacts. Revues A1/A2/A3/A4 attendues ; A6 non déclenché.
- [2026-10-27] **Post-traitement A5/A6 en production (D-032)** :
  - module Rust `crates/workflow-steps/src/post/` : parseur `.ecproj`,
    détecteurs ffprobe/cropdetect/bandes, A1, A2, A3, A4, A6, audit ;
  - hook après STEP8 réussi et commande `workflow-cli post run` ;
  - invariant : brut **octet pour octet inchangé**, `_assist.ecproj` toujours
    publié (copie exacte si aucune décision), artefacts additifs sous
    `docs/post/<projet_assisté>/` ;
  - A1/A2 ne déplacent jamais l'Anchor Point ; A3/A4 en revue ; A6 éligible
    court-circuite A1–A4 ; conflit A1/A2 → revue ; échec de post → STEP8
    sort en code non nul ;
  - validations : 295 assist identique à la référence et rendu 0/255 à
    0/10/30/50 s ; décisions A1/A2/A3/A4 identiques au prototype sur 268,
    187 DISNEYWORLD, 187 ECOLE, 191 et 181 ; 270/276/286/287 0 apply et
    assist identique au brut ; A2 synthétique 11/11 ; idempotence 187 ECOLE ;
  - docs : `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-1-STEP8.md`,
    `docs/architecture/CONTRAT-ENVIRONNEMENT.md`, `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` §3.9,
    `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` §14.1, `README.md`, `AGENTS.md` ;
  - prochaines étapes : exposer `review.json` en lecture seule dans l'UI,
    recalibrer A3/A4, traiter les références manquantes 179/287.
- [2026-10-27] **Spike STEP9 — A5 v2 et A6 intégré** :
  - hash vidéo **échantillonné** (début/milieu/fin, 3 × 4 Mio) dans
    `audit.json` ; `timings_ms` par étage dans `post.json`/`audit.json` ;
  - A1 : 10e percentile des hauteurs actives, couverture mathématique de tous
    les crops, keyframes en `review` ; 268 15 applies à 231,1 % + Y −287,9 px,
    191 37 applies à 249,6 % + Y −367,8 px enfant ; 181 Concierge 0 apply,
    10 barres asymétriques en `review` ;
  - A2 : scan haut+bas, bandes sombres par paires de bords, stabilité
    géométrique des bords (centre/hauteur > 12 px → `review`), logo/partiel/
    décor en `review` ; 187 DISNEYWORLD/ECOLE 206,6 %, 191 logo review,
    181 noop ; validation synthétique 11/11 (3 apply, 7 review, 1 noop) ;
  - A5 coûte 3,7–33,5 s par vidéo (0,07–0,24× la durée, sans rendu) ;
  - A6 : `a5_report.py --template sdm` émet la décision et le bloc
    `template` ; 295 apply/audit pass, 295 keyframé review, 187 noop ;
    l'écriture du projet reste dans `a6_apply.py --apply` ;
  - rapport : `/tmp/opencode/step9-spike/06-corpus/a5v2/RAPPORT-A5-V2.md` ;
  - prochaine étape : brancher A5 v2/A6 dans le post-traitement de
    production (hors panel), puis traiter les références manquantes 179/287.
- [2026-10-27] **Spike STEP9 — A5 et évaluation** :
  - `03-scenario-a/a5_report.py` : A0/A1/A2 + candidats A3/A4, écriture de
    `post.json`, `review.json`, `audit.json` sans modifier le `.ecproj` ;
  - 4 cas exécutés : 268, 187 DISNEYWORLD, 187 ECOLE, 191 cancer ;
  - A2 : 206,6 % vs 207 % sur 187 (0,4 %) ; décor statique de 187 ECOLE
    séparé ; logo de 191 en `review` ;
  - A1 : 231,1 % (268) dans la plage humaine 221,3–239,7 % ; 249,6 % vs
    255 % (191, 2,1 %) ; garde-fou de barres asymétriques (faux positif 187
    segment 14 évité) ;
  - validation visuelle sur copies relinkées : A2 206,6 % supprime la
    bannière de 187 aux timecodes 10/40/100 s ; A1 249,6 % sur 191 laisse
    19,5 % de barre en haut (ancre décentrée), mais la compensation
    Position Y de -417,8 px supprime les barres → A5 prescrit scale +
    compensation et signale le cadrage vertical en `review` ;
  - A3 : précision 0,29–0,60, rappel 0,20–0,43, jamais appliqué ;
  - A4 : cut dur intra-segment désactivé par défaut (32 faux positifs sur
    187), seuls segments courts et bornes CSV sont signalés ;
  - rapport : `/tmp/opencode/step9-spike/06-corpus/a5/RAPPORT-A5.md` ;
  - prochaine étape : finaliser le seuil de compensation A1, durcir A2
    (bannière sombre/animée, logo de coin), intégrer A6 avec le profil
    mesuré du GOOD 295.
- [2026-10-27] **Spike STEP9 — corpus élargi (19 paires / 32 vidéos)** :
  - 1059 segments bruts, 520 clés humaines, 145 segments keyframés ;
  - 278/520 clés (53,5 %) à ≤ 60 px d'un objet suivi ; 458/907 segments
    statiques (50,5 %) à ≤ 50 px du cadrage brut, 363/907 (40 %) à > 100 px ;
  - **A3 invalidé en auto** : précision 0,20, rappel 0,23 ; 35 faux positifs
    sur les vidéos « sans piège » → candidats en `review` ou flag projet ;
  - **A1 validé** sur le letterbox noir (191 : 249,6 % vs 255 % avec marge
    4 % ; 268 : 179,1/222,2 % par segment), mais bords non noirs de 268 non
    couverts ;
  - **A2 fragile** : 207,4 % correct sur 187 DISNEYWORLD, faux positif à
    260 % sur 187 ECOLE ; logos de coin (191) et logo de template (295,
    `sdm.png`) hors A2 ; **corrigé en v2 (D-031)** ;
  - **A6 SDM validé** sur le GOOD 295 reçu le 2026-10-27 : brut et GOOD
    partagent le même `_ae.json`/`_tracking.json` (SHA-256 identiques). Le
    template (fond 180 % flouté 50, panneau 84,3497774862575 % à y=171,032,
    panneau 56,294573830734535 % à y=1082,026, logo `sdm.png`) est reproduit
    par `06-corpus/a6_template.py` à **0/255** (exact, 0 pixel > 2) à 0, 10,
    30 et 50 s. Wrapper `03-scenario-a/a6_apply.py` : déclencheur configuré
    `M6 - SDM - `, 295 → apply/audit pass, 187 → noop/audit pass, 295
    keyframé → review sans projet ; rendu relinké 0/255. Reste à l'intégrer
    au post-traitement de production ;
  - réserves : 179 sans `.ecproj` GOOD, 287 avec 1 référence sur 3, 2CV
    exclue ; **dérive de baseline identifiée** : STEP4/5 tracking legacy vs
    Rust, pas STEP6/8. Le STEP6 Rust sur le `_tracking.json` legacy reproduit
    le `_GOOD/_ae.json` à l'identique (4355/4355 sur 187, 2979/2979 sur
    181) ; recette de régénération documentée, delta d'ancre médian vs GOOD
    ramené de 62,75 à 8,4 px sur 187 et de 44,01 à 2,43 px sur 181. Le
    commit legacy `0e9765b` du 2026-10-16 19:01 (`STEP4_TPU_FAST=1`,
    `object_min_missing=25`) est postérieur aux GOOD ; `--object-min-missing
    1` remonte la couverture personne à 1722 frames vs 1623 legacy, mais ne
    ramène pas les ancres (64,33 px) : le gate n'explique que la couverture.
    Diagnostic : `/tmp/opencode/step9-spike/06-corpus/DIAGNOSTIC-DERIVE.md` ;
  - **clamp du panel** : correct pour une position centrée
    (`[C/(2s), W-C/(2s)]`) ; les ancres hors plage des GOOD sont des
    déplacements manuels de `Position` couverts par l'intervalle généralisé
    `[p/s, W-(C-p)/s]`, sauf 181 segment 34 (vide de 36,7 px) à signaler en
    `review` ;
  - décision D-030 ; rapports `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` §3.6/§3.8,
    `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` §4.6/§5.7/§8.1,
    `/tmp/opencode/step9-spike/06-corpus/RAPPORT-CORPUS.md` et
    `RAPPORT-A6.md` ;
  - prochaine étape : A5, validation A1 synthétique, A2 durci, intégration A6.
- [2026-10-26] **Spike STEP9 — Phase 0 terminée** :
  - baseline STEP8 reproductible sur 298/300 (calques identiques, seuls les
    chemins médias diffèrent) ;
  - labels produits : 118 segments bruts, 120 finaux, 0 ancre modifiée,
    23 keyframés, 72 clés, 1 fusion, 3 splits ;
  - A0/A2 validé : aucune bande sur 298 (no-op), bande de 108 px sur 300 →
    scale 207,4 % vs 207 % humain ; A1 no-op (aucun letterbox) ;
  - A3 limité : 39 % des clés humaines à ≤ 60 px d'un objet suivi ; le
    re-cadrage statique de 300 (29/36 et 35/60 segments à > 100 px du brut)
    n'est pas déterministe avec `_ae.json` seul ;
  - rapport : `/tmp/opencode/step9-spike/01-baseline/RAPPORT-PHASE0.md` ;
    aucun code de production modifié, aucune écriture dans `/mnt/cache`.
- [2026-10-26] **Étude STEP9 agentique et décision D-029** :
  - étude `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` et protocole
    `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` ;
  - analyse des projets témoins `298 Camille_GOOD.zip` et
    `300 Camille_GOOD.zip` : 117 segments comparables, 0 ancre modifiée,
    23/120 calques keyframés, 72 clés, 92 segments à +27 % d'échelle,
    1 fusion et 3 splits, 2 logos + 1 jingle sur 298 ;
  - décision : consolidation déterministe (A0–A5 : profilage, letterbox,
    overlays source, keyframes, transitions, rapport) d'abord, agent QA
    read-only (B) ensuite, co-monteur (C) en laboratoire seulement ; logos,
    jingle et rendu restent humains. Aucun code de production modifié,
    aucune écriture dans `/mnt/cache` ou `runtime/`.
- [2026-10-26] **Rollback Tauri et retour au headless (D-028)** :
  - abandon du client desktop Tauri, des crates
    `workflow-desktop-services`/`workflow-gui-tauri`, du paquet
    `workflow-manager`, de l'unité `workflow-server.service` et des scripts
    PoC ;
  - dépôt ramené à `a93a554` ; branche de sauvegarde
    `backup/tauri-abandon-20261026` sur `1613b17` ;
  - validation : 280 tests au rollback strict, 282 après correctifs ;
    fmt/clippy, `config check`, build release ;
  - production : Rust `:5004` relancé via `scripts/switch-rust.sh`, Python
    `:5003` arrêté, aucun client desktop ;
  - correctifs réappliqués : `514e2d6` (ETXTBSY), `4ddb91b` (secrets,
    281 tests) et `713b6bc` (polling web masqué, 282 tests) ; réorg docs
    conservée dans la branche de sauvegarde.
- [2026-10-24] **Synchronisation documentaire** :
  - chiffre de tests aligné sur la mesure
    (`cargo test --workspace -- --list`) : 280 ;
  - statut `stopping`/`is_stopping`, en-têtes de sécurité, `server.allow_remote`,
    `download.allow_private_hosts`, quotas STEP1 et `steps.step7.timeout_s`
    documentés dans le contrat et les docs de lot ;
  - index D-025 à D-027 complété dans `decisionLog.md` ;
  - purge des résidus au démarrage ajoutée à `RETENTION.md` ;
  - audit de `.agents/rules/codingstandards.md` et des 22 `SKILL.md` :
    compte de tests 280, clé `steps.step3.max_workers` retirée, verrou STEP1,
    arrêt `stopping`, gate `effectcraft-cli`, écriture legacy interdite dans les
    skills frontend.
- [2026-10-24] **Journal serveur enrichi** :
  - `workflow-server.log` inclut désormais la configuration au démarrage, les
    requêtes mutantes et erreurs HTTP avec durée, le cycle webhook, les étapes
    avec PID et les séquences.
  - ANSI désactivé automatiquement quand stderr est redirigé vers un fichier ;
    `RUST_LOG=debug` pour les GET et le détail webhook.
  - `start-rust.sh` n'affiche plus les 40 anciennes lignes au démarrage, ce qui
    évite de confondre les erreurs webhook du 2026-10-23 avec un incident
    courant.
- [2026-10-24] **Widget système et auto-ouverture des logs** :
  - `/api/system_monitor` expose la GPU via `nvidia-smi` (première carte,
    contrat `null`/`error` conservé) ; le batch STEP1 → STEP8 du jour a servi
    de validation d'exploitation.
  - L'auto-ouverture de l'overlay de logs est désactivée par défaut côté
    frontend Rust ; le toggle Settings et la préférence persistée restent
    effectifs.
  - Logs du batch `1531 Camille` vérifiés : 8/8 étapes `completed`, aucun
    échec d'étape ; seules les erreurs webhook du 2026-10-23 restent dans le
    journal serveur.
- [2026-10-24] **Remédiation de l'audit backend (36 constats)** : six lots
  appliqués sur les neuf crates. Orchestrateur : `RunId`, `StepPhase::Stopping`,
  fin générationnelle, canal de logs borné, rotation en cours d'exécution,
  erreurs HTTP typées, arrêt des groupes de processus. Configuration :
  `allow_remote`, verrou `/tmp/workflow-rs-hw.lock`, clés TOML mortes
  supprimées, `steps.step7.timeout_s`. Sécurité : SSRF public-only, token
  constant-time, en-têtes CSP, quotas ZIP/RAR, symlinks STEP7 refusés, logs
  confinés. Parité : `round_to` partagé STEP3, RAR streamé. Persistance :
  `fsync` avant publication, temporaires cachés, purge des orphelins.
  Documentation : D-025 (404 `/sound-design`), D-026 (frontières typées),
  D-027 (arrêt manuel CLI). Suite : 280 tests, fmt/clippy verts.
- [2026-10-24] **Bootstrap `.agents/` et `memory-bank/`** :
  - création de `.agents/rules/` (10 règles), `.agents/skills/` (21 skills +
    9 ressources) et `.agents/workflows/` (6 workflows) ;
  - renommage des skills legacy obsolètes vers la numérotation Rust
    (`step3-audio-orchestrator`, `step4-tracking-ops`) ;
  - remplacement des skills legacy-only par des skills Rust
    (`onnx-runtime-cuda-expert`, `rust-concurrency-orchestrator`,
    `after-effects-bridge`, `after-effects-json-contract`) ;
  - initialisation de `memory-bank/` avec 5 fichiers à partir de l'historique
    git (31 commits) et des docs de lot ;
  - ajout d'un pointeur dans `AGENTS.md`.
- [2026-10-24] **Intégration de `rust-patterns`** :
  - normalisation du frontmatter et du Markdown (table, commandes, structure) ;
  - ajout du routage priorité 3 dans `skills-integration.md` ;
  - mise à jour du décompte : 22 skills, 52 fichiers dans `.agents/` +
    `memory-bank/`.
- [2026-10-24] **Audit documentaire** :
  - correction des commandes de parité : le venv legacy est
    `../workflow_mediapipe/coral_env/bin/python`, pas `coral_env/` ;
  - alignement de l'état de production dans `README.md`, `SWITCH-PUBLIC.md` et
    `CONTRAT-ENVIRONNEMENT.md` : Rust `:5004`, Python `:5003` en rollback ;
  - `.agents/workflows/repomix-bundle.md` est supprimé dans l'arbre de travail
    (non commité) ; le décompte courant est 51 fichiers dans `.agents/` +
    `memory-bank/`.

## État du système

- Backend de production : Rust `:5004` headless via
  `scripts/switch-rust.sh` ; Python legacy `:5003` arrêté (rollback
  disponible).
- Interface : page web legacy `assets/web` uniquement ; aucun client desktop
  natif (D-028).
- Webhook Rust activé dans la configuration depuis le 2026-10-23
  (`enabled = true`) ; il ne tourne que si le serveur est démarré.
- 492 tests unitaires ; pas de CI.
- Audit backend du 2026-10-24 remédié : 33 corrigés, 3 documentés
  (C3 → D-025, C4 → D-015, C8 → D-024).
- Dernière validation : STEP1 → STEP8 sur `1720 Camille`, shadow `162/162`.
- Verrou matériel : `/tmp/workflow-rs-hw.lock`.
- Runtime : `runtime/` ; sortie finalisée : `/mnt/cache`.

## Blocages / points ouverts

1. **D-024** : le STEP4 Python régénéré sur `153 Camille` diffère de l'archive
   (4 977 objets contre 11 853). Ne pas utiliser `--prefer-raw` sur ce projet.
2. **Reverse proxy** : aucun nginx/caddy/cloudflared installé ; la bascule
   publique reste à faire.
3. **Surveillance production** : vérifier le moniteur webhook et les
   téléchargements après quelques jours.
4. **Parité STEP4** : la configuration d'origine de `153` reste à retrouver.
5. **Signaux** : les étapes 1 à 4 installent un gestionnaire `SIGTERM`/`SIGINT`
   qui termine leurs enfants ; STEP8 conserve son propre gestionnaire. Voir
   D-027.

## Prochaines actions

- Post-traitement A5/A6 : exposer `review.json` en lecture seule dans l'UI,
  recalibrer A3/A4, traiter les références manquantes 179/287 (voir
  `docs/lots/LOT-9-POST-STEP8.md`).
- Poursuivre les scénarios B/C décrits dans
  `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`, sans toucher à la production.
- QA logos : faire relire et signer les 12 labels dev (`review_status = done`) ;
  les 2 labels 270 sont `arbitrated` (branding source intentionnel). Valider
  DINOv2 sur des logos inconnus et des masques partiels avant d'envisager son
  activation par défaut ; le probe reste entraîné sur 13 assets synthétiques.
- QA logos : étendre la validation aux assets non textuels et aux logos
  inconnus ; l'ablation couvre 14/14 overlays non déclarés, mais les 39 cas
  de galerie ne sont pas indépendants (13 PNG alpha).
- Diagnostiquer D-024 sans modifier les données de production.
- Lire `docs/exploitation/SWITCH-PUBLIC.md` avant toute bascule publique.
- Vérifier `history stats`, `webhook probe --plan` et les logs du moniteur.
- Maintenir `memory-bank/progress.md` et `decisionLog.md` à jour.

## Rappels

- Ne pas modifier `/home/kidpixel/workflow_mediapipe`.
- Ne pas écrire dans `/mnt/cache` depuis un test.
- Ne pas lancer les moniteurs Python et Rust simultanément.
- Ne pas lancer STEP2/3/4 matérielles des deux stacks en même temps.
- Ne pas nettoyer les quirks D-001, D-002, D-004, D-016.
- Ne pas réintroduire de client desktop Tauri/GTK/WebKit/zbus (D-028).
- Le serveur headless est lancé par `scripts/switch-rust.sh` ; ne pas
  réinstaller `workflow-server.service` sans nouvelle décision.
- Lire `.agents/rules/codingstandards.md` avant toute modification de code.

## Références

- `AGENTS.md`
- `README.md`
- `docs/README.md`
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`
- `docs/lots/LOT-*.md`
- `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`
- `docs/etudes/SPIKE-STEP9-PROTOCOLE.md`
- `parity/DRIFT.md`
- `memory-bank/progress.md`
- `memory-bank/decisionLog.md`
