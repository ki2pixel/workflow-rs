---
description: Exécuter les contrôles Rust, committer et pousser la branche courante
---

# Commit & Push (branche courante)

## Prérequis

- Des fichiers modifiés existent.
- Le remote `origin` est configuré.
- Les secrets ne sont pas dans le diff (`config/launcher.env`, `.env`).
- Les données de production (`runtime/`, `/mnt/cache`) ne sont pas modifiées.

## Étapes

1. Vérifier l'état : `git status`, `git diff --stat`.
2. Lancer les contrôles :
   ```sh
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
3. Stager : `git add -A`.
4. Committer avec un message Conventional Commits en anglais, généré depuis le
   diff (voir `.agents/rules/commit-message-format.md`).
5. Pousser : `git push -u origin "$(git branch --show-current)"`.

## Exécution sûre

```sh
MSG="<Prefix>: <Summary (imperative/concise)>" \
BRANCH=$(git branch --show-current) && \
cargo fmt --check && \
cargo clippy --workspace --all-targets -- -D warnings && \
cargo test --workspace && \
git add -A && \
git commit -m "$MSG" && \
git push -u origin "$BRANCH"
```

## Exécution étape par étape

```sh
BRANCH=$(git branch --show-current)

cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

git status
git diff --stat
git add -A
git commit -m "<Prefix>: <Summary>"
git push -u origin "$BRANCH"
```

## Notes

- Il n'y a pas de CI : ces commandes sont la vérification obligatoire.
- Ne pas committer `config/launcher.env`, `runtime/`, `target/`, `vendor/*.so`,
  les modèles binaires ou les corpus de parité.
- Ne pas committer une modification de `config/config.toml` contenant des
  chemins expérimentaux.
- Ne pas utiliser `git add -A` si des fichiers générés non ignorés traînent :
  vérifier `git status` d'abord.
- Pour un changement de contrat JSON/CSV/HTTP, ajouter le résultat de parité
  dans le corps du commit.
- En cas d'échec d'un check, corriger avant de committer ; ne pas contourner.
