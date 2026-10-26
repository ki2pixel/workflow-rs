/**
 * Centralized polling management utility for workflow_mediapipe frontend.
 * 
 * This module provides safe interval management with automatic cleanup
 * to prevent memory leaks and ensure proper resource management.
 */

class PollingManager {
    /**
     * Initialize the polling manager.
     */
    constructor() {
        this.intervals = new Map();
        this.timeouts = new Map();
        this.isDestroyed = false;
        this.pendingResumes = new Map();
        this.pausedPollers = new Map();
        this.pauseOnHide = new Set();
        this.errorCounts = new Map();
        this.inFlight = new Set();
        this.skippedTicks = new Map();
        this.maxErrorCount = 5;
        
        this._bindCleanupEvents();
        
        console.debug('PollingManager initialized');
    }

    /**
     * Start polling with a given callback function.
     * 
     * @param {string} name - Unique name for this polling operation
     * @param {Function} callback - Function to call on each interval
     * @param {number} interval - Interval in milliseconds
     * @param {Object} options - Additional options
     * @param {boolean} options.immediate - Whether to call callback immediately
     * @param {number} options.maxErrors - Maximum consecutive errors before stopping
     * @param {boolean} options.pauseWhenHidden - Suspend this poller while the
     *   document is hidden (WebView minimised in the tray) and refresh it when
     *   the document becomes visible again
     * @returns {string|null} - Polling ID or null if manager is destroyed
     */
    startPolling(name, callback, interval, options = {}) {
        if (this.isDestroyed) {
            console.warn('PollingManager: Cannot start polling, manager is destroyed');
            return null;
        }

        this.stopPolling(name);

        const {
            immediate = false,
            maxErrors = this.maxErrorCount,
            pauseWhenHidden = false
        } = options;

        if (pauseWhenHidden) {
            this.pauseOnHide.add(name);
        } else {
            this.pauseOnHide.delete(name);
        }

        this.errorCounts.set(name, 0);

        const wrappedCallback = async () => {
            if (this.isDestroyed) {
                return;
            }

            // Anti-pile-up: never overlap two runs of the same poller. A slow
            // response (e.g. while a large download saturates the server) used
            // to stack requests and starve the browser connection pool, which
            // froze unrelated widgets until the backlog drained.
            if (this.inFlight.has(name)) {
                this.skippedTicks.set(name, (this.skippedTicks.get(name) || 0) + 1);
                console.debug(`Skipped tick for ${name}: previous run still in flight`);
                return;
            }

            this.inFlight.add(name);

            try {
                const result = await callback();
                this.errorCounts.set(name, 0);

                if (typeof result === 'number' && result > 0) {
                    this._reschedule(name, wrappedCallback, interval, result);
                    return;
                }
            } catch (error) {
                const errorCount = (this.errorCounts.get(name) || 0) + 1;
                this.errorCounts.set(name, errorCount);
                
                console.error(`Polling error in ${name} (attempt ${errorCount}):`, error);

                // Transient failures must not kill a poller for the rest of the
                // session: back off, then resume. The error is only reported
                // once the consecutive-error budget is exhausted.
                const backoffMs = Math.min(interval * Math.pow(2, errorCount), 30000);
                this._reschedule(name, wrappedCallback, interval, backoffMs);

                if (errorCount >= maxErrors) {
                    this._dispatchPollingError(name, error, errorCount);
                }
            } finally {
                this.inFlight.delete(name);
            }
        };

        // Une WebView minimisée dans le tray reste vivante : sans cette
        // suspension, /api/system_monitor et le suivi des téléchargements
        // continuent d'interroger le serveur et font grossir
        // WebKitNetworkProcess pendant toute la durée du soak.
        if (pauseWhenHidden && this._isPageHidden()) {
            this.pausedPollers.set(name, {
                callback: wrappedCallback,
                interval: interval
            });
            console.debug(`Started polling paused (page hidden): ${name}`);
            return name;
        }

        if (immediate) {
            wrappedCallback();
        }

        const intervalId = setInterval(wrappedCallback, interval);
        this.intervals.set(name, {
            id: intervalId,
            callback: wrappedCallback,
            interval: interval,
            startTime: Date.now()
        });

        console.debug(`Started polling: ${name} (interval: ${interval}ms)`);
        return name;
    }

    /**
     * Pause a poller and resume it after a delay (backoff).
     * 
     * @param {string} name - Polling operation name
     * @param {Function} callback - Wrapped callback to resume
     * @param {number} interval - Original interval in milliseconds
     * @param {number} delayMs - Delay before resuming
     * @private
     */
    _reschedule(name, callback, interval, delayMs) {
        const existing = this.intervals.get(name);
        if (existing) {
            clearInterval(existing.id);
            this.intervals.delete(name);
        }
        const pending = this.pendingResumes.get(name);
        if (pending) {
            clearTimeout(pending.id);
            this.pendingResumes.delete(name);
        }

        // Un backoff ne doit pas réveiller un poller suspendu : on le garde
        // en attente et la reprise au focus repartira de zéro.
        if (this.pauseOnHide.has(name) && this._isPageHidden()) {
            this.pausedPollers.set(name, {
                callback: callback,
                interval: interval
            });
            return;
        }

        const resumeId = setTimeout(() => {
            this.pendingResumes.delete(name);
            if (!this.isDestroyed && !this.intervals.has(name)) {
                const newIntervalId = setInterval(callback, interval);
                this.intervals.set(name, {
                    id: newIntervalId,
                    callback: callback,
                    interval: interval,
                    startTime: Date.now()
                });
                console.debug(`Resumed polling: ${name} after ${delayMs}ms backoff`);
            }
        }, delayMs);
        this.pendingResumes.set(name, {
            id: resumeId,
            callback: callback,
            interval: interval
        });
    }

    /**
     * Stop a specific polling operation.
     * 
     * @param {string} name - Name of the polling operation to stop
     * @returns {boolean} - True if polling was stopped, false if not found
     */
    stopPolling(name) {
        const pollingInfo = this.intervals.get(name);
        const wasPending = this.pendingResumes.has(name)
            || this.pausedPollers.has(name)
            || this.inFlight.has(name);
        if (!pollingInfo && !wasPending) {
            return false;
        }
        if (pollingInfo) {
            clearInterval(pollingInfo.id);
            this.intervals.delete(name);
            const duration = Date.now() - pollingInfo.startTime;
            console.debug(`Stopped polling: ${name} (ran for ${duration}ms)`);
        }
        this.errorCounts.delete(name);
        this.skippedTicks.delete(name);
        this.pausedPollers.delete(name);
        this.pauseOnHide.delete(name);
        const pending = this.pendingResumes.get(name);
        if (pending) {
            clearTimeout(pending.id);
            this.pendingResumes.delete(name);
        }
        return true;
    }

    /**
     * Schedule a one-time delayed execution.
     * 
     * @param {string} name - Unique name for this timeout
     * @param {Function} callback - Function to call after delay
     * @param {number} delay - Delay in milliseconds
     * @returns {string|null} - Timeout ID or null if manager is destroyed
     */
    setTimeout(name, callback, delay) {
        if (this.isDestroyed) {
            console.warn('PollingManager: Cannot set timeout, manager is destroyed');
            return null;
        }

        this.clearTimeout(name);

        const wrappedCallback = () => {
            if (this.isDestroyed) {
                return;
            }

            try {
                callback();
            } catch (error) {
                console.error(`Timeout callback error in ${name}:`, error);
            } finally {
                this.timeouts.delete(name);
            }
        };

        const timeoutId = setTimeout(wrappedCallback, delay);
        this.timeouts.set(name, {
            id: timeoutId,
            callback: wrappedCallback,
            delay: delay,
            startTime: Date.now()
        });

        console.debug(`Set timeout: ${name} (delay: ${delay}ms)`);
        return name;
    }

    /**
     * Clear a specific timeout.
     * 
     * @param {string} name - Name of the timeout to clear
     * @returns {boolean} - True if timeout was cleared, false if not found
     */
    clearTimeout(name) {
        const timeoutInfo = this.timeouts.get(name);
        if (timeoutInfo) {
            clearTimeout(timeoutInfo.id);
            this.timeouts.delete(name);
            
            console.debug(`Cleared timeout: ${name}`);
            return true;
        }
        return false;
    }

    /**
     * Get information about active polling operations.
     * 
     * @returns {Object} - Information about active operations
     */
    getActiveOperations() {
        const now = Date.now();
        
        return {
            intervals: Array.from(this.intervals.entries()).map(([name, info]) => ({
                name,
                interval: info.interval,
                runningTime: now - info.startTime,
                errorCount: this.errorCounts.get(name) || 0,
                skippedTicks: this.skippedTicks.get(name) || 0
            })),
            timeouts: Array.from(this.timeouts.entries()).map(([name, info]) => ({
                name,
                delay: info.delay,
                timeRemaining: Math.max(0, (info.startTime + info.delay) - now)
            })),
            paused: Array.from(this.pausedPollers.keys()),
            totalIntervals: this.intervals.size,
            totalTimeouts: this.timeouts.size,
            totalPaused: this.pausedPollers.size
        };
    }

    /**
     * Get summarized statistics for monitoring consumers (PerformanceMonitor).
     *
     * @returns {Object}
     */
    getStats() {
        const ops = this.getActiveOperations();
        return {
            totalIntervals: ops.totalIntervals,
            totalTimeouts: ops.totalTimeouts,
            totalPaused: ops.totalPaused,
            intervals: ops.intervals,
            timeouts: ops.timeouts,
            paused: ops.paused
        };
    }

    /**
     * Destroy the polling manager and clean up all resources.
     */
    destroy() {
        if (this.isDestroyed) {
            return;
        }

        console.debug('Destroying PollingManager...');

        this.intervals.forEach((info, name) => {
            clearInterval(info.id);
            console.debug(`Cleaned up interval: ${name}`);
        });
        this.intervals.clear();

        this.timeouts.forEach((info, name) => {
            clearTimeout(info.id);
            console.debug(`Cleaned up timeout: ${name}`);
        });
        this.timeouts.clear();

        this.errorCounts.clear();
        this.skippedTicks.clear();
        this.inFlight.clear();
        this.pausedPollers.clear();
        this.pauseOnHide.clear();

        this.pendingResumes.forEach((pending, name) => {
            clearTimeout(pending.id);
            console.debug(`Cleared pending resume: ${name}`);
        });
        this.pendingResumes.clear();

        this.isDestroyed = true;
        console.debug('PollingManager destroyed');
    }

    /**
     * Bind cleanup events to prevent memory leaks.
     * @private
     */
    _bindCleanupEvents() {
        if (typeof globalThis.addEventListener === 'function') {
            globalThis.addEventListener('beforeunload', () => {
                this.destroy();
            });

            globalThis.addEventListener('pagehide', () => {
                this.destroy();
            });
        }

        if (typeof document !== 'undefined' && typeof document.addEventListener === 'function') {
            document.addEventListener('visibilitychange', () => {
                this._handleVisibilityChange();
            });
        }
    }

    /**
     * Whether the document is currently hidden (WebView minimised in the tray).
     * @returns {boolean}
     * @private
     */
    _isPageHidden() {
        if (typeof document === 'undefined') {
            return false;
        }
        if (typeof document.hidden === 'boolean') {
            return document.hidden;
        }
        return document.visibilityState === 'hidden';
    }

    /**
     * Suspend only the pollers that opted in with `pauseWhenHidden`.
     * @private
     */
    _handleVisibilityChange() {
        if (this.isDestroyed) {
            return;
        }
        if (this._isPageHidden()) {
            this._suspendPausedPollers();
        } else {
            this._resumePausedPollers();
        }
    }

    /**
     * Move opted-in active intervals and backoffs to the paused set.
     * @private
     */
    _suspendPausedPollers() {
        let suspended = 0;

        for (const [name, info] of Array.from(this.intervals.entries())) {
            if (!this.pauseOnHide.has(name)) {
                continue;
            }
            clearInterval(info.id);
            this.intervals.delete(name);
            this.pausedPollers.set(name, {
                callback: info.callback,
                interval: info.interval
            });
            suspended += 1;
        }

        for (const [name, pending] of Array.from(this.pendingResumes.entries())) {
            if (!this.pauseOnHide.has(name)) {
                continue;
            }
            clearTimeout(pending.id);
            this.pendingResumes.delete(name);
            this.pausedPollers.set(name, {
                callback: pending.callback,
                interval: pending.interval
            });
            suspended += 1;
        }

        if (suspended > 0) {
            console.debug(`Page hidden, suspended ${suspended} polling operation(s)`);
        }
    }

    /**
     * Restart opted-in pollers and refresh them immediately on focus.
     * @private
     */
    _resumePausedPollers() {
        if (this.pausedPollers.size === 0) {
            return;
        }

        let resumed = 0;
        for (const [name, info] of Array.from(this.pausedPollers.entries())) {
            if (this.isDestroyed) {
                break;
            }
            this.pausedPollers.delete(name);
            if (this.intervals.has(name) || this.pendingResumes.has(name)) {
                continue;
            }
            const intervalId = setInterval(info.callback, info.interval);
            this.intervals.set(name, {
                id: intervalId,
                callback: info.callback,
                interval: info.interval,
                startTime: Date.now()
            });
            // Évite un état périmé si la séquence a progressé pendant que la
            // fenêtre était masquée ; le tick suivant reprend le rythme normal.
            info.callback();
            resumed += 1;
        }

        if (resumed > 0) {
            console.debug(`Page visible, resumed ${resumed} polling operation(s)`);
        }
    }

    /**
     * Dispatch a custom event for polling errors.
     * @private
     */
    _dispatchPollingError(name, error, errorCount) {
        const event = new CustomEvent('pollingError', {
            detail: {
                name,
                error,
                errorCount,
                timestamp: new Date().toISOString()
            }
        });
        
        globalThis.dispatchEvent(event);
    }
}

// Create and export global polling manager instance
const pollingManager = new PollingManager();

// Export for use in other modules
export { PollingManager, pollingManager };
