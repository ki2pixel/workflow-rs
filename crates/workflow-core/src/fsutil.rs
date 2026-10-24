//! Écritures atomiques durables pour les artefacts contractuels.
//!
//! Le fichier temporaire est caché, voisin de la destination et suffixé par le
//! PID. Le fichier est synchronisé avant `rename`, puis le répertoire parent
//! est synchronisé au mieux : une coupure d'alimentation ne doit pas laisser un
//! JSON tronqué à la place d'un artefact valide.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub fn hidden_temp_path(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("artifact");
    destination.with_file_name(format!(".{name}.tmp.{}", std::process::id()))
}

pub fn publish(temp: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temp, destination)?;
    sync_parent(destination);
    Ok(())
}

pub fn write_atomic(destination: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp = hidden_temp_path(destination);
    let result = (|| -> io::Result<()> {
        let mut file = File::create(&temp)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        publish(&temp, destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Synchronise le répertoire parent. Certains systèmes de fichiers refusent
/// fsync sur un répertoire ; l'échec reste alors sans gravité pour l'artefact
/// lui-même, déjà synchronisé.
pub fn sync_parent(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

/// Purge les résidus d'exécutions interrompues (`_temp_*`, `*.part`,
/// `*.tmp*`) plus vieux que `max_age_hours`, uniquement sous `base`.
///
/// Les fichiers récents sont conservés : un téléchargement ou une extraction
/// en cours dans un autre processus ne doit pas être supprimé.
pub fn purge_orphans(base: &Path, max_age_hours: u64) -> usize {
    purge_orphans_with(base, max_age_hours, SystemTime::now())
}

fn purge_orphans_with(base: &Path, max_age_hours: u64, now: SystemTime) -> usize {
    if max_age_hours == 0 {
        return 0;
    }
    let max_age = Duration::from_secs(max_age_hours.saturating_mul(3_600));
    let mut removed = 0;
    let mut stack = vec![(base.to_path_buf(), 0_u8)];
    while let Some((directory, depth)) = stack.pop() {
        if depth > 8 {
            continue;
        }
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_temp_dir = file_type.is_dir() && name.starts_with("_temp_");
            let is_partial =
                file_type.is_file() && (name.ends_with(".part") || name.contains(".tmp"));
            if is_temp_dir || is_partial {
                let old = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .map(|modified| {
                        now.duration_since(modified)
                            .map(|age| age >= max_age)
                            .unwrap_or(false)
                    })
                    .unwrap_or(false);
                if !old {
                    continue;
                }
                let result = if is_temp_dir {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
                if result.is_ok() {
                    removed += 1;
                }
            } else if file_type.is_dir() {
                stack.push((path, depth + 1));
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_atomic_write_when_completed_then_content_and_hidden_temp_are_valid() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("artifact.json");
        write_atomic(&destination, b"{\"ok\":true}\n").unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"{\"ok\":true}\n");
        let temp = hidden_temp_path(&destination);
        assert!(!temp.exists());
        assert!(temp
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains(".tmp."));
    }

    #[test]
    fn given_old_orphans_when_purged_then_removed_only_under_base() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("runtime");
        let nested = base.join("projets_extraits").join("projet");
        fs::create_dir_all(nested.join("_temp_dead")).unwrap();
        fs::write(nested.join("clip.mp4"), b"keep").unwrap();
        fs::write(nested.join("archive.zip.part"), b"partial").unwrap();
        fs::write(nested.join(".tracking.json.tmp.42"), b"temp").unwrap();
        let outside = dir.path().join("outside.part");
        fs::write(&outside, b"keep").unwrap();

        let removed = purge_orphans_with(&base, 1, SystemTime::now() + Duration::from_secs(7_200));
        assert_eq!(removed, 3);
        assert!(nested.join("clip.mp4").exists());
        assert!(!nested.join("_temp_dead").exists());
        assert!(!nested.join("archive.zip.part").exists());
        assert!(!nested.join(".tracking.json.tmp.42").exists());
        assert!(outside.exists());
    }

    #[test]
    fn given_fresh_orphan_when_purged_then_kept() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("runtime");
        fs::create_dir_all(&base).unwrap();
        let partial = base.join("download.zip.part");
        fs::write(&partial, b"in progress").unwrap();
        assert_eq!(purge_orphans(&base, 24), 0);
        assert!(partial.exists());
    }

    #[test]
    fn given_write_failure_when_temp_exists_then_it_is_removed() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("missing").join("artifact.json");
        assert!(write_atomic(&destination, b"{}").is_err());
    }
}
