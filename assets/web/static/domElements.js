const _SAFE_STEP_KEY_PATTERN = /^[A-Za-z0-9_-]+$/;

function byId(id) {
    return document.getElementById(id);
}

function bySelectorAll(selector) {
    return document.querySelectorAll(selector);
}

export const getWorkflowWrapper = () => byId('workflow-wrapper');
export const getStepsColumn = () => byId('steps-column');
export const getLogsColumnGlobal = () => byId('logs-column-global');
export const getLogPanelTitle = () => byId('log-panel-title');
export const getLogPanelSubheader = () => byId('log-panel-subheader');
export const getLogPanelContextStep = () => byId('log-panel-context-step');
export const getLogPanelContextStatus = () => byId('log-panel-context-status');
export const getLogPanelContextTimer = () => byId('log-panel-context-timer');
export const getLogPanelSpecificButtonsContainer = () => byId('log-panel-specific-buttons-container');
export const getMainLogContainerPanel = () => byId('main-log-container-panel');
export const getMainLogOutputPanel = () => byId('main-log-output-panel');
export const getCurrentStepLogNamePanel = () => byId('current-step-log-name-panel');
export const getSpecificLogContainerPanel = () => byId('specific-log-container-panel');
export const getSpecificLogHeaderTextPanel = () => byId('specific-log-header-text-panel');
export const getSpecificLogPathInfoPanel = () => byId('specific-log-path-info-panel');
export const getSpecificLogOutputContentPanel = () => byId('specific-log-output-content-panel');
export const getRunAllButton = () => byId('run-all-steps-button');
export const getTopbarAffix = () => byId('topbar-affix');
export const getTopbarControls = () => byId('topbar-controls');
export const getGlobalProgressAffix = () => byId('global-progress-affix');
export const getGlobalProgressContainer = () => byId('global-progress-container');
export const getGlobalProgressBar = () => byId('global-progress-bar');
export const getGlobalProgressText = () => byId('global-progress-text');
export const getSequenceSummaryPopupOverlay = () => byId('sequence-summary-popup-overlay');
export const getSequenceSummaryList = () => byId('sequence-summary-list');
export const getCloseSummaryPopupButton = () => byId('close-summary-popup');
export const getRunCustomSequenceButton = () => byId('run-custom-sequence-button');
export const getClearCustomSequenceButton = () => byId('clear-custom-sequence-button');
export const getCustomSequenceCheckboxes = () => bySelectorAll('.custom-sequence-checkbox');
export const getCustomSequenceConfirmPopupOverlay = () => byId('custom-sequence-confirm-popup-overlay');
export const getCustomSequenceConfirmList = () => byId('custom-sequence-confirm-list');
export const getConfirmRunCustomSequenceButton = () => byId('confirm-run-custom-sequence-button');
export const getCancelRunCustomSequenceButton = () => byId('cancel-run-custom-sequence-button');
export const getNotificationsArea = () => byId('notifications-area');

// Lazy DOM element getters to ensure elements are available when accessed
export function getAllStepDivs() {
    const elements = document.querySelectorAll('.step');
    console.debug(`[DOM] getAllStepDivs found ${elements.length} elements`);
    return elements;
}

export function getAllRunButtons() {
    const elements = document.querySelectorAll('.run-button');
    console.debug(`[DOM] getAllRunButtons found ${elements.length} elements`);
    return elements;
}

export function getAllCancelButtons() {
    const elements = document.querySelectorAll('.cancel-button');
    console.debug(`[DOM] getAllCancelButtons found ${elements.length} elements`);
    return elements;
}

export function getAllSpecificLogButtons() {
    const elements = document.querySelectorAll('.specific-log-button');
    console.debug(`[DOM] getAllSpecificLogButtons found ${elements.length} elements`);
    return elements;
}

// Enhanced step element getter with validation
export function getStepElement(stepKey) {
    if (!stepKey) {
        console.warn('[DOM] getStepElement called with invalid stepKey:', stepKey);
        return null;
    }

    if (!_SAFE_STEP_KEY_PATTERN.test(String(stepKey))) {
        console.warn('[DOM] getStepElement called with unsafe stepKey (refusing to query by id):', stepKey);
        return null;
    }

    const element = document.getElementById(`step-${stepKey}`);
    if (!element) {
        console.warn(`[DOM] Step element not found: step-${stepKey}`);
        console.debug('[DOM] Available step elements:',
            Array.from(document.querySelectorAll('[id^="step-"]')).map(el => el.id));
    }

    return element;
}

// Validate DOM structure for debugging
export function validateDOMStructure() {
    const results = {
        stepElements: getAllStepDivs().length,
        runButtons: getAllRunButtons().length,
        cancelButtons: getAllCancelButtons().length,
        workflowWrapper: !!getWorkflowWrapper(),
        stepsColumn: !!getStepsColumn(),
        issues: []
    };

    // Check for common issues
    if (results.stepElements === 0) {
        results.issues.push('No step elements found (.step)');
    }

    if (!results.workflowWrapper) {
        results.issues.push('Workflow wrapper not found (#workflow-wrapper)');
    }

    if (!results.stepsColumn) {
        results.issues.push('Steps column not found (#steps-column)');
    }

    console.debug('[DOM] Structure validation:', results);
    return results;
}

// New getter functions for lazy DOM access
export const getCloseLogPanelButton = () => byId('close-log-panel');
export const getLocalDownloadsList = () => byId('local-downloads-list');

// ÉLÉMENTS POUR LE CONTRÔLE SONORE
export const getSoundToggle = () => byId('sound-toggle');
export const getSoundStatus = () => byId('sound-status');
export const getSoundControlWidget = () => byId('sound-control-widget');

// Toggle auto ouverture logs
export const getLogsAutoOpenToggle = () => byId('logs-auto-open-toggle');

// ÉLÉMENTS POUR LE PANNEAU DE RÉGLAGES (top bar)
export const getSettingsToggle = () => byId('settings-toggle');
export const getSettingsPanel = () => byId('settings-panel');