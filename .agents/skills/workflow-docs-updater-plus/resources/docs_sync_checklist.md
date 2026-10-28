# Checklist de synchronisation documentaire — workflow-rs

## 1. Périmètre

- [ ] `README.md` : état des lots, commandes, prérequis, liens.
- [ ] `AGENTS.md` : layout, commandes, safety rails, conventions.
- [ ] `docs/README.md` : index, taxonomie et table de correspondance.
- [ ] `docs/architecture/CONTRAT-ENVIRONNEMENT.md` : routes, états, schémas, fichiers.
- [ ] `docs/lots/LOT-*.md` : périmètre, parité, performance, limites.
- [ ] `docs/exploitation/SYNC-BACKENDS.md` : fusion Python ↔ Rust.
- [ ] `docs/exploitation/SWITCH-PUBLIC.md` : bascule et rollback.
- [ ] `docs/exploitation/RETENTION.md` : rétention.
- [ ] `docs/validation/VALIDATION-STEP1-8.md` : validation de bout en bout.
- [ ] `parity/DRIFT.md` : écarts et décisions D-xxx.
- [ ] `parity/BASELINE.md` : mesures de référence.
- [ ] `.agents/` : règles, skills, workflows.
- [ ] `memory-bank/` : état, décisions, progrès.

## 2. Vérifications de cohérence

- [ ] Les 8 étapes sont nommées STEP1…STEP8 partout.
- [ ] Le port Rust est 5004 ; le port Python legacy est 5003.
- [ ] `workflow-cli` est l'unique entrée des étapes.
- [ ] `runtime/` est la racine Rust ; `/mnt/cache` est la sortie finalisée.
- [ ] Le verrou Rust est `/tmp/workflow-rs-hw.lock`.
- [ ] Le webhook est actif sur une seule stack.
- [ ] D-002 (sans audio), D-004 (CSV/audio requis), D-016 (extensions STEP1),
      D-022 (Windows/CEP) sont cohérents.
- [ ] Les chiffres de tests/parité/validation sont à jour.

## 3. Commandes à vérifier

```sh
cargo run -p workflow-cli -- --help
cargo run -p workflow-cli -- step1 --help
cargo run -p workflow-cli -- step2 --help
cargo run -p workflow-cli -- step3 --help
cargo run -p workflow-cli -- step4 --help
cargo run -p workflow-cli -- step5 --help
cargo run -p workflow-cli -- step6 --help
cargo run -p workflow-cli -- step7 --help
cargo run -p workflow-cli -- step8 --help
cargo run -p workflow-cli -- sync --help
cargo run -p workflow-cli -- history --help
cargo run -p workflow-cli -- webhook --help
```

## 4. Liens et chemins

- [ ] Les liens relatifs pointent vers des fichiers existants.
- [ ] Les chemins absolus sont ceux de la machine de production.
- [ ] Les exemples sous `/tmp/opencode` ne sont pas présentés comme production.
- [ ] Les secrets ne sont jamais cités.

## 5. Memory-bank

- [ ] `activeContext.md` reflète la dernière session.
- [ ] `progress.md` contient les deltas et les prochaines actions.
- [ ] `decisionLog.md` contient les décisions durables.
- [ ] `systemPatterns.md` contient les nouveaux patrons.
- [ ] `productContext.md` reste stable sauf changement de périmètre.

## 6. Finalisation

- [ ] `git diff --check`.
- [ ] `cargo fmt --check`.
- [ ] `cargo test --workspace` si du code a changé.
- [ ] Aucun fichier `runtime/` ou `/mnt/cache` modifié.
- [ ] Commit `docs(scope): ...` en anglais.
