//! Classification et stockage borné des lignes de log.

use parking_lot::Mutex;
use regex::Regex;
use std::collections::VecDeque;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Error,
    Warning,
    Success,
    Progress,
    Command,
    Info,
    Debug,
    Neutral,
}

fn regex(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("regex de log valide"))
}

static ERROR: OnceLock<Regex> = OnceLock::new();
static WARNING: OnceLock<Regex> = OnceLock::new();
static SUCCESS: OnceLock<Regex> = OnceLock::new();
static PROGRESS: OnceLock<Regex> = OnceLock::new();
static COMMAND: OnceLock<Regex> = OnceLock::new();
static INFO: OnceLock<Regex> = OnceLock::new();
static DEBUG: OnceLock<Regex> = OnceLock::new();

pub fn classify(line: &str) -> LogLevel {
    let line = line.trim();
    if line.is_empty() {
        return LogLevel::Neutral;
    }
    if regex(
        &ERROR,
        r"(?i)(?:erreur|error|échec|failed|exception|critical|fatal|crash)",
    )
    .is_match(line)
    {
        LogLevel::Error
    } else if regex(
        &WARNING,
        r"(?i)(?:warning|attention|avertissement|warn|caution|deprecated)",
    )
    .is_match(line)
    {
        LogLevel::Warning
    } else if regex(
        &SUCCESS,
        r"(?i)(?:success|réussi|terminé|completed|finished|done|✓|✔|ok\b)",
    )
    .is_match(line)
    {
        LogLevel::Success
    } else if regex(
        &PROGRESS,
        r"(?i)(?:\d+%|\d+/\d+|progress|progression|chargement|loading|téléchargement|downloading)",
    )
    .is_match(line)
    {
        LogLevel::Progress
    } else if regex(
        &COMMAND,
        r"(?i)^(?:commande:|command:|executing:|exécution:|\$|>)",
    )
    .is_match(line)
    {
        LogLevel::Command
    } else if regex(
        &INFO,
        r"(?i)(?:info|information|démarrage|starting|lancement|initiated|status)",
    )
    .is_match(line)
    {
        LogLevel::Info
    } else if regex(&DEBUG, r"(?i)(?:debug|trace|verbose|détail)").is_match(line) {
        LogLevel::Debug
    } else {
        LogLevel::Neutral
    }
}

/// Décode les entités produites par `workflow-server::progress::html_escape`.
pub fn html_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

pub struct LogStore {
    capacity: usize,
    lines: Mutex<VecDeque<String>>,
}

impl LogStore {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            lines: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
        }
    }

    pub fn push(&self, line: impl Into<String>) {
        let mut lines = self.lines.lock();
        if lines.len() >= self.capacity {
            lines.pop_front();
        }
        lines.push_back(line.into());
    }

    pub fn replace(&self, lines: impl IntoIterator<Item = String>) {
        let mut store = self.lines.lock();
        store.clear();
        for line in lines {
            if store.len() >= self.capacity {
                store.pop_front();
            }
            store.push_back(line);
        }
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.lines.lock().iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.lines.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clear(&self) {
        self.lines.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_legacy_patterns() {
        assert_eq!(classify("Erreur: CUDA out of memory"), LogLevel::Error);
        assert_eq!(classify("warning: deprecated option"), LogLevel::Warning);
        assert_eq!(classify("STEP2 terminé avec succès"), LogLevel::Success);
        assert_eq!(classify("progression 42/100"), LogLevel::Progress);
        assert_eq!(classify("$ ffmpeg -i input.mp4"), LogLevel::Command);
        assert_eq!(classify("info: démarrage du serveur"), LogLevel::Info);
        assert_eq!(classify("debug: détail interne"), LogLevel::Debug);
        assert_eq!(classify("ligne neutre"), LogLevel::Neutral);
    }

    #[test]
    fn unescapes_server_entities_without_double_decoding() {
        assert_eq!(
            html_unescape("a &amp;lt; b &amp; c &#x27;x&#x27;"),
            "a &lt; b & c 'x'"
        );
    }

    #[test]
    fn ring_buffer_keeps_last_lines() {
        let store = LogStore::new(3);
        for index in 0..5 {
            store.push(format!("ligne {index}"));
        }
        assert_eq!(store.snapshot(), vec!["ligne 2", "ligne 3", "ligne 4"]);
        store.replace(["a".into(), "b".into(), "c".into(), "d".into()]);
        assert_eq!(store.snapshot(), vec!["b", "c", "d"]);
        assert_eq!(store.len(), 3);
    }
}
