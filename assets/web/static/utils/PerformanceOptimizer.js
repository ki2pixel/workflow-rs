/**
 * Performance Optimizer
 * Provides debouncing, throttling, and performance monitoring utilities.
 */

class PerformanceOptimizer {
    constructor() {
        this.debounceTimers = new Map();
        this.throttleTimers = new Map();
        this.performanceMetrics = {
            apiCalls: [],
            domUpdates: [],
            errors: []
        };
        this.isDestroyed = false;
        
        console.debug('[PerformanceOptimizer] Initialized');
    }
    
    /**
     * Debounce a function call.
     * @param {string} key - Unique key for the debounced function
     * @param {Function} func - Function to debounce
     * @param {number} delay - Delay in milliseconds
     * @param {boolean} [immediate] - Execute immediately on first call
     * @returns {Function} Debounced function
     */
    debounce(key, func, delay = 300, immediate = false) {
        return (...args) => {
            if (this.isDestroyed) return;
            
            const callNow = immediate && !this.debounceTimers.has(key);
            
            // Clear existing timer
            if (this.debounceTimers.has(key)) {
                clearTimeout(this.debounceTimers.get(key));
            }
            
            // Set new timer
            const timerId = setTimeout(() => {
                this.debounceTimers.delete(key);
                if (!immediate) func.apply(this, args);
            }, delay);
            
            this.debounceTimers.set(key, timerId);
            
            // Execute immediately if requested
            if (callNow) func.apply(this, args);
        };
    }
    
    /**
     * Throttle a function call.
     * @param {string} key - Unique key for the throttled function
     * @param {Function} func - Function to throttle
     * @param {number} interval - Minimum interval between calls in milliseconds
     * @returns {Function} Throttled function
     */
    throttle(key, func, interval = 100) {
        return (...args) => {
            if (this.isDestroyed) return;
            
            if (!this.throttleTimers.has(key)) {
                func.apply(this, args);
                
                this.throttleTimers.set(key, setTimeout(() => {
                    this.throttleTimers.delete(key);
                }, interval));
            }
        };
    }
    
    /**
     * Create a debounced update function for DOM elements.
     * @param {string} key - Unique key
     * @param {Function} updateFn - Update function
     * @param {number} [delay] - Debounce delay
     * @returns {Function} Debounced update function
     */
    debouncedUpdate(key, updateFn, delay = 100) {
        return this.debounce(`update_${key}`, updateFn, delay);
    }
    
    /**
     * Create a throttled scroll handler.
     * @param {string} key - Unique key
     * @param {Function} scrollFn - Scroll handler function
     * @param {number} [interval] - Throttle interval
     * @returns {Function} Throttled scroll handler
     */
    throttledScroll(key, scrollFn, interval = 16) { // ~60fps
        return this.throttle(`scroll_${key}`, scrollFn, interval);
    }
    
    /**
     * Create a throttled resize handler.
     * @param {string} key - Unique key
     * @param {Function} resizeFn - Resize handler function
     * @param {number} [interval] - Throttle interval
     * @returns {Function} Throttled resize handler
     */
    throttledResize(key, resizeFn, interval = 100) {
        return this.throttle(`resize_${key}`, resizeFn, interval);
    }
    
    /**
     * Measure and record API call performance.
     * @param {string} endpoint - API endpoint name
     * @param {Function} apiCall - Function that makes the API call
     * @returns {Promise} Promise that resolves with API call result
     */
    async measureApiCall(endpoint, apiCall) {
        const startTime = performance.now();
        let success = true;
        let error = null;
        
        try {
            const result = await apiCall();
            return result;
        } catch (err) {
            success = false;
            error = err;
            throw err;
        } finally {
            const duration = performance.now() - startTime;
            
            this.recordApiMetric({
                endpoint,
                duration,
                success,
                error: error ? error.message : null,
                timestamp: Date.now()
            });
        }
    }
    
    /**
     * Measure DOM update performance.
     * @param {string} operation - Operation name
     * @param {Function} updateFn - DOM update function
     * @returns {*} Result of update function
     */
    measureDomUpdate(operation, updateFn) {
        const startTime = performance.now();
        
        try {
            const result = updateFn();
            const duration = performance.now() - startTime;
            
            this.recordDomMetric({
                operation,
                duration,
                success: true,
                timestamp: Date.now()
            });
            
            return result;
        } catch (error) {
            const duration = performance.now() - startTime;
            
            this.recordDomMetric({
                operation,
                duration,
                success: false,
                error: error.message,
                timestamp: Date.now()
            });
            
            throw error;
        }
    }
    
    /**
     * Record API performance metric.
     * @param {Object} metric - Metric data
     */
    recordApiMetric(metric) {
        this.performanceMetrics.apiCalls.push(metric);
        
        // Keep only last 100 metrics
        if (this.performanceMetrics.apiCalls.length > 100) {
            this.performanceMetrics.apiCalls.shift();
        }
        
        // Log slow API calls
        if (metric.duration > 1000) {
            console.warn(`[PerformanceOptimizer] Slow API call: ${metric.endpoint} took ${metric.duration.toFixed(2)}ms`);
        }
    }
    
    /**
     * Record DOM update performance metric.
     * @param {Object} metric - Metric data
     */
    recordDomMetric(metric) {
        this.performanceMetrics.domUpdates.push(metric);
        
        // Keep only last 100 metrics
        if (this.performanceMetrics.domUpdates.length > 100) {
            this.performanceMetrics.domUpdates.shift();
        }
        
        // Log slow DOM updates
        if (metric.duration > 16) { // More than one frame
            console.warn(`[PerformanceOptimizer] Slow DOM update: ${metric.operation} took ${metric.duration.toFixed(2)}ms`);
        }
    }
    
    /**
     * Get performance statistics.
     * @returns {Object} Performance statistics
     */
    getPerformanceStats() {
        const apiCalls = this.performanceMetrics.apiCalls;
        const domUpdates = this.performanceMetrics.domUpdates;
        
        const apiStats = this.calculateStats(apiCalls.map(m => m.duration));
        const domStats = this.calculateStats(domUpdates.map(m => m.duration));
        
        return {
            api: {
                ...apiStats,
                totalCalls: apiCalls.length,
                errorRate: apiCalls.filter(m => !m.success).length / apiCalls.length,
                slowCalls: apiCalls.filter(m => m.duration > 1000).length
            },
            dom: {
                ...domStats,
                totalUpdates: domUpdates.length,
                errorRate: domUpdates.filter(m => !m.success).length / domUpdates.length,
                slowUpdates: domUpdates.filter(m => m.duration > 16).length
            },
            memory: this.getMemoryStats()
        };
    }
    
    /**
     * Calculate basic statistics for an array of numbers.
     * @param {number[]} values - Array of values
     * @returns {Object} Statistics object
     */
    calculateStats(values) {
        if (values.length === 0) {
            return { avg: 0, min: 0, max: 0, median: 0 };
        }
        
        const sorted = [...values].sort((a, b) => a - b);
        const sum = values.reduce((a, b) => a + b, 0);
        
        return {
            avg: sum / values.length,
            min: sorted[0],
            max: sorted[sorted.length - 1],
            median: sorted[Math.floor(sorted.length / 2)]
        };
    }
    
    /**
     * Get memory usage statistics (if available).
     * @returns {Object} Memory statistics
     */
    getMemoryStats() {
        if (performance.memory) {
            return {
                usedJSHeapSize: performance.memory.usedJSHeapSize,
                totalJSHeapSize: performance.memory.totalJSHeapSize,
                jsHeapSizeLimit: performance.memory.jsHeapSizeLimit,
                usagePercent: (performance.memory.usedJSHeapSize / performance.memory.jsHeapSizeLimit) * 100
            };
        }
        
        return { available: false };
    }
    
    /**
     * Clear all timers and reset metrics.
     */
    reset() {
        // Clear all debounce timers
        this.debounceTimers.forEach(timerId => clearTimeout(timerId));
        this.debounceTimers.clear();
        
        // Clear all throttle timers
        this.throttleTimers.forEach(timerId => clearTimeout(timerId));
        this.throttleTimers.clear();
        
        // Reset metrics
        this.performanceMetrics = {
            apiCalls: [],
            domUpdates: [],
            errors: []
        };
        
        console.debug('[PerformanceOptimizer] Reset completed');
    }
    
    /**
     * Get current timer counts.
     * @returns {Object} Timer counts
     */
    getTimerCounts() {
        return {
            debounceTimers: this.debounceTimers.size,
            throttleTimers: this.throttleTimers.size,
            totalTimers: this.debounceTimers.size + this.throttleTimers.size
        };
    }
    
    /**
     * Cancel specific timer.
     * @param {string} key - Timer key
     * @param {string} type - Timer type ('debounce' or 'throttle')
     * @returns {boolean} True if timer was cancelled
     */
    cancelTimer(key, type) {
        const timers = type === 'debounce' ? this.debounceTimers : this.throttleTimers;
        
        if (timers.has(key)) {
            clearTimeout(timers.get(key));
            timers.delete(key);
            return true;
        }
        
        return false;
    }
    
    /**
     * Destroy the optimizer and cleanup resources.
     */
    destroy() {
        this.reset();
        this.isDestroyed = true;
        
        console.debug('[PerformanceOptimizer] Destroyed');
    }
}

// Create and export singleton instance
export const performanceOptimizer = new PerformanceOptimizer();

// Cleanup on page unload
globalThis.addEventListener('beforeunload', () => {
    performanceOptimizer.destroy();
});

// Development helpers
if (globalThis.location.hostname === 'localhost' || globalThis.location.hostname === '127.0.0.1') {
    
    // Log performance stats periodically in development using PollingManager
    const startStatsPolling = () => {
        // Use dynamic import to avoid circular dependency at module eval time
        import('./PollingManager.js').then(({ pollingManager: pm }) => {
            pm.startPolling('perf-optimizer-stats', () => {
                const stats = performanceOptimizer.getPerformanceStats();
                if (stats.api.totalCalls > 0 || stats.dom.totalUpdates > 0) {
                    console.debug('[PerformanceOptimizer] Stats:', stats);
                }
            }, 30000);
        }).catch(() => {
            setTimeout(startStatsPolling, 1000);
        });
    };
    startStatsPolling();
}

export default performanceOptimizer;
