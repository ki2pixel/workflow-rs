---
name: workflow-operator
description: Exploiter le pipeline Rust workflow-rs (8 étapes, CLI, serveur 5004, verrous, sync) sans toucher au legacy Python.
---

# Opérateur workflow-rs

Cette skill décrit l'exploitation du pipeline Rust. Elle s'appuie sur
`AGENTS.md`, `.agents/rules/codingstandards.md` et
`docs/CONTRAT-ENVIRONNEMENT.md`.

## 1. Architecture et responsabilités

| Étape | Module Rust | Commande CLI | Sortie principale |
| --- | --- | --- | --- |
| STEP1 | `workflow-steps::step1` | `step1 run` / `step1 normalize-only` | `<projet>/docs/` normalisé |
| STEP2 | `step2` | `step2 detect` | `<stem>.csv` |
| STEP3 | `step3` | `step3 diarize` | `<stem>_audio.json` |
| STEP4 | `step4` | `step4 track` | `<stem>_tracking.json` |
| STEP5 | `step5` | `step5 project` / `audio` / `video` | JSON réduits |
| STEP6 | `step6` | `step6 project` / `build` | `<stem>_ae.json` |
| STEP7 | `step7` | `step7 finalize` | copie finalisée + manifeste |
| STEP8 | `step8` | `step8 run` | `<video>_9x16.ecproj` |

- Le serveur `workflow-server` écoute sur **5004** et délègue à
  `workflow-cli`. Il ne contient pas la logique métier.
- `workflow-cli` est l'unique entrée opérationnelle. Aucun fallback Python.
- La config est `config/config.toml` ; `--config` est global.

## 2. Règles d'or

1. **Un seul backend avec le webhook.** Si `webhook.enabled = true`, Python
   `:5003` doit être arrêté avant de démarrer Rust, et inversement.
2. **Ne jamais lancer STEP2/3/4 matérielles des deux stacks en même temps.**
3. **Le verrou matériel Rust est `/tmp/workflow-rs-hw.lock`**, distinct du
   verrou Python `/tmp/workflow-hw.lock`. La sync utilise
   `/tmp/workflow-sync.lock`.
4. **`runtime/` est la production locale** : pas de test destructif, pas de
   `retention apply` sans plan.
5. **`/mnt/cache` est une sortie finalisée partagée** : ne pas y écrire depuis
   un test.
6. **Le dépôt Python est en lecture seule** : ne pas le modifier.
7. **Toujours passer `--config`** quand on exécute depuis un autre répertoire.

## 3. Commandes d'exploitation

### Construire et vérifier

```sh
cargo build --release -p workflow-server -p workflow-cli
cargo run -p workflow-cli -- config check
cargo run -p workflow-cli -- config show
```

### Lancer une étape

```sh
cargo run --release -p workflow-cli -- --config config/config.toml step1 run \
  --source-dir runtime/downloads

cargo run --release -p workflow-cli -- --config config/config.toml step2 detect \
  --input-dir runtime/projets_extraits

cargo run --release -p workflow-cli -- --config config/config.toml step3 diarize \
  --input-dir runtime/projets_extraits --edgetpu vendor/libedgetpu.so.1

cargo run --release -p workflow-cli -- --config config/config.toml step4 track \
  --input-dir runtime/projets_extraits --device tpu

cargo run --release -p workflow-cli -- --config config/config.toml step5 project \
  --work-dir runtime/projets_extraits --keyword Camille

cargo run --release -p workflow-cli -- --config config/config.toml step6 project \
  --work-dir runtime/projets_extraits --keyword Camille

cargo run --release -p workflow-cli -- --config config/config.toml step7 finalize --dry-run

cargo run --release -p workflow-cli -- --config config/config.toml step8 run --dry-run
```

### Serveur et bascule

```sh
set -a; source config/launcher.env; set +a
./scripts/switch-rust.sh check
./scripts/switch-rust.sh start
./scripts/switch-rust.sh status
./scripts/switch-rust.sh stop

./scripts/sync-backends.sh            # dry-run
./scripts/sync-backends.sh --apply    # fusion réelle
```

Le lanceur `scripts/start-rust.sh` est interactif : il crée les secrets, lance
la sync, démarre le serveur et ouvre Firefox. Pour un usage agent, préférer
`switch-rust.sh`.

## 4. État et diagnostic

- `GET /api/step_status/STEP4` : état sans logs.
- `GET /status/STEP4` : état avec logs.
- `GET /sequence/status` : état de la séquence.
- `GET /api/ping` : authentifié avec `X-Worker-Token`.
- Les routes mutantes exigent `X-Worker-Token`.
- Les logs par étape sont sous `runtime/logs/stepN/`.
- Le serveur sérialise les étapes : un deuxième lancement renvoie **409**.

## 5. Sécurité et production

- Le serveur refuse le port 5003, une base hors `paths.base`, le verrou Python
  et les jetons par défaut.
- Le moniteur webhook refuse une base vide et exige
  `steps.step1.source_dir == paths.downloads`.
- `step1 run` supprime les archives après succès par défaut.
- `step7 finalize` supprime la source par défaut ; utiliser `--no-delete` ou
  `--dry-run` pour un test.
- `retention plan` avant `retention apply`.
- `sync --apply` réécrit les deux bases : arrêter les deux serveurs, faire un
  dry-run, puis laisser la sync créer sa sauvegarde sous
  `runtime/backups/sync/<horodatage>`.

## 6. Frontend et AE

- `assets/web` est une copie du frontend legacy ; ne pas y ajouter de logique
  Rust.
- `assets/effectcraft/MediaSolution.jsx` est une copie du panel ; STEP8 ne le
  modifie pas.
- Le panneau CEP et les JSX continuent d'appeler Python (D-022).

## 7. Tests et docs

- `cargo test --workspace` (233 tests, sans matériel).
- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`.
- Parité : `scripts/shadow-step56.py`, `scripts/validate-lot5.py`,
  `scripts/validate-step1.py`, `tools/parity/parity.py`.
- Docs : `README.md`, `docs/LOT-*.md`, `docs/CONTRAT-ENVIRONNEMENT.md`,
  `parity/DRIFT.md`, `memory-bank/`.

## 8. Ressource

- `resources/step_command_matrix.md` : matrice des 8 étapes (commande, logs,
  prérequis, options destructives).
