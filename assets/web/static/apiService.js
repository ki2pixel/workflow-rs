import { POLLING_INTERVAL, POLLING_INTERVAL_HIGH_FREQUENCY } from './constants.js';
import * as ui from './uiUpdater.js';
import * as dom from './domElements.js';
import { appState } from './state/AppState.js';
import { showNotification, sendBrowserNotification } from './utils.js';
import { soundEvents } from './soundManager.js';
import { pollingManager } from './utils/PollingManager.js';
import { errorHandler } from './utils/ErrorHandler.js';


/**
 * Fetch helper that toggles a loading state on a button during the request.
 * It adds data-loading="true" and disables the button while the fetch runs.
 * @param {string} url
 * @param {RequestInit} options
 * @param {HTMLElement|string|null} buttonElOrId - element or element id
 * @returns {Promise<any>} parsed JSON response (or throws on network/error)
 */
export async function fetchWithLoadingState(url, options = {}, buttonElOrId = null) {
    let btn = null;
    if (typeof buttonElOrId === 'string') {
        btn = document.getElementById(buttonElOrId);
    } else if (buttonElOrId && buttonElOrId.nodeType === 1) {
        btn = buttonElOrId;
    }

    // Extract the worker token from the meta tag if present
    const tokenEl = document.querySelector('meta[name="worker-token"]');
    const token = tokenEl ? tokenEl.getAttribute('content') : '';
    if (token) {
        options.headers = {
            ...options.headers,
            'X-Worker-Token': token
        };
    }

    // Inject CSRF token for mutative requests (POST, PUT, DELETE)
    const method = (options.method || 'GET').toUpperCase();
    if (['POST', 'PUT', 'DELETE'].includes(method)) {
        const csrfEl = document.querySelector('meta[name="csrf-token"]');
        const csrfToken = csrfEl ? csrfEl.getAttribute('content') : '';
        if (csrfToken) {
            options.headers = {
                ...options.headers,
                'X-CSRF-Token': csrfToken
            };
        }
    }

    try {
        if (btn) {
            btn.setAttribute('data-loading', 'true');
            btn.disabled = true;
        }
        const response = await fetch(url, options);
        const data = await response.json().catch(() => ({}));
        if (!response.ok) {
            throw new Error((data && data.message) || `Erreur HTTP ${response.status}`);
        }
        return data;
    } finally {
        if (btn) {
            btn.removeAttribute('data-loading');
            btn.disabled = false;
        }
    }
}

/**
 * Centralized function to handle UI updates and state changes when a step fails.
 * This avoids code duplication in runStepAPI and performPoll.
 * @param {string} stepKey - The key of the step that failed.
 * @param {Error} error - The error object.
 * @param {string} errorSource - A string indicating where the error occurred (e.g., 'Lancement', 'Polling').
 */
function handleStepFailure(stepKey, error, errorSource) {
    const errorMessage = error.message || 'Erreur inconnue.';
    console.error(`[API handleStepFailure] Erreur ${errorSource} pour ${stepKey}:`, error);

    if (errorMessage.includes('Étape inconnue')) {
        console.warn(`[API handleStepFailure] Étape '${stepKey}' n'est pas reconnue.`);
    }

    soundEvents.errorEvent();

    showNotification(`Erreur ${errorSource} ${stepKey}: ${errorMessage}`, 'error');

    const statusEl = document.getElementById(`status-${stepKey}`);
    if (statusEl) {
        statusEl.textContent = `Erreur: ${errorMessage.substring(0, 50)}`;
        statusEl.className = 'status-badge status-failed';
    }

    if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey) {
        ui.updateMainLogOutputUI(`<i>Erreur d'initiation: ${errorMessage}</i>`);
    }

    const runButton = document.querySelector(`.run-button[data-step="${stepKey}"]`);
    if (runButton) {
        runButton.disabled = !!appState.getStateProperty('isAnySequenceRunning');
    }
    const cancelButton = document.querySelector(`.cancel-button[data-step="${stepKey}"]`);
    if (cancelButton) {
        cancelButton.disabled = true;
    }

    ui.stopStepTimer(stepKey);
    const progressBar = document.getElementById(`progress-bar-${stepKey}`);
    if (progressBar) {
        progressBar.style.backgroundColor = 'var(--red)';
    }

    stopPollingAPI(stepKey);
}


export async function runStepAPI(stepKey) {
    // --- UI setup (unchanged) ---
    ui.resetStepTimerDisplay(stepKey);
    const statusEl = document.getElementById(`status-${stepKey}`);
    if(statusEl) { statusEl.textContent = 'Initiation...'; statusEl.className = 'status-badge status-initiated'; }
    const runButton = document.querySelector(`.run-button[data-step="${stepKey}"]`);
    if(runButton) runButton.disabled = true;
    const cancelButton = document.querySelector(`.cancel-button[data-step="${stepKey}"]`);
    if(cancelButton) cancelButton.disabled = false;

    if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey) {
        ui.updateMainLogOutputUI('<i>Initiation du processus...</i>');
    }

    try {
        const data = await fetchWithLoadingState(`/run/${stepKey}`, { method: 'POST' }, runButton);
        console.log(`[API runStepAPI] Réponse pour ${stepKey}:`, data);

        if (data.status === 'initiated') {
            const statusEl = document.getElementById(`status-${stepKey}`);
            if(statusEl) { statusEl.textContent = 'Lancé'; statusEl.className = 'status-badge status-starting'; }
            ui.startStepTimer(stepKey);
            console.log(`[API runStepAPI] Appel de startPollingAPI pour ${stepKey}`);
            startPollingAPI(stepKey);
            return true;
        } else {
            throw new Error(data.message || `Réponse invalide du serveur pour le lancement de ${stepKey}.`);
        }
    } catch (error) {
        handleStepFailure(stepKey, error, 'Lancement');
        return false;
    }
}

function appendItalicLineToMainLog(panelEl, message) {
    if (!panelEl) return;
    const br = document.createElement('br');
    const i = document.createElement('i');
    i.textContent = String(message ?? '');
    panelEl.appendChild(br);
    panelEl.appendChild(i);
}

export async function cancelStepAPI(stepKey) {
    if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey) {
        appendItalicLineToMainLog(dom.mainLogOutputPanel, 'Annulation en cours...');
    }

    try {
        const cancelUrl = `/cancel/${stepKey}`;
        const fullUrl = new URL(cancelUrl, globalThis.location.origin).href;
        console.log(`[CANCEL DEBUG] Attempting to cancel ${stepKey}:`);
        console.log(`  - Relative URL: ${cancelUrl}`);
        console.log(`  - Full URL: ${fullUrl}`);
        console.log(`  - Current origin: ${globalThis.location.origin}`);
        console.log(`  - Current port: ${globalThis.location.port}`);

        const cancelButton = document.querySelector(`.cancel-button[data-step="${stepKey}"]`);
        const data = await fetchWithLoadingState(cancelUrl, { method: 'POST' }, cancelButton);
        console.log(`[CANCEL DEBUG] Response received (ok):`, data);
        showNotification(data.message || "Annulation demandée", 'info');

        if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey) {
            appendItalicLineToMainLog(dom.mainLogOutputPanel, data.message || 'Annulation demandée');
        }
    } catch (error) {
        console.error(`Erreur annulation ${stepKey}:`, error);

        errorHandler.handleApiError(`cancel/${stepKey}`, error, { stepKey });

        if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey) {
            appendItalicLineToMainLog(dom.mainLogOutputPanel, `Erreur communication pour annulation: ${error.toString()}`);
        }
    }
}

export function startPollingAPI(stepKey, isAutoModeHighFrequency = false) {
    stopPollingAPI(stepKey);

    const pollingInterval = isAutoModeHighFrequency ? POLLING_INTERVAL_HIGH_FREQUENCY : POLLING_INTERVAL;
    console.log(`[API startPollingAPI] 🚀 Polling démarré pour ${stepKey}. Intervalle: ${pollingInterval}ms ${isAutoModeHighFrequency ? '(AutoMode high-frequency)' : '(normal)'}`);

    const performPoll = async () => {
        try {
            const pollStartTime = performance.now();
            console.log(`[API POLL] Fetching status for ${stepKey} at ${new Date().toISOString()}`);

            const response = await fetch(`/status/${stepKey}?t=${Date.now()}`);
            if (!response.ok) {
                console.warn(`[API performPoll] Erreur ${response.status} lors du polling pour ${stepKey}. Arrêt du polling.`);
                stopPollingAPI(stepKey);
                if (!appState.getStateProperty('isAnySequenceRunning')) {
                    handleStepFailure(stepKey, new Error(`Erreur statut (${response.status})`), 'Polling');
                }
                return;
            }
            const data = await response.json();

            const pollEndTime = performance.now();
            const statusEmoji = data.status === 'running' ? '🔄' : data.status === 'completed' ? '✅' : data.status === 'failed' ? '❌' : '⚪';
            console.log(`[API POLL RESPONSE] ${statusEmoji} ${stepKey} (${(pollEndTime - pollStartTime).toFixed(2)}ms): status="${data.status}", progress=${data.progress_current}/${data.progress_total}, return_code=${data.return_code}`);

            const previousStatus = appState.getStateProperty(`processInfo.${stepKey}.status`) || 'unknown';
            appState.setState({ processInfo: { [stepKey]: data } }, 'process_info_polled');

            if (typeof data.is_any_sequence_running === 'boolean') {
                appState.setState({ isAnySequenceRunning: data.is_any_sequence_running }, 'sequence_running_polled');
            }

            ui.updateStepCardUI(stepKey, data);

            const workflowWrapper = typeof dom.getWorkflowWrapper === 'function' ? dom.getWorkflowWrapper() : dom.workflowWrapper;
            if (appState.getStateProperty('activeStepKeyForLogsPanel') === stepKey && workflowWrapper && workflowWrapper.classList.contains('logs-active')) {
                ui.updateMainLogOutputUI(data.log.join(''));
            }
            const isTerminal = ['completed', 'failed'].includes(data.status);
            if (isTerminal && previousStatus !== data.status) {
                const title = data.status === 'completed' ? '✅ Étape terminée' : '❌ Étape en erreur';
                const body = `${stepKey} — statut: ${data.status}`;
                sendBrowserNotification(title, body).catch(() => {});
            }

            const shouldStopPolling = isTerminal ||
                                    (data.status === 'idle' && !appState.getStateProperty('autoModeEnabled'));

            if (shouldStopPolling) {
                console.log(`[API performPoll] Statut final '${data.status}' pour ${stepKey}. Arrêt du polling.`);
                stopPollingAPI(stepKey);
            } else if (data.status === 'idle' && appState.getStateProperty('autoModeEnabled')) {
                console.log(`[API performPoll] ⚪ ${stepKey} idle mais AutoMode actif - maintien du polling pour détecter les transitions`);
            }
        } catch (error) {
            console.error(`[API performPoll] Erreur CATCH polling ${stepKey}:`, error);
            stopPollingAPI(stepKey);
            if (!appState.getStateProperty('isAnySequenceRunning')) {
                handleStepFailure(stepKey, error, 'Polling');
            }
        }
    };

    const pollingId = pollingManager.startPolling(
        `step-${stepKey}`,
        performPoll,
        pollingInterval, // Use dynamic interval (200ms for AutoMode, 500ms for normal)
        { immediate: true, maxErrors: 3 }
    );
}

export function stopPollingAPI(stepKey) {
    pollingManager.stopPolling(`step-${stepKey}`);
    console.log(`[API stopPollingAPI] Polling arrêté pour ${stepKey}.`);
}

export async function fetchSpecificLogAPI(stepKey, logIndex, logName, buttonElOrId = null) {
    ui.updateSpecificLogUI(logName, null, "<i>Chargement...</i>");
    try {
        let data;
        const url = `/get_specific_log/${stepKey}/${logIndex}`;
        if (buttonElOrId) {
            data = await fetchWithLoadingState(url, {}, buttonElOrId);
        } else {
            const response = await fetch(url);
            data = await response.json();
            if (!response.ok) {
                throw new Error(data.error || `Erreur HTTP ${response.status}`);
            }
        }
        ui.updateSpecificLogUI(logName, data.path, data.content);
    } catch (error) {
        console.error(`Erreur fetch log spécifique ${stepKey}/${logIndex}:`, error);
        ui.updateSpecificLogUI(logName, null, '', true, `Erreur de communication: ${error.toString()}`);
    }
}


export async function fetchInitialStatusAPI(stepKey) {
    try {
        const response = await fetch(`/status/${stepKey}?t=${Date.now()}`);
        if (!response.ok) {
            console.warn(`Initial status fetch failed for ${stepKey}: ${response.status}. Using fallback.`);
            appState.setState({
                processInfo: {
                    [stepKey]: appState.getStateProperty(`processInfo.${stepKey}`) || {
                        status: 'idle', log: [], progress_current: 0, progress_total: 0, progress_text: '',
                        is_any_sequence_running: false
                    }
                }
            }, 'process_info_initial_fallback');
        } else {
            const data = await response.json();
            appState.setState({ processInfo: { [stepKey]: data } }, 'process_info_initial');

            if (typeof data.is_any_sequence_running === 'boolean') {
                appState.setState({ isAnySequenceRunning: data.is_any_sequence_running }, 'sequence_running_initial');
            }
        }

        const stepInfo = appState.getStateProperty(`processInfo.${stepKey}`) || {
            status: 'idle', log: [], progress_current: 0, progress_total: 0, progress_text: '',
            is_any_sequence_running: false
        };

        if (stepKey === 'clear_disk_cache') {
            ui.updateClearCacheGlobalButtonState(stepInfo.status);
        } else {
            ui.updateStepCardUI(stepKey, stepInfo);
        }
        
        if (['running', 'starting', 'initiated'].includes(stepInfo.status)) {
            console.log(`[API fetchInitialStatusAPI] Étape ${stepKey} en cours au démarrage. Lancement du polling.`);
            startPollingAPI(stepKey);
        }

    } catch (err) {
        console.error(`Erreur CATCH fetchInitialStatusAPI pour ${stepKey}:`, err);
        const fallbackData = appState.getStateProperty(`processInfo.${stepKey}`) || {
            status: 'idle', log: [], progress_current: 0, progress_total: 0, progress_text: '',
            is_any_sequence_running: false
        };
        if (stepKey === 'clear_disk_cache') {
            ui.updateClearCacheGlobalButtonState(fallbackData.status);
        } else {
            ui.updateStepCardUI(stepKey, fallbackData);
        }
    }
}

export async function fetchLocalDownloadsStatusAPI() {
    try {
        const response = await fetch('/api/csv_downloads_status');
        if (!response.ok) {
            console.warn(`Erreur lors de la récupération du statut des téléchargements CSV: ${response.status}`);
            return [];
        }
        return await response.json();
    } catch (error) {
        console.error("Erreur réseau fetchLocalDownloadsStatusAPI:", error);
        return [];
    }
}



export async function fetchCSVMonitorStatusAPI() {
    try {
        const response = await fetch('/api/csv_monitor_status');
        if (!response.ok) {
            throw new Error(`Erreur HTTP ${response.status}`);
        }
        return await response.json();
    } catch (error) {
        console.error("Erreur fetchCSVMonitorStatusAPI:", error);
        return {
            csv_monitor: { status: "error", last_check: null, error: "Impossible de récupérer le statut" },
            auto_mode_enabled: false,
            csv_url: "",
            check_interval: 15
        };
    }
}

