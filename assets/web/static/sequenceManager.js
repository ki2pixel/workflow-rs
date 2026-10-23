import { POLLING_INTERVAL } from './constants.js';
import * as ui from './uiUpdater.js';
import { runStepAPI } from './apiService.js';
import { showSequenceSummaryUI } from './popupManager.js';
import { formatElapsedTime } from './utils.js';
import { scrollToActiveStep, isSequenceAutoScrollEnabled } from './scrollManager.js';

import { appState } from './state/AppState.js';
import { pollingManager } from './utils/PollingManager.js';

import { soundEvents } from './soundManager.js';

/**
 * Executes and tracks a single step within a sequence.
 * This helper function encapsulates all logic for one step, from initiation to completion.
 * @private
 * @param {string} stepKey - The unique key for the step.
 * @param {string} sequenceName - The name of the parent sequence.
 * @param {number} currentStepNum - The step's number in the sequence (e.g., 1, 2, 3...).
 * @param {number} totalSteps - The total number of steps in the sequence.
 * @returns {Promise<object>} A promise that resolves to a result object: { name, success, duration }.
 */
async function _executeSingleStep(stepKey, sequenceName, currentStepNum, totalSteps) {
    const stepConfig = ui.getStepsConfig()[stepKey];
    const stepDisplayName = stepConfig ? stepConfig.display_name : stepKey;

    console.log(`[SEQ_MGR] ${sequenceName} - Step ${currentStepNum}/${totalSteps}: ${stepDisplayName} (${stepKey})`);

    ui.updateGlobalProgressUI(`${sequenceName} - Étape ${currentStepNum}/${totalSteps}: ${stepDisplayName}`,
        Math.round(((currentStepNum - 1) / totalSteps) * 100)
    );

    if (stepKey !== 'clear_disk_cache') {
        ui.openLogPanelUI(stepKey);
        ui.setActiveStepForLogPanelUI(stepKey);
        
        if (isSequenceAutoScrollEnabled()) {
            setTimeout(() => {
                scrollToActiveStep(stepKey, { behavior: 'smooth', scrollDelay: 0 });
            }, 0);
        }
    }

    const stepInitiated = await runStepAPI(stepKey);
    if (!stepInitiated) {
        console.error(`[SEQ_MGR] Initiation FAILED for ${stepKey}`);
        ui.updateGlobalProgressUI(`ÉCHEC: L'étape "${stepDisplayName}" n'a pas pu être initiée. Séquence interrompue.`,
            Math.round(((currentStepNum - 1) / totalSteps) * 100), true
        );
        return { name: stepDisplayName, success: false, duration: "N/A (échec initiation)" };
    }

    ui.startStepTimer(stepKey);
    console.log(`[SEQ_MGR] Started timer for ${stepKey}`);

    try {
        ui.setActiveStepForLogPanelUI(stepKey);
    } catch (e) {
        console.debug('[SEQ_MGR] setActiveStepForLogPanelUI post-start failed (non-fatal):', e);
    }

    let timerData = appState.getStateProperty(`stepTimers.${stepKey}`);
    if (timerData && timerData.startTime) {
        console.log(`[SEQ_MGR] Timer verified for ${stepKey}, start time:`, timerData.startTime);
    } else {
        console.error(`[SEQ_MGR] Timer NOT properly started for ${stepKey}:`, timerData);
    }

    console.log(`[SEQ_MGR] Waiting for completion of ${stepKey}`);
    const stepCompleted = await waitForStepCompletionInSequence(stepKey);

    ui.stopStepTimer(stepKey);
    console.log(`[SEQ_MGR] Stopped timer for ${stepKey}`);

    timerData = appState.getStateProperty(`stepTimers.${stepKey}`);
    const duration = (timerData?.elapsedTimeFormatted) || "N/A";

    console.log(`[SEQ_MGR] Timer data for ${stepKey}:`, {
        timerData,
        duration,
        startTime: timerData?.startTime,
        elapsedTimeFormatted: timerData?.elapsedTimeFormatted
    });

    if (!stepCompleted) {
        console.error(`[SEQ_MGR] Execution FAILED for ${stepKey}`);

        ui.updateGlobalProgressUI(`ÉCHEC: L'étape "${stepDisplayName}" a échoué. Séquence interrompue.`,
            Math.round((currentStepNum / totalSteps) * 100), true
        );
        return { name: stepDisplayName, success: false, duration };
    }

    console.log(`[SEQ_MGR] Step ${stepDisplayName} completed successfully.`);

    soundEvents.stepSuccess();

    return { name: stepDisplayName, success: true, duration };
}

export async function runStepSequence(stepsToExecute, sequenceName = "Séquence") {
    console.log(`[SEQ_MGR] Starting sequence: ${sequenceName} with steps:`, stepsToExecute);
    ui.updateGlobalUIForSequenceState(true);
    ui.updateGlobalProgressUI(`Démarrage de la ${sequenceName}...`, 0);

    const sequenceStart = Date.now();

    const sequenceResults = [];
    const totalStepsInThisSequence = stepsToExecute.length;
    const isAutoModeSequence = sequenceName === "AutoMode";
    let sequenceFailed = false;

    if (isAutoModeSequence) {
        appState.setState({ ui: { autoModeLogPanelOpened: false } }, 'auto_mode_sequence_reset');
    }

    for (let i = 0; i < stepsToExecute.length; i++) {
        const stepKey = stepsToExecute[i];
        const currentStepNum = i + 1;

        const result = await _executeSingleStep(stepKey, sequenceName, currentStepNum, totalStepsInThisSequence);
        sequenceResults.push(result);

        if (!result.success) {
            sequenceFailed = true;
            break; // Exit the loop immediately on failure
        }

        if (i < stepsToExecute.length - 1) {
            const nextStepKey = stepsToExecute[i + 1];
            if (nextStepKey && nextStepKey !== 'clear_disk_cache') {
                try {
                    ui.openLogPanelUI(nextStepKey);
                    ui.setActiveStepForLogPanelUI(nextStepKey);
                    
                    if (isSequenceAutoScrollEnabled()) {
                        setTimeout(() => {
                            scrollToActiveStep(nextStepKey, { behavior: 'smooth', scrollDelay: 0 });
                        }, 0);
                    }
                } catch (e) {
                    console.debug('[SEQ_MGR] Pre-focus next step failed (non-fatal):', e);
                }
            }
            ui.updateGlobalProgressUI(`${sequenceName} - Étape ${currentStepNum}/${totalStepsInThisSequence}: ${result.name} terminée.`,
                Math.round((currentStepNum / totalStepsInThisSequence) * 100)
            );
        }
    }

    console.log(`[SEQ_MGR] Sequence ${sequenceName} finished. sequenceFailed: ${sequenceFailed}`);

    if (sequenceFailed) {
    } else {
        ui.updateGlobalProgressUI(`${sequenceName} terminée avec succès ! 🎉`, 100);
        soundEvents.workflowCompletion();
    }

    if (sequenceResults.length > 0) {
        const overallDuration = formatElapsedTime(new Date(sequenceStart));
        showSequenceSummaryUI(sequenceResults, !sequenceFailed, sequenceName, overallDuration);
    } else {
        console.warn(`[SEQ_MGR] No results to show for sequence ${sequenceName}`);
    }

    ui.updateGlobalUIForSequenceState(false);
    if (isAutoModeSequence) {
        appState.setState({ ui: { autoModeLogPanelOpened: false } }, 'auto_mode_sequence_reset_end');
    }
}

function waitForStepCompletionInSequence(stepKey) {
    return new Promise((resolve) => {
        const intervalIdForLog = `wait_${stepKey}_${Date.now()}`;
        console.log(`[SEQ_MGR - ${intervalIdForLog}] Waiting for final status...`);

        pollingManager.startPolling(intervalIdForLog, () => {
            const data = appState.getStateProperty(`processInfo.${stepKey}`);

            if (!data) {
                return;
            }

            if (data.status === 'completed') {
                console.log(`[SEQ_MGR - ${intervalIdForLog}] Resolved as COMPLETED.`);
                pollingManager.stopPolling(intervalIdForLog);
                resolve(true);
            } else if (data.status === 'failed' || data.return_code === -9) {
                console.error(`[SEQ_MGR - ${intervalIdForLog}] Resolved as FAILED or CANCELLED.`);
                pollingManager.stopPolling(intervalIdForLog);
                resolve(false);
            }
        }, POLLING_INTERVAL / 2);
    });
}