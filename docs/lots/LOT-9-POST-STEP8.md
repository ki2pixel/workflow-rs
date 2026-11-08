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
- `apply` sinon : `scale = 1920 / active × 100 × 1,04`, puis compensation
  `Position Y` pour centrer la zone active. La couverture mathématique de tous
  les crops est vérifiée après scale et compensation (tolérance 0,5 px).

Validation : 268 → 15 segments à **231,1 %** et Y **−287,9 px** ; 191 → 37
segments à **249,6 %** et Y **−367,8 px** (cible enfant), soit −417,8 px en
coordonnées composition. Les ancres restent identiques au brut.

## A2 — bannière basse pleine largeur

- Médiane temporelle à 1 fps sur une miniature 480 px de large ; bandes
  claires et paires de bords sombres.
- Une bande **haute** plausible, un logo (< 35 % de largeur), une bande
  partielle (35–50 %), un décor (> 200 px), une bande sombre (médiane < 60)
  ou une bande dont la géométrie bouge (centre/hauteur > 12 px) partent en
  `review`.
- Une bannière qui change seulement de texte ou de luminosité reste
  applicable si sa géométrie est stable.
- `apply` seulement si largeur ≥ 50 %, hauteur 24–200 px (seuils ramenés à
  la hauteur source), persistance ≥ 60 %, aspect ≥ 3, géométrie stable et
  scale ≤ 300 % :
  `scale = hauteur_comp / (hauteur_source − hauteur_bande) × 100 × 1,05`.

Validation : 187 DISNEYWORLD et ECOLE → **206,6 %** (bannière de 104 px),
bannière supprimée à 10/40/100 s. Les 11 cas synthétiques donnent 3 `apply`,
7 `review`, 1 `noop`, conformes au prototype.

### Gate OCR A2 v3 (D-051, 2026-11-07)

Le gate est **opt-in** (`a2_ocr = false`) : sans flag, la décision A2 reste
strictement géométrique et aucun modèle ONNX n'est ouvert. Quand il est
activé, 12 timestamps uniformes sont lus sur le crop du candidat (marge de
16 px sur les quatre côtés), PP-OCRv6 det/rec tourne en CPU (2 threads), et
les boîtes d'une même ligne sont fusionnées avant l'agrégation. Le texte
d'un seul caractère est ignoré.

Features : `text_persistence` (échantillons avec texte / total),
`dominant_ratio`, `y_drift_px`, `score`, `overlap`, `coverage_x`.

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
  haute, largeur < 50 %, hauteur hors 24–200, persistance < 60 %,
  `temporal_std` > 35 et scale > 300 % ;
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

Coût mesuré : 15–25 s par projet sur le Threadripper. Modèles absents ou
OCR en échec → warning et retour à la décision A2 actuelle, sans échec du
post. Les fichiers `PP-OCRv6_det_small.onnx` et `PP-OCRv6_rec_small.onnx`
doivent être copiés sous `paths.models/qa/`, ou renseignés en absolu.

### Registre spatial des logos source (D-052, phase A, 2026-11-08)

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

## A3/A4 — revue seulement

- A3 lit `_ae.json` et `segment_recommendations`, calcule l'amplitude et le
  trou de suivi, et signale des candidats de keyframes en `review`. Jamais
  d'application.
- A4 signale les segments courts, les fusions CSV et les bornes non alignées
  en `review`. La détection de cuts durs intra-segment reste désactivée
  (trop de faux positifs).

## A6 — template SDM

Déclencheur explicite : préfixe configuré `M6 - SDM - ` sur le nom de la
source (ou, à défaut, sur le nom du projet sans `_9x16`). Le contenu ne sert
jamais à deviner le template.

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

Prédicat volontairement conservateur : nom préfixé par `template_prefix`,
`.mp4`, H.264, 1920×1080, SAR 1:1, 25 fps CFR, durée de flux et `nb_frames`
valides. Un nom SDM qui ne respecte pas ce profil repart dans le pipeline
normal.

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
template_prefix = "M6 - SDM - "
# Familles sans `.ecproj` : CSV de scènes archivé uniquement.
archive_only_prefixes = ["W9 - LPHDF"]
ffmpeg = "ffmpeg"
ffprobe = "ffprobe"
a1_margin = 1.04
a2_margin = 1.05
# Gate OCR A2 (D-051), désactivé par défaut.
a2_ocr = false
a2_ocr_det = "qa/PP-OCRv6_det_small.onnx"
a2_ocr_rec = "qa/PP-OCRv6_rec_small.onnx"
# a2_ocr_onnxruntime = "/chemin/vers/libonnxruntime.so"
# Registre spatial des logos source (D-052), annotation seule.
a2_logo_prior = false
a2_logo_catalog = "qa/source_logos.toml"
```

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

| Cas | Résultat |
| --- | --- |
| 295 SDM | 1 `apply` A6, audit `pass`, assist identique à la référence, rendu 0/255 |
| 295 keyframé | 0 `apply`, 3 `review`, assist copie exacte |
| 268 | 15 A1 à 231,1 %, 9 `review`, 53 `noop`, audit `pass` |
| 187 DISNEYWORLD | 1 A2 à 206,6 %, 14 `review`, audit `pass` |
| 187 ECOLE | 1 A2 à 206,6 %, 2 `review`, audit `pass` |
| 191 | 37 A1 à 249,6 %, 12 `review`, audit `pass` |
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

Les décisions A1/A2/A3/A4 de 268, 187 DISNEYWORLD, 187 ECOLE, 191 et 181 sont
**identiques au prototype Python** (code, action, cible, raison et détails).
Les GOOD 270/276/286 ne changent aucune échelle ; 287 n'a qu'une référence
GOOD sur trois, donc la comparaison y est partielle.

## Limites

- A3/A4 restent en revue ; aucun keyframe ni cut n'est appliqué.
- A1 ne couvre pas les bords non noirs ni les recadrages sémantiques ; ces cas
  restent en `review`.
- A2 OCR est **opt-in** (`a2_ocr = false`) et coûte 15–25 s par projet ; il
  ne couvre que le texte, pas les logos purement visuels. Les bandeaux
  sombres restent en `review` (niveau 1) et les candidats étroits ne sont
  jamais recadrés.
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
