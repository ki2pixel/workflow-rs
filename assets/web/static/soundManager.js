// ===== SOUND MANAGER =====
// Manages audio feedback for user interactions and workflow events
import { appState } from './state/AppState.js';

/**
 * Sound event types and their corresponding audio files
 */
const SOUND_EVENTS = {
    WORKFLOW_START: 'DM_20250703182122_001.mp3',
    CHECKBOX_INTERACTION: 'DM_20250703182143_001.mp3',
    WORKFLOW_COMPLETION: 'DM_20250703182159_001.mp3',
    ERROR_EVENT: 'DM_20250703182219_001.mp3',
    AUTO_MODE_TOGGLE: 'DM_20250703182234_001.mp3',
    STEP_SUCCESS: 'DM_20250703182315_001.mp3',
    CSV_DOWNLOAD_INITIATION: 'DM_20250703194305_001.mp3',
    CSV_DOWNLOAD_COMPLETION: 'DM_20250703182159_001.mp3' // Reuse workflow completion sound
};

/**
 * Sound manager state
 */
let soundEnabled = true;
let audioCache = new Map();
let masterVolume = 0.7;

/**
 * Initialize the sound manager
 */
export function initializeSoundManager() {
    // Load user preferences from AppState
    const savedSoundEnabled = appState.getStateProperty('ui.soundEnabled');
    if (typeof savedSoundEnabled === 'boolean') {
        soundEnabled = savedSoundEnabled;
    }

    const savedVolume = appState.getStateProperty('ui.soundVolume');
    if (typeof savedVolume === 'number') {
        masterVolume = savedVolume;
    }

    // Preload audio files
    preloadAudioFiles();

    console.log('[SOUND] Sound manager initialized', {
        soundEnabled,
        masterVolume,
        availableSounds: Object.keys(SOUND_EVENTS)
    });
}

/**
 * Preload all audio files for better performance
 */
function preloadAudioFiles() {
    Object.entries(SOUND_EVENTS).forEach(([eventType, filename]) => {
        try {
            const audio = new Audio(`/sound-design/${filename}`);
            audio.preload = 'auto';
            audio.volume = masterVolume;
            
            // Handle loading events
            audio.addEventListener('canplaythrough', () => {
                console.log(`[SOUND] Preloaded: ${filename}`);
            });
            
            audio.addEventListener('error', (e) => {
                console.warn(`[SOUND] Failed to preload: ${filename}`, e);
            });
            
            audioCache.set(eventType, audio);
        } catch (error) {
            console.warn(`[SOUND] Error creating audio object for ${filename}:`, error);
        }
    });
}

// Rate limiting for sound events to prevent spam
const soundRateLimit = new Map();
const SOUND_RATE_LIMIT_MS = 1000; // Minimum time between same sound events

/**
 * Play a sound for a specific event type with rate limiting and proper cleanup
 * @param {string} eventType - The type of event (from SOUND_EVENTS keys)
 * @param {Object} options - Optional parameters
 * @param {number} options.volume - Volume override (0.0 to 1.0)
 * @param {boolean} options.force - Force play even if sounds are disabled
 */
export function playSound(eventType, options = {}) {
    if (!soundEnabled && !options.force) {
        return;
    }

    if (!SOUND_EVENTS[eventType]) {
        console.warn(`[SOUND] Unknown event type: ${eventType}`);
        return;
    }

    // Rate limiting - prevent rapid-fire sound events
    const now = Date.now();
    const lastPlayed = soundRateLimit.get(eventType) || 0;
    if (now - lastPlayed < SOUND_RATE_LIMIT_MS) {
        return; // Skip this sound event to prevent spam
    }
    soundRateLimit.set(eventType, now);

    try {
        const audio = audioCache.get(eventType);
        if (!audio) {
            console.warn(`[SOUND] Audio not found in cache for event: ${eventType}`);
            return;
        }

        // Reuse the same audio element instead of cloning to prevent WebMediaPlayer accumulation
        audio.currentTime = 0; // Reset to beginning
        audio.volume = options.volume !== undefined ? options.volume : masterVolume;

        // Play the sound
        const playPromise = audio.play();

        if (playPromise !== undefined) {
            playPromise
                .then(() => {
                    // Reduced logging - only log success once per audio element
                    if (!audio.hasLoggedSuccess) {
                        console.log(`[SOUND] Audio system working for ${eventType}`);
                        audio.hasLoggedSuccess = true;
                    }
                })
                .catch((error) => {
                    // Handle autoplay restrictions gracefully with reduced logging
                    if (error.name === 'NotAllowedError') {
                        if (!audio.hasLoggedAutoplayBlock) {
                            console.log(`[SOUND] Autoplay blocked - user interaction required`);
                            audio.hasLoggedAutoplayBlock = true;
                        }
                    } else {
                        console.warn(`[SOUND] Error playing ${eventType}:`, error);
                    }
                });
        }
    } catch (error) {
        console.warn(`[SOUND] Exception playing sound for ${eventType}:`, error);
    }
}

/**
 * Enable or disable sound effects
 * @param {boolean} enabled - Whether sounds should be enabled
 */
export function setSoundEnabled(enabled) {
    soundEnabled = !!enabled;
    const currentUI = appState.getStateProperty('ui') || {};
    appState.setState({ ui: { ...currentUI, soundEnabled: soundEnabled } }, 'setSoundEnabled');
    console.log(`[SOUND] Sound ${enabled ? 'enabled' : 'disabled'}`);
}

/**
 * Get current sound enabled state
 * @returns {boolean} Whether sounds are enabled
 */
export function isSoundEnabled() {
    return soundEnabled;
}

/**
 * Set master volume for all sounds
 * @param {number} volume - Volume level (0.0 to 1.0)
 */
export function setMasterVolume(volume) {
    masterVolume = Math.max(0, Math.min(1, volume));
    const currentUI = appState.getStateProperty('ui') || {};
    appState.setState({ ui: { ...currentUI, soundVolume: masterVolume } }, 'setMasterVolume');
    
    // Update volume for all cached audio objects
    audioCache.forEach((audio) => {
        audio.volume = masterVolume;
    });
    
    console.log(`[SOUND] Master volume set to: ${masterVolume}`);
}

/**
 * Get current master volume
 * @returns {number} Current master volume (0.0 to 1.0)
 */
export function getMasterVolume() {
    return masterVolume;
}

/**
 * Convenience functions for specific event types
 */
export const soundEvents = {
    workflowStart: () => playSound('WORKFLOW_START'),
    checkboxInteraction: () => playSound('CHECKBOX_INTERACTION'),
    workflowCompletion: () => playSound('WORKFLOW_COMPLETION'),
    errorEvent: () => playSound('ERROR_EVENT'),
    autoModeToggle: () => playSound('AUTO_MODE_TOGGLE'),
    stepSuccess: () => playSound('STEP_SUCCESS'),
    csvDownloadInitiation: () => playSound('CSV_DOWNLOAD_INITIATION'),
    csvDownloadCompletion: () => playSound('CSV_DOWNLOAD_COMPLETION')
};
