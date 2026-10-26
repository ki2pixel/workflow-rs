# MVP J2 — soak 2 h, STEP8 systemd et bascule production

Date : 2026-10-26
Branche : `staging`
Périmètre : clôture du MVP J2 avant bascule publique.
Statut : **accepté**.

## Décisions ratifiées

1. **Sécurité P1** : les secrets HTTP du serveur ne doivent jamais être
   hérités par `workflow-cli`, `effectcraft-cli`, `ffmpeg` ou leurs enfants.
   Le correctif `env_remove` est inclus dans ce jalon.
2. **Push `main`** : option A, sur le SHA final consolidé après validation de
   l'intégralité des gates.
3. **Robustesse** : `systemctl --user restart` est le test de reprise retenu ;
   `Linger=yes` est actif, le test de reboot complet n'est pas rejoué.
4. **Codification** : les runbooks `scripts/poc/soak-mvp.sh` et
   `scripts/poc/verify-step8-systemd.sh` sont versionnés.

## J2.0 — étanchéité des secrets (P1)

### Implémentation

`crates/workflow-server/src/runner.rs` :

- constante `CHILD_SECRET_ENV` avec `WORKFLOW_SERVER_WORKER_TOKEN`,
  `WORKFLOW_SERVER_SECRET_KEY`, `FLASK_SECRET_KEY` et
  `INTERNAL_WORKER_COMMS_TOKEN` ;
- `sanitize_child_environment(&mut Command)` appelle `env_remove` sur ces
  quatre clés avant `spawn()` ;
- le reste de l'environnement est conservé, notamment `CUDA_CACHE_PATH`,
  `CUDA_CACHE_MAXSIZE` et `CACHE_DIRECTORY`.

Test unitaire
`runner::tests::child_environment_excludes_server_secrets_but_keeps_cache_vars` :
les quatre secrets sont absents (`value = None`) et `CUDA_CACHE_PATH` est
présent avec sa valeur. Le test passe dans la suite workspace.

### Preuve sous systemd

Le wrapper STEP8 consigne la présence des variables, jamais leur valeur :

```text
CUDA_CACHE_PATH=/home/kidpixel/.cache/workflow-rs/cuda-compute
CUDA_CACHE_MAXSIZE=1073741824
CACHE_DIRECTORY=/home/kidpixel/.cache/workflow-rs/cuda-compute
WORKFLOW_SERVER_WORKER_TOKEN=<unset>
WORKFLOW_SERVER_SECRET_KEY=<unset>
FLASK_SECRET_KEY=<unset>
INTERNAL_WORKER_COMMS_TOKEN=<unset>
```

Le serveur lui-même garde les secrets dans son `EnvironmentFile` 0600 ; seuls
les sous-processus d'étape sont nettoyés. Les runbooks versionnés assertent
les quatre secrets et la stabilité D-Bus ; les artefacts J2.1/J2.2 confirment
ces assertions.

## J2.1 — STEP8 sous unité systemd isolée

Runbook : `scripts/poc/verify-step8-systemd.sh`.

Instance : `/tmp/opencode/mvp-j2/step8-20261026-014517`.
Unité de test : `workflow-server-step8-test.service`, port `5007`, config et
chemins sous `/tmp/opencode`, webhook désactivé, verrou matériel isolé.

### Unité et permissions

```text
Environment=CUDA_CACHE_PATH=/home/kidpixel/.cache/workflow-rs/cuda-compute CUDA_CACHE_MAXSIZE=1073741824
PrivateTmp=no
ProtectHome=no
ProtectSystem=full
NoNewPrivileges=yes
CacheDirectory=workflow-rs/cuda-compute
```

`systemd-analyze verify` : rc 0 (avertissements hors unité : netplan,
commandcode-proxy, snapd).

Sonde de permissions avec les mêmes propriétés :

```text
usr_readonly=OK
tmp_writable=OK
home_writable=OK
```

### Exécution STEP8

- `POST /run/STEP8` → HTTP 202.
- Statut final : `completed`.
- Rapport `step8_run_20261026_014519.json` :

```json
{
  "success": true,
  "mode": "headless",
  "bridge_port": null,
  "app_launched": false,
  "timed_out": false,
  "duration_s": 0.21,
  "result": {
    "ok": true,
    "total": 1,
    "processed": 1,
    "errors": []
  }
}
```

- Projet : `projets/short_2997_9x16.ecproj`, 26 726 octets, `kidpixel:kidpixel`,
  mode 664.
- `effectcraft-cli 0.17.0 --project … info` : comp `short_2997_9x16`
  1080×1920 à 29,97002997002997 fps, 3,003 s, `Null Segments` et
  `Segment 1`.
- `MS_BATCH_RESULT: {"ok":true,...}` présent dans `logs/step_STEP8.log`.
- Aucun fichier appartenant à `root` dans la fixture.
- Aucune écriture dans `/mnt/cache` ni dans `runtime/` pendant le run.
- Aucun processus `workflow-cli` ou `effectcraft-cli` résiduel.
- Unité de test arrêtée et retirée ; aucune unité `workflow*` ne reste
  installée après le run.

## J2.2 — soak 2 h

Runbook : `scripts/poc/soak-mvp.sh`.
Instance : `/tmp/opencode/mvp-j2/soak-2h-20261026`.
Serveur `:5005`, GUI Tauri A-Strict, fenêtre minimisée, aucun projet, aucun
run. `systemd-inhibit --what=idle:sleep` actif.

### Tableau d'acceptation

Méthode : 7 200 échantillons à 1 Hz ; deltas = médiane des 600 premiers
échantillons contre médiane des 600 derniers ; pente = régression linéaire sur
tous les échantillons.

| Critère | Valeur | Seuil | OK |
|---|---:|---:|---:|
| Échantillons | 7 200 | 7 200 ± 5 | oui |
| Durée mesurée | 7 199,131 s | ~7 200 s | oui |
| Dérive p95 | 0,237 ms | ≤ 50 ms | oui |
| Dérive max | 1,201 ms | ≤ 250 ms | oui |
| Fenêtre cachée | 1,000 | ≥ 0,99 | oui |
| GUI vivant | 1,000 | ≥ 0,999 | oui |
| PSS complet | 1,000 | ≥ 0,999 | oui |
| Delta RSS hôte | +28 Kio (+0,027 Mio) | ≤ +12 Mio | oui |
| Pente RSS hôte | -0,186 Kio/min | ≤ 0,08 Mio/min | oui |
| Delta RSS GUI | +1 716 Kio (+1,676 Mio) | ≤ +32 Mio | oui |
| Pente RSS GUI | 12,183 Kio/min (0,0119 Mio/min) | ≤ 0,267 Mio/min | oui |
| Delta PSS GUI | +1 701 Kio (+1,661 Mio) | ≤ +32 Mio | oui |
| **Delta RSS WebKitNetwork** | **0 Kio** | **≤ 0 Kio (strict MVP)** | **oui** |
| Delta RSS WebKitWeb | +1 692 Kio (+1,652 Mio) | ≤ +32 Mio | oui |
| Delta RSS serveur | 0 Kio | ≤ +12 Mio | oui |
| Delta MemAvailable | -118 336 Kio (-115,56 Mio) | ≥ -256 Mio | oui |
| Delta swap | 0 Kio | ≤ +16 Mio | oui |
| Logs hôte | 458 octets | ≤ 16 Mio | oui |
| Logs serveur | 897 octets | ≤ 16 Mio | oui |
| CSV endurance | 1 924 268 octets | ≤ 8 Mio | oui |
| CSV VRAM | 1 582 535 octets | — | — |
| Changements tray | 0 | ≤ 20 | oui |
| État final tray | Idle | Idle | oui |
| Pic VRAM | 222 MiB | < 3 900 MiB | oui |
| VRAM libre min | 3 678 MiB | > 196 MiB | oui |
| OOM | non | non | oui |
| Serveur joignable | oui (0 échec) | oui | oui |
| Watcher | 7 202 ticks, 0 erreur | 0 erreur | oui |
| D-Bus tray stable | oui | oui | oui |
| Capture finale | 1440×900, stddev 0,103978 | non blanche | oui |
| `network_pids` présents | 1,000 | ≥ 0,99 | oui |
| Un seul PID hôte | 1,000 | oui | oui |
| Membres session GUI | 3 (hôte + 2 WebKit), stable | stable | oui |

Le gate réseau strict est donc satisfait : **la dérive de
`WebKitNetworkProcess` est nulle sur 2 h**, alors que le même soak J4 mesurait
+22,06 Mio avec l'ancien polling. La croissance résiduelle est portée par le
processus WebKitWeb (+1,65 Mio) et reste très en dessous des plafonds.

### Nettoyage

- Ports `5003`–`5007` libres.
- Aucun processus `workflow-server`, `workflow-gui-tauri`, `workflow-cli`,
  `effectcraft-cli` ou WebKit résiduel.
- Verrou matériel non tenu.
- GNOME inchangé (`dynamic-workspaces=true`, `num-workspaces=4`).
- Token absent des artefacts.
- Aucune écriture dans `/mnt/cache` ; `runtime/` n'a reçu aucun fichier
  pendant le soak (seul le mtime du répertoire a changé).

## J2.3 — bascule de production

### Préflight et synchronisation

- Aucun processus Python/Rust/GUI, ports `5003`/`5004` libres.
- `sync-backends.sh --quiet` (simulation) : historique 911 → 911, STEP1
  97 → 97, 0 téléchargement, 1 008 doublons STEP7, 0 conflit.
- `sync-backends.sh --apply --quiet` : mêmes chiffres, appliqué.
- Sauvegarde : `runtime/backups/sync/20261026_035127/`
  (`python-download_history.sqlite3`, `rust-download_history.sqlite3`,
  listes STEP1 Python/Rust, `sync-report.json`).
- `history audit` après synchronisation : 911 URL, 0 changement.
- `webhook probe --plan` : 462 enregistrements, 0 planifié, 0 échec.

### Unité et secrets

- Unité installée : `~/.config/systemd/user/workflow-server.service`.
- `ExecStart=…/workflow-server --config /home/kidpixel/workflow-rs/config/config.toml`.
- `EnvironmentFile=/home/kidpixel/.config/workflow-rs/launcher.env`.
- Répertoire des secrets : `0700 kidpixel:kidpixel`.
- Fichier des secrets : `0600 kidpixel:kidpixel`, 239 octets, SHA-256
  identique à `config/launcher.env`.
- Unité : `0644`.
- `CacheDirectory=workflow-rs/cuda-compute`, `CUDA_CACHE_PATH=%C/...`,
  `CUDA_CACHE_MAXSIZE=1073741824`, `ProtectSystem=full`,
  `NoNewPrivileges=true`, `PrivateTmp=false`, `Restart=on-failure`.

### Smoke webhook désactivé

Une copie temporaire de la configuration avec `[webhook] enabled = false` a
d'abord été installée et démarrée :

```text
ActiveState=active, SubState=running
/api/ping → {"status":"pong","worker_type":"ubuntu_new","runtime":"rust"}
csv_monitor.status=stopped, webhook.available=false
```

Le port `5004` n'écoutait que sur `127.0.0.1`, un seul processus serveur, aucun
processus Python. L'unité a ensuite été arrêtée et remplacée par la
configuration de production.

### Activation du webhook réel

Configuration de production : `webhook.enabled = true`, `dry_run = false`,
intervalle 15 s.

```text
ActiveState=active, SubState=running
MainPID=175480 (après restart ; 175295 au premier démarrage)
processus workflow-server = 1
127.0.0.1:5004 (loopback uniquement), 5003 libre
/api/ping → pong / runtime=rust
csv_monitor.status=running
csv_monitor.error=null
webhook.available=true
webhook.records=462
webhook.last_fetch_ts=2026-10-26 03:52:37
webhook.error=null
failed_urls_count=0
webhook probe --plan → history=911, records=462, planned=0
history audit → 911, changes=0
```

- Unité `enabled`, `Linger=yes`.
- `systemctl --user restart` : ancien PID 175295, nouveau PID 175480, état
  `active (running)`, un seul processus, port toujours en loopback.
- Journal de l'unité : aucune ligne `error`, `warn`, `panic`, `refus` ou
  `failed` depuis le démarrage.
- Aucun processus Python legacy ; `:5003` libre.
- Aucune référence à la configuration de smoke dans l'unité finale.

## Validation finale

```text
cargo fmt --all --check                                  rc 0
cargo clippy --workspace --all-targets -- -D warnings    rc 0
cargo test --workspace                                   rc 0, 326 tests
cargo build --release -p workflow-server -p workflow-cli rc 0
```

Binaires release (inchangés après rebuild) :

```text
e5d2d1cf55e3e411aa5b6c318166efbcf804db6bd93f24035e65cd230a1eebf8  target/release/workflow-server
f69a3db6fe12b75999f8c1dfc16ac349ae4b235b9676bbb2da4c7b1efcb45c6e  target/release/workflow-cli
955cc0035e42e46be75a10eb682b59227748b2e8aa93f44e91d8fd5fb7fdd8f8  target/release/workflow-gui-tauri
```

## Limites et réserves

- `effectcraft-cli` installé en **0.17.0** ; la documentation historique
  mentionne 0.13. Le run STEP8 de ce jalon utilise 0.17.0 et le rapport le
  consigne.
- Le manifeste STEP7 de production
  `runtime/logs/step7/finalized_projects_20261024_160353.json` référence
  `/mnt/cache/1531 Camille`, absent. Le bouton STEP8 de production échouera
  tant qu'un STEP7 courant n'aura pas produit un manifeste valide. Le run
  J2.1 utilise une fixture isolée et ne prouve pas ce cas de production.
- Le test de reboot complet n'est pas rejoué ; `Linger=yes` et
  `systemctl --user restart` sont validés.
- Le soak mesure le client Tauri A-Strict avec le frontend legacy copié dans
  `assets/web` ; il ne valide pas les routes Python-only ni le reverse proxy.
- Le moniteur webhook réel est actif ; les nouveaux enregistrements éventuels
  seront téléchargés selon la politique d'intervalle et de cooldown. Aucun
  plan n'était en attente au moment de la bascule.

## Artefacts

- Soak : `/tmp/opencode/mvp-j2/soak-2h-20261026/`
  (`summary.json`, `soak/monitor/endurance.csv`, `soak/monitor/vram.csv`,
  captures, logs serveur/GUI).
- STEP8 : `/tmp/opencode/mvp-j2/step8-20261026-014517/`
  (`state/logs/step8/step8_run_20261026_014519.json`, `.ecproj`,
  `child-env.txt`, `unit-show.txt`, `perm-probe.txt`).
- Production : `/tmp/opencode/mvp-j2/production/`
  (`sync-apply.txt`, `sync-dryrun.txt`, `webhook-plan-*.json`,
  `install-smoke.log`, `install-production.log`, `enable-unit.log`).
- Sauvegarde de synchronisation :
  `runtime/backups/sync/20261026_035127/`.
- Runbooks : `scripts/poc/soak-mvp.sh`,
  `scripts/poc/verify-step8-systemd.sh`.
