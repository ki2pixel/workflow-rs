use std::fs::{self, File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Verrou exclusif inter-processus.
///
/// Le verrou est utilisé pour sérialiser les étapes qui touchent le GPU ou le
/// Coral. STEP5 et STEP6 n'en ont pas besoin, mais le type est partagé par le
/// serveur et les étapes matérielles.
///
/// Le fichier contient le PID du détenteur après acquisition. Cette trace sert
/// uniquement au diagnostic (`fuser`, inspection manuelle) : la sémantique du
/// verrou reste portée par `flock` et libérée dans `Drop`.
pub struct HardwareLock {
    file: File,
    path: PathBuf,
}

impl HardwareLock {
    /// Ouvre et verrouille le fichier. `wait=false` échoue immédiatement si le
    /// verrou est déjà tenu.
    pub fn acquire(path: &Path, wait: bool) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)?;
        if wait {
            fs2::FileExt::lock_exclusive(&file)?;
        } else {
            fs2::FileExt::try_lock_exclusive(&file)?;
        }
        // Le verrou est tenu : aucun autre détenteur ne peut réécrire le PID.
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        writeln!(file, "{}", std::process::id())?;
        file.flush()?;
        file.sync_all()?;
        Ok(Self {
            file,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for HardwareLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_acquire_when_locked_then_pid_is_recorded() {
        // Given: un chemin de verrou temporaire.
        // When:  le verrou est acquis.
        // Then:  le fichier contient le PID courant et le verrou est exclusif.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workflow-rs-hw.lock");
        let lock = HardwareLock::acquire(&path, false).unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert_eq!(content.trim(), std::process::id().to_string());
        assert_eq!(lock.path(), path.as_path());
        assert!(HardwareLock::acquire(&path, false).is_err());
        drop(lock);
        let reacquired = HardwareLock::acquire(&path, false).unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap().trim(),
            std::process::id().to_string()
        );
        drop(reacquired);
    }

    #[test]
    fn given_wait_when_lock_is_free_then_acquire_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("hardware.lock");
        let lock = HardwareLock::acquire(&path, true).unwrap();
        assert_eq!(lock.path(), path.as_path());
    }
}
