/**
 * CSV Download Monitor
 * Monitors CSV download completions and triggers auto-workflow prompts
 */

import { appState } from './state/AppState.js';
import { soundEvents } from './soundManager.js';
import { showCSVWorkflowPrompt } from './csvWorkflowPrompt.js';

/**
 * CSV download monitoring state
 */
let previousDownloads = new Map();
let isMonitoringEnabled = true;
const processedDownloads = new Set();

/**
 * Handle download status updates from the polling system
 * @param {Array} newDownloads - New download list from API
 * @param {Array} oldDownloads - Previous download list
 */
const handleDownloadStatusUpdate = (newDownloads, oldDownloads) => {
    if (!isMonitoringEnabled) {
        console.log('[CSV_MONITOR] Monitoring disabled, skipping update');
        return;
    }

    if (!Array.isArray(newDownloads)) {
        console.warn('[CSV_MONITOR] Invalid data format - expected array, got:', typeof newDownloads, newDownloads);
        return;
    }

    try {
        const csvDownloads = newDownloads.filter(download => isCSVTriggeredDownload(download));

        if (csvDownloads.length > 0) {
            console.log('[CSV_MONITOR] CSV downloads found:', csvDownloads.length, csvDownloads.map(d => ({ id: d.id, status: d.status })));
        }

        const newlyCompletedDownloads = detectNewlyCompletedCSVDownloads(newDownloads);
        console.log('[CSV_MONITOR] Newly completed CSV downloads:', newlyCompletedDownloads.length);

        newlyCompletedDownloads.forEach(download => {
            console.log('[CSV_MONITOR] Processing completion for:', download.id, download.filename);
            handleCSVDownloadCompletion(download);
        });

        updateDownloadTrackingState(newDownloads);

    } catch (error) {
        console.error('[CSV_MONITOR] Error handling download status update:', error);
    }
};

/**
 * Detect newly completed CSV downloads
 * @param {Array} currentDownloads - Current download list
 * @returns {Array} Newly completed CSV downloads
 */
function detectNewlyCompletedCSVDownloads(currentDownloads) {
    const newlyCompleted = [];

    currentDownloads.forEach(download => {
        const downloadId = download.id;
        const isCSVDownload = isCSVTriggeredDownload(download);
        const isCompleted = download.status === 'completed';
        
        if (isCSVDownload && isCompleted) {
            const previousDownload = previousDownloads.get(downloadId);
            const alreadyProcessed = processedDownloads.has(downloadId);

            if (!alreadyProcessed && (!previousDownload || previousDownload.status !== 'completed')) {
                newlyCompleted.push(download);
                console.log('[CSV_MONITOR] Detected newly completed CSV download:', {
                    id: downloadId,
                    filename: download.filename,
                    previousStatus: previousDownload?.status || 'unknown',
                    alreadyProcessed: false
                });
            } else if (alreadyProcessed) {
                console.log('[CSV_MONITOR] Skipping already processed download:', downloadId);
            }
        }
    });

    return newlyCompleted;
}

/**
 * Check if a download was triggered by CSV monitoring
 * @param {Object} download - Download object
 * @returns {boolean} True if CSV-triggered
 */
function isCSVTriggeredDownload(download) {
    return (
        download.id && download.id.startsWith('csv_') ||
        download.csv_timestamp ||
        (download.message && download.message.includes('CSV'))
    );
}

/**
 * Handle a completed CSV download
 * @param {Object} download - Completed download object
 */
function handleCSVDownloadCompletion(download) {
    console.log('[CSV_MONITOR] Processing completed CSV download:', download.filename);

    try {
        processedDownloads.add(download.id);
        console.log('[CSV_MONITOR] Marked download as processed:', download.id);

        soundEvents.csvDownloadCompletion();

        showCSVWorkflowPrompt(download);

        appState.setState({
            lastCSVDownloadCompletion: {
                downloadId: download.id,
                filename: download.filename,
                timestamp: new Date().toISOString(),
                promptShown: true
            }
        }, 'csv_download_completed');

    } catch (error) {
        console.error('[CSV_MONITOR] Error handling CSV download completion:', error);
        processedDownloads.add(download.id);
    }
}

/**
 * Update our internal tracking state
 * @param {Array} currentDownloads - Current download list
 */
function updateDownloadTrackingState(currentDownloads) {

    currentDownloads.forEach(download => {
        if (download.id) {
            previousDownloads.set(download.id, {
                id: download.id,
                status: download.status,
                filename: download.filename,
                timestamp: new Date().toISOString()
            });
        }
    });

    const oneHourAgo = Date.now() - (60 * 60 * 1000);
    for (const [downloadId, downloadInfo] of previousDownloads.entries()) {
        const downloadTime = new Date(downloadInfo.timestamp).getTime();
        if (downloadTime < oneHourAgo) {
            previousDownloads.delete(downloadId);
            processedDownloads.delete(downloadId);
        }
    }
}

/**
 * Initialize CSV download monitoring
 */
export function initializeCSVDownloadMonitor() {
    console.log('[CSV_MONITOR] CSV download completion monitor initializing...');

    console.log('[CSV_MONITOR] handleDownloadStatusUpdate type:', typeof handleDownloadStatusUpdate);
    console.log('[CSV_MONITOR] handleDownloadStatusUpdate function:', handleDownloadStatusUpdate);

    if (typeof handleDownloadStatusUpdate !== 'function') {
        console.error('[CSV_MONITOR] ERROR: handleDownloadStatusUpdate is not a function!');
        return;
    }

    try {
        appState.subscribeToProperty('csvDownloads', handleDownloadStatusUpdate);
        console.log('[CSV_MONITOR] Successfully subscribed to csvDownloads state changes');
    } catch (error) {
        console.error('[CSV_MONITOR] Failed to subscribe to appState:', error);
        return;
    }

    console.log('[CSV_MONITOR] CSV download completion monitor initialized successfully');
}

/**
 * Enable or disable CSV download monitoring
 * @param {boolean} enabled - Whether monitoring should be enabled
 */
export function setCSVMonitoringEnabled(enabled) {
    isMonitoringEnabled = enabled;
    console.log(`[CSV_MONITOR] CSV download monitoring ${enabled ? 'enabled' : 'disabled'}`);
}

/**
 * Get current monitoring state
 * @returns {boolean} Whether monitoring is enabled
 */
export function isCSVMonitoringEnabled() {
    return isMonitoringEnabled;
}

/**
 * Get statistics about monitored downloads
 * @returns {Object} Monitoring statistics
 */
export function getMonitoringStats() {
    return {
        isEnabled: isMonitoringEnabled,
        trackedDownloads: previousDownloads.size,
        lastUpdate: new Date().toISOString()
    };
}
