class AppState {
    constructor() {
        this.PERSISTED_PATHS = [
            'ui.compactMode',
            'ui.autoOpenLogOverlay',
            'selectedStepsOrder',
            'ui.localDownloadsVisible',
            'ui.systemMonitorMinimized',
            'ui.settingsOpen',
            'ui.autoScroll',
            'ui.sequenceAutoScroll',
            'ui.soundEnabled',
            'ui.soundVolume',
            'ui.theme',
            'ui.localDownloadsAlertedOnce'
        ];

        this.LEGACY_MIGRATIONS = {
            'autoOpenLogOverlay': 'ui.autoOpenLogOverlay',
            'ui.compactMode': 'ui.compactMode',
            'ui.localDownloadsVisible': 'ui.localDownloadsVisible',
            'ui.systemMonitorMinimized': 'ui.systemMonitorMinimized',
            'ui.settingsOpen': 'ui.settingsOpen',
            'workflow-auto-scroll': 'ui.autoScroll',
            'workflow-sequence-auto-scroll': 'ui.sequenceAutoScroll',
            'soundEnabled': 'ui.soundEnabled',
            'soundVolume': 'ui.soundVolume',
            'theme': 'ui.theme',
            'localDownloadsAlertedOnce': 'ui.localDownloadsAlertedOnce'
        };

        // Marqueur de migration one-shot de la sélection d'étapes persistée.
        // La renumérotation STEP1→STEP7 (suppression de l'étape de conversion)
        // rend toute sélection antérieure ambiguë : l'ancien "STEP2" désigne
        // désormais l'analyse des transitions, pas la conversion.
        this.STEP_KEYS_MIGRATION_FLAG = 'appstate:migration:stepKeysV2';
        this.PERSISTED_STEPS_ORDER_PATH = 'appstate:selectedStepsOrder';

        this.state = {
            pollingIntervals: {},
            
            activeStepKeyForLogsPanel: null,
            isAnySequenceRunning: false,
            focusedElementBeforePopup: null,
            ui: {
                compactMode: false,
                autoOpenLogOverlay: true,
                localDownloadsVisible: true,
                systemMonitorMinimized: false,
                settingsOpen: false,
                autoScroll: true,
                sequenceAutoScroll: true,
                soundEnabled: true,
                soundVolume: 1.0,
                theme: 'dark',
                localDownloadsAlertedOnce: false
            },
            
            stepTimers: {},
            selectedStepsOrder: [],
            
            processInfo: {},
            
            performanceMetrics: {
                apiResponseTimes: [],
                errorCounts: {},
                lastUpdate: null
            },
            
            cacheStats: {
                hits: 0,
                misses: 0,
                hitRate: 0
            }
        };
        
        this.listeners = new Set();
        this.isDestroyed = false;
        
        this.stateChangeCount = 0;
        this.lastStateChange = Date.now();
        
        // Load persisted state synchronously to ensure initial state is correct before UI renders
        this._loadPersistedState();
        
        console.debug('[AppState] Initialized with immutable state management');
    }
    
    getState() {
        if (this.isDestroyed) {
            console.warn('[AppState] Attempted to access destroyed state');
            return {};
        }
        return this._deepClone(this.state);
    }
    
    getStateProperty(path) {
        if (this.isDestroyed) return undefined;
        
        return path.split('.').reduce((obj, key) => {
            return obj && obj[key] !== undefined ? obj[key] : undefined;
        }, this.state);
    }
    
    setState(updates, source = 'unknown') {
        if (this.isDestroyed) {
            console.warn('[AppState] Attempted to update destroyed state');
            return;
        }
        
        const oldState = this._deepClone(this.state);
        const newState = this._mergeDeep(this.state, updates);
        
        if (this._stateChanged(oldState, newState)) {
            this.state = newState;
            this.stateChangeCount++;
            this.lastStateChange = Date.now();
            
            console.debug(`[AppState] State updated from ${source}:`, updates);
            
            this._persistStateChanges(oldState, newState);
            
            this._notifyListeners(newState, oldState, source);
        }
    }
    
    subscribe(listener) {
        if (typeof listener !== 'function') {
            throw new Error('[AppState] Listener must be a function');
        }
        
        this.listeners.add(listener);
        
        return () => {
            this.listeners.delete(listener);
        };
    }
    
    /**
     * Subscribe to specific state property changes.
     * @param {string} path - Dot-notation path to property
     * @param {Function} listener - Callback function (newValue, oldValue) => void
     * @returns {Function} Unsubscribe function
     */
    subscribeToProperty(path, listener) {
        const propertyListener = (newState, oldState) => {
            const newValue = this._getPropertyByPath(newState, path);
            const oldValue = this._getPropertyByPath(oldState, path);
            
            if (newValue !== oldValue) {
                listener(newValue, oldValue);
            }
        };
        
        return this.subscribe(propertyListener);
    }
    
    batchUpdate(updateFn, source = 'batch') {
        const originalNotify = this._notifyListeners;
        const updates = [];
        
        this._notifyListeners = (newState, oldState, updateSource) => {
            updates.push({ newState, oldState, source: updateSource });
        };
        
        try {
            updateFn();
        } finally {
            this._notifyListeners = originalNotify;
            
            if (updates.length > 0) {
                const finalUpdate = updates[updates.length - 1];
                this._notifyListeners(finalUpdate.newState, updates[0].oldState, source);
            }
        }
    }
    
    reset() {
        const initialState = {
            pollingIntervals: {},
            activeStepKeyForLogsPanel: null,
            isAnySequenceRunning: false,
            focusedElementBeforePopup: null,
            ui: {
                compactMode: false,
                autoOpenLogOverlay: true,
                localDownloadsVisible: true,
                systemMonitorMinimized: false,
                settingsOpen: false
            },
            stepTimers: {},
            selectedStepsOrder: [],

            processInfo: {},
            performanceMetrics: {
                apiResponseTimes: [],
                errorCounts: {},
                lastUpdate: null
            },
            cacheStats: {
                hits: 0,
                misses: 0,
                hitRate: 0
            }
        };
        
        this.setState(initialState, 'reset');
        console.info('[AppState] State reset to initial values');
        
        // When resetting, we should also clear the persisted state
        try {
            this.PERSISTED_PATHS.forEach(path => {
                localStorage.removeItem(`appstate:${path}`);
            });
        } catch (e) {
            console.warn('[AppState] Failed to clear persisted state on reset:', e);
        }
    }
    
    getStats() {
        return {
            listenerCount: this.listeners.size,
            stateChangeCount: this.stateChangeCount,
            lastStateChange: this.lastStateChange,
            isDestroyed: this.isDestroyed,
            stateSize: JSON.stringify(this.state).length
        };
    }
    
    destroy() {
        console.info('[AppState] Destroying state manager');
        
        this.listeners.clear();
        this.state = {};
        this.isDestroyed = true;
    }
    
    _deepClone(obj) {
        if (typeof structuredClone === 'function') {
            try {
                return structuredClone(obj);
            } catch (error) {
                console.warn('[AppState] structuredClone failed, falling back to manual clone:', error);
            }
        }

        if (obj === null || typeof obj !== 'object') {
            return obj;
        }

        if (obj instanceof Date) {
            return new Date(obj.getTime());
        }

        if (Array.isArray(obj)) {
            return obj.map(item => this._deepClone(item));
        }

        const cloned = {};
        for (const key in obj) {
            if (Object.hasOwn(obj, key)) {
                cloned[key] = this._deepClone(obj[key]);
            }
        }
        return cloned;
    }
    
    _mergeDeep(target, source) {
        const result = this._deepClone(target);
        
        for (const key in source) {
            if (Object.hasOwn(source, key)) {
                if (source[key] && typeof source[key] === 'object' && !Array.isArray(source[key])) {
                    result[key] = this._mergeDeep(result[key] || {}, source[key]);
                } else {
                    result[key] = source[key];
                }
            }
        }
        
        return result;
    }
    
    _stateChanged(oldState, newState) {
        return !this._areValuesEqual(oldState, newState);
    }

    _areValuesEqual(a, b, visited = new WeakMap()) {
        if (Object.is(a, b)) {
            return true;
        }

        if (typeof a !== typeof b) {
            return false;
        }

        if (a === null || b === null) {
            return false;
        }

        if (typeof a !== 'object') {
            return false;
        }

        if (visited.has(a) && visited.get(a) === b) {
            return true;
        }
        visited.set(a, b);

        const aKeys = Object.keys(a);
        const bKeys = Object.keys(b);
        if (aKeys.length !== bKeys.length) {
            return false;
        }

        for (const key of aKeys) {
            if (!Object.hasOwn(b, key)) {
                return false;
            }
            if (!this._areValuesEqual(a[key], b[key], visited)) {
                return false;
            }
        }

        return true;
    }
    
    _getPropertyByPath(obj, path) {
        return path.split('.').reduce((current, key) => {
            return current && current[key] !== undefined ? current[key] : undefined;
        }, obj);
    }
    
    _setPropertyByPath(obj, path, value) {
        const keys = path.split('.');
        let current = obj;
        for (let i = 0; i < keys.length - 1; i++) {
            const key = keys[i];
            if (current[key] === undefined || current[key] === null || typeof current[key] !== 'object') {
                current[key] = {};
            }
            // Ensure we create a new reference if we are deep cloning/merging
            if (!Array.isArray(current[key])) {
                current[key] = { ...current[key] };
            } else {
                current[key] = [...current[key]];
            }
            current = current[key];
        }
        current[keys[keys.length - 1]] = value;
        return obj;
    }
    
    _loadPersistedState() {
        if (typeof window === 'undefined' || typeof localStorage === 'undefined') return;
        
        let stateHasBeenLoaded = false;
        
        // Step 0: purge one-shot de la sélection d'étapes après renumérotation.
        // Sans cela, une sélection enregistrée avant la migration relancerait
        // silencieusement une autre étape que celle attendue par l'utilisateur.
        try {
            if (localStorage.getItem(this.STEP_KEYS_MIGRATION_FLAG) === null) {
                localStorage.removeItem(this.PERSISTED_STEPS_ORDER_PATH);
                localStorage.setItem(this.STEP_KEYS_MIGRATION_FLAG, 'done');
                console.info('[AppState] Sélection d\'étapes purgée (renumérotation STEP1→STEP7)');
            }
        } catch (e) {
            console.warn('[AppState] Failed to purge legacy step selection:', e);
        }

        // Step 1: Migrate legacy keys if they exist and delete them
        try {
            for (const [legacyKey, appStatePath] of Object.entries(this.LEGACY_MIGRATIONS)) {
                const storedValue = localStorage.getItem(legacyKey);
                if (storedValue !== null) {
                    // Try to parse if JSON, otherwise handle boolean string ('true' / 'false')
                    let parsedValue = storedValue;
                    if (storedValue === 'true' || storedValue === 'enabled') parsedValue = true;
                    else if (storedValue === 'false' || storedValue === 'disabled') parsedValue = false;
                    else {
                        try {
                            parsedValue = JSON.parse(storedValue);
                        } catch (e) {
                            // Keep as string if parsing fails
                        }
                    }
                    
                    localStorage.setItem(`appstate:${appStatePath}`, JSON.stringify(parsedValue));
                    localStorage.removeItem(legacyKey);
                    console.debug(`[AppState] Migrated legacy key '${legacyKey}' to 'appstate:${appStatePath}'`);
                }
            }
        } catch (e) {
            console.warn('[AppState] Failed to run migrations:', e);
        }

        // Step 2: Load explicitly persisted paths
        this.PERSISTED_PATHS.forEach(path => {
            try {
                const stored = localStorage.getItem(`appstate:${path}`);
                if (stored !== null) {
                    const parsed = JSON.parse(stored);
                    this.state = this._setPropertyByPath(this.state, path, parsed);
                    stateHasBeenLoaded = true;
                    console.debug(`[AppState] Restored ${path} from localStorage:`, parsed);
                }
            } catch (e) {
                console.warn(`[AppState] Failed to restore ${path} from localStorage:`, e);
            }
        });
        
        if (stateHasBeenLoaded) {
            console.debug('[AppState] Initial state merged with persisted preferences.');
        }
    }
    
    _persistStateChanges(oldState, newState) {
        if (typeof window === 'undefined' || typeof localStorage === 'undefined') return;
        
        this.PERSISTED_PATHS.forEach(path => {
            const oldValue = this._getPropertyByPath(oldState, path);
            const newValue = this._getPropertyByPath(newState, path);
            
            // Note: Deep equality check may be better here but reference change is enough
            // since we use immutable state management
            if (oldValue !== newValue && newValue !== undefined) {
                try {
                    localStorage.setItem(`appstate:${path}`, JSON.stringify(newValue));
                    console.debug(`[AppState] Persisted ${path} to localStorage:`, newValue);
                } catch (e) {
                    console.warn(`[AppState] Failed to persist ${path} to localStorage:`, e);
                }
            }
        });
    }
    
    _notifyListeners(newState, oldState, source) {
        this.listeners.forEach(listener => {
            try {
                listener(newState, oldState, source);
            } catch (error) {
                console.error('[AppState] Listener error:', error);
            }
        });
    }
}

export const appState = new AppState();

globalThis.addEventListener('beforeunload', () => {
    appState.destroy();
});

if (globalThis.location.hostname === 'localhost' || globalThis.location.hostname === '127.0.0.1') {
    
    appState.subscribe((newState, oldState, source) => {
        console.debug(`[AppState] Change from ${source}:`, {
            newState: newState,
            oldState: oldState
        });
    });
}

export default appState;
