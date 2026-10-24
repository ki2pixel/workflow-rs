---
description: Mettre à jour la documentation Rust à partir du code, des mesures et de la memory-bank
---

# Workflow Docs Updater

> Ce workflow harmonise la documentation de workflow-rs à partir du code réel,
> des mesures de parité et de l'état de la memory-bank. Il ne remplace pas la
> relecture humaine.

## Protocoles critiques

1. **Outils autorisés** : `read`, `grep`, `glob`, `edit`, `write`, `shell`
   (limité aux audits `cargo metadata`, `cargo tree`, `wc`, `rg`, `find`, `ls`),
   `fast_read_file`/`fast_write_file` pour la memory-bank.
2. **Contexte** : lire uniquement
   `/home/kidpixel/workflow-rs/memory-bank/activeContext.md` au démarrage.
   Ne lire les autres fichiers que si une divergence majeure est détectée.
3. **Source de vérité** : code et mesures > docs existantes > memory-bank.
4. **Sécurité** : ne pas lire la memory-bank par chemin relatif ; utiliser
   `/home/kidpixel/workflow-rs/memory-bank/...`.

## Étape 1 — Audit structurel et métrique

### Cartographie

```sh
cargo metadata --no-deps --format-version 1 | jq '.packages[].name'
cargo tree -p workflow-steps
find crates -maxdepth 3 -type f -name '*.rs' | sort
```

### Volumétrie

```sh
find crates -name '*.rs' -print0 | xargs -0 wc -l | tail -1
wc -l README.md AGENTS.md docs/*.md parity/*.md
```

### Complexité et dette

```sh
rg -n 'TODO|FIXME|HACK|unwrap\(|expect\(' crates --glob '*.rs'
cargo clippy --workspace --all-targets -- -D warnings
```

### Fichiers récemment modifiés

```sh
git log --since='30 days ago' --name-only --pretty=format: | sort -u
```

## Étape 2 — Diagnostic triangulé

| Source | Rôle | Outil |
| --- | --- | --- |
| Intention | Le « pourquoi » | `memory-bank/activeContext.md`, `docs/LOT-*.md` |
| Réalité | Le « quoi » et le « comment » | code, `cargo metadata`, `rg` |
| Existant | L'état documenté | `README.md`, `docs/CONTRAT-ENVIRONNEMENT.md`, `parity/DRIFT.md` |

Identifier les divergences :

- une étape documentée qui n'existe plus ;
- une commande CLI obsolète ;
- un chiffre de test/parité périmé ;
- un chemin de production qui a changé ;
- une décision non consignée.

## Étape 3 — Sélection du standard de rédaction

- **API / routes** : entrées/sorties, codes, auth, erreurs.
- **Pipeline** : flux de données, fichiers consommés/produits, prérequis
  matériels.
- **Architecture** : crates, responsabilités, diagrammes Mermaid si utile.
- **Exploitation** : commandes, dry-run, rollback, limites.
- **Parité** : mesures, tolérances, écarts, décisions D-xxx.

Lire `.agents/skills/documentation/SKILL.md` avant de rédiger un document long.

## Étape 4 — Proposition de mise à jour

Produire un plan avant d'appliquer :

```markdown
## Plan de mise à jour

### Audit
- Cible : `docs/LOT-4-STEP3.md`
- Constat : la commande `--edgetpu` n'est plus à jour.

### Modifications
- Type : pipeline
- Diagnostic : obsolète
- Correction : ...

### Application du skill
- Modèle : fiche technique
- Éléments : TL;DR, problème, commandes, limites
```

## Étape 5 — Application et finalisation

1. Appliquer avec `edit` (changement ciblé) ou `write` (réécriture).
2. Vérifier les commandes et chemins.
3. Mettre à jour la memory-bank :
   - `progress.md` pour les deltas ;
   - `activeContext.md` pour l'état ;
   - `decisionLog.md` pour une décision durable.
4. Lancer `git diff --check`.
5. Vérifier qu'aucun fichier `runtime/` ou `/mnt/cache` n'est modifié.

## Checkpoints

Avant :

- [ ] TL;DR ou résumé si le document est long.
- [ ] Public cible identifié.
- [ ] Sources de vérité lues.

Pendant :

- [ ] Tableau de trade-offs si plusieurs options.
- [ ] Commandes copiables.
- [ ] Limites et rollback.
- [ ] Pas de style IA.

Après :

- [ ] Liens et chemins valides.
- [ ] Chiffres à jour.
- [ ] Cohérence avec `AGENTS.md`, `README.md`, `CONTRAT-ENVIRONNEMENT.md`.
- [ ] Memory-bank à jour.

## Traçabilité

- Le corps du commit mentionne les documents synchronisés et les décisions
  prises.
- Le workflow ne se termine pas si les checkpoints ne sont pas cochés.
- Les deltas de session sont résumés dans `memory-bank/progress.md` et
  `memory-bank/activeContext.md`, pas dans les documents de lot.

## Références

- `.agents/skills/workflow-docs-updater-plus/SKILL.md`
- `.agents/skills/documentation/SKILL.md`
- `AGENTS.md`
- `docs/CONTRAT-ENVIRONNEMENT.md`
- `parity/DRIFT.md`
- `memory-bank/`
