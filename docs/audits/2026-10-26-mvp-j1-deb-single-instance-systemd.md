# MVP J1 — `.deb` réel, single-instance D-Bus et cache CUDA systemd

Date : 2026-10-26
Branche : `staging`
Périmètre : premier jalon MVP du client desktop Tauri A-Strict.

## Décisions ratifiées

1. Identité D-Bus `org.workflow.desktop`, objet `/org/workflow/desktop`,
   interface `org.freedesktop.Application`, `DBusActivatable=false`.
2. Second lancement : focus strict et rejet immédiat (`InvalidArgs`) de toute
   URL serveur explicitement divergente.
3. Cache CUDA/NVVM : `%C/workflow-rs/cuda-compute`, plafond 1 Gio
   (`1073741824` octets).
4. Cycle du paquet : désinstallation du `.deb` de référence J5 à la fin de la
   phase 1, puis reconstruction/installation du paquet MVP.

## Phase 1 — installation réelle du `.deb`

### Paquet de référence J5

- Fichier : `target/release/bundle/deb/workflow-manager_0.1.0_amd64.deb`
- Taille : 5 856 248 octets (5,59 Mio)
- SHA-256 : `51d4592707971396d0b8abcf0d5a5a804429abcb6b6c1ddb610e881ad148b4e9`
- `Installed-Size` : 15 227 Kio
- Installation : `sudo apt install -y ./workflow-manager_0.1.0_amd64.deb`
  (rc 0), puis désinstallation `sudo apt remove -y workflow-manager` (rc 0).

### Paquet MVP final

- Taille : 6 595 548 octets (6,29 Mio)
- SHA-256 : `66da7031601e66591a27c9fa03bbb9cee1ae9f46a5022d3381dc5640201439f8`
- `Installed-Size` : 17 159 Kio
- Dépendances : `libasound2`, `libssl3`, `libayatana-appindicator3-1`,
  `libwebkit2gtk-4.1-0`, `libgtk-3-0`
- Installation : `sudo apt install -y ./workflow-manager_0.1.0_amd64.deb`
  (mise à jour du paquet installé, rc 0).
- Contenu : binaire `/usr/bin/workflow-gui-tauri`, `.desktop`
  `/usr/share/applications/workflow-manager.desktop`, icônes 32/128/256/512.
  Aucun serveur, config, secret ni chemin de production.

### Intégration GNOME / GIO

`desktop-file-validate` :

```text
desktop_validate_rc=0
```

`xdg-mime query filetype /usr/share/applications/workflow-manager.desktop` :

```text
application/x-desktop
```

`Gio.DesktopAppInfo.new("workflow-manager.desktop")` :

```text
name       : Workflow Manager
visible    : True
icon       : workflow-gui-tauri
executable : workflow-gui-tauri
commandline: workflow-gui-tauri
GIO_APPINFO_OK
```

Icônes installées :

```text
/usr/share/icons/hicolor/32x32/apps/workflow-gui-tauri.png     1016 octets
/usr/share/icons/hicolor/128x128/apps/workflow-gui-tauri.png   3563 octets
/usr/share/icons/hicolor/256x256/apps/workflow-gui-tauri.png   7710 octets
/usr/share/icons/hicolor/512x512/apps/workflow-gui-tauri.png  14611 octets
```

L'entrée « Workflow Manager » est visible dans le menu GNOME avec l'icône
applicative (capture
`/tmp/opencode/mvp-j1-final/gnome-menu.png`, non versionnée).

### Lancement par le menu

Le client a été lancé par `gtk-launch workflow-manager` avec un serveur isolé
sur `:5004` (webhook désactivé, chemins sous `/tmp/opencode/mvp-j1-final`).
Résultat :

```text
page=http://127.0.0.1:5004/
process=workflow-gui-tauri
window=Workflow Manager (WM_CLASS workflow-gui-tauri)
name owner=org.workflow.desktop
```

Le serveur n'est jamais un enfant du GUI ; il a été démarré séparément avec
`setsid` et son PID est distinct.

## Phase 2 — single-instance D-Bus

### Implémentation

- `crates/workflow-gui-tauri/src-tauri/src/single_instance.rs` :
  - nom `org.workflow.desktop`, objet `/org/workflow/desktop` ;
  - interface `org.freedesktop.Application` avec `Activate`, `Open`,
    `ActivateAction` ;
  - revendication `DoNotQueue` avant `AppServices::bootstrap` ;
  - payload `a{sv}` : `argv`, `cwd`, `server-url`, `explicit-server-url` ;
  - limites : 64 arguments, 4 096 octets par argument, 32 Kio au total ;
  - URL divergente explicitement fournie → `org.freedesktop.DBus.Error.InvalidArgs` ;
  - option `--no-single-instance` pour le diagnostic.
- `crates/workflow-gui-tauri/src-tauri/src/x11_activation.rs` :
  - envoi de `_NET_ACTIVE_WINDOW` avec l'indication « pager » (2), comme
    `wmctrl -a` ;
  - `send_event(...).check()` + `sync()` pour garantir le traitement avant la
    fermeture de la connexion X11.
- `host.rs` : `show()`, `unminimize()`, puis activation X11 différée de
  500 ms. Le délai évite que `present_with_time(0)` de GTK ne replace la
  fenêtre en `_NET_WM_STATE_DEMANDS_ATTENTION` après l'activation.

### Preuves D-Bus

Propriétaire du nom :

```text
gdbus call --session --dest org.freedesktop.DBus \
  --object-path /org/freedesktop/DBus \
  --method org.freedesktop.DBus.GetNameOwner org.workflow.desktop
(':1.584',)
```

Introspection (1 091 octets) :

```text
node /org/workflow/desktop {
  interface org.freedesktop.Application {
    methods:
      Activate(in  a{sv} platform_data);
      Open(in  as _uris,
           in  a{sv} platform_data);
      ActivateAction(in  s _action_name,
                     in  av _parameter,
                     in  a{sv} platform_data);
    signals:
    properties:
  };
  interface org.freedesktop.DBus.Introspectable { ... };
  interface org.freedesktop.DBus.Peer { ... };
  interface org.freedesktop.DBus.Properties { ... };
};
```

Trace `dbus-monitor` du second lancement :

```text
method call ... destination=org.workflow.desktop
  path=/org/workflow/desktop
  interface=org.freedesktop.Application
  member=Activate
  array [
    dict entry(string "cwd", variant string "/home/kidpixel/workflow-rs")
    dict entry(string "server-url", variant string "http://127.0.0.1:5004/")
    dict entry(string "explicit-server-url", variant boolean true)
    dict entry(string "argv", variant array [ ... ])
  ]
```

### Résultats du second lancement

Avec le binaire installé `/usr/bin/workflow-gui-tauri` et un serveur isolé sur
`:5004` :

```text
second_rc=0
elapsed_ms=53
gui_processes=1
hidden_after_second=none
focused_state_after_second=_NET_WM_STATE_FOCUSED
map_state_after_second=IsViewable
focus_after_second=85983235
dbus_activate_calls=1
```

La fenêtre minimisée est restaurée et focalisée ; aucun second processus GUI
n'est créé.

URL divergente :

```text
divergent_rc=1
org.freedesktop.DBus.Error.InvalidArgs:
  URL divergente: instance active sur http://127.0.0.1:5004/,
  second lancement sur http://127.0.0.1:5999/
```

Serveur arrêté :

```text
offline_second_rc=0
activations_before=1
activations_after=2
```

L'activation D-Bus précède le bootstrap HTTP : un second lancement fonctionne
même si le serveur est indisponible. À l'arrêt de l'instance primaire, le nom
est libéré (`GetNameOwner` → `NameHasNoOwner`).

## Phase 3 — unité systemd et cache CUDA/NVVM

### Directives ajoutées

`packaging/systemd/workflow-server.service.in` :

```ini
CacheDirectory=workflow-rs/cuda-compute
Environment=CUDA_CACHE_PATH=%C/workflow-rs/cuda-compute
Environment=CUDA_CACHE_MAXSIZE=1073741824
```

`%C` vaut `$XDG_CACHE_HOME` ou `~/.cache`. Sur la machine de validation :
`/home/kidpixel/.cache/workflow-rs/cuda-compute`. Il n'existe pas de
`NVVM_CACHE_PATH` : le cache NVVM est le cache JIT CUDA.

### Unité de test

Une unité temporaire `workflow-server-cache-test.service` a été installée avec
`scripts/install-systemd-user.sh`, une configuration isolée sous
`/tmp/opencode/mvp-j1-cache` (webhook désactivé, port `:5006`) et un wrapper
`workflow-cli` qui journalise l'environnement du sous-processus. L'unité de
production `workflow-server.service` n'a pas été démarrée : la configuration
réelle active le webhook.

`systemctl --user show` :

```text
Environment=CUDA_CACHE_PATH=/home/kidpixel/.cache/workflow-rs/cuda-compute CUDA_CACHE_MAXSIZE=1073741824
CacheDirectory=workflow-rs/cuda-compute
ProtectSystem=full
NoNewPrivileges=yes
PrivateTmp=no
ProtectHome=no
```

`/proc/<pid>/environ` du serveur :

```text
CACHE_DIRECTORY=/home/kidpixel/.cache/workflow-rs/cuda-compute
CUDA_CACHE_MAXSIZE=1073741824
CUDA_CACHE_PATH=/home/kidpixel/.cache/workflow-rs/cuda-compute
```

Environnement du `workflow-cli` enfant (wrapper) :

```text
cwd=/home/kidpixel/workflow-rs
argv=--config /tmp/opencode/mvp-j1-cache/config.toml step2 detect --input-dir ...
CUDA_CACHE_PATH=/home/kidpixel/.cache/workflow-rs/cuda-compute
CUDA_CACHE_MAXSIZE=1073741824
CACHE_DIRECTORY=/home/kidpixel/.cache/workflow-rs/cuda-compute
XDG_CACHE_HOME=<unset>
HOME=/home/kidpixel
```

`systemd-analyze verify` : rc 0 (avertissements hors unité : netplan,
`commandcode-proxy`, snapd).

### Run STEP2 sous l'unité

Entrée : `parity/cases/short_2997.mp4` copié dans
`/tmp/opencode/mvp-j1-cache/state/projets_extraits/`.

Run 1 :

```text
POST /run/STEP2 -> 202
status=completed
return_code=0
duration_str=7s
progress_text=short_2997.csv créé avec 1 scènes (75 trames, 0.59 s)
CSV_IDENTIQUE=oui
```

Le CSV produit est identique octet pour octet à
`parity/fixtures/short_2997_step2/expected/short_2997.csv`.

Après redémarrage de l'unité, run 2 (CSV supprimé au préalable) :

```text
status=completed
return_code=0
duration_str=2s
CSV_RUN2_IDENTIQUE=oui
```

### Métriques de cache

| Étape | Taille `du -sb` | Fichiers | Preuve |
| --- | ---: | ---: | --- |
| Avant tests | 4 096 octets | 0 | répertoire créé par systemd |
| STEP2 run 1 | 4 096 octets | 0 | aucun JIT PTX nécessaire |
| Sonde PTX systemd | 15 794 octets | 2 | `index` 32 o + `d/2/0dae57bf937f11` 3 474 o |
| Redémarrage unité | 15 794 octets | 2 | hashes et mtimes identiques |
| Sonde PTX 2 | 15 794 octets | 2 | hashes et mtimes identiques (cache chaud) |
| STEP2 run 2 | 15 794 octets | 2 | CSV identique, aucun nouveau fichier |

La sonde PTX est un petit binaire `nvcc -arch=compute_75 -code=compute_75`
(PTX seul) exécuté sous `systemd-run --user` avec les mêmes propriétés
`CacheDirectory`/`CUDA_CACHE_PATH`. Elle prouve que le chemin est honoré par le
pilote et persiste après redémarrage. Le run STEP2 lui-même n'a pas peuplé le
cache : les bibliothèques CUDA/ONNX Runtime fournissent du SASS pour `sm_75`,
donc aucun JIT PTX n'a été déclenché. C'est un résultat attendu, pas un échec
du chemin de cache.

## Validation globale

```text
cargo fmt --check                                      rc 0
cargo clippy --workspace --all-targets -- -D warnings  rc 0
cargo test --workspace                                 325 tests, 0 échec
cargo build --release -p workflow-server -p workflow-cli -p workflow-gui-tauri  rc 0
npx @tauri-apps/cli@2.12.4 build --bundles deb --ci    rc 0
scripts/poc/verify-single-instance.sh (binaire installé) RESULT=OK
```

Répartition des tests : 281 pipeline + 38 services + 6 GUI = 325.

## Risques résiduels

- `DBusActivatable=false` : le nom D-Bus `org.workflow.desktop` ne correspond
  pas au fichier `workflow-manager.desktop`. Une activation D-Bus réelle
  exigerait de renommer le `.desktop` et d'ajouter un service D-Bus ; différé.
- Le focus single-instance est validé sous X11/GNOME. Le repli Wayland utilise
  `set_focus()` de GTK et n'est pas validé.
- L'unité de production n'a pas été démarrée : le webhook réel est actif dans
  `config/config.toml`. Le modèle est à jour ; l'installation réelle se fera
  lors de la bascule.
- Le soak 2 h de confirmation et le run STEP8 sous l'unité restent à rejouer
  avant la bascule publique.
- Le cache CUDA n'est peuplé par STEP2 que si le pilote doit compiler du PTX ;
  la sonde PTX couvre ce cas séparément.

## Reproductibilité

```sh
# Paquet
cd crates/workflow-gui-tauri/src-tauri
npx --yes @tauri-apps/cli@2.12.4 build --bundles deb --ci

# Single-instance (serveur isolé sur :5004 déjà démarré)
scripts/poc/verify-single-instance.sh \
  --gui /usr/bin/workflow-gui-tauri \
  --server-url http://127.0.0.1:5004 \
  --server-pid-file /tmp/opencode/mvp-j1-final/server.pid \
  --stop-server

# Unité de test avec cache
./scripts/install-systemd-user.sh \
  --config /tmp/opencode/mvp-j1-cache/config.toml \
  --env-file /tmp/opencode/mvp-j1-cache/launcher.env \
  --env-dest /tmp/opencode/mvp-j1-cache/launcher.env \
  --unit-name workflow-server-cache-test \
  --url http://127.0.0.1:5006 --start
```
