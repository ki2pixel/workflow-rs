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
  `/mnt/cache` par défaut). `Config::step1_source_dir()` dérive la source STEP1
  de `paths.downloads` si elle est vide ; `WORKFLOW_PATHS_BASE` isole donc tout
  le runtime (D-071).
- Le serveur et la CLI refusent `history_db`, `projects`, `archives`, `logs` et
  `downloads` hors `paths.base`, ainsi qu'un `paths.projects` dans le dépôt
  legacy. `steps.step1.source_dir` suit `paths.downloads` par défaut ; une
  valeur explicite reste prioritaire. Le serveur refuse en plus le port 5003 ;
  `validate_common` refuse le verrou Python.
  Un hôte non-loopback exige `server.allow_remote = true`, et
  `allow_remote = true` exige une `secret_key` non vide et non préfixée `dev-`.
- La purge des orphelins se limite aux fichiers `.part`, `.tmp`, `.*.tmp.*` et
  aux dossiers `_temp_*` émis par le pipeline.
- `runtime/` est la production locale ; `/mnt/cache` est la sortie finalisée.
- Les tests utilisent `tempfile` ou `/tmp/opencode`, jamais `runtime/` ni
  `/mnt/cache`.

## 3. Erreurs

- `thiserror` pour les types publics (`ConfigError`, `MissionError`,
  `StepError`, `ArchiveError`, `IngestError`, `SyncError`,
  `WorkflowServerError`) ; `TpuError` implémente `Display` manuellement.
- `anyhow` pour les détails internes de l'ingestion, de la sync, du CLI et du
  serveur, et dans les étapes lourdes STEP2/3/4/7/8 (D-026). Les points d'entrée
  publics d'ingestion et de sync restent typés.
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
- `cargo test --workspace` : 689 tests listés (687 exécutés + 2 ignorés).
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
- `docs/lots/LOT-*.md` : périmètre, parité, performance, limites.
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md` : contrat HTTP/artefacts.
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
- Écart assumé : la barre `.workflow-controls` de la topbar est centrée
  (`justify-content: center`, `controls.css`) au lieu d'être calée à gauche
  comme dans le legacy. `.sequence-controls` centre aussi ses lignes en cas de
  wrap ; le media query 768 px conserve la disposition mobile en colonne.
- Écart assumé : la carte STEP8 suit aussi le post-traitement A5/A6. Quand
  `progress_phase = "post"`, la barre passe en orange, le badge affiche
  « Post-traitement » et le texte du serveur (`Post-traitement A5/A6 : i/N —
  <projet>`) est affiché tel quel, sans réapposer le compteur global.
- Pas de runner Node dans le dépôt Rust ; tests de rendu côté serveur.
- Écart assumé de sécurité (D-074, AUD-FE-01/AUD-FE-06) :
  `DOMUpdateUtils.escapeHtml` (`utils/DOMBatcher.js`) échappe explicitement
  `&`, `<`, `>`, `"` et `'` par remplacements de chaînes, sans
  `document.createElement`, ce qui couvre aussi les contextes d'attribut ;
  `handleStepFailure` (`apiService.js`) échappe `errorMessage` avant
  `updateMainLogOutputUI`. Les sélecteurs DOM contractuels et les autres
  contrats HTTP/JSON/CSV sont inchangés. Le legacy Python n'est pas modifié.
- Écart assumé de monitoring (D-075, AUD-FE-02/AUD-FE-05) : dans la copie
  Rust, `PerformanceMonitor.generatePerformanceReport()` appelle
  `errorHandler.getErrorStats()` (le legacy appelle une méthode inexistante
  `getStats()`), les pollers `perf-memory-usage`, `perf-periodic-reporting` et
  `perf-optimizer-stats` passent `{ pauseWhenHidden: true }`, et
  `sendPerformanceReport()` utilise `this.originalFetch` avec l'en-tête
  `X-Worker-Token` lu dans `<meta name="worker-token">`. Le POST de télémétrie
  n'entre donc plus dans `metrics.apiCalls`. Le payload de
  `/api/performance/frontend` et le handler serveur sont inchangés.
- Écart assumé d'immuabilité (D-076, AUD-FE-03/AUD-FE-04/AUD-FE-11) : dans la
  copie Rust, `subscribeToProperty()` compare les valeurs via
  `_areValuesEqual` au lieu de `!==`, donc `csvDownloads` et les autres objets
  ou tableaux ne sont plus notifiés par les ticks `processInfo`.
  `getStateProperty()` retourne un clone profond ; `_deepClone()` préserve la
  référence des objets hôtes non clonables (nœud DOM de
  `focusedElementBeforePopup`) et `_mergeDeep()` les traite comme des feuilles
  tout en clonant les tableaux. `reset()` remplace l'état complet (11 clés
  `ui`, `processInfo`/`stepTimers`/`pollingIntervals` vidés) et `ui.theme` vaut
  `'dark-pro'` au constructeur comme après reset, en alignement avec
  `themes.css`. Le legacy Python conserve l'ancien comportement.
- Écart assumé de syntaxe CSS (D-077, AUD-FE-08) : les variables `*-rgb` de
  `variables.css` sont écrites en composantes séparées par des espaces
  (`--accent-primary-rgb: 121 134 203;`). Les `box-shadow` qui les composent
  utilisent donc exclusivement `rgb(var(--*-rgb) / <alpha>)` et jamais
  `rgba(var(--*-rgb), <alpha>)`, qui se résoudrait en
  `rgba(121 134 203, 0.3)` — une forme invalide en CSS Color 4, rejetée en
  silence par le moteur. `steps.css` (survol, focus, sélection, statut
  `running`) et `csv-workflow-prompt.css` (`.workflow-launch-btn` et son
  survol) suivent désormais la convention déjà appliquée par `logs.css`. Les
  ombres neutres `rgba(0, 0, 0, …)` restent valides et inchangées ; le legacy
  Python conserve l'ancienne syntaxe.
- Écart assumé de robustesse Worker (D-078, AUD-FE-07) : dans la copie Rust,
  `WorkerManager._executeTask` résout `null` (avec `console.debug`) quand une
  réponse arrive après qu'une requête plus récente a été émise sur le même
  canal ; le fallback synchrone fait de même, et les vraies erreurs continuent
  de rejeter. `uiUpdater.js` ignore `null` avant toute mutation DOM, donc un
  panneau n'est jamais écrasé par une réponse périmée.
  `workerManager.destroy()` est attaché à `beforeunload`, avec un garde
  `typeof globalThis.addEventListener === 'function'` pour rester chargeable
  dans un contexte Node, comme `appState`, `domBatcher` et `pollingManager`.
- Écart assumé de spécificateurs ES Modules (D-078, AUD-FE-09) : aucun import
  de `assets/web/static` ne porte de query parameter de cache-busting.
  `PollingManager.js` est importé sous son chemin canonique par les sept
  modules concernés, y compris l'import dynamique de `PerformanceOptimizer.js` ;
  la revalidation est assurée par l'en-tête Axum
  `Cache-Control: no-cache, must-revalidate`. Un suffixe divergent créerait un
  second singleton avec son propre registre `inFlight`.
- Écart assumé d'aperçu CSV (D-078, AUD-FE-11) : `csvWorkflowPrompt.js` décrit
  les huit étapes canoniques (Extraction, Transitions, Audio, Tracking,
  Réduction JSON, Pré-traitement AE, Finalisation, Media Solution) et annonce
  « étapes 1-8 » ; la séquence lancée consomme déjà `STEP1`…`STEP8` via
  `defaultSequenceableStepsKeys`. Le legacy Python conserve l'aperçu à six
  étapes.
- Contrat ARIA de l'overlay de logs (D-079, AUD-FE-12) :
  `#logs-column-global` (`assets/web/templates/index_new.html`) porte
  `role="dialog"`, `aria-modal="true"` et `aria-labelledby="log-panel-title"` ;
  `#log-panel-title` reste le titre « Logs ». `popupManager.js` continue de
  basculer `aria-hidden`, d'appliquer le focus trap, `Escape` et la restauration
  du focus par l'ID ; `uiUpdater.js` et les fonctions
  `openLogPanelUI`/`closeLogPanelUI` sont inchangés. L'overlay suit le même
  contrat sémantique que `#sequence-summary-popup-overlay` et
  `#custom-sequence-confirm-popup-overlay`. Toute nouvelle modale plein écran
  doit déclarer ces trois attributs et un titre référençable.
- Propriété CSS unique par composant (D-079, AUD-FE-10) :
  `.local-downloads-section` appartient à `components/downloads.css` (base,
  `[data-visible="true"|"false"]`, animation GPU, `prefers-reduced-motion`) ;
  le mode compact `.workflow-wrapper.compact-mode .steps-column` appartient à
  `components/steps.css` (`display: flex`, `gap`, `transition: opacity`) ;
  `#run-all-steps-button` appartient à `components/workflow-buttons.css` (base,
  `min-height: var(--touch-target-min)`, shine, survol, actif, désactivé,
  dégradé). `layout.css` ne garde que le layout générique (`.workflow-wrapper`,
  scrollbar, base `.steps-column`) et `controls.css` ne contient plus aucune
  règle du bouton principal. `features/responsive.css` reste la couche de
  breakpoints mobile (padding, rayon, ombre), pas un doublon. Toute nouvelle
  règle sur ces composants doit être ajoutée à la feuille autoritaire.
- Pas de client desktop natif (D-028) : ne pas ajouter de dépendance
  GTK/WebKit/zbus/Tauri ; `assets/web` est l'unique interface.

## 15. After Effects

- `workflow-bridge` gère `cuts` et les manifestes `layers`.
- Le format de sortie et le préfixe `[PY] Error:` sont contractuels.
- Le panneau CEP et les JSX restent Python (D-022).
- Ne pas modifier les scripts AE pour adapter une sortie Rust.

## 16. Post-traitement STEP8 (A5/A6/A7)

- `crates/workflow-steps/src/post/` lit le `.ecproj` brut et ne le modifie
  jamais : hors `--report-only`, il publie un second `<stem>_assist.ecproj`
  (copie exacte si aucune décision ne s'applique).
- Les artefacts `post.json`, `review.json`, `audit.json` sont additifs sous
  `<docs>/post/<projet_assisté>/` ; ils ne touchent ni `_ae.json`, ni
  `_audio.json`, ni le CSV, ni le panneau.
- A1/A2 ne changent que `Scale` et, pour A1, `Position Y` ; l'Anchor Point
  STEP6 reste intact. A3/A4 sont en `review` uniquement. A6 applique le
  template SDM mesuré et court-circuite A1–A4.
- Un conflit A1/A2 part en `review`. Un échec de post-traitement fait échouer
  STEP8 côté CLI.
- A7 (D-064 → D-069) résout la mission email, écrit `<stem>_mission.json`
  (`schema: 2`) uniquement si l'assist est publié, et ajoute `requested`,
  `source` (`local_docs` / `central_fallback`) et `known_absent` de façon
  additive. `docs/` local gagne ; `assets/mission_overlays` est le repli ;
  `tail_overlay` ne prolonge jamais la comp et coupe l'audio du MOV.
- Depuis D-051, A2 dispose d'un gate OCR opt-in (`a2_ocr = false`) : preuve
  additive sous `details.ocr`, veto si le texte change, override de
  géométrie borné, bandeaux sombres laissés en `review` avec scale proposé.
  Les garde-fous durs (largeur, hauteur, persistance, `temporal_std`,
  keyframes, bande haute) restent inchangés.
- Configuration : `[steps.step8.post]` ; `--no-post` désactive le hook ;
  `workflow-cli post run --report-only` calcule les rapports sans publier.
- Progression : `POST_TOTAL`, `POST_CURRENT`, `POST_PROGRESS` et `POST_DONE`
  prolongent la barre STEP8. Le hook STEP8 les écrit sur `stdout` avec
  `MS_BATCH_*` pour préserver l'ordre ; `post run` les met sur `stderr` et
  garde son JSON sur `stdout`. `POST_CURRENT` change le texte,
  `POST_PROGRESS` avance la barre après le projet ; le serveur expose
  `progress_phase = "post"`.
- Détail : `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-12*.md`.
