# Matrice d'exécution des tests — workflow-rs

| Suite | Commande | Prérequis | Résultat attendu |
| --- | --- | --- | --- |
| Format | `cargo fmt --check` | `rustfmt` | Aucun diff |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | Dépendances compilées | Aucun warning |
| Suite complète | `cargo test --workspace` | Aucun matériel | 280 tests verts |
| STEP1 | `cargo test -p workflow-steps step1 --lib` | Aucun matériel | Tests module step1 |
| STEP2 | `cargo test -p workflow-steps step2 --lib` | Aucun GPU | Tests fenêtres/CSV/CPU |
| STEP3 | `cargo test -p workflow-steps step3 --lib` | Aucun TPU | Tests mel/VAD/AHC/writer |
| STEP4 | `cargo test -p workflow-steps step4 --lib` | Aucun TPU | Tests NMS/crop/filtre/suivi |
| STEP5 | `cargo test -p workflow-steps step5 --lib` | Aucun matériel | Tests audio/vidéo/analytics |
| STEP6 | `cargo test -p workflow-steps step6 --lib` | Aucun matériel | Tests `_ae.json`/CSV |
| STEP7 | `cargo test -p workflow-steps step7 --lib` | Aucun matériel | Tests finalisation/manifeste |
| STEP8 | `cargo test -p workflow-steps step8 --lib` | Aucun EffectCraft | Tests driver/parsing/timeout |
| Serveur | `cargo test -p workflow-server --lib` | Aucun port réel | Tests 409/stop/auth/routes |
| Ingestion | `cargo test -p workflow-ingest --lib` | Aucun réseau | Tests URL/SQLite/cooldown |
| Sync | `cargo test -p workflow-sync --lib` | Aucun serveur | Tests fusion/dry-run |
| Bridge | `cargo test -p workflow-bridge` | Aucun AE | Tests cuts/manifeste |
| CLI | `cargo test -p workflow-cli --bins` | Aucun matériel | Tests parsing/config |
| TPU FFI | `cargo test -p workflow-tpu --lib` | Aucun `/dev/apex_0` | 2 tests de contrat ; matériel via `workflow-tpu-spike` |

## Pré-checklist

- [ ] `cargo build --workspace` passe.
- [ ] Aucun processus STEP1 (normalisation), STEP2/3/4 ou serveur de test ne tourne.
- [ ] Les tests n'écrivent pas dans `runtime/` ni `/mnt/cache`.
- [ ] Les chemins temporaires sont sous `tempfile` ou `/tmp/opencode`.
- [ ] Les variables d'environnement de production ne sont pas chargées par
      erreur (`config/launcher.env`).

## Tests manuels / matériels

| Cas | Procédure | Trace |
| --- | --- | --- |
| STEP2 CUDA | `workflow-cli step2 detect --device cuda` sur un cas du corpus | CSV + `nvidia-smi` |
| STEP3 TPU | `workflow-cli step3 diarize --edgetpu vendor/libedgetpu.so.1` | `_audio.json` + log |
| STEP4 TPU | `workflow-cli step4 track --device tpu` | `_tracking.json` + log |
| STEP8 EffectCraft | `workflow-cli step8 run --dry-run` puis réel | `MS_BATCH_RESULT` |
| Parité STEP5/6 | `python3 scripts/shadow-step56.py --limit 10` | rapport shadow |

## Astuces

- Utiliser `-- --nocapture` pour voir les logs d'un test ciblé.
- `RUST_BACKTRACE=1` pour une panique.
- `RUST_LOG=debug` pour `tracing`.
- Ne pas lancer `cargo test --workspace -- --test-threads=1` sans raison : les
  tests sont isolés par `tempfile`.
- Un test qui nécessite un modèle ou un device doit être `#[ignore]` ou
  documenté comme manuel.
