// WorkerManager.js
// Orchestrateur singleton pour les tâches lourdes déléguées aux Web Workers

class WorkerManager {
    constructor() {
        this.worker = null;
        this.callbacks = new Map();
        this.nextRequestId = 1;
        this.isSupported = typeof globalThis !== 'undefined' && typeof globalThis.Worker !== 'undefined';
        
        // Track the latest request ID for specific channels to handle race conditions
        this.latestRequestForChannel = new Map();
        
        console.debug('[WorkerManager] Initialized. Web Workers supported:', this.isSupported);
    }
    
    _initWorker() {
        if (!this.isSupported) return;
        if (this.worker) return;
        
        try {
            // Instantiate lazily
            this.worker = new Worker('/static/utils/parseWorker.js', { type: 'module' });
            this.worker.addEventListener('message', (e) => {
                const { id, success, result, error } = e.data;
                const callback = this.callbacks.get(id);
                
                if (callback) {
                    this.callbacks.delete(id);
                    if (success) {
                        callback.resolve(result);
                    } else {
                        callback.reject(new Error(error));
                    }
                }
            });
            this.worker.addEventListener('error', (err) => {
                console.error('[WorkerManager] Erreur critique dans le Web Worker:', err);
            });
            console.debug('[WorkerManager] Web Worker initialisé avec succès.');
        } catch (error) {
            console.error('[WorkerManager] Impossible d\'initialiser le Web Worker. Fallback activé.', error);
            this.isSupported = false;
        }
    }
    
    /**
     * Effectue une requête asynchrone au Worker.
     * @param {string} type - Le type d'opération (ex: 'parseLogs')
     * @param {*} payload - Les données à traiter
     * @param {string|null} channel - Si fourni, garantit qu'on ignore les réponses obsolètes pour ce canal
     * @returns {Promise<*>}
     */
    async _executeTask(type, payload, channel = null) {
        const id = this.nextRequestId++;
        
        if (channel) {
            this.latestRequestForChannel.set(channel, id);
        }
        
        if (!this.isSupported) {
            // Synchronous fallback for Node or old browsers
            return this._executeSynchronously(type, payload)
                .then(result => {
                    // Vérification de race condition même en synchrone (peu probable mais robuste)
                    if (channel && this.latestRequestForChannel.get(channel) !== id) {
                        throw new Error(`[WorkerManager] Réponse obsolète ignorée pour le canal: ${channel}`);
                    }
                    return result;
                });
        }
        
        this._initWorker();
        
        return new Promise((resolve, reject) => {
            this.callbacks.set(id, {
                resolve: (result) => {
                    if (channel && this.latestRequestForChannel.get(channel) !== id) {
                        // C'est une réponse périmée, une nouvelle requête a déjà été envoyée sur ce canal
                        reject(new Error(`[WorkerManager] Réponse obsolète ignorée pour le canal: ${channel}`));
                    } else {
                        resolve(result);
                    }
                },
                reject
            });
            
            this.worker.postMessage({ id, type, payload });
        });
    }
    
    /**
     * Fallback synchrone lorsque l'API Worker n'est pas disponible (Node.js/Tests)
     */
    async _executeSynchronously(type, payload) {
        switch (type) {
            case 'parseLogs': {
                const mod = await import('./logParserUtils.js');
                const { DOMUpdateUtils } = await import('./DOMBatcher.js');
                if (typeof mod.parseAndStyleLogContent !== 'function') {
                    throw new Error('[WorkerManager] parseAndStyleLogContent is missing in logParserUtils');
                }
                return mod.parseAndStyleLogContent(payload, DOMUpdateUtils.escapeHtml);
            }
            case 'parseJson':
                return typeof payload === 'string' ? JSON.parse(payload) : payload;
            default:
                throw new Error(`Type de message inconnu : ${type}`);
        }
    }
    
    /**
     * Demande au Worker de parser et colorer syntaxiquement un log.
     * @param {string} rawLogContent - Le log brut
     * @param {string} channel - Canal (ex: 'mainLogPanel') pour éviter les out-of-order updates
     * @returns {Promise<string>}
     */
    async parseLogs(rawLogContent, channel = null) {
        return this._executeTask('parseLogs', rawLogContent, channel);
    }
    
    /**
     * Demande au Worker de parser un JSON volumineux.
     * @param {string} jsonString 
     * @param {string} channel 
     * @returns {Promise<Object>}
     */
    async parseJson(jsonString, channel = null) {
        return this._executeTask('parseJson', jsonString, channel);
    }
    
    destroy() {
        if (this.worker) {
            this.worker.terminate();
            this.worker = null;
        }
        this.callbacks.clear();
        this.latestRequestForChannel.clear();
    }
}

export const workerManager = new WorkerManager();
