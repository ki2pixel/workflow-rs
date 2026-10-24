# Timeline — design tokens et hooks

## 1. Variables CSS principales

```css
:root {
  --bg-dark: #1e1e2f;
  --bg-card: #2c2c3e;
  --bg-tertiary: #3a3a4e;
  --log-bg: #161625;

  --text-primary: #e0e0e0;
  --text-secondary: #a0a0b0;
  --text-muted: #707080;

  --border-color: #39394d;
  --border-bright: #4a4a5e;
  --topbar-height: 68px;

  --motion-duration-fast: 120ms;
  --motion-duration-medium: 220ms;
  --motion-duration-slow: 420ms;
  --motion-ease-standard: cubic-bezier(0.4, 0, 0.2, 1);

  --accent-primary: #7986cb;
  --accent-secondary: #ff8a65;
  --status-running: #4dabf7;
  --status-success: #4caf50;
  --status-error: #e53935;
  --status-warning: #ff9800;
  --status-idle: #9e9e9e;

  --pipeline-node-size: 80px;
  --pipeline-connector-width: 3px;
  --pipeline-gap: 2rem;
  --pipeline-card-radius: 20px;
  --pipeline-card-padding: 1.5rem;
  --pipeline-card-max-width: 980px;
  --pipeline-node-dot-size: 18px;
  --pipeline-node-border-width: 3px;
  --pipeline-rail-column-width: 110px;
  --pipeline-rail-gap: 1.25rem;
  --pipeline-axis-width: 4px;
}
```

## 2. Structure HTML

```html
<section class="workflow-pipeline">
  <div class="pipeline-timeline" role="list">
    <div class="timeline-axis" aria-hidden="true"></div>
    <div class="timeline-row">
      <div class="timeline-rail-column" aria-hidden="true">
        <div class="timeline-node" data-step="STEP1"></div>
      </div>
      <div class="timeline-cards-column">
        <div class="step timeline-step"
             id="step-STEP1"
             data-step-key="STEP1"
             data-step-name="..."
             data-status="idle"
             role="listitem"
             tabindex="0">
          <div class="timeline-content">
            <div class="timeline-head">
              <h2>...</h2>
              <span class="step-state-chip state-idle"
                    id="state-chip-STEP1"
                    aria-live="polite">Prêt</span>
            </div>
            <div class="step-controls">
              <button class="run-button" data-step="STEP1">Lancer</button>
            </div>
            <div class="step-progress-container" id="progress-container-STEP1">
              <div class="progress-bar-step"
                   id="progress-bar-STEP1"
                   role="progressbar"
                   aria-valuenow="0"
                   aria-valuemin="0"
                   aria-valuemax="100">0%</div>
              <div class="progress-text-step"
                   id="progress-text-STEP1"
                   aria-live="polite"></div>
            </div>
            <button class="specific-log-button"
                    data-step="STEP1"
                    data-log-index="0">Log Extraction</button>
          </div>
        </div>
      </div>
    </div>
  </div>
</section>
```

## 3. Hooks JS

| Hook | Contrat |
| --- | --- |
| `#step-STEPn` | Carte d'étape, ne pas renommer |
| `.timeline-step` | Sélecteur de carte |
| `[data-step-key]` | Clé d'étape |
| `[data-status]` | `idle`, `running`, `completed`, `failed`, `stopped` |
| `.run-button` | Lancement |
| `.specific-log-button` | Logs spécifiques |
| `.custom-sequence-checkbox` | Sélection de séquence |
| `#state-chip-STEPn` | Badge d'état |
| `#progress-bar-STEPn` | Barre de progression |
| `#progress-text-STEPn` | Texte de progression |
| `#global-progress-bar` | Progression globale |
| `#global-progress-text` | Texte global |
| `#logs-column-global` | Overlay de logs |

## 4. Auto-scroll

- `scrollManager.scrollToActiveStep(stepKey)`.
- `calculateOptimalScrollPosition(element)` tient compte de
  `--topbar-height`.
- Ne pas utiliser `scrollIntoView()` brut.
- Respecter `prefers-reduced-motion`.

## 5. Scénarios de test visuel

1. **Timeline vide** : 8 cartes, états `idle`, aucun log.
2. **Étape en cours** : badge `running`, barre de progression, auto-scroll.
3. **Étape terminée** : badge `completed`, progression 100 %, log final.
4. **Échec** : badge `failed`, texte d'erreur, log.
5. **Séquence** : cartes sélectionnées, ordre affiché, progression globale.
6. **Overlay de logs** : focus trap, fermeture, header contextuel.
7. **Mobile** : cartes empilées, boutons accessibles, pas de débordement.
8. **Reduced motion** : animations désactivées, auto-scroll instantané.

## 6. Règles

- Ne pas modifier les IDs/classes sans mettre à jour `uiUpdater.js`,
  `eventHandlers.js` et les tests.
- Ne pas injecter de HTML non échappé.
- Ne pas ajouter de `setInterval` : `PollingManager`.
- Ne pas modifier `assets/web` directement si le changement existe dans le
  legacy.
