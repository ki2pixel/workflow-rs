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
| D-015 | moyenne | Frontend/API | Le serveur Rust n'implémente pas encore CSV, cache, statistiques, diagnostics ni `/api/audio/*`. | Ces routes restent dans Python. Le serveur Rust expose des stubs de polling (`/api/csv_*`, `/api/system_monitor`, `/api/performance/frontend`) pour que le frontend ne casse pas ; le panneau de téléchargements reste vide. |
| D-016 | basse | STEP1 | Le scan Python teste `Path.suffix` contre une liste contenant `.tar.gz`, `.tar.bz2` et `.tar.xz` : ces trois formes ne matchent jamais, car seul le dernier suffixe est comparé. | Le port Rust reproduit le scan effectif (`.zip`, `.zipx`, `.rar`, `.tgz`, `.7z`, `.tar`). Les décodeurs gzip/bzip2/xz restent testés au niveau module, mais le scan ne les sélectionne pas. |
| D-017 | basse | STEP1 | Python décode les noms de membres TAR avec `surrogateescape` ; Rust utilise un décodage UTF-8 lossy. | Écart limité aux archives dont les noms ne sont pas en UTF-8. Les noms concernés restent visibles dans les journaux, avec un caractère de remplacement. |

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
