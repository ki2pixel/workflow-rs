/**
 * Frontend Performance Monitor
 * Monitors and reports frontend performance metrics.
 */

import { performanceOptimizer } from './PerformanceOptimizer.js';
import { domBatcher } from './DOMBatcher.js';
import { pollingManager } from './PollingManager.js';
import { errorHandler } from './ErrorHandler.js';

class PerformanceMonitor {
    constructor() {
        this.isMonitoring = false;
        this.metrics = {
            pageLoad: null,
            apiCalls: [],
            domUpdates: [],
            memoryUsage: [],
            userInteractions: []
        };
        this.observers = new Map();
        this.startTime = performance.now();
        
        console.debug('[PerformanceMonitor] Initialized');
    }
    
    /**
     * Start performance monitoring.
     */
    startMonitoring() {
        if (this.isMonitoring) {
            console.warn('[PerformanceMonitor] Already monitoring');
            return;
        }
        
        this.isMonitoring = true;
        
        // Monitor page load performance
        this.monitorPageLoad();
        
        // Monitor API calls
        this.monitorApiCalls();
        
        // Monitor DOM updates
        this.monitorDomUpdates();
        
        // Monitor memory usage
        this.monitorMemoryUsage();
        
        // Monitor user interactions
        this.monitorUserInteractions();
        
        // Monitor long tasks
        this.monitorLongTasks();
        
        // Start periodic reporting
        this.startPeriodicReporting();
        
        console.info('[PerformanceMonitor] Monitoring started');
    }
    
    /**
     * Stop performance monitoring.
     */
    stopMonitoring() {
        if (!this.isMonitoring) return;
        
        this.isMonitoring = false;
        
        // Disconnect all observers
        this.observers.forEach(observer => {
            if (observer.disconnect) observer.disconnect();
        });
        this.observers.clear();
        
        // Stop periodic reporting
        if (this.reportingInterval) {
            pollingManager.stopPolling(this.reportingInterval);
            this.reportingInterval = null;
        }
        
        // Restore original fetch
        if (this.originalFetch) {
            globalThis.fetch = this.originalFetch;
            this.originalFetch = null;
        }
        
        console.info('[PerformanceMonitor] Monitoring stopped');
    }
    
    /**
     * Monitor page load performance.
     */
    monitorPageLoad() {
        if (document.readyState === 'complete') {
            this.recordPageLoadMetrics();
        } else {
            globalThis.addEventListener('load', () => {
                this.recordPageLoadMetrics();
            });
        }
    }
    
    /**
     * Record page load metrics.
     */
    recordPageLoadMetrics() {
        const navigation = performance.getEntriesByType('navigation')[0];
        if (!navigation) return;
        
        this.metrics.pageLoad = {
            domContentLoaded: navigation.domContentLoadedEventEnd - navigation.domContentLoadedEventStart,
            loadComplete: navigation.loadEventEnd - navigation.loadEventStart,
            domInteractive: navigation.domInteractive - navigation.navigationStart,
            firstPaint: this.getFirstPaint(),
            firstContentfulPaint: this.getFirstContentfulPaint(),
            timestamp: Date.now()
        };
        
        console.debug('[PerformanceMonitor] Page load metrics recorded:', this.metrics.pageLoad);
    }
    
    /**
     * Get First Paint timing.
     * @returns {number|null} First Paint time or null
     */
    getFirstPaint() {
        const paintEntries = performance.getEntriesByType('paint');
        const firstPaint = paintEntries.find(entry => entry.name === 'first-paint');
        return firstPaint ? firstPaint.startTime : null;
    }
    
    /**
     * Get First Contentful Paint timing.
     * @returns {number|null} First Contentful Paint time or null
     */
    getFirstContentfulPaint() {
        const paintEntries = performance.getEntriesByType('paint');
        const fcp = paintEntries.find(entry => entry.name === 'first-contentful-paint');
        return fcp ? fcp.startTime : null;
    }
    
    /**
     * Monitor API calls by wrapping fetch.
     */
    monitorApiCalls() {
        if (!this.originalFetch) {
            this.originalFetch = globalThis.fetch.bind(globalThis);
        }
        
        globalThis.fetch = async (...args) => {
            const startTime = performance.now();
            const url = args[0];
            
            try {
                const response = await this.originalFetch(...args);
                const duration = performance.now() - startTime;
                
                this.recordApiCall({
                    url,
                    method: args[1]?.method || 'GET',
                    status: response.status,
                    duration,
                    success: response.ok,
                    timestamp: Date.now()
                });
                
                return response;
            } catch (error) {
                const duration = performance.now() - startTime;
                
                this.recordApiCall({
                    url,
                    method: args[1]?.method || 'GET',
                    status: 0,
                    duration,
                    success: false,
                    error: error.message,
                    timestamp: Date.now()
                });
                
                throw error;
            }
        };
    }
    
    /**
     * Record API call metrics.
     * @param {Object} metric - API call metric
     */
    recordApiCall(metric) {
        this.metrics.apiCalls.push(metric);
        
        // Keep only last 50 API calls
        if (this.metrics.apiCalls.length > 50) {
            this.metrics.apiCalls.shift();
        }
        
        // Log slow API calls
        if (metric.duration > 1000) {
            console.warn(`[PerformanceMonitor] Slow API call: ${metric.url} took ${metric.duration.toFixed(2)}ms`);
        }
    }
    
    /**
     * Monitor DOM updates using MutationObserver.
     */
    monitorDomUpdates() {
        if (!globalThis.MutationObserver) return;
        
        const observer = new MutationObserver((mutations) => {
            const updateCount = mutations.length;
            const timestamp = Date.now();
            
            this.recordDomUpdate({
                mutationCount: updateCount,
                timestamp
            });
        });
        
        observer.observe(document.body, {
            childList: true,
            subtree: true,
            attributes: true,
            attributeOldValue: false,
            characterData: true,
            characterDataOldValue: false
        });
        
        this.observers.set('domUpdates', observer);
    }
    
    /**
     * Record DOM update metrics.
     * @param {Object} metric - DOM update metric
     */
    recordDomUpdate(metric) {
        this.metrics.domUpdates.push(metric);
        
        // Keep only last 100 DOM updates
        if (this.metrics.domUpdates.length > 100) {
            this.metrics.domUpdates.shift();
        }
    }
    
    /**
     * Monitor memory usage periodically.
     */
    monitorMemoryUsage() {
        if (!performance.memory) return;
        
        const recordMemory = () => {
            if (!this.isMonitoring) return;
            
            const memory = performance.memory;
            this.metrics.memoryUsage.push({
                usedJSHeapSize: memory.usedJSHeapSize,
                totalJSHeapSize: memory.totalJSHeapSize,
                jsHeapSizeLimit: memory.jsHeapSizeLimit,
                usagePercent: (memory.usedJSHeapSize / memory.jsHeapSizeLimit) * 100,
                timestamp: Date.now()
            });
            
            // Keep only last 20 memory measurements
            if (this.metrics.memoryUsage.length > 20) {
                this.metrics.memoryUsage.shift();
            }
            
            // Check for memory leaks
            const currentUsage = memory.usedJSHeapSize / memory.jsHeapSizeLimit;
            if (currentUsage > 0.8) {
                console.warn(`[PerformanceMonitor] High memory usage: ${(currentUsage * 100).toFixed(1)}%`);
            }
        };
        
        // Record immediately and then every 10 seconds
        recordMemory();
        pollingManager.startPolling('perf-memory-usage', recordMemory, 10000);
        this.observers.set('memoryUsage', { disconnect: () => pollingManager.stopPolling('perf-memory-usage') });
    }
    
    /**
     * Monitor user interactions.
     */
    monitorUserInteractions() {
        const interactionTypes = ['click', 'keydown', 'scroll', 'resize'];
        
        interactionTypes.forEach(type => {
            const handler = performanceOptimizer.throttle(`interaction_${type}`, (event) => {
                this.recordUserInteraction({
                    type,
                    target: event.target.tagName,
                    timestamp: Date.now()
                });
            }, 100);
            
            document.addEventListener(type, handler, { passive: true });
            
            this.observers.set(`interaction_${type}`, {
                disconnect: () => document.removeEventListener(type, handler)
            });
        });
    }
    
    /**
     * Record user interaction metrics.
     * @param {Object} metric - User interaction metric
     */
    recordUserInteraction(metric) {
        this.metrics.userInteractions.push(metric);
        
        // Keep only last 50 interactions
        if (this.metrics.userInteractions.length > 50) {
            this.metrics.userInteractions.shift();
        }
    }
    
    /**
     * Monitor long tasks using PerformanceObserver.
     */
    monitorLongTasks() {
        if (!globalThis.PerformanceObserver) return;
        
        try {
            const observer = new PerformanceObserver((list) => {
                for (const entry of list.getEntries()) {
                    if (entry.duration > 50) { // Tasks longer than 50ms
                        console.warn(`[PerformanceMonitor] Long task detected: ${entry.duration.toFixed(2)}ms`);
                        
                        this.recordLongTask({
                            duration: entry.duration,
                            startTime: entry.startTime,
                            timestamp: Date.now()
                        });
                    }
                }
            });
            
            observer.observe({ entryTypes: ['longtask'] });
            this.observers.set('longTasks', observer);
            
        } catch (error) {
            console.debug('[PerformanceMonitor] Long task monitoring not supported');
        }
    }
    
    /**
     * Record long task metrics.
     * @param {Object} metric - Long task metric
     */
    recordLongTask(metric) {
        if (!this.metrics.longTasks) {
            this.metrics.longTasks = [];
        }
        
        this.metrics.longTasks.push(metric);
        
        // Keep only last 20 long tasks
        if (this.metrics.longTasks.length > 20) {
            this.metrics.longTasks.shift();
        }
    }
    
    /**
     * Start periodic performance reporting.
     */
    startPeriodicReporting() {
        pollingManager.startPolling('perf-periodic-reporting', () => this.sendPerformanceReport(), 60000);
        this.reportingInterval = 'perf-periodic-reporting';
    }
    
    /**
     * Send performance report to backend.
     */
    async sendPerformanceReport() {
        if (!this.isMonitoring) return;
        
        try {
            const report = this.generatePerformanceReport();
            
            // Send to backend performance API
            await fetch('/api/performance/frontend', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify(report)
            });
            
        } catch (error) {
            console.debug('[PerformanceMonitor] Failed to send performance report:', error);
        }
    }
    
    /**
     * Generate comprehensive performance report.
     * @returns {Object} Performance report
     */
    generatePerformanceReport() {
        const optimizerStats = performanceOptimizer.getPerformanceStats();
        const batcherStats = domBatcher.getStats();
        const pollingStats = pollingManager.getStats();
        const errorStats = errorHandler.getStats();
        
        return {
            timestamp: Date.now(),
            uptime: performance.now() - this.startTime,
            pageLoad: this.metrics.pageLoad,
            apiCalls: this.getApiCallSummary(),
            domUpdates: this.getDomUpdateSummary(),
            memoryUsage: this.getMemoryUsageSummary(),
            userInteractions: this.getUserInteractionSummary(),
            longTasks: this.metrics.longTasks || [],
            components: {
                optimizer: optimizerStats,
                batcher: batcherStats,
                polling: pollingStats,
                errorHandler: errorStats
            }
        };
    }
    
    /**
     * Get API call summary.
     * @returns {Object} API call summary
     */
    getApiCallSummary() {
        const calls = this.metrics.apiCalls;
        if (calls.length === 0) return { count: 0 };
        
        const durations = calls.map(c => c.duration);
        const errors = calls.filter(c => !c.success);
        
        return {
            count: calls.length,
            averageDuration: durations.reduce((a, b) => a + b, 0) / durations.length,
            maxDuration: Math.max(...durations),
            errorRate: errors.length / calls.length,
            slowCalls: calls.filter(c => c.duration > 1000).length
        };
    }
    
    /**
     * Get DOM update summary.
     * @returns {Object} DOM update summary
     */
    getDomUpdateSummary() {
        const updates = this.metrics.domUpdates;
        return {
            count: updates.length,
            totalMutations: updates.reduce((sum, u) => sum + u.mutationCount, 0)
        };
    }
    
    /**
     * Get memory usage summary.
     * @returns {Object} Memory usage summary
     */
    getMemoryUsageSummary() {
        const usage = this.metrics.memoryUsage;
        if (usage.length === 0) return { available: false };
        
        const latest = usage[usage.length - 1];
        const usagePercents = usage.map(u => u.usagePercent);
        
        return {
            current: latest,
            average: usagePercents.reduce((a, b) => a + b, 0) / usagePercents.length,
            peak: Math.max(...usagePercents)
        };
    }
    
    /**
     * Get user interaction summary.
     * @returns {Object} User interaction summary
     */
    getUserInteractionSummary() {
        const interactions = this.metrics.userInteractions;
        const byType = {};
        
        interactions.forEach(interaction => {
            byType[interaction.type] = (byType[interaction.type] || 0) + 1;
        });
        
        return {
            total: interactions.length,
            byType
        };
    }
    
    /**
     * Get current performance metrics.
     * @returns {Object} Current metrics
     */
    getMetrics() {
        return this.generatePerformanceReport();
    }
    
    /**
     * Reset all metrics.
     */
    resetMetrics() {
        this.metrics = {
            pageLoad: null,
            apiCalls: [],
            domUpdates: [],
            memoryUsage: [],
            userInteractions: []
        };
        
        console.debug('[PerformanceMonitor] Metrics reset');
    }
    
    /**
     * Destroy the monitor and cleanup resources.
     */
    destroy() {
        this.stopMonitoring();
        this.resetMetrics();
        
        console.debug('[PerformanceMonitor] Destroyed');
    }
}

// Create and export singleton instance
export const performanceMonitor = new PerformanceMonitor();

// Auto-start monitoring when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', () => {
        performanceMonitor.startMonitoring();
    });
} else {
    performanceMonitor.startMonitoring();
}

// Cleanup on page unload
globalThis.addEventListener('beforeunload', () => {
    performanceMonitor.destroy();
});

export default performanceMonitor;
