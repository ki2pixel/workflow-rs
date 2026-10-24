---
trigger: model_decision
description: Format des messages de commit (Conventional Commits, résumé et corps en anglais, généré depuis le diff)
globs:
---

# Format des messages de commit

## Position de cette règle

- Convention basée sur Conventional Commits.
- Conserve `Prefix`, `BREAKING CHANGE` et `scope`, et ajoute les règles du
  dépôt : langue anglaise, corps en bullets, génération depuis le diff.
- En cas de réutilisation dans un autre projet, ajuster `language` et la liste
  des prefixes.

## Langue

- `language = "en"`.
- Le résumé et le corps sont écrits en anglais.
- Les termes techniques et les noms de fichiers restent tels quels.

> Exception au français : les commits de workflow-rs sont en anglais, comme
> l'historique du dépôt. La documentation, les commentaires et les logs restent
> en français.

## Format de base (obligatoire)

```text
<Prefix>: <Summary (imperative/concise)>

- Change 1
- Change 2
- ...

Refs: #<Issue> (optional)
BREAKING CHANGE: <content> (optional)
```

## Prefix

Prefix correspond au `type` Conventional Commits, en minuscules :

- `feat` : nouvelle fonctionnalité
- `fix` : correction
- `refactor` : refactor sans changement de comportement
- `perf` : performance
- `test` : tests
- `docs` : documentation
- `build` : build/dépendances
- `ci` : CI
- `chore` : outillage/scripts
- `style` : style seul
- `revert` : revert

Le format `<Prefix>(scope):` est autorisé, par exemple
`fix(step2): ...`.

### Scopes réellement utilisés

`step1`, `step2`, `step3`, `step4`, `step5`, `step6`, `step7`, `step8`,
`server`, `ingest`, `sync`, `config`, `launcher`, `bridge`, `tpu`, `parity`,
`docs`, `agents`, `memory`.

## Résumé (première ligne)

- Concis, en anglais, sans point final.
- Exprimer quoi et pourquoi si nécessaire.
- Viser environ 50 caractères, maximum raisonnable 72.

## Principes de génération

- Toujours générer depuis les diffs non commités (`git diff`,
  `git diff --cached`) après les avoir lus.
- Ne pas deviner à partir du titre d'une issue ou du nom de branche.
- Même pour une génération automatique, utiliser le diff comme entrée.
- Vérifier que le message correspond aux fichiers réellement modifiés.

## Corps

- Bullets commençant par `- `.
- Même langue que le résumé (anglais).
- Ajouter si possible : impact, migration, risques, rollback.
- Ne pas répéter le titre ; expliquer les décisions et les limites.

## Footer (optionnel)

- `Refs: #123` / `Closes: #123`.
- `BREAKING CHANGE:` si un contrat JSON, CSV, HTTP, verrou ou format de fichier
  change.
- Notation `!` possible : `fix(step6)!: ...`.

## Exemples

```text
fix(step2): keep CUDA provider selection explicit

- Fail when the CUDA execution provider cannot be registered
- Keep the CPU path available only as an explicit test mode
- Document the ONNX Runtime library override
```

```text
docs(agents): bootstrap Rust agent rules, skills and memory bank

- Create .agents/rules, .agents/skills and .agents/workflows
- Replace legacy-only skills with Rust equivalents
- Initialize memory-bank from the migration history
```

## Interdit

- Résumé dans une autre langue que `en`.
- Résumés vagues (« update », « fix bug », « changes »).
- Corps sans bullets, difficile à parcourir.
- Commit qui ne fait que désactiver un check ou contourner un test sans
  amélioration substantielle.
- Commit qui mélange une migration de données et un refactor sans lien.
