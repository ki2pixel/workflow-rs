# Packaging et supervision du client desktop

Ce document couvre le `.deb` Tauri du client `workflow-manager` et les deux
modes de supervision du serveur. Le client ne démarre, n'arrête et ne
surveille jamais le processus serveur : il reste un client HTTP de
`workflow-server`.

## Prérequis de build

- Rust 1.90 ou plus récent pour `workflow-gui-tauri`.
- Tauri CLI 2.12 (via `npx`, aucun paquet global nécessaire).
- Paquets de développement : `libwebkit2gtk-4.1-dev`,
  `libayatana-appindicator3-dev`, `libasound2-dev`, `libssl-dev`,
  `librsvg2-dev`, `libxdo-dev`.
- `dpkg-deb` et `fakeroot` (fournis par `dpkg`).

Le serveur et le CLI doivent être construits avant le paquet :

```sh
cargo build --release -p workflow-server -p workflow-cli
```

## Construire le `.deb`

```sh
cd crates/workflow-gui-tauri/src-tauri
npx --yes @tauri-apps/cli@2.12.4 build --bundles deb --ci
```

Artefact :

```text
target/release/bundle/deb/workflow-manager_0.1.0_amd64.deb
```

Le bundle est configuré dans `tauri.conf.json` :

- `productName = workflow-manager`, `identifier = org.workflow.desktop` ;
- icônes 32, 128, 256 et 512 px ;
- `.desktop` `workflow-manager.desktop`, `StartupWMClass=workflow-gui-tauri` ;
- dépendances Debian : `libasound2`, `libssl3`,
  `libayatana-appindicator3-1`, `libwebkit2gtk-4.1-0`, `libgtk-3-0` ;
- binaire GUI strippé via `[profile.release.package.workflow-gui-tauri]`
  (`strip = "symbols"`), sans impact sur le serveur ni le CLI.

Le paquet contient uniquement :

```text
/usr/bin/workflow-gui-tauri
/usr/share/applications/workflow-manager.desktop
/usr/share/icons/hicolor/{32x32,128x128,256x256,512x512}/apps/workflow-gui-tauri.png
```

Il ne contient ni serveur, ni `config.toml`, ni `launcher.env`, ni secret, ni
chemin `/home/kidpixel` ou `/mnt/cache`.

## Vérifier sans root

L'extraction et l'exécution ne demandent aucun privilège :

```sh
DEB=target/release/bundle/deb/workflow-manager_0.1.0_amd64.deb
rm -rf /tmp/workflow-manager-deb
dpkg-deb -x "$DEB" /tmp/workflow-manager-deb
/tmp/workflow-manager-deb/usr/bin/workflow-gui-tauri --version
desktop-file-validate \
  /tmp/workflow-manager-deb/usr/share/applications/workflow-manager.desktop
dpkg-deb -f "$DEB" Package Version Architecture Depends
```

## Installer et lancer

L'installation système demande root :

```sh
sudo apt install ./workflow-manager_0.1.0_amd64.deb
```

Le client se lance par l'entrée « Workflow Manager » ou par
`workflow-gui-tauri`. Il charge `http://127.0.0.1:5004/` par défaut.

Options utiles :

```sh
workflow-gui-tauri --server-url http://127.0.0.1:5005
workflow-gui-tauri --telemetry-dir ~/.local/state/workflow-manager
workflow-gui-tauri --log-file ~/.local/state/workflow-manager/host.log
workflow-gui-tauri --no-tray --no-close-to-tray
workflow-gui-tauri --no-single-instance
workflow-gui-tauri --notifications
workflow-gui-tauri --audio --sound-dir /chemin/vers/assets/web/sound-design
```

Par défaut, l'état et la télémétrie vont dans
`$XDG_STATE_HOME/workflow-manager` ou `~/.local/state/workflow-manager`.
Les notifications et l'audio restent opt-in. Le paquet n'embarque pas les MP3 ;
`--audio` exige donc un `--sound-dir` valide.

### Single-instance D-Bus

Le premier processus possède `org.workflow.desktop` sur le bus de session et
sert `org.freedesktop.Application` à l'objet `/org/workflow/desktop`. Un second
lancement transmet `argv`, `cwd`, `server-url` et `explicit-server-url` à
l'instance active, puis se termine sans créer de second processus.

- même URL : la fenêtre est restaurée, déminimisée et focalisée ;
- URL explicitement divergente : le second lancement échoue avec
  `org.freedesktop.DBus.Error.InvalidArgs` ;
- serveur HTTP arrêté : l'activation D-Bus précède `AppServices::bootstrap`,
  donc le second lancement fonctionne quand même ;
- `--no-single-instance` désactive le mécanisme pour le diagnostic.

Le `.desktop` reste `workflow-manager.desktop` et ne déclare pas
`DBusActivatable=true` : le nom D-Bus ne correspond pas au nom du fichier.
L'activation D-Bus réelle (lancement par le bus) est donc différée ; le
single-instance au sens « deuxième lancement » est actif.

Sous X11/GNOME, le focus utilise `_NET_ACTIVE_WINDOW` avec l'indication
« pager » (2), après `show()`/`unminimize()`. `present_with_time(0)` de GTK ne
suffit pas quand la fenêtre est minimisée. Le repli Wayland reste
`set_focus()` et n'est pas validé.

## Supervision du serveur

Le serveur n'est jamais un enfant du GUI. Deux modes sont supportés ; ils ne
doivent pas tourner ensemble.

### PoC : `switch-rust.sh`

```sh
set -a
source config/launcher.env
set +a
export WORKFLOW_RUST_CONFIG="$PWD/config/config.toml"
./scripts/switch-rust.sh check
./scripts/switch-rust.sh start
./scripts/switch-rust.sh status
./scripts/switch-rust.sh stop
```

Le script lance le serveur avec `setsid`, écrit un PID file et vérifie
`/api/ping`. Il refuse de démarrer si Python répond sur `:5003` alors que le
webhook Rust est activé.

### MVP : systemd --user

Le modèle d'unité est `packaging/systemd/workflow-server.service.in`. Le script
d'installation génère l'unité, copie les secrets en 0600 et peut démarrer le
service après préflight :

```sh
./scripts/install-systemd-user.sh            # installe sans démarrer
./scripts/install-systemd-user.sh --start    # préflight + démarrage
systemctl --user status workflow-server.service
systemctl --user enable workflow-server.service
journalctl --user -u workflow-server.service -f
./scripts/install-systemd-user.sh --uninstall
```

Options : `--config`, `--env-file`, `--env-dest`, `--unit-name`, `--url`.
Le script refuse un fichier de secrets non 0600, un port déjà occupé et un
démarrage si Python répond sur `:5003` avec le webhook activé. Il ne démarre
jamais le serveur sans `--start`.

L'unité utilise :

- `EnvironmentFile` 0600 pour les secrets, jamais le GUI ;
- `WorkingDirectory` et `ExecStart` vers le dépôt et le binaire release ;
- `Restart=on-failure`, `KillMode=mixed`, `TimeoutStopSec=60` ;
- `ProtectSystem=full`, `NoNewPrivileges=true` ;
- `PrivateTmp=false`, car `/tmp/workflow-rs-hw.lock` est partagé avec le
  stack Python ;
- `ProtectHome` désactivé, car CUDA, EffectCraft et les caches utilisateur
  peuvent écrire sous `$HOME` ;
- `CacheDirectory=workflow-rs/cuda-compute`,
  `CUDA_CACHE_PATH=%C/workflow-rs/cuda-compute` et
  `CUDA_CACHE_MAXSIZE=1073741824` (1 Gio). `%C` vaut `$XDG_CACHE_HOME` ou
  `~/.cache` ; le cache JIT PTX→SASS est partagé par STEP2/STEP3/STEP4 et
  persiste entre les redémarrages. Il n'existe pas de `NVVM_CACHE_PATH` : le
  cache NVVM est le cache CUDA.

Le GUI peut être fermé ou tué : le service reste actif et le client se
resynchronise au prochain lancement. Le cache CUDA est créé et conservé par
systemd ; il n'est jamais supprimé par `--uninstall`.

La procédure de bascule, de smoke webhook désactivé, d'activation du moniteur
réel et de rollback est détaillée dans `docs/SWITCH-SYSTEMD.md`.

## Désinstallation

```sh
sudo apt remove workflow-manager
./scripts/install-systemd-user.sh --uninstall
```

La désinstallation du `.deb` ne touche ni `runtime/`, ni `/mnt/cache`, ni les
secrets systemd. Le script `--uninstall` retire l'unité mais conserve le
fichier de secrets pour éviter une suppression accidentelle.

## Limites connues

- Le `.deb` ne contient que le client ; le serveur, les modèles et les
  dépendances CUDA/TPU restent dans le dépôt.
- Le paquet est construit pour `amd64` sur Ubuntu 22.04 ; il n'est pas
  portable tel quel.
- L'audio hôte est opt-in et n'embarque pas les MP3.
- `DBusActivatable=true` n'est pas activé : le `.desktop` devrait être renommé
  `org.workflow.desktop.desktop` et un service D-Bus ajouté. Le
  single-instance « deuxième lancement » est validé, pas l'activation par le
  bus.
- Le focus single-instance est validé sous X11/GNOME ; Wayland n'est pas
  validé.
- Le run STEP2 sous l'unité de test n'a pas peuplé le cache JIT (SASS
  précompilé pour `sm_75`). Une sonde PTX séparée a validé le chemin et la
  persistance ; un GPU/driver différent peut peupler le cache pendant STEP2.
- Le soak 2 h de confirmation et le run STEP8 sous l'unité ont été validés le
  2026-10-26 (7 200 échantillons, delta `WebKitNetworkProcess` 0 Kio,
  `.ecproj` 1080×1920). Voir
  `docs/audits/2026-10-26-mvp-j2-soak-step8-production.md`.
- `effectcraft-cli` est installé en 0.17.0 ; la documentation historique
  mentionne 0.13. Le run STEP8 J2 a été validé avec 0.17.0.
- Le manifeste STEP7 de production peut référencer un projet absent de
  `/mnt/cache` ; STEP8 échouera alors jusqu'à un STEP7 courant.
