# Validation STEP1 → STEP8 sur `1720 Camille`

| Champ | Valeur |
| --- | --- |
| Date | 2026-10-23 |
| Archive | `/mnt/cache/old/1720 Camille.zip` |
| Taille | 1 717 852 006 octets |
| Contenu | 4 MP4 1080p H.264 25 fps, 1 PNG, entrée racine `/` |
| Configuration | `/tmp/opencode/step1-8/config.toml` |
| Sorties | `/tmp/opencode/step1-8` (aucune écriture dans `/mnt/cache`) |
| Résultat | **8/8 étapes réussies**, 4 `.ecproj` produits |

## 1. Pourquoi ce nom

`172 Camille.zip` avait déjà été traité et figure dans
`processed_archives.txt`. Le fichier a été renommé `1720 Camille.zip` pour
forcer un nouveau passage de STEP1 sans modifier l'archive ni vider la liste de
production.

Le test utilise un `processed_archives.txt` isolé sous
`/tmp/opencode/step1-8/runtime/logs/step1`, avec un marqueur mensuel `2026-10`.
L'archive source n'a pas été supprimée (`delete_archive = false`) et son mtime
est resté au `2026-10-22 18:57:12`.

## 2. Isolation

La configuration de test reprend `config/config.toml` et remplace uniquement
les chemins et le filtre :

```toml
[server]
port = 5007

[paths]
base = "/tmp/opencode/step1-8/runtime"
projects = "/tmp/opencode/step1-8/runtime/projets_extraits"
archives = "/tmp/opencode/step1-8/runtime/archives"
logs = "/tmp/opencode/step1-8/runtime/logs"
downloads = "/tmp/opencode/step1-8/runtime/downloads"
history_db = "/tmp/opencode/step1-8/runtime/download_history.sqlite3"
cache_root = "/tmp/opencode/step1-8/cache"
finalized = "/tmp/opencode/step1-8/out"

[steps.filter]
keyword = "1720"

[steps.step1]
source_dir = "/mnt/cache/old"
delete_archive = false

[steps.step7]
delete_source = false
```

Le mot-clé `1720` évite de scanner les autres archives de `/mnt/cache/old`.
`cache_root` et `finalized` sont sous `/tmp` : STEP7 et STEP8 n'écrivent jamais
dans `/mnt/cache`.

## 3. Séquence

Lancée par `POST /run_custom_sequence` sur le serveur de test (port 5007) :

| Étape | Durée | Résultat |
| --- | --- | --- |
| STEP1 | 3 s | 1 archive extraite, 0 problème de sécurité, 4 vidéos déjà conformes (0 réencodage) |
| STEP2 | 43 s | 35 / 45 / 61 / 59 scènes ; 5 158 / 4 673 / 5 022 / 5 272 trames |
| STEP3 | 1 min 40 s | 27 / 27 / 46 / 26 segments ; 4 à 5 locuteurs par vidéo |
| STEP4 | 2 min 58 s | 5 158 / 4 673 / 5 022 / 5 272 trames |
| STEP5 | < 1 s | 1 projet, 4 fichiers réduits |
| STEP6 | < 1 s | 1 projet, 4 `_ae.json` écrits |
| STEP7 | 2 s | 1 projet finalisé dans `/tmp/opencode/step1-8/out` |
| STEP8 | 4 s | 4/4 `.ecproj` produits, `effectcraft-cli 0.16.0`, 0 erreur |

Message final du serveur :

```text
Séquence Custom Terminée avec succès.
Détails: 1. Extraction et normalisation des archives: Réussie (3s) |
2. Analyse des transitions: Réussie (43s) |
3. Analyse audio: Réussie (1m 40s) |
4. Analyse du tracking: Réussie (2m 58s) |
5. Réduction JSON: Réussie (0s) |
6. Pré-traitement AE: Réussie (0s) |
7. Finalisation: Réussie (2s) |
8. Media Solution (EffectCraft): Réussie (4s)
```

## 4. Contrôles d'intégrité

| Contrôle | Résultat |
| --- | --- |
| Archive source | toujours présente, taille et mtime inchangés |
| PNG | md5 `07dd42113e0ba7c1a9c4747fe3275428` identique à l'archive |
| Liste des archives | contient `/mnt/cache/old/1720 Camille.zip`, marqueur `2026-10` |
| STEP3 | 4 `_audio.json` avec `speaker_stats` et 4 à 5 locuteurs |
| STEP4 | nombre de trames identique à `ffprobe` pour les 4 vidéos |
| STEP5/6 | 4 `_audio.json`, 4 `_tracking.json`, 4 `_ae.json` |
| STEP7 | copie finalisée **byte-identique** (`cmp`) aux 25 fichiers du projet |
| STEP8 | 4 fichiers `.ecproj` non vides, `MS_BATCH_RESULT` à `4/4` |
| Écritures `/mnt/cache` | aucune : `finalized` et `cache_root` sous `/tmp` |

Le PNG fait 97 387 octets. Les quatre MP4 étaient déjà en H.264 1080p 25 fps :
STEP1 les a marqués « déjà conformes » et n'a lancé aucun réencodage.

## 5. Limites

- Aucun artefact Python de production n'existe pour `172 Camille` : la
  validation est fonctionnelle et interne (cohérence des comptes, copies
  finalisées, ZIP et `.ecproj`), pas une comparaison sémantique avec Python.
  La parité sémantique STEP5/6 a été validée séparément sur `244` au lot 7.
- Le test a utilisé le mot-clé `1720` pour isoler l'archive. La configuration de
  production garde `keyword = "Camille"`.
- Les sorties de test sont conservées sous `/tmp/opencode/step1-8` (environ
  3,4 Go) pour inspection ; elles peuvent être supprimées sans impact.

## 6. Reproduction

```bash
# 1. Construire la configuration isolée (voir section 2)
# 2. Lancer le serveur de test
WORKFLOW_SERVER_SECRET_KEY=test-secret \
WORKFLOW_SERVER_WORKER_TOKEN=test-token-123 \
RUST_LOG=info \
./target/release/workflow-server --config /tmp/opencode/step1-8/config.toml

# 3. Lancer la séquence
curl -H 'X-Worker-Token: test-token-123' \
  -H 'Content-Type: application/json' \
  -d '{"steps":["STEP1","STEP2","STEP3","STEP4","STEP5","STEP6","STEP7","STEP8"]}' \
  http://127.0.0.1:5007/run_custom_sequence

# 4. Suivre l'avancement
curl -H 'X-Worker-Token: test-token-123' \
  http://127.0.0.1:5007/sequence/status
```

## 7. Synchronisation Python ↔ Rust

La synchronisation au changement a été validée sur l'état réel le 2026-10-23 :

| Contrôle | Résultat |
| --- | --- |
| `download_history` | 911 URL identiques, 0 ajout, 0 mise à jour |
| `processed_archives.txt` | 96 entrées de chaque côté, chemins remappés |
| Marqueur STEP1 | `2026-10` des deux côtés |
| Téléchargements en attente | 0 |
| Analyses STEP7 | 640 hash / 2 473 fichiers / 1,8 Go copiés, 0 conflit |
| Deuxième passage | 0 copie, 1 280 doublons ignorés (640 par sens) |
| Sauvegarde | `runtime/backups/sync/20261023_215036/sync-report.json` |

La copie STEP7 n'a pas modifié les archives Python. Le détail des fusions et
des options est dans `docs/exploitation/SYNC-BACKENDS.md`.

Un aller-retour complet a ensuite été validé le 2026-10-23 à 22:18 : arrêt de
Rust, `sync-backends.sh --apply`, démarrage de Python sur 5003 avec
`DRY_RUN_DOWNLOADS=true` (911 URL, 96 entrées STEP1, 2 473 fichiers STEP7),
arrêt de Python, puis relance du lanceur Rust. La synchronisation automatique
a retrouvé les mêmes comptes côté Rust (911 / 96 / 2 473) et le moniteur a
repris sur 5004. Aucun téléchargement n'a été créé pendant ce test.

Un test isolé de bout en bout (téléchargement HTTP → STEP1 → STEP7 → sync
retour) a également été exécuté dans `/tmp/opencode/e2e-900` ; le détail est
dans `docs/exploitation/SYNC-BACKENDS.md`, section 10. Les corpus 153, 154, 155, 156, 158, 159
et 1720 ont été préparés ; le shadow STEP5/6 donne 162/162 sur 20 projets
(voir `docs/lots/LOT-1-SHADOW.md`).
