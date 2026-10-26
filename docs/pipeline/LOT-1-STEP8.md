# Lot 1 — STEP8 EffectCraft via `effectcraft-cli`

## Périmètre livré

STEP8 génère les projets `<video>_9x16.ecproj` en pilotant
`scripts/MediaSolution-EC/MediaSolution.jsx` avec `effectcraft-cli`, en
headless. Le pont Python, le lancement de l'application et le mode `--bridge`
sont hors-scope.

Le wrapper :

- lit le manifeste STEP7 le plus récent ou des dossiers `--folder` ;
- sélectionne les dossiers contenant `docs/` et applique
  `docsSubFolderName` du fichier `mediasolution_ec.json` ;
- concatène le panel et un driver `runBatch(..., null, 0)` (« Tout exécuter ») ;
- active la préférence d'écriture par le CLI, pas par le script ;
- relaie les lignes du panel et les lignes normalisées `MS_BATCH_*` ;
- écrit `logs/step8/step8_run_<timestamp>.json` et purge les anciens drivers ;
- applique un timeout interne et arrête le groupe de processus sur
  SIGTERM/SIGINT.

## Point important : le gate scripting headless

Avec `effectcraft-cli 0.13.0`, `script <driver>` démarre une session headless
avec `scripting.allowScriptsWriteFiles = false`. Un script ne peut pas
s'octroyer cette permission : `app.run("prefs.set", …)` est refusé avec
« Scripts can't change what scripts are allowed to do ». Le simple
`effectcraft-cli script` ne suffit donc pas.

Le wrapper utilise une seule session CLI :

```sh
effectcraft-cli --empty --json run \
  prefs.set '{"key":"scripting.allowScriptsWriteFiles","value":true}' \
  file.runScript '{"path":"<driver.jsx>"}'
```

`prefs.set` est exécuté par le CLI, puis `file.runScript` charge le driver dans
la même session. Aucun bridge, aucun lancement d'`effectcraft`, aucune
préférence globale modifiée. Le `--empty` évite le projet de démonstration.

Comme `file.runScript` bufferise la sortie du script, le driver installe un tee
sur `$.writeln` : chaque ligne du panel est ajoutée à
`logs/step8/drivers/step8_progress_<timestamp>.log`. Un thread Rust lit ce
fichier en temps réel, imprime la ligne brute et émet les lignes `MS_BATCH_*`
attendues par le polling backend :

```
MS_BATCH_TOTAL: N
MS_BATCH_PROGRESS: i/N - <vidéo>
MS_BATCH_OK: <projet.ecproj>
MS_BATCH_FAIL: <vidéo>
MS_BATCH_END: <résumé>
MS_BATCH_RESULT: {...}
```

Le résultat final `MS_BATCH_RESULT` reste produit par le driver et évalué par
Rust (`ok == true` et `errors` vide).

## Validation réelle

Projet 250, copie isolée sous `/tmp/opencode`, `effectcraft-cli 0.13.0` :

| Vérification | Résultat |
| --- | --- |
| Dossiers validés | 1 |
| Driver généré | oui |
| Lignes panel relayées | oui, y compris `MS_BATCH_PROGRESS` |
| Projet créé | `<video>_9x16.ecproj`, 1 370 013 octets |
| Ouverture du `.ecproj` | oui (`effectcraft-cli --project … info`) |
| Composition | 1080×1920, 25 fps, 133 s |
| Segments | 82 segments + `Null Segments` |
| Rapport | `success: true`, `mode: headless`, `bridge_port: null` |
| Code de sortie | 0 |

Le projet de production existant (généré avant ce run) contient 85 calques et
pas de `Null Segments` : il reflète une configuration antérieure. Le projet
Rust suit la configuration panel actuelle (`nullParentSegments: true`) ; la
comparaison porte donc sur la composition et les 82 segments, pas sur une
égalité binaire avec l'ancien `.ecproj`.

Aucune écriture n'a été faite dans `/mnt/cache` ni dans les dossiers de
production. Les seuls fichiers créés sont sous `/tmp/opencode`.

## Commandes

```sh
# Vérification sans exécuter EffectCraft.
workflow-cli step8 run --dry-run

# Dossiers manuels.
workflow-cli step8 run --folder "/tmp/projet"

# Après STEP7 : dernier manifeste de logs/step7.
workflow-cli step8 run

# Timeout interne explicite (0 désactive le timeout).
workflow-cli step8 run --timeout-s 3600
```

Options : `--manifest`, `--folder` (répétable), `--cli`, `--panel-jsx`,
`--config-json`, `--logs-dir`, `--timeout-s`, `--dry-run`.

## Annulation et timeout

Le wrapper place `effectcraft-cli` dans son propre groupe de processus. Sur
SIGTERM ou SIGINT, il envoie SIGTERM au groupe, attend jusqu'à 5 secondes, puis
envoie SIGKILL. Le wrapper sort avec le code `143`, comme le script Python.
Aucun rapport n'est écrit dans ce cas, puisque l'arrêt est demandé de
l'extérieur.

Le timeout interne est actif par défaut via `steps.step8.timeout_s` (14 400 s).
`--timeout-s` permet de le surcharger ; `0` le désactive. À l'expiration, le
groupe est arrêté de la même manière, un rapport est écrit avec
`timed_out: true`, `timeout_s` et `cli_return_code: 124`, et le wrapper sort
avec le code `124`. Ce timeout est une protection du wrapper, indépendante du
timeout du futur exécuteur de workflow.

Validation locale sur un faux CLI qui dort : SIGTERM arrête le groupe,
le wrapper sort avec `143` et l'enfant n'est plus présent. Un timeout de 1 s
écrit un rapport `timed_out: true` et sort avec `124`.

## Limites

- `effectcraft-cli` 0.13 est requis pour `run prefs.set file.runScript` et pour
  l'ouverture des `.ecproj` produits.
- Le driver contient le panel complet (environ 82 Kio) ; les anciens drivers et
  fichiers de progression sont purgés au-delà de `max_drivers_kept`.
- La validation Windows/After Effects du pont CEP reste hors-scope de ce lot.
- Le timeout et l'arrêt de groupe sont testés sous Linux ; la branche Windows
  utilise `taskkill /T /F` mais n'a pas été validée dans ce lot.

## Tests

```sh
cargo test -p workflow-steps step8 --lib
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```
