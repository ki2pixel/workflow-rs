//! Suivi des sous-processus longs et arrêt propre sur signal.
//!
//! Le serveur tue le groupe de processus du CLI, mais un lancement manuel
//! `workflow-cli stepN run` peut recevoir `SIGTERM` seul. Les étapes 1 à 4
//! enregistrent donc leurs enfants (`ffmpeg`, `unrar`) afin qu'un gestionnaire
//! de signal puisse les terminer avant que le CLI ne quitte.
//!
//! Le gestionnaire n'utilise que des opérations sûres en contexte de signal :
//! lecture d'atomics et `kill(2)`, puis `_exit(2)`.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

pub const MAX_TRACKED_CHILDREN: usize = 64;

static CHILD_PIDS: [AtomicI32; MAX_TRACKED_CHILDREN] =
    [const { AtomicI32::new(0) }; MAX_TRACKED_CHILDREN];
static HANDLER_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Réserve un emplacement pour un PID enfant. Le `Drop` libère l'emplacement.
pub struct ChildGuard {
    slot: usize,
}

impl ChildGuard {
    pub fn register(pid: u32) -> Option<Self> {
        if pid == 0 {
            return None;
        }
        for (slot, cell) in CHILD_PIDS.iter().enumerate() {
            if cell
                .compare_exchange(0, pid as i32, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return Some(Self { slot });
            }
        }
        tracing::warn!(
            pid,
            "plus d'emplacement libre pour suivre un sous-processus long"
        );
        None
    }

    pub fn pid(&self) -> u32 {
        CHILD_PIDS[self.slot].load(Ordering::SeqCst).max(0) as u32
    }

    pub fn terminate(&self, signal: i32) {
        terminate_pid(self.pid(), signal);
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        CHILD_PIDS[self.slot].store(0, Ordering::SeqCst);
    }
}

fn terminate_pid(pid: u32, signal: i32) {
    if pid == 0 {
        return;
    }
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, signal);
    }
    #[cfg(not(unix))]
    let _ = (pid, signal);
}

fn terminate_registered(signal: i32) {
    for cell in CHILD_PIDS.iter() {
        let pid = cell.load(Ordering::SeqCst);
        if pid > 0 {
            terminate_pid(pid as u32, signal);
        }
    }
}

/// Installe le gestionnaire `SIGTERM`/`SIGINT`. À n'appeler que pour les
/// étapes qui n'ont pas déjà leur propre gestionnaire (STEP8 en possède un).
pub fn install_shutdown_handler() {
    if HANDLER_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    #[cfg(unix)]
    unsafe {
        libc::signal(
            libc::SIGTERM,
            handle_signal as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            handle_signal as *const () as libc::sighandler_t,
        );
    }
}

#[cfg(unix)]
extern "C" fn handle_signal(signal: libc::c_int) {
    terminate_registered(libc::SIGTERM);
    // Les fichiers `.part`/`.tmp` résiduels sont purgés au démarrage suivant.
    unsafe {
        libc::_exit(128 + signal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    #[test]
    #[allow(clippy::zombie_processes)] // le test tue et attend l'enfant sur chaque chemin
    fn given_registered_child_when_terminated_then_it_exits() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let guard = ChildGuard::register(child.id()).unwrap();
        assert_eq!(guard.pid(), child.id());
        guard.terminate(libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        panic!("le sous-processus enregistré a survécu à SIGTERM");
    }

    #[cfg(unix)]
    #[test]
    #[allow(clippy::zombie_processes)] // kill/wait best-effort avant les assertions
    fn given_child_guard_dropped_when_new_child_registered_then_slot_reused() {
        let mut child = Command::new("sh")
            .args(["-c", "sleep 5"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let pid = child.id();
        {
            let guard = ChildGuard::register(pid).unwrap();
            assert_eq!(guard.pid(), pid);
        }
        let guard = ChildGuard::register(pid).unwrap();
        assert_eq!(guard.pid(), pid);
        let _ = child.kill();
        let _ = child.wait();
    }
}
