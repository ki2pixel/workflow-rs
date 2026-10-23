import { formatElapsedTime, showNotification } from './utils.js';
import * as dom from './domElements.js';
import { appState } from './state/AppState.js';
import { setActiveStepKeyForLogs as legacySetActiveStepKeyForLogs, getAutoOpenLogOverlay } from './state.js';
import { scrollToActiveStep, isAutoScrollEnabled } from './scrollManager.js';
import { pollingManager } from './utils/PollingManager.js';
import { parseAndStyleLogContent as _parseAndStyleLogContent } from './utils/logParserUtils.js';
import { startStepTimer, stopStepTimer, resetStepTimerDisplay, getStepTimer } from './timerManager.js';
export { startStepTimer, stopStepTimer, resetStepTimerDisplay, getStepTimer };
export { updateLocalDownloadsListUI, updateClearCacheGlobalButtonState } from './downloadsListManager.js';

const lastProgressTextByStep = {};

const _lastAutoCenterTsByStep = {};
const _AUTO_CENTER_THROTTLE_MS = 700;

import { soundEvents } from './soundManager.js';
import { domBatcher, DOMUpdateUtils } from './utils/DOMBatcher.js';
import { performanceOptimizer } from './utils/PerformanceOptimizer.js';
import { openPopupUI, closePopupUI } from './popupManager.js';
import { DOMDiff } from './utils/DOMDiff.js';
import { workerManager } from './utils/WorkerManager.js';

const STATUS_UI_MAP = {
    running: { label: 'En cours', badgeClass: 'status-running', chipClass: 'state-running', icon: '⏱️' },
    starting: { label: 'Préparation', badgeClass: 'status-running', chipClass: 'state-running', icon: '⚙️' },
    initiated: { label: 'Initialisation', badgeClass: 'status-running', chipClass: 'state-running', icon: '⚙️' },
    completed: { label: 'Terminé', badgeClass: 'status-completed', chipClass: 'state-success', icon: '✅' },
    success: { label: 'Terminé', badgeClass: 'status-success', chipClass: 'state-success', icon: '✅' },
    failed: { label: 'Échec', badgeClass: 'status-failed', chipClass: 'state-error', icon: '❌' },
    error: { label: 'Erreur', badgeClass: 'status-error', chipClass: 'state-error', icon: '⚠️' },
    cancelled: { label: 'Annulé', badgeClass: 'status-cancelled', chipClass: 'state-error', icon: '⛔' },
    warning: { label: 'Attention', badgeClass: 'status-warning', chipClass: 'state-warning', icon: '⚠️' },
    paused: { label: 'En pause', badgeClass: 'status-warning', chipClass: 'state-warning', icon: '⏸️' },
    idle: { label: 'Prêt', badgeClass: 'status-idle', chipClass: 'state-idle', icon: '🕒' },
    pending: { label: 'En attente', badgeClass: 'status-warning', chipClass: 'state-warning', icon: '⏳' }
};

let STEPS_CONFIG_FROM_SERVER = {};
export function setStepsConfig(config) {
    STEPS_CONFIG_FROM_SERVER = config;
}

function getWorkflowWrapperElement() {
    return typeof dom.getWorkflowWrapper === 'function' ? dom.getWorkflowWrapper() : dom.workflowWrapper;
}

function getLogsColumnElement() {
    return typeof dom.getLogsColumnGlobal === 'function' ? dom.getLogsColumnGlobal() : dom.logsColumnGlobal;
}

export function isLogsPanelOpen() {
    const logsColumn = getLogsColumnElement();
    if (!logsColumn) return false;

    if (typeof logsColumn.getAttribute === 'function') {
        const attrVisible = logsColumn.getAttribute('data-visible');
        if (attrVisible === 'true') {
            return true;
        }
    }

    if (logsColumn.dataset && logsColumn.dataset.visible === 'true') {
        return true;
    }

    if (logsColumn.style && typeof logsColumn.style.display === 'string') {
        return logsColumn.style.display !== 'none';
    }

    return false;
}

function resolveElement(getterFn, legacyValue = null) {
    if (typeof getterFn === 'function') {
        try {
            return getterFn();
        } catch (_) {
            return null;
        }
    }
    return null;
}

function formatProgressText(baseText, current, total) {
    const suffix = `(${current}/${total})`;
    if (!baseText || baseText.trim() === "") {
        return suffix;
    }
    const trimmed = baseText.trim();
    if (trimmed.endsWith(suffix) || trimmed.includes(suffix)) {
        return trimmed;
    }
    return `${trimmed} ${suffix}`;
}

function getIsAnySequenceRunning() {
    return !!appState.getStateProperty('isAnySequenceRunning');
}

function getActiveStepKeyForLogs() {
    return appState.getStateProperty('activeStepKeyForLogsPanel');
}

function setActiveStepKeyForLogs(stepKey) {
    if (typeof legacySetActiveStepKeyForLogs === 'function') {
        legacySetActiveStepKeyForLogs(stepKey);
    } else {
        appState.setState({ activeStepKeyForLogsPanel: stepKey }, 'setActiveStepKeyForLogs');
    }
}

function getSelectedStepsOrder() {
    return appState.getStateProperty('selectedStepsOrder') || [];
}

function getProcessInfo(stepKey) {
    if (!stepKey) return null;
    return appState.getStateProperty(`processInfo.${stepKey}`) || null;
}

function setProcessInfo(stepKey, info) {
    if (!stepKey) return;
    appState.setState({ processInfo: { [stepKey]: info } }, 'process_info_update');
}



function hideNonActiveSteps(activeStepKey, hidden) {
    try {
        const stepDivs = dom.getAllStepDivs();
        stepDivs.forEach(el => {
            const isActive = activeStepKey && el.id === `step-${activeStepKey}`;
            if (!isActive && hidden) {
                el.classList.add('steps-hidden');
            } else if (isActive && hidden) {
                el.classList.remove('steps-hidden');
            } else if (!hidden) {
                el.classList.remove('steps-hidden');
            }
        });
    } catch (e) {
        console.warn('[UI] hideNonActiveSteps error', e);
    }
}


export function getStepsConfig() {
    return STEPS_CONFIG_FROM_SERVER;
}

function normalizeStatus(status) {
    return typeof status === 'string' ? status.toLowerCase() : 'idle';
}

function getStatusMeta(status) {
    const normalized = normalizeStatus(status);
    return STATUS_UI_MAP[normalized] || STATUS_UI_MAP.idle;
}

function getStepDisplayNameForLogPanel(stepKey) {
    if (!stepKey) return '';
    const config = getStepsConfig();
    const stepConfig = config ? config[stepKey] : null;
    if (stepConfig && stepConfig.display_name) return stepConfig.display_name;

    const stepEl = document.getElementById(`step-${stepKey}`);
    const datasetName = stepEl && stepEl.dataset ? stepEl.dataset.stepName : null;
    if (datasetName) return datasetName;

    return stepKey.replace(/_/g, ' ').replace(/\b\w/g, l => l.toUpperCase());
}

function updateLogPanelContextUI(stepKey) {
    const displayName = stepKey ? getStepDisplayNameForLogPanel(stepKey) : '';

    const statusEl = stepKey ? document.getElementById(`status-${stepKey}`) : null;
    const timerEl = stepKey ? document.getElementById(`timer-${stepKey}`) : null;

    const contextStepEl = resolveElement(dom.getLogPanelContextStep, dom.logPanelContextStep);
    const contextStatusEl = resolveElement(dom.getLogPanelContextStatus, dom.logPanelContextStatus);
    const contextTimerEl = resolveElement(dom.getLogPanelContextTimer, dom.logPanelContextTimer);

    if (contextStepEl) {
        contextStepEl.textContent = stepKey ? displayName : 'Aucune étape active';
    }
    if (contextStatusEl) {
        contextStatusEl.textContent = statusEl ? (statusEl.textContent || '') : '';
    }
    if (contextTimerEl) {
        contextTimerEl.textContent = timerEl ? (timerEl.textContent || '') : '';
    }
}

function clearLogPanelSpecificButtons() {
    const container = resolveElement(dom.getLogPanelSpecificButtonsContainer, dom.logPanelSpecificButtonsContainer);
    if (!container) return;

    while (container.firstChild) {
        container.removeChild(container.firstChild);
    }
}

function updateStepStateChip(stepKey, status) {
    const chip = document.getElementById(`state-chip-${stepKey}`);
    if (!chip) return;
    const meta = getStatusMeta(status);
    chip.className = `step-state-chip ${meta.chipClass}`;
    chip.textContent = `${meta.icon} ${meta.label}`;
}



export function updateGlobalUIForSequenceState(isRunning) {
    const runAllButton = resolveElement(dom.getRunAllButton, dom.runAllButton);
    const runCustomSequenceButton = resolveElement(dom.getRunCustomSequenceButton, null);
    const clearCustomSequenceButton = resolveElement(dom.getClearCustomSequenceButton, null);
    const customSequenceCheckboxes = resolveElement(dom.getCustomSequenceCheckboxes, dom.customSequenceCheckboxes) || [];

    if (runAllButton) runAllButton.disabled = isRunning;
    if (runCustomSequenceButton) runCustomSequenceButton.disabled = isRunning || getSelectedStepsOrder().length === 0;
    if (clearCustomSequenceButton) clearCustomSequenceButton.disabled = isRunning || getSelectedStepsOrder().length === 0;

    customSequenceCheckboxes.forEach(cb => cb.disabled = isRunning);

    Object.keys(STEPS_CONFIG_FROM_SERVER).forEach(stepKeyConfig => {
        const runButton = document.querySelector(`.run-button[data-step="${stepKeyConfig}"]`);
        const cancelButton = document.querySelector(`.cancel-button[data-step="${stepKeyConfig}"]`);
        const stepInfo = getProcessInfo(stepKeyConfig);

        if (runButton) runButton.disabled = isRunning;

        if (cancelButton) {
            if (stepInfo && ['running', 'starting', 'initiated'].includes(stepInfo.status)) {
                cancelButton.disabled = false;
            } else {
                cancelButton.disabled = true;
            }
        }
    });
}

export function setActiveStepForLogPanelUI(stepKey) {
    console.log(`[UI] setActiveStepForLogPanelUI, new active step for logs: ${stepKey}`);
    setActiveStepKeyForLogs(stepKey);

    const allStepDivs = dom.getAllStepDivs();
    allStepDivs.forEach(s => {
        s.classList.remove('active-for-log-panel');
    });
    if (stepKey && stepKey !== 'clear_disk_cache') {
        const activeStepElement = document.getElementById(`step-${stepKey}`);
        if (activeStepElement) {
            activeStepElement.classList.add('active-for-log-panel');

            const logsOpen = isLogsPanelOpen();
            if (logsOpen) {
                hideNonActiveSteps(stepKey, true);
            }
            if (isAutoScrollEnabled() && !logsOpen) {
                console.log(`[UI] Auto-scrolling to active step: ${stepKey}`);
                scrollToActiveStep(stepKey);
            }
        }
    }

    clearLogPanelSpecificButtons();

    if (stepKey) {
        const config = getStepsConfig();
        const stepConfig = config ? config[stepKey] : null;
        const displayName = stepConfig ? stepConfig.display_name : stepKey.replace(/_/g, ' ').replace(/\b\w/g, l => l.toUpperCase());
        console.log(`[UI] setActiveStepForLogPanelUI, displayName for logs: ${displayName}`);

        const logPanelTitle = resolveElement(dom.getLogPanelTitle, dom.logPanelTitle);
        const currentStepLogName = resolveElement(dom.getCurrentStepLogNamePanel, dom.currentStepLogNamePanel);
        if(logPanelTitle) logPanelTitle.textContent = `Logs: ${displayName}`;
        if(currentStepLogName) currentStepLogName.textContent = displayName;
        updateLogPanelContextUI(stepKey);

        const buttonsContainer = resolveElement(dom.getLogPanelSpecificButtonsContainer, dom.logPanelSpecificButtonsContainer);
        if (stepConfig && stepConfig.specific_logs && stepConfig.specific_logs.length > 0 && buttonsContainer) {
            stepConfig.specific_logs.forEach((logConf, index) => {
                const button = document.createElement('button');
                button.className = 'specific-log-button';
                button.textContent = logConf.name;
                button.dataset.step = stepKey;
                button.dataset.logIndex = index;
                button.addEventListener('click', async () => {
                    const apiModule = await import('./apiService.js');
                    await apiModule.fetchSpecificLogAPI(stepKey, index, logConf.name);
                });
                buttonsContainer.appendChild(button);
            });
        }
    } else {
        const logPanelTitle = resolveElement(dom.getLogPanelTitle, dom.logPanelTitle);
        const currentStepLogName = resolveElement(dom.getCurrentStepLogNamePanel, dom.currentStepLogNamePanel);
        if(logPanelTitle) logPanelTitle.textContent = "Logs";
        if(currentStepLogName) currentStepLogName.textContent = "Aucune étape active";
        updateLogPanelContextUI(null);
    }
}

async function fetchAndDisplayLogsForPanel(stepKeyToFocus) {
    console.log(`[UI] fetchAndDisplayLogsForPanel called for: ${stepKeyToFocus}. Current active log panel: ${getActiveStepKeyForLogs()}`);
    if (!stepKeyToFocus) return;

    const stepConfig = getStepsConfig()[stepKeyToFocus];
    const displayName = stepConfig ? (stepConfig.display_name || stepKeyToFocus) : stepKeyToFocus;

    const mainLogOutputPanel = resolveElement(dom.getMainLogOutputPanel, dom.mainLogOutputPanel);
    const mainLogContainer = resolveElement(dom.getMainLogContainerPanel, dom.mainLogContainerPanel);
    const specificLogContainer = resolveElement(dom.getSpecificLogContainerPanel, dom.specificLogContainerPanel);

    if (mainLogOutputPanel) {
        mainLogOutputPanel.textContent = `Chargement des logs pour ${displayName}...`;
    }

    if(mainLogContainer) mainLogContainer.style.display = 'flex';
    if(specificLogContainer) specificLogContainer.style.display = 'none';

    try {
        const response = await fetch(`/status/${stepKeyToFocus}?t=${Date.now()}`);
        if (!response.ok) {
            console.error(`[UI] fetchAndDisplayLogsForPanel - fetch failed for ${stepKeyToFocus}: ${response.status}`);
            throw new Error(`Erreur ${response.status} lors de la récupération des logs pour ${displayName}`);
        }
        const data = await response.json();
        setProcessInfo(stepKeyToFocus, { ...(getProcessInfo(stepKeyToFocus) || {}), ...data });
        console.log(`[UI] fetchAndDisplayLogsForPanel - response for: ${stepKeyToFocus}, Log content length: ${data.log ? data.log.length : 'N/A'}`);

        if (getActiveStepKeyForLogs() === stepKeyToFocus && mainLogOutputPanel) {
            console.log(`[UI] fetchAndDisplayLogsForPanel - Updating main log for ${stepKeyToFocus} with ${data.log ? data.log.length : 0} lines.`);
            updateMainLogOutputUI(data.log.join(''));
        } else {
            console.log(`[UI] fetchAndDisplayLogsForPanel - Log focus changed. Current: ${getActiveStepKeyForLogs()}, Fetched for: ${stepKeyToFocus}. Not updating main log panel.`);
        }
    } catch (error) {
        console.error(`[UI] fetchAndDisplayLogsForPanel - CATCH error for ${stepKeyToFocus}:`, error);
        const logPanel = resolveElement(dom.getMainLogOutputPanel, dom.mainLogOutputPanel);
        if (getActiveStepKeyForLogs() === stepKeyToFocus && logPanel) {
            logPanel.textContent = `Erreur: ${error?.message || 'Erreur inconnue'}`;
        }
    }
}

export function openLogPanelUI(stepKeyToFocus, forceOpen = false) {
    const logsColumn = getLogsColumnElement();
    if (!logsColumn) {
        console.warn('[UI] openLogPanelUI aborted: logs overlay missing.');
        return;
    }

    const workflowWrapper = getWorkflowWrapperElement();

    const currentActiveLogStep = getActiveStepKeyForLogs();
    const isPanelOpen = isLogsPanelOpen();
    const shouldAutoOpen = forceOpen || getAutoOpenLogOverlay();
    console.log(`[UI] openLogPanelUI called for: ${stepKeyToFocus}, forceOpen: ${forceOpen}, currentActive: ${currentActiveLogStep}, isPanelOpen: ${isPanelOpen}`);

    if (shouldAutoOpen) {
        console.log(`[UI] Forcing panel open/update for ${stepKeyToFocus}`);
        setActiveStepForLogPanelUI(stepKeyToFocus);
        hideNonActiveSteps(stepKeyToFocus, true);
        if (workflowWrapper) {
            workflowWrapper.classList.add('logs-active');
        }
        openPopupUI(logsColumn);
        fetchAndDisplayLogsForPanel(stepKeyToFocus);
        return;
    }

    // Auto-open disabled: uniquement stocker l'étape active pour une ouverture manuelle ultérieure
    setActiveStepForLogPanelUI(stepKeyToFocus);
}

export function closeLogPanelUI() {
    const logsColumn = getLogsColumnElement();
    const workflowWrapper = getWorkflowWrapperElement();
    if (!logsColumn) {
        console.warn('[CLOSE_LOG] Logs overlay missing; aborting close sequence.');
        setActiveStepForLogPanelUI(null);
        const mainLogOutputPanel = resolveElement(dom.getMainLogOutputPanel, dom.mainLogOutputPanel);
        const specificLogContainer = resolveElement(dom.getSpecificLogContainerPanel, dom.specificLogContainerPanel);
        if (mainLogOutputPanel) mainLogOutputPanel.textContent = "";
        if (specificLogContainer) specificLogContainer.style.display = 'none';
        clearLogPanelSpecificButtons();
        return;
    }

    console.log('[CLOSE_LOG] Closing logs overlay.');
    closePopupUI(logsColumn);
    hideNonActiveSteps(null, false);
    if (workflowWrapper) {
        workflowWrapper.classList.remove('logs-active');
    }
    setActiveStepForLogPanelUI(null);

    const mainLogOutputPanel = resolveElement(dom.getMainLogOutputPanel, dom.mainLogOutputPanel);
    const specificLogContainer = resolveElement(dom.getSpecificLogContainerPanel, dom.specificLogContainerPanel);
    if (mainLogOutputPanel) mainLogOutputPanel.textContent = "";
    if (specificLogContainer) specificLogContainer.style.display = 'none';
    clearLogPanelSpecificButtons();
}

export function updateStepCardUI(stepKey, data) {
    console.group(`[PROGRESS DEBUG] updateStepCardUI - ${stepKey}`);
    console.log('Raw data received:', {
        progress_current: data.progress_current,
        progress_total: data.progress_total,
        progress_current_fractional: data.progress_current_fractional,
        status: data.status,
        progress_text: data.progress_text,
        timestamp: new Date().toISOString()
    });

    performanceOptimizer.measureDomUpdate(`updateStepCard-${stepKey}`, () => {
        try {
            const statusEl = document.getElementById(`status-${stepKey}`);
            const runButton = document.querySelector(`.run-button[data-step="${stepKey}"]`);
            const cancelButton = document.querySelector(`.cancel-button[data-step="${stepKey}"]`);
            const workflowWrapper = getWorkflowWrapperElement();

            const normalizedStatus = normalizeStatus(data.status || 'idle');
            const statusMeta = getStatusMeta(normalizedStatus);

            if (statusEl) {
                statusEl.textContent = statusMeta.label;
                statusEl.className = `status-badge ${statusMeta.badgeClass}`;
            }

            const stepCardEl = document.getElementById(`step-${stepKey}`);
            if (stepCardEl) {
                stepCardEl.setAttribute('data-status', normalizedStatus);
            }

            updateStepStateChip(stepKey, normalizedStatus);

            if (runButton && cancelButton) {
                const isCurrentlyRunningOrStarting = ['running', 'starting', 'initiated'].includes(normalizedStatus);
                runButton.disabled = isCurrentlyRunningOrStarting || getIsAnySequenceRunning();
                cancelButton.disabled = !isCurrentlyRunningOrStarting;
            }

            const logsOpen = isLogsPanelOpen();
            if (logsOpen && ['running', 'starting', 'initiated'].includes(normalizedStatus)) {
                if (getActiveStepKeyForLogs() !== stepKey) {
                    setActiveStepForLogPanelUI(stepKey);
                    hideNonActiveSteps(stepKey, true);
                }
            }

            if (logsOpen && getActiveStepKeyForLogs() === stepKey) {
                updateLogPanelContextUI(stepKey);
            }

            if (['completed', 'failed'].includes(normalizedStatus) || (normalizedStatus === 'idle' && getStepTimer(stepKey))) {
                stopStepTimer(stepKey);
            } else if (normalizedStatus === 'idle' && !getStepTimer(stepKey)) {
                resetStepTimerDisplay(stepKey);
            } else if (['running', 'starting', 'initiated'].includes(normalizedStatus) && !getStepTimer(stepKey)?.intervalId) {
                // TODO: Implement proper timer resumption after page reload
                // Date: 2026-01-19
                // Owner: kidpixel
                // Issue: startStepTimer doesn't resume from existing startTime
                // Solution needed: Backend should provide duration_str for running steps
            }

            const progressContainer = document.getElementById(`progress-container-${stepKey}`);
            const progressBar = document.getElementById(`progress-bar-${stepKey}`);
            const progressTextEl = document.getElementById(`progress-text-${stepKey}`);

            let percentage = 0;

            if (progressContainer && progressBar && progressTextEl) {
                let currentProgress = 0;
                if (data.progress_total > 0) {
                    currentProgress = data.progress_current_fractional || data.progress_current;

                    if (data.progress_current_fractional === null && data.progress_text) {
                        const isSpecialRunning = (['STEP2','STEP3','STEP4'].includes(stepKey)) && ['running','starting','initiated'].includes(normalizedStatus);
                        if (!isSpecialRunning) {
                            const percentMatch = data.progress_text.match(/(\d+)%/);
                            if (percentMatch) {
                                const textPercent = parseInt(percentMatch[1]);
                                currentProgress = (textPercent / 100) * data.progress_total;
                                console.log(`[PROGRESS FALLBACK] ${stepKey}: Extracted ${textPercent}% from text, using fractional: ${currentProgress}`);
                            }
                        }
                    }

                    percentage = Math.round((currentProgress / data.progress_total) * 100);
                    percentage = Math.min(percentage, 100);

                    if ((['STEP2','STEP3','STEP4'].includes(stepKey)) && ['running', 'starting', 'initiated'].includes(normalizedStatus)) {
                        if (percentage >= 100) {
                            percentage = 99;
                        }
                        if (data.progress_total > 0 && data.progress_current === data.progress_total) {
                            percentage = Math.min(percentage, 99);
                        }
                    }
                }

                if (data.status === 'completed') {
                    progressContainer.style.display = 'block';
                    progressBar.style.backgroundColor = 'var(--green)';
                    progressBar.removeAttribute('data-active');

                    if (data.progress_total === 0) {
                        let noWorkText = "Aucun élément à traiter";
                        if (data.progress_text && data.progress_text.trim() !== "") {
                            noWorkText = data.progress_text;
                        }
                        progressTextEl.textContent = noWorkText;
                        progressBar.style.width = '10%';
                        progressBar.textContent = '✓';
                        progressBar.setAttribute('aria-valuenow', 0);
                    } else {
                        let baseCompletionText = formatProgressText(data.progress_text || "Terminé", data.progress_current, data.progress_total);
                        const config = STEPS_CONFIG_FROM_SERVER[stepKey];
                        if (config && config.post_completion_message_ui) {
                            progressTextEl.textContent = `${baseCompletionText}\n${config.post_completion_message_ui}`;
                        } else {
                            progressTextEl.textContent = baseCompletionText;
                        }

                        progressBar.style.width = '100%';
                        progressBar.textContent = '100%';
                        progressBar.setAttribute('aria-valuenow', 100);

                        if (['STEP3','STEP4','STEP5'].includes(stepKey)) {
                            const stepNames = { STEP3: 'Étape 3 — Transitions', STEP4: 'Étape 4 — Audio', STEP5: 'Étape 5 — Tracking' };
                            try { updateGlobalProgressUI(`${stepNames[stepKey] || stepKey}: Terminé`, 100, false); } catch (_) {}
                        }
                        delete lastProgressTextByStep[stepKey];
                    }
                } else if (data.status === 'failed') {
                    progressContainer.style.display = 'block';
                    progressBar.style.backgroundColor = 'var(--red)';
                    let failureText = `Échec`;
                    if (data.progress_total > 0) {
                        failureText = formatProgressText(data.progress_text ? `Échec: ${data.progress_text}` : `Échec`, data.progress_current, data.progress_total);
                        progressBar.style.width = `${percentage}%`;
                        progressBar.textContent = `${percentage}%`;
                        progressBar.setAttribute('aria-valuenow', percentage);
                    } else {
                        progressBar.style.width = '100%';
                        progressBar.textContent = '✗';
                        progressBar.setAttribute('aria-valuenow', 0);
                        if (data.progress_text) failureText += `: ${data.progress_text}`;
                    }
                    progressTextEl.textContent = failureText;
                    progressBar.removeAttribute('data-active');
                    progressTextEl.removeAttribute('data-processing');

                    if (['STEP3','STEP4','STEP5'].includes(stepKey)) {
                        const stepNames = { STEP3: 'Étape 3 — Transitions', STEP4: 'Étape 4 — Audio', STEP5: 'Étape 5 — Tracking' };
                        try { updateGlobalProgressUI(`${stepNames[stepKey] || stepKey}: ${failureText}`, percentage, true); } catch (_) {}
                    }
                    delete lastProgressTextByStep[stepKey];
                } else if (data.progress_total > 0 && ['running', 'starting', 'initiated'].includes(normalizedStatus)) {
                    console.log(`[PROGRESS CALC] ${stepKey}:`, {
                        progress_current: data.progress_current,
                        progress_current_fractional: data.progress_current_fractional,
                        progress_total: data.progress_total,
                        currentProgress: currentProgress,
                        calculatedPercentage: (currentProgress / data.progress_total) * 100,
                        finalPercentage: percentage,
                        status: data.status,
                        progress_text: data.progress_text
                    });

                    let displayCurrent = data.progress_current;
                    if ((!displayCurrent || displayCurrent === 0) && typeof data.progress_current_fractional === 'number' && data.progress_current_fractional > 0) {
                        const frac = Math.max(0, Math.min(data.progress_total, data.progress_current_fractional));
                        displayCurrent = Math.min(data.progress_total, Math.floor(frac) + 1);
                    }

                    progressContainer.style.display = 'block';
                    progressBar.style.backgroundColor = 'var(--blue)';
                    progressBar.style.width = `${percentage}%`;
                    progressBar.textContent = `${percentage}%`;
                    progressBar.setAttribute('aria-valuenow', percentage);
                    progressBar.setAttribute('data-active', 'true');

                    const candidateText = (data.progress_text && data.progress_text.trim()) ? data.progress_text : (lastProgressTextByStep[stepKey] || '');
                    if (data.progress_text && data.progress_text.trim()) {
                        lastProgressTextByStep[stepKey] = data.progress_text;
                    }
                    const subText = formatProgressText(candidateText, displayCurrent, data.progress_total);
                    progressTextEl.textContent = subText;

                    const shouldAutoCenter = getIsAnySequenceRunning() && ['running', 'starting', 'initiated'].includes(normalizedStatus);
                    if (shouldAutoCenter) {
                        const logsOpenNow = isLogsPanelOpen();
                        if (!logsOpenNow) {
                            const now = performance.now();
                            const lastTs = _lastAutoCenterTsByStep[stepKey] || 0;
                            if ((now - lastTs) > _AUTO_CENTER_THROTTLE_MS) {
                                _lastAutoCenterTsByStep[stepKey] = now;
                                requestAnimationFrame(() => {
                                    scrollToActiveStep(stepKey, { behavior: 'auto', scrollDelay: 0 });
                                });
                            }
                        }
                    }

                    if (candidateText && ['running','starting','initiated'].includes(data.status)) {
                        progressTextEl.setAttribute('data-processing', 'true');
                    } else {
                        progressTextEl.removeAttribute('data-processing');
                    }

                    if (['STEP3','STEP4','STEP5'].includes(stepKey)) {
                        const stepNames = { STEP3: 'Étape 3 — Transitions', STEP4: 'Étape 4 — Audio', STEP5: 'Étape 5 — Tracking' };
                        try { updateGlobalProgressUI(`${stepNames[stepKey] || stepKey}: ${subText}`, percentage, false); } catch (_) {}
                    }
                } else if (['running', 'starting', 'initiated'].includes(data.status)) {
                    progressContainer.style.display = 'block';
                    progressBar.style.backgroundColor = 'var(--blue)';
                    progressBar.setAttribute('data-active', 'true');
                    progressBar.style.width = '0%';
                    progressBar.textContent = '0%';
                    progressBar.setAttribute('aria-valuenow', 0);

                    const defaultText = (data.status === 'starting' || data.status === 'initiated') ? "Démarrage..." : "En cours d'exécution...";
                    const runningText = (data.progress_text && data.progress_text.trim()) ? data.progress_text : (lastProgressTextByStep[stepKey] || defaultText);
                    if (data.progress_text && data.progress_text.trim()) lastProgressTextByStep[stepKey] = data.progress_text;
                    progressTextEl.textContent = runningText;

                    if (['STEP3','STEP4','STEP5'].includes(stepKey)) {
                        const stepNames = { STEP3: 'Étape 3 — Transitions', STEP4: 'Étape 4 — Audio', STEP5: 'Étape 5 — Tracking' };
                        const globalText = `${stepNames[stepKey] || stepKey}: ${runningText || 'En cours...'}`;
                        try { updateGlobalProgressUI(globalText, 0, false); } catch (_) {}
                    }

                    if (runningText && runningText.trim()) {
                        progressTextEl.setAttribute('data-processing', 'true');
                    } else {
                        progressTextEl.removeAttribute('data-processing');
                    }
                } else {
                    progressContainer.style.display = 'none';
                    progressBar.setAttribute('aria-valuenow', 0);
                }
            }

            const anyRunning = !!document.querySelector('.step[data-status="running"], .step[data-status="starting"], .step[data-status="initiated"]');
            if (workflowWrapper) {
                if (anyRunning) {
                    workflowWrapper.classList.add('any-step-running');
                    if (['running','starting','initiated'].includes(data.status)) {
                        workflowWrapper.setAttribute('data-active-step', stepKey);
                    } else if (!document.querySelector(`.step[data-status="running"], .step[data-status="starting"], .step[data-status="initiated"]`)) {
                        workflowWrapper.removeAttribute('data-active-step');
                    }
                } else {
                    workflowWrapper.classList.remove('any-step-running');
                    workflowWrapper.removeAttribute('data-active-step');
                }
            }
        } catch (err) {
            console.error('[updateStepCardUI ERROR]', err);
        }

        console.groupEnd();
    });
}

export function updateCustomSequenceButtonsUI() {
    const hasSelection = getSelectedStepsOrder().length > 0;
    const isRunning = getIsAnySequenceRunning();
    const shouldDisable = !hasSelection || isRunning;

    domBatcher.scheduleUpdate('custom-sequence-buttons', () => {
        const runCustomSequenceButton = resolveElement(dom.getRunCustomSequenceButton, null);
        const clearCustomSequenceButton = resolveElement(dom.getClearCustomSequenceButton, null);

        if (runCustomSequenceButton) {
            runCustomSequenceButton.disabled = shouldDisable;
        }
        if (clearCustomSequenceButton) {
            clearCustomSequenceButton.disabled = shouldDisable;
        }
    });
}

export function updateGlobalProgressUI(text, percentage, isError = false) {
    if(dom.globalProgressAffix) dom.globalProgressAffix.style.display = 'flex';
    if(dom.globalProgressContainer) dom.globalProgressContainer.style.display = 'block';
    if(dom.globalProgressText) {
        dom.globalProgressText.style.display = 'block';
        dom.globalProgressText.textContent = text;
        dom.globalProgressText.style.color = isError ? 'var(--red)' : 'var(--text-secondary)';
    }
    if(dom.globalProgressBar) {
        dom.globalProgressBar.style.width = `${percentage}%`;
        dom.globalProgressBar.textContent = `${percentage}%`;
        dom.globalProgressBar.setAttribute('aria-valuenow', percentage);
        dom.globalProgressBar.style.backgroundColor = isError ? 'var(--red)' : 'var(--green)';
    }
}

export async function updateSpecificLogUI(logName, path, content, isError = false, errorMessage = '') {
    try {
        let styledContent = '';
        if (isError) {
            const escapedErrorMessage = DOMUpdateUtils.escapeHtml(errorMessage);
            styledContent = `<span class="log-line log-error">${escapedErrorMessage}</span>`;
        } else {
            styledContent = await workerManager.parseLogs(content, 'specificLogPanel');
        }

        domBatcher.scheduleUpdate('specific-log-ui', () => {
            const headerText = resolveElement(dom.getSpecificLogHeaderTextPanel, dom.specificLogHeaderTextPanel);
            const pathInfo = resolveElement(dom.getSpecificLogPathInfoPanel, dom.specificLogPathInfoPanel);
            const outputContent = resolveElement(dom.getSpecificLogOutputContentPanel, dom.specificLogOutputContentPanel);
            const specificLogContainer = resolveElement(dom.getSpecificLogContainerPanel, dom.specificLogContainerPanel);
            const mainLogContainer = resolveElement(dom.getMainLogContainerPanel, dom.mainLogContainerPanel);

            if(headerText) headerText.textContent = isError ? `Erreur chargement "${logName}"` : `Log Spécifique: "${logName}"`;
            if(pathInfo) pathInfo.textContent = path ? `(Source: ${path})` : "";
            
            if(outputContent) {
                outputContent.innerHTML = styledContent;
            }
            
            if(specificLogContainer) specificLogContainer.style.display = 'flex';
            if(mainLogContainer) mainLogContainer.style.display = 'none';
            if(outputContent) outputContent.scrollTop = 0;
        });
    } catch (e) {
        console.debug('[UI] Log parsing aborted/obsolete:', e.message);
    }
}

/**
 * DOM-context wrapper around the shared logParserUtils.parseAndStyleLogContent.
 * Automatically uses DOMUpdateUtils.escapeHtml for XSS safety.
 *
 * @param {string} rawContent - Raw log content
 * @returns {string} - Styled HTML content
 */
export function parseAndStyleLogContent(rawContent) {
    return _parseAndStyleLogContent(rawContent, DOMUpdateUtils.escapeHtml);
}

export async function updateMainLogOutputUI(htmlContent) {
    const mainLogOutputPanel = resolveElement(dom.getMainLogOutputPanel, dom.mainLogOutputPanel);
    const mainLogContainerPanel = resolveElement(dom.getMainLogContainerPanel, dom.mainLogContainerPanel);
    const specificLogContainerPanel = resolveElement(dom.getSpecificLogContainerPanel, dom.specificLogContainerPanel);

    if (mainLogContainerPanel) mainLogContainerPanel.style.display = 'flex';
    if (specificLogContainerPanel) specificLogContainerPanel.style.display = 'none';

    if (mainLogOutputPanel) {
        try {
            const styledContent = await workerManager.parseLogs(htmlContent, 'mainLogPanel');
            domBatcher.scheduleUpdate('main-log-output', () => {
                mainLogOutputPanel.innerHTML = styledContent;
                mainLogOutputPanel.scrollTop = mainLogOutputPanel.scrollHeight;
            });
        } catch (e) {
            console.debug('[UI] Main log parsing aborted/obsolete:', e.message);
        }
    }
}


