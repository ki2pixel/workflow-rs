# Lot 1 — pont After Effects (`workflow-bridge`)

## Périmètre livré

Le binaire `workflow-bridge` remplace les deux analyses JSON appelées par le
panneau After Effects :

- `mode = "cuts"` : conversion du CSV de scènes en segments de composition
  (`media_solution_bridge.py`) ;
- mode tracking : `analyze_manifest` de `preprocess_ae_json.py` (choix du
  visage/personne, centre et dispersion par calque).

Le format du fichier de sortie est inchangé : résultat direct, ou
`{"error": ...}` en cas d'échec. Le préfixe `[PY] Error:` reste utilisé sur
stderr pour les erreurs d'écriture. Le panneau et le JSX de production ne sont
pas modifiés dans ce lot : le basculement des chemins `pythonCutsScriptPath` et
`step7AnalyzerScriptPath` se fera au cutover.

## Parité

| Vérification | Cas | Résultat |
| --- | ---: | --- |
| Cuts CSV, 4 projets | 4 | identique |
| `analyze_manifest`, tous les segments CSV | 273 calques | identique |
| `analyze_manifest` sur une entrée `_ae.json` | 4 | identique |
| Overrides `SPREAD_THRESHOLD`, poids, labels et `video_scale` | 4 | identique |

Les comparaisons sont faites avec le pont Python d'origine
(`media_solution_bridge.py`, `preprocess_ae_json.py`) sur les CSV, tracking et
audio de production. Aucun écart de valeur, de centre, de dispersion, de label
ou de raison de sélection.

## Performance

Mesures release sur 244 :

| Charge | Python | Rust | Ratio |
| --- | ---: | ---: | ---: |
| Cuts CSV | 0,0576 s | 0,0027 s | 21× |
| 78 calques (stress) | 0,4578 s | 0,0837 s | 5,5× |

Le panneau appelle normalement `analyze_manifest` pour un calque à la fois ; le
coût est alors dominé par le chargement du tracking et de l'audio, pas par
l'analyse.

## Construction

```sh
cargo build --release -p workflow-bridge
./target/release/workflow-bridge \
  --manifest_path /tmp/ms_manifest.json \
  --output_path /tmp/ms_results.json
```

Le code ne contient aucune dépendance spécifique à Linux. La compilation
Windows (`x86_64-pc-windows-gnu` ou MSVC) et le test dans After Effects restent
à faire avant de remplacer le script Python dans le JSX.

## Limites

- Le binaire Windows `.exe` n'a pas encore été compilé ni validé sur la machine
  de production.
- Le JSX et le CEP continuent d'appeler Python ; aucun changement de production
  n'a été effectué.
- Le mode `cuts` conserve le format direct attendu par le JSX ; le wrapper
  `{status: "ok", data: ...}` du CEP n'est pas modifié.
