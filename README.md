# workflow-rs

Réécriture Rust du pipeline `workflow_mediapipe`. Rust est le backend de
production depuis le 2026-10-23 ; le dépôt Python reste la référence de parité
et le rollback, sans être arrêté ni modifié par la bascule. Le lot 0 a validé le
pont matériel Coral, le contrat d'environnement et le harnais de parité.
Le lot 1 est terminé : le socle Rust, la configuration TOML, STEP5
audio/vidéo, STEP6 (`_ae.json`), le pont After Effects (`cuts` et
`analyze_manifest`), STEP7 (finalisation) et STEP8 (EffectCraft via
`effectcraft-cli`) sont opérationnels. Le post-traitement A5/A6 est intégré
après STEP8 : le projet brut reste intact et un second `_assist.ecproj` est
toujours publié avec ses rapports. Le lot 2 ajoute STEP2 TransNetV2 en ONNX
Runtime CUDA, avec un CSV identique à la production sur les quatre cas du
corpus. Le lot 3 porte STEP4 sur Coral Edge TPU : BlazeFace, FaceMesh,
blendshapes, EfficientDet, suivi et détection de parole, avec un JSON identique
à la production sur `face_pan`, `real_244` et `real_245`. Le lot 4 porte STEP3
sur Coral Edge TPU : VAD YAMNet Split, embeddings ECAPA-TDNN et AHC, avec un
`_audio.json` identique à la production sur `vox`, `real_244` et `real_245`.
Le lot 5 ajoute le serveur d'orchestration Rust sur le port 5004, avec état
thread-safe, contrat de polling, séquence, arrêt, SQLite et verrou isolés, et
exécution des étapes par sous-processus `workflow-cli`. Le lot 6 porte STEP1 :
extraction sécurisée des archives et normalisation MP4 / H.264 / 25 fps. Les
huit étapes disposent désormais d'une implémentation Rust ; aucune commande
Python n'est lancée en repli. Le lot 8 porte l'ingestion : moniteur webhook,
normalisation d'URL, historique SQLite, cooldown des échecs, téléchargement
R2/Dropbox reprenable et statuts CSV. Le moniteur est désactivé par défaut ;
il est actif sur l'instance de production depuis le 2026-10-23, après import
de l'historique. La chaîne STEP1 → STEP8 est
validée sur une archive réelle (`1720 Camille.zip`). La validation Windows du
pont et du JSX After Effects est abandonnée (D-022) : la production cible
Linux et le panneau CEP continue d'appeler Python.

## État

- Interface : page web legacy `assets/web` ; aucun client desktop natif n'est
  maintenu (décision D-028).
- Lot 0.1 validé : `libtensorflowlite_c.so` compilé, BlazeFace et FaceMesh identiques au bit près aux références Python, 10 000 invocations stables.
- Lot 0.2 terminé : contrat d'environnement et table de correspondance des clés.
- Lot 0.3 terminé : 23 cas de parité, 14 étapes réelles sur quatre projets (244, 245, 249, 250) identiques à la production, 9 cas synthétiques déterministes.
- Lot 1.1 terminé : workspace `workflow-core` / `workflow-schema` /
  `workflow-steps` / `workflow-cli`, configuration TOML typée, CLI de
  validation, réducteur audio STEP5 avec en-tête avant le tableau.
- Lot 1.2 terminé : réducteur vidéo STEP5 en streaming, analytics et alignement
  temporel, chemin rapide typé pour les STEP4 bruts.
- STEP5 audio et vidéo sont identiques à la production sur les quatre projets
  (244, 245, 249, 250). STEP4 brut pour 244/245 ; idempotence sur 249/250.
- Performance release : 0,096 s (5,9×) sur 244, 0,101 s sur 245, 0,072 s sur
  249, 0,039 s sur 250, RSS 16 Mio. Détail et décision ouverte dans
  `docs/LOT-1-STEP5.md`.
- Lot 1.3 terminé : STEP6 `_ae.json` avec index tracking/audio, enrichissement
  `is_speaking`/`tier`, parse CSV et recommandations de segments. Identique à la
  production sur les quatre projets ; 0,108 s et 44 Mio sur 244.
- Shadow STEP5/6 : cible de dix projets atteinte, 57/57 vérifications
  identiques sur 19 vidéos (148, 151, 152, 244, 245, 249, 250, 253, 254, 257),
  sans écriture dans `/mnt/cache`. Le passage `--prefer-raw` reproduit aussi les
  19 `_tracking.json` depuis les STEP4 bruts. Détail dans
  `docs/LOT-1-SHADOW.md`.
- Lot 1.4 terminé : `workflow-bridge` pour les cuts CSV et `analyze_manifest`.
  Parité sur 273 calques, les quatre entrées `_ae.json` et les overrides de
  configuration ; cuts 21× plus rapide que Python.
- Lot 1.5 terminé : STEP7 archive par SHA-256, copie vers `/mnt/cache`,
  normalisation `docs/`, manifeste lu par STEP8. Test réel sur une copie du
  projet 250 : `docs/` identique, empreinte vidéo identique.
- Lot 1.6 terminé : STEP8 pilote `MediaSolution.jsx` en headless avec
  `effectcraft-cli`, sans pont Python ni application. Le projet 250 a produit
  un `.ecproj` ouvrable (82 segments, 1080×1920, 25 fps, 133 s) et la
  progression `MS_BATCH_*` est relayée en direct.
- Post-traitement A5/A6 intégré après STEP8 : A1 letterbox noir, A2 bannière
  basse, A3/A4 en revue, A6 template SDM. Le brut `<vidéo>_9x16.ecproj` reste
  inchangé ; `<vidéo>_9x16_assist.ecproj` et `post.json`/`review.json`/
  `audit.json` sont toujours produits. Validation 295 au pixel (0/255) et
  parité de décisions avec le prototype sur 268/187/191/181. Détail dans
  `docs/LOT-9-POST-STEP8.md`.
- Lot 2 terminé : STEP2 TransNetV2 en `ort` CUDA, FFmpeg `rawvideo` via
  `tokio::process`, fenêtres 100/50/25 et sortie CSV identique sur
  `cuts`, `short_2997`, `real_244` et `real_245`. Débit supérieur à la baseline
  sur les quatre cas ; VRAM pic sous la baseline. Détail dans
  `docs/LOT-2-STEP2.md`.
- Lot 3 terminé : STEP4 Coral Edge TPU avec modèles co-compilés BlazeFace et
  FaceMesh, crop affine 192×192, 52 blendshapes filtrées One-Euro, EfficientDet
  avec hystérésis, suivi `obj_N`, détection de parole et JSON en streaming
  atomique. Parité complète à `1e-6` sur `face_pan`, `real_244` et `real_245` ;
  blendshapes à `5,96e-7`, objets IoU 1,0, RSS 199,5 Mio sur `real_244`.
  Détail dans `docs/LOT-3-STEP4.md`.
- Lot 4 terminé : STEP3 YAMNet Split Coral, frontend log-mel NPY, VAD médian
  avec hangover, ECAPA-TDNN ONNX CPU, AHC average et writer JSON atomique.
  JSON brut identique sur `vox_2spk`, `real_244` et `real_245` ; projection
  STEP5 audio identique sur les deux projets. Le chemin CPU YAMNet reste un
  mode de test explicite. Détail, performance et limites dans
  `docs/LOT-4-STEP3.md`.
- Lot 5 terminé : `workflow-server` (axum) expose le contrat de polling sur
  le port 5004, avec base et racine Rust, verrou `/tmp/workflow-rs-hw.lock`,
  séquence sérialisée, arrêt SIGTERM, timeout et logs. STEP5/STEP6 disposent
  d'un mode batch ; 12/12 artefacts identiques aux copies de production 244,
  245, 249 et 250. Détail dans `docs/LOT-5-ORCHESTRATOR.md`.
- Lot 6 terminé : STEP1 Rust extrait les archives ZIP/RAR/TAR avec
  assainissement des chemins, réinitialisation mensuelle et suppression après
  succès, puis normalise les vidéos (ffprobe, GPU avec repli CPU, `yadif`) en
  préservant les `.mov` alpha ou listés. Validation réelle sur une archive
  synthétique et via le serveur 5004. Détail dans `docs/LOT-6-STEP1.md`.
- Lot 7 terminé : séquence STEP2 → STEP8 sur une copie de 244, STEP5/6
  rejoués sur les artefacts Rust avec `_ae.json`, `_audio.json` et
  `_tracking.json` identiques sémantiquement à la production, puis bascule
  contrôlée 5003/5004 avec rollback vérifié. Détail dans
  `docs/LOT-7-INTEGRATION.md`.
- Lot 8 terminé : ingestion webhook, normalisation d'URL, historique SQLite,
  échecs/cooldown, téléchargement `.part` + `Range`, validation ZIP et routes
  `/api/csv_*`. Import réel de 911 URL identiques à la base Python, audit de
  normalisation à 0 changement, webhook réel à 462 enregistrements. Le dernier
  lien du webhook (`276 Camille.zip`, 175 Mo) a été téléchargé via R2 puis via
  Dropbox, et le moniteur a été validé bout en bout sur une base isolée, y
  compris le repli R2 → Dropbox. Détail dans `docs/LOT-8-INGESTION.md`.
- Synchronisation Python ↔ Rust : `workflow-cli sync` fusionne l'historique,
  la liste STEP1 et les analyses STEP7 avant chaque bascule. Première fusion
  réelle le 2026-10-23 : 911 URL, 96 archives traitées et 640 hash STEP7
  (1,8 Go) de chaque côté. Détail dans `docs/SYNC-BACKENDS.md`.
- Validation isolée de bout en bout le 2026-10-23 : téléchargement HTTP →
  STEP1 → STEP7 → sync, puis corpus 153, 154, 155, 156, 158, 159 et 1720
  préparés ; shadow STEP5/6 162/162 sur 20 projets.
- Chaîne complète validée : STEP1 → STEP8 sur `1720 Camille.zip` (8/8 étapes,
  4 `.ecproj`, aucune écriture dans `/mnt/cache`). Détail dans
  `docs/VALIDATION-STEP1-8.md`.
- Validation Windows/After Effects abandonnée (D-022) : le pont Python et le
  panneau CEP restent en place, sans dépendance Rust.
- Modèles de production copiés dans `assets/models/` avec manifeste SHA-256 ;
  les binaires restent locaux, seuls le manifeste et le README sont versionnés.
- `crates/workflow-tpu` : chargement dynamique de `libtensorflowlite_c.so` et de
  `libedgetpu.so.1`, session transactionnelle, binaire de spike.

## Documents

- `docs/CONTRAT-ENVIRONNEMENT.md` : routes, états, schémas, fichiers, intégrations.
- `config/config.toml` : configuration typée ; secrets par variables d'environnement.
- `config/env.map.toml` : correspondance entre l'ancien `.env` et le futur `config.toml`.
- `assets/models/manifest.toml` : rôles, tailles et SHA-256 des modèles copiés.
- `docs/LOT-1-STEP5.md` : parité, performance et limites du port STEP5.
- `docs/LOT-1-STEP6.md` : parité, performance et limites du port STEP6.
- `docs/LOT-1-SHADOW.md` : validation shadow STEP5/STEP6 et limite des projets disponibles.
- `docs/LOT-1-BRIDGE.md` : parité et limites du pont After Effects.
- `docs/LOT-1-STEP7.md` : archivage, publication et manifeste STEP7.
- `docs/LOT-1-STEP8.md` : pilotage headless EffectCraft, gate scripting, timeout, annulation et hook A5/A6.
- `docs/LOT-9-POST-STEP8.md` : post-traitement A5/A6, projet `_assist`, artefacts, template SDM et validations.
- `docs/LOT-2-STEP2.md` : moteur TransNetV2 ONNX Runtime, parité et mesures STEP2.
- `docs/LOT-3-STEP4.md` : cascade Coral STEP4, parité, performance, backend CPU de test et limites.
- `docs/LOT-4-STEP3.md` : VAD YAMNet Split, ECAPA-TDNN, AHC, parité STEP3 et limites.
- `docs/LOT-5-ORCHESTRATOR.md` : serveur axum, contrat HTTP, isolation, séquence, bascule contrôlée et limites.
- `docs/LOT-6-STEP1.md` : extraction ZIP/RAR/TAR, assainissement, normalisation vidéo et limites STEP1.
- `docs/LOT-7-INTEGRATION.md` : séquence réelle STEP2 → STEP8, parité STEP5/6 et bascule contrôlée 5003/5004.
- `docs/LOT-8-INGESTION.md` : moniteur webhook, historique, téléchargement reprenable et migration.
- `docs/SYNC-BACKENDS.md` : synchronisation au changement entre Python et Rust, commandes, sauvegardes et limites.
- `docs/VALIDATION-STEP1-8.md` : séquence complète STEP1 → STEP8 sur `1720 Camille.zip`, isolation et contrôles d'intégrité.
- `docs/SWITCH-PUBLIC.md` : bascule publique par reverse proxy Nginx, canari, rollback et routes Python-only.
- `docs/STEP2-WINDOWS.md` : protocole optionnel de validation STEP2 CUDA sous Windows (cible Linux en production).
- `docs/RETENTION.md` : rétention des données Rust et cohabitation avec le workflow Python legacy.
- `docs/audits/2026-10-24-audit-backend-rust.md` : audit backend (architecture, concurrence, sécurité, contrats), constats P0/P1/P2, audit croisé Gemini Web et plan de remédiation.
- `docs/DECISION-ABANDON-TAURI.md` : décision D-028, motifs de l'abandon du client desktop Tauri et procédure de restauration.
- `docs/ETUDE-STEP9-AGENTIQUE.md` : étude d'opportunité et de faisabilité du post-STEP8 (recadrage 9:16, nettoyage, habillage, rendu), confrontation des scénarios déterministe, QA agentique et co-monteur.
- `docs/SPIKE-STEP9-PROTOCOLE.md` : protocole de spike isolé, corpus 298/300 puis 19 paires / 32 vidéos, métriques, critères d'acceptation et d'arrêt.
- `parity/README.md` : corpus, commandes et sécurité du harnais.
- `parity/BASELINE.md` : mesures de référence.
- `parity/DRIFT.md` : écarts de comportement à ne pas reproduire.

## Lanceurs locaux

Le raccourci Python `~/Bureau/Legacy_Workflow.desktop` reste inchangé : il
lance `workflow_mediapipe/start_workflow.sh` sur 5003.

Le raccourci Rust `~/Bureau/Rust_Workflow.desktop` appelle
`scripts/start-rust.sh` : vérification, démarrage sur 5004, ouverture de
Firefox et suivi des logs. Ctrl+C ou fermeture du terminal arrête le serveur ;
GNOME Terminal demande confirmation avant de fermer la fenêtre
(`org.gnome.Terminal.Legacy.Settings confirm-close`). Le serveur tournant sous
`setsid`, c'est le lanceur qui l'arrête sur SIGINT/SIGTERM/SIGHUP
(`scripts/switch-rust.sh stop`).
Les secrets sont générés au premier lancement dans `config/launcher.env`
(non versionné, `chmod 600`).

Le client desktop Tauri et son paquet `workflow-manager` ont été retirés
(D-028) ; le serveur reste headless et s'utilise via `start-rust.sh` ou
`switch-rust.sh`. Pour utiliser `switch-rust.sh` manuellement :

```sh
set -a; source config/launcher.env; set +a
./scripts/switch-rust.sh status
```

En pratique, avec `webhook.enabled = true`, un seul backend tourne à la fois :

- le lanceur Rust exécute `sync-backends.sh --apply --quiet` puis refuse de
  démarrer si Python répond encore sur 5003 ;
- le raccourci Python reste inchangé et démarre son propre moniteur ; arrêter
  Rust avant de le lancer ;
- avant chaque bascule, `scripts/sync-backends.sh --apply` fusionne
  l'historique, la liste STEP1 et les analyses STEP7. Le lanceur Rust le fait
  automatiquement ; pour revenir à Python, lancer la commande à la main ;
- ne pas lancer STEP2/STEP3/STEP4 matérielles des deux côtés simultanément ;
- les projets et téléchargements Rust restent dans `runtime/`, Python ne les
  voit pas. Le runtime Rust démarre avec `runtime/projets_extraits` vide : pour
  tester un projet, le copier dans cette racine, jamais l'inverse.

La procédure complète, les options et les sauvegardes sont décrites dans
`docs/SYNC-BACKENDS.md`.

## CLI

```sh
cargo run -p workflow-cli -- config check
cargo run -p workflow-cli -- config show
cargo run -p workflow-cli -- step2 detect \
  --input-dir path/to/docs \
  --force

cargo run --release -p workflow-cli -- step3 diarize \
  --config config/config.toml \
  --input-dir path/to/docs \
  --edgetpu /path/to/libedgetpu.so.1

cargo run --release -p workflow-cli -- step4 track \
  --config config/config.toml \
  --input-dir path/to/docs \
  --device tpu

cargo run -p workflow-cli -- step5 audio \
  --input path/to/audio.json \
  --output path/to/audio_reduced.json

cargo run -p workflow-cli -- step5 video \
  --input path/to/real_244.json \
  --output path/to/real_244_tracking.json \
  --audio path/to/real_244_audio.json

cargo run -p workflow-cli -- step6 build \
  --tracking path/to/real_244_tracking.json \
  --audio path/to/real_244_audio.json \
  --csv path/to/real_244.csv \
  --output path/to/real_244_ae.json

cargo run -p workflow-cli -- step7 finalize --dry-run
cargo run -p workflow-cli -- step7 finalize --no-delete \
  --work-dir /tmp/work --output-dir /tmp/out \
  --archives-dir /tmp/archives --logs-dir /tmp/logs

cargo run -p workflow-cli -- step8 run --dry-run
cargo run -p workflow-cli -- step8 run --folder /tmp/projet
cargo run -p workflow-cli -- step8 run --folder /tmp/projet --no-post
cargo run -p workflow-cli -- post run --folder /tmp/projet
cargo run -p workflow-cli -- post run --project /tmp/projet/projets/video_9x16.ecproj --docs /tmp/projet/docs

cargo run -p workflow-cli -- step1 run --source-dir ~/Téléchargements
cargo run -p workflow-cli -- step1 normalize-only [--force]

cargo run -p workflow-cli -- history import \
  --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3
cargo run -p workflow-cli -- history audit
cargo run -p workflow-cli -- history stats
cargo run -p workflow-cli -- webhook probe --json
cargo run -p workflow-cli -- webhook probe --plan
cargo run -p workflow-cli -- download --url 'https://...' --output-dir /tmp/verification

cargo run -p workflow-cli -- retention plan
cargo run -p workflow-cli -- retention apply

cargo run --release -p workflow-server -- --config config/config.toml --check
cargo run --release -p workflow-server -- --config config/config.toml
./scripts/switch-rust.sh check   # avec WORKFLOW_SERVER_WORKER_TOKEN
./scripts/switch-rust.sh start
./scripts/switch-rust.sh stop

./scripts/validate-lot5.py       # parité batch STEP5/STEP6 sur copies
./scripts/validate-step1.py      # parité STEP1 Python/Rust sur archive synthétique
```

Sans `--output` audio, le fichier d'entrée est réécrit de façon atomique, comme
dans la production. Sans `--output` vidéo, la CLI écrit
`<stem>_tracking.json` (ou réécrit un fichier déjà nommé `_tracking.json`).
L'en-tête `fps` / `total_frames` est écrit avant `frames_analysis` ; une seconde
réduction conserve donc les métadonnées.

## Ingestion

Le moniteur est désactivé par défaut. Après import et audit de l'historique,
passer `webhook.enabled = true` dans `config/config.toml`. Le serveur refuse de
démarrer le moniteur si `download_history` est vide, ou si
`steps.step1.source_dir` diffère de `paths.downloads`.

Sur l'instance actuelle, il est actif (`enabled = true`). Un seul backend doit
tourner à la fois. Pour passer à Python : arrêter Rust, lancer
`./scripts/sync-backends.sh --apply`, puis le raccourci Python. Pour revenir à
Rust : arrêter Python, lancer le raccourci Rust (il synchronise
automatiquement) ou `./scripts/sync-backends.sh --apply` puis
`./scripts/switch-rust.sh start`.

```sh
./target/release/workflow-cli history import \
  --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3
./target/release/workflow-cli history audit
./target/release/workflow-cli history stats
./target/release/workflow-cli webhook probe
./target/release/workflow-cli sync --apply --quiet
```

`DRY_RUN_DOWNLOADS=true` (ou `webhook.dry_run = true`) marque les nouvelles URL
dans l'historique sans télécharger. Ne pas lancer les moniteurs Python et Rust
en même temps sur le même webhook.

## Pont After Effects

```sh
cargo build --release -p workflow-bridge
./target/release/workflow-bridge \
  --manifest_path /tmp/ms_manifest.json \
  --output_path /tmp/ms_results.json
```

Le binaire traite `mode = "cuts"` (CSV de scènes) et les manifestes de tracking
(`layers`). Le format de sortie direct ou `{"error": ...}` est conservé ; le JSX
de production n'est pas modifié. La compilation Windows du pont et le
basculement du JSX sont abandonnés (D-022) : le panneau CEP continue d'appeler
Python.

## Parité

```sh
../workflow_mediapipe/coral_env/bin/python tools/parity/parity.py verify --probe
../workflow_mediapipe/coral_env/bin/python tools/parity/parity.py run --case real_244_step6
../workflow_mediapipe/coral_env/bin/python tools/parity/parity.py run-all --tag synthetic
../workflow_mediapipe/coral_env/bin/python tools/parity/parity.py report
```

Le venv `../workflow_mediapipe/coral_env/bin/python` fournit Python 3.11
(`tomllib`) et `psutil` ; il n'existe pas de `coral_env/` dans ce dépôt.

Les étapes réelles STEP2 à STEP6 des projets `244` et `245` sont identiques à la
production, ainsi que STEP5 et STEP6 de `249` et `250`. Le port Rust STEP2
reproduit les quatre CSV du corpus ; le processus complet prend 2,3–10,9 s
selon le cas, pour une VRAM pic de 2,1–2,2 GiB. Le port Rust STEP4 reproduit le
JSON complet de `face_pan`, `real_244` et `real_245` à `1e-6` près, avec les
mêmes IDs, bbox et champs de parole ; il mesure 44,2 s sur `real_244` et
41,0 s sur `real_245`. STEP5 et STEP6 prennent moins d'une seconde.

## Prérequis

- Rust 1.86 ou plus récent.
- `python3 scripts/import-models.py` pour copier les modèles de production.
- `libtensorflowlite_c.so`, produit par `scripts/build-tflite-c.sh`.
- `libedgetpu.so.1` (paquet Coral) et un `/dev/apex_0` accessible. Sur la
  machine de référence, la bibliothèque est copiée dans
  `vendor/libedgetpu.so.1` (ignorée par git) et `config/config.toml` pointe
  dessus ; sinon, installer le paquet système ou passer `--edgetpu`.
- Les modèles STEP4 co-compilés et `detect_edgetpu.tflite` pour le chemin TPU ;
  les trois modèles CPU du même manifeste ne servent qu'aux tests
  `--device cpu`.
- ONNX Runtime 1.23 (CUDA) et `ffmpeg`/`ffprobe` dans le `PATH` pour STEP2.
  Le chemin de la bibliothèque ONNX Runtime se règle dans `config/config.toml`.

## Spike

```sh
cargo run -p workflow-tpu --bin workflow-tpu-spike -- \
  --model assets/models/step4/blazeface_front_quantized_edgetpu.tflite \
  --edgetpu ../workflow_mediapipe/tmp_lib/usr/lib/x86_64-linux-gnu/libedgetpu.so.1 \
  --iterations 1000
```

Le binaire vérifie la signature `edgetpu-custom-op`, exécute l'inférence et
affiche les dimensions, la quantization, la latence et une empreinte des sorties.
