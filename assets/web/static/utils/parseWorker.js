// parseWorker.js
// Web Worker for heavy parsing operations (log syntax highlighting and JSON parsing)

import { parseAndStyleLogContent, escapeHtmlWorker } from './logParserUtils.js';

// Listen for messages from the main thread
self.addEventListener('message', (e) => {
    // Sécurité: vérifier l'origine du message
    if (e.origin && e.origin !== self.location.origin) {
        console.warn("[parseWorker] Message ignoré: origine non autorisée", e.origin);
        return;
    }

    const { id, type, payload } = e.data;
    
    try {
        let result = null;
        
        switch (type) {
            case 'parseLogs':
                result = parseAndStyleLogContent(payload, escapeHtmlWorker);
                break;
            case 'parseJson':
                // Simply parse JSON in the worker thread to offload heavy JSON.parse operations
                result = typeof payload === 'string' ? JSON.parse(payload) : payload;
                break;
            default:
                throw new Error(`Type de message inconnu : ${type}`);
        }
        
        self.postMessage({ id, success: true, result });
    } catch (error) {
        self.postMessage({ id, success: false, error: error.message });
    }
});
