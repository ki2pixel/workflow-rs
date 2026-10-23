/**
 * Theme Manager - Workflow MediaPipe v4.0
 * Handles dynamic theme switching with AppState persistence
 */
import { appState } from './state/AppState.js';

const THEME_STORAGE_KEY = 'workflow-theme-preference';

const THEMES = {
    'dark-pro': {
        name: 'Dark Pro',
        description: 'Professional dark theme (default)'
    },
    'light-mode': {
        name: 'Light Mode',
        description: 'Clean and bright professional theme'
    },
    'pastel-zen': {
        name: 'Pastel Zen',
        description: 'Soft and calming pastel colors'
    },
    'neon-cyberpunk': {
        name: 'Neon Cyberpunk',
        description: 'Vibrant futuristic theme with glows'
    },
    'forest-night': {
        name: 'Forest Night',
        description: 'Natural earthy tones'
    },
    'ocean-depth': {
        name: 'Ocean Depth',
        description: 'Deep blue oceanic theme'
    }
};

class ThemeManager {
    constructor() {
        this.currentTheme = this.loadTheme();
        this.themeSelector = null;
    }

    /**
     * Initialize the theme system
     */
    init() {
        console.log('[ThemeManager] Initializing theme system...');
        
        // Apply saved theme immediately
        this.applyTheme(this.currentTheme);
        
        // Setup theme selector if it exists
        this.setupThemeSelector();
        
        console.log(`[ThemeManager] Theme system initialized with: ${this.currentTheme}`);
    }

    /**
     * Load theme preference from localStorage
     * @returns {string} Theme identifier
     */
    loadTheme() {
        try {
            const savedTheme = appState.getStateProperty('ui.theme');
            if (savedTheme && THEMES[savedTheme]) {
                console.log(`[ThemeManager] Loaded saved theme: ${savedTheme}`);
                return savedTheme;
            }
        } catch (error) {
            console.error('[ThemeManager] Error loading theme from AppState:', error);
        }
        
        // Default to dark-pro
        return 'dark-pro';
    }

    /**
     * Save theme preference to AppState
     * @param {string} themeId - Theme identifier
     */
    saveTheme(themeId) {
        try {
            const currentUI = appState.getStateProperty('ui') || {};
            appState.setState({ ui: { ...currentUI, theme: themeId } }, 'saveTheme');
            console.log(`[ThemeManager] Saved theme preference: ${themeId}`);
        } catch (error) {
            console.error('[ThemeManager] Error saving theme to AppState:', error);
        }
    }

    /**
     * Apply a theme to the document
     * @param {string} themeId - Theme identifier
     */
    applyTheme(themeId) {
        if (!THEMES[themeId]) {
            console.warn(`[ThemeManager] Unknown theme: ${themeId}, falling back to dark-pro`);
            themeId = 'dark-pro';
        }

        console.log(`[ThemeManager] Applying theme: ${themeId}`);
        
        // Apply theme to document root
        document.documentElement.setAttribute('data-theme', themeId);
        
        // Update current theme
        this.currentTheme = themeId;
        
        // Save preference
        this.saveTheme(themeId);
        
        // Update selector if available
        if (this.themeSelector) {
            this.themeSelector.value = themeId;
        }

        // Dispatch custom event for other components
        globalThis.dispatchEvent(new CustomEvent('themeChanged', {
            detail: { theme: themeId, themeName: THEMES[themeId].name }
        }));
        
        console.log(`[ThemeManager] Theme applied successfully: ${THEMES[themeId].name}`);
    }

    /**
     * Setup the theme selector dropdown
     */
    setupThemeSelector() {
        this.themeSelector = document.getElementById('theme-selector');
        
        if (!this.themeSelector) {
            console.warn('[ThemeManager] Theme selector element not found');
            return;
        }

        // Populate options
        this.themeSelector.innerHTML = '';
        Object.entries(THEMES).forEach(([id, theme]) => {
            const option = document.createElement('option');
            option.value = id;
            option.textContent = theme.name;
            option.title = theme.description;
            this.themeSelector.appendChild(option);
        });

        // Set current selection
        this.themeSelector.value = this.currentTheme;

        // Add change event listener
        this.themeSelector.addEventListener('change', (e) => {
            const selectedTheme = e.target.value;
            console.log(`[ThemeManager] User selected theme: ${selectedTheme}`);
            this.applyTheme(selectedTheme);
        });

        console.log('[ThemeManager] Theme selector setup complete');
    }

    /**
     * Get current theme info
     * @returns {Object} Current theme information
     */
    getCurrentTheme() {
        return {
            id: this.currentTheme,
            ...THEMES[this.currentTheme]
        };
    }

    /**
     * Get all available themes
     * @returns {Object} All themes
     */
    getAvailableThemes() {
        return THEMES;
    }
}

// Create singleton instance
const themeManager = new ThemeManager();

// Export for module usage
export { themeManager, THEMES };

