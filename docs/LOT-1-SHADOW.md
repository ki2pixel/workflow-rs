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

`/mnt/cache` contient les projets de production complets (dossiers `docs/`
avec vidéos et artefacts STEP1 à STEP6). Le shadow en couvrait dix le
2026-10-22 ; les corpus préparés depuis portent la couverture à vingt projets.

### Corpus 153 à 159 et 1720

Le 2026-10-23 et le 2026-10-24, sept corpus ont été ajoutés :

| Projet | Source | Vidéos | Préparation | Shadow |
| --- | --- | ---: | --- | --- |
| 153 Camille | ZIP + archive 2026-10-14 | 5 | STEP1 Rust, artefacts archive, STEP6 Python | 15/15 |
| 154 Camille | ZIP + archive 2026-10-14 | 4 | idem | 12/12 |
| 155 Camille | ZIP + archive 2026-10-14 | 2 | idem | 6/6 |
| 156 Camille | ZIP + archive 2026-10-14 | 5 | idem | 15/15 |
| 158 Camille | ZIP + archive 2026-10-14 | 5 | idem | 15/15 |
| 159 Camille | ZIP + archive 2026-10-14 | 4 | idem | 12/12 |
| 1720 Camille | ZIP seul | 4 | STEP1 Rust, STEP2-6 Python | 12/12 (+12/12 raw) |

La préparation réutilise STEP1 Rust pour l'extraction (vidéos déjà conformes),
puis copie les artefacts de l'archive STEP7 correspondante et lance le STEP6
Python pour les `_ae.json`. Pour 1720, faute d'archive, le STEP2-6 Python a
été exécuté sur les vidéos extraites.

Les archives STEP7 de 154, 155, 156, 158 et 159 proviennent du lot
`2026-10-14` ; les plus anciennes (`2026-09-17` pour 154/155) ne
correspondaient pas aux vidéos des ZIP actuels. Les vidéos étaient déjà
conformes : STEP1 n'a réencodé aucun fichier.

Le passage global `--limit 20` donne `162/162` vérifications identiques.

### 153 et `--prefer-raw`

Le STEP4 Python a été relancé sur `153 Camille` le 2026-10-24 avec la
détection d'objets active. Les `<stem>.json` bruts existent désormais, mais
`--prefer-raw` ne reproduit pas le `_tracking.json` d'archive : `5/15`
seulement. La production du 2026-10-14 avait une configuration STEP4 plus
détectrice (11 853 objets contre 4 977 sur la vidéo Manon). Le mode par défaut
reste `15/15`. La dérive est suivie dans `parity/DRIFT.md` (D-024).

## Limite restante

La validation Windows/After Effects est abandonnée (D-022) ; le branchement de
l'orchestrateur Rust reste hors de ce shadow. Le script ne prouve pas les
chemins d'échec du comparateur ; il compare des sorties de production valides.
