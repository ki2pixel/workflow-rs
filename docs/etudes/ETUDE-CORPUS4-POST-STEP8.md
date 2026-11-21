# Étude Corpus 4 — Fiabilisation de la post-STEP8 (2026)

> **Périmètre** : `Corpus_Exemple_4` — 414 dossiers `X Camille`, 1 203 projets bruts, 1 200 triplets complets `brut / assist / GOOD`.
> **Date** : 21 novembre 2026.
> **Nature** : analyse descriptive et shadow run en lecture seule, sans réencodage vidéo.
> **Espace de travail** : `benchmark_corpus4/` (partition `/media/kidpixel/WORK`, aucun artefact lourd sur `/`).

---

## 1. Synthèse exécutive

| Indicateur | Valeur | Lecture |
|---|---:|---|
| Projets bruts indexés | **1203** | 414 dossiers |
| Triplets complets mesurables | **1200** | 3 projets sans `_GOOD.ecproj` |
| Segments bruts | **40 999** | moyenne 34.2 / projet |
| Segments appariés | **40 572** | couverture **98.96 %** |
| Accord strict d'échelle (±0,01) | **46.49 %** | 46 % des segments |
| Précision zoom (both / both+auto_only) | **99.13 %** | l'automate zoome rarement à tort |
| Rappel zoom (both / both+human_only) | **40.21 %** | le monteur zoome beaucoup plus |
| MAE globale | **21.22 pts** | 15.17 pts quand les deux zooment |
| A1 — applies conservés | **5 900 / 5 939 (99.34 %)** | faux applies : **39** |
| A2 — applies conservés | **81 / 83 (97.59 %)** | dont 14 à ±2 pts ; faux applies : **2** |
| A2 — revues avec bandeau humain | **235 / 924 (25.43 %)** | 52 proches de la cible A2 (±8 pts) |
| A3 — revues suivies d'une action | **2 300 / 3 390 (67.85 %)** | A3 reste en **review only** |
| A4 — revues suivies d'une action | **25 / 361 (6.93 %)** | mais 664 projets avec cuts ajoutés |
| A5 — revues résolues par zoom | **143 / 143 (100 %)** | 100 % |
| A6 — applies valides | **2 / 2** | template SDM |
| A7 — applies couverts | **44 / 45** | 1 apply « lot only » |
| A7 — revues « video_not_in_mission » | **38 / 103** | mission retrouvée par titre, mais 35/38 ont > 30 j (shadow) |
| A7 — couverture projets | **171 / 1203 (14.21 %)** | A7 actif sur 42 lots, 12–20 nov. |

**Trois faits structurants :**

1. **A1 est globalement fiable** : 0,66 % de faux applies, mais **32 des 39** proviennent d'un seul projet pathologique (`363 Camille / OCLC_DION_AFIDA_DION`). Hors ce projet, le taux résiduel tombe à **0.12 %**.
2. **A7 n'est pas défaillant sur ses applies** (44/45 cohérents), mais son **binding de lot est faux** : le code déduit le lot du nom de dossier (`X Camille`), alors que le dossier n'est pas le lot e-mail. Résultat : **38 revues `video_not_in_mission`** dont **23 avec logos** et **15 sans logo**. Le shadow corrige toutefois l'ampleur : avec une fenêtre de 30 jours, seules **3** de ces 38 missions sont plausibles ; les 35 autres sont des titres réutilisés de janvier à juin 2026.
3. **Le rappel zoom de 40 % n'est pas un KPI de qualité A1/A2** : il mesure surtout les zooms sémantiques du monteur (punch-ins, split-screens, emphases) que A1/A2 ne doivent pas décider. Les KPI utiles sont les taux de conservation A1/A2, les faux applies et les revues transformées en action.

---

## 2. Corpus, intégrité et périmètre

| Élément | Présent | Attendu / commentaire |
|---|---:|---|
| Dossiers `X Camille` | 414 | 414 |
| Projets bruts `_9x16.ecproj` | 1203 | 1 203 |
| Projets `_assist.ecproj` | 1203 | 1 203 |
| Projets `_assist_GOOD.ecproj` | 1200 | **1 200** (3 manquants) |
| `post.json` / `audit.json` / `review.json` | 1203 / 1203 / 1203 | 1 203 chacun |
| `_ae.json` | 1201 | 1 201 |
| `.aep` GOOD | 1128 | 1 128 (le plan annonçait 1 132) |
| `.csv` / `.mp4` | 1203 / 1203 | 1 203 chacun |

**Triplets sans `_GOOD.ecproj`** (exclus des métriques comparatives, conservés dans l'index) :

| Dossier | Projet |
|---|---|
| 62 Camille | Spike drague Buffy (1) |
| 269 Camille | TEVA_Doc mères protectrices_Edouard Durand  92% des enfants victimes demandant de l'aide ne seront pas protégés |
| 379 Camille | Booder n'a pas oublié ses profs… et leurs petites piques  |

**Verdicts d'audit** : `pass` 1093 · `warn` 107.

**Profils d'émission** : `explicit` 816 · `folder` 202 · `unknown` 182. Les profils sont heuristiques ; les tableaux par émission restent indicatifs pour les 182 projets non typés.

---

## 3. Méthode

- **Appariement brut → GOOD** : nom exact, puis bornes exactes, puis bornes à 40 ms, puis recouvrement temporel ≥ 0,80. Qualité : nom exact 38 958 · bornes exactes 1 512 · recouvrement 89 · quasi-bornes 13 · non appariés 427.
- **Comparaison automate → monteur** : `_assist` et `_assist_GOOD` sont appariés segment par segment ; les scales sont comparés à ±0,01.
- **Zoom** : `auto_zoom` / `human_zoom` = variation par rapport au segment brut, pas seulement `> 180 %`. Les scales humaines dominantes sont 180 %, 225 %, 216 %, 243 %, 252 %.
- **A1** : `apply` conservé si le monteur zoome le segment ; faux apply si le monteur reste à 180 %.
- **A2** : projet `apply` conservé si le monteur zoome ; `review` avec bandeau si l'échelle dominante humaine est dans 190–230 % avec couverture ≥ 50 % et hors explication A1.
- **A3/A4/A5** : réponse structurale du GOOD (clés ajoutées, bornes modifiées, zoom) ; A3 est en `review only` (D-030), A5 également.
- **A7** : missions lues dans `email_missions.sqlite3` (42 lots, 12–20 nov. 2026) + `emails_json` filtrés 2026 ; appariement par titre normalisé ; contrôle des applies et des revues.
- **Aucune vidéo n'a été réanalysée** : toutes les métriques viennent des `.ecproj` et des artefacts `docs/post/`.

---

## 4. Analyse globale

### 4.1 Matrice de confusion du zoom

| Both (TP) | Auto seul (FP) | Monteur seul (FN) | Neither (TN) | Précision | Rappel | F1 |
|---|---|---|---|---|---|---|
| 8 798 | 77 | 13 082 | 18 615 | 99.13 % | 40.21 % | 57.21 % |

La précision de **99.13 %** signifie que lorsque l'automate zoome, le monteur zoome presque toujours. Le rappel de **40.21 %** signifie que l'automate ne couvre que 40 % des zooms humains : c'est attendu, car A3/A4/A5 ne sont pas en apply et les zooms éditoriaux ne sont pas des règles géométriques.

### 4.2 Distribution des échelles dominantes du monteur

| Échelle | Segments | Part |
|---|---|---|
| 180 % | 18 692 | 46.07 % |
| 225 % | 2 291 | 5.65 % |
| 216 % | 1 115 | 2.75 % |
| 243 % | 542 | 1.34 % |
| 252 % | 499 | 1.23 % |
| 215.6 % | 337 | 0.83 % |
| 234 % | 317 | 0.78 % |
| 211.8 % | 314 | 0.77 % |

- **33.94 %** des segments sont dans la plage bandeau 190–230 %.
- **26.67 %** des projets ont une échelle dominante « bandeau » (190–230 %, couverture ≥ 50 %).
- **7.92 %** des projets ont une échelle dominante > 230 % (punch-in / emphase).

### 4.3 Types de cadrage humain

| Type | Projets | Part |
|---|---|---|
| uniforme | 428 | 35.67 % |
| partiel | 390 | 32.50 % |
| aucun | 207 | 17.25 % |
| mixte | 175 | 14.58 % |

### 4.4 Cuts : le monteur ajoute des segments

| Indicateur | Valeur |
|---|---:|
| Projets avec cuts ajoutés (`n_good > n_raw`) | **664** |
| Projets avec cuts supprimés | 58 |
| Projets stables | 478 |
| Delta total de segments | **+2 492** |
| Delta moyen / projet | **+2.08** |
| Delta médian | +1.0 |
| P95 | +9 |
| Maximum | +29 |

Le GOOD contient **+2 492 segments** par rapport au brut, soit **6.08 %**. A4 ne voit que 361 revues et 6,9 % d'actions : il ne couvre donc pas l'essentiel du travail de découpe.

---

## 5. Analyse par module

### 5.1 A1 — Letterbox / pillarbox

| Métrique | Valeur |
|---|---:|
| Applies A1 sur segments appariés | **5 939** |
| Conservés (le monteur zoome) | **5 900 (99.34 %)** |
| Faux applies | **39 (0.66 %)** |
| Zoom automatique > monteur de +2 pts | 150 |
| Zoom automatique < monteur de +2 pts | 5137 |
| Revues A1 | 1 728 |
| Revues suivies d'un zoom humain | **955 (55.27 %)** |
| Revues dont le zoom humain est proche de la cible A1 (±3 pts) | 78 |
| Noops A1 avec zoom humain (hors périmètre) | 15 025 / 32 903 (45.66 %) |

**Faux applies par cause** :

| Cause | Segments |
|---|---|
| scene_sombre_faux_letterbox | 30 |
| crop_content_asymetrique | 7 |
| autre | 2 |

**Concentration** : `363 Camille / OCLC_DION_AFIDA_DION` concentre **32** faux applies sur 39. Hors ce projet, il reste **7** faux applies, soit **0.12 %** des applies.

**Faux applies — détail (20 premiers)** :

| Dossier | Programme | Projet | Cause | Auto | Monteur | Pillarbox |
|---|---|---|---|---|---|---|
| 25 Camille | Enquête d'action | EQA 2026 01 16 MOTARDS PS1 SUJET 1 EXTRAIT | crop_content_asymetrique | 229.8 | 180 | non |
| 34 Camille | Non typé | BA DIM PA | autre | 184.5 | 180 | non |
| 83 Camille | la revue de presse | PP - la revue de presse - Giroud et stotz en vendeur Corse | crop_content_asymetrique | 184.5 | 180 | non |
| 101 Camille | 66 Minutes | 66M - MAMIE BOITE DE NUIT_TAT_M6 | crop_content_asymetrique | 225.6 | 180 | non |
| 101 Camille | 66 Minutes | 66M - MAMIE BOITE DE NUIT_TAT_M6 | crop_content_asymetrique | 283.3 | 180 | non |
| 101 Camille | 66 Minutes | 66M - PARK LA BREA_TAT_M6 | crop_content_asymetrique | 184.5 | 180 | non |
| 280 Camille | Les Marseillais | Luna s’énerve contre Paga et Greg à cause de leurs comportements | autre | 193.3 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 203 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 203 | 180 | non |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | scene_sombre_faux_letterbox | 199.7 | 180 | non |

**Lecture** :
- Les **32** faux applies du projet `OCLC_DION_AFIDA_DION` se répartissent en **30 scènes sombres** (`active_height` 960–976, barres symétriques 50–60 px, `barred_frac` 0) et **2 crops de contenu asymétriques** (barres 68/52 px, ratio 0,15–0,47), tous dans la bande ambiguë 950–990 px.
- Les **3** faux applies de `66 Minutes` sont des inserts archive / images déjà cadrées ; le monteur laisse à 180 %.
- Les **4** autres cas sont deux crops de contenu asymétriques (Enquête d'action, revue de presse), un bord haut/bas de 10–14 px (`BA DIM PA`) et un cadrage à 34/38 px (`Les Marseillais`).

**Recommandations A1** :
1. **Pas de garde-fou statique 900–1040** : les deux projets `361 Camille` (`TEVA_Incroyable embouteillage`) totalisent **58 applies valides à 203 %**, avec la même géométrie que 363. Un garde-fou aveugle rétrograderait ces 58 applies, plus 3 applies à 199,7 % (dont 1 dans 363), soit **61 applies valides**. La hauteur active seule ne sépare pas une scène sombre d'un vrai letterbox.
2. **Confirmation native sur la bande ambiguë** : activer `a1_refine_native` pour les applies dont `active_height ∈ [950, 990]` ou `scale_target ∈ [195, 205]`, avec `barred_frac < 0,20`. Si le natif confirme un letterbox stable, conserver l'apply ; sinon `review`. Coût : une passe ffmpeg par segment ambigu.
3. **Alerte projet, pas de downgrade** : si A1 applique sur > 50 % des segments avec une cible médiane dans 195–205 %, émettre une **revue projet d'audit** sans annuler les applies. Le cas 363 est ainsi signalé, mais 361 reste intact tant que le natif n'a pas tranché.
4. **Prior 66 Minutes** : les inserts letterboxés y sont éditoriaux ; privilégier `review` sur les segments dont `active_height < 1072` et (`crop.ratio < 0,8` ou durée < 2 s), ce qui couvre les trois faux applies observés.
5. **Conserver le veto latéral** (`lateral_min = 32 px`) : aucun faux apply pillarbox latéral n'apparaît dans ce corpus, ce qui valide le garde-fou existant.

**Contre-exemple mesuré** : un garde-fou brut « > 50 % d'applies et cible médiane < 210 % » toucherait **27 projets**, dont **1** seulement contient un faux apply. Il ne doit donc pas annuler les applies.

### 5.2 A2 — Logos / bandeaux

| Métrique | Valeur |
|---|---:|
| Applies A2 | **83** |
| Conservés (zoom humain) | **81 (97.59 %)** |
| Dont échelle exacte ±2 pts | 14 (16.87 %) |
| Ajustés (zoom humain, autre échelle) | 67 |
| Reverted / faux applies | **2** |
| Revues A2 | **924** |
| Revues avec bandeau humain | **235 (25.43 %)** |
| Revues proches de la cible A2 (±8 pts) | **52** |
| Noops A2 avec bandeau humain | 12 / 191 (6.28 %) |

**Types de revues A2** :

| Type | Revues | Action humaine | Taux |
|---|---|---|---|
| décor bas | 305 | 43 | 14.10 % |
| logo étroit | 201 | 30 | 14.93 % |
| bande haute | 150 | 36 | 24 % |
| bande animée | 118 | 44 | 37.29 % |
| autre | 56 | 44 | 78.57 % |
| bande partielle | 52 | 9 | 17.31 % |
| sous-titres | 20 | 13 | 65 % |
| bande sombre | 17 | 16 | 94.12 % |
| bande non persistante | 5 | 0 | 0 % |

`Action humaine` = clé, borne ou zoom modifié dans le GOOD ; ce n'est pas limité aux bandeaux.

**Taux de bandeau humain par programme (revues A2, N ≥ 10)** :

| Programme | Revues | Bandeau humain | Taux |
|---|---|---|---|
| Les Marseillais | 208 | 6.25 | 6.25 % |
| Non typé | 51 | 47.06 | 47.06 % |
| M6 - Non typé | 51 | 70.59 | 70.59 % |
| PP - Postdiff/Prédiff | 60 | 51.67 | 51.67 % |
| Un Jour, un Doc | 16 | 81.25 | 81.25 % |
| M6 - Romance | 40 | 50.0 | 50 % |
| L'Amour est dans le pré | 38 | 44.74 | 44.74 % |
| Cauchemar en cuisine | 25 | 12.0 | 12 % |
| 66 Minutes | 23 | 43.48 | 43.48 % |
| L'Île de la tentation | 35 | 77.14 | 77.14 % |
| Les Apprentis Aventuriers | 30 | 3.33 | 3.33 % |
| Ma Super Ligue 1 | 15 | 33.33 | 33.33 % |
| Vous les femmes | 17 | 0.0 | 0 % |
| Piquantes ! | 17 | 0.0 | 0 % |
| Les Traîtres | 15 | 0.0 | 0 % |

**Bandeaux manqués — cas forts (delta ≤ 8 pts)** :

| Dossier | Programme | Projet | Type | Humain | Cible A2 | Delta | OCR |
|---|---|---|---|---|---|---|---|
| 221 Camille | Laisser aller | EXTRAIT PREDIFF 1 – Il est étonné de voir sa fille derrière le rideau  | bande animée | 207.4 | 207.4 | 0 | yaoue |
| 182 Camille | Un Jour, un Doc | UJUD - REPARATION LAMPE | bande sombre | 200.7 | 200.8 | -0.1 | ENCOMBRANTS : DE L'OR SUR LES TROTTOIRS |
| 255 Camille | Kaamelott | FONTAINE A EAU_TAT_M6 | bande animée | 204.8 | 204.9 | -0.1 | BES |
| 170 Camille | L'Île de la tentation | postdiff 1 – Rémi accuse Virginie de mentir  | bande haute | 216 | 216.3 | -0.3 |  |
| 227 Camille | MSL1 | Gulli - MSL1 - Une soirée magique à la cérémonie des trophée UNFP | sous-titres | 223.3 | 224 | -0.7 | TROPHÉES |
| 31 Camille | Un Jour, un Doc | UJUD - COMMANDE SHEIN | bande sombre | 205.6 | 206.6 | -1 | LA MODE LOW COST : TOUJOURS PLUS ! |
| 414 Camille | Un Jour, un Doc | UJUD - CONSTRUCTION | autre | 207 | 208.3 | -1.3 | UNJOUR |
| 384 Camille | Non typé | Elle se fait incarcérer volontairement ! | autre | 207 | 205.7 | 1.3 | ENQUETE |
| 42 Camille | Ma Super Ligue 1 -Les enfants aux commentaires avec Smaïl Bouabdellah | Gulli - Ma Super Ligue 1 -Les enfants aux commentaires avec Smaïl Bouabdellah_TAT_Gulli | bande haute | 215.9 | 214.5 | 1.5 |  |
| 262 Camille | 66 Minutes | 66M - APPAT DU GAIN | autre | 216.7 | 218.2 | -1.5 | JEUX DU DOPAGE : LE SPECTACLE À TOUT PRI |
| 32 Camille | Les Marseillais | Patrick Sébastien raconte une blague de Coluche ! | bande animée | 222.2 | 224 | -1.8 | LA |
| 293 Camille | PP - Postdiff/Prédiff | POSTDIFF 2 | bande sombre | 205.2 | 203.2 | 2.0 | POMPIERS ET SAMU EN ORDRE DE BATAILLE |
| 196 Camille | L'Île de la tentation | La tentatrice pense qu’il peut céder à la tentation | autre | 225 | 223.0 | 2.0 |  |
| 232 Camille | M6 - Romance | Il démontre que la maturité est mieux que la jeunesse | bande animée | 225 | 227.0 | -2.0 | COUGAR |
| 203 Camille | PP - Postdiff/Prédiff | XPENG P7+, la voiture la plus intelligente du monde !  | bande partielle | 213.0 | 210.9 | 2.1 | XPENG P7+ LA VOITURE INTELLIGENTE |

**Lecture** :
- A2 `apply` est fiable : **97.59 %** des applies sont conservés par le monteur, avec seulement **2** revert.
- Les revues A2 sont **très bruitées** : seulement **25.43 %** correspondent à un bandeau humain. Les types `décor bas` (305) et `logo étroit` (201) dominent, avec seulement **14.10 %** et **14.93 %** d'action humaine.
- À l'inverse, `bande sombre` (**94.12 %** d'action), `autre` (**78.57 %**) et `sous-titres` (**65 %**) sont des signaux forts : ce sont les revues à prioriser, pas à supprimer.
- Les émissions à fort taux de bandeau humain sont **Un Jour un Doc (81.25 %)**, **L'Île de la tentation (77.14 %)**, **M6 Romance (50 %)**, **PP Postdiff (51.67 %)**, **66 Minutes (43.48 %)**.
- À l'inverse, **Les Marseillais (6.25 %)**, **Cauchemar en cuisine (12 %)**, **Piquantes (0 %)** ne doivent pas déclencher d'apply automatique : leurs zooms sont sémantiques.

**Recommandations A2** (revues par le shadow) :
1. **`décor bas` conservateur** : passer en `noop` si la zone est haute (`height > 200`), sans OCR et si la cible est hors de **190–300 %**. Le shadow mesure **131 noops** pour **14 actions humaines perdues** ; un seuil 190–230 % en perdrait 61.
2. **`logo étroit` : ne pas nooper** sur le seul critère `width < 0,35` — le shadow perd 30 actions humaines sur 201. Seul un OCR explicitement `logo_only` autorise le noop.
3. **Priors par émission** : appliquer automatiquement les bandeaux sur `Un Jour un Doc`, `L'Île de la tentation`, `M6 Romance`, `PP Postdiff` lorsque la cible géométrique est à ±8 pts et que l'OCR est persistant ; conserver `review` pour `Les Marseillais`, `Piquantes`, `Cauchemar en cuisine`.
4. **Promouvoir `bande sombre`** : 13 promotions en `apply`, dont 12 confirmées par `a2_missed` et 12 par une action humaine. `sous-titres` reste en `review` ; `bande animée` (37 %) aussi.

### 5.3 A3 — Mouvements / keyframes

| Métrique | Valeur |
|---|---:|
| Revues A3 | **3 390** |
| Revues suivies d'une action structurale | **2 300 (67.85 %)** |
| Revues avec clés ajoutées | **757 (22.33 %)** |

**Taux d'action par famille** :

| Famille | Revues A3 | Taux d'action |
|---|---|---|
| M6 | 2188 | 68.42 % |
| PP | 390 | 58.46 % |
| Gulli | 227 | 77.53 % |
| Autre | 283 | 78.45 % |
| 6ter | 200 | 62.50 % |
| Teva | 102 | 50.98 % |

Les familles `Autre` et Gulli sont les plus actives ; Teva et PP les plus réservées.

**Lecture** : A3 est en **review only** (`A3 désactivé`, D-030), mais **68 %** des revues sont suivies d'une action du monteur, et **22 %** ajoutent des clés. C'est le plus fort potentiel d'automatisation non exploité du corpus.

**Recommandations A3** :
1. **Pilote A3 apply conservateur** sur les candidats `strong_candidate` / `high_confidence`, en priorité Gulli, M6 magazines, 6ter.
2. **Garde-fous** : pas de clés sur segments < 1 s, pas de clés si `amplitude_px < 150`, pas de clés si le visage sort du cadre, limiter à 2–3 clés.
3. **Mesurer** le gain sur un échantillon de 100 projets avant généralisation.

### 5.4 A4 — Découpes / raccords

| Métrique | Valeur |
|---|---:|
| Revues A4 | **361** |
| Revues suivies d'une action | **25 (6.93 %)** |
| Projets avec cuts ajoutés (`n_good > n_raw`) | **664 / 1200** |
| Delta total de segments | **+2 492** |

**Lecture** : A4 ne traite que les segments courts (0,04–0,48 s) et 93 % de ses revues ne sont pas suivies d'une modification de bornes. Pourtant, le GOOD ajoute **+2 492 segments** : le travail réel de découpe est massif et majoritairement invisible pour A4.

**Recommandations A4** :
1. **Réconciliation de cuts** : comparer `n_raw` et `n_good` par projet ; si `|delta| ≥ 2`, émettre une revue projet « cuts ajoutés/supprimés ».
2. **Alignement des bornes** : détecter les segments GOOD qui chevauchent plusieurs segments bruts et proposer une fusion/découpe.
3. **Prioriser** Les Apprentis Aventuriers (+5.57 segments/projet), Les Marseillais (+3.95), L'Amour est dans le pré (+2.92), Vous les femmes (+2.67).

### 5.5 A5 — Visages tronqués

| Métrique | Valeur |
|---|---:|
| Revues A5 | **143** |
| Résolues par un zoom humain | **143 (100 %)** |

**Lecture** : 100 % des revues A5 sont résolues par le monteur. Les cas sont simples : source 1280×720 dans comp 1080×1920, vide max 624 px (16 cas) ou 1401,6 px (127 cas). A5 est report-only et laisse tout le travail au monteur.

**Recommandations A5** :
1. **Promouvoir A5 en apply** pour les cas mono-segment, sans keyframes, avec `resolution=resolved` et gaps symétriques.
2. **Conserver en review** les cas multi-segments ou avec visage détecté hors cadre.
3. **Mesurer** le taux de conservation sur 50 projets avant généralisation.

### 5.6 A6 — Asservissements

| Métrique | Valeur |
|---|---:|
| Applies A6 | **2** |
| Valides | **2** |

A6 ne se déclenche que sur le template SDM (Scènes de ménages) mono-segment sans keyframe. Les 2 applies sont valides. Volume trop faible pour conclure, mais le mécanisme est cohérent.

### 5.7 A7 — Missions & logos e-mail

**Sources de missions** :

| Source | Lots | Lignes mission |
|---|---|---|
| `email_missions.sqlite3` | 42 | 76 |
| `emails_json` 2026 | 272 | 814 |
| **Dédupliqué** | **250** | **863** |

Le dédupliqué contient **855 vidéos uniques**, dont **592** avec logos et **271** sans logo.

Les e-mails 2025 (361) sont exclus : les numéros de lot sont réutilisés d'une année sur l'autre (170 lots sur 319 existent en 2025 et 2026). La base `email_missions.sqlite3` couvre les 42 lots du 12 au 20 novembre 2026, dont **tous les lots des applies A7**.

**Décisions A7** :

| Contrôle | Nombre | Lecture |
|---|---|---|
| covered | 44 | apply cohérent avec la mission et les logos |
| mission_lot_only | 1 | mission du lot non appariée par titre |
| correct_review | 61 | review justifiée, aucune mission trouvée |
| false_negative_candidate | 38 | mission 2026 exacte retrouvée par titre |
| correct_noop | 21 | noop justifié, mission sans logo |
| already_present | 2 | logo déjà présent |
| other | 4 | autre raison |

**Revues `video_not_in_mission` avec mission exacte retrouvée** :

- **38** cas, dont **23 avec logos** (auraient dû être des applies) et **15 sans logo** (auraient dû être des noops `no_logo`).
- Soit **36.89 %** des revues A7 auditées (103 revues sur 1 203 projets ; le benchmark en décompte 102 sur les 1 200 projets mesurables).

| Dossier | Projet | Mission retrouvée | Lot | Logos | Assets |
|---|---|---|---|---|---|
| 34 Camille | BA DIM PA | BA DIM PA | 194 | oui | m6.png |
| 43 Camille | PP - Kaamelot - Il fait gouter le pain | PP - Kaamelot - Il fait gouter le pain | 240 | oui | pp.png |
| 43 Camille | PP - Kaamelot - Le pain | PP - Kaamelot - Le pain | 240 | oui | pp.png |
| 121 Camille | 6TER_Kaamelott_EXTRAIT 1 | 6TER_Kaamelott_EXTRAIT 1 | 179 | oui | 6ter.png |
| 122 Camille | PP - Kaamelot - Coupe de cheveux | PP - Kaamelot - Coupe de cheveux | 181 | oui | pp.png |
| 122 Camille | UJUD - MACHINES A CAFE_TAT_M6 | UJUD - MACHINES A CAFE_TAT_M6 | 181 | non |  |
| 122 Camille | UJUD - RUSH MIDI_TAT_M6 | UJUD - RUSH MIDI_TAT_M6 | 181 | non |  |
| 124 Camille | TEVA_PIQUANTES EM8 PREDIFF_Son anecdote de la salade au McDo | TEVA_PIQUANTES EM8 PREDIFF_Son anecdote de la salade au McDo | 196 | non |  |
| 131 Camille | M6 - LMBF - Bruno tente un mensonge... mais Noëmie le dénonc | M6 - LMBF - Bruno tente un mensonge… mais Noëmie le dénonc | 231 | non |  |
| 134 Camille | bouton | bouton | 242 | oui | m6+.png + Boucle Hayu x BD |
| 134 Camille | embrouille_RHOSLC E12S5 | embrouille_RHOSLC E12S5 | 242 | oui | m6+.png + Boucle Hayu x BD |
| 170 Camille | prédiff 2 – La confrontation houleuse de Rémi et Virginie ! | prédiff 2 – La confrontation houleuse de Rémi et Virginie  | 173 | non |  |
| 171 Camille | La descente aux enfers de Michael Jackson  | La descente aux enfers de Michael Jackson | 179 | oui | m6+.png |
| 171 Camille | MJ était terrorrisé par son pere | MJ était terrorrisé par son pere | 179 | oui | m6+.png |
| 176 Camille | M6 - Top Chef - Ce candidat est obligé d’abandonner Top Chef | M6 - Top Chef - Ce candidat est obligé d’abandonner Top Che | 195 | non |  |
| 176 Camille | TEVA_Piquantes EM12_prédiff | TEVA_Piquantes EM12_prédiff | 195 | oui | teva.png |
| 177 Camille | COMPTE INSTAGRAM_TAT_M6 | COMPTE INSTAGRAM_TAT_M6 | 200 | non |  |
| 177 Camille | JAEKOO | JAEKOO | 200 | oui | turbo.png |
| 177 Camille | JET 42M_TAT_M6 | JET 42M_TAT_M6 | 200 | non |  |
| 177 Camille | TRAQUEUR STARS_TAT_M6 | TRAQUEUR STARS_TAT_M6 | 200 | non |  |

**Cause racine dans le code Rust** :

- `crates/workflow-core/src/mission.rs` : `ClientMission::lot_from_folder(folder)` extrait le lot du nom de dossier (`136 Camille`, `Lot 136`).
- `crates/workflow-steps/src/post/run.rs` : `run_a7()` appelle `lot_from_folder(project_folder)` pour alimenter `A7Options.lot`.
- `crates/workflow-steps/src/post/a7.rs` : `apply()` filtre les missions par `mission.lot == lot` avant `mission.match_video(stem)`.

Or **le dossier `X Camille` n'est pas le lot e-mail**. Les couples (dossier, lot) des applies A7 sont parfaitement ordonnés (Spearman = 1.0) mais l'offset varie de **-209 à -136** (médiane -162).

| Dossier | N° dossier | Lot mission | Offset | Projet |
|---|---|---|---|---|
| 375 Camille | 375 | 166 | -209 | PP - LRDP - Karine Dubernet en pilleur |
| 377 Camille | 377 | 181 | -196 | Vrai ou faux  quand la date est dépassée, il faut  |
| 379 Camille | 379 | 185 | -194 | Elodie se clash avec une québécoise sur le françai |
| 379 Camille | 379 | 185 | -194 | Jarry et Élodie révisent leurs langues...à leur ma |
| 379 Camille | 379 | 185 | -194 | Quand tu te retrouve dans un train low cost_TAT_M6 |
| 381 Camille | 381 | 194 | -187 | E3325811Short dates de péremptionl_TAT_M6 |
| 382 Camille | 382 | 195 | -187 | Gulli - Totally Spies - Clover et Blaine surfent  |
| 382 Camille | 382 | 195 | -187 | Gulli - Totally Spies - Les retrouvailles entre Bl |
| 383 Camille | 383 | 196 | -187 | PP - Inspecteur Lavardin - Il attend la nuit pour  |
| 385 Camille | 385 | 201 | -184 | LMCR2_RS BFC EXTRAIT 2.Copy.01 |
| 386 Camille | 386 | 209 | -177 | Gulli - Mortelle Adèle - Adèle piège son gâteau d' |
| 387 Camille | 387 | 213 | -174 | Hillary se motive et veut s’essayer au gogo dancin |

**Couverture A7** :

| Indicateur | Valeur |
|---|---:|
| Projets avec au moins une action A7 | **171 / 1203 (14.21 %)** |
| Dossiers avec A7 | **71 / 414** |
| Projets A7 avec mission appariée | 109 |
| Projets A7 avec logos | 73 |
| Projets sans A7 mais mission trouvée | 313 (dont **1** dans la fenêtre 30 j) |
| Projets sans A7 et sans mission | 719 |

**Banque de logos** :

| Indicateur | Valeur |
|---|---:|
| Assets demandés par les missions 2026 | 72 |
| Couverts par `logos_overlay` + fallback | **59** |
| Résolus localement (`docs/`) | 4 |
| Absents des banques centrales | **9** |

| Asset manquant | Missions | A7 | Local |
|---|---|---|---|
| lphdf | 29 | 0 | — |
| tec | 3 | 0 | — |
| cec | 2 | 0 | — |
| baywatch | 1 | 0 | — |
| clair de lune | 1 | 0 | — |
| derriere la positive attitude | 1 | 0 | — |
| lorie | 1 | 0 | — |
| m6 mov inclure signature animee en | 1 | 0 | — |
| sphere 5 | 1 | 0 | — |

`lphdf` (29 missions), `cec` (2), `tec` (3) sont les absences les plus fréquentes. `cec` correspond probablement à `cauchemar en cuisine.png` ; un alias suffirait. `lphdf` et `tec` doivent être ajoutés à `assets/mission_overlays`.

**Recommandations A7** :
1. **P0 — Ne plus déduire le lot du dossier** : supprimer `lot_from_folder` du chemin A7 pour les corpus reconstitués, ou le remplacer par le lot réel issu du manifeste de mission.
2. **P0 — Fallback global par titre, borné dans le temps** : si aucune mission du lot ne matche la vidéo, chercher dans les missions dont la date est dans `[génération − 30 j, génération + 1 j]` ; si exactement une, l'utiliser avec `match_source: global_fallback` ; si plusieurs avec des instructions différentes, `review ambiguous_global_video`. Sans borne basse, 35 des 38 revues et 312 des 313 candidats venaient de missions trop anciennes.
3. **P0 — Mission sans logo** : si la mission retrouvée a `no_logo`, émettre `noop no_logo`, jamais `review video_not_in_mission`.
4. **P1 — Compléter la banque** : alias `cec -> cauchemar en cuisine.png`, ajouter `lphdf.png`, `tec.png`, `baywatch.png`, `clair de lune.png`, `lorie.png`, `sphere 5.png`.
5. **P1 — Étendre A7** : seulement 14 % des projets ont une action A7 ; 313 titres sont retrouvés ailleurs, mais **312 hors de la fenêtre de 30 jours**. Après contrôle des dates, le gain réel est **1 nouveau projet** et **3 revues récupérées** ; l'extension rétroactive n'est donc pas prioritaire.

---

## 6. Analyse par émission

### 6.1 Familles

| Famille | Projets | Segments appariés | Précision | Rappel | F1 | MAE | MAE both | A1 kept | A1 faux | A2 bandeau | A3 acté | A4 acté |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| M6 | 673 | 26406 | 99.25 % | 41 % | 58.03 % | 20.37 | 15.95 | 99.87 % | 5 | 25.22 % | 68.42 % | 7.78 % |
| PP | 174 | 5075 | 99.89 % | 30.69 % | 46.95 % | 20.05 | 14.36 | 99.88 % | 1 | 34.07 % | 58.46 % | 4.55 % |
| Gulli | 109 | 2864 | 100 % | 31.49 % | 47.90 % | 24.39 | 17.44 | 100 % | 0 | 26.09 % | 77.53 % | 8.70 % |
| Autre | 95 | 3674 | 97.02 % | 43.95 % | 60.50 % | 35.34 | 15.76 | 94.19 % | 33 | 47.06 % | 78.45 % | 1.79 % |
| 6ter | 82 | 1870 | 100 % | 64.94 % | 78.75 % | 8.21 | 6.58 | 100 % | 0 | 5.88 % | 62.50 % | 12.50 % |
| Teva | 67 | 683 | 100 % | 61.18 % | 75.92 % | 9.29 | 4.05 | 100 % | 0 | 1.96 % | 50.98 % | 9.09 % |

### 6.2 Programmes (N ≥ 10)

| Programme | Famille | Projets | F1 | Rappel | MAE | A1 kept | A1 faux | A2 bandeau | A3 acté | A4 acté |
|---|---|---|---|---|---|---|---|---|---|---|
| Les Marseillais | M6 | 216 | 3.31 % | 1.68 % | 22.90 | 97.73 % | 1 | 6.25 % | 60.94 % | 4.26 % |
| Non typé | Autre | 95 | 60.50 % | 43.95 % | 35.34 | 94.19 % | 33 | 47.06 % | 78.45 % | 1.79 % |
| M6 - Non typé | M6 | 71 | 42.59 % | 27.43 % | 36.13 | 100 % | 0 | 70.59 % | 90.42 % | 0 % |
| PP - Postdiff/Prédiff | PP | 67 | 36.19 % | 22.09 % | 26.93 | 100 % | 0 | 51.67 % | 68.38 % | 9.09 % |
| Un Jour, un Doc | M6 | 42 | 81.15 % | 68.78 % | 15.95 | 100 % | 0 | 81.25 % | 99.32 % | 0 % |
| M6 - Romance | M6 | 40 | 91.47 % | 84.28 % | 13.64 | 100 % | 0 | 50 % | 75 % | 21.88 % |
| L'Amour est dans le pré | M6 | 39 | 91.48 % | 84.29 % | 18.96 | 100 % | 0 | 44.74 % | 86.47 % | 0 % |
| Cauchemar en cuisine | M6 | 38 | 57.35 % | 40.20 % | 8.04 | 100 % | 0 | 12 % | 47.93 % | 16.67 % |
| 66 Minutes | M6 | 37 | 25.85 % | 14.93 % | 17.21 | 0 % | 3 | 43.48 % | 80.52 % | 10 % |
| L'Île de la tentation | M6 | 36 | 86.43 % | 76.11 % | 24.05 | 100 % | 0 | 77.14 % | 88.42 % | 5.26 % |
| Les Apprentis Aventuriers | M6 | 30 | 18.48 % | 10.18 % | 27.78 | 100 % | 0 | 3.33 % | 67.53 % | 0 % |
| Ma Super Ligue 1 | Gulli | 24 | 59.04 % | 41.88 % | 41.97 | — | 0 | 33.33 % | 88.73 % | 0 % |
| Vous les femmes | Teva | 24 | — | 0 % | 30.89 | — | 0 | 0 % | 100 % | 0 % |
| Piquantes ! | Teva | 18 | 4.88 % | 2.50 % | 13.02 | 100 % | 0 | 0 % | 36.73 % | 20 % |
| Les Traîtres | M6 | 16 | — | 0 % | 20.27 | — | 0 | 0 % | 25 % | 0 % |
| Pékin Express | M6 | 16 | — | 0 % | 29.17 | — | 0 | 0 % | 76.92 % | — |
| Top Chef | M6 | 15 | — | 0 % | 7.29 | — | 0 | 0 % | 30.77 % | 0 % |
| 6ter - Non typé | 6ter | 14 | 98.36 % | 96.77 % | 2.22 | 100 % | 0 | 25 % | 85.71 % | 0 % |
| Norbert commis d'office | 6ter | 14 | — | 0 % | 3.05 | — | 0 | 0 % | 43.24 % | 0 % |
| Kaamelott | PP | 12 | 71.28 % | 55.37 % | 6.16 | — | 0 | 14.29 % | 46.88 % | 0 % |
| La France a un incroyable talent | M6 | 12 | 82.55 % | 70.29 % | 10.43 | 100 % | 0 | 28.57 % | 48.44 % | 0 % |
| LMBF | M6 | 11 | — | 0 % | 6.09 | — | 0 | 9.09 % | 33.33 % | 0 % |
| The Middle | Gulli | 11 | — | 0 % | 5.24 | — | 0 | 0 % | 38.46 % | 0 % |
| E=M6 | M6 | 10 | 78.57 % | 64.71 % | 10.88 | 100 % | 0 | 25 % | 100 % | 13.33 % |

### 6.3 Signatures et typologie

**Émissions déterministes** (F1 > 75 %, MAE < 10, faible bruit A1/A2) :
- **6ter** (F1 78.75 %, MAE 8.21) : Norbert, Les Mamans, Le Château de mes rêves, Les Reines de la Route.
- **Teva** (F1 75.92 %, MAE 9.29) : Piquantes, Vous les femmes, documentaires.
- **Un Jour un Doc** (F1 81.15 %) : très stable, A2 bandeau 81 %.
- **L'Amour est dans le pré** (F1 91.48 %) : cadrage stable, A2 44 %.
- **M6 Romance** (F1 91.47 %) : cadrage stable, A2 51 %.
- **L'Île de la tentation** (F1 86.43 %) : A2 75 %, bon candidat apply.

**Émissions créatives / sémantiques** (F1 faible, zooms humains nombreux, cuts ajoutés) :
- **Les Marseillais** (F1 3.31 %, 216 projets) : punch-ins, split-screens, réactions ; A2 bandeau 6.25 %, A3 60.94 %.
- **Les Apprentis Aventuriers** (F1 18.48 %) : +5.57 cuts/projet, le plus fort delta de découpe.
- **Piquantes !** (F1 4.88 %) : A2 bandeau 0 %, A3 36.73 %.
- **Gulli jeunesse** (Malcolm, Les Sisters, Mortelle Adèle) : F1 47.90 %, MAE 24.39 ; les zooms sont souvent des recadrages sur personnage.

**Famille PP** (postdiff/prédiff/films) : F1 46.95 %, rappel 30.69 %. Les postdiff/prédiff mélangent plusieurs émissions ; le profiling par programme y est moins fiable.

---

## 7. Recommandations priorisées pour `crates/workflow-steps/src/post/`

| Priorité | Module | Changement | Gain attendu | Risque |
|---|---|---|---|---|
| **P0** | A7 | Fallback global par titre, fenêtre 30 j ; `no_logo` -> noop | **3 revues** récupérées (2 apply + 1 noop) et **1** nouveau projet ; 35 revues et 312 candidats hors fenêtre | faible avec borne basse |
| **P0** | A1 | Confirmation native sur la bande ambiguë `active_height 950–990` / cible 195–205 % | **32 faux applies** de 363 -> review ; **61 applies valides** à préserver (dont 58 de 361) | moyen, dépend du natif ; shadow run ciblé requis |
| **P0** | A1 | Alerte projet (pas de downgrade) si > 50 % d'applies et cible 195–205 % | signale `OCLC_DION` sans casser les 27 projets légitimes | faible |
| **P1** | A2 | Noop `décor bas` hors 190–300 % ; pas de noop `logo étroit` sans OCR `logo_only` ; promotions `bande sombre` | revues **-14,2 %** (131), 14 actions perdues, 13 promotions | faible/moyen |
| **P1** | A3 | Pilote apply conservateur sur `strong_candidate` | **68 %** des revues A3 sont actées | moyen, keyframes |
| **P1** | A4 | Réconciliation `n_raw` vs `n_good` ; revue projet si `|delta| ≥ 2` | couvre **664 projets** | faible |
| **P2** | A5 | Apply des cas mono-segment sans keyframes | **100 %** des revues résolues | moyen |
| **P2** | A6 | Étendre le template SDM à d'autres missions récurrentes | volume faible | faible |
| **P2** | A7 | Alias et ajout des logos manquants (`lphdf`, `tec`, `cec`, ...) | 9 assets centraux manquants | faible |
| **P2** | Config | Priors par émission (`6ter`, `Teva`, `Un Jour un Doc`, `IDLT`, `Les Marseillais`) | personnalisation des seuils | à maintenir |

**Ordre d'exécution conseillé** : A7 (P0, gain immédiat sur les revues), A1 (P0, faux applies), puis A2/A3 en pilote, A4/A5 en second lot. Le shadow run peut être lancé après A1/A2/A7 pour mesurer l'impact sur un échantillon de 100–200 projets, avant généralisation.

---

## 8. Limites

- **Profiling d'émission heuristique** : 182 projets restent `unknown`, 202 sont rattachés par majorité de dossier. Les tableaux par programme sont indicatifs, pas une vérité de programmation.
- **A7 par titre** : les 38 « false negatives » sont des missions 2026 exactes retrouvées par titre ; certaines peuvent être des rediffusions/lots postérieurs. Elles restent des candidates à revue, pas des erreurs certaines.
- **A3/A4/A5** : la réponse structurale du GOOD est une approximation ; les keyframes peuvent être « cuites » dans le projet, et les bornes peuvent différer pour des raisons de conformité.
- **3 projets sans GOOD** exclus des métriques comparatives.
- **Shadow run en lecture seule** : A1/A2/A7 simulés sur 1 203 projets ; le contrôle natif A1 est un proxy luminance/gradient sur 93 segments, pas le détecteur Rust exact. Aucun détecteur YOLOX/ONNX n'a été rejoué, aucune vidéo réencodée.
- **Pas de réanalyse vidéo** : les conclusions A1 « scène sombre » reposent sur `cropdetect` et les détails A1, pas sur une mesure de luminance directe.

---

## 9. Annexes

### 9.1 Sorties produites

| Fichier | Contenu |
|---|---|
| `out/index.json` | inventaire, intégrité, profils |
| `out/summary.json` | métriques globales, familles, programmes |
| `out/shows.csv` | table famille + programme |
| `out/show_profiles.csv` | profil de chaque projet |
| `out/projects.csv` | une ligne par projet |
| `out/segments.csv` | une ligne par segment apparié |
| `out/reviews.csv` | revues A1–A7 |
| `out/false_applies.json` | 39 faux applies A1 détaillés |
| `out/a2_missed.json` | 235 bandeaux A2 candidats |
| `out/a7-audit.json` | synthèse A7 |
| `out/a7-actions.csv` | 171 actions A7 contrôlées |
| `out/a7-missions.csv` | missions 2026 dédupliquées |
| `out/a7-projects.csv` | appariement projet → mission |
| `out/a7-logos.csv` | couverture des assets |

### 9.2 Commandes

```sh
BENCH="/media/kidpixel/WORK/Backup/CLIENTS/Média Solution/benchmark_corpus4"
python3 "$BENCH/scripts/benchmark-corpus4.py" index
python3 "$BENCH/scripts/benchmark-corpus4.py" analyze
python3 "$BENCH/scripts/audit-a7-corpus4.py"
python3 "$BENCH/scripts/report-corpus4.py"
```

### 9.3 Reproductibilité

- Corpus lu en lecture seule ; aucun `.ecproj` modifié.
- Sorties uniquement sous `benchmark_corpus4/out/`.
- `analyze` traite 1 200 projets en ~21 s (24 cœurs, 62 Go RAM) ; `audit-a7` en ~10 s.
- `shadow-corpus4.py` traite 1 203 projets en ~1 min 40 s, dont 93 segments A1 décodés.
- Aucun fichier temporaire lourd sur `/`.

---

## 10. Shadow run full corpus (exécuté)

| Module | Avant | Après shadow | Lecture |
|---|---:|---:|---|
| A1 — faux applies | **39** | **4** | 32 faux de 363 + 3 inserts 66 Minutes traités |
| A1 — applies valides | **5 900** | **5 897** | 3 applies rétrogradés, dont 2 punch-ins Piquantes |
| A1 — revues ajoutées | — | **38** | 35 natifs + 3 prior 66 Minutes |
| A2 — revues | **924** | **780** | 131 noops, 13 promotions apply |
| A2 — actions humaines perdues | — | **14** | 14 noops recoupent `a2_missed` |
| A7 — revues récupérées | **38 / 103** | **3 récupérées (2 apply + 1 noop)** | fenêtre 30 j ; 35 revues hors fenêtre |
| A7 — nouveaux candidats | — | **1** | 1 apply ; 312 candidats hors fenêtre |

Le shadow complet (1 203 projets, 93 segments A1 ambigus, 924 revues A2,
171 actions A7 + 1 candidat dans la fenêtre) est détaillé dans
`benchmark_corpus4/out/shadow/SHADOW-RUN-CORPUS4.md`. Il valide le natif sur
la bande 950–990 px et le fallback A7 **avec une borne basse de 30 jours**, mais
**invalide** le garde-fou statique 900–1040 px, le noop systématique des
`logo étroit` et le fallback A7 sans borne basse (35 revues et 312 candidats
reposaient sur des missions de plus de 30 jours).

## 11. Décision attendue

Le shadow run est exécuté. La décision porte désormais sur l'implémentation :

1. **Implémenter les P0 derrière flags (recommandé)** : A1 natif ambigu +
   prior 66 Minutes, A7 fallback global avec fenêtre 30 jours, A2 triage
   conservateur. Les flags restent désactivés par défaut jusqu'au contrôle des
   4 faux applies A1 résiduels et des 4 récupérations A7 plausibles.
2. **Poursuivre le shadow** sur les 4 faux applies A1 résiduels et les 4
   récupérations A7 dans la fenêtre avant toute implémentation.
3. **Laisser les flags désactivés** : l'implémentation et les tests sont
   livrés, mais aucune activation en production n'est faite.

La spécification d'implémentation détaillée est dans
`benchmark_corpus4/out/SPEC-RUST-CORPUS4.md` (copie :
`workflow-rs/docs/etudes/SPEC-RUST-CORPUS4-POST-STEP8.md`).

---


*Rapport généré automatiquement à partir des sorties de `benchmark_corpus4/out/`. Les chiffres sont arrondis ; les CSV conservent la précision complète.*
