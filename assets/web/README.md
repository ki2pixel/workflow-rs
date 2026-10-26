# Frontend web

Copie du frontend natif de `workflow_mediapipe` :

- `static/` : modules ES, CSS et utilitaires ;
- `templates/index_new.html` : gabarit de la page principale ;
- `sound-design/` : sons du panneau.

Le serveur Rust ne modifie pas ces fichiers. `index_new.html` est rendu par
Minijinja après remplacement des appels `url_for('static', ...)` et du filtre
`tojson` de Flask. Pour synchroniser une évolution du frontend legacy, recopier
les fichiers puis relancer les tests de rendu du crate `workflow-server`.

## Écarts assumés avec le legacy

- `PollingManager` accepte `pauseWhenHidden` : `systemMonitor` et
  `localDownloadsStatus` sont suspendus quand `document.hidden` passe à vrai,
  puis rafraîchis au focus. Le legacy continue à poller en arrière-plan. Voir
  `docs/desktop/MVP-FRONTEND-POLLING.md`.
- `PollingManager.js` est importé avec `?v=20261025a` dans ses sept
  consommateurs : incrémenter ce suffixe à chaque modification du module pour
  éviter un ancien module dans le cache WebKit.
- L'auto-ouverture de l'overlay de logs est désactivée par défaut
  (`ui.autoOpenLogOverlay = false`), contrairement au legacy.
