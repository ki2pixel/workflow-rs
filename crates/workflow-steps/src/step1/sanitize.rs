//! Assainissement des chemins d'archives et protection contre les traversées.
//!
//! Reproduit la logique de `utils/filename_security.py` : normalisation NFKC,
//! suppression des composants `.` / `..`, remplacement des caractères
//! interdits, noms réservés Windows et limites de longueur.

use std::path::{Component, Path};
use unicode_normalization::UnicodeNormalization;

const WINDOWS_FORBIDDEN_CHARS: [char; 7] = ['<', '>', ':', '"', '|', '?', '*'];
const MAX_FILENAME_LENGTH: usize = 255;
const MAX_PATH_LENGTH: usize = 4096;
const WINDOWS_RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanitizationResult {
    pub original_name: String,
    pub sanitized_name: String,
    pub was_modified: bool,
    pub security_issues: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct SanitizationStats {
    pub total_processed: usize,
    pub modified_count: usize,
    pub security_issues_found: usize,
}

/// Assainisseur avec statistiques, à usage séquentiel par archive.
#[derive(Debug, Default)]
pub struct FilenameSanitizer {
    stats: SanitizationStats,
}

impl FilenameSanitizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> SanitizationStats {
        self.stats
    }

    /// Assainit un chemin complet de membre d'archive.
    pub fn sanitize_archive_member_path(&mut self, member_path: &str) -> SanitizationResult {
        self.stats.total_processed += 1;

        let original_path = member_path.to_string();
        let mut security_issues = Vec::new();
        let mut warnings = Vec::new();

        for pattern in ["../", "..\\"] {
            if member_path.contains(pattern) {
                security_issues.push(format!("Path traversal pattern detected: {pattern}"));
            }
        }
        if member_path.starts_with('/') || member_path.starts_with('\\') {
            security_issues.push("Path traversal pattern detected: absolute path".into());
        }
        for pattern in ["/../", "\\..\\"] {
            if member_path.contains(pattern) {
                security_issues.push(format!("Path traversal pattern detected: {pattern}"));
            }
        }
        for pattern in ["/..", "\\.."] {
            if member_path.ends_with(pattern)
                && !security_issues.iter().any(|i| i.contains(pattern))
            {
                security_issues.push(format!("Path traversal pattern detected: {pattern}"));
            }
        }

        let normalized: String = member_path.nfkc().collect();
        let mut path_parts: Vec<String> = Vec::new();
        for part in normalized.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                security_issues.push(format!("Dangerous path component: '{part}'"));
                continue;
            }
            let component = self.sanitize_filename_component(part);
            warnings.extend(component.warnings);
            security_issues.extend(component.security_issues);
            if !component.sanitized_name.is_empty() {
                path_parts.push(component.sanitized_name);
            }
        }

        let mut sanitized_path = if path_parts.is_empty() {
            "sanitized_file".to_string()
        } else {
            path_parts.join("/")
        };

        if sanitized_path.chars().count() > MAX_PATH_LENGTH {
            warnings.push(format!(
                "Path length {} exceeds limit {MAX_PATH_LENGTH}",
                sanitized_path.chars().count()
            ));
            sanitized_path = truncate_path(&sanitized_path);
        }

        let was_modified = original_path != sanitized_path;
        if was_modified {
            self.stats.modified_count += 1;
        }
        if !security_issues.is_empty() {
            self.stats.security_issues_found += 1;
        }

        SanitizationResult {
            original_name: original_path,
            sanitized_name: sanitized_path,
            was_modified,
            security_issues,
            warnings,
        }
    }

    fn sanitize_filename_component(&self, filename: &str) -> SanitizationResult {
        let original_filename = filename.to_string();
        let mut security_issues = Vec::new();
        let mut warnings = Vec::new();

        let mut sanitized: String = filename
            .chars()
            .filter(|character| {
                let code = *character as u32;
                let control = code < 32;
                let forbidden = control || *character == '\0';
                if forbidden {
                    security_issues.push(format!("Control/null character detected: {character:?}"));
                }
                !forbidden
            })
            .map(|character| {
                if WINDOWS_FORBIDDEN_CHARS.contains(&character) {
                    warnings.push(format!(
                        "Windows forbidden character replaced: '{character}'"
                    ));
                    '_'
                } else {
                    character
                }
            })
            .collect();

        let stripped = sanitized.trim_matches(|character| character == '.' || character == ' ');
        if stripped != sanitized {
            warnings.push("Removed leading/trailing dots or spaces".into());
        }
        sanitized = stripped.to_string();

        let stem = Path::new(&sanitized)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_uppercase();
        if WINDOWS_RESERVED_NAMES.contains(&stem.as_str()) {
            security_issues.push(format!("Windows reserved name detected: {stem}"));
            sanitized = format!("safe_{sanitized}");
        }

        if sanitized.chars().count() > MAX_FILENAME_LENGTH {
            warnings.push(format!(
                "Filename length {} exceeds limit {MAX_FILENAME_LENGTH}",
                sanitized.chars().count()
            ));
            sanitized = truncate_filename(&sanitized);
        }

        if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
            warnings.push("Empty or invalid filename, using fallback".into());
            sanitized = "sanitized_file".into();
        }

        let was_modified = original_filename != sanitized;
        SanitizationResult {
            original_name: original_filename,
            sanitized_name: sanitized,
            was_modified,
            security_issues,
            warnings,
        }
    }
}

/// Vérifie qu'un chemin assaini reste relatif et sans composant parent.
pub fn validate_extraction_path(member_path: &str, _base_extraction_dir: &Path) -> bool {
    let path = Path::new(member_path);
    if path.is_absolute() {
        return false;
    }
    path.components()
        .all(|component| matches!(component, Component::Normal(_)))
}

fn truncate_filename(filename: &str) -> String {
    if filename.chars().count() <= MAX_FILENAME_LENGTH {
        return filename.to_string();
    }
    let (name_part, extension) = split_extension(filename);
    let extension_len = extension.chars().count();
    if MAX_FILENAME_LENGTH <= extension_len {
        return filename.chars().take(MAX_FILENAME_LENGTH).collect();
    }
    let max_name_length = MAX_FILENAME_LENGTH - extension_len;
    let mut truncated: String = name_part.chars().take(max_name_length).collect();
    truncated.push_str(extension);
    truncated
}

fn split_extension(filename: &str) -> (&str, &str) {
    match filename.rfind('.') {
        Some(index) if index > 0 => (&filename[..index], &filename[index..]),
        _ => (filename, ""),
    }
}

fn truncate_path(path: &str) -> String {
    let characters: Vec<char> = path.chars().collect();
    if characters.len() <= MAX_PATH_LENGTH {
        return path.to_string();
    }
    let excess = characters.len() - MAX_PATH_LENGTH + 10;
    let middle = characters.len() / 2;
    let start = middle.saturating_sub(excess / 2);
    let end = (middle + excess / 2).min(characters.len());
    let mut truncated: String = characters[..start].iter().collect();
    truncated.push_str("...");
    truncated.extend(characters[end..].iter());
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_components_are_dropped() {
        let mut sanitizer = FilenameSanitizer::new();
        let result = sanitizer.sanitize_archive_member_path("../../etc/passwd");
        assert_eq!(result.sanitized_name, "etc/passwd");
        assert!(!result.security_issues.is_empty());
        assert!(validate_extraction_path(
            &result.sanitized_name,
            Path::new("/tmp")
        ));
    }

    #[test]
    fn absolute_paths_become_relative() {
        let mut sanitizer = FilenameSanitizer::new();
        let result = sanitizer.sanitize_archive_member_path("/etc/passwd");
        assert_eq!(result.sanitized_name, "etc/passwd");
        assert!(!validate_extraction_path("/etc/passwd", Path::new("/tmp")));
    }

    #[test]
    fn forbidden_characters_are_rewritten() {
        let mut sanitizer = FilenameSanitizer::new();
        let result = sanitizer.sanitize_archive_member_path("docs/CON:clip?.mp4");
        assert_eq!(result.sanitized_name, "docs/CON_clip_.mp4");
    }

    #[test]
    fn reserved_names_are_prefixed() {
        let mut sanitizer = FilenameSanitizer::new();
        let result = sanitizer.sanitize_archive_member_path("CON.mp4");
        assert_eq!(result.sanitized_name, "safe_CON.mp4");
        assert!(!result.security_issues.is_empty());
    }

    #[test]
    fn long_names_keep_their_extension() {
        let mut sanitizer = FilenameSanitizer::new();
        let name = format!("{}.mp4", "a".repeat(300));
        let result = sanitizer.sanitize_archive_member_path(&name);
        assert_eq!(result.sanitized_name.chars().count(), MAX_FILENAME_LENGTH);
        assert!(result.sanitized_name.ends_with(".mp4"));
    }

    #[test]
    fn stats_count_modified_and_unsafe_paths() {
        let mut sanitizer = FilenameSanitizer::new();
        sanitizer.sanitize_archive_member_path("ok/file.mp4");
        sanitizer.sanitize_archive_member_path("../escape.mp4");
        let stats = sanitizer.stats();
        assert_eq!(stats.total_processed, 2);
        assert_eq!(stats.modified_count, 1);
        assert_eq!(stats.security_issues_found, 1);
    }
}
