# Contrat d'environnement

| Champ | Valeur |
| --- | --- |
| Version | 1.5 |
| Date | 2026-10-28 |
| Source | `workflow_mediapipe` à l'état du dépôt |
| Portée | Ce que l'application Rust doit reproduire pour remplacer l'application Flask sans régression |

Ce document décrit le comportement observable, pas l'implémentation Python. Il sert de cahier des charges au lot 1 et de référence au harnais de parité. Les chemins cités sont relatifs à `<BASE>`, la racine de l'application actuelle.

Voir aussi `../parity/DRIFT.md` pour les écarts mesurés pendant la baseline et les décisions de portage associées.

## 1. Topologie

L'application est servie par Gunicorn avec **un seul worker** et quatre threads, sur le port 5003. Le worker unique est une contrainte, pas un réglage : `WorkflowState` est un singleton en mémoire et l'état des étapes, des séquences et des téléchargements n'est jamais persisté.

Trois boucles d'arrière-plan tournent dans le processus :

| Boucle | Intervalle | Rôle |
| --- | --- | --- |
| Webhook monitor | 15 s (`WEBHOOK_MONITOR_INTERVAL`) | Récupère les liens, déclenche les téléchargements, enregistre les échecs |
| Snapshot système | 2 s (`SYSTEM_SNAPSHOT_INTERVAL_S`) | Échantillonne CPU, RAM, GPU, disque pour `/api/system_monitor` |
| Nettoyage orphelins | 12 h, premier passage après 5 min | Supprime les projets orphelins de plus de 48 h et les logs anciens |

La nouvelle application utilise le port 5004 et une racine de travail distincte. Elle ne partage ni la base SQLite, ni les répertoires, ni le verrou matériel.

Le lot 5 implémente ce contrat pour les STEP2 à STEP8 et le lot 6 pour STEP1 : `POST /run/STEP1` extrait les archives puis normalise les projets, sans appel Python. Le lot 8 porte le moniteur webhook, l'historique SQLite, le téléchargement R2/Dropbox et les statuts CSV. Les routes cache, statistiques, diagnostics et `/api/audio/*` restent dans le serveur Python legacy. Voir `docs/lots/LOT-5-ORCHESTRATOR.md`, `docs/lots/LOT-6-STEP1.md` et `docs/lots/LOT-8-INGESTION.md` pour le détail et les écarts volontaires.

## 2. API HTTP

### 2.1 Authentification

Les routes mutantes exigent l'en-tête `X-Worker-Token`, comparé à `INTERNAL_WORKER_COMMS_TOKEN`.

- En-tête absent : `401 {"error": "Missing authentication token"}`.
- Valeur incorrecte : `401 {"error": "Invalid authentication token"}`.
- Jeton non configuré côté serveur : `500 {"error": "Authentication not configured"}`.
- En mode `TESTING`, le contrôle est ignoré.

Le frontend lit le jeton dans `<meta name="worker-token" content="...">` et l'ajoute à chaque requête via `static/apiService.js`.

### 2.2 Routes

Blueprint API, préfixe `/api` :

| Méthode | Chemin | Auth | Rôle |
| --- | --- | --- | --- |
| GET | `/api/system_monitor` | non | Snapshot CPU, RAM, GPU, disque |
| GET | `/api/system/diagnostics` | non | Versions Python, FFmpeg, GPU, flags |
| GET | `/api/step_status/<step_key>` | non | Statut d'une étape, sans logs |
| GET | `/api/csv_monitor_status` | non | État du moniteur, échecs, webhook |
| GET | `/api/ping` | oui | `{"status":"pong","worker_type":"ubuntu_new"}` |
| GET | `/api/performance/metrics` | non | Métriques de performance |
| POST | `/api/performance/reset` | oui | Remise à zéro des métriques |
| GET | `/api/cache/stats` | non | Statistiques du cache |
| GET | `/api/cache/search?number=` | non | Recherche sous `/mnt/cache` |
| GET | `/api/cache/list_today` | non | Dossiers créés ou modifiés aujourd'hui |
| POST | `/api/cache/open` | oui | Ouvre un dossier dans l'explorateur |
| POST | `/api/cache/clear` | oui | Vide le cache |
| GET | `/api/csv_downloads_status` | non | Téléchargements actifs et récents |
| GET | `/api/stats/dashboard` | non | Tableau de bord |
| GET | `/api/stats/history?type=&limit=` | non | Historique, `type ∈ {api, system, all}`, `1 ≤ limit ≤ 500` |
| POST | `/api/audio/lemonfox` | oui | Conservé dans le legacy Python ; non repris par le workflow Rust |
| POST | `/api/audio/deepinfra` | oui | Conservé dans le legacy Python ; non repris par le workflow Rust |

Blueprint workflow, sans préfixe :

| Méthode | Chemin | Auth | Rôle |
| --- | --- | --- | --- |
| GET | `/` | non | Page principale |
| POST | `/run/<step_key>` | oui | Lance une étape |
| POST | `/run_custom_sequence` | oui | Lance une séquence, corps `{"steps": ["STEP1", ...]}` |
| GET | `/status/<step_key>` | non | Statut détaillé avec logs |
| POST | `/stop/<step_key>` | oui | Arrête une étape |
| POST | `/cancel/<step_key>` | oui | Alias d'arrêt avec codes différents |
| GET | `/sequence/status` | non | État de la séquence |
| POST | `/sequence/stop` | oui | Arrête la séquence |
| GET | `/get_specific_log/<step_key>/<index>` | non | Extrait d'un log spécifique |
| GET | `/get_specific_log_test/<step_key>/<index>` | oui | Variante, uniquement en debug |
| GET | `/sound-design/<filename>` | non | Sert un fichier de `sound-design/` ; **requis** par `static/soundManager.js` |
| GET | `/test-sound` | non | Page de test audio conservée dans le legacy Python ; non reprise par le workflow Rust |

Routes de l'application : `GET /favicon.ico` retourne 204, `GET /test-slideshow-fixes` lit un fichier HTML du répertoire courant et n'est référencé nulle part. Cette page reste dans l'application Python legacy et n'est pas reprise par le workflow Rust. Les fichiers statiques sont servis sous `/static/`.

### 2.3 Formes de réponse

`GET /api/step_status/<key>` et `GET /status/<key>` :

```json
{
  "step": "STEP3",
  "display_name": "3. Analyse audio",
  "status": "idle|starting|running|stopping|completed|failed|stopped|cancelled|initiated",
  "is_stopping": false,
  "return_code": null,
  "progress_current": 0,
  "progress_total": 0,
  "progress_current_fractional": null,
  "progress_text": "",
  "duration_str": null,
  "is_any_sequence_running": false,
  "log": []
}
```

`log` n'est présent que sur `/status/<key>`. Les deux routes renvoient `Cache-Control: no-store, no-cache, must-revalidate, max-age=0`, `Pragma: no-cache` et `Expires: 0`. Le serveur Rust ajoute le champ `is_stopping` et publie `stopping` pendant un arrêt demandé (voir §3).

`GET /sequence/status` :

```json
{
  "is_running": false,
  "current_step": null,
  "progress": {"current": 0, "total": 8},
  "last_outcome": {"status": "never_run", "type": null, "message": null, "timestamp": null}
}
```

`GET /api/system_monitor` :

```json
{
  "cpu_percent": 12.5,
  "memory": {"percent": 31.2, "used_gb": 9.8, "total_gb": 31.4, "available_gb": 21.6},
  "gpu": {"name": "NVIDIA GeForce GTX 1650", "utilization_percent": 0, "memory": {"percent": 12.0, "used_gb": 0.5, "total_gb": 4.0}, "temperature_c": 41},
  "disk": {"percent": 84.0, "used_gb": 110.0, "total_gb": 139.0, "free_gb": 22.0},
  "timestamp": "2026-10-22T00:00:00+00:00",
  "snapshot_age_s": 0.4
}
```

`gpu` vaut `null` si NVML est indisponible, ou `{"error": "GPU data not available"}` si le device existe sans données.

Le serveur Rust lit la première carte via `nvidia-smi` et applique les mêmes
arrondis (`round(x, 1)` pour le pourcentage mémoire, `round(x, 2)` pour les
Go). Une absence de `nvidia-smi` produit `gpu: null` ; une commande en erreur
produit l'objet `error`.

`GET /api/csv_monitor_status` :

```json
{
  "csv_monitor": {"status": "running|stopped", "last_check": "...", "error": null},
  "data_source": "webhook",
  "monitor_interval": 15,
  "failed_urls": [],
  "failed_urls_count": 0,
  "webhook": {"available": true, "last_fetch_ts": "2026-10-23 15:03:48", "error": null, "records": 3}
}
```

`GET /api/csv_downloads_status` retourne une liste triée par horodatage décroissant :

```json
[
  {
    "id": "csv_ab12cd34",
    "filename": "archive.zip",
    "original_url": "...",
    "url": "...",
    "url_type": "dropbox",
    "status": "pending|completed|failed",
    "progress": 42,
    "message": "...",
    "display_timestamp": "2026-10-22 00:00:00",
    "csv_timestamp": "..."
  }
]
```

`GET /get_specific_log/<key>/<index>` :

```json
{"content": "...", "error": null, "path": "/abs/path.log", "type": "directory_latest", "lines_requested": 150}
```

`GET /api/cache/search` : `{"number": "115", "matches": ["..."], "best_match": "/mnt/cache/115 Camille"}`.
`GET /api/cache/list_today` : `{"folders": [{"path": "...", "name": "...", "number": "115", "mtime": "..."}]}`.
`POST /api/cache/open` : `{"success": true, "message": "..."}` ; corps `{"path": "...", "select_parent": false}`.
`GET /api/ping` : `{"status": "pong", "worker_type": "ubuntu_new"}` ; le serveur Rust ajoute `"runtime": "rust"`.

Les formes de `/api/performance/metrics`, `/api/stats/dashboard` et `/api/stats/history` sont définies par `PerformanceService` ; les clés de premier niveau sont `profiling_stats`, `cache_stats`, `system_performance`, `performance_history`, `alerts`, `timestamp` pour les métriques, et `summary`, `api_metrics`, `system_metrics`, `step_history`, `alerts`, `cache_stats`, `profiling_summary`, `timestamp` pour le tableau de bord.

### 2.4 Codes et messages

| Route | Cas | Code | Corps |
| --- | --- | --- | --- |
| `POST /run/<key>` | lancé | 202 | `{"status":"initiated","message":"Lancement de: ..."}` |
| | étape inconnue | 404 | `{"status":"error","message":"Étape inconnue"}` |
| | séquence ou étape déjà en cours | 409 | `{"status":"error","message":"... en cours."}` |
| `POST /run_custom_sequence` | corps invalide | 400 | `{"status":"error","message":"Invalid steps list"}` |
| | étape inconnue | 400 | `{"message":"Étape inconnue : STEP9"}` |
| | séquence déjà en cours | 409 | `{"message":"Une autre séquence de workflow est déjà en cours."}` |
| `POST /stop/<key>` | étape non lancée | 200 | `{"status":"error","message":"Step 'STEP3' is not running"}` |
| `POST /cancel/<key>` | étape non lancée | 400 | même corps |
| `POST /sequence/stop` | aucune séquence | 409 | `{"status":"error","message":"Aucune séquence en cours d'exécution"}` |
| `GET /get_specific_log/...` | index invalide | 404 | `{"error":"Log index out of range"}` |
| `GET /sound-design/<file>` | traversée de chemin | 403 | `{"error":"Access denied"}` |

Les messages français sont utilisés par le frontend (`"en cours"`, `"inconnue"`, `"aucune"`, `"not running"`). Les traduire ou les reformuler casse la logique d'affichage.

## 3. État

`WorkflowState` contient, par étape :

- `status` parmi `idle`, `starting`, `running`, `completed`, `failed`, `cancelled`, plus `stopped` et `initiated` utilisés par le service. Le serveur Rust ajoute `stopping` entre la demande d'arrêt et la mort effective du sous-processus ; `stopped` n'est publié qu'ensuite.
- `log`, `deque(maxlen=300)`, lignes déjà échappées en HTML.
- `return_code`, `process`, `progress_current`, `progress_total`, `progress_text`, `start_time_epoch`, `duration_str`, `progress_current_fractional`, `files_completed`.

Globalement : `sequence_running`, `sequence_outcome`, `active_csv_downloads`, `kept_csv_downloads` (20 entrées), `csv_monitor_status`.

Règles de concurrence observées :

- Une seule séquence à la fois.
- Une étape déjà lancée ne peut pas être relancée.
- Deux étapes différentes peuvent être lancées simultanément par l'API. Le frontend ne le fait pas, mais le contrat le permet. La nouvelle application doit sérialiser les étapes lourdes pour protéger le GPU et le TPU ; c'est un durcissement volontaire, à documenter.
- Une séquence s'arrête à la première étape en échec.
- Le lancement d'une étape positionne `initiated`, puis le thread passe à `starting` et `running`.
- Un arrêt manuel positionne `stopped` et `return_code = -1` ; côté Rust, `return_code` passe à `-1` dès la phase `stopping`.

Après la fin d'un processus, l'exécuteur force une durée minimale de 0,6 s avant de publier le statut final, pour éviter un clignotement de l'interface.

## 4. Persistance

Base SQLite à `<BASE>/download_history.sqlite3`, en mode WAL, avec deux tables :

```sql
CREATE TABLE download_history (
  url TEXT PRIMARY KEY,
  timestamp TEXT NOT NULL DEFAULT ''
);

CREATE TABLE download_failures (
  url             TEXT PRIMARY KEY,
  kind            TEXT NOT NULL DEFAULT '',
  status_code     INTEGER,
  attempts        INTEGER NOT NULL DEFAULT 0,
  last_error      TEXT NOT NULL DEFAULT '',
  first_seen_at   TEXT NOT NULL DEFAULT '',
  last_attempt_at TEXT NOT NULL DEFAULT ''
);
```

Aucun index explicite. Les fichiers `-wal` et `-shm` sont créés par SQLite. Le groupe partagé est optionnel (`DOWNLOAD_HISTORY_SHARED_GROUP`), avec un mode de fichier `0o664`.

Le serveur Rust utilise le même schéma dans sa base isolée. La migration depuis Python est explicite et en lecture seule :

```bash
workflow-cli history import --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3
workflow-cli history audit
workflow-cli history stats
```

Pour les bascules répétées, `workflow-cli sync` remplace l'import ponctuel :
il fusionne les deux bases et la liste STEP1, puis copie les archives non
traitées et les analyses STEP7. Voir `docs/exploitation/SYNC-BACKENDS.md`.

`history audit` vérifie que chaque URL stockée est déjà normalisée. Le moniteur est désactivé par défaut (`webhook.enabled = false`) et refuse une base vide (`webhook.require_history = true`) pour éviter de re-télécharger tout le webhook.

Normalisation des URLs avant insertion : trim, décodage des entités HTML, décodage récursif des séquences doublement encodées (`amp%3B`), minuscule du schéma et de l'hôte, suppression des ports par défaut, tri des paramètres de requête, suppression des paramètres vides, un seul `dl=1` pour Dropbox, suppression du slash final hors racine, décodage puis réencodage du chemin.

Types d'échec : `not_found`, `auth`, `http_error`, `network`, `stalled`, `incomplete`, `invalid_payload`, `local_io`, `internal`.

Délai avant nouvel essai : `min(intervalle_monitorage × 2^(tentatives-1), DOWNLOAD_COOLDOWN_MAX_S)`, avec un minimum d'un intervalle.

## 5. Système de fichiers

| Chemin | Rôle |
| --- | --- |
| `<BASE>/projets_extraits/` | Racine de travail, un dossier par projet |
| `<BASE>/archives/<projet>/<sha256>/` | Résultats archivés par contenu |
| `<BASE>/logs/app.log` | Log applicatif, rotation 10 Mo × 5 |
| `<BASE>/logs/startup.log`, `stdout.log` | Logs du lanceur |
| `<BASE>/logs/step{1..8}/` | Logs spécifiques affichés dans l'interface |
| `<BASE>/logs/step_{STEP}.log` | Log du sous-processus, rotation 5 Mo × 5 |
| `<BASE>/logs/workflow-server.log` | Log du serveur Rust : configuration, requêtes mutantes, webhook, étapes et séquences ; texte brut, sans ANSI |
| `<BASE>/download_history.sqlite3` | Historique et échecs |
| `~/Téléchargements` | Archives déposées et téléchargées (`LOCAL_DOWNLOADS_DIR`) |
| `/mnt/cache` | Sortie STEP7 par défaut (`OUTPUT_DIR`), et racine du cache |
| `<BASE>/_finalized_output/` | Repli STEP7 si `/mnt/cache` est inscriptible mais absent |
| `<BASE>/sound-design/` | Sons servis par `/sound-design/<file>` |

Le dossier `csv_downloads/` présent à la racine n'est plus utilisé que par les tests.

Nomenclature des artefacts :

| Étape | Fichier |
| --- | --- |
| STEP1 | `<stem>.mp4` normalisé, ou fichier `.mov` préservé tel quel |
| STEP2 | `<stem>.csv` (le suffixe `_scenes.csv` n'apparaît qu'à l'archivage STEP7) |
| STEP3 | `<stem>_audio.json` |
| STEP4 | `<stem>.json`, structure `{"metadata": {...}, "frames": [{"frame": 1, "tracked_objects": [...]}]}` |
| STEP5 | `<stem>_tracking.json`, et réécriture sur place de `<stem>_audio.json` |
| STEP6 | `<stem>_ae.json` |
| STEP7 | `<projet>__finalized_<timestamp>/docs/`, manifeste dans `logs/step7/finalized_projects_<ts>.json` |
| STEP8 | `<video>_9x16.ecproj` (brut) et `<video>_9x16_assist.ecproj` (post A5/A6), plus `<docs>/post/<projet_assisté>/` |

Points d'attention :

- STEP5 modifie `_audio.json` en place. Toute réexécution doit être idempotente.
- STEP5 n'est pas idempotent sur l'ordre des clés : il écrit `fps` et `total_frames` après `frames_analysis`, alors que son parseur streaming s'arrête au début du tableau. Une seconde passe perd `audio_fps` et `temporal_alignment`. La version Rust doit écrire l'en-tête avant le tableau ou parser indépendamment de l'ordre.
- STEP4 dépend implicitement de `<stem>.csv` et `<stem>_audio.json` pour remplir `speaking_sources.audio` et `.segment`. Sans eux, il sort en 0 mais produit un JSON partiel. Ces entrées doivent être explicites dans le graphe Rust.
- STEP6 lit `<stem>.csv`, jamais `<stem>_scenes.csv`. Le second nom est la forme archivée par STEP7. Un `docs/` qui ne contient que `_scenes.csv` prive STEP6 des recommandations de segments, sans erreur visible.
- STEP5 et STEP6 écrivent via fichier temporaire puis `os.replace`, donc atomique.
- STEP4 écrit le JSON directement, sans fichier temporaire : une interruption laisse un fichier tronqué. La version Rust doit utiliser un temporaire puis un renommage, sans changer le format final.
- STEP1 extrait dans `_temp_<projet>_<timestamp>` puis déplace le contenu vers le dossier final.
- Un `.mov` est préservé s'il a une couche alpha réelle ou s'il figure dans `PRESERVED_MOV_NAMES` (défaut `m6+.mov`). `SKIP_MOV_FILES=true` active ce comportement. Les autres `.mov` sont normalisés en MP4.
- STEP7 protège `archives/`, `logs/`, `~/Téléchargements`, `<BASE>` et les dossiers `_temp_*` du nettoyage.

## 6. Contrat des étapes

| Étape | Commande | Répertoire courant | Timeout |
| --- | --- | --- | --- |
| STEP1 | `env/bin/python workflow_scripts/step1/extract_archives.py --source-dir ~/Téléchargements` | `<BASE>` | 1800 s |
| STEP2 | `transnet_cv5_env/bin/python workflow_scripts/step2/run_transnet_cv5.py` | `<BASE>/projets_extraits` | 1800 s |
| STEP3 | `coral_env/bin/python workflow_scripts/step3/run_audio_diarization_tpu.py --log_dir <logs/step3>` | `<BASE>/projets_extraits` | 1800 s |
| STEP4 | `coral_env/bin/python workflow_scripts/step4/run_tracking_tpu.py` | `<BASE>/projets_extraits` | 1800 s |
| STEP5 | `env/bin/python workflow_scripts/step5/json_reducer.py --log_dir ... --work_dir ...` | `<BASE>/projets_extraits` | 1800 s |
| STEP6 | `env/bin/python workflow_scripts/step6/preprocess_ae_json.py --log_dir ... --work_dir ...` | `<BASE>/projets_extraits` | 1800 s |
| STEP7 | `env/bin/python workflow_scripts/step7/finalize_and_copy.py` | `<BASE>/projets_extraits` | 1800 s |
| STEP8 | `env/bin/python workflow_scripts/step8/run_mediasolution_ec.py` | `<BASE>` | 14400 s |

Le lot 6 exécute STEP1 par `workflow-cli step1 run --source-dir <source>`, avec un timeout de 14400 s. Le serveur Rust applique `steps.default_timeout_s` à toutes les étapes, sauf STEP1, STEP7 et STEP8, qui utilisent respectivement `steps.step1.timeout_s`, `steps.step7.timeout_s` et `steps.step8.timeout_s` (14400 s par défaut). Les commandes Python ci-dessus restent la référence comportementale ; elles ne sont plus lancées par l'application Rust.

Variables injectées dans chaque sous-processus : `PYTHONIOENCODING=UTF-8`, `PYTHONUTF8=1`, `PYTHONUNBUFFERED=1`, `ROOT_SCAN_DIR`, `FOLDER_KEYWORD`, `SUBFOLDER_NAME`, `TRACKING_DISABLE_GPU`, `TRACKING_CPU_WORKERS`, `LD_LIBRARY_PATH` (incluant les répertoires `nvidia/*/lib` du venv), `PYTORCH_CUDA_ALLOC_CONF` (STEP2 et STEP3), `AUDIO_PARTIAL_SUCCESS_OK=1`, `AUDIO_GPU_ISOLATION`.

> Lot 2 : le port Rust de STEP2 est lancé par
> `workflow-cli step2 detect --input-dir <docs>`. Il écrit le même
> `<stem>.csv`, avec les mêmes lignes `PROCESSING_VIDEO`,
> `INTERNAL_PROGRESS: <n>/<total> frames (<p>%) - <nom>` et `Succès:`. Il prend
> le verrou matériel et charge ONNX Runtime CUDA depuis
> `steps.step2.onnxruntime`. Il est branché à l'orchestrateur depuis le lot 5 ;
> l'application Python reste la référence de parité et le rollback.
>
> Lot 3 : le port Rust de STEP4 est lancé par
> `workflow-cli step4 track --input-dir <docs>`. Il écrit le même
> `<stem>.json` (même schéma, IDs, bbox, blendshapes et champs de parole) via
> un fichier temporaire puis un renommage. Le chemin nominal exige le Coral
> (`device = "tpu"`) ; `--device cpu` est un mode de test explicite, pas un
> fallback. `<stem>.csv` et `<stem>_audio.json` sont requis par défaut ;
> `--allow-partial-speaking` est réservé aux fixtures sans ces entrées. Le
> module est compilé sous Unix uniquement, la cible de production restant
> Linux. Il est branché à l'orchestrateur depuis le lot 5.
>
> Lot 4 : le port Rust de STEP3 est lancé par
> `workflow-cli step3 diarize --input-dir <docs>`. Il écrit le même
> `<stem>_audio.json` brut (en-tête, `speaker_stats`, une entrée par frame) via
> temporaire puis renommage. Le chemin nominal exige YAMNet Split compilé et le
> Coral (`device = "tpu"`, `require_edgetpu = true`) ; `--device cpu` utilise
> le modèle quantifié et reste un mode de test explicite. ECAPA-TDNN tourne sur
> CPU via ONNX Runtime. Le traitement des vidéos est séquentiel ; le module est
> branché à l'orchestrateur depuis le lot 5. Détail et mesures dans
> `docs/lots/LOT-4-STEP3.md`.
>
> Sur la machine de référence, `config/config.toml` pointe
> `[steps.step3].edgetpu` et `[steps.step4].edgetpu` vers
> `vendor/libedgetpu.so.1`, copie locale du paquet Coral ignorée par git. Sur
> une autre machine, installer `libedgetpu.so.1` dans le chemin système ou
> passer `--edgetpu` à la commande.

STEP4 reçoit en plus `--videos_json_path <temp.json>` : la liste des vidéos sans `<stem>.json` voisin, produite par l'orchestrateur avant lancement. Si la liste est vide, l'étape se termine immédiatement en succès sans lancer de processus.

STEP3 sans piste audio est un cas limite dangereux : le worker lève une traceback ffmpeg, le parent compte 0/1 réussite, sort en code 0 et ne produit aucun `_audio.json`. Le portage doit transformer ce succès silencieux en skip explicite ou en erreur, jamais en étape « completed » sans artefact. Voir `parity/DRIFT.md`, D-002.

STEP4 charge aussi `<stem>.csv` et `<stem>_audio.json` depuis le dossier de la vidéo. Ces fichiers ne figurent pas dans la commande, mais ils sont nécessaires à `speaking_sources.audio` et `.segment`. Le DAG Rust doit les déclarer comme dépendances de STEP4.

Progression sur `stdout`, parsée en continu :

- `TOTAL_VIDEOS_TO_PROCESS: N` ou `TOTAL_VIDEO_TO_PROCESS: N`.
- `PROCESSING_VIDEO: <nom>`.
- `INTERNAL_PROGRESS: <n>/<total> frames (<p>%) - <nom>`.
- `INTERNAL_PROGRESS: <n> batches - <nom>`.
- `[Progression-MultiLine] texte || texte` remplace le texte de progression, les `||` devenant des retours à la ligne.
- Succès : `Succès: ... créé`, `Succès: analyse audio terminée pour ...`, `Finalisation terminée pour '...'`, `MS_BATCH_OK: ...`.
- Échec : `Échec: ... échoué`, `MS_BATCH_FAIL: ...`.

Les lignes contenant `\r` ou des séquences ANSI sont ignorées. Chaque ligne conservée est échappée en HTML avant stockage.

Codes de sortie : `0` succès ; code non nul échec ; `-9` après dépassement de timeout (terminate puis kill) ; `-1` si l'exécutable est absent ou si la préparation de STEP4 échoue ; `-1` avec statut `stopped` sur arrêt manuel.

## 7. Intégrations externes

Webhook : `WEBHOOK_JSON_URL` vaut en production `https://webhook.kidpixel.fr/webhook_proxy.php`. Le service accepte deux schémas :

- Nouveau : `{"source_url", "r2_url", "original_filename", "provider", "created_at"}`. L'URL préférée est `r2_url`, avec `source_url` en repli.
- Ancien : `{"url", "timestamp", "source", "url_type"}`.

Le cache est valable `WEBHOOK_CACHE_TTL` secondes. Trois tentatives avec backoff. Le statut exposé contient `available`, `last_fetch_ts`, `error`, `records`.

Téléchargement : R2 en priorité, Dropbox en repli. Le nom de fichier vient de `Content-Disposition`, puis de l'URL, puis de `original_filename`. Les conflits sont résolus par suffixe. Un fichier `.part` permet la reprise. La taille minimale d'un ZIP est `DOWNLOAD_MIN_ZIP_SIZE_BYTES`, et `DOWNLOAD_VALIDATE_ZIP` active la vérification. Les échecs alimentent `download_failures` et déclenchent le cooldown décrit plus haut. Le résolveur DNS n'accepte que des adresses publiques et les redirections sont limitées à trois ; `download.allow_private_hosts = true` est réservé aux tests locaux.

EffectCraft : STEP8 appelle `effectcraft-cli` en headless, dans une session
`run prefs.set … file.runScript` (aucun bridge TCP, aucune application lancée),
avec le panel `assets/effectcraft/MediaSolution.jsx` (copie du panel de
production) et sa configuration `assets/effectcraft/mediasolution_ec.json`,
pour produire les `.ecproj`.

Post-traitement A5/A6 : après un STEP8 réussi, le projet brut
`<vidéo>_9x16.ecproj` reste **inchangé** et un second projet
`<vidéo>_9x16_assist.ecproj` est publié à côté (copie exacte du brut si aucune
décision ne s'applique). Trois artefacts additifs sont écrits sous
`<docs>/post/<projet_assisté>/` : `post.json` (décisions, timings, specs,
template), `review.json` (abstentions) et `audit.json` (contrôles, SHA-256 du
brut et du projet assisté, hash échantillonné de la vidéo). A1/A2 ne déplacent
jamais l'Anchor Point ; A3/A4 restent en revue ; A6 applique le template SDM
si le préfixe configuré correspond et si la structure est mono-segment sans
keyframe. La configuration est `[steps.step8.post]` ; le hook est désactivable
avec `workflow-cli step8 run --no-post`, et le post-traitement seul se lance
avec `workflow-cli post run`. Détail : `docs/lots/LOT-9-POST-STEP8.md`.

After Effects : `scripts/after_effects/MediaSolution-CEP/` en ExtendScript ES3 appelle Python via `system.callSystem()`. Le contrat JSON `{ "status": "ok", "data": ... }` et le préfixe `[PY] Error:` sont conservés. Le remplacement par `workflow-bridge.exe` est abandonné (D-022) : la production cible Linux et le panneau CEP continue d'appeler Python.

## 8. Configuration unifiée

Le `.env` actuel dépasse 300 lignes et la majorité des clés sert à choisir entre des méthodes supprimées. La cible est un `config.toml` validé au démarrage, avec surcharge par variables d'environnement pour les secrets. Décision du 2026-10-22 : ce format TOML est retenu. Le fichier de production compte 161 clés ; `config/env.map.toml` documente 148 correspondances.

Sections : `[server]`, `[paths]`, `[legacy]`, `[webhook]`, `[download]`, `[steps]`, `[steps.filter]`, `[steps.stepN]` (dont `[steps.step8.post]`), `[retention]` et `[hardware]`.

Correspondance complète dans `config/env.map.toml`. Les règles de validation sont les suivantes :

- Refus de démarrer si `worker_token` est vide, égal à `dev-internal-worker-token`, ou commence par `dev-`.
- Refus de démarrer si `server.host` n'est pas une adresse loopback, sauf avec `server.allow_remote = true`.
- Refus de tout `hardware.lock_path` dont le nom de fichier est `workflow-hw.lock`, quel que soit le répertoire.
- `server.secret_key` est optionnel : aucun mécanisme de session ne l'utilise.
- Refus de démarrer si un chemin obligatoire n'existe pas ou n'est pas accessible en écriture.
- `port` entre 1024 et 65535, `fps` entier positif, `threshold` dans `[0, 1]`.
- `require_edgetpu = true` par défaut pour STEP3 et STEP4.
- Toute clé inconnue provoque une erreur, pas un avertissement.
- Les commentaires inline du `.env` actuel (`STEP5_INCLUDE_TRACKING_ANALYTICS=1  # ...`) sont retirés par `python-dotenv`. Un parseur naïf les conserve et désactive silencieusement l'option. Le chargeur Rust doit les retirer hors guillemets, ou passer à TOML.

Clés à ne pas porter : `STEP3_METHOD`, `STEP3_USE_LEMONFOX`, `STEP3_DEEPINFRA_FALLBACK_TO_PYANNOTE`, tout `LEMONFOX_*` et `DEEPINFRA_*` audio, `STEP4_TRACKING_ENGINE`, `STEP4_ENABLE_GPU`, `STEP4_GPU_*`, `STEP4_INSIGHTFACE_*`, `TRACKING_DISABLE_GPU`, `USE_OPENCV5_STEP2`, `USE_OPENCV5_STEP4`, `STEP2_ENABLE_CORAL_TPU`, `STEP2_CV5_*`, `STEP4_CV5_*`, `MP_LANDMARKER_*`, `STEP4_MEDIAPIPE_*`, `PYTHON_VENV_EXE_ENV`, `VENV_BASE_DIR`.

## 9. Comportements à préserver et corrections assumées

À préserver tel quel :

- Les codes HTTP et les messages français listés en 2.4.
- L'échappement HTML des logs et des noms de fichiers dans les messages d'erreur.
- `Cache-Control: no-store` sur `/status` et `/sequence/status`.
- Le délai minimal de 0,6 s avant le statut final.
- L'arrêt d'une séquence à la première étape en échec.
- Le no-op de STEP4 quand aucune vidéo n'a besoin de tracking.
- Le 204 sur `/favicon.ico`.
- Le jeton dans la balise meta et l'en-tête `X-Worker-Token`.
- La CSP déclarée dans le template ; le serveur Rust l'émet aussi en en-tête HTTP (`Content-Security-Policy`), avec `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` et `Permissions-Policy` restreignant caméra, micro et géolocalisation.

Corrections assumées dans la version Rust :

- Écriture atomique de STEP4, via temporaire et renommage.
- Sérialisation des étapes lourdes par verrou matériel, même si l'API actuelle autorise deux étapes différentes en parallèle.
- STEP3 sans audio : skip explicite et journalisé, au lieu du code 0 silencieux actuel.
- Dépendances STEP4 explicites : CSV de scènes et JSON audio requis pour un output complet.
- STEP5 : en-tête `fps`/`total_frames` avant `frames_analysis`, pour rendre la réduction idempotente.
- Non-reprise de `/api/audio/lemonfox`, `/api/audio/deepinfra`, `/test-sound` et `/test-slideshow-fixes` dans le workflow Rust. L'application Python legacy reste inchangée et conserve ces routes. Vérifié le 2026-10-22 : elles ne sont référencées que par leurs tests, alors que `/sound-design` est appelée par `static/soundManager.js` et reste indispensable dans les deux workflows.
- Pas de SSE au lot 1 : le frontend fonctionne au polling, ajouter SSE changerait le contrat.

## 10. Décisions et questions ouvertes

Décisions du 2026-10-22 :

- STEP7 conserve `/mnt/cache` comme sortie par défaut ; `_finalized_output` reste un repli.
- `/sound-design` est conservée : le frontend précharge huit sons de notification.
- L'application Python legacy reste strictement inchangée : `/api/audio/*`, `/test-sound` et `/test-slideshow-fixes` y restent disponibles. Le workflow Rust ne les reprend pas.

Reste à trancher :

1. Le nouveau serveur doit-il exposer `/api/system/diagnostics` avec les mêmes clés, ou une version enrichie (Rust, version des modèles, état du verrou) ?
