# Suivi de progression

## Statut actuel

- **Lot 12 — logos de mission A7 (2026-11-12, D-064)** : résolution des
  missions `email_proxy.php` par lot dans un cache SQLite dédié, validation
  stricte des assets 1080×1920 alpha et insertion des logos dans le seul
  `_assist.ecproj`. End-card MOV court + PNG (comp prolongée), MOV long en
  plein durée, MOV ambigu en `review`, `Pas de logo` sans calque ni
  suppression, déduplication A6. Sidecar `<docs>/<stem>_mission.json`, bloc
  `mission` après `summary`, contrôles `mission_*`, QA 1.4.0 et cache 11.
  Validation sur copies `/tmp/opencode` : 136, 80, 131, 158, 89 et un cas
  d'asset manquant (`teva.png`, review + exit 0) ; `--no-email` et
  `--no-mission-overlay` vérifiés. 634 tests exécutés / 2 ignorés, fmt et
  clippy verts. Ticket `docs/lots/LOT-12-A7-MISSIONS.md` clos. Les projets et
  médias de production n'ont pas été modifiés.
- **Lot 11 — veto latéral A1 (2026-11-11, D-063)** : `a1.rs` rétrograde en
  `review` (confiance 0,4) tout `apply` avec une barre gauche ou droite
  ≥ `arrondi(32 × hauteur_source / 1080)` (32 px en 1080p, 21 px en 720p,
  64 px en 4K), motif explicite et `details.lateral_min`. 9 tests A1
  nouveaux, 598 tests listés (596 exécutés + 2 ignorés), fmt/clippy verts.
  Rejeu réel 6 projets / 7 segments : 7/7 en `review`, 0 autre changement
  sur 305 décisions A1. Rejeu des 382 applies : 19 bons rétrogradés sur
  380 appariés (12 à ±2 pts), 354 applies sans barre ≥ 32 px inchangés.
  Ticket `docs/lots/LOT-11-A1-VETO-LATERAL.md` clos.
- **Étude corpus 248 — post-STEP8 et flags A2 (2026-11-11)** : audit en
  lecture seule de 248 projets / 9 359 segments appariés. Précision de zoom
  98,95 %, rappel 17,02 % (le monteur zoome aussi pour des raisons
  sémantiques) ; A1 373/380 `apply` conservés, 7 faux applies pillarbox ;
  A2 287/287 conservés, 35/175 revues avec bandeau, 13 cibles A2 manquées.
  Shadow 248 des flags `a2_logo_prior` + `a2_logo_detector` : 1 `apply`
  correct créé (`257 Camille`), 0 perdu, 0 faux apply, 2 fausses revues,
  41/199 actions de revue confirmées toutes étroites, 0 action de bandeau confirmée,
  détecteur à 2 313 ms/projet en moyenne. Décision D-062 :
  `a2_logo_prior=true` activé dans `config/config.toml` (pilote),
  `a2_logo_detector=false` ; veto A1 barres latérales ≥ 32 px implémenté et
  validé sous D-063 (`docs/lots/LOT-11-A1-VETO-LATERAL.md`). Rapport :
  `docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`. Aucun code de production,
  `runtime/` ou `/mnt/cache` modifié.
- **Synchronisation documentaire (2026-11-10)** : alignement post-D-061 et
  B2.1. AGENTS.md passe à 589 tests listés, le contrat en version 1.6
  documente `template_prefixes` (D-061) et les décomptes réels (189 clés,
  152 correspondances), `docs/README.md` met à jour l'étude YOLO et indexe
  `Architecture_QA_Vidéo_Locale.md`, et le README mentionne B2.1. Aucun
  code ni artefact de production modifié.
- Migration Rust terminée pour les 8 étapes.
- Backend de production : Rust `:5004` headless via
  `scripts/switch-rust.sh` ; Python legacy `:5003` arrêté (rollback
  disponible).
- Interface : page web legacy `assets/web` uniquement ; client desktop Tauri
  abandonné (D-028).
- Rollback au commit `a93a554` validé : 280 tests au checkpoint, 282 après
  correctifs, fmt/clippy, `config check` et build release.
- Moniteur webhook Rust activé dans la configuration depuis le 2026-10-23
  (`webhook.enabled = true`).
- **SDM multi-préfixes (2026-11-09, D-061)** : `template_prefixes`
  (`M6 - SDM - `, `SDM - `) remplace le scalaire, avec alias
  `template_prefix` ; A6, bypass direct, Stage 0 QA et `post run` acceptent
  les deux nommages. Rejeu réel `116 Camille` sous `/tmp/test-sdm` :
  `template_applied`, 4 calques A6, audit `pass`, aucune décision A1–A4
  (auparavant `template: null` et pipeline lourd). 589 tests listés,
  fmt/clippy verts.
- **Lots B0/B1 YOLOX/NanoDet (2026-11-09)** : exports ONNX Apache-2.0
  (YOLOX-Nano 416/640, NanoDet-Plus-m 416) et microbenchmark Rust `ort`
  CUDA. YOLOX 416 FP32 : 5,29 ms p95 de session, 8,40 ms au total, +107 Mo
  de VRAM ; YOLOX 640 FP16 : 15,00 ms au total ; NanoDet 416 FP32 :
  12,16 ms. FP16 stable pour YOLOX, rejeté pour NanoDet (118/181
  détections appariées). Zéro-shot COCO inutilisable. B1 : fine-tuning
  YOLOX-Nano 416 depthwise sur 1 495 logos et 15 800 images copy-paste,
  checkpoint époque 5 figé (arrêt après l'époque 15). Point 0,30 / NMS 0,65 :
  rappel D-060 95,5 % références, 96,7 % de présence lower-third A2
  (91,7 % sur la boîte logo), 3,3 % de FP négatifs, open-set 90,3 %,
  latence 8,01 ms p95, +107 Mo VRAM. Seuils de présence, open-set, FP et
  latence atteints ; rappel connu combiné 94,6 % et localisation A2 91,7 %,
  sous la cible 95 %. Pilote B2 `report_only` autorisé sous réserve,
  `a2.rs` intact. Aucun changement de production.
  Preuves : `docs/etudes/ETUDE-YOLO-ONNX-A2/results/` et `results/B1.md`.
- **Lot B2 — annotation A2 `report_only` (2026-11-09)** : détecteur B1
  branché en annotation seule (`a2_logo_detector = false`) dans
  `vision/logo_detector.rs` et `post/logo.rs`. 12 frames, ROI A2, clusters
  IoU 0,50 / centre 10 px, persistance 0,70, `details.logo_detector` additif ;
  aucune action A2 modifiée. Pilote sur 13 projets finalisés (bruts
  supprimés par STEP7, 2 applies exclus) : 0 différence d'action et de
  `review.json`, aucun audit `fail` (2 `warn` `source_coverage`
  préexistants), 2/13 logos persistants (11/12) tous deux inconnus du
  catalogue D-052, 2 clusters animés à 7/12 sous le seuil, latence p50
  9,28–10,86 ms / p95 10,77–12,25 ms, coût 4,45 s/projet. Aucune
  activation de production ; suite B1.2 (ROI deux passes/fusion
  temporelle) ou pilote opérateur. Preuves : `results/B2.md`,
  `results/b2-pilot.json` et `scripts/pilot-b2-logo-detector.py`.
- **598 tests unitaires listés** (596 exécutés + 2 ignorés), sans matériel.
- **Étude corpus GOOD (D-056)** : 75 triplets, 2 609 segments, 676 revues,
  71 exports comparés en lecture seule. A1 : marge 1,04 → **1,015** (marge
  humaine médiane 1,0125). A2 : seuil **0,35** avec OCR avant le gate pour
  les bandes basses persistantes (9 bandes 37–45 %). A3 : review-only, trié
  par amplitude/clés, `high_confidence` et `gap < 0,7` opt-in. A4 :
  `flash_probable` < 0,3 s sans continuité des deux côtés. A5 :
  `resolution = "resolved"`. A6 inchangé. R2 validé sur 68 (18 keyframes
  GOOD sur le Segment 7) ; R6 validé sur 54/356 (12 revues ajoutées, 6
  utiles, aucun `apply`). Les deux restent opt-in.
  Preuves : `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8/` ;
  mise en œuvre : `docs/lots/LOT-9-POST-STEP8.md`.
- **Étude `vapoursynth-letterbox` pour A1 (D-057, 2026-11-08)** : dépôt
  `Akatmks/vapoursynth-letterbox` (commit `ac274fa`) analysé en lecture
  seule. EWMA 5σ et validation de contour retenus pour un spike natif
  opt-in ; 305 seg 1 mesuré à 81/78 px contre 86/82 px pour
  `cropdetect=24:16:0` (211,6 % vs 213,7 % avec la marge 1,015, humain
  210,0 %). Le masque Kirsch seul ne rejette ni un aplat large ni un ciel
  nuageux ; le 304 seg 9 produit encore un faux `top=935` sur 2/5 frames.
  Décision : pas d'intégration VapourSynth, `cropdetect` reste le défaut,
  clustering temporel requis pour le natif. Rapport :
  `docs/etudes/ETUDE-VSLETTERBOX-A1.md` ; ADR :
  `docs/decisions/DECISION-A1-DETECTEUR-LETTERBOX.md`.
- **Spike Rust A1 (D-057, 2026-11-08)** : exemple
  `crates/workflow-steps/examples/a1_letterbox_spike.rs` (EWMA 5σ, Sobel Y
  3×3 ciblé, couverture de colonnes, clustering ±2 px). Six fixtures,
  `gray` et `chroma` : 305 seg 1/9 à 81/79 (1 px du prototype), 305 seg 17
  en `review`, 304 seg 9 et 356 seg 2 en `noop`, 304 seg 34 en `noop`
  (faux négatif connu, A3 filet). Le Sobel rejette les candidats 935/936
  du 304 seg 9 à 0,38–0,46 de couverture, contre 0,76–0,83 pour
  l'ExKirsch. Coût : analyse Rust 8–17 ms (`gray`), ratio médian 1,12×
  `cropdetect` ; `gray` est le candidat. Aucun code de production modifié ;
  sortie : `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-rust.md`.
- **Évaluation A1 natif sur 201 segments GOOD (D-058, proposition,
  2026-11-09)** : 65 applies + 116 reviews + 20 noops. Natif seul :
  0 faux apply (0/123), 32/56 faux négatifs (57,1 %), 0/13 faux vetos
  résolus (0/7 asymétriques), 305 15/17, 68 6/7, 89 4/34. Le mode
  **natif gardé** est proposé : `cropdetect` d'abord, natif seulement pour
  raffiner un apply avec cluster ≥ 60 %, sinon fallback. Projection pixel
  1,98/1,54 px vs 2,95/2,06 px, 53/63 à ≤ 2 px, coût +0,45 s/apply.
  ADR : `docs/decisions/DECISION-A1-NATIVE-GARDE-D058.md` ; preuves :
  `results/spike-good-sample.{md,csv}`.
- **Calibration A1 natif v2 (D-058, 2026-11-09)** : diagnostic 89
  (couverture 132/226 côtés vs EWMA 22/226), Sobel 20 + couverture 0,45
  pour les barres < 20/255. 89 **33/34** applies, FN clairs **5/56**,
  0 apply sur 20 noop + 92 vetos, 4/6 revues couverture résolues ; Sobel
  seul 17/34, couverture seule 9/34. 304 seg 9 reste `noop` 5/5 ; scale 89
  241,5–242,4 % contre 243,0 % humain. Arbitrage A gardé / B v2 seul /
  C v2 + fallback. Preuves :
  `results/spike-good-sample-v2.{md,csv,json}`.
- **A1 natif gardé — D-058 acceptée (2026-11-09)** : option A retenue,
  `a1_refine_native = false` par défaut. Le natif v2 (Sobel 20/1020,
  couverture 2/3 ou 0,45 sous 10/255, cluster ≥ 60 %) raffine uniquement
  un `apply` déjà décidé par `cropdetect` ; tout `review`/`noop`/instable
  ou échec de veto conserve `cropdetect`. Détails additives
  `native_refined`, `native`, `cropdetect`. Validation 305 : 16/17 applies
  à 211,8 % au lieu de 213,7 %. Tests unitaires + `fmt` + `clippy` +
  `cargo test --workspace` (557) verts. Passe 2 609 segments et activation
  pilote à faire ; option C reportée à D-059.
- **Grounding DINO — spike A2 (D-060, 2026-11-09)** : modèle tiny
  Swin-T (659 Mio) évalué hors production sur 242 frames. Au seuil 0,20,
  96,7 % des 122 logos du catalogue à IoU ≥ 0,50, 98,3 % des 60 bandeaux
  A2, 97,2 % du texte (IoU ≥ 0,30), mais 25 % de faux positifs frame sur
  60 négatifs. Le clustering IoU ≥ 0,50 + centre ≤ 10 px + persistance
  ≥ 70 % donne 0/5 clip négatif persistant et 5/5 logos positifs
  persistants ; texte persistant sur 3/5 clips (54/58 à 100/91,7 %,
  40 CITROUILLES à 83,3 % ; 40 ABHARTACH/FOOTBALLEUR à 66,7/50 %,
  cohérent avec les bandeaux animés). Coût local 1,279 s/frame à 480 px,
  1,47 Gio de VRAM, 15,3 s pour 12 frames. Décision : laboratoire
  seulement, aucun flag de production ; Replicate écarté (données client,
  réseau, secret ; non exécuté), ONNX non prêt (pas de config Optimum,
  ops custom). Preuves : `docs/etudes/ETUDE-GROUNDING-DINO-A2/` ; ADR :
  `docs/decisions/DECISION-GROUNDING-DINO-A2.md`.
- **Threads OCR A2 (D-053)** : `a2_ocr_threads` configurable, 8 en
  production ; sur `54`, OCR 23,6 s à 2 threads, 13,2 s à 8, 12,5 s à 12.
  A1 reste séquentiel (~23,5 s). `a2_ocr` est activé en production (D-055) ;
  `a2_logo_prior` reste `false`.
- **EffectCraft 0.22.0** : STEP8 réel sur copie temporaire de `54`
  (1/1 projet, `MS_BATCH_RESULT ok`), post STEP8 (`_assist`, audit `pass`)
  et ouverture du projet assisté par `effectcraft-cli --json info` validés.
- **Contrôle QA read-only Stage 0 + OCR + logos + visage (D-036 à D-048)** :
  `workflow-cli qa run` écrit `qa.json` sous `--out`, distinct de
  l'`audit.json` A5/A6. 35 exports, 35 `pass`, 0 écart Stage 0 avec le PoC.
  PP-OCRv6 ONNX CPU : 1 124 échantillons, 12 détections isolées, 0 finding
  sur 35 GOOD, 12/12 sous-titres synthétiques. Logos : masque alpha des
  overlays `.ecproj`, 30/32 exports évalués `pass`, 2 exports `review` (270,
  branding source, 4 findings), 264/386 détections masquées, galerie
  synthétique 60/60 sur 22 fonds (IC 95 % 94,0–100 %) et 0/16 FP, assets
  réels 34/39 (87,2 %) et 0/17 FP, 13/13 masqués, 11/13 à l'échelle 1,0 ;
  regroupement spatial et containment des boîtes OCR. Vérificateur DINOv2
  actif par défaut (`--no-logo-visual` pour l'opt-out) : union 39/39 sur la
  galerie (5 sauvetages), 0/13 masqués, 0/4 négatifs, 0 nouveau `pass`
  signalé sur les 32 exports avec projet ; 270 reste `review` et est confirmé
  intentionnel (split screen). Validation réelle par ablation : 14/14
  overlays non déclarés détectés, 0/14 avec le projet d'origine, 298 sauvé
  par DINOv2 (2/2).
  YuNet : 745 échantillons avec visage,
  0 finding sur 35 GOOD, 2/2 recadrages synthétiques. QA complet 298 en
  32,1 s ; mode lot `--folder` à 4 workers (Stage 0 : 4,01 s/export), cache
  par empreinte (0,23 s sur hit) et `export_sha256` avec une frame WebP par
  finding. `auto_approve_eligible` est vrai sur un `pass` complet avec les
  contrôles requis exécutés, faux sinon (D-049). Détail :
  `docs/lots/LOT-10-QA.md`.
- **Clustering QA 1.3.0 (D-051, 2026-11-07)** : l'OCR regroupe désormais les
  détections par boîte stable au lieu d'une moyenne de bloc ; un texte
  persistant et dominant devient `fixed_overlay` (check dédié, non
  bloquant), un texte changeant reste `subtitle_burned_in`. Rejeu réel :
  `40 ABHARTACH`, `40 FOOTBALLEUR`, `54` et `58` produisent chacun un
  `fixed_overlay` (100 % de persistance, dominance 96–100 %), `54` détecte
  en plus un sous-titre temporaire auparavant manqué. Schéma `qa.json`
  1.3.0, cache 7. Non-régression OCR sur 36 exports GOOD : 36 `pass`,
  0 finding.
- **Module `vision` (D-051, phase 2, 2026-11-07)** : `frame`, `inference`,
  `ocr`, `dino`, `text` et `hash` sont partagés sous
  `crates/workflow-steps/src/vision/` ; `qa/ocr.rs` et `qa/logo.rs` ne
  portent plus que l'orchestration. Rejeu QA de `54` identique (hors
  timings), ce qui prépare le gate OCR de A2 sans dépendance `post → qa`.
- **Gate OCR A2 (D-051, phase 3, 2026-11-07)** : `a2_ocr = false` par
  défaut ; 12 frames, PP-OCRv6 CPU, fusion des boîtes d'une même ligne,
  veto sur texte changeant, bandeaux sombres en `review` avec scale
  proposé, garde-fous durs inchangés. `54` reste `apply` 200,8 %, `58`
  passe à `apply` 210,0 %, `40` restent `review` avec `details.ocr`,
  `sub_fast` est vétoé (dominance 25 %). Projets assistés : diffs limitées
  à `Scale`, audits `pass`. Coût 15–25 s/projet à 2 threads, 13,2 s à 8.
- **Registre spatial des logos source (D-052, phases A/B/C, 2026-11-08)** :
  catalogue versionné de 30 émissions (29 bas-gauche, Piquantes ! haut-gauche),
  21 clean feeds et 9 émissions sans logo ; la 30ᵉ est *Les Reines de la
  Route* (6ter, bbox `[99, 848, 315, 1022]`) ; crops RGBA externes. A2 annote
  `details.source_logo` derrière `a2_logo_prior = false` et retire du groupe
  OCR dominant les textes du logo (`logo_texts`, `logo_only`), le QA ajoute
  le check `source_logo` (`pass`/`info`) derrière `--source-logo`. A2 lit
  aussi les overlays déclarés : si, aux douze instants du gate OCR, toutes
  les occurrences visibles du logo sont couvertes à ≥ 90 %, `visible_in_comp` passe à `false` et le veto logo
  seul est neutralisé (rejeu `54` avec habillage synthétique : `apply`
  200,8 % inchangé, 2/2 occurrences masquées).
  `--source-logo-visual` confirme localement par NCC/contours et mesure la
  stabilité des zones génériques, sans décision. Rejeu : `54`/`58`
  inchangés, `40` reste `review` mais gagne la bannière réelle (91,7 %,
  scale 218,2 %), `sub_fast` toujours vétoé ; 35 exports GOOD avec `.ecproj`
  → 25 `pass`, 10 `info`, 0 nouveau finding ; quatre rendus H.264 construits
  depuis les rushes et transformations réelles (`54`, `58`, `40 ABHARTACH`,
  `40 FOOTBALLEUR`) → NCC 0,95 / 0,94 / 0,64 / 0,58, 3/3 ; 10 `info` non
  confirmés car le logo source n'est pas à la position projetée dans
  l'export final (logo social ajouté manuellement en overlay ; vérification
  manuelle des rushes UJUD/66M : logo présent sur toute la durée ; balayage
  exhaustif à 1 i/s : maximum 0,38). Phase D optionnelle (DINOv2
  résiduel, confirmation A2) non implémentée.
- **Staging OCR A2 (D-054, 2026-11-08)** : PP-OCRv6 det/rec installés sous
  `assets/models/qa/` (hashes du manifeste), runtime ONNX local dans
  `vendor/`, `a2_ocr_threads = 8`. Copies de `40`, `54`, `58` : `54` reste
  `apply` 200,8 % (confiance 0,90 → 0,95), `58` passe `review` → `apply`
  210,0 %, `40` restent `review` avec OCR mais `coverage_x` 0,09–0,11 ; audits
  `pass`, lot 87,3 s → 130,7 s. `a2_ocr` et `a2_logo_prior` restent `false`
  jusqu'à la validation du 2026-11-08 (D-055) ; `a2_logo_prior` seul
  confirme la phase B sur `40` (`scale_target` 218,2, dominance 0,9167).
- **Activation OCR A2 (D-055, 2026-11-08)** : `a2_ocr = true` en
  production, `a2_logo_prior = false`. Vérification avec la configuration
  de production sur une copie de `54` : `apply` 200,8 %, OCR 13,0 s, post
  43,9 s, audit `pass`. `a2_logo_prior` différé : sur `54`/`58` il ne
  change que `texts` → `logo_texts` ; sur `40` il change le motif et
  propose 218,2 % (review).
- Bypass direct SDM (D-034) et archive-only LPHDF (D-035) actifs : STEP2–STEP6
  sautés, CSV de scènes synthétisé par STEP7 ; SDM produit brut + A6, LPHDF
  aucun `.ecproj` (validé sur 295, 340, 341 et lots mixtes).
- Chaîne STEP1 → STEP8 validée sur `1720 Camille.zip`, puis sur `1531 Camille`
  via le frontend Rust (8/8 étapes `completed`).
- Widget système Rust : CPU/RAM/disque et GPU `nvidia-smi` ; auto-ouverture
  des logs désactivée par défaut (toggle conservé).
- Journal serveur enrichi (configuration, requêtes, webhook, étapes,
  séquences), ANSI désactivé à l'écriture fichier et terminal limité à la
  session courante au démarrage.
- Lanceur Rust : la fermeture de la fenêtre de terminal arrête désormais le
  serveur `:5004` (SIGHUP intercepté, `cleanup` robuste au pty détruit).
- Shadow STEP5/6 `162/162` sur 20 projets.
- Sync réelle : 911 URL, 96 archives, 640 hash STEP7 (1,8 Go).
- Audit documentaire du 2026-10-24 : commandes de parité corrigées et état de
  production aligné sur Rust `:5004`.
- Remédiation de l'audit backend du 2026-10-24 : 33 constats corrigés,
  3 documentés (C3 → D-025, C4 → D-015, C8 → D-024). `cargo fmt --check`,
  clippy strict, 280 tests et `config check` verts.
- Synchronisation documentaire du 2026-10-24 : statut `stopping`, en-têtes de
  sécurité, `server.allow_remote`, quotas STEP1, purge des résidus et index
  D-025 à D-027 alignés sur le code. Audit `.agents/` : `codingstandards.md` et
  les 22 `SKILL.md` alignés (tests 280, `max_workers` retiré, verrou STEP1,
  gate EffectCraft, legacy en lecture seule).
- Étude STEP9 agentique terminée (D-029) : consolidation déterministe (A0–A5 :
  profilage, letterbox, overlays source, keyframes, transitions, rapport)
  d'abord, agent QA read-only (B) ensuite, co-monteur (C) en laboratoire.
  Logos, jingle et rendu restent humains. Aucun code de production modifié.
- **Post-traitement A5/A6 intégré (D-032)** : après un STEP8 réussi, le brut
  reste intact et `<stem>_assist.ecproj` est toujours publié, avec
  `post.json`/`review.json`/`audit.json` sous `docs/post/`. A1 letterbox, A2
  bannière, A3/A4 en revue, A6 template SDM. Commande `workflow-cli post`.
  Validation 295 au pixel (0/255), parité de décisions avec le prototype sur
  268/187/191/181, 270/276/286/287 sans changement d'échelle, A2 synthétique
  11/11, 315 tests, fmt/clippy verts. Depuis D-050, la barre STEP8 prolonge
  sa progression pendant le post (`progress_phase = "post"`, marqueurs
  `POST_*`). Reste : exposer `review.json` dans l'UI, recalibrer A3/A4 et
  traiter 179/287.

## Étude logos et bandeaux fixes (2026-11-07)

- **Périmètre** : audit de `post/a2.rs` et de `qa/ocr.rs`, `qa/logo.rs`,
  `qa/dino.rs`, `post/overlay.rs` ; benchmark de sept dépôts GitHub (YOLOv7,
  YOLOv8 API, open-set YOLOv5, Logodetect, VideOCR, deux détecteurs
  morphologiques OpenCV) ; diagnostic en lecture seule sur `40`, `54` et
  `58 Camille`.
- **Conclusion benchmark** : la pile Rust existante (PP-OCRv6 + DINOv2 +
  `OverlayMask` + persistance) reste la référence pour le broadcast
  généraliste. Les YOLO sont fermés et/ou copyleft, Logodetect exige un
  asset exact et est AGPL, VideOCR confirme PaddleOCR mais reste une
  application Python, les détecteurs morphologiques n'ont ni OCR ni
  persistance. Aucun import retenu.
- **Défaut QA mesuré** : l'agrégation par moyenne de bloc jette les bandeaux
  persistants dès qu'un sous-titre coexiste. Dérive verticale 0,0642 (`54`),
  0,0637 (`58`), 0,0562 (`40 FOOTBALLEUR`) contre `MAX_Y_DRIFT = 0.04` ;
  trois bandeaux sur quatre sont manqués, `40 ABHARTACH` (dérive 0,0019) est
  signalé mais classé `subtitle_burned_in`.
- **Défaut A2 mesuré** : décision purement géométrique ; `40 ABHARTACH` et
  `40 FOOTBALLEUR` sont sombres (médiane 37 et 55) et les quatre candidats
  ont une géométrie de scène instable, alors que l'OCR lit le même texte
  fixe sur 96–100 % des échantillons (score 0,979–0,987, dérive Y 2–9 px).
  L'application `54` est propre : 112 différences JSON, toutes des `Scale`
  180,0 → 200,8, aucune ancre ni position.
- **Décision D-051** : QA 1.3.0 avec clustering par détection et
  `fixed_overlay` ; A2 v3 avec gate OCR derrière `a2_ocr = false`, garde-fous
  durs inchangés, veto sur texte changeant, bandeaux sombres en `review`
  avec preuve et scale proposé ; extraction ultérieure d'un module neutre
  `vision`. Aucun code de production modifié.
- **Validation** : 442 tests, `fmt` et `clippy -D warnings` verts ; QA, frames
  et médianes sous `/tmp/workflow_investigation/` ; aucune écriture dans
  `/mnt/cache`.
- **Référence** : `docs/etudes/ETUDE-LOGOS-BANDEAUX-POST-QA.md`.

## Synchronisation documentaire (2026-10-28)

- Audit du workflow `docs-updater` : 315 tests (283 `#[test]` + 32
  `#[tokio::test]`), `clippy -D warnings` et `cargo fmt --check` verts, arbre
  git propre, aucun TODO/FIXME/HACK dans `crates/`.
- Comptes de tests corrigés de 282 à 315 dans `memory-bank/systemPatterns.md`,
  `memory-bank/productContext.md`, `.agents/rules/test-strategy.md`,
  `.agents/skills/tests-suite-guardian/`, `.agents/skills/workflow-operator/`
  et `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`.
- `parity/DRIFT.md` D-015 : `/api/system_monitor` sert des métriques réelles
  CPU/RAM/disque/GPU ; seul `/api/performance/frontend` reste un stub.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md` passe en version 1.5 :
  nomenclature STEP8 avec `_assist.ecproj` et `docs/post/`, panneau
  `assets/effectcraft/MediaSolution.jsx`, 161 clés TOML et sections réelles
  (`[legacy]`, `[steps.filter]`, `[steps.step8.post]`, `[retention]`).
- `docs/lots/LOT-1-STEP8.md` pointe la copie versionnée du panneau ;
  `docs/lots/LOT-8-INGESTION.md` date ses comptes de tests de clôture.
- `.agents/skills/workflow-operator/` documente le hook A5/A6 obligatoire
  après STEP8 (`--no-post`, `post run`, `--report-only`) et les artefacts
  `_assist`.

## Post-traitement A5/A6 (2026-10-27)

- Code : `crates/workflow-steps/src/post/` ; hook dans `step8.rs` ;
  commande `workflow-cli post run`.
- Invariants : brut inchangé, `_assist.ecproj` toujours publié (copie exacte
  sans décision), artefacts additifs, A1/A2 sans déplacement d'Anchor Point,
  A3/A4 en revue, A6 éligible court-circuite A1–A4, conflit A1/A2 → revue.
- Validation : 295 assist identique à la référence et rendu 0/255 à
  0/10/30/50 s ; décisions identiques au prototype sur 268, 187 DISNEYWORLD,
  187 ECOLE, 191 et 181 ; 270/276/286/287 0 apply et assist identique au
  brut ; A2 synthétique 11/11 ; idempotence 187 ECOLE.
- Configuration : `[steps.step8.post]` ; `--no-post` pour désactiver ;
  `post run --report-only` pour ne pas publier d'assist.
- Documentation : `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-1-STEP8.md`,
  `docs/architecture/CONTRAT-ENVIRONNEMENT.md`, étude §3.9, protocole §14.1.
- Limites : pas d'exposition UI/CEP, A3/A4 en revue, bords non noirs et
  recadrages sémantiques en revue, A6 multi-segment/keyframé en revue,
  timeout serveur STEP8 incluant le post, 179/287 partiels.

## Validation production A5/A6 (2026-10-28)

- Batch STEP2–8 sur `223 Camille` (2 vidéos) : 7/7 étapes `completed`,
  STEP8 2/2 projets, post 2 assist, 0 apply, 13 review, 84 noop, 0 échec,
  audit `pass` sur les deux projets.
- Brut et assist **octet pour octet identiques** (aucune action auto) ;
  `_ae.json`, `_audio.json` et CSV non modifiés (mtimes antérieurs au post).
- Revues : A1 barres asymétriques haut 6 / bas 82 (2 segments Audi, 1
  Peugeot), A2 bande animée (Audi) / logo étroit (Peugeot), A3 7 candidats
  (désactivé), A4 segment court 0,32 s (Peugeot). A6 non déclenché.
- Clamps panneau Peugeot segments 16/19/21 (300/1620 px) : couverture exacte,
  `source_coverage` pass, pas de revue A5.
- Le STEP8 isolé de 00:28 avait échoué sur l'ancien manifeste
  `/mnt/cache/309 Camille` (dossier introuvable), avant le batch ; sans lien.

## Étude STEP9 agentique (2026-10-26)

- Documents : `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`,
  `docs/etudes/SPIKE-STEP9-PROTOCOLE.md`.
- Corpus : `298 Camille_GOOD.zip` et `300 Camille_GOOD.zip`, comparés aux
  sorties STEP8 brutes.
- Constats : 117 segments comparables, 0 ancre STEP6 modifiée, 23/120 calques
  keyframés (72 clés), 92 segments à +27 % d'échelle sur 300, 1 fusion et
  3 splits, 2 logos + 1 jingle sur 298.
- Décision D-029 : A0–A5 maintenant, B en PoC, C en laboratoire ; pas d'agent
  en écriture sur la production, pas de dépendance cloud bloquante ; logos,
  jingle et rendu restent humains.
- Prochaine étape : spike A sur copies isolées, sans toucher aux 8 étapes.

### Phase 0 du spike (2026-10-26)

- Baseline STEP8 reproductible : calques identiques sur 298/300, seuls les
  chemins médias changent.
- Labels : 118 segments bruts, 120 finaux, 0 ancre modifiée, 23 keyframés,
  72 clés, 1 fusion, 3 splits, Null parent 118 → 0.
- A0/A2 : bande de 108 px détectée sur 300 → scale 207,4 % (humain 207 %) ;
  no-op sur 298 ; A1 no-op (aucun letterbox).
- A3 : 39 % des clés humaines à ≤ 60 px d'un objet suivi ; règle de sélection
  précision 0,62 (298), 0,29 (JOSEPH), 0,20 (KATIA). Re-cadrage statique de
  300 non déterministe avec `_ae.json` seul.
- Rapport : `/tmp/opencode/step9-spike/01-baseline/RAPPORT-PHASE0.md`.
- Prochaine étape : A2 sur copie (rendu de contrôle), prototype A3 conservateur
  sur 298, A4 splits/transitions.

### Corpus élargi du spike (2026-10-27)

- 19 paires / 32 vidéos exploitables : 179, 181, 187, 191, 208, 211, 221,
  223, 227, 266, 268, 270, 276, 286, 287, 295, 298, 300, 303.
- 1059 segments bruts, 520 clés humaines, 145 segments keyframés.
- 278/520 clés (53,5 %) à ≤ 60 px d'un objet suivi ; 458/907 segments
  statiques (50,5 %) à ≤ 50 px du brut, 363/907 (40 %) à > 100 px.
- A3 invalidé en auto : précision 0,20, rappel 0,23 ; 35 faux positifs sur
  les vidéos « sans piège ». Décision D-030 : candidats en `review` ou flag
  projet.
- A1 validé sur le letterbox noir (191 : 249,6 % vs 255 % avec marge 4 % ;
  268 : 179,1/222,2 % par segment) ; bords non noirs de 268 en `review`.
- A2 fragile : 207,4 % correct sur 187 DISNEYWORLD, faux positif à 260 % sur
  187 ECOLE ; logos de coin (191) et logo de template (295, `sdm.png`) hors
  A2 ; corrigé en v2 (D-031).
- A6 SDM validé sur le GOOD 295 reçu le 2026-10-27 : brut et GOOD partagent
  le même `_ae.json`/`_tracking.json` (SHA-256 identiques). Template fond
  180 % flouté 50 + panneau 84,3497774862575 % à y=171,032 + panneau
  56,294573830734535 % à y=1082,026 + logo `sdm.png`, reproduit par
  `06-corpus/a6_template.py` à 0/255 (exact, 0 pixel > 2) à 0, 10, 30 et
  50 s. Wrapper `03-scenario-a/a6_apply.py` : déclencheur configuré
  `M6 - SDM - `, 295 → apply/audit pass, 187 → noop/audit pass, 295
  keyframé → review sans projet ; rendu relinké 0/255. Reste à l'intégrer
  au post-traitement de production.
- Réserves : 179 sans `.ecproj` GOOD, 287 avec 1 référence sur 3, 2CV
  exclue ; dérive de baseline identifiée comme STEP4/5 (tracking legacy
  vs Rust), pas STEP6/8. Le STEP6 Rust sur le `_tracking.json` legacy
  reproduit le `_GOOD/_ae.json` à l'identique (4355/4355 sur 187,
  2979/2979 sur 181) ; delta d'ancre médian vs GOOD ramené de 62,75 à
  8,4 px sur 187 et de 44,01 à 2,43 px sur 181 avec la recette legacy.
  Le commit legacy `0e9765b` du 2026-10-16 19:01 (`STEP4_TPU_FAST=1`,
  `object_min_missing=25`) est postérieur aux GOOD ; `--object-min-missing
  1` remonte la couverture personne à 1722 frames vs 1623 legacy, mais ne
  ramène pas les ancres (64,33 px) : le gate n'explique que la couverture.
  Diagnostic : `/tmp/opencode/step9-spike/06-corpus/DIAGNOSTIC-DERIVE.md`.
- Clamp du panel : correct pour une position centrée (`[C/(2s),
  W-C/(2s)]`) ; les ancres hors plage des GOOD sont des déplacements
  manuels de `Position` couverts par l'intervalle généralisé
  `[p/s, W-(C-p)/s]`, sauf 181 segment 34 (vide de 36,7 px) à signaler en
  `review`.
- Rapports : `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` §3.6/§3.8,
  `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` §4.6/§5.7/§8.1,
  `/tmp/opencode/step9-spike/06-corpus/RAPPORT-CORPUS.md`,
  `RAPPORT-A6.md` et `DIAGNOSTIC-DERIVE.md`.
- Prochaine étape : A5 (`post.json`, `review.json`, `audit.json`), validation
  A1 sur letterbox synthétique, puis A2 durci et intégration A6.

### A5 v2 et A6 intégré (2026-10-27)

- `a5_report.py` v2 : hash vidéo échantillonné (3 × 4 Mio), `timings_ms`,
  A1 10e percentile + couverture mathématique + keyframes en `review`, A2
  haut/bas + sombre + géométrie + logo/partiel/décor, `--template sdm`.
- A1 : 268 15 applies à 231,1 % + Y −287,9 px ; 191 37 applies à 249,6 % +
  Y −367,8 px enfant ; 181 Concierge 0 apply, 10 barres asymétriques en
  `review`. Rendu 268 : 194/170 px de barres → 0/0 à 2,5/5/9/24,5/35/54 s.
- A2 : 187 DISNEYWORLD/ECOLE 206,6 %, géométrie 1,3/2,6 et 0/0 px ; 191
  logo review ; 268/181 noop. Validation synthétique 11/11 : 3 apply,
  7 review, 1 noop.
- A6 : `a5_report.py --template sdm` émet la décision et le bloc `template` ;
  295 apply/audit pass, 295 keyframé review, 187 noop. `a6_apply.py --apply`
  reste le seul writer du projet.
- Coût A5 : 3,7–33,5 s par vidéo (0,07–0,24× la durée, sans rendu).
- Rapport : `/tmp/opencode/step9-spike/06-corpus/a5v2/RAPPORT-A5-V2.md` ;
  décision D-031.
- Prochaine étape : brancher A5/A6 dans le post-traitement de production,
  hors panel ; références 179/287 toujours manquantes.

### Prototype A5 (2026-10-27)

- `03-scenario-a/a5_report.py` : rapport déterministe A0–A4, écrit
  `post.json`/`review.json`/`audit.json` sans modifier le `.ecproj`.
- 4 cas : 268, 187 DISNEYWORLD, 187 ECOLE, 191 cancer.
- A2 : 206,6 % vs 207 % sur 187 (0,4 %) ; décor de 187 ECOLE séparé ; logo
  de 191 en `review`.
- A1 : 231,1 % (268) dans la plage 221,3–239,7 % ; 249,6 % vs 255 % (191,
  2,1 %). Validation visuelle : A2 propre ; A1 laisse 19,5 % de barre en haut
  sur 191 (ancre décentrée), mais la compensation Position Y de -417,8 px
  supprime les barres. A5 prescrit scale + compensation et signale le cadrage
  vertical en `review`.
- A3 : précision 0,29–0,60, rappel 0,20–0,43, jamais appliqué.
- A4 : cut dur intra-segment désactivé par défaut (32 faux positifs sur 187).
- Rapport : `/tmp/opencode/step9-spike/06-corpus/a5/RAPPORT-A5.md`.
- Prochaine étape : contrôle de couverture A1 (Position Y), A2 durci,
  intégration A6 opt-in avec le profil mesuré.

## Rollback Tauri (2026-10-26)

- Décision D-028 : abandon du client desktop Tauri et retour à la stack Rust
  headless avec `assets/web`.
- `git reset --hard` sur `a93a554c4c5ed5e17805575cf15ac533d7cd50a3` ; branche
  de sauvegarde `backup/tauri-abandon-20261026` sur `1613b17`.
- Crates desktop, paquet `workflow-manager`, unité `workflow-server.service`,
  scripts PoC et documentation desktop retirés ; `target/` purgé (3,8 Gio).
- Validation : 280 tests au checkpoint, 282 après correctifs, fmt/clippy,
  `config check`, build release, serveur `:5004` seul.
- Correctifs réappliqués : `514e2d6` (ETXTBSY), `4ddb91b` (secrets, 281)
  et `713b6bc` (polling web masqué, 282). La réorg docs reste dans la
  branche de sauvegarde.

## Chronologie de migration

| Lot | Dates | Commits principaux | Livré | Validation |
| --- | --- | --- | --- | --- |
| 0 | 2026-10-22 | `e83b0a9`, `4440718` | Workspace, contrat, harnais de parité, spike TPU, modèles | 23 cas de parité, spike stable |
| 1 | 2026-10-22 | `d792e06`, `6530ba6`, `afdc93f`, `43c5fbf`, `e195a89`, `c0b6482`, `97ef1bc` | STEP5 audio/vidéo, STEP6, bridge, STEP7, STEP8, rétention | Parité 244/245/249/250, shadow 57/57, projet 250 |
| 2 | 2026-10-22 | `50722f1`, `d8f2ad2` | STEP2 TransNetV2 ONNX CUDA | CSV identiques sur 4 cas, VRAM sous baseline |
| 3 | 2026-10-22 | `b0983af`, `b7d2cbc` | STEP4 Coral tracking | Parité `1e-6` sur `face_pan`, `real_244`, `real_245` |
| 4 | 2026-10-22 | `f5d9e81` | STEP3 audio YAMNet/ECAPA/AHC | JSON identiques sur `vox_2spk`, `real_244`, `real_245` |
| 5 | 2026-10-23 | `ea10057`, `b257868` | Serveur axum `:5004`, séquence, arrêt, tests concurrence | 12/12 artefacts, 409, SIGTERM |
| 6 | 2026-10-23 | `b349ca6`, `85f3289` | STEP1 extraction/normalisation | ZIP/RAR/TAR, `.mov`, archive réelle |
| 7 | 2026-10-23 | `5f51ee2`, `34c6b97` | Intégration STEP2 → STEP8, bascule contrôlée | Séquence réelle, parité STEP5/6 |
| 8 | 2026-10-23 | `73b9312`, `f1fc509`, `e0b14b8` | Ingestion webhook, SQLite, R2/Dropbox, routes CSV | 911 URL, audit 0 changement, 462 enregistrements |
| Webhook | 2026-10-23 | `07ae8da` | Activation du moniteur Rust | Base non vide, un seul backend |
| Lanceur | 2026-10-23 | `f827d53` | `start-rust.sh`, secrets, Firefox | Lancement réel |
| Sync | 2026-10-23 | `28287af`, `6b6e5e1` | Fusion Python ↔ Rust | 911/96/640, 1,8 Go |
| Shadow | 2026-10-23/24 | `01fbbf2`, `b7b594b` | Corpus 153–159 et 1720 | 162/162 sur 20 projets |

## Remédiation audit backend (2026-10-24)

| Lot | Contenu | Preuve |
| --- | --- | --- |
| 1 | P0 concurrence : `RunId`, `Stopping`, fin générationnelle, erreurs typées, `parking_lot` | 40 tests serveur, 409 après `/stop`, fin périmée ignorée |
| 2 | Cycle de vie : arrêt des groupes, verrou STEP1, isolation partagée, `allow_remote`, purges | test SIGTERM/SIGKILL de groupe, `bash -n switch-rust.sh` |
| 3 | Parité : `round_to` STEP3, RAR streamé | 0,1 % sur 1,8 s/3 600 s, faux `unrar` multi-Mio |
| 4 | Sécurité/contrats : CSP, constant-time, SSRF, quotas, symlinks, logs, fsync, timeout STEP7 | tests headers, IP privées, zip bomb, symlink hors logs |
| 5 | Robustesse : canal borné, writer bloquant, rotation, progression, purge orphelins, `media.rs`, regex, cache resize | ligne 64 Kio, rotation en cours, progression 3 étapes |
| 6 | Clôture : `unwrap` STEP2, TraceLayer, tests CLI/TPU, docs | 278 tests à la clôture, 280 après les deux commits suivants, clippy/fmt, D-025 à D-027 |

## Détails des validations

### STEP1 → STEP8

- Archive : `/mnt/cache/old/1720 Camille.zip` (1,7 Go, 4 MP4, 1 PNG).
- Configuration isolée sous `/tmp/opencode/step1-8`.
- Résultat : 8/8 étapes, 4 `.ecproj`, aucune écriture dans `/mnt/cache`.
- Durées : STEP1 3 s, STEP2 43 s, STEP3 1 min 40 s, STEP4 2 min 58 s,
  STEP5/6 < 1 s, STEP7 2 s, STEP8 4 s.

### Parité

- STEP2 : CSV identiques sur `cuts`, `short_2997`, `real_244`, `real_245`.
- STEP3 : JSON bruts identiques sur `vox_2spk`, `real_244`, `real_245`.
- STEP4 : JSON identiques à `1e-6` sur `face_pan`, `real_244`, `real_245`.
- STEP5/6 : shadow `162/162` sur 20 projets.
- STEP1 : validation ZIP/RAR/TAR et préservation `.mov`.

### Ingestion

- Import : 911 URL identiques à la base Python.
- Audit de normalisation : 0 changement.
- Webhook réel : 462 enregistrements.
- Téléchargement `276 Camille.zip` : R2 puis repli Dropbox, 183 450 047 octets,
  ZIP valide.
- Sync : 96 archives, 640 hash STEP7, 1,8 Go.

## Réorganisation documentaire (2026-10-28)

- `docs/` passe de 23 documents à plat à six domaines : `architecture/`,
  `lots/`, `exploitation/`, `decisions/`, `etudes/`, `validation/`.
- 24 fichiers déplacés par `git mv` en deux commits : move pur détecté à
  100 %, puis réécriture des renvois.
- `docs/README.md` devient l'index : taxonomie, statuts, parcours par profil,
  table par étape et correspondance ancien → nouveau.
- 211 renvois réécrits dans 30 fichiers (README, AGENTS, règles, skills,
  workflow, memory-bank, parity) et 18 renvois internes normalisés en chemins
  repo-relatifs.
- Convention audits : `docs/validation/audits/AAAA-MM-JJ-<sujet>.md` (D-033).
- Les chemins applicatifs `<projet>/docs/` et `docs/post/` sont inchangés ;
  `cargo fmt`, `clippy -D warnings` et les 315 tests restent verts.

## Prochaines actions

1. **D-024** : diagnostiquer l'écart STEP4 Python sur `153 Camille`
   (4 977 objets contre 11 853). Ne pas utiliser `--prefer-raw` sur ce projet
   tant que la configuration d'origine n'est pas retrouvée.
2. **Reverse proxy** : aucun nginx/caddy/cloudflared n'est installé. La bascule
   publique reste à faire ; voir `docs/exploitation/SWITCH-PUBLIC.md`.
3. **Surveillance production** : vérifier le moniteur webhook Rust après
   quelques jours d'activité, les téléchargements et la cohérence de
   l'historique.
4. **Documentation** : maintenir `README.md`, `docs/README.md`,
   `docs/lots/LOT-*.md`, `docs/architecture/CONTRAT-ENVIRONNEMENT.md` et
   `parity/DRIFT.md` à jour.
5. **Memory-bank** : mettre à jour `activeContext.md` et ce fichier à chaque
   session significative.
6. **Interface** : ne pas réintroduire de client desktop Tauri/GTK/WebKit ;
   D-028 reste la référence.

## Politique de rétention

- Ce fichier conserve les tâches depuis octobre 2026.
- Les périodes plus anciennes seront résumées et déplacées vers
  `memory-bank/archives/` si le volume le justifie.
- L'historique Python legacy reste dans
  `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.
