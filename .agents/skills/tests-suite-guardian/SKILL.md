---
name: tests-suite-guardian
description: Maintenir et exécuter la suite de tests Rust de workflow-rs (tests inline, fmt, clippy, filtres par crate/étape).
---

# Tests Suite Guardian

## Portée

- **282 tests** : 250 `#[test]` + 32 `#[tokio::test]`.
- Tests **inline** dans les modules, sous `#[cfg(test)]`. Il n'y a pas de
  dossier `tests/` d'intégration.
- Crates avec tests : `workflow-core`, `workflow-schema`, `workflow-steps`,
  `workflow-server`, `workflow-ingest`, `workflow-sync`, `workflow-bridge`,
  `workflow-cli` et `workflow-tpu`. Le spike TPU reste la validation matérielle
  de référence ; les tests unitaires ne chargent ni modèle, ni GPU, ni Coral.
- Les tests ne couvrent ni les modèles, ni le GPU, ni le Coral réel.
- La parité réelle est mesurée par les scripts de shadow/validation, pas par
  `cargo test`.

## Procédure générale

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Filtres utiles :

```sh
cargo test -p workflow-steps step1 --lib
cargo test -p workflow-steps step2 --lib
cargo test -p workflow-steps step3 --lib
cargo test -p workflow-steps step4 --lib
cargo test -p workflow-steps step5 --lib
cargo test -p workflow-steps step6 --lib
cargo test -p workflow-steps step7 --lib
cargo test -p workflow-steps step8 --lib
cargo test -p workflow-server --lib
cargo test -p workflow-ingest --lib
cargo test -p workflow-sync --lib
cargo test -p workflow-bridge
cargo test -p workflow-cli --bins
cargo test -p workflow-tpu --lib
```

## Règles de test

1. Table de perspectives avant d'écrire les tests (voir
   `.agents/rules/test-strategy.md`).
2. Autant ou plus de cas d'échec que de cas nominaux.
3. Given/When/Then en commentaire.
4. Vérifier le type et le message d'erreur.
5. Utiliser `tempfile` pour les chemins temporaires.
6. Ne jamais écrire dans `runtime/`, `/mnt/cache` ou le dépôt Python.
7. Ne jamais lancer STEP1/2/3/4/7/8 réels dans un test unitaire.
8. Les tests `#[cfg(unix)]` doivent rester compilables sur Linux ; le projet
   cible Linux.

## Checklist spécifique

- **Config** : tester `deny_unknown_fields`, les bornes (`fps`, `crf`,
  `threshold`, `padding*2 < window`), les secrets vides et le port 5003.
- **Chemins** : tester l'isolation (`history_db` sous `base`, verrou Python
  refusé).
- **JSON** : tester l'ordre des clés (`fps`/`total_frames` avant
  `frames_analysis`), les arrondis Python, les écritures atomiques.
- **STEP1** : tester les traversées d'archives, les extensions réellement
  scannées, la préservation `.mov`.
- **STEP2** : tester les fenêtres/padding, le CSV `\r\n`, le mode CPU test.
- **STEP3** : tester le VAD, le hangover, l'AHC, le cas sans audio.
- **STEP4** : tester le NMS, le crop, One-Euro, le suivi, le cas CSV/audio
  manquant.
- **Serveur** : tester le 409, le stop de séquence, l'authentification, les
  routes `/api/ping` et `/api/csv_*`.
- **Ingestion** : tester la normalisation d'URL, le schéma SQLite, le cooldown,
  le repli R2 → Dropbox.
- **Sync** : tester la fusion historique/liste STEP1/archives STEP7 et les
  modes dry-run/apply.

## Diagnostic rapide

| Symptôme | Piste |
| --- | --- |
| Erreur de compilation | `cargo check --workspace`, lire le premier error |
| Test bloqué | Vérifier un `Mutex` tenu à travers un `await`, un canal non fermé |
| Test qui touche le réseau | Isoler avec un serveur local ou un mock |
| Test qui touche le matériel | Le sortir de la suite, le documenter comme test manuel |
| Échec `ort`/CUDA | Utiliser `--device cpu` uniquement dans un test explicite |
| Échec TFLite | Vérifier `libtensorflowlite_c.so`, pas `/dev/apex_0` |
| Échec SQLite | Vérifier le schéma et `PRAGMA integrity_check` |
| Test instable | Chercher une dépendance à l'ordre, au temps ou au parallélisme |

## Parité (hors `cargo test`)

```sh
python3 scripts/shadow-step56.py --root /mnt/cache --limit 10
python3 scripts/validate-lot5.py
python3 scripts/validate-step1.py
../workflow_mediapipe/coral_env/bin/python tools/parity/parity.py verify --probe
```

- Ces scripts lisent la production et écrivent sous `/tmp`.
- Le venv du harnais est celui du legacy :
  `../workflow_mediapipe/coral_env/bin/python` (pas de `coral_env/` dans ce
  dépôt, même si `parity/README.md` utilise encore le chemin relatif).
- `tools/parity/parity.py` relance Python legacy et compare aux fixtures ; il
  n'invoque pas les binaires Rust.
- Ne pas utiliser `--prefer-raw` sur `153 Camille` (D-024).

## Références

- `.agents/rules/test-strategy.md`
- `AGENTS.md`
- `docs/LOT-*.md`
- `parity/README.md`
- `parity/BASELINE.md`
- `parity/DRIFT.md`
- `resources/test_execution_matrix.md`
