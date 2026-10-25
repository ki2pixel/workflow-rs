//! Utilitaires réservés aux tests.
//!
//! Les exécutables factices (ffmpeg, unrar, effectcraft-cli) sont écrits par un
//! processus `sh` séparé plutôt qu'avec `fs::write` dans le processus de test.
//! Sinon, un `fork` concurrent d'un autre test peut hériter du descripteur
//! ouvert en écriture et provoquer `ETXTBSY` (« Text file busy ») au moment de
//! l'`exec` du script factice. Le processus d'écriture détient le descripteur,
//! jamais le processus qui lance les tests.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Écrit `content` dans `path`, puis rend le fichier exécutable.
pub fn write_executable(path: &Path, content: &str) {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("cat > \"$1\" && chmod 755 \"$1\"")
        .arg("sh")
        .arg(path)
        .stdin(Stdio::piped())
        .spawn()
        .expect("lancement de l'écriture de l'exécutable factice");
    child
        .stdin
        .as_mut()
        .expect("stdin de l'écriture")
        .write_all(content.as_bytes())
        .expect("écriture du contenu de l'exécutable factice");
    let status = child.wait().expect("attente de l'écriture");
    assert!(status.success(), "écriture de l'exécutable factice");
}
