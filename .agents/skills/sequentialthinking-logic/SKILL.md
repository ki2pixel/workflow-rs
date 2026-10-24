---
name: sequentialthinking-logic
description: Raisonnement décomposé pour les architectures Rust de workflow-rs (pipeline, verrous, serveur, parité).
---

# Sequential Thinking Logic

## Expertise

Décomposer un problème complexe en étapes logiques validées :

- analyser un flux de données entre étapes ;
- valider les dépendances et les invariants ;
- identifier les points de défaillance ;
- construire une chaîne de raisonnement traçable.

## Modèle mental

1. **Décomposer** : identifier les composants et leurs contrats.
2. **Valider** : utiliser `sequentialthinking_tools` pour chaque étape.
3. **Chaîner** : connecter les étapes en une séquence cohérente.
4. **Tester la logique** : vérifier les hypothèses et les points de rupture.

## Workflow obligatoire

1. **Décomposition** : lister les composants, entrées, sorties, invariants.
2. **Validation** : pour chaque composant, vérifier les dépendances et les
   erreurs possibles.
3. **Chaînage** : vérifier que la sortie de l'étape N est l'entrée de N+1.
4. **Test logique** : simuler les cas limites et les échecs.
5. **Conclusion** : produire un plan ou un diagnostic, pas du code.

## Patterns

### Analyser un pipeline

```text
STEP1 produit docs/*.mp4 normalisés
→ STEP2 produit <stem>.csv
→ STEP3 produit <stem>_audio.json
→ STEP4 produit <stem>_tracking.json (requiert CSV + audio)
→ STEP5 produit les JSON réduits
→ STEP6 produit <stem>_ae.json
→ STEP7 publie et archive
→ STEP8 produit les .ecproj
```

Questions :

- Quelle étape peut tourner sans entrée ?
- Quel artefact est optionnel ?
- Quel échec est fatal ?
- Quel échec est un skip attendu (D-002) ?
- Où le verrou matériel doit-il être pris ?

### Analyser un verrou

```text
STEP2/3/4 prennent /tmp/workflow-rs-hw.lock
→ le serveur sérialise les étapes
→ une deuxième étape renvoie 409
→ stop/cancel envoie SIGTERM
```

Questions :

- Que se passe-t-il si le verrou est déjà tenu ?
- Que se passe-t-il si le processus meurt ?
- Le verrou est-il relâché par `Drop` ?
- Le serveur vérifie-t-il l'état avant de lancer ?

### Analyser une parité

```text
Python produit un JSON
→ Rust produit un JSON
→ le comparateur applique les tolérances
→ les écarts sont documentés dans DRIFT.md
```

Questions :

- Quel champ est contractuel ?
- Quel champ est ignoré ?
- Quel arrondi est appliqué ?
- Quelle fixture fait foi ?

## Gestion des erreurs logiques

- Identifier les points de défaillance :
  - entrée absente ;
  - format invalide ;
  - timeout ;
  - device indisponible ;
  - verrou tenu ;
  - disque plein ;
  - JSON tronqué.
- Pour chaque point, définir : détection, message, code de sortie, artefact.
- Vérifier qu'aucun chemin ne produit un succès silencieux.

## Pièges

| Piège | Symptôme | Correction |
| --- | --- | --- |
| Séquence incomplète | Une étape manque une entrée | Déclarer le DAG |
| Dépendance circulaire | Deux étapes s'attendent | Casser la dépendance |
| Mauvais ordre | STEP4 lancé avant STEP2/3 | Vérifier les fichiers requis |
| Verrou oublié | Deux étapes matérielles | `HardwareLock` |
| Succès silencieux | Code 0 sans sortie | Vérifier les artefacts |
| Parité supposée | Tests verts mais JSON différent | Shadow/parity |

## Debugging checklist

- [ ] Les entrées existent.
- [ ] Les sorties attendues sont nommées.
- [ ] Les erreurs sont explicites.
- [ ] Les cas limites sont couverts.
- [ ] Les verrous sont pris et relâchés.
- [ ] Le serveur sérialise.
- [ ] La parité est mesurée, pas supposée.

## Quand l'utiliser

- Concevoir une nouvelle étape ou un nouveau lot.
- Diagnostiquer un enchaînement qui échoue.
- Vérifier un contrat avant de coder.
- Préparer un plan d'exécution complexe.

## Intégration

- Avec `shrimp-task-manager` : décomposer en tâches.
- Avec `json-mcp-expert` : inspecter les artefacts.
- Avec `rust-concurrency-orchestrator` : vérifier les verrous et canaux.
- Avec `workflow-operator` : exécuter le plan.
