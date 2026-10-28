# Décision D-028 — Abandon du client desktop Tauri

- **Statut** : acceptée
- **Date** : 2026-10-26
- **Portée** : interface, packaging, supervision systemd, dépendances Cargo
- **Voir aussi** : `memory-bank/decisionLog.md` (D-028), `README.md`, `AGENTS.md`

## TL;DR

Le client desktop Tauri (PoC J1 à J5, MVP J1/J2) est abandonné. La production
revient à la stack Rust headless (`workflow-server` sur `:5004`,
`workflow-cli`) avec l'interface web legacy `assets/web`, servie par le
serveur. Les crates `workflow-desktop-services` et `workflow-gui-tauri`, le
paquet `workflow-manager`, les scripts de soak et la documentation desktop sont
retirés. Le dépôt est ramené à `a93a554` ; la branche
`backup/tauri-abandon-20261026` conserve l'état précédent.

## Contexte

Le PoC Tauri A-Strict devait fournir une fenêtre native, un tray D-Bus, un
single-instance et un polling 1 Hz du serveur headless. Il a été conduit en
cinq jalons, puis prolongé par un MVP (`.deb`, single-instance, cache CUDA,
soak 2 h, unité systemd utilisateur). Les audits J1 à J5 et MVP J1/J2
documentaient des résultats conformes aux critères du PoC.

L'usage réel a néanmoins montré une instabilité du client et un coût de
maintenance disproportionné par rapport au besoin : l'interface web legacy
couvre déjà la timeline, les logs et le pilotage des huit étapes. Le client
desktop ajoutait une seconde chaîne graphique et un mode de supervision
supplémentaire, sans contrat métier nouveau.

## Décision

1. Abandonner le client desktop Tauri et son socle
   `workflow-desktop-services`.
2. Revenir à l'architecture headless : `workflow-server` sur `:5004`,
   `workflow-cli` pour les étapes, interface `assets/web`.
3. Retirer le paquet `workflow-manager`, l'unité systemd
   `workflow-server.service` et les scripts PoC du dépôt.
4. Conserver la porte de sortie : la branche
   `backup/tauri-abandon-20261026` contient l'état complet avant rollback.

## Motifs

- **Instabilité constatée à l'usage** : les soaks ne couvraient pas les
  combinaisons réelles de tray, minimisation, focus X11 et changements de
  page ; le client a été jugé moins fiable que la page web legacy.
- **Complexité WebKitGTK** : le soak J4 mesurait encore une croissance de
  `WebKitNetworkProcess` (+22,1 Mio) ; la correction par suspension du polling
  a ajouté des règles de cache et de cycle de vie propres à la WebView.
- **Surface fonctionnelle inutile** : tray D-Bus, single-instance, focus X11,
  `.deb`, systemd utilisateur et cache CUDA représentaient beaucoup de pièces
  mobiles pour un client qui ne fait qu'afficher la page servie par Rust.
- **Coût de toolchain** : Tauri 2, GTK3, WebKitGTK 4.1, zbus, x11rb,
  notify-rust et sysinfo imposaient une seconde chaîne de build et une MSRV
  1.90, contre 1.86 pour le pipeline.
- **Duplication de l'interface** : `assets/web` est déjà la copie de référence
  du frontend legacy ; le client desktop n'apportait pas de valeur métier
  équivalente à son coût de maintenance.
- **Surface de sécurité et de packaging** : D-Bus, `.deb` installé sur l'hôte,
  unité systemd utilisateur et secrets dédiés sortaient du périmètre du
  pipeline.

## Périmètre retiré

| Élément | Détail |
| --- | --- |
| Crates | `crates/workflow-desktop-services`, `crates/workflow-gui-tauri` |
| Cargo | membres du workspace, dépendances Tauri/GTK/WebKit/zbus/x11rb/notify-rust/sysinfo, profil release GUI |
| Paquet système | `workflow-manager 0.1.0`, binaire `/usr/bin/workflow-gui-tauri`, `.desktop`, icônes 32/128/256/512 |
| Supervision | `workflow-server.service`, `packaging/systemd/workflow-server.service.in`, `scripts/install-systemd-user.sh` |
| Scripts | `scripts/poc/*` (audits, stress, soak, runbooks, single-instance) |
| Documentation | `docs/desktop/*`, études GUI et audits Tauri des 2026-10-25/26 |
| Résidus utilisateur | `~/.local/share/org.workflow.desktop`, `~/.local/state/workflow-manager` |
| Build | `target/` (3,8 Gio), `.deb` Tauri |

## Ce qui est conservé

- STEP1 à STEP8, `workflow-cli`, `workflow-server` et `assets/web`.
- Les contrats JSON/CSV/HTTP et la tolérance de parité `1e-6`.
- `runtime/`, `/mnt/cache`, le dépôt Python legacy en lecture seule et les
  secrets opérationnels (`config/launcher.env`,
  `~/.config/workflow-rs/launcher.env`).
- Le cache CUDA `~/.cache/workflow-rs/cuda-compute`, qui n'est pas un artefact
  du client desktop.
- Les lanceurs pré-Tauri `~/Bureau/Legacy_Workflow.desktop` et
  `~/Bureau/Rust_Workflow.desktop`.

## Validation du rollback

- `git reset --hard a93a554c4c5ed5e17805575cf15ac533d7cd50a3`.
- `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.
- `cargo test --workspace` : 280 tests, 0 échec au rollback strict, puis
  282 après réapplication des correctifs indépendants.
- `cargo run -p workflow-cli -- config check` : configuration valide.
- `cargo build --release -p workflow-server -p workflow-cli` : binaires
  reconstruits sans dépendance desktop.
- Serveur relancé seul sur `:5004` via `scripts/switch-rust.sh` ; Python
  `:5003` arrêté.

## Correctifs indépendants réappliqués

Trois correctifs indépendants du client desktop ont été réappliqués après le
rollback strict :

- `514e2d6` : écriture des exécutables factices hors du processus de test,
  contre le flake `ETXTBSY` ;
- `4ddb91b` : retrait des secrets du serveur de l'environnement des étapes
  (281 tests) ;
- `713b6bc` : suspension du polling web quand la page est masquée, avec
  `Cache-Control: no-store` sur `/static/` (282 tests).

La réorganisation documentaire (`687eaf9`, `1613b17`) reste dans la branche de
sauvegarde et n'est pas réappliquée. Le script PoC de validation du polling
n'est pas réintroduit.

## Restauration

Pour revenir à l'état Tauri complet :

```sh
git reset --hard backup/tauri-abandon-20261026
```

Le paquet installé et l'unité systemd ont été supprimés ; il faudra
reconstruire le `.deb` et réinstaller l'unité depuis les sources restaurées
pour retrouver l'état d'exploitation précédent.

## Références

- `memory-bank/decisionLog.md` — décision D-028.
- `memory-bank/activeContext.md`, `memory-bank/progress.md`.
- `README.md` — interface et lanceurs.
- `docs/exploitation/SWITCH-PUBLIC.md` — bascule et rollback Python.
- `docs/lots/LOT-5-ORCHESTRATOR.md` — serveur et contrat HTTP.
- `parity/DRIFT.md` — écarts legacy.
