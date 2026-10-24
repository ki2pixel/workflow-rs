# Ingestion — requêtes SQLite et vérifications

> Utiliser `sqlite3 runtime/download_history.sqlite3` ou une copie en lecture
> seule. Ne jamais modifier la base Python.

## 1. Dernières URL importées

```sql
SELECT url, timestamp
FROM download_history
ORDER BY timestamp DESC
LIMIT 20;
```

## 2. Doublons de normalisation

```sql
SELECT url, COUNT(*) AS c
FROM download_history
GROUP BY url
HAVING c > 1;
```

- La clé primaire `url` empêche les doublons exacts ; un doublon sémantique
  indique une normalisation incomplète.

## 3. Échecs actifs et cooldown

```sql
SELECT url, kind, status_code, attempts, last_error,
       first_seen_at, last_attempt_at
FROM download_failures
ORDER BY last_attempt_at DESC
LIMIT 20;
```

## 4. Échecs par type

```sql
SELECT kind, COUNT(*) AS c
FROM download_failures
GROUP BY kind
ORDER BY c DESC;
```

## 5. Statistiques globales

```sql
SELECT
  (SELECT COUNT(*) FROM download_history) AS urls,
  (SELECT COUNT(*) FROM download_failures) AS failures;
```

## 6. Intégrité

```sql
PRAGMA integrity_check;
PRAGMA journal_mode;
```

- Attendu : `ok` et `wal`.

## 7. Vérifier une URL précise

```sql
SELECT url, timestamp
FROM download_history
WHERE url LIKE '%<motif>%';
```

## 8. Script shell de synthèse

```sh
DB=runtime/download_history.sqlite3
sqlite3 "$DB" "PRAGMA integrity_check;"
sqlite3 "$DB" "SELECT COUNT(*) FROM download_history;"
sqlite3 "$DB" "SELECT COUNT(*) FROM download_failures;"
sqlite3 "$DB" "SELECT kind, COUNT(*) FROM download_failures GROUP BY kind;"
```

## 9. Comparaison avec le legacy

```sh
sqlite3 /home/kidpixel/workflow_mediapipe/download_history.sqlite3 \
  "SELECT COUNT(*) FROM download_history;"
```

- La base Python est en lecture seule.
- Utiliser `workflow-cli history import` pour fusionner, pas une copie manuelle.
- Ne pas importer `download_failures` : les échecs Python ne sont pas fusionnés.

## 10. Signaux d'alerte

- `download_history` vide avec `webhook.enabled = true` : le serveur doit
  refuser de démarrer.
- `steps.step1.source_dir` différent de `paths.downloads` : STEP1 ne verra pas
  les archives.
- Deux backend actifs : arrêter immédiatement l'un des deux.
- Croissance anormale de `download_failures` : vérifier les URL R2 expirées, le
  cooldown et le repli Dropbox.
