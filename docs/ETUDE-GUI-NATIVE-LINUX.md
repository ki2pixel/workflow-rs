# Étude de faisabilité — interface desktop Linux native

| Champ | Valeur |
| --- | --- |
| Version | 1.1 |
| Date | 2026-10-24 |
| Statut | Proposition soumise à décision ; aucun code fonctionnel modifié |
| Portée | Remplacement de l'interface web `assets/web` par une application desktop Linux (X11/Wayland) |
| Références | `AGENTS.md`, `docs/CONTRAT-ENVIRONNEMENT.md`, `memory-bank/activeContext.md`, `memory-bank/decisionLog.md` |
| Confrontation | `docs/audits/2026-10-24-confrontation-etudes-gui-linux.md` (avec `docs/audits/Migration-Architecture-GUI-Linux-workflow-rs.md`) |
| Décision consignée | `memory-bank/decisionLog.md`, entrée « Interface desktop Linux » du 2026-10-24 |

> **Révision 1.1 (2026-10-24)** — Corrections issues de la confrontation avec
> l'étude indépendante : `GtkFileLauncher` remplacé par `gio::AppInfo`
> (GTK 4.6), empreinte GTK4 révisée, ajout de `GSK_RENDERER=cairo`, de NVML,
> de SSE et du modèle `LogEntry`. Le §12 récapitule les changements ; la
> recommandation consolidée dépend toujours de l'arbitrage « widgets natifs ».

## 1. Résumé exécutif

**Verdict : faisable, sans toucher au pipeline ni au contrat HTTP.**

La recommandation est une **application desktop Tauri v2 en modèle client du
serveur** (`:5004`), c'est-à-dire une coque native qui charge l'interface
existante et délègue toute l'orchestration à `workflow-server`. Les fonctions
de bureau (icône de zone de notification, notifications système, ouverture de
dossiers, instance unique, fermeture vers la zone de notification) vivent côté
Rust, pas dans la page. Le serveur reste headless, le moniteur webhook continue
après fermeture de la fenêtre, et le rollback navigateur/Python reste intact.

- **Effort** : 5 à 8 jours pour un PoC mesurable, 12 à 18 jours pour un MVP
  packagé. À comparer à 70–95 jours pour une réécriture GTK4/Relm4.
- **Idéal natif** : GTK4 + Relm4 reste la meilleure cible si l'exigence
  « widgets natifs, accessibilité AT-SPI, tableaux de logs virtualisés » devient
  prioritaire. C'est un lot séparé, pas un prérequis.
- **Écartés pour cette interface** : Iced (accessibilité toujours absente,
  widgets personnalisés à écrire) et egui (esthétique et accessibilité
  insuffisantes pour une UI opérateur). Slint est techniquement crédible mais
  impose une réécriture complète, un MSRV 1.92 et un choix de licence.
- **Modèle monolithique rejeté à ce stade** : une fenêtre GUI n'est pas un
  démon. Fermer la fenêtre arrêterait le webhook, les séquences et l'ingestion,
  et un crash graphique emporterait l'orchestrateur.

Trois corrections au cadrage initial, mesurées sur le dépôt :

1. **Les logs ne sont pas « des milliers de lignes par seconde ».** Le serveur
   borne chaque étape à 300 lignes (`MAX_STEP_LOG_LINES`) et la production
   mesurée plafonne à **5,4 lignes/s** (STEP4). Le problème réel est le
   **re-rendu complet** de ces 300 lignes deux fois par seconde, pas le débit.
   Il se corrige aussi bien en JavaScript qu'en natif.
2. **La séquence personnalisée n'utilise pas `POST /run_custom_sequence`.** Le
   frontend boucle côté navigateur (`sequenceManager.js`) ; fermer l'onglet
   interrompt la séquence après l'étape en cours. Passer à la séquence serveur
   est une amélioration à ratifier, pas un simple portage.
3. **Le gestionnaire de cache n'existe pas dans le frontend Rust.** Les routes
   `/api/cache/*` sont Python-only (D-015), aucun bouton de cache n'est rendu,
   et les références `clear_disk_cache` sont du code mort. `xdg-open` n'est
   utilisé nulle part aujourd'hui.

**Révision 1.1** : la confrontation avec l'étude indépendante
(`docs/audits/2026-10-24-confrontation-etudes-gui-linux.md`) confirme
l'architecture et déplace la décision technologique sur une exigence à
arbitrer. Tauri reste la voie rapide (12–18 j) ; GTK4/Relm4 devient la cible
si les widgets natifs, l'isolation VRAM (`GSK_RENDERER=cairo`) et une table de
logs virtualisée sont exigés (70–95 j, PoC 8–10 j). Le §12 récapitule les
corrections et la règle de décision.

## 2. Méthode et limites

L'étude est **documentaire et métrologique** : lecture du frontend, du serveur,
du contrat et des skills ; mesures sur la machine de production ; aucune
exécution de prototype GUI. Les versions de frameworks ont été vérifiées sur
leurs dépôts/notes de version au 2026-10-24.

Limites assumées :

- Les estimations de charge sont des **jours-homme** pour un développeur qui
  connaît le dépôt mais pas le toolkit, à ±30 %.
- Le comportement de WebKitGTK avec le pilote NVIDIA propriétaire doit être
  mesuré par le PoC, pas déduit.
- Aucun test n'a été exécuté contre `runtime/` ni `/mnt/cache`.

## 3. Inventaire fonctionnel du frontend actuel

### 3.1 Vue d'ensemble

| # | Fonction | Fichiers principaux | État réel |
| --- | --- | --- | --- |
| 1 | Timeline 8 étapes, statuts, progression | `templates/index_new.html`, `uiUpdater.js`, `steps.css` | Complet, rendu serveur |
| 2 | Lancement / arrêt d'une étape | `eventHandlers.js`, `apiService.js` | Complet |
| 3 | Séquence 1–8 et séquence personnalisée | `sequenceManager.js`, `eventHandlers.js` | Complète mais **pilotée par le client** |
| 4 | Overlay de logs + logs spécifiques | `uiUpdater.js`, `popupManager.js`, `logs.css` | Complet ; coloration, pas de filtre ni copie |
| 5 | Moniteur système CPU/RAM/GPU/disque | `main.js`, `widgets.css` | Complet (GPU via `nvidia-smi`) |
| 6 | Téléchargements / ingestion | `downloadsListManager.js`, `csvDownloadMonitor.js`, `csvWorkflowPrompt.js` | Complet |
| 7 | Gestionnaire de cache | — | **Absent** du frontend Rust (Python-only, D-015) |
| 8 | Design sonore | `soundManager.js`, `sound-design/*.mp3` | Complet, Web Audio |
| 9 | Thèmes, préférences, raccourcis | `themeManager.js`, `AppState.js`, `themes.css` | 6 thèmes, 12 préférences persistées |
| 10 | Accessibilité | template, `popupManager.js`, CSS | Rôles ARIA, focus trap, `aria-live`, reduced motion |

### 3.2 Timeline

- Structure contractuelle : `.pipeline-timeline[role=list]`,
  `.timeline-step[role=listitem]`, `id="step-{{ step_key }}"`,
  `data-step-key`, `data-status`. Les cartes sont générées par la boucle
  Minijinja sur `steps_config` (`index_new.html:104-158`).
- Statuts exposés par le contrat : `idle`, `starting`, `running`, `stopping`,
  `completed`, `failed`, `stopped`, `cancelled`, `initiated`
  (`CONTRAT-ENVIRONNEMENT.md` §2.3). L'UI accepte en plus `success`, `error`,
  `warning`, `paused`, `pending` (`uiUpdater.js:24-38`).
- Progression : `progress_current`, `progress_total`,
  `progress_current_fractional`, `progress_text`. Repli spécifique
  STEP2/3/4 : extraction d'un pourcentage depuis le texte, plafonnement à 99 %
  tant que l'étape tourne (`uiUpdater.js:477-505`).
- Auto-scroll : centrage sur la carte active, topbar de 68 px, délai 150 ms,
  throttle 700 ms pendant une séquence, désactivé quand l'overlay de logs est
  ouvert (`scrollManager.js:10-19`, `uiUpdater.js:596-609`).
- Sélection de séquence : cases à cocher, numéro d'ordre, popup de
  confirmation, bouton de purge.

### 3.3 Contrôles

| Action | Route réellement appelée | Remarque |
| --- | --- | --- |
| Lancer une étape | `POST /run/<step>` | Jeton `X-Worker-Token`, bouton désactivé pendant la requête |
| Annuler une étape | `POST /cancel/<step>` | `/stop/<step>` existe mais n'est pas utilisé par l'UI |
| Lancer 1–8 | boucle cliente `POST /run/<step>` | Pas d'appel à `/run_custom_sequence` |
| Séquence personnalisée | boucle cliente | Idem ; confirmation locale |
| Arrêter une séquence | — | `/sequence/stop` n'est jamais appelé |
| État de séquence | — | `/sequence/status` n'est jamais appelé |

Conséquence opérationnelle : **la durabilité d'une séquence dépend de l'onglet
du navigateur**. Le serveur termine l'étape courante, mais aucune étape
suivante n'est lancée si la page est fermée. Un client desktop qui se contente
de rejouer ce comportement hérite du défaut ; c'est pourquoi la séquence côté
serveur est recommandée (§6.4).

### 3.4 Overlay de logs

- Overlay plein écran `#logs-column-global`, `aria-hidden` piloté par
  `popupManager`, focus trap complet (Tab/Shift+Tab), fermeture par Échap et
  bouton dédié, restauration du focus.
- Log principal : `/status/<step>` (300 dernières lignes, déjà échappées côté
  serveur), réaffiché en totalité à chaque poll (`uiUpdater.js:751-770`).
- Logs spécifiques : `/get_specific_log/<step>/<index>` (150 lignes par défaut,
  100 pour STEP4, 200 pour STEP8, 50 pour la liste STEP1), remplacement du
  panneau principal par le panneau spécifique.
- Coloration : 8 motifs regex (erreur, avertissement, succès, progression,
  commande, info, horodatage, debug) appliqués dans un **Web Worker**
  (`logParserUtils.js`, `logPatterns.js`, `parseWorker.js`).
- **Absents** : filtre par niveau, recherche, bouton « copier tout », export du
  log ; la sélection texte native du navigateur et Ctrl+C restent possibles.
- **Défaut connu** : `scrollTop = scrollHeight` est appliqué sans condition à
  chaque mise à jour ; un opérateur qui remonte dans le log est ramené en bas
  au poll suivant.
- Auto-ouverture : désactivée par défaut dans la copie Rust
  (`ui.autoOpenLogOverlay = false`), toggle Settings et préférence persistée
  respectés.

### 3.5 Moniteur système

- `/api/system_monitor` toutes les 5 s (`main.js:175`, `437-459`).
- CPU et RAM en jauges, GPU optionnel (masqué si `null`, message si `error`),
  détails RAM/VRAM/température, mode compact minimisé.
- Le serveur Rust lit la première carte avec `nvidia-smi` à chaque requête
  (`http.rs:671-700`). Mesure locale : **10–20 ms** par appel, soit < 0,4 % de
  CPU à 5 s d'intervalle.

### 3.6 Ingestion et téléchargements

- `/api/csv_downloads_status` toutes les secondes (`POLLING_INTERVAL * 2`),
  liste triée, pourcentage, message tronqué, notification et son à l'arrivée
  d'un nouveau téléchargement.
- `/api/csv_monitor_status` disponible côté serveur ; le frontend surveille
  surtout les transitions `pending → completed` pour proposer le lancement
  d'un workflow (`csvWorkflowPrompt.js`).
- Bascule d'affichage de la section « Téléchargements Locaux » avec alerte si
  des téléchargements sont en cours et que la section est masquée.

### 3.7 Cache, statistiques, diagnostics

- `assets/web` ne contient **aucun** appel à `/api/cache/*`,
  `/api/stats/*` ni `/api/system/diagnostics`.
- `stats-dashboard.css` n'est même pas lié par le template ; c'est un actif
  orphelin.
- `downloadsListManager.js:79-130` gère un bouton
  `clear-disk-cache-global-btn` qui n'existe pas dans le template et une étape
  `clear_disk_cache` absente du catalogue Rust : code mort.
- Conclusion : si le desktop doit offrir la recherche/ouverture de cache, c'est
  une **fonction nouvelle**, pas une migration. Elle est triviale en local
  (`/mnt/cache` + `xdg-open`) mais doit rester en lecture seule et hors du
  contrat HTTP.

### 3.8 Son, thèmes, préférences

- 8 fichiers MP3 (272 Ko), préchargés, limite de 1 son/s par événement, volume
  maître piloté par la préférence `ui.soundVolume` (1,0 par défaut ; 0,7 en dur
  dans le module avant application de la préférence).
- Événements : démarrage workflow, interaction case à cocher, complétion,
  erreur, bascule AutoMode, succès d'étape, début/fin de téléchargement CSV.
- 6 thèmes CSS (`dark-pro`, `light-mode`, `pastel-zen`, `neon-cyberpunk`,
  `forest-night`, `ocean-depth`), appliqués via `data-theme` sur
  `document.documentElement`.
- 12 préférences persistées dans `localStorage` (`AppState.PERSISTED_PATHS`) :
  mode compact, auto-ouverture des logs, ordre de séquence, visibilité des
  téléchargements, moniteur minimisé, panneau Settings, auto-scroll, auto-scroll
  de séquence, son, volume, thème, alerte téléchargement.
- Notifications : bandeau interne 5 s + API Web Notification
  (`utils.js:34-57`).

### 3.9 Accessibilité existante

- Lien d'évitement, `role="region"`, `role="list"`/`listitem`, `aria-live`
  (`polite`/`assertive`), `role="progressbar"` avec `aria-valuenow`,
  `aria-modal`, `aria-expanded`, `aria-pressed`.
- Focus trap et restauration ; `prefers-reduced-motion` dans 5 feuilles CSS.
- Cette base est un **actif** : un toolkit qui ne l'expose pas (Iced, egui sous
  Linux) régresse l'interface opérateur.

## 4. Mesures de référence

### 4.1 Machine et environnement

| Élément | Valeur mesurée |
| --- | --- |
| OS | Ubuntu 22.04.5 LTS |
| Session | X11, GNOME (`ubuntu:GNOME`), `DISPLAY=:0` |
| CPU / RAM | 24 threads, 62 Gio |
| GPU | NVIDIA GTX 1650, 4 096 Mio, pilote 535.309.01 |
| VRAM déjà consommée | ~933 Mio (Xorg 159, warp-taskbar 465, Firefox 160, GNOME 41, IDE 56, divers 52) |
| Rust | 1.97.0 ; MSRV workspace 1.86 |
| GTK 4 | 4.6.9 (runtime) |
| libadwaita | 1.1.7 (runtime) |
| WebKitGTK 4.1 | 2.50.4 (runtime) |
| Audio | PipeWire + compatibilité Pulse, ALSA 1.2.6.1 |
| Polices emoji | Noto Color Emoji présent |
| Outils bureau | `xdg-open`, `notify-send`, `gio`, `dbus-send`, systemd user, AT-SPI actif |
| Paquets de développement | `libwebkit2gtk-4.1-dev`, `libgtk-4-dev`, `libadwaita-1-dev`, `libxdo-dev`, `libayatana-appindicator3-dev` **absents**, installables depuis les dépôts jammy |
| Réseau crates.io | accessible |

Point important : les **bibliothèques d'exécution** WebKitGTK 4.1, GTK4 et
libadwaita sont déjà présentes ; seuls les en-têtes de développement manquent.
Le coût d'installation est un `apt install` de build, pas une dépendance
runtime nouvelle pour la machine.

### 4.2 Volume de logs

| Fichier | Lignes | Octets | Moyenne | Débit observé |
| --- | ---: | ---: | ---: | ---: |
| `step_STEP1.log` | 43 | 2 618 | 61 o | — |
| `step_STEP2.log` | 87 | 9 067 | 104 o | ~2,0 l/s (43 s) |
| `step_STEP3.log` | 188 | 20 070 | 107 o | ~1,9 l/s (100 s) |
| `step_STEP4.log` | 979 | 115 247 | 118 o | **~5,4 l/s** (179 s) |
| `step_STEP8.log` | 95 | 8 614 | 91 o | — |

Budget réseau du polling :

- 300 lignes × 118 o ≈ **35 Ko** par réponse `/status` ;
- 2 polls/s (500 ms) ≈ **70 Ko/s** par étape active ;
- pire cas théorique : 300 × 4 Kio (limite par ligne) = 1,2 Mo par poll, soit
  2,4 Mo/s ; le canal du serveur est borné à 256 messages et la ligne est
  tronquée à 64 Kio avant parsing.
- Au démarrage : 8 GET `/status` ≈ 280 Ko.

Charge de rendu actuelle : le panneau réécrit **300 lignes HTML complètes deux
fois par seconde** (≈ 600 lignes/s de DOM), sans différentiel ni ancrage de
scroll. C'est le vrai point chaud, et il est indépendant du toolkit.

### 4.3 Cycle de vie actuel du « poste de travail »

- `~/Bureau/Lancer_Workflow_Rust.desktop` → `scripts/start-rust.sh` →
  terminal + Firefox + `tail` du journal.
- `start-rust.sh` installe un `trap INT TERM HUP` qui appelle
  `switch-rust.sh stop` : **fermer le terminal arrête le serveur et le
  webhook**.
- `switch-rust.sh start` lance le serveur avec `setsid` et un fichier PID : le
  serveur survit à la session appelante.
- Un desktop GUI doit choisir explicitement son modèle : coque jetable +
  serveur supervisé (recommandé), ou coque propriétaire du serveur (à éviter).

## 5. Évaluation comparative des frameworks

### 5.0 Référence de coût : navigateur en mode application

`firefox --app=http://127.0.0.1:5004/` (ou `chromium --app=...`) supprime la
barre d'adresse et donne une fenêtre dédiée en **moins d'une heure**. Cela ne
répond toutefois pas aux exigences d'une application native : pas d'icône de
zone de notification, pas d'instance unique, pas de notifications système
fiables, dépendance au profil et aux extensions Firefox, pas de packaging.
C'est le **degré zéro** à battre, pas une option du tableau.

### 5.1 Faits vérifiés (2026-10-24)

| Framework | Version | MSRV | Rendu Linux | Dépendances système | Licence |
| --- | --- | --- | --- | --- | --- |
| Tauri | 2.12 (2026-09-26) | 1.90 | WebKitGTK 4.1 | `libwebkit2gtk-4.1-dev`, GTK3, `libxdo`, AppIndicator, librsvg | MIT/Apache-2.0 |
| Slint | 1.18.1 (2026-09-21) | 1.92 (depuis 1.17) | Femtovg/Skia, software | peu (X11/Wayland, EGL) | GPLv3 ou royalty-free avec attribution |
| Iced | 0.14.0 (2025-12-07) | ~1.88 | WGPU (Vulkan/GL), tiny-skia | Mesa/Vulkan ou GL | MIT |
| GTK4 / Relm4 | GTK 4.6.9 ; gtk4-rs 0.11 ; Relm4 0.11 (2026-05-10) | 1.83 (gtk4-rs) / 1.93 (Relm4) | GTK4 natif | `libgtk-4-dev`, `libadwaita-1-dev` | LGPL-2.1+ (GTK), MIT (Relm4) |
| egui / eframe | 0.37.0 (2026-10-15) | 1.95 | glow/WGPU | Mesa/GL | MIT/Apache-2.0 |

Le workspace déclare **1.86**. Adopter Tauri, Slint, Relm4 ou egui impose donc
une **révision de la politique MSRV** (`workspace.package.rust-version`), même
si la toolchain installée (1.97) compile déjà tout.

### 5.2 Option A — Tauri v2

Trois modes sont possibles, et le choix du mode change tout le reste :

| Mode | Principe | Coût | Verdict |
| --- | --- | --- | --- |
| **T1 — URL distante** | La fenêtre charge `http://127.0.0.1:5004/` | ~0 ligne de frontend | **Retenu pour le PoC** |
| **T2 — assets embarqués** | La page est servie par Tauri, l'API reste HTTP | CORS + CSP + rendu du template | Reporté |
| **T3 — commandes IPC** | `fetch` remplacé par des commandes Tauri | Orchestrateur dans le processus GUI | **Rejeté** (modèle 2) |

Le mode T2 est plus coûteux qu'il n'y paraît : `index_new.html` est un
**gabarit Minijinja** qui injecte `steps_config` et `worker_token` au rendu.
Embarquer les assets oblige soit à récupérer le HTML du serveur (donc T1), soit
à porter `preprocess_template` et `frontend_config` dans le binaire desktop,
avec un risque de dérive. De plus, une page d'origine `tauri://localhost` qui
appelle `http://127.0.0.1:5004` déclenche un préflight CORS à cause de
`X-Worker-Token` ; le serveur n'a aucune couche CORS. Enfin, la CSP du serveur
(`connect-src 'self'`) devrait être élargie.

En T1 :

- aucun changement serveur, aucun changement frontend ;
- même origine → CSP, jeton dans la balise meta, `localStorage` et Web Audio
  fonctionnent comme aujourd'hui ;
- les fonctions de bureau sont dans le Rust : tray, instance unique,
  notifications (le superviseur Rust interroge `/api/step_status/*`, route non
  authentifiée, et émet `notify-rust`), ouverture de dossiers via
  `tauri-plugin-opener`, fermeture vers le tray ;
- la page ne peut pas appeler l'IPC Tauri sans
  `dangerousRemoteDomainIpcAccess` ; on évite cette surface en ne mettant
  **aucune** fonction de bureau dans le JS.

Limites : la fenêtre est un WebView, pas des widgets natifs ; l'API Web
Notification n'existe pas dans WebKitGTK (d'où le superviseur Rust) ; le
`localStorage` du WebView est un profil neuf (les préférences Firefox ne sont
pas reprises) ; WebKitGTK + NVIDIA propriétaire est le point de risque
principal (DMA-BUF).

### 5.3 Option B — Slint

- Avantages : natif, très faible empreinte, `ListView` virtualisée, licence
  royalty-free possible, excellente stabilité X11/Wayland, accessibilité par
  propriétés `accessible-role`/`accessible-label`.
- Coûts : réécriture intégrale des 2 700 lignes de CSS et de la logique de
  présentation ; les 6 thèmes deviennent 6 jeux de tokens ; la timeline, les
  cartes, les popups et le focus trap sont à reconstruire ; `ListView` suppose
  des lignes de hauteur fixe (le wrap des logs demande une fenêtre manuelle) ;
  la sélection/copie de texte dans un `Text` non éditable est à valider.
- MSRV 1.92 et licence à trancher (GPLv3 incompatible avec un binaire
  propriétaire ; royalty-free avec clause d'attribution).
- Verdict : bon candidat pour un outil interne neuf, pas pour un portage à
  parité visuelle.

### 5.4 Option C — Iced

- Avantages : architecture Elm claire, WGPU/X11/Wayland, rendu de qualité,
  bonne intégration Tokio, écosystème de widgets correct.
- Coûts : **accessibilité toujours absente** (issue #552 ouverte depuis 2020,
  travaux System76 en cours) ; pas de liste virtualisée prête à l'emploi, il
  faut calculer la fenêtre visible ; sélection de texte dans les logs limitée ;
  widgets timeline/progress personnalisés à écrire ; API cassante à chaque
  version (0.13 → 0.14 a déplacé des traits).
- Verdict : le plus mauvais rapport risque/parité pour cette UI.

### 5.5 Option D — GTK4 / Relm4

- Avantages : intégration GNOME parfaite (la cible est Ubuntu GNOME) ;
  `GtkListView`/`Gio::ListStore` virtualisés pour les logs ; `GtkTextView`
  donne sélection, copie et recherche ; accessibilité AT-SPI native ;
  `GtkApplication` fournit l'instance unique par D-Bus ; `Gio::Notification`
  pour les notifications ; `gio::AppInfo::launch_default_for_uri` (ou
  `xdg-open`) pour ouvrir un dossier — `GtkFileLauncher` exige GTK 4.10 et
  n'existe pas sur Ubuntu 22.04 ; thèmes via `CssProvider` ;
  **`GSK_RENDERER=cairo` donne un rendu logiciel et 0 Mio de VRAM
  applicative**, ce qui isole totalement STEP2 CUDA sur une carte 4 Gio.
- Coûts : réécriture complète ; GTK 4.6/libadwaita 1.1 sur Ubuntu 22.04 bride
  les widgets récents (Relm4 0.11 doit être utilisé avec la feature
  `gnome_42`, à valider au PoC, sinon gtk4-rs directement) ; programmation par
  `Rc<RefCell<>>` et signaux moins directe que l'Elm ; CSS GTK différent du
  CSS web ; animations et effets visuels à refaire ; MSRV 1.93 si Relm4 0.11.
- Verdict : **la solution native idéale**, à condition d'accepter 3 à 4 mois de
  réécriture et de perdre la parité pixel.

### 5.6 Option E — egui / eframe

- Avantages : `ScrollArea::show_rows` virtualise trivialement les logs ;
  `TextEdit` en lecture seule donne sélection/copie ; intégration Tokio
  immédiate ; binaire unique ; idéal pour un tableau de bord de diagnostic.
- Coûts : esthétique « outil de développeur », difficile de reproduire les
  cartes, ombres, transitions et les 6 thèmes ; accessibilité Linux
  expérimentale (AccessKit Windows/macOS) ; MSRV 1.95 ; texte enrichi et
  wrapping à gérer ; pas de composants natifs (menus, dialogs).
- Verdict : excellent pour un panneau de diagnostic interne, insuffisant pour
  remplacer l'UI opérateur.

### 5.7 Matrice de décision pondérée

Notes de 1 (faible) à 5 (excellent). Les poids traduisent les priorités du
projet : ne pas réécrire, ne pas régresser l'UX, rester natif Linux.

| Critère (poids) | Tauri | Slint | Iced | GTK4 | egui |
| --- | :---: | :---: | :---: | :---: | :---: |
| Réutilisation du frontend existant (25) | **5** | 1 | 1 | 2 | 1 |
| Coût / effort de portage (20) | **5** | 3 | 2 | 3 | 4 |
| Fidélité UX + accessibilité (15) | 4 | 4 | 2 | **5** | 2 |
| Maturité Linux X11/Wayland + bureau (15) | 3 | **5** | 4 | **5** | 4 |
| Performance logs / gros volumes (10) | 3 | 4 | 3 | **5** | **5** |
| Empreinte mémoire / GPU (5) | 3 | **5** | 4 | 4 | **5** |
| Maintenance, licence, packaging (10) | 4 | 3 | 3 | 3 | 2 |
| **Total pondéré /5** | **4,15** | 3,15 | 2,35 | **3,60** | 2,90 |

Lecture :

- Tauri gagne sur les deux critères les plus lourds (réutilisation, effort).
- GTK4 est deuxième et gagne sur l'UX native, l'accessibilité, les logs et
  l'intégration bureau ; il perd sur la réutilisation.
- **Sensibilité** : si le poids « UX native + accessibilité » passe de 15 à 30
  et « réutilisation » de 25 à 10, GTK4 (4,05) dépasse Tauri (4,00). La
  décision bascule donc sur une seule question : **des widgets natifs sont-ils
  une exigence, ou un confort ?**
- Slint reste une alternative sérieuse si un jour la réécriture est décidée et
  que la licence est tranchée ; Iced et egui sont écartés pour cette UI.

Ordres de grandeur d'empreinte (estimations à confirmer par le PoC, sur la
machine de référence) :

| Option | RSS attendu | Démarrage | VRAM additionnelle |
| --- | ---: | ---: | ---: |
| Tauri / WebKitGTK | 150–300 Mo | 1–2 s | 100–250 Mo |
| GTK4 (GL) | 60–120 Mo | < 1 s | 30–80 Mo |
| GTK4 (`GSK_RENDERER=cairo`) | 60–120 Mo | < 1 s | **0 Mo** |
| Slint | 60–120 Mo | < 0,5 s | 30–100 Mo |
| Iced | 80–160 Mo | < 1 s | 50–150 Mo |
| egui | 60–120 Mo | < 0,5 s | 30–100 Mo |
| Firefox `--app` | 350–600 Mo | 2–4 s | 150–300 Mo |

Les fourchettes GTK4 sont des estimations à mesurer au PoC ; la confrontation
a écarté le 90–180 Mo initial, trop pessimiste pour une application sans
libadwaita lourde. Le point décisif n'est pas le RSS mais la VRAM : Cairo
supprime toute allocation GPU applicative, ce qui n'est vrai d'aucune autre
option de la table.

## 6. Topologie d'intégration

### 6.1 Modèle 1 — client lourd découplé (recommandé)

```text
┌──────────────────────────────┐
│ Application desktop (jetable)│
│  Tauri + WebKitGTK (T1)      │
│  · timeline / logs           │
│  · tray / notifications      │
│  · xdg-open (menu)           │
└──────────────┬───────────────┘
               │ HTTP loopback + X-Worker-Token
               │ polling 500 ms
               ▼
┌────────────────────────────────────┐
│ workflow-server  axum :5004        │
│  WorkflowState · Runner · sequence │
└──────────────┬─────────────────────┘
               │ tokio::process
               ▼
      workflow-cli stepN
      (HardwareLock, CUDA/TPU)
```

Caractéristiques :

- Le serveur reste **headless** et **autoritaire** : webhook, SQLite, verrou
  matériel, sérialisation 409, timeouts, SIGTERM/SIGKILL.
- La fenêtre est **jetable** : sa fermeture n'arrête ni le serveur, ni le
  moniteur, ni une séquence (si celle-ci est côté serveur).
- Le navigateur reste un client de secours et de diagnostic.
- Aucun changement de contrat HTTP ; le GUI est un client de plus.
- Coût d'intégration : nul côté serveur en T1.

### 6.2 Modèle 2 — binaire GUI monolithique

Techniquement possible : `workflow-server` expose déjà `Runner` et
`WorkflowState` en bibliothèque, et `Runner::execute_reserved` est publique.
Un binaire GUI pourrait donc lier l'orchestrateur sans axum.

Mais les bloqueurs opérationnels sont décisifs :

| Bloqueur | Détail |
| --- | --- |
| Cycle de vie | Fermer la fenêtre tue le webhook, l'ingestion et la séquence. Il faudrait un mode démon/tray avec `--headless`, donc réinventer le serveur. |
| Isolation des crashs | Un panic GUI, un crash WebKit/GTK ou un OOM graphique emporte l'orchestrateur. Aujourd'hui, seule la fenêtre tombe. |
| Double exécution | Rien n'empêcherait de lancer le binaire GUI **et** `workflow-server` : deux moniteurs webhook, deux bases, conflit de verrou. Il faudrait un verrou d'instance globale et une détection de serveur existant. |
| Verrou matériel | Le GUI devrait acquérir `/tmp/workflow-rs-hw.lock` dans son propre processus, avec les mêmes règles de course que le serveur (RunId, stopping, groupes de processus). |
| Sous-processus | Le GUI devrait reproduire `kill_on_drop`, les groupes de processus, SIGTERM→SIGKILL, la rotation des logs et le parsing de progression. |
| Testabilité | Toute la logique d'orchestration deviendrait dépendante d'une boucle d'événements graphique. |
| Bascule | Le rollback Python et l'automatisation headless perdent leur point d'entrée stable. |

Le modèle 2 n'apporte **aucun** gain fonctionnel : les étapes restent des
sous-processus (on ne veut pas lier CUDA/Coral dans le GUI), donc le seul gain
serait d'économiser un processus et un port loopback. Le coût est une
duplication de l'orchestrateur et une perte de robustesse.

**Verdict** : rejeté. Il ne redeviendrait pertinent que si l'on extrait un
`workflow-orchestrator` réutilisable **et** que l'application devient un démon
avec tray, mode headless et verrou d'instance — un autre projet.

### 6.3 Variantes du modèle 1

| Variante | Supervision du serveur | Avantages | Inconvénients |
| --- | --- | --- | --- |
| M1a « client pur » | Aucune ; le serveur est démarré par `switch-rust.sh` | Zéro secret dans le GUI, zéro cycle de vie | Message clair si le serveur est absent |
| M1b « superviseur » | `switch-rust.sh start` si `/api/ping` échoue | Expérience « un seul clic » | Le GUI doit lire `config/launcher.env` (0600) et ne jamais logger le jeton |
| M1c « systemd user » | `systemctl --user start workflow-server` | Survit aux crashes, redémarrage auto, secrets par `EnvironmentFile` | Introduit une unité à maintenir, cohabitation avec le PID file |

Recommandation : **M1a pour le PoC**, **M1c pour le MVP** si l'on veut un
service durable ; M1b seulement si l'opérateur refuse systemd. Dans tous les
cas, le GUI ne doit pas être le parent du serveur.

### 6.4 Séquence côté serveur (amélioration à ratifier)

Le serveur possède déjà `POST /run_custom_sequence`, `GET /sequence/status` et
`POST /sequence/stop`, testés (`http.rs` routes, `sequence.rs`). Le frontend
web ne les utilise pas.

Pour le desktop, utiliser la séquence serveur :

- la séquence survit à la fermeture de la fenêtre et au redémarrage du GUI ;
- un seul acteur pilote la séquence (pas de boucle JS) ;
- l'arrêt de séquence devient possible (`POST /sequence/stop`), ce que l'UI
  actuelle ne fait pas ;
- le résumé de séquence se reconstruit depuis `/sequence/status` et les états
  par étape.

C'est un **changement de comportement volontaire** (pas une régression
d'affichage) : il doit être documenté et couvert par un test serveur
supplémentaire (séquence démarrée, GUI fermé, étapes suivantes exécutées).

### 6.5 Sécurité et secrets

- Le jeton worker est aujourd'hui dans la balise `<meta>` de la page rendue.
  En T1, rien ne change : la page vient du serveur, même origine.
- En T2 (assets embarqués), il faudrait sortir le jeton du HTML et le fournir
  par commande Tauri ; c'est un avantage, mais cela impose CORS + CSP.
- Le GUI ne doit jamais écrire le jeton dans ses logs, ses crash reports ou son
  `localStorage`.
- L'écoute reste `127.0.0.1` ; `server.allow_remote = true` reste interdit pour
  l'usage desktop.
- Instance unique : `tauri-plugin-single-instance` (ou `GtkApplication` avec un
  app-id stable) pour éviter deux fenêtres et deux jeux de préférences.

### 6.6 Parité et rollback

| Contrat | Impact desktop |
| --- | --- |
| `/status`, `/api/step_status` | Inchangé ; le GUI consomme le même JSON |
| Codes 401/404/409 et messages français | Inchangé ; le GUI doit les afficher tels quels |
| `Cache-Control: no-store` | Inchangé |
| Logs échappés HTML | Inchangé ; le WebView les rend comme aujourd'hui |
| `/sound-design` | Inchangé (ServeDir) |
| DB SQLite, verrou, `/mnt/cache` | Aucun accès direct du GUI |
| Python `:5003` | Non modifié ; `sync-backends.sh` inchangé |
| Navigateur `:5004` | Reste disponible pour le rollback et le diagnostic |

## 7. Contraintes critiques

### 7.1 Logs : débit, rendu, virtualisation

Le débit mesuré ne justifie **pas** une virtualisation pour le log principal :
300 lignes × 118 o = 35 Ko, à 2 Hz. En revanche :

- le rendu actuel réécrit tout le HTML à chaque poll ; la correction la plus
  rentable est un **rendu incrémental** (append des nouvelles lignes, ring
  buffer de 300, ancrage du scroll si l'utilisateur est en bas) ;
- cette correction est faisable en JavaScript, sans changer de toolkit ; elle
  doit être testée dans le navigateur avant d'être portée ;
- une virtualisation devient nécessaire si l'on affiche **le log complet**
  (fichier de 143 Ko à plusieurs Mo, 1 000 à 50 000 lignes), ce qui est une
  **nouvelle fonction** (lecture directe de `runtime/logs/stepN/*.log`) ;
- en GTK4, `GtkListView` + `Gio::ListStore` virtualise nativement ; un
  `GtkTextView` suffit pour 300–10 000 lignes et donne la sélection/copie ;
- en egui, `ScrollArea::show_rows` fait le travail ; en Slint, `ListView`
  impose des hauteurs fixes ; en Iced, la fenêtre visible est à calculer.

| Toolkit | Virtualisation | Sélection / copie | Effort |
| --- | --- | --- | --- |
| Tauri / WebView | fenêtrage à écrire en JS (ou innerHTML actuel) | native (DOM) | Faible |
| GTK4 | `GtkListView` native ; `GtkTextView` pour ≤ 10 000 lignes | native | Faible |
| Slint | `ListView` native, hauteur de ligne fixe | à valider sur `Text` non éditable | Moyen |
| Iced | fenêtre visible à calculer et maintenir | partielle | Élevé |
| egui | `ScrollArea::show_rows` | `TextEdit` lecture seule | Faible |

Conclusion : ne pas dimensionner l'architecture sur un faux problème de débit.
Dimensionner sur le **rendu incrémental** et sur la **lisibilité** (filtre,
recherche, copie), qui sont les vrais manques.

#### Conception de rendu retenue (issue de la confrontation)

Quel que soit le toolkit, le modèle suivant est adopté :

- `VecDeque<LogEntry>` borné à 300 lignes en phase 1 (taille du contrat
  serveur), extensible si un flux complet est ajouté ;
- `LogEntry { seq_id: u64, level: LogLevel, text: Arc<str> }` : l'identifiant
  de séquence permet de dédupliquer et d'ordonner les deltas, `Arc<str>` évite
  les copies par ligne ;
- **coalescence de 16 ms** avant rendu : plusieurs polls ou deltas sont
  fusionnés en une seule passe, ce qui borne le travail à ~60 rendus/s même en
  rafale ;
- `stick_to_bottom` : l'auto-scroll ne s'applique que si l'utilisateur est déjà
  en bas ; un scroll manuel le désactive et affiche un bouton « revenir en
  bas » ;
- **classification par les motifs existants** de `logPatterns.js` (8 regex :
  casse, accents, `Échec`, horodatages, `[INFO]`), pas par des `contains` ad
  hoc ;
- sélection et copie multi-lignes natives (DOM, `GtkTextView` ou `TextEdit`) ;
- **SSE** (`GET /api/events`) est une option de phase 3, additive, authentifiée
  et bornée : elle sert à afficher le log complet ou à réduire la latence, pas
  à faire fonctionner le PoC.

### 7.2 Audio sous Linux

- Le WebView Tauri (WebKitGTK) supporte Web Audio ; les MP3 sont servis en
  même origine. Le risque est la politique d'autoplay : les sons déclenchés par
  un clic passent, ceux déclenchés par une fin d'étape asynchrone peuvent être
  bloqués. À tester en priorité (critère de sortie du PoC).
- Repli natif : `rodio` (cpal + symphonia) ou `kira`. PipeWire est présent avec
  compatibilité Pulse ; ALSA est présent. `rodio` évite `libpulse-dev` et
  décode le MP3 en pur Rust.
- Conserver la limite de 1 son/s et le volume maître. En T1, les préférences
  (`ui.soundEnabled`, `ui.soundVolume`) restent dans le `localStorage` du
  WebView, qui a son propre profil : elles survivent au redémarrage de
  l'application mais ne sont pas partagées avec Firefox. Si le son devient
  natif, les stocker dans un fichier de configuration du GUI.
- En GTK4 : `Gio::MediaFile`/GStreamer est plus lourd ; `rodio` reste le plus
  simple.

### 7.3 Notifications

- L'API Web Notification n'est pas disponible dans WebKitGTK. Le bandeau
  interne continue de fonctionner, mais les notifications hors fenêtre
  nécessitent `notify-rust` (D-Bus `org.freedesktop.Notifications`) ou
  `tauri-plugin-notification`.
- Solution recommandée en T1 : le **superviseur Rust** interroge
  `/api/step_status/*` (ou `/status/*`) à 1–2 Hz et émet une notification sur
  transition `completed`/`failed`. Avantage : aucune IPC distante, aucun
  changement frontend, et les notifications fonctionnent même si la page est
  sur un autre onglet.
- En GTK4 : `Gio::Notification` + `Application::send_notification`.

### 7.4 GPU, VRAM et Coral

- La GTX 1650 a 4 Gio, dont ~933 Mio déjà consommés par le bureau. STEP2 CUDA
  a besoin du reste ; un WebView qui composite en GPU ajoute 100–300 Mio.
- WebKitGTK + NVIDIA propriétaire : risques d'artefacts ou de crash avec le
  renderer DMA-BUF. Mitigations : `WEBKIT_DISABLE_DMABUF_RENDERER=1`,
  `WEBKIT_DISABLE_COMPOSITING_MODE=1`, ou passage en rendu logiciel. Coût :
  CPU plus élevé, à mesurer.
- Ne pas lancer d'animation pendant STEP2 ; le moniteur système reste à 5 s ;
  le GUI doit pouvoir être minimisé sans consommer.
- Coral : aucun accès depuis le GUI. Ne jamais ouvrir `/dev/apex_0` pour
  « vérifier » la TPU : un accès concurrent au device pendant STEP3/STEP4 est
  proscrit (sessions TFLite exclusives).
- Single worker : conservé par le modèle 1. Le GUI ne doit pas contourner le
  409 en lançant `workflow-cli` directement.

### 7.5 Sous-processus et arrêt

- Inchangés en modèle 1 : `Runner` possède `tokio::process`, `kill_on_drop`,
  les groupes de processus, le timeout et la séquence SIGTERM → SIGKILL.
- Le GUI ne doit envoyer aucun signal aux PID : il appelle `/stop` ou
  `/cancel`, et affiche `stopping` jusqu'à la mort réelle.
- En Tauri, fermer la fenêtre ne doit pas déclencher de `kill` sur le serveur :
  pas de `Command::spawn` sans `setsid`/détachement, ou pas de spawn du tout.

### 7.6 Packaging et dépendances

| Élément | Build | Runtime |
| --- | --- | --- |
| Tauri | `libwebkit2gtk-4.1-dev`, `libxdo-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `libssl-dev` | WebKitGTK 4.1 (présent), GTK3 (présent), AppIndicator |
| GTK4 | `libgtk-4-dev`, `libadwaita-1-dev` | GTK 4.6 (présent), libadwaita 1.1 (présent) |
| Slint / Iced / egui | Mesa/EGL headers selon backend | GL/Vulkan, X11/Wayland |

- Ubuntu 22.04 fournit tous les paquets de build ; ils sont juste absents.
- Livraison recommandée : **`.deb`** pour cette machine (dépendances explicites),
  ou binaire + `.desktop` + icônes pour Slint/Iced/egui. L'AppImage Tauri
  embarque les dépendances (70+ Mo) et, comme l'application joue des sons,
  exige `bundleMediaFramework` (GStreamer) : plus lourd à maintenir ici qu'un
  `.deb`.
- L'application doit fonctionner hors terminal : pas de dépendance à
  `config/launcher.env` dans l'environnement du GUI ; le serveur, lui, le lit.
- Polices : Noto Color Emoji est présent ; pour GTK/Slint/Iced, prévoir un
  fallback d'icônes si l'emoji n'est pas rendu (les icônes du template sont des
  emoji).

### 7.7 Accessibilité, thèmes, i18n

- Conserver : rôles, `aria-live`, focus trap, restauration du focus,
  `prefers-reduced-motion`, contrastes.
- Tauri/WebKitGTK hérite d'AT-SPI : le comportement actuel est préservé.
- GTK4 expose AT-SPI nativement ; il faut re-déclarer les rôles des composants
  custom (timeline, cartes) et vérifier avec Orca.
- Iced/egui sous Linux : régression d'accessibilité à assumer explicitement.
- Les 6 thèmes CSS sont un actif : en Tauri ils sont gratuits ; en GTK4 il faut
  les retraduire en `CssProvider` ; en Slint/Iced/egui, en tokens de style.
- Les libellés sont en français et les messages d'erreur du serveur aussi ; ne
  pas introduire de traduction automatique.

## 8. Recommandation et feuille de route

### 8.1 Décision recommandée

1. **Cible** : application desktop Linux, **modèle 1** (client du serveur).
2. **Technologie du premier lot** : **Tauri v2 en mode T1** (URL distante),
   fonctions de bureau côté Rust, **aucun changement serveur ni frontend**.
3. **Option native de repli** : **GTK4 + gtk4-rs/Relm4**, évaluée uniquement si
   les critères de sortie du PoC échouent.
4. **Séquences** : basculer sur `/run_custom_sequence` + `/sequence/status`
   (décision séparée, testée).
5. **Cycle de vie** : le serveur est supervisé indépendamment du GUI
   (`switch-rust.sh start` en PoC, systemd user en MVP).
6. **Ne pas** fusionner l'orchestrateur dans le binaire GUI.

Le compromis, en une table :

| Axe | Pragmatique — Tauri T1 | Idéal natif — GTK4/Relm4 |
| --- | --- | --- |
| Délai | 5–8 j PoC, 12–18 j MVP | 70–95 j |
| Réutilisation | ~95 % du frontend | ~0 % (réécriture) |
| Rendu | WebView WebKitGTK | widgets GTK4 natifs |
| Accessibilité | héritée du DOM/WebKitGTK | AT-SPI natif |
| Logs | DOM + fenêtrage à écrire | `GtkListView` native |
| Risque principal | WebKitGTK + NVIDIA | réécriture + GTK 4.6 |
| Réversibilité | totale (coque jetable) | faible (nouveau client) |

### 8.2 Pourquoi cette recommandation

- Elle préserve **100 % du contrat HTTP** et **100 % du frontend**, donc la
  parité, les tests serveur et le rollback navigateur/Python.
- Elle livre une valeur immédiate (fenêtre dédiée, tray, notifications,
  instance unique, ouverture de dossiers) sans réécrire 11 000 lignes.
- Elle est **réversible** : si WebKitGTK déçoit, on jette la coque et on garde
  le serveur et la page ; le coût perdu est de quelques jours.
- Elle concentre le risque sur un point mesurable (WebKitGTK/NVIDIA) au lieu de
  le répartir sur une réécriture de trois mois.

### 8.3 Jalons et critères de sortie

#### Phase 0 — Cadrage (1–2 j)

- Trancher : widgets natifs exigés ? distribution sur d'autres machines ?
  licence Slint si cette voie est un jour explorée ?
- Mettre à jour la politique MSRV (Tauri 2.12 → 1.90).
- Créer `crates/workflow-desktop` sans toucher aux autres crates.
- Installer les paquets de build et documenter la commande.
- Rédiger la décision « séquence côté serveur » et son test.

#### Phase 1 — PoC Tauri T1 (5–8 j)

Livrables :

- fenêtre Tauri qui charge `http://127.0.0.1:5004/` ;
- tray + instance unique + fermeture vers le tray ;
- superviseur Rust : sonde `/api/system_monitor` (non authentifiée),
  notifications sur transition `completed`/`failed` via `/api/step_status/*` ;
- menu « Ouvrir le cache » via `xdg-open` sur `/mnt/cache` ;
- page d'erreur native si le serveur est absent ;
- aucun changement dans `workflow-server`, `workflow-cli` ou `assets/web`.

Critères de sortie (tous obligatoires) :

| Critère | Seuil |
| --- | --- |
| Les 8 cartes s'affichent, statuts et progression à 500 ms | conforme |
| Lancer/annuler une étape, 409 affiché | conforme |
| Overlay de logs, focus trap, Échap | conforme |
| Sons audibles (clic puis fin d'étape) | conforme ou repli `rodio` validé |
| Notification bureau sur fin d'étape | conforme |
| RSS au repos | ≤ 300 Mo |
| CPU au repos | ≤ 2 % d'un cœur |
| Artefacts WebKitGTK avec NVIDIA 535 | aucun, ou workaround documenté et mesuré |
| Accessibilité Orca | rôles et annonces conservés |
| `git diff` hors nouveau crate/docs | vide |

#### Phase 2 — MVP (10–12 j)

- `.deb` + entrée `.desktop` + icône + autostart optionnel.
- Serveur en systemd user (ou `switch-rust.sh start` documenté) ; secrets par
  `EnvironmentFile` 0600.
- Rendu incrémental des logs (patch frontend documenté dans
  `memory-bank/systemPatterns.md`) ou fenêtre de logs native si T2.
- Séquences serveur + résumé reconstruit depuis `/sequence/status`.
- Gestion des états dégradés : serveur arrêté, jeton invalide, port occupé,
  Python actif.
- Moniteur enrichi : espace libre `/mnt/cache`, badge `/dev/apex_0`, état réel
  du verrou matériel (`fcntl(F_GETLK)` ou état serveur, jamais
  `Path::exists()`), et NVML direct (`nvml-wrapper`) avec repli `nvidia-smi`.
- Documentation opérateur (`docs/`), tests unitaires du superviseur, `fmt` +
  `clippy` + `cargo test --workspace`.
- Mesures VRAM pendant STEP2 avec la fenêtre ouverte.

#### Phase 3 — Porte de décision (2 j)

Comparer les mesures du PoC Tauri aux seuils. Si échec :

- artefacts GPU non corrigeables, ou
- RSS > 500 Mo, ou
- latence d'entrée > 100 ms, ou
- crash WebKit pendant STEP2,

alors lancer un **PoC GTK4 de 8–10 j** limité à la timeline et aux logs, avant
tout engagement plus large.

#### Phase 4 — Réécriture native (optionnelle, 70–95 j)

Uniquement si la Phase 3 échoue ou si « widgets natifs » devient une exigence.
Découpage : socle + timeline (15 j), logs + virtualisation (10 j), contrôles +
séquences (10 j), moniteur + ingestion (8 j), thèmes + a11y (10 j), packaging +
tests (10 j), marge (15 j).

### 8.4 Arborescence proposée (phase 1)

```text
crates/workflow-desktop/
  Cargo.toml
  tauri.conf.json          # fenêtre -> http://127.0.0.1:5004/
  capabilities/default.json
  icons/
  ui/index.html            # page locale de secours si le serveur est absent
  src/main.rs              # bootstrap, instance unique, tray
  src/supervisor.rs        # santé + notifications (reqwest + notify-rust)
  src/menu.rs              # ouvrir le cache, ouvrir les logs
```

Aucune dépendance npm : Tauri v2 se construit avec `cargo tauri` et un
`frontendDist` minimal (la page de secours). Le reste est du Rust.

Principes Rust du nouveau crate (`rust-patterns`, `rust-concurrency-orchestrator`) :

- `DesktopError` typée avec `thiserror` à la frontière du superviseur ;
  `anyhow` uniquement dans `main` ;
- état de santé modélisé par un enum (`Reachable`, `Degraded`, `Unreachable`)
  plutôt que par des booléens, et matching exhaustif ;
- aucune IPC bloquante dans l'executor : `reqwest` avec timeout explicite,
  `notify-rust` (bloquant) dans `spawn_blocking` ;
- canal borné entre le poller et le thread de notifications, pas de file non
  bornée ;
- pas de `unwrap()` dans le superviseur ; les erreurs de notification sont
  journalisées sans faire tomber la fenêtre ;
- surface `pub` minimale (`pub(crate)` par défaut) et tests inline
  `#[cfg(test)]`, conformément au dépôt.

### 8.5 Ce qu'il ne faut pas faire

- Lier `workflow-steps` ou `workflow-tpu` dans le binaire GUI.
- Lancer `workflow-cli` depuis le GUI pour contourner le 409.
- Ajouter un second moniteur webhook ou une seconde base.
- Écrire dans `/mnt/cache` depuis un test ou depuis le GUI.
- Diverger `assets/web` en silence ; toute correction UI doit être documentée.
- Ajouter npm/Node au dépôt (règle `frontend-timeline-designer`).
- Supprimer l'accès navigateur `:5004` : c'est le rollback et l'outil de
  diagnostic.
- Ajouter SSE ou changer `/status` sans décision de contrat.

## 9. Risques

| ID | Risque | Prob. | Impact | Mitigation |
| --- | --- | --- | --- | --- |
| R1 | Artefacts/crash WebKitGTK + NVIDIA 535 | Moyenne | Élevé | PoC sur la machine réelle ; `WEBKIT_DISABLE_DMABUF_RENDERER=1` ; sinon GTK4 |
| R2 | WebKitGTK 2.50 plus ancien que Firefox | Faible | Moyen | Tester modules ES, `Worker` module, `?.`/`??` ; le code est déjà compatible |
| R3 | Autoplay Web Audio bloqué | Moyenne | Faible | Repli `rodio` + sons déclenchés par interaction |
| R4 | MSRV 1.86 → 1.90 (Tauri) ou 1.93 (Relm4) | Certaine | Faible | Décision explicite, toolchain 1.97 déjà installée |
| R5 | En-têtes de dev absents | Certaine | Faible | `apt install` documenté ; pas de dépendance runtime nouvelle |
| R6 | Fermeture du GUI interrompt une séquence client | Certaine (état actuel) | Élevé | Séquence serveur (`/run_custom_sequence`) + test dédié |
| R7 | Re-rendu complet des logs | Certaine | Moyen | Rendu incrémental ; virtualisation seulement si log complet |
| R8 | Notifications Web absentes dans WebKitGTK | Certaine | Moyen | Superviseur Rust + `notify-rust` |
| R9 | Préférences Firefox non reprises | Certaine | Faible | Documenter ; éventuel import one-shot |
| R10 | Deux instances / deux backends | Faible | Élevé | Instance unique + refus si Python actif + doc |
| R11 | VRAM saturée pendant STEP2 | Moyenne | Élevé | Mesure VRAM en PoC ; GUI minimisé ; pas d'animation |
| R12 | Licence Slint si cette voie est choisie | Possible | Moyen | Trancher avant tout investissement |
| R13 | GTK 4.6/libadwaita 1.1 bride Relm4 0.11 | Certaine si GTK4 | Moyen | Feature `gnome_42` validée au PoC, sinon gtk4-rs direct |
| R14 | Dérive de parité si `assets/web` est modifié | Moyenne | Moyen | Corrections minimales, documentées, testées côté serveur |
| R15 | Le GUI devient un point de panne du poste | Faible | Moyen | Serveur indépendant, tray, page d'erreur, logs |

## 10. Charges comparées

| Option | PoC | Parité / MVP | Maintenance/an | Risque projet |
| --- | ---: | ---: | ---: | --- |
| Tauri T1 | 5–8 j | 12–18 j | 4–6 j | Moyen |
| Tauri T2 | 8–12 j | 20–28 j | 5–7 j | Moyen |
| Slint | 8–10 j | 55–80 j | 3–5 j | Élevé |
| Iced | 10–12 j | 75–105 j | 8–12 j | Très élevé |
| GTK4 / Relm4 | 8–10 j | 70–95 j | 4–6 j | Élevé |
| egui | 5–7 j | 30–45 j (+15–25 j design) | 8–12 j | Moyen (dette UX) |
| Navigateur `--app` | 0,5 j | 1 j | 0 | Faible, hors exigences |

Hypothèses : un développeur, connaissance du dépôt mais pas du toolkit ;
hors temps d'installation système ; hors tests matériels STEP2/3/4 longs.

## 11. Questions à trancher

1. **Widgets natifs** : exigence ou confort ? Cette réponse décide entre Tauri
   et GTK4.
2. **Distribution** : cette machine seulement, ou plusieurs postes ? Un `.deb`
   lié à Ubuntu 22.04 n'est pas portable tel quel.
3. **Supervision** : `switch-rust.sh` (PID file) ou systemd user ? Qui possède
   le serveur quand le GUI est fermé ?
4. **Séquence** : accepte-t-on de passer à la séquence serveur, avec un
   changement de comportement documenté ?
5. **Cache** : ajoute-t-on la recherche/ouverture de `/mnt/cache` (nouvelle
   fonction) ou reste-t-on hors périmètre ?
6. **Notifications** : superviseur Rust (aucune IPC) ou plugin Tauri (IPC
   distante à autoriser) ?
7. **MSRV** : accepte-t-on de monter à 1.90 (Tauri) ou 1.93 (Relm4) ?
8. **Licence Slint** : GPLv3, royalty-free avec attribution, ou abandon ?
9. **VRAM** : la VRAM doit-elle être strictement isolée pendant STEP2 ? Si oui,
   Cairo GTK4 est la seule option à 0 Mio applicative ; Tauri ajoute
   100–250 Mo et réduit la marge sur 4 Gio.
10. **Log complet** : afficher 10 000 lignes ou plus est-il requis ? Si oui,
    SSE ou lecture directe des fichiers, et GTK4 devient plus attractif ; si
    non, 300 lignes suffisent et le débat est clos.
11. **Accessibilité** : AT-SPI est-il contractuel ? Si oui, Iced est
    définitivement exclu et Tauri doit être validé avec Orca au PoC.

## 12. Mise à jour après confrontation (révision 1.1)

L'étude indépendante `docs/audits/Migration-Architecture-GUI-Linux-workflow-rs.md`
(E2) et la présente étude (E1) ont été confrontées dans
`docs/audits/2026-10-24-confrontation-etudes-gui-linux.md`. Les deux convergent
sur l'architecture (modèle 1, serveur headless, GUI jamais parent) et divergent
sur le toolkit. Les corrections suivantes sont appliquées ici :

| # | Correction | Emplacement |
| --- | --- | --- |
| 1 | `GtkFileLauncher` (GTK 4.10) remplacé par `gio::AppInfo`/`xdg-open` | §5.5, §8.3 |
| 2 | RSS GTK4 révisé de 90–180 Mo à 60–120 Mo, à mesurer | §5.7 |
| 3 | `GSK_RENDERER=cairo` ajouté : 0 Mio de VRAM applicative | §5.5, §5.7 |
| 4 | NVML direct en phase 2, avec repli `nvidia-smi` | §8.3 |
| 5 | Badges `/mnt/cache` et `/dev/apex_0`, verrou via `F_GETLK` | §8.3 |
| 6 | Modèle `LogEntry`, coalescence 16 ms, `stick_to_bottom` | §7.1 |
| 7 | SSE classé en phase 3, additif et optionnel | §7.1 |
| 8 | Iced confirmé exclu (AT-SPI absent, issue #552) | §5.4, §11 |

La matrice du §5.7 est conservée comme lecture d'E1 (elle pèse fortement la
réutilisation). La matrice consolidée, qui note l'effort GTK4 à 2/5 et montre
que le classement bascule selon la pondération, se trouve dans le document de
confrontation (§3.1). Aucun total unique ne doit être présenté comme un
verdict.

Deux points de l'étude E2 ne sont **pas** repris, car contredits par le code :
`/api/system_monitor` existe bien en Rust (`http.rs:82`, `614-669`) et le
frontend n'interroge jamais `/sequence/status` ; le polling réel est de
2 req/s par étape active, pas 5 req/s. L'estimation E2 de 4 semaines pour une
parité GTK4 est également écartée : le périmètre réel reste à 70–95 jours.

**Recommandation consolidée** : l'architecture est acquise ; le choix du
toolkit dépend d'un arbitrage d'exigence, pas d'un fait technique.

| Si l'exigence dominante est… | Alors | Effort |
| --- | --- | --- |
| Délai court, parité visuelle, zéro changement serveur/frontend | **Tauri v2 T1** | 12–18 j (PoC 5–8 j) |
| Widgets natifs, 0 Mio VRAM, AT-SPI, logs virtualisés | **GTK4/Relm4** | 70–95 j (PoC 8–10 j) |
| Stack 100 % Rust sans WebKit, accessibilité minimale | Slint | 55–80 j |
| Panneau de diagnostic interne | egui | 30–45 j |
| UI opérateur accessible | **Pas Iced** | — |

Si le budget le permet, le PoC de décision à deux pistes (15–20 j) est
préférable à une erreur de choix de trois mois : piste A Tauri (intégration,
RSS, sons, notifications) et piste B GTK4 (timeline, 10 000 lignes
synthétiques, Cairo, Orca), puis comparaison VRAM pendant STEP2.

## Annexe A — Fichiers inspectés

- Frontend : `assets/web/templates/index_new.html`, `assets/web/static/*.js`,
  `static/state/AppState.js`, `static/utils/*.js`, `static/css/**`,
  `sound-design/*.mp3`.
- Serveur : `crates/workflow-server/src/{http,state,runner,progress,catalog,sequence,lib}.rs`.
- CLI : `crates/workflow-cli/src/main.rs` (surface des commandes).
- Contrats : `docs/CONTRAT-ENVIRONNEMENT.md`, `memory-bank/{activeContext,
  decisionLog,systemPatterns,productContext,progress}.md`, `AGENTS.md`,
  `.agents/rules/codingstandards.md`, `.agents/skills/*`.
- Lanceurs : `scripts/start-rust.sh`, `scripts/switch-rust.sh`, `README.md`.

## Annexe B — Commandes de mesure

```sh
# Versions et dépendances
pkg-config --modversion gtk+-3.0 webkit2gtk-4.1 gtk4 libadwaita-1 2>/dev/null
ldconfig -p | grep -E 'libwebkit2gtk|libgtk-4|libadwaita|libpulse'
fc-match "Noto Color Emoji"

# Volume de logs
for f in runtime/logs/step_STEP*.log; do
  printf '%s %s %s\n' "$f" "$(wc -l < "$f")" "$(stat -c%s "$f")"
done

# Coût nvidia-smi
/usr/bin/time -f '%e s' nvidia-smi --query-gpu=name,utilization.gpu,memory.used \
  --format=csv,noheader

# Poids du frontend
find assets/web/static -name '*.js' -printf '%s\n' | awk '{s+=$1} END{print s}'
find assets/web/static/css -name '*.css' -printf '%s\n' | awk '{s+=$1} END{print s}'
```

## Annexe C — Esquisse `tauri.conf.json` (T1)

```json
{
  "productName": "Workflow Manager",
  "identifier": "fr.kidpixel.workflow",
  "build": {
    "frontendDist": "ui"
  },
  "app": {
    "windows": [
      {
        "title": "Workflow Manager",
        "url": "http://127.0.0.1:5004/",
        "width": 1440,
        "height": 900,
        "minWidth": 1024,
        "minHeight": 700
      }
    ],
    "security": {
      "csp": null
    }
  }
}
```

Notes : `frontendDist` ne sert qu'à la page de secours locale (`ui/index.html`) ;
la fenêtre pointe sur le serveur. `csp: null` laisse la CSP du serveur
s'appliquer à la page distante. Ne pas activer `dangerousRemoteDomainIpcAccess`
sans besoin explicite.

## Annexe D — Synthèse en une page

| Question | Réponse |
| --- | --- |
| Faut-il réécrire le pipeline ? | Non. |
| Faut-il modifier `workflow-server` ? | Non en T1 ; CORS/CSP seulement si T2. |
| Faut-il modifier `assets/web` ? | Non en T1 ; correctifs ciblés (logs) en phase 2. |
| Quel modèle ? | Client du serveur (M1), serveur headless. |
| Quelle techno ? | Tauri v2 T1 si le délai prime ; GTK4/Relm4 si « widgets natifs » et 0 Mio VRAM sont exigés (arbitrage §11). |
| Quel risque principal ? | WebKitGTK + NVIDIA (Tauri) ; effort et GTK 4.6 (GTK4). |
| Quel gain immédiat ? | Fenêtre dédiée, tray, notifications, instance unique, cycle de vie serveur découplé. |
| Quelle régression ? | Aucune fonctionnelle en T1 ; préférences Firefox non reprises. |
| Combien de temps ? | 5–8 j PoC, 12–18 j MVP. |
| Quand basculer vers GTK4 ? | Si « widgets natifs » est une exigence, ou si le PoC Tauri échoue aux seuils du §8.3. |
