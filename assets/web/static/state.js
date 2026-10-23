/**
 * @deprecated Use constants.js for REMOTE_SEQUENCE_STEP_KEYS and AppState directly for state management.
 * This file is kept for backward compatibility only.
 */
import { appState } from './state/AppState.js';
import { REMOTE_SEQUENCE_STEP_KEYS, defaultSequenceableStepsKeys } from './constants.js';

// Re-export for legacy consumers
export { REMOTE_SEQUENCE_STEP_KEYS, defaultSequenceableStepsKeys };

// Modern state management functions using AppState
export function setActiveStepKeyForLogs(key) {
    appState.setState({ activeStepKeyForLogsPanel: key }, 'setActiveStepKeyForLogs');
}
export function getActiveStepKeyForLogs() {
    return appState.getStateProperty('activeStepKeyForLogsPanel') || null;
}

export function addStepTimer(stepKey, timerData) {
    appState.setState({
        stepTimers: { ...appState.getStateProperty('stepTimers'), [stepKey]: timerData }
    }, 'addStepTimer');
}
export function getStepTimer(stepKey) {
    return appState.getStateProperty(`stepTimers.${stepKey}`) || null;
}
export function clearStepTimerInterval(stepKey) {
    const timer = getStepTimer(stepKey);
    if (timer && timer.intervalId) {
        clearInterval(timer.intervalId);
        const updatedTimer = { ...timer, intervalId: null };
        addStepTimer(stepKey, updatedTimer);
    }
}
export function deleteStepTimer(stepKey) {
    if (getStepTimer(stepKey)) {
        clearStepTimerInterval(stepKey);
        const currentTimers = appState.getStateProperty('stepTimers') || {};
        const { [stepKey]: removed, ...remainingTimers } = currentTimers;
        appState.setState({ stepTimers: remainingTimers }, 'deleteStepTimer');
    }
}

export function setSelectedStepsOrder(order) {
    appState.setState({ selectedStepsOrder: order }, 'setSelectedStepsOrder');
}
export function getSelectedStepsOrder() {
    return appState.getStateProperty('selectedStepsOrder') || [];
}

export function setIsAnySequenceRunning(running) {
    appState.setState({ isAnySequenceRunning: running }, 'setIsAnySequenceRunning');
}
export function getIsAnySequenceRunning() {
    return appState.getStateProperty('isAnySequenceRunning') || false;
}

export function setFocusedElementBeforePopup(element) {
    appState.setState({ focusedElementBeforePopup: element }, 'setFocusedElementBeforePopup');
}
export function getFocusedElementBeforePopup() {
    return appState.getStateProperty('focusedElementBeforePopup') || null;
}

export function setAutoOpenLogOverlay(enabled) {
    const currentUI = appState.getStateProperty('ui') || {};
    appState.setState({ ui: { ...currentUI, autoOpenLogOverlay: !!enabled } }, 'setAutoOpenLogOverlay');
}

export function getAutoOpenLogOverlay() {
    const uiValue = appState.getStateProperty('ui.autoOpenLogOverlay');
    return typeof uiValue === 'boolean' ? uiValue : true;
}

export function setAutoModeLogPanelOpened(opened) {
    appState.setState({ ui: { autoModeLogPanelOpened: !!opened } }, 'setAutoModeLogPanelOpened');
}

export function getAutoModeLogPanelOpened() {
    return !!appState.getStateProperty('ui.autoModeLogPanelOpened');
}

// Export the appState for direct access to modern state management
export { appState };