# Overlay de logs — focus, auto-ouverture, diagnostics

## 1. Scénarios d'ouverture

| Scénario | Déclencheur | Comportement attendu |
| --- | --- | --- |
| Ouverture manuelle | Bouton « Logs » | Overlay ouverte, focus dans l'overlay |
| Auto-ouverture | Démarrage d'une étape | Ouverte si `getAutoOpenLogOverlay()` est vrai |
| Séquence | Changement d'étape | Header mis à jour ; l'ouverture dépend de la préférence |
| Fermeture | Échap ou bouton | Focus restauré sur le déclencheur |
| Erreur | Étape en échec | Log d'erreur visible, overlay stable |

## 2. Structure d'accessibilité

```html
<div class="logs-overlay-container popup-overlay" id="logs-column-global">
  <div class="logs-content-wrapper">
    <div class="log-panel-header">
      <div class="log-panel-header-main">
        <span id="log-panel-title">Logs</span>
        <button id="close-log-panel" aria-label="Fermer le panneau des logs">×</button>
      </div>
      <div class="log-panel-subheader" id="log-panel-subheader" aria-live="polite">
        <span id="log-panel-context-step"></span>
        <span id="log-panel-context-status"></span>
        <span id="log-panel-context-timer"></span>
      </div>
      <div class="log-panel-specific-buttons"
           id="log-panel-specific-buttons-container"
           aria-label="Logs spécifiques"></div>
    </div>
    <div class="log-container" id="main-log-container-panel">
      <div class="log-output" id="main-log-output-panel"
           aria-live="polite" aria-atomic="false"></div>
    </div>
  </div>
</div>
```

## 3. Focus trap

- `popupManager` gère l'ouverture, le trap et la restauration.
- Éléments focusables : boutons, liens, `[tabindex="0"]`.
- Échap ferme.
- Le focus ne doit pas sortir de l'overlay.
- L'ouverture ajoute `logs-active` au wrapper, ce qui replie la timeline.
- À la fermeture, le focus revient au bouton déclencheur.
- `aria-hidden` passe à `false`/`true` ; `data-visible="true"` pilote l'animation
  CSS.

## 4. Auto-ouverture

- Préférence stockée dans `AppState` et `localStorage`.
- `openLogPanelUI()` vérifie `getAutoOpenLogOverlay()`.
- Pendant une séquence, ne pas forcer l'ouverture si la préférence est false
  (`forceOpen` reste réservé aux boutons manuels).
- Le changement de préférence doit être immédiat.

## 5. Diagnostics

```sh
# Vérifier les hooks dans le template
rg -n 'logs-column-global|log-panel-context|main-log-output-panel|close-log-panel' \
  assets/web/templates/index_new.html

# Vérifier les handlers
rg -n 'openLogPanelUI|closeLogPanel|openPopupUI|Escape|getAutoOpenLogOverlay' \
  assets/web/static/
```

- Vérifier la console navigateur.
- Vérifier que le polling met à jour `AppState`.
- Vérifier que le header reçoit l'étape active.
- Vérifier que la timeline est repliée (`logs-active`) derrière l'overlay.

## 6. Tests

```sh
cargo test -p workflow-server --lib
```

- Le test de rendu vérifie que le template est prétraité.
- Un test manuel est nécessaire pour le focus trap et le rendu visuel.
- Documenter le scénario, la config et le résultat.

## 7. Pièges

| Piège | Correction |
| --- | --- |
| `aria-hidden="true"` laissé à l'ouverture | `openPopupUI` doit le passer à `false` |
| Focus qui reste derrière | Trap clavier de `popupManager` |
| Échap non géré | Handler `handlePopupKeydown` |
| Auto-ouverture forcée | Respecter `getAutoOpenLogOverlay()` |
| Header obsolète | Mettre à jour via `uiUpdater` |
| Double overlay | Vérifier `data-visible` et `isLogsPanelOpen()` |
| Timeline visible derrière | Classe `logs-active` sur le wrapper |
| Styles en dur | Utiliser les variables CSS |
