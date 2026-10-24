# Rétention des données de travail

Cette politique couvre uniquement les données de travail du pipeline Rust. Elle
ne supprime jamais un livrable finalisé et ne cible jamais l'application Python
legacy, qui reste active et fonctionnelle.

## Règles par jeu de données

| Donnée | Emplacement par défaut | Rétention | Nettoyage |
| --- | --- | --- | --- |
| Projets sources | `runtime/projets_extraits` | Supprimés après STEP7 réussi ; orphelins après 48 h d'inactivité | `retention apply` |
| Résidus d'exécution interrompue | `runtime/` (`_temp_*`, `*.part`, `*.tmp*`) | 24 h | Purge au démarrage du serveur |
| Archives d'analyses | `runtime/archives` | 90 jours | `retention apply` |
| Journaux `.log` | `runtime/logs` | 30 jours | `retention apply` |
| Rapports STEP7/STEP8 | `runtime/logs/step7`, `runtime/logs/step8` | 90 jours | `retention apply` |
| Drivers STEP8 | `runtime/logs/step8/drivers` | 10 derniers fichiers | STEP8 |
| Téléchargements | `runtime/downloads` | Manuel (désactivé par défaut) | `--downloads-days` explicite |
| Sorties finalisées | `/mnt/cache` | Conservation indéfinie | Manuelle uniquement |
| Base d'historique | `runtime/download_history.sqlite3` | Conservation indéfinie | Manuelle uniquement |
| Sauvegardes de synchronisation | `runtime/backups/sync` | Manuelle | — |
| Application Python legacy | Racines Python | Hors-scope | Jamais par `retention` |

Les seuils sont configurables dans `[retention]` de `config/config.toml`. Le
seuil des orphelins reste `steps.step7.cleanup_threshold_hours`. Une valeur
`0` désactive le nettoyage correspondant. Le manifeste STEP7 le plus récent et
le dernier rapport STEP8 sont toujours conservés, même au-delà de 90 jours.

Au démarrage, le serveur purge aussi les `_temp_*`, `*.part` et `*.tmp*` de
plus de `server.orphan_max_age_hours` (24 h par défaut) sous `runtime/` ; une
valeur `0` désactive cette purge. Les fichiers récents sont conservés pour ne
pas interrompre une extraction ou un téléchargement en cours.

Le nettoyage des téléchargements est désactivé par défaut (`downloads_days =
0`). Le moniteur Rust écrit dans `runtime/downloads`, sous `paths.base` : ce
dossier peut donc être géré par `retention` si un seuil positif est fourni.
Le dossier Python `~/Téléchargements` reste hors-scope et n'est jamais
considéré comme géré.

### Pourquoi les sorties finalisées sont exclues

`paths.finalized` contient les `.ecproj` et les médias livrés. Le pipeline ne
connaît ni la date de validation client ni la date de livraison : une
suppression automatique serait irréversible et dangereuse. Le nettoyage de
`/mnt/cache` reste une décision humaine, après validation et archivage.

### Pourquoi les archives peuvent être purgées

STEP7 archive les analyses pour permettre une reconstruction, puis copie les
`docs/` dans la sortie finalisée. Après 90 jours, la copie de sécurité n'est
plus nécessaire au fonctionnement courant ; les livrables restent la référence.

La synchronisation Python → Rust copie les analyses dans `runtime/archives`.
Cette copie n'est pas une sauvegarde indépendante : `retention` peut la
supprimer après 90 jours, et le prochain `sync` la recopiera depuis Python si
elle existe encore. Les sauvegardes de `runtime/backups/sync` ne sont pas
purgées automatiquement ; les supprimer manuellement après validation.

## Commande

Le plan est toujours calculé sans suppression :

```sh
cd /home/kidpixel/workflow-rs
workflow-cli retention plan
```

L'application est explicite :

```sh
workflow-cli retention apply
```

Les deux commandes acceptent des surcharges de chemins et de seuils :

```sh
workflow-cli retention plan \
  --projects-dir /chemin/projets \
  --archives-dir /chemin/archives \
  --logs-dir /chemin/logs \
  --downloads-dir /chemin/downloads \
  --orphan-hours 48 \
  --archives-days 90 \
  --logs-days 30 \
  --downloads-days 0 \
  --reports-days 90
```

Le rapport JSON liste les candidats, les suppressions effectuées, les éléments
ignorés et les erreurs. `apply` revalide chaque candidat juste avant de le
supprimer ; un fichier modifié entre le plan et l'application est conservé.

## Garde-fous

- Seuls les répertoires configurés sont inspectés ; les sorties finalisées et
  la base d'historique ne sont jamais candidates.
- Les projets sources et les archives ne sont supprimés que s'ils sont des
  enfants directs du répertoire racine, des répertoires réels (pas des liens
  symboliques), et non protégés.
- Les noms `archives`, `logs`, `docs`, `static`, `templates`, `.git`,
  `_finalized_output`, `venv`, `env`, `models`, `downloads`, `runtime`,
  `projets_extraits`, ainsi que les préfixes `_temp_` et `.`, sont protégés.
- Une archive doit contenir `metadata.json`, un `*_audio.json`, un
  `*_tracking.json` ou un `*_scenes.csv` pour être éligible. Un répertoire
  inconnu dans `archives/` est conservé.
- Les journaux ne sont reconnus que sous la forme `*.log` ou `*.log.<numéro>`.
  Les rapports ne sont reconnus que sous les préfixes `step7`/`step8` attendus.
- Le nettoyage des téléchargements n'est accepté que sous `paths.base` ; le
  dossier Python `~/Téléchargements` reste hors-scope, tandis que
  `runtime/downloads` (moniteur Rust) est éligible.
- `apply` doit être lancé hors batch actif. Le seuil de 48 h protège les
  projets en cours, mais ne remplace pas cette règle d'exploitation.

## Calendrier recommandé

1. Chaque semaine : lancer `retention plan` et vérifier les candidats.
2. Chaque mois, ou après validation d'un lot : lancer `retention apply` si le
   plan est conforme.
3. Ne jamais planifier `apply` pendant une extraction, un tracking ou un batch
   STEP8.
4. Ne jamais lancer `retention apply` sur les racines de l'application Python
   legacy. Son moniteur de nettoyage (`cleanup_monitor`) reste actif pour son
   propre périmètre ; les deux politiques ne doivent pas partager les mêmes
   répertoires de travail.

## Application Python legacy

La migration Rust ne décommissionne pas et ne modifie pas l'application
Python. Elle reste installée, active et redémarrable comme workflow legacy, avec
son code, ses routes, ses venvs, sa base SQLite, ses archives et son moniteur de
nettoyage, sans changement imposé par les lots Rust.

- Aucun retrait de l'application Python n'est planifié, et aucune fenêtre de
  30 ou 90 jours ne conditionne sa suppression.
- La période de deux batches + 30 jours reste une fenêtre d'observation de la
  stabilité Rust ; elle ne déclenche aucun retrait.
- `retention` Rust ne cible que les racines Rust configurées. Les données
  Python restent la propriété du workflow legacy et ne sont jamais supprimées
  par cette commande.
- Les deux workflows doivent rester isolés : bases, répertoires de travail et
  ports distincts. Le verrou matériel Coral/GPU reste obligatoire pour éviter
  deux traitements lourds simultanés.
- Si un répertoire de travail devait être partagé, suspendre explicitement le
  nettoyage de l'un des deux workflows ; ne jamais laisser deux politiques de
  rétention actives sur la même racine.

## État au 2026-10-22

- Archives Python : 239 répertoires, environ 1,8 Gio, du 2026-08-03 au
  2026-10-22. Elles restent gérées par le workflow legacy et ne sont pas
  candidates à `retention`.
- Journaux Python : environ 55 Mio, conservés par le workflow legacy.
- Sorties finalisées : 7 projets, environ 3,2 Gio dans `/mnt/cache`.
- Les seuils proposés tiennent dans l'espace disque actuel ; ils restent des
  valeurs de départ, à ajuster après quelques semaines d'exploitation Rust.
