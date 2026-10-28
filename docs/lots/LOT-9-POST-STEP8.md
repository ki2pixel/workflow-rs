# Lot 9 — post-traitement A5/A6 après STEP8

> Statut : intégré à la production le 2026-10-27.
> Décisions : **D-030** (A3 en review, A1/A2 durcis), **D-031** (A5 v2,
> hash échantillonné, A6 mesuré), **D-032** (écriture du projet assisté).
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
- Le panneau `MediaSolution.jsx` et `assets/web` ne sont pas modifiés. Les
  artefacts sont consultables en lecture seule sur disque.
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

## Configuration

```toml
[steps.step8.post]
enabled = true
suffix = "_assist"
projects_subfolder = "projets"
artifacts_subfolder = "post"
template_prefix = "M6 - SDM - "
ffmpeg = "ffmpeg"
ffprobe = "ffprobe"
a1_margin = 1.04
a2_margin = 1.05
```

Le timeout `steps.step8.timeout_s` couvre EffectCraft **et** le
post-traitement lorsque STEP8 est lancé par le serveur. Le coût mesuré du
post-traitement est de 0,07–0,24× la durée de la vidéo (sans rendu).

## Commandes

```sh
# Hook automatique après STEP8 (désactivable).
workflow-cli step8 run --folder /tmp/projet
workflow-cli step8 run --folder /tmp/projet --no-post

# Post-traitement seul, sur un dossier projet.
workflow-cli post run --folder /tmp/projet

# Sur un projet isolé, avec un dossier docs explicite.
workflow-cli post run \
  --project /tmp/projet/projets/video_9x16.ecproj \
  --docs /tmp/projet/docs

# Calcul des rapports sans publier le projet assisté.
workflow-cli post run --folder /tmp/projet --report-only
```

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
| A2 synthétique | 11/11 conformes (3 `apply`, 7 `review`, 1 `noop`) |

Les décisions A1/A2/A3/A4 de 268, 187 DISNEYWORLD, 187 ECOLE, 191 et 181 sont
**identiques au prototype Python** (code, action, cible, raison et détails).
Les GOOD 270/276/286 ne changent aucune échelle ; 287 n'a qu'une référence
GOOD sur trois, donc la comparaison y est partielle.

## Limites

- A3/A4 restent en revue ; aucun keyframe ni cut n'est appliqué.
- A1 ne couvre pas les bords non noirs ni les recadrages sémantiques ; ces cas
  restent en `review`.
- A2 n'a pas d'OCR : une bannière sombre sans bord horizontal net, un logo de
  coin ou une bande partielle ne sont pas appliqués.
- A6 est un template SDM mesuré ; un projet multi-segment ou keyframé part en
  `review`. Le profil est volontairement figé.
- Les artefacts ne sont pas encore exposés dans l'interface web ou le panneau
  CEP ; ils sont consultables sous `docs/post/`.
- Le post-traitement ajoute du temps au STEP8 ; le timeout serveur doit en
  tenir compte sur les vidéos très longues.
- 179 n'a pas de `.ecproj` GOOD et 287 n'a qu'une référence sur trois : la
  validation corpus reste partielle sur ces deux projets.
- Les projets **multi-source** ne sont pas traités par A1/A2 : la vidéo
  n'est pas choisie arbitrairement, les décisions partent en `review`.

## Tests

```sh
cargo test -p workflow-steps post:: --lib
cargo test -p workflow-steps step8 --lib
cargo test --workspace
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```
