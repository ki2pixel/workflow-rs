/**
 * CSV Workflow Prompt
 * Engaging popup that prompts users to process completed CSV downloads
 */

import { openPopupUI, closePopupUI } from './popupManager.js';
import { soundEvents } from './soundManager.js';
import { appState } from './state/AppState.js';
import { runStepSequence } from './sequenceManager.js';
import { defaultSequenceableStepsKeys } from './constants.js';
import { showNotification } from './utils.js';
import { DOMUpdateUtils } from './utils/DOMBatcher.js';

// Global variables to track popup state and prevent duplicates
let currentCSVPopup = null;
const shownPopups = new Set(); // Track downloads that have already shown popups
const POPUP_COOLDOWN_MS = 5000; // 5 second cooldown between popups
let lastPopupTime = 0;

// Helper: detect Dropbox hostnames
function isDropboxUrl(rawUrl) {
    try {
        const u = new URL(String(rawUrl || '').trim());
        const host = (u.hostname || '').toLowerCase();
        return host === 'dropbox.com' || host === 'www.dropbox.com' || host === 'dl.dropboxusercontent.com';
    } catch {
        return false;
    }
}

// Helper: detect Dropbox proxy URLs (R2/Worker) like https://<host>.workers.dev/dropbox/<...>/file
function isDropboxProxyUrl(rawUrl) {
    try {
        const u = new URL(String(rawUrl || '').trim());
        const host = (u.hostname || '').toLowerCase();
        const path = (u.pathname || '').toLowerCase();
        return (host.includes('workers.dev') || host.includes('worker')) && path.includes('/dropbox/');
    } catch {
        return false;
    }
}

function isDropboxLikeDownload(download) {
    try {
        const isManualOpen = Boolean(download && download.manual_open);
        if (isManualOpen) return false;
        const urlStr = (download && (download.original_url || download.url)) || '';
        const urlType = (download && String(download.url_type || '').toLowerCase()) || '';
        return urlType === 'dropbox' || isDropboxUrl(urlStr) || isDropboxProxyUrl(urlStr);
    } catch {
        return false;
    }
}

/**
 * Show the CSV workflow prompt popup
 * @param {Object} download - Completed download object
 */
export function showCSVWorkflowPrompt(download) {
    console.log('[CSV_WORKFLOW_PROMPT] Showing workflow prompt for:', download.filename);

    if (!isDropboxLikeDownload(download)) {
        console.log('[CSV_WORKFLOW_PROMPT] Ignoring non-Dropbox download:', download && download.id);
        return;
    }

    // Rate limiting: Check if we've already shown a popup for this download
    if (shownPopups.has(download.id)) {
        console.log('[CSV_WORKFLOW_PROMPT] Popup already shown for download:', download.id);
        return;
    }

    // Rate limiting: Check cooldown period
    const now = Date.now();
    if (now - lastPopupTime < POPUP_COOLDOWN_MS) {
        console.log('[CSV_WORKFLOW_PROMPT] Popup cooldown active, skipping:', download.id);
        return;
    }

    // Close any existing popup first
    if (currentCSVPopup) {
        console.log('[CSV_WORKFLOW_PROMPT] Closing existing popup before showing new one');
        closeCSVWorkflowPrompt();
    }

    // Mark this download as having shown a popup
    shownPopups.add(download.id);
    lastPopupTime = now;

    // Create the popup overlay
    currentCSVPopup = createPopupOverlay(download);

    // Add to document body
    document.body.appendChild(currentCSVPopup);

    // Show the popup
    openPopupUI(currentCSVPopup);

    // Add event listeners
    setupPromptEventListeners(download);

    // Play notification sound
    soundEvents.workflowCompletion();
}

/**
 * Create the popup overlay element
 * @param {Object} download - Completed download object
 * @returns {HTMLElement} Popup overlay element
 */
function createPopupOverlay(download) {
    const overlay = document.createElement('div');
    overlay.className = 'popup-overlay csv-workflow-prompt';
    overlay.style.display = 'none';
    overlay.setAttribute('role', 'dialog');
    overlay.setAttribute('aria-modal', 'true');

    const popupContent = document.createElement('div');
    popupContent.className = 'popup-content';

    // Add close button
    const closeButton = document.createElement('button');
    closeButton.className = 'popup-close-button';
    closeButton.innerHTML = '×';
    closeButton.setAttribute('aria-label', 'Fermer');
    closeButton.onclick = () => closeCSVWorkflowPrompt();

    // Add title (conditional for FromSmash)
    const title = document.createElement('h2');
    title.className = 'popup-title';
    const titleId = `csv-workflow-prompt-title-${Date.now()}`;
    title.id = titleId;
    overlay.setAttribute('aria-labelledby', titleId);
    const urlStr = (download && (download.original_url || download.url)) || '';
    const isFromSmash = (download && (download.url_type === 'fromsmash' || urlStr.toLowerCase().includes('fromsmash.com')));
    const isSwissTransfer = (download && (download.url_type === 'swisstransfer' || urlStr.toLowerCase().includes('swisstransfer.com')));
    const isDropbox = isDropboxLikeDownload(download);
    title.textContent = (!isDropbox) ? '🚀 Nouveau lien disponible !' : '🎉 Téléchargement Terminé !';

    // Add main content
    const contentDiv = document.createElement('div');
    contentDiv.innerHTML = createWorkflowPromptContent(download);

    // Assemble popup
    popupContent.appendChild(closeButton);
    popupContent.appendChild(title);
    popupContent.appendChild(contentDiv);
    overlay.appendChild(popupContent);

    return overlay;
}

/**
 * Create the HTML content for the workflow prompt
 * @param {Object} download - Completed download object
 * @returns {string} HTML content
 */
function createWorkflowPromptContent(download) {
    const filename = download.filename || 'Fichier téléchargé';
    const downloadTime = DOMUpdateUtils.escapeHtml(download.display_timestamp || 'maintenant');
    const urlStr = (download && (download.original_url || download.url || ''));
    const isFromSmash = (download && (download.url_type === 'fromsmash' || urlStr.toLowerCase().includes('fromsmash.com')));
    const isSwissTransfer = (download && (download.url_type === 'swisstransfer' || urlStr.toLowerCase().includes('swisstransfer.com')));
    const isDropbox = isDropboxLikeDownload(download);
    const isDropboxByTypeOrUrl = (
        (download && String(download.url_type || '').toLowerCase() === 'dropbox')
        || isDropboxUrl(urlStr)
        || isDropboxProxyUrl(urlStr)
    );
    
    if (!isDropbox) {
        const safeUrl = sanitizeExternalUrl(download.original_url || download.url || '');
        const providerLabel = isFromSmash ? 'FromSmash' : (isSwissTransfer ? 'SwissTransfer' : (isDropboxByTypeOrUrl ? 'Dropbox' : 'Lien Externe'));
        const hiddenId = isFromSmash ? 'csv-fromsmash-hidden-link' : (isSwissTransfer ? 'csv-swisstransfer-hidden-link' : 'csv-external-hidden-link');
        const openBtnId = isFromSmash ? 'csv-open-fromsmash-btn' : (isSwissTransfer ? 'csv-open-swisstransfer-btn' : 'csv-open-external-btn');
        const mainMsg = isFromSmash
            ? '🚀 Un nouveau lien FromSmash est disponible. Cliquez sur « Ouvrir et télécharger » pour l’ouvrir dans un nouvel onglet et démarrer le téléchargement.'
            : (isSwissTransfer
                ? '🚀 Un nouveau lien SwissTransfer est disponible. Cliquez sur « Ouvrir et télécharger » pour l’ouvrir dans un nouvel onglet et démarrer le téléchargement.'
                : (isDropboxByTypeOrUrl
                    ? '🚀 Un nouveau lien Dropbox est disponible. Cliquez sur « Ouvrir et télécharger » pour l’ouvrir dans un nouvel onglet et démarrer le téléchargement.'
                    : '🚀 Un nouveau lien externe est disponible. Cliquez sur « Ouvrir manuellement » pour l’ouvrir dans un nouvel onglet.'));
        return `
        <div class="csv-workflow-prompt-content">
            <div class="prompt-header">
                <div class="download-icon">🔗</div>
                <div class="download-info">
                    <h3 class="download-title">${DOMUpdateUtils.escapeHtml(filename)}</h3>
                    <p class="download-subtitle">Reçu à ${downloadTime}</p>
                </div>
            </div>
            
            <div class="prompt-message">
                <p class="main-message">${mainMsg}</p>
                ${!safeUrl ? '<p class="warning">⚠️ Domaine non autorisé. L’ouverture automatique est bloquée pour des raisons de sécurité.</p>' : ''}
            </div>
            
            <div class="prompt-actions">
                <button id="${openBtnId}" class="btn-primary" ${!safeUrl ? 'disabled' : ''}>
                    <span class="btn-icon">🌐</span>
                    <span class="btn-text">${isFromSmash || isSwissTransfer || isDropboxByTypeOrUrl ? 'Ouvrir et télécharger' : 'Ouvrir manuellement'}</span>
                </button>
                <button id="csv-workflow-dismiss-btn" class="btn-secondary workflow-dismiss-btn">
                    <span class="btn-icon">⏭️</span>
                    <span class="btn-text">Plus tard</span>
                </button>
            </div>
            
            <div class="prompt-footer">
                <p class="footer-note">
                    💡 Aucun traitement automatique ne sera lancé pour ce lien (${providerLabel}). Revenez lancer le workflow une fois le fichier téléchargé et disponible localement.
                </p>
            </div>
            
            <a id="${hiddenId}" href="${safeUrl || '#'}" target="_blank" rel="noopener noreferrer" style="display:none;">open</a>
        </div>
        `;
    }
    
    return `
        <div class="csv-workflow-prompt-content">
            <div class="prompt-header">
                <div class="download-icon">📥</div>
                <div class="download-info">
                    <h3 class="download-title">${DOMUpdateUtils.escapeHtml(filename)}</h3>
                    <p class="download-subtitle">Téléchargé à ${downloadTime}</p>
                </div>
            </div>
            
            <div class="prompt-message">
                <p class="main-message">
                    🚀 Votre fichier est prêt ! Voulez-vous lancer le workflow complet 
                    pour traiter automatiquement ce contenu ?
                </p>
                <div class="workflow-preview">
                    <div class="workflow-steps">
                        <span class="step-badge">1️⃣ Extraction</span>
                        <span class="step-arrow">→</span>
                        <span class="step-badge">2️⃣ Analyse</span>
                        <span class="step-arrow">→</span>
                        <span class="step-badge">3️⃣ Détection</span>
                        <span class="step-arrow">→</span>
                        <span class="step-badge">4️⃣ Audio</span>
                        <span class="step-arrow">→</span>
                        <span class="step-badge">5️⃣ Tracking</span>
                        <span class="step-arrow">→</span>
                        <span class="step-badge">6️⃣ Finalisation</span>
                    </div>
                </div>
            </div>
            
            <div class="prompt-actions">
                <button id="csv-workflow-launch-btn" class="btn-primary workflow-launch-btn">
                    <span class="btn-icon">✨</span>
                    <span class="btn-text">Oui, Lancer le Workflow !</span>
                </button>
                <button id="csv-workflow-dismiss-btn" class="btn-secondary workflow-dismiss-btn">
                    <span class="btn-icon">⏭️</span>
                    <span class="btn-text">Plus tard</span>
                </button>
            </div>
            
            <div class="prompt-footer">
                <p class="footer-note">
                    💡 Le workflow traitera automatiquement votre fichier selon la séquence complète (étapes 1-6)
                </p>
            </div>
        </div>
    `;
}

/**
 * Close the CSV workflow prompt popup
 */
function closeCSVWorkflowPrompt() {
    if (currentCSVPopup) {
        closePopupUI(currentCSVPopup);
        document.body.removeChild(currentCSVPopup);
        currentCSVPopup = null;
    }
}

/**
 * Setup event listeners for the prompt buttons
 * @param {Object} download - Download object
 */
function setupPromptEventListeners(download) {
    const urlStr = (download && (download.original_url || download.url || ''));
    const isFromSmash = (download && (download.url_type === 'fromsmash' || urlStr.toLowerCase().includes('fromsmash.com')));
    const isSwissTransfer = (download && (download.url_type === 'swisstransfer' || urlStr.toLowerCase().includes('swisstransfer.com')));
    const isDropbox = isDropboxLikeDownload(download);
    const dismissBtn = document.getElementById('csv-workflow-dismiss-btn');

    if (dismissBtn) {
        dismissBtn.addEventListener('click', () => handleWorkflowDismiss(download));
    }

    if (!isDropbox) {
        const btnId = isFromSmash ? 'csv-open-fromsmash-btn' : (isSwissTransfer ? 'csv-open-swisstransfer-btn' : 'csv-open-external-btn');
        const openBtn = document.getElementById(btnId);
        if (openBtn) {
            openBtn.addEventListener('click', () => openExternalLink(download, { isFromSmash, isSwissTransfer }));
        }
    } else {
        const launchBtn = document.getElementById('csv-workflow-launch-btn');
        if (launchBtn) {
            launchBtn.addEventListener('click', () => handleWorkflowLaunch(download));
        }
    }
}

/**
 * Open FromSmash link in a new tab with basic URL sanitization
 * @param {Object} download
 */
function openExternalLink(download, { isFromSmash = false, isSwissTransfer = false } = {}) {
    try {
        const rawUrl = (download && (download.original_url || download.url)) || '';
        const safeUrl = sanitizeExternalUrl(rawUrl);
        if (!safeUrl) {
            showNotification('Lien invalide ou domaine non autorisé.', 'error');
            return;
        }

        // Close the popup first
        closeCSVWorkflowPrompt();

        // Attempt to open via hidden anchor to respect browser policies
        const hiddenId = isFromSmash ? 'csv-fromsmash-hidden-link' : (isSwissTransfer ? 'csv-swisstransfer-hidden-link' : 'csv-external-hidden-link');
        const hiddenA = document.getElementById(hiddenId);
        if (hiddenA && hiddenA.href) {
            hiddenA.click();
        } else {
            globalThis.open(safeUrl, '_blank', 'noopener,noreferrer');
        }

        const provider = isFromSmash ? 'FromSmash' : (isSwissTransfer ? 'SwissTransfer' : 'externe');
        showNotification(`Ouverture du lien ${provider} dans un nouvel onglet...`, 'success');
        soundEvents.workflowStart();
    } catch (e) {
        console.error('[CSV_WORKFLOW_PROMPT] Failed to open external link:', e);
        showNotification("Impossible d'ouvrir le lien.", 'error');
        soundEvents.errorEvent();
    }
}

/**
 * Open SwissTransfer link in a new tab with basic URL sanitization
 * @param {Object} download
 */
function openSwissTransferLink(download) {
    try {
        const rawUrl = (download && (download.original_url || download.url)) || '';
        const safeUrl = sanitizeExternalUrl(rawUrl);
        if (!safeUrl) {
            showNotification("Lien invalide ou non supporté.", 'error');
            return;
        }

        // Close the popup first
        closeCSVWorkflowPrompt();

        // Attempt to open via hidden anchor to respect browser policies
        const hiddenA = document.getElementById('csv-swisstransfer-hidden-link');
        if (hiddenA && hiddenA.href) {
            hiddenA.click();
        } else {
            globalThis.open(safeUrl, '_blank', 'noopener,noreferrer');
        }

        showNotification("Ouverture du lien SwissTransfer dans un nouvel onglet...", 'success');
        soundEvents.workflowStart();
    } catch (e) {
        console.error('[CSV_WORKFLOW_PROMPT] Failed to open SwissTransfer link:', e);
        showNotification("Impossible d'ouvrir le lien.", 'error');
        soundEvents.errorEvent();
    }
}

/**
 * Sanitize external URL (very basic allowlist)
 * @param {string} url
 * @returns {string|null}
 */
function sanitizeExternalUrl(url) {
    if (typeof url !== 'string') return null;
    const trimmed = url.trim();
    if (!trimmed) return null;
    try {
        const u = new URL(trimmed);
        const hostname = (u.hostname || '').toLowerCase();
        // Allow any valid HTTP(S) URL. Non-Dropbox links are opened manually in a new tab.
        if (!['http:', 'https:'].includes(u.protocol)) {
            return null;
        }
        return u.toString();
    } catch {
        return null;
    }
}

/**
 * Handle workflow launch button click
 * @param {Object} download - Download object
 */
async function handleWorkflowLaunch(download) {
    console.log('[CSV_WORKFLOW_PROMPT] User chose to launch workflow for:', download.filename);
    
    try {
        // Close the popup first
        closeCSVWorkflowPrompt();
        
        // Check if a sequence is already running
        if (appState.getStateProperty('isAnySequenceRunning')) {
            showNotification("Une séquence est déjà en cours. Veuillez attendre qu'elle se termine.", 'warning');
            return;
        }
        
        // Play workflow start sound
        soundEvents.workflowStart();
        
        // Show success notification
        showNotification(`🚀 Lancement du workflow pour "${download.filename}"`, 'success');
        
        // Launch the complete workflow sequence
        await runStepSequence(defaultSequenceableStepsKeys, "Séquence CSV Auto");
        
        // Update app state to track the auto-launch
        appState.setState({
            lastCSVAutoLaunch: {
                downloadId: download.id,
                filename: download.filename,
                timestamp: new Date().toISOString(),
                sequenceType: 'CSV Auto'
            }
        }, 'csv_workflow_auto_launched');
        
    } catch (error) {
        console.error('[CSV_WORKFLOW_PROMPT] Error launching workflow:', error);
        showNotification("Erreur lors du lancement du workflow. Veuillez réessayer.", 'error');
        soundEvents.errorEvent();
    }
}

/**
 * Handle workflow dismiss button click
 * @param {Object} download - Download object
 */
function handleWorkflowDismiss(download) {
    console.log('[CSV_WORKFLOW_PROMPT] User dismissed workflow prompt for:', download.filename);
    
    // Close the popup
    closeCSVWorkflowPrompt();
    
    // Show a gentle notification
    showNotification("Workflow reporté. Vous pouvez le lancer manuellement quand vous le souhaitez.", 'info');
    
    // Update app state to track the dismissal
    appState.setState({
        lastCSVPromptDismissal: {
            downloadId: download.id,
            filename: download.filename,
            timestamp: new Date().toISOString()
        }
    }, 'csv_workflow_prompt_dismissed');
}

// Using DOMUpdateUtils.escapeHtml from DOMBatcher for XSS safety (project standard)
