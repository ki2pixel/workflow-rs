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
| Tests `workflow-ingest` | 42 tests à la clôture du lot : URL, webhook, historique, cooldown, reprise, ZIP, repli, échecs |
| Tests workspace | 207 tests à la clôture du lot, `clippy -D warnings`, `fmt --check` |
| Téléchargement Dropbox réel | `m6_.png` (22 047 octets) téléchargé depuis un lien `scl/fi` réel |
| Reprise réelle `Range` | `.part` de 5 000 octets, réponse HTTP 206, `resumed_from=5000`, md5 identique |
| ZIP réel de production | `155 Camille.zip` (783 691 649 octets) via HTTP local, 1,5 s, md5 identique, validation ZIP réussie |
| ZIP corrompu | 2 Mo aléatoires nommés `.zip` : `invalid_payload`, `.part` supprimé, aucun fichier final |
| Dernier lien webhook (R2) | `276 Camille.zip` : 183 450 047 octets, 175 Mo, 29 s, ZIP valide |
| Dernier lien webhook (Dropbox) | même archive via `source_url`, 33 s, ZIP valide |
| Moniteur réel bout en bout | base isolée sans les deux URL de `276` : 1 téléchargement planifié, R2 réussi, 183 450 047 octets, ZIP valide, historique réécrit à 911 URL, 0 échec |
| Repli R2 → Dropbox bout en bout | webhook local avec `r2_url` en 404 et `source_url` réel : repli Dropbox, 175 Mo, message `(R2 absent - repli Dropbox utilisé)`, aucun échec persistant |
| Chaîne complète STEP1 → STEP8 | `1720 Camille.zip` : 8/8 étapes, 4 `.ecproj`, aucune écriture `/mnt/cache` (voir `docs/validation/VALIDATION-STEP1-8.md`) |

### 7.1 Chaîne complète STEP1 → STEP8

Le test `1720 Camille` couvre l'enchaînement complet, y compris STEP1 :
1 archive extraite, 4 vidéos déjà conformes, 35/45/61/59 scènes,
27/27/46/26 segments audio, 5 158/4 673/5 022/5 272 trames de tracking,
4 `_ae.json`, 1 projet finalisé et 4 `.ecproj`. Détail, isolation et contrôles
d'intégrité dans `docs/validation/VALIDATION-STEP1-8.md`.

### 7.2 Simuler un nouveau lien sans attendre le webhook

Un lien déjà présent dans l'historique est ignoré, même s'il est toujours
valide. Pour rejouer le moniteur sur un lien existant, sans toucher à la base
de production :

1. copier `runtime/download_history.sqlite3` dans un runtime isolé ;
2. supprimer de la copie les deux URL de l'enregistrement choisi
   (`source_url` normalisée et `r2_url`) ;
3. configurer `paths.downloads` et `steps.step1.source_dir` vers un dossier
   isolé, puis `webhook.enabled = true`, `require_history = true`,
   `dry_run = false`, `max_concurrent = 1` ;
4. démarrer le serveur sur un port séparé. Le moniteur planifie exactement
   l'enregistrement supprimé ;
5. vérifier `/api/csv_downloads_status`, le fichier téléchargé et le retour de
   l'historique à son nombre initial d'URL.

Pour exercer le repli, servir un JSON local avec un `r2_url` qui répond 404 et
le vrai `source_url` Dropbox, puis pointer `webhook.url` vers ce fichier. Le
test du 2026-10-23 a produit `277_Camille.zip` (183 450 047 octets) avec le
message `(R2 absent - repli Dropbox utilisé)`. La base et le dossier
`runtime/downloads` de production n'ont pas été modifiés.

## 8. Limites

- Le téléchargement réel a été validé sur Dropbox (`m6_.png`), sur un ZIP de
  production local (`155 Camille.zip`, 747 Mo) et sur le dernier lien du
  webhook (`276 Camille.zip`, 175 Mo) via R2 puis via Dropbox. Les liens R2
  restent éphémères : un objet expiré doit basculer sur `source_url`.
- Le moniteur ne télécharge que les liens Dropbox/R2 ressemblant à des archives
  (`/scl/fo/`, `.zip`, `original_filename` en `.zip`), comme Python. Au
  2026-10-23, les 4 seules URL absentes des 911 de l'historique étaient des
  images et une vidéo `.mov` : le moniteur Rust n'a donc rien téléchargé,
  exactement comme Python.
- Les routes cache, statistiques, diagnostics et audio restent dans Python.
- La validation Windows/After Effects et `workflow-bridge.exe` est abandonnée
  (D-022) : la production cible Linux et le JSX/CEP continue d'appeler Python.

## 9. Activation

```bash
# juste avant l'activation : fusionner l'historique et la liste STEP1.
# L'opération est idempotente, sauvegarde les deux bases et ne modifie pas
# les archives.
./scripts/sync-backends.sh --apply --downloads skip --step7 skip
./target/release/workflow-cli history audit
./target/release/workflow-cli webhook probe --plan

$EDITOR config/config.toml   # webhook.enabled = true
cargo build --release -p workflow-server -p workflow-cli
./scripts/switch-rust.sh check
./scripts/switch-rust.sh start
curl -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/api/csv_monitor_status
```

Le serveur Python reste sur 5003 pour le rollback, mais il ne doit pas être
démarré tant que le moniteur Rust est actif : `start_workflow.sh` lance
toujours son propre moniteur CSV. La bascule contrôlée passe par
`scripts/sync-backends.sh --apply` (voir `docs/exploitation/SYNC-BACKENDS.md`) :

- Python → Rust : arrêter Python, synchroniser, démarrer Rust ;
- Rust → Python : arrêter Rust, synchroniser, démarrer Python.

Le lanceur Rust synchronise automatiquement avant de démarrer et refuse de
démarrer si Python répond encore sur 5003.

### 9.1 État de l'instance au 2026-10-23

Le moniteur Rust est actif sur cette machine depuis le 2026-10-23 21:13 :

- import de 911 URL, audit à 0 changement, plan à 0 téléchargement sur 462
  enregistrements webhook ;
- `webhook.enabled = true`, `require_history = true`, `dry_run = false` ;
- synchronisation réelle à 21:50 : 911 URL, 96 entrées STEP1 et 640 hash
  STEP7 (1,8 Go) de chaque côté. Sauvegarde et rapport dans
  `runtime/backups/sync/20261023_215036/` ;
- Python 5003 n'est pas lancé ; son moniteur ne tourne pas. Il peut être
  relancé après `scripts/sync-backends.sh --apply` et arrêt de Rust ;
- les nouvelles archives iront dans `runtime/downloads` et seront traitées par
  STEP1 Rust. Une synchronisation `both` avec `--downloads copy` (défaut)
  copie les archives non traitées dans les deux dossiers ; `move` les déplace
  dans le dossier du backend cible.

La procédure de bascule publique par reverse proxy est décrite dans
`docs/exploitation/SWITCH-PUBLIC.md`.

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
