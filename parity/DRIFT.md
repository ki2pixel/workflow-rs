# Journal de divergence

Ce journal consigne les comportements de l'application Python qui ne sont pas
évidents dans le contrat, ou qui ne doivent pas être reproduits tels quels. Il
est mis à jour à chaque mesure de parité.

| ID | Sévérité | Étape | Constat | Décision pour le portage |
| --- | --- | --- | --- | --- |
| D-001 | moyenne | STEP2/6 | STEP2 écrit `<stem>.csv` ; l'archive ajoute `_scenes.csv`. STEP6 lit `<stem>.csv` uniquement. | Corrigé dans le contrat. Le portage conserve `<stem>.csv` et nomme l'archive `_scenes.csv` à la finalisation. |
| D-002 | haute | STEP3 | Sans piste audio, le worker lève une traceback ffmpeg, le parent compte 0/1 réussite mais sort en code 0 et ne produit aucun `_audio.json`. | Ne pas reproduire le succès silencieux. Le portage journalise un skip structuré et sort en 0 sans artefact, ou en erreur selon la politique du lot 1. Implémenté au lot 4 : `SKIP_NO_AUDIO`, aucune sortie, code 0 et `skipped_no_audio = 1`. |
| D-003 | haute | STEP5 | `_extract_metadata_streaming` s'arrête au début de `frames_analysis`. STEP5 écrit `fps`/`total_frames` après le tableau : une seconde passe perd `audio_fps` et `temporal_alignment`. | Écrire l'en-tête avant le tableau, ou rendre le parseur indépendant de l'ordre. Le harnais reconstruit l'ordre brut pour le fixture. |
| D-004 | haute | STEP4 | `EnhancedSpeakingDetector` charge implicitement `<stem>.csv` et `<stem>_audio.json`. Sans eux, STEP4 sort en 0 mais omet `speaking_sources.audio` et `.segment`. | Déclarer ces dépendances dans le DAG Rust. Un STEP4 sans CSV/audio doit être un mode explicite, pas un succès partiel silencieux. Implémenté au lot 3 : les deux fichiers sont requis par défaut, `--allow-partial-speaking` est réservé aux fixtures. |
| D-005 | basse | STEP4 | `metadata.video_path` contient le chemin absolu de production. | Le comparateur l'ignore. Le portage peut conserver le champ pour compatibilité, ou stocker un chemin relatif ; décision au lot 3 : le chemin absolu observé est conservé. |
| D-006 | moyenne | Config | Les `.env` contiennent des commentaires inline (`STEP5_INCLUDE_TRACKING_ANALYTICS=1  # ...`). `python-dotenv` les retire, un parseur naïf non. | Le chargeur Rust retire les commentaires hors guillemets, ou passe à TOML. |
| D-007 | moyenne | STEP3/5 | STEP3 produit `speaker_stats` par locuteur (`total_time_sec`, `percentage`) ; STEP5 le remplace par `unique_speakers` et `speaker_frame_counts`. | Définir un schéma unique ou documenter la transition. Le harnais applique la projection STEP5 avant comparaison. |
| D-008 | basse | STEP4 | Le JSON est écrit en streaming directement dans le fichier final ; une interruption laisse un fichier tronqué. | Écriture atomique dans le portage (temporaire + rename), format inchangé. Implémenté au lot 3. |
| D-009 | info | STEP2-6 | Sur les projets 244 et 245, les sorties STEP2, STEP3 (après projection), STEP4, STEP5 et STEP6 sont identiques à la production ; 249 et 250 sont identiques sur STEP5/6. Au lot 4, le JSON STEP3 brut de 244 et 245 est aussi identique au script Python relancé sur copie isolée. | Aucun écart de comportement à corriger ; ces sorties servent de fixtures. |
| D-010 | info | STEP1 | Un MOV avec alpha est préservé à l'octet près, sans réencodage (`skipped=1`). | Conserver la détection alpha et la liste `PRESERVED_MOV_NAMES`. Implémenté au lot 6, y compris la préservation par précaution quand ffprobe ne conclut pas. |
| D-011 | basse | Harnais | La VRAM globale n'est pas imputable à une étape ; le harnais distingue désormais `gpu_memory_peak_mb` (processus) et `gpu_total_peak_mb` (carte). | Utiliser la colonne « GPU étape » pour les comparaisons. |
| D-012 | haute | Isolation | L'application Python n'utilise pas `workflow-hw.lock`. Le harnais le prend, mais l'ancienne application peut démarrer STEP1-4 pendant un test. | Le portage et les scripts de test prennent le verrou. Pendant la transition, vérifier `/sequence/status` et `pgrep` avant toute mesure. |
| D-013 | moyenne | Orchestrateur | Le contrat Python autorise deux étapes différentes en parallèle ; le frontend ne le fait pas. | Lot 5 : le serveur Rust refuse la deuxième étape avec 409 et sérialise toutes les exécutions. Durcissement volontaire documenté. |
| D-014 | haute | STEP1 | Le lot 5 n'avait pas d'implémentation Rust de l'extraction/normalisation. | Résolu au lot 6 : extraction ZIP/RAR/TAR, assainissement, réinitialisation mensuelle, suppression des archives et normalisation vidéo. Aucun fallback Python. |
| D-015 | moyenne | Frontend/API | Le serveur Rust n'implémente pas encore cache, statistiques, diagnostics ni `/api/audio/*`. | Ces routes restent dans Python. Depuis le lot 8, `/api/csv_monitor_status` et `/api/csv_downloads_status` sont réelles ; `/api/system_monitor` et `/api/performance/frontend` restent des stubs de polling. |
| D-016 | basse | STEP1 | Le scan Python teste `Path.suffix` contre une liste contenant `.tar.gz`, `.tar.bz2` et `.tar.xz` : ces trois formes ne matchent jamais, car seul le dernier suffixe est comparé. | Le port Rust reproduit le scan effectif (`.zip`, `.zipx`, `.rar`, `.tgz`, `.7z`, `.tar`). Les décodeurs gzip/bzip2/xz restent testés au niveau module, mais le scan ne les sélectionne pas. |
| D-017 | basse | STEP1 | Python décode les noms de membres TAR avec `surrogateescape` ; Rust utilise un décodage UTF-8 lossy. | Écart limité aux archives dont les noms ne sont pas en UTF-8. Les noms concernés restent visibles dans les journaux, avec un caractère de remplacement. |
| D-018 | haute | Téléchargement | Le moniteur webhook, l'historique SQLite, le téléchargement R2/Dropbox et les statuts CSV n'étaient pas portés avant le lot 8. | Résolu au lot 8 : moniteur, normalisation d'URL, historique, échecs/cooldown, téléchargement reprenable et routes de statut. Le moniteur reste désactivé par défaut et refuse une base vide (`require_history`). |
| D-019 | moyenne | Téléchargement | Python télécharge dans `~/Téléchargements`, aussi utilisé par STEP1. Le Rust utilise `runtime/downloads` pour rester isolé. | `LOCAL_DOWNLOADS_DIR` n'est volontairement pas repris ; `WORKFLOW_PATHS_DOWNLOADS` et `WORKFLOW_STEP1_SOURCE_DIR` permettent de choisir un dossier commun lors d'une bascule explicite. Quand le moniteur est actif, le serveur exige `steps.step1.source_dir == paths.downloads`. |
| D-020 | basse | Téléchargement | Python lance un thread par lien, déplace un téléchargement en historique dès qu'une tentative échoue (même si le repli Dropbox réussit ensuite), et conserve l'ancien `error` du moniteur après un succès. | Le port borne la concurrence (`download.max_concurrent`, défaut 4), ne publie le statut final qu'après le repli, remet `error` à null après un passage réussi, et classe une fermeture de corps prématurée en `incomplete`. |
| D-021 | haute | Migration | Le moniteur Rust ne peut pas deviner l'historique Python. Une base vide ferait télécharger tous les liens du webhook. | Import explicite `workflow-cli history import --from <base>` (lecture seule, 911 URL identiques au 2026-10-23), audit `history audit`, puis `webhook.enabled = true`. Les échecs Python ne sont pas importés : ils sont transitoires et liés à des URL R2 expirées. |
| D-022 | info | Validation | La compilation Windows de `workflow-bridge.exe`, le basculement du JSX After Effects et la validation STEP8 sur la machine Windows étaient listés comme restant. | Abandonnés : la production cible Linux, STEP8 est validé en headless avec `effectcraft-cli`, et le JSX/CEP continue d'appeler Python. Aucun chemin Rust ne dépend de cette validation. |
| D-023 | haute | Migration | Python et Rust gardent des racines séparées (`download_history`, `processed_archives.txt`, téléchargements, archives STEP7). Une bascule sans fusion fait retélécharger les archives traitées par l'autre backend et perd la liste STEP1. | Synchronisation explicite `workflow-cli sync` avant chaque bascule : union de l'historique (timestamp minimal), fusion de `processed_archives.txt` par nom de fichier avec remappage, copie/déplacement des archives non traitées et copie/hardlink des analyses STEP7 par `(projet, hash)`. Les échecs ne sont pas fusionnés. |
| D-024 | moyenne | Validation | Le STEP4 Python relancé sur `153 Camille` le 2026-10-24 ne reproduit pas le `_tracking.json` archivé : 4 977 objets contre 11 853 sur une vidéo, et le shadow `--prefer-raw` tombe à 5/15. | Le corpus par défaut utilise le `_tracking.json` de production et reste à 15/15. Les fichiers bruts régénérés sont conservés pour diagnostic. Ne pas utiliser `--prefer-raw` sur 153 tant que la configuration STEP4 d'origine n'est pas retrouvée. |

## Détails

### D-001 — Deux noms pour le CSV de scènes

La production conserve `<stem>.csv` dans `docs/`. STEP7 copie ce fichier vers
`archives/<projet>/<hash>/<stem>_scenes.csv`. STEP6 cherche `<stem>.csv` et ne
lit jamais `_scenes.csv`. Un dossier qui ne contient que la forme archivée perd
silencieusement les recommandations de segments.

### D-002 — STEP3 sans audio

Reproduction :

```bash
coral_env/bin/python tools/parity/parity.py run --case silent_step3
```

Le code de sortie est 0, `outputs` est vide, et la console contient
`CalledProcessError` puis `0/1 réussie(s)`. Le cas est marqué
`allow_empty_outputs = true` dans le manifeste pour ne pas le confondre avec
une régression. Le port Rust produit `SKIP_NO_AUDIO` et
`skipped_no_audio = 1`, sans supprimer une éventuelle sortie existante.

### D-003 — Ordre des clés et réduction audio

Le fichier `_audio.json` produit par STEP3 place `total_frames`, `fps` et
`speaker_stats` **avant** `frames_analysis`. STEP5 réécrit le fichier avec
`frames_analysis` en premier. Le parseur streaming s'arrête au tableau et ne
voit plus le fps. Le harnais `real_244_step5` applique une transformation
`step5_raw_audio` qui replace l'en-tête avant le tableau.

### D-004 — Dépendances implicites de STEP4

Le détecteur de parole charge le CSV de scènes et le JSON audio à côté de la
vidéo. Sans ces fichiers, le tracking reste valide mais la clé
`speaking_sources` ne contient que `visual`. Le premier essai de parité a
produit 23 585 différences pour cette raison ; après ajout des deux entrées,
le diff est nul.

### D-009 — Mesures de référence

Sur `/mnt/cache/244 Cvmille/docs` (1920×1080, 25 fps, 4 920 frames) :

| Étape | Durée | RSS max | VRAM étape | Diff |
| --- | ---: | ---: | ---: | --- |
| STEP2 | 12,2 s | 1 040 MiB | 2 224 MiB | 0 |
| STEP3 | 18,0 s | 965 MiB | 0 (Coral/CPU) | 0 |
| STEP4 | 40,5 s | 593 MiB | 0 (Coral/CPU) | 0 |
| STEP5 | 0,57 s | 18 MiB | 0 | 0 |
| STEP6 | 0,47 s | 29 MiB | 0 | 0 |

Sur `/mnt/cache/245 Cvmille/docs` (1920×1080, 25 fps, 4 728 frames) : STEP2
12,1 s / 1 034 MiB, STEP3 18,0 s / 964 MiB, STEP4 39,6 s / 590 MiB, STEP5 et
STEP6 environ 0,5 s, tous à 0 différence. Les projets 249 et 250 sont à 0 sur
STEP5 et STEP6.

La VRAM STEP2 est mesurée par processus ; le total carte est plus élevé.
STEP3 et STEP4 n'utilisent pas CUDA dans cette configuration.

### D-012 — Verrou matériel

Le harnais prend `flock` sur `$XDG_RUNTIME_DIR/workflow-hw.lock` pour STEP1 à
STEP4 et refuse de démarrer si `/sequence/status` indique une séquence active.
L'application Python ne connaît pas ce verrou. Toute mesure doit donc être
lancée quand la production est arrêtée, ou après accord explicite.

### D-014 — STEP1 porté

Le lot 6 fournit `workflow-cli step1 run` et `step1 normalize-only`. Le serveur
5004 construit la commande avec `steps.step1.source_dir` et applique le timeout
STEP1 (14 400 s par défaut). Le journal horodaté reste écrit dans
`logs/step1/extract_archives_*.log` et `processed_archives.txt` reste exposé au
frontend.

### D-016 — Scan des archives composées

La liste Python contient `.tar.gz`, `.tar.bz2` et `.tar.xz`, mais le test porte
sur `archive.suffix` : pour `projet.tar.gz`, `suffix` vaut `.gz`, donc l'archive
n'est jamais retenue. Le port Rust reproduit ce filtre pour ne pas changer
quelles archives sont traitées. Les décodeurs gzip/bzip2/xz restent disponibles
dans `extract_archive` et sont couverts par les tests unitaires, mais le scan
quotidien ne sélectionne que les suffixes simples.

### D-018 — Téléchargement automatique

Le lot 8 porte la chaîne complète : `workflow-ingest` interroge le webhook,
normalise les URL, consulte `download_history`, applique le cooldown de
`download_failures`, télécharge dans un `.part` reprenable, valide le ZIP, puis
écrit l'historique. Le serveur expose les vrais statuts sur
`/api/csv_monitor_status` et `/api/csv_downloads_status`.

La logique reproduite est celle de Python, et non une limite « derniers N
liens » :

- le webhook est interrogé avec un cache de 60 s et trois tentatives ;
- les deux schémas (`source_url`/`r2_url` et `url` historique) sont acceptés ;
- un lien n'est téléchargé que si son URL normalisée n'est pas déjà dans
  `download_history` ;
- les échecs sont conservés avec un cooldown exponentiel plafonné ;
- R2 est tenté en premier, puis Dropbox ; le `.part` est partagé pour reprendre
  sur l'autre URL ;
- l'historique n'est écrit qu'après un téléchargement validé.

La base Python contient 911 URL au 2026-10-23. L'import Rust a été vérifié
comme identique ligne pour ligne, et `history audit` confirme que les 911 URL
sont déjà normalisées selon l'algorithme Rust.

### D-019 — Dossier de téléchargement isolé

Python écrit dans `LOCAL_DOWNLOADS_DIR` (par défaut `~/Téléchargements`), que
STEP1 scanne ensuite. Le Rust écrit dans `paths.downloads`
(`runtime/downloads`) pour ne pas partager de fichiers avec le moniteur Python.
Lors d'une bascule complète, il suffit d'aligner explicitement
`WORKFLOW_PATHS_DOWNLOADS` et `WORKFLOW_STEP1_SOURCE_DIR` ; le serveur refuse
de démarrer le moniteur si les deux diffèrent.

### D-020 — Statut final et concurrence

Trois durcissements sont volontaires :

- `download.max_concurrent` borne les téléchargements simultanés. Python
  lançait un thread par lien sans limite ; le choix des liens est identique.
  Les liens en attente de permis sont enregistrés dans le tracker avant
  l'attente, sinon le passage suivant du moniteur les replanifierait.
- Le callback de progression ne bascule plus le statut en `failed` à chaque
  tentative. Un R2 absent suivi d'un Dropbox réussi doit apparaître comme
  `completed`, avec le message de repli. Validé bout en bout le 2026-10-23 :
  `r2_url` en 404, `source_url` Dropbox en 200, statut `completed`, message
  `(R2 absent - repli Dropbox utilisé)`, aucun échec persistant.
- Le moniteur efface `error` après un passage réussi, au lieu de conserver
  indéfiniment la dernière erreur.

### D-021 — Migration explicite de l'historique

Le moniteur est désactivé par défaut (`webhook.enabled = false`) et
`webhook.require_history = true` refuse une base vide. Sur l'instance de
production, il est activé depuis le 2026-10-23 21:13 (911 URL importées,
audit à 0 changement, plan à 0/462). Python 5003 doit rester arrêté tant que
le moniteur Rust est actif. La procédure est :

```bash
workflow-cli history import --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3
workflow-cli history audit
workflow-cli history stats
```

L'import ouvre la base source en lecture seule et conserve le plus petit
timestamp par URL. Les échecs Python ne sont pas importés : les objets R2
expirés ne doivent pas imposer un cooldown à la nouvelle instance.

Le diagnostic `workflow-cli webhook probe --plan` compare le webhook réel à
l'historique Rust sans rien écrire. Au 2026-10-23 : 911 URL importées → 0
téléchargement planifié ; base vide → 429 téléchargements planifiés. Le verrou
`require_history` n'est donc pas décoratif.

### D-022 — Validation Windows abandonnée

Le lot 1 avait livré `workflow-bridge` côté Rust et le lot 7 avait laissé
comme reste externe la compilation Windows du binaire, le basculement du JSX
After Effects et une validation STEP8 sur la machine Windows. Ces points sont
abandonnés : la cible de production est Linux, STEP8 est validé de bout en
bout en headless avec `effectcraft-cli`, et le panneau JSX/CEP continue
d'appeler le pont Python. Aucune étape Rust ne dépend de ce binaire.

Conséquence pratique : ne pas rejouer cette validation lors des lots suivants.
Le document `docs/LOT-1-BRIDGE.md` et le protocole `docs/STEP2-WINDOWS.md`
restent comme référence historique ; ils ne sont pas des prérequis de bascule.

### D-023 — Synchronisation au changement

Le partage direct des racines Python a été écarté : il aurait fait écrire Rust
en permanence dans `~/Téléchargements`, `logs/step1` et `archives`, et un
processus Python oublié aurait pu modifier l'état pendant une exécution Rust.
La cohabitation retenue est une synchronisation explicite, exécutée serveurs
arrêtés, avant chaque bascule.

La commande `workflow-cli sync` :

- fusionne `download_history` dans les deux sens et conserve le plus petit
  timestamp par URL. Elle refuse de réécrire des URL non normalisées sauf avec
  `--normalize` ;
- fusionne `processed_archives.txt` par nom de fichier, remappe les chemins
  vers la racine du backend cible, applique la réinitialisation mensuelle et
  ignore les entrées hors du dossier de téléchargement ;
- copie ou déplace les archives non traitées, sans jamais écraser un fichier
  de même nom mais de contenu différent ;
- copie ou hardlinke les analyses STEP7 par `(projet, hash vidéo)`, en
  complétant les hash partiels et en signalant les conflits.

Les échecs de téléchargement ne sont pas fusionnés : ce sont des cooldowns
transitoires, et une URL peut réussir d'un côté après avoir échoué de l'autre.
Les projets en cours ne sont pas déplacés ; le rapport avertit s'il en reste.

La synchronisation prend le verrou `/tmp/workflow-sync.lock`, refuse les ports
5003/5004 actifs et les processus d'étape, et sauvegarde les bases
(`VACUUM INTO`) et les listes avant écriture. Le lanceur Rust l'exécute
automatiquement ; le raccourci Python reste inchangé. Détail dans
`docs/SYNC-BACKENDS.md`.

### D-024 — STEP4 régénéré vs archive 153

Le 2026-10-24, le STEP4 Python a été relancé sur les 5 vidéos de
`153 Camille` avec `STEP4_ENABLE_OBJECT_DETECTION=1`, comme la production.
Les fichiers `<stem>.json` bruts sont produits (5/5), mais leur réduction
STEP5 ne retrouve pas le `_tracking.json` archivé : 4 977 objets contre
11 853 sur la vidéo Manon, et 5/15 vérifications `--prefer-raw` identiques.
La vidéo est pourtant identique (SHA-256 `5d1951...`). La différence vient
donc de la configuration ou du modèle STEP4 utilisé le 2026-10-14, non
conservé dans l'archive. Le shadow par défaut, qui part du `_tracking.json`
de production, reste à 15/15. Les corpus 154-159 et 1720 ne sont pas
affectés : leurs références et leurs bruts (1720) sont cohérents.
