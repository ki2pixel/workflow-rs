# Lot 5 — Orchestrateur Rust et bascule contrôlée

> Mise à jour lot 6 : STEP1 est désormais porté en Rust. Les mentions
> d'indisponibilité de STEP1 ci-dessous décrivent l'état livré au lot 5 et sont
> levées par `docs/LOT-6-STEP1.md`.

## Objectif

Remplacer le rôle d'orchestration de `app_new.py` sans modifier le serveur
Python legacy :

- serveur HTTP Rust sur le port **5004** ;
- base SQLite, racine de travail, journaux et verrou matériel dédiés ;
- état en mémoire thread-safe et contrat de polling conservé ;
- exécution des étapes portées par un sous-processus `workflow-cli` ;
- séquence, arrêt, timeout, logs et rotation ;
- interface web existante servie depuis `assets/web/` ;
- script de démarrage/arrêt contrôlé, sans toucher au port 5003.

Le lot 5 ne porte pas STEP1. Aucune étape ne bascule implicitement vers le
script Python : lancer STEP1 renvoie une erreur explicite et une séquence qui
contient STEP1 est refusée avant démarrage.

## Architecture

```text
navigateur / curl
       │  polling HTTP
       ▼
workflow-server (axum, port 5004)
  ├── WorkflowState (RwLock, logs bornés à 300 lignes)
  ├── RunControl (PID, demande d'arrêt)
  ├── Sequence runner (une étape à la fois)
  ├── SQLite runtime/download_history.sqlite3
  └── workflow-cli stepN ... (sous-processus isolé)
            │
            ├── STEP2  ort CUDA
            ├── STEP3  Coral + ECAPA CPU
            ├── STEP4  Coral
            ├── STEP5  audio/vidéo (mode batch)
            ├── STEP6  _ae.json (mode batch)
            ├── STEP7  finalisation
            └── STEP8  effectcraft-cli
```

Chaque étape tourne dans un processus neuf. Cela évite de partager les
sessions ONNX Runtime, les sessions Coral, les handles GPU et les allocations
TFLite entre deux étapes. Le serveur ne fait que sérialiser les lancements,
collecter la sortie et publier l'état.

### Isolation

| Ressource | Python legacy | Rust |
| --- | --- | --- |
| Port HTTP | 5003 | 5004 |
| Racine | `workflow_mediapipe/projets_extraits` | `workflow-rs/runtime/projets_extraits` |
| Base | `workflow_mediapipe/download_history.sqlite3` | `workflow-rs/runtime/download_history.sqlite3` |
| Journaux | `workflow_mediapipe/logs` | `workflow-rs/runtime/logs` |
| Verrou matériel | `$XDG_RUNTIME_DIR/workflow-hw.lock` | `/tmp/workflow-rs-hw.lock` |
| Livrable final | `/mnt/cache` | `/mnt/cache` (partagé, requis) |

`validate_isolation` refuse un port 5003, une base hors de `paths.base` et le
verrou Python connu. Les tests utilisent une base et un verrou dans un
`tempdir`.

Le verrou Rust est distinct de celui du harnais Python. Par conséquent, une
exécution simultanée de STEP2/3/4 sur les deux piles n'est pas protégée : la
bascule contrôlée suppose qu'une seule pile exécute des étapes matérielles à
la fois. C'est une contrainte d'exploitation documentée, pas un fallback.

## Contrat HTTP

Les routes et formes de réponse sont celles de `docs/CONTRAT-ENVIRONNEMENT.md`.

| Route | Auth | Comportement Rust |
| --- | --- | --- |
| `GET /` | non | rend `assets/web/templates/index_new.html` avec la config des étapes |
| `POST /run/<STEP>` | oui | 202, 404, 409 ; STEP1 → 400 explicite |
| `GET /status/<STEP>` | non | statut + `log`, en-têtes no-cache |
| `GET /api/step_status/<STEP>` | non | statut sans `log` |
| `POST /stop/<STEP>` | oui | arrêt SIGTERM, statut `stopped`, code -1 |
| `POST /cancel/<STEP>` | oui | alias avec code 400 en cas de refus |
| `POST /run_custom_sequence` | oui | 202 ; validation stricte des clés |
| `GET /sequence/status` | non | `never_run`, succès, erreur ou arrêt manuel |
| `POST /sequence/stop` | oui | 409 sans séquence, arrêt de l'étape courante |
| `GET /get_specific_log/<STEP>/<index>` | non | journal propre ou journal principal |
| `GET /api/ping` | oui | `{"status":"pong","worker_type":"ubuntu_new","runtime":"rust"}` |
| `GET /sound-design/<fichier>` | non | `ServeDir` avec protection de traversée |
| `GET /static/*` | non | fichiers copiés depuis le frontend Python |

Le frontend envoie `X-Worker-Token` depuis la balise `meta`. Sans jeton :
401. Jeton non configuré : 500. Les routes mutantes ne sont jamais anonymes.

### Écarts volontaires

- Deux étapes différentes ne peuvent pas être lancées en parallèle : le
  serveur refuse la deuxième avec 409. Le Python le permettait, mais le
  contrat demande de sérialiser les étapes lourdes. Le frontend attend déjà la
  fin d'une étape avant la suivante.
- STEP1 n'est pas disponible. Le refus est explicite, avant toute réservation
  d'état, et aucune commande Python n'est lancée.
- Les routes CSV, cache, statistiques, diagnostics et audio restent dans le
  legacy Python. Le serveur Rust fournit seulement les stubs de polling
  nécessaires au frontend (`/api/csv_*`, `/api/performance/frontend`,
  `/api/system_monitor`). Le panneau « Téléchargements locaux » reste donc vide
  tant que la bascule CSV n'est pas faite.

## Commandes d'étape

Les commandes sont construites dans `catalog.rs`, sans logique métier :

```sh
workflow-cli --config config/config.toml step2 detect --input-dir <projets>
workflow-cli --config config/config.toml step3 diarize --input-dir <projets>
workflow-cli --config config/config.toml step4 track --input-dir <projets>
workflow-cli --config config/config.toml step5 project --work-dir <projets> --keyword Camille
workflow-cli --config config/config.toml step6 project --work-dir <projets> --keyword Camille
workflow-cli --config config/config.toml step7 finalize --work-dir <projets> --logs-dir <logs>
workflow-cli --config config/config.toml step8 run --logs-dir <logs>
```

### Modes batch STEP5/STEP6

`step5 project` parcourt les dossiers projet filtrés par mot-clé, ignore les
médias `.mov` préservés (alpha ou `m6+.mov`), choisit
`<stem>_tracking.json` sinon `<stem>.json`, réduit l'audio en place, puis écrit
le tracking réduit. `step6 project` écrit `<stem>_ae.json`. Les deux modes
réutilisent exactement les fonctions unitaires déjà validées et émettent les
mêmes lignes de progression que Python (`TOTAL_*`, `REDUCING_JSON`,
`PREPROCESS_AE`, `INTERNAL_PROGRESS`, `Succès:`).

Parité batch vérifiée sur copies isolées des projets 244, 245, 249 et 250 :
12/12 fichiers identiques (audio réduit, tracking, `_ae.json`). Aucune écriture
dans `/mnt/cache`. Le contrôle est reproductible avec :

```sh
./scripts/validate-lot5.py
```

## Processus, logs et arrêt

- Sortie stdout/stderr fusionnée ligne par ligne, échappée HTML dans l'état.
- Journal `runtime/logs/step_STEPx.log`, rotation à 5 Mio (`.1` à `.6`).
- Progression dérivée des lignes `TOTAL_*`, `PROCESSING_VIDEO`,
  `REDUCING_JSON`, `PREPROCESS_AE`, `INTERNAL_PROGRESS`, `Succès`, `MS_BATCH_*`.
- Timeout par étape : `steps.default_timeout_s`, sauf STEP8
  (`steps.step8.timeout_s`).
- Arrêt : SIGTERM, puis SIGKILL après 5 s. Le statut est publié immédiatement
  comme `stopped` ; le worker ne peut pas l'écraser.
- Une séquence s'arrête à la première étape non `completed`.
- Le résultat est écrit dans `orchestrator_runs` (SQLite) pour audit.

## Bascule contrôlée

```sh
export WORKFLOW_SERVER_SECRET_KEY='...'
export WORKFLOW_SERVER_WORKER_TOKEN='...'

cargo build --release -p workflow-server -p workflow-cli
./scripts/switch-rust.sh check
./scripts/switch-rust.sh start
./scripts/switch-rust.sh status
./scripts/switch-rust.sh stop
```

`start` ne touche jamais au port 5003 : Python reste le rollback. Le
basculement du trafic public (reverse proxy, DNS ou port) reste une opération
externe. En cas d'échec Rust, il suffit de continuer à router vers 5003.

`check` exige que le binaire `workflow-cli` et le gabarit existent, que la base
et le verrou soient isolés, et que le jeton soit configuré.

## Validation

- `cargo test --workspace` : tests unitaires du catalogue, de l'état, du
  parseur de progression, de la base, du runner (succès, échec, SIGTERM), de
  la séquence et des routes HTTP avec un faux CLI.
- Test de rendu du vrai `index_new.html` avec la syntaxe Jinja existante.
- `workflow-server --check` sur la configuration réelle.
- Smoke HTTP réel : index 200, ping, statut, lancement STEP5 sur racine vide,
  séquence STEP5→STEP6, refus explicite de STEP1.
- `cargo clippy --workspace --all-targets -- -D warnings` et `cargo fmt`.

## Limites restantes

- STEP1 n'était pas porté au lot 5 : le frontend « lancer 1-8 » échouait à la
  première étape. Le lot 6 lève cette limite ; les séquences STEP1 à STEP8
  fonctionnent désormais.
- Le mode batch STEP5 ne fait pas l'enrichissement legacy depuis un STEP4 brut
  lorsqu'un `_tracking.json` partiel existe déjà. Le pipeline Rust neuf n'en a
  pas besoin ; les projets Python déjà réduits restent identiques (shadow).
- Les routes CSV/downloads/cache/statistiques/audio et `/test-sound` restent
  dans Python. Le serveur Rust expose des stubs vides pour ne pas casser le
  polling du panneau.
- La validation Windows/After Effects et le JSX restent externes et différés.
- Le verrou Rust est distinct du verrou Python : ne pas lancer de STEP2/3/4
  matérielle des deux côtés simultanément pendant la période de dual-run.
