import { COMPILED_LOG_PATTERNS, LOG_LINE_EMPTY_OR_WHITESPACE_PATTERN } from './logPatterns.js';

const _LOG_LINE_EMPTY_OR_WHITESPACE_PATTERN = LOG_LINE_EMPTY_OR_WHITESPACE_PATTERN;
const _COMPILED_LOG_PATTERNS = COMPILED_LOG_PATTERNS;

export function escapeHtmlWorker(text) {
    if (!text) return '';
    return String(text)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#39;");
}

/**
 * Parse and style log content with CSS classes for different log types.
 * Accepts a custom escapeHtml function so it works in both DOM and Worker contexts.
 *
 * @param {string} rawContent - Raw log content
 * @param {Function} escapeFn - Function to escape HTML (DOMUpdateUtils.escapeHtml or escapeHtmlWorker)
 * @returns {string} - Styled HTML content
 */
export function parseAndStyleLogContent(rawContent, escapeFn) {
    if (!rawContent || typeof rawContent !== 'string') {
        return rawContent || '';
    }

    const lines = rawContent.split('\n');
    const styledLines = new Array(lines.length);

    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        if (line === '' || _LOG_LINE_EMPTY_OR_WHITESPACE_PATTERN.test(line)) {
            styledLines[i] = escapeFn ? escapeFn(line) : line;
            continue;
        }

        const escapedLine = escapeFn ? escapeFn(line) : line;

        let logType = 'default';
        for (let j = 0; j < _COMPILED_LOG_PATTERNS.length; j++) {
            const pattern = _COMPILED_LOG_PATTERNS[j];
            if (pattern.regex.test(line)) {
                logType = pattern.type;
                break;
            }
        }

        styledLines[i] = logType !== 'default'
            ? `<span class="log-line log-${logType}">${escapedLine}</span>`
            : escapedLine;
    }

    return styledLines.join('\n');
}
