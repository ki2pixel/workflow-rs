# Confrontation des deux études GUI Linux

| Champ | Valeur |
| --- | --- |
| Version | 1.0 |
| Date | 2026-10-24 |
| Objet | Comparer `docs/ETUDE-GUI-NATIVE-LINUX.md` (E1) et `docs/audits/Migration-Architecture-GUI-Linux-workflow-rs.md` (E2) |
| Méthode | Relecture contradictoire, vérification à HEAD, mesures locales, aucun code modifié |
| Statut | Résultat de confrontation ; recommandation consolidée soumise à arbitrage |

## 1. Synthèse en dix lignes

1. **Les deux études convergent sur l'essentiel** : modèle client/serveur
   découplé, `workflow-server` intact et autoritaire, pas de fusion
   orchestrateur/GUI, pas de CUDA/TPU dans le processus graphique.
2. **Elles divergent sur la technologie** : E1 recommande Tauri v2 (pragmatique,
   réutilisation), E2 recommande GTK4/Relm4 (natif, VRAM nulle, logs).
3. **Le désaccord vient d'une ambiguïté de exigence**, pas d'un fait : que
   signifie « natif » ? Application de bureau (Tauri convient) ou widgets natifs
   (GTK4) ?
4. **E2 apporte trois arguments techniques réels** que E1 sous-estimait :
   `GSK_RENDERER=cairo` pour 0 Mio de VRAM, NVML direct sans `fork`, et la
   conception ring buffer + coalescence + liste virtualisée.
5. **E2 contient cinq affirmations contredites par le code** : `/api/system_monitor`
   serait absent de Rust, le frontend interrogerait `/sequence/status`, le
   polling tournerait à 200 ms/5 req/s, `clear_disk_cache` serait dans
   `apiService.js`, et les tokens CSS GTK seraient réutilisables tels quels.
6. **E2 sous-estime massivement l'effort** : 4 semaines pour une parité GTK4
   complète contre 70–95 jours dans E1. Un PoC GTK4 limité vaut déjà 8–10 jours.
7. **E2 surestime la pression sur les logs** : le serveur borne à 300 lignes et
   la production mesure 5,4 lignes/s ; les 10 000/50 000 lignes sont une
   **nouvelle fonction** (SSE ou lecture directe des fichiers), pas l'existant.
8. **E2 sous-évalue l'accessibilité d'Iced** (4/5 alors que l'issue AT-SPI
   #552 est ouverte) ; Iced n'est pas un repli acceptable pour cette UI.
9. **E1 contient une erreur à corriger** : `GtkFileLauncher` exige GTK 4.10,
   absent d'Ubuntu 22.04 (GTK 4.6) ; utiliser `gio::AppInfo` ou `xdg-open`.
10. **Recommandation consolidée** : architecture modèle 1 (ferme) ; technologie
    tranchée par une règle de décision, avec un PoC à deux pistes si le budget
    le permet. Aucun des deux documents ne peut revendiquer un gagnant unique
    sans arbitrer l'exigence « widgets natifs ».

## 2. Ce sur quoi les deux études sont d'accord

| Point | E1 | E2 | Statut |
| --- | --- | --- | --- |
| Topologie | Client GUI → `workflow-server` | Topologie A | **Consensus** |
| Binaire monolithique | Rejeté | Rejeté | **Consensus** |
| Serveur headless | Conservé | Conservé | **Consensus** |
| Webhook/ingestion | Restent dans le serveur | Restent dans le serveur | **Consensus** |
| Verrou matériel | Jamais pris par la GUI | Jamais pris par la GUI | **Consensus** |
| CUDA/TPU | Jamais dans la GUI | Jamais dans la GUI | **Consensus** |
| `/mnt/cache` | Aucune écriture | Aucune écriture | **Consensus** |
| Jeton `X-Worker-Token` | Requis pour les mutations | Requis pour les mutations | **Consensus** |
| 409 et `stopping` | Affichés jusqu'à la mort réelle | Affichés jusqu'à la mort réelle | **Consensus** |
| Séquence | Recommande le serveur | Utilise `/run_custom_sequence` | **Consensus** |
| Logs | Buffer borné + rendu incrémental | Ring buffer + coalescence + liste virtualisée | **Consensus sur le principe** |
| Notifications | `notify-rust` / `Gio::Notification` | `Gio::Notification` / `notify-rust` | **Consensus** |
| Son | Web Audio ou `rodio` | `rodio` + canal borné | **Consensus sur `rodio` en natif** |
| Packaging | `.deb` + systemd user | `.deb` + systemd user | **Consensus** |
| Navigateur de secours | Conservé | Conservé | **Consensus** |
| Serveur non parent de la GUI | Exigé | Implicite (démon séparé) | **Consensus** |

Le désaccord n'est donc **pas architectural**. Il porte sur le choix du toolkit,
le poids relatif de l'effort, et le périmètre du premier lot.

## 3. Divergences structurantes

### 3.1 Framework : Tauri vs GTK4

#### Les deux positions

- **E1** : Tauri v2 en mode URL distante, 12–18 j de MVP, réutilisation ~95 %,
  aucune modification serveur/frontend. GTK4 est l'option native de repli,
  70–95 j.
- **E2** : GTK4/Relm4, 4,78/5 contre 3,25/5 pour Tauri, avec Iced en repli.
  Arguments : 0 Mio de VRAM via Cairo, `GtkListView`/`GtkTextView`, AT-SPI,
  notifications natives, CSS réutilisable.

#### Vérification des arguments

| Argument E2 | Vérification | Verdict |
| --- | --- | --- |
| `GSK_RENDERER=cairo` ⇒ 0 Mio VRAM | Vrai : le renderer Cairo est logiciel, pas de buffers GL de l'application | **Valide, à adopter** |
| `GtkListView` virtualise | Vrai depuis GTK 4.0 | **Valide** |
| `GtkTextView` + `TextTag` pour les logs | Vrai ; sélection/copie natives | **Valide** |
| AT-SPI natif | Vrai ; GTK4 est la référence GNOME | **Valide** |
| CSS réutilisable « quasi directement » | Faux sur GTK 4.6 : `var()` n'existe qu'à partir de 4.16 ; la mise en page CSS web (flex/grid) n'existe pas dans GTK | **À corriger** |
| `GtkFileLauncher` pour `xdg-open` | Faux sur GTK 4.6 : API depuis 4.10 | **À corriger (aussi dans E1)** |
| GTK4 ~35–60 Mio RSS | Optimiste mais plausible pour une app minimale ; 60–120 Mio plus réaliste avec libadwaita et le modèle applicatif | **À mesurer** |
| Tauri 180–350 Mio RSS | Plausible (WebKitWebProcess + NetworkProcess) | **Valide** |
| Tauri 100 % fidélité visuelle | Surestimé : le rendu est WebKitGTK, pas Gecko ; polices et détails CSS peuvent différer | **Nuancer** |
| Iced 4/5 accessibilité | Faux : aucune exposition AT-SPI, issue #552 ouverte | **Erreur bloquante pour un repli** |
| Effort GTK4 4/5 | Faux : 70–95 j contre 12–18 j pour Tauri, soit un facteur 4–6 | **Erreur majeure** |

#### Matrice consolidée, sans total unique trompeur

Notes de 1 à 5. Les deux scénarios ci-dessous utilisent les mêmes notes mais des
poids différents ; ils montrent que **le classement dépend de la pondération**,
pas d'une supériorité absolue.

Notes retenues après vérification :

| Critère | Tauri | Slint | Iced | GTK4 | egui |
| --- | :---: | :---: | :---: | :---: | :---: |
| Effort / réutilisation | **5** | 3 | 2,5 | 2 | 4 |
| Performance / réactivité | 3 | **5** | 4,5 | **5** | 4,5 |
| Empreinte RAM/VRAM/stabilité | 2 | 4,5 | 4 | **5** | 4 |
| Logs / virtualisation | 2,5 | 3 | 3 | **5** | 4,5 |
| Fidélité UX + accessibilité | **5** | 4 | 2 | 4,5 | 2 |
| Intégration Linux + maintenance | 3 | **5** | 3 | **5** | 4 |

Scénario A — « time-to-value » (effort 40 %, intégration 20 %, fidélité 15 %,
performance 10 %, empreinte 5 %, logs 10 %) :

| Toolkit | Total /5 |
| --- | ---: |
| Tauri | **4,00** |
| Slint | 3,83 |
| egui | 3,80 |
| GTK4 | 3,73 |
| Iced | 2,85 |

Scénario B — « qualité native » (effort 10 %, intégration 15 %, fidélité 20 %,
performance 20 %, empreinte 20 %, logs 15 %) :

| Toolkit | Total /5 |
| --- | ---: |
| GTK4 | **4,60** |
| Slint | 4,20 |
| egui | 3,78 |
| Tauri | 3,33 |
| Iced | 3,25 |

Lecture :

- **Tauri gagne si le temps compte pour ~40 % de la décision.**
- **GTK4 gagne si la qualité native, la VRAM et les logs comptent pour ~60 %.**
- Slint est un compromis crédible mais sa sélection de texte dans les logs reste
  à valider et sa licence doit être tranchée.
- egui reste un outil de diagnostic, pas l'UI opérateur.
- Iced est éliminé par l'accessibilité, quel que soit le scénario.

**Verdict de confrontation** : E1 a raison sur l'effort et la réutilisation ;
E2 a raison sur la valeur opérationnelle de GTK4 (VRAM, logs, AT-SPI). Aucun
des deux ne peut conclure sans arbitrer l'exigence « widgets natifs ».

### 3.2 SSE vs polling

- **E1** : pas de SSE en phase 1 ; le contrat actuel est le polling ; 300 lignes
  par étape, 2 Hz, 5,4 lignes/s mesurées.
- **E2** : ajouter `GET /api/events` (SSE), un `broadcast` de 1024, des deltas
  de logs, et un buffer GUI de 10 000 lignes.

Vérification :

- Le serveur ne renvoie que **300 lignes** (`MAX_STEP_LOG_LINES`), pas 10 000.
  Un buffer de 10 000 lignes côté GUI ne peut donc pas être rempli par l'API
  actuelle ; il faut soit du SSE avec deltas, soit lire les fichiers de log
  directement (nouvelle fonction), soit changer le buffer serveur.
- Le PoC d'E2 (5 000 lignes) ne peut être qu'un **test de widget synthétique**
  s'il ne modifie pas le serveur ; ce n'est pas un test de bout en bout.
- Le polling réel est de **2 req/s** par étape active, pas 5 req/s : le chemin
  `POLLING_INTERVAL_HIGH_FREQUENCY = 200` n'est jamais activé (aucun appelant ne
  passe `true`) et `autoModeEnabled` n'est jamais positionné dans le code Rust.
- SSE est **additif** et ne casse pas les routes existantes, mais il ajoute une
  surface serveur, des tests, une gestion de déconnexion et une seconde source
  de vérité (polling + flux). Ce n'est pas gratuit.

**Verdict** : SSE est une **optimisation de phase 3**, pas un prérequis du PoC.
E1 a raison sur le séquencement ; E2 a raison sur l'intérêt de SSE si l'on veut
afficher le log complet ou réduire la latence. La conception d'E2 (ring buffer,
coalescence 16 ms, `stick_to_bottom`) est bonne et doit être reprise.

### 3.3 Télémétrie : endpoint serveur vs collecte locale

- **E1** : réutiliser `/api/system_monitor` (déjà implémenté en Rust, GPU
  compris) ; 5 s ; `nvidia-smi` coûte 10–20 ms.
- **E2** : le serveur ne l'implémenterait pas (D-015) ; collecter localement via
  `sysinfo` + `nvml-wrapper` à 1 Hz, avec espace libre `/mnt/cache`, badge
  `/dev/apex_0` et état du verrou.

Vérification :

- **L'affirmation d'E2 est fausse.** `GET /api/system_monitor` est déclaré dans
  `crates/workflow-server/src/http.rs:82` et implémenté par `system_metrics`
  (`http.rs:614`) avec CPU, RAM, GPU `nvidia-smi` et disque. La décision
  D-015 couvre le cache, les statistiques, `/api/system/diagnostics` et
  `/api/audio/*`, **pas** `/api/system_monitor`.
- L'optimisation d'E2 reste valable : NVML direct évite un `fork` par
  échantillon, donne l'espace libre de `/mnt/cache` et des badges matériels.
- En revanche, si `hw_lock_held` est déduit de l'existence du fichier, le
  badge sera **faux** : `/tmp/workflow-rs-hw.lock` est créé et persiste après
  `unlock` (`crates/workflow-core/src/lock.rs`). Il faut `fcntl(F_GETLK)` ou
  interroger le serveur, pas `Path::exists()`. E2 ne montre que le champ
  `hw_lock_held` et ne documente pas sa méthode : le point est à vérifier au
  PoC, pas à lui imputer.
- Le CPU du serveur est calculé depuis `/proc/loadavg`, tandis que `sysinfo`
  utilise `/proc/stat` : deux valeurs différentes si les deux sont affichées.

**Verdict** : réutiliser l'endpoint en phase 1 (zéro duplication) ; ajouter
NVML/`/mnt/cache`/badges en phase 2 si le besoin opérateur le justifie, en
corrigeant la détection du verrou.

### 3.4 Effort et feuille de route

- **E1** : Tauri 12–18 j de MVP ; GTK4 70–95 j ; PoC GTK4 8–10 j.
- **E2** : 4 semaines (≈ 20 j) pour une parité GTK4 complète, PoC en semaine 1.

Vérification : le périmètre de parité comprend 8 cartes, 2 panneaux de logs,
3 popups, 6 thèmes, séquences, ingestion, sons, notifications, focus trap,
packaging, tests et accessibilité. Un PoC GTK4 limité à la timeline et aux logs
vaut déjà 8–10 j. Une parité complète en 20 j supposerait environ 550 lignes de
Rust par jour, sans tests ni thèmes : ce n'est pas crédible.

**Verdict** : E1 est réaliste ; E2 doit être corrigé d'un facteur 3,5 à 5. Si
GTK4 est choisi, planifier **3 à 4 mois**, pas 4 semaines.

### 3.5 Logs : 300 lignes vs 10 000/50 000

- **E1** : 300 lignes, 5,4 l/s, le problème est le re-rendu complet à 2 Hz.
- **E2** : 50 000 lignes, buffer 10 000, SSE, `GtkListView`.

Les deux décrivent la même solution (ring buffer, coalescence, virtualisation,
auto-scroll intelligent). La différence est le **périmètre** : E1 traite
l'existant, E2 traite une cible qui exige un changement serveur ou une lecture
directe des fichiers.

**Verdict** : adopter la conception d'E2, mais la classer « affichage du log
complet » (phase 3), pas « PoC ». Corriger aussi `LogLevel::classify` : les
`contains` ad hoc ne reproduisent pas les 8 regex de `logPatterns.js`
(casse, accents, `Échec`, horodatages, `[INFO]`). Il faut porter les motifs
existants, pas en inventer.

### 3.6 CSS et thèmes

- **E1** : CSS GTK différent ; 6 thèmes à retraduire ; prévoir 5–10 j.
- **E2** : tokens réutilisables « quasi directement ».

Vérification : GTK 4.6 (Ubuntu 22.04) **ne supporte pas** les custom properties
CSS (`var()`), ajoutées dans GTK 4.16. Les couleurs et espacements peuvent être
transcrits en `@define-color` ou en constantes Rust ; la mise en page (flex/grid,
`gap`, `position`) n'existe pas et doit être reconstruite avec des conteneurs
GTK. Les 6 thèmes restent un lot de 5–10 j.

**Verdict** : E1 a raison ; E2 doit corriger son affirmation. Corriger aussi E1 :
`GtkFileLauncher` est une API GTK 4.10, utiliser `gio::AppInfo` ou `xdg-open`
sur GTK 4.6.

### 3.7 Iced comme repli

- **E2** : Iced 4/5 en fidélité/accessibilité, meilleur repli 100 % Rust.
- **E1** : Iced écarté, accessibilité absente.

Vérification : l'issue `iced-rs/iced#552` « Implement accessibility support »
est toujours ouverte ; aucune exposition AT-SPI. Pour une UI opérateur qui
conserve `aria-live`, focus trap et rôles, c'est une régression structurelle.

**Verdict** : E1 a raison. Si une stack 100 % Rust est exigée, **Slint** est un
meilleur repli (propriétés `accessible-role`/`accessible-label`), ou la
régression d'accessibilité doit être acceptée explicitement.

### 3.8 Périmètre des PoC

- **E1** : coque Tauri + page entière + tray/notifications/sons.
- **E2** : carte STEP4 + 5 000 lignes de logs + `stopping`/409.

Les deux PoC sont **complémentaires** : E1 valide l'intégration et le poste de
travail ; E2 valide le rendu natif, la VRAM et la virtualisation. Un PoC de
décision complet devrait couvrir les deux dimensions, sinon il valide un axe et
laisse l'autre en hypothèse.

## 4. Erreurs factuelles et imprécisions

### 4.1 Dans E2 (étude indépendante)

| # | Affirmation E2 | Réalité vérifiée | Preuve | Impact |
| --- | --- | --- | --- | --- |
| 1 | `workflow-server` n'implémente pas `/api/system_monitor` (D-015) | Il l'implémente, GPU compris | `http.rs:82`, `http.rs:614-669` | Fausse la justification de la télémétrie locale |
| 2 | D-015 inclut `/api/system_monitor` | D-015 = cache, stats, diagnostics, `/api/audio/*` | `decisionLog.md` D-015 ; `CONTRAT` §2.2 | Confusion diagnostics/monitor |
| 3 | Le frontend interroge `/sequence/status` | Aucune référence | `grep` = 0 dans `assets/web` | Surestime la charge de polling |
| 4 | Polling à 200 ms / 5 req/s en mode séquence | 2 req/s ; le chemin 200 ms n'est jamais activé | `apiService.js:193-197`, appelants 140/327 | Surestime la pression d'un facteur 2,5 |
| 5 | Le frontend interroge `/api/csv_monitor_status` | Fonction définie mais jamais appelée | `apiService.js:360`, aucun appelant | Surestime la charge |
| 6 | `clear_disk_cache` est géré dans `apiService.js` | Il est dans `downloadsListManager.js`, et c'est du code mort | `downloadsListManager.js:79-130` ; aucun bouton dans le template | Confusion de fichiers |
| 7 | `stats-dashboard.css` est du code hérité actif | Non lié par le template | `index_new.html` (15 CSS, pas celle-là) | Surestime la dette UI |
| 8 | Les tokens CSS GTK sont réutilisables tels quels | Pas de `var()` avant GTK 4.16 ; pas de flex/grid | Docs GTK ; version 4.6.9 | Sous-estime le lot thèmes |
| 9 | Iced 4/5 accessibilité | Aucune AT-SPI, issue #552 ouverte | GitHub iced-rs/iced#552 | Repli inadapté |
| 10 | Parité GTK4 en 4 semaines | 70–95 j réalistes | Périmètre 11k lignes + 8 étapes | Sous-financement |
| 11 | Buffer 10k/50k lignes avec l'API actuelle | API bornée à 300 lignes | `state.rs:20` | Incohérence PoC/SSE |
| 12 | État du verrou déduit de l'existence du fichier (implicite) | Le fichier persiste après unlock | `workflow-core/src/lock.rs` | Badge faux si implémenté ainsi ; à vérifier |
| 13 | `GtkFileLauncher` (implicite via « xdg-open natif ») | API GTK 4.10 | Docs GTK | Non disponible sur 22.04 |
| 14 | « 100 % fidélité visuelle » Tauri | WebKitGTK ≠ Gecko ; rendu proche mais pas identique | — | Nuance |

Les points 1 à 5 sont les plus dommageables : ils servent de base à la
justification « D-015 non résolu » et à la surestimation de la charge.

### 4.2 Dans E1 (notre étude)

| # | Affirmation E1 | Réalité | Correction |
| --- | --- | --- | --- |
| 1 | `GtkFileLauncher` pour `xdg-open` | API GTK 4.10, absente de GTK 4.6 | Utiliser `gio::AppInfo::launch_default_for_uri` ou `xdg-open` |
| 2 | GTK4 90–180 Mio RSS | Probablement surestimé | Mesurer ; 60–120 Mio plus réaliste |
| 3 | Pas de mention de `GSK_RENDERER=cairo` | Argument fort pour la VRAM | Ajouter à l'option GTK4 |
| 4 | Pas de mention de NVML direct | `nvidia-smi` coûte 10–20 ms, acceptable mais perfectible | Ajouter en phase 2 |
| 5 | Pas de mention de SSE | Le contrat actuel n'en a pas, mais c'est une option | Ajouter en phase 3 |
| 6 | Pas de modèle `LogEntry` détaillé | La conception d'E2 est meilleure sur ce point | Reprendre ring buffer/coalescence |
| 7 | Pas de badge TPU/cache | Fonctions opérateur utiles | Ajouter en phase 2 |

E1 reste globalement exact sur les faits vérifiables (versions, MSRV, D-015,
débit de logs, absence de `/sequence/status`, modes Tauri, effort). Ses limites
sont des **omissions d'optimisation**, pas des erreurs de fond, sauf
`GtkFileLauncher`.

## 5. Recommandation consolidée

### 5.1 Architecture (consensus ferme)

**Modèle 1** : client GUI découplé de `workflow-server`, serveur headless
autoritaire, aucune fusion de l'orchestrateur. Les deux études le disent ; les
bloquants du modèle 2 (cycle de vie, crash, double webhook, verrou) sont
confirmés.

### 5.2 Technologie : règle de décision

| Si l'exigence dominante est… | Alors | Effort |
| --- | --- | --- |
| Widgets natifs, 0 Mio VRAM, AT-SPI, table de logs | **GTK4 + Relm4** | 70–95 j (PoC 8–10 j) |
| Délai court, parité pixel, zéro changement serveur/frontend | **Tauri v2 T1** | 12–18 j (PoC 5–8 j) |
| Stack 100 % Rust sans WebKit, avec accessibilité minimale | **Slint** | 55–80 j |
| Panneau de diagnostic interne uniquement | egui | 30–45 j |
| UI opérateur avec accessibilité | **Pas Iced** | — |

**Pour la mission actuelle** (« application Linux native »), la cible
recommandée est **GTK4/Relm4**, à condition de financer 3–4 mois et de valider
par un PoC de 8–10 j. **Tauri T1 reste le repli** si le délai de deux semaines
est impératif ou si le PoC GTK4 échoue.

Cette recommandation est un **changement par rapport à E1**, assumé : E1
privilégiait le temps de mise en œuvre ; la confrontation ajoute trois faits
(opérationnels) qui pèsent dans l'autre sens : 0 VRAM avec Cairo sur une carte
de 4 Gio partagée avec CUDA, `GtkListView`/`GtkTextView` pour les logs, et
AT-SPI natif. Elle ne repose pas sur les chiffres erronés d'E2 (4 semaines,
5 req/s, D-015).

### 5.3 PoC de décision à deux pistes (recommandé si budget)

| Piste | Contenu | Durée | Critères de sortie |
| --- | --- | --- | --- |
| A — Tauri T1 | Fenêtre sur `:5004`, tray, instance unique, notifications Rust, son, fermeture vers tray | 5–8 j | RSS ≤ 300 Mo, CPU ≤ 2 %, sons OK, aucun artefact NVIDIA |
| B — GTK4 | Timeline 8 cartes + liste de logs 10 000 lignes synthétiques + thème + Cairo | 8–10 j | 0 Mio VRAM (Cairo), 60 fps au scroll, AT-SPI Orca, transcription thème |
| Porte | Comparer VRAM pendant STEP2, RSS, latence d'entrée, effort restant | 2 j | Décision documentée |

Coût total : **15–20 j**, inférieur à une erreur de choix de 3 mois. Si une
seule piste est financée : B si « natif » est une exigence, A sinon.

### 5.4 Ce qu'il faut reprendre d'E2

1. `GSK_RENDERER=cairo` pour l'option GTK4 (0 Mio VRAM applicative).
2. NVML direct (`nvml-wrapper`) en phase 2, avec fallback `nvidia-smi`.
3. Espace libre `/mnt/cache` et badge `/dev/apex_0` dans le moniteur.
4. SSE `/api/events` en phase 3, additif, authentifié, borné, testé.
5. Ring buffer `VecDeque<LogEntry>`, coalescence 16 ms, `stick_to_bottom`.
6. `LogEntry { seq_id, level, text: Arc<str> }` et classification par les motifs
   existants (pas de `contains` ad hoc).
7. `systemd --user` + `.deb` + `cargo-deb`.
8. Acteur audio `rodio` avec canal borné.

### 5.5 Ce qu'il faut corriger dans E2 avant réutilisation

- Retirer la prémisse « `/api/system_monitor` absent / D-015 ».
- Retirer « 200 ms / 5 req/s » et « polling `/sequence/status` ».
- Corriger `clear_disk_cache` (fichier, code mort) et `stats-dashboard.css`.
- Corriger « tokens CSS réutilisables » et `GtkFileLauncher`.
- Corriger la note Iced (accessibilité) et l'effort GTK4 (×3,5 à 5).
- Distinguer « logs existants (300 lignes) » et « log complet (nouvelle
  fonction, SSE ou lecture fichier) ».
- Remplacer `Path::exists()` pour le verrou par `F_GETLK` ou l'état serveur.

### 5.6 Ce qu'il faut corriger dans E1

- Remplacer `GtkFileLauncher` par `gio::AppInfo`/`xdg-open` (GTK 4.6).
- Réviser l'estimation RSS GTK4 à 60–120 Mio (à mesurer).
- Ajouter Cairo, NVML, SSE et le modèle `LogEntry` comme options explicites.
- Ajouter les badges TPU/cache au périmètre phase 2.

## 6. Matrice de synthèse finale

| Question | Réponse consolidée |
| --- | --- |
| Architecture | Modèle 1, client du serveur (unanime) |
| Serveur | Inchangé en phase 1 ; SSE additif en phase 3 |
| Frontend web | Conservé, rollback et diagnostic |
| Technologie par défaut | GTK4/Relm4 si « natif » est une exigence ; sinon Tauri T1 |
| PoC | Deux pistes 15–20 j, ou une piste selon l'arbitrage |
| Logs | 300 lignes existantes ; log complet = nouvelle fonction |
| Télémétrie | Endpoint serveur en phase 1 ; NVML en phase 2 |
| Audio | Web Audio en Tauri ; `rodio` en GTK4 |
| Verrou/TPU | Jamais touchés par la GUI |
| Effort GTK4 | 70–95 j, pas 4 semaines |
| Effort Tauri | 12–18 j |
| Risque principal | WebKitGTK/NVIDIA (Tauri) ; effort et GTK 4.6 (GTK4) |

## 7. Questions à arbitrer

1. **« Natif » signifie-t-il widgets natifs ou application de bureau ?** C'est
   la question qui tranche entre GTK4 et Tauri.
2. **Le budget est-il de 2 semaines ou de 3–4 mois ?** Si 2 semaines, Tauri.
3. **La VRAM doit-elle être strictement isolée pendant STEP2 ?** Si oui, Cairo
   GTK4 est le seul choix à 0 Mio applicative.
4. **Le log complet (10 000+ lignes) est-il requis ?** Si oui, SSE ou lecture
   directe, et GTK4 devient plus attractif.
5. **L'accessibilité AT-SPI est-elle contractuelle ?** Si oui, Iced est exclu.
6. **Le serveur peut-il recevoir une route additive `/api/events` ?** Si non,
   le buffer GUI reste borné à 300 lignes.
7. **La distribution cible est-elle cette machine ou plusieurs postes ?** Cela
   décide du packaging et des dépendances système.

## Annexe A — Preuves vérifiées

| Fait | Preuve |
| --- | --- |
| `/api/system_monitor` en Rust | `crates/workflow-server/src/http.rs:82`, `614-669` |
| GPU via `nvidia-smi` | `http.rs:676-696` |
| 300 lignes max | `crates/workflow-server/src/state.rs:20` |
| 5,4 l/s STEP4 | `runtime/logs/step_STEP4.log` : 979 lignes / 179 s |
| 200 ms jamais activé | `assets/web/static/apiService.js:193-197`, appelants 140/327 |
| `/sequence/status` jamais appelé | `grep` = 0 dans `assets/web` |
| `fetchCSVMonitorStatusAPI` jamais appelé | `apiService.js:360`, aucun appelant |
| `clear_disk_cache` mort | `downloadsListManager.js:79-130`, aucun bouton |
| `stats-dashboard.css` non lié | `index_new.html` |
| GTK 4.6.9 | `dpkg -l libgtk-4-1` |
| `var()` GTK depuis 4.16 | Docs GTK « Custom Properties » |
| `GtkFileLauncher` depuis 4.10 | Docs GTK |
| Iced sans AT-SPI | `iced-rs/iced#552` |
| STEP2 2 203 Mio VRAM | `docs/LOT-2-STEP2.md:99` |
| STEP4 199,5 Mio RSS | `docs/LOT-3-STEP4.md:139` |
| 82 segments STEP8 | `docs/LOT-1-STEP8.md:72` |
| `max_concurrent = 4` | `config/config.toml:83` |
| Threadripper 1920X | `lscpu` |
| Verrou fichier persistant | `crates/workflow-core/src/lock.rs` |
| `start-rust.sh` arrête le serveur | `scripts/start-rust.sh:131` |
| `switch-rust.sh` détache | `scripts/switch-rust.sh:159` |

## Annexe B — Commandes de vérification

```sh
grep -n "system_monitor\|fn system_metrics\|fn gpu_metrics" \
  crates/workflow-server/src/http.rs
grep -rn "sequence/status\|fetchCSVMonitorStatusAPI" assets/web/static
grep -rn "startPollingAPI(" assets/web/static
grep -n "MAX_STEP_LOG_LINES" crates/workflow-server/src/state.rs
grep -n "max_concurrent" config/config.toml
grep -n "GtkFileLauncher\|4.10" /usr/share/doc/libgtk-4-doc 2>/dev/null
pkg-config --modversion gtk4
```
