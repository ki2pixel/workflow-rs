use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Verrou exclusif inter-processus.
///
/// Le verrou est utilisé pour sérialiser les étapes qui touchent le GPU ou le
/// Coral. STEP5 et STEP6 n'en ont pas besoin, mais le type est partagé par le
/// serveur et les étapes matérielles.
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
        let file = OpenOptions::new()
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
