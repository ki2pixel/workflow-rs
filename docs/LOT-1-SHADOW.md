# Lot 1 — shadow STEP5/STEP6

## Commande

```sh
python3 scripts/shadow-step56.py --limit 10
```

Le script lit `/mnt/cache/<projet>/docs`, écrit dans `/tmp/workflow-shadow`
(ou `--out`) et compare chaque sortie au fichier de production avec le
comparateur sémantique du harnais. Il refuse d'écrire sous `/mnt/cache`.

## Résultat du 2026-10-22

Quatre projets de production contiennent actuellement un `docs/` complet :
244, 245, 249 et 250. Les douze vérifications sont identiques :

| Projet | STEP5 audio | STEP5 vidéo | STEP6 |
| --- | --- | --- | --- |
| 244 | OK | OK | OK |
| 245 | OK | OK | OK |
| 249 | OK | OK | OK |
| 250 | OK | OK | OK |

Chaque projet enchaîne le shadow STEP5 (audio + tracking) puis STEP6 à partir
des sorties shadow, et compare :

- `_audio.json` au fichier de production ;
- `_tracking.json` au fichier de production (idempotence) ;
- `_ae.json` au fichier de production.

Aucune écriture n'a été faite dans `/mnt/cache`.

## Limite

La cible du plan est de dix projets. Seuls quatre projets complets sont
disponibles sur la machine à cette date ; le script est prêt à traiter les
suivants dès qu'ils apparaissent (`--limit 10`). La cible « dix projets » reste
donc ouverte et ne doit pas être déclarée acquise.
