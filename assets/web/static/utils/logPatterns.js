// logPatterns.js
// Shared log syntax highlighting patterns and regex definitions

export const LOG_LINE_EMPTY_OR_WHITESPACE_PATTERN = /^\s*$/;

export const LOG_TIMESTAMP_PATTERN = /^(?:\d{4}-\d{2}-\d{2}|\d{2}:\d{2}:\d{2})/;
export const LOG_ERROR_PATTERN = /(?:erreur|error|échec|failed|exception|critical|fatal|crash)/i;
export const LOG_WARNING_PATTERN = /(?:warning|attention|avertissement|warn|caution|deprecated)/i;
export const LOG_SUCCESS_PATTERN = /(?:success|réussi|terminé|completed|finished|done|✓|✔|ok\b)/i;
export const LOG_INFO_PATTERN = /(?:info|information|démarrage|starting|lancement|initiated|status)/i;
export const LOG_DEBUG_PATTERN = /(?:debug|trace|verbose|détail)/i;
export const LOG_COMMAND_PATTERN = /^(?:commande:|command:|executing:|exécution:|\$|>)/i;
export const LOG_PROGRESS_PATTERN = /(?:\d+%|\d+\/\d+|progress|progression|chargement|loading|téléchargement|downloading)/i;

export const LOG_PATTERNS = [
    { regex: LOG_ERROR_PATTERN, type: 'error' },
    { regex: LOG_WARNING_PATTERN, type: 'warning' },
    { regex: LOG_SUCCESS_PATTERN, type: 'success' },
    { regex: LOG_PROGRESS_PATTERN, type: 'progress' },
    { regex: LOG_COMMAND_PATTERN, type: 'command' },
    { regex: LOG_INFO_PATTERN, type: 'info' },
    { regex: LOG_TIMESTAMP_PATTERN, type: 'info' },
    { regex: LOG_DEBUG_PATTERN, type: 'debug' }
];

export const COMPILED_LOG_PATTERNS = LOG_PATTERNS.map(p => ({
    type: p.type,
    regex: new RegExp(p.regex.source, p.regex.flags)
}));
