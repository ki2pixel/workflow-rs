// timerManager.js
// Dedicated module for managing step execution timers

import { appState } from './state/AppState.js';
import { pollingManager } from './utils/PollingManager.js';
import { formatElapsedTime } from './utils.js';
import { domBatcher } from './utils/DOMBatcher.js';

export function getStepTimers() {
    return appState.getStateProperty('stepTimers') || {};
}

export function getStepTimer(stepKey) {
    return getStepTimers()[stepKey];
}

function setStepTimer(stepKey, timerData, source = 'setStepTimer') {
    const timers = getStepTimers();
    appState.setState({ stepTimers: { ...timers, [stepKey]: timerData } }, source);
}

function deleteStepTimer(stepKey) {
    const timers = getStepTimers();
    const { [stepKey]: _, ...remaining } = timers;
    appState.setState({ stepTimers: remaining }, 'deleteStepTimer');
}

export function startStepTimer(stepKey) {
    const timerName = `step-timer-${stepKey}`;
    pollingManager.stopPolling(timerName);

    const startTime = Date.now();
    setStepTimer(stepKey, {
        startTime: startTime,
        startTimeDate: new Date(startTime),
        intervalId: 'polling',
        elapsedTimeFormatted: "0s"
    }, 'startStepTimer');

    if (stepKey !== 'clear_disk_cache') {
        domBatcher.scheduleUpdate(`timer-init-${stepKey}`, () => {
            const timerEl = document.getElementById(`timer-${stepKey}`);
            if (timerEl) timerEl.textContent = "(0s)";
        });
    }

    pollingManager.startPolling(timerName, () => {
        const currentTimer = getStepTimer(stepKey);
        if (!currentTimer || (!currentTimer.startTime && !currentTimer.startTimeDate)) {
            pollingManager.stopPolling(timerName);
            return;
        }

        const startTimeToUse = currentTimer.startTime ? new Date(currentTimer.startTime) : currentTimer.startTimeDate;
        const elapsedTimeStr = formatElapsedTime(startTimeToUse);
        setStepTimer(stepKey, { ...currentTimer, elapsedTimeFormatted: elapsedTimeStr }, 'timer_tick');

        if (stepKey !== 'clear_disk_cache') {
            domBatcher.scheduleUpdate(`timer-update-${stepKey}`, () => {
                const timerEl = document.getElementById(`timer-${stepKey}`);
                if (timerEl) timerEl.textContent = `(${elapsedTimeStr})`;
            });
        }
    }, 1000);
}

export function stopStepTimer(stepKey) {
    const timerName = `step-timer-${stepKey}`;
    pollingManager.stopPolling(timerName);

    const timerData = getStepTimer(stepKey);
    if (timerData) {
        setStepTimer(stepKey, { ...timerData, intervalId: null }, 'timer_interval_cleared');
    }
    const updatedTimerData = getStepTimer(stepKey);
    if (updatedTimerData && (updatedTimerData.startTime || updatedTimerData.startTimeDate)) {
        const startTimeToUse = updatedTimerData.startTime ? new Date(updatedTimerData.startTime) : updatedTimerData.startTimeDate;
        const elapsedTimeStr = formatElapsedTime(startTimeToUse);
        setStepTimer(stepKey, { ...updatedTimerData, elapsedTimeFormatted: elapsedTimeStr }, 'timer_stopped');
        if (stepKey !== 'clear_disk_cache') {
            const timerEl = document.getElementById(`timer-${stepKey}`);
            if (timerEl) timerEl.textContent = `(Terminé en ${elapsedTimeStr})`;
        }
    }
}

export function resetStepTimerDisplay(stepKey) {
    if (stepKey !== 'clear_disk_cache') {
        const timerEl = document.getElementById(`timer-${stepKey}`);
        if (timerEl) timerEl.textContent = "";
    }
    deleteStepTimer(stepKey);
}
