# Lot 1 — STEP7 finalisation

## Périmètre livré

- Sélection des projets prêts selon `lenient`, `strict` ou `videos`.
- Archivage des analyses par empreinte SHA-256 de la vidéo :
  `archives/<projet> <UTC>/<sha256>/<stem>_{scenes.csv,audio.json,tracking.json}`,
  avec `metadata.json`.
- Copie du projet vers `paths.finalized` (`/mnt/cache` par défaut), puis
  normalisation des médias, CSV et JSON d'analyse dans `docs/`.
- Manifeste `logs/step7/finalized_projects_<timestamp>.json`, compatible avec
  le lecteur STEP8.
- Suppression de la source après publication ; le CLI expose `--no-delete` et
  `--dry-run` pour les tests.
- Restauration optionnelle des analyses archivées avec `--restore-archives`.

Commandes :

```sh
# Inventaire sans écriture.
workflow-cli step7 finalize --dry-run

# Publication vers /mnt/cache (comportement de production).
workflow-cli step7 finalize

# Test isolé : conserve la source et écrit dans /tmp.
workflow-cli step7 finalize \
  --work-dir /tmp/work --output-dir /tmp/out \
  --archives-dir /tmp/archives --logs-dir /tmp/logs --no-delete
```

Les chemins par défaut viennent de `paths.projects`, `paths.finalized`,
`paths.archives` et `paths.logs`. `steps.step7.mode`,
`steps.step7.restore_archives` et `steps.step7.delete_source` fournissent les
valeurs de configuration ; les options CLI les surchargent pour un run.

## Validation

Le 2026-10-22, un projet 250 reconstruit à partir de `docs/` de production a
été traité sur une copie isolée :

| Vérification | Résultat |
| --- | --- |
| Projet détecté en mode `lenient` | 1 |
| Arborescence `docs/` copiée | identique (`diff -r`) |
| SHA-256 de la vidéo | `ceed37b6…f63478`, identique à `sha256sum` |
| Archive `_scenes.csv`, `_audio.json`, `_tracking.json` | présents |
| Manifeste STEP7 | écrit, un projet |
| Source après `--no-delete` | conservée |
| Source après suppression | supprimée (test unitaire) |

Les tests unitaires couvrent aussi la sélection par mode, la normalisation des
fichiers racine vers `docs/`, l'archivage, la restauration et la suppression.

## Sécurité et durabilité

- Un lien symbolique rencontré dans l'arborescence source est refusé
  (`symlink interdit pendant la finalisation STEP7`), au lieu d'être suivi.
- Le manifeste STEP7 est écrit via `write_atomic` : fichier temporaire,
  `sync_all`, renommage, puis fsync du répertoire parent quand la plateforme
  le permet.

## Limites

- Le warning Python « média préservé uniquement » n'est pas repris : les
  fichiers `.mov` sont copiés tels quels, sans analyse supplémentaire.
- L'archivage utilise le dossier projet du `--work-dir` ; le script Python
  utilisait `PROJECTS_DIR`. Les deux coïncident avec les chemins par défaut.
- Aucune donnée de production n'a été déplacée ni supprimée pendant la
  validation ; les essais réels ont eu lieu sous `/tmp/opencode`.

## Tests

```sh
cargo test -p workflow-steps step7 --lib
cargo clippy --workspace --all-targets -- -D warnings
```
