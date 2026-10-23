export const POLLING_INTERVAL = 500;
export const POLLING_INTERVAL_HIGH_FREQUENCY = 200; // High-frequency polling interval for AutoMode sequences

// --- MODIFICATION: La liste des étapes est mise à jour pour correspondre au backend ---
export const defaultSequenceableStepsKeys = [
    "STEP1",
    "STEP2",
    "STEP3",
    "STEP4",
    "STEP5",
    "STEP6",
    "STEP7",
    "STEP8"
];

// Re-export REMOTE_SEQUENCE_STEP_KEYS for legacy compatibility (deprecated, use defaultSequenceableStepsKeys)
export const REMOTE_SEQUENCE_STEP_KEYS = defaultSequenceableStepsKeys;

export const STEP_STATUS = Object.freeze({
    PENDING: 'pending',
    RUNNING: 'running',
    STARTING: 'starting',
    INITIATED: 'initiated',
    COMPLETED: 'completed',
    SUCCESS: 'success',
    FAILED: 'failed',
    ERROR: 'error',
    CANCELLED: 'cancelled',
    WARNING: 'warning',
    PAUSED: 'paused',
    IDLE: 'idle'
});