# Lot 1 — shadow STEP5/STEP6

## Commande

```sh
python3 scripts/shadow-step56.py --root /mnt/cache --limit 10
```

Le script lit les `docs/` de production, écrit dans `/tmp/workflow-shadow`
(ou `--out`) et compare chaque sortie au fichier de production avec le
comparateur sémantique du harnais. Il refuse d'écrire sous `/mnt/cache`.

Depuis le déplacement des anciens projets dans `/mnt/cache/old/`, la découverte
accepte `*/docs` et `*/*/docs`. L'option `--prefer-raw` remplace l'entrée
`<stem>_tracking.json` par le STEP4 brut `<stem>.json` pour tester le réducteur
STEP5 complet, pas seulement son idempotence.

## Résultat du 2026-10-22

La cible de dix projets est atteinte. Les dix projets sélectionnés couvrent
19 vidéos et 57 vérifications :

| Projet | Vidéos | STEP5 audio | STEP5 vidéo | STEP6 |
| --- | ---: | ---: | ---: | ---: |
| 148 Camille | 2 | OK | OK | OK |
| 151 Camille | 5 | OK | OK | OK |
| 152 Camille | 5 | OK | OK | OK |
| 244 Cvmille | 1 | OK | OK | OK |
| 245 Cvmille | 1 | OK | OK | OK |
| 249 Cvmille | 1 | OK | OK | OK |
| 250 Cvmille | 1 | OK | OK | OK |
| 253 Cvmille | 1 | OK | OK | OK |
| 254 Cvmille | 1 | OK | OK | OK |
| 257 Cvmille | 1 | OK | OK | OK |
| **Total** | **19** | **19/19** | **19/19** | **19/19** |

Les 57 vérifications sont identiques. Le second passage avec `--prefer-raw`
réduit les 19 fichiers STEP4 bruts et compare aux 19 `_tracking.json` de
production : 57/57 identiques également. Les entrées brutes et réduites sont
structurellement différentes (`frames` contre `frames_analysis`) ; le résultat
n'est donc pas une comparaison de fichier à lui-même.

Chaque projet enchaîne le shadow STEP5 audio, STEP5 vidéo puis STEP6 à partir
des sorties shadow et compare :

- `_audio.json` au fichier de production ;
- `_tracking.json` au fichier de production ;
- `_ae.json` au fichier de production.

Aucune écriture n'a été faite dans `/mnt/cache` ; les manifestes de taille et
de mtime avant/après sont identiques.

## Couverture disponible

`/mnt/cache` contient 14 projets complets, soit 23 vidéos avec les six artefacts
STEP1 à STEP6. Le shadow en couvre dix. Les quatre projets restants
(`260`, `262`, `263`, `265`) peuvent être ajoutés avec `--limit 14`.

Le dossier `153 Camille` a été préparé le 2026-10-23 et n'est plus une limite :

- extraction STEP1 Rust de `/mnt/cache/old/153 Camille.zip` (1,9 Go) vers
  `/mnt/cache/153 Camille/docs` : 5 vidéos H.264 1080p déjà conformes, plus le
  PNG, aucune réécriture ;
- artefacts STEP2 à STEP5 récupérés de l'archive de production
  `153 Camille 2026-10-14_16-17-13` (5 hash) : `<stem>.csv`,
  `<stem>_audio.json`, `<stem>_tracking.json` ;
- `_ae.json` générés par le STEP6 Python pour servir de référence ;
- shadow STEP5/6 : `15/15` vérifications identiques (5 vidéos × audio, vidéo,
  AE) le 2026-10-23.

Un passage `--limit 11` incluant `153 Camille` donne `81/81` vérifications
identiques (11 projets, 27 vidéos).

Limite : le STEP4 brut `<stem>.json` n'est pas présent, donc le mode
`--prefer-raw` n'est pas disponible pour ce projet. Le mode par défaut, qui
part du `_tracking.json` de production, est validé.

Les dossiers complets à la racine incluent désormais `148`, `151`, `152` et
`153`.

## Limite restante

La validation Windows/After Effects est abandonnée (D-022) ; le branchement de
l'orchestrateur Rust reste hors de ce shadow. Le script ne prouve pas les
chemins d'échec du comparateur ; il compare des sorties de production valides.
