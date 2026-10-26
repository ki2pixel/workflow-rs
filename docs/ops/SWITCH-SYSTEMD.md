# Bascule production — unité systemd `workflow-server`

| Champ | Valeur |
| --- | --- |
| Type | Runbook |
| Statut | Actif — production depuis 2026-10-26 |
| Date | 2026-10-26 |
| Portée | Unité systemd `workflow-server.service`, secrets, webhook réel et rollback. |
| Voir aussi | `docs/ops/SWITCH-PUBLIC.md`, `docs/ops/SYNC-BACKENDS.md`, `docs/desktop/PACKAGING-GUI.md` |
| Objet | Exploiter `workflow-server` comme unité systemd --user de production |
| Backend | Rust `127.0.0.1:5004`, webhook réel actif |
| Rollback | Python `127.0.0.1:5003`, après arrêt et synchronisation |
| Prérequis | `Linger=yes`, secrets 0600, répertoire 0700, aucun serveur actif |

Cette procédure remplace le lancement par `scripts/switch-rust.sh` pour la
production. Les deux modes ne doivent **jamais** tourner ensemble : une seule
unité, un seul moniteur webhook, un seul processus serveur.

## 1. État attendu

```bash
systemctl --user is-active workflow-server.service
systemctl --user show workflow-server.service \
  -p MainPID -p ActiveState -p SubState -p ExecMainStartTimestamp
loginctl show-user "$USER" -p Linger
```

Attendu :

```text
active
ActiveState=active
SubState=running
Linger=yes
```

Un seul processus :

```bash
pgrep -cx workflow-server          # 1
ss -ltnp | grep -E ':5003|:5004'  # 127.0.0.1:5004 uniquement
```

## 2. Secrets et permissions

L'unité lit `~/.config/workflow-rs/launcher.env` via `EnvironmentFile`.
Le fichier source est `config/launcher.env` (gitignoré).

```bash
stat -c '%a %U:%G %n' \
  "$HOME/.config/workflow-rs" \
  "$HOME/.config/workflow-rs/launcher.env" \
  "$HOME/.config/systemd/user/workflow-server.service"
```

Attendu :

```text
700 kidpixel:kidpixel /home/kidpixel/.config/workflow-rs
600 kidpixel:kidpixel /home/kidpixel/.config/workflow-rs/launcher.env
644 kidpixel:kidpixel /home/kidpixel/.config/systemd/user/workflow-server.service
```

Le répertoire doit être explicitement passé à `0700` ; `chmod 700` seul peut
conserver le bit setgid sur certains systèmes. Utiliser si nécessaire :

```bash
chmod g-s "$HOME/.config/workflow-rs"
chmod 700 "$HOME/.config/workflow-rs"
```

Les secrets ne sont jamais écrits dans `config/config.toml`, dans le GUI ni
dans les artefacts de test.

## 3. Installation ou mise à jour de l'unité

```bash
cd /home/kidpixel/workflow-rs
./scripts/install-systemd-user.sh \
  --config config/config.toml \
  --env-file config/launcher.env \
  --start
systemctl --user enable workflow-server.service
```

Le script :

- refuse un fichier de secrets non 0600 ;
- refuse un port déjà occupé ;
- refuse de démarrer si `webhook.enabled = true` et que Python répond sur
  `:5003` ;
- copie les secrets en 0600 et génère l'unité depuis
  `packaging/systemd/workflow-server.service.in`.

Ne pas lancer `scripts/switch-rust.sh start` en parallèle.

## 4. Smoke avant activation du webhook

Pour tester l'unité sans lancer le moniteur réel, utiliser une copie de la
configuration avec `[webhook] enabled = false` :

```bash
python3 - config/config.toml /tmp/config-smoke.toml <<'PY'
import re
from pathlib import Path
import sys

source = Path(sys.argv[1]).read_text(encoding="utf-8")
match = re.search(r"(?ms)^\[webhook\]\n(.*?)(?=^\[|\Z)", source)
body, count = re.subn(r"(?m)^enabled = .*$", "enabled = false", match.group(1), count=1)
if count != 1:
    raise SystemExit("clé enabled absente")
Path(sys.argv[2]).write_text(
    source[:match.start(1)] + body + source[match.end(1):],
    encoding="utf-8",
)
PY

./scripts/install-systemd-user.sh \
  --config /tmp/config-smoke.toml \
  --env-file config/launcher.env \
  --start
```

Vérifier :

```bash
set -a; source "$HOME/.config/workflow-rs/launcher.env"; set +a
curl -fs -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/api/ping
curl -fs -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/api/csv_monitor_status | jq '.csv_monitor'
```

Attendu : `status=stopped`, `error=null`, aucun processus Python.

Puis arrêter l'unité et réinstaller la configuration de production :

```bash
systemctl --user stop workflow-server.service
./scripts/install-systemd-user.sh \
  --config config/config.toml \
  --env-file config/launcher.env \
  --start
```

## 5. Vérification du webhook réel

```bash
set -a; source "$HOME/.config/workflow-rs/launcher.env"; set +a
curl -fs -H "X-Worker-Token: $WORKFLOW_SERVER_WORKER_TOKEN" \
  http://127.0.0.1:5004/api/csv_monitor_status \
  | jq '{csv_monitor,webhook,data_source,monitor_interval}'

./target/release/workflow-cli --config config/config.toml \
  webhook probe --plan | jq '{history,records,planned}'
./target/release/workflow-cli --config config/config.toml \
  history audit | jq '{total,changes}'
```

Attendu au démarrage :

```text
csv_monitor.status=running
csv_monitor.error=null
webhook.available=true
webhook.error=null
planned=0
changes=0
```

Un `planned` non nul après quelques minutes peut correspondre à de nouvelles
archives publiées : vérifier l'historique et les téléchargements avant de
conclure à une anomalie.

## 6. Redémarrage et reprise

```bash
systemctl --user restart workflow-server.service
sleep 3
systemctl --user is-active workflow-server.service
systemctl --user show workflow-server.service -p MainPID -p ActiveState -p SubState
```

Vérifier ensuite `/api/ping`, `csv_monitor.status=running` et
`planned=0`. `Restart=on-failure` couvre les arrêts anormaux ; un redémarrage
manuel ne doit pas laisser de processus orphelin.

## 7. Journaux et diagnostic

```bash
systemctl --user status workflow-server.service --no-pager
journalctl --user -u workflow-server.service -f
journalctl --user -u workflow-server.service --since today --no-pager \
  | grep -iE 'error|warn|panic|failed' || true
```

Le serveur écrit aussi dans `runtime/logs/` pour les étapes. Le cache CUDA est
créé par systemd :

```bash
stat -c '%a %U:%G %n' "$HOME/.cache/workflow-rs/cuda-compute"
```

## 8. Rollback vers Python

1. Arrêter les séquences et l'unité Rust :

```bash
systemctl --user stop workflow-server.service
```

2. Vérifier que `:5004` est libre et que Python n'écoute pas encore.
3. Synchroniser avant de démarrer Python :

```bash
cd /home/kidpixel/workflow-rs
./scripts/sync-backends.sh --apply
```

4. Désactiver le webhook Rust dans `config/config.toml`
   (`[webhook] enabled = false`) pour qu'un futur démarrage Rust ne reprenne
   pas le moniteur.
5. Démarrer Python sur `:5003` avec `FLASK_HOST=127.0.0.1`.
6. Vérifier `/api/ping` et `:5003` en loopback.

Ne jamais lancer Python avant la synchronisation : son moniteur démarrerait
aussi et deux moniteurs écriraient l'historique.

## 9. Checklist de bascule

- [ ] Les deux backends sont arrêtés.
- [ ] `sync-backends.sh` (simulation) puis `--apply` exécutés.
- [ ] `history audit` à 0 changement.
- [ ] `webhook probe --plan` à 0 (ou nouvelles archives justifiées).
- [ ] Secrets 0600, répertoire 0700, unité 0644.
- [ ] Smoke webhook désactivé : `csv_monitor.status=stopped`.
- [ ] Activation : `csv_monitor.status=running`, `webhook.available=true`.
- [ ] Un seul processus, `127.0.0.1:5004`, `:5003` libre.
- [ ] `systemctl --user enable` et `Linger=yes`.
- [ ] `systemctl --user restart` puis re-vérification.
- [ ] Aucun `error`/`panic` dans le journal.
