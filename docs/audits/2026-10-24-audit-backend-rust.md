# Audit backend Rust — workflow-rs

- **Date** : 2026-10-24
- **Commit audité** : `ae2098d` (`docs: align production status and fix parity harness commands`)
- **Périmètre** : les neuf crates du workspace, orchestrateur `workflow-server`
  (Axum 0.8, `:5004`), CLI `workflow-cli`, ingestion `workflow-ingest`,
  abstraction matérielle `workflow-tpu`, étapes STEP1 à STEP8.
- **Méthode** : audit statique du code, exécution de la suite de tests et des
  lints, lecture croisée de `AGENTS.md`, `.agents/rules/`, `memory-bank/`,
  `docs/CONTRAT-ENVIRONNEMENT.md` et `parity/DRIFT.md`.
- **Hors périmètre** : aucune étape matérielle lancée, aucun serveur démarré,
  aucune écriture dans `runtime/`, `/mnt/cache` ou le dépôt Python legacy,
  aucune mesure de parité réelle (les tests ne chargent ni modèle, ni GPU, ni
  Coral).
- **Statut** : audit en lecture seule lors de sa rédaction. Remédiation
  appliquée le 2026-10-24 : 31 constats corrigés, 2 partiels (A9, R8),
  3 documentés (C3 → D-025, C4 → D-015, C8 → D-024). Voir la section 0
  ci-dessous.

## 0. Statut de remédiation (2026-10-24)

Après l'audit, les six lots du plan ont été appliqués sans toucher au dépôt
Python legacy, à `runtime/` ni à `/mnt/cache`. La suite passe de 233 à 275
tests, `cargo fmt --check` et
`cargo clippy --workspace --all-targets -- -D warnings` sont verts, et
`workflow-cli config check` valide la configuration de production.

| Constat | Statut | Preuve principale |
| --- | --- | --- |
| A1 | corrigé | `RunId`/`StepPhase::Stopping`, `finish_step` générationnel, tests 409 après `/stop` |
| A2 | corrigé | arrêt gracieux du serveur, groupes de processus, `switch-rust.sh stop` |
| A3 | corrigé | STEP1 prend `HardwareLock` si normalisation GPU |
| A4 | corrigé | canal borné 1 024, lignes plafonnées à 64 Kio (4 Kio dans l'état) |
| A5 | corrigé | `parking_lot::RwLock`, `FinishGuard` RAII, `WorkflowServerError` |
| A6 | corrigé | rotation pendant l'écriture, lecture en queue de fichier |
| A7 | corrigé | progression calculée sur les étapes réelles de la séquence |
| A8 | corrigé | purge au démarrage des `_temp_*`, `*.part`, `*.tmp*` anciens |
| A9 | partiel | groupes de processus côté serveur ; CLI manuel documenté D-027 |
| S1 | corrigé | hôte non-loopback refusé sans `server.allow_remote` |
| S2 | corrigé | défaut `/tmp/workflow-rs-hw.lock`, rejet global de `workflow-hw.lock` |
| S3 | corrigé | `secret_key` optionnel, documenté comme inutilisé |
| S4 | corrigé | `validate_isolation` partagé CLI/serveur |
| S5 | corrigé | comparaison constant-time et en-têtes de sécurité |
| S6 | corrigé | résolveur DNS public-only, redirections limitées |
| S7 | corrigé | budget de décompression, symlinks STEP7, antislashs |
| S8 | corrigé | logs confinés sous `paths.logs`, refus hors racine |
| S9 | corrigé | `.repomixignore` |
| R1 | corrigé | `IngestError`/`SyncError` aux frontières ; dérogation steps D-026 |
| R2 | corrigé | lectures SQLite groupées et journaux via `spawn_blocking` |
| R3 | corrigé | clés TOML mortes supprimées (rupture assumée) |
| R4 | corrigé | surcharges d'environnement strictes, erreur si valeur invalide |
| R5 | corrigé | plus de décision HTTP sur `message.contains` |
| R6 | corrigé | tests CLI/TPU, Given/When/Then, zéro `unwrap` hors tests |
| R7 | corrigé | `unrar p` streamé, stderr drainé, partiel supprimé |
| R8 | partiel | blendshapes filtrés avant allocation, regex `OnceLock` ; buffers STEP4 inchangés |
| R9 | corrigé | `workflow-steps/src/media.rs` partagé |
| C1 | corrigé | message `Step 'STEPX' is not running` |
| C2 | corrigé | moniteur CSV publie `running`/`stopped` |
| C3 | documenté | D-025 (404 au lieu de 403) |
| C4 | documenté | dette D-015 inchangée |
| C5 | corrigé | `sync_all` + fsync du répertoire avant publication |
| C6 | corrigé | temporaires cachés `.nom.tmp.<pid>` |
| C7 | corrigé | `steps.step7.timeout_s` |
| C8 | documenté | D-024 inchangé |
| C9 | corrigé | `workflow_schema::round::round_to`, tests 1,8 s/3 600 s |

## 1. Résumé exécutif

L'architecture en neuf crates est cohérente et la séparation des
responsabilités est réelle : le serveur orchestre, la CLI exécute, les étapes
isolent leurs ressources dans des sous-processus, et les contrats JSON
conservent l'ordre d'insertion et, hors le cas C9, les arrondis Python. La
suite de tests est saine (233/233), rapide et correctement isolée, sans accès
matériel.

Le principal défaut est un **P0 d'orchestration** : entre `POST /stop` et la
mort effective du sous-processus, l'état publie `stopped` et libère le créneau
d'exécution. Une nouvelle étape peut démarrer pendant que l'ancienne se termine,
ce qui casse l'invariant « une seule étape à la fois » et peut faire écraser
l'état de la nouvelle exécution par le `finish_step` de l'ancienne.

Six sujets majeurs (P1) complètent le tableau : l'arrêt du serveur ne tue pas
les enfants, le verrou matériel ne couvre pas STEP1 (ni STEP5–8 hors serveur),
l'isolation réseau dépend d'un jeton publié par une page non authentifiée, le
verrou par défaut de `workflow-core` aliase le chemin du harnais Python,
l'arrondi local de STEP3 peut dériver de Python sur les demi-valeurs, et
l'extraction RAR charge chaque membre entier en RAM. Les deux derniers
proviennent de l'audit croisé avec le rapport Gemini Web (section 11) et ont
été reproduits à HEAD.

Le reste relève de la dette de qualité : observabilité du serveur presque
absente, canal de logs non borné, I/O bloquante sur le runtime, clés de
configuration mortes, allocations par frame dans STEP4, duplication de
`probe_mov_alpha`, quelques dérives de messages HTTP/CSV, et absence de tests
sur `workflow-cli` et `workflow-tpu`.

## 2. Preuves exécutées

| Vérification | Commande | Résultat |
| --- | --- | --- |
| Compilation | `cargo check --workspace --all-targets` | OK |
| Formatage | `cargo fmt --check` | OK, aucun écart |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | OK, 0 avertissement |
| Tests | `cargo test --workspace` | 233 réussis, 0 échec, ~4 s |
| Doublons | `cargo tree -d` | 4 familles transitives (voir ci-dessous) |
| Configuration | `workflow-cli --config config/config.toml config check` | `configuration valide` |
| État dépôt | `git status --porcelain` | propre |
| Ports | `ss -ltnp` | aucun serveur sur 5003/5004 |

Doublons de dépendances, tous transitifs et non actionnables directement :

- `base64` 0.22.1 (reqwest) / 0.23.1 (hyper-util) ;
- `hashbrown` 0.14.5 (via `hashlink`/`rusqlite`) / 0.17.1 (via `indexmap`) ;
- `smallvec` 1.16.3 / 2.0.0-alpha.10 (imposé par `ort` 2.0.0-rc.10) ;
- `syn` 2.0.119 (proc-macros OpenSSL) / 3.0.9 (Clap 4.6, displaydoc).

Répartition des tests : `workflow-steps` 121, `workflow-ingest` 42,
`workflow-server` 30 (26 lib + 4 binaire), `workflow-sync` 26,
`workflow-core` 7, `workflow-bridge` 5, `workflow-schema` 2,
`workflow-cli` 0, `workflow-tpu` 0. Cinquante et un fichiers contiennent un
module `#[cfg(test)]` ; plus de 90 tests utilisent `tempfile::tempdir`.

## 3. Axe 1 — Architecture, séparation des responsabilités et concurrence

**Points forts**

- Le serveur ne contient aucune logique métier : `catalog.rs` construit les
  commandes, `runner.rs` exécute un sous-processus, `state.rs` publie le
  contrat de polling.
- L'isolation par sous-processus évite tout partage d'état ORT, de session
  Coral ou de handle GPU entre deux étapes (`runner.rs:1-6`).
- `reserve_step` est atomique sous verrou d'écriture et refuse une deuxième
  étape (`state.rs:132-154`) ; `start_sequence` fait de même
  (`state.rs:156-173`).
- Les canaux de frames sont bornés : capacité 2 dans STEP2
  (`step2/mod.rs:371`), capacité 3 dans STEP4 (`step4/mod.rs:506`).
- STEP2 déporte l'inférence sur un thread dédié et STEP8 utilise un groupe de
  processus avec watchdog (`step2/mod.rs:378-392`, `step8.rs:645-669`).
- STEP1 borne sa parallélisation par deux pools Rayon distincts
  (`normalize.rs:554-572`, `normalize.rs:592-599`).

### A1 — P0 — Course `stop` → `reserve` et écrasement d'état

**Constat.** `stop_step` positionne `status = "stopped"` et
`return_code = -1` immédiatement, avant l'arrêt réel du processus
(`state.rs:303-329`). `stop_sequence` remet `sequence_running = false` dans la
même fenêtre (`state.rs:331-370`). Or `is_active()` ne considère actifs que
`starting`, `running` et `initiated` (`state.rs:485-487`) : `reserve_step`
accepte donc une nouvelle exécution pendant les 0 à 5 secondes que dure la
terminaison (`runner.rs:243-266`). Si la nouvelle exécution porte sur la même
étape, le `finish_step` tardif de l'ancienne écrase son enregistrement
(`state.rs:272-294`), y compris le `RunControl` nouvellement installé : `stop`
devient alors inopérant sur l'étape en cours.

**Scénario.** Lancer STEP2, appuyer sur Arrêter, relancer STEP2 ou une séquence
immédiatement : la seconde requête reçoit 202 au lieu de 409. Deux étapes
lourdes peuvent tourner simultanément (GPU/TPU), et l'interface affiche un état
incohérent.

**Recommandation.** Ajouter un identifiant de génération (`run_id`) dans
`RunControl` et `StepRecord`. Introduire un statut intermédiaire `stopping`
considéré comme actif. Ne libérer le créneau qu'après `child.wait()`. Ignorer
tout `finish_step` dont le `run_id` ne correspond plus au propriétaire courant.

**Tests à ajouter.** `stop` puis `reserve` immédiat doit renvoyer 409 ; un
`finish` périmé ne doit pas modifier le statut de la nouvelle exécution ;
`stop_sequence` puis `run_custom_sequence` immédiat doit renvoyer 409.

### A2 — P1 — L'arrêt du serveur laisse les enfants vivants

**Constat.** `shutdown_signal` n'arrête que le moniteur webhook
(`main.rs:88-113`). `scripts/switch-rust.sh:167-187` tue uniquement le PID du
serveur. Un `workflow-cli` en cours, et ses éventuels `ffmpeg`, survivent au
`SIGTERM` puis au `SIGKILL` du serveur, en conservant
`/tmp/workflow-rs-hw.lock` et les ressources GPU. Un redémarrage échoue
ensuite sur « verrou matériel indisponible » ; après un `SIGKILL`, STEP1 ou
STEP7 peuvent même se chevaucher avec l'orphelin.

**Recommandation.** Sur `SIGTERM`, demander l'arrêt de toutes les exécutions,
attendre un délai borné, puis tuer le groupe de processus. Lancer le serveur
dans son propre groupe (`setsid`) et adapter le script d'arrêt pour tuer le
groupe. Ajouter un test avec un faux CLI qui ignore `SIGTERM`.

### A3 — P1 — Portée incomplète du verrou matériel

**Constat.** Seuls STEP2 (`step2/mod.rs:216-226`), STEP3
(`step3/mod.rs:271-281`) et STEP4 (`step4/mod.rs:285-295`) prennent
`HardwareLock`. STEP1 normalise avec NVENC (`use_gpu = true`) sans verrou
(`step1/mod.rs:155`), et STEP5 à STEP8 n'en prennent pas non plus. Le serveur
sérialise, mais un `workflow-cli step1 run` manuel ou un cron peut tourner
pendant STEP2 et saturer le GPU, ou pendant STEP7 et supprimer une source en
cours de copie.

**Recommandation.** Prendre le verrou dans STEP1 autour de la normalisation
GPU. Documenter explicitement que STEP5–8 ne sont protégés que par le serveur,
ou déplacer l'acquisition au niveau du `workflow-cli` pour toutes les commandes
mutantes.

### A4 — P2 — Canal de logs non borné et lignes non bornées

**Constat.** `runner.rs:214` crée un `mpsc::unbounded_channel::<String>()`.
`pump` lit avec `read_until` sans plafond de taille (`runner.rs:341-362`).
STEP1 imprime chaque ligne de journal sur stdout (`step1/mod.rs:54`), ce qui
peut faire croître la mémoire si le producteur dépasse le consommateur. Les
écritures `std::fs` avec `flush` par ligne (`runner.rs:364-369`) s'exécutent de
plus sur un thread du runtime Tokio. Le même schéma existe dans STEP2 avec
`std::sync::mpsc::channel` (`step2/mod.rs:372`), mais l'impact y est faible :
les prédictions sont de toute façon conservées dans `predictions` jusqu'à la
fin (`step2/mod.rs:496-498`).

**Recommandation.** Canal borné (par exemple 1 024 messages), troncature des
lignes au-delà de 64 Kio, écriture groupée et déport via `spawn_blocking`.

### A5 — P2 — Pas de supervision des tâches, verrous empoisonnables

**Constat.** Les `tokio::spawn` de `http.rs:164` et de `sequence.rs:20-50` ne
sont jamais joints : une panique laisse l'étape en `starting` ou `running`
jusqu'au redémarrage. Tous les accès à `WorkflowState` utilisent
`.expect("état verrouillé")` (`state.rs:133` et suivants) : une panique sous
verrou empoisonne l'état et fait paniquer toutes les requêtes suivantes.

**Recommandation.** Utiliser `parking_lot::RwLock` (non empoisonnant) ou
récupérer le verrou via `into_inner()`. Encadrer l'exécution d'étape par un
garde RAII qui publie `failed` si la tâche se termine anormalement.

### A6 — P2 — Rotation des logs uniquement à l'ouverture

**Constat.** `rotate_log` n'est appelé qu'au démarrage d'une étape
(`runner.rs:385-416`). Une exécution peut dépasser `max_step_log_bytes`
(5 Mio) sans rotation. `read_log` lit ensuite le fichier entier en mémoire
(`http.rs:455`).

**Recommandation.** Vérifier la taille pendant l'écriture (tous les N blocs) et
lire les journaux en queue de fichier plutôt qu'en totalité.

### A7 — P2 — Progression de séquence non fiable

**Constat.** `sequence_status` renvoie toujours
`progress.total = StepKey::ALL.len()` (8) et `progress.current` comme le nombre
d'étapes `completed` de l'état global (`state.rs:431-452`), y compris celles
des exécutions précédentes. Pour une séquence personnalisée de trois étapes,
la progression affichée est fausse.

**Recommandation.** Mémoriser la liste d'étapes de la séquence courante dans
l'état et calculer la progression sur cette liste.

### A8 — P2 — Fuite des fichiers temporaires et orphelins

**Constat.** Les dossiers `_temp_*` de STEP1 ne sont nettoyés que par
`TempDirGuard` en sortie normale (`archive.rs:414-443`). Après `SIGKILL`, ils
restent et `retention.rs:459-470` les protège explicitement. Même problème pour
les `*.part` de téléchargement et les `*.tmp` d'écriture atomique. Aucun
balayage n'est fait au démarrage.

**Recommandation.** Purger au démarrage les `_temp_*`, `*.part` et `*.tmp` plus
vieux que N heures, uniquement sous `paths.base`.

### A9 — P2 — Pas de groupe de processus pour STEP1–4

**Constat.** Seul STEP8 crée un groupe de processus et tue `-pid`
(`step8.rs:645`, `step8.rs:809-817`). Pour STEP1 à STEP4, un `SIGTERM` au CLI
laisse `ffmpeg` orphelin jusqu'à ce qu'il écrive dans un tube fermé ; un
fichier `.temp_normalize.mp4` peut subsister.

**Recommandation.** Généraliser `process_group(0)` et la terminaison du groupe
aux sous-processus ffmpeg.

## 4. Axe 2 — Sécurité, isolation et secrets

**Points forts**

- `validate_server` refuse le port 5003 (`config.rs:330`), un token vide,
  `dev-internal-worker-token` ou préfixé `dev-` (`config.rs:361`), et un secret
  vide (`config.rs:355`).
- `validate_isolation` canonicalise `base`, `history_db`, `projects`,
  `archives`, `logs` et `downloads` et vérifie leur appartenance à `base`
  (`server/main.rs:122-180`).
- Le serveur refuse le verrou Python exact `/tmp/workflow-hw.lock`
  (`server/main.rs:176-178`) ; la CLI ne fait pas ce contrôle (voir S2).
- L'extraction d'archives assainit chaque composant et n'accepte que des
  `Component::Normal` (`sanitize.rs:50-122`, `sanitize.rs:193-200`) ; TAR
  refuse les entrées non régulières (`archive.rs:525-533`).
- `sync --apply --allow-running` est refusé (`main.rs:1357-1361`) et la
  synchronisation exige l'arrêt des serveurs (`sync/lock.rs:21-58`).
- `config show` masque `secret_key` et `worker_token`
  (`main.rs:763-780`).

### S1 — P1 — Jeton worker publié par une page non authentifiée

**Constat.** `index` injecte `worker_token` dans le HTML rendu
(`http.rs:101-111`), sur une route publique. Toute personne pouvant joindre le
port lit le jeton puis peut appeler `/run`, `/stop` et `/sequence/stop`. La
protection par en-tête ne bloque donc pas un client direct ; elle ne protège
que contre les POST sans en-tête (CSRF simple).

Le risque dépend de `server.host`, aujourd'hui `127.0.0.1`, mais rien ne
valide que l'hôte reste loopback : `host = "0.0.0.0"` exposerait le jeton au
réseau local. Aucun reverse proxy n'est installé (cf. `memory-bank/activeContext.md`).

**Recommandation.** Refuser un `server.host` non-loopback sauf option explicite
`server.allow_remote = true`, avec avertissement au démarrage. À terme, sortir
le jeton du HTML au profit d'une session signée, mais ce changement modifie le
contrat frontend et doit être arbitré. À défaut, documenter que le serveur ne
doit jamais être exposé sans proxy authentifiant.

### S2 — P1 — Le verrou par défaut aliase le chemin du harnais Python

**Constat.** `HardwareConfig::default()` construit
`$XDG_RUNTIME_DIR/workflow-hw.lock` (`config.rs:990-1003`), qui est exactement
le chemin pris par le harnais de parité (D-012) ; si `XDG_RUNTIME_DIR` est
absent, le repli est `/tmp/workflow-hw.lock`. `validate_isolation` ne rejette
que la chaîne exacte `/tmp/workflow-hw.lock` (`server/main.rs:176-178`) : le
chemin XDG passe donc, et `workflow-cli` ne l'appelle même pas
(`cli/main.rs:756` ne fait que `validate_common`, qui ne contrôle pas
`hardware.lock_path`). Une configuration sans section `[hardware]` peut donc
utiliser le verrou du harnais, contrairement à l'invariant « verrou Rust
distinct ».

**Recommandation.** Rejeter dans `validate_common` tout chemin dont le nom de
fichier est `workflow-hw.lock`, faire de `/tmp/workflow-rs-hw.lock` la valeur
par défaut, et ajouter un test qui charge une configuration sans `[hardware]`
et vérifie le chemin Rust.

### S3 — P2 — `secret_key` obligatoire mais inutilisé

**Constat.** `config.rs:355` exige `server.secret_key`, mais aucun mécanisme de
session, cookie ou CSRF ne l'utilise côté Rust. Ce secret donne une fausse
impression de protection.

**Recommandation.** Soit le retirer de la validation, soit l'utiliser lors du
passage à une session signée.

### S4 — P2 — L'isolation est dans le binaire, pas dans `workflow-core`

**Constat.** `validate_isolation` vit dans `crates/workflow-server/src/main.rs`
et n'est pas réutilisable par la CLI, ni testée au niveau de `workflow-core`.
Le contrôle anti-dépôt legacy compare en outre `resolved.projects` non
canonicalisé (`server/main.rs:167-175`).

**Recommandation.** Déplacer la fonction dans `workflow-core` sous forme de
`validate_isolation()`, l'appeler depuis `validate_server`, et y déplacer les
tests.

### S5 — P2 — Comparaison de jeton non constante, absence d'en-têtes de sécurité

**Constat.** `http.rs:372` compare avec `!=`. Aucun
`Content-Security-Policy` ni `X-Content-Type-Options` n'est ajouté à la page
d'index.

**Recommandation.** Utiliser `subtle::ConstantTimeEq` et ajouter les en-têtes
de sécurité statiques.

### S6 — P2 — Surface SSRF du téléchargeur

**Constat.** `plan_downloads` accepte toute URL http(s) dont le provider est
`dropbox` ou qui contient `/dropbox/` et `worker`, puis `reqwest` suit les
redirections sans liste d'hôtes (`download.rs`, `url.rs:340-354`). La source
est le webhook, mais une compromission de celui-ci permettrait de viser
`http://127.0.0.1:...`.

**Recommandation.** Refuser les adresses loopback/link-local après résolution,
limiter les redirections et journaliser l'hôte final.

### S7 — P2 — Pas de quota d'extraction, suivi des symlinks

**Constat.** `write_member` copie sans limite la taille décompressée
(`archive.rs:621-659`), ce qui expose à une bombe ZIP. `copy_tree` de STEP7
suit les symlinks (`step7.rs:328-348`). Les archives viennent du webhook, mais
un budget de taille est peu coûteux.

Le découpage des membres ne traite que `/` (`sanitize.rs:78-91`) : un membre
`dossier\..\..\evil.mp4` conserve ses antislashs et devient un nom de fichier
littéral sous Linux. `validate_extraction_path` ne voit qu'un
`Component::Normal`, donc il n'y a pas d'évasion du répertoire d'extraction ;
c'est un défaut d'hygiène des noms et un compteur `security_issues` non
bloquant, pas une traversée sur la cible Linux.

**Recommandation.** Vérifier `entry.size()` et un budget global d'extraction ;
refuser les entrées non régulières à la copie STEP7. Normaliser `\` en `/`
avant le découpage, après vérification du comportement legacy, ou rejeter
explicitement les membres contenant `\`.

### S8 — P2 — Journaux et chemins exposés sans authentification

**Constat.** `/status/*`, `/get_specific_log/*`, `/api/csv_*` et
`/api/system_monitor` sont publics, conformément au contrat.
`/get_specific_log` renvoie le chemin absolu et lit le fichier entier
(`http.rs:313-336`, `http.rs:447-471`), en suivant les symlinks.

**Recommandation.** Acceptable en loopback ; à restreindre si le serveur est
exposé. Vérifier que le chemin résolu reste sous `paths.logs`.

### S9 — P2 — Secrets locaux exposés dans l'export d'audit

**Constat.** Le rapport Gemini signale que `config/launcher.env` figurait dans
l'export Repomix transmis pour analyse, avec `WORKFLOW_SERVER_SECRET_KEY` et
`WORKFLOW_SERVER_WORKER_TOKEN` en clair. Localement, le fichier est non
versionné, ignoré (`.gitignore:26`) et en mode 600 ; aucun `.repomixignore` ni
configuration d'export n'existe dans le dépôt. Le `secret_key` est inutilisé
par Rust (S3) et le jeton worker ne garde qu'un serveur loopback, ce qui borne
l'impact, mais l'export a quitté la machine.

**Recommandation.** Régénérer les deux secrets (`openssl rand -hex 32`),
redémarrer le serveur, ajouter un `.repomixignore` excluant
`config/launcher.env`, `runtime/`, `parity/cases`, `parity/work`,
`parity/fixtures`, `assets/models`, `vendor` et `target`, et vérifier l'archive
avant tout partage. Classé P2, et non P0 comme dans le rapport Gemini, parce
que le fichier n'est pas versionné et que le serveur n'écoute qu'en loopback ;
la rotation reste immédiate.

## 5. Axe 3 — Robustesse, gestion des erreurs et performance

**Points forts**

- Un seul `.unwrap()` hors tests dans tout le workspace
  (`step2/windows.rs:139`), protégé par le retour anticipé sur
  `frames.is_empty()`.
  Les `.expect()` restants couvrent des invariants locaux (regex statiques,
  mutex, frames non vides, étape initialisée).
- SQLite en WAL, `synchronous=NORMAL`, `busy_timeout` de 30 s, transactions et
  vérification du nombre de lignes après écriture
  (`history.rs:44-78`, `sync/history.rs:353-398`).
- Le téléchargement est reprenable, valide la taille et le ZIP, classe les
  erreurs en types et nettoie ses `.part` en cas d'échec
  (`download.rs:509-577`, `download.rs:852-870`).
- La synchronisation sauvegarde les deux bases avec `VACUUM INTO` avant
  d'écrire (`sync/history.rs:162-190`, `sync/history.rs:353-373`).
- STEP8 possède un watchdog, un groupe de processus et un handler
  `SIGTERM`/`SIGINT` avec double frappe (`step8.rs:644-669`,
  `step8.rs:776-805`).
- Aucun `TODO`/`FIXME`/`HACK` dans le code.

### R1 — P2 — `anyhow` dans les bibliothèques

**Constat.** `codingstandards.md` impose `thiserror` en bibliothèque et
`anyhow` aux frontières. `workflow-ingest`, `workflow-sync` et
`workflow-steps` (steps 2, 3, 4, 7, 8) exposent des `anyhow::Result` publics.
Le serveur ne voit alors qu'un code de sortie, sans type d'erreur.
`workflow-schema` déclare `thiserror` dans `Cargo.toml:10` mais ne l'utilise
pas ; `workflow-ingest` expose 11 occurrences d'`anyhow`.

**Recommandation.** Soit assumer et documenter la dérogation, soit introduire
des enums `thiserror` par crate avec conversion aux frontières.

### R2 — P2 — I/O bloquante sur le runtime Tokio

**Constat.** Un seul `spawn_blocking` dans tout le workspace
(`download.rs:870`, validation ZIP). Les appels SQLite
(`history.rs:84-87`, utilisés par `service.rs:193-194` et
`service.rs:119-120`) et la lecture des journaux (`http.rs:455`) s'exécutent
directement sur les threads du runtime.

**Recommandation.** Déporter les lectures de journaux et les rafales SQLite via
`spawn_blocking` ; le reste est de faible volume et peut rester synchrone.

### R3 — P2 — Clés de configuration mortes

**Constat.** Plusieurs clés sont déclarées, validées ou documentées mais jamais
lues : `steps.step3.max_workers` (la boucle STEP3 est séquentielle),
`hardware.gpu_mem_limit_mb`, `hardware.require_tpu`, `hardware.tpu_device`,
`server.polling_interval_ms`, `server.system_monitor_interval_ms`,
`server.snapshot_interval_s`, `server.slow_api_threshold_ms`,
`server.gpu_monitoring`, `server.debug`, `download.chunk_bytes` (documenté) et
`server.secret_key` (voir S3).

**Recommandation.** Supprimer les clés mortes ou les câbler ; un opérateur qui
règle `max_workers` ou `gpu_mem_limit_mb` n'obtient aujourd'hui aucun effet.

### R4 — P2 — Surcharges d'environnement silencieusement ignorées

**Constat.** `apply_env_overrides` ignore toute valeur non parsable
(`config.rs:53-154`). Un `WORKFLOW_SERVER_PORT=abc` ou un
`WORKFLOW_WEBHOOK_ENABLED=oui` laisse la valeur TOML sans avertissement.

**Recommandation.** Journaliser un avertissement ou refuser le démarrage en
cas de valeur invalide.

### R5 — P2 — Décision HTTP par correspondance de chaînes

**Constat.** `http.rs:154-159` et `http.rs:205-211` choisissent 409 vs 500/400
avec `message.contains("en cours")`. Une reformulation du message change le
code HTTP.

**Recommandation.** Retourner un enum d'erreur (`AlreadyRunning`,
`NotRunning`) et mapper vers le code HTTP.

### R6 — P2 — Absence de CI et angles morts de test

**Constat.** Aucune CI. `workflow-cli` (1 460 lignes de câblage) et
`workflow-tpu` n'ont aucun test ; `workflow-schema` en a deux. Aucun test ne
couvre A1, A2, les bombes ZIP, les lignes géantes ou l'écrasement d'un dossier
`/mnt/cache` par STEP7. `cargo-llvm-cov` n'est pas installé, donc la couverture
n'est pas mesurable localement. Enfin, `test-strategy.md` demande des
commentaires `Given/When/Then` : aucun test n'en contient.

Le seul `.unwrap()` hors tests (`step2/windows.rs:139`) est protégé par le
retour anticipé `frames.is_empty()` (`windows.rs:128-130`) ; il reste une
entorse stylistique au standard zéro-unwrap, à remplacer par
`let Some(last) = frames.last() else { return Vec::new() }`. `step1 run` n'a
pas de `--dry-run` ; l'asymétrie avec STEP7/STEP8/retention/sync est documentée
dans `AGENTS.md:86` mais mérite un ticket produit. `workflow-cli` n'a pas de
cible lib : `cargo test -p workflow-cli --bins` exécute 0 test.

**Recommandation.** Ajouter les tests listés en section 8, quelques tests de
construction de commandes dans `workflow-cli`, et adopter la convention
Given/When/Then.

### R7 — P1 — Extraction RAR chargée intégralement en RAM

**Constat.** `step1/archive.rs:593-607` exécute `unrar p` avec
`Command::output()` puis enveloppe `output.stdout` dans un `io::Cursor`.
L'intégralité du membre décompressé est donc matérialisée en mémoire avant
l'écriture, contrairement à ZIP et TAR qui streament via `io::copy`. Une vidéo
de plusieurs gigaoctets dans un RAR peut provoquer un OOM de STEP1.
`write_member` accepte déjà un `&mut impl Read` (`archive.rs:621-627`), donc le
correctif est local.

**Recommandation.** Lancer `unrar` avec `stdout`/`stderr` en pipes, passer
`stdout` à `write_member`, puis `wait_with_output()` pour drainer `stderr` et
vérifier le code de retour ; supprimer le fichier partiel en cas d'échec.
Ajouter un test avec un faux `unrar` qui émet un flux volumineux sans tout
matérialiser.

### R8 — P2 — Allocations par frame et regex recompilées

**Constat.** `BlendFilter::filter` construit les 52 `String` de
`ARKIT_52_BLENDSHAPE_NAMES` avant même de filtrer le profil
(`step4/tracking.rs:244-251`), y compris pour `none` ou une seule clé. Les
tampons `xofs`, `xalpha`, `row0`, `row1` et les images de sortie sont réalloués
à chaque frame (`step4/image.rs:80-102`, `step4/image.rs:318`). Côté ingestion,
`Regex::new` est appelé à chaque téléchargement (`download.rs:684`,
`download.rs:769-770`).

**Recommandation.** Précalculer les indices de blendshapes et filtrer avant
toute allocation ; réutiliser des buffers par thread dans STEP4 ; passer les
regex en `OnceLock<Regex>`.

### R9 — P2 — Duplication de `probe_mov_alpha` et `is_preserved_media`

**Constat.** `probe_mov_alpha` est dupliqué à l'identique dans
`step2/mod.rs:696`, `step3/mod.rs:482` et `step4/mod.rs:891` (environ 60 lignes
chacun). `is_preserved_media` existe aussi en trois exemplaires
(`step2/mod.rs:656`, `step3/mod.rs:456`, `step4/mod.rs:853`), alors que
`step2::is_preserved_media_with` est déjà public et utilisé par STEP1/STEP5.

**Recommandation.** Extraire ces helpers dans un module partagé
(`workflow-steps/src/media.rs` par exemple), avec des tests uniques, et faire
pointer les trois étapes dessus.

## 6. Axe 4 — Contrats JSON/CSV/HTTP et parité

**Points forts**

- `serde_json` est déclaré avec `preserve_order` dans toutes les dépendances du
  workspace qui l'utilisent ; l'ordre d'insertion est donc préservé.
- Les arrondis passent par `workflow_schema::round::{round2, round4, round_to}`
  (36 références, tests inclus) ; les demi-valeurs Python sont testées
  (`round.rs:24-29`). **Exception :** STEP3 redéfinit localement `round_to`
  (voir C9).
- Les artefacts critiques sont écrits en temporaire puis renommés :
  `_tracking.json` (`step4/json.rs:91-128`, avec `sync_all`), `_audio.json`
  (`step5/audio.rs:187-210`), `_ae.json` (`step6.rs:1264-1278`), CSV STEP2
  (`step2/windows.rs:208-215`), manifestes STEP7/STEP8 et sortie du bridge.
- `_audio.json` écrit `fps`/`total_frames` avant `frames_analysis` (D-003).
- STEP4 exige CSV et audio par défaut et refuse le succès partiel silencieux
  (D-004, `step4/mod.rs:446-472`).
- Le scan STEP1 reproduit la liste de suffixes réellement effective (D-016).
- Le registre `parity/DRIFT.md` contient bien les 24 entrées D-001 à D-024,
  aucune suppression constatée.

### C1 — P2 — Message `/stop` non conforme

**Constat.** `state.rs:310` renvoie « L'étape n'est pas en cours. » alors que
le contrat (`docs/CONTRAT-ENVIRONNEMENT.md:189`) spécifie
`Step 'STEP3' is not running`, et précise ligne 195 que le frontend dépend de
`"not running"`. Le frontend actuel utilise `/cancel`, donc l'impact est
latent, mais la dérive est réelle.

**Recommandation.** Aligner le message sur le contrat ou documenter la dérive
dans `parity/DRIFT.md`.

### C2 — P2 — Statut du moniteur CSV non conforme

**Constat.** `service.rs:155-176` publie `checking`, `active` ou `error`,
alors que le contrat (`docs/CONTRAT-ENVIRONNEMENT.md:138`) attend
`running|stopped`. Un consommateur qui teste `status === "running"` ne verra
jamais le moniteur actif.

**Recommandation.** Publier `running` quand le moniteur est actif et conserver
`stopped` à l'arrêt.

### C3 — P2 — Traversée `/sound-design` : 404 au lieu de 403

**Constat.** Le contrat (`docs/CONTRAT-ENVIRONNEMENT.md:192`) attend
`403 {"error":"Access denied"}`. `ServeDir` (`http.rs:80`) renvoie 404. La
sécurité est bonne (pas de fuite), seul le code diffère.

**Recommandation.** Soit ajouter un middleware dédié, soit documenter le 404.

### C4 — P2 — Routes de contrat absentes (dette documentée D-015)

**Constat.** `/api/cache/*`, `/api/stats/*`, `/api/system/diagnostics`,
`/api/performance/metrics|reset` et `/api/audio/*` ne sont pas routées. C'est
assumé par D-015, mais le serveur Rust étant le backend de production `:5004`,
ces panneaux de l'interface legacy sont indisponibles dès que le frontend est
servi par Rust.

Vérification du frontend courant : `assets/web/static` n'appelle que
`/api/csv_downloads_status`, `/api/csv_monitor_status`,
`/api/performance/frontend` et `/api/system_monitor`, toutes routées. Les
composants `stats-dashboard.css` et `#clear-cache-global-button` sont
orphelins (CSS), mais aucun appel JS vers `/api/cache/*`, `/api/stats/*`,
`/api/system/diagnostics` ou `/api/audio/*` n'a été trouvé. Le risque de bruit
dans `#notifications-area` n'est donc pas avéré à HEAD ; il le deviendrait si
le frontend était resynchronisé avec une version Python plus récente.

**Recommandation.** Traiter comme dette produit explicite : soit un proxy vers
Python pour ces routes, soit leur portage ultérieur.

### C5 — P2 — Atomicité sans durabilité

**Constat.** Les écritures `step3/mod.rs:927-975`, `step5/audio.rs:187-210`,
`step6.rs:1264-1278`, `step7.rs:640-643`, `step8.rs:906-910` et
`bridge/main.rs:61-76` font temporaire + `rename` sans `fsync` du fichier ni du
répertoire. Une coupure d'alimentation peut laisser un fichier vide ou tronqué.
Seul STEP4 fait `sync_all()` (`step4/json.rs:99`).

**Recommandation.** Ajouter `sync_all()` avant `rename` sur les artefacts
critiques, ou documenter que D-008 n'exige que l'atomicité logique.

### C6 — P2 — Noms de temporaires dérivés par `with_extension("tmp")`

**Constat.** `_audio.json` devient `_audio.tmp`, `_ae.json` devient
`_ae.tmp` : collision possible avec un fichier utilisateur du même nom et
résidus non nettoyés après `SIGKILL`. STEP4 utilise un nom caché préfixé
(`.nom.tmp`), plus sûr.

**Recommandation.** Aligner les autres étapes sur le modèle de STEP4 et
nettoyer les résidus au démarrage.

### C7 — P2 — Timeout STEP7 non configurable

**Constat.** `catalog.rs:98-104` applique `default_timeout_s` (1 800 s) à
STEP7. Une finalisation volumineuse vers `/mnt/cache` peut être `SIGKILL`ée en
pleine copie.

**Recommandation.** Ajouter `steps.step7.timeout_s`, comme pour STEP1 et
STEP8.

### C8 — P2 — D-024 reste ouvert

**Constat.** Le STEP4 Python régénéré sur `153 Camille` diffère de l'archive
(4 977 objets contre 11 853). Ce n'est pas un défaut Rust, mais c'est le seul
risque de parité non résolu.

**Recommandation.** Conserver la note D-024, ne pas utiliser `--prefer-raw`
sur ce projet, et documenter la configuration d'origine dès qu'elle est
retrouvée.

### C9 — P1 — Arrondi STEP3 hors contrat

**Constat.** `step3/mod.rs:1008-1011` redéfinit localement `round_to` par
`(value * factor).round_ties_even() / factor`, au lieu d'utiliser
`workflow_schema::round::round_to` imposé par `codingstandards.md`. Python
calcule `round((speaker_times[speaker] / total_speech) * 100, 1)`
(`workflow_scripts/step3/run_audio_diarization_tpu.py:1371-1372`) ; la
multiplication préalable décale
la représentation binaire et peut changer le résultat sur les demi-valeurs.
Exemple concret : pour un locuteur de 1,8 s dans 3 600 s de parole, la valeur
est 0,05 ; Python renvoie 0,1, la formule locale renvoie 0,0. Un second cas
(5,4 s) donne 0,1 côté Python et 0,2 côté Rust. Les tests locaux
(`step3/mod.rs:1048-1051`) ne couvrent que 0,125 et 0,135 et ne détectent pas
ce cas ; la fixture `vox_step3` ne tombe pas sur une demi-valeur et ne le
détecte pas non plus.

**Recommandation.** Importer `workflow_schema::round::round_to`, supprimer la
fonction locale et ses tests, et ajouter un test de non-régression avec le cas
1,8 s / 3 600 s, plus un test comparatif aléatoire entre les deux
implémentations.

## 7. Matrice de priorisation

| Priorité | Nombre | Éléments |
| --- | ---: | --- |
| **P0** | 1 | A1 — course `stop` → `reserve` / `finish` |
| **P1** | 6 | A2 arrêt serveur → enfants orphelins ; A3 verrou matériel incomplet ; S1 jeton publié par `/` et hôte non validé ; S2 verrou par défaut aliasant le harnais ; C9 arrondi STEP3 hors contrat ; R7 extraction RAR en RAM |
| **P2** | 29 | A4–A9, S3–S9, R1–R6 et R8–R9, C1–C8 |

Le décompte initial de la matrice (23 P2) était erroné : les identifiants
A4–A9, S3–S8, R1–R6 et C1–C8 totalisent 26 constats. La matrice ci-dessus
corrige ce total et inclut les cinq constats issus de l'audit croisé
(section 11).

Lecture : aucun P0 de données. Les artefacts JSON/CSV sont atomiques et
conformes ; le P0 est un défaut d'orchestration, précisément la fonction
centrale du serveur. Les P1 sont des risques d'exploitation (redémarrage, GPU,
exposition réseau, verrou, mémoire RAR) et une dérive d'arrondi latente.

## 8. Plan de remédiation proposé

1. **P0 — Sérialisation stricte.**
   `run_id` + statut `stopping` actif ; `finish` idempotent et générationnel ;
   tests `stop → run = 409`, `finish` périmé sans effet,
   `stop_sequence → nouvelle séquence = 409`.
2. **P1 — Arrêt propre.**
   Hook d'arrêt qui annule et attend les enfants (`SIGTERM`, 5 s, `SIGKILL`
   groupe), `setsid` au lancement, mise à jour de `switch-rust.sh stop` ;
   test avec un faux CLI ignorant `SIGTERM`.
3. **P1 — Verrou.**
   STEP1 sous `HardwareLock` autour de la normalisation GPU ; défaut
   `/tmp/workflow-rs-hw.lock` ; rejet de tout basename `workflow-hw.lock`
   dans `validate_common` ; test dans `workflow-core`.
4. **P1 — Isolation et secret.**
   Déplacer `validate_isolation` dans `workflow-core`, l'appeler depuis
   `validate_server` ; refuser un `host` non-loopback sans option explicite ;
   comparaison de jeton constante ; décider du sort de `secret_key`.
5. **P1 — Parité STEP3.**
   Remplacer le `round_to` local par `workflow_schema::round::round_to` ;
   test de non-régression 1,8 s / 3 600 s (0,1 attendu) et test comparatif
   aléatoire entre les deux implémentations.
6. **P1 — Extraction RAR.**
   Streamer `unrar p` vers `write_member`, drainer `stderr`, supprimer le
   fichier partiel en cas d'échec ; test avec un faux `unrar` émettant un flux
   volumineux.
7. **P1 — Observabilité.**
   `TraceLayer` (la feature est déjà activée) ; `tracing::info/warn/error`
   dans `Runner` sur début, fin, code de retour, durée et chemin de journal ;
   journalisation des 4xx/5xx.
8. **P2 — Contrats.**
   Aligner le message `/stop` et `csv_monitor.status` ; clarifier
   `/sound-design` ; documenter D-015 comme dette assumée.
9. **P2 — Robustesse.**
   Canal borné et plafond de ligne ; `spawn_blocking` pour `read_log` et
   SQLite ; `fsync` des artefacts critiques ; purge des `_temp_*`/`.part`/
   `.tmp` au démarrage ; supprimer ou câbler les clés mortes ;
   `parking_lot` ou récupération de poison.
10. **P2 — Performance et dette.**
    Précalculer les blendshapes et réutiliser les buffers STEP4 ;
    `OnceLock<Regex>` dans le téléchargement ; extraire
    `probe_mov_alpha`/`is_preserved_media` dans un module partagé ; rotation
    des secrets et `.repomixignore` pour les exports.
11. **P2 — Tests.**
    Ajouter les cas ci-dessus, des tests de construction de commandes dans
    `workflow-cli`, un test de contrat HTTP exhaustif, et adopter
    `Given/When/Then`.

Les décisions durables issues de ce plan (sérialisation générationnelle,
sémantique d'arrêt, portée du verrou et isolation) devront être consignées
dans `memory-bank/decisionLog.md` au moment de leur implémentation.

## 9. Limites de l'audit

- Audit statique : la suite de tests ne charge ni modèle, ni GPU, ni Coral.
  Une suite verte ne prouve pas la parité STEP1–STEP8.
- Aucune mesure de performance réelle (débit, VRAM, RSS) n'a été refaite ; les
  chiffres de référence restent ceux de D-009.
- Aucun test de charge ni de coupure d'alimentation n'a été exécuté.
- Les routes legacy non portées (D-015) et la parité STEP4 (D-024) sont des
  sujets produits, pas des défauts de code Rust.
- La couverture de branches n'est pas instrumentée (`cargo-llvm-cov` absent).
- L'audit croisé Gemini Web (section 11) s'appuie sur un export Repomix
  tronqué ; chaque constat a été revérifié à HEAD, mais l'archive d'origine
  n'était pas disponible localement.
- La divergence d'arrondi STEP3 est démontrée par lecture du code Python et par
  simulation numérique, pas par une exécution STEP3 réelle.

## 10. Annexes

### 10.1 Checklist des instructions de la mission

| Étape | Statut | Commentaire |
| --- | --- | --- |
| 1. Santé statique et compilation | Fait | check, fmt, clippy, tests, `cargo tree -d` |
| 2. Architecture serveur | Fait | A1, A2, A4–A7, S1, S5 |
| 3. Sécurité, authentification, confinement | Fait | S1–S9, A3 |
| 4. Concurrence et ressources | Fait | A1–A4, A9, R2, R7 |
| 5. Robustesse et erreurs | Fait | R1–R9, A5, A8 |
| 6. Contrats et persistance | Fait | C1–C9, D-001 à D-024 |
| 7. Tests et assurance qualité | Fait | 233/233, R6 |
| 8. Restitution | Ce document | 4 axes, priorités P0/P1/P2 |
| 9. Audit croisé Gemini Web | Fait | 24 constats triés ; 2 P1, 3 P2 retenus ; 1 faux positif ; 2 hors périmètre |

### 10.2 Commandes de reproduction

```sh
cd /home/kidpixel/workflow-rs

cargo check --workspace --all-targets
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo tree -d

./target/release/workflow-cli --config config/config.toml config check
git status --porcelain
ss -ltnp | grep -E ':500[34]' || true
```

### 10.3 Fichiers examinés en priorité

- `crates/workflow-server/src/{state,runner,http,main,sequence,catalog,db,progress}.rs`
- `crates/workflow-core/src/{config,lock,paths,error}.rs`
- `crates/workflow-ingest/src/{service,history,download,monitor,webhook,tracker,url}.rs`
- `crates/workflow-steps/src/{step1,step2,step3,step4,step5}/`, `step6.rs`,
  `step7.rs`, `step8.rs`, `retention.rs`
- `crates/workflow-sync/src/{lib,lock,history,processed,archives,report}.rs`
- `crates/workflow-bridge/src/{main,cuts}.rs`
- `crates/workflow-tpu/src/{session,ffi}.rs`
- `crates/workflow-cli/src/main.rs`
- `config/config.toml`, `scripts/switch-rust.sh`, `parity/DRIFT.md`,
  `docs/CONTRAT-ENVIRONNEMENT.md`, `memory-bank/`.

## 11. Audit croisé avec le rapport Gemini Web

### 11.1 Méthode

Le rapport Gemini Web était composé de trois fichiers, retirés du dépôt après
synthèse pour ne conserver qu'un rapport unique. Chaque constat avait été relu,
puis revérifié indépendamment sur le code à HEAD `ae2098d` : grep ciblé,
lecture des fonctions, simulation numérique pour les arrondis, contrôle du
frontend et des fixtures de parité. Les conclusions du rapport Gemini n'ont pas
été reprises sur parole.

Deux limites de l'export Gemini expliquent plusieurs de ses angles morts : les
volets 1 et 2 indiquent une troncature au milieu de `AppState.js` et de
`step1/archive.rs`, alors que le volet 3 revendique une inspection à 100 % de
STEP1–STEP5. Cette contradiction impose la revérification locale. L'archive
Repomix elle-même n'est pas présente dans le dépôt.

### 11.2 Nouveaux constats retenus

| Réf. Gemini | Constat | Vérification à HEAD | Devenir |
| --- | --- | --- | --- |
| STP3-01 | `round_to` local dans STEP3 | Confirmé : `step3/mod.rs:1008-1011` ; écart concret 1,8 s / 3 600 s (Python 0,1 ; Rust 0,0) | **C9 P1** |
| STP1-02 | Extraction RAR en RAM | Confirmé : `archive.rs:593-607`, `Command::output()` puis `Cursor` | **R7 P1** |
| SEC-01 | `config/launcher.env` dans l'export | Fichier local non versionné, ignoré et en 600 ; export Gemini non disponible | **S9 P2** |
| STP4-01 + ING-02 | Allocations STEP4 et regex par téléchargement | Confirmé : `tracking.rs:244-251`, `image.rs:80-102`, `download.rs:684/769-770` | **R8 P2** |
| STP-DUP-01 | Duplication `probe_mov_alpha` / `is_preserved_media` | Confirmé : `probe_mov_alpha` (~60 lignes) et `is_preserved_media` en trois exemplaires | **R9 P2** |

### 11.3 Enrichissements de constats existants

| Réf. Gemini | Apport | Vérification à HEAD | Intégration |
| --- | --- | --- | --- |
| CFG-01 | Défaut `workflow-hw.lock` et CLI sans `validate_isolation` | Confirmé : repli `/tmp/workflow-hw.lock` ; `cli/main.rs:756` | S2 |
| SRV-01 | Canal de logs non borné | Confirmé | A4 |
| STP2-02 | Canal de résultats STEP2 non borné | Confirmé : `std::sync::mpsc` ; impact faible car `predictions` est de toute façon accumulé | A4 |
| STP2-01 | `.unwrap()` résiduel dans `reference_windows` | Confirmé et protégé par `frames.is_empty()` | R6 |
| AUD-03 / CLI-01 | `step1 run` sans `--dry-run` | Confirmé ; limitation déjà documentée dans `AGENTS.md:86` | R6 |
| AUD-04 | Appels frontend vers les routes D-015 | Non reproduit : aucun appel JS ; seuls des composants CSS orphelins subsistent | C4 |
| STP1-01 | Antislashs non découpés dans les noms de membres | Confirmé, mais sans traversée sur Linux : le nom reste un `Component::Normal` | S7 |
| ERR-01 / ERR-02 | `anyhow` en bibliothèque et `thiserror` inutilisé | Confirmé : 11 occurrences dans `workflow-ingest`, `thiserror` déclaré sans usage dans `workflow-schema` | R1 |

### 11.4 Doublons confirmés

| Réf. Gemini | Constat déjà couvert par |
| --- | --- |
| SRV-02 | A6 — rotation des logs uniquement à l'ouverture |
| ING-01 | R2 — SQLite sur les threads Tokio |
| SEC-02 | S5 — comparaison de jeton non constante |
| AUD-02 | R6 — `workflow-tpu` sans test |
| AUD-05 | R6 — absence de CI et de couverture |

Les doublons SRV-01, SRV-02 et ING-01 sont classés P1 par Gemini ; nous les
conservons en P2 (A4, A6, R2) car ils dégradent l'observabilité ou la réactivité
sans casser l'invariant d'exécution ni les contrats.

### 11.5 Constats non retenus

- **AUD-01 (P0 Gemini)** : la troncature de l'export Repomix est une limite de
  l'audit Gemini, pas un défaut du backend. Elle est mentionnée en 11.1.
- **AUD-06** : faux positif à HEAD. `parity/README.md` utilise déjà
  `../workflow_mediapipe/coral_env/bin/python` (lignes 30, 33, 36, 39, 42, 47,
  50 et 139) et précise qu'il n'existe pas de `coral_env/` local.
- **AUD-07** : l'écriture non atomique de `MediaSolution.jsx` relève du
  panneau EffectCraft, hors périmètre du backend Rust (D-022).
- **SEC-01, sévérité** : requalifié P2 et non P0. Le fichier n'est pas
  versionné, il est en mode 600 et le serveur n'écoute qu'en loopback ; la
  rotation des secrets reste recommandée (S9).
- **STP1-01, sévérité** : requalifié P2 et non P1. Les antislashs produisent un
  nom de fichier littéral sous Linux, pas une évasion du répertoire
  d'extraction (S7).
- **STP2-02, sévérité** : le canal non borné de STEP2 double temporairement des
  prédictions qui sont de toute façon conservées jusqu'à la fin ; l'impact est
  faible (A4).
- **Incohérence interne** : le volet 2 affirme « 0 `.unwrap()` hors tests »,
  alors que le volet 3 en signale un. HEAD en contient bien un, protégé
  (`step2/windows.rs:139`).

### 11.6 Bilan

Sur les 24 constats Gemini, 6 donnent 5 nouveaux constats (STP4-01 et ING-02
fusionnent en R8) : 2 P1 (C9, R7) et 3 P2 (S9, R8, R9) ; 10 enrichissent ou
clarifient un constat existant, 5 sont des doublons et 3 ne sont pas retenus.
Deux sévérités sont requalifiées (SEC-01 et STP1-01 en P2). Le rapport compte
désormais **36 constats : 1 P0, 6 P1 et 29 P2**.
