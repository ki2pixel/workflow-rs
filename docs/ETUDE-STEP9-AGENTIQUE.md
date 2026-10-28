# Étude STEP9 — Agentique vs limites du montage automatique

> **Investigation & confrontation** : recadrage vertical 9:16, nettoyage
> éditorial et rendu final après STEP8, sous EffectCraft 0.17.
>
> Date : 2026-10-26 — Statut : étude terminée ; post-traitement A5/A6 intégré
> en production le 2026-10-27 (**D-032**).
> Décisions associées : **D-029**, **D-030**, **D-031**, **D-032**
> (`memory-bank/decisionLog.md`).
> Protocole de spike : `docs/SPIKE-STEP9-PROTOCOLE.md`.

## Résumé exécutif

La question n'est pas « faut-il un agent ? » mais « quelle part du travail
restant est réellement une décision ? ». L'analyse du corpus 298/300 montre que
le travail humain post-STEP8 est majoritairement **mécanique, peu dense et
répétable** :

- **117 segments comparables** entre les projets bruts et les projets finaux ;
  l'ancre horizontale calculée par STEP6 n'a été modifiée sur **aucun** segment
  (delta max 0,00 px) ;
- **23 calques sur 120** seulement portent des keyframes, avec **72 keyframes**
  au total, dont 20/23 calques entre 2 et 6 clés (médiane 2, moyenne 3,1) ;
- sur les 94 segments comparables de 300, l'échelle passe de 180 % à **207 %**
  sur 92 segments pour sortir une bannière permanente, plus **2 zooms
  sémantiques** à 254,7 % et 287,6 % ;
- **1 fusion** et **3 splits** de transitions sur 120 segments ;
- 298 ajoute **2 logos** et **1 jingle** de 3 s, et allonge la composition de
  3 s ; 300 n'a aucun habillage ;
- les deux exports finaux mesurés sont en 1080×1920, 25 fps,
  **10,01–10,27 Mbit/s**, aux noms et emplacements attendus.

Autrement dit, l'essentiel du delta de **cadrage et de nettoyage** n'est pas
de la « subjectivité de monteur » : c'est du **contrat manquant** (pas de
keyframes, pas de détection d'overlay, pas de contrôle qualité) et du
**calcul géométrique déterministe**. La subjectivité se concentre sur
quelques décisions ponctuelles : choisir un zoom qui sacrifie du cadre pour
supprimer un sous-titre, réparer une transition, valider esthétiquement le
résultat.

**Périmètre A retenu après clarification** : A couvre uniquement le cadrage et
le nettoyage géométrique — A0 profilage, A1 letterbox, A2 overlays/sous-titres
source, A3 keyframes, A4 transitions, A5 rapport. Le relink est un contrôle
optionnel de sécurité. Le **placement des logos/jingle**, l'**extension de
composition** et le **rendu H.264** restent humains : directives client,
assets personnalisés, export GPU externe. Les 70–85 % ci-dessous portent donc
sur le delta cadrage/nettoyage, **pas** sur le temps total post-STEP8.

**Recommandation :**

| Horizon | Décision | Justification |
| --- | --- | --- |
| Maintenant | **Scénario A** : consolidation déterministe post-STEP8, en spike isolé | couvre le delta cadrage/nettoyage, coût marginal nul, risque faible, testable |
| Ensuite | **Scénario B** : agent QA read-only, local-first | garde-fou utile, ne touche pas aux projets, coût borné, ROI immédiat sur les oublis |
| Laboratoire seulement | **Scénario C** : co-monteur en boucle fermée | subjectivité réelle, mais non-déterminisme, coût et risque industriels encore injustifiés |

Le gain attendu de A est de **70 à 85 % des gestes de cadrage/nettoyage
observés**, sans IA générative. B peut faire passer les cas simples à
**80–90 %** d'auto-validation, mais ne doit pas écrire dans les projets. C ne
doit pas être industrialisé avant que B ait prouvé sa précision sur un corpus
labellisé.

---

## 1. Méthode, sources et limites

### 1.1 Sources

| Source | Usage |
| --- | --- |
| `Guide_Recadrage_Videos.pdf` | règles métier de délégation humaine |
| `crates/workflow-steps/src/step6.rs` | contrat `_ae.json`, ancre, `spread_x`, `needs_review` |
| `crates/workflow-steps/src/step5/video.rs` | réduction du tracking, perte de l'axe Y |
| `crates/workflow-steps/src/step8.rs` | wrapper headless, absence de rendu/QC |
| `assets/effectcraft/MediaSolution.jsx` | panel, cuts, recentrage statique, imports |
| `assets/effectcraft/mediasolution_ec.json` | configuration réelle du panel |
| `effectcraft-cli 0.17.0` | commandes, MCP, `render-frame`, `render` |
| `/mnt/cache/298 Camille.zip`, `300 Camille.zip` | entrées brutes (docs + projets STEP8) |
| `/mnt/cache/298 Camille_GOOD.zip`, `300 Camille_GOOD.zip` | résultats finaux attendus, utilisés comme référence |
| `memory-bank/decisionLog.md` | décisions durables, D-029 |

### 1.2 Ce qui a été mesuré

- extraction des archives dans `/tmp/opencode/step9-study`, jamais dans
  `/mnt/cache` ;
- parsing des `.ecproj` bruts et finaux (JSON, timebase After Effects
  `254 016 000 000` ticks/s) ;
- comparaison couche par couche : ancre, échelle, position, keyframes,
  `in_point`/`out_point`, durée de composition, assets ;
- rendu de frames de contrôle avec `effectcraft-cli render-frame` ;
- rendu complet d'un projet 298 avec `effectcraft-cli render` ;
- lecture du code STEP5/6/8 et du panel ;
- interrogation du serveur MCP EffectCraft (27 outils) ;
- mesure des temps de rendu et des débits avec `ffprobe`.

### 1.3 Ce qui n'a pas été fait

- aucune inférence VLM : aucun modèle multimodal n'est installé sur la machine
  au moment de l'étude ; les coûts et latences VLM sont des **ordres de
  grandeur** à mesurer dans le spike ;
- aucun test sur un vrai letterbox 2.39:1 : le corpus 298/300 est en 16:9
  plein cadre (`cropdetect` = 1920×1072, soit 8 px de bord noir résiduel) ;
- aucun test statistique sur un large corpus : 2 projets, 120 segments, ce qui
  suffit pour une étude d'opportunité, pas pour une validation de production ;
- aucune modification de `/home/kidpixel/workflow_mediapipe`, de `runtime/`,
  de `/mnt/cache` ou des contrats JSON/CSV.

### 1.4 Niveaux de preuve

| Niveau | Signification | Exemples dans ce document |
| --- | --- | --- |
| **Mesuré** | chiffre obtenu par exécution ou parsing | temps de rendu, keyframes, ancres, débits |
| **Inspecté** | comportement lu dans le code | absence de keyframes dans le panel, `videoSkipExtensions` |
| **Inféré** | conséquence architecturale, à valider | coût VLM, qualité d'un modèle local, seuils |

Les recommandations sont formulées pour être **falsifiables** : chaque
scénario a des critères d'acceptation et des critères d'arrêt dans le
protocole de spike.

---

## 2. Décomposition et typologie des tâches post-STEP8

### 2.1 Tableau de classification

| # | Exigence du guide | Nature | Ce que fait le pipeline aujourd'hui | Carence / décision |
| --- | --- | --- | --- | --- |
| 1 | Relier les métrages manquants | Mécanique | STEP8 ouvre les médias par chemin ; même poste, chemins bruts stables | Contrôle optionnel de sécurité, hors périmètre A |
| 2 | Cadrer le sujet qui parle / l'action centrale | Heuristique + sémantique | STEP6 calcule `target_anchor_x` par segment ; le panel applique une ancre statique | Pas d'axe Y, pas de barycentre de groupe, pas de choix du sujet |
| 3 | Groupe : englober un maximum de visages, ou la personne qui parle | Heuristique + sémantique | `tier` et `is_speaking` aident, mais une seule cible est choisie | Le choix « groupe vs locuteur » reste sémantique |
| 4 | Sujet en mouvement : 2 à 6 keyframes lissées | Algorithmique | Aucune keyframe : position statique par segment | Carence STEP6/STEP8, calculable |
| 5 | Coupes manquées (fondus, mouvements brusques) | Algorithmique + sémantique | STEP2 fournit les scènes ; STEP8 duplique et trim les segments | Pas de détection de split/merge, pas de réparation |
| 6 | Bandes noires cinéma | Mécanique | Aucune détection ; échelle fixe 180 % | Carence, `cropdetect` + scale |
| 7 | Zéro logo de chaîne | Heuristique + sémantique | Aucune détection ; imports d'images mais aucun placement | Logo statique = calculable ; logo mobile/transparent = sémantique |
| 8 | Zéro sous-titre incrusté | Heuristique + sémantique | Aucune détection | Bande basse = calculable ; choix du zoom = sémantique |
| 9 | Logo de l'émission en calque supérieur | Mécanique | Les PNG sont importés mais jamais ajoutés à la comp | **Hors périmètre A** : directive client, choix humain assumé |
| 10 | Jingle de fin, composition allongée | Mécanique | Les `.mov` sont explicitement ignorés (`videoSkipExtensions`) | **Hors périmètre A** : directive client, choix humain assumé |
| 11 | Export H.264 CBR 10 Mbit/s, `_9x16.mp4` | Mécanique | STEP8 s'arrête au `.ecproj` | **Hors périmètre A** : export GPU externe + validation humaine |
| 12 | Contrôle qualité plein écran | Sémantique | Aucun contrôle automatique | Décision humaine, assistable par agent |
| 13 | Validation esthétique finale | Sémantique | Aucune | Décision humaine, non délégable sans responsabilité |

### 2.2 Carence de contrat vs arbitrage intrinsèque

**Carence de STEP6/STEP8** — ce qui manque peut être décrit par une règle,
un seuil ou un calcul :

- l'axe Y et les boîtes Y sont présents dans le JSON brut STEP4 mais **jetés
  par STEP5** (`step5/video.rs`, `FastObject` ne garde que `centroid_x`) ;
- `_ae.json` ne contient, pour le cadrage, que `target_anchor_x`,
  `spread_x` et `needs_review` ;
- le panel ne pose aucune keyframe, ne détecte ni bandes noires ni overlays,
  et ne produit aucun rapport de contrôle ;
- il n'existe aucun artefact post-STEP8 (pas de `_post.json`, pas d'audit).

**Décisions humaines assumées (hors A)** — ce qui est volontairement laissé à
l'humain, pour des raisons de personnalisation ou de validation :

- **relink** : contrôle optionnel de sécurité ; les chemins sont stables sur
  le même poste, donc pas un module ;
- **logos/jingle** : directives client par mail, assets variables ; les PNG
  restent importés mais jamais ajoutés à la comp, et les `.mov` restent
  ignorés par le panel ;
- **rendu H.264** : l'export EffectCraft CPU n'est pas optimal et le GPU
  H.264 n'est pas disponible ; l'export reste un passage humain avec
  l'outil GPU externe.

**Arbitrage intrinsèque** — ce qui dépend du sens, du rythme, de l'émotion ou
du contrat client :

- choisir la personne qui compte dans une scène de groupe, surtout si elle ne
  parle pas ;
- décider qu'un plan de réaction est plus important qu'un plan large ;
- accepter de sacrifier un cadrage pour supprimer un sous-titre ;
- juger si un logo est « suffisamment » sorti du champ ;
- valider qu'un zoom ne coupe pas un geste ou un regard ;
- signer la conformité finale.

**Frontière opérationnelle** : si la décision peut s'exprimer par
« si mesurable, alors action déterministe », c'est une carence. Si elle exige
de comprendre le récit, c'est un arbitrage. Le corpus montre que les
arbitrages sont **rares mais coûteux** : 2 zooms spéciaux, 3 splits, 1 fusion,
et la validation finale.

### 2.3 Conséquence architecturale

Il ne faut pas demander à un LLM de faire ce qu'un calcul peut faire. La bonne
architecture est :

```text
STEP6/STEP8 déterministes  ──►  géométrie, keyframes, détection overlays
        │
        └──►  signaux de doute  ──►  agent QA (lecture seule)
                                      │
                                      └──►  humain : arbitrages,
                                           logos/jingle, rendu
```

L'agent n'est pas le moteur de géométrie ; il est le **détecteur de doute** et
le **rédacteur de checklist**. La géométrie reste dans un exécuteur
déterministe, testable et rejouable.

---

## 3. Base de preuve : ce que les projets 298/300 révèlent

### 3.1 Corpus

| Cas | Source | Durée | Segments bruts | Segments finaux | Particularité |
| --- | --- | ---: | ---: | ---: | --- |
| 298 | `The 100 extrait 2.mp4` | 65,8 s | 24 | 24 | 2 logos + jingle, 1 fusion, 1 split |
| 300 JOSEPH | `UJUD - JOSEPH.mp4` | 147,9 s | 36 | 36 | bannière permanente, sous-titres, 1 zoom spécial |
| 300 KATIA | `UJUD - KATIA.mp4` | 214,4 s | 58 | 60 | bannière, archive 4:3, 2 splits, 1 zoom spécial |
| **Total** | | **428,1 s** | **118** | **120** | |

Les projets « bruts » sont les sorties STEP8 déjà présentes dans les archives
`298 Camille.zip` / `300 Camille.zip`. Les projets `_GOOD` sont les résultats
finaux, utilisés **en lecture seule** comme référence. Le projet brut 298
contient un `Null Segments` et 24 segments ; le projet final n'a plus le null
(probable artefact de conversion AE → `.ecproj`), ce qui ne change pas la
comparaison des segments.

### 3.2 Delta humain mesuré

| Cas | Keyframes | Clés | Échelle | Habillage | Durée comp | Transitions |
| --- | ---: | ---: | --- | --- | --- | --- |
| 298 | 5 / 24 calques | 16 | 180 % partout | 2 logos + 1 jingle | +3,0 s (65,8 → 68,8) | 1 fusion, 1 split |
| 300 JOSEPH | 10 / 36 | 25 | 207 % (35/36), 254,7 % (1) | aucun | inchangée | aucune |
| 300 KATIA | 8 / 60 | 31 | 207 % (59/60), 287,6 % (1) | aucun | inchangée | 2 splits |
| **Total** | **23 / 120** | **72** | 92 segments comparables à +27 %, 2 spéciaux | 2 logos, 1 jingle | +3,0 s | 1 fusion, 3 splits |

Les comptes de keyframes incluent les fragments issus des splits (298
segment 21, KATIA segments 26 et 49). Les colonnes **Habillage** et **Durée
comp** sont des observations : le placement des logos/jingle et l'extension
de composition restent **hors périmètre A** (directives client).

**Lecture :**

- **19,2 %** des calques seulement sont keyframés ; **80,8 %** restent
  statiques. Un agent qui inspecte toutes les frames pour « décider » est
  massivement surdimensionné.
- La distribution des clés est `1:1, 2:11, 3:4, 4:3, 5:1, 6:1, 7:2`.
  **20/23** calques sont dans la fourchette 2–6 du guide ; 2 calques à 7 clés
  et 1 à 1 clé sont des outliers.
- L'ancre STEP6 est **stable** : sur les 117 segments comparables, aucun
  `target_anchor_x` n'a été modifié au-delà de 0,5 px. Les keyframes humaines
  déplacent la **Position**, pas l'**Anchor Point**. C'est exactement la
  séparation que doit conserver un exécuteur déterministe.
- L'échelle 207 % sur 300 est un **geste global**, pas un arbitrage par plan :
  elle sort la bannière basse permanente. Les deux zooms à 254,7 % et 287,6 %
  sont, eux, des arbitrages locaux (sous-titre, archive 4:3).
- Les transitions réparées sont peu nombreuses mais réelles : 1 fusion de
  faux cut (298, segments 1+2) et 3 splits (298 segment 21 à 53,64 s ; KATIA
  segments 26 à 82,6 s et 49 à 176,64 s). Deux splits coïncident avec une
  perte de l'objet recommandé ; le troisième avec une discontinuité de suivi.
  Le spike doit confirmer que ces signaux suffisent à les détecter.

### 3.3 Ce que le delta ne montre pas

- Il ne prouve pas que STEP6 a toujours raison : le corpus est petit et
  centré sur deux projets « Camille ». Un autre monteur, un autre genre ou un
  autre format peuvent produire des arbitrages différents.
- Il ne prouve pas que les keyframes humaines sont reproductibles par un
  algorithme : il montre seulement qu'elles sont **éparses, lissées et
  exprimables en Position**, ce qui est compatible avec un générateur
  déterministe. Les `.ecproj` GOOD peuvent provenir d'une conversion
  AE → EffectCraft ; les valeurs observées sont l'état final, pas
  nécessairement l'historique exact des gestes.
- Il ne prouve pas que 207 % est la bonne règle générale : c'est la réponse
  observée à une bannière de hauteur donnée. La règle doit être calculée à
  partir de la boîte de l'overlay, pas codée en dur.
- Il ne prouve pas que le letterbox est un problème fréquent : le corpus n'en
  contient pas. La détection `cropdetect` reste à valider sur un vrai 2.39:1.
- Il ne mesure **aucun taux de faux positifs** : les deux projets contiennent
  au moins un piège, aucun n'est un contrôle propre. Or le risque principal de
  A est de modifier une vidéo qui n'avait besoin de rien. Le corpus élargi
  (protocole, §8.3) doit donc contenir une majorité de vidéos sans piège.
- Les deux projets partagent la même émission, le même habillage et le même
  passage 180 % → 207 % : les 70–85 % de couverture sont une **borne haute**
  pour ce type de contenu, pas une prévision sur un catalogue hétérogène.

### 3.4 Résultats Phase 0 du spike (2026-10-26)

La Phase 0 du protocole `docs/SPIKE-STEP9-PROTOCOLE.md` a été exécutée
sur des copies isolées sous `/tmp/opencode/step9-spike/` (rapport détaillé :
`01-baseline/RAPPORT-PHASE0.md`). Résultats mesurés :

| Mesure | 298 | 300 JOSEPH | 300 KATIA |
| --- | --- | --- | --- |
| STEP8 reproductible | oui (25 calques) | oui (37) | oui (59) |
| Ancre modifiée | 0 | 0 | 0 |
| Bande basse persistante | aucune | 108 px | 108 px |
| Scale A2 calculé (marge 5 %) | 180,0 % (no-op) | **207,4 %** | **207,4 %** |
| Scale humain | 180 % | 207 % (35), 254,7 % (1) | 207 % (59), 287,6 % (1) |
| Cadrage statique à ≤ 50 px du brut centré | 13/23 | 7/36 | 24/60 |
| Clés humaines à ≤ 60 px d'un objet suivi | 9/16 | 11/25 | 8/31 |

Trois conclusions se dégagent :

1. **A2 est validé** : la bande de 108 px donne 207,4 % avec une marge de
   5 %, soit 0,4 % d'écart avec le monteur ; 298 est un no-op correct.
2. **A3 est partiel** : 39 % des clés humaines seulement sont à moins de
   60 px d'un objet suivi. La règle « amplitude > 100 px + suivi continu »
   a une précision de 0,62 sur 298 mais de 0,29 et 0,20 sur 300.
3. **Le re-cadrage statique est l'angle mort** : sur 300, 29/36 et 35/60
   segments sont à plus de 100 px du cadrage brut centré, sans keyframe. Le
   monteur y suit un sujet (dos, action, archive) que le suivi de visage ne
   voit pas. A ne peut pas le retrouver avec `_ae.json` seul.

L'application de A2 sur une copie de 300 JOSEPH est **déterministe** : deux
exécutions indépendantes donnent le même SHA-256, l'ancre et la position sont
inchangées, la bannière disparaît au rendu. Le prototype A3 sur 298 obtient
une précision de 0,50 et un rappel de 0,60, avec un RMSE médian de 239 px :
il retrouve 3 segments sur 5, rate les suivis troués et ajoute des clés sur
3 segments statiques. L'écart est un écart de **composition** (le monteur
centre le visage et laisse du hors-champ), pas une erreur de calcul.

A4 est **validé sur le mécanique** : la fusion des segments de moins de 0,5 s
détecte la fusion humaine de 298 (1/1, aucun faux positif) et le cut dur
manqué par STEP2 est retrouvé à 53,68 s (1/1). Les deux splits de KATIA ne
sont pas des cuts visuels (corrélation 0,99) : ce sont des segmentations
sémantiques, laissées en `review`.

La couverture de 70–85 % reste plausible pour les **gestes mécaniques**
(letterbox, bande, scale : 92 segments à 207 %), mais elle est trop optimiste
pour le **cadrage éditorial**. C'est un argument pour A + B, pas pour A seul.

### 3.5 Écarts de contrat et de code

| Écart | Preuve | Conséquence |
| --- | --- | --- |
| Pas d'axe Y dans `_ae.json` | `step6.rs` ne produit que `target_anchor_x` | impossible de calculer le tiers supérieur des yeux |
| Axe Y jeté par STEP5 | `step5/video.rs`, `FastObject` ne garde que `centroid_x` | le JSON brut STEP4 contient `centroid_y`, mais il n'est plus disponible après STEP5 |
| `spread_x` insuffisant | 298 : 5/5 keyframés au-dessus de 354 px ; JOSEPH : 3/10 ; KATIA : 4/7 | un seuil sur `spread_x` rate de nombreux keyframes humains |
| Aucune keyframe dans le panel | `MediaSolution.jsx` applique une ancre et une position statiques | le geste « 2 à 6 points » n'est pas automatisé |
| Images importées, jamais placées | `importImages()` ligne 877, aucun `layers.add` | les logos présents dans `docs/` ne sont pas dans la comp |
| `.mov` ignorés | `videoSkipExtensions: ["mov"]` ligne 44 | le jingle alpha `m6+.mov` n'est même pas importé |
| Aucun rendu dans STEP8 | `step8.rs` s'arrête au `.ecproj` | l'export H.264 reste manuel |
| Aucun QC | pas d'artefact d'audit | la conformité n'est pas traçable |

### 3.6 Corpus élargi (2026-10-27)

Le corpus a été porté à **19 paires / 32 vidéos exploitables** (179, 181,
187, 191, 208, 211, 221, 223, 227, 266, 268, 270, 276, 286, 287, 295, 298,
300, 303). Deux paires restent incomplètes : 179 n'a pas de `.ecproj` GOOD
(seulement `.aep`/`.prproj`), et 287 n'a qu'un `.ecproj` GOOD sur trois
vidéos. La 2CV de 211 est exclue. Le GOOD de **295** a été fourni le
2026-10-27 : il partage le même `_ae.json` et le même `_tracking.json` que le
brut (SHA-256 identiques), ce qui en fait un contrôle propre pour le template
SDM (§3.8).

| Constat mesuré | Valeur |
| --- | ---: |
| Segments bruts analysés | 1059 |
| Segments keyframés | 145 |
| Clés humaines | 520 |
| Clés à ≤ 60 px d'un objet suivi | 278 (53,5 %) |
| Segments statiques à ≤ 50 px du brut | 458 / 907 (50,5 %) |
| Segments statiques à > 100 px du brut | 363 / 907 (40,0 %) |
| Règle A3 (amplitude > 100 px, trous < 30 %) | précision 0,20, rappel 0,23 |
| Faux positifs A3 sur les vidéos « sans piège » | 35 pour 1 vrai positif |

**Réserve de mesure — cause identifiée (2026-10-27)** : le delta vient de
**STEP4/5 (tracking)**, pas de STEP6/8. Les `_GOOD` 187/181 datent du
2026-10-15/16 et utilisent le tracking legacy ; les bruts du 2026-10-27
utilisent le tracking Rust. Preuve : le STEP6 Rust alimenté par le
`_tracking.json` legacy reproduit le `_GOOD/_ae.json` **à l'identique**
(4355/4355 frames sur 187, 2979/2979 sur 181). Le tracking Rust détecte moins
de personnes (164 vs 511 sur 187) et change les ids. La bascule est datée :
le commit legacy `0e9765b` du **2026-10-16 19:01** a introduit
`STEP4_TPU_FAST=1` et `object_min_missing=25`, après les GOOD. Le Rust
reproduit donc le mode *fast* actuel, pas le mode pré-fast des GOOD. Un run
Rust `--object-min-missing 1` remonte à 1722 frames personne contre 1623
legacy (au lieu de 276), mais ne suffit pas à retrouver les ancres : le delta
médian vs GOOD reste 64,33 px, contre 8,39 px avec le `_tracking.json`
legacy. Le gate explique la couverture personne, pas le delta d'ancre ; le
résidu vient de la détection de visage et de la sélection d'objet.

**Portée de la recette legacy** : cette reconstruction est un **outil de
diagnostic historique**, pas la méthode du corpus. La référence de production
est désormais **STEP1–8 Rust** : les bruts, le tracking et les ancres sortent
du même pipeline, puis `MediaSolution.jsx` arbitre le recentrage. Les GOOD
181/187 sont des références historiques (tracking legacy, avant le fast et
avant `segment_recommendations`) ; ils ne doivent pas être mélangés aux bruts
Rust dans une base de travail. Le GOOD 295 est le seul contrôle propre, car
brut et GOOD partagent exactement les mêmes entrées. La parité STEP4/5
legacy/Rust est un chantier externe, hors-scope STEP9 : le tracking Rust est
la référence de production. La recette legacy ne sert qu'à expliquer le delta
passé, pas à générer de nouveaux corpus. Le clamp du panel est correct pour
une position centrée ; les cas hors plage sont des déplacements humains de
`Position`, à traiter en `review`. Détail :
`/tmp/opencode/step9-spike/06-corpus/DIAGNOSTIC-DERIVE.md`.

**Nuance sur les « sans piège »** : les 5 paires 181, 270, 276, 286, 287 ne
sont pas des no-op. Elles totalisent 227 segments, 13 keyframés, 40 clés et
62 deltas d'ancre, et 132/220 segments statiques seulement sont à ≤ 50 px du
brut. Le monteur y fait donc encore des ajustements sporadiques de position,
des clés et parfois des cuts. Il n'existe pas, dans ce corpus, de contrôle
strict où A devrait ne rien faire ; les faux positifs se mesurent sur ces
paires à gestes faibles, pas sur une vérité terrain absolue.

Trois résultats changent le périmètre :

1. **A1 est validé sur le letterbox noir** : 191 a 152/128 px de barres
   (hauteur active 800 px) → 240 %, le monteur a utilisé 255 % (marge 4 %
   → 249,6 %, écart 2,1 % ; marge 6,25 % → 255 %) ; 268 alterne
   180 % et 222,2 % par segment, avec des écarts de 0,4 à 17 % selon la
   portion. En revanche, les **bords non noirs** (flous, colorés) de 268 ne
   sont pas vus par `cropdetect` : 8 segments sont scalés à 204–224 % par le
   monteur sans barre noire détectée.
2. **A2 était fragile avant A5** : la bannière de 108 px donne bien 207,4 %
   sur 187, mais le détecteur initial produit un faux positif à 340 px sur
   187 ECOLE (zone statique de décor) et sous-détecte la boîte de texte à
   44 px. Le détecteur A5 v2 corrige ce cas (§3.7), mais les logos de coin
   (191) et le logo de template (295, `sdm.png`) doivent aller en `review`
   ou relever de A6, pas en scale global.
3. **A3 est invalidé en auto** : la règle de sélection a une précision de
   0,20 et un rappel de 0,23 sur 1059 segments ; sur les vidéos sans piège,
   elle ajoute 35 segments keyframés là où le monteur n'en a mis qu'un. Le
   mouvement du centroïde ne prédit pas la décision humaine. A3 doit devenir
   un **générateur de candidats en `review`**, ou être activé par un flag
   explicite par projet.

Ces métriques A3 sont mesurées sur les bruts **Rust**. La recette legacy
change les candidats et les rendrait peut-être un peu meilleurs, mais elles
sont à re-mesurer sur les bruts reconstruits. La décision D-030 (review-only)
ne dépend pas de ce chiffre : elle est confortée par l'absence de contrôle
strict et par les faux positifs sur les paires à gestes faibles.

Le vrai delta restant est le **re-cadrage statique** : 40 % des segments
statiques sont à plus de 100 px du cadrage brut, et même les paires de
référence 298/300 en comptent 49/96. La recette legacy montre que ce chiffre
inclut une part de tracking : sur 181 Concierge, il passe de 25/58 à 12/58 ;
sur 187 DISNEYWORLD, il reste à 16/28. Le **40 % est donc une borne haute**,
pas une mesure purement humaine ; le reframing humain reste majoritaire sur
187 et sur les références. Le monteur centre le sujet qu'il juge important
(personne floutée qui parle, personne de dos, groupe), pas l'ancre STEP6.
C'est sémantique, et A3 ne le couvre pas.

**Cas du sujet non détectable** : `MediaSolution.jsx` recentre par priorité
visage > person > objet, ce qui est efficace quand le sujet principal est
détecté. Mais il arrive que la personne qui parle soit **floutée** (ou au
second plan) et que le sujet principal du plan soit ailleurs : le tracking la
voit, l'audio la rapproche, et le monteur choisit pourtant un autre sujet.
Aucun signal déterministe (centroïde, `is_speaking`, priorité d'objet) ne
permet de trancher. C'est un cas explicite de `review`/B, pas d'automatisation
A. Il doit être documenté comme une **limite de perception**, pas comme un
bug de la règle de recentrage.

Enfin, le cas **295 (SDM)** n'est ni A1 ni A2 ni A3 : c'est un template de
composition à trois panneaux (deux tailles + fond flou), constant sur la
vidéo. Il est déterministe et récurrent, et le GOOD fourni le 2026-10-27 a
permis de le mesurer et de le reproduire au pixel près avec un module dédié
(A6, §3.8).

**Conséquence** : A reste rentable comme **pré-nettoyage mécanique**
(letterbox noir, bannière pleine largeur, cuts durs), mais il ne remplace pas
le cadrage humain. La couverture de 70–85 % ne vaut que pour les gestes
mécaniques ; pour le cadrage, la moitié des segments est déjà correcte et
l'autre moitié est sémantique.

### 3.7 Prototype A5 (2026-10-27)

Le prototype `03-scenario-a/a5_report.py` produit `post.json`, `review.json`
et `audit.json` sans modifier le projet. Il a été exécuté sur 4 vidéos :
268 (letterbox), 187 DISNEYWORLD, 187 ECOLE (bannière) et 191 cancer
(letterbox + logo de coin).

| Cas | A1 apply | Δ scale A1 | Comp. Y | A2 apply | Δ scale A2 | A3 préc. | A3 rappel |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 268 | 15 | 6,7 % médian | −287,9 px (comp) | 0 | — | 0,29 | 0,25 |
| 187 DISNEYWORLD | 0 | — | — | 1 | 0,4 % | 0,33 | 0,20 |
| 187 ECOLE | 0 | — | — | 1 | 0,4 % | 0,50 | 0,25 |
| 191 cancer | 37 | 5,4 % médian | −367,8 px enfant | 0 (logo en review) | — | 0,60 | 0,43 |
| 181 Concierge (« sans piège ») | 0 | — | — | 0 | — | 8 candidats / 0 clé humaine | — |

Quatre enseignements :

1. **A2 est solide sur la bannière claire** : 206,6 % vs 207 % sur 187, et le
   décor statique de 187 ECOLE n'est plus confondu avec la bannière. Le logo
   de coin de 191 part en `review`, pas en zoom global.
2. **A1 est dans la plage humaine mais pas à 2 % près** : sur 268, la cible
   mécanique de 231,1 % est dans la plage 221,3–239,7 % du monteur ; sur 191,
   249,6 % vs 255 % (2,1 %). La variation restante est créative. Le garde-fou
   de géométrie asymétrique évite un faux positif à 201,3 % sur 187
   (barre en haut seulement).
3. **A3 reste un générateur de revue** : précision 0,29–0,60, rappel
   0,20–0,43. Il ne doit pas écrire de keyframes.
4. **Le scale A1 ne suffit pas si l'ancre est décentrée** : sur 191,
   249,6 % supprime la barre du bas mais laisse 19,5 % de barre noire en
   haut, parce que l'ancre du sujet n'est pas au centre de la zone active.
   Avec une compensation de **Position Y de −367,8 px enfant (−417,8 px en
   comp)**, les barres disparaissent complètement ; le cadrage vertical est
   plus serré mais propre. Il faut donc appliquer scale + compensation
   (jamais l'Ancre) et signaler le cadrage vertical en `review`. Le rendu de
   contrôle reste nécessaire pour A1, comme le prévoyait le protocole. A2,
   lui, est visuellement propre sur 187 aux timecodes 10, 40 et 100 s.

**Durcissement v2 (2026-10-27)** : A5 utilise un 10e percentile des hauteurs
actives, vérifie la couverture mathématique de tous les crops et met en
`review` tout segment keyframé. A2 scanne toute la hauteur, détecte les
bandes sombres par paires de bords, et mesure la stabilité géométrique des
bords (centre/hauteur) plutôt que la luminosité : une bannière qui change de
texte reste applicable, une bannière qui glisse part en `review`. Validation
synthétique de 11 cas : 3 apply, 7 review, 1 noop conformes. Sur 181
Concierge (« sans piège »), A1/A2 ne produisent **aucune action
automatique** (10 barres asymétriques en review). A5 v2 coûte 3,7–33,5 s
par vidéo (0,07–0,24× la durée) et calcule un hash échantillonné de
3 × 4 Mio au lieu du fichier complet. Détail :
`06-corpus/a5v2/RAPPORT-A5-V2.md`.

La détection de cut dur intra-segment est désactivée par défaut : elle produit
32 faux positifs sur 187 DISNEYWORLD (mouvements rapides). A4 ne garde que les
segments courts et les bornes non alignées sur le CSV.

Rapport détaillé : `/tmp/opencode/step9-spike/06-corpus/a5/RAPPORT-A5.md`.

### 3.8 Template A6 validé sur 295 (2026-10-27)

Le GOOD 295 est un contrôle propre : le brut et le GOOD partagent le même
`_ae.json` et le même `_tracking.json` (SHA-256 identiques). La seule
différence est le `.ecproj`. Le brut STEP8 est un plan serré sur Liliane
(ancre 1390,39 px) ; le GOOD est le template SDM :

| Calque | Échelle | Position (x, y) | Rôle |
| --- | ---: | ---: | --- |
| Fond | 180 % | (540,095, 0) | source 9:16 floutée (Gaussien 50) |
| Principal | 84,3497774862575 % | (540,095, 171,032) | panneau 1080×911 |
| Secondaire | 56,294573830734535 % | (540,095, 1082,026) | panneau 16:9 1080×608 |
| Logo | 100 % | (540, 960) | `sdm.png` (83..274, 286..425) |

Ordre d'affichage : logo, secondaire, principal, fond. Le profil stocke les
valeurs mesurées, pas des valeurs arrondies : un profil « propre » à
84,375 %/56,25 % et y=171/1082 dévie de 106/255 (0,69 moyen) sur le rendu.
Les valeurs mesurées correspondent à des largeurs source visibles de
1280,4 px et 1918,5 px, soit l'intention 1280/1920 avec l'imprécision
manuelle du GOOD.

Le prototype `06-corpus/a6_template.py` transforme le `.ecproj` brut :
suppression du parent Null, ancre au centre source 960, position 540,095, ajout
du flou et des deux duplicats, puis logo. Coût : **0,03 s** et 12 Mo de RSS.
Rendu de contrôle `effectcraft-cli render-frame` à 0, 10, 30 et 50 s :
différence maximale **0/255**, moyenne 0, **0 pixel > 2**. Le rendu
H.264 complet (52,68 s, 1080×1920) prend **96,2 s** (1,83× la durée) ; son
SSIM vs le GOOD est 0,9717 (PSNR 33,3 dB), mais l'écart est dominé par
l'encodage (notre rendu est High 15 Mbps, le GOOD Main 10 Mbps). Le template
est donc reproductible au niveau du projet et du rendu.

Conditions : un seul segment, aucune keyframe, aucun parent animé. Un projet
multi-segment ou keyframé part en `review` : dupliquer les panneaux par
segment n'est pas encore validé. Le template est déclenché par une règle de
nommage **configurée** (`M6 - SDM - ` par défaut) ou un mapping projet ; le
module ne devine pas le template à partir du contenu. Le préfixe est vérifié,
puis les conditions structurelles ; si elles échouent, `review`. Le logo est
optionnel : s'il n'est pas fourni, les trois panneaux restent valides.

**Intégration A5/A6 (2026-10-27)** : `03-scenario-a/a6_apply.py` applique le
template sur une copie et écrit `post.json`/`review.json`/`audit.json` ;
`a5_report.py --template sdm` ajoute la décision A6 et le bloc `template` à
son rapport, sans écrire le projet. Résultats : 295 → apply, audit `pass`,
rendu relinké 0/255 ; 295 avec une keyframe → review, aucun projet ; 187 →
noop. Ce module est désormais branché dans le post-traitement de production
(voir §3.9), pas dans le panel.

### 3.9 Passage en production (2026-10-27)

Le prototype A5/A6 est porté dans `crates/workflow-steps/src/post/` et appelé
après un STEP8 réussi. Le contrat de production est volontairement plus
strict que le prototype :

- le `.ecproj` brut reste **octet pour octet inchangé** ;
- un second projet `<vidéo>_9x16_assist.ecproj` est **toujours** publié, même
  sans décision applicable (copie exacte du brut) ;
- `post.json`, `review.json` et `audit.json` sont écrits sous
  `<docs>/post/<projet_assisté>/` et restent additifs ;
- A1/A2 ne déplacent jamais l'Anchor Point ; A3/A4 restent en revue ;
- A6 éligible remplace le projet et court-circuite A1–A4 ; un préfixe SDM
  incohérent part en revue sans bloquer A1–A4 ;
- un conflit A1/A2 part en revue, aucune des deux décisions n'est appliquée.

Le port Rust a été comparé au prototype Python : sur 268, 187 DISNEYWORLD,
187 ECOLE, 191 et 181, les listes de décisions A1/A2/A3/A4 sont **identiques**
(code, action, cible, raison, détails). Le projet assisté de 295 est identique
au JSON de référence et le rendu relinké reste à **0/255** à 0, 10, 30 et
50 s. Les 11 cas A2 synthétiques donnent 3 `apply`, 7 `review`, 1 `noop`,
conformes au prototype. Les projets 270/276/286/287 ne changent aucune échelle
et leur assist est identique au brut.

Détail d'exploitation, configuration et limites : `docs/LOT-9-POST-STEP8.md`.
Décision : **D-032**.

---

## 4. Évaluation technique d'une « STEP9 agentique »

### 4.1 Boucle de perception et d'action

```text
                 ┌──────────────────────────────────────────────┐
                 │  Projet .ecproj (copie de travail)            │
                 └───────────────┬──────────────────────────────┘
                                 │
                    render_frame(time, max_side)
                                 ▼
                 ┌──────────────────────────────────────────────┐
                 │  VLM : image + contexte + schéma JSON         │
                 │  sortie = intention sémantique + confiance    │
                 └───────────────┬──────────────────────────────┘
                                 │  JSON validé (pas de floats libres)
                                 ▼
                 ┌──────────────────────────────────────────────┐
                 │  Exécuteur déterministe                       │
                 │  set_property / add_keyframe / batch / split  │
                 │  clamp, quantize, bornes, continuité          │
                 └───────────────┬──────────────────────────────┘
                                 │
                    render_frame de vérification
                                 ▼
                 ┌──────────────────────────────────────────────┐
                 │  Comparaison baseline / seuils / undo         │
                 └──────────────────────────────────────────────┘
```

La séparation est volontaire : **le VLM ne produit pas de coordonnées
flottantes**. Il produit une intention (`keep`, `shift_left`, `zoom_to_subject`,
`split_here`, `place_logo`, `flag_for_review`) avec une cible, une plage
temporelle, une confiance et des frames de preuve. L'exécuteur transforme
l'intention en géométrie avec les mêmes helpers que le panel, puis vérifie par
rendu.

### 4.2 Ce qu'EffectCraft 0.17 sait réellement faire

Mesures et inspection du binaire `/usr/bin/effectcraft-cli` :

| Besoin | Capacité | Statut |
| --- | --- | --- |
| Lire le projet | `info`, `props`, `get`, MCP `get_project`/`get_comp`/`get_layer` | disponible |
| Voir une frame | `render-frame`, MCP `render_frame` (PNG inline) | disponible, ~0,18 s/frame en 1080×1920 |
| Écrire une transform | `set`, MCP `set_property` | disponible |
| Keyframes | MCP `add_keyframe` (lot de clés, interpolation), CLI `prop.addKey` | disponible |
| Split de calque | `edit.splitLayer` | disponible |
| Crop/scale | `set` sur `transform/scale`, `comp.cropToRegionOfInterest` | disponible |
| Ajouter un calque | `layer.addItem`, `run_script` | disponible |
| Effets | `add_effect` / `effect.apply` | 307 effets |
| Batch atomique | MCP `batch` | disponible |
| Undo/history | MCP `undo`, `history` | disponible |
| Rendu | `render` H.264, bitrate, progress | disponible |
| Confinement | `mcp --automation-read-root`, `--automation-write-root` | disponible |
| Surface MCP | 27 outils, JSON-RPC stdio | mesuré |

**Mesures de rendu** (GTX 1650 4 Go, 24 cœurs, projet 298) :

| Opération | Résultat |
| --- | --- |
| `render-frame` 1080×1920 | 0,15–0,19 s / frame |
| `render` 5 s à 10 Mbit/s | 6,07 s, 10,36 Mbit/s |
| `render` 65,8 s à 10 Mbit/s | 72,3 s, 10,29 Mbit/s, 4,9 Go RSS, ~16 cœurs |
| Export final GOOD 298 | 68,8 s, 10,01 Mbit/s |
| Export final GOOD JOSEPH | 147,9 s, 10,27 Mbit/s |

Le rendu n'est **pas** le goulot d'étranglement : il est proche du temps réel
et peut être parallélisé par projet. Le goulot est la décision et la
vérification.

### 4.3 Latence et économie de tokens

Hypothèses de calcul (à remplacer par des mesures dans le spike) :

- vidéo de 2 min, 40 segments ;
- 3 frames par segment = 120 frames ;
- 2 passes (audit + vérification) = 240 images ;
- 300 à 1 500 tokens par image selon le modèle et la résolution ;
- 200 à 500 tokens de sortie par segment, soit 8 000 à 20 000 tokens par passe.

| Scénario | Images | Tokens d'entrée | Tokens de sortie | Coût cloud indicatif* |
| --- | ---: | ---: | ---: | ---: |
| Basse résolution, petit modèle | 240 | 72 k | 20 k | 0,01–0,03 $ |
| Résolution moyenne | 240 | 192 k | 20 k | 0,10–0,15 $ |
| Haute résolution, grand modèle | 240 | 360 k | 20 k | 1,0–1,5 $ |
| 5 itérations de co-monteur | 600 | 180–900 k | 50–100 k | 0,05–4 $ |

\* Ordres de grandeur à 0,10–3 $/M tokens d'entrée et 0,40–15 $/M tokens de
sortie. Les tarifs changent ; le spike doit les relever au moment du test.

**Lecture économique :**

- à l'échelle d'une vidéo, le coût cloud est **faible** (centimes à quelques
  euros) ; le vrai coût est l'ingénierie, la validation et le risque ;
- à 1 000 vidéos/an, même 1 $/vidéo reste marginal face au coût humain ;
- le coût explose si l'agent boucle, si les frames sont envoyées en pleine
  résolution, ou si chaque décision déclenche un rendu complet ;
- le coût local est dominé par le temps GPU/CPU, pas par l'électricité.

**Latence :**

| Mode | Latence par appel | 120 frames séquentielles | 120 frames par lots de 5 |
| --- | ---: | ---: | ---: |
| Cloud API | 1–3 s | 2–6 min | 30 s–2 min |
| VLM local 3B Q4 (GTX 1650 4 Go) | 1–3 s/frame | 2–6 min | 2–6 min |
| VLM local 7B Q4 | ne tient pas en 4 Go | — | — |
| VLM local CPU | 5–15 s/frame | 10–30 min | 10–30 min |

Le VLM local 3B est **juste faisable** sur la carte actuelle, mais avec une
qualité de jugement limitée et une VRAM très serrée. Le 7B Q4 (~4,5 Go) ne
tient pas. Un modèle cloud est plus capable, mais casse la promesse offline.

### 4.4 Fiabilité de l'actionnement

Risques observés ou inférés :

| Risque | Exemple | Mitigation |
| --- | --- | --- |
| Coordonnées hallucinées | le VLM propose `x=612,4` alors que la cible est à 583,65 | intention + exécuteur ; clamp ; quantification ; rendu de contrôle |
| Ambiguïté de repère | `Position` vaut `[540,0]` dans le panel mais `[50,50]` dans le `.ecproj` parenté | utiliser les helpers du panel, jamais lire/écrire des pixels bruts sans validation |
| Confusion de calque | deux calques `Segment 21` après un split | cibler par **id**, pas par nom |
| Base de temps | `add_keyframe` en temps calque vs temps comp | `timeBase` explicite ; tests de non-régression |
| Sur-correction | zoom excessif qui coupe un visage | bornes de delta, comparaison baseline, max 1–2 itérations |
| Discontinuité temporelle | clés à 1 frame d'intervalle, à-coups | lissage One-Euro, simplification RDP, 2–6 clés, pas de clé sur une coupe |
| Écrasement silencieux | sauvegarde sur le projet d'origine | copie, `save_project` vers un nouveau chemin, checksum, undo |
| Rendu non conforme | débit 8 Mbit/s au lieu de 10 | `ffprobe` obligatoire, contrôle du nom/emplacement |

**Règle d'or** : l'agent peut proposer, l'exécuteur dispose. Toute écriture
passe par un exécuteur qui valide le schéma, les bornes, la continuité et
l'identité du calque. Le VLM ne reçoit jamais un accès direct en écriture au
projet de production.

### 4.5 Localité vs souveraineté

| Critère | Local | Cloud |
| --- | --- | --- |
| Confidentialité | frames restent sur la machine | frames envoyées à un tiers |
| Offline | oui | non |
| Coût marginal | électricité + temps GPU | tokens + réseau |
| Qualité de jugement | limitée par 4 Go VRAM | généralement supérieure |
| Latence | dépend du GPU local | dépend du réseau |
| Mise à jour modèle | manuelle, versionnée | subie, peut changer |
| Panne | locale, maîtrisée | réseau/API |
| Conformité client | plus simple | DPA, région, rétention, audit |

La promesse actuelle du pipeline est **100 % local, offline, Edge TPU/CUDA
dédié**. Introduire un cloud sans isolation claire est une rupture de contrat,
pas un simple choix technique. La recommandation est :

- **local par défaut** pour l'audit et les checks déterministes ;
- **cloud opt-in par projet**, avec accord explicite, DPA et politique de
  rétention ;
- **aucune dépendance réseau bloquante** dans les 8 étapes ;
- **fallback déterministe** si le modèle est indisponible.

### 4.6 Non-déterminisme

Un LLM à température 0 reste non déterministe en pratique : version du modèle,
quantification, kernels GPU, redimensionnement d'image, ordre des tokens.
Pour un projet client, cela signifie que deux exécutions peuvent produire deux
cadrages différents.

| Scénario | Déterminisme attendu | Conséquence |
| --- | --- | --- |
| A déterministe | fort (hash de projet stable) | testable, rejouable, livrable |
| B audit | moyen (rapport variable) | acceptable si advisory |
| C co-monteur | faible (projet variable) | approbation humaine obligatoire |

Mitigations : température 0, seed, prompt versionné, schéma JSON, cache par
hash de contenu, modèle épinglé, double exécution de contrôle, journal
d'audit, approbation humaine. Cela **réduit** la variance, ne l'annule pas.

---

## 5. Matrice de confrontation

### 5.1 Bénéfices réels

| Bénéfice | Preuve / attente | Condition |
| --- | --- | --- |
| Autonomie accrue | 70–85 % des gestes observés couverts par A | keyframes et overlays déterministes fiables |
| Homogénéité | mêmes règles de scale, overlay, rendu sur tous les projets | configuration unique, tests |
| Zéro oubli | checklist QA systématique | B avec rappel ≥ 90 % |
| Traçabilité | audit JSON, frames de preuve, hash | journalisation |
| Temps monteur | déplacé vers les arbitrages, pas la géométrie | A + B |
| Passage à l'échelle | profilage/édition ~1–3 min par vidéo ; rendu humain ~72 s pour 66 s | file d'attente GPU/CPU |

### 5.2 Limites dures

| Limite | Pourquoi elle est dure | Conséquence |
| --- | --- | --- |
| Subjectivité du monteur | humour, rythme, émotion, réaction ne se mesurent pas en pixels | l'agent ne peut pas décider seul |
| Non-déterminisme | modèles, versions, sampling | pas de garantie de reproductibilité |
| Ancrage visuel | le VLM peut mal localiser un logo mobile ou un sous-titre | faux positifs/négatifs |
| Longue traîne | formats, langues, logos, sous-titres variés | seuils fragiles |
| Responsabilité | un export client engage l'agence | validation humaine obligatoire |
| « Zéro » absolu | aucun détecteur ne garantit 100 % | définir un seuil mesurable, pas une promesse |

### 5.3 Risques industriels

| Risque | Probabilité | Impact | Mitigation |
| --- | --- | --- | --- |
| Régression silencieuse | moyenne | élevé | canari, QC, échantillon humain, diff |
| Temps d'exécution explosif | moyenne | moyen | budget par vidéo, sampling, cache, parallélisme |
| Dérive des coûts API | moyenne | moyen | budget, tarification, fallback local |
| Fuite de données | faible/moyenne | élevé | local-first, DPA, opt-in, chiffrement |
| Verrouillage fournisseur | moyenne | moyen | interface VLM abstraite, fallback |
| Sur-correction | élevée en C | élevé | bornes, itérations max, undo, humain |
| Absence d'évaluation | élevée | élevé | corpus labellisé, test aveugle |
| Maintenance des prompts | moyenne | moyen | prompts versionnés, golden set |
| Conformité contractuelle | moyenne | élevé | mention IA, validation humaine |

**Le risque le plus sous-estimé** n'est pas une frame laide : c'est une frame
esthétiquement correcte qui a laissé passer un logo ou un sous-titre. C'est
précisément le rôle du scénario B.

---

## 6. Trois scénarios d'architecture

### 6.1 Scénario A — Consolidation heuristique déterministe

**Principe** : un étage de **profilage** mesure les signaux réellement présents
dans la vidéo, puis seuls les modules dont le signal est détecté sont
appliqués. Le résultat est un `.ecproj` enrichi et un rapport, **sans rendu,
sans logo/jingle**. Le rendu reste humain.

```text
_ae.json + _tracking.json + CSV + source vidéo + .ecproj
        │
        ├─ A0 profil : ffprobe, cropdetect, médiane/OCR, tracking, frame-diff
        │              → video_profile.json (signaux + confiance)
        ├─ A1 letterbox : si bandes noires persistantes → scale/offset
        ├─ A2 overlays : si bande/texte persistant → exclusion ou review
        ├─ A3 keyframes : si mouvement fort → 2–6 clés
        ├─ A4 transitions : si perte/rupture → split/merge ou review
        └─ A5 rapport : actions appliquées, review.json, audit déterministe
```

**A0 — Profilage (le point clé)** : avant toute modification, produire
`video_profile.json` à partir de :

- `ffprobe` : résolution, fps, durée, codec, audio ;
- `ffmpeg cropdetect` sur des fenêtres échantillonnées (début, milieu, fin,
  ou 1 frame/s) : zone active, bandes noires, stabilité ;
- médiane temporelle / gradient / OCR sur la bande basse et les coins :
  overlays persistants, texte, logos source ;
- `_ae.json` / `_tracking.json` : présence, `centroid_x`, confiance,
  `is_speaking`, `tier`, trous de suivi, changements d'identité ;
- CSV STEP2 + différence de frames / histogrammes autour des frontières :
  fondus, mouvements brusques, cuts manqués ou faux cuts ;
- audio : silence, changement de locuteur, musique.

Chaque signal porte un **score de confiance**. Si le score est sous le seuil,
A **ne fait rien** et écrit `review`. La plupart des vidéos n'ayant aucun
signal, le profil est vide et A est un no-op : c'est le comportement voulu.

**Comment A détecte un piège** — A n'infère pas la présence d'un piège à
partir du nom du projet ou d'un dossier d'analyses : il la **mesure** sur la
vidéo et sur les artefacts STEP. `docs/` n'est pas un dépôt d'analyses, c'est
le dossier d'entrées du projet (`_tracking.json`, CSV, `_ae.json`,
`_audio.json`, vidéo source) et d'assets (`.png`, `.mov`). Rien n'y décrit
« cette vidéo a une bannière » ; c'est justement ce que A0 doit produire.

| Piège | Signal mesuré | Où le lire | Seuil d'activation | Si sous le seuil |
| --- | --- | --- | --- | --- |
| Letterbox | zone active stable plus petite que la frame | `cropdetect` sur fenêtres échantillonnées | même crop sur ≥ 90 % des fenêtres | rien ; `review` si instable |
| Bande / overlay fixe | persistance d'une zone basse ou d'un coin | médiane temporelle, gradient, OCR | présent sur > 80 % des frames | `review` |
| Sous-titre | texte OCR récurrent à boîte stable | OCR de la bande basse | ≥ 2 occurrences, confiance OCR suffisante | `review`, jamais auto |
| Mouvement | amplitude et continuité du centroïde | `_tracking.json`, `_ae.json` | amplitude > seuil (ex. 15 % de la largeur) et suivi continu | statique, comportement actuel |
| Transition | perte/réacquisition, saut de position, cut absent du CSV | tracking + CSV + frame-diff | saut > seuil ou cut absent | `review` |
| Groupe | plusieurs visages, dispersion latérale | `_ae.json` (`spread_x`, `needs_review`) | `spread_x` > seuil ou `needs_review` | `review` : A ne choisit pas le sujet |

Deux conséquences :

- **le silence est la sortie par défaut** : une vidéo propre produit un profil
  vide, zéro modification, et un projet inchangé (hash identique, ou diff
  sémantique nul si l'outil resérialise le fichier). C'est le test le plus
  important du spike ;
- **un piège non mesurable n'est pas deviné** : logo mobile, sous-titre
  créatif, choix du sujet dans un groupe partent en `review`. A ne comble pas
  ces cas, il les signale.

**Détail des briques :**

- **A1 — Letterbox** : si `cropdetect` trouve une zone active stable
  inférieure à la frame, calculer
  `scale = max(1080/largeur_active, 1920/hauteur_active) × 100` ; position
  centrée ; vérifier par `render_frame` sur 3 frames. Sur le corpus,
  `cropdetect` donne 1920×1072 (8 px), donc aucun changement visible ; la
  brique reste à valider sur un vrai 2.39:1.
- **A2 — Overlays et sous-titres source** : si une bande basse ou un texte
  est persistant (> 80 % des frames échantillonnées), calculer le scale
  d'exclusion (calage haut, ancre Y=0) ; pour les sous-titres, OCR + boîte
  temporelle ; **ne pas auto-zoomer** si le zoom requis dépasse un seuil ou
  si le visage sort du cadre ; produire un candidat `review`. La bannière 300
  est un cas typique : 207 % la sort. A2 ne place **jamais** le logo de
  l'émission ; il nettoie les overlays de la source.
- **A3 — Keyframes** : si la trajectoire de l'objet recommandé est stable et
  que le mouvement dépasse un seuil, partir de `centroid_x` (et `centroid_y`
  si un artefact `_motion.json` est ajouté) ; lisser (One-Euro) ; simplifier
  (RDP) ; viser 2–6 clés ; conserver l'ancre STEP6 ; interpoler en bezier ;
  ne jamais poser de clé sur une coupe ; clamper la position à la couverture.
  Si le suivi est troué ou l'identité instable : pas de clé, `review`.
- **A4 — Transitions** : détecter les pertes/réacquisitions de suivi, les
  sauts de position, les incohérences entre CSV et mouvement, les fondus
  (frame-diff/histogramme) ; proposer `split` ou `merge` ; auto-appliquer
  seulement au-dessus d'un seuil de confiance, sinon `review`.
- **A5 — Rapport et QC déterministe** : écrire `actions.json`,
  `review.json` et `audit.json` ; vérifier sur le `.ecproj` la durée, le
  nombre de segments, les ancres, les clés, les échelles et les overlays
  détectés ; rendre quelques frames de contrôle si nécessaire. **Pas de
  rendu final.**

**Hors périmètre A, par décision** :

- **relink** : contrôle optionnel `--check-media` (les chemins sont stables
  sur le même poste) ;
- **logos/jingle** : jamais ajoutés automatiquement ; les PNG restent
  importés, les `.mov` restent ignorés, les directives client sont humaines.
  Seule exception : le logo fixe d'un template A6 explicitement configuré
  (`sdm.png`), fourni par le profil, jamais détecté ni placé automatiquement ;
- **rendu H.264** : l'export EffectCraft CPU n'est pas optimal et le GPU
  H.264 n'est pas disponible ; l'export final reste humain, avec l'outil GPU
  externe.

**Effort indicatif** : 8–12 jours pour un prototype A0–A5 sur 298/300,
15–25 jours pour une version durcie (tests, configuration, erreurs,
documentation). Le chiffrage suppose un ingénieur Rust/JSX familier du panel.

**Bénéfices** : déterminisme, coût marginal nul, offline, testable, pas de
nouvelle dépendance. **Limites** : ne gère pas les logos mobiles, les
sous-titres créatifs, les arbitrages de récit, le rendu ; les seuils doivent
être configurables et revus ; un faux positif sur une vidéo propre est le
risque principal.

**Couverture attendue** : 70–85 % des gestes de **cadrage/nettoyage**
observés, pas 70–85 % du temps total post-STEP8. Les 15–30 % restants sont
les 2 zooms spéciaux, les 3 splits/1 fusion ambigus, les logos/jingle et la
validation finale.

### 6.2 Scénario B — Agent QA / inspecteur de conformité

**Principe** : A produit le projet ; l'export préliminaire est produit par
l'outil GPU humain habituel, puis B inspecte des frames clés et produit un
rapport, sans jamais modifier le projet.

```text
A → .ecproj + audit_deterministe.json
        │
        └─ export humain (GPU externe) → MP4
                         │
                         ▼
              échantillonnage de frames
              (début/fin de segment, milieu,
               pics de mouvement, coupes)
                         │
                         ▼
        checks déterministes + VLM (local-first)
                         │
                         ▼
              audit.json + checklist
                         │
          ┌──────────────┴──────────────┐
          ▼                             ▼
   auto-approve si 100 %        review humain sinon
```

**Checks déterministes** : specs ffprobe, frames noires/figées, audio,
template des logos connus, OCR des bandes de sous-titres, détection de visage
hors cadre (modèle léger ou STEP4), continuité des cuts.

**Checks VLM** : visage coupé, logo résiduel, sous-titre visible, cadrage
étrange, plan de réaction non couvert. Sortie structurée :
`status` (`pass`/`review`/`fail`), `score`, `findings[]` avec frame, type,
sévérité, confiance, preuve.

**Politique de décision** :

- `pass` automatique seulement si tous les checks déterministes passent, si le
  VLM ne signale rien au-dessus du seuil, et si le projet est un cas simple
  (pas de zoom spécial, pas de split) ;
- sinon `review` avec checklist priorisée ;
- jamais de correction automatique.

**Effort indicatif** : 5–10 jours pour un PoC sur 298/300, 15–25 jours pour
une version intégrée avec corpus labellisé.

**Bénéfices** : risque faible (lecture seule), coût borné, traçabilité,
réduction des oublis. **Limites** : faux positifs/négatifs, couverture
d'échantillonnage, ne corrige pas.

**C'est le meilleur premier usage agentique** : il apporte de la valeur sans
mettre en danger la livraison.

### 6.3 Scénario C — Agent co-monteur en boucle fermée

**Principe** : un agent orchestrateur pilote EffectCraft via MCP, lit la
timeline, rend des frames, décide, manipule les keyframes et revérifie.

```text
open_project(copie)
   │
   ├─ get_comp / get_layer / get_property
   ├─ render_frame(time)
   ├─ VLM → intention sémantique + confiance
   ├─ exécuteur → set_property / add_keyframe / batch / split
   ├─ render_frame(vérification)
   ├─ comparaison baseline → accept / undo
   └─ max N itérations → save → revue humaine
```

**Architecture MCP concrète** :

- processus `step9-agent` **hors serveur et hors catalogue**, lancé
  manuellement sur une copie ;
- client MCP stdio vers `effectcraft-cli mcp --automation-read-root <copie>
  --automation-write-root <copie>` ;
- passerelle d'outils avec allowlist : `get_*`, `render_frame`,
  `set_property`, `add_keyframe`, `batch`, `undo`, `save_project` ;
  `run_script` interdit en production, `open_project` limité à la copie ;
- VLM local via llama.cpp/Ollama en HTTP local, ou cloud opt-in ; le MCP
  EffectCraft reste local dans les deux cas ;
- machine à états : `observe → propose → validate → apply → verify →
  commit|rollback` ; une seule écriture atomique par cycle ;
- journal JSONL : projet, hash, modèle, version, prompt hash, frame, action,
  confiance, résultat, durée.

**Périmètre autorisé** : choix du sujet, zoom spécial, split/merge,
keyframes correctives, priorisation des plans de réaction. **Hors périmètre** :
suppression de médias, changement de durée global, placement logo/jingle et rendu final (hors périmètre).

**Garde-fous indispensables** :

- copie de travail, `--automation-write-root` limité à cette copie ;
- lecture seule sur la production ;
- max 3 itérations par segment, max 10 par vidéo ;
- deltas bornés (position ±200 px, scale ±20 %, sauf zoom explicite) ;
- pas de clé sur une coupe, pas de clé à moins de 2 frames d'une autre ;
- `batch` atomique + `undo` en cas de régression ;
- approbation humaine avant `save` final ; le rendu reste l'outil GPU humain ;
- journal : modèle, version, prompt hash, frames, décisions, confiance.

**Effort indicatif** : 6–10 semaines pour un PoC crédible, avec une forte
incertitude sur la qualité. **Bénéfices** : traite les arbitrages sémantiques.
**Risques** : non-déterminisme, sur-correction, coût, lenteur, régression
silencieuse, maintenance des prompts.

**Position recommandée** : laboratoire uniquement, après B, sur 3–5 segments
choisis, avec revue humaine et mesure de préférence aveugle.

### 6.4 Comparaison synthétique

| Critère | A — déterministe | B — QA read-only | C — co-monteur |
| --- | --- | --- | --- |
| Déterminisme | fort | moyen | faible |
| Couverture du delta cadrage/nettoyage | 70–85 % | +5–10 % (auto-approve) | +5–15 % (sémantique) |
| Coût marginal | ~0 | 0,01–1 $/vidéo | 0,1–5 $/vidéo |
| Latence ajoutée | profilage + édition ~1–3 min (pas de rendu) | +1–6 min | +5–20 min |
| Risque qualité | faible | faible (lecture seule) | élevé |
| Souveraineté | locale | locale/cloud opt-in | locale/cloud |
| Complexité | moyenne | moyenne | élevée |
| TRL | 6–7 | 4–5 | 2–3 |
| Recommandation | **maintenant** | **PoC ensuite** | **labo seulement** |

---

## 7. Conclusions et feuille de route

### 7.1 Ce qu'il faut intégrer tout de suite, sans IA générative

1. **Profiler la vidéo** (A0) : `ffprobe`, `cropdetect`, médiane/OCR,
   tracking, frame-diff ; écrire `video_profile.json` avec les signaux et
   leur confiance.
2. **Détecter le letterbox** avec `cropdetect` et calculer le scale/offset.
3. **Détecter les bandes d'overlay persistantes** (bannière basse, texte) et
   calculer un scale global d'exclusion, avec vérification par frames.
4. **Générer des keyframes déterministes** à partir du tracking : lissage,
   simplification, 2–6 clés, ancre STEP6 conservée, position clampée.
5. **Détecter les splits/merges candidats** à partir des pertes de suivi et
   des discontinuités ; auto seulement au-dessus d'un seuil.
6. **Produire un audit déterministe** : specs, frames noires, audio, OCR,
   cohérence des segments ; `actions.json` + `review.json`.
7. **Vérifier les médias en option** (`--check-media`) : les chemins sont
   stables sur le même poste, donc ce n'est pas un module.

Ces briques ne nécessitent ni VLM ni réseau. Elles attaquent la majorité du
delta de cadrage/nettoyage, pas les logos/jingle/rendu qui restent humains.

### 7.2 Ce qui mérite un PoC agentique

1. **QA de conformité** (B) sur 298/300 : précision/rappel des logos,
   sous-titres, visages hors cadre ; local vs cloud ; coût/temps.
2. **Zooms spéciaux** (sous-titre, archive 4:3) : l'agent propose, l'humain
   valide ; mesurer si le gain de temps est réel.
3. **Co-monteur** (C) uniquement après B, sur des segments choisis, avec
   approbation humaine.

### 7.3 Ce qu'il ne faut pas faire

- remplacer l'ancre STEP6 par un VLM : le corpus montre qu'elle est déjà
  stable et pertinente ;
- laisser un LLM écrire des coordonnées flottantes sans exécuteur ;
- auto-livrer un export sans contrôle humain ;
- introduire une dépendance cloud bloquante dans les 8 étapes ;
- modifier `_ae.json`, `_audio.json` ou le CSV pour ajouter des champs sans
  nouvelle version de contrat ;
- ajouter un 9e step au serveur avant validation du spike.

### 7.4 Feuille de route proposée

| Phase | Durée indicative | Livrable | Porte de décision |
| --- | --- | --- | --- |
| 0. Cadrage et baseline | 2–3 jours | mesures 298/300 puis corpus 32 vidéos, labels, baseline A | corpus et métriques prêts |
| 1. A déterministe | 2–4 semaines | prototype A sur copies, rapport | 70 %+ du delta, zéro régression |
| 2. B QA | 2–3 semaines | audit local/cloud, précision/rappel | précision ≥ 95 %, rappel ≥ 90 % |
| 3. Intégration optionnelle | 2–4 semaines | flag/config, tests, docs | validation exploitation |
| 4. C laboratoire | 6–10 semaines | PoC borné, test aveugle | préférence ≥ 60 %, pas de régression |

**Portes :**

- **G1** : A est déterministe (hash stable), couvre ≥ 70 % des **gestes
  mécaniques détectés** (letterbox noir, bannière pleine largeur, cuts durs),
  ne modifie pas l'ancre, et **A3 reste en `review`** (précision mesurée
  0,20 sur 1059 segments) ; le re-cadrage non traçable part en `review` →
  intégration optionnelle.
- **G2** : B atteint précision ≥ 95 %, rappel ≥ 90 %, faux positifs ≤ 10 %,
  coût ≤ 0,50 €/vidéo, temps ≤ 5 min → pilote.
- **G3** : C est préféré à A+B dans ≥ 60 % des comparaisons aveugles, sans
  régression qualité, coût ≤ 2 €/vidéo → pilote limité.
- **Kill** : A non déterministe, B trop de faux positifs, C ne bat pas A+B,
  ou gouvernance des données non validée → arrêt.

### 7.5 Gain attendu et confiance

| Niveau | Gain | Confiance | Condition |
| --- | ---: | --- | --- |
| A seul | 70–85 % des gestes mécaniques (letterbox noir, bannière, cuts durs) ; A3 en review (précision 0,20) ; 50 % des segments statiques déjà bien cadrés | moyenne sur le mécanique, faible sur le cadrage | A1/A2/A4 durcis ; re-cadrage et keyframes en review |
| A + B | 80–90 % d'auto-validation des cas simples | moyenne | B précis, seuils calibrés |
| A + B + C | 85–95 % des gestes cadrage/nettoyage (théorique) | faible | C validé, humain sur les cas durs |

Les résultats de la Phase 0 (§3.4) et du corpus élargi (§3.6) confirment A1
et A2, mais invalident A3 en auto : sur 1059 segments, la règle de sélection a
une précision de 0,20 et ajoute 35 keyframes sur les vidéos sans piège. Le gain
de A est donc **mécanique avant d'être éditorial** : c'est le scale, le
letterbox et les cuts qui sont automatisables, pas le choix du sujet.

Le gain de A est **mesurable et réversible** ; le gain de C est **hypothétique**
et coûteux. C'est pourquoi la recommandation est asymétrique : A maintenant,
B ensuite, C en laboratoire.

---

## 8. Protocole de spike (résumé)

Le détail est dans `docs/SPIKE-STEP9-PROTOCOLE.md`.

### 8.1 Objectif

Valider sur copies isolées, sans toucher aux 8 étapes ni à `/mnt/cache` :

1. A reproduit les gestes mécaniques et les keyframes observés ;
2. B détecte les non-conformités avec une précision/rappel suffisants ;
3. C apporte un gain sémantique mesurable sans régression.

### 8.2 Jeu de données

- **Entrées** : `/mnt/cache/298 Camille.zip`, `/mnt/cache/300 Camille.zip`.
- **Références** : `/mnt/cache/298 Camille_GOOD.zip`,
  `/mnt/cache/300 Camille_GOOD.zip`, en lecture seule.
- **Copies** : `/tmp/opencode/step9-spike/`, avec SHA-256 avant/après.
- **Interdits** : écrire dans `/mnt/cache`, `runtime/`, ou le dépôt legacy.

### 8.3 Phases

| Phase | Contenu | Critère de sortie |
| --- | --- | --- |
| 0 | baseline STEP8, labels GOOD, métriques | baseline et labels reproductibles |
| A | prototype déterministe | 70 %+ du delta cadrage/nettoyage, hash stable, zéro faux positif sur les contrôles propres |
| B | audit local/cloud | précision ≥ 95 %, rappel ≥ 90 % |
| C | 3–5 segments, agent propose, humain valide | préférence ≥ 60 %, pas de régression |

### 8.4 Métriques

- **A** : ancre inchangée, 2–6 clés (tolérance 7 sur les cas complexes),
  scale ≤ 2 %, détection letterbox/overlay avec précision ≥ 95 % et rappel
  ≥ 90 %, zéro action sur les vidéos propres. Keyframes : mesurer à la fois
  l'écart strict (≤ 20 px) et l'écart utile (≤ 60 px, dans la boîte du
  visage) ; tout ce qui dépasse part en `review`. **Le rendu final n'est pas
  dans le périmètre A.**
- **B** : rappel logo ≥ 90 %, précision ≥ 95 %, rappel sous-titre ≥ 85 %,
  visage hors cadre ≥ 95 %, faux positifs ≤ 10 %, ≤ 5 min, ≤ 0,50 €/vidéo.
- **C** : préférence aveugle ≥ 60 %, réduction de l'écart d'édition ≥ 30 %,
  pas de régression, ≤ 10 min de plus, ≤ 2 €/vidéo.

### 8.5 Critères d'arrêt

- toute écriture hors `/tmp/opencode/step9-spike` ;
- toute modification de `/mnt/cache` ou `runtime/` ;
- toute régression des 282 tests ou des contrats ;
- tout projet A non déterministe ;
- tout envoi cloud sans accord explicite ;
- tout export sans revue humaine.

---

## 9. Annexes

### 9.1 Mesures de référence

Les mesures de rendu sont conservées comme **contexte** (comparaison, scénario
B) : le rendu final n'est pas une sortie du scénario A.

| Mesure | Valeur | Contexte |
| --- | ---: | --- |
| `render-frame` 1080×1920 | 0,15–0,19 s | projet 298, GTX 1650 |
| `render` 5 s, 10 Mbit/s | 6,07 s | 10,36 Mbit/s |
| `render` 65,8 s, 10 Mbit/s | 72,3 s | 10,29 Mbit/s, 4,9 Go RSS |
| Export GOOD 298 | 68,8 s | 10,01 Mbit/s |
| Export GOOD JOSEPH | 147,9 s | 10,27 Mbit/s |
| Outils MCP | 27 | EffectCraft 0.17 |
| Commandes | 734 | EffectCraft 0.17 |
| Effets | 307 | EffectCraft 0.17 |
| Tests Rust | 282 | état du dépôt |

### 9.2 Formules utiles

**Scale pour remplir 1080×1920 :**

```text
scale = max(1080 / largeur_active, 1920 / hauteur_active) × 100
```

**Scale pour sortir une bande basse de hauteur `h_bande` (calage haut) :**

```text
scale ≥ 1920 / (1080 - h_bande) × 100
```

Exemple 300 : bande d'environ 70 px → 1920 / 1010 × 100 ≈ 190 % ; le monteur a
utilisé 207 %, avec une marge de sécurité.

**Coût cloud :**

```text
coût = (tokens_entrée / 1e6) × prix_entrée
     + (tokens_sortie / 1e6) × prix_sortie
```

**Tokens image (ordre de grandeur) :**

```text
tokens ≈ (largeur / patch) × (hauteur / patch) / facteur_de_merge
```

Avec 300–1 500 tokens par image selon le modèle et la résolution.

### 9.3 Schéma `_post.json` proposé (additif, non contractuel)

```json
{
  "schema": 1,
  "source": "exemple.mp4",
  "fps": 25,
  "total_frames": 3000,
  "letterbox": {
    "active_width": 1920,
    "active_height": 1072,
    "top": 4,
    "bottom": 4
  },
  "source_overlays": [
    {
      "kind": "banner",
      "bbox": [0, 955, 1920, 70],
      "persistent": true,
      "action": "exclude_global",
      "time_range": [0.0, 120.0],
      "confidence": 0.94
    }
  ],
  "segments": [
    {
      "num": "3",
      "anchor_x": 760.0,
      "scale": 207.0,
      "keys": [
        {"t": 13.0, "x": 1352.3},
        {"t": 15.0, "x": 830.3}
      ],
      "split_at": null,
      "confidence": 0.92
    }
  ],
  "review": [
    {
      "kind": "subtitle",
      "time_range": [12.6, 18.64],
      "action": "zoom_local",
      "confidence": 0.61
    }
  ]
}
```

### 9.4 Schéma `audit.json` proposé

```json
{
  "schema": 1,
  "project": "The 100 extrait 2_9x16.ecproj",
  "status": "review",
  "score": 0.87,
  "checks": [
    {
      "id": "logo_residual",
      "status": "fail",
      "severity": "high",
      "frames": [1234, 1235],
      "confidence": 0.91,
      "evidence": "logo détecté en bas à gauche"
    }
  ],
  "model": {
    "name": "local-vlm",
    "version": "a-verifier",
    "prompt_hash": "..."
  },
  "reviewer": null
}
```

### 9.5 Références internes

- `AGENTS.md` — règles du dépôt, isolation, contrats.
- `.agents/rules/codingstandards.md` — standards Rust et JSON.
- `.agents/rules/memorybankprotocol.md` — protocole memory-bank.
- `docs/LOT-1-STEP6.md` — contrat `_ae.json`, parité, performance.
- `docs/LOT-1-STEP8.md` — gate scripting, driver, timeout.
- `docs/LOT-7-INTEGRATION.md` — séquence réelle STEP2 → STEP8.
- `docs/VALIDATION-STEP1-8.md` — validation complète, isolation.
- `memory-bank/decisionLog.md` — D-029 à D-032.
- `docs/SPIKE-STEP9-PROTOCOLE.md` — protocole détaillé.
- `docs/LOT-9-POST-STEP8.md` — post-traitement A5/A6 en production.

### 9.6 Glossaire

| Terme | Définition |
| --- | --- |
| **Ancre** | point de la source qui est placé au centre de la composition ; STEP6 le calcule en X |
| **Keyframe** | clé de position dans le temps ; 2–6 suffisent pour un plan long |
| **Letterbox** | bandes noires haut/bas d'un format cinéma |
| **Overlay** | logo, bannière, sous-titre incrusté dans la source |
| **Habillage** | logos et jingle ajoutés par le monteur |
| **QA** | contrôle qualité de conformité |
| **VLM** | modèle multimodal vision-langage |
| **MCP** | Model Context Protocol, JSON-RPC stdio |
| **RDP** | Ramer–Douglas–Peucker, simplification de trajectoire |
| **One-Euro** | filtre de lissage à faible latence |

---

## 10. Conclusion en une phrase

Le montage automatique post-STEP8 n'a pas d'abord besoin d'un agent plus
intelligent ; il a besoin d'un **exécuteur déterministe plus complet** et d'un
**auditeur de conformité**. L'agent co-monteur est un laboratoire utile, pas
une décision de production.
