---
trigger: model_decision
description: Format des titres et corps de Pull Request (anglais, structure Overview/Changes/Tests)
globs:
---

# Format des messages de Pull Request

## Position de cette règle

- Alignée sur `commit-message-format.md`.
- Le titre reprend le style Conventional Commits ; le corps est structuré.
- Adapter `language` et les sections obligatoires selon le projet.

## Langue

- `language = "en"`.
- Titre et corps en anglais.
- Les références aux fichiers, commandes et contrats restent en anglais.

## Titre (première ligne)

```text
<Prefix>: <Summary (imperative/concise)>
```

- `Prefix` recommandé : `feat`, `fix`, `refactor`, `docs`, `chore`.
- Concis, sans point final, environ 50 caractères.
- Exprimer quoi et pourquoi si nécessaire.

## Corps structuré

```markdown
## Overview

Summary of what was implemented or fixed.

## Changes

- Change 1
- Change 2
- Change 3

## Technical Details (optional)

- Design decisions, trade-offs, invariants preserved.

## Test Content

- Unit tests, manual verification, shadow/parity checks.
- Commands and results.

## Related Issues

- Closes #123
- Refs #456
```

- `Overview` et `Changes` sont obligatoires en principe.
- `Test Content` est obligatoire pour tout changement de comportement.
- Les bullets doivent permettre de comprendre quoi, où et pourquoi.
- Pour workflow-rs, mentionner explicitement :
  - la parité (`parity/DRIFT.md`, fixtures, shadow) ;
  - les commandes exécutées (`cargo fmt`, `clippy`, `test`) ;
  - l'impact production (`runtime/`, `/mnt/cache`, moniteur, verrou) ;
  - le rollback s'il y a un changement d'exploitation.

## Principes de génération

- Toujours générer depuis les diffs et l'historique de commits réels
  (`git diff`, `git log`).
- Ne pas deviner à partir du titre d'issue ou de branche.
- Décrire le contenu, l'impact et les tests.
- Aligner le prefix et le résumé sur les commits.

## Interdit

- Titre ou corps dans une autre langue que `en`.
- Titres vagues (« update », « fix issue », « changes »).
- Corps non structuré, sans sections ni bullets.
- Description qui ne correspond pas au diff ou qui omet un changement
  important, un impact ou un résultat de test.
- PR qui ne mentionne pas un changement de contrat JSON/CSV/HTTP.
