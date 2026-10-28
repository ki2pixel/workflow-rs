# Journal des décisions

Ce document enregistre les décisions architecturales et techniques durables de
workflow-rs. Les écarts de comportement legacy sont détaillés dans
`parity/DRIFT.md`.

> **Politique de conservation** : ce fichier conserve les décisions depuis le
> début de la migration Rust (2026-10-22). Les décisions legacy antérieures
> restent dans `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`.

## 2026-10-26 — D-028 : abandon du client desktop Tauri

- **Contexte** : le PoC Tauri A-Strict (J1 à J5) puis le MVP J1/J2 ont ajouté
  `workflow-desktop-services`, `workflow-gui-tauri`, un paquet `.deb`
  `workflow-manager`, une unité systemd utilisateur et des scripts de soak.
  Le client devait fournir une fenêtre native, un tray D-Bus et un polling
  1 Hz du serveur headless. L'usage réel a montré une instabilité du client et
  un coût de maintenance disproportionné par rapport au besoin.
- **Décision** : abandonner le client desktop Tauri et revenir à la stack
  headless Rust (`workflow-server` `:5004`, `workflow-cli`) avec l'interface
  web legacy `assets/web`. Retirer les crates desktop, les dépendances
  GTK/WebKit/Tauri, le paquet système, l'unité systemd, les scripts PoC et la
  documentation desktop.
- **Motifs** :
  1. instabilité constatée à l'usage, non couverte par les soaks (tray,
     minimisation, focus X11, changements de page) ;
  2. croissance `WebKitNetworkProcess` mesurée en J4 (+22,1 Mio) et correction
     par suspension du polling, avec un cache et un cycle de vie spécifiques
     à la WebView ;
  3. surface fonctionnelle inutile (tray D-Bus, single-instance, focus X11,
     `.deb`, systemd utilisateur, cache CUDA) pour un client qui affiche la
     page servie par Rust ;
  4. seconde toolchain Tauri 2/GTK3/WebKitGTK 4.1/zbus/x11rb/notify-rust/
     sysinfo et MSRV 1.90 contre 1.86 pour le pipeline ;
  5. duplication de `assets/web`, qui couvre déjà le besoin métier ;
  6. surface de sécurité et de packaging hors périmètre du pipeline.
- **Périmètre retiré** : crates `workflow-desktop-services` et
  `workflow-gui-tauri`, entrées Cargo associées, `workflow-manager 0.1.0`,
  `/usr/bin/workflow-gui-tauri`, `workflow-server.service`,
  `packaging/systemd/`, `scripts/install-systemd-user.sh`, `scripts/poc/`,
  `docs/desktop/`, études et audits GUI des 2026-10-25/26, résidus
  `~/.local/share/org.workflow.desktop` et
  `~/.local/state/workflow-manager`.
- **Conservé** : STEP1 à STEP8, CLI, serveur, `assets/web`, contrats
  JSON/CSV/HTTP, tolérance `1e-6`, `runtime/`, `/mnt/cache`, dépôt Python en
  lecture seule, secrets opérationnels, cache CUDA
  `~/.cache/workflow-rs/cuda-compute` et lanceurs pré-Tauri.
- **Validation** : dépôt ramené à
  `a93a554c4c5ed5e17805575cf15ac533d7cd50a3` ; 280 tests au rollback strict,
  282 après correctifs, 0 échec ; fmt/clippy verts ; `config check` valide ;
  binaires release reconstruits ; serveur relancé seul sur `:5004` via
  `scripts/switch-rust.sh`, Python `:5003` arrêté.
- **Correctifs réappliqués** : `514e2d6` (ETXTBSY), `4ddb91b` (secrets,
  281 tests) et `713b6bc` (polling web masqué et Cache-Control, 282 tests).
  `687eaf9`/`1613b17` (réorg docs) restent dans la branche de sauvegarde.
- **Sauvegarde** : branche `backup/tauri-abandon-20261026` sur `1613b17`.
- **Référence** : `docs/decisions/DECISION-ABANDON-TAURI.md`.

## 2026-10-26 — D-029 : pas de STEP9 agentique en production, consolidation déterministe d'abord

- **Contexte** : étude d'opportunité demandée sur l'automatisation
  post-STEP8 (recadrage 9:16, nettoyage éditorial, habillage, rendu) avec
  EffectCraft 0.17. L'étude confronte le guide de recadrage, le code
  STEP5/6/8, le panel `MediaSolution.jsx`, les capacités MCP d'EffectCraft et
  les projets témoins `298 Camille_GOOD.zip` / `300 Camille_GOOD.zip`.
- **Constats mesurés** (détail dans `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`) :
  1. sur 117 segments comparables, l'ancre horizontale STEP6 n'a jamais été
     modifiée (delta max 0,00 px) ; les keyframes humaines déplacent la
     Position, pas l'Anchor Point ;
  2. 23 calques sur 120 seulement sont keyframés, avec 72 clés au total ;
     20/23 calques sont dans la fourchette 2–6 du guide ;
  3. sur 300, l'échelle passe de 180 % à 207 % sur 92 segments pour sortir une
     bannière basse permanente ; deux zooms locaux à 254,7 % et 287,6 %
     traitent un sous-titre et une archive 4:3 ;
  4. 1 fusion et 3 splits de transitions sur 120 segments ;
  5. 298 ajoute 2 logos et 1 jingle de 3 s, et allonge la composition de 3 s ;
  6. les deux exports finaux mesurés sont en 1080×1920, 25 fps,
     10,01–10,27 Mbit/s ;
  7. le rendu EffectCraft d'un projet de 65,8 s prend 72,3 s sur la GTX 1650,
     à 10,29 Mbit/s : le rendu n'est pas le goulot.
- **Décision** :
  1. **Ne pas implémenter de STEP9 agentique en production maintenant.**
     Aucun agent n'écrit dans les `.ecproj` de production.
  2. **Consolider d'abord le déterministe** dans un module post-STEP8 isolé
     (spike) : A0 profilage (`video_profile.json`), A1 détection letterbox
     (`cropdetect`), A2 détection des overlays/sous-titres source, A3
     keyframes 2–6 issues du tracking, A4 splits/merges candidats, A5
     rapport/QC. **Hors périmètre A, par décision** : relink automatique
     (contrôle optionnel), placement des logos/jingle, extension de
     composition et rendu H.264. Ces gestes restent humains (directives
     client, export GPU externe, validation).
  3. **Ajouter un artefact additif** (`_post.json` ou équivalent) plutôt que
     modifier `_ae.json`, `_audio.json`, le CSV ou l'ordre des clés. Le panel
     `MediaSolution.jsx` reste le chemin existant ; le post-traitement est
     séparé et optionnel.
  4. **Seul usage agentique autorisé à court terme** : un agent QA read-only
     (scénario B), local-first, qui inspecte des frames clés et produit un
     rapport `audit.json` sans modifier le projet. Cloud uniquement opt-in
     par projet, avec DPA et politique de rétention ; aucune dépendance
     réseau bloquante dans les 8 étapes.
  5. **Scénario C (co-monteur en boucle fermée)** : laboratoire uniquement,
     sur copies, avec approbation humaine, max 3 itérations par segment,
     deltas bornés, `batch` atomique et `undo`. Aucune livraison automatique.
- **Alternatives rejetées** :
  - **Remplacer STEP6 par un VLM** : le corpus montre que l'ancre STEP6 est
    déjà stable et pertinente ; le problème est temporel (keyframes), pas
    l'ancre.
  - **Agent co-monteur immédiat** : non-déterminisme, sur-correction,
    latence, coût et absence de corpus d'évaluation.
  - **Cloud-first** : rupture de la promesse offline et du confinement des
    rushes ; risque juridique et dépendance réseau.
  - **Mutation directe des `.ecproj` en JSON** : risque de casser les
    timebases, les parentés et les contrats ; passer par MCP/`script`.
  - **Ajouter un 9e step au serveur avant validation** : le spike doit
    rester hors catalogue et hors serveur.
- **Conséquences** :
  - `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` et `docs/etudes/SPIKE-STEP9-PROTOCOLE.md`
    documentent l'analyse et le protocole ;
  - aucun code de production n'est modifié par cette décision ;
  - le spike doit mesurer A0–A5 (couverture cadrage/nettoyage ≥ 70 %, hash
    stable, zéro faux positif sur les contrôles propres), B (précision ≥ 95 %,
    rappel ≥ 90 %) et C (préférence ≥ 60 %) avant toute intégration ;
  - si A échoue, ne pas passer à B ; si B échoue, ne pas passer à C.
- **Références** : `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`,
  `docs/etudes/SPIKE-STEP9-PROTOCOLE.md`, `docs/lots/LOT-1-STEP6.md`,
  `docs/lots/LOT-1-STEP8.md`, `memory-bank/activeContext.md`.

## 2026-10-27 — D-030 : A3 en review, A1/A2 durcis, template SDM séparé

- **Contexte** : le corpus STEP9 a été porté à 19 paires / 32 vidéos
  exploitables (1059 segments bruts, 520 clés humaines), avec des paires
  « sans piège », letterbox, overlays, cas complexe, mixte et template SDM.
  Les mesures du 2026-10-27 invalident l'application automatique de A3 et
  précisent les limites de A1/A2.
- **Constats** :
  1. A3 (amplitude du centroïde > 100 px, trous < 30 %, durée ≥ 1 s) a une
     précision de 0,20 et un rappel de 0,23 sur 1059 segments ; sur les
     vidéos « sans piège », elle ajoute 35 segments keyframés pour 1 vrai
     positif. Le mouvement du centroïde ne prédit pas la décision humaine.
  2. A1 est validé sur le letterbox noir : 191 (800 px actifs, barres
     152/128) → 249,6 % avec marge 4 % vs 255 % humain (2,1 %), 255 % avec
     une marge de 6,25 % ; 268 alterne 179,1/222,2 % par
     segment. Les bords non noirs de 268 (8 segments scalés à 204–224 %)
     ne sont pas vus par `cropdetect`.
  3. A2 reste fragile : 207,4 % correct sur 187 DISNEYWORLD, mais faux
     positif à 260 % sur 187 ECOLE (zone statique de décor) ; les logos de
     coin (191) et incrustés (295) ne sont pas des bannières.
  4. 40 % des segments statiques sont à plus de 100 px du cadrage brut, et
     même 298/300 en comptent 49/96. Le re-cadrage est sémantique (sujet
     choisi par le monteur, personne floutée qui parle, dos, groupe).
  5. Le cas 295 (SDM) est un template de composition à trois panneaux, pas
     un geste de cadrage.
  6. Le scale A1 seul ne garantit pas la couverture : sur 191, 249,6 %
     supprime la barre du bas mais laisse 19,5 % de barre en haut, car
     l'ancre du sujet est décentrée. Une compensation de Position Y de
     -417,8 px supprime les barres ; le cadrage vertical reste à valider.
- **Décision** :
  1. **A3 n'est plus auto-appliqué.** Il produit des candidats dans
     `review.json` ou s'active par flag explicite par projet, après validation
     humaine. La spécification des keyframes reste celle du protocole.
  2. **A1 auto** pour le letterbox noir, avec le **maximum** de letterbox par
     segment, un **contrôle de couverture** par frames et une compensation de
     Position Y si l'ancre est décentrée ; les bords non noirs et les cas
     ambigus vont en `review`.
  3. **A2 auto limité à la bannière claire pleine largeur** (largeur > 50 %,
     hauteur 24–200 px, persistance > 60 %). Validé sur 187
     DISNEYWORLD/ECOLE (206,6 % vs 207 %) ; les logos de coin, les zones
     sombres/animées et les bannières en haut restent en `review` tant que le
     classifieur n'est pas durci.
  4. **A4 auto** pour les gestes mécaniques (fusion < 0,5 s, cut dur manqué) ;
     les splits sémantiques restent en `review`.
  5. **A6 template SDM** est validé sur le GOOD 295 reçu le 2026-10-27 :
     brut et GOOD partagent le même `_ae.json`/`_tracking.json` (SHA-256
     identiques). Le template est un fond 180 % flouté 50, un panneau
     84,3497774862575 % à y=171,032, un panneau 56,294573830734535 % à
     y=1082,026 et le logo `sdm.png`. Le prototype `a6_template.py` reproduit
     le rendu à **0/255** (exact, 0 pixel > 2) à 0, 10, 30 et 50 s. Le
     déclencheur est une règle de nommage **configurée** (`M6 - SDM - ` par
     défaut) ou un mapping projet, suivie d'un contrôle structurel
     (mono-segment, sans keyframe, comp 1080×1920, source 1920×1080) ; sinon
     `review`. Le wrapper `a6_apply.py` écrit `post.json`/`review.json`/
     `audit.json` et le projet transformé ; l'intégration au rapport A5 et
     les validations complémentaires sont en **D-031**.
  6. Le delta de baseline vient de **STEP4/5 (tracking legacy vs Rust)**, pas
     de STEP6/8. Le STEP6 Rust alimenté par le `_tracking.json` legacy
     reproduit le `_GOOD/_ae.json` à l'identique (4355/4355 frames sur 187,
     2979/2979 sur 181). La bascule est datée : le commit legacy `0e9765b` du
     **2026-10-16 19:01** (`STEP4_TPU_FAST=1`, `object_min_missing=25`) est
     postérieur aux GOOD. Le Rust reproduit le mode *fast* actuel. Un run
     `--object-min-missing 1` remonte la couverture personne à 1722 frames
     contre 1623 legacy, mais ne ramène pas les ancres (delta médian 64,33 px
     contre 8,39 px avec le tracking legacy) : le gate explique la couverture,
     pas la sélection d'objet. **Portée** : cette recette est un outil de
     diagnostic historique, pas la méthode du corpus. La référence de
     production est **STEP1–8 Rust** ; les GOOD 181/187 sont des références
     historiques et ne doivent pas être mélangés aux bruts Rust. La parité
     STEP4/5 legacy/Rust est un chantier externe, hors-scope STEP9 ; le
     tracking Rust est la référence de production, et `MediaSolution.jsx`
     arbitre le recentrage.
  7. Le **clamp du panel** (300..1620 px) est correct pour une position
     centrée : `anchor ∈ [C/(2s), W - C/(2s)]`. Les ancres hors plage des GOOD
     sont des déplacements manuels de `Position` qui restent couverts par
     l'intervalle généralisé `[p/s, W - (C-p)/s]` ; un seul cas (181 segment
     34) laisse un vide de 36,7 px, à signaler en `review`. Ce n'est pas un
     bug à corriger dans le panel.
- **Conséquences** :
  - G1 est révisé : couverture ≥ 70 % des **gestes mécaniques** seulement,
    A3 en review, pas de revendication de remplacement du cadrage humain ;
  - `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` §3.6 et
    `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` §4.6/§8.1 sont mis à jour ;
  - aucun code de production n'est modifié ; les prototypes restent sous
    `/tmp/opencode/step9-spike/` ;
  - prochaine étape : finaliser le contrôle de couverture A1 (seuil de
    compensation), durcir A2 (bannière sombre/animée, logo de coin), puis
    intégrer A6 comme module opt-in avec le profil mesuré ; 179 reste sans
    GOOD, 287 n'a qu'une référence sur trois.
- **Références** : `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`,
  `docs/etudes/SPIKE-STEP9-PROTOCOLE.md`, `06-corpus/RAPPORT-CORPUS.md` (spike),
  `memory-bank/activeContext.md`.

## 2026-10-27 — D-031 : A5 v2, hash échantillonné et A2 géométrique

- **Contexte** : la v1 de A5 laissait trois trous : hash vidéo complet coûteux,
  A1 sans contrôle de couverture systématique, A2 limité à la bande basse
  claire. A6 était validé mais hors du rapport A5.
- **Décision** :
  1. `audit.json` hashe un **échantillon** déterministe de la vidéo (début,
     milieu, fin, 3 × 4 Mio) plus la taille et les offsets ; le hash complet
     n'est plus utilisé en production.
  2. A1 prend le **10e percentile** des hauteurs actives du segment, vérifie
     la couverture mathématique de tous les crops après scale et compensation
     Y, et met en `review` tout segment keyframé ou letterbox intermittent
     (≥ 10 % des crops).
  3. A2 scanne toute la hauteur. Une bande haute plausible, un logo
     (< 35 %), une bande partielle (35–50 %), un décor (> 200 px), une bande
     sombre (paires de bords + médiane < 60) ou une bande dont les bords
     bougent (centre/hauteur > 12 px) partent en `review`. Une bannière qui
     change seulement de texte ou de luminosité reste applicable si sa
     géométrie est stable.
  4. A5 expose `--template sdm` : décision A6 et bloc `template` dans
     `post.json`, sans écrire le projet. `a6_apply.py --apply` reste le seul
     writer du `.ecproj` transformé dans le prototype ; D-032 le remplace en
     production par le module Rust `post`.
  5. `post.json` et `audit.json` portent `timings_ms` par étage.
- **Validation** : 268 (15 A1, 231,1 %, Y −287,9 px), 191 (37 A1, 249,6 %,
  Y −367,8 px enfant), 187 DISNEYWORLD/ECOLE (A2 206,6 %, géométrie 1,3/2,6
  et 0/0 px), 181 Concierge (« sans piège ») 0 action auto, 295 A6 apply.
  Validation synthétique A2 11/11 (3 apply, 7 review, 1 noop). A5 coûte
  3,7–33,5 s par vidéo (0,07–0,24× la durée, sans rendu).
- **Conséquences** : aucun code de production modifié ; A2 reste volontairement
  conservateur sur les cas ambigus ; A6 reste un module post-STEP8 opt-in,
  pas une fonction du panel ; 179/287 restent bloqués par les références
  manquantes.
- **Références** : `06-corpus/a5v2/RAPPORT-A5-V2.md`,
  `docs/etudes/ETUDE-STEP9-AGENTIQUE.md` §3.7/§3.8,
  `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` §5.2/§5.3/§5.6/§5.7/§8.1,
  `memory-bank/activeContext.md`.

## 2026-10-27 — D-032 : post-traitement A5/A6 en production, projet `_assist` toujours publié

- **Contexte** : les prototypes A5 v2 et A6 sont validés sur 268, 187, 191,
  181, 295 et 11 cas A2 synthétiques. Le besoin de production est de rendre
  ces gestes disponibles après chaque STEP8 sans jamais écraser le projet
  brut ni les artefacts STEP4/5/6.
- **Décision** :
  1. Le post-traitement est porté dans `crates/workflow-steps/src/post/` et
     appelé après un STEP8 réussi. Il est aussi exposé par
     `workflow-cli post run`.
  2. Le `.ecproj` brut reste **octet pour octet inchangé**. Un second projet
     `<stem>_assist.ecproj` est **toujours** publié à côté ; sans décision
     applicable, c'est une copie exacte du brut.
  3. `post.json`, `review.json` et `audit.json` sont additifs sous
     `<docs>/post/<projet_assisté>/`. Aucun `_ae.json`, `_audio.json`, CSV,
     ordre de clés ou panneau n'est modifié.
  4. A1/A2 ne déplacent jamais l'Anchor Point ; A3/A4 restent en `review`
     (D-030). Un conflit A1/A2 part en `review`, aucune action n'est
     appliquée.
  5. A6 éligible applique le template SDM et court-circuite A1–A4. Un préfixe
     SDM incohérent part en `review` sans bloquer A1–A4.
  6. L'écriture A6 de production est le module Rust `post`, pas
     `a6_apply.py`. Le prototype Python reste un outil de comparaison, hors
     production.
  7. La configuration est `[steps.step8.post]` ; `--no-post` désactive le
     hook et `post run --report-only` calcule les rapports sans publier de
     projet assisté.
  8. Un échec de post-traitement fait sortir la CLI STEP8 en code non nul,
     afin que la séquence serveur s'arrête au lieu de laisser croire que le
     projet assisté existe.
- **Validation** :
  - 295 : assist identique au JSON de référence, rendu relinké **0/255** à
    0, 10, 30 et 50 s ;
  - 268, 187 DISNEYWORLD, 187 ECOLE, 191 et 181 : décisions A1/A2/A3/A4
    **identiques au prototype** (code, action, cible, raison, détails) ;
  - 270/276/286/287 : 0 `apply`, 7 projets, assist identique au brut, audit
    `pass` ;
  - A2 synthétique : 11/11 (3 `apply`, 7 `review`, 1 `noop`) ;
  - 315 tests, `fmt` et `clippy -D warnings` verts ; idempotence vérifiée sur
    187 ECOLE.
- **Limites** : artefacts non exposés dans l'interface web ou le panneau CEP ;
  A3/A4 en revue ; bords non noirs et recadrages sémantiques en revue ; A6
  multi-segment/keyframé en revue ; le timeout serveur STEP8 couvre aussi le
  post-traitement ; 179 sans GOOD et 287 avec une seule référence sur trois.
- **Références** : `docs/lots/LOT-9-POST-STEP8.md`, `docs/etudes/ETUDE-STEP9-AGENTIQUE.md`
  §3.9, `docs/etudes/SPIKE-STEP9-PROTOCOLE.md` §14.1, `config/config.toml`.

## 2026-10-28 — D-033 : documentation réorganisée en six domaines

- **Contexte** : `docs/` regroupait 23 documents à plat (lots STEP1–STEP8,
  A5/A6, contrat, décision, études STEP9, exploitation, validation) et un
  dossier `audits/`, sans index ni règle de navigation. Les renvois croisés
  étaient dispersés dans 30 fichiers du dépôt. La réorganisation antérieure
  (`687eaf9`/`1613b17`, branche `backup/tauri-abandon-20261026`) n'a jamais
  été fusionnée et utilisait une autre taxonomie.
- **Décision** :
  1. Six domaines : `architecture/` (contrat), `lots/` (bilans par lot et
     `STEP2-WINDOWS.md`), `exploitation/` (sync, bascule, rétention),
     `decisions/` (ADR), `etudes/` (étude et spike STEP9), `validation/`
     (validation de bout en bout et `audits/`).
  2. `docs/README.md` devient l'index unique : taxonomie, statuts, parcours
     par profil, table par étape et correspondance ancien → nouveau.
  3. Les renvois entre documents sont repo-relatifs en backticks
     (`docs/lots/LOT-1-STEP8.md`) ; les liens Markdown relatifs sont réservés
     à l'index. Les chemins applicatifs `<projet>/docs/` et `docs/post/`
     restent inchangés.
  4. Les audits suivent désormais `docs/validation/audits/AAAA-MM-JJ-<sujet>.md`.
  5. Aucun fichier n'est renommé : seuls les déplacements changent les chemins,
     ce qui préserve `git log --follow` et `git blame`.
- **Conséquences** : les fiches de lot quittent la racine pour `docs/lots/`,
  le contrat passe sous `docs/architecture/`, et
  `.agents/workflows/docs-updater.md` compte les documents via
  `find docs -name '*.md'`. `AGENTS.md`, `README.md`, les règles, skills et la
  memory-bank pointent vers l'index.
- **Validation** : 24 fichiers déplacés par `git mv` en deux commits (move pur
  détecté à 100 %, puis réécriture) ; 211 renvois réécrits dans 30 fichiers ;
  zéro ancien chemin hors table de correspondance ; toutes les cibles citées
  existent ; liens de l'index résolus ; 315 tests, `fmt` et `clippy
  -D warnings` verts ; aucun fichier sous `crates/`, `scripts/`, `assets/` ou
  `config/` modifié.
- **Références** : `docs/README.md`, `AGENTS.md`, `README.md`,
  `parity/DRIFT.md`.

## 2026-10-28 — D-034 : bypass direct des vidéos SDM

- **Contexte** : le template SDM (A6) ne consomme que le `.ecproj` brut, la
  vidéo source et `sdm.png`. Les vidéos SDM traversaient pourtant STEP2
  (cuts), STEP3 (audio), STEP4 (tracking) et STEP5/6 (réduction), puis le
  panel `MediaSolution.jsx` reconstruisait un projet que A6 devait à nouveau
  transformer. Sur 340, le projet du panel n'est pas compatible A6 ; le
  bypass évite ces étapes et rend le résultat déterministe.
- **Décision** :
  1. `steps.step8.post.direct` (défaut `false`) active le bypass. Le prédicat
     est conservateur : nom préfixé par `template_prefix`, `.mp4`, H.264,
     1920×1080, SAR 1:1, 25 fps CFR, durée de flux et `nb_frames` valides.
     Un nom SDM qui ne respecte pas le profil repart dans le pipeline normal.
  2. STEP2 à STEP6 ignorent les vidéos valides (`SKIP_TEMPLATE`) **avant**
     le chargement des modèles et le verrou matériel ; les rapports exposent
     `skipped_template`.
  3. STEP7 considère une vidéo SDM valide comme prête en `lenient` ; le mode
     `strict` conserve sa sémantique (scènes + tracking réels). Il synthétise
     le CSV mono-segment depuis `nb_frames` avec le
     format STEP2 (CRLF, frames 1-based incluses, `Timecode Out` = début de
     la dernière frame), l'écrit dans l'archive sous `<stem>_scenes.csv` et
     dans la copie finale sous `<stem>.csv`, sans modifier le projet source
     et sans écraser un CSV existant. `metadata.json` reçoit
     `scenes_origin` (`template`, `step2` ou `null`).
  4. STEP8 construit le brut `<stem>_9x16.ecproj` depuis l'asset embarqué
     `assets/effectcraft/sdm_raw_template.json`, sans `MediaSolution.jsx`,
     puis publie `_assist.ecproj` via A6. Le brut est aussi créé avec
     `--no-post` ; un brut existant n'est jamais écrasé.
  5. Un dossier qui ne contient que des SDM n'appelle pas le panel. Un
     dossier mixte l'appelle avec les noms SDM injectés en minuscules dans
     `videoSkipNames` ; les non-SDM suivent le pipeline normal et leurs
     sorties restent inchangées.
  6. `workflow-cli post run --direct` crée le brut pour un lancement manuel,
     mais jamais en `--report-only`.
  7. `direct` reste `false` dans le code par défaut ; `config/config.toml`
     l'active après validation.
- **Validation** :
  - 295 : CSV STEP7 identique au CSV STEP2 de référence ; chaîne STEP7 →
    STEP8 ; rendu à 0/10/30/50 s **identique au pixel** à la référence Rust
    acceptée (`a6exact`/`prod`) ;
  - 340 : CSV `1,00:00:00.000,00:00:21.200,1,531` ; brut et assist rendus,
    logo et panneaux conformes ;
  - lot mixte 303 + 340 : `.ecproj` et `_assist.ecproj` non-SDM identiques
    octet pour octet à l'ancien pipeline (chemins normalisés), rendu
    identique à 10/60 s ; SDM `apply` A6, audit `pass` ;
  - `cargo test --workspace`, `fmt` et `clippy -D warnings` verts.
- **Limites** : profil statique limité au MP4 H.264 1920×1080 25 fps ; les
  autres formats ou cadences repartent dans le pipeline normal ; A6 continue
  de valider la structure du brut.
- **Références** : `docs/lots/LOT-9-POST-STEP8.md`,
  `crates/workflow-steps/src/template.rs`,
  `crates/workflow-steps/src/post/direct.rs`, `config/config.toml`.

## 2026-10-29 — D-035 : bypass archive-only des vidéos LPHDF

- **Contexte** : la famille récurrente `W9 - LPHDF` utilise un template sans
  découpe géré manuellement. Elle traversait pourtant STEP2–STEP8, y compris
  le panel, pour produire des `.ecproj` inutilisés. Le seul artefact
  nécessaire est le CSV de scènes archivé.
- **Décision** :
  1. `steps.step8.post.archive_only_prefixes` (défaut vide) liste les
     familles sans `.ecproj` ; `direct` reste l'interrupteur maître.
     `config/config.toml` active `["W9 - LPHDF"]`.
  2. STEP2 à STEP6 appliquent le même prédicat conservateur que SDM avant
     modèles et verrou matériel.
  3. STEP7 synthétise le CSV mono-segment pour les deux familles ; l'archive
     reste `<stem>_scenes.csv` et la copie finale `<stem>.csv`. La copie
     `/mnt/cache` et la suppression de source sont conservées.
  4. STEP8 ignore les LPHDF **par leur nom**, sans sonde : aucun `.ecproj`
     n'est créé, même hors profil. Un dossier 100 % LPHDF n'appelle pas le
     panel ; un dossier mixte l'appelle avec `videoSkipNames`.
  5. Le post-traitement ignore les dossiers 100 % LPHDF et les projets
     résiduels dont le stem correspond à une vidéo LPHDF.
  6. Un fichier LPHDF hors profil traverse STEP2–7 normalement pour fournir
     un vrai CSV, puis STEP8 l'ignore quand même.
- **Validation** :
  - 341 : 4 CSV (`5259`, `5293`, `5240`, `5253` frames) archivés et copiés,
    `scenes_origin = "template"`, aucun `projets/`, aucun appel
    `effectcraft-cli` ;
  - 341 + 303 : panel appelé pour la seule vidéo ordinaire, nom LPHDF en
    minuscules dans `videoSkipNames`, projet ordinaire inchangé hors version
    EffectCraft et chemins ;
  - `cargo test --workspace`, `fmt` et `clippy -D warnings` verts.
- **Limites** : préfixe `W9 - LPHDF` sans séparateur final, pour couvrir les
  variantes à double espace ; la frontière évite `W9 - LPHDFX`. Le CSV
  synthétique reste à 25 fps, comme STEP2.
- **Références** : `docs/lots/LOT-9-POST-STEP8.md`,
  `crates/workflow-steps/src/template.rs`,
  `crates/workflow-steps/src/post/direct.rs`, `config/config.toml`.

## 2026-10-24 — Journal serveur Rust enrichi et sans ANSI

- **Contexte** : le lanceur affichait dans le terminal les 40 dernières lignes
  du journal, dont d'anciennes erreurs webhook du 2026-10-23 ; l'opérateur les
  a prises pour des erreurs du batch courant. Le journal serveur était aussi
  plus pauvre que `logs/app.log` du legacy et contenait des séquences ANSI.
- **Décision** :
  1. Désactiver ANSI quand stderr n'est pas un terminal (cas du lanceur), tout
     en conservant la couleur en exécution manuelle ; même règle pour le CLI.
  2. Enrichir `workflow-server.log` : résumé de configuration, requêtes
     mutantes/4xx-5xx avec durée, démarrage/arrêt et heartbeat du moniteur
     webhook, téléchargements planifiés, étapes avec PID et séquences avec
     durée/résultat.
  3. Faire démarrer `tail` après la dernière ligne existante dans
     `start-rust.sh`, pour n'afficher que la session courante.
- **Alternatives rejetées** :
  - Tout passer en `debug` par défaut : trop de bruit toutes les 15 s.
  - Supprimer les anciennes lignes du journal : perte de traçabilité.
- **Conséquences** : `RUST_LOG=debug` reste le levier pour les requêtes GET et
  le détail des passages webhook ; les anciennes lignes ANSI déjà écrites ne
  sont pas réécrites.
- **Validation** : serveur lancé avec stderr redirigé, zéro octet ESC dans le
  nouveau journal, résumé de configuration et téléchargement dry-run visibles.

## 2026-10-24 — Widget système Rust et auto-ouverture des logs

- **Contexte** : un batch STEP1 → STEP8 lancé depuis le frontend Rust a montré
  deux écarts d'interface : le widget Moniteur Système n'affichait pas la GPU,
  et l'overlay de logs s'ouvrait automatiquement à chaque lancement, alors que
  l'opérateur legacy travaillait avec l'auto-ouverture désactivée.
- **Décision** :
  1. `/api/system_monitor` lit la première carte NVIDIA avec `nvidia-smi` et
     expose le contrat GPU existant (`name`, `utilization_percent`, `memory`,
     `temperature_c`), avec `null` si l'outil est absent et
     `{"error": "GPU data not available"}` en cas d'échec.
  2. La copie frontend Rust passe `ui.autoOpenLogOverlay` à `false` par
     défaut ; le toggle « Auto-ouverture des logs » reste disponible et la
     valeur persistée dans `localStorage` reste prioritaire.
- **Alternatives rejetées** :
  - Continuer à router `/api/system_monitor` vers Python : le serveur Rust
    direct `:5004` affichait CPU/RAM sans GPU et dépendait d'un second
    processus pour une métrique machine.
  - Supprimer le toggle : l'opérateur doit pouvoir réactiver l'auto-ouverture.
- **Conséquences** : `/api/system_monitor` peut être routé vers le backend
  actif (voir `docs/exploitation/SWITCH-PUBLIC.md`) ; la divergence de valeur par défaut
  est consignée dans `systemPatterns.md`.
- **Validation** : endpoint réel sur GTX 1650 (30 %, 0,41/4,0 Go, 44 °C),
  tests serveur GPU, page rendue avec la case décochée.

## 2026-10-24 — Audit croisé du rapport Gemini Web

- **Contexte** : trois rapports d'audit Gemini Web (retirés du dépôt après
  synthèse), produits à partir d'un export Repomix partiellement tronqué.
- **Décision** :
  1. Ne retenir un constat Gemini qu'après reproduction à HEAD ; ne pas
     importer ses sévérités telles quelles.
  2. Retenir cinq nouveaux constats : C9 (arrondi STEP3, P1), R7 (RAR en RAM,
     P1), S9 (secrets d'export, P2), R8 (allocations/regex, P2) et R9
     (duplication, P2).
  3. Enrichir S2 (verrou par défaut et CLI sans `validate_isolation`), A4,
     R1, R6, S7 et C4.
  4. Requalifier SEC-01 et STP1-01 en P2 ; rejeter AUD-06 (déjà corrigé) ;
     classer AUD-01 et AUD-07 hors périmètre.
- **Alternatives rejetées** :
  - Reprendre le P0 SEC-01 : le fichier est ignoré, en mode 600 et le serveur
    est loopback ; la rotation reste une action d'hygiène.
  - Corriger STP1-01 comme une traversée : sous Linux, les antislashs restent
    un nom de fichier littéral.
- **Conséquences** : le rapport passe à 36 constats (1 P0, 6 P1, 29 P2) ; la
  matrice de priorisation est corrigée (l'ancien total de 23 P2 était
  arithmétiquement faux). Aucun code n'est modifié.
- **Validation** : revérification par grep/lecture à HEAD `ae2098d`, simulation
  numérique Python vs formule Rust pour C9, contrôle des fixtures
  `vox_step3`, du frontend et de `parity/README.md`.

## 2026-10-24 — Audit backend Rust

- **Contexte** : audit statique et opérationnel demandé sur les neuf crates,
  sans exécution matérielle ni modification des données de production.
- **Décision** :
  1. Consigner le rapport dans
     `docs/validation/audits/2026-10-24-audit-backend-rust.md` ; les audits futurs suivent
     la même convention `docs/validation/audits/AAAA-MM-JJ-<sujet>.md`.
  2. Classer les constats en P0 (bloquant), P1 (majeur) et P2 (qualité), avec
     preuve `fichier:ligne` et test de non-régression attendu.
  3. Traiter en priorité la course `stop` → `reserve` / `finish` (A1), qui
     autorise deux étapes simultanées et peut écraser l'état de la nouvelle
     exécution.
- **Alternatives rejetées** :
  - Corriger directement pendant l'audit : mélange diagnostic et remédiation,
    et empêche de mesurer l'état initial.
  - Documenter uniquement dans la conversation : perte de traçabilité.
- **Conséquences** : les correctifs P0/P1 (sérialisation générationnelle,
  arrêt propre des sous-processus, portée du verrou, isolation/secret) devront
  être consignés dans ce journal au moment de leur implémentation. Le rapport
  ne modifie ni `runtime/`, ni `/mnt/cache`, ni le dépôt Python legacy.
- **Validation** : `cargo check`, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace` (233/233 à l'audit), `cargo tree -d`.

## 2026-10-24 — Commandes de parité et état de production

- **Contexte** : les commandes du harnais citaient `coral_env/bin/python` depuis
  la racine du dépôt Rust, où ce venv n'existe pas. Le README présentait aussi
  Python comme le workflow legacy actif, alors que Rust `:5004` est le backend
  de production depuis le 2026-10-23.
- **Décision** :
  1. Les commandes de parité du dépôt Rust utilisent
     `../workflow_mediapipe/coral_env/bin/python`.
  2. Les documents d'état (`README.md`, `CONTRAT-ENVIRONNEMENT.md`,
     `SWITCH-PUBLIC.md`) présentent Rust comme backend de production et Python
     comme référence de parité et rollback.
- **Conséquence** : ne pas recréer `coral_env/` dans ce dépôt ; toute nouvelle
  commande de parité doit pointer vers le venv legacy.

## 2026-10-24 — Bootstrap des agents et de la memory-bank

- **Contexte** : le dépôt Rust n'avait ni `.agents/` ni `memory-bank/`, alors
  que le legacy Python disposait d'une infrastructure d'agents éprouvée.
- **Décision** :
  1. Créer `.agents/rules/`, `.agents/skills/`, `.agents/workflows/` et
     `memory-bank/` dans le dépôt Rust.
  2. Renommer les skills legacy dont la numérotation était obsolète :
     `step4-audio-orchestrator` → `step3-audio-orchestrator`,
     `step5-gpu-ops` → `step4-tracking-ops`.
  3. Remplacer les skills legacy-only par des skills Rust :
     `opencv5-dnn-engine-expert` → `onnx-runtime-cuda-expert`,
     `cv5-multiprocessing-orchestrator` → `rust-concurrency-orchestrator`,
     `after-effects-cep-panel` → `after-effects-bridge`,
     `after-effects-scripts` → `after-effects-json-contract`.
  4. Conserver 5 fichiers canoniques de memory-bank, sans archives initiales.
  5. Pointer les arborescences depuis `AGENTS.md`.
- **Alternatives rejetées** :
  - Copier tel quel le contenu legacy : trop de commandes Python obsolètes.
  - Supprimer les skills sans équivalent Rust : perte de la frontière
    CEP/JSX/AE.
  - Renommer tous les skills : aurait cassé la correspondance avec le legacy.
- **Validation** : arborescence de 51 fichiers à l'époque, vérification des
  commandes et chemins, `cargo fmt`, `clippy`, `test`, commit unique.

## 2026-10-24 — Skill `rust-patterns`

- **Contexte** : ajout d'une skill générique sur les idiomes Rust.
- **Décision** : normaliser son format et l'intégrer à la matrice de routage en
  priorité 3, derrière les skills d'exploitation et de domaine.
- **Contenu adapté** : tests inline (pas de dossier `tests/`), commandes
  `cargo test --workspace` et `cargo clippy --workspace --all-targets`,
  références vers `codingstandards.md` et `rust-concurrency-orchestrator`.
- **Conséquence** : 22 skills, 52 fichiers dans `.agents/` + `memory-bank/`.
  `codingstandards.md` reste prioritaire en cas de conflit.

## Décisions de migration

### 2026-10-22 — Réécriture Rust sans fallback Python

- **Décision** : les huit étapes sont portées en Rust. `workflow-cli` est
  l'unique entrée ; aucun chemin Rust n'appelle `workflow_scripts/`.
- **Raison** : éviter un couplage permanent aux venvs Python et permettre la
  bascule contrôlée.
- **Conséquence** : le legacy reste la référence de parité et le rollback,
  mais n'est pas une dépendance d'exécution Rust.

### 2026-10-22 — Configuration TOML typée

- **Décision** : `config/config.toml` avec `deny_unknown_fields`, secrets par
  environnement, chemins résolus.
- **Alternatives rejetées** : parseur `.env` maison, JSON de config, variables
  d'environnement pour tout.
- **Conséquence** : les anciens noms d'environnement sont acceptés pendant la
  transition, mais le TOML est la source unique.

### 2026-10-22 — JSON contractuel et arrondis Python

- **Décision** : `serde_json` avec `preserve_order`, écritures atomiques,
  `round2`/`round4`/`round_to`.
- **Raison** : les scripts AE, le CEP et les fixtures dépendent de l'ordre des
  clés et des arrondis.
- **Conséquence** : toute modification de writer doit être couverte par le
  shadow STEP5/6 et `parity/DRIFT.md`.

### 2026-10-22 — Verrou matériel distinct

- **Décision** : `/tmp/workflow-rs-hw.lock` pour STEP2/3/4 ; la sync utilise
  `/tmp/workflow-sync.lock` et refuse un serveur actif.
- **Raison** : éviter toute collision avec le verrou Python
  `/tmp/workflow-hw.lock`.
- **Conséquence** : le serveur refuse le verrou Python et sérialise les étapes.

### 2026-10-23 — Serveur axum sur 5004

- **Décision** : orchestrateur Rust sur `:5004`, CLI en sous-processus,
  contrat de polling conservé.
- **Alternatives rejetées** : réutiliser Flask, lier la logique des étapes au
  serveur, partager la base Python.
- **Conséquence** : le serveur ne contient pas la logique métier ; il sérialise
  et expose l'état.

### 2026-10-23 — Runtime isolé et synchronisation explicite

- **Décision** : racines Rust séparées (`runtime/`), sync explicite avant
  bascule.
- **Raison** : un partage direct des racines aurait exposé Rust aux écritures
  Python et inversement.
- **Conséquence** : `workflow-cli sync` fusionne l'historique, la liste STEP1
  et les archives STEP7 ; les deux serveurs doivent être arrêtés.

### 2026-10-23 — Routes Python-only conservées

- **Décision** : cache, stats, diagnostics et `/api/audio/*` restent dans
  Python (D-015).
- **Raison** : éviter un portage spéculatif sans contrat ni besoin production.
- **Conséquence** : le reverse proxy devra router ces routes vers `:5003`.

### 2026-10-23 — Moniteur webhook Rust activé

- **Décision** : activer `webhook.enabled = true` après import de l'historique
  Python.
- **Préconditions** : 911 URL importées, audit à 0 changement, base non vide,
  `source_dir == downloads`.
- **Conséquence** : un seul moniteur à la fois ; Python doit être arrêté.

### 2026-10-23 — Lanceur Rust

- **Décision** : `scripts/start-rust.sh` pour le raccourci Bureau, avec
  génération des secrets, sync et ouverture Firefox.
- **Usage agent** : préférer `scripts/switch-rust.sh`.

### 2026-10-23 — Windows et CEP abandonnés

- **Décision** : ne pas porter la validation Windows, le JSX ou le CEP en Rust
  (D-022).
- **Raison** : la production cible Linux ; STEP8 est validé en headless avec
  `effectcraft-cli`.
- **Conséquence** : `workflow-bridge` reste un contrat disponible ; le CEP
  continue d'appeler Python.

## Écarts legacy (D-001 → D-027)

| ID | Sujet | Décision Rust |
| --- | --- | --- |
| D-001 | CSV de scènes | Conserver `<stem>.csv` ; l'archive utilise `_scenes.csv`. |
| D-002 | STEP3 sans audio | `SKIP_NO_AUDIO`, aucune sortie, code 0. |
| D-003 | Ordre des clés `_audio.json` | Écrire `fps`/`total_frames` avant `frames_analysis`. |
| D-004 | Dépendances STEP4 | CSV et audio requis ; `--allow-partial-speaking` en test. |
| D-005 | `metadata.video_path` | Conserver le chemin absolu observé. |
| D-006 | Commentaires inline `.env` | Loader TOML, pas de parseur naïf. |
| D-007 | `speaker_stats` STEP3/STEP5 | Projection STEP5 documentée. |
| D-008 | JSON STEP4 non atomique | Écriture atomique. |
| D-009 | Parité STEP2–6 | Fixtures de référence. |
| D-010 | MOV alpha | Préserver à l'octet près. |
| D-011 | VRAM | Distinguer processus et carte. |
| D-012 | Verrou legacy | Verrou Rust distinct. |
| D-013 | Parallélisme serveur | Sérialisation, 409. |
| D-014 | STEP1 manquant | Porté au lot 6. |
| D-015 | Routes Python-only | Cache, stats, diagnostics et `/api/audio/*` restent dans Python ; les routes CSV sont portées. |
| D-016 | Extensions STEP1 | Reproduire le scan effectif (pas `.tar.gz`). |
| D-017 | Noms TAR non UTF-8 | Décodage lossy. |
| D-018 | Ingestion manquante | Portée au lot 8. |
| D-019 | Dossier de téléchargement | `runtime/downloads` isolé. |
| D-020 | Concurrence téléchargements | Borne, statut final après repli. |
| D-021 | Historique vide | Import explicite obligatoire. |
| D-022 | Validation Windows/CEP | Abandonnée. |
| D-023 | Racines séparées | Sync explicite. |
| D-024 | STEP4 `153 Camille` | Ne pas utiliser `--prefer-raw` sur ce projet. |
| D-025 | `/sound-design` hors racine | 404 standard de `ServeDir`, pas de middleware dédié. |
| D-026 | Frontières d'erreur | `thiserror` pour ingest/sync ; `anyhow` interne STEP2/3/4/7/8. |
| D-027 | Arrêt manuel CLI | Groupes de processus, SIGTERM puis SIGKILL ; STEP8 garde son gestionnaire. |

## Références

- `parity/DRIFT.md` (détails et preuves)
- `docs/lots/LOT-*.md`
- `docs/exploitation/SYNC-BACKENDS.md`
- `docs/exploitation/SWITCH-PUBLIC.md`
- `docs/exploitation/RETENTION.md`
- `docs/validation/VALIDATION-STEP1-8.md`

## Remédiation de l'audit backend du 2026-10-24

- [2026-10-24 15:00:00] - **Statut `stopping` exposé sans libérer le créneau**
  - `WorkflowState` attribue un `RunId` à chaque exécution ; `StepPhase::Stopping`
    reste actif jusqu'à la fin réelle du sous-processus.
  - `finish_step(step, run_id, …)` ignore une finalisation périmée : un arrêt
    tardif ne peut plus écraser l'état d'une exécution suivante.
  - Le frontend mappe `stopping` sur « Arrêt en cours » et le traite comme un
    état actif.
- [2026-10-24 15:03:00] - **Suppression des clés TOML mortes**
  - Retirées : `server.debug`, `server.polling_interval_ms`,
    `server.system_monitor_interval_ms`, `server.snapshot_interval_s`,
    `server.slow_api_threshold_ms`, `server.gpu_monitoring`,
    `download.chunk_bytes`, `steps.step3.max_workers`, `hardware.tpu_device`,
    `hardware.gpu_mem_limit_mb`, `hardware.require_tpu`.
  - Rupture assumée : `deny_unknown_fields` rejette désormais une ancienne
    configuration ; `config/config.toml` et `config/env.map.toml` sont alignés.
- [2026-10-24 15:06:00] - **`thiserror` aux frontières d'ingestion et de sync**
  - `IngestError` et `SyncError` exposent des résultats typés ; les détails
    internes restent en `anyhow`.
  - Les steps STEP2/3/4/7/8 conservent `anyhow` en interne : dérogation
    documentée D-026, car leurs erreurs traversent le serveur comme code de
    sortie.
- [2026-10-24 15:09:00] - **Verrou matériel unique et écoute explicite**
  - Défaut `/tmp/workflow-rs-hw.lock`, distinct du verrou Python ; tout
    basename `workflow-hw.lock` est refusé, où que soit le chemin.
  - `server.host` non-loopback exige `server.allow_remote = true` ; le
    lancement journalise un avertissement dans ce cas.
- [2026-10-24 15:12:00] - **Isolation partagée et arrêt des groupes**
  - `workflow_core::validate_isolation` est appelé par la CLI et le serveur.
  - Chaque CLI d'étape tourne dans son groupe ; l'arrêt serveur envoie
    `SIGTERM` puis `SIGKILL` au groupe, et `switch-rust.sh stop` tue aussi les
    descendants orphelins.
- [2026-10-24 15:13:18] - **Traversée `/sound-design` documentée**
  - `ServeDir` renvoie 404 au lieu du 403 du contrat Python ; aucune fuite.
    Décision D-025 : conserver le comportement standard et documenter l'écart.

## Compléments A4, A9 et R8 (2026-10-24)

- [2026-10-24 15:25:00] - **Journaux hors du runtime Tokio**
  - Les lignes de journal sont transmises à un writer dédié exécuté via
    `spawn_blocking` ; un disque lent ne bloque plus un worker HTTP.
  - Le canal du writer est borné (512) et le canal amont reste à 1 024.
  - `Runner` journalise début, fin, code de retour, durée et chemin de
    journal ; le middleware HTTP signale les réponses 4xx/5xx.
- [2026-10-24 15:28:00] - **Gestionnaire de signaux pour STEP1–4**
  - `workflow_steps::process` suit jusqu'à 64 enfants longs dans une table
    atomique ; `ffmpeg` et `unrar p` y sont enregistrés.
  - La CLI installe `SIGTERM`/`SIGINT` uniquement pour STEP1–4 : les enfants
    reçoivent `SIGTERM`, puis le CLI sort en `128 + signal`.
  - STEP8 garde son gestionnaire `ctrlc` à double signal ; aucun remplacement.
- [2026-10-24 15:31:00] - **Cache de resize STEP4**
  - Les coefficients `xofs`/`xalpha` et les lignes `row0`/`row1` sont calculés
    une fois par thread et par couple de dimensions, puis réutilisés.
  - L'allocation de l'image de sortie reste inhérente au retour d'un
    `RgbImage` possédé ; elle est la seule allocation par frame conservée.
