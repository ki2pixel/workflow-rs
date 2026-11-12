# Contexte actif

## Tâche en cours

**Lot 12 — logos de mission A7 (2026-11-12, implémenté et validé, D-064).**
A7 résout les missions `email_proxy.php` par lot, valide les assets
1080×1920 avec alpha et insère les logos dans le seul `_assist.ecproj` :
end-card `.mov` court avec PNG (comp prolongée), plein durée pour un MOV long,
`review` pour un MOV ambigu, `Pas de logo` sans calque ni suppression. Le
cache est la base dédiée `email_missions.sqlite3` ; doubles interrupteurs
`email_proxy.enabled` / `mission_overlay`, plus `a2_mission_mask` et
`mission_end_card_max_s`. Sidecar `<docs>/<stem>_mission.json`, bloc `mission`
après `summary` dans `post.json`, contrôles `mission_*`, QA `1.4.0` et cache
11. Validation sur copies `/tmp/opencode` : 136 end-card 44,4→47,4 s (19
calques), 80 MOV plein durée 0–79,2 s et `Pas de logo`, 131 déduplication A6
(0 doublon), 158 trois `Pas de logo`, 89 PNG statique 0–85,84 s, 999 asset
absent (`review`, exit 0). `--no-email` isole totalement (44,4 s, 16 calques,
aucun sidecar, aucun `a7_ms`) ; `--no-mission-overlay` audite sans insérer.
634 tests exécutés / 2 ignorés, fmt et clippy verts. Documentation :
`docs/lots/LOT-12-A7-MISSIONS.md` ; décision D-064. Activation en production :
`email_proxy.enabled = true` et `mission_overlay = true` configurés dans `config/config.toml`.
Les projets et médias de production n'ont pas été modifiés.

**Lot 11 — veto latéral A1 (2026-11-11, implémenté et validé, D-063).**
`crates/workflow-steps/src/post/a1.rs` rétrograde désormais en `review`
(`confidence = 0.4`) tout `apply` dont une barre gauche **ou** droite atteint
`lateral_min = arrondi(32 × hauteur_source / 1080)`, minimum 1 px : 32 px en
1080p, 21 px en 720p, 64 px en 4K. Le motif est explicite (« barres latérales
(gauche X px, droite Y px) : recadrage non fiable ») et `details.lateral_min`
est ajouté à côté de `details.bars`. `noop`, keyframes, letterbox
intermittent/variable et différés A3 conservent leur décision ; le chemin
natif ne contourne pas le veto. Validation : 9 tests A1 nouveaux, 598 tests
listés (596 exécutés + 2 ignorés), `fmt` et clippy verts ; rejeu réel des
6 projets / 7 segments du corpus 248 : 7/7 passent en `review`, 0 autre
changement d'action sur 305 décisions A1 ; rejeu déterministe des 382
`apply` : 19 bons applies rétrogradés sur 380 appariés (dont 12 à ±2 pts),
354 applies appariés sans barre ≥ 32 px inchangés ; 7 frames contrôlées.
Documentation : `docs/lots/LOT-11-A1-VETO-LATERAL.md` ; décision D-063.
Aucun `.ecproj` du corpus, `runtime/`, `/mnt/cache` ou `config/config.toml`
modifié.

**Étude corpus 248 — post-STEP8 et flags A2 (2026-11-11, lecture seule).**
Le corpus `Corpus_Exemple_3` (92 dossiers, 248 projets, 9 779 segments) a été
audité sans modifier les `.ecproj` ni `/mnt/cache` : 9 359 segments appariés
(95,7 %), accord strict 58,93 %, matrice zoom 660 TP / 7 FP / 3 218 FN /
5 474 TN, précision 98,95 %, rappel 17,02 %. A1 conserve 373/380 `apply`
(98,16 %) ; les 7 faux applies sont tous des sources pillarboxées,
`horizontal` calculé mais non utilisé comme veto. A2 conserve 287/287
segments (0 revert, 0 sur-zoom) ; 35/175 revues (20 %) cachent un bandeau et
13 cibles A2 sont manquées (7,4 %). A3 ajoute des clés dans 31,6 % des
revues, A4 agit dans 6 %, A5 résout 111/111 en report-only. La shadow
evaluation des 248 projets avec `a2_logo_prior=true` + `a2_logo_detector=true`
crée **1 `apply` correct** (`257 Camille — 66M VENTES`, 218,2 % contre
215,3 % humain), ne perd aucun apply, ajoute 2 fausses revues (`108`, `243`)
et reformule 15 motifs ; le détecteur ne change aucune action et coûte
2 313 ms/projet en moyenne (médiane 1 858,5 ms, max 5,34 s). Décision D-062 :
`a2_logo_prior = true` activé dans `config/config.toml` en pilote contrôlé,
`a2_logo_detector` maintenu `false` ; le veto A1 barres latérales ≥ 32 px est
implémenté et validé sous D-063 (`docs/lots/LOT-11-A1-VETO-LATERAL.md`).
Rapport :
`docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`. Aucun code de production,
`runtime/` ou `/mnt/cache` modifié.

**SDM multi-préfixes — A6 et bypass direct (2026-11-09, D-061).**
La vidéo `116 Camille/docs/SDM - Liliane et josé.mp4` n'était pas reconnue
comme template SDM : `template_prefix` valait `M6 - SDM - `, l'audit STEP8
concluait `a6_template = pass` (« aucun template demandé ») et la vidéo
traversait STEP2–STEP6 puis un recadrage A1/A2 standard. Le champ devient
`template_prefixes = ["M6 - SDM - ", "SDM - "]`, accepté par `Bypass`,
`post/a6.rs`, Stage 0 QA, `post run` et `step7 finalize`. L'ancien scalaire
reste un alias Serde ; `config check` rejette les préfixes vides et les
chevauchements. Rejeu réel sous `/tmp/test-sdm` : les deux nommages donnent
`a6_eligible = 1`, `template_applied = true`, 4 calques A6, audit `pass`,
aucune décision A1–A4. 589 tests listés, `fmt` et clippy verts.

**Lots B0/B1/B2 — détecteur local ONNX pour A2 (2026-11-09, exécutés).**
Après validation de D-060, l'approche B a été testée hors production :
YOLOX-Nano 416/640 et NanoDet-Plus-m 416 (Apache-2.0), exportés en ONNX
FP32/FP16 et exécutés nativement en Rust via `ort` CUDA. B0 : YOLOX 416 FP32
donne 5,29 ms p95 de session et 8,40 ms au total pour +107 Mo de VRAM ;
YOLOX 640 FP16 15,00 ms, NanoDet 416 FP32 12,16 ms. FP16 est stable pour
YOLOX mais pas pour NanoDet (127 vs 181 détections), qui reste en FP32. Le
zéro-shot COCO est inutilisable. B1 : fine-tuning YOLOX-Nano 416 depthwise
(896 754 paramètres) sur 1 495 PNG du catalogue, 15 800 images synthétiques
copy-paste (14 000 positives, 1 800 négatifs), test fermé et open-set avec
228 logos de chaînes internationales. Entraînement arrêté après l'époque 15
(la val synthétique montait, D-060 se dégradait), **checkpoint époque 5
figé**. Au point **0,30 / NMS 0,65** : rappel D-060 **95,5 %** sur les
références, **96,7 %** de présence lower-third A2 (91,7 % sur la boîte
`gt_logo`), **3,3 %** de frames négatives avec FP, open-set **90,3 %**,
latence totale **8,01 ms p95**, +107 Mo de VRAM, ONNX 3,6 Mo
(`sha256 5c03e8b6…cb8caa`). Les seuils de présence, open-set, FP et latence
sont atteints ; le rappel connu combiné (94,6 %) et la localisation fine du
logo A2 (91,7 %) restent sous la cible 95 %. Le pilote A2 `report_only` (B2)
est autorisé **sous réserve**, sans modification de `a2.rs`.
Résultats : `docs/etudes/ETUDE-YOLO-ONNX-A2/results/B1.md` et `b1-*.json` ;
étude : `docs/etudes/ETUDE-YOLO-ONNX-A2.md` §11 ; binaire :
`crates/workflow-steps/examples/yolo_bench.rs`.

**Lot B2 — pilote `report_only` A2 (2026-11-09, exécuté).** Le détecteur
B1 est branché en annotation seule (`a2_logo_detector = false`) dans
`vision/logo_detector.rs` et `post/logo.rs` : 12 frames à la résolution
source, filtre ROI A2, clusters IoU 0,50 / centre 10 px, persistance 0,70,
`details.logo_detector` additif. Aucune action, raison, confiance ou
`scale_target` n'est modifiée ; toute erreur laisse A2 intact. Pilote sur
les **13** projets finalisés dont le brut est byte-identique à l'assist
(STEP7 supprime les bruts, 2 projets `apply` exclus) : **0 différence
d'action** et de `review.json`, aucun audit `fail` (2 `warn`
`source_coverage` préexistants), **2/13** logos
persistants (11/12, 91,7 %), tous deux `unknown_logo` (M6 hors catalogue
D-052), deux logos animés à 7/12 sous le seuil (fragmentation 7/12 +
3/12), latence p50 **9,28–10,86 ms** / p95 **10,77–12,25 ms**, coût
**4,45 s/projet** (décodage ffmpeg + chargement). Le recadrage
`bottom_left` améliore la boîte A2 (91,7 → 98,3 %) mais dégrade les
références (95,5 → 84,0 %), donc `full` reste le point B2. Aucune
activation de production ; suite B1.2 (ROI deux passes, fusion temporelle)
ou pilote opérateur. Preuves :
`docs/etudes/ETUDE-YOLO-ONNX-A2/results/B2.md` et `b2-pilot.json`.

**Lot B2.1 — consolidation temporelle et cache (2026-11-09, exécuté).**
Sans changer le modèle ni la décision, `post/logo.rs` tente l'association
stricte (IoU 0,50 / centre 10 px) puis une association animée (IoU 0,35 /
centre 25 px / ratio de taille 0,5–2,0), exposée dans
`details.logo_detector.clustering`. `vision/frame.rs` ajoute un cache
mono-projet des 12 frames, partagé avec le gate OCR A2 quand
`a2_logo_detector` est actif. Pilote `report_only` sur les mêmes 13 projets :
**0 différence d'action** et de `review.json`, aucun `apply`, **5/13** logos
persistants contre 2/13 en B2 (dont `Il déteste` 12/12, `Manifestations`
9/12 + 3/12, `LMCR3` 9/12 et 11/12), tous `unknown_logo`, latence p95
**10,92–14,28 ms**, coût **4 535 ms/projet**. Le cache n'a pas été exercé :
aucun projet ne déclenche l'OCR A2, donc le second passage à 0,0 ms n'est
prouvé qu'en laboratoire. Le sous-cluster intercalé de `Manifestations`
reste séparé (première boîte à ~30 px du cluster dominant) ; une fusion de
clusters en post-traitement est la piste identifiée. Preuves :
`docs/etudes/ETUDE-YOLO-ONNX-A2/results/B2.1.md` et `b2.1-pilot.json`.

**Grounding DINO — spike A2 et logos source (2026-11-09, D-060).** Le
détecteur ouvert `IDEA-Research/grounding-dino-tiny` (Swin-T, 659 Mio) a été
évalué hors production sur 242 frames : 122 logos du catalogue, 60 bandeaux
A2 (54/58 UJUD, 40 66 Minutes) et 60 frames négatives (5 clean feeds). Le
prompt composite `logo . text . watermark . lower third . banner` donne
96,7 % de logos à IoU ≥ 0,50 au seuil 0,20, 98,3 % de bandeaux et 97,2 % de
texte IoU ≥ 0,30 ; mais 25 % de faux positifs **par frame** sur les
négatifs. Le clustering temporel (IoU ≥ 0,50, centre ≤ 10 px, persistance
≥ 70 %) ne laisse **0/5 clip négatif** persistant (maximum 16,7 %, IC 95 %
[0 ; 43,4 %]) et conserve 5/5 logos et 3/5 textes sur les clips positifs.
Coût local : 1,279 s/frame à 480 px, 1,47 Gio de VRAM, 15,3 s pour 12
frames, soit +35 % sur le post complet. Décision : **laboratoire
seulement**, aucun flag de production ; Replicate écarté (données client,
réseau, secret ; non exécuté faute de jeton) ; ONNX non prêt (pas de config
Optimum, ops custom). ADR :
`docs/decisions/DECISION-GROUNDING-DINO-A2.md` ; étude :
`docs/etudes/ETUDE-GROUNDING-DINO-A2.md` ; preuves :
`docs/etudes/ETUDE-GROUNDING-DINO-A2/results/`.

**A1 natif gardé — D-058 option A (2026-11-09).** Le flag opt-in
`a1_refine_native = false` active un raffinement du seul `apply` A1 déjà
décidé par `cropdetect` : natif v2 Sobel 20/1020, couverture 2/3 (0,45 si
les quatre lignes côté barre sont sous 10/255), clustering dominant ≥ 60 %,
marge D-056 1,015. Un natif `review`/`noop`/instable ou qui échoue à un veto
conserve `cropdetect` ; le natif ne crée ni ne retire jamais un `apply`.
Détails additives `native_refined`, `native`, `cropdetect`. Validation
unitaire et réelle sur le 305 (16/17 applies à 211,8 % au lieu de 213,7 %),
puis passe complète sur le corpus GOOD : 2 607 segments A1, 0 changement
d'action, 52/65 applies raffinés, gain pixel moyen 1,46 px, surcoût 29,27 s
(2,66 % du temps A1). Reste le contrôle visuel des 4 dégradations 89 et des
4 applies 89 bloqués par la couverture, puis l'activation pilote. L'option C
est reportée à D-059. ADR :
`docs/decisions/DECISION-A1-NATIVE-GARDE-D058.md` ; preuve :
`docs/etudes/ETUDE-VSLETTERBOX-A1/results/full-corpus-v2.md`.

**Registre spatial des logos source — phases A/B/C (2026-11-08, D-052).** Le
catalogue versionné `assets/models/qa/source_logos.toml` contient les 30
émissions documentées par le client (position, bbox d'enveloppe, variantes,
sources exactes), 21 clean feeds et 9 émissions sans logo ; la 30ᵉ est
*Les Reines de la Route* (6ter, `bottom_left`, bbox `[99, 848, 315, 1022]`).
Les crops RGBA restent externes. `vision/source_logo.rs` résout la source
et projette la bbox par les transformations du `.ecproj` (keyframes compris).
A2 ajoute
`details.source_logo` derrière `a2_logo_prior = false` et, quand un prior
est actif, retire du groupe OCR dominant les détections majoritairement
dans la bbox du logo (`details.ocr.logo_texts`, `logo_detections_total`,
`logo_only`) ; un logo seul part en `review`. Sans prior, ces clés restent
absentes. Quand un overlay déclaré couvre, aux douze instants du gate OCR, toutes
les occurrences visibles du logo (≥ 90 %), `visible_in_comp` passe à `false`,
`overlay_covered`/`hidden_by_overlay` sont ajoutés et le veto logo seul est
remplacé par la décision géométrique ; rejeu `54` avec un habillage
synthétique : `apply` 200,8 % inchangé, 2/2 occurrences masquées. Le QA
ajoute `source_logo` (`pass`/`info`) derrière
`--source-logo`. `--source-logo-visual` (ou `--source-logo-references`)
confirme localement les pixels par NCC couleur + contours, ou la stabilité
d'une zone générique ; aucun `review`, aucun recadrage. Le catalogue est
généré par `scripts/import-source-logos.py` depuis le CSV client. Rejeu
A2 : `54`/`58` inchangés (`apply` 200,8 % / 210,0 %) et
`UNJOUR`/`UNDOC` déplacés dans `logo_texts` ; `40` reste `review` mais
devient « bande sombre, texte persistant confirmé » avec la bannière réelle
à 91,7 % et un scale proposé de 218,2 % ; `sub_fast` reste vétoé à 25 %.
Validation visuelle : références 66 Minutes → NCC 0,97 (3/3) ; export 9:16
synthétique UJUD avec transformation réelle → NCC 0,96 (3/3) ; quatre
rendus H.264 depuis les rushes et transformations réelles (`54`, `58`,
`40 ABHARTACH`, `40 FOOTBALLEUR`) → 0,95 / 0,94 / 0,64 / 0,58, 3/3 ;
rendu UJUD évalué avec le prior 66 Minutes → 0,07 ; zone générique
statique → 100 % stable (contours 19,6 %). QA sur 35 exports GOOD avec
`.ecproj` : 25 `pass`, 10 `info`, 0 nouveau finding ; un balayage
exhaustif de six des dix exports `info` (UJUD ×4, 66M ×2) à 1 i/s ne
dépasse jamais 0,50 (meilleur 0,38 sur une bande de 24 px) ; les quatre
autres n'ont pas été balayés. Ces `info` ne signifient pas une absence du
logo dans le rush (vérification manuelle UJUD/66M : présent sur toute la
durée), mais l'absence du logo source à la position projetée dans l'export
final, où le logo social est ajouté manuellement en overlay. Activation
production toujours soumise à validation client ; phase D optionnelle
(DINOv2 résiduel, confirmation A2) non implémentée.

**Threads OCR A2, staging et compatibilité EffectCraft (2026-11-08, D-053/D-054).**
`steps.step8.post.a2_ocr_threads` borne chaque session PP-OCRv6 det/rec
(défaut 2, `config/config.toml` = 8). Sur `54` : OCR 23,6 s à 2 threads,
13,2 s à 8, 12,5 s à 12 ; A1 reste le goulot à ~23,5 s. Les PP-OCRv6 sont
installés sous `assets/models/qa/` (SHA-256 du manifeste) et le runtime
pointe vers `vendor/libonnxruntime.so.1.23.2`. Staging sur copies de `40`,
`54` et `58` : `54` reste `apply` 200,8 % (confiance 0,90 → 0,95), `58`
passe `review` → `apply` 210,0 %, les deux `40` restent `review` avec les
preuves OCR mais sans scale proposé (`coverage_x` 0,09–0,11 < 0,25) ; audits
`pass`, lot 87,3 s → 130,7 s. `a2_logo_prior = true` sur `40` confirme la
phase B : review bande sombre, `scale_target` 218,2, dominance 0,9167,
`logo_texts` `66`/`MINUTES`. Validation humaine du 2026-11-08 : `a2_ocr`
est activé seul en production (D-055), `a2_logo_prior` reste `false` ; sur
`54`/`58` le prior ne change que `texts`/`logo_texts`, mais sur `40` il
modifie le motif, la confiance et le `scale_target`, donc il attend une
période d'observation. Vérification avec la configuration de production
sur une copie de `54` : `apply` 200,8 %, OCR 13,0 s, post 43,9 s, audit
`pass`. EffectCraft 0.22.0 est compatible :
STEP8 réel sur copie temporaire de `54` (1/1 projet, `MS_BATCH_RESULT ok`),
post STEP8 (`_assist`, audit `pass`) et ouverture du projet assisté par
`effectcraft-cli --json info`.

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

- [2026-11-11] **Lot 11 — veto latéral A1 (D-063)** :
  - `decide_letterbox` rétrograde en `review` (0,4) si
    `left >= lateral_min || right >= lateral_min`, avec
    `lateral_min = arrondi(32 × hauteur_source / 1080)` (32 px en 1080p,
    21 px en 720p, 64 px en 4K) ; `details.lateral_min` ajouté ;
  - 9 tests A1 nouveaux ; 598 tests listés (596 + 2 ignorés), `fmt` et
    `clippy -D warnings` verts ;
  - rejeu réel 6 projets / 7 segments : 7/7 `apply` → `review`, 0 autre
    changement sur 305 décisions A1 ;
  - rejeu des 382 applies : 19 bons rétrogradés (12 à ±2 pts), 354 applies
    appariés sans barre ≥ 32 px inchangés ; 7 frames pillarbox contrôlées ;
  - ticket `docs/lots/LOT-11-A1-VETO-LATERAL.md` clos, D-063 dans le
    journal des décisions ;

- [2026-11-11] **Étude corpus 248 post-STEP8 (lecture seule)** :
  - 248 projets / 9 359 segments appariés ; 660 TP / 7 FP / 3 218 FN /
    5 474 TN, précision 98,95 %, rappel 17,02 % ;
  - A1 373/380 `apply` conservés, 7 faux applies pillarbox (veto latéral
    `horizontal` non utilisé) ; A2 287/287 segments conservés, 0 revert ;
    35/175 revues avec bandeau, 13 cibles A2 manquées ;
  - shadow 248 `a2_logo_prior + a2_logo_detector` : 1 `apply` créé
    (`257 Camille`, 218,2 % contre 215,3 %), 0 perdu, 3 changements
    d'action (dont 2 fausses revues `108`/`243`), 15 reformulations de
    motif, 41/199 actions de revue confirmées toutes étroites, 0 action de bandeau ;
  - coût détecteur : 2 313 ms/projet en moyenne, 1 858,5 ms de médiane,
    5,34 s max, 573,6 s sur le run ;
  - D-062 : `a2_logo_prior=true` activé dans `config/config.toml` (pilote),
    `a2_logo_detector=false` ; ticket A1 `LOT-11-A1-VETO-LATERAL.md` ;
  - aucun code de production, `runtime/` ou `/mnt/cache` modifié ; rapport
    `docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`.

- [2026-11-10] **Synchronisation documentaire post-D-061/B2.1** :
  - AGENTS.md aligné sur la mesure réelle : 589 tests listés (587 exécutés +
    2 ignorés) ;
  - contrat en version 1.6 : D-061 (`template_prefixes`, alias scalaire,
    validation des chevauchements, `template.prefix`, cache QA 10) et
    décomptes réels (189 clés TOML, 152 correspondances `env.map.toml`) ;
  - `docs/README.md` : description de l'étude YOLO passée à B0–B2.1 et
    indexation de `etudes/Architecture_QA_Vidéo_Locale.md` ;
  - README : consolidation temporelle et cache B2.1 ;
  - aucune modification de code, de `runtime/` ou de `/mnt/cache`.

- [2026-11-09] **SDM multi-préfixes (D-061)** :
  - `PostConfig.template_prefixes` remplace `template_prefix`, avec
    `["M6 - SDM - ", "SDM - "]` par défaut et alias scalaire Serde ;
  - `template::Bypass`, `a6::validate`, `post/direct.rs`, `post/run.rs`,
    Stage 0 QA et `post run --template-prefix` (répétable) acceptent la
    liste ; `template.prefix` enregistre le préfixe matché ;
  - validation des chevauchements entre préfixes de template et avec
    `archive_only_prefixes` ; cache QA version 10 ;
  - rejeu réel `116 Camille` sous `/tmp/test-sdm` : `SDM - ` et
    `M6 - SDM - ` donnent 4 calques A6, `template_applied = true`, audit
    `pass`, aucune décision A1–A4 ; `config check` accepte l'ancien
    scalaire ;
  - 589 tests listés (587 exécutés + 2 ignorés), `fmt` et
    `clippy -D warnings` verts ; docs LOT-9, LOT-10, README, AGENTS et
    memory-bank alignés.

- [2026-11-09] **Lot B2.1 YOLOX-Nano A2 — fusion temporelle et cache** :
  - `post/logo.rs` : association stricte puis animée (IoU 0,35 / centre
    25 px / taille 0,5–2,0), seuils exposés dans
    `details.logo_detector.clustering` ; `vision/frame.rs` : cache
    mono-projet des 12 frames partagé avec l'OCR A2 ;
  - pilote `report_only` sur 13 projets : 13/13 annotés, 0 différence
    d'action et de `review.json`, 0 `apply`, 5/13 logos persistants contre
    2/13 (tous `unknown_logo`), p95 10,92–14,28 ms, coût 4 535 ms/projet ;
  - `Manifestations` passe de 7/12 à 9/12 + 3/12 (0,75, détecté) ; le
    sous-cluster intercalé reste séparé, la première boîte étant à ~30 px
    du cluster dominant ; test synthétique 10/12 ;
  - cache non sollicité par le pilote (aucun candidat OCR A2) : 0,0 ms
    prouvé en labo, coût réel inchangé ; piste : fusion de clusters en
    post-traitement ;
  - preuves : `docs/etudes/ETUDE-YOLO-ONNX-A2/results/B2.1.md` et
    `b2.1-pilot.json`.

- [2026-11-09] **Lots B0/B1 YOLOX-Nano A2** :
  - B0 : exports ONNX YOLOX-Nano 416/640 et NanoDet-Plus-m 416, FP32/FP16,
    parité PyTorch→ONNX, bench Rust `ort` CUDA ; YOLOX 416 FP32 8,40 ms
    p95 total, +107 Mo VRAM ; NanoDet FP16 rejeté ;
  - B1 : 1 495 logos, 15 800 images copy-paste, 1 800 négatifs, 228 logos
    externes pour l'open-set ; marques CSA/« gulli » conservées comme
    négatifs durs (les masquer donnait 23,3 % de FP négatifs) ;
  - entraînement YOLOX-Nano 416 depthwise 1 classe, arrêté après l'époque 15
    (val synthétique 0,948 mais D-060 en baisse) ; checkpoint époque 5 figé,
    ONNX 3,6 Mo, `sha256 5c03e8b6…cb8caa` ;
  - point 0,30 / NMS 0,65 : D-060 rappel réf. 95,5 %, présence lower-third A2
    96,7 % (boîte logo 91,7 %), FP négatifs 3,3 %, open-set 90,3 %, test
    fermé mAP50 0,918, open-set mAP50 0,898 ; latence 8,01 ms p95,
    +107 Mo VRAM ;
  - seuils de présence, open-set, FP et latence atteints ; rappel connu
    combiné 94,6 % et localisation A2 91,7 %, sous la cible 95 %. Pilote B2
    `report_only` autorisé sous réserve ; `a2.rs` intact, aucune
    modification de production ; preuves :
    `docs/etudes/ETUDE-YOLO-ONNX-A2.md` §11 et `results/B1.md`.
  - limites : sélection du checkpoint sur D-060, fonds M6 partagés en
    open-set, FP synthétiques 12–13 % à 0,30, AP small 0,22–0,26.

- [2026-11-09] **Spike Grounding DINO A2 (D-060)** :
  - `scripts/spike-grounding-dino.py` : manifeste de 242 frames (122 logos
    du catalogue, 60 bandeaux A2, 60 négatifs), inférence locale
    Transformers/CUDA, prompts, ROI, clustering temporel, métriques et
    prototype de sidecar JSON Lines (`serve`) ; aucune écriture dans
    `/mnt/cache`, `runtime/` ou le dépôt Python ;
  - résultats au seuil 0,20 : 96,7 % de logos à IoU ≥ 0,50 (118/122),
    98,3 % de bandeaux (59/60), 97,2 % de texte IoU ≥ 0,30 (35/36), 25 %
    de faux positifs frame sur 60 négatifs ; après persistance ≥ 70 %,
    0/5 clip négatif persistant (maximum 16,7 %) et 5/5 logos positifs
    persistants ;
  - coût : 1,279 s/frame à 480 px (1,47 Gio de VRAM, 3,48 s de chargement),
    2,024 s à 640, 3,053 s à 800 ; 15,3 s pour 12 frames à chaud, 7,7 s
    pour 6 ;
  - limites : confusion logo/bandeau (boîtes « logo + lower third »),
    corpus curé, 5 clips négatifs, 2 styles de bandeau, Replicate non
    exécuté (pas de jeton), ONNX non exporté ;
  - décision D-060 : laboratoire seulement, aucun flag de production ;
    backend local PyTorch 480 px pour le labo, Replicate écarté en
    production, ONNX/TensorRT ou Rust natif comme condition de réouverture ;
  - preuves : `docs/etudes/ETUDE-GROUNDING-DINO-A2.md`,
    `docs/etudes/ETUDE-GROUNDING-DINO-A2/results/`,
    `docs/decisions/DECISION-GROUNDING-DINO-A2.md`.

- [2026-11-08] **Étude `vapoursynth-letterbox` pour A1 et D-057** :
  - dépôt `Akatmks/vapoursynth-letterbox` (commit `ac274fa`) analysé en
    lecture seule : EWMA 5σ (`alpha = 0,05`, plancher 6,375 niveaux à
    8 bits), masque ExKirsch 5×5, `dynamic_ref_thr = 2/3`, snap
    `permanent` à ±1 px, `clean_letterbox` hors périmètre ;
  - prototype Python dans `docs/etudes/ETUDE-VSLETTERBOX-A1/` : 305 seg 1
    mesuré à 81/78 px (actif 921, scale 208,5 % ; 211,6 % avec 1,015)
    contre 86/82 px (actif 912, 210,5 % ; 213,7 % avec 1,015) pour
    `cropdetect=24:16:0`, choix humain 210,0 % ;
  - 304 seg 9 : faux haut 642 px rejeté sur les frames à ciel lisse, mais
    2/5 frames à 2 fps produisent un faux `top = 935` (bandeau bas) ; un
    p10 naïf donnerait 68 px, d'où le clustering temporel ;
  - 304 seg 34 : barre sombre non détectée (`top = 0`) alors qu'A1 applique
    211,5 % ; A3 reste le filet ; 356 seg 2 : `noop` natif correct ;
  - tests synthétiques : texte fragmenté rejeté jusqu'à 80 % de largeur,
    aplat ≥ 70 % et nuage large acceptés à tort ;
  - décision D-057 : pas d'intégration VapourSynth, spike natif Rust
    opt-in (`a1_detector = "native"`), `cropdetect` par défaut, contrat A1
    inchangé ; rapport `docs/etudes/ETUDE-VSLETTERBOX-A1.md`, ADR
    `docs/decisions/DECISION-A1-DETECTEUR-LETTERBOX.md`.

- [2026-11-08] **Spike Rust A1 (D-057)** :
  - exemple hors production
    `crates/workflow-steps/examples/a1_letterbox_spike.rs` : EWMA 5σ,
    Sobel Y 3×3 ciblé, couverture de colonnes ≥ 2/3, clustering ±2 px
    avec cluster dominant ≥ 60 %, sinon `review` ;
  - six fixtures (305 seg 1/9/17, 304 seg 9/34, 356 seg 2), variantes
    `gray` et `chroma` : mêmes décisions et mêmes barres ;
  - 305 seg 1/9 : 81/79, à 1 px du prototype ; scale 211,8 % avec la
    marge 1,015 (humain 210,0 %) ; 304 seg 9 : `noop`, les candidats
    935/936 et 1002/1003 sont rejetés par le Sobel (couverture
    0,38–0,46) alors que l'ExKirsch les acceptait à 0,76–0,83 ;
  - 305 seg 17 : `review` (cluster 2/4) ; 356 seg 2 : `noop` ; 304 seg 34 :
    `noop`, faux négatif connu sur bord faible, A3 reste le filet ;
  - coût : analyse Rust 8–17 ms (`gray`) / 18–34 ms (`chroma`) ; ratio
    médian vs `cropdetect=24:16:0` 1,12× / 1,24× ; `gray` est le candidat ;
  - 4 tests unitaires dans l'exemple, `cargo test --workspace` OK (545
    tests), aucun code de production modifié ; sortie brute
    `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-rust.md`.

- [2026-11-09] **Calibration A1 natif v2 (D-058, arbitrage)** :
  - diagnostic des 30 échecs du 89 : 132/226 côtés bloqués par la couverture
    (58,4 %), 22/226 par l'EWMA (9,7 %) ; couverture médiane 0,385 haut,
    0,418 bas ;
  - Sobel 20 + couverture 0,45 pour les barres < 20/255 : 89 **33/34**
    applies (contre 4/34), FN clairs **5/56** (contre 32/56), 0 `apply` sur
    les 20 `noop` + 92 vetos, 4/6 revues couverture résolues ; Sobel seul
    17/34, couverture seule 9/34 ;
  - 89 : 42/48 segments en `apply`, scale natif 241,5–242,4 % contre
    243,0 % humain ; les 9 applies sur A1 `review` sont des 89 recadrés
    humain (5 cropdetect faux, 4 à 140/140), pas des faux positifs ;
  - 304 seg 9 : `noop` 5/5, candidats 935/936 et 1002/1003 rejetés ;
    `dark_luma 10` équivalent sur les applies et plus sûr ;
  - limites : 5 FN clairs (305 seg 17, 325 seg 15, 335 seg 55, 356 seg 8,
    89 seg 22), 304 seg 34/36, 6 segments 89 non appliqués, 0/7 asymétriques ;
  - arbitrage A gardé / B natif v2 seul / C v2 + fallback cropdetect ;
    ADR `docs/decisions/DECISION-A1-NATIVE-GARDE-D058.md` ; preuves
    `results/spike-good-sample-v2.{md,csv,json}` ; aucun code de production
    modifié.

- [2026-11-09] **Évaluation A1 natif sur 201 segments GOOD (D-058, proposition)** :
  - échantillon 65 `apply` + 116 `review` + 20 `noop` (20 vidéos), `gray`,
    une passe ; matrice A1×natif : apply 25/26/14, review 0/12/104,
    noop 0/1/19 ;
  - natif seul : **0 faux apply** (0/123), **32/56 faux négatifs** (57,1 %),
    **0/13 faux vetos résolus** (0/7 asymétriques), 25 applies à ≤ 2 px
    (24/25 à ≤ 1 px) ; 305 15/17, 68 6/7, **89 4/34** ;
  - cause principale : barres basses sombres/progressives du 89, couverture
    Sobel 0,45–0,59 ou EWMA sans déclenchement ; 304 seg 34 reste `noop` ;
  - proposition D-058 : mode **natif gardé** (cropdetect d'abord, natif
    seulement pour raffiner un apply avec cluster ≥ 60 %, sinon fallback) ;
    projection pixel 1,98/1,54 px vs 2,95/2,06 px, 53/63 à ≤ 2 px, coût
    +0,45 s/apply ; ADR
    `docs/decisions/DECISION-A1-NATIVE-GARDE-D058.md` ; preuves
    `results/spike-good-sample.{md,csv}` ; aucun code de production modifié.

- [2026-11-08] **Catalogue D-052 étendu (30/21/9)** :
  - 30ᵉ émission *Les Reines de la Route* (6ter, `bottom_left`, bbox
    `[99, 848, 315, 1022]`, crop `Les_Reines_de_la_Route/crop.png`,
    source exacte et alias `6ter - LRDLR -` / `LRDLR`) ;
  - 6 clean feeds (Le Seigneur des Anneaux, Younger, WANTED ×2, Pretty
    Little Liars ×2) et 4 émissions sans logo (mêmes programmes) ;
  - `import-source-logos.py` normalise le suffixe CSV `(LRDLR)`, la note et
    les dossiers absents ; régénération complète, 3 notes existantes
    rafraîchies (Top Chef, Enquête Exclusive, E=M6) ;
  - tests 30/21/9 + résolution LRDLR + clean feed WANTED ; 545 tests,
    `fmt` et `clippy` verts ; générateur reproductible.

- [2026-11-08] **Étude corpus GOOD et D-056** :
  - 75 triplets `_9x16` / `_assist` / `_assist_GOOD`, 37 dossiers,
    2 609 segments, 676 revues, 71 exports ; comparaison de la géométrie
    projetée (chaîne de parents) et matching par source + intervalle, pas
    par ID ;
  - A1 : marge humaine médiane 1,0125, 51/65 ≤ 1,015, sur-zoom 1,04 de
    2,7 % ; décision de passer la marge à 1,015 ;
  - A2 : 5/5 apply conservés ; 9 bandes basses 37–45 % laissées en review,
    bbox hors comp sur 324/326 segments ; seuil 0,50 → 0,35 avec OCR avant
    le gate ;
  - A3 : 69 TP keyframes sur 456 revues (15,1 %), 248 FN à gap ≥ 0,3 ;
    reste review-only, tri par amplitude/clés, `gap < 0,7` en revue ;
  - A4/A5/A6 inchangés (review/report-only, template SDM conservé) ;
  - preuves et scripts dans `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8/`,
    rapport `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md`.

- [2026-11-08] **Mise en œuvre D-056 dans le post-traitement** :
  - A1 : marge par défaut et `config/config.toml` à **1,015** ; `A1Options`
    porte la marge et les cibles différées R2 ;
  - A2 : OCR avant le gate de largeur, bande basse 35–50 % applicable si
    `Confirmed` + géométrie strictement stable ; formule de scale inchangée
    (`scale_basis = "band_height"`) ;
  - A3 : `noop` sous 250 px / 2 clés, `high_confidence` à ≥ 700 px ou 6 clés,
    `strong_candidate` pour R2, `gap_relaxed` opt-in ; toujours review-only ;
  - A4 : `flash_probable` sur les segments < 0,3 s sans continuité de source
    des deux côtés ; A5 : `resolution = "resolved"` et `report_only = true` ;
  - R2 (`a1_defer_on_a3`) et R6 (`a3_relaxed_gap`) restent opt-in, défaut
    `false`, validés sur copies le 2026-11-08 ;
  - 120 tests post, 543 tests workspace, `fmt` et `clippy -D warnings` verts.

- [2026-11-08] **Validation R2/R6 sur copies** : 68 (R2), 54 et 356 (R6),
  configs identiques hors flags. 68 passe de 7 à 6 `apply` et conserve le
  transform brut du Segment 7 ; 54 passe de 15 à 24 `review` (9
  `gap_relaxed`) ; 356 passe de 9 à 12 `review` (3 `gap_relaxed`). Aucun
  `apply` A3, assist identique pour 54/356, audits `pass`. La référence GOOD
  de 68 porte 18 keyframes Position sur le Segment 7 (R2 confirmé) ; sur les
  12 nouvelles revues R6, 5 sont keyframées GOOD et 1 a un repositionnement
  statique (6/12 utiles), 6 sans changement. Les flags restent opt-in.

- [2026-11-07] **Gate OCR A2 (D-051, phase 3)** :
  - `PostConfig`/`PostOptions` : `a2_ocr` (défaut `false`), `a2_ocr_det`,
    `a2_ocr_rec`, `a2_ocr_onnxruntime` ; `A2Options` regroupe la marge et
    l'OCR, `decide_a2` garde 7 arguments ;
  - 12 frames, crop 16 px sur quatre côtés, PP-OCRv6 avec
    `a2_ocr_threads` (défaut 2, production 8), fusion des
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
    `post/overlay.rs` ; benchmark des sept dépôts fournis (YOLOv7, YOLOv8 API,
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
  - `post/overlay.rs` : calques image du `.ecproj`, bbox alpha via `png`,
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
- 598 tests unitaires listés (596 exécutés + 2 ignorés) ; pas de CI.
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
- D-056 P1 est implémenté (marge A1 1,015, A2 0,35 avec OCR avant le gate,
  tri A3, flash A4, A5 résolu). R2 (`a1_defer_on_a3`) et R6
  (`a3_relaxed_gap`) sont validés sur copies (68/54/356) ; décider de leur
  activation ou les laisser opt-in.
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
