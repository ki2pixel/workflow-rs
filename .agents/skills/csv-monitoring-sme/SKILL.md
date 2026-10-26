---
name: csv-monitoring-sme
description: Exploiter l'ingestion Rust (webhook, normalisation d'URL, historique SQLite, cooldown, téléchargements R2/Dropbox).
---

# CSV Monitoring SME — ingestion Rust

> Le legacy Python et le Rust partagent le même webhook, mais **un seul
> moniteur doit tourner**. Depuis le 2026-10-23, le moniteur Rust est celui qui
> est activé dans la configuration (`webhook.enabled = true`) ; Python doit
> rester arrêté tant que le serveur Rust tourne.

## Composants

- `workflow-ingest` : client webhook, normalisation d'URL, historique SQLite,
  échecs/cooldown, téléchargement R2/Dropbox reprenable.
- `workflow-server` : moniteur intégré, routes `/api/csv_monitor_status` et
  `/api/csv_downloads_status`.
- `workflow-cli` : `history`, `webhook`, `download`.
- `workflow-sync` : fusion de l'historique Python ↔ Rust.

## Schéma SQLite

```sql
CREATE TABLE download_history (
    url TEXT PRIMARY KEY,
    timestamp TEXT NOT NULL DEFAULT ''
);

CREATE TABLE download_failures (
    url             TEXT PRIMARY KEY,
    kind            TEXT NOT NULL DEFAULT '',
    status_code     INTEGER,
    attempts        INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT NOT NULL DEFAULT '',
    first_seen_at   TEXT NOT NULL DEFAULT '',
    last_attempt_at TEXT NOT NULL DEFAULT ''
);
```

- `download_history` conserve le plus petit timestamp vu.
- `download_failures` porte le cooldown exponentiel.
- Les échecs ne sont pas fusionnés par la sync.

## Préparation

1. Lire `config/config.toml`, sections `[webhook]`, `[download]`,
   `[steps.step1]`.
2. Vérifier `webhook.enabled`, `webhook.dry_run`, `webhook.url`,
   `webhook.require_history`.
3. Vérifier `paths.downloads` et `steps.step1.source_dir`.
4. Vérifier la base `paths.history_db`.
5. Vérifier qu'un seul backend écoute (`:5003` ou `:5004`).

## Commandes

```sh
# Importer l'historique Python (lecture seule)
workflow-cli --config config/config.toml history import \
  --from /home/kidpixel/workflow_mediapipe/download_history.sqlite3

# Vérifier la normalisation
workflow-cli --config config/config.toml history audit

# Statistiques
workflow-cli --config config/config.toml history stats

# Webhook
workflow-cli --config config/config.toml webhook probe --json
workflow-cli --config config/config.toml webhook probe --plan

# Téléchargement isolé, sans historique
workflow-cli --config config/config.toml download \
  --url 'https://...' --output-dir /tmp/opencode/verification
```

Routes serveur :

- `GET /api/csv_monitor_status`
- `GET /api/csv_downloads_status`

## Checklists

- [ ] `webhook.enabled = true` sur une seule stack.
- [ ] `webhook.dry_run = false` en production.
- [ ] `webhook.require_history = true`.
- [ ] `download_history` non vide.
- [ ] `steps.step1.source_dir == paths.downloads`.
- [ ] `download.max_concurrent` borné (4 par défaut).
- [ ] `download.validate_zip = true`.
- [ ] `download.min_zip_bytes = 1000000`.
- [ ] Les échecs sont visibles dans `download_failures`.
- [ ] La sync a été faite avant la bascule.

## Incidents

| Symptôme | Piste |
| --- | --- |
| Téléchargements en boucle | Historique vide, URL non normalisée, cooldown |
| Moniteur refuse de démarrer | `require_history` et base vide |
| STEP1 ne voit pas les archives | `source_dir` différent de `downloads` |
| Doublons d'URL | Vérifier `normalize_url` et la clé primaire |
| Échec R2 puis Dropbox | Le statut final ne doit être publié qu'après le repli |
| `error` persiste après succès | Vérifier la remise à null |
| Base corrompue | `PRAGMA integrity_check`, restaurer un backup |
| Deux moniteurs actifs | Arrêter Python ou Rust immédiatement |
| Téléchargement incomplet | Vérifier `Range`, `.part`, la taille et le ZIP |

## Sécurité

- Ne jamais lancer `webhook` avec `dry_run = false` sur un environnement de
  test sans base isolée.
- Ne pas importer les échecs Python : ils sont transitoires et liés à des URL
  expirées.
- Ne pas modifier la base Python.
- Ne pas partager `runtime/downloads` avec Python sans décision explicite
  (D-019).

## Références

- `docs/pipeline/LOT-8-INGESTION.md`
- `docs/ops/SYNC-BACKENDS.md`
- `parity/DRIFT.md` (D-018, D-019, D-020, D-021, D-023)
- `crates/workflow-ingest/src/`
- `resources/sqlite_triage_commands.md`
