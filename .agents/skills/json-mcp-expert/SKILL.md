---
name: json-mcp-expert
description: Manipuler les JSON massifs de workflow-rs (tracking, audio, ae, config) sans charger tout le fichier.
---

# JSON Query Expert

> Le nom historique du dossier est `json-mcp-expert` ; le frontmatter legacy
> utilisait `json-query-expert`. Cette version cible les outils réellement
> disponibles : `json_query_query_json`, `json_query_search_keys`,
> `json_query_search_values` et `jq`.

## Modèle mental

Les artefacts de tracking et d'audio peuvent dépasser 100 Mo. Le pattern
« sniper » consiste à :

1. localiser une clé ou une branche ;
2. extraire uniquement le nœud utile ;
3. éditer chirurgicalement ;
4. valider la syntaxe et l'invariant.

## Workflow obligatoire

1. **Inspection** : `json_query_search_keys` ou `json_query_search_values`
   pour localiser.
2. **Extraction** : `json_query_query_json` pour lire la branche.
3. **Localisation** : `grep` pour trouver la ligne exacte dans le fichier.
4. **Édition** : `edit` pour une modification ciblée.
5. **Validation** : `jq empty` ou un test de schéma.

## Outils

- `json_query_query_json(json_data, query)` : requête simple par clé.
- `json_query_search_keys(json_data)` : toutes les clés.
- `json_query_search_values(json_data, key)` : toutes les valeurs d'une clé.
- `jq` pour les fichiers locaux :
  ```sh
  jq '.frames_analysis | length' fichier.json
  jq 'keys' fichier.json
  jq '.frames_analysis[0]' fichier.json
  ```

## Patterns

### Config TOML/JSON

```sh
rg -n '^\[steps\.step4\]' config/config.toml
rg -n 'device|object_min_missing|face_reset_frames' config/config.toml
```

### Tracking massif

```sh
jq '{total_frames, fps, frames: (.frames_analysis|length)}' tracking.json
jq '[.frames_analysis[] | .frame_index] | {first: .[0], last: .[-1]}' tracking.json
```

### Audio

```sh
jq '{total_frames, fps, speakers: (.speaker_stats|length)}' audio.json
jq '.frames_analysis[0]' audio.json
```

### `_ae.json`

```sh
jq 'keys' ae.json
jq '.dataByFrame | keys | length' ae.json
jq '.segment_recommendations[0]' ae.json
```

## Règles de sécurité

- Ne jamais charger un fichier > 1000 lignes avec `read`.
- Ne jamais réécrire un JSON contractuel avec un script ad hoc : utiliser le
  code Rust et ses tests.
- Ne pas changer l'ordre des clés.
- Ne pas utiliser `jq` pour « corriger » un fichier de production ; `jq` sert
  au diagnostic.
- Valider après édition : `jq empty fichier.json`.
- Ne pas éditer un fichier sous `runtime/` ou `/mnt/cache` sans plan de
  rollback.

## Erreurs fréquentes

| Symptôme | Cause |
| --- | --- |
| `jq` renvoie `null` | Chemin inexistant ou mauvaise casse |
| `Cannot index array with string` | Type de nœud incorrect |
| Fichier corrompu après édition | Édition partielle ou virgule manquante |
| Clés dans le désordre | Réécriture par un outil non contractuel |
| JSON tronqué | Écriture non atomique ou interruption |
| Mémoire saturée | Fichier chargé entièrement |

## Intégration

- Avec `sequentialthinking-logic` : décomposer une transformation JSON
  complexe avant de l'implémenter.
- Avec `shrimp-task-manager` : créer une tâche par artefact ou par invariant.
- Avec `fast-filesystem` : lecture ciblée, chemins absolus.

## Quand l'utiliser

- Comprendre un JSON de production.
- Vérifier un invariant (`fps`, `total_frames`, `frame_index`).
- Localiser une clé avant une modification de code.
- Diagnostiquer un écart de parité.

## Quand ne pas l'utiliser

- Pour éditer un JSON de production sans passer par le code et les tests.
- Pour contourner un réducteur streaming.
- Pour modifier un fichier sous `/mnt/cache`.
