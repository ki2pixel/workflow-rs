/**
 * DOM Update Batcher
 * Batches DOM updates using requestAnimationFrame for optimal performance.
 */

class DOMUpdateBatcher {
    constructor() {
        this.pendingUpdates = new Map();
        this.rafId = null;
        this.isDestroyed = false;
        this.updateCount = 0;
        this.lastFlushTime = 0;
        
        this.stats = {
            totalUpdates: 0,
            batchedUpdates: 0,
            averageBatchSize: 0,
            lastBatchSize: 0
        };
        
        console.debug('[DOMBatcher] Initialized');
    }
    
    /**
     * Schedule a DOM update to be batched.
     * @param {string} key - Unique key for the update (prevents duplicates)
     * @param {Function} updateFn - Function that performs DOM updates
     * @param {number} [priority] - Update priority (lower = higher priority)
     */
    scheduleUpdate(key, updateFn, priority = 0) {
        if (this.isDestroyed) {
            console.warn('[DOMBatcher] Attempted to schedule update on destroyed batcher');
            return;
        }
        
        if (typeof updateFn !== 'function') {
            console.error('[DOMBatcher] Update function must be a function');
            return;
        }
        
        this.pendingUpdates.set(key, {
            updateFn,
            priority,
            timestamp: performance.now()
        });
        
        this.updateCount++;
        
        if (!this.rafId) {
            this.rafId = requestAnimationFrame(() => {
                this.flushUpdates();
            });
        }
    }
    
    /**
     * Schedule multiple related updates as a group.
     * @param {string} groupKey - Key for the update group
     * @param {Object} updates - Object with update keys and functions
     * @param {number} [priority] - Priority for the entire group
     */
    scheduleUpdateGroup(groupKey, updates, priority = 0) {
        Object.entries(updates).forEach(([key, updateFn]) => {
            this.scheduleUpdate(`${groupKey}:${key}`, updateFn, priority);
        });
    }
    
    /**
     * Schedule a high-priority update that should be processed first.
     * @param {string} key - Unique key for the update
     * @param {Function} updateFn - Function that performs DOM updates
     */
    scheduleHighPriorityUpdate(key, updateFn) {
        this.scheduleUpdate(key, updateFn, -1);
    }
    
    /**
     * Cancel a scheduled update.
     * @param {string} key - Key of the update to cancel
     * @returns {boolean} True if update was cancelled
     */
    cancelUpdate(key) {
        return this.pendingUpdates.delete(key);
    }
    
    /**
     * Cancel all updates matching a pattern.
     * @param {RegExp|string} pattern - Pattern to match against keys
     * @returns {number} Number of cancelled updates
     */
    cancelUpdatesMatching(pattern) {
        let cancelled = 0;
        const regex = pattern instanceof RegExp ? pattern : new RegExp(pattern);
        
        for (const key of this.pendingUpdates.keys()) {
            if (regex.test(key)) {
                this.pendingUpdates.delete(key);
                cancelled++;
            }
        }
        
        return cancelled;
    }
    
    /**
     * Force immediate flush of all pending updates.
     */
    flushUpdates() {
        if (this.isDestroyed || this.pendingUpdates.size === 0) {
            this.rafId = null;
            return;
        }
        
        const startTime = performance.now();
        const batchSize = this.pendingUpdates.size;
        
        try {
            const sortedUpdates = Array.from(this.pendingUpdates.entries())
                .sort(([, a], [, b]) => a.priority - b.priority);
            
            for (const [key, { updateFn }] of sortedUpdates) {
                try {
                    updateFn();
                } catch (error) {
                    console.error(`[DOMBatcher] Update error for key "${key}":`, error);
                }
            }
            
            this.stats.totalUpdates += batchSize;
            this.stats.batchedUpdates++;
            this.stats.lastBatchSize = batchSize;
            this.stats.averageBatchSize = this.stats.totalUpdates / this.stats.batchedUpdates;
            
            const flushTime = performance.now() - startTime;
            this.lastFlushTime = flushTime;
            
            if (flushTime > 16) { // More than one frame
                console.warn(`[DOMBatcher] Slow batch update: ${flushTime.toFixed(2)}ms for ${batchSize} updates`);
            }
            
        } catch (error) {
            console.error('[DOMBatcher] Flush error:', error);
        } finally {
            this.pendingUpdates.clear();
            this.rafId = null;
        }
    }
    
    /**
     * Get current statistics about batching performance.
     * @returns {Object} Statistics object
     */
    getStats() {
        return {
            ...this.stats,
            pendingUpdates: this.pendingUpdates.size,
            isScheduled: this.rafId !== null,
            lastFlushTime: this.lastFlushTime,
            updateCount: this.updateCount
        };
    }
    
    /**
     * Reset statistics.
     */
    resetStats() {
        this.stats = {
            totalUpdates: 0,
            batchedUpdates: 0,
            averageBatchSize: 0,
            lastBatchSize: 0
        };
        this.updateCount = 0;
        this.lastFlushTime = 0;
        
        console.debug('[DOMBatcher] Statistics reset');
    }
    
    /**
     * Check if there are pending updates.
     * @returns {boolean} True if updates are pending
     */
    hasPendingUpdates() {
        return this.pendingUpdates.size > 0;
    }
    
    /**
     * Get list of pending update keys.
     * @returns {string[]} Array of pending update keys
     */
    getPendingUpdateKeys() {
        return Array.from(this.pendingUpdates.keys());
    }
    
    /**
     * Destroy the batcher and cleanup resources.
     */
    destroy() {
        if (this.rafId) {
            cancelAnimationFrame(this.rafId);
            this.rafId = null;
        }
        
        this.pendingUpdates.clear();
        this.isDestroyed = true;
        
        console.debug('[DOMBatcher] Destroyed');
    }
}

/**
 * Performance-optimized DOM update utilities.
 */
class DOMUpdateUtils {
    /**
     * Batch update multiple element properties.
     * @param {HTMLElement} element - Target element
     * @param {Object} properties - Properties to update
     */
    static updateElementProperties(element, properties) {
        if (!element) return;
        
        if (properties.style) {
            Object.assign(element.style, properties.style);
        }
        
        if (properties.attributes) {
            Object.entries(properties.attributes).forEach(([attr, value]) => {
                if (value === null || value === undefined) {
                    element.removeAttribute(attr);
                } else {
                    element.setAttribute(attr, value);
                }
            });
        }
        
        Object.entries(properties).forEach(([prop, value]) => {
            if (prop !== 'style' && prop !== 'attributes' && prop in element) {
                element[prop] = value;
            }
        });
    }
    
    /**
     * Efficiently update text content with HTML escaping.
     * @param {HTMLElement} element - Target element
     * @param {string} text - Text content
     * @param {boolean} [escapeHtml] - Whether to escape HTML
     */
    static updateTextContent(element, text, escapeHtml = true) {
        if (!element) return;
        
        const content = escapeHtml ? DOMUpdateUtils.escapeHtml(text) : text;
        
        if (escapeHtml) {
            element.textContent = content;
        } else {
            element.innerHTML = content;
        }
    }
    
    /**
     * Escape HTML characters.
     * @param {string} text - Text to escape
     * @returns {string} Escaped text
     */
    static escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }
    
    /**
     * Update progress bar with animation.
     * @param {HTMLElement} progressBar - Progress bar element
     * @param {number} percentage - Progress percentage (0-100)
     * @param {boolean} [animate] - Whether to animate the change
     */
    static updateProgressBar(progressBar, percentage, animate = true) {
        if (!progressBar) return;
        
        const clampedPercentage = Math.max(0, Math.min(100, percentage));
        
        if (animate) {
            progressBar.style.transition = 'width 0.3s ease-in-out';
        } else {
            progressBar.style.transition = 'none';
        }
        
        progressBar.style.width = `${clampedPercentage}%`;
        progressBar.setAttribute('aria-valuenow', clampedPercentage);
        
        const textElement = progressBar.querySelector('.progress-text');
        if (textElement) {
            textElement.textContent = `${Math.round(clampedPercentage)}%`;
        }
    }
    
    /**
     * Update element visibility with optional animation.
     * @param {HTMLElement} element - Target element
     * @param {boolean} visible - Whether element should be visible
     * @param {string} [animation] - Animation type ('fade', 'slide', 'none')
     */
    static updateVisibility(element, visible, animation = 'none') {
        if (!element) return;
        
        if (animation === 'fade') {
            element.style.transition = 'opacity 0.3s ease-in-out';
            element.style.opacity = visible ? '1' : '0';
            element.style.display = visible ? '' : 'none';
        } else if (animation === 'slide') {
            element.style.transition = 'max-height 0.3s ease-in-out';
            element.style.maxHeight = visible ? '1000px' : '0';
            element.style.overflow = 'hidden';
        } else {
            element.style.display = visible ? '' : 'none';
        }
    }
}

// Create and export singleton instance
export const domBatcher = new DOMUpdateBatcher();

// Cleanup on page unload
globalThis.addEventListener('beforeunload', () => {
    domBatcher.destroy();
});

// Export utilities
export { DOMUpdateUtils };

// Development helpers
if (globalThis.location.hostname === 'localhost' || globalThis.location.hostname === '127.0.0.1') {
    
    const originalFlush = domBatcher.flushUpdates;
    domBatcher.flushUpdates = function() {
        const startTime = performance.now();
        originalFlush.call(this);
        const duration = performance.now() - startTime;
        
        if (duration > 16) {
            console.warn(`[DOMBatcher] Performance warning: ${duration.toFixed(2)}ms flush time`);
        }
    };
}

export default domBatcher;
