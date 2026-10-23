import * as dom from './domElements.js';
import * as ui from './uiUpdater.js';
import * as api from './apiService.js';
import { runStepSequence } from './sequenceManager.js';
import { defaultSequenceableStepsKeys } from './constants.js';
import { showNotification } from './utils.js';
import { openPopupUI, closePopupUI, showCustomSequenceConfirmUI } from './popupManager.js';
import { scrollToStepImmediate } from './scrollManager.js';
import { soundEvents } from './soundManager.js';
import { appState } from './state/AppState.js';
import { setAutoOpenLogOverlay, getAutoOpenLogOverlay } from './state.js';

function getIsAnySequenceRunning() {
    return !!appState.getStateProperty('isAnySequenceRunning');
}

function getSelectedStepsOrder() {
    return appState.getStateProperty('selectedStepsOrder') || [];
}

function setSelectedStepsOrder(order) {
    const safeOrder = Array.isArray(order) ? [...order] : [];
    appState.setState({ selectedStepsOrder: safeOrder }, 'selected_steps_order_update');
}

export function initializeEventHandlers() {
    const closeLogButton = dom.getCloseLogPanelButton();
    if (closeLogButton) {
        closeLogButton.addEventListener('click', ui.closeLogPanelUI);
    }

    const runButtons = dom.getAllRunButtons() || [];
    runButtons.forEach(button => {
        button.addEventListener('click', async () => {
            try {
                if (getIsAnySequenceRunning()) {
                    showNotification("Une séquence est déjà en cours. Veuillez attendre sa fin.", 'warning');
                    return;
                }
                const stepKey = button.dataset.step;
                ui.updateMainLogOutputUI('');
                const specificLogContainer = dom.getSpecificLogContainerPanel();
                if (specificLogContainer) specificLogContainer.style.display = 'none';
                if (getAutoOpenLogOverlay()) {
                    ui.openLogPanelUI(stepKey, true);
                }

                scrollToStepImmediate(stepKey, { scrollDelay: 0 });

                soundEvents.workflowStart();

                await api.runStepAPI(stepKey);
            } catch (error) {
                console.error('[EVENT] Error in run button handler:', error);
                showNotification("Erreur lors de l'exécution de l'étape", 'error');
            }
        });
    });

    const cancelButtons = dom.getAllCancelButtons() || [];
    cancelButtons.forEach(button => {
        button.addEventListener('click', async () => {
            try {
                const stepKey = button.dataset.step;
                await api.cancelStepAPI(stepKey);
            } catch (error) {
                console.error('[EVENT] Error in cancel button handler:', error);
                showNotification("Erreur lors de l'annulation de l'étape", 'error');
            }
        });
    });

    const logsAutoOpenToggle = dom.getLogsAutoOpenToggle();
    if (logsAutoOpenToggle) {
        logsAutoOpenToggle.checked = getAutoOpenLogOverlay();
        // Subscribe to stay in sync if updated elsewhere
        appState.subscribeToProperty('ui.autoOpenLogOverlay', (enabled) => {
            logsAutoOpenToggle.checked = !!enabled;
        });
        logsAutoOpenToggle.addEventListener('change', (event) => {
            const enabled = event.target.checked;
            setAutoOpenLogOverlay(enabled);
            console.log(`[EVENT] Auto log overlay ${enabled ? 'enabled' : 'disabled'} by user`);
        });
    }

    const specificLogButtons = dom.getAllSpecificLogButtons() || [];
    specificLogButtons.forEach(button => {
        button.addEventListener('click', async () => {
            const stepKey = button.dataset.step;
            const logIndex = button.dataset.logIndex;
            const workflowWrapper = dom.getWorkflowWrapper();

            if (!workflowWrapper || !workflowWrapper.classList.contains('logs-active') || appState.getStateProperty('activeStepKeyForLogsPanel') !== stepKey) {
                ui.openLogPanelUI(stepKey, true);
                try {
                    const statusResponse = await fetch(`/status/${stepKey}?t=${Date.now()}`);
                    if (!statusResponse.ok) throw new Error(`Erreur statut: ${statusResponse.status}`);
                    const statusData = await statusResponse.json();
                    ui.updateMainLogOutputUI(statusData.log ? statusData.log.join('') : '<i>Log principal non disponible.</i>');
                } catch (error) {
                    console.error("Erreur chargement statut pour log panel:", error);
                    ui.updateMainLogOutputUI(`<i>Erreur chargement log principal: ${error.message}</i>`);
                }
            }
            // Pass the clicked button to enable loading state (spinner + disabled)
            await api.fetchSpecificLogAPI(stepKey, logIndex, button.textContent.trim(), button);
        });
    });

    const runAllButton = dom.getRunAllButton();
    if (runAllButton) {
        runAllButton.addEventListener('click', async () => {
            if (getIsAnySequenceRunning()) {
                showNotification("Séquence déjà en cours.", 'warning'); return;
            }
            // Play workflow start sound for complete sequence 1-8
            soundEvents.workflowStart();
            await runStepSequence(defaultSequenceableStepsKeys, "Séquence 1-8");
        });
    }

    const customSequenceCheckboxes = dom.getCustomSequenceCheckboxes() || [];
    customSequenceCheckboxes.forEach(checkbox => {
        checkbox.addEventListener('change', (event) => {
            const stepKey = event.target.dataset.stepKey;
            const stepCard = document.getElementById(`step-${stepKey}`);
            const orderNumberEl = document.getElementById(`order-${stepKey}`);
            let currentOrder = getSelectedStepsOrder();

            // Play checkbox interaction sound
            soundEvents.checkboxInteraction();

            if (event.target.checked) {
                if (!currentOrder.includes(stepKey)) {
                    currentOrder.push(stepKey);
                    if (stepCard) stepCard.classList.add('custom-sequence-selected');
                }
            } else {
                const index = currentOrder.indexOf(stepKey);
                if (index > -1) currentOrder.splice(index, 1);
                if (stepCard) stepCard.classList.remove('custom-sequence-selected');
            }
            setSelectedStepsOrder(currentOrder);
            document.querySelectorAll('.step-selection-order-number').forEach(el => { el.textContent = ''; });
            getSelectedStepsOrder().forEach((sk, idx) => {
                const orderEl = document.getElementById(`order-${sk}`);
                if (orderEl) orderEl.textContent = idx + 1;
            });
            ui.updateCustomSequenceButtonsUI();
        });
    });

    const clearCustomSequenceButton = dom.getClearCustomSequenceButton();
    if (clearCustomSequenceButton) {
        clearCustomSequenceButton.addEventListener('click', () => {
            setSelectedStepsOrder([]);
            customSequenceCheckboxes.forEach(cb => {
                cb.checked = false;
                const stepCard = document.getElementById(`step-${cb.dataset.stepKey}`);
                if (stepCard) stepCard.classList.remove('custom-sequence-selected');
                const orderEl = document.getElementById(`order-${cb.dataset.stepKey}`);
                if (orderEl) orderEl.textContent = '';
            });
            ui.updateCustomSequenceButtonsUI();
        });
    }

    const runCustomSequenceButton = dom.getRunCustomSequenceButton();
    if (runCustomSequenceButton) {
        runCustomSequenceButton.addEventListener('click', () => {
            if (getSelectedStepsOrder().length === 0) {
                showNotification("Veuillez sélectionner au moins une étape.", 'warning');
                return;
            }
            if (getIsAnySequenceRunning()) {
                showNotification("Une autre séquence est déjà en cours.", 'warning'); return;
            }
            showCustomSequenceConfirmUI();
        });
    }

    const confirmRunCustomSequenceButton = dom.getConfirmRunCustomSequenceButton();
    const customSequenceConfirmOverlay = dom.getCustomSequenceConfirmPopupOverlay();
    if (confirmRunCustomSequenceButton) {
        confirmRunCustomSequenceButton.addEventListener('click', async () => {
            closePopupUI(customSequenceConfirmOverlay);
            if (getIsAnySequenceRunning()) {
                showNotification("Une autre séquence est déjà en cours.", 'warning'); return;
            }
            // Loading state on confirm button and disable run-custom while executing
            try {
                confirmRunCustomSequenceButton.setAttribute('data-loading', 'true');
                confirmRunCustomSequenceButton.disabled = true;
                if (runCustomSequenceButton) runCustomSequenceButton.disabled = true;

                // Play workflow start sound for custom sequence
                soundEvents.workflowStart();
                await runStepSequence(getSelectedStepsOrder(), "Séquence Personnalisée");
            } finally {
                confirmRunCustomSequenceButton.removeAttribute('data-loading');
                confirmRunCustomSequenceButton.disabled = false;
                if (runCustomSequenceButton) runCustomSequenceButton.disabled = getIsAnySequenceRunning();
            }
        });
    }

    const cancelRunCustomSequenceButton = dom.getCancelRunCustomSequenceButton();
    if (cancelRunCustomSequenceButton) {
        cancelRunCustomSequenceButton.addEventListener('click', () => {
            closePopupUI(customSequenceConfirmOverlay);
        });
    }
    const closeSummaryPopupButton = dom.getCloseSummaryPopupButton();
    const sequenceSummaryOverlay = dom.getSequenceSummaryPopupOverlay();
    if (closeSummaryPopupButton) {
        closeSummaryPopupButton.addEventListener('click', () => {
            closePopupUI(sequenceSummaryOverlay);
        });
    }

    // Restore custom sequence checkbox state from persisted order
    const initialOrder = getSelectedStepsOrder();
    if (initialOrder.length > 0) {
        const cbArray = Array.from(dom.getCustomSequenceCheckboxes() || []);
        initialOrder.forEach((stepKey, idx) => {
            const cb = cbArray.find(checkbox => checkbox.dataset.stepKey === stepKey);
            if (cb) {
                cb.checked = true;
                const stepCard = document.getElementById(`step-${stepKey}`);
                if (stepCard) stepCard.classList.add('custom-sequence-selected');
                const orderEl = document.getElementById(`order-${stepKey}`);
                if (orderEl) orderEl.textContent = idx + 1;
            }
        });
        ui.updateCustomSequenceButtonsUI();
    }

    if (dom.getSoundToggle()) {
        import('./soundManager.js').then(({ isSoundEnabled, setSoundEnabled }) => {
            const isEnabled = isSoundEnabled();
            dom.getSoundToggle().checked = isEnabled;
            if (dom.getSoundStatus()) {
                dom.getSoundStatus().textContent = isEnabled ? 'Activé' : 'Désactivé';
            }

            dom.getSoundToggle().addEventListener('change', (event) => {
                const enabled = event.target.checked;
                setSoundEnabled(enabled);
                if (dom.getSoundStatus()) {
                    dom.getSoundStatus().textContent = enabled ? 'Activé' : 'Désactivé';
                }
                console.log(`[EVENT] Sound effects ${enabled ? 'enabled' : 'disabled'} by user`);
            });
        });
    }
}