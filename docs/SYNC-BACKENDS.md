# Synchronisation Python ↔ Rust

La cohabitation retenue est une **synchronisation au changement**, pas un
partage des racines Python. Les deux backends gardent leur base, leurs
téléchargements, leurs journaux, leurs projets et leurs archives STEP7. Avant
de basculer de l'un à l'autre, `workflow-cli sync` fusionne explicitement les
états qui doivent survivre au changement.

Le partage direct des racines a été écarté : il aurait obligé Rust à écrire
dans les répertoires Python en permanence, et un ancien processus Python aurait
pu modifier l'état pendant une exécution Rust. La synchronisation garde une
frontière nette et laisse Python intact entre deux bascules.

## 1. Ce qui est synchronisé

| État | Source | Identité | Sens |
| --- | --- | --- | --- |
| `download_history` | `download_history.sqlite3` | URL | union bidirectionnelle |
| `processed_archives.txt` | `logs/step1/` | nom de fichier | union bidirectionnelle, chemins remappés |
| Archives non traitées | dossiers de téléchargement | nom de fichier | copie ou déplacement |
| Analyses STEP7 | `archives/` | `(projet, hash vidéo)` | copie ou hardlink |

Les sorties finalisées (`/mnt/cache`) sont déjà partagées : elles ne sont pas
synchronisées.

## 2. Ce qui n'est pas synchronisé

- **Les échecs de téléchargement.** `download_failures` contient des cooldowns
  transitoires. Une URL en échec côté Python peut réussir côté Rust, et
  inversement. Les fusionner retarderait inutilement les nouvelles tentatives.
  Les compteurs sont seulement affichés dans le rapport.
- **Les projets en cours.** `projets_extraits` est un état de travail
  temporaire. Un projet non finalisé doit être terminé ou abandonné avant la
  bascule. Le rapport émet un avertissement s'il reste des dossiers.
- **Les sorties finalisées.** `/mnt/cache` est commun aux deux backends.
- **Les secrets et la configuration.** Chaque backend garde son `.env` ou son
  `config/launcher.env`.

## 3. Sécurité

La synchronisation écrit dans les deux bases et dans les deux listes. Elle
refuse donc de démarrer si :

- un serveur écoute sur 5003 ou 5004 ;
- un processus `workflow-server`, `app_new.py`, `workflow-cli` (hors `sync`) ou
  un script `workflow_scripts/step*` tourne encore ;
- une autre synchronisation tient le verrou `/tmp/workflow-sync.lock`.

`--allow-running` existe uniquement pour un diagnostic en lecture ; il est
refusé avec `--apply`. Il ne doit pas être utilisé pour une synchronisation
réelle.

Avant toute écriture, la commande crée un répertoire de sauvegarde. Les
fichiers présents dépendent des côtés réellement réécrits :

```text
runtime/backups/sync/<horodatage>/
  python-download_history.sqlite3          # si Python est écrit
  rust-download_history.sqlite3            # si Rust est écrit
  python-processed_archives_<horodatage>.txt  # si la liste Python non vide est réécrite
  rust-processed_archives_<horodatage>.txt    # si la liste Rust non vide est réécrite
  sync-report.json
```

Les bases sont sauvegardées avec `VACUUM INTO` (instantané cohérent, y compris
le WAL). Les listes non vides sont copiées avant réécriture. Les archives STEP7
ne sont pas supprimées en mode `copy` ; en mode `move` pour les téléchargements,
la source n'est supprimée qu'après vérification de la taille copiée.

## 4. Commandes

```sh
# Plan complet, aucune écriture
./scripts/sync-backends.sh

# Fusion réelle
./scripts/sync-backends.sh --apply

# Résumé lisible
./scripts/sync-backends.sh --apply --quiet

# Un seul sens
./scripts/sync-backends.sh --apply --direction python-to-rust

# Analyses STEP7 en hardlinks (même système de fichiers, économise l'espace)
./scripts/sync-backends.sh --apply --step7 link

# Ignorer une catégorie
./scripts/sync-backends.sh --apply --downloads skip --step7 skip
```

La commande sous-jacente est :

```sh
./target/release/workflow-cli --config config/config.toml sync --apply
```

Options principales :

| Option | Défaut | Effet |
| --- | --- | --- |
| `--apply` | non | écrit ; sans lui, simulation |
| `--direction` | `both` | `both`, `python-to-rust`, `rust-to-python` |
| `--history` | `merge` | `merge` ou `skip` |
| `--processed` | `merge` | `merge` ou `skip` |
| `--downloads` | `copy` | `copy`, `move`, `skip` |
| `--step7` | `copy` | `copy`, `link`, `skip` |
| `--normalize` | non | réécrit les URL avec la normalisation Rust |
| `--backup-dir` | `runtime/backups/sync/<horodatage>` | sauvegardes |
| `--allow-running` | non | diagnostic en lecture seule, incompatible avec `--apply` |
| `--quiet` | non | résumé au lieu du JSON |

## 5. Détail des fusions

### 5.1 Historique des téléchargements

Les deux implémentations conservent le plus petit timestamp vu pour une URL.
La fusion est donc idempotente : elle prend l'union des deux bases et, pour
chaque URL commune, le minimum des deux timestamps.

Par défaut, les URL sont fusionnées telles quelles. La commande vérifie
auparavant que chaque URL est déjà normalisée selon les règles Rust. Si une URL
ne l'est pas, elle s'arrête et demande `--normalize` : cela évite de réécrire
silencieusement une clé que le moniteur Python ne reconnaîtrait plus.

`--normalize` réécrit les deux bases avec la forme normalisée Rust. C'est une
migration explicite, à utiliser après un audit.

Les échecs ne sont pas fusionnés. Le rapport affiche seulement
`failures_python` et `failures_rust`.

### 5.2 Liste STEP1 des archives traitées

`processed_archives.txt` contient des chemins absolus. La synchronisation
identifie une archive par son **nom de fichier** et réécrit le chemin avec la
racine du backend cible :

```text
Python : /home/kidpixel/Téléchargements/101 Camille.zip
Rust   : /home/kidpixel/workflow-rs/runtime/downloads/101 Camille.zip
```

La réinitialisation mensuelle est respectée. Si le marqueur
`processed_archives.last_reset` n'est pas le mois courant, la liste est
sauvegardée puis vidée avant la fusion, exactement comme STEP1. Une entrée
dont le chemin est hors du dossier de téléchargement du backend est ignorée :
elle ne peut pas être traitée par le scan STEP1 et ne doit pas polluer l'autre
côté. Le rapport indique `python_ignored` / `rust_ignored` et des exemples.

### 5.3 Archives non traitées

Le scan reprend les extensions de STEP1 (`.zip`, `.zipx`, `.rar`, `.tgz`,
`.7z`, `.tar`), le mot-clé (`Camille` par défaut) et ignore les fichiers
partiels (`.part`, `.crdownload`, fichiers cachés).

- `copy` (défaut) : la source reste en place. Après traitement par le backend
  actif, la copie restée sur l'autre côté n'est pas supprimée automatiquement ;
  utiliser `move` ou nettoyer manuellement.
- `move` : la source est déplacée après une copie vérifiée, ou par `rename`
  atomique si les deux dossiers sont sur le même système de fichiers.
- Un fichier de même nom mais de contenu différent n'est **jamais écrasé** :
  il est signalé comme conflit.

### 5.4 Analyses STEP7

Les archives STEP7 sont organisées en
`<projet> <horodatage>/<hash vidéo>/<fichiers>`. La synchronisation identifie
une analyse par `(projet, hash vidéo)`, pas par le nom du dossier
horodaté. Un hash déjà présent côté cible est un doublon : il n'est pas
recopié. Si le hash existe mais que des fichiers manquent, seuls les fichiers
manquants sont ajoutés.

- `copy` (défaut) : copie indépendante, sûre.
- `link` : hardlink si possible, sinon copie. Les deux côtés partagent alors
  l'inode ; à réserver aux archives considérées comme immuables.
- Un fichier de même chemin mais de taille ou de contenu différent est signalé
  comme conflit et laissé intact.

Les fichiers `metadata.json` sont copiés tels quels. Le champ `video_path`
qu'ils contiennent peut référencer l'ancienne racine ; il est informatif et
n'intervient pas dans la recherche par hash.

## 6. Procédures de bascule

### 6.1 Python → Rust

1. Arrêter Python (Ctrl+C dans son terminal, ou fermeture du raccourci).
2. Vérifier que 5003 et 5004 sont libres.
3. Lancer :

   ```sh
   ./scripts/sync-backends.sh --apply
   ```

4. Démarrer Rust avec le raccourci `Lancer_Workflow_Rust.desktop`.

Le lanceur Rust exécute lui-même `sync-backends.sh --apply --quiet` avant de
démarrer le serveur. Il refuse de démarrer si Python répond encore sur 5003
alors que `webhook.enabled = true` : cela évite deux moniteurs concurrents.
Pour désactiver cette synchronisation automatique :

```sh
WORKFLOW_SYNC_ON_START=0 ./scripts/start-rust.sh
```

### 6.2 Rust → Python

1. Arrêter Rust (Ctrl+C dans le lanceur, ou `./scripts/switch-rust.sh stop`).
2. Lancer :

   ```sh
   ./scripts/sync-backends.sh --apply
   ```

3. Lancer le raccourci Python `Lancer_Workflow.desktop`.

Le raccourci Python reste inchangé. Il démarre son propre moniteur CSV ; c'est
pourquoi Rust doit être arrêté avant.

### 6.3 Bascule sans traitement

Si l'on veut seulement consulter l'autre interface sans traiter de nouvelle
archive, on peut utiliser `--downloads skip --step7 skip` pour ne fusionner que
l'historique et la liste STEP1. Les projets et archives restent alors dans
leur racine d'origine.

## 7. Restauration

Pour revenir à l'état d'avant une synchronisation :

1. arrêter les deux serveurs ;
2. copier les fichiers du répertoire de sauvegarde vers leur emplacement
   d'origine (les noms indiquent `python-` ou `rust-`) ;
3. relancer le backend voulu.

Exemple :

```sh
cp runtime/backups/sync/20261023_215036/python-download_history.sqlite3 \
  /home/kidpixel/workflow_mediapipe/download_history.sqlite3
cp runtime/backups/sync/20261023_215036/python-processed_archives_20261023_215036.txt \
  /home/kidpixel/workflow_mediapipe/logs/step1/processed_archives.txt
```

Les archives STEP7 copiées ne sont pas supprimées par une restauration : elles
peuvent être retirées manuellement si nécessaire.

## 8. Limites

- La synchronisation n'est pas un temps réel : elle s'exécute avant la
  bascule, serveurs arrêtés.
- Les échecs de téléchargement ne sont pas fusionnés.
- Les projets en cours ne sont pas déplacés.
- Le mode `link` partage les inodes : une réécriture en place d'un fichier
  d'analyse affecterait les deux côtés.
- Le premier passage STEP7 copie l'intégralité des analyses existantes
  (1,8 Go sur l'instance du 2026-10-23). Les passages suivants ne copient que
  les nouveaux hash.
- La synchronisation ne remplace pas la procédure de bascule publique par
  reverse proxy (`SWITCH-PUBLIC.md`).

## 9. État de l'instance au 2026-10-23

La première synchronisation réelle a été appliquée le 2026-10-23 à 21:50 :

| Élément | Python | Rust |
| --- | --- | --- |
| `download_history` | 911 URL | 911 URL |
| `download_failures` | 0 | 0 |
| `processed_archives.txt` | 96 entrées | 96 entrées |
| Marqueur STEP1 | `2026-10` | `2026-10` |
| Analyses STEP7 | 640 hash / 2 473 fichiers / 1,8 Go | 640 hash / 2 473 fichiers / 1,8 Go |
| Téléchargements en attente | 0 | 0 |

Une entrée de test (`Testalpha_Camille.zip`, hors dossier de téléchargement) a
été ignorée et retirée de la liste Python. Le rapport complet est dans
`runtime/backups/sync/20261023_215036/sync-report.json`. Le moniteur Rust est
actif sur 5004 ; Python 5003 est arrêté.

## 10. Validation de bout en bout isolée (2026-10-23)

Un test complet a été exécuté dans `/tmp/opencode/e2e-900`, avec une
configuration isolée et un faux backend Python. Il ne touche ni `runtime/`,
ni `~/Téléchargements`, ni `/mnt/cache` :

- archive synthétique `900_Test_Camille.zip` (vidéo `vox_2spk.mp4`, 30 s)
  servie en HTTP local, téléchargée par `workflow-cli download` après import
  de l'URL dans l'historique isolé ;
- STEP1 Rust : extraction, vidéo déjà conforme, `processed_archives.txt` écrit ;
- STEP2 → STEP7 : CSV, audio, tracking, `_tracking.json`, `_ae.json`, archive
  STEP7 et sortie finalisée produits sans échec ;
- sync `both` : historique 1 → 2, STEP1 1 → 2, 2 archives téléchargées
  copiées, 2 hash STEP7 copiés. Le fichier Rust reste dans
  `runtime/downloads` et une copie apparaît côté Python, ce qui confirme le
  comportement de `copy` ;
- deuxième sync : 0 copie, 4 téléchargements identiques ignorés, 4 hash
  dupliqués ignorés, historique et listes stables.

L'état réel est resté à 911 URL, 96 entrées STEP1 et 2 473 fichiers STEP7.
Le corpus `153 Camille` a par ailleurs été préparé et validé 15/15 par le
shadow STEP5/6 (voir `LOT-1-SHADOW.md`).
