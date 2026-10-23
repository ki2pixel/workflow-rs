import * as dom from './domElements.js';
import * as ui from './uiUpdater.js';
import * as api from './apiService.js';
import { initializeEventHandlers } from './eventHandlers.js';
import { POLLING_INTERVAL } from './constants.js';
import { showNotification } from './utils.js';
import { showSequenceSummaryUI } from './popupManager.js';
import { scrollToStepImmediate } from './scrollManager.js';

import { initializeSoundManager } from './soundManager.js';
import { pollingManager } from './utils/PollingManager.js';
import { errorHandler } from './utils/ErrorHandler.js';
import { performanceMonitor } from './utils/PerformanceMonitor.js';
import { domBatcher, DOMUpdateUtils } from './utils/DOMBatcher.js';

import { performanceOptimizer } from './utils/PerformanceOptimizer.js';
import { appState } from './state/AppState.js';
import { initializeCSVDownloadMonitor } from './csvDownloadMonitor.js';
import { themeManager } from './themeManager.js';



globalThis.addEventListener('unhandledrejection', (event) => {
    console.error('[MAIN] Unhandled promise rejection:', event.reason);

    // Check if this is the specific browser extension error we're trying to fix
    if (event.reason && event.reason.message &&
        event.reason.message.includes('message channel closed')) {
        console.debug('[MAIN] Suppressing browser extension message channel error');
        event.preventDefault(); // Prevent the error from appearing in console
        return;
    }
});

function setupLocalDownloadsToggle() {
    const section = document.querySelector('.local-downloads-section');
    const btn = document.getElementById('toggle-local-downloads');
    if (!section || !btn) return;

    const visible = !!appState.getStateProperty('ui.localDownloadsVisible');

    if (visible) {
        section.style.display = '';
    } else {
        section.style.display = 'none';
    }

    applyLocalDownloadsVisibility(section, btn, visible);

    appState.subscribeToProperty('ui.localDownloadsVisible', (newVal) => {
        applyLocalDownloadsVisibility(section, btn, !!newVal);
    });

    btn.addEventListener('click', () => {
        const current = !!appState.getStateProperty('ui.localDownloadsVisible');
        appState.setState({ ui: { localDownloadsVisible: !current } }, 'downloads_visibility_toggle');
        if (!current) {
            btn.classList.remove('downloads-toggle--alert');
            const currentUI = appState.getStateProperty('ui') || {};
            appState.setState({ ui: { ...currentUI, localDownloadsAlertedOnce: false } }, 'downloads_visibility_reset');
        }
    });

    updateDownloadsToggleAlert(appState.getStateProperty('csvDownloads') || []);
}

function applyLocalDownloadsVisibility(section, btn, visible) {
    domBatcher.scheduleUpdate('downloads-visibility-toggle', () => {
        if (visible) {
            section.style.display = '';
            section.classList.remove('minimized');
            btn.setAttribute('aria-pressed', 'true');
            btn.classList.remove('downloads-toggle--hidden');
            // Focus and highlight the Downloads section title for accessibility feedback
            requestAnimationFrame(() => {
                const title = section.querySelector('h2');
                if (title) {
                    safeFocusAndHighlight(title);
                }
            });
        } else {
            btn.setAttribute('aria-pressed', 'false');
            btn.classList.add('downloads-toggle--hidden');
            section.style.display = 'none';
        }
    });
}

function safeFocusAndHighlight(el) {
    if (!el || typeof el !== 'object') return;
    try {
        // Make sure element can be focused without altering tab order permanently
        let removeTabIndex = false;
        if (el !== document.body && el.tabIndex === -1) {
            // Already programmatically focusable
        } else if (el !== document.body && (el.getAttribute && el.getAttribute('tabindex') === null)) {
            el.setAttribute('tabindex', '-1');
            removeTabIndex = true;
        }
        el.focus && el.focus();
        // Apply highlight flash
        if (el.classList) {
            el.classList.add('section-focus-highlight');
            setTimeout(() => { try { el.classList.remove('section-focus-highlight'); } catch(_){} }, 450);
        }
        if (removeTabIndex) {
            setTimeout(() => { try { el.removeAttribute('tabindex'); } catch(_){} }, 500);
        }
    } catch(_){}
}

function updateDownloadsToggleAlert(downloads) {
    const btn = document.getElementById('toggle-local-downloads');
    const section = document.querySelector('.local-downloads-section');
    if (!btn || !section) return;

    const visible = btn.getAttribute('aria-pressed') === 'true';
    const list = Array.isArray(downloads) ? downloads : [];
    const inProgress = list.some(d => {
        const s = (d && d.status) ? String(d.status).toLowerCase() : '';
        return s === 'downloading' || s === 'starting' || s === 'pending';
    });

    if (!visible && inProgress) {
        btn.classList.add('downloads-toggle--alert');
        try {
            const alerted = !!appState.getStateProperty('ui.localDownloadsAlertedOnce');
            if (!alerted && typeof showNotification === 'function') {
                showNotification('Téléchargements', 'Des téléchargements locaux sont en cours. Cliquez pour afficher.');
                const currentUI = appState.getStateProperty('ui') || {};
                appState.setState({ ui: { ...currentUI, localDownloadsAlertedOnce: true } }, 'downloads_alert_shown');
            }
        } catch (_) {}
    } else {
        btn.classList.remove('downloads-toggle--alert');
        const currentUI = appState.getStateProperty('ui') || {};
        if (currentUI.localDownloadsAlertedOnce) {
            appState.setState({ ui: { ...currentUI, localDownloadsAlertedOnce: false } }, 'downloads_alert_reset');
        }
    }
}

globalThis.addEventListener('error', (event) => {
    console.error('[MAIN] Uncaught error:', event.error);
    errorHandler.handleApiError('uncaught-error', event.error);
});

let stepsConfigData = {};
try {
    stepsConfigData = JSON.parse(document.getElementById('steps-config-data').textContent);
} catch (e) {
    console.error("Could not parse steps_config_data:", e);
}
ui.setStepsConfig(stepsConfigData);

function initializeProcessInfoFromDOM() {
    const initialProcessInfo = {};
    document.querySelectorAll('.step').forEach(s => {
        const stepKey = s && s.dataset ? s.dataset.stepKey : null;
        if (!stepKey) return;
        initialProcessInfo[stepKey] = {
            status: 'idle',
            log: [],
            progress_current: 0,
            progress_total: 0,
            progress_text: '',
            is_any_sequence_running: false
        };
    });
    appState.setState({ processInfo: initialProcessInfo }, 'process_info_init');
}

const LOCAL_DOWNLOAD_POLLING_INTERVAL = POLLING_INTERVAL * 2;

const SYSTEM_MONITOR_POLLING_INTERVAL = 5000;



function initializeStateManagement() {
    if (globalThis.location.hostname === 'localhost' || globalThis.location.hostname === '127.0.0.1') {
        appState.subscribe((newState, oldState, source) => {
            console.debug(`[StateManagement] State changed from ${source}:`, {
                changes: findStateChanges(oldState, newState),
                newState: newState
            });
        });
    }

    appState.subscribeToProperty('isAnySequenceRunning', (newValue, oldValue) => {
        if (newValue !== oldValue) {
            ui.updateGlobalUIForSequenceState(newValue);
        }
    });

    appState.subscribeToProperty('activeStepKeyForLogsPanel', (newValue, oldValue) => {
        if (newValue !== oldValue && newValue) {
            domBatcher.scheduleUpdate('logs-panel-update', () => {
                console.debug(`Active step for logs changed to: ${newValue}`);
            });
        }
    });

    setTimeout(() => {
        if (typeof CacheService !== 'undefined' && CacheService.warm_cache) {
            CacheService.warm_cache();
        }
    }, 1000);

}

function findStateChanges(oldState, newState) {
    const changes = {};

    function compareObjects(old, current, path = '') {
        for (const key in current) {
            const currentPath = path ? `${path}.${key}` : key;

            if (typeof current[key] === 'object' && current[key] !== null && !Array.isArray(current[key])) {
                if (typeof old[key] === 'object' && old[key] !== null) {
                    compareObjects(old[key], current[key], currentPath);
                } else {
                    changes[currentPath] = { from: old[key], to: current[key] };
                }
            } else if (old[key] !== current[key]) {
                changes[currentPath] = { from: old[key], to: current[key] };
            }
        }
    }

    compareObjects(oldState, newState);
    return changes;
}

async function pollLocalDownloadsStatus() {
    if (!dom.getLocalDownloadsList()) return;

    try {
        const downloads = await api.fetchLocalDownloadsStatusAPI();
        ui.updateLocalDownloadsListUI(downloads);

        appState.setState({ csvDownloads: downloads }, 'downloads_polled');

        updateDownloadsToggleAlert(downloads);

        errorHandler.clearErrors('localDownloadsStatus', {
            elementId: 'local-downloads-list'
        });

    } catch (error) {
        const delay = await errorHandler.handlePollingError('localDownloadsStatus', error, {
            elementId: 'local-downloads-list'
        });

        if (delay > 0) {
            console.debug(`Applying ${delay}ms delay for localDownloadsStatus polling`);
        }
    }
}



async function pollSystemMonitor() {
    const monitorWidget = document.getElementById('system-monitor-widget');
    if (!monitorWidget) {
        console.warn('[MAIN] System monitor widget not found during polling');
        return;
    }

    try {
        const fetchStartedAt = performance.now();
        const response = await fetch('/api/system_monitor');
        const fetchDurationMs = performance.now() - fetchStartedAt;
        if (!response.ok) {
            console.warn(`[MAIN] System monitor API failed: ${response.status} (after ${fetchDurationMs.toFixed(0)}ms)`);
            monitorWidget.style.opacity = '0.5';
            return;
        }
        const data = await response.json();
        console.debug('[MAIN] System monitor data received:', data);

        domBatcher.scheduleUpdate('system-monitor-update', () => {
            const lagMs = performance.now() - fetchStartedAt;
            if (lagMs > 1000) {
                console.warn(`[MAIN] System monitor update lag: fetch=${fetchDurationMs.toFixed(0)}ms, repaint=${lagMs.toFixed(0)}ms`);
            }
            const cpuBar = document.getElementById('cpu-monitor-bar');
            const cpuValue = document.getElementById('cpu-monitor-value');
            if (cpuBar && cpuValue) {
                const cpuPercent = data.cpu_percent || 0;
                cpuBar.style.width = `${cpuPercent}%`;
                cpuValue.textContent = `${cpuPercent.toFixed(1)} %`;
                cpuBar.dataset.usageLevel = cpuPercent > 85 ? 'high' : cpuPercent > 60 ? 'medium' : 'low';
            }

            const ramBar = document.getElementById('ram-monitor-bar');
            const ramValue = document.getElementById('ram-monitor-value');
            const ramDetails = document.getElementById('ram-monitor-details');
            if (ramBar && ramValue && ramDetails && data.memory) {
                const memPercent = data.memory.percent || 0;
                ramBar.style.width = `${memPercent}%`;
                ramValue.textContent = `${memPercent.toFixed(1)} %`;
                ramDetails.textContent = `${data.memory.used_gb.toFixed(2)} / ${data.memory.total_gb.toFixed(2)} GB`;
                ramBar.dataset.usageLevel = memPercent > 85 ? 'high' : memPercent > 70 ? 'medium' : 'low';
            }

            const gpuSection = document.getElementById('gpu-monitor-section');
            const gpuError = document.getElementById('gpu-monitor-error');
            if (gpuSection && gpuError) {
                if (data.gpu && !data.gpu.error) {
                    gpuSection.style.display = 'block';
                    gpuError.style.display = 'none';

                    const gpuBar = document.getElementById('gpu-monitor-bar');
                    const gpuValue = document.getElementById('gpu-monitor-value');
                    const gpuDetails = document.getElementById('gpu-monitor-details');

                    const gpuPercent = data.gpu.utilization_percent || 0;
                    gpuBar.style.width = `${gpuPercent}%`;
                    gpuValue.textContent = `${gpuPercent.toFixed(1)} %`;
                    gpuBar.dataset.usageLevel = gpuPercent > 85 ? 'high' : gpuPercent > 60 ? 'medium' : 'low';

                    const temp = data.gpu.temperature_c || 'N/A';
                    const memUsed = data.gpu.memory ? data.gpu.memory.used_gb.toFixed(2) : 'N/A';
                    const memTotal = data.gpu.memory ? data.gpu.memory.total_gb.toFixed(2) : 'N/A';
                    gpuDetails.textContent = `${temp}°C | ${memUsed} / ${memTotal} GB`;
                } else {
                    gpuSection.style.display = 'none';
                    if (data.gpu && data.gpu.error) {
                        gpuError.textContent = data.gpu.error;
                        gpuError.style.display = 'block';
                    }
                }
            }

            const compactLine = document.getElementById('monitor-compact-line');
            if (compactLine) {
                const compactCpu = document.getElementById('compact-cpu');
                const compactRam = document.getElementById('compact-ram');
                const compactGpu = document.getElementById('compact-gpu');

                const cpuPercent = data.cpu_percent || 0;
                const memPercent = (data.memory && data.memory.percent) ? data.memory.percent : 0;

                if (compactCpu) compactCpu.textContent = `${cpuPercent.toFixed(1)}%`;
                if (compactRam && data.memory) {
                    const used = (typeof data.memory.used_gb === 'number') ? data.memory.used_gb.toFixed(1) : 'N/A';
                    const total = (typeof data.memory.total_gb === 'number') ? data.memory.total_gb.toFixed(1) : 'N/A';
                    compactRam.textContent = `${memPercent.toFixed(1)}% (${used}/${total}G)`;
                }
                if (compactGpu) {
                    if (data.gpu && !data.gpu.error && typeof data.gpu.utilization_percent === 'number') {
                        const temp = (typeof data.gpu.temperature_c === 'number') ? data.gpu.temperature_c : 'N/A';
                        const gUsed = data.gpu.memory && typeof data.gpu.memory.used_gb === 'number' ? data.gpu.memory.used_gb.toFixed(1) : 'N/A';
                        const gTotal = data.gpu.memory && typeof data.gpu.memory.total_gb === 'number' ? data.gpu.memory.total_gb.toFixed(1) : 'N/A';
                        compactGpu.textContent = `${data.gpu.utilization_percent.toFixed(1)}% (${temp}C)`;
                    } else if (data.gpu && data.gpu.error) {
                        compactGpu.textContent = 'err';
                    } else {
                        compactGpu.textContent = 'N/A';
                    }
                }
            }

            monitorWidget.style.opacity = '1';
        });

        errorHandler.clearErrors('systemMonitor', {
            elementId: 'system-monitor-widget'
        });

    } catch (error) {
        const delay = await errorHandler.handlePollingError('systemMonitor', error, {
            elementId: 'system-monitor-widget'
        });

        domBatcher.scheduleUpdate('system-monitor-error', () => {
            monitorWidget.style.opacity = '0.5';
        });

        if (delay > 0) {
            console.debug(`Applying ${delay}ms delay for systemMonitor polling`);
        }
    }
}

document.addEventListener('DOMContentLoaded', async () => {
    ui.closeLogPanelUI();
    if (dom.sequenceSummaryPopupOverlay) dom.sequenceSummaryPopupOverlay.style.display = 'none';
    
    if (dom.customSequenceConfirmPopupOverlay) dom.customSequenceConfirmPopupOverlay.style.display = 'none';

    themeManager.init();


    const initialStatusPromises = [];
    const allStepKeysForInitialStatus = Object.keys(stepsConfigData);

    initializeProcessInfoFromDOM();

    allStepKeysForInitialStatus.forEach(stepKey => {
        initialStatusPromises.push(api.fetchInitialStatusAPI(stepKey));
    });



    try {
        await Promise.all(initialStatusPromises);
    } catch (error) {
        console.warn("[Main.js DOMContentLoaded] Error fetching some initial statuses:", error);
    } finally {
        ui.updateGlobalUIForSequenceState(appState.getStateProperty('isAnySequenceRunning'));
        ui.updateCustomSequenceButtonsUI();
    }

    initializeEventHandlers();

    initializeSoundManager();

    initializeCSVDownloadMonitor();

    initializeStateManagement();

    if (dom.getLocalDownloadsList()) {
        pollingManager.startPolling(
            'localDownloadsStatus',
            pollLocalDownloadsStatus,
            LOCAL_DOWNLOAD_POLLING_INTERVAL,
            { immediate: true }
        );
    }

    setupLocalDownloadsToggle();




    const startSystemMonitorPolling = () => {
        const widget = document.getElementById('system-monitor-widget');
        if (widget) {
            pollingManager.startPolling(
                'systemMonitor',
                pollSystemMonitor,
                SYSTEM_MONITOR_POLLING_INTERVAL,
                { immediate: true }
            );
            return true;
        } else {
            console.warn('[MAIN] System monitor widget not found, retrying...');
            return false;
        }
    };

    if (!startSystemMonitorPolling()) {
        setTimeout(() => {
            if (!startSystemMonitorPolling()) {
                console.error('[MAIN] System monitor widget not found after retry');
            }
        }, 1000);
    }

    setupCompactMode();

    setupSettingsPanel();

    setupSystemMonitorMinimize();

    setupKeyboardShortcuts();


});

function setupCompactMode() {
    const wrapper = typeof dom.getWorkflowWrapper === 'function'
        ? dom.getWorkflowWrapper()
        : dom.workflowWrapper;
    if (!wrapper) {
        console.warn('[COMPACT] Wrapper not found, skipping setup');
        return;
    }

    // Force compact mode as the only mode
    appState.setState({ ui: { compactMode: true } }, 'compact_forced_default');

    // Apply immediately and keep in sync if state changes elsewhere
    applyCompactClass(wrapper, true);
    appState.subscribeToProperty('ui.compactMode', (newVal) => {
        applyCompactClass(wrapper, !!newVal);
    });
}

function applyCompactClass(wrapper, enabled) {
    domBatcher.scheduleUpdate('compact-mode-toggle', () => {
        if (enabled) {
            wrapper.classList.add('compact-mode');
        } else {
            wrapper.classList.remove('compact-mode');
        }
    });
}


function setupSystemMonitorMinimize() {
    const widget = document.getElementById('system-monitor-widget');
    const btn = document.getElementById('system-monitor-minimize');
    const compactLine = document.getElementById('monitor-compact-line');
    if (!widget || !btn) {
        console.warn('[SYSTEM-MONITOR] Elements not found, skipping minimize setup');
        return;
    }

    // Init from state
    const minimized = !!appState.getStateProperty('ui.systemMonitorMinimized');
    applySystemMonitorMinimized(widget, compactLine, minimized);

    // Subscribe to state changes
    appState.subscribeToProperty('ui.systemMonitorMinimized', (newVal) => {
        applySystemMonitorMinimized(widget, compactLine, !!newVal);
    });

    // Click on button to minimize
    btn.addEventListener('click', (e) => {
        e.stopPropagation();
        const current = !!appState.getStateProperty('ui.systemMonitorMinimized');
        appState.setState({ ui: { systemMonitorMinimized: !current } }, 'system_monitor_toggle');
    });

    // Click on widget restores when minimized
    widget.addEventListener('click', () => {
        const isMinimized = widget.classList.contains('minimized');
        if (isMinimized) {
            appState.setState({ ui: { systemMonitorMinimized: false } }, 'system_monitor_restore_click');
        }
    });
}

function applySystemMonitorMinimized(widget, compactLine, minimized) {
    domBatcher.scheduleUpdate('system-monitor-minimized-toggle', () => {
        if (minimized) {
            widget.classList.add('minimized');
            if (compactLine) {
                compactLine.style.display = 'flex';
                compactLine.setAttribute('aria-hidden', 'false');
            }
        } else {
            widget.classList.remove('minimized');
            if (compactLine) {
                compactLine.style.display = 'none';
                compactLine.setAttribute('aria-hidden', 'true');
            }
        }
    });
}

function setupSettingsPanel() {
    const toggle = dom.getSettingsToggle();
    const panel = dom.getSettingsPanel();
    if (!toggle || !panel) {
        console.warn('[SETTINGS] Elements not found, skipping setup');
        return;
    }

    // Apply initial state
    const initialOpen = !!appState.getStateProperty('ui.settingsOpen');
    applySettingsPanel(panel, toggle, initialOpen);

    // Keep DOM in sync with state changes
    appState.subscribeToProperty('ui.settingsOpen', (open) => {
        applySettingsPanel(panel, toggle, !!open);
    });

    // Toggle handler
    toggle.addEventListener('click', () => {
        const next = !appState.getStateProperty('ui.settingsOpen');
        appState.setState({ ui: { settingsOpen: next } }, 'settings_toggle');
    });
}

function applySettingsPanel(panel, toggle, open) {
    domBatcher.scheduleUpdate('settings-panel-update', () => {
        if (!panel || !toggle) return;
        if (open) {
            panel.classList.add('open');
            panel.hidden = false;
        } else {
            panel.classList.remove('open');
            panel.hidden = true;
        }
        toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
        toggle.setAttribute('aria-label', open ? 'Refermer les réglages' : 'Ouvrir les réglages');
    });
}

function setupKeyboardShortcuts() {
    document.addEventListener('keydown', (e) => {
        // Only handle shortcuts when not typing in inputs
        if (e.target && (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA' || e.target.tagName === 'SELECT' || e.target.isContentEditable)) {
            return;
        }

        // Toggle Settings panel with 'S' key
        if (e.key === 's' || e.key === 'S') {
            e.preventDefault();
            const toggle = dom.getSettingsToggle();
            if (toggle) {
                toggle.click();
            }
        }
    });
}
