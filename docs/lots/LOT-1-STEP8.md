# Lot 1 — STEP8 EffectCraft via `effectcraft-cli`

## Périmètre livré

STEP8 génère les projets `<video>_9x16.ecproj` en pilotant le panel
`assets/effectcraft/MediaSolution.jsx` (copie du panel de production
`workflow_mediapipe/scripts/MediaSolution-EC/MediaSolution.jsx`) avec
`effectcraft-cli`, en headless. Le pont Python, le lancement de l'application et
le mode `--bridge` sont hors-scope.

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

Le post-traitement A5/A6 qui suit prolonge la même progression. Le CLI émet :

```
POST_TOTAL: N
POST_CURRENT: i/N - <projet>
POST_PROGRESS: i/N - <projet>
POST_DONE: N - <échecs>
```

Dans le hook STEP8, ces marqueurs partagent `stdout` avec `MS_BATCH_*` pour
préserver l'ordre. `workflow-cli post run` les écrit sur `stderr` pour garder
son JSON intact sur `stdout`. `POST_CURRENT` met à jour le texte avant le
projet, `POST_PROGRESS` avance la barre après sa fin. Le serveur ajoute ces
projets au total STEP8 et publie `progress_phase = "post"` ; `assets/web`
affiche alors la barre en orange avec le statut « Post-traitement ».

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

## Correctif 2026-11-19 — `URIError` et objet d'erreur `effectcraft-cli`

Le batch STEP8 du 2026-11-19 (26 dossiers, 72 projets) a produit 67 projets
puis a échoué sur les 5 vidéos de `/mnt/cache/122 Camille`. Deux causes
distinctes se cumulaient :

1. `MediaSolution.jsx` appelait `decodeURI(csvFiles[k].name)` sans garde dans
   `findMatchingCsv`. Un nom de CSV contenant `%` suivi d'une espace
   (`… 92% des enfants …`) lève une `URIError: invalid hexadecimal digit
   found` ; la recherche de CSV échouait pour toutes les vidéos du dossier.
2. `effectcraft-cli` répond `{"error": "… (step 1)"}` (objet, pas tableau)
   quand `file.runScript` échoue. Le wrapper cherchait `prefs.set` dans ce
   tableau absent, concluait à tort que la préférence scripting n'était pas
   activée et sortait avant d'écrire `step8_run_*.json`.

Correctifs :

- `safeDecodeURI` protège `decodeURI` par `try/catch` et retombe sur le nom
  brut ; `findMatchingCsv` compare d'abord le nom décodé puis le nom brut.
  `decodeNameForLog` réutilise `safeDecodeURI`. La copie installée
  `~/.config/effectcraft/Scripts/ScriptUI Panels/MediaSolution.jsx` est
  synchronisée.
- `run_cli` distingue l'erreur de l'étape 0 (`prefs.set`, message conservé et
  bloquant) de l'étape 1 (`file.runScript`) : cette dernière journalise
  l'erreur et retourne `CliOutcome { return_code, result, timed_out: false }`,
  ce qui laisse `run_step8` écrire le rapport d'échec.
- Le driver écrit `MS_BATCH_RESULT` via `$.writeln` (tee du fichier de
  progression) et non plus `writeLn`, afin que le résultat `ok: false` et le
  détail des erreurs soient disponibles même quand le script lève ensuite.

Validation :

- `cargo test -p workflow-steps step8 --lib` : 17 tests, dont
  `cli_script_error_writes_failure_report_with_batch_details` et
  `cli_prefs_error_still_mentions_scripting_preference` ;
- évaluation réelle `effectcraft-cli 0.24.1` sur
  `/mnt/cache/122 Camille/docs` : les 5 CSV sont appariés, y compris
  `… 92% …` ;
- reprise ciblée `workflow-cli step8 run --folder "/mnt/cache/122 Camille"` :
  5/5 projets bruts et 5/5 `_assist.ecproj`, `MS_BATCH_RESULT` avec
  `ok: true`, rapport `success: true`, `cli_return_code: 0`, aucun `URIError`.

## Post-traitement A5/A6

Depuis le 2026-10-27, un STEP8 réussi enchaîne le post-traitement déterministe
A5/A6. Le projet brut `<vidéo>_9x16.ecproj` reste **inchangé** et un second
projet `<vidéo>_9x16_assist.ecproj` est toujours publié à côté (copie exacte
si aucune décision ne s'applique). Les artefacts `post.json`, `review.json` et
`audit.json` sont écrits sous `<docs>/post/<projet_assisté>/`.

Le post-traitement est activé par `[steps.step8.post]` et désactivable avec
`--no-post`. Il peut aussi être lancé seul avec `workflow-cli post run`. Sa
progression (`POST_*`) prolonge la barre STEP8 (D-050). Détail des décisions
A1/A2/A3/A4/A6, du template SDM et des validations :
`docs/lots/LOT-9-POST-STEP8.md`.

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

# Sans post-traitement A5/A6.
workflow-cli step8 run --folder "/tmp/projet" --no-post
```

Options : `--manifest`, `--folder` (répétable), `--cli`, `--panel-jsx`,
`--config-json`, `--logs-dir`, `--timeout-s`, `--dry-run`, `--no-post`.

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
