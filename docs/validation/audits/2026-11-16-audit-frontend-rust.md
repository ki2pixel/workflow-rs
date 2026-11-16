L'audit technique autonome du frontend embarqué de workflow-rs (assets/web/) et de sa couche d'intégration serveur (crates/workflow-server/ et crates/workflow-core/) évalue la robustesse intrinsèque de l'architecture ES Modules sans étape de build.

## **1\. Grille d'Audit Interne (6 Axes)**

| Axe | Domaine audité | Périmètre et fichiers inspectés | Critères de conformité |
| :---- | :---- | :---- | :---- |
| **Axe 1** | **Architecture & Servabilité Minijinja** | crates/workflow-core/src/config.rs, crates/workflow-server/src/http.rs, assets/web/templates/index\_new.html | Prétraitement des balises Jinja/Flask (url\_for, tojson, csrf\_token, steps\_config.items()), garde réseau allow\_remote et validation worker\_token. |
| **Axe 2** | **Gestion d'état & Flux de données** | assets/web/static/state/AppState.js, assets/web/static/csvDownloadMonitor.js | Immuabilité de l'état (structuredClone), isolation des références dans getStateProperty, exactitude de \_mergeDeep, reset() et subscribeToProperty(). |
| **Axe 3** | **Performance DOM & Optimisations** | DOMBatcher.js, DOMDiff.js, WorkerManager.js, parseWorker.js, logParserUtils.js, domElements.js | Ordonnancement via requestAnimationFrame, morphing de listes avec clés (data-key/id), déport du parsing dans un Web Worker avec gestion de concurrence par canal. |
| **Axe 4** | **Gestion Réseau & Polling** | PollingManager.js, apiService.js, PerformanceMonitor.js, PerformanceOptimizer.js, ErrorHandler.js | Centralisation sous PollingManager, anti-empilement (inFlight), backoff exponentiel, suspension pauseWhenHidden (document.hidden) et cohérence du cache-busting ES Modules. |
| **Axe 5** | **Accessibilité (a11y) & UX / CSS** | base.css, layout.css, variables.css, themes.css, logs.css, steps.css, popups.css, csv-workflow-prompt.css | Sémantique ARIA (role="list", role="listitem", aria-live="polite", aria-hidden), focus trap, prefers-reduced-motion, responsivité mobile (95vw/90vh) et validité des variables CSS. |
| **Axe 6** | **Sécurité & Robustesse** | apiService.js, DOMBatcher.js, logParserUtils.js, parseWorker.js, csvWorkflowPrompt.js, domElements.js | Échappement XSS systématique (escapeHtml), validation d'origine Worker, assainissement des sélecteurs DOM et transport du header X-Worker-Token. |

## **2\. Synthèse des Constats par Niveau de Gravité**

| ID | Gravité | Composant / Fichier | Constat technique | Impact en exécution |
| :---- | :---- | :---- | :---- | :---- |
| **AUD-FE-01** | **Critique** | apiService.js (handleStepFailure) | Injection directe de errorMessage dans ui.updateMainLogOutputUI(\\\<i\>Erreur d'initiation: \${errorMessage}\</i\>\`)\` sans échappement HTML préalable. | Risque d'injection HTML/XSS dans le panneau de logs principal si un message d'erreur HTTP contient des balises HTML. |
| **AUD-FE-02** | **Critique** | PerformanceMonitor.js & ErrorHandler.js | generatePerformanceReport() appelle errorHandler.getStats() (ligne 343\) alors que ErrorHandler expose getErrorStats() (ligne 195). | TypeError: errorHandler.getStats is not a function levée toutes les 60 s dans sendPerformanceReport() et lors de tout appel à getMetrics(). |
| **AUD-FE-03** | **Majeur** | state/AppState.js (subscribeToProperty) | Comparaison par référence stricte (newValue \!== oldValue) après clonage intégral de l'arbre par \_deepClone à chaque setState(). | Tout abonné à une propriété de type objet ou tableau (ex. csvDownloads dans csvDownloadMonitor.js) est déclenché à chaque mise à jour de n'importe quelle autre clé d'état (toutes les 500 ms pendant le polling). |
| **AUD-FE-04** | **Majeur** | state/AppState.js (getStateProperty, \_mergeDeep, reset) | getStateProperty() retourne une référence directe non clonée ; reset() fusionne des objets vides {} via \_mergeDeep sans vider les clés existantes de processInfo ou stepTimers et omet 6 clés de ui. | Rupture possible de l'immuabilité par mutation directe et persistance d'états obsolètes après un reset(). |
| **AUD-FE-05** | **Majeur** | PerformanceMonitor.js & PerformanceOptimizer.js | Pollers perf-memory-usage (10 s), perf-Optimizer-stats (30 s) et perf-periodic-reporting (60 s) lancés sans pauseWhenHidden: true, et POST /api/performance/frontend envoyé sans X-Worker-Token tout en s'auto-enregistrant dans monitorApiCalls(). | Activité périodique non suspendue lorsque l'onglet est masqué et pollution des métriques d'appels API par le télémètre lui-même. |
| **AUD-FE-06** | **Majeur** | utils/DOMBatcher.js (DOMUpdateUtils.escapeHtml) | escapeHtml utilise div.textContent \= text; return div.innerHTML;, ce qui n'échappe ni les guillemets doubles (") ni les apostrophes ('), et alloue un nœud \<div\> à chaque appel. | Risque de sortie d'attribut HTML si le résultat est interpolé dans un attribut, et surcoût d'allocation DOM par rapport à escapeHtmlWorker. |
| **AUD-FE-07** | **Majeur** | utils/WorkerManager.js (\_executeTask) | Rejet de promesse (reject(new Error(...))) lorsqu'une réponse de Web Worker est ignorée car périmée sur un channel. | Risque de déclenchement du listener global unhandledrejection de ErrorHandler.js (qui affiche une notification d'erreur à l'utilisateur) lors de mises à jour rapides des logs. |
| **AUD-FE-08** | **Mineur** | variables.css, steps.css, csv-workflow-prompt.css | Utilisation de rgba(var(--accent-primary-rgb), 0.3) et rgba(var(--status-running-rgb), 0.25) alors que les variables \*-rgb sont définies avec des espaces (121 134 203\) sans virgules. | Règles box-shadow invalides en CSS Color 4 (rgba(121 134 203, 0.3)) et ignorées par le moteur de rendu (contrairement à logs.css qui utilise correctement rgb(var(...) / alpha)). |
| **AUD-FE-09** | **Mineur** | apiService.js, PerformanceMonitor.js, PerformanceOptimizer.js | Import de ./utils/PollingManager.js?v=20261025a avec suffixe de cache-busting uniquement sur ce module, tandis que tous les autres modules ES sont importés sans ?v=.... | Hétérogénéité du versionnement ES Modules ; tout import futur de ./utils/PollingManager.js sans le query parameter exact instancierait un second singleton PollingManager. |
| **AUD-FE-10** | **Mineur** | layout.css, downloads.css, steps.css, controls.css, workflow-buttons.css | Conflits et redéfinitions CSS entre fichiers (.local-downloads-section avec opacity: 1 \+ \[data-visible\] dans downloads.css vs opacity: 0 \+ .downloads-visible dans layout.css ; .compact-mode .steps-column en grid dans layout.css vs flex dans steps.css ; \#run-all-steps-button défini deux fois). | Dépendance fragile à l'ordre de chargement des feuilles de style et code CSS redondant. |
| **AUD-FE-11** | **Mineur** | csvWorkflowPrompt.js, themes.css, AppState.js | Aperçu du workflow codé en dur sur 6 étapes dans csvWorkflowPrompt.js au lieu des 8 étapes (STEP1–STEP8) ; thème par défaut 'dark' dans AppState.js alors que themes.css cible \[data-theme="dark-pro"\]. | Incohérence visuelle de la modale de prompt CSV avec le pipeline à 8 étapes et décalage de nommage du thème par défaut. |
| **AUD-FE-12** | **Mineur** | logs-column-global (index\_new.html / SKILL.md) | L'overlay de logs \#logs-column-global gère aria-hidden via popupManager, mais ne porte pas role="dialog" ni aria-modal="true" (contrairement aux popups de séquence et csvWorkflowPrompt). | Sémantique modale incomplète pour les lecteurs d'écran lors de l'ouverture plein écran de l'overlay de logs. |

## **3\. Analyse Détaillée par Axe**

### **Axe 1 : Architecture & Servabilité Minijinja (workflow-server & index\_new.html)**

> * **Architecture zéro-build** : Le frontend repose intégralement sur des ES Modules natifs sous assets/web/static/, sans bundler, transpileur ni dépendance Node/npm.  
> * **Prétraitement contractuel Minijinja (preprocess\_template)** : Le serveur Axum (crates/workflow-server/src/http.rs) applique les transformations suivantes avant le rendu de assets/web/templates/index\_new.html :  
  * url\_for('static', filename='...') est converti en /static/....  
  * steps\_config.items() est converti en filtre Minijinja steps\_config\|items.  
  * csrf\_token() est neutralisé en chaîne vide.  
  * worker\_token if worker\_token else '' est normalisé en worker\_token.  
  * steps\_config \| tojson \| safe est remplacé par steps\_config\_json \| safe, sérialisé côté Rust.  
> * **Validation de sécurité au démarrage du serveur (Config::validate\_server)** :  
  * Le port 5003 est explicitement refusé afin d'éviter toute collision avec le serveur Python legacy (port par défaut 5004).  
  * Toute écoute sur une adresse non-loopback (\!is\_loopback\_host(\&self.server.host)) est bloquée sauf si server.allow\_remote \= true est activé.  
  * Si server.allow\_remote \= true, le serveur exige un server.secret\_key non vide et non préfixé par dev-.  
  * Le jeton server.worker\_token est systématiquement vérifié : le démarrage échoue s'il est vide, égal à "dev-internal-worker-token", ou préfixé par "dev-".  
> * **Structure sémantique de la timeline et de l'overlay** :  
  * La timeline utilise .pipeline-timeline\[role="list"\] et .step.timeline-step\[role="listitem"\]\[tabindex="0"\] avec les identifiants contractuels \#step-STEP1 à \#step-STEP8, \#state-chip-STEPn\[aria-live="polite"\], \#progress-bar-STEPn\[role="progressbar"\] et \#progress-text-STEPn\[aria-live="polite"\].  
  * L'overlay de logs conserve les ancres contractuelles \#logs-column-global, \#log-panel-title, \#close-log-panel, \#log-panel-subheader\[aria-live="polite"\], \#log-panel-specific-buttons-container, \#main-log-output-panel\[aria-live="polite"\]\[aria-atomic="false"\] et .specific-log-button.

### **Axe 2 : Gestion d'État & Flux de Données (AppState.js)**

> * **Points forts** :  
  * AppState gère la persistance synchrone dans localStorage au démarrage (\_loadPersistedState), la migration des anciennes clés (LEGACY\_MIGRATIONS) et la purge one-shot de la sélection d'étapes lors de la renumérotation (appstate:migration:stepKeysV2).  
  * getState() protège l'état interne via structuredClone (avec repli récursif manuel) et \_areValuesEqual() gère les références circulaires via WeakMap.  
> * **Anomalies identifiées** :  
  1. **Sur-notification critique dans subscribeToProperty(path, listener) (AUD-FE-03)** :  
     `// assets/web/static/state/AppState.js (lignes 138-149)`  
     `subscribeToProperty(path, listener) {`  
         `const propertyListener = (newState, oldState) => {`  
             `const newValue = this._getPropertyByPath(newState, path);`  
             `const oldValue = this._getPropertyByPath(oldState, path);`  
             `if (newValue !== oldValue) {`  
                 `listener(newValue, oldValue);`  
             `}`  
         `};`  
         `return this.subscribe(propertyListener);`  
     `}`  
     Comme setState() produit oldState \= this.\_deepClone(this.state) et newState \= this.\_mergeDeep(this.state, updates) (qui appelle aussi \_deepClone), toute propriété de type Object ou Array reçoit une nouvelle référence mémoire dans newState et oldState à chaque appel de setState(). Par conséquent, newValue \!== oldValue est toujours true pour un tableau tel que csvDownloads (écouté par csvDownloadMonitor.js), déclenchant handleDownloadStatusUpdate à chaque tick de polling d'étape (processInfo).  
  2. **Fuite de référence dans getStateProperty(path) (AUD-FE-04)** : getStateProperty(path) renvoie directement la sous-propriété de this.state sans la cloner. Toute modification in-place par l'appelant altère silencieusement l'état interne sans passer par setState.  
  3. **Réinitialisation incomplète dans reset() (AUD-FE-04)** : reset() passe processInfo: {} et stepTimers: {} à this.setState(initialState, 'reset'). Or \_mergeDeep(target, source) fusionne récursivement les sous-objets sans supprimer les clés absentes de source : un objet vide {} dans initialState conserve donc toutes les entrées existantes de this.state.processInfo et this.state.stepTimers. De plus, initialState.ui dans reset() omet autoScroll, sequenceAutoScroll, soundEnabled, soundVolume, theme et localDownloadsAlertedOnce.

### **Axe 3 : Performance DOM & Optimisations (DOMBatcher.js, DOMDiff.js, WorkerManager.js)**

> * **Points forts** :  
  * DOMUpdateBatcher regroupe les mutations DOM dans une unique passe requestAnimationFrame, trie les opérations par priorité (priority) et surveille le budget de frame de 16 ms.  
  * DOMDiff.morph() effectue un diffing chirurgical en place des attributs, propriétés spéciales (value, checked, disabled) et nœuds enfants avec réconciliation par clé (data-key ou id).  
  * WorkerManager déporte la coloration syntaxique des logs (parseLogs) et le parsing JSON lourd (parseJson) dans parseWorker.js, tout en attribuant un identifiant séquentiel par canal (latestRequestForChannel) pour éviter l'application hors-ordre de résultats asynchrones.  
> * **Anomalies identifiées** :  
  1. **Rejet de promesse sur les réponses obsolètes du Worker (AUD-FE-07)** : Dans WorkerManager.\_executeTask (lignes 73-77), lorsqu'un message plus récent a été émis sur le même channel, la promesse de la requête précédente est rejetée via reject(new Error('\[WorkerManager\] Réponse obsolète ignorée...')). Si l'appelant n'intercepte pas spécifiquement cette erreur bénigne d'annulation, elle peut remonter jusqu'au gestionnaire global unhandledrejection de ErrorHandler.js.  
  2. **Absence de nettoyage du Worker au déchargement** : Contrairement à AppState et DOMBatcher, workerManager.destroy() n'est pas attaché à l'événement beforeunload.

### **Axe 4 : Gestion Réseau & Polling (PollingManager.js, apiService.js, PerformanceMonitor.js)**

> * **Points forts** :  
  * PollingManager implémente un verrou this.inFlight.has(name) qui empêche tout chevauchement de requêtes lentes (anti-pile-up) et comptabilise les ticks sautés dans skippedTicks.  
  * En cas d'erreur réseau transitoire, PollingManager applique un backoff exponentiel plafonné à 30 s (Math.min(interval \* Math.pow(2, errorCount), 30000)) via \_reschedule sans tuer le poller.  
  * Le support de pauseWhenHidden permet de suspendre les pollers lorsque document.hidden est vrai (minimisation en barre système) et de les rafraîchir immédiatement au retour au premier plan.  
> * **Anomalies identifiées** :  
  1. **Exception toutes les 60 s dans PerformanceMonitor (AUD-FE-02)** : PerformanceMonitor.generatePerformanceReport() appelle errorHandler.getStats() (ligne 343\) au lieu de errorHandler.getErrorStats(), provoquant une exception systématique lors du rapport périodique.  
  2. **Pollers de monitoring actifs hors focus et auto-instrumentation fetch (AUD-FE-05)** :  
     * PerformanceMonitor.js démarre automatiquement au chargement de la page et lance deux pollers (perf-memory-usage à 10 s et perf-periodic-reporting à 60 s) sans l'option { pauseWhenHidden: true }.  
     * PerformanceOptimizer.js lance également perf-optimizer-stats (30 s) sur localhost/127.0.0.1 sans pauseWhenHidden: true, et utilise un setTimeout natif (ligne 319\) en cas d'erreur d'import.  
     * PerformanceMonitor.monitorApiCalls() surcharge globalThis.fetch, de sorte que l'appel fetch('/api/performance/frontend', { method: 'POST', ... }) effectué par sendPerformanceReport() s'enregistre lui-même dans this.metrics.apiCalls et n'inclut pas l'en-tête X-Worker-Token.  
  3. **Utilisation de setTimeout natifs dans ErrorHandler.js** : ErrorHandler.\_showErrorNotification utilise setTimeout / clearTimeout natifs (lignes 212, 249, 260, 272\) au lieu de pollingManager.setTimeout.  
  4. **Disparité du cache-busting ES Modules (AUD-FE-09)** : apiService.js, PerformanceMonitor.js et PerformanceOptimizer.js importent ./utils/PollingManager.js?v=20261025a, tandis que tous les autres modules sont importés sans paramètre de version.  
  5. **Destruction définitive sur pagehide** : PollingManager.\_bindCleanupEvents() appelle this.destroy() sur pagehide (ligne 333). Si le navigateur met la page en cache de navigation (bfcache) puis la restaure, this.isDestroyed reste à true et bloque tout nouveau polling.

### **Axe 5 : Accessibilité (a11y) & UX / CSS**

> * **Points forts** :  
  * Prise en charge globale et granulaire de @media (prefers-reduced-motion: reduce) dans base.css, layout.css, downloads.css, logs.css, popups.css, steps.css et animations.css.  
  * Responsive mobile de l'overlay de logs conforme aux spécifications : .logs-content-wrapper passe à width: 95vw; height: 90vh; sous @media (max-width: 900px) dans logs.css.  
  * Lien d'évitement .skip-link fonctionnel au focus clavier dans base.css et anneaux :focus-visible sur l'ensemble des contrôles interactifs.  
  * Gestion du focus trap, de la touche Escape et de la restauration du focus sur l'élément déclencheur (focusedElementBeforePopup) par popupManager.  
> * **Anomalies identifiées** :  
  1. **Syntaxe rgba() invalide avec les variables RGB séparées par des espaces (AUD-FE-08)** : Dans variables.css, les variables RGB sont déclarées selon la syntaxe moderne sans virgules :  
     `--accent-primary-rgb: 121 134 203;`  
     `--status-running-rgb: 77 171 247;`  
     Or csv-workflow-prompt.css (lignes 169, 174\) et steps.css (lignes 382, 389, 397, 421\) les utilisent avec une virgule : rgba(var(--accent-primary-rgb), 0.3) et rgba(var(--status-running-rgb), 0.25). En CSS, rgba(121 134 203, 0.3) est syntaxiquement invalide (mélange d'espaces et de virgule) ; il faut utiliser rgb(var(--accent-primary-rgb) / 0.3) comme dans logs.css.  
  2. **Règles CSS dupliquées et contradictoires (AUD-FE-10)** :  
     * .local-downloads-section est défini avec opacity: 1; transform: none; et piloté par \[data-visible="true"\|"false"\] dans components/downloads.css, mais redéfini avec opacity: 0; transform: translateX(30px) scale(0.98); et piloté par .downloads-visible dans layout.css.  
     * .workflow-wrapper.compact-mode .steps-column est défini en display: grid dans layout.css (ligne 39\) puis écrasé en display: flex; flex-direction: column; dans components/steps.css (ligne 321).  
     * \#run-all-steps-button est stylisé deux fois avec des propriétés divergentes dans components/controls.css (ligne 97\) et components/workflow-buttons.css (lignes 12 et 52).  
  3. **Couleurs en dur et désalignement thématique (AUD-FE-11)** :  
     * csv-workflow-prompt.css et les classes .log-debug / .log-command de logs.css utilisent des couleurs hexadécimales en dur et @media (prefers-color-scheme: dark) au lieu des variables de themes.css (\[data-theme="..."\]).  
     * AppState.js initialise ui.theme: 'dark' (ligne 53\) alors que themes.css définit \[data-theme="dark-pro"\].  
  4. **Attributs ARIA de l'overlay de logs (AUD-FE-12)** : Contrairement aux modales de séquence et à csvWorkflowPrompt qui portent role="dialog" et aria-modal="true", \#logs-column-global n'utilise que aria-hidden togglé par popupManager.

### **Axe 6 : Sécurité & Robustesse (XSS, Worker Token, Sélecteurs DOM)**

> * **Points forts** :  
  * **Validation des clés DOM** : domElements.js valide stepKey via la regex stricte /^\[A-Za-z0-9\_-\]+\$/ (\_SAFE\_STEP\_KEY\_PATTERN) avant toute requête document.getElementById(\\step-\${stepKey}\`)\`.  
  * **Sécurité du Web Worker** : parseWorker.js vérifie if (e.origin && e.origin \!== self.location.origin) avant de traiter un message et échappe systématiquement les lignes de log via escapeHtmlWorker (qui échappe &, \<, \>, ", ').  
  * **Transport du X-Worker-Token** : fetchWithLoadingState dans apiService.js extrait \<meta name="worker-token"\> et l'attache dans l'en-tête X-Worker-Token sur les actions POST /run/$stepKeyetPOST/cancel/${stepKey}.  
  * **Assainissement des URL externes** : csvWorkflowPrompt.js valide les URL externes via new URL() en restreignant le protocole à http: et https:, et ouvre les liens avec rel="noopener noreferrer".  
> * **Anomalies identifiées** :  
  1. **Injection HTML non échappée dans handleStepFailure (AUD-FE-01)** : Dans assets/web/static/apiService.js (ligne 99\) :  
     `if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey) {`  
         ``ui.updateMainLogOutputUI(`<i>Erreur d'initiation: ${errorMessage}</i>`);``  
     `}`  
     Si errorMessage contient des chevrons ou des balises renvoyés par une erreur serveur ou intermédiaire, ils sont interprétés comme du HTML. Il convient d'échapper errorMessage via DOMUpdateUtils.escapeHtml(errorMessage).  
  2. **Échappement incomplet des guillemets dans DOMUpdateUtils.escapeHtml (AUD-FE-06)** : Dans assets/web/static/utils/DOMBatcher.js (lignes 257-261) :  
     `static escapeHtml(text) {`  
         `const div = document.createElement('div');`  
         `div.textContent = text;`  
         `return div.innerHTML;`  
     `}`  
     div.innerHTML n'échappe pas " ni ', contrairement à escapeHtmlWorker dans logParserUtils.js.

## **4\. Vérification Dynamique et Tests Automatisés**

Dans le cadre de l'architecture workflow-rs, la validation automatisée de l'intégration serveur et du prétraitement Minijinja s'appuie sur la suite de tests unitaires inline (\#\[cfg(test)\]) du workspace Rust (689 tests listés dans le workspace) :

> * **Tests de rendu Minijinja et contrat HTTP (workflow-server)** :  
  * cargo test \-p workflow-server \--lib http::tests : valide le routeur Axum, l'authentification par X-Worker-Token et les en-têtes HTTP.  
  * cargo test \-p workflow-server index\_renders\_without\_a\_template\_engine\_specific\_filter : garantit que preprocess\_template() élimine toute dépendance aux filtres spécifiques à Flask/Jinja2 (tojson, url\_for, csrf\_token).  
  * cargo test \-p workflow-server real\_index\_template\_renders\_with\_existing\_frontend\_syntax : vérifie que le fichier réel assets/web/templates/index\_new.html est rendu sans erreur par Minijinja avec le catalogue des 8 étapes (STEP1 à STEP8).  
> * **Tests de validation de la configuration serveur (workflow-core)** :  
  * legacy\_port\_is\_rejected\_for\_the\_rust\_server : vérifie le rejet du port 5003 et l'acceptation du port 5004\.  
  * worker\_token\_default\_is\_rejected\_for\_server : vérifie le refus d'un worker\_token vide ou préfixé par dev-.  
  * non\_loopback\_host\_requires\_allow\_remote et allow\_remote\_rejects\_missing\_or\_trivial\_secret\_key : vérifient la garde réseau sur server.host, server.allow\_remote et server.secret\_key.

## **5\. Plan de Remédiation Interne (Zéro Dépendance Node/npm)**

Toutes les corrections ci-dessous s'appliquent directement dans assets/web/static/ sans modifier les sélecteurs DOM contractuels (\#step-STEPn, .specific-log-button, data-step, data-log-type) ni introduire d'outillage externe.

### **Correctif 1 : Assainissement XSS dans apiService.js et DOMBatcher.js (AUD-FE-01, AUD-FE-06)**

Dans assets/web/static/utils/DOMBatcher.js, remplacer l'implémentation de DOMUpdateUtils.escapeHtml par un échappement complet sans allocation de nœud DOM (aligné sur escapeHtmlWorker) :  
`// assets/web/static/utils/DOMBatcher.js`  
`static escapeHtml(text) {`  
    `if (text === null || text === undefined) return '';`  
    `return String(text)`  
        `.replace(/&/g, "&")`  
        `.replace(/</g, "<")`  
        `.replace(/>/g, ">")`  
        `.replace(/"/g, """)`  
        `.replace(/'/g, "'");`  
`}`

Dans assets/web/static/apiService.js, échapper errorMessage dans handleStepFailure :  
`// assets/web/static/apiService.js`  
`if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey) {`  
    `const safeMsg = DOMUpdateUtils.escapeHtml(errorMessage);`  
    ``ui.updateMainLogOutputUI(`<i>Erreur d'initiation: ${safeMsg}</i>`);``  
`}`

### **Correctif 2 : Réparation de PerformanceMonitor.js et suspension hors focus (AUD-FE-02, AUD-FE-05)**

Dans assets/web/static/utils/PerformanceMonitor.js et assets/web/static/utils/PerformanceOptimizer.js :

> 1. Remplacer errorHandler.getStats() par errorHandler.getErrorStats() à la ligne 343\.  
> 2. Passer { pauseWhenHidden: true } aux appels pollingManager.startPolling('perf-memory-usage', ...), pollingManager.startPolling('perf-periodic-reporting', ...) et au poller de développement perf-optimizer-stats.  
> 3. Utiliser this.originalFetch (au lieu du globalThis.fetch instrumenté) dans sendPerformanceReport() pour éviter l'auto-enregistrement infini des requêtes de télémétrie et transporter le jeton worker :

`// assets/web/static/utils/PerformanceMonitor.js`  
`pollingManager.startPolling('perf-memory-usage', recordMemory, 10000, { pauseWhenHidden: true });`  
`// ...`  
`pollingManager.startPolling('perf-periodic-reporting', () => this.sendPerformanceReport(), 60000, { pauseWhenHidden: true });`  
`// ...`  
`generatePerformanceReport() {`  
    `const optimizerStats = performanceOptimizer.getPerformanceStats();`  
    `const batcherStats = domBatcher.getStats();`  
    `const pollingStats = pollingManager.getStats();`  
    `const errorStats = errorHandler.getErrorStats();`  
    `// ...`

`// assets/web/static/utils/PerformanceMonitor.js (sendPerformanceReport)`  
`const fetchFn = this.originalFetch || globalThis.fetch;`  
`const tokenEl = document.querySelector('meta[name="worker-token"]');`  
`const token = tokenEl ? tokenEl.getAttribute('content') : '';`  
`const headers = { 'Content-Type': 'application/json' };`  
`if (token) {`  
    `headers['X-Worker-Token'] = token;`  
`}`  
`await fetchFn('/api/performance/frontend', { method: 'POST', headers, body: JSON.stringify(report) });`

`// assets/web/static/utils/PerformanceOptimizer.js (startStatsPolling)`  
`pm.startPolling('perf-optimizer-stats', () => { /* ... */ }, 30000, { pauseWhenHidden: true });`

### **Correctif 3 : Comparaison structurelle et immuabilité dans AppState.js (AUD-FE-03, AUD-FE-04, AUD-FE-11)**

Dans assets/web/static/state/AppState.js :

> 1. Utiliser \!this.\_areValuesEqual(newValue, oldValue) dans subscribeToProperty afin de ne notifier les abonnés (csvDownloadMonitor.js) que lorsque la valeur de la propriété a réellement changé.  
> 2. Cloner la valeur retournée par getStateProperty(path) via this.\_deepClone(...).  
> 3. Aligner le thème initial sur 'dark-pro' et restaurer l'intégralité de l'objet this.state dans reset() :

`// assets/web/static/state/AppState.js`  
`getStateProperty(path) {`  
    `if (this.isDestroyed) return undefined;`  
    `const value = path.split('.').reduce((obj, key) => {`  
        `return obj && obj[key] !== undefined ? obj[key] : undefined;`  
    `}, this.state);`  
    `return this._deepClone(value);`  
`}`

`subscribeToProperty(path, listener) {`  
    `const propertyListener = (newState, oldState) => {`  
        `const newValue = this._getPropertyByPath(newState, path);`  
        `const oldValue = this._getPropertyByPath(oldState, path);`  
        `if (!this._areValuesEqual(newValue, oldValue)) {`  
            `listener(newValue, oldValue);`  
        `}`  
    `};`  
    `return this.subscribe(propertyListener);`  
`}`

Dans `reset()`, remplacer la fusion par un remplacement franc de l'état complet
(toutes les clés du constructeur, `ui` à 11 propriétés dont `theme: 'dark-pro'`)
avec `_stateChanged`, `stateChangeCount`, `_persistStateChanges` et
`_notifyListeners(newState, oldState, 'reset')`, puis conserver la purge
`try/catch` des `PERSISTED_PATHS` dans `localStorage`. `_mergeDeep` reste la
fusion partielle de `setState()` ; il traite désormais les valeurs non plaines
(nœud DOM de `focusedElementBeforePopup`) comme des feuilles et clone les
tableaux pour couper tout alias avec l'appelant.

### **Correctif 4 : Correction de la syntaxe CSS rgb(var(...) / alpha) (AUD-FE-08)**

Dans assets/web/static/css/components/csv-workflow-prompt.css et assets/web/static/css/components/steps.css, remplacer les appels rgba(var(--\*-rgb), \<alpha\>) par la syntaxe CSS Color 4 compatible avec les variables séparées par des espaces de variables.css :  
`/* assets/web/static/css/components/csv-workflow-prompt.css */`  
`.workflow-launch-btn {`  
    `box-shadow: 0 4px 12px rgb(var(--accent-primary-rgb) / 0.3);`  
`}`  
`.workflow-launch-btn:hover {`  
    `box-shadow: 0 6px 20px rgb(var(--accent-primary-rgb) / 0.4);`  
`}`

`/* assets/web/static/css/components/steps.css */`  
`.timeline-step:hover:not(.active-for-log-panel) {`  
    `box-shadow:`  
        `0 20px 40px rgba(0, 0, 0, 0.25),`  
        `0 0 0 1px rgb(var(--accent-primary-rgb) / 0.2);`  
`}`  
`.timeline-step:focus-within:not(.active-for-log-panel),`  
`.timeline-step.is-selected {`  
    `box-shadow:`  
        `0 16px 34px rgba(0, 0, 0, 0.24),`  
        `0 0 0 2px rgb(var(--accent-primary-rgb) / 0.26);`  
`}`  
`.timeline-step[data-status="running"],`  
`.timeline-step[data-status="starting"],`  
`.timeline-step[data-status="initiated"] {`  
    `box-shadow: 0 10px 30px rgb(var(--status-running-rgb) / 0.25);`  
`}`

### **Correctif 5 : Réponses Worker obsolètes, cache-busting et aperçu CSV à 8 étapes (AUD-FE-07, AUD-FE-09, AUD-FE-11)**

Dans `assets/web/static/utils/WorkerManager.js`, une réponse périmée sur un canal résout désormais la promesse avec `null` en consignant un `console.debug`, au lieu de la rejeter : le listener global `unhandledrejection` d'`ErrorHandler.js` ne reçoit plus cette annulation bénigne. Le fallback synchrone suit la même convention, et `workerManager.destroy()` est attaché à `beforeunload`, comme `domBatcher`, `appState` et `pollingManager`. Dans `assets/web/static/uiUpdater.js`, `updateSpecificLogUI` et `updateMainLogOutputUI` ignorent une valeur `null`, donc une réponse périmée n'écrase plus le panneau courant.

Dans les sept modules importateurs (`main.js`, `apiService.js`, `sequenceManager.js`, `timerManager.js`, `uiUpdater.js`, `utils/PerformanceMonitor.js`, `utils/PerformanceOptimizer.js`, y compris l'import dynamique), le spécificateur passe de `PollingManager.js?v=20261025a` au chemin canonique `PollingManager.js`. Axum servant déjà `Cache-Control: no-cache, must-revalidate`, le query parameter statique n'apportait aucune fraîcheur et pouvait créer un second singleton avec son propre registre `inFlight`.

Dans `assets/web/static/csvWorkflowPrompt.js`, l'aperçu `.workflow-steps` remplace les six badges legacy par les huit étapes canoniques (Extraction, Transitions, Audio, Tracking, Réduction JSON, Pré-traitement AE, Finalisation, Media Solution) et le texte de pied de modale passe à « étapes 1-8 ». `runStepSequence(defaultSequenceableStepsKeys, "Séquence CSV Auto")` consommait déjà les huit clés de `constants.js` ; seul l'affichage était resté sur l'ancien découpage.

## **6\. Grille de suivi des correctifs**

| Correctif | Constats | Fichiers | État | Preuve |
| :---- | :---- | :---- | :---- | :---- |
| **1 — Assainissement XSS** | AUD-FE-01, AUD-FE-06 | `assets/web/static/utils/DOMBatcher.js`, `assets/web/static/apiService.js` | **Corrigé et validé** (2026-11-16, D-074) | `escapeHtml` échappe `&`, `<`, `>`, `"`, `'` sans allocation DOM ; `handleStepFailure` échappe `errorMessage` avant `updateMainLogOutputUI` ; `node --check` sur les deux modules, cas d'échappement vérifiés, `cargo test -p workflow-server --lib`, `cargo test --workspace`, `cargo fmt --check` et `cargo clippy --workspace --all-targets -- -D warnings` verts. |
| **2 — PerformanceMonitor et suspension hors focus** | AUD-FE-02, AUD-FE-05 | `assets/web/static/utils/PerformanceMonitor.js`, `assets/web/static/utils/PerformanceOptimizer.js` | **Corrigé et validé** (2026-11-16, D-075) | `generatePerformanceReport()` appelle `errorHandler.getErrorStats()` (plus de `TypeError` toutes les 60 s) ; `perf-memory-usage`, `perf-periodic-reporting` et `perf-optimizer-stats` passent `{ pauseWhenHidden: true }` ; `sendPerformanceReport()` utilise `this.originalFetch` et transporte `X-Worker-Token` issu de `<meta name="worker-token">` ; harnais Node : rapport sans exception, `components.errorHandler` conforme, trois pollers suspendus, télémétrie absente de `metrics.apiCalls`, token présent ; `node --check` sur les deux modules, `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu Minijinja, `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés), `cargo fmt --check` et `cargo clippy --workspace --all-targets -- -D warnings` verts. |
| **3 — Comparaison structurelle et immuabilité** | AUD-FE-03, AUD-FE-04, AUD-FE-11 | `assets/web/static/state/AppState.js` | **Corrigé et validé** (2026-11-16, D-076) | `subscribeToProperty` compare via `_areValuesEqual`, donc `csvDownloads` n'est plus notifié par les ticks `processInfo` ; `getStateProperty` clone via `_deepClone` (références hôtes préservées) ; `reset()` remplace l'état complet et vide `processInfo`, `stepTimers` et `pollingIntervals` (11 clés `ui`, `theme: 'dark-pro'`) ; harnais Node 15/15 (isolation `csvDownloads`, mutation de clone sans effet, reset exhaustif et idempotent, référence DOM et horodatages `Date` conservés), `node --check`, `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu Minijinja, `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés), `cargo fmt --check` et `cargo clippy --workspace --all-targets -- -D warnings` verts. |
| **4 — Syntaxe CSS `rgb(var(...) / alpha)`** | AUD-FE-08 | `assets/web/static/css/components/csv-workflow-prompt.css`, `assets/web/static/css/components/steps.css` | **Corrigé et validé** (2026-11-16, D-077) | Les six `box-shadow` utilisent `rgb(var(--*-rgb) / <alpha>)` : `0.2` (survol de carte), `0.26` (`:focus-within` et `.is-selected`), `0.25` (statut `running`), `0.3` et `0.4` (`.workflow-launch-btn` et son survol) ; valeurs d'alpha, sélecteurs et ordre des déclarations inchangés. `grep -rn -- "rgba(var(--" assets/web/static/css/` vide ; `tinycss2` : 0 erreur (105 + 36 règles, 362 + 186 déclarations) ; accolades/parenthèses équilibrées ; A/B Chromium sur une copie des trois feuilles : l'ancienne forme via variable calcule `none`, les six déclarations corrigées calculent `rgba(121, 134, 203, 0.2/0.26/0.3/0.4)` et `rgba(77, 171, 247, 0.25)` ; `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu Minijinja, `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés), `cargo fmt --check` et `cargo clippy --workspace --all-targets -- -D warnings` verts. |
| **5 — Worker, cache-busting et aperçu CSV** | AUD-FE-07, AUD-FE-09, AUD-FE-11 | `assets/web/static/utils/WorkerManager.js`, `assets/web/static/uiUpdater.js`, sept modules importateurs de `PollingManager.js`, `assets/web/static/csvWorkflowPrompt.js` | **Corrigé et validé** (2026-11-16, D-078) | Une réponse Worker périmée sur un canal résout `null` avec `console.debug` (chemin Worker et fallback synchrone) au lieu de rejeter ; `workerManager.destroy()` est attaché à `beforeunload` ; `updateSpecificLogUI` et `updateMainLogOutputUI` ignorent `null` sans écraser le DOM. Les sept imports passent à `PollingManager.js` sans query parameter ; `grep -rn "20261025a" assets/web/static/` vide. L'aperçu CSV affiche les huit badges canoniques et « étapes 1-8 » ; la séquence consomme déjà les huit clés de `constants.js`. `node --check` sur les neuf modules, harnais Node jetables 6/6 (fallback) et 5/5 (Worker simulé, réponses hors ordre, `unhandledrejection` vide), `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu Minijinja, `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés), `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` et `git diff --check` verts. |
| **6 — Sémantique modale de l'overlay de logs et assainissement CSS** | AUD-FE-10, AUD-FE-12 | `assets/web/templates/index_new.html`, `assets/web/static/css/layout.css`, `assets/web/static/css/components/controls.css`, `assets/web/static/css/components/steps.css`, `assets/web/static/css/components/workflow-buttons.css` | **Corrigé et validé** (2026-11-16, D-079) | `#logs-column-global` porte `role="dialog" aria-modal="true" aria-labelledby="log-panel-title"`, aligné sur les popups de séquence ; `#log-panel-title` reste le titre ciblé et `popupManager.js` continue de piloter `aria-hidden`, le focus trap et `Escape` par l'ID. Le bloc `.local-downloads-section` (`opacity: 0`, `.downloads-visible`, `prefers-reduced-motion`) quitte `layout.css` : `components/downloads.css` est l'unique source (`[data-visible]`), et `downloads-visible` ne subsiste nulle part. La règle grid `.workflow-wrapper.compact-mode .steps-column` et son media query 1200 px quittent `layout.css` : `components/steps.css` (ligne 455) reste l'unique source (`display: flex`, `gap`, `transition: opacity`). Les règles de base et de survol de `#run-all-steps-button` quittent `components/controls.css` ; `components/workflow-buttons.css` porte désormais tout le bouton (base, `min-height: var(--touch-target-min)`, shine, survol, actif, désactivé, dégradé). Preuves : `grep` ciblés (0 occurrence de `downloads-visible`, `#run-all-steps-button` uniquement dans `workflow-buttons.css` + `responsive.css`, `.compact-mode .steps-column` uniquement dans `steps.css`) ; `tinycss2` 1.4.0 : 0 erreur sur les cinq feuilles, accolades et parenthèses équilibrées ; A/B Firefox 158 headless + geckodriver (1400 px et 700 px, feuilles chargées dans l'ordre de `index_new.html`) : styles calculés et tailles identiques pour `.local-downloads-section` (`[data-visible="true"]` et `"false"`), `.steps-column` compact et `#run-all-steps-button` (base et survol) ; seuls quatre longhands grid inertes diffèrent (`grid-template-columns`, `grid-auto-flow`, `align-content`, `justify-items`) alors que `display` calcule `flex` dans les deux variantes et que la taille reste 1356 × 503,2 px (desktop) et 656 × 553,4 px (mobile) ; nouvelles assertions ARIA dans `real_index_template_renders_with_existing_frontend_syntax`, `cargo test -p workflow-server --lib` (46 tests), les deux tests de rendu Minijinja, `cargo test --workspace` (689 listés, 687 exécutés + 2 ignorés), `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` et `git diff --check` verts. |

## **7\. Synthèse finale**

Les douze constats **AUD-FE-01 → AUD-FE-12 sont 100 % résolus et validés** au
2026-11-16, sans dépendance Node/npm et sans modification du dépôt Python
legacy :

- Correctif 1 (D-074) : AUD-FE-01, AUD-FE-06.
- Correctif 2 (D-075) : AUD-FE-02, AUD-FE-05.
- Correctif 3 (D-076) : AUD-FE-03, AUD-FE-04, AUD-FE-11 (thème par défaut).
- Correctif 4 (D-077) : AUD-FE-08.
- Correctif 5 (D-078) : AUD-FE-07, AUD-FE-09, AUD-FE-11 (aperçu CSV à 8 étapes).
- Correctif 6 (D-079) : AUD-FE-10, AUD-FE-12.

Les sélecteurs contractuels (`#logs-column-global`, `#log-panel-title`,
`#run-all-steps-button`, `.local-downloads-section`, `.compact-mode`,
`.steps-column`), les contrats HTTP/JSON/CSV et le prétraitement Minijinja sont
préservés. Le seul écart de comportement assumé est la suppression de règles CSS
mortes ou entièrement surchargées ; les déclarations encore effectives
(`transition: opacity` du mode compact, `min-height` du bouton principal) ont
été déplacées vers la feuille qui fait désormais autorité, avec une équivalence
de styles calculés mesurée en A/B.
