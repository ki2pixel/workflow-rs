# Journal des décisions

Ce document enregistre les décisions architecturales et techniques durables de
workflow-rs. Les écarts de comportement legacy sont détaillés dans
`parity/DRIFT.md`.

> **Politique de conservation** : les décisions actives D-041 → D-079 restent
> en texte intégral. L'historique de bootstrap, les audits d'octobre 2026, les
> décisions fondatrices D-028 → D-040 et les brouillons D-058 sont conservés
> dans
> [Archives des décisions historiques (octobre 2026)](file:///home/kidpixel/workflow-rs/memory-bank/archives/decisionLog-archive-2026-10.md).
> Les décisions legacy antérieures à la migration Rust restent dans
> `/home/kidpixel/workflow_mediapipe/memory-bank/archives/`. Fenêtre de détail :
> 90 jours (`.agents/rules/memorybankprotocol.md` §3).

## Index

| Bloc | Décisions | Emplacement |
| --- | --- | --- |
| Journal actif récent | D-080 → D-060 | ci-dessous |
| Journal actif consolidé | D-058 → D-041 | ci-dessous |
| Numéro réservé | D-059 (option C A1, non ouverte) | voir D-058 |
| Fondations, QA et bypass | D-040 → D-028 | [archive](file:///home/kidpixel/workflow-rs/memory-bank/archives/decisionLog-archive-2026-10.md) |
| Écarts legacy | D-027 → D-001 | tableau ci-dessous ; détail `parity/DRIFT.md` |
| Bootstrap, migration et audits | 2026-10-22 → 2026-10-24 | archive |
| Brouillons D-058 | calibration v2, proposition | archive |

## 2026-11-16 — D-080 : déclenchement du template SDM par mission `sdm.png`

- **Contexte** : `/mnt/cache/216 Camille/docs/Alice et Sofiane.mp4` et
  `Liliane et José.mp4` ont été livrés sans préfixe `SDM - `. Leur mission
  `recadrage_lot_216_20261116` demande pourtant uniquement `sdm.png`. Le
  prédicat historique (`template_prefixes` + `starts_with`) n'a déclenché ni
  le bypass STEP2–STEP6 ni A6 : les deux vidéos ont traversé TransNetV2,
  YAMNet/ECAPA, BlazeFace/FaceMesh/EfficientDet, STEP5/6 et le panneau
  `MediaSolution.jsx`, puis A7 a ajouté `sdm.png` en calque statique. Le
  `post.json` du lot porte `template: null` et `a6_eligible: 0`.
- **Décision** :
  1. `steps.step8.post.mission_template` (défaut `false`, activé dans
     `config/config.toml`) autorise un second déclencheur, sans remplacer le
     nommage. Le préfixe reste prioritaire ; la mission n'est consultée
     qu'après échec de tous les `template_prefixes`.
  2. Règle stricte : une **mission unique** pour le lot, une instruction vidéo
     résolue, ni `Pas de logo`, ni `logo_unresolved`, et **exclusivement**
     `sdm.png` après normalisation (`filename_key`, accents/casse). Un logo
     supplémentaire (`sdm.png` + sponsor) reste un habillage A7 standard.
  3. `workflow_core::mission::SdmTemplateIndex` précalcule les couples
     lot/vidéo éligibles. Un lot porteur de plusieurs missions est exclu en
     bloc, comme la règle d'ambiguïté A7 : aucun choix « le plus récent ».
  4. `template::Bypass` consulte l'index (`mode_for_name`, `probe`) et
     `a6::validate` reçoit le match explicite. Les deux partagent la même
     source de vérité ; le profil FFprobe 1920×1080 25 fps H.264 reste
     obligatoire pour STEP2–STEP7, et les contrôles structurels A6 restent
     inchangés.
  5. STEP2 synchronise une fois le cache `email_missions` quand le drapeau est
     actif ; STEP3 à STEP7 relisent le cache local sans réseau. Base absente,
     cache illisible ou mission non résolue : repli silencieux sur le nommage
     historique, aucune erreur bloquante.
  6. `template.trigger` (`name`, `mission`, `none`) et `template.mission`
     (identifiant) sont ajoutés de façon additive à `post.json`, `audit.json`
     et aux détails A6. Pour un déclenchement mission, `template.prefix` reste
     vide, conformément au contrat D-061. A7 conserve sa déduplication : le
     `sdm.png` inséré par A6 devient `skipped`, jamais dupliqué.
  7. Arbitrage du précédent lot 131 (`Raymond.mp4`, `sdm.png` sans préfixe) :
     la règle mission s'applique aussi aux rejeux futurs, car l'intention
     client est identique. Aucun artefact historique n'est réécrit et aucun
     seuil de lot n'est codé en dur ; les sorties déjà publiées restent la
     référence de leur lot. Un rejeu de 131 produirait désormais A6 pour
     `Raymond`, ce qui est le comportement attendu par la mission.
- **Validation** : `cargo test --workspace` = 704 tests listés (702 exécutés +
  2 ignorés, 0 échec), `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts, build release
  `workflow-server` + `workflow-cli` vert. Rejeu isolé sous
  `/tmp/opencode/test-lot216` (copie de `216 Camille`, cache email copié,
  réseau neutralisé) : `post run --direct` crée les deux bruts, A6 applique
  4 calques (logo, panneau secondaire, panneau principal, fond 180 %), A7
  déduplique `sdm.png` (`added: []`, `skipped: ["sdm.png"]`),
  `template.trigger = "mission"`, audit `pass` pour les deux projets.
  `step7 finalize` avec CSV source retiré synthétise deux `_scenes.csv`
  mono-segment (`Alice` 1087 frames, `Liliane` 1236 frames),
  `scenes_origin = "template"`, et les copie en `.csv` dans la sortie
  finalisée. Un rejeu `post run` sur les bruts panneau déjà présents laisse
  `Liliane` en revue A6 (segment 49,36 s contre comp 49,44 s) : le correctif ne
  réécrit jamais un brut existant, la régénération passe par le chemin direct.
- **Portée / rollback** : additif et opt-in. `mission_template = false`
  restaure exactement le déclenchement par nommage. Aucun champ existant n'est
  renommé ni réordonné ; `trigger` et `mission` sont ajoutés en fin de bloc.
  Les chemins, l'immutabilité du brut et les décisions A1–A5 sont inchangés.
- **Références** : D-034, D-035, D-061, D-064, D-065, D-066,
  `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-12-A7-MISSIONS.md`,
  `crates/workflow-core/src/mission.rs`,
  `crates/workflow-steps/src/template.rs`,
  `crates/workflow-steps/src/post/a6.rs`,
  `crates/workflow-steps/src/post/direct.rs`,
  `crates/workflow-cli/src/main.rs`.

## 2026-11-16 — D-079 : sémantique modale de l'overlay de logs et assainissement CSS (AUD-FE-10, AUD-FE-12)

- **Contexte** : l'audit frontend du 2026-11-16 laissait deux constats ouverts.
  `#logs-column-global` ne portait que `aria-hidden`, piloté par
  `popupManager.js`, sans `role="dialog"` ni `aria-modal="true"`, contrairement
  aux popups de séquence et au prompt CSV. Trois conflits CSS opposaient par
  ailleurs des feuilles chargées dans un ordre précis : `.local-downloads-section`
  était animé à la fois par `layout.css` (`opacity: 0` + classe obsolète
  `.downloads-visible`) et par `components/downloads.css` (`opacity: 1` +
  `[data-visible]`) ; `.workflow-wrapper.compact-mode .steps-column` était défini
  en `display: grid` dans `layout.css` puis écrasé en `display: flex` dans
  `components/steps.css` ; `#run-all-steps-button` était stylisé dans
  `components/controls.css` avant d'être intégralement réécrit dans
  `components/workflow-buttons.css`. Le rendu dépendait donc de l'ordre
  d'inclusion et le CSS mort s'accumulait.
- **Décision — ARIA** : le template `assets/web/templates/index_new.html`
  déclare `role="dialog" aria-modal="true" aria-labelledby="log-panel-title"`
  sur `#logs-column-global`. `#log-panel-title` porte déjà le titre « Logs ».
  Aucun module JS n'est modifié : `openPopupUI`/`closePopupUI` continuent de
  piloter `aria-hidden`, le focus trap, `Escape` et la restauration du focus par
  l'ID ; `openLogPanelUI`/`closeLogPanelUI` et `uiUpdater.js` sont inchangés.
  L'overlay suit désormais le même contrat sémantique que
  `#sequence-summary-popup-overlay` et `#custom-sequence-confirm-popup-overlay`.
- **Décision — propriété CSS unique** : chaque composant a une seule feuille
  autoritaire. `components/downloads.css` conserve l'intégralité de
  `.local-downloads-section` (base, `[data-visible="true"|"false"]`, animation
  GPU et `prefers-reduced-motion`) ; `layout.css` ne garde que le layout générique
  (`.workflow-wrapper`, scrollbar, base `.steps-column`).
  `components/steps.css` reste l'unique source du mode compact (`display: flex`,
  `flex-direction: column`, `align-items: center`, `gap`) ; la règle grid et son
  media query 1200 px quittent `layout.css`. `components/workflow-buttons.css`
  porte tout `#run-all-steps-button` (base, shine, survol, actif, désactivé,
  dégradé) ; les règles de base et de survol de `components/controls.css` sont
  supprimées. `features/responsive.css` conserve ses surcharges mobiles
  (padding, rayon, ombre) : c'est une couche de breakpoint légitime, pas un
  doublon.
- **Décision — préservation des déclarations effectives** : deux déclarations
  présentes dans les blocs supprimés n'étaient pas réellement mortes, car la
  feuille gagnante ne les définissait pas. `transition: opacity 0.5s ease-in-out`
  (mode compact) est déplacée dans `components/steps.css`, et
  `min-height: var(--touch-target-min)` (cible tactile de 44 px) dans la règle de
  base de `components/workflow-buttons.css`. Les styles calculés restent donc
  identiques, y compris pour les tailles de boîte.
- **Portée / invariants** : aucun changement de `main.js`
  (`setupLocalDownloadsToggle`, `applyLocalDownloadsVisibility`),
  `popupManager.js`, `uiUpdater.js`, des sélecteurs contractuels
  (`#logs-column-global`, `#log-panel-title`, `#close-log-panel`,
  `#run-all-steps-button`, `.local-downloads-section`, `.compact-mode`,
  `.steps-column`), des contrats HTTP/JSON/CSV ni du prétraitement Minijinja.
  Aucun package npm, bundler ou préprocesseur introduit. Le dépôt Python legacy
  n'est pas modifié et rien n'est écrit dans `runtime/` ou `/mnt/cache`.
- **Validation** : arbre pré-changement reconstruit à l'identique depuis `HEAD`
  pour l'A/B ; `tinycss2` 1.4.0 : 0 erreur sur les cinq feuilles, accolades et
  parenthèses équilibrées ; A/B Firefox 158 headless + geckodriver (1400 px et
  700 px, feuilles chargées dans l'ordre de `index_new.html`) : styles calculés
  et tailles identiques pour `.local-downloads-section` visible et masqué,
  `.steps-column` compact et `#run-all-steps-button` (base et survol) ; seuls
  quatre longhands grid inertes diffèrent (`grid-template-columns`,
  `grid-auto-flow`, `align-content`, `justify-items`) alors que `display` calcule
  `flex` dans les deux variantes et que la taille reste 1356 × 503,2 px (desktop)
  et 656 × 553,4 px (mobile) ; `grep` : aucune occurrence de
  `downloads-visible`, `#run-all-steps-button` uniquement dans
  `workflow-buttons.css` + `responsive.css`, `.compact-mode .steps-column`
  uniquement dans `steps.css` ; assertions ARIA ajoutées à
  `real_index_template_renders_with_existing_frontend_syntax` ;
  `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu
  Minijinja, `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés,
  0 échec), `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings` et `git diff --check`
  verts.
- **Rollback** : un revert de `index_new.html`, `layout.css`, `controls.css`,
  `steps.css` et `workflow-buttons.css` restaure la cascade précédente. Aucune
  migration de données ni de configuration n'est nécessaire.

## 2026-11-16 — D-078 : réponses Worker obsolètes, cache-busting ES Modules et aperçu CSV à 8 étapes (AUD-FE-07, AUD-FE-09, AUD-FE-11)

- **Contexte** : l'audit frontend du 2026-11-16 laissait ouverts trois
  constats. `WorkerManager._executeTask` rejetait la promesse d'une requête
  dont la réponse arrivait après qu'une requête plus récente avait été émise
  sur le même canal ; ce rejet bénin pouvait atteindre le listener global
  `unhandledrejection` d'`ErrorHandler.js` et afficher une notification
  d'erreur pendant un défilement ou un rafraîchissement rapide des logs. Le
  singleton n'était pas non plus détruit sur `beforeunload`, contrairement à
  `appState`, `domBatcher` et `pollingManager`. Sept modules importaient en
  outre `PollingManager.js?v=20261025a`, seul query parameter de version du
  frontend ; un import ultérieur sans ce suffixe exact aurait instancié un
  second singleton avec son propre registre `inFlight`. Enfin, la modale de
  prompt CSV affichait encore six badges legacy et « étapes 1-6 » alors que la
  séquence lancée consomme les huit clés `STEP1`…`STEP8` de `constants.js`.
- **Décision — réponses obsolètes** : dans `WorkerManager.js`, une réponse
  périmée sur un canal résout la promesse avec `null` et consigne un
  `console.debug` ; elle ne rejette plus. Le fallback synchrone
  (`_executeSynchronously`) suit la même convention, avec un retour `null` au
  lieu du `throw`. Les erreurs réelles du Worker (`success: false`, type
  inconnu) continuent de rejeter avec un `Error`, donc la distinction entre
  annulation et panne est préservée. `destroy()` est attaché à `beforeunload`
  derrière un garde `typeof globalThis.addEventListener === 'function'`, pour
  rester chargeable dans un contexte Node sans `EventTarget` global.
- **Décision — consommateurs** : `uiUpdater.js` ignore explicitement `null`
  dans `updateSpecificLogUI` et `updateMainLogOutputUI` avant de programmer la
  mutation DOM, afin qu'une réponse périmée n'écrase jamais le panneau courant.
  Une chaîne vide reste un contenu légitime et continue d'être appliquée.
- **Décision — cache-busting** : les sept importateurs (`main.js`,
  `apiService.js`, `sequenceManager.js`, `timerManager.js`, `uiUpdater.js`,
  `utils/PerformanceMonitor.js` et l'import dynamique de
  `utils/PerformanceOptimizer.js`) utilisent le spécificateur canonique
  `PollingManager.js` sans query parameter. Le serveur Axum applique déjà
  `Cache-Control: no-cache, must-revalidate` aux assets statiques ; le suffixe
  statique n'apportait aucune fraîcheur et divergeait du reste des modules ES.
- **Décision — aperçu CSV** : `csvWorkflowPrompt.js` affiche les huit étapes
  canoniques du pipeline Rust (Extraction, Transitions, Audio, Tracking,
  Réduction JSON, Pré-traitement AE, Finalisation, Media Solution) et le pied
  de modale annonce « étapes 1-8 ». `runStepSequence` n'est pas modifié : il
  consomme déjà `defaultSequenceableStepsKeys` (`STEP1`…`STEP8`).
- **Portée / invariants** : aucun changement de `parseWorker.js`, de contrat
  HTTP/JSON/CSV, de sélecteur DOM contractuel, de `config/config.toml`, de
  `runtime/` ou de `/mnt/cache`. Aucun package npm, bundler ou préprocesseur
  introduit ; l'architecture zéro-build est conservée. Le legacy Python n'est
  pas modifié.
- **Validation** : `node --check` sur les neuf modules ; harnais Node jetables
  sous `/tmp/opencode` : 6/6 sur le fallback synchrone (deux requêtes
  concurrentes sur un canal, canaux isolés, requêtes sans canal, `destroy()`
  sur `beforeunload`) et 5/5 sur un Worker simulé (réponses hors ordre, rejet
  réel préservé, purge des registres), `unhandledrejection` vide dans les deux
  cas ; `grep -rn "20261025a" assets/web/static/` vide ; `cargo test
  -p workflow-server --lib` (46 tests), les deux tests de rendu Minijinja,
  `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés, 0 échec),
  `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
  et `git diff --check` verts.
- **Rollback** : un revert de `WorkerManager.js`, `uiUpdater.js`,
  `csvWorkflowPrompt.js` et des sept imports restaure le comportement
  précédent (rejets sur réponses périmées, badge à 6 étapes). Aucune migration
  de données ni de configuration n'est nécessaire.

## 2026-11-16 — D-077 : syntaxe CSS Color 4 des ombres `rgb(var(...) / alpha)` (AUD-FE-08)

- **Contexte** : `assets/web/static/css/variables.css` déclare les couleurs
  sous forme de composantes RGB séparées par des espaces
  (`--accent-primary-rgb: 121 134 203;`, `--status-running-rgb: 77 171 247;`),
  conformément à CSS Color 4. Six déclarations `box-shadow` héritées du legacy
  mélangeaient toutefois cette forme avec la virgule de `rgba()` :
  `rgba(var(--accent-primary-rgb), 0.3)` se résout en
  `rgba(121 134 203, 0.3)`. La spécification interdit de mélanger espaces et
  virgules dans une même fonction de couleur : la déclaration est invalide et le
  moteur la rejette en silence. Les halos de survol, de focus, de sélection et
  de statut `running` des cartes d'étapes, ainsi que l'ombre de
  `.workflow-launch-btn` et de son survol, n'étaient donc jamais rendus.
  `components/logs.css` appliquait déjà la forme conforme
  `rgb(var(--accent-primary-rgb) / 0.16)`.
- **Décision — forme unique** : les six occurrences passent à
  `rgb(var(--*-rgb) / <alpha>)` (`rgb(121 134 203 / 0.3)`), forme moderne où
  l'alpha est séparé des composantes par une barre oblique. Les valeurs d'alpha (`0.2`,
  `0.26`, `0.25`, `0.3`, `0.4`), les géométries d'ombre, les couleurs et
  l'ordre des déclarations sont inchangés. Aucun fallback codé en dur, aucune
  variable dupliquée : les variables `*-rgb` restent la source unique et
  `variables.css` n'est pas modifié.
- **Décision — périmètre** : seules
  `static/css/components/steps.css` (quatre `box-shadow`) et
  `static/css/components/csv-workflow-prompt.css` (deux `box-shadow`) sont
  touchés. Les `rgba(0, 0, 0, …)` des ombres neutres restent valides et sont
  conservés. Aucun préprocesseur, bundler ou dépendance Node/npm n'est
  introduit : le CSS vanilla est servi tel quel par Axum.
- **Écart legacy assumé** : `/home/kidpixel/workflow_mediapipe` conserve la
  syntaxe invalide ; la correction vit uniquement dans `assets/web`. Aucune
  écriture dans le dépôt Python. L'écart est documenté dans
  `memory-bank/systemPatterns.md` §14.
- **Portée / invariants** : sélecteurs contractuels préservés
  (`.timeline-step`, `.timeline-step.is-selected`, `.timeline-step[data-status]`,
  `.workflow-launch-btn`), ainsi que les classes d'état `data-status` et
  `active-for-log-panel`. Aucun changement de HTML, de JS, de contrat
  HTTP/JSON/CSV, de `runtime/`, `/mnt/cache` ou `config/config.toml`.
- **Validation** : `grep -rn -- "rgba(var(--" assets/web/static/css/` ne renvoie
  plus rien ; `tinycss2` analyse les deux feuilles sans erreur (105 et 36
  règles, 362 et 186 déclarations) et sérialise les six ombres corrigées ;
  accolades et parenthèses équilibrées (123/123 et 257/257 pour `steps.css`,
  61/61 et 42/42 pour `csv-workflow-prompt.css`) ; contrôle A/B dans Chromium
  sur une copie des trois feuilles : l'ancienne forme via variable calcule
  `none`, la nouvelle calcule les ombres attendues, `.workflow-launch-btn`
  (`rgba(121, 134, 203, 0.3)`), son survol (`0.4`), le survol d'une carte
  (`0.2`), `:focus-within` (`0.26`), `.is-selected` (`0.26`) et
  `data-status="running"` (`rgba(77, 171, 247, 0.25)`) ; `cargo test
  -p workflow-server --lib` (46 tests), les deux tests de rendu Minijinja,
  `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés, 0 échec),
  `cargo fmt --check` et `cargo clippy --workspace --all-targets -- -D warnings`
  verts.
- **Rollback** : un revert des deux feuilles restaure les ombres ignorées ;
  aucun état serveur, artefact ou migration à nettoyer.

## 2026-11-16 — D-076 : comparaison structurelle et immuabilité d'AppState (AUD-FE-03, AUD-FE-04, AUD-FE-11)

- **Contexte** : l'audit frontend du 2026-11-16 relève trois défauts dans
  `assets/web/static/state/AppState.js`. `subscribeToProperty()` comparait les
  valeurs par référence alors que `setState()` clone l'arbre à chaque appel :
  tout abonné à un objet ou tableau (`csvDownloads` de `csvDownloadMonitor.js`,
  `processInfo`) était notifié à chaque tick de polling de 500 ms (AUD-FE-03).
  `getStateProperty()` retournait la référence interne, permettant une mutation
  silencieuse hors `setState()` (AUD-FE-04), et `reset()` fusionnait
  `processInfo: {}`, `stepTimers: {}` et `pollingIntervals: {}` via
  `_mergeDeep`, qui conserve les clés absentes de la source ; `initialState.ui`
  omettait six préférences (AUD-FE-04). Le constructeur initialisait enfin
  `ui.theme: 'dark'` alors que `themes.css` cible `[data-theme="dark-pro"]`
  (AUD-FE-11).
- **Décision — comparaison structurelle** : `subscribeToProperty()` notifie via
  `!this._areValuesEqual(newValue, oldValue)`. Primitives, objets, tableaux et
  structures imbriquées ne déclenchent donc le callback que sur un changement
  réel de contenu ; les cycles restent couverts par la `WeakMap` existante.
- **Décision — lectures isolées** : `getStateProperty()` retourne
  `this._deepClone(value)`. Les primitives sont inchangées, les objets et
  tableaux sont des copies profondes. `_deepClone()` conserve en revanche la
  référence des objets hôtes non clonables (nœud DOM de
  `focusedElementBeforePopup`) : un clone plain perdrait `focus()` et casserait
  la restauration du focus de `popupManager.closePopupUI()`. `_mergeDeep()`
  traite ces valeurs comme des feuilles et clone les tableaux, ce qui supprime
  aussi l'alias entre le tableau de l'appelant et `this.state`.
- **Décision — reset exhaustif** : `reset()` construit l'état initial complet
  (11 clés `ui`, `pollingIntervals`, `activeStepKeyForLogsPanel`,
  `isAnySequenceRunning`, `focusedElementBeforePopup`, `stepTimers`,
  `selectedStepsOrder`, `processInfo`, `performanceMetrics`, `cacheStats`),
  clone l'ancien et le nouveau état, puis remplace `this.state` en un seul
  passage avec `_stateChanged`, incrémentation de `stateChangeCount`, mise à
  jour de `lastStateChange`, `_persistStateChanges()` et
  `_notifyListeners(newState, oldState, 'reset')`. `processInfo`, `stepTimers`
  et `pollingIntervals` sont réellement vidés ; les clés dynamiques
  (`lastCSVDownloadCompletion`, `ui.autoModeLogPanelOpened`) disparaissent aussi,
  ce qui est le comportement attendu d'une remise à zéro. La purge `try/catch`
  des `PERSISTED_PATHS` dans `localStorage` est conservée, et un reset sans
  changement structurel ne notifie plus. `reset()` sur une instance détruite est
  ignoré avec un avertissement.
- **Décision — thème par défaut** : `ui.theme` vaut `'dark-pro'` au constructeur
  et dans `reset()`, en alignement avec `themes.css`. `themeManager.loadTheme()`
  continue de valider l'identifiant contre `THEMES` avant application.
- **Écart legacy assumé** : le dépôt Python conserve la comparaison par
  référence, les lectures non clonées, le `reset()` fusionné et `theme: 'dark'` ;
  la correction vit uniquement dans `assets/web`. L'écart est documenté dans
  `memory-bank/systemPatterns.md` §14, sans écriture dans
  `/home/kidpixel/workflow_mediapipe`.
- **Portée / invariants** : architecture zéro-build ES Modules, aucune
  dépendance Node/npm, aucun bundler. `LEGACY_MIGRATIONS`,
  `STEP_KEYS_MIGRATION_FLAG`, `PERSISTED_PATHS`, les sélecteurs DOM et les
  contrats HTTP/JSON/CSV sont inchangés ; aucun fichier de `runtime/` ou
  `/mnt/cache` touché.
- **Validation** : `node --check` ; harnais Node jetable sur une copie
  byte-identique du module (SHA-256 vérifié) : 15/15 cas — isolation de
  `csvDownloads` face aux ticks `processInfo`, absence de notification pour un
  contenu identique, mutation des clones `ui`/`processInfo`/`selectedStepsOrder`
  sans effet, référence hôte conservée, égalité structurelle des `Date` de
  `stepTimers`, `reset()` vidant les dictionnaires d'exécution avec 11 clés `ui`
  et `theme: 'dark-pro'`, purge `localStorage`, reset idempotent et instance
  détruite neutralisée ; `cargo test -p workflow-server --lib` (46 tests), les
  deux tests de rendu Minijinja, `cargo test --workspace` (689 listés, 687
  exécutés + 2 ignorés, 0 échec), `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.
- **Rollback** : un revert d'`AppState.js` restaure le comportement précédent ;
  aucune migration de données ni état serveur à nettoyer.

## 2026-11-16 — D-075 : réparation de PerformanceMonitor et suspension hors focus (AUD-FE-02, AUD-FE-05)

- **Contexte** : l'audit frontend du 2026-11-16
  (`docs/validation/audits/2026-11-16-audit-frontend-rust.md`) relève que
  `generatePerformanceReport()` (`utils/PerformanceMonitor.js`) appelle
  `errorHandler.getStats()`, méthode inexistante : `ErrorHandler` expose
  `getErrorStats()`, d'où un `TypeError: errorHandler.getStats is not a
  function` toutes les 60 s dans `sendPerformanceReport()` et à chaque
  `getMetrics()` (AUD-FE-02). Les pollers de monitoring
  (`perf-memory-usage` 10 s, `perf-periodic-reporting` 60 s,
  `perf-optimizer-stats` 30 s) tournaient sans `pauseWhenHidden: true`, et le
  POST de télémétrie passait par le `fetch` instrumenté de `monitorApiCalls()`
  sans `X-Worker-Token` (AUD-FE-05).
- **Décision — statistiques d'erreurs** : `generatePerformanceReport()` appelle
  `errorHandler.getErrorStats()` et conserve la clé `components.errorHandler`,
  dont la structure (`totalErrors`, `recentErrors`, `activeErrorOperations`,
  `errorsByOperation`) est celle déjà produite par `ErrorHandler`.
- **Décision — suspension hors focus** : les trois pollers passent
  `{ pauseWhenHidden: true }` au quatrième argument de
  `PollingManager.startPolling`. `PollingManager` enregistre alors les noms
  dans `pauseOnHide`, suspend les intervalles sur `visibilitychange` et
  rejoue immédiatement un tick au retour au premier plan. Aucun nom de poller
  ni intervalle n'est modifié.
- **Décision — découplage de la télémétrie** : `sendPerformanceReport()`
  sélectionne `this.originalFetch || globalThis.fetch`, donc l'appel
  `/api/performance/frontend` n'est plus compté dans `metrics.apiCalls` et ne
  peut plus entretenir de boucle d'auto-surveillance. Le jeton worker est lu
  dans `<meta name="worker-token">` comme dans `apiService.js` et ajouté en
  `X-Worker-Token` ; `Content-Type: application/json` et le corps
  `JSON.stringify(report)` restent inchangés. Le handler serveur
  `performance_frontend` n'est pas modifié.
- **Écart legacy assumé** : le dépôt Python conserve `errorHandler.getStats()`
  et l'absence de `pauseWhenHidden` ; la correction vit uniquement dans la
  copie Rust `assets/web`. L'écart est documenté dans
  `memory-bank/systemPatterns.md` §14, sans écriture dans
  `/home/kidpixel/workflow_mediapipe`.
- **Portée / invariants** : architecture zéro-build ES Modules, aucune
  dépendance Node/npm, aucun bundler. Les sélecteurs DOM contractuels, les
  contrats HTTP/JSON/CSV et la structure du payload de rapport sont préservés ;
  aucun fichier de `runtime/` ou `/mnt/cache` touché.
- **Validation** : `node --check` sur les deux modules ; harnais Node jetable
  (globals DOM/fetch simulés) vérifiant que le rapport ne lève plus,
  `components.errorHandler` est conforme, les trois pollers figurent dans
  `pauseOnHide`, la télémétrie ne grossit pas `metrics.apiCalls`, le token est
  transporté et l'instrumentation des appels applicatifs reste active ;
  `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu
  Minijinja ciblés, `cargo test --workspace` (689 listés, 687 exécutés + 2
  ignorés, 0 échec), `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.
- **Rollback** : un revert des deux modules JS restaure le comportement
  précédent ; aucune migration de données, aucun état serveur à nettoyer.

## 2026-11-16 — D-074 : assainissement XSS du frontend (AUD-FE-01, AUD-FE-06)

- **Contexte** : l'audit frontend du 2026-11-16
  (`docs/validation/audits/2026-11-16-audit-frontend-rust.md`) relève deux
  failles : `handleStepFailure` (`apiService.js`) interpole `errorMessage` non
  échappé dans un fragment HTML du panneau de logs principal (AUD-FE-01), et
  `DOMUpdateUtils.escapeHtml` (`utils/DOMBatcher.js`) repose sur
  `div.textContent`/`div.innerHTML`, qui n'échappe ni `"` ni `'` et alloue un
  nœud DOM à chaque appel (AUD-FE-06).
- **Décision — échappement sans allocation DOM** : `escapeHtml` retourne `''`
  pour `null`/`undefined`, convertit via `String(text)` puis remplace dans
  l'ordre `&` → `&amp;`, `<` → `&lt;`, `>` → `&gt;`, `"` → `&quot;` et `'` →
  `&#39;`. L'implémentation est alignée sur `escapeHtmlWorker`
  (`logParserUtils.js`), conserve le rendu des nombres et couvre désormais les
  contextes d'attribut, notamment `data-key` dans `downloadsListManager.js`.
- **Décision — assainissement du message d'erreur** : `apiService.js` importe
  `DOMUpdateUtils` et calcule `safeMsg` avant
  `updateMainLogOutputUI(\`<i>Erreur d'initiation: ${safeMsg}</i>\`)`.
  `showNotification` et `statusEl.textContent` restent inchangés : le second
  échappe nativement et le premier ne construit pas de HTML.
- **Portée / invariants** : modification limitée à `assets/web/static/`,
  architecture zéro-build ES Modules, aucune dépendance Node/npm. Les
  sélecteurs contractuels (`#step-STEP*`, `.status-badge`, `#status-*`,
  `#progress-bar-*`, `.run-button`, `.cancel-button`) et les contrats
  HTTP/JSON/CSV sont inchangés. Le dépôt Python legacy reste en lecture seule ;
  l'écart est documenté dans `memory-bank/systemPatterns.md` §14.
- **Validation** : `node --check` sur les deux modules et cas d'échappement
  (balises, guillemets, apostrophes, `null`, `undefined`, nombres) ;
  `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu
  Minijinja ciblés, `cargo test --workspace` (689 listés, 687 exécutés + 2
  ignorés, 0 échec), `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts.
- **Portée / rollback** : un revert des deux modules JS restaure le
  comportement précédent ; aucune migration de données ni écriture dans
  `runtime/`, `/mnt/cache` ou le dépôt Python.

## 2026-11-16 — D-073 : modularisation des règles Antigravity et création de .antigravityignore

- **Contexte** : Dans l'IDE Google Antigravity, `.agents/rules/codingstandards.md`
  dépassait le plafond pratique de contexte par fichier et saturait le budget
  global des règles permanentes (`alwaysApply: true`, 17,4 Ko), provoquant
  l'erreur `You have exceeded the maximum token limit for this file`.
- **Décision — découpage et densification** :
  - `.agents/rules/codingstandards.md` a été refactorisé et densifié à 59 lignes
    (4,8 Ko, 572 mots) pour rester sous le seuil d'alerte IDE (~1 000 tokens), en
    éliminant les redondances avec `AGENTS.md` tout en conservant 100 % des
    invariants normatifs (stack, erreurs, atomicités, JSON, arrondis, isolation
    D-071, réseau, read-only du brut, verrou, qualité, anti-patrons). Le
    frontmatter legacy `alwaysApply: true` est remplacé par
    `trigger: model_decision`.
  - La section pipeline (§5) est extraite dans une règle dédiée
    `.agents/rules/pipeline-standards.md` (42 lignes, 4,0 Ko, 509 mots, `trigger: model_decision`)
    couvrant STEP1 à STEP8, les templates directs (SDM/LPHDF), le post-traitement
    A5/A6/A7 et le QA.
  - `.agents/rules/v5.md` a été refactorisé et condensé à 43 lignes (2,9 Ko, 380 mots)
    en fusionnant la classification et les flux d'exécution et en éliminant les
    redondances.
  - `.agents/rules/prompt-injection-guard.md` a été condensé à 54 lignes (3,2 Ko,
    439 mots) en synthétisant la défense contre les injections et le protocole
    destructif sans perte d'exigence.
  - Tous les fichiers de règles du dépôt sont désormais strictement sous 5,4 Ko.
- **Décision — exclusion Antigravity** : création de `.antigravityignore` à la
  racine pour exclure `target/`, `runtime/`, `assets/mission_overlays/*.mov`,
  `assets/models/*`, `parity/cases/*` et `vendor/*.so*` de l'indexation et du
  contexte de l'agent.

## 2026-11-16 — D-072 : alignement du standard de codage sur le dépôt

- **Contexte** : `.agents/rules/codingstandards.md` datait d'avant les modules
  `post/`, `qa/`, `vision/`, `template`, les sous-commandes CLI étendues et les
  décisions D-062 → D-071. L'audit section par section a relevé des omissions
  sur la structure des crates, les types d'erreurs, l'isolation `paths.base`, le
  post-traitement et le contrat QA.
- **Décision — structure et CLI (§2)** : `workflow-steps` documente désormais
  `template` (bypass SDM/LPHDF), `post/` (A5/A6/A7), `qa/` (read-only) et
  `vision/` ; `workflow-cli` liste `step1`…`step8`, `post`, `qa`, `retention`,
  `sync`, `webhook`, `email`, `download`, `history` et `config`. `workflow-core`
  porte aussi l'isolation, le parseur de missions et les écritures atomiques ;
  `workflow-ingest` porte le cache des missions email.
- **Décision — erreurs et sorties (§3)** : `thiserror` couvre les types publics
  `ConfigError`, `MissionError`, `StepError`, `ArchiveError`, `IngestError`,
  `SyncError` et `WorkflowServerError` ; `TpuError` garde son `Display` manuel ;
  `anyhow` reste aux frontières et dans STEP2/3/4/7/8 (D-026). Les sorties
  `POST_*`, `DIRECT_TEMPLATE_TOTAL`, `ARCHIVE_ONLY_TOTAL` et les JSON de
  commande sont des exceptions documentées ; les champs de contrat sont
  additifs.
- **Décision — isolation et exposition (§4, §8)** : le standard reprend D-071
  (chemins dérivés de `paths.base`, `WORKFLOW_PATHS_BASE`, purge limitée aux
  motifs du pipeline `.part`/`.tmp`/`.*.tmp.*`/`_temp_*`) et la règle
  `allow_remote = true` avec `secret_key` non vide et non préfixée `dev-`. Il
  interdit d'écraser le `.ecproj` brut, d'écrire hors des racines prévues et de
  contourner les gardes réseau.
- **Décision — pipeline (§5)** : ajout de deux sous-sections : template direct
  SDM/LPHDF (D-034, D-035, D-061), post-traitement A1–A7 avec `_assist.ecproj`,
  artefacts additifs, sidecar mission `schema: 2` (D-063, D-065, D-066, D-069),
  et QA read-only (`qa.json` 1.5.0, cache 12, masque MOV alpha, D-067).
- **Validation** : `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -- -D warnings` et `cargo test --workspace` (689 tests listés, 687 exécutés +
  2 ignorés, 0 échec) exécutés. Aucun changement de comportement : seuls des
  commentaires de module ont été alignés (`post/`, `qa/`, `step8`, `state`).
  Aucun `.ecproj`, `runtime/`, `/mnt/cache` ni dépôt Python legacy modifié. Les
  commandes et modules cités ont été vérifiés dans
  `crates/workflow-cli/src/main.rs` et `crates/workflow-steps/src/`.
- **Portée / rollback** : documentaire et commentaires. Un revert restaure la
  version précédente du standard ; aucun contrat JSON/CSV/HTTP n'est modifié.

## 2026-11-16 — D-071 : durcissement P2/P3 et isolation du runtime

- **Contexte** : l'audit backend du 2026-11-16
  (`docs/validation/audits/2026-11-16-audit-backend-rust.md`) laisse ouverts les constats
  P2/P3 AUD-04 à AUD-10 : découpage UTF-8 par octets, chemins absolus qui
  neutralisent `WORKFLOW_PATHS_BASE`, purge d'orphelins trop large, absence de
  PID dans le verrou, runtimes Tokio multi-thread répétés, `secret_key` non
  validée en accès distant et déduplications quadratiques.
- **Décision — parseur de missions (AUD-04, AUD-10)** : `parse_lenient_logos`
  retire les préfixes par nombre de caractères (`strip_chars` +
  `char_indices()`), jamais par longueur d'octets. `split_logo_tokens`,
  `extract_delivery_links` et `audit_missions` utilisent `HashSet` pour la
  déduplication ; `extract_delivery_links` emprunte les URL au corps au lieu de
  les cloner avant le test.
- **Décision — chemins dérivés (AUD-05)** : `config/config.toml` n'écrit plus
  `projects`, `archives`, `logs`, `downloads`, `history_db` ni
  `steps.step1.source_dir` lorsque les valeurs standard s'appliquent.
  `PathsConfig::resolve()` dérive les cinq premiers de `paths.base` ;
  `Config::step1_source_dir()` dérive le dernier de `paths.downloads` et reste
  prioritaire s'il est explicite. `WORKFLOW_PATHS_BASE` isole donc tout le
  runtime, y compris la source STEP1, sans réécriture textuelle du TOML. Le
  serveur, le moniteur d'ingestion et STEP1 consomment le getter ; la
  validation d'égalité `source_dir == downloads` est conservée. Le moniteur
  webhook reste activé en production : la recommandation retenue est
  l'isolation des chemins, pas la désactivation du moniteur.
- **Décision — orphelins et verrou (AUD-06)** : `purge_orphans_with` ne purge
  que `.part`, `.tmp` final et `.<name>.tmp.<pid>` ; un `.tmp` au milieu d'un
  nom client est conservé. `HardwareLock::acquire` écrit le PID courant après
  `flock` (`set_len`, `seek`, `flush`, `sync_all`) ; `Drop` conserve la
  libération exclusive. Le fichier de verrou reste un artefact de diagnostic,
  jamais une source de vérité pour l'exclusion.
- **Décision — CLI (AUD-07)** : `default_video_output` compare le suffixe en
  ASCII minuscule, comme `default_ae_output`. Les sous-commandes ponctuelles
  `webhook`, `email`, `download` et `load_missions_for_folders` passent par
  `block_on_cli`/`block_on_cli_result`, qui instancient un runtime Tokio
  `Builder::new_current_thread().enable_all()` au lieu d'un pool multi-thread.
- **Décision — accès distant (AUD-08)** : `validate_server` exige, lorsque
  `server.allow_remote = true`, une `server.secret_key` non vide, non blanche
  et non préfixée `dev-` ; le rejet des `worker_token` vides, par défaut ou
  `dev-` reste inchangé. En écoute loopback, une clé vide reste acceptée.
- **Décision — dette documentée (AUD-09)** : aucune nouvelle signature
  publique `anyhow::Result` n'est introduite dans `workflow-steps` ; D-025 et
  D-026 sont vérifiées et datées dans `parity/DRIFT.md`. Le contrat
  d'environnement passe en 1.10 pour la règle `secret_key`/`allow_remote`.
- **Conséquences** : `config show` affiche désormais `null` pour les chemins
  dérivés et une chaîne vide pour `steps.step1.source_dir` ; c'est attendu, la
  résolution se fait à l'usage. Les configurations qui explicitaient ces
  chemins restent valides et prioritaires. `validate_isolation()` est
  inchangé ; `scripts/validate-step1.py` continue de fonctionner puisque son
  remplacement textuel de `runtime` porte encore sur `paths.base` et que
  `WORKFLOW_PATHS_BASE` couvre le reste.
- **Validation** : `cargo test --workspace` 689 tests listés (687 exécutés +
  2 ignorés), 0 échec ; `workflow-core` 68/68 ; `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts ;
  `WORKFLOW_PATHS_BASE=/tmp/opencode/audit-p2-runtime ... config check`
  valide ; `scripts/validate-step1.py` exécuté avec succès. Aucune écriture
  dans `runtime/`, `/mnt/cache` ou le dépôt Python legacy.
- **Portée / rollback** : correctifs locaux. Réintroduire les chemins
  explicites et l'ancien prédicat `contains(".tmp")` rétablit le comportement
  précédent ; retirer la validation `secret_key` et le runtime current-thread
  rétablit AUD-08/AUD-07. Aucun format JSON, CSV, `.ecproj` ni code HTTP n'est
  modifié.

## 2026-11-16 — D-070 : durcissement P1 de workflow-ingest

- **Contexte** : l'audit de robustesse du backend Rust
  (`docs/validation/audits/2026-11-16-audit-backend-rust.md`) identifie trois constats P1
  dans `workflow-ingest` : contournement du filtre anti-SSRF par IP littérale
  (AUD-01), `std::sync::Mutex` avec `.expect()` et SQLite bloquant dans un
  contexte async (AUD-02), absence de `fsync` avant et après le renommage
  final (AUD-03).
- **Décision — anti-SSRF (AUD-01)** : `validate_url_host` refuse toute IP
  littérale non publique (`is_public_ipv4` / `is_public_ip`) sur l'URL
  initiale, avant `probe` et avant `GET`. La politique
  `reqwest::redirect::Policy::custom` réapplique le même filtre à chaque saut
  et conserve la borne de trois redirections de l'ancien `limited(3)`.
  `allow_private_hosts = true` reste l'échappatoire explicite des tests locaux
  et des déploiements internes ; un refus produit `ErrorKind::Auth` sans
  contacter le réseau ni créer de `.part`.
- **Décision — verrous et async (AUD-02)** : `HistoryRepository`,
  `EmailMissionStore` et le cache d'index de `EmailProxyClient` passent à
  `parking_lot::Mutex`, qui ne s'empoisonne pas : les `.expect("... verrouillé")`
  disparaissent. Dans `EmailProxyClient::sync`, `needs_detail` et `upsert`
  sont exécutés par `tokio::task::spawn_blocking` ; un `JoinError` est converti
  en `IngestError::Message` et remonte dans `report.errors`, sans bloquer un
  worker Tokio pendant le `busy_timeout` de 30 s.
- **Décision — durabilité (AUD-03)** : `stream_to_part` exécute
  `file.sync_all().await` après `flush` ; une erreur reste un
  `ErrorKind::LocalIo` avec l'offset courant. `finalize_download` appelle
  `workflow_core::fsutil::sync_parent(target_path)` juste après le `rename`
  atomique ; l'échec de fsync du répertoire reste non bloquant, comme dans
  `fsutil::publish`.
- **Conséquences** : aucune signature publique hors `workflow-ingest` ne
  change. `url = "2"` devient une dépendance directe du crate (déjà
  verrouillée dans `Cargo.lock` via `reqwest`) pour nommer `url::Host`. Les
  tests du service d'ingestion activent explicitement
  `allow_private_hosts = true` pour leurs mocks sur `127.0.0.1` ; les tests de
  téléchargement couvrent le refus direct, le refus de redirection et la
  finalisation durable.
- **Validation** : `cargo test -p workflow-ingest` 55/55 ; suite workspace
  678 tests listés (676 exécutés + 2 ignorés), 0 échec ; `cargo fmt --check` et
  `cargo clippy --workspace --all-targets -- -D warnings` verts. Aucun
  fichier de `runtime/`, `/mnt/cache` ou du dépôt Python legacy modifié.
- **Portée / rollback** : correctifs additifs et locaux au crate. Retirer
  `validate_url_host`/`redirect_policy` et revenir à `Policy::limited(3)`
  rétablit le comportement AUD-01 ; revenir à `std::sync::Mutex` et aux appels
  directs dans `sync` rétablit AUD-02 ; supprimer les deux synchronisations
  rétablit AUD-03.

## 2026-11-13 — D-069 : A7 — repli central sur asset local corrompu

- **Contexte** : les lots 183 et 186 Camille contiennent un `docs/LFAUIT.png`
  factice de 15 octets (`logo de la base`, sans signature PNG). `resolve_target`
  le trouvait sur disque et renvoyait `LocalDocs` ; la sonde A7 échouait ensuite
  (`asset_unreadable`), mais le repli central n'était jamais retenté. Le sidecar
  sortait en `review` (`missing_asset`, `added: []`) alors que
  `assets/mission_overlays/LFAUIT.png` est un PNG 1080×1920 sain, au SHA-256
  conforme au manifeste.
- **Décision — niveau d'intervention** : le correctif reste dans A7
  (`workflow-core::mission` + `workflow-steps::post::a7`). Le projet brut
  `.ecproj`, les assets clients de `docs/` et `MediaSolution.jsx` ne sont pas
  modifiés (seul le sidecar A7 est ajouté à `docs/`) : D-064 impose un brut
  immuable, D-022 fige le JSX et aucun fichier de `/mnt/cache` ne doit être
  assaini par le pipeline. Le projet brut conserve donc l'échec d'import
  d'`effectcraft-cli` ; seul `_assist.ecproj` est réparé, ce qui reste le rôle
  de l'assistance A7.
- **Décision — résolution** : `LogoResolver::resolve_fallback_target` interroge
  la banque avec la même sémantique que `resolve_target` (exact, normalisé,
  extension implicite, alias) sans jamais retomber sur `docs/`, et force la
  provenance `central_fallback`.
- **Décision — reprise A7** : quand un asset `LocalDocs` échoue à la sonde
  (`asset_unreadable` ou `asset_dimensions`), A7 sonde immédiatement son
  homologue central. En cas de succès, l'asset est retenu pour le chutier et la
  composition avec `source = "central_fallback"` ; le sidecar reste `schema: 2`
  et reçoit un avertissement additif
  `local_asset_corrupted_fallback_used` dans `warnings` (même forme
  `code`/`line`/`detail` que les avertissements du parseur). La décision reste
  `apply` si aucun autre asset ne manque : un asset officiel de la banque
  satisfait la mission.
- **Dégradation** : banque absente, désactivée, ou asset central illisible =
  `MissingAsset` local initial conservé, détail enrichi de l'échec du repli ;
  sidecar `review` (`missing_asset`), exit 0, aucun calque. Aucun panic, aucune
  boucle : le repli n'est tenté que depuis `LocalDocs`, jamais depuis
  `CentralFallback`.
- **Conséquences** : un asset local corrompu, tronqué ou hors profil ne masque
  plus un asset central sain. Les sidecars produits restent lisibles par les
  consommateurs schéma 2 ; les anciens ignorent simplement le warning additif.
  Aucun changement de contrat JSON, de clé de sidecar ni de chemin `.ecproj`.
- **Validation** : échantillon réel extrait de `183 Camille_brut.zip` sous
  `/tmp/opencode` (15 octets, SHA-256 `52ce014e…b063a4`) ; banque `LFAUIT.png`
  vérifiée 35 582 octets, SHA-256 `cc9a036f…d4e10a`. Tests :
  `fallback_target_bypasses_a_corrupt_local_asset`,
  `corrupt_local_asset_falls_back_to_central_bank`,
  `invalid_local_profile_falls_back_to_central_bank`,
  `corrupt_local_asset_without_fallback_degrades_to_review`,
  `corrupt_fallback_keeps_local_error_and_reviews`. Suite workspace :
  675 tests listés (673 exécutés + 2 ignorés), 0 échec ; `fmt` et
  `clippy -D warnings` verts. Aucun `.ecproj`, `runtime/` ou `/mnt/cache`
  modifié.
- **Portée / rollback** : additif. Retirer `resolve_fallback_target` et la
  reprise de `resolve_assets` restaure le comportement D-066 (asset local
  invalide → revue). Les warnings supplémentaires n'affectent ni `reasons`, ni
  `missing`, ni les décisions d'audit.
- **Références** : D-064, D-065, D-066, D-067,
  `docs/lots/LOT-12-A7-MISSIONS.md`,
  `docs/lots/LOT-12.2-BANQUE-CENTRALE.md`,
  `assets/mission_overlays/README.md`.

## 2026-11-12 — D-068 : archivage de la memory-bank et fenêtre de rétention

- **Contexte** : `activeContext.md` (1 166 lignes), `decisionLog.md`
  (2 234 lignes) et `progress.md` (649 lignes) mêlaient l'état courant à
  l'historique des lots B2 → 12.2, aux audits d'octobre 2026 et aux brouillons
  D-058 ; la reprise devenait coûteuse et le protocole prévoit 90 jours de
  détail (`.agents/rules/memorybankprotocol.md` §3).
- **Décision** :
  1. Créer `memory-bank/archives/` et y déplacer **verbatim** l'historique :
     `sessions-2026-10-to-11.md`, `decisionLog-archive-2026-10.md` et
     `progress-archive-2026-10.md`, avec en-tête de provenance (fichier
     source, plage de lignes, SHA-256, date).
  2. Garder les cinq fichiers du protocole à la racine ; l'actif ne conserve
     que l'état courant, les décisions D-041 → D-068 en texte intégral, le
     tableau D-001 → D-027 et les jalons.
  3. Ordonner le journal actif du plus récent au plus ancien
     (D-068 → D-041) ; D-059 reste réservé (option C A1, non ouverte).
  4. Traiter les archives comme lecture seule : une décision nouvelle se
     consigne dans le journal actif avec un renvoi daté vers l'archive.
- **Conséquences** : `activeContext.md` passe de 1 166 à 134 lignes,
  `decisionLog.md` de 2 234 à 1374 et `progress.md` de 649 à 76 ; le
  détail technique reste consultable par lien `file://` et par
  `docs/lots/LOT-*.md`. Aucun contrat JSON, CSV ou HTTP modifié ; aucun code,
  `.ecproj`, `runtime/`, `config/config.toml` ni `/mnt/cache`.
- **Références** : `.agents/rules/memorybankprotocol.md` §2 et §3,
  `memory-bank/archives/*.md`.

## 2026-11-12 — D-067 : Lot 12.3 — benchmark, masque QA MOV et canari

- **Contexte** : après D-066, la banque et la résolution étaient validées par
  tests, mais il manquait un benchmark consolidé du corpus, le masque QA des
  habillages `.mov` et une validation de bout en bout sur des missions réelles
  récentes.
- **Décision — benchmark** : `email audit --json` expose en plus un champ
  additif `format_cohorts` (mois × `html_only`/`text_plain`). Le corpus de
  932 emails donne 2 469 tokens, 2 228 résolus, 47 absences volontaires,
  193 manquants, 1 ambigu et 39 instructions non résolues. Historique
  (2025-03 → 2026-09) : 785 emails, 90,9 % de résolution hors absences ;
  récent (2026-10/11) : 147 emails, 100 % hors absences, tous formats.
  `scripts/audit-a7-benchmark.py` consolide ces strates sans réseau ni base.
- **Décision — masque MOV** : le masque d'overlays lit aussi les calques
  `.mov` porteurs d'alpha (une vidéo opaque reste du contenu). 12 à 24 frames
  sont échantillonnées en une passe `ffmpeg` rawvideo RGBA entre le début et
  la fin du calque ; la boîte alpha médiane sert de référence, les frames
  d'IoU ≥ 0,70 sont fusionnées et au moins 75 % des frames visibles doivent
  être stables. La région publiée porte `kind: "mov"` ; un MOV instable est
  ignoré avec un warning non bloquant. La lecture de l'asset est ramenée au
  temps source (un end-card de 3 s placé à 44 s est lu sur 0–3 s), sinon
  `ffmpeg` ne renvoie aucune frame.
- **Décision — contrat QA** : `OverlayRegion.kind` est ajouté en fin d'objet
  avec `#[serde(default)]` (`image` pour les anciens rapports),
  `QA_SCHEMA_VERSION` passe à **1.5.0** et `CACHE_VERSION` à **12**. Le masque
  reste non bloquant : il écarte des détections, ne provoque ni `fail` ni
  veto.
- **Correctif parseur** : un email réel écrit `... trompe-l'œil .mp4` (espace
  avant l'extension). `stem_key` est désormais `trim()`é après retrait de
  l'extension ; le test `matches_filename_with_space_before_extension` couvre
  le cas. Le lot 20 passe de `video_not_in_mission` à `apply`.
- **Canari** : 20 missions réelles d'octobre–novembre 2026, une vidéo chacune,
  rejouées sur des copies du projet brut 179 sous `/tmp/opencode` avec la
  banque active, `a2_mission_mask = true`, `--report-only` et un store email
  temporaire (proxy local injoignable). Résultat : 20/20 `apply`, 28 assets
  ajoutés (18 `central_fallback`, 10 `local_docs`), 0 manquant, 0 doublon,
  0 `_assist.ecproj` écrit, exit 0. Cinq cas limites contrôlés valident
  `known_absent`, asset manquant, ambiguïté (`MSL1`), `tail_overlay`
  (`MOTION_FIN_EM6_V03.mov`, `tail_s = 0`) et déduplication (`sdm.png` déjà
  présent). Seul warning : masque A2 de `MSL1.mov` instable (7/24 frames),
  ignoré sans bloquer la mission.
- **Validation** : 18 tests `post::overlay`, 28 tests `mission`, suite
  workspace **670 tests listés (668 exécutés + 2 ignorés)**, 0 échec ;
  `fmt` et `clippy -D warnings` verts. Aucun `.ecproj`, `runtime/` ou
  `/mnt/cache` modifié.
- **Portée / rollback** : additif. Le bump de schéma invalide une fois le cache
  QA ; le masque MOV privilégie le sous-masquage au sur-masquage sur un logo
  animé. Retirer les calques MOV du masque revient à ignorer les
  régions `kind: "mov"` sans toucher au reste du rapport.

## 2026-11-12 — D-066 : Lot 12.2 — banque centrale de repli A7

- **Contexte** : A7 (D-064/D-065) ne résolvait que les assets présents dans
  `<projet>/docs`. Quand un email demandait un logo non joint par le client
  mais présent dans la banque de référence Média Solution, la mission partait
  en revue. Le repli devait rester optionnel et ne jamais bloquer le pipeline.
- **Décision — configuration** : ajouter `paths.mission_overlays`
  (`Option<PathBuf>`, vide/absent par défaut) dans `PathsConfig` et
  `ResolvedPaths`. `config check` échoue si le chemin est configuré mais que
  le répertoire n'existe pas ; une valeur vide désactive le repli. La
  configuration de production utilise `assets/mission_overlays`, relatif au
  workdir du serveur (racine du dépôt).
- **Décision — banque** : déployer les 117 assets réels dans
  `assets/mission_overlays/` pour obtenir des chemins `.ecproj` stables
  (`../../assets/mission_overlays/...`). Les 109 PNG (5 Mo) sont versionnés ;
  les 8 MOV ProRes (2,2 Go) sont gitignorés. `manifest.toml` fige dimensions,
  durée, codec, pixel format, audio, taille et SHA-256 ; `README.md` documente
  le déploiement et les limites.
- **Décision — résolution** : ordre local exact → local normalisé → banque
  exacte → banque normalisée → extension implicite (locale puis banque) →
  alias → `known_absent` → revue. Un asset local valide gagne toujours ; une
  ambiguïté locale (`MSL1` PNG+MOV) n'est pas masquée par la banque. Chaque
  asset central repasse `probe_asset` (1080×1920 alpha, MOV compatible).
- **Décision — provenance** : `LogoProvenance` (`local_docs` /
  `central_fallback`) est portée par `LogoTarget`, `Asset`, `A7Added` et le
  sidecar `schema: 2`. Un asset central non conforme reste en revue.
- **Dégradation** : banque absente ou illisible au runtime = repli ignoré,
  local puis `review` ; `config check` signale seulement la configuration
  invalide. Aucun avertissement bloquant, exit 0.
- **Validation** : 27 tests mission, 18 tests A7, 1 test de propagation
  `PostOptions`, validation `paths.mission_overlays` ; banque seule
  (`source: central_fallback`, chemin `../../assets/mission_overlays/...`),
  priorité locale et banque absente → `review`. `config check` vert sur la
  configuration réelle. Suite workspace : 661 tests listés (659 exécutés +
  2 ignorés), `fmt` et clippy verts. Aucun
  `.ecproj`, `runtime/` ou `/mnt/cache` modifié.
- **Portée / rollback** : additif. Retirer `mission_overlays` de la
  configuration désactive le repli ; les chemins locaux et le sidecar restent
  compatibles. Les MOV non versionnés doivent être recopiés après un checkout.

## 2026-11-12 — D-065 : Lot 12.1 — parseur de missions, alias et `tail_overlay`

- **Contexte** : A7 (D-064) était fragile sur le corpus client réel : 570 des
  932 emails sont HTML-only, 569 contiennent `\'`, des noms de vidéos et des
  listes de logos sont coupés sur plusieurs lignes, certaines parenthèses sont
  déséquilibrées ou contiennent un titre, et les alias historiques
  (`LRDLR`, `EM6`, `CEC`, `SI ON SE RENCONTRAIT`) n'étaient pas résolus. La
  banque compte désormais 117 assets (109 PNG + 8 MOV), dont `eqa.png`,
  `LRDLR.png`, `les invincibles.png`, `eqc.png`, `AVDP.png` et `em6.png`.
- **Décision — parseur** : `logical_lines` fusionne jusqu'à 4 lignes
  physiques / 600 caractères tant que l'instruction est incomplète (extension
  vidéo absente, parenthèses non équilibrées, continuation `Logo …` ou `(`).
  `split_parenthetical` ferme par équilibrage, `unescape_mission_text`
  remplace `\'`, `parse_lenient_logos` accepte `Logo <nom>` et
  `parse_bare_asset_list` accepte une parenthèse composée uniquement de tokens
  d'assets (`(yaq.png)`). Un titre (`(2)`) ou une note libre n'est jamais
  converti en logo.
- **Décision — alias** : table `const` fermée dans `workflow-core::mission`
  couvrant les 22 acronymes officiels et les variantes historiques. Résolution
  ordonnée : exact, nom normalisé unique, extension implicite, puis alias.
  `LRDLR.png` et `em6.png` priment sur leurs alias ; `MSL1` sans extension
  reste ambigu (PNG + MOV) ; `lphdf.png` et `edc.png` sont des
  `known_absent` neutres, pas des oublis client. Un token coupé peut être
  recollé sur 2–3 tokens (virgule ou `et`) seulement si le nom reconstitué
  existe.
- **Décision — `tail_overlay`** : nouveau `PlacementMode::TailOverlay` pour un
  MOV explicitement demandé. Calque de `max(0, main - d)` à `main`,
  `tail_s = 0`, comp non prolongée, audio coupé. Si `d > main + 1 ms`, revue
  `short_video_for_tail_overlay`, aucun rognage. Aucun déclenchement
  automatique `stem_contains("EM6")` : `MOTION_FIN_EM6_V03.mov` n'a aucune
  occurrence dans les 932 emails.
- **Décision — registre MOV** : `const` dans `a7.rs`. `m6+.mov` = `end_card` ;
  `MOTION_FIN_EM6_V03.mov` = `tail_overlay` ; `MSL1.mov`,
  `9_16_RDV_PROGRAMME_EM6_V02_5MIN.mov`, `the power.mov`,
  `hayu x below deck.mov` = `full_duration` ; `m6+max anime 1ep.mov` et
  `W9_M6+MAX.mov` (19,72 s) = `review` jusqu'à arbitrage.
- **Décision — artefacts** : sidecar `schema: 2` avec `requested` et `source`
  (`local_docs` ; `central_fallback` au sous-lot 12.2) par entrée `added`, et
  `known_absent` dans le bloc `mission`. `audit.json` ajoute
  `mission_known_absent` et filtre les absences volontaires de
  `mission_assets` ; le QA les traite en neutre.
- **Décision — audit** : `workflow-cli email audit --json-dir <dir> --assets
  <dir> [--json]`, hors ligne, sans base ni écriture ; rapport par cohorte
  mensuelle et top des manquants.
- **Validation** : 25 tests mission, 14 tests A7, 1 test QA ; suite workspace
  654 listés (652 exécutés + 2 ignorés), `fmt` et `clippy -D warnings` verts.
  Audit réel : 932 emails, 2 477 instructions, 2 469 tokens, 2 228 résolus,
  47 absences volontaires, 193 manquants, 39 non résolues ; octobre 2026
  100 % hors absences, novembre 2026 100 %. Aucun `.ecproj`, `runtime/` ou
  `/mnt/cache` modifié.
- **Portée / rollback** : additif. Retour à D-064 = retirer
  `TailOverlay`/`MovPolicy`, revenir au sidecar schéma 1 et supprimer
  `email audit`. Les sidecars schéma 1 restent lisibles (champs additifs
  absents). Banque centrale (12.2), benchmark complet et masque QA MOV (12.3)
  restent à faire.

## 2026-11-12 — D-064 : logos de mission A7 (emails `email_proxy.php`)

- **Contexte** : les emails Média Solution décrivent, par lot, les vidéos et
  les logos à incruster (`Logos requis: …`) ou `Pas de logo`. Ces informations
  étaient appliquées manuellement après STEP8. Le pipeline doit pouvoir les
  exploiter sans jamais bloquer une production quand l'endpoint
  `email_proxy.php` est absent, lent, en erreur ou renvoie `[]`.
- **Décision** : ajouter une passe A7 strictement additive au post-traitement
  STEP8 (`crates/workflow-steps/src/post/a7.rs`). Elle résout la mission du lot
  à la demande depuis un cache SQLite dédié (`email_missions.sqlite3`), valide
  les assets (PNG/MOV 1080×1920 avec alpha), puis insère les calques dans le
  seul projet `_assist`. Le brut n'est jamais modifié et aucun calque humain ou
  A6 n'est supprimé. Trois interrupteurs : `[email_proxy] enabled` (réseau +
  cache), `steps.step8.post.mission_overlay` (écriture) et
  `steps.step8.post.a2_mission_mask` (masque A2). `post run`/`step8 run`
  ajoutent `--no-email` (isolation totale) et `--no-mission-overlay` (mission
  auditée, `added: []`). Le seuil `mission_end_card_max_s` (défaut 5,0 s)
  distingue end-card et MOV ambigu.
- **Classification** : MOV `d >= main - 1 ms` = `full_duration` ; MOV court
  (`<= 5 s + 1 ms`) avec au moins un PNG = `end_card` (comp prolongée) ; MOV
  court seul, MOV intermédiaire ou plusieurs MOV = `review` sans insertion du
  MOV ; les PNG valides restent insérés. Premier asset de l'email = calque le
  plus haut, end-card au-dessus des PNG, déduplication par nom normalisé.
  `Pas de logo` ne crée rien et ne supprime rien ; un `sdm.png` A6 déjà présent
  est signalé en `review` sans être retiré.
- **Artefacts** : sidecar `<docs>/<stem>_mission.json` écrit uniquement si
  l'assist est publié ; bloc `mission` ajouté **après** `summary` dans
  `post.json` (ordre des clés préservé) ; contrôles `mission_*` dans
  `audit.json`. Le QA découvre le sidecar, expose `mission_overlays` et passe
  en `QA_SCHEMA_VERSION = 1.4.0` ; le cache QA passe en version 11 et hash le
  sidecar.
- **Validation** : 634 tests exécutés (2 ignorés), `fmt` et
  `clippy -D warnings` verts. Rejeux sur copies `/tmp/opencode` : 136
  end-card `44,4 → 47,4 s` (19 calques, frame RGBA à 45 s), 80 `MSL1.mov`
  plein durée `0–79,2 s` et `Pas de logo` à 0 calque, 131 déduplication A6
  (`added: []`, `skipped: ["sdm.png"]`, 4 calques), 158 trois `Pas de logo`,
  89 PNG statique `0–85,84 s`, 999 `teva.png` absente (`review`, exit 0,
  audit `warn`). `--no-email` conserve 44,4 s / 16 calques sans sidecar ni
  `a7_ms` ; `--no-mission-overlay` conserve 44,4 s avec sidecar et audit
  `warn`. Les projets et médias de production n'ont pas été modifiés.
- **Portée / rollback** : A7 est désactivé par défaut
  (`email_proxy.enabled = false`, `mission_overlay = false`). Rollback =
  remettre ces deux clés à `false` puis redémarrer ; les sidecars et les
  calques déjà produits restent additifs et peuvent être ignorés. Aucune
  migration de `download_history.sqlite3`, aucune modification du panneau CEP
  ni de `MediaSolution.jsx`.
- **Références** : D-041, D-052, D-061,
  `docs/lots/LOT-12-A7-MISSIONS.md`,
  `crates/workflow-core/src/mission.rs`,
  `crates/workflow-ingest/src/email.rs`.

## 2026-11-11 — D-063 : veto latéral A1 (pillarbox)

- **Contexte** : l'étude `Corpus_Exemple_3` (248 projets, 9 359 segments
  appariés) isole 7 faux applies A1 sur 380 `apply` appariés (1,84 %), tous
  des sources pillarboxées. `a1.rs` calculait `horizontal` mais ne l'utilisait
  pas comme veto ; sa définition exige les deux côtés et un équilibre à 50 %,
  alors que `134 Camille — Segment 24` n'a qu'une barre gauche de 64 px. Le
  cropdetect trouve un letterbox vertical (parfois aidé par une scène
  nocturne), A1 zoome sur la hauteur, mais la largeur active ne couvre pas la
  composition ; le monteur a laissé 180 %.
- **Décision** : dans `decide_letterbox`, après les vetos scale/Position
  Y/couverture et après `noop`/`a1_defer_on_a3`, une barre
  `left >= lateral_min` **ou** `right >= lateral_min` rétrograde l'`apply` en
  `review` (`confidence = 0.4`) avec le motif « barres latérales (gauche
  X px, droite Y px) : recadrage non fiable ». Le seuil est normalisé :
  `lateral_min = arrondi(32 × hauteur_source / 1080)`, minimum 1 px, soit
  32 px en 1080p, 21 px en 720p et 64 px en 4K. `details.lateral_min` est
  ajouté pour l'audit, à côté de `details.bars`. Le veto ne touche ni `noop`,
  ni keyframes, ni letterbox intermittent/variable, ni les revues A3, et ne
  peut pas être contourné par `a1_refine_native`. Aucune clé de configuration
  n'est ajoutée.
- **Validation** : 598 tests listés (596 exécutés + 2 ignorés), `fmt` et
  `clippy -D warnings` verts, dont 9 nouveaux tests A1 (barre gauche seule
  64/0, 164/412, 120/24, 20/15 sous le seuil, `noop` et keyframes préservés,
  seuils 720p/4K). Rejeu réel des 6 projets / 7 segments du corpus 248 en
  `post run --report-only` sur le miroir : 7/7 passent `apply` → `review`
  avec le motif attendu, 0 autre changement d'action sur 305 décisions A1.
  Rejeu déterministe des 382 `apply` A1 : 7/7 faux applies couverts,
  19 bons applies rétrogradés sur 380 appariés (dont 12 à ±2 pts), 354
  applies appariés sans barre ≥ 32 px inchangés ; 1 apply non apparié
  rétrogradé. Contrôle visuel des 7 frames : toutes pillarboxées. Aucun
  `.ecproj` du corpus, `runtime/` ou `/mnt/cache` modifié.
- **Portée / rollback** : A1 du post-STEP8 uniquement, sans changement de
  schéma `post.json` hors détails A1. Rollback = revert du commit ; aucune
  migration de données ni configuration à défaire. Le coût mesuré est de
  19 revues supplémentaires sur 380 applies, sans perte de zoom.
- **Références** : D-056, D-057, D-058, D-062,
  `docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md` (3.5, 5.6),
  `docs/lots/LOT-11-A1-VETO-LATERAL.md`.

## 2026-11-11 — D-062 : activation pilote de `a2_logo_prior`

- **Contexte** : l'étude `Corpus_Exemple_3` (248 projets, 9 359 segments
  appariés) chiffre A2 : 9 `apply`, 287 segments conservés, 0 revert, mais
  13 recadrages de bandeau manqués sur 175 revues mesurables. La shadow
  evaluation des flags A2 sur les 248 projets montre que le prior crée
  **1 `apply` correct** (`257 Camille — 66M VENTES`, 218,2 % contre 215,3 %
  humain, +2,9 points), 0 `apply` perdu, 0 faux apply, 2 fausses revues
  (`108`, `243`) et 15 reformulations de motif, sans coût CPU mesurable.
  Le détecteur YOLOX ne change aucune action, confirme 41/199 actions de
  revue toutes étroites (0 des 38 actions de bandeau) et coûte 2 313 ms/
  projet en moyenne (médiane 1 858,5 ms, max 5,34 s).
- **Décision** : `config/config.toml` passe à `a2_logo_prior = true` en
  pilote contrôlé ; `a2_logo_detector` reste `false`. Aucun changement de
  code : le flag active le comportement livré par D-052. Le pilote surveille
  les `apply` créés, les revues « logo source uniquement » et les faux
  applies ; retour à `false` au premier faux apply ou si la charge de revue
  dépasse le gain.
- **Validation** : run shadow 248/248, `rc=0`, 245 audits `pass`, 3 `warn`
  `source_coverage`, 0 `fail` ; corpus en lecture seule, aucun `.ecproj`
  modifié. `config check` exécuté après la bascule.
- **Portée** : A2 du post-STEP8 et la configuration de production
  uniquement. Les 12 autres cibles A2 manquées restent hors de portée
  (bandes larges, hautes, sombres, animées, sous-titres). Le veto latéral A1
  (7 faux applies pillarbox sur 380) est une décision séparée, formalisée
  dans `docs/lots/LOT-11-A1-VETO-LATERAL.md`.
- **Références** : D-052, D-055, D-060, D-061,
  `docs/etudes/ETUDE-CORPUS-248-POST-STEP8.md`,
  `docs/decisions/DECISION-A2-LOGO-PRIOR.md`,
  `docs/lots/LOT-11-A1-VETO-LATERAL.md`.

## 2026-11-09 — D-061 : support multi-préfixes du template SDM

- **Contexte** : `/mnt/cache/116 Camille/docs/SDM - Liliane et josé.mp4`
  porte le préfixe `SDM - ` alors que `steps.step8.post.template_prefix`
  valait `M6 - SDM - `. L'audit STEP8 concluait `a6_template = pass`
  (« aucun template demandé ») et la vidéo traversait inutilement
  STEP2–STEP6 puis un recadrage standard A1/A2 au lieu du template A6.
- **Décision** : `PostConfig.template_prefixes: Vec<String>` remplace le
  scalaire, avec `["M6 - SDM - ", "SDM - "]` par défaut. `Bypass`,
  `a6::validate`, Stage 0 QA et `post run` acceptent la liste ; le préfixe
  effectivement matché est écrit dans `template.prefix` (`post.json`) et
  dans les détails de la décision A6. L'ancien champ `template_prefix` reste
  accepté comme alias Serde (scalaire ou liste). `config check` refuse les
  préfixes vides, les chevauchements entre préfixes de template et avec
  `archive_only_prefixes`.
- **Validation** : 589 tests listés (587 exécutés + 2 ignorés), `fmt` et
  `clippy -D warnings` verts ; rejeu réel sous `/tmp/test-sdm` sur
  `116 Camille` : les nommages `SDM - ` et `M6 - SDM - ` donnent
  `a6_eligible = 1`, `template_applied = true`, 4 calques A6 (180 %,
  84,3497774862575 %, 56,294573830734535 %, logo 100 %), audit `pass`,
  aucune décision A1–A4 ; `config check` accepte la configuration de
  production et l'ancien scalaire.
- **Portée** : aucun changement A1–A5 ni nouvel artefact ; `post.json` et
  `audit.json` gardent leur schéma, seul `template.prefix` reflète le
  préfixe matché (vide si aucun). Le cache QA passe en version 10.
- **Références** : D-034, D-035, `docs/lots/LOT-9-POST-STEP8.md`,
  `docs/lots/LOT-10-QA.md`, `crates/workflow-core/src/config.rs`,
  `crates/workflow-steps/src/template.rs`,
  `crates/workflow-steps/src/post/a6.rs`,
  `crates/workflow-steps/src/post/run.rs`,
  `crates/workflow-steps/src/qa/stage0.rs`.

## 2026-11-09 — D-060 : Grounding DINO hors production A2

- **Décision** : Grounding DINO **n'entre pas** dans A2 en production. Aucun
  flag `a2_grounding_dino`, aucune clé TOML, aucun appel de modèle dans
  `post/a2.rs`. Le détecteur ouvert reste un outil de **laboratoire** pour
  proposer des logos inconnus au catalogue `source_logos.toml` et qualifier
  les revues A2. Backend local PyTorch 480 px retenu pour le laboratoire ;
  Replicate écarté pour la production (données client, réseau, secret) et
  non exécuté faute de `REPLICATE_API_TOKEN` ; ONNX non prêt (pas de config
  d'export Optimum, ops de deformable attention, encodeur BERT).
- **Spike** : `IDEA-Research/grounding-dino-tiny` (Swin-T, 172,3 M
  paramètres, 659 Mio), 242 frames (122 logos du catalogue, 60 bandeaux A2,
  60 frames négatives), 7 prompts, 3 résolutions, 592 inférences.
  Prompt composite `logo . text . watermark . lower third . banner` :
  - logos : **96,7 %** à IoU ≥ 0,50 au seuil 0,20 (118/122, IC 95 %
    91,9–98,7 %), **91,8 %** à 0,25 (112/122) ; IoU médian 0,90 ;
  - bandeaux : **98,3 %** à 0,20 (59/60), **91,7 %** à 0,25 (55/60) ;
    texte IoU ≥ 0,30 sur **97,2 %** (35/36) à 0,20, 77,8 % à 0,25 ;
  - faux positifs : **25 %** des 60 frames négatives à 0,20, 20 % à 0,25 ;
    après clustering IoU ≥ 0,50 + centre ≤ 10 px + persistance ≥ 70 %,
    **0/5 clips** négatifs persistants (maximum observé 16,7 %, IC 95 %
    [0 ; 43,4 %]) ;
  - persistance : **5/5** clips positifs avec logo ≥ 70 % à 0,20 (100,
    100, 100, 100, 91,7 %) ; **3/5** avec texte ≥ 70 % (54 et 58 à
    100/91,7 %, 40 CITROUILLES à 83,3 % ; 40 ABHARTACH/FOOTBALLEUR à
    66,7/50 %, cohérent avec leurs bandeaux animés déjà en `review`) ;
  - coût : **1,279 s/frame** à 480 px (1,47 Gio de VRAM, 3,48 s de
    chargement), 1,320 s/frame pour l'ablation 480, 2,024 s à 640,
    3,053 s à 800 ; 12 frames = 15,3 s à chaud (18,8 s avec chargement),
    6 frames = 7,7 s.
- **Limites** : confusion logo/bandeau (boîtes « logo + lower third » sur
  Enquête d'action, Enquête Exclusive, Zone Interdite, Les Traîtres) ;
  corpus de logos curé et non indépendant ; cinq clips négatifs seulement,
  sans décor statique piégeur ; deux styles de bandeau (UJUD, 66 Minutes) ;
  un seul modèle (tiny) et une seule carte (GTX 1650 4 Gio) ; Replicate non
  mesuré ; ONNX non exporté.
- **Règles si réouverture** : Grounding DINO ne peut ni créer un `apply`, ni
  retirer un veto, ni modifier `scale_target` ; il annote
  `details.grounding_dino` (branche logo serrée ≤ 0,35, branche texte
  ≥ 0,20 et y ≥ 0,60, persistance ≥ 70 %, `unknown_logo`,
  `banner_confirmed`, `logo_merged_with_lower_third`) et A2 décide. Le
  sidecar local doit prendre le verrou matériel ou vérifier qu'aucun
  STEP2/3/4 ne tourne (1,47 Gio de VRAM sur 3,81 Gio utilisables).
- **Réouverture** : export ONNX/TensorRT ou moteur Rust natif validé ;
  corpus indépendant ≥ 20 clips négatifs et ≥ 20 logos inconnus avec
  rappel ≥ 90 % à IoU 0,50, centre ≥ 95 %, ≤ 5 % de clips négatifs
  persistants ; latence ≤ 8 s/6 frames ou ≤ 16 s/12 frames ; pilote
  `report_only` sans faux `apply`.
- **Preuves** : `docs/etudes/ETUDE-GROUNDING-DINO-A2.md`,
  `docs/etudes/ETUDE-GROUNDING-DINO-A2/results/`,
  `scripts/spike-grounding-dino.py`,
  `docs/decisions/DECISION-GROUNDING-DINO-A2.md`.
- **Références** : D-051, D-052, D-055, D-056, D-058,
  `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-10-QA.md`,
  `assets/models/qa/source_logos.toml`,
  `crates/workflow-steps/src/post/a2.rs`.
- **Suivi B0/B1 (2026-11-09)** : l'approche B locale est lancée. YOLOX-Nano
  416/640 et NanoDet-Plus-m 416 (Apache-2.0) sont exportés en ONNX FP32 et
  FP16, puis mesurés nativement en Rust `ort` CUDA. YOLOX 416 FP32 donne
  8,40 ms p95 au total (5,29 ms de session), +107 Mo de VRAM ; YOLOX 640
  FP16 15,00 ms ; NanoDet 416 FP32 12,16 ms. FP16 est stable pour YOLOX,
  rejeté pour NanoDet. B1 a fine-tuné YOLOX-Nano 416 depthwise sur 1 495
  logos et 15 800 images copy-paste ; le checkpoint époque 5 est figé après
  un arrêt volontaire à l'époque 15 (val synthétique en hausse, D-060 en
  baisse). Point 0,30 / NMS 0,65 : rappel D-060 95,5 % références, 96,7 % de
  présence lower-third A2 (91,7 % sur la boîte logo), 3,3 % de FP négatifs,
  open-set 90,3 %, test fermé mAP50 0,918, open-set mAP50 0,898, latence
  8,01 ms p95, +107 Mo de VRAM. Les seuils de présence, open-set, FP et
  latence passent ; le rappel connu combiné (94,6 %) et la localisation A2
  (91,7 %) restent sous 95 %. B2 (pilote `report_only`) est autorisé sous
  réserve, aucune production modifiée. Preuves :
  `docs/etudes/ETUDE-YOLO-ONNX-A2.md` §11 et `results/B1.md`.
- **Suivi B2 (2026-11-09)** : le détecteur B1 est branché en annotation
  seule (`a2_logo_detector = false`) dans `vision/logo_detector.rs` et
  `post/logo.rs`, avec 12 frames, ROI A2, clusters IoU 0,50 / centre 10 px,
  persistance 0,70 et `details.logo_detector` additif. Le pilote
  `report_only` couvre 13 projets finalisés dont le brut est byte-identique
  à l'assist (STEP7 supprime les bruts ; 2 projets `apply` exclus) :
  **0 différence d'action** et de `review.json`, aucun audit `fail`
  (2 `warn` `source_coverage` préexistants),
  **2/13** logos persistants (11/12), tous deux hors catalogue D-052,
  deux logos animés à 7/12 sous le seuil, latence p50 9,28–10,86 ms / p95
  10,77–12,25 ms, coût 4,45 s/projet. Le recadrage `bottom_left` améliore
  la boîte A2 (91,7 → 98,3 %) mais dégrade les références (95,5 → 84,0 %),
  donc `full` reste le point B2. Aucune activation de production ;
  `a2.rs`, `config/config.toml` et le runtime sont inchangés. Preuves :
  `docs/etudes/ETUDE-YOLO-ONNX-A2/results/B2.md`,
  `results/b2-pilot.json`, `scripts/pilot-b2-logo-detector.py`.

## 2026-11-09 — D-058 acceptée : option A, natif gardé

- **Décision** : l'option **A — natif gardé** est retenue. `cropdetect`
  reste le décideur primaire ; le natif v2 ne raffine un `apply` que si
  `cropdetect` a déjà conclu `apply`. Un natif `review`/`noop`, instable,
  asymétrique ou qui échoue à un veto A1 conserve la décision `cropdetect`.
  Le natif ne crée ni ne retire jamais un `apply`. L'option B est écartée
  (5/56 FN clairs, 304 seg 34/36, 0/7 asymétriques) ; l'option C est
  reportée à **D-059** (test 2 609 segments + réévaluation des vetos).
- **Paramètres** : Sobel Y **20/1020**, couverture **2/3** par défaut et
  **0,45** si les quatre lignes côté barre sont sous **10/255**
  (`dark_luma 10`), clustering ±2 px avec dominant **≥ 60 %**, marge D-056
  **1,015**. `dark_luma 10` conserve les mêmes `apply` que 20 sur
  l'échantillon et supprime 5 revues 309 inutiles.
- **Configuration** : `PostConfig.a1_refine_native` et
  `PostOptions.a1_refine_native`, défaut `false` ;
  `config/config.toml` contient `a1_refine_native = false`. Sans le flag,
  le chemin A1 est strictement identique.
- **Implémentation** : `post/detect.rs` expose `NativeParams`,
  `NativeLetterbox` et `native_letterbox` (décodage gray, EWMA 5σ, Sobel Y
  ciblé, couverture, clustering). `post/a1.rs` ajoute `refine_native` et
  `refine_with_native` : recalcul de `scale`/`Position Y`/`coverage` via
  `decide_letterbox`, couverture évaluée sur les crops `cropdetect`
  d'origine. Détails JSON additives : `native_refined`, `native`,
  `cropdetect`.
- **Validation** : 6 tests A1 du mode gardé + 4 tests natifs ; test TOML du
  flag ; `cargo fmt --check`, `cargo clippy --workspace --all-targets --
  -D warnings`, `cargo test --workspace` verts. Validation réelle sur le
  projet 305 : 16/17 applies passent de 213,7 % à **211,8 %** (un à
  211,6 %), le segment 17 (transition) reste à 213,7 % par fallback ;
  mesures natives 305 seg 1 **81/79**, 304 seg 9 **0/0**, 304 seg 34
  **0/0**.
- **Corpus GOOD complet (2026-11-09)** : 75 projets, **2 607 segments A1**,
  0 erreur ; `apply`/`review`/`noop` inchangés (65/116/2 426),
  **0 changement d'action**, **52/65 applies raffinés** (80,0 %). Gain
  pixel moyen vs `cropdetect=24:2:0` : **1,46 px** (médiane 1,75, p10/p90
  1,00/2,00, min/max −2,00/+2,50 ; 47 améliorés, 1 neutre, 4 dégradés sur
  le 89). MAE par côté : **2,23 px** (`cropdetect=24:16:0`) contre
  **0,77 px** (natif).
  Coût : 1 102,4 s → 1 131,7 s, soit **+29,27 s** (**+2,66 %**) et 563 ms
  par apply raffiné.
- **Reste** : contrôle visuel des 4 dégradations 89 et des 4 applies 89
  bloqués par la couverture, puis activation pilote. Le flag reste `false`
  en production.
- **Références** : `docs/decisions/DECISION-A1-NATIVE-GARDE-D058.md`,
  `docs/lots/LOT-9-POST-STEP8.md`,
  `docs/etudes/ETUDE-VSLETTERBOX-A1/README.md`,
  `docs/etudes/ETUDE-VSLETTERBOX-A1/results/full-corpus-v2.md`,
  `crates/workflow-steps/src/post/{a1,detect}.rs`,
  `crates/workflow-steps/examples/a1_full_eval.rs`,
  `crates/workflow-core/src/config.rs`, `config/config.toml`.

## 2026-11-08 — D-057 : détecteur de letterbox A1 natif opt-in

- **Contexte** : étude du dépôt `Akatmks/vapoursynth-letterbox` (commit
  `ac274fa`, 13 août 2026) pour remplacer ou compléter
  `cropdetect=24:16:0` dans A1. Le dépôt combine un scan ligne à ligne, une
  EWMA de Tony Finch (`alpha = 0,05`, seuil `μ + 5 × max(σ, 0,005)`) et une
  validation par masque de contours ExKirsch 5×5 (`dynamic_ref_thr = 2/3`).
- **Constats** :
  1. L'EWMA rejette le faux ciel nocturne lent du 304 seg 9 (haut 642 px →
     `top = 0` sur les frames à ciel lisse) ; le scan 1 px mesure 81/78 px
     sur le 305 seg 1 contre 86/82 px pour `cropdetect=24:16:0`, soit 9 px
     de hauteur active et ~2,1 points de scale. Avec la marge D-056, le 305
     passe de 213,7 % (A1 actuel) à 211,6 % (natif), pour un choix humain à
     210,0 %.
  2. Le masque ExKirsch seul ne rejette ni un bandeau bas large ni un ciel
     nuageux large : sur le 304 seg 9 à 2 fps, 2 frames sur 5 produisent un
     faux `top = 935` (bandeau) ; les tests synthétiques montrent qu'un aplat
     de ≥ ~70 % de largeur passe le seuil 2/3, alors qu'un texte fragmenté
     est rejeté.
  3. Le dépôt est par frame : pas d'agrégation, pas de symétrie, pas de
     décision. Le magnétisme `permanent` impose une barre fournie par
     l'utilisateur (détection au-dessus, tolérance 1 px vers le bas) ; ce
     n'est pas un anti-jitter temporel.
     `clean_letterbox`, le deringing, `fullblack_thr` et
     `_VSLETTERBOX_AREA_MULTIPLIER` sont des garde-fous de nettoyage, hors
     A1.
  4. Le 304 seg 34 (barre sombre à bord faible) n'est pas détecté par le
     prototype (`top = 0`) alors qu'A1 applique 211,5 % ; A3 doit rester le
     filet pour les keyframes.
- **Décision** :
  1. Ne pas intégrer le plugin C++, le wrapper Python, `vs-jetpack`,
     `vsmasktools` ni `clean_letterbox`.
  2. Expérimenter un détecteur natif Rust **opt-in**
     (`a1_detector = "native"`) : EWMA 5σ, Sobel Y 3×3 ciblé sur les lignes
     candidates, couverture de colonnes et clustering temporel
     (`top`, `bottom`) avec cluster dominant ≥ 60 %, sinon `review`.
  3. Conserver `cropdetect` par défaut et tous les vetos A1 (keyframes,
     asymétrie, `barred_frac`, `scale > 300 %`, `Position Y > 600`,
     couverture, différé A3). Ne jamais passer les mesures natives brutes au
     p10 actuel.
  4. Préserver le contrat `apply`/`review`/`noop`, `scale`, `Position Y`,
     `coverage`, `confidence` ; clés additives seulement, schéma `post.json`
     inchangé.
  5. Aucun code de production modifié par l'étude ; le spike est hors
     production, en lecture seule sur le corpus.
- **Validation** : `docs/etudes/ETUDE-VSLETTERBOX-A1.md`, scripts et
  résultats `docs/etudes/ETUDE-VSLETTERBOX-A1/` ; ADR
  `docs/decisions/DECISION-A1-DETECTEUR-LETTERBOX.md`. Mesures 304 seg 9,
  305 seg 1/9/17, 304 seg 34, 356 seg 2, tests synthétiques
  texte/aplat/ciel/contraste.
- **Spike Rust (2026-11-08)** : exemple hors production
  `crates/workflow-steps/examples/a1_letterbox_spike.rs` ; six fixtures,
  variantes `gray` et `chroma`. 305 seg 1/9 à 81/79 (1 px du prototype),
  305 seg 17 en `review` (cluster 2/4), 304 seg 9 et 356 seg 2 en `noop`,
  304 seg 34 en `noop` (faux négatif connu, A3 filet). Le Sobel Y rejette
  les candidats 935/936 du 304 seg 9 à 0,38–0,46 de couverture, contre
  0,76–0,83 pour l'ExKirsch. Analyse Rust 8–17 ms (`gray`), ratio médian
  1,12× `cropdetect` ; `gray` est le candidat. Sortie :
  `docs/etudes/ETUDE-VSLETTERBOX-A1/results/spike-rust.md`.
- **Limites** : prototype Python approximatif, pas une exécution
  VapourSynth ; échantillon restreint ; images synthétiques non compressées ;
  `cropdetect=24:2:0` est une référence à 2 px, pas une vérité manuelle.
- **Références** : D-056, `docs/lots/LOT-9-POST-STEP8.md`,
  `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md`,
  `crates/workflow-steps/src/post/{a1,detect}.rs`.

## 2026-11-08 — D-056 : réglage A1/A2/A3 issu du corpus GOOD

- **Contexte** : 75 triplets `_9x16` / `_9x16_assist` / `_9x16_assist_GOOD`
  (37 dossiers, 2 609 segments, 5 333 actions, 676 revues) comparés en
  lecture seule, projet par projet, pour mesurer où le post-traitement A1–A6
  converge avec le monteur et où il s'en écarte.
- **Constats** :
  1. A1 (65 `apply`) : la marge humaine médiane est **1,0125** ; **51/65**
     segments sont ≤ 1,015 et **53/65** ≤ 1,02. La marge 1,04 sur-zoome donc
     de **2,7 %** en médiane. **18/65** segments sont ensuite keyframés par
     l'humain ; **56/65** ont un résidu géométrique ≤ 8 px et **68/71**
     exports n'ont aucune barre haute. Sur le 305 (letterbox), le monteur
     reste à 210 % quand A1 propose 218,9 %, sans bord noir dans les deux
     cas (barre réelle 81 px, estimée 86).
  2. A2 (5 `apply`) : les 5 sont conservés par le monteur. Les 9 revues
     `partial_width` (37–45 %) correspondent à des bandeaux que l'humain
     retire : bbox hors comp sur **324/326** segments. Le gate de largeur
     0,50 s'exécute **avant** l'OCR.
  3. A3 (456 revues) : **69 TP** keyframes, soit **15,1 %** de précision et
     **21,8 %** de rappel (36,6 % / 12,1 % en lecture tolérante). Le suivi
     proposé est bon (corrélation médiane 0,99, 59/69 ≥ 0,6) mais le gate
     `gap < 0,3` rate **248 keyframes** ; 1 045 déplacements statiques forts
     ne sont couverts qu'à 9,2 % et la recommandation STEP4 ne colle au
     choix humain que dans 46 % des cas.
  4. A4 : 20 revues, 9 actions, 11 `none` (80 % tolérant). A5 : 17/17
     résolues par le recadrage humain. A6 : 1/1 template conservé.
- **Décision** :
  1. A1 : marge **1,015** (bornée 1,01–1,02), `coverage` et vetos conservés.
     Différer A1 sur candidat A3 (≥ 4 clés ou amplitude ≥ 300 px avec
     gap < 0,3) dans un second temps.
  2. A2 : `BANNER_MIN_WIDTH` **0,35** pour les bandes basses persistantes,
     avec OCR exécuté avant le gate ; conserver les vetos `tall_bottom`,
     `top`, `logo` et `changing_text`.
  3. A3 : reste **review-only** ; `noop` si amplitude < 250 px et ≤ 2 clés
     candidates ; `gap < 0,7` en revue seulement pour une amplitude
     ≥ 200 px ; pas de recadrage statique automatique.
  4. A4 : review-only avec tag `flash probable` < 0,3 s ; A5 : report-only ;
     A6 : inchangé.
  5. Aucun code de production n'est modifié par cette étude : R1/R3/R5/R7/
     R8/R9 forment un lot court, R2/R6 un lot de validation sur copies.
- **Limites** : 4 exports manquants (305, 364, 86 extraits 3 et 4) ; le 305
  est validé par rendu synthétique. Les hashes d'`audit.json` ne sont pas
  exploitables (projets renommés/corrigés) : la validation est structurelle.
  Le tracking `_ae.json` est incomplet (frames manquantes) et trois projets
  présentent un écart review/post d'une unité (304, 322, 35), sans effet sur
  les matrices.
- **Validation** : `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8.md` et preuves
  versionnées `docs/etudes/ETUDE-CORPUS-GOOD-POST-STEP8/` (21 images,
  7 scripts, 3 jeux de données). Scan des 71 exports avec
  `cropdetect=24:2:0` : 67/71 pleine frame, 4 anomalies ponctuelles
  d'intro/transition.
- **Références** : D-030, D-032, D-051, D-052, D-055 ;
  `docs/lots/LOT-9-POST-STEP8.md` ;
  `crates/workflow-steps/src/post/{a1,a2,a3,a4,a6}.rs` et `run.rs` (A5).

### Mise en œuvre de D-056 (2026-11-08)

- **Règles actives** : R1 (`a1_margin = 1,015`, défaut code et
  `config/config.toml`), R3/R4 (bande basse 35–50 % avec OCR exécuté avant le
  gate de largeur, formule de scale inchangée), R5 (`noop` sous 250 px et
  2 clés, `high_confidence` à ≥ 700 px ou 6 clés), R7 (A3 review-only), R8
  (`flash_probable` < 0,3 s sans continuité de source des deux côtés), R9
  (A5 `resolution = "resolved"`, report-only).
- **Règles P2 opt-in** : `a1_defer_on_a3` (R2, défaut `false`) diffère A1 en
  revue sur `strong_candidate` A3, sans appliquer de scale partiel ;
  `a3_relaxed_gap` (R6, défaut `false`) autorise `gap < 0,7` en revue pour
  une amplitude ≥ 200 px.
- **Artefacts** : schéma `post.json`/`review.json`/`audit.json` inchangé
  (`schema = 1`) ; clés additives `partial_width_ocr`, `scale_basis`,
  `high_confidence`, `strong_candidate`, `gap_relaxed`, `flash_probable`,
  `source_continuity`, `a1_deferred_on_a3`, `resolution`, `report_only`.
  `review.json` passe A3 haute confiance en sévérité `high`.
- **Validation code** : `cargo test -p workflow-steps --lib post` (120 tests),
  `cargo test --workspace` (543 tests), `fmt` et `clippy -D warnings` verts,
  `config check` valide ; aucun accès `/mnt/cache`, `runtime/` ou dépôt
  Python.
- **Validation R2/R6 sur copies (2026-11-08)** : copies de 68, 54 et 356
  sous `/tmp/opencode/r2r6`, configs identiques (`a1_margin = 1,015`,
  `a2_ocr = false`, `direct = false`) et flags opposés. 68 : le seul `apply`
  A1 partagé avec un candidat A3 fort (Segment 7, 317 px, gap 0,261) passe
  en `review` avec `a1_deferred_on_a3` et l'assist conserve le transform
  brut ; 54 : 9 A3 `noop` → `review` avec `gap_relaxed`, aucun `apply` A3,
  assist identique ; 356 : 3 A3 `noop` → `review`, l'`apply` A1 du Segment 8
  reste inchangé, assist identique. Audits `pass`, 3 hautes confiances en
  sévérité `high` sur 54. La référence GOOD de 68 porte 18 keyframes Position
  sur le Segment 7, ce qui confirme R2 ; sur les 12 nouvelles revues R6,
  5 segments sont keyframés GOOD et 1 a un repositionnement statique
  (6/12 utiles), 6 sans changement humain. R6 reste review-only. Les deux
  flags restent opt-in.
- **Références** : `docs/lots/LOT-9-POST-STEP8.md`,
  `docs/architecture/CONTRAT-ENVIRONNEMENT.md`,
  `crates/workflow-steps/src/post/{a1,a2,a3,a4,model,run}.rs`,
  `crates/workflow-core/src/config.rs`, `config/config.toml`,
  `config/env.map.toml`.

## 2026-11-08 — D-055 : activation de `a2_ocr` en production, `a2_logo_prior` différé

- **Contexte** : le staging D-054 a été validé le 2026-11-08. `a2_ocr`
  apporte une preuve OCR réelle (`54` confiance 0,90 → 0,95, `58` `review`
  → `apply` 210,0 %) mais `a2_logo_prior` modifie aussi le rapport A2 sur
  les bandeaux sombres et dépend du catalogue de logos.
- **Décision** :
  1. `steps.step8.post.a2_ocr = true` dans `config/config.toml`, avec
     `a2_ocr_threads = 8`, les modèles sous `assets/models/qa/` et le
     runtime `vendor/libonnxruntime.so.1.23.2`.
  2. `a2_logo_prior` reste `false` : l'activer seul permet d'isoler l'effet
     OCR et de valider les nouveaux motifs en production. Il sera évalué
     après une période de runs réels, ou sur un lot de copies.
  3. Vérification avec la configuration de production sur une copie de
     `54` : `apply` 200,8 %, OCR 13,0 s, post 43,9 s, audit `pass`, aucune
     erreur de chargement ONNX.
  4. Le prior ne change pas l'action dans les cas testés, mais il n'est pas
     purement cosmétique : sur `54`/`58` il déplace `UNJOUR`/`UNDOC` de
     `texts` vers `logo_texts` ; sur `40 ABHARTACH`/`40 FOOTBALLEUR` il
     remplace « bande basse animée, géométrie non fiable » (confiance 0,4,
     sans scale) par « bande basse sombre, texte persistant confirmé »
     (confiance 0,55, `scale_target` 218,2, dominance 0,9167). Les
     opérateurs doivent accepter ces motifs avant l'activation.
- **Rollback** : repasser `a2_ocr = false` ; aucune migration de données,
  `post.json` reste schéma 1 et les clés OCR disparaissent.
- **Validation** : `config check`, `post run` réel sur copie de `54`,
  505 tests, `fmt` et `clippy -D warnings` verts. Aucune écriture dans
  `/mnt/cache`.
- **Références** : `config/config.toml`,
  `docs/lots/LOT-9-POST-STEP8.md`,
  `crates/workflow-steps/src/post/a2.rs`,
  `crates/workflow-steps/src/post/run.rs`,
  `crates/workflow-core/src/config.rs`.

## 2026-11-08 — D-054 : installation des modèles OCR et validation staging A2

- **Contexte** : suite de D-053. Les flags restaient désactivés et les
  PP-OCRv6 étaient absents de `assets/models/qa/` ; la séquence recommandée
  demandait d'installer les modèles, de rejouer un lot représentatif en
  staging, puis de soumettre les écarts à validation avant toute activation
  en production.
- **Décision** :
  1. `PP-OCRv6_det_small.onnx` et `PP-OCRv6_rec_small.onnx` sont copiés sous
     `assets/models/qa/` ; leurs SHA-256 correspondent au manifeste. Les
     binaires restent gitignorés. `vendor/libonnxruntime.so.1.23.2` est copié
     depuis le paquet Python legacy et `steps.step8.post.a2_ocr_onnxruntime`
     pointe dessus.
  2. Staging sur copies de `40`, `54` et `58`, sans écriture dans
     `/mnt/cache` : baseline `a2_ocr = false` contre staging
     `a2_ocr = true`, `a2_logo_prior = false`, `a2_ocr_threads = 8`.
     `54` reste `apply` 200,8 % (confiance 0,90 → 0,95) ; `58` passe de
     `review` à `apply` 210,0 % ; `40 ABHARTACH` et `40 FOOTBALLEUR`
     restent `review` avec les preuves OCR mais sans `scale_target`
     (dominance `66`/`MINUTES`, `coverage_x` 0,09–0,11 < 0,25). Tous les audits
     sont `pass`. Coût du lot : 87,3 s en baseline, 130,7 s en staging.
  3. Le même lot avec `a2_logo_prior = true` confirme la phase B sur `40` :
     `review` « bande basse sombre, texte persistant confirmé »,
     `scale_target` 218,2, dominance 0,9167, `logo_texts` `66`/`MINUTES`.
  4. `a2_ocr` et `a2_logo_prior` restent `false` dans
     `config/config.toml` jusqu'à validation client. Après validation,
     activer `a2_ocr` seul d'abord, puis `a2_logo_prior` après validation
     des phases B/C.
- **Limites** : le staging couvre trois dossiers et quatre projets, pas un
  lot complet. Les écarts `58` (`review` → `apply`) et la confiance `54`
  (0,90 → 0,95) doivent être validés par le client. Le coût OCR reste
  séquentiel par projet.
- **Références** : `config/config.toml`, `docs/lots/LOT-9-POST-STEP8.md`,
  `crates/workflow-steps/src/post/a2.rs`,
  `crates/workflow-steps/src/post/run.rs`,
  `crates/workflow-core/src/config.rs`, `assets/models/qa/manifest.toml`.

## 2026-11-08 — D-053 : threads OCR A2 configurables et compatibilité EffectCraft 0.22.0

- **Contexte** : le gate A2 était figé à deux threads intra-op alors que la
  station dispose de 24 threads logiques (Ryzen Threadripper 1920X,
  12 cœurs). A1 reste séquentiel et devient le principal coût du post sur
  `54`. EffectCraft 0.22.0 est installé et doit rester compatible avec le
  driver STEP8.
- **Décision** :
  1. `steps.step8.post.a2_ocr_threads` borne chaque session PP-OCRv6
     det/rec (défaut 2, `config/config.toml` = 8). Les deux sessions sont
     séquentielles, contrairement au mode lot QA qui ouvre deux sessions
     par worker.
  2. Mesure sur `54` (12 échantillons) : 2 threads → OCR 23,6 s / post
     55,0 s ; 8 threads → 13,2 s / 44,1 s ; 12 threads → 12,5 s / 43,6 s.
     8 est retenu : 12 n'apporte que ~5 % et réduit la marge pour
     ffmpeg/A1 et le serveur.
  3. Aucune parallélisation de A1 n'est activée : `cropdetect` reste
     séquentiel par segment (~23,5 s sur `54`). Un pool borné est une piste
     mesurable, pas une décision.
  4. `a2_ocr` et `a2_logo_prior` restent à `false` : activation après
     validation humaine/client. `a2_logo_prior` seul annote sans changer la
     décision.
- **Validation EffectCraft 0.22.0** : `effectcraft-cli --version` renvoie
  `0.22.0` ; le contrat `prefs.set` + `file.runScript` et la forme JSON
  `[{"command":...,"result":...}]` sont inchangés. STEP8 réel sur une copie
  temporaire de `54` : 1/1 projet, `MS_BATCH_RESULT ok`, projet enregistré ;
  post STEP8 : `_assist.ecproj` produit, `audit = pass` ;
  `effectcraft-cli --project ..._assist.ecproj --json info` charge la
  composition 1080×1920 (57 calques). Aucun changement STEP8 nécessaire.
- **Limites** : les PP-OCRv6 ne sont pas présents sous
  `assets/models/qa/` sur cette machine ; la validation a utilisé
  `/media/kidpixel/WORK/qa-poc/models`. `a2_ocr = true` sans modèles
  journalise un avertissement et retombe sur la décision géométrique. Le
  post reste séquentiel par projet ; seul le QA batch expose
  `--workers`/`--intra-threads`.
- **Références** : `crates/workflow-steps/src/post/a2.rs`,
  `crates/workflow-steps/src/post/run.rs`,
  `crates/workflow-core/src/config.rs`, `config/config.toml`,
  `docs/lots/LOT-9-POST-STEP8.md`.

## 2026-11-08 — D-052 : registre spatial des logos source, annotation, exclusion OCR et confirmation visuelle (phases A/B/C)

- **Contexte** : le client a documenté 29 émissions avec un logo de programme
  quasi déterministe dans le canvas source 1920×1080 (28 en bas-gauche,
  Piquantes ! en haut-gauche) et fourni les crops RGBA, une liste de clean
  feeds et cinq émissions sans logo. Une détection plein cadre serait plus
  coûteuse et moins fiable que ce prior spatial ; le logo doit rester
  distinct de la bannière éditoriale et du sous-titre.
- **Décision** :
  1. les phases A/B/C restent **report-only** : le registre ne recadre, ne
     masque et ne supprime jamais rien ; il annote A2 et le QA, et un logo
     seul ne peut que déclencher une revue ;
  2. `assets/models/qa/source_logos.toml` est versionné (schéma 1, zones
     génériques, 29 émissions, 15 clean feeds, 5 émissions sans logo,
     variantes et provenance). Les crops restent hors dépôt ; le catalogue ne
     contient que des bboxes et des métadonnées ;
  3. les noms de fichiers explicitement rattachés par le CSV priment sur les
     alias, pour ne jamais confondre « Kamila ne sait pas écrire
     Marseillais » (Le Cross) avec Les Marseillais ; un clean feed déclaré
     n'hérite jamais d'un prior ;
  4. la projection utilise la formule A1/A5
     (`position + parent_dx + (source - anchor) * scale / 100`), interpole
     Position/Anchor/Scale et refuse un parent animé ; la visibilité est la
     part de la bbox **projetée** restant dans la composition ;
  5. A2 ajoute `details.source_logo` derrière `a2_logo_prior = false` (flag
     désactivé par défaut) ; le QA ajoute le check `source_logo` derrière
     `--source-logo`, `pass`/`info` uniquement, et refuse l'auto-approbation
     si le catalogue demandé n'a pas pu tourner ;
  6. la zone générique est exposée dans le catalogue ; la phase C mesure sa
     stabilité temporelle pour les sources inconnues et produit un `info`
     « logo possible non identifié », sans identité ni revue ;
  7. phase B, même flag : les détections OCR majoritairement (≥ 60 %) dans
     la bbox du logo sont retirées du groupe dominant et exposées sous
     `details.ocr.logo_texts` (`logo_detections_total`, `logo_only`). Ces
     clés n'existent que si un prior est actif. Un logo seul ne peut jamais
     confirmer une bannière : A2 répond `review` au lieu d'appliquer. Les
     garde-fous de géométrie, de largeur et de bande sombre gardent la
     priorité ;
  8. phase C : `--source-logo-visual` (impliqué par
     `--source-logo-references`) confirme localement le logo par NCC couleur
     sur masque alpha + NCC des contours, sur six instants et toutes les
     variantes. Seuils : couleur ≥ 0,45, contours ≥ 0,10, score ≥ 0,50, deux
     échantillons forts. Les crops restent externes et le chemin accepte
     `emissions` ou son parent. Un format d'export différent de la
     composition abandonne la comparaison. Pour une source inconnue, la
     stabilité des zones génériques (écart-type ≤ 12/255, ≥ 5 % stables,
     contours persistants ≥ 0,8 %, densité ≥ 4 %) ne produit qu'un `info`.
     Aucun `review`, aucun recadrage ;
  9. A2 lit le masque des overlays image déclarés dans le `.ecproj` via le
     module partagé `post::overlay`. Si, aux douze instants du gate OCR,
     toutes les occurrences visibles du logo projeté sont couvertes à
     ≥ 90 %, `visible_in_comp` passe à
     `false`, `overlay_covered` et `hidden_by_overlay` documentent la
     couverture, et le veto « logo seul » est remplacé par la décision
     géométrique A2. L'exclusion OCR du texte du logo reste inchangée :
     elle porte sur la source, où l'overlay n'existe pas encore.
- **Alternatives rejetées** :
  - détection plein cadre ou YOLO : fermée au monde ouvert et sans le prior
    client ;
  - identification par le seul nom de fichier : le corpus contient des
    ambiguïtés documentées (Marseillais/Le Cross, 266/66 Minutes) ;
  - recadrage ou masquage automatique : changerait une politique validée
    sans mesure sur le corpus GOOD ;
  - copie des crops dans le dépôt : assets lourds et redondants.
- **Validation phase A** : catalogue généré (29/15/5), parseur et projection
  testés (keyframes, parent animé, mise à l'échelle). Rejeu A2 avec OCR :
  `54` inchangé `apply` 200,8 %, `58` inchangé `apply` 210,0 %,
  `40 ABHARTACH` et `40 FOOTBALLEUR` inchangés `review` avec le logo
  66 Minutes documenté (72,6 % du candidat) ; seules les clés `source_logo`
  et les temps OCR diffèrent. QA sur 35 exports GOOD avec `.ecproj` :
  25 `pass`, 10 `info`, 0 nouveau finding ; `227` E=M6 hors cadre, `270`
  GOAL clean feed.
- **Validation phase B** : `54` et `58` conservent `apply` 200,8 % / 210,0 %
  et leur scale, mais `UNJOUR`/`UNDOC` passent de `texts` à `logo_texts` ;
  `40 ABHARTACH` et `40 FOOTBALLEUR` restent `review` et passent de « bande
  animée » à « bande sombre, texte persistant confirmé », avec la bannière
  réelle dominante (91,7 %) et `scale_target` 218,2 % ; `sub_fast` reste
  vétoé à 25 %. Les audits restent `pass`.
- **Validation phase C** : frame de référence 66 Minutes → NCC 0,97 (3/3) ;
  export 9:16 synthétique UJUD avec la transformation réelle du projet →
  NCC 0,96 (3/3) ; quatre rendus H.264 1080×1920 construits depuis les
  rushes de production et les transformations réelles (`54` à 148,68 s,
  `58` à 49,16 s, `40 ABHARTACH` à 8,36 s, `40 FOOTBALLEUR` à 45,64 s) →
  NCC 0,95 / 0,94 / 0,64 / 0,58, 3/3 à chaque fois ; le même rendu UJUD
  évalué avec le prior 66 Minutes reste non confirmé (0,07) ; image
  statique inconnue → zone générique stable à 100 % (contours 19,6 %). Sur
  les 35 exports GOOD appariés : 25 `pass`, 10 `info`, 0 nouveau finding,
  aucune zone générique stable sur les sources inconnues ; un balayage
  exhaustif de six des dix exports `info` (UJUD ×4, 66M ×2) à 1 i/s ne
  trouve aucune frame au-dessus de 0,50 (meilleur 0,38 sur une bande de
  24 px) : pour ces six, l'échantillonnage n'est pas la cause ; les quatre
  autres (270 Gulli, 3 W9 LAA9) n'ont pas été balayés. Ces `info` ne
  signifient pas une absence du logo dans le rush : la vérification manuelle
  des rushes UJUD et 66M le montre présent sur toute la durée. Le check
  compare l'export final au crop du logo source à la position projetée ; le
  logo social est ajouté manuellement en overlay, donc cette position peut
  ne pas contenir le logo source. Un `info` « logo non confirmé visuellement
  » veut dire « logo source non retrouvé dans l'export analysé », pas « logo
  absent du rush ». Le check ne valide pas l'overlay manuel.
  505 tests workspace, `fmt` et `clippy -D warnings` verts ; aucune écriture
  dans `/mnt/cache` (lecture seule).
- **Validation overlays A2** : sur `54` avec un habillage déclaré
  synthétique couvrant le logo, `apply` 200,8 % inchangé,
  `visible_in_comp=false`, `overlay_covered=1.0`, 2/2 occurrences visibles
  masquées et `logo_hidden_by_overlay=true` ; sans habillage, la sortie
  reste identique à la phase B. Le veto logo seul neutralisé est couvert par
  un test unitaire (`logo_only_hidden_by_overlay_does_not_veto`).
- **Limites** : la confirmation visuelle est une corrélation globale ; un
  logo animé, une variante non cataloguée ou un reflet peut rester non
  confirmé, et un fond statique peut déclencher la stabilité d'une zone
  générique. Les seuils sont calibrés sur les références, les cas
  synthétiques, quatre rendus H.264 construits depuis les rushes et les
  transformations réelles, et les 35 GOOD ; ils ne sont pas encore
  confrontés à des exports AE produits par l'opérateur. A2 lit les overlays
  déclarés mais ne consulte pas la confirmation visuelle NCC ; DINOv2 sur
  les scores résiduels et une confirmation A2 restent optionnels (phase D).
- **Références** : `docs/etudes/ETUDE-LOGOS-BANDEAUX-POST-QA.md` (§7.5),
  `crates/workflow-steps/src/vision/source_logo.rs`,
  `crates/workflow-steps/src/qa/source_logo.rs`,
  `crates/workflow-steps/src/qa/source_logo_visual.rs`,
  `crates/workflow-steps/src/post/a2.rs`,
  `crates/workflow-steps/src/post/overlay.rs`,
  `scripts/import-source-logos.py`.
- **Révision 2026-11-08 — catalogue étendu à 30/21/9** : le CSV client
  ajoute la 30ᵉ émission *Les Reines de la Route* (6ter, `bottom_left`, bbox
  `[99, 848, 315, 1022]`, crop `Les_Reines_de_la_Route/crop.png`, source
  exacte et alias `6ter - LRDLR -` / `LRDLR`), 6 clean feeds (Le Seigneur
  des Anneaux, Younger, WANTED ×2, Pretty Little Liars ×2) et 4 émissions
  sans logo. `scripts/import-source-logos.py` normalise le suffixe CSV
  `(LRDLR)` vers le titre éditorial, la note de safe area, le titre complet
  du clean feed Seigneur et les dossiers absents du CSV ; la régénération
  rafraîchit aussi les notes Top Chef, Enquête Exclusive et E=M6. Aucun
  changement de code A2/QA : le registre reste report-only, `clean_feeds`
  porte l'exclusion runtime et les crops restent externes. Validation :
  545 tests, `fmt` et `clippy -D warnings` verts ; sortie du générateur
  identique au catalogue versionné ; bbox LRDLR contenue dans la zone
  `bottom_left` existante.

## 2026-11-07 — D-051 : preuve OCR des bandeaux fixes, QA par clustering et non-import des dépôts logos/sous-titres

- **Contexte** : étude comparative demandée sur la détection de logos et de
  bandeaux/sous-titres fixes dans `post/a2.rs` et `qa/`, confrontée à sept
  dépôts GitHub (YOLOv7, YOLOv8 API, open-set YOLOv5, Logodetect, VideOCR,
  deux détecteurs morphologiques OpenCV) et au corpus réel `40`, `54`, `58
  Camille`.
- **Constats mesurés** :
  1. la pile Rust existante (PP-OCRv6 + DINOv2 + `OverlayMask` + persistance)
     surpasse les sept dépôts pour le broadcast généraliste ; aucun ne
     combine open-set, temporel, masque d'overlays et exécution Rust/ONNX ;
  2. l'agrégation OCR de `qa/ocr.rs` jette les bandeaux persistants dès qu'un
     sous-titre coexiste : la moyenne verticale du bloc dérive de 0,0642
     (`54`), 0,0637 (`58`) et 0,0562 (`40 FOOTBALLEUR`), au-delà de
     `MAX_Y_DRIFT = 0.04`, alors que `40 ABHARTACH` (dérive 0,0019) est
     signalé ; trois bandeaux persistants sur quatre sont donc manqués et le
     quatrième est classé `subtitle_burned_in` ;
  3. A2 ne peut pas exploiter l'OCR : les bandeaux `40` sont sombres
     (médiane 37 et 55) et les candidats `40`/`58` ont une géométrie de
     scène instable, alors que l'OCR lit le même texte fixe sur 96–100 %
     des échantillons (score 0,979–0,987, dérive Y 2–9 px) ;
  4. l'application A2 de `54` est propre : 112 différences JSON, toutes des
     valeurs `Scale 180.0 → 200.8`, aucune ancre ni position modifiée ;
  5. DINOv2 détecte `VIDÉO TIKTOK` sur `40 FOOTBALLEUR` (4/12, score 0,671)
     là où l'OCR texte échoue.
- **Décision** :
  1. ne pas importer les dépôts étudiés : YOLO est fermé au monde ouvert,
     Logodetect exige des exemplaires, les deux sont lourds ou copyleft
     (GPL/AGPL), et les détecteurs morphologiques OpenCV n'ont ni OCR ni
     persistance ; `OverlayMask` couvre déjà les assets déclarés ;
  2. faire évoluer le QA en **1.3.0** avec un clustering par **boîte
     stable** (le texte normalisé ne sert qu'à mesurer la dominance), un
     type `fixed_overlay` non bloquant et un cache 7 ; un bandeau fixe
     (persistance ≥ 60 %, dominance ≥ 80 %) reste `review` mais n'est plus
     confondu avec un sous-titre et n'est plus masqué par la dérive d'un
     sous-titre concurrent ; un texte changeant dans la même boîte reste
     `subtitle_burned_in` ;
  3. ajouter à A2 un **gate OCR** derrière `a2_ocr = false` : il ne peut
     jamais contourner les garde-fous durs (largeur, hauteur, scale,
     keyframes, bande haute, conflit A1/A2) ; il peut confirmer un bandeau
     clair stable et oppose un veto si le texte change trop vite ; les
     bandeaux sombres restent en `review` avec preuve OCR et scale proposé
     (niveau 1), le niveau 2 automatique étant réservé à une validation
     ultérieure ;
  4. extraire `frame`, `inference`, `ocr`, `dino` et le texte dans un module
     neutre `vision` avant de brancher `post` sur l'OCR, pour ne pas faire
     dépendre le pré-export du QA post-export ;
  5. conserver la formule de scale A2 actuelle : le bord visible après scale
     reste au-dessus du texte OCR sur les quatre cas, au prix d'un sur-crop
     de 24–65 px de scène ; une formule basée sur `y0` est laissée à une
     étude séparée.
- **Alternatives rejetées** :
  - import direct `post → qa` : couple les couches et masque le cycle
    conceptuel ; acceptable seulement pour un spike ;
  - duplication d'un OCR léger dans `post` : deux préprocessings à maintenir ;
  - seuil Sobel/bright global plus haut : mesuré insuffisant, des bords de
    scène restent détectés au-dessus du texte ;
  - application automatique des bandeaux sombres : change une politique
    validée sans corpus dédié.
- **Validation** : quatre projets du corpus, QA en lecture seule sous
  `/tmp/workflow_investigation/`, frames et médianes sous `/tmp`, aucun
  fichier de `/mnt/cache` modifié. **Phase 1 (QA 1.3.0) implémentée** dans
  `qa/ocr.rs`, `qa/model.rs` et `qa/cache.rs` : rejeu réel des quatre rushes,
  un `fixed_overlay` par projet (persistance 100 %, dominance 96–100 %),
  zéro régression, plus un `subtitle_burned_in` réel sur `54` auparavant
  manqué ; `sub_fast` donne un sous-titre à dominance 29 % et
  `181_sub_center` un `fixed_overlay`. Tests workspace, `fmt` et
  `clippy -D warnings` verts ; aucune écriture dans `/mnt/cache`. Lot OCR
  sur 36 exports GOOD : 36 `pass`, 0 finding, aucun faux positif
  `fixed_overlay` ni `subtitle_burned_in`. **Phase 3** : `54` apply 200,8 %
  inchangé (112 diffs `Scale`), `58` promu à 210,0 % (40 diffs `Scale`),
  `40` reviews enrichis, `sub_fast` vétoé, audits `pass` ; 467 tests
  workspace.
- **Suites** : phase 2 (module neutre `vision`) **implémentée le
  2026-11-07** : `frame`, `inference`, `ocr`, `dino`, `text` et `hash` sous
  `crates/workflow-steps/src/vision/`, rejeu QA de `54` identique hors
  timings. Phase 3 (gate OCR A2 derrière `a2_ocr = false`) **implémentée le
  2026-11-07** : `54` reste `apply` 200,8 %, `58` passe à `apply` 210,0 %,
  les deux `40` restent `review` avec `details.ocr`, `sub_fast` est vétoé
  (dominance 25 %) ; les diffs de projet se limitent à `Scale`, audits
  `pass`. Restent le niveau 2 des bandeaux sombres, le volet logo et
  l'activation en production après validation humaine.
- **Références** : `docs/etudes/ETUDE-LOGOS-BANDEAUX-POST-QA.md`,
  `docs/lots/LOT-9-POST-STEP8.md`, `docs/lots/LOT-10-QA.md`,
  `crates/workflow-steps/src/post/a2.rs`,
  `crates/workflow-steps/src/qa/ocr.rs`,
  `crates/workflow-steps/src/qa/logo.rs`,
  `crates/workflow-steps/src/qa/dino.rs`.

## 2026-11-02 — D-050 : progression du post-traitement A5/A6 dans STEP8

- **Contexte** : la barre STEP8 suivait EffectCraft puis restait à 100 %
  pendant le post-traitement A5/A6. L'avancement n'était visible qu'en ouvrant
  le panneau de logs, alors que le post peut durer plusieurs minutes.
- **Décision** :
  1. `run_post_jobs` émet `POST_TOTAL: N`, `POST_CURRENT: i/N - <projet>`
     avant le projet, `POST_PROGRESS: i/N - <projet>` après sa fin et
     `POST_DONE: N - <échecs>`. Le hook STEP8 les écrit sur `stdout` avec
     `MS_BATCH_*` pour préserver l'ordre ; `workflow-cli post run` les met
     sur `stderr` et garde son JSON sur `stdout`.
  2. `ProgressParser` ajoute les projets A5/A6 au total EffectCraft déjà reçu
     (`MS_BATCH_TOTAL`) et publie `progress_phase = "post"` dans
     `/status/<key>`. La barre ne repart donc pas de zéro.
  3. `assets/web` affiche la phase post en orange, le badge
     « Post-traitement » et le texte `Post-traitement A5/A6 : i/N — <projet>`
     tel quel ; le compteur global n'est pas réapposé au texte.
  4. La granularité reste le projet, pas les étages A1/A2/A3/A4/A6.
- **Validation** : tests parser avec base EffectCraft (total 1+3,
  `POST_CURRENT`, `POST_PROGRESS`, clôture `POST_DONE`) et sans base ;
  marqueurs vérifiés sur deux projets avec `workflow-cli post run` (JSON
  `stdout` intact) ; 442 tests workspace, `fmt`, `clippy -D warnings` et
  `node --check` verts ; harnais navigateur sur le vrai `uiUpdater.js` :
  barre orange, statut « Post-traitement », texte brut, complétion verte,
  échec rouge et retour idle sans phase.
- **Limite** : un projet long garde la même position dans la barre ; seuls
  les changements de projet la font avancer.
- **Références** : `crates/workflow-steps/src/post/run.rs`,
  `crates/workflow-server/src/progress.rs`,
  `crates/workflow-server/src/state.rs`,
  `assets/web/static/uiUpdater.js`, `docs/lots/LOT-9-POST-STEP8.md`,
  `docs/architecture/CONTRAT-ENVIRONNEMENT.md`.

## 2026-11-02 — D-049 : passage en production du QA, auto-approbation conditionnelle et DINOv2 par défaut

- **Contexte** : l'ablation réelle valide 14/14 overlays non déclarés, avec
  0/14 finding quand le projet d'origine est utilisé ; les 2 exports 270
  restent `review` et sont confirmés intentionnels (split screen). Le client
  juge le lot assez complet pour la production.
- **Décision** :
  1. `auto_approve_eligible` n'est plus une constante : il est calculé par
     `aggregate::auto_approve`. Il est vrai seulement si le statut global est
     `pass` et si `project_readable`, `subtitle_ocr`, `logo_residual` et
     `face_cut` sont `pass`/`info`. Un `review`, un `fail` ou un contrôle
     requis `not_applicable` le laisse faux, avec un motif explicite.
  2. Si un check `logo_visual` est présent (DINOv2 actif mais modèle/probe
     absent), il doit être `pass`/`info` ; un `not_applicable` bloque
     l'auto-approbation. `--no-logo-visual` retire le check et assume un
     contrôle texte seul.
  3. DINOv2 passe en opt-out : `logo_visual_enabled = true` par défaut,
     `--no-logo-visual` pour le désactiver. L'ancien `--logo-visual` reste
     accepté en alias caché, sans effet.
  4. Schéma `qa.json` **1.2.0**, `CACHE_VERSION = 6` : les anciens rapports
     avec `auto_approve_eligible = false` et les anciennes empreintes sont
     invalidés. `summary.json` passe en schéma **2** avec le drapeau par
     export et le compte `auto_approve_eligible`.
- **Validation** : 6 tests d'agrégation ajoutés (pass complet, review/fail,
  contrôle requis absent ou `not_applicable`, `logo_visual` non exécuté,
  opt-out explicite) ; **439 tests workspace**, `fmt` et
  `clippy -D warnings` verts. Spot-check release : 300 JOSEPH `pass` +
  `auto_approve_eligible = true` avec DINOv2, `pass` + vrai avec
  `--no-logo-visual`, `pass` + faux si le modèle DINOv2 manque
  (`logo_visual=not_applicable`) ; 270 GOAL `review` + faux.
- **Limite** : les logos inconnus, les masques partiels et les erreurs de
  montage effectives ne sont pas labellisés. L'auto-approbation ne vaut que
  pour les contrôles exécutés ; `--no-logo-visual` reste une soupape
  explicite.
- **Références** : `crates/workflow-steps/src/qa/aggregate.rs`,
  `crates/workflow-steps/src/qa/mod.rs`,
  `crates/workflow-cli/src/main.rs`, `docs/lots/LOT-10-QA.md`, D-048.

## 2026-11-02 — D-048 : vérificateur visuel DINOv2 opt-in

- **Contexte** : le contrôle texte plafonne à 34/39 sur la galerie d'assets
  (87,2 %) et rate M6+ aux trois échelles, Les 100 à 1,0 et Gulli à 0,5. Les
  candidats OCR existent pourtant : M6+ est lu « 1+ », « 161+ », « K+ », et
  Les 100 produit une boîte de 204 px de haut, au-delà du filtre texte 180.
- **Décision** :
  1. Ajouter un second étage DINOv2 ViT-S/14 quantifié (24,4 Mo,
     Apache-2.0), ONNX Runtime CPU, derrière `--logo-visual`, sans remplacer
     l'OCR. D-049 le passe en actif par défaut avec `--no-logo-visual` en
     opt-out.
  2. Entraîner un probe logistique sur les embeddings CLS des assets **dev**
     (7/6 holdout) : 438 positifs, 284 négatifs, AUC holdout 0,824.
  3. Préprocessing Rust aligné sur le PoC : pad carré, bicubique 256,
     crop 224, normalisation ImageNet ; le probe versionné contient les 384
     poids, le biais et le seuil.
  4. Point de fonctionnement : seuil 0,50, persistance 3/12, géométrie
     18–220 px, aspect ≥ 1,2. Le plafond 220 inclut Les 100 à 1,0 sans
     laisser passer les boîtes pleine frame ; le texte garde 18–180.
  5. Un finding visuel qui recoupe un finding texte est ignoré ; les
     findings visuels utilisent `detected_by_stage = stage1_dinov2_top`.
  6. `stage1_logo_visual_evidence`, `models_provenance.embedding` et
     `timings_ms.stage1_logo_visual_ms` documentent la passe ; schéma
     `qa.json` 1.1.0, `CACHE_VERSION = 5`.
- **Validation** : galerie 56 clips, union **39/39** (34 texte + 5 visuels),
  0/13 masqués, 0/4 négatifs ; corpus 32 exports avec projet, **0 nouveau
  `pass` signalé**, 2 exports 270 toujours `review` ; temps 1,25–1,31 s par
  export isolé, jusqu'à 5,9 s sous 4 workers. 433 tests workspace à cette
  date, `fmt` et `clippy -D warnings` verts.
- **Validation réelle (2026-11-02)** : 14 exports GOOD avec 16 overlays
  image, projet copié sans les calques image. Projet d'origine : **0/14**
  finding ; déclaration retirée : **14/14** (26 findings, 19 texte,
  7 DINOv2 ; IC 95 % 78,5–100 %). Le cas 298 « The 100 » est sauvé par
  DINOv2 (2/2). Les overlays et les deux exports 270 (split screen) sont
  confirmés intentionnels par le client. Cette ablation valide les overlays
  non déclarés, pas les logos inconnus ni les masques partiels. Détail :
  `/media/kidpixel/WORK/qa-poc/reports/real-ablation/`.
- **Limite** : probe entraîné sur 13 assets synthétiques ; l'ablation réelle
  couvre 14/14 overlays non déclarés, mais aucun défaut de montage réel ni
  logo inconnu n'est labellisé. À cette date, flag opt-in et
  `auto_approve_eligible = false` conservés ; D-049 les remplace.
- **Références** : `crates/workflow-steps/src/qa/dino.rs`,
  `crates/workflow-steps/src/qa/logo.rs`, `assets/models/qa/`,
  `POC-RESULTS.md` §2.10–2.11, `/media/kidpixel/WORK/qa-poc/reports/real-ablation/`,
  D-047.

## 2026-11-02 — D-047 : containment pour le masque et l'agrégation des logos

- **Contexte** : la galerie 13×3 a révélé deux erreurs liées aux boîtes OCR
  plus grandes que le logo :
  1. M6+ masqué : la détection 218×150 contient le logo 168×87 mais ne le
     recouvre qu'à 44,7 %, sous le seuil 0,5 ; 11/12 détections échappaient
     au masque.
  2. e=m6 à 0,5 : « e=m6 » (5 échantillons) et « =m6 »/« m6 » (5) formaient
     deux groupes, car la petite boîte n'avait pas l'IoU suffisant avec la
     grande.
- **Décision** :
  1. `OverlayMask::overlap_ratio` divise l'intersection par la plus petite
     des deux aires (détection ou overlay) : une boîte qui contient
     l'overlay est expliquée. Le seuil 0,5 est conservé.
  2. `bbox_similarity` accepte la containment (marge 2 px) en plus de l'IoU
     et du centre proche.
  3. Tests : détection qui contient l'overlay → 1,0 ; détection à moitié
     dans l'overlay → 0,5 ; boîtes imbriquées de même texte → un finding ;
     textes différents dans des boîtes imbriquées → deux findings.
  4. `CACHE_VERSION` passe à 3 : l'agrégation, le masque et le préprocessing
     changent les résultats à entrées identiques.
- **Validation** : galerie 13×3 34/39, 0/17 FP, 13/13 masqués ; M6+ masqué
  12/12 ; e=m6 à 0,5 récupéré (10/12). Lot 35 exports : 30 `pass`,
  3 `not_applicable` (179), 2 `review` (270), 4 findings, 264/386 détections
  masquées. Galerie texte : 60/60, 0/16 FP, 8/8 masqués.
- **Références** : `crates/workflow-steps/src/post/overlay.rs`,
  `crates/workflow-steps/src/qa/logo.rs`,
  `crates/workflow-steps/src/qa/cache.rs`.

## 2026-11-02 — D-046 : galerie d'assets 13×3 et métriques par échelle

- **Contexte** : la galerie 13×2 changeait le fond et la position entre les
  deux transformations, ce qui confondait l'effet de l'échelle avec celui du
  contexte. Le corpus ne contient que 13 PNG alpha valides (14 noms ;
  `211 LFAUIT.png` est un fichier texte de 15 octets).
- **Décision** :
  1. Trois échelles par asset (1,0 production, 0,75, 0,5), avec le même
     fond, le même offset et la même position du coin haut-gauche ; seule
     l'échelle change.
  2. 56 clips : 39 positifs, 13 masqués (un par asset), 4 négatifs.
  3. Split dev/holdout par asset (7/6), jamais par crop ; tags `square` et
     `short` pour l'analyse.
  4. `eval_logo_gallery.py` expose `by_split`, `false_positives_by_split`,
     `by_scale`, `by_tag`, `by_asset` et un résumé `assets`
     (`any_detected`/`all_detected`).
  5. Garde-fous de génération : PNG validé, ffmpeg borné à 120 s, image
     overlay bornée à 6 s, offset < 9 s (fonds de 15 s).
- **Validation** : 34/39 (87,2 %, IC 95 % de Wilson 73,3–94,4 %),
  0/17 faux positifs, 13/13 masqués ; 1,0 : 11/13, 0,75 : 12/13,
  0,5 : 11/13 ; dev 19/21, holdout 15/18. Échecs : M6+ (0/3), Les 100 à
  1,0, Gulli à 0,5.
- **Limite** : 13 assets indépendants seulement ; les 39 cas ne sont pas
  indépendants et les 22 fonds sont partagés. La borne basse de Wilson
  (73,3 %) reste sous 90 %.
- **Références** : `scripts/gen_logo_asset_gallery.py`,
  `scripts/eval_logo_gallery.py`, `POC-RESULTS.md` §2.9.

## 2026-11-02 — D-045 : détection OCR à 736 testée et rejetée

- **Contexte** : RapidOCR (PoC Python) utilise `limit_side_len=736` avec
  `limit_type=min`. Rust gardait 480 : les bandes 1080×480 (sous-titres) et
  1080×614 (logos) restaient à leur résolution native, alors que Python les
  sur-échantillonne.
- **Test** : `DET_LIMIT_SIDE` passé à 736, sans autre changement, sur la
  galerie 13 assets × 2 transformations (26 positifs).
- **Résultat** : 22/26 positifs contre 24/26 à 480 ; 0 faux positif dans les
  deux cas. Les boîtes de détection fusionnent : Les 100 à 1,0 produit une
  boîte de 216 px de haut (filtre `MAX_BOX_HEIGHT = 180`), Gulli à 1,0
  devient carré (`MIN_ASPECT = 1,2`) et M6+ est lu « Ma+ », « al+ », « K+ »
  au lieu de « 161+ », « 11+ », « 1+ ».
- **Décision** : conserver 480 pour les deux bandes. La parité de
  préprocessing avec RapidOCR n'est pas un objectif en soi : le rappel et les
  filtres priment. Un test fige la taille native des bandes (`1088×608` pour
  1080×614, `1088×1920` pour une frame pleine).
- **Références** : `crates/workflow-steps/src/qa/ocr.rs`,
  `POC-RESULTS.md` §2.9, D-043/D-044.

## 2026-11-02 — D-044 : regroupement spatial des variantes OCR de logos

- **Contexte** : M6+ à l'échelle 0,75 était détecté sur 9 échantillons mais
  l'OCR produisait « 161+ », « 11+ » puis « 1+ », donc aucun texte
  n'atteignait 50 %. Gulli à 0,75 avait le même profil. La boîte est stable,
  seul le texte varie.
- **Décision** :
  1. La clé texte exacte reste prioritaire, pour conserver le comportement du
     baseline.
  2. Une détection peut rejoindre un groupe si la distance d'édition est ≤ 1
     (texte de 4 caractères ou moins) ou ≤ 2 (texte plus long), si la
     similarité est ≥ 0,6, et si la boîte est dans la même zone (IoU ≥ 0,3,
     ou centres proches avec une taille comparable).
  3. Un groupe ne compte qu'une fois par échantillon, même si l'OCR produit
     deux variantes. Les notes listent jusqu'à trois variantes OCR.
  4. Quatre tests couvrent la fusion, le refus de fusion à des positions
     distinctes, le refus de fusion de textes différents et le comptage
     unique par échantillon.
- **Validation** : assets réels 24/26 (92,3 %, IC 95 % de Wilson
  75,9–97,9 %), 0/10 faux positifs, 6/6 masqués ; galerie texte 60/60,
  0/16 ; lot 35 exports inchangé (30 `pass`, 2 exports `review` sur 270,
  4 findings, 259/386 détections masquées). 421 tests, `fmt` et
  `clippy -D warnings` verts.
- **Limites** : M6+ à 1,0 et Les 100 à 1,0 ne sont détectés qu'une seule
  fois ; le problème est le rappel OCR, pas l'agrégation. La borne basse de
  Wilson reste sous 90 %, donc G3 n'est pas conclue. Le regroupement exact
  par texte continue de fusionner deux logos identiques à des positions
  différentes, comme le baseline.
- **Références** : `docs/lots/LOT-10-QA.md`,
  `/media/kidpixel/WORK/qa-poc/reports/POC-RESULTS.md`,
  `crates/workflow-steps/src/qa/logo.rs`.

## 2026-11-02 — D-043 : galerie de logos réels et limite de l'OCR texte

- **Contexte** : la galerie texte 60/60 ne teste que des chaînes `drawtext`
  larges. Les assets PNG du corpus (M6+, Gulli, Les 100, LFAUIT, turbo…)
  incluent des logos stylisés, carrés ou courts, plus proches de la production.
- **Décision** :
  1. `gen_logo_asset_gallery.py` compose 13 PNG alpha valides sur 22 fonds
     propres : 26 positifs (13 assets × 2 transformations), 6 masqués et
     4 négatifs. Les PNG sont validés avant usage ; `211 LFAUIT.png`, fichier
     texte de 15 octets, est ignoré. Chaque ffmpeg est borné à 120 s.
  2. Les transformations sont contraintes pour garder le logo dans la frame
     et dans la bande haute (M6+ à droite).
  3. `eval_logo_gallery.py` accepte `--gallery` et `--reports` et normalise
     les slugs (accents, apostrophes) pour retrouver les 36 cas.
  4. Un log `debug` par détection logo expose texte, score et bbox ; il a
     permis d'identifier M6+ lu « 161+ », « 11+ » puis « 1+ ».
- **Validation** : 36 clips, `--no-face --no-evidence --workers 4`, 85 s de
  QA. Rappel 22/26 (84,6 %, IC 95 % de Wilson 66,5–93,9 %), faux positifs
  0/10, 6/6 masquages effectifs. Les logos à texte large passent ; M6+,
  Gulli à 0,75 et Les 100 à 1,0 échouent (texte instable, logo carré ou
  stylisé).
- **Conséquence** : la cible G3 de rappel ≥ 90 % n'est pas tenue sur les
  assets réels. La galerie texte 60/60 reste valable pour le sous-ensemble
  texte, mais DINOv2 ou un classifieur de logos reste nécessaire avant de
  considérer G3 acquise. À cette date, `auto_approve_eligible` restait faux ;
  D-049 le remplace par un calcul conditionnel.
- **Références** : `docs/lots/LOT-10-QA.md`,
  `/media/kidpixel/WORK/qa-poc/reports/POC-RESULTS.md`,
  `scripts/gen_logo_asset_gallery.py`, `scripts/eval_logo_gallery.py`.

## 2026-11-01 — D-042 : galerie synthétique de logos et préremplissage Rust

- **Contexte** : les 35 exports GOOD ne fournissent aucun positif réel
  labellisé, donc le rappel G3 n'est pas mesurable sur le corpus. Les labels
  dev pré-remplis depuis le baseline texte contenaient 13 faux positifs
  d'habillage éditorial, que le masque `.ecproj` écarte désormais.
- **Décision** :
  1. `scripts/gen_logo_gallery.py` génère 76 clips de 6 s sur 22 fonds réels
     des clips synthétiques (bande haute sans texte, offsets variés sur 1–9 s) :
     60 positifs persistants (textes de 24 à 96 px, quatre styles, huit
     positions), 8 cas masqués par un PNG déclaré, 4 textes transitoires (2 s
     sur 6) et 4 négatifs sans texte. Chaque clip reçoit un `.ecproj` minimal ;
     les cas masqués déclarent un PNG plein cadre dont la zone alpha couvre le
     texte.
  2. `scripts/eval_logo_gallery.py` compare les findings `logo_residual` aux
     attendus et écrit `metrics.json`.
  3. `scripts/prefill_labels.py` préfère `reports/rust-logo/<dossier>-<slug>`
     au baseline texte ; sans rapport Rust exploitable, il n'injecte plus les
     anciens candidats. Les 12 labels dev deviennent vides, cohérents avec
     les 12 exports dev qui passent tous.
  4. 270 reste un candidat `review` : le logo « MASUPER »/« LIGUEZ » est déjà
     dans le rush `MSL1.mov`, donc c'est le branding de l'émission. Aucune
     suppression automatique n'est ajoutée.
- **Validation** : galerie 76 clips sur 22 fonds, `--no-face --no-evidence
  --workers 4`, 177,2 s de mur. Rappel 60/60 (IC 95 % de Wilson 94,0–100 %),
  faux positifs 0/16 (IC 95 % 0–19,4 %), 8/8 masquages effectifs. Le lot 35 exports reste à
  30/32 `pass` et 2 exports `review` (270).
- **Limites** : texte `drawtext` net et overlay synthétique, même si les 22
  fonds sont réels ; ce résultat ne remplace pas une galerie de logos réels. Le rappel réel et les
  courbes P/R restent à produire.
- **Références** : `docs/lots/LOT-10-QA.md`,
  `/media/kidpixel/WORK/qa-poc/reports/POC-RESULTS.md`,
  `scripts/gen_logo_gallery.py`, `scripts/eval_logo_gallery.py`.

## 2026-11-01 — D-041 : masque des overlays `.ecproj` et contrôle logos

- **Contexte** : le baseline OCR de la bande haute produisait 15/35 flags sur
  les exports GOOD, dont 13 habillages éditoriaux. La note d'architecture
  conditionnait la détection de logos au masque des overlays intentionnels
  dérivé du `.ecproj`. Sans lui, un OCR de bande haute ne distingue pas un
  logo résiduel d'un titre d'émission ou d'un sticker de template.
- **Décision** :
  1. `post/overlay.rs` lit les calques image du `.ecproj` (`png`, `jpg`,
     `jpeg`, `webp`), décode le PNG (`png` 0.18) et réduit l'overlay à la
     boîte des pixels dont l'alpha dépasse 8. Les pixels alpha ≥ 200 sont
     comptés comme opaques.
  2. Les transformations statiques (ancre, position, échelle) et l'intervalle
     `in_point`/`out_point` sont appliqués. Un calque animé, pivoté ou sans
     PNG est listé dans `overlays_skipped` ; si aucun overlay n'est
     exploitable, le contrôle passe `not_applicable` au lieu de produire des
     faux positifs.
  3. L'asset est résolu par le chemin déclaré, puis `docs/` à côté de
     `projets/` ; le projet est canonicalisé pour que les liens symboliques
     du corpus de validation ne cassent pas la résolution. Un PNG sans canal
     alpha est refusé (masquer la frame entière désactiverait le contrôle).
  4. `qa/logo.rs` lit la bande haute 0–32 % sur 12 frames uniformes, applique
     les seuils du baseline (score ≥ 0,50, hauteur 18–180 px, largeur ≥ 30 px,
     aspect ≥ 1,2), écarte toute détection couverte à ≥ 50 % par un overlay
     actif, puis agrège les textes persistants sur ≥ 50 % des échantillons.
  5. Le contrôle exige un `.ecproj` ; il partage les sessions ONNX de l'OCR
     des sous-titres et tourne dans le même thread. `--no-logo` le désactive ;
     `--no-ocr` le désactive aussi.
  6. `qa.json` ajoute `stage1_logo_evidence` (`masked_total`, `overlays`,
     `overlays_skipped`), `timings_ms.stage1_logo_ms`, et `cache_version=2`
     invalide les rapports antérieurs.
- **Validation** : lot 35 exports à 4 workers, `--no-face --no-evidence` :
  30 `pass`, 3 `not_applicable` (sans projet), 2 exports en `review`
  (270, 4 findings) ; 386 détections, 259 masquées (67 %). Les 13 flags du baseline sur les exports
  avec projet sont tous couverts (191/211/276/287 LFAUIT, 223 turbo, 227 e=m6,
  266 EQC, 268 Lorie, 286 20H10, 298 les 100). Il reste 2 faux positifs
  connus (270 « MASUPER »/« LIGUEZ », branding présent dès les rushes), soit
  6,25 % des 32 exports évalués, sous la cible G3 de 10 %. Synthétique :
  « CHAIN LOGO » persistant 12/12 → 1 finding ; même clip avec overlay PNG
  déclaré → 12 détections masquées, 0 finding ; texte transitoire → 0 finding.
  QA complet 298 : 32,1 s (OCR 17,2 s, logos 7,4 s, visage 14,2 s). 76 tests
  `qa`, suite 417 tests, `fmt` et `clippy -D warnings` verts.
- **Limites** : le rappel réel n'est pas mesurable sur les GOOD (pas de
  positifs labellisés) ; 270 est confirmé comme branding source (présent dans
  `MSL1.mov`) mais reste un candidat `review` ; le masque ne couvre que les
  PNG déclarés, pas les habillages incrustés dans la source.
- **Références** : `docs/lots/LOT-10-QA.md`,
  `crates/workflow-steps/src/post/overlay.rs`,
  `crates/workflow-steps/src/qa/logo.rs`.

## Écarts legacy (D-001 → D-027)

| ID | Sujet | Décision Rust |
| --- | --- | --- |
| D-001 | CSV de scènes | Conserver `<stem>.csv` ; l'archive utilise `_scenes.csv`. |
| D-002 | STEP3 sans audio | `SKIP_NO_AUDIO`, aucune sortie, code 0. |
| D-003 | Ordre des clés `_audio.json` | Écrire `fps`/`total_frames` avant `frames_analysis`. |
| D-004 | Dépendances STEP4 | CSV et audio requis ; `--allow-partial-speaking` en test. |
| D-005 | `metadata.video_path` | Conserver le chemin absolu observé. |
| D-006 | Commentaires inline `.env` | Loader TOML, pas de parseur naïf. |
| D-007 | `speaker_stats` STEP3/STEP5 | Projection STEP5 documentée. |
| D-008 | JSON STEP4 non atomique | Écriture atomique. |
| D-009 | Parité STEP2–6 | Fixtures de référence. |
| D-010 | MOV alpha | Préserver à l'octet près. |
| D-011 | VRAM | Distinguer processus et carte. |
| D-012 | Verrou legacy | Verrou Rust distinct. |
| D-013 | Parallélisme serveur | Sérialisation, 409. |
| D-014 | STEP1 manquant | Porté au lot 6. |
| D-015 | Routes Python-only | Cache, stats, diagnostics et `/api/audio/*` restent dans Python ; les routes CSV sont portées. |
| D-016 | Extensions STEP1 | Reproduire le scan effectif (pas `.tar.gz`). |
| D-017 | Noms TAR non UTF-8 | Décodage lossy. |
| D-018 | Ingestion manquante | Portée au lot 8. |
| D-019 | Dossier de téléchargement | `runtime/downloads` isolé. |
| D-020 | Concurrence téléchargements | Borne, statut final après repli. |
| D-021 | Historique vide | Import explicite obligatoire. |
| D-022 | Validation Windows/CEP | Abandonnée. |
| D-023 | Racines séparées | Sync explicite. |
| D-024 | STEP4 `153 Camille` | Ne pas utiliser `--prefer-raw` sur ce projet. |
| D-025 | `/sound-design` hors racine | 404 standard de `ServeDir`, pas de middleware dédié. |
| D-026 | Frontières d'erreur | `thiserror` pour ingest/sync ; `anyhow` interne STEP2/3/4/7/8. |
| D-027 | Arrêt manuel CLI | Groupes de processus, SIGTERM puis SIGKILL ; STEP8 garde son gestionnaire. |

## Références

- `parity/DRIFT.md` (détails et preuves)
- `docs/lots/LOT-*.md`
- `docs/exploitation/SYNC-BACKENDS.md`
- `docs/exploitation/SWITCH-PUBLIC.md`
- `docs/exploitation/RETENTION.md`
- `docs/validation/VALIDATION-STEP1-8.md`
