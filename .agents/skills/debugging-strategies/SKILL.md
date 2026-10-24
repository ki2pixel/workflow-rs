---
name: debugging-strategies
description: Débogage systématique Rust de workflow-rs (reproduction, hypothèses, outils, performance, concurrence).
---

# Stratégies de débogage

Transformer le débogage en démarche méthodique : reproduire, observer,
formuler une hypothèse, tester, corriger, vérifier.

## Quand l'utiliser

- Bug difficile à reproduire.
- Performance dégradée ou régression.
- Crash, deadlock, panique.
- Comportement incohérent entre Rust et Python.
- Fuite mémoire ou RSS anormal.
- Échec d'une étape matérielle.

## Principes

### 1. Méthode scientifique

1. **Observer** : quel est le comportement réel ?
2. **Reproduire** : quelle est la plus petite entrée qui échoue ?
3. **Hypothèse** : quelle cause unique explique le symptôme ?
4. **Prédire** : que devrait-on voir si l'hypothèse est vraie ?
5. **Tester** : modifier une seule variable.
6. **Conclure** : corriger, puis vérifier l'absence de régression.

### 2. État d'esprit

- Ne pas deviner ; mesurer.
- Chercher la cause racine, pas le symptôme.
- Réduire l'espace de recherche.
- Documenter chaque essai et son résultat.
- Si deux causes sont possibles, les isoler une par une.

### 3. Rubber duck

Expliquer le problème à voix haute, ligne par ligne, force à expliciter les
hypothèses implicites.

## Processus

### Phase 1 — Reproduire

- [ ] Version exacte (`git rev-parse HEAD`), config, commande.
- [ ] Entrée minimale reproductible.
- [ ] Comportement attendu vs observé.
- [ ] Conditions : device, modèle, variable d'environnement, verrou.
- [ ] Reproductibilité (1 fois sur 10 ? systématique ?).
- [ ] Logs et code de sortie.

### Phase 2 — Collecter

- `RUST_BACKTRACE=1`, `RUST_LOG=debug`.
- `cargo test -- --nocapture`.
- `strace -f`, `perf stat`, `perf top`.
- `lsof`, `ss`, `pgrep`, `ps`.
- `jq` pour les JSON, `sqlite3` pour l'historique.
- `nvidia-smi`, `ls -l /dev/apex_0`, `dmesg` pour le matériel.
- Comparer avec la fixture Python et `parity/DRIFT.md`.

### Phase 3 — Formuler une hypothèse

Une hypothèse doit être falsifiable :

- « Le provider CUDA n'est pas enregistré parce que `libonnxruntime` est
  chargé sans les libs CUDA voisines. »
- « Le JSON est tronqué parce que l'écriture n'est pas atomique. »
- « Le TPU est instable parce que deux threads accèdent aux tenseurs sans
  verrou. »

### Phase 4 — Tester

- Modifier une seule variable.
- Utiliser un test unitaire ou une fixture isolée.
- Comparer avant/après.
- Vérifier la sortie, le code de retour et les logs.
- Si l'hypothèse est fausse, revenir aux observations.

## Outils Rust

```sh
cargo test -p workflow-steps step2 --lib -- --nocapture
RUST_BACKTRACE=full cargo run --release -p workflow-cli -- step2 detect ...
RUST_LOG=debug cargo run --release -p workflow-server -- --config config/config.toml
cargo clippy --workspace --all-targets -- -D warnings
```

- `gdb`/`lldb` pour un crash natif.
- `perf` pour les hot spots.
- `cargo flamegraph` si disponible.
- `valgrind` pour la mémoire (hors async).
- `strace` pour les fichiers/réseau.
- `lsof` pour les verrous et fichiers ouverts.

## Outils legacy (comparaison uniquement)

- `py-spy` sur le script Python legacy pour comparer un profil.
- `pytest` legacy pour reproduire un comportement, jamais pour valider Rust.
- Ne pas modifier le dépôt Python.

## Patrons par symptôme

### Bug intermittent

- Chercher une dépendance à l'ordre, au temps, au parallélisme.
- Réduire les workers à 1.
- Vérifier les canaux, les verrous, les timeout.
- Instrumenter avec `tracing` et un identifiant de run.

### Performance

- Mesurer avant/après.
- Distinguer CPU, GPU, TPU, I/O, sérialisation JSON.
- Vérifier les allocations par frame.
- Vérifier les copies de buffers et les `clone()`.

### Production

- Ne pas reproduire sur `runtime/` ni `/mnt/cache`.
- Copier l'entrée sous `/tmp/opencode`.
- Comparer avec un artefact de production.
- Préserver les logs et la config exacte.

### Concurrence

- Vérifier les `Mutex` tenus à travers un `await`.
- Vérifier les canaux non bornés.
- Vérifier le verrou matériel.
- Vérifier la sérialisation serveur.

## Checklist rapide

- [ ] J'ai la version, la config et la commande exactes.
- [ ] J'ai une entrée minimale.
- [ ] J'ai une hypothèse falsifiable.
- [ ] Je n'ai changé qu'une variable.
- [ ] J'ai vérifié les logs et le code de sortie.
- [ ] J'ai comparé avec la fixture ou la production.
- [ ] J'ai ajouté un test de non-régression.
- [ ] Je n'ai pas touché `/mnt/cache`, `runtime/` ou le dépôt Python.

## Erreurs fréquentes

- Corriger plusieurs choses à la fois.
- Ignorer les logs.
- Supposer que le cache est vide.
- Confondre corrélation et causalité.
- Contourner le verrou pour « aller plus vite ».
- Modifier un test pour qu'il passe.
- Oublier de vérifier la parité après correction.
