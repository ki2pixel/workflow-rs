/**
 * Scroll Manager Module
 * Handles automatic scrolling to active workflow steps with smooth animations
 * and intelligent viewport positioning.
 */

import * as dom from './domElements.js';
import { appState } from './state/AppState.js';

const SCROLL_CONFIG = {
    behavior: 'smooth',
    block: 'center',
    inline: 'nearest',
    topOffset: 100,
    minScrollDistance: 50,
    scrollDelay: 150,
    topbarHeight: 68, // Hauteur de la topbar depuis variables.css
    bottomMargin: 40   // Marge inférieure pour éviter le débordement
};

/**
 * Calculates the optimal scroll position for an element with perfect centering
 * @param {HTMLElement} element - The target element
 * @returns {number} The optimal scroll top position
 */
function calculateOptimalScrollPosition(element) {
    if (!element) return 0;
    
    const rect = element.getBoundingClientRect();
    const currentScrollTop = globalThis.pageYOffset || document.documentElement.scrollTop;
    const windowHeight = globalThis.innerHeight || document.documentElement.clientHeight;
    
    // Zone visible effective : topbar + margin inférieure
    const effectiveViewportHeight = windowHeight - SCROLL_CONFIG.topbarHeight - SCROLL_CONFIG.bottomMargin;
    const elementTop = rect.top + currentScrollTop;
    const elementHeight = rect.height;
    
    // Centrage agressif dans la zone effective (pas de contraintes min/max)
    const viewportCenter = SCROLL_CONFIG.topbarHeight + (effectiveViewportHeight / 2);
    const targetScrollTop = elementTop + (elementHeight / 2) - viewportCenter;
    
    // Contrainte simple : ne pas aller en négatif
    const finalScrollTop = Math.max(0, targetScrollTop);
    
    console.log('[SCROLL] Position calculation:', {
        elementTop,
        elementHeight,
        windowHeight,
        effectiveViewportHeight,
        viewportCenter,
        targetScrollTop,
        finalScrollTop
    });
    
    return finalScrollTop;
}

/**
 * Smoothly scrolls to bring the target element into optimal view
 * @param {HTMLElement} element - The element to scroll to
 * @param {Object} options - Additional scroll options
 */
function scrollToElement(element, options = {}) {
    if (!element) {
        console.warn('[SCROLL] No element provided for scrolling');
        return;
    }
    
    const config = { ...SCROLL_CONFIG, ...options };
    
    console.log(`[SCROLL] Scrolling to element: ${element.id || element.className}`);

    const behavior = config.behavior === 'smooth' ? 'smooth' : 'auto';
    const targetScrollTop = calculateOptimalScrollPosition(element);
    globalThis.scrollTo({
        top: targetScrollTop,
        behavior
    });
}

/**
 * Scrolls to the active workflow step with a delay to allow UI transitions
 * @param {string} stepKey - The key of the step to scroll to
 * @param {Object} options - Additional options for scrolling
 */
export function scrollToActiveStep(stepKey, options = {}) {
    if (!stepKey) {
        console.warn('[SCROLL] No stepKey provided for scrollToActiveStep');
        return;
    }
    
    const stepElement = document.getElementById(`step-${stepKey}`);
    if (!stepElement) {
        console.warn(`[SCROLL] Step element not found: step-${stepKey}`);
        return;
    }
    
    const config = { ...SCROLL_CONFIG, ...options };
    
    setTimeout(() => {
        scrollToElement(stepElement, config);
    }, config.scrollDelay);
}

/**
 * Scrolls to a step immediately without delay (for manual triggers)
 * @param {string} stepKey - The key of the step to scroll to
 * @param {Object} options - Additional options for scrolling
 */
export function scrollToStepImmediate(stepKey, options = {}) {
    if (!stepKey) return;
    
    const stepElement = document.getElementById(`step-${stepKey}`);
    if (!stepElement) return;
    
    scrollToElement(stepElement, { ...SCROLL_CONFIG, ...options });
}

/**
 * Checks if auto-scroll should be enabled based on user preferences and context
 * @returns {boolean} True if auto-scroll should be active
 */
export function isAutoScrollEnabled() {
    const userPreference = appState.getStateProperty('ui.autoScroll');
    if (typeof userPreference === 'boolean') {
        return userPreference;
    }

    const isLogsActive = dom.workflowWrapper && dom.workflowWrapper.classList.contains('logs-active');
    return isLogsActive;
}

export function setAutoScrollEnabled(enabled) {
    const currentUI = appState.getStateProperty('ui') || {};
    appState.setState({ ui: { ...currentUI, autoScroll: !!enabled } }, 'setAutoScrollEnabled');
    console.log(`[SCROLL] Auto-scroll ${enabled ? 'enabled' : 'disabled'}`);
}

export function isSequenceAutoScrollEnabled() {
    const sequencePreference = appState.getStateProperty('ui.sequenceAutoScroll');
    if (typeof sequencePreference === 'boolean') {
        return sequencePreference;
    }
    
    // Par défaut, activer l'auto-scroll pour les séquences
    return true;
}

export function setSequenceAutoScrollEnabled(enabled) {
    const currentUI = appState.getStateProperty('ui') || {};
    appState.setState({ ui: { ...currentUI, sequenceAutoScroll: !!enabled } }, 'setSequenceAutoScrollEnabled');
    console.log(`[SCROLL] Sequence auto-scroll ${enabled ? 'enabled' : 'disabled'}`);
}

export { SCROLL_CONFIG };
