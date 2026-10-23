# Lot 8 — Ingestion webhook, historique et téléchargement

| Champ | Valeur |
| --- | --- |
| Version | 1.0 |
| Date | 2026-10-23 |
| Portée | Moniteur webhook, normalisation d'URL, historique SQLite, échecs/cooldown, téléchargement R2/Dropbox, statuts CSV |
| Statut | Implémenté et validé sur la base réelle ; moniteur désactivé par défaut |

## 1. Pourquoi ce lot

Les STEP1 à STEP8 étaient portées, mais les archives n'arrivaient dans le
dossier de travail que par le moniteur Python. Le lot 8 ferme cette dépendance :
`workflow-ingest` interroge le webhook, décide quels liens télécharger, écrit
l'historique et expose les statuts attendus par le panneau.

Le moniteur reste **désactivé par défaut**. Une base vide associée au webhook
réel ferait re-télécharger l'intégralité des liens historiques. La migration est
donc une opération explicite.

## 2. Architecture

```text
workflow-server (axum)
  ├── AppState
  │     ├── Runner (STEP1-8)
  │     └── IngestionService
  │           ├── WebhookClient      cache 60 s, 3 tentatives
  │           ├── HistoryRepository  download_history + download_failures
  │           ├── Downloader         HEAD, .part, Range, validation ZIP
  │           └── DownloadTracker    actifs + 20 derniers, JSON frontend
  │
  └── workflow-cli history import|audit|stats
      workflow-cli webhook probe
      workflow-cli download --url ...   diagnostic, sans historique
```

`workflow-ingest` ne dépend d'aucun service Python. Le serveur ne fait que
construire le service et exposer deux routes.

## 3. Sécurité d'activation

Trois verrous :

1. `webhook.enabled = false` dans `config/config.toml`.
2. `webhook.require_history = true` : le serveur refuse de démarrer le moniteur
   si `download_history` est vide.
3. `steps.step1.source_dir` doit être égal à `paths.downloads` quand le
   moniteur est actif, sinon STEP1 ne verrait pas les archives téléchargées.

Le dossier de téléchargement Rust est `runtime/downloads`, distinct de
`~/Téléchargements` utilisé par Python. C'est un écart volontaire d'isolation
(D-019). Pour une bascule complète, aligner explicitement les deux variables :

```bash
export WORKFLOW_PATHS_DOWNLOADS=/chemin/commun
export WORKFLOW_STEP1_SOURCE_DIR=/chemin/commun
```

## 4. Migration de l'historique

La base Python contient 911 URL au 2026-10-23. L'import ouvre la source en
lecture seule et conserve le plus petit timestamp par URL.

```bash
cd /home/kidpixel/workflow-rs

# 1. Compter sans écrire
./target/release/workflow-cli history import \
  --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3 --dry-run

# 2. Importer dans runtime/download_history.sqlite3
./target/release/workflow-cli history import \
  --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3

# 3. Vérifier que les URL sont déjà normalisées
./target/release/workflow-cli history audit

# 4. Contrôler les compteurs
./target/release/workflow-cli history stats
```

`history import --replace` remplace l'historique Rust au lieu de fusionner.
Les échecs Python ne sont pas importés : les objets R2 expirés ne doivent pas
imposer un cooldown à la nouvelle instance.

Au 2026-10-23, la base réelle `runtime/download_history.sqlite3` contient les
911 URL (audit à 0 changement). Le moniteur Python peut encore en ajouter :
réimporter juste avant l'activation, l'opération est idempotente.

Avant d'activer, vérifier ce que le moniteur ferait réellement :

```bash
./target/release/workflow-cli webhook probe --plan
```

Avec l'historique importé, la réponse doit être `"planned": 0` (ou uniquement
des archives réellement nouvelles). Avec une base vide, le même webhook planifie
429 téléchargements : c'est précisément ce que `require_history` bloque.

Une fois l'audit à `changes: 0`, activer :

```toml
[webhook]
enabled = true
require_history = true
```

Puis redémarrer le serveur. `--check` affiche le nombre d'URL, l'état du
moniteur et le mode dry-run.

## 5. Comportement reproduit

### Webhook

- deux schémas acceptés : `source_url`/`r2_url` (moderne) et `url` (historique) ;
- `r2_url` prioritaire, `source_url` conservée comme repli ;
- cache 60 s, trois tentatives avec backoff 1 s puis 2 s ;
- timestamps aware convertis en heure locale, timestamps naïfs conservés ;
- statut `{available, last_fetch_ts, error, records}`.

### Normalisation d'URL

Reproduite depuis `CSVService._normalize_url` :

- trim, entités HTML, décodage récursif de `amp%3B` ;
- schéma et hôte en minuscules, ports par défaut supprimés ;
- chemin décodé, slashs doublés supprimés, slash final retiré hors racine ;
- paramètres triés, vides supprimés, doublons exacts supprimés ;
- Dropbox : un seul `dl=1` ;
- fragment supprimé.

`history audit` a confirmé que les 911 URL de production sont déjà normalisées
selon cette implémentation (`changes: 0`).

### Historique et échecs

Schéma identique à Python, WAL, `busy_timeout` 30 s. `upsert` conserve le plus
petit timestamp. Les échecs stockent `kind`, `status_code`, `attempts`,
`last_error`, `first_seen_at`, `last_attempt_at`. Le cooldown vaut
`min(intervalle × 2^(tentatives-1), max)`.

### Téléchargement

- HEAD de sonde, réécriture `dl.dropboxusercontent.com` pour les fichiers ;
- `.part` + `Range`, reprise après coupure ou changement d'URL ;
- `416` supprime le partiel et repart de zéro ;
- un `200` qui ignore `Range` tronque le partiel au lieu de l'ajouter ;
- validation : taille attendue, rejet HTML/JSON/XML, taille minimale des
  dossiers Dropbox, archive ZIP lisible ;
- types d'échec : `not_found`, `auth`, `http_error`, `network`, `stalled`,
  `incomplete`, `invalid_payload`, `local_io`, `internal` ;
- historique écrit uniquement après succès, pour l'URL principale et le repli ;
- échec R2 puis succès Dropbox : message `(R2 absent - repli Dropbox utilisé)`.

### Interface

`GET /api/csv_downloads_status` retourne la liste triée des téléchargements
actifs et des 20 derniers terminés. `GET /api/csv_monitor_status` retourne
l'état du moniteur, les 20 derniers échecs, le compteur d'échecs et le statut
webhook. Le polling frontend existant est inchangé.

## 6. Écarts volontaires

| ID | Écart | Raison |
| --- | --- | --- |
| D-019 | dossier `runtime/downloads` au lieu de `~/Téléchargements` | isolation du dual-run |
| D-020 | concurrence bornée à 4, statut final après repli, `error` remis à null | éviter les threads illimités et un faux échec R2 |
| D-021 | migration explicite, échecs non importés | empêcher un téléchargement massif |

Détail dans `../parity/DRIFT.md`.

## 7. Validation

| Contrôle | Résultat |
| --- | --- |
| Import de la base Python | 911 / 911 URL, timestamps identiques ligne pour ligne |
| `history audit` | 911 URL, 0 changement de normalisation |
| Webhook réel | 462 enregistrements, `available: true` |
| Plan avec 911 URL importées | `webhook probe --plan` : 0 téléchargement planifié |
| Plan avec base vide | 429 téléchargements planifiés : le verrou `require_history` est indispensable |
| Serveur dry-run sur webhook réel | moniteur `active`, 0 nouvelle archive, aucun fichier téléchargé |
| Tests `workflow-ingest` | 42 tests : URL, webhook, historique, cooldown, reprise, ZIP, repli, échecs |
| Tests workspace | 207 tests, `clippy -D warnings`, `fmt --check` |
| Téléchargement Dropbox réel | `m6_.png` (22 047 octets) téléchargé depuis un lien `scl/fi` réel |
| Reprise réelle `Range` | `.part` de 5 000 octets, réponse HTTP 206, `resumed_from=5000`, md5 identique |
| ZIP réel de production | `155 Camille.zip` (783 691 649 octets) via HTTP local, 1,5 s, md5 identique, validation ZIP réussie |
| ZIP corrompu | 2 Mo aléatoires nommés `.zip` : `invalid_payload`, `.part` supprimé, aucun fichier final |

## 8. Limites

- Le téléchargement réel a été validé sur Dropbox et sur un ZIP de production
  servi en HTTP local. Aucune grosse archive Dropbox n'a été téléchargée
  pendant le lot : les objets R2 du webhook avaient expiré et le repli Dropbox
  a été validé sur un petit fichier.
- Le moniteur ne télécharge que les liens Dropbox/R2 ressemblant à des archives
  (`/scl/fo/`, `.zip`, `original_filename` en `.zip`), comme Python. Au
  2026-10-23, les 4 seules URL absentes des 911 de l'historique étaient des
  images et une vidéo `.mov` : le moniteur Rust n'a donc rien téléchargé,
  exactement comme Python.
- Les routes cache, statistiques, diagnostics et audio restent dans Python.
- La validation Windows/After Effects reste externe et non bloquante.

## 9. Activation

```bash
# après import + audit
$EDITOR config/config.toml   # webhook.enabled = true
cargo build --release -p workflow-server -p workflow-cli
./scripts/switch-rust.sh check
./scripts/switch-rust.sh start
curl -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/api/csv_monitor_status
```

Le serveur Python reste sur 5003 pour le rollback. Ne pas laisser les deux
moniteurs actifs sur le même webhook : ils téléchargeraient les mêmes liens
dans deux dossiers différents.

Diagnostic ponctuel avec le même moteur, sans écrire l'historique :

```bash
./target/release/workflow-cli download \
  --url 'https://www.dropbox.com/scl/fi/...' \
  --filename '153 Camille.zip' \
  --output-dir /tmp/verification
```

`download` est strictement un outil de diagnostic : il n'écrit pas
l'historique. Ne pas l'utiliser pour alimenter `runtime/downloads`, sinon STEP1
traiterait le fichier et le moniteur le retéléchargerait faute d'historique.
