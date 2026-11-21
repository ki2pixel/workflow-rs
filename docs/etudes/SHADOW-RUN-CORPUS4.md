# Shadow run Corpus 4 — corrections P0 sur 1 203 projets

> **Statut** : shadow run exécuté en lecture seule, sans réencodage vidéo et
> sans modification du code de production.
> **Date** : 21 novembre 2026.
> **Script** : `benchmark_corpus4/scripts/shadow-corpus4.py`.
> **Sorties** : `benchmark_corpus4/out/shadow/`.

## 1. Périmètre

| Élément | Valeur |
|---|---:|
| Projets parcourus | **1 203** |
| Applies A1 simulés | **5 939** |
| Segments A1 ambigus (950–990 px) | **93** |
| Revues A2 simulées | **924** |
| Actions A7 existantes | **171** |
| Projets sans A7 avec mission retrouvée | **1** |
| Projets sans mission | **719** |

Le contrôle natif A1 est un **proxy luminance/gradient** : il décode 3 frames
par segment ambigu, mesure la couverture du front et le saut de luminance aux
barres. Il a été validé sur les deux cas extrêmes du corpus :

- `363 Camille / OCLC_DION_AFIDA_DION` : 32/32 faux applies rejetés ;
- `361 Camille / TEVA_Incroyable embouteillage` : 58/58 applies valides confirmés.

Ce n'est pas le détecteur Rust `native_letterbox` ; le shadow valide le principe,
pas l'implémentation exacte.

## 2. A1 — letterbox ambigu

| Indicateur | Avant | Après shadow |
|---|---:|---:|
| Applies A1 | 5 939 | **5 901** |
| Faux applies | **39** | **4** |
| Applies valides | **5 900** | **5 897** |
| Revues A1 ajoutées | — | **38** |

- **58** segments ambigus confirmés par le natif (tous les applies de 361).
- **35** rejetés : **32 faux** de 363 + **3 applies valides** rétrogradés.
- **3** faux applies de `66 Minutes` traités par le prior d'émission
  (`66 Minutes` -> `review` sur tout apply A1).
- Les **4** faux applies restants sont documentés ci-dessous.

**Faux applies restants (4)** :

| Dossier | Programme | Projet | Segment | Active | Cible | Humain |
|---|---|---|---|---|---|---|
| 25 Camille | Enquête d'action | EQA 2026 01 16 MOTARDS PS1 SUJET 1 EXTRAIT | Segment 6 (00:00:14 360 - 00:00:18 920) | 848 | 229.8 | 180.0 |
| 34 Camille | Non typé | BA DIM PA | Segment 6 (00:00:04 480 - 00:00:05 360) | 1056 | 184.5 | 180.0 |
| 83 Camille | la revue de presse | PP - la revue de presse - Giroud et stotz en vendeur Co | Segment 13 (00:01:19 480 - 00:01:21 600) | 1056 | 184.5 | 180.0 |
| 280 Camille | Les Marseillais | Luna s’énerve contre Paga et Greg à cause de leurs comp | Segment 31 (00:02:06 480 - 00:02:08 160) | 1008 | 193.3 | 180.0 |

**Applies valides rétrogradés (3)** :

| Dossier | Programme | Projet | Segment | Active | Cible | Humain |
|---|---|---|---|---|---|---|
| 149 Camille | Piquantes ! | PP - Piquantes - Sa déclaration émouvante à sa fille | Segment 10 (00:00:59 640 - 00:01:06 920) | 976 | 199.7 | 214.0741 |
| 149 Camille | Piquantes ! | PP - Piquantes - Sa déclaration émouvante à sa fille | Segment 14 (00:02:17 200 - 00:02:21 360) | 976 | 199.7 | 210.7407 |
| 363 Camille | Non typé | OCLC_DION_AFIDA_DION | Segment 30 (00:00:51 320 - 00:00:52 760) | 976 | 199.7 | 237.4074 |

Deux des trois applies perdus sont des punch-ins sémantiques de `Piquantes`
(échelles humaines 210–214 %), pas des letterbox ; le troisième est le segment
30 de 363 (zoom humain 237 %). Le coût éditorial réel est donc plus faible que
le compte brut.

**Lecture** : le shadow valide la recommandation « natif sur la bande ambiguë »
et **invalide** le garde-fou statique 900–1040 (qui aurait rétrogradé 61
applies valides). Le prior `66 Minutes` est un gain séparé, sans perte.

## 3. A2 — triage des revues

Deux variantes ont été testées :

| Variante | Noops | Revues restantes | Actions humaines perdues |
|---|---:|---:|---:|
| Agressive (premier jet) | 442 | 482 | **61** |
| Conservative (retenue) | **131** | **780** | **14** |

La variante agressive a été **rejetée** : elle transformait en noop 61 revues
qui avaient une action humaine, dont 61 candidates `a2_missed`. La variante
conservative ne noope que les `décor bas` dont la cible est hors de la plage
190–300 %, et ne noope les `logo étroit` que si l'OCR est explicitement
`logo_only`.

| Indicateur | Valeur |
|---|---:|
| Revues A2 avant | 924 |
| Noops simulés | **131** |
| Revues promues en apply | **13** |
| Revues restantes | **780** |
| Noops avec action humaine | **14** |
| Noops présents dans `a2_missed` | **14** |
| Promotions confirmées par `a2_missed` | **12 / 13** |

**Noops avec action humaine (14) — cas à surveiller** :

| Dossier | Programme | Projet | Type | Cible | Hauteur | Largeur | Persistance |
|---|---|---|---|---|---|---|---|
| 46 Camille | L'Île de la tentation | Extrait en direct – Nelly fait une déclaration à DIego | décor bas | 1571.6 | 956.0 | 1.0 | 0.89 |
| 96 Camille | Ma Super Ligue 1 | Gulli - Ma Super Ligue 1 - Pierre Sage doit deviner le  | décor bas | 2706.7 | 1008.0 | 1.0 | 1.0 |
| 110 Camille | L'Amour est dans le pré | MAPR - Elle ne se projette pas lors de ses essayages de | décor bas | 1282.1 | 928.0 | 1.0 | 1.0 |
| 120 Camille | L'Amour est dans le pré | M6 - MAPR - Le marié perd ses mots en répondant à la ma | décor bas | 435.0 | 632.0 | 0.442 | 0.952 |
| 159 Camille | L'Île de la tentation | Extrait 1 Elle confronte son mec après 4 jours dans l'a | décor bas | 1571.6 | 956.0 | 1.0 | 0.89 |
| 159 Camille | L'Île de la tentation | Extrait 6 Une danse sexy fait craquer Tino | décor bas | 1571.6 | 956.0 | 1.0 | 0.859 |
| 184 Camille | L'Amour est dans le pré | M6 - MAPR - Perrine est très agacée par l’humour d’Alex | décor bas | 1282.1 | 928.0 | 1.0 | 1.0 |
| 188 Camille | Le tatoué | declaration | décor bas | 553.6 | 728.0 | 0.835 | 1.0 |
| 196 Camille | L'Île de la tentation | Elle fait un câlin à son tentateur | décor bas | 329.2 | 488.0 | 1.0 | 1.0 |
| 227 Camille | MSL1 | Gulli - MSL1 - La surprise d’E.Mbappé et A.Bouaddi | décor bas | 3045.0 | 1016.0 | 0.923 | 0.994 |
| 233 Camille | L'Amour est dans le pré | M6 - MAPR - Lucile appréhende la relation fusionnelle e | décor bas | 1282.1 | 928.0 | 0.946 | 0.975 |
| 243 Camille | M6 - Romance | Elle pleure car elle ne veut blesser aucun des garçons | décor bas | 1353.3 | 936.0 | 0.938 | 0.941 |
| 258 Camille | Raymond Devos | PP - Raymond Devos - L'art d'utiliser les mots | décor bas | 316.4 | 464.0 | 0.535 | 0.951 |
| 356 Camille | UDPP | Gulli - UDPP - Son entrée est vivement critiquée | décor bas | 2564.2 | 1004.0 | 0.806 | 0.943 |

**Revues A2 promues en apply (13)** :

| Dossier | Programme | Projet | Cible | Hauteur | Action humaine |
|---|---|---|---|---|---|
| 31 Camille | Un Jour, un Doc | UJUD - COMMANDE SHEIN | 199.7 | 104.0 | oui |
| 31 Camille | Un Jour, un Doc | UJUD - GARDE ROBE SOLENE | 194.1 | 76.0 | oui |
| 97 Camille | Un Jour, un Doc | UJUD - DEMANDE EN MARIAGE_TAT_M6 | 194.1 | 76.0 | oui |
| 149 Camille | Piquantes ! | PP - Piquantes - Sa déclaration émouvante à sa fille | 203.8 | 124.0 | non |
| 182 Camille | Un Jour, un Doc | UJUD - REPARATION LAMPE | 194.1 | 76.0 | oui |
| 189 Camille | PP - Postdiff/Prédiff | Post diff 1 Course-poursuite dans sa fuite il manque de | 196.5 | 88.0 | oui |
| 189 Camille | PP - Postdiff/Prédiff | Post diff 2 Ces policiers sont pris à partie lors d’une | 196.5 | 88.0 | oui |
| 220 Camille | Un Jour, un Doc | UJUD - LAMANTIN_TAT_M6 | 199.7 | 104.0 | oui |
| 242 Camille | Non typé | extrait 3 - soirée pour célibataires en surpoids | 191.1 | 60.0 | oui |
| 254 Camille | Un Jour, un Doc | UJUD - SHOPPING | 194.1 | 76.0 | oui |
| 278 Camille | Couple libre | COUPLE LIBRE | 188.1 | 44.0 | oui |
| 293 Camille | PP - Postdiff/Prédiff | POSTDIFF 2 | 196.5 | 88.0 | oui |
| 297 Camille | PP - Postdiff/Prédiff | post diff 2 | 196.5 | 88.0 | oui |

**Lecture** : le triage conservateur retire **131 revues
(14,2 %)** avec **14 actions humaines perdues**.
Les 13 applies `bande sombre` sont confirmés par l'OCR et 12/13 par une action
humaine. `logo étroit` doit rester en revue : le noop y perd 30 actions sur 201.
Le gain est donc plus modeste que l'estimation initiale de 30–50 %, mais il est
mesuré et contrôlé.

## 4. A7 — fallback global sous fenêtre temporelle

Le premier passage du shadow appliquait seulement un cutoff haut
(`génération + 1 j`). La validation a montré que ce cutoff laissait passer des
missions de janvier à juin 2026 : **299 des 313 « nouveaux candidats »**
correspondaient à des missions de plus de 90 jours, donc à des titres réutilisés.
Le fallback retenu ajoute une **borne basse de 30 jours** entre la mission et la
génération du projet.

| Indicateur | Valeur |
|---|---:|
| Actions A7 existantes | **171** |
| Revues `video_not_in_mission` récupérées (≤ 30 j) | **3** |
| dont applies | **2** |
| dont noops `no_logo` | **1** |
| Revues hors fenêtre, conservées en revue | **35** |
| Nouveaux candidats (projets sans A7, ≤ 30 j) | **1** |
| dont applies potentiels | **1** |
| Nouveaux candidats hors fenêtre, ignorés | **312** |
| Projets sans mission | 719 |

**Récupérations dans la fenêtre** :

| Dossier | Projet | Action | Âge mission (j) | Logos |
|---|---|---|---|---|
| 278 Camille | COUPLE LIBRE | noop | 3 | — |
| 327 Camille | Maeva fait une grosse déclaration d’amour à Greg | apply | -1 | les marseillais.png |
| 379 Camille | Booder n'a pas oublié ses profs… et leurs petites pique | apply | 1 | elodie poux.png |
| 116 Camille | M6 - LMF - Tout se joue sur cette question_TAT_M6 | apply | 9 | lmf.png |

**Ancienneté des candidatures hors fenêtre** :

| Tranche d'âge | Candidats |
|---|---|
| > 180 j | 214 |
| 91–180 j | 121 |
| 31–90 j | 12 |

**Lecture** : le fallback global corrige bien les 3 revues plausibles
(`COUPLE LIBRE` noop, `Maeva…` et `Booder…` applies) et un nouveau candidat
(`LMF - Tout se joue…`). En revanche, **il ne valide pas** les 38/38 et 313 du
premier passage : 35 revues et 312 candidats supplémentaires reposaient sur des
missions trop anciennes. Sans borne basse, le fallback aurait appliqué des logos
de janvier à des projets de novembre.

## 5. Critères d'acceptation

| Critère | Cible | Résultat |
|---|---:|---:|
| A1 — faux applies de 363 supprimés | ≥ 30 / 32 | **32 / 32** |
| A1 — applies valides de 361 conservés | ≥ 55 / 58 | **58 / 58** |
| A1 — faux applies totaux | ≤ 7 | **4** |
| A7 — récupérations plausibles (≤ 30 j) | ≥ 2 | **3** |
| A7 — aucune mission > 30 j appliquée | 0 | **0** |
| A2 — actions humaines perdues | ≤ 15 | **14** |
| A2 — applies confirmés | ≥ 12 / 13 | **12 / 13** |

## 6. Conclusion et suite

Le shadow run **valide** :

1. le garde-fou natif A1 sur la bande 950–990 px ;
2. le prior `66 Minutes` pour les inserts letterboxés ;
3. le triage A2 conservateur, centré sur `décor bas` hors plage 190–300 % ;
4. le fallback A7 **uniquement avec une fenêtre basse** de 30 jours.

Il **invalide** :

- le garde-fou statique A1 900–1040 px (61 applies valides menacés) ;
- le noop systématique des `logo étroit` (30 actions humaines perdues) ;
- la réduction A2 de 30–50 % (le gain mesuré est de 14,2 %) ;
- le fallback A7 sans borne basse : **35 des 38 revues** et **312 des 313
  candidats** correspondaient à des missions de plus de 30 jours, parfois de
  janvier 2026.

La suite recommandée est l'implémentation des flags P0 derrière des options
désactivées par défaut. Le flag `a7_global_fallback` doit conserver la fenêtre
`[génération − 30 j, génération + 1 j]` ; le détecteur Rust exact doit être
rejoué sur les 93 segments A1 ambigus avant activation.

---

*Shadow run généré à partir de `benchmark_corpus4/out/shadow/`. Les décisions
A1 natives sont un proxy ; le détecteur Rust exact doit être rejoué sur les
93 segments ambigus avant activation.*
