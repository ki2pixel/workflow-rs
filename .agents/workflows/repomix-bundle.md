---
description: Générer un bundle Repomix du dépôt Rust pour analyse par un LLM externe
---

# `/repomix-bundle` — Générer le bundle Repomix

## Objectif

Créer un bundle optimisé du codebase Rust pour analyse par des LLM externes
(Claude, ChatGPT, etc.), sans inclure les modèles, le runtime, les corpus ou les
binaires.

## Étape 1 — Vérifier l'outil et le dépôt

```sh
npx --yes repomix --version
git status --short
```

- Le dépôt n'a pas de `repomix.config.json` : utiliser les options CLI.
- Ne pas inclure les fichiers non suivis par git (sauf demande explicite).
- Vérifier qu'aucun secret n'est présent dans les fichiers inclus.

## Étape 2 — Générer le bundle

```sh
npx --yes repomix . \
  --style markdown \
  --output /tmp/opencode/repomix-workflow-rs.md \
  --include 'README.md,AGENTS.md,Cargo.toml,Cargo.lock,config/**,crates/**/*.rs,docs/**/*.md,parity/*.md,scripts/**/*.sh,scripts/**/*.py,.agents/**/*.md,memory-bank/**/*.md' \
  --ignore 'target/**,runtime/**,vendor/**,assets/models/**,assets/web/static/**,assets/web/sound-design/**,assets/effectcraft/MediaSolution.jsx,parity/cases/**,parity/fixtures/**,parity/baseline/**,parity/work/**,config/launcher.env'
```

- Sortie sous `/tmp/opencode`, jamais dans le dépôt.
- Le bundle exclut les gros assets, les modèles, les logs et les données de
  production.
- `AGENTS.md` et `.agents/` sont inclus pour donner le contexte opérationnel.

## Étape 3 — Vérifier le résultat

```sh
wc -c /tmp/opencode/repomix-workflow-rs.md
rg -n 'workflow_mediapipe|SECRET|TOKEN|launcher.env' \
  /tmp/opencode/repomix-workflow-rs.md | head
```

- Vérifier la taille et le nombre de tokens.
- Vérifier qu'aucun secret n'est présent.
- Vérifier que les fichiers critiques sont inclus : `Cargo.toml`,
  `crates/workflow-steps/src/step*/`, `docs/LOT-*.md`, `parity/DRIFT.md`.
- Vérifier que `config/launcher.env` n'est pas inclus.

## Résultat attendu

- Fichier : `/tmp/opencode/repomix-workflow-rs.md`.
- Contenu : code Rust, docs, scripts, règles agents, memory-bank.
- Exclusions : `target/`, `runtime/`, `vendor/`, modèles, corpus, médias.
- Usage : partage avec un LLM externe pour revue ou analyse.

## Notes

- Le bundle est un instantané : le régénérer après un changement significatif.
- Ne pas committer le bundle.
- Ne pas inclure `config/launcher.env` ni aucun fichier de credentials.
- Pour un bundle plus petit, utiliser `--compress` ou restreindre
  `--include`.
- Pour un bundle de revue ciblée, inclure seulement `crates/workflow-steps/`
  et `docs/LOT-*.md`.
