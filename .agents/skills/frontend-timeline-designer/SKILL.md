---
name: frontend-timeline-designer
description: Maintenir la copie du frontend legacy (timeline, AppState, DOMBatcher, Minijinja) en documentant les écarts assumés.
---

# Frontend Timeline Designer

## Portée

`assets/web` est une **copie** du frontend natif de `workflow_mediapipe` :

- `assets/web/templates/index_new.html` : page principale ;
- `assets/web/static/` : modules ES, CSS, utilitaires ;
- `assets/web/sound-design/` : sons du panneau.

Le serveur Rust ne modifie pas ces fichiers. Il les sert après un prétraitement
Minijinja (`preprocess_template` dans `crates/workflow-server/src/http.rs`) :

- `url_for('static', filename='...')` → `/static/...` ;
- `steps_config.items()` → `steps_config|items` ;
- `csrf_token()` → chaîne vide ;
- `worker_token if worker_token else ''` → `worker_token` ;
- `steps_config | tojson | safe` → `steps_config_json | safe`, sérialisé côté
  Rust.

Le contexte injecte aussi les cartes d'étapes et le jeton worker.

## Principe directeur

**Rester un miroir, avec des écarts explicites.** Le dépôt Python est en
lecture seule : une évolution UI est recopiée depuis le legacy si elle y existe
déjà, sinon elle est faite dans `assets/web` et documentée dans
`memory-bank/systemPatterns.md`. Le serveur Rust doit rester un miroir, sauf
adaptations assumées (par exemple l'auto-ouverture des logs désactivée par
défaut).

Si une adaptation Rust est indispensable (par exemple un chemin d'API ou une
préférence par défaut), la documenter et la tester, mais ne pas réécrire la
logique UI.

## Structure clé

| Fichier | Rôle |
| --- | --- |
| `templates/index_new.html` | Timeline, cartes d'étapes, overlay de logs |
| `static/uiUpdater.js` | Mise à jour de l'état et des logs |
| `static/scrollManager.js` | Auto-scroll de la timeline |
| `static/sequenceManager.js` | Séquence personnalisée |
| `static/state/AppState.js` | État immuable |
| `static/utils/DOMBatcher.js` | Mutations DOM batchées |
| `static/utils/DOMDiff.js` | Diff DOM |
| `static/utils/PollingManager.js` | Polling centralisé |
| `static/apiService.js` | Requêtes et jeton worker |
| `static/css/layout.css` | Layout général |
| `static/css/base.css` | Styles de base |

## Patterns à respecter

1. **Structure sémantique** : `.pipeline-timeline[role=list]`,
   `.timeline-step[role=listitem]`, `id="step-{{ step_key }}"`.
2. **AppState immuable** : `AppState.setState()`, abonnements.
3. **DOMBatcher** : toute mutation DOM passe par `scheduleUpdate()`.
4. **Échappement** : `DOMUpdateUtils.escapeHtml()` pour tout contenu dynamique.
5. **DOMDiff** : envelopper les listes dynamiques (`<li>`) dans un conteneur
   (`<ul>`) avant `morph`.
6. **PollingManager** : pas de `setInterval` isolé.
7. **Accessibilité** : `aria-live="polite"`, focus visible,
   `prefers-reduced-motion`.
8. **Auto-scroll** : `scrollToActiveStep()` et
   `calculateOptimalScrollPosition()`, pas de `scrollIntoView()` brut.

## Workflow de modification

1. Vérifier si le changement existe déjà dans le dépôt Python (lecture seule).
2. Si oui, recopier le fichier depuis le legacy.
3. Si non, faire le changement dans `assets/web` et le documenter dans
   `memory-bank/systemPatterns.md` ; ne jamais écrire dans le dépôt Python.
4. Relancer les tests de rendu du crate `workflow-server`.
5. Vérifier que `index_new.html` ne contient plus `url_for(` ni `| tojson`.
6. Vérifier que les IDs/classes consommés par le JS n'ont pas changé.

## Tests

```sh
cargo test -p workflow-server --lib
cargo test -p workflow-server index_renders_without_a_template_engine_specific_filter
cargo test -p workflow-server real_index_template_renders_with_existing_frontend_syntax
```

- Les tests de rendu vérifient le prétraitement Minijinja.
- Il n'y a pas de runner frontend Node dans le dépôt Rust ; ne pas en inventer.
- Pour un test visuel, lancer le serveur sur une config isolée et ouvrir
  `http://127.0.0.1:<port>/`.

## Pièges

| Symptôme | Cause |
| --- | --- |
| Page blanche | Template non prétraité ou erreur Minijinja |
| `url_for` dans le HTML | Remplacement non appliqué |
| `tojson` dans le HTML | Filtre Flask non remplacé |
| Timeline cassée | ID/classe modifié |
| Logs non mis à jour | Polling ou AppState désynchronisé |
| Auto-scroll incorrect | Topbar fixe non prise en compte |
| XSS | `innerHTML` sans échappement |
| Conflit de merge | Fichier modifié directement dans Rust |

## Règles

- Ne pas ajouter de dépendance npm au dépôt Rust.
- Ne pas modifier `assets/web` pour « corriger » un comportement backend.
- Ne pas casser les IDs `#step-STEPn`, `.step`, `.run-button`,
  `.specific-log-button`.
- Ne pas supprimer `data-log-type`, `aria-modal`, `role="dialog"`.
- Documenter toute divergence dans `memory-bank/systemPatterns.md`.

## Références

- `assets/web/README.md`
- `crates/workflow-server/src/http.rs`
- `docs/contracts/CONTRAT-ENVIRONNEMENT.md`
- `.agents/skills/logs-overlay-conductor/SKILL.md`
- `resources/timeline_design_tokens.md`
