# Rapport Deep Search GUI Linux — analyse critique et vérifications locales

- **Date** : 2026-10-24
- **Objet** : analyse de `docs/audits/Analyse_Architecturale_GUI_Linux.md` (E3),
  rapport de recherche externe sur le remplacement de `assets/web` par un
  client desktop Linux.
- **Documents liés** : `docs/ETUDE-GUI-NATIVE-LINUX.md` (E1, v1.1),
  `docs/audits/Migration-Architecture-GUI-Linux-workflow-rs.md` (E2),
  `docs/audits/2026-10-24-confrontation-etudes-gui-linux.md` (C).
- **Périmètre** : aucune source fonctionnelle modifiée, aucun accès à
  `runtime/`, aucun écrit dans `/mnt/cache`. Les tests locaux sont des lectures
  et des lancements de `MiniBrowser` et d'une sonde GTK4 sur des pages
  temporaires dans `/tmp/opencode`.

## TL;DR

1. **L'architecture de E3 est la bonne** : modèle 1 (GUI client, serveur
   headless autoritaire), Tauri v2 en URL distante sans IPC (`A-Strict`) en
   phase 1, GTK4/Relm4 en repli, rejet de l'hybride C2, socle Rust partagé
   C3. E3 confirme E1/C sur l'essentiel.
2. **E3 apporte des faits neufs utiles** : le patch Ubuntu
   `disable-nvidia-dmabuf.patch` existe bien, `libwebkitgtk-6.0-4` et
   `-dev` sont disponibles dans `jammy-updates`, le risque IPC des URL
   distantes Tauri est réel, le tray GTK4 doit passer par D-Bus (pas
   `libayatana-appindicator3`), l'autoplay WebKitGTK justifie `rodio`, et
   Relm4 0.11 expose bien la feature `gnome_42`.
3. **Le risque R1 (écran blanc / SIGSEGV DMA-BUF) n'est pas reproduit sur le
   chemin par défaut** : `MiniBrowser` 2.50.4 rend une page statique *et* une
   page avec calque de composition dans les profils par défaut,
   `WEBKIT_DISABLE_DMABUF_RENDERER=1` et `FORCE_SHM`, y compris après cinq
   bascules d'espace de travail. En forçant le chemin DMA-BUF
   (`WEBKIT_FORCE_DMABUF_RENDERER=1`), on reproduit en revanche une page vide
   et des erreurs `AcceleratedSurface...` : le risque est réel, mais le
   chemin par défaut d'Ubuntu est protégé par le patch. R1 devient un risque
   de **régression future** (apt upgrade, autre distribution, build non
   patchée), pas un risque immédiat.
4. **Plusieurs affirmations décisives de E3 sont fausses ou trop fortes** :
   `/api/system_monitor` est interrogé toutes les **5 s**, pas à 2 Hz ; les
   MSRV de repli citées sont fausses (`gtk4 0.11.5` = 1.92, `relm4 0.10.1` =
   1.92 ; les versions à 1.83/1.85 sont `gtk4 0.10.3` et `relm4 0.10.0`) ;
   les 10 000 lignes de logs sont un problème **serveur**, pas un argument
   toolkit ; la commande VRAM du PoC (`--query-compute-apps`) ne voit pas
   WebKit ; l'identifiant AppIndicator testé n'est pas celui d'Ubuntu ; le
   « 0 Mio » de GTK4 est crédible au niveau contexte GPU (pas de contexte G
   sous Cairo) mais le processus ouvre quand même `/dev/nvidia0`.
5. **Décision recommandée** : PoC **Tauri A-Strict d'abord, 5 jours**, avec
   des portes corrigées et une mesure relative à la baseline Firefox, puis
   GTK4 seulement si le PoC échoue. Le crate partagé
   `workflow-desktop-services` (C3) doit être créé dès le premier jour.

## 1. Verdict

E3 est **le document le plus utile des trois pour préparer un PoC**, mais pas
pour trancher seul. Il apporte une veille externe réelle et un plan de tests
falsifiable ; il reste faible sur la mesure, la traçabilité des sources et
certaines versions. Il ne faut donc pas le traiter comme un audit de code : il
n'a pas relu le dépôt, il reprend les faits de C.

La recommandation conditionnelle de E3 est conservée, avec deux corrections de
cap :

- le **PoC Tauri est prioritaire**, pas un PoC à deux pistes de 15–20 jours :
  le test local ne justifie pas de financer GTK4 en parallèle dès maintenant ;
- les **seuils absolus** (50 Mio de VRAM, 300 Mio de RSS) sont remplacés par
  des seuils **relatifs à la baseline** (Firefox + bureau actuel) et par
  l'absence d'OOM CUDA, seule conséquence métier qui compte.

## 2. Ce que E3 confirme

| Affirmation | Statut | Preuve |
| --- | --- | --- |
| Modèle 1 : GUI client, serveur headless autoritaire, GUI jamais parent | Confirmé | E1, E2 et C convergent ; aucune raison de fusionner. |
| Tauri v2 en URL distante est la voie la moins chère | Confirmé | 12–18 j-h estimés, réemploi du HTML/JS, aucun changement de contrat. |
| GTK4/Relm4 est la voie native de repli | Confirmé | 70–95 j-h, widgets natifs, AT-SPI direct, `GSK_RENDERER=cairo`. |
| Iced et egui sont écartés | Confirmé | AT-SPI insuffisant, composants à réécrire. |
| L'hybride C2 (GTK4 + WebKitGTK 6.0) est un piège | Confirmé | `GSK_RENDERER=cairo` ne gouverne que le processus GTK4, pas `WebKitWebProcess` ; C2 réimporte les bogues et la RAM de WebKit. |
| Le tray GTK4 ne peut pas lier `libayatana-appindicator3` | Confirmé | Collision GTK3/GTK4 dans le même espace d'adressage ; passer par `ksni`/zbus ou D-Bus SNI. |
| L'audio WebView peut être bloqué par l'autoplay | Confirmé | Politique WebKitGTK ; `rodio` côté Rust supprime la dépendance au geste utilisateur. |
| Les API GTK 4.10+ et libadwaita 1.4+ manquent sur 22.04 | Confirmé | `GtkFileLauncher`, `GtkAlertDialog`, `adw::NavigationView`, `adw::Dialog` ; `var()` CSS absent avant GTK 4.16. |
| Relm4 0.11 a une feature `gnome_42` | Confirmé | docs.rs : feature par défaut, alignée sur GTK 4.6 / libadwaita 1.1. |
| Le risque IPC des URL distantes Tauri est réel | Confirmé | Avertissement officiel Tauri v2 : impossible de distinguer une iframe de la fenêtre principale sous Linux/Android ; la parade A-Strict (zéro capability remote) est correcte. |
| Le patch Ubuntu `disable-nvidia-dmabuf.patch` existe | Confirmé | Changelog Debian de `libwebkit2gtk-4.1-0` : patch synchronisé en 2.46.6 et 2.48.0. |
| WebKitGTK 6.0 est installable sur 22.04 | Confirmé | `apt-cache policy` : `libwebkitgtk-6.0-4` et `-dev` candidats 2.50.4 dans `jammy-updates`/`jammy-security`. |

## 3. Apports neufs et valides

- **Veille DMA-BUF / NVIDIA** : E3 documente la régression de
  `WEBKIT_DISABLE_DMABUF_RENDERER=1` sur WebKitGTK 2.50+ (issue
  `block/buzz#3654`, ouverte le 2026-07-30) et la variable
  `WEBKIT_DMABUF_RENDERER_FORCE_SHM`. C'est un apport réel : le contournement
  historique de Tauri n'est plus anodin.
- **Sécurité Tauri en URL distante** : distinction entre `remote` dans les
  capabilities et absence totale d'IPC ; la seconde est la seule acceptable
  pour A-Strict.
- **Tray** : explication correcte de l'impossibilité de charger GTK3 et GTK4
  dans le même processus, et de la voie D-Bus `StatusNotifierItem`.
- **Autoplay / audio** : la solution `rodio` côté Rust est la bonne, quel que
  soit le toolkit.
- **C2** : démonstration que la coque GTK4 ne protège pas le sous-processus
  WebKit ; le rejet est argumenté.
- **C3** : extraction d'un crate `workflow-desktop-services` (état, client
  HTTP, audio, notifications, tray, télémétrie) réutilisable par Tauri puis
  GTK4. C'est la meilleure idée architecturale du rapport.
- **Plan de PoC** : portes séquentielles, scénarios A1–A3/B1–B3, critères
  d'échec. Le principe est bon ; les commandes de mesure doivent être
  corrigées (§5).
- **MSRV Relm4 0.11** : 1.93 confirmé ; la feature `gnome_42` évite de
  descendre à gtk4-rs nu pour des raisons d'API C.

## 4. Vérifications locales

### 4.1 Environnement

| Élément | Valeur mesurée |
| --- | --- |
| Système | Ubuntu 22.04.5 LTS, noyau 6.8.0-138-generic |
| Session | X11 (`XDG_SESSION_TYPE=x11`, `DISPLAY=:0`) |
| Bureau | GNOME Shell 42.9 |
| GPU | NVIDIA GTX 1650, 4 096 Mio, pilote 535.309.01 |
| GTK | 4.6.9+ds-0ubuntu0.22.04.2 |
| libadwaita | 1.1.7-0ubuntu0.22.04.1 |
| WebKitGTK | 2.50.4-0ubuntu0.22.04.1 (`libwebkit2gtk-4.1-0`) |
| Rust | 1.97.0 ; MSRV workspace 1.86 (`Cargo.toml:18`) |
| En-têtes `-dev` | absents (`pkg-config` échoue) ; installables |
| AppIndicator | `gnome-shell-extension-appindicator` 42-2~fakesync1, **activée** sous `ubuntu-appindicators@ubuntu.com` |
| VRAM bureau | ~975–1 005 Mio utilisés (Xorg, GNOME, Firefox, Antigravity, EasyEffects) |
| Firefox | ~1 030 Mio RSS (processus complet, tous onglets) |

### 4.2 Test de rendu WebKitGTK

Protocole : `MiniBrowser` 2.50.4 (même moteur que Tauri), page locale dans
`/tmp/opencode`, profils listés ci-dessous, capture d'écran par `import`, analyse
`identify` (moyenne, écart-type, nombre de couleurs), contrôle du processus et
des erreurs stderr.

| Profil | Page | Vivant après 6 s | Capture | RSS MiniBrowser |
| --- | --- | --- | --- | --- |
| Défaut | dégradé + texte | oui | moyenne 0,419 / écart-type 0,211 / 6 334 couleurs | 130 Mio |
| `DISABLE_DMABUF_RENDERER=1` | dégradé + texte | oui | identique | 125 Mio |
| `DISABLE=0 FORCE_SHM=1` | dégradé + texte | oui | identique | 130 Mio |
| Défaut | calque `translateZ(0)` + animation | oui | moyenne 0,168 / écart-type 0,198 / 1 416 couleurs | 130 Mio |
| `DISABLE=1` | calque + animation | oui | identique | 122 Mio |
| `DISABLE=0 FORCE_SHM=1` | calque + animation | oui | identique | 130 Mio |
| `FORCE_DMABUF=1` | calque + animation | oui | **contenu vide**, moyenne 0,173 / écart-type 0,028 / 664 couleurs ; stderr `AcceleratedSurface...` | — |
| `FORCE_DMABUF=1 FORCE_SHM=1` | calque + animation | oui | texte seul, **calque gradient absent**, moyenne 0,100 / écart-type 0,127 / 1 231 couleurs | — |

Test supplémentaire, le plus proche du déclencheur décrit par l'issue amont :
cinq bascules d'espace de travail X11 (aller-retour 0 → 1 → 0), page avec
calque de composition, pour tous les profils. **Aucun crash** ; les profils
DMA-BUF forcés sont visuellement cassés (page vide ou calque absent), les
profils par défaut, `DISABLE=1` et `FORCE_SHM` rendent correctement et restent
identiques entre eux.

Le patch Ubuntu lui-même (récupéré depuis Launchpad, branche
`ubuntu/jammy-updates`) est plus précis que le changelog : `isNVIDIA()` fait un
`return` **avant** `mode.add(RendererBufferTransportMode::SharedMemory)` et
**avant** la lecture de `WEBKIT_DMABUF_RENDERER_FORCE_SHM`. Autrement dit,
`FORCE_SHM` est inopérant sur cette build, et le seul moyen de forcer le chemin
DMA-BUF est `WEBKIT_FORCE_DMABUF_RENDERER=1` (variable lue par `isNVIDIA()`).
Le build 2.50.4 gère néanmoins le mode vide sans crash dans nos tests ; le PoC
doit donc ajouter le profil `WEBKIT_FORCE_DMABUF_RENDERER=1` pour tester le
chemin réellement risqué, au lieu de croire que `DISABLE=0 FORCE_SHM=1` le
force.

Interprétation : sur ce couple Ubuntu 22.04 / WebKitGTK 2.50.4 / NVIDIA 535,
le patch `disable-nvidia-dmabuf.patch` empêche le chemin DMA-BUF de se
manifester par défaut. Le risque R1 est néanmoins **réel et reproduit** dès
qu'on force ce chemin : page vide et erreurs
`AcceleratedSurface was unable to construct a complete framebuffer` avec
`FORCE_DMABUF=1`, calque de composition absent avec
`FORCE_DMABUF=1 + FORCE_SHM=1`. C'est une **bonne nouvelle pour Tauri
A-Strict** sur le chemin de production actuel, mais ce n'est pas une preuve de
stabilité : le test dure quelques secondes, utilise `MiniBrowser` et non une
application Tauri, et ne couvre ni l'UI complète, ni 2 h de polling, ni les
changements de thème. Le profil par défaut doit être le mode livré ; le profil
`FORCE_DMABUF=1` doit rester un canari de régression.

### 4.3 Empreinte

| Mesure | Valeur | Commentaire |
| --- | --- | --- |
| RSS total MiniBrowser + `WebKitWebProcess` + `WebKitNetworkProcess` | **305 Mio** (130 + 122 + 53) | Page statique ; cohérent avec la fourchette E3 180–350 Mio. |
| VRAM totale avant/après | ~1 000 Mio, variations ±10 Mio | Pas de delta mesurable à cette granularité. |
| `nvidia-smi --query-compute-apps` | vide | WebKit est un contexte **graphique**, pas compute : la commande du PoC E3 ne le verra pas. |
| `nvidia-smi pmon` | `MiniBrowser` en type G | `WebKitWebProcess` n'apparaît pas dans `pmon`. |
| `fuser /dev/nvidia0` | `MiniBrowser` **et** `WebKitWebProcess` | Le sous-processus WebKit ouvre bien le périphérique NVIDIA : Tauri n'est donc **pas structurellement** à 0 Mio, même avec `FORCE_SHM=1` ; l'allocation réelle reste à mesurer. |
| Firefox (baseline) | ~1 030 Mio RSS | Le statut quo est un navigateur, pas une app native : c'est la référence pertinente. |

Contrôle GTK4 (sonde Python GI minimale, GTK 4.6.9, fenêtre 800×600) :

| Profil GTK4 | RSS | `nvidia-smi pmon` | `fuser /dev/nvidia0` |
| --- | --- | --- | --- |
| `GSK_RENDERER=cairo` | 75 Mio | processus absent (pas de contexte G) | présent |
| défaut (GL) | 146 Mio | processus présent (type G) | présent |

Le renderer Cairo évite bien le **contexte GPU** de rendu (`pmon` ne voit pas
le processus), ce qui rend crédible l'argument « 0 Mio » de E3. Mais le
processus ouvre quand même `/dev/nvidia0` (probablement pour l'énumération
d'affichage) : `fuser` seul produirait un faux positif. La preuve correcte est
`pmon` + VRAM totale sous baseline contrôlée, pas `fuser` isolé. Le RSS Cairo
(75 Mio) est cohérent avec la fourchette 60–120 Mio de E3.

### 4.4 Autres vérifications

- **Polling** : `SYSTEM_MONITOR_POLLING_INTERVAL = 5000` ms
  (`assets/web/static/main.js:175`) ; le polling de statut est
  `POLLING_INTERVAL = 500` ms (`assets/web/static/constants.js:1`). E3
  confond les deux.
- **Logs** : `MAX_STEP_LOG_LINES = 300` (`crates/workflow-server/src/state.rs:20`).
- **CSP** : `default-src 'self'; script-src 'self' 'unsafe-inline'; ...;
  connect-src 'self'; frame-ancestors 'none'` (`http.rs:129`). Une WebView
  Tauri charge l'URL en document top-level : `frame-ancestors` ne la bloque
  pas.
- **Token** : injecté dans le HTML par Minijinja
  (`assets/web/templates/index_new.html:10`, `http.rs:165`). Un hôte Rust
  A-Strict qui veut interroger l'API doit donc récupérer ce token (lire `/`
  et parser la balise, ou lire la configuration serveur). E3 ne traite pas ce
  point.
- **`/api/system_monitor`** : route `http.rs:82`, handler `http.rs:411`,
  helper GPU `http.rs:676`. E3 cite `614-696` ; la plage est approximative.
- **MSRV** (crates.io, 2026-10-24) : `tauri 2.12.0` = 1.90 ;
  `relm4 0.11.0` = 1.93 ; `relm4 0.10.1` = **1.92** ; `relm4 0.10.0` =
  1.85 ; `gtk4 0.11.5` = **1.92** ; `gtk4 0.10.3` = 1.83.
- **AppIndicator** : l'extension Ubuntu est
  `ubuntu-appindicators@ubuntu.com`, pas
  `appindicatorsupport@rgcjonas.gmail.com`. La commande de surveillance de E3
  produirait un faux négatif sur cette machine.
- **`gtk4-demo`** : absent (`gtk-4-examples` non installé) ; le test 4 de E3
  échoue tel quel.

## 5. Erreurs et imprécisions

| # | Affirmation E3 | Réalité vérifiée | Gravité | Correction |
| --- | --- | --- | --- | --- |
| 1 | `/api/system_monitor` tourne à 2 Hz (500 ms) | 5 000 ms ; le statut d'étape est à 500 ms | Majeur (mesure) | Distinguer les deux endpoints ; le gate VRAM peut garder 2 Hz pour `/status`, pas pour le moniteur. |
| 2 | `gtk4 0.11.5` MSRV 1.83 ; `relm4 0.10.1` MSRV 1.85 | 1.92 et 1.92 ; les versions 1.83/1.85 sont `gtk4 0.10.3` et `relm4 0.10.0` | Majeur (repli) | Corriger les versions de repli ; ne pas promettre une MSRV 1.86 compatible avec ces versions. |
| 3 | `DISABLE_DMABUF_RENDERER=1` provoque un SIGSEGV sur 2.50+ | Bug amont réel (`block/buzz#3654`, ouvert) ; **non reproduit** localement sur le chemin par défaut, y compris après bascules ; en forçant `WEBKIT_FORCE_DMABUF_RENDERER=1`, le contenu devient vide avec des erreurs `AcceleratedSurface` | Majeur (risque) | Conserver le risque amont ; livrer le profil par défaut patché et garder `FORCE_DMABUF=1` comme canari. |
| 4 | La VRAM WebKit se mesure avec `--query-compute-apps` | La commande ne liste que les contextes compute ; WebKit est graphique ; `pmon`/`fuser` le voient, pas `--query-compute-apps` | Majeur (PoC invalide) | Mesurer la VRAM totale, `pmon`, `fuser` et `/proc/<pid>/fd` ; baseline contrôlée. |
| 5 | 10 000 lignes de logs justifient GTK4 dès le Lot 1 | Le serveur borne à 300 lignes ; 10 000 lignes exigent SSE ou lecture de fichier, indépendamment du toolkit ; Tauri peut virtualiser aussi | Majeur (logique) | Traiter les 10 000 lignes comme une décision **serveur** séparée ; ne pas l'utiliser comme gate toolkit. |
| 6 | L'extension tray à vérifier est `appindicatorsupport@rgcjonas.gmail.com` | Ubuntu utilise `ubuntu-appindicators@ubuntu.com`, activée ici | Mineur | Utiliser `gnome-extensions list --enabled \| grep -i appindicator` ou l'ID Ubuntu. |
| 7 | E1 contient encore l'erreur `GtkFileLauncher` | E1 v1.1 l'a déjà remplacée par `gio::AppInfo`/`xdg-open` | Mineur (obsolète) | E3 audite E1 v1.0 ; mettre à jour la référence. |
| 8 | WebKitGTK expose AT-SPI mais « dégradé par rapport à Chromium » | Source faible (site agrégateur), aucun test Orca local ; l'UI actuelle a déjà aria-live et focus trap | Majeur (non vérifié) | Tester Orca ; faire d'AT-SPI une exigence seulement si l'opérateur l'utilise. |
| 9 | `GSK_RENDERER=cairo` garantit 0 Mio de VRAM | Sonde GTK4 : cairo 75 Mio RSS, pas de contexte G dans `pmon` ; GL 146 Mio, contexte G. Le processus ouvre quand même `/dev/nvidia0` ; « 0 Mio » est crédible au niveau contexte, pas prouvé au niveau device | Moyen | Qualifier la claim ; mesurer la VRAM totale dans le PoC GTK4. |
| 10 | `rodio` garantit 100 % de lecture audio | Supprime l'autoplay, mais dépend du sink PipeWire/ALSA | Mineur | Dire « indépendant de l'autoplay WebView », pas « 100 % ». |
| 11 | PoC de 5 à 10 jours | Le tableau J0–J9 couvre 9 jours | Mineur | Aligner le titre et le tableau. |
| 12 | Test `GSK_RENDERER=cairo gtk4-demo` | `gtk4-demo` absent | Mineur | Installer `gtk-4-examples` ou écrire une sonde GTK4 minimale. |
| 13 | A-Strict : l'hôte Rust interroge `:5004` | Le token est dans le HTML (`index_new.html:10`) ; l'hôte doit le récupérer ou lire la config | Majeur (design) | Définir un chemin de session/token propre ; ne pas lire les secrets serveur depuis le GUI. |
| 14 | Commandes `pmap`/`pgrep` du PoC | `pmap` ne prend qu'un PID ; `pgrep -d, -f "workflow-gui\|WebKit"` utilise un `\|` littéral en ERE ; `nvidia-smi` par défaut ne montre pas WebKit | Moyen | Sommer explicitement les PID ; utiliser `pmon` et `fuser`. |
| 15 | Bibliographie « sources primaires vérifiées » | Plusieurs entrées sont hors sujet (Reddit Lutris, AUR smoothcsv, blueman, gKeyMapper) ou faibles (agrégateurs, blogs) | Majeur (confiance) | Ne garder que les sources citées et primaires ; baisser les niveaux de confiance. |
| 16 | Seuils de décision « falsifiables » | Les glyphes LaTeX sont cassés (`\le 2\text{`, `100\text{`, `< 5\text{`) | Moyen | Réécrire les seuils en texte brut. |
| 17 | Le graphique note Slint 4,20 et egui 3,78 | Le texte ne les analyse pas ; seul Iced est disqualifié | Moyen | Analyser ou retirer ces candidats ; sinon le classement induit en erreur. |
| 18 | R1 60–75 %, R2 35–50 % | Probabilités a priori, non mesurées ; le test local suggère R1 plus bas sur le chemin par défaut, mais le reproduit sur le chemin DMA-BUF forcé | Moyen | Étiqueter comme priors ; réviser après PoC. |
| 19 | « 14 erreurs d'E2 » | Reprise de C, sans nouvel audit du code | Moyen (circulaire) | Citer C ; ne pas présenter comme une vérification indépendante. |
| 20 | `/api/system_monitor` à `http.rs:614-696` | Handler à 411, GPU à 676–696 | Mineur | Citer les lignes exactes. |

## 6. Critiques méthodologiques

### 6.1 Sources et confiance

E3 mélange des sources primaires (issues GitHub, docs Tauri, docs.rs,
changelog Ubuntu) avec des agrégateurs et des blogs. Certaines affirmations
décisives sont étiquetées « Vérifié / Élevée » alors que la source est un site
de contenu (`skillsdirectory.com`) ou un blog personnel. La bibliographie
contient des entrées non citées et hors sujet, ce qui donne une impression de
volume plutôt que de traçabilité. **Règle proposée** : pour chaque affirmation
qui change la décision, exiger une source primaire datée ou une mesure locale ;
sinon la classer « à vérifier ».

### 6.2 Circularité

E3 reprend les faits de C (code à HEAD, 300 lignes, 2 Hz, code mort
`POLLING_INTERVAL_HIGH_FREQUENCY`, `clear_disk_cache`) et les présente comme
une vérification. Ce n'est pas un nouvel audit : c'est une synthèse. Les
chiffres de code doivent rester attribués à C, et E3 doit être lu comme une
veille externe, pas comme une seconde lecture du dépôt.

### 6.3 Seuils absolus contre baseline

Les gates « VRAM ≤ 50 Mio » et « RSS ≤ 300 Mio » sont arbitraires. Le statut
quo est Firefox : ~1 030 Mio de RSS et un contexte GPU graphique. Un Tauri à
305 Mio de RSS est déjà **meilleur en RAM** que le statut quo, même s'il
échoue au seuil absolu de E3 ; son allocation VRAM réelle reste à mesurer. La
bonne question n'est
pas « le GUI alloue-t-il 0 Mio ? » mais « le GUI ajoute-t-il assez de VRAM
pour faire échouer STEP2, alors que Firefox ne le fait pas ? ». Le gate doit
être : **aucun OOM CUDA sur N runs, VRAM totale < 3 900 Mio, RSS total ≤ 2×
la baseline Firefox**. Le seuil de 3 900 Mio garde 196 Mio de marge sur 4 096 ;
il est déjà dans le rapport, mais il est contredit par le gate 50 Mio.

### 6.4 Les 10 000 lignes ne sont pas un argument toolkit

Le serveur renvoie au maximum 300 lignes. Pour afficher 10 000 lignes, il faut
changer le serveur (SSE, pagination ou lecture de fichier). Une fois ce flux
disponible, Tauri peut virtualiser la liste en JS aussi bien que GTK4 avec
`GtkListView`. Utiliser ce besoin comme discriminateur revient à faire payer à
GTK4 une fonctionnalité qui manque aux deux. Si l'inspection du log complet
est contractuelle, c'est un lot serveur à part, à décider avant le choix du
toolkit.

### 6.5 Notation et probabilités

Les scores 4,60/4,00/4,20 ne sont pas reproductibles : les poids des critères
autres que « effort » ne sont pas donnés, et Slint/egui apparaissent dans le
graphique sans analyse textuelle. Les probabilités R1/R2 sont des priors
d'auteur, pas des mesures. Elles sont utiles pour prioriser les tests, pas pour
trancher.

### 6.6 Plan de test

Le plan E3 est bon dans son principe, mais :

- le scénario A1 crée bien `workflow-gui-tauri`, mais les commandes de mesure
  fournies sont celles de `nvidia-smi` compute, inadaptées à WebKit ;
- il ne mesure pas la baseline Firefox avant le PoC ;
- il ne teste pas l'UI complète (8 cartes, overlay, thèmes) ;
- il ne teste pas Orca avec l'UI réelle ;
- il ne teste pas le token A-Strict ;
- il ne teste pas la fermeture vers le tray avec l'extension Ubuntu réelle ;
- il ne teste pas la reprise après crash du GUI pendant une séquence.

## 7. Impact sur la décision

### 7.1 Ce qui ne change pas

- **Modèle 1** : GUI client, serveur headless, GUI jamais parent.
- **Tauri A-Strict** : URL distante, zéro capability remote, fonctions
  bureau côté Rust.
- **GTK4/Relm4** : repli natif, 70–95 j-h, `GSK_RENDERER=cairo`,
  `gnome_42`, GTK 4.6.
- **Rejet de C2** : la coque GTK4 + WebKitGTK 6.0 ne protège pas le
  sous-processus WebKit.
- **C3** : crate `workflow-desktop-services` partagé, à créer dès le jour 1.

### 7.2 Ce qui change

1. **Le PoC Tauri passe en premier**, 5 jours, pas un PoC à deux pistes de
   15–20 jours. Le test local ne justifie pas de financer GTK4 en parallèle
   tant que Tauri n'a pas échoué sur une porte mesurable.
2. **R1 est reclassé** : le chemin par défaut patché est stable ; le chemin
   DMA-BUF forcé est cassé (page vide, `AcceleratedSurface`). Le risque
   immédiat est faible, mais le PoC garde `FORCE_DMABUF=1` comme canari et le
   profil par défaut comme mode livré. Un écran blanc/SIGSEGV sur le profil
   par défaut reste un critère d'arrêt. Le risque résiduel est un risque de
   **déploiement** : le patch est propre à Ubuntu 22.04 ; sur une autre
   distribution ou après un changement de version WebKitGTK, le chemin
   DMA-BUF peut redevenir actif.
3. **La VRAM devient un critère relatif** : aucun OOM CUDA sur 5 runs de
   STEP2 et VRAM totale < 3 900 Mio, plutôt qu'un seuil absolu de 50 Mio.
4. **Les 10 000 lignes sortent du gate toolkit** : si nécessaire, lot serveur
   SSE/fichier, décidé séparément.
5. **AT-SPI devient conditionnel** : exigence seulement si l'opérateur
   utilise Orca quotidiennement ; sinon, test de non-régression, pas critère
   de choix.
6. **La MSRV est traitée par crate** : `workflow-gui` peut déclarer
   `rust-version = "1.93"` (Relm4 0.11) ou `"1.90"` (Tauri 2.12) sans changer
   la MSRV 1.86 des neuf crates existantes. Si la politique interdit toute
   exception, le repli correct est `gtk4 0.10.3` (1.83) + `relm4 0.10.0`
   (1.85), pas les versions citées par E3.
7. **Le token A-Strict doit être spécifié** : soit le GUI lit `/` et parse
   la balise `worker-token`, soit le serveur expose une route de session
   dédiée. Lire `config/config.toml` depuis le GUI est à éviter (couplage aux
   secrets).

### 7.3 Règle de décision consolidée

| Situation | Décision |
| --- | --- |
| Le PoC Tauri passe les portes 2.1–2.5 corrigées | Tauri A-Strict, MVP 12–18 j, puis durcissement. |
| Écran blanc/SIGSEGV reproductible sur le profil par défaut, ou OOM CUDA sur STEP2 | GTK4/Relm4, 70–95 j, avec C3 déjà en place. |
| Canari `FORCE_DMABUF=1` cassé alors que le profil par défaut est stable | Ne pas forcer DMA-BUF ; verrouiller/surveiller WebKitGTK ; pas de bascule GTK4 immédiate. |
| Besoin contractuel de widgets natifs, d'AT-SPI direct ou de 0 Mio de VRAM | GTK4/Relm4 directement, sans PoC Tauri. |
| Besoin de 10 000 lignes de logs dès le Lot 1 | Lot serveur SSE/fichier d'abord ; le toolkit n'est pas le facteur limitant. |
| MSRV 1.86 non négociable pour tous les crates | `gtk4 0.10.3` + `relm4 0.10.0` (1.83/1.85), ou Tauri 2.12 si la règle accepte 1.90 pour le seul crate GUI. |

## 8. PoC révisé (5 jours)

> **Sécurité d'exécution** : tous les runs utilisent un projet de test et des
> répertoires dédiés sous `/tmp/opencode` (`--work-dir`, `--output-dir`),
> jamais `runtime/`, jamais `/mnt/cache`, jamais le serveur de production avec
> le webhook actif. Utiliser une copie de configuration avec
> `webhook.enabled = false` et, si nécessaire, un port de test distinct.

### 8.1 J1 — Socle et baseline

- Créer `crates/workflow-desktop-services` : client HTTP typé
  (`X-Worker-Token`, 409, `stopping`), `VecDeque<LogEntry>` avec coalescence
  16 ms, acteur audio `rodio`, notifications `notify-rust`, télémétrie
  (`/mnt/cache`, `/dev/apex_0`, verrou par `fcntl(F_GETLK)`), tray D-Bus.
- Créer `workflow-gui-tauri` minimal : fenêtre sur `http://127.0.0.1:5004/`,
  **aucune capability remote**, aucune injection IPC.
- Mesurer la baseline : Firefox avec l'UI actuelle ouverte, STEP2 non lancé,
  puis STEP2 lancé sur un projet de test isolé ; RSS, VRAM totale, `pmon`,
  `fuser`.

### 8.2 J2 — Stabilité

- 20 lancements/fermetures, 5 bascules d'espace de travail par lancement,
  quatre profils : défaut, `WEBKIT_DISABLE_DMABUF_RENDERER=1`,
  `WEBKIT_FORCE_DMABUF_RENDERER=1`, et
  `WEBKIT_FORCE_DMABUF_RENDERER=1 WEBKIT_DMABUF_RENDERER_FORCE_SHM=1`.
  Le profil par défaut est le mode livré ; `FORCE_DMABUF=1` est un canari,
  pas une option de production.
  **Mise à jour du 2026-10-25** : le protocole J2 ratifié porte sur
  35 lancements au total et 25 balayages de 5 bureaux GNOME (125 bascules).
- 2 h de polling à 2 Hz sur `/status/<step>` avec l'overlay ouvert.
- Critère d'échec : tout écran blanc, écran noir ou SIGSEGV reproductible sur
  le profil par défaut ; erreur DRM/GBM dans stderr.

### 8.3 J3 — STEP2 et VRAM

- 5 runs consécutifs de STEP2 (CUDA TransNetV2, ~2 203 Mio) sur un projet de
  test isolé, overlay ouvert, polling 2 Hz.
- Mesurer : `nvidia-smi --query-gpu=memory.used --format=csv -l 1`,
  `nvidia-smi pmon -c 1`, `fuser /dev/nvidia0`, RSS total des processus
  `workflow-gui`, `WebKitWebProcess`, `WebKitNetworkProcess`.
- Critère d'échec : `CUDA_ERROR_OUT_OF_MEMORY`, ou VRAM totale > 3 900 Mio
  pendant STEP2.
- Ne pas utiliser `--query-compute-apps` comme mesure principale : il ne voit
  pas les contextes graphiques.

### 8.4 J4 — Bureau, audio, accessibilité

- Tray : icône visible sous GNOME 42, fermeture vers le tray, restauration.
- Audio : STEP4 terminé sans geste utilisateur, MP3 joué par `rodio`.
- Notifications : fin d'étape, échec, arrêt.
- Orca : aria-live, focus trap de l'overlay, annonces de statut ; comparer
  avec Firefox.
- Token : démarrage du GUI sans lire `config/config.toml` (parser `/` ou
  route dédiée).
- Séquence : lancer une séquence de test, fermer le GUI, vérifier que le
  serveur continue et que la réouverture resynchronise.

### 8.5 J5 — Décision

- ADR binaire : Tauri A-Strict, GTK4, ou C3 + GTK4.
- Si Tauri échoue : démarrer GTK4 avec le crate partagé déjà écrit ; ne pas
  repartir de zéro.
- Consigner les mesures, les seuils et les écarts dans
  `memory-bank/decisionLog.md`.

## 9. Questions ouvertes

1. **« Widgets natifs » : exigence ou confort ?** C'est l'arbitrage qui décide
   entre Tauri et GTK4. Sans réponse explicite, Tauri reste le choix
   économique.
2. **Orca est-il utilisé quotidiennement ?** Si non, AT-SPI est un test de
   non-régression, pas un critère de choix.
3. **Les 10 000 lignes de logs sont-elles nécessaires dès le Lot 1 ?** Si oui,
   décider SSE/fichier côté serveur avant le toolkit.
4. **La VRAM est-elle réellement un problème ?** Le statut quo est un
   navigateur ; STEP2 tourne déjà avec le bureau chargé (batch 8/8 du
   2026-10-24). Mesurer avant de financer GTK4 pour 0 Mio.
5. **La MSRV 1.86 est-elle une règle pour tous les crates ou pour le cœur ?**
   Une exception pour `workflow-gui` est la solution la moins coûteuse.
6. **Le tray est-il contractuel ?** L'extension est active ici, mais pas
   forcément sur une session GNOME Vanilla.
7. **Supervision serveur** : `switch-rust.sh start` pour le PoC, systemd user
   pour le MVP.
8. **Séquences côté serveur** : ratifier `POST /run_custom_sequence`,
   `/sequence/status`, `/sequence/stop` et ajouter le test « GUI fermé,
   séquence continue ».

## Annexe A — Commandes et sorties locales

```sh
# Versions
dpkg-query -W -f='${Package} ${Version}\n' \
  libwebkit2gtk-4.1-0 libgtk-4-1 libadwaita-1-0 \
  gnome-shell-extension-appindicator
# libwebkit2gtk-4.1-0 2.50.4-0ubuntu0.22.04.1
# libgtk-4-1 4.6.9+ds-0ubuntu0.22.04.2
# libadwaita-1-0 1.1.7-0ubuntu0.22.04.1
# gnome-shell-extension-appindicator 42-2~fakesync1

# AppIndicator
gnome-extensions list --enabled | grep -i appindicator
# ubuntu-appindicators@ubuntu.com

# WebKitGTK 6.0
apt-cache policy libwebkitgtk-6.0-4 libwebkitgtk-6.0-dev
# candidat 2.50.4-0ubuntu0.22.04.1 (jammy-updates, jammy-security)

# Patch Ubuntu
zgrep -n 'nvidia\|dmabuf' \
  /usr/share/doc/libwebkit2gtk-4.1-0/changelog.Debian.gz
# 2.46.6 : debian/patches/disable-nvidia-dmabuf.patch: updated
# 2.48.0 : d/p/disable-nvidia-dmabuf.patch: synced from plucky

# Extrait du patch (Launchpad, ubuntu/jammy-updates)
#   if (isNVIDIA())
#       return;
#   mode.add(RendererBufferTransportMode::SharedMemory);
#   const char* forceSHM = getenv("WEBKIT_DMABUF_RENDERER_FORCE_SHM");
# => FORCE_SHM est lu après le retour NVIDIA ; il est inopérant sur Ubuntu.

# Rendu
WEBKIT_DISABLE_DMABUF_RENDERER=1 \
  /usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/MiniBrowser \
  file:///tmp/opencode/e3-test.html
# processus vivant, capture non blanche, aucun message DRM/GBM

# Bascules d'espace de travail (5 aller-retours)
python3 /tmp/opencode/switch_ws.py 1
sleep 0.6
python3 /tmp/opencode/switch_ws.py 0
# aucun crash sur les profils par défaut, DISABLE=1 et FORCE_SHM ;
# DMA-BUF forcé : contenu vide / calque absent

# Empreinte
ps -o pid,comm,rss -p $(pgrep -f '^/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/')
# MiniBrowser 133 368 Kio, WebKitNetworkProcess 54 296 Kio,
# WebKitWebProcess 124 856 Kio (total 312 520 Kio ≈ 305 Mio)
nvidia-smi --query-compute-apps=pid,process_name,used_memory --format=csv
# vide : WebKit est un contexte graphique
nvidia-smi pmon -c 1
# MiniBrowser type G ; WebKitWebProcess absent
fuser /dev/nvidia0
# MiniBrowser et WebKitWebProcess présents

# Sonde GTK4 (Python GI, fenêtre 800x600)
GSK_RENDERER=cairo python3 /tmp/opencode/gtk4_probe.py &
# cairo : 75 Mio RSS, absent de nvidia-smi pmon, /dev/nvidia0 ouvert
python3 /tmp/opencode/gtk4_probe.py &
# GL : 146 Mio RSS, présent dans pmon (type G), /dev/nvidia0 ouvert
```

## Annexe B — Sources externes vérifiées

| Source | Date | Ce qu'elle établit |
| --- | --- | --- |
| `block/buzz#3654` | ouvert 2026-07-30, MAJ 2026-09-21 | `WEBKIT_DISABLE_DMABUF_RENDERER=1` retourne avant `SharedMemory`, `AcceleratedBackingStore::create()` nul, SIGSEGV au premier calque de composition ; bascule d'espace de travail reproductible. |
| `webkit2gtk-nvidia-quirk` 2.3.0 | 2026-10-20 | Workarounds DMA-BUF/NVIDIA selon session ; confirme le contexte amont. |
| crates.io `tauri 2.12.0` | 2026-09-26 | MSRV 1.90. |
| crates.io `relm4 0.11.0` | 2026-04-08 | MSRV 1.93 ; feature `gnome_42` par défaut. |
| crates.io `gtk4 0.11.5` | 2026-09-20 | MSRV 1.92 ; `gtk4 0.10.3` = 1.83. |
| crates.io `relm4 0.10.1` | 2025-12-29 | MSRV 1.92 ; `relm4 0.10.0` = 1.85. |
| Changelog Debian `libwebkit2gtk-4.1-0` | 2025-03-24 et 2025-04-07 | `disable-nvidia-dmabuf.patch` maintenu dans Ubuntu 22.04. |
| Patch `disable-nvidia-dmabuf.patch` (Launchpad, `ubuntu/jammy-updates`) | consulté 2026-10-24 | `isNVIDIA()` retourne avant `SharedMemory` et avant `FORCE_SHM` ; `WEBKIT_FORCE_DMABUF_RENDERER=1` force le chemin DMA-BUF. |
| `apt-cache policy` | 2026-10-24 | `libwebkitgtk-6.0-4`/`-dev` 2.50.4 disponibles. |
