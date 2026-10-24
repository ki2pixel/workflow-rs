---
name: after-effects-bridge
description: Contrat du binaire Rust workflow-bridge pour After Effects (cuts, manifeste de tracking, erreurs, D-022).
---

# After Effects Bridge

> Remplace la skill legacy `after-effects-cep-panel`. Le panneau CEP et les
> scripts JSX restent en Python ; le pont Rust est un contrat disponible, pas
> un basculement de production (D-022).

## Rôle

`workflow-bridge` remplace deux analyses JSON appelées par le panneau After
Effects :

- `mode = "cuts"` : conversion du CSV de scènes en segments de composition ;
- manifeste de tracking (`layers`) : choix du visage/personne, centre,
  dispersion et raison de sélection.

Le format de sortie est inchangé : résultat direct, ou `{"error": ...}` en cas
d'échec. Les erreurs d'écriture utilisent le préfixe `[PY] Error:` sur stderr.

## Commande

```sh
cargo build --release -p workflow-bridge
./target/release/workflow-bridge \
  --manifest_path /tmp/ms_manifest.json \
  --output_path /tmp/ms_results.json
```

Le binaire :

1. lit le manifeste JSON ;
2. dispatche selon `mode` ou la présence de `layers` ;
3. écrit le résultat de façon atomique ;
4. retourne `0` en succès, `1` en erreur.

## Modes

### `cuts`

Entrée :

```json
{
  "mode": "cuts",
  "csv_path": "/path/to/scenes.csv",
  "frame_rate": 25,
  "comp_duration": 133.0,
  "config": {}
}
```

Sortie : segments de composition au format attendu par le JSX.

### `layers`

Entrée : manifeste contenant `layers`, avec les overrides de configuration
(`SPREAD_THRESHOLD`, poids, labels, `video_scale`). Sortie : analyse par calque.

## Vérifications

```sh
cargo test -p workflow-bridge --lib
```

Parité de référence :

| Vérification | Cas | Résultat |
| --- | ---: | --- |
| Cuts CSV, 4 projets | 4 | identique |
| `analyze_manifest`, 273 calques | 273 | identique |
| `analyze_manifest` sur `_ae.json` | 4 | identique |
| Overrides de config | 4 | identique |

Performance release sur 244 : cuts 21× plus rapide que Python.

## Règles de frontière

- Ne pas modifier le panneau CEP, les scripts JSX ou le dépôt Python.
- Ne pas brancher le binaire dans le JSX de production sans décision explicite.
- Ne pas changer le format de sortie ou le préfixe d'erreur.
- Ne pas ajouter de dépendance Windows : la production cible Linux (D-022).
- Le wrapper `{status: "ok", data: ...}` du CEP n'est pas modifié.

## Incidents

| Symptôme | Piste |
| --- | --- |
| `mode inconnu` | Vérifier `mode` (`cuts` uniquement) |
| `manifest.mode absent` | Ajouter `mode` ou `layers` |
| JSON d'erreur dans la sortie | Lire le champ `error`, corriger l'entrée |
| `[PY] Error:` | Erreur d'écriture ou de lecture de fichier |
| Résultat différent | Vérifier la config, le CSV, le tracking et l'audio |
| Binaire absent | `cargo build --release -p workflow-bridge` |

## Références

- `crates/workflow-bridge/src/main.rs`
- `crates/workflow-bridge/src/cuts.rs`
- `crates/workflow-steps/src/step6.rs` (`analyze_manifest_value`)
- `docs/LOT-1-BRIDGE.md`
- `parity/DRIFT.md` (D-022)
- `.agents/skills/after-effects-json-contract/SKILL.md`
