---
name: workflow-docs-updater-plus
description: Synchroniser la documentation Rust (README, docs/lots/LOT-*, contrat, parité, memory-bank) après un changement de code ou d'exploitation.
---

# Workflow Docs Updater Plus

## Quand l'utiliser

- Après un changement de comportement, de configuration, de contrat JSON/CSV,
  de route HTTP, de verrou ou de procédure d'exploitation.
- Après une validation de parité ou une bascule de backend.
- Pour aligner `README.md`, `docs/lots/LOT-*.md`,
  `docs/architecture/CONTRAT-ENVIRONNEMENT.md`, `parity/DRIFT.md`, `AGENTS.md`
  et `memory-bank/`.
- Pour ajouter une décision durable dans `memory-bank/decisionLog.md`.
- Pour localiser un document, commencer par `docs/README.md` (index,
  taxonomie et table de correspondance).

## Sources de vérité

1. **Le code** (`crates/`, `scripts/`, `config/`) : comportement réel.
2. **Les mesures** (`parity/`, `docs/validation/VALIDATION-*.md`) : ce qui a été vérifié.
3. **Les docs existantes** : ce qui est promis.
4. **La memory-bank** : l'état et les décisions récentes.

En cas de conflit, le code et les mesures priment ; citer les deux et demander
arbitrage si le contrat est ambigu.

## Processus recommandé

### 1. Collecter le contexte

- Lire `/home/kidpixel/workflow-rs/memory-bank/activeContext.md` et
  `/home/kidpixel/workflow-rs/memory-bank/progress.md`.
- Lire les `docs/lots/LOT-*.md` concernés.
- Inspecter le code réel : `rg`, `cargo metadata`, `cargo tree`, `wc -l`.
- Vérifier les commandes et chemins cités.

### 2. Identifier les docs impactées

| Changement | Docs à mettre à jour |
| --- | --- |
| Config TOML | `README.md`, `docs/architecture/CONTRAT-ENVIRONNEMENT.md`, `config/env.map.toml` |
| STEP1/2/3/4 | `docs/lots/LOT-*.md`, `docs/validation/VALIDATION-STEP1-8.md`, `parity/DRIFT.md` |
| Serveur/API | `docs/architecture/CONTRAT-ENVIRONNEMENT.md`, `docs/lots/LOT-5-ORCHESTRATOR.md` |
| Ingestion | `docs/lots/LOT-8-INGESTION.md`, `docs/exploitation/SYNC-BACKENDS.md` |
| Bascule | `docs/exploitation/SWITCH-PUBLIC.md`, `docs/exploitation/SYNC-BACKENDS.md` |
| Rétention | `docs/exploitation/RETENTION.md` |
| Parité | `parity/DRIFT.md`, `parity/BASELINE.md`, `parity/README.md` |
| Agents | `AGENTS.md`, `.agents/rules/`, `.agents/skills/`, `memory-bank/` |

### 3. Rédiger

- Respecter le format existant (tableaux, sections numérotées, français).
- Expliquer le **pourquoi**, pas seulement le **comment**.
- Documenter les variables, commandes, fichiers et limites.
- Ne pas promettre un comportement non testé.
- Pour un écart legacy, renvoyer vers `parity/DRIFT.md` (D-xxx).
- Pour une décision durable, ajouter une entrée datée dans
  `memory-bank/decisionLog.md`.

### 4. Vérifications croisées

- [ ] Les commandes citées existent (`--help`, `cargo run ... --help`).
- [ ] Les chemins cités existent.
- [ ] Les noms d'étapes et de crates sont exacts.
- [ ] Les chiffres (tests, URL, archives, hash) sont à jour.
- [ ] Les contrats JSON/CSV/HTTP ne sont pas contredits.
- [ ] `AGENTS.md` et `README.md` ne se contredisent pas.
- [ ] La memory-bank reflète l'état réel.

### 5. Finaliser

- Mettre à jour la memory-bank avec les deltas de la session.
- Lancer `cargo fmt --check` et `git diff --check`.
- Vérifier qu'aucun fichier de production (`runtime/`, `/mnt/cache`) n'a été
  modifié.

## Checkpoints de rédaction

Avant :

- [ ] TL;DR ou résumé en tête si le document est long.
- [ ] Public cible identifié (opérateur, développeur, agent).

Pendant :

- [ ] Tableaux de trade-offs si plusieurs options.
- [ ] Exemples de commandes copiables.
- [ ] Limites et rollback explicites.

Après :

- [ ] Pas de style « IA » (phrases creuses, listes sans contenu).
- [ ] Ponctuation naturelle (éviter ` - ` comme séparateur).
- [ ] Relecture des liens et des chemins.

## Références

- `.agents/skills/documentation/SKILL.md`
- `README.md`
- `AGENTS.md`
- `docs/README.md`
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`
- `parity/DRIFT.md`
- `memory-bank/`
- `resources/docs_sync_checklist.md`
