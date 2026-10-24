---
name: shrimp-task-manager
description: Planifier et suivre les tâches de migration/exploitation de workflow-rs avec les outils shrimp-task-manager.
---

# Shrimp Task Manager

## Expertise

Transformer une demande complexe en plan structuré :

- parser un PRD ou une demande ;
- analyser la complexité technique ;
- générer un backlog priorisé ;
- décomposer en sous-tâches avec dépendances ;
- suivre l'exécution et vérifier les résultats.

## Modèle mental

1. **Plan** : comprendre le besoin et le contexte.
2. **Analyse** : évaluer la faisabilité, les risques et les dépendances.
3. **Décomposition** : créer des sous-tâches indépendantes.
4. **Exécution** : suivre les tâches dans l'ordre.
5. **Vérification** : scorer et valider.
6. **Réflexion** : identifier les optimisations.

## Workflow obligatoire

1. **Lire le contexte** :
   `/home/kidpixel/workflow-rs/memory-bank/activeContext.md`.
2. **Plan Task** : `plan_task` avec la description et les exigences.
3. **Analyze Task** : `analyze_task` pour la faisabilité et les risques.
4. **Reflect Task** : `reflect_task` pour réviser l'analyse.
5. **Split Tasks** : `split_tasks` en sous-tâches avec dépendances.
6. **Execute** : `execute_task` pour chaque sous-tâche.
7. **Verify** : `verify_task` avec un score et des critères.
8. **Reflect** : `reflect_task` sur le résultat.
9. **Update** : `memory-bank/progress.md` et `activeContext.md`.

## Outils

| Outil | Usage |
| --- | --- |
| `plan_task` | Planification initiale |
| `analyze_task` | Analyse technique |
| `reflect_task` | Revue critique |
| `split_tasks` | Décomposition |
| `list_tasks` | État des tâches |
| `execute_task` | Exécution guidée |
| `verify_task` | Vérification et score |
| `query_task` | Recherche |
| `get_task_detail` | Détail d'une tâche |
| `update_task` | Mise à jour |
| `delete_task` | Suppression |
| `clear_all_tasks` | Nettoyage (destructif) |
| `process_thought` | Raisonnement |
| `init_project_rules` | Règles projet |
| `research_mode` | Recherche |

## Exemples

### Nouveau lot

```text
1. plan_task: "Porter STEP9 X en Rust"
2. analyze_task: dépendances, risques, tests
3. split_tasks:
   - lecture du contrat
   - implémentation
   - tests unitaires
   - parité
   - documentation
4. execute_task pour chaque sous-tâche
5. verify_task avec critères
```

### Évolution d'une étape

```text
1. analyze_task: impact sur JSON/CSV/HTTP
2. split_tasks: code, tests, docs, parité
3. execute_task
4. verify_task
```

### Incident production

```text
1. plan_task: diagnostic
2. analyze_task: hypothèses et risques
3. split_tasks: reproduction, isolation, correctif, validation
4. execute_task
5. verify_task
```

## Priorités

- **P0** : production cassée, sécurité, perte de données.
- **P1** : régression de parité, étape bloquée, incident moniteur.
- **P2** : performance, dette, documentation.
- **P3** : confort, refactor sans impact.

## Intégration memory-bank

- Lire `activeContext.md` avant de planifier.
- Mettre à jour `progress.md` après chaque tâche terminée.
- Consigner les décisions durables dans `decisionLog.md`.
- Ne pas dupliquer le plan complet dans la memory-bank : garder un résumé et
  un lien.

## Pièges

| Piège | Symptôme | Correction |
| --- | --- | --- |
| Tâche trop complexe | Impossible à vérifier | Découper |
| Dépendances manquées | Blocage en cours | Cartographier le DAG |
| Score de vérification faible | Tâche non terminée | Revoir les critères |
| Plan rigide | Le contexte change | Replanifier |
| Tâches sans propriétaire | Personne ne sait qui fait quoi | Assigner |
| Plan non suivi | Dérive | Mettre à jour à chaque fin de tâche |

## Debugging checklist

- [ ] La demande est comprise.
- [ ] Les dépendances sont identifiées.
- [ ] Les risques sont évalués.
- [ ] Les tâches sont indépendantes ou ordonnées.
- [ ] Les critères de vérification sont explicites.
- [ ] Les tests/parité sont prévus.
- [ ] La mémoire est mise à jour.

## Quand l'utiliser

- Nouveau lot de migration.
- Fonctionnalité multi-fichiers.
- Incident complexe.
- Roadmap ou backlog.

## Quand ne pas l'utiliser

- Correction triviale d'une ligne.
- Question de lecture de code.
- Tâche déjà parfaitement spécifiée et unique.
