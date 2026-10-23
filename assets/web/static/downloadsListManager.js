// downloadsListManager.js
// Dedicated module for managing local downloads list UI and clear cache actions

import * as dom from './domElements.js';
import { domBatcher, DOMUpdateUtils } from './utils/DOMBatcher.js';
import { DOMDiff } from './utils/DOMDiff.js';
import { soundEvents } from './soundManager.js';
import { showNotification } from './utils.js';
import { appState } from './state/AppState.js';

let previousDownloadIds = new Set();

function getIsAnySequenceRunning() {
    return !!appState.getStateProperty('isAnySequenceRunning');
}

function getProcessInfo(stepKey) {
    return appState.getStateProperty(`processInfo.${stepKey}`);
}

export function updateLocalDownloadsListUI(downloadsData) {
    const listEl = dom.getLocalDownloadsList();
    if (!listEl) return;
    
    let htmlContent = '';
    if (!downloadsData || downloadsData.length === 0) {
        htmlContent = '<li class="placeholder">Aucune activité de téléchargement locale récente.</li>';
    } else {
        const currentDownloadIds = new Set();
        downloadsData.forEach(download => {
            if (download.id) {
                currentDownloadIds.add(download.id);
                if (!previousDownloadIds.has(download.id) &&
                    (download.status === 'pending' || download.status === 'downloading')) {
                    console.log(`[SOUND] New CSV download detected: ${download.filename}`);
                    soundEvents.csvDownloadInitiation();

                    const filename = download.filename && download.filename !== 'Détermination en cours...'
                        ? download.filename.substring(0, 30) + (download.filename.length > 30 ? '...' : '')
                        : 'nouveau fichier';
                    showNotification(`Mode Auto: Téléchargement démarré - ${filename}`, "info", 5000);
                }
            }
        });

        previousDownloadIds = currentDownloadIds;

        downloadsData.forEach(download => {
            const escapedOriginalUrl = DOMUpdateUtils.escapeHtml(download.original_url || '');
            const escapedFilename = DOMUpdateUtils.escapeHtml(download.filename || 'Nom inconnu');
            const escapedStatus = DOMUpdateUtils.escapeHtml(download.status || '');
            const escapedDisplayTimestamp = DOMUpdateUtils.escapeHtml(download.display_timestamp || 'N/A');

            const timestampSpan = `<span class="timestamp">${escapedDisplayTimestamp}</span>`;
            const filenameSpan = `<span class="filename" title="${escapedOriginalUrl}">${escapedFilename}</span>`;
            let statusText = `Statut: <span class="status-text">${escapedStatus}</span>`;
            let progressText = '';
            if (download.status === 'downloading' && typeof download.progress === 'number') {
                progressText = ` <span class="progress-percentage">(${download.progress}%)</span>`;
            }
            if (download.message) {
                const escapedMessage = DOMUpdateUtils.escapeHtml(download.message);
                const messagePreview = escapedMessage.substring(0, 50) + (escapedMessage.length > 50 ? '...' : '');
                statusText += ` <span class="message" title="${escapedMessage}">${messagePreview}</span>`;
            }
            
            const keyAttr = download.id ? ` data-key="${DOMUpdateUtils.escapeHtml(download.id)}"` : '';
            htmlContent += `<li class="download-status-${download.status}"${keyAttr}>${timestampSpan} - ${filenameSpan} - ${statusText}${progressText}</li>`;
        });
    }

    domBatcher.scheduleUpdate('downloads-list-render', () => {
        const wrapper = listEl.cloneNode(false);
        wrapper.innerHTML = htmlContent;
        DOMDiff.morph(listEl, wrapper);
    });
}

export function updateClearCacheGlobalButtonState(status, message = '') {
    const clearBtn = document.getElementById('clear-disk-cache-global-btn');
    if (!clearBtn) return;

    clearBtn.classList.remove('idle', 'running', 'completed', 'failed');
    const textSpan = clearBtn.querySelector('.button-text');
    const currentStepInfo = getProcessInfo('clear_disk_cache');
    const isOtherSequenceRunning = getIsAnySequenceRunning() && currentStepInfo?.status !== 'running';

    switch (status) {
        case 'idle':
            clearBtn.disabled = isOtherSequenceRunning;
            if (textSpan) textSpan.textContent = "Vider le Cache";
            clearBtn.classList.add('idle');
            break;
        case 'starting':
        case 'initiated':
            clearBtn.disabled = true;
            if (textSpan) textSpan.textContent = "Lancement...";
            clearBtn.classList.add('running');
            break;
        case 'running':
            clearBtn.disabled = true;
            if (textSpan) textSpan.textContent = "Nettoyage...";
            clearBtn.classList.add('running');
            break;
        case 'completed':
            clearBtn.disabled = isOtherSequenceRunning;
            if (textSpan) textSpan.textContent = "Cache Vidé";
            clearBtn.classList.add('completed');
            showNotification("Nettoyage du cache disque terminé avec succès.", "success");
            setTimeout(() => updateClearCacheGlobalButtonState('idle'), 5000);
            break;
        case 'failed':
            clearBtn.disabled = isOtherSequenceRunning;
            if (textSpan) textSpan.textContent = "Échec Nettoyage";
            clearBtn.classList.add('failed');
            let notifMessage = "Échec du nettoyage du cache disque.";
            if (message && typeof message === 'string' && message.trim() !== '' && !message.startsWith('<')) {
                notifMessage += ` Détail: ${message.substring(0, 100)}`;
            }
            showNotification(notifMessage, "error");
            setTimeout(() => updateClearCacheGlobalButtonState('idle'), 8000);
            break;
        default:
            clearBtn.disabled = isOtherSequenceRunning;
            if (textSpan) textSpan.textContent = "Vider le Cache";
            clearBtn.classList.add('idle');
    }
}
