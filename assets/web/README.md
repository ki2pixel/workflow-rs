# Frontend web

Copie du frontend natif de `workflow_mediapipe` :

- `static/` : modules ES, CSS et utilitaires ;
- `templates/index_new.html` : gabarit de la page principale ;
- `sound-design/` : sons du panneau.

Le serveur Rust ne modifie pas ces fichiers. `index_new.html` est rendu par
Minijinja après remplacement des appels `url_for('static', ...)` et du filtre
`tojson` de Flask. Pour synchroniser une évolution du frontend legacy, recopier
les fichiers puis relancer les tests de rendu du crate `workflow-server`.
