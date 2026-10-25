# Patrons et conventions du système

Ce document décrit les patrons récurrents de workflow-rs. Les règles
obligatoires sont dans `.agents/rules/codingstandards.md` ; les écarts de
comportement legacy sont dans `parity/DRIFT.md`.

## 1. Configuration

- `config/config.toml` est la source unique ; les structures utilisent
  `serde(default, deny_unknown_fields)`.
- Les secrets ne sont jamais dans le TOML :
  `WORKFLOW_SERVER_SECRET_KEY`, `WORKFLOW_SERVER_WORKER_TOKEN`.
- Les anciens noms `FLASK_SECRET_KEY` et `INTERNAL_WORKER_COMMS_TOKEN` sont
  acceptés pendant la transition.
- `Config::load` applique les surcharges d'environnement, puis
  `validate_common` (CLI) ou `validate_server` (serveur).
- `config show` masque les secrets.

## 2. Chemins et isolation

- `PathsConfig::resolve()` dérive `projects`, `archives`, `logs`, `downloads`,
  `history_db` et `finalized_fallback` depuis `base` s'ils sont absents.
  `models`, `cache_root` et `finalized` restent explicites (`finalized` vaut
  `/mnt/cache` par défaut).
- Le serveur refuse une base hors `paths.base`, le port 5003, le verrou Python
  et les jetons par défaut. Un hôte non-loopback exige
  `server.allow_remote = true`.
- `runtime/` est la production locale ; `/mnt/cache` est la sortie finalisée.
- Les tests utilisent `tempfile` ou `/tmp/opencode`, jamais `runtime/` ni
  `/mnt/cache`.

## 3. Erreurs

- `thiserror` pour les erreurs de bibliothèque (`workflow-core`,
  `workflow-tpu`, `workflow-steps`).
- `anyhow` aux frontières CLI, serveur, ingestion et sync.
- Pas de `unwrap()` en production ; `expect()` réservé aux invariants locaux.
- Les erreurs de config citent la clé et la valeur fautives.
- Les étapes retournent un rapport avec succès/échec et un code de sortie.

## 4. JSON contractuel

- `serde_json` avec `preserve_order` : l'ordre des clés est contractuel.
- `_audio.json` écrit `fps`/`total_frames` avant `frames_analysis`.
- `_ae.json` est indenté, avec `dataByFrame`, `audioByFrame`,
  `segment_recommendations`.
- Écriture atomique : temporaire voisin, `flush`, `rename`.
- Arrondis via `workflow_schema::round::{round2, round4, round_to}`.
- Ne jamais utiliser `f64::round` pour une valeur contractuelle.

## 5. Serveur

- `AppState` contient l'état thread-safe, le runner et l'ingestion.
- `Runner` réserve une étape, lance `workflow-cli`, pompe stdout/stderr,
  applique le timeout et met à jour l'état.
- Une seule étape à la fois : deuxième lancement → 409.
- Une séquence s'arrête au premier échec.
- `stop`/`cancel` passent par `stopping`, envoient SIGTERM puis SIGKILL et
  conservent le créneau jusqu'à la mort du sous-processus. Un `finish_step`
  d'une génération précédente est ignoré (`RunId`).
- Les routes mutantes exigent `X-Worker-Token`.
- Les logs sont stockés sous `runtime/logs/stepN/`.

## 6. Sous-processus

- Serveur et STEP2 : `tokio::process::Command`, `kill_on_drop(true)`, timeout
  par étape, lecture concurrente stdout/stderr.
- STEP1/3/4/8 : `std::process::Command` avec threads dédiés et arrêt explicite.
  STEP8 place `effectcraft-cli` dans son propre groupe, applique un watchdog
  SIGTERM puis SIGKILL, et termine le groupe avant de sortir.
- Les codes de sortie et les lignes `MS_BATCH_*` sont contractuels.

## 7. TPU et GPU

- `workflow-tpu` charge dynamiquement `libtensorflowlite_c.so` et
  `libedgetpu.so.1`.
- `Session` vérifie la signature `edgetpu-custom-op`.
- STEP3 : une session YAMNet réutilisée séquentiellement (`max_workers = 1`).
- STEP4 : un délégué par modèle TPU, blendshapes sur CPU.
- STEP2 : `ort` avec `CUDAExecutionProvider`, préchargement des libs CUDA.
- Le matériel est protégé par `HardwareLock` et la sérialisation serveur.

## 8. Concurrence

- Canaux bornés pour les flux de frames/fenêtres.
- `rayon` borné pour STEP1.
- `Semaphore` pour les téléchargements.
- `Arc<Mutex<...>>` pour les moteurs et la base SQLite.
- Pas de `Mutex` tenu à travers un `await`.
- Pas de canal non borné pour un flux vidéo.
- Pas de parallélisme TPU non sérialisé.

## 9. Ingestion

- `workflow-ingest` normalise les URL comme `CSVService`.
- `download_history(url, timestamp)` conserve le plus petit timestamp.
- `download_failures` porte le cooldown exponentiel.
- Le moniteur refuse une base vide et exige
  `steps.step1.source_dir == paths.downloads`.
- Un seul moniteur actif (Python ou Rust).
- Les échecs Python ne sont pas importés.

## 10. Sync

- `workflow-cli sync` fusionne l'historique, la liste STEP1 et les archives
  STEP7.
- Sans `--apply`, dry-run.
- Les deux serveurs doivent être arrêtés.
- Sauvegarde sous `runtime/backups/sync/<timestamp>`.
- Les archives STEP7 sont copiées ou hardlinkées par `(projet, hash)`.

## 11. Tests

- Tests inline `#[cfg(test)]`, pas de dossier `tests/`.
- `cargo test --workspace` : 280 tests.
- Pas de CI : exécution manuelle.
- Les tests ne touchent ni le matériel, ni `/mnt/cache`, ni `runtime/`.
- La parité se mesure avec les scripts de shadow/validation.

## 12. Journalisation

- `tracing_subscriber` écrit sur stderr ; ANSI uniquement si stderr est un
  terminal. Le lanceur redirige vers `runtime/logs/workflow-server.log`, donc
  le fichier reste du texte brut.
- Niveau par défaut `info` : configuration, requêtes mutantes, erreurs HTTP,
  webhook significatif, étapes et séquences. `RUST_LOG=debug` active les GET,
  les passages webhook complets et les tentatives de téléchargement.
- Les journaux d'étapes restent dans `runtime/logs/step_STEPx.log`, avec la
  sortie brute du CLI et rotation de taille.

## 13. Documentation et mémoire

- Docs en français, code/commandes en anglais.
- `README.md` : état et commandes.
- `docs/LOT-*.md` : périmètre, parité, performance, limites.
- `docs/CONTRAT-ENVIRONNEMENT.md` : contrat HTTP/artefacts.
- `parity/DRIFT.md` : écarts et décisions D-xxx.
- `memory-bank/` : état courant, décisions, progrès.
- `AGENTS.md` : résumé opérationnel pour les agents.

## 14. Frontend

- `assets/web` est une copie du legacy ; ne pas diverger sans décision
  explicite.
- `index_new.html` est prétraité par Minijinja (`url_for`, `tojson`).
- `AppState`, `DOMBatcher`, `DOMDiff`, `PollingManager` sont les patrons.
- L'overlay de logs utilise un focus trap et une auto-ouverture configurable.
  Contrairement au legacy, la copie Rust désactive l'auto-ouverture par
  défaut (`ui.autoOpenLogOverlay = false`) ; le toggle Settings reste
  fonctionnel et une valeur persistée par l'opérateur reste prioritaire.
- `PollingManager` accepte `pauseWhenHidden` (opt-in) : les pollers
  `systemMonitor` et `localDownloadsStatus` sont suspendus sur
  `visibilitychange` (`document.hidden`) et rafraîchis au focus. Les pollers
  d'étapes et de séquence restent actifs pour ne pas bloquer l'auto-mode
  quand la fenêtre est dans le tray. C'est un écart assumé avec le legacy
  (qui continue à poller en arrière-plan) ; validation dans
  `docs/MVP-FRONTEND-POLLING.md`.
- Les imports de `PollingManager.js` portent `?v=20261025a` : incrémenter ce
  suffixe à chaque modification du module, sinon WebKit peut servir l'ancien
  module malgré `Cache-Control: no-store`.
- Pas de runner Node dans le dépôt Rust ; tests de rendu côté serveur.

## 15. After Effects

- `workflow-bridge` gère `cuts` et les manifestes `layers`.
- Le format de sortie et le préfixe `[PY] Error:` sont contractuels.
- Le panneau CEP et les JSX restent Python (D-022).
- Ne pas modifier les scripts AE pour adapter une sortie Rust.
