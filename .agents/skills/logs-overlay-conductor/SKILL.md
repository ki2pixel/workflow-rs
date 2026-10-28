---
name: logs-overlay-conductor
description: Maintenir l'overlay de logs de la copie frontend legacy (focus trap, auto-ouverture, header contextuel).
---

# Logs Overlay Conductor

## Portée

L'overlay de logs vit dans la copie frontend `assets/web` :

- HTML : `assets/web/templates/index_new.html` (`#logs-column-global`,
  header, boutons de logs spécifiques).
- CSS : `assets/web/static/css/` (layout, overlay, responsive).
- JS : `uiUpdater.js`, `eventHandlers.js`, `popupManager.js`,
  `sequenceManager.js`.
- État : `AppState` (`logPanel`, préférence d'auto-ouverture).

## Principes UX

1. Overlay plein écran ; `aria-hidden` est togglé par `popupManager`. Les
   popups de séquence portent `role="dialog"`/`aria-modal`, pas l'overlay de
   logs.
2. Focus trap complet et restauration du focus à la fermeture.
3. Header contextuel : étape active, statut, timer.
4. Boutons « logs spécifiques » regroupés et accessibles au clavier.
5. Auto-ouverture configurable via `AppState.getAutoOpenLogOverlay()`. La copie
   Rust la désactive par défaut (`ui.autoOpenLogOverlay = false`) ; le toggle
   Settings et la préférence persistée restent effectifs.
6. `prefers-reduced-motion` respecté.
7. Mobile : `95vw` × `90vh` sous 900 px (`logs.css`).

## Checklist d'implémentation

- [ ] `#logs-column-global` et `#main-log-output-panel` présents.
- [ ] `aria-hidden` mis à jour par `openPopupUI`/`closePopupUI`.
- [ ] `.specific-log-button` et `data-step` conservés.
- [ ] Focus trap géré par `popupManager`.
- [ ] Fermeture par Échap et bouton `#close-log-panel`.
- [ ] Restauration du focus.
- [ ] Header mis à jour via `AppState`/`uiUpdater`.
- [ ] Auto-ouverture désactivée par défaut, toggle et préférence persistée
      respectés.
- [ ] Styles via variables CSS, pas de couleurs en dur.
- [ ] Test de rendu serveur vert.

## Workflow de modification

1. Vérifier si le changement existe dans le dépôt Python legacy.
2. Si oui, recopier le fichier dans `assets/web`.
3. Si non, faire le changement dans `assets/web` et le documenter ; ne jamais
   écrire dans le dépôt Python.
4. Relancer `cargo test -p workflow-server --lib`.
5. Vérifier le rendu avec une config isolée.

## Diagnostics

| Symptôme | Piste |
| --- | --- |
| Overlay ne s'ouvre pas | `AppState.logPanel.isOpen`, `openLogPanelUI()` |
| Focus bloqué | `popupManager` focus trap, éléments focusables |
| Header vide | `uiUpdater` ne reçoit pas l'étape active |
| Auto-ouverture ignorée | Préférence `AppState` |
| Logs non mis à jour | `fetchAndDisplayLogsForPanel`, polling `apiService` |
| Étapes visibles derrière l'overlay | `workflowWrapper.logs-active`, `hideNonActiveSteps` |
| Mobile cassé | `95vw`/`90vh` de `logs.css` |
| Conflit de merge | Fichier modifié directement dans Rust |

## Règles

- Ne pas ajouter de logique Rust dans l'overlay.
- Ne pas casser les hooks `.specific-log-button`, `data-step`, `aria-*`.
- Ne pas utiliser `innerHTML` sans échappement.
- Ne pas remplacer `PollingManager` par un `setInterval`.
- Ne pas modifier `assets/web` si le changement peut être fait dans le legacy.

## Références

- `assets/web/README.md`
- `crates/workflow-server/src/http.rs`
- `docs/architecture/CONTRAT-ENVIRONNEMENT.md`
- `.agents/skills/frontend-timeline-designer/SKILL.md`
- `resources/overlay_focus_playbook.md`
