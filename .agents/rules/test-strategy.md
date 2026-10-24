---
trigger: model_decision
description: Règles de test pour workflow-rs (Rust) : table de perspectives, Given/When/Then, couverture et parité
globs:
---

# Stratégie de test — workflow-rs

Ces règles s'appliquent dès qu'un test est créé ou modifié. Une tâche de test
n'est terminée que si les étapes ci-dessous sont satisfaites.

## 1. Table de perspectives (partitions d'équivalence / bornes)

1. **Conception** : avant d'implémenter, produire une table Markdown des
   perspectives de test.
2. **Non bloquant** : ne pas attendre une validation utilisateur après la
   table ; enchaîner sur l'implémentation, sauf ambiguïté critique.
3. Colonnes minimales : `Case ID`, `Entrée / Précondition`, `Perspective
   (équivalence / borne)`, `Résultat attendu`, `Notes`.
4. Couvrir les cas normaux, les erreurs et les bornes : `0`, minimum, maximum,
   `±1`, vide, `NULL`/absent.
5. **Auto-contrôle** : utiliser la table comme checklist pendant le codage. Si
   un nouveau cas apparaît, vérifier qu'il est couvert.
6. Pour une modification mineure (message, log), la table est optionnelle.

### Modèle

| Case ID | Entrée / Précondition | Perspective | Résultat attendu | Notes |
|---|---|---|---|---|
| TC-N-01 | Entrée valide | Équivalence — normal | Traitement réussi | - |
| TC-A-01 | `NULL` / fichier absent | Borne — absent | Erreur explicite | - |

## 2. Politique d'implémentation

1. Implémenter **tous** les cas de la table en tests automatisés.
2. **Autant ou plus de cas d'échec que de cas nominaux** : validation,
   fichiers absents, JSON invalide, timeout, sous-processus en échec.
3. Couvrir :
   - cas nominaux ;
   - erreurs de validation et chemins d'exception ;
   - bornes (`0`, minimum, maximum, `±1`, vide, absent) ;
   - types/formats invalides ;
   - dépendances externes en échec (FFmpeg, ONNX Runtime, TFLite, SQLite,
     EffectCraft) ;
   - types et messages d'erreur.
4. Viser 100 % de couverture de branches quand c'est raisonnable. Sinon,
   couvrir en priorité les branches à fort impact métier et les chemins
   d'erreur principaux. Documenter les branches non couvertes et leur impact.
5. Les tests vivent **inline** dans les modules, sous `#[cfg(test)]`. Il n'y a
   pas de dossier `tests/` d'intégration.

## 3. Commentaires Given / When / Then

Chaque test doit être traçable :

```rust
// Given: préconditions
// When:  opération exécutée
// Then:  résultat attendu
```

Les commentaires vont juste au-dessus du test ou dans ses étapes.

## 4. Vérification des erreurs

1. Vérifier explicitement le **type** et le **message** d'erreur.
2. Pour les erreurs de validation de config, vérifier la clé et la valeur
   fautives.
3. Pour les dépendances externes, utiliser des fixtures ou des chemins
   temporaires et vérifier le code de sortie, le message et l'absence
   d'artefact partiel.

## 5. Commandes et couverture

Commandes de référence :

```sh
cargo test --workspace
cargo test -p workflow-steps step6 --lib
cargo test -p workflow-server --lib
cargo test -p workflow-ingest --lib
cargo test -p workflow-sync --lib
```

- La suite compte environ 233 tests (210 `#[test]` + 23 `#[tokio::test]`).
- Pas de CI : exécuter la suite manuellement avant de terminer.
- La couverture n'est pas instrumentée par défaut. Si `cargo-llvm-cov` est
  disponible, documenter la commande et le résultat ; sinon, l'indiquer.
- Les tests ne doivent toucher ni `/mnt/cache`, ni `runtime/`, ni le dépôt
  Python, ni le matériel Coral/CUDA réel.

## 6. Tests et parité

- Une suite verte ne prouve pas la parité : les modèles, le GPU et le Coral ne
  sont pas exercés.
- La parité réelle se mesure avec :
  - `scripts/shadow-step56.py` (STEP5/6 sur production) ;
  - `scripts/validate-lot5.py` (batch STEP5/6) ;
  - `scripts/validate-step1.py` (STEP1 Python/Rust) ;
  - `tools/parity/parity.py` (baseline Python, fixtures figées).
- Le harnais `parity/` relance **Python legacy** et compare à des fixtures ; il
  n'invoque pas les binaires Rust.
- Ne jamais écrire sous `/mnt/cache` pour un test. Utiliser une copie sous
  `/tmp/opencode`.

## 7. Notes opérationnelles

1. Un diff de test non conforme à ces règles est refusé en revue.
2. Même sans dépendance externe, simuler les échecs (fichier absent, JSON
   invalide, timeout).
3. Quand une branche ou une contrainte change, mettre à jour la table et le
   test simultanément.
4. Si un cas n'est pas automatisable (matériel, modèle, GPU), documenter :
   fonctionnalité, risque, procédure manuelle, résultat attendu, trace.
5. Toute PR ou commit qui change le comportement doit ajouter ou mettre à jour
   les tests correspondants.
6. Un refactor sans changement de comportement doit être couvert par les tests
   existants ; sinon, en ajouter.
