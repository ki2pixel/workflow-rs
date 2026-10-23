/**
 * Comprehensive error handling utility for workflow_mediapipe frontend.
 * 
 * This module provides centralized error handling with user feedback,
 * exponential backoff for repeated errors, and proper error reporting.
 */

import { showNotification } from '../utils.js';

class ErrorHandler {
    constructor() {
        this.consecutiveErrors = new Map();
        this.errorHistory = [];
        this.maxHistorySize = 100;
        this.notificationTimeouts = new Map();
        
        this._bindGlobalErrorHandlers();
        
        console.debug('ErrorHandler initialized');
    }

    /**
     * Handle polling errors with exponential backoff and user feedback.
     * 
     * @param {string} operation - Name of the operation that failed
     * @param {Error} error - The error that occurred
     * @param {Object} context - Additional context information
     * @returns {number} - Delay in milliseconds before retry (0 = no delay)
     */
    async handlePollingError(operation, error, context = {}) {
        const errorKey = `${operation}_${context.stepKey || 'global'}`;
        const count = this.consecutiveErrors.get(errorKey) || 0;
        const newCount = count + 1;
        
        this.consecutiveErrors.set(errorKey, newCount);
        
        console.error(`Polling error in ${operation} (attempt ${newCount}):`, error);
        
        this._addToHistory({
            operation,
            error: error.message || error.toString(),
            context,
            count: newCount,
            timestamp: new Date().toISOString()
        });
        
        if (newCount >= 3) {
            this._showErrorNotification(
                operation,
                `Impossible de mettre à jour ${operation}. Nouvelle tentative...`,
                'warning',
                context.elementId
            );
        }
        
        if (newCount >= 5) {
            this._showErrorNotification(
                operation,
                `${operation} rencontre des problèmes persistants. Veuillez vérifier votre connexion.`,
                'error',
                context.elementId
            );
        }
        
        let delay = 0;
        if (newCount >= 3) {
            delay = Math.min(30000, 2000 * Math.pow(2, newCount - 3));
        }
        
        this._dispatchErrorEvent(operation, error, newCount, delay);
        
        return delay;
    }

    /**
     * Clear error state for a successful operation.
     * 
     * @param {string} operation - Name of the operation that succeeded
     * @param {Object} context - Additional context information
     */
    clearErrors(operation, context = {}) {
        const errorKey = `${operation}_${context.stepKey || 'global'}`;
        const hadErrors = this.consecutiveErrors.has(errorKey);
        
        this.consecutiveErrors.delete(errorKey);
        
        if (hadErrors) {
            console.debug(`Cleared error state for ${operation}`);
            this._clearErrorNotification(operation);
            
            if (context.elementId) {
                this.clearErrorState(context.elementId);
            }
        }
    }

    /**
     * Handle API errors with proper user feedback.
     * 
     * @param {string} endpoint - API endpoint that failed
     * @param {Error} error - The error that occurred
     * @param {Object} context - Additional context information
     */
    handleApiError(endpoint, error, context = {}) {
        console.error(`API error for ${endpoint}:`, error);
        
        this._addToHistory({
            type: 'api',
            endpoint,
            error: error.message || error.toString(),
            context,
            timestamp: new Date().toISOString()
        });
        
        let message = 'Une erreur inattendue est survenue';
        let type = 'error';
        
        if (error.name === 'TypeError' && error.message.includes('fetch')) {
            message = 'Erreur de connexion réseau. Veuillez vérifier votre connexion Internet.';
            type = 'warning';
        } else if (error.message.includes('401')) {
            message = 'Erreur d\'authentification. Veuillez rafraîchir la page.';
            type = 'error';
        } else if (error.message.includes('404')) {
            message = 'Service non trouvé. Veuillez contacter le support.';
            type = 'error';
        } else if (error.message.includes('500')) {
            message = 'Erreur serveur. Veuillez réessayer plus tard.';
            type = 'error';
        } else if (error.message) {
            message = error.message;
        }
        
        this._showErrorNotification(
            `api-${endpoint}`,
            message,
            type
        );
        
        this._dispatchErrorEvent(`api-${endpoint}`, error, 1, 0);
    }

    /**
     * Show error state on a specific UI element.
     * 
     * @param {string} elementId - ID of the element to show error state
     * @param {string} message - Error message to display
     */
    showErrorState(elementId, message) {
        const element = document.getElementById(elementId);
        if (!element) {
            console.warn(`Element not found for error state: ${elementId}`);
            return;
        }
        
        element.classList.add('error-state');
        
        let errorElement = element.querySelector('.error-message');
        if (!errorElement) {
            errorElement = document.createElement('div');
            errorElement.className = 'error-message';
            element.appendChild(errorElement);
        }
        
        errorElement.textContent = message;
        errorElement.style.display = 'block';
        
        console.debug(`Showing error state for ${elementId}: ${message}`);
    }

    /**
     * Clear error state from a specific UI element.
     * 
     * @param {string} elementId - ID of the element to clear error state
     */
    clearErrorState(elementId) {
        const element = document.getElementById(elementId);
        if (!element) {
            return;
        }
        
        element.classList.remove('error-state');
        
        // Hide error message
        const errorElement = element.querySelector('.error-message');
        if (errorElement) {
            errorElement.style.display = 'none';
        }
        
        console.debug(`Cleared error state for ${elementId}`);
    }

    /**
     * Get error statistics for monitoring.
     * 
     * @returns {Object} - Error statistics
     */
    getErrorStats() {
        const now = Date.now();
        const recentErrors = this.errorHistory.filter(
            error => (now - new Date(error.timestamp).getTime()) < 300000
        );
        
        return {
            totalErrors: this.errorHistory.length,
            recentErrors: recentErrors.length,
            activeErrorOperations: Array.from(this.consecutiveErrors.keys()),
            errorsByOperation: this._groupErrorsByOperation(recentErrors)
        };
    }

    /**
     * Clear all error history and state.
     */
    clearAllErrors() {
        this.consecutiveErrors.clear();
        this.errorHistory = [];
        
        this.notificationTimeouts.forEach((timeout, key) => {
            clearTimeout(timeout);
            this._clearErrorNotification(key);
        });
        this.notificationTimeouts.clear();
        
        console.debug('Cleared all error state');
    }

    /**
     * Add error to history with size limit.
     * @private
     */
    _addToHistory(errorInfo) {
        this.errorHistory.push(errorInfo);
        
        // Maintain history size limit
        if (this.errorHistory.length > this.maxHistorySize) {
            this.errorHistory.shift();
        }
    }

    /**
     * Show error notification with deduplication.
     * @private
     */
    _showErrorNotification(key, message, type = 'error', elementId = null) {
        const existingTimeout = this.notificationTimeouts.get(key);
        if (existingTimeout) {
            clearTimeout(existingTimeout);
        }

        showNotification(message, type);

        if (elementId) {
            this.showErrorState(elementId, message);
        }
        
        const timeout = setTimeout(() => {
            this._clearErrorNotification(key);
        }, 10000);
        
        this.notificationTimeouts.set(key, timeout);
    }

    /**
     * Clear error notification.
     * @private
     */
    _clearErrorNotification(key) {
        const timeout = this.notificationTimeouts.get(key);
        if (timeout) {
            clearTimeout(timeout);
            this.notificationTimeouts.delete(key);
        }
    }

    /**
     * Dispatch custom error event.
     * @private
     */
    _dispatchErrorEvent(operation, error, count, delay) {
        const event = new CustomEvent('applicationError', {
            detail: {
                operation,
                error: error.message || error.toString(),
                count,
                delay,
                timestamp: new Date().toISOString()
            }
        });
        
        globalThis.dispatchEvent(event);
    }

    /**
     * Group errors by operation for statistics.
     * @private
     */
    _groupErrorsByOperation(errors) {
        const grouped = {};
        
        errors.forEach(error => {
            const operation = error.operation || error.endpoint || 'unknown';
            if (!grouped[operation]) {
                grouped[operation] = 0;
            }
            grouped[operation]++;
        });
        
        return grouped;
    }

    /**
     * Bind global error handlers.
     * @private
     */
    _bindGlobalErrorHandlers() {
        globalThis.addEventListener('unhandledrejection', (event) => {
            console.error('Unhandled promise rejection:', event.reason);
            this.handleApiError('unhandled-promise', event.reason);
        });
        
        globalThis.addEventListener('error', (event) => {
            console.error('Global JavaScript error:', event.error);
            this._addToHistory({
                type: 'javascript',
                error: event.error?.message || event.message,
                filename: event.filename,
                lineno: event.lineno,
                colno: event.colno,
                timestamp: new Date().toISOString()
            });
        });
    }
}

// Create and export global error handler instance
const errorHandler = new ErrorHandler();

// Export for use in other modules
export { ErrorHandler, errorHandler };
