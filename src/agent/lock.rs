//! Single-instance lock for the SSH agent backend (`ok agent`).
//!
//! Mirrors [`crate::instance_lock::InstanceLock`] but uses a distinct lock file
//! (`.agent.lock`) so the agent daemon and the TUI can run side by side against
//! the same vault directory. A second `ok agent` against the same `data_dir`
//! fails loud with [`AgentLockError::AlreadyRunning`].
//!
//! # Lock file
//!
//! `.agent.lock` lives in the same directory as `.instance.lock` (the vault /
//! `data_dir`). It is created on first acquire and is NOT removed on release —
//! the OS advisory lock is released by closing the file descriptor (which
//! happens when [`AgentLock`] is dropped). This matches `InstanceLock`.
//!
//! [`crate::instance_lock::InstanceLock`]: crate::instance_lock::InstanceLock

use std::fs::{self, File, OpenOptions};
use std::path::Path;

use fs4::fs_std::FileExt;

/// Lock filename within the vault / `data_dir`. Distinct from
/// [`instance_lock::LOCK_FILENAME`] (`.instance.lock`) so the two locks are
/// independent advisory locks on independent inodes.
///
/// [`instance_lock::LOCK_FILENAME`]: crate::instance_lock#lock-filename
const LOCK_FILENAME: &str = ".agent.lock";

/// RAII guard holding the agent's single-instance advisory lock.
///
/// The lock is released when this value is dropped (the inner [`File`] closes,
/// releasing the OS exclusive lock). Hold it for the daemon's entire lifetime.
#[derive(Debug)]
pub struct AgentLock {
    _file: File,
}

/// Errors returned by [`AgentLock::acquire`].
#[derive(Debug, thiserror::Error)]
pub enum AgentLockError {
    /// Another `ok agent` instance is already running against this `data_dir`.
    #[error("Another oak-keyring agent instance is already running.\nPlease stop the existing agent before starting a new one.")]
    AlreadyRunning,
    /// filesystem I/O error while creating or locking the lock file.
    #[error("Failed to acquire agent lock: {0}")]
    Io(#[from] std::io::Error),
}

impl AgentLock {
    /// Acquire the agent single-instance advisory lock on `<data_dir>/.agent.lock`.
    ///
    /// Creates `data_dir` (recursively) and the lock file if missing. On
    /// success the returned [`AgentLock`] holds the exclusive lock for the
    /// daemon's lifetime; dropping it releases the lock. A concurrent holder
    /// yields [`AgentLockError::AlreadyRunning`].
    pub fn acquire(data_dir: &Path) -> Result<Self, AgentLockError> {
        fs::create_dir_all(data_dir)?;
        let lock_path = data_dir.join(LOCK_FILENAME);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;

        match file.try_lock_exclusive() {
            Ok(true) => Ok(Self { _file: file }),
            Ok(false) => Err(AgentLockError::AlreadyRunning),
            Err(e) => Err(AgentLockError::Io(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance_lock::InstanceLock;

    #[test]
    fn acquire_succeeds_when_no_other_holder() {
        let dir = tempfile::tempdir().unwrap();
        let lock = AgentLock::acquire(dir.path()).expect("first acquire should succeed");
        let lock_path = dir.path().join(".agent.lock");
        assert!(lock_path.exists(), "agent lock file should be created");
        drop(lock);
    }

    #[test]
    fn second_acquire_in_same_dir_fails_already_running() {
        let dir = tempfile::tempdir().unwrap();
        let _lock1 = AgentLock::acquire(dir.path()).expect("first acquire should succeed");
        let result = AgentLock::acquire(dir.path());
        assert!(result.is_err(), "second acquire must fail");
        match result.unwrap_err() {
            AgentLockError::AlreadyRunning => {}
            other => panic!("expected AlreadyRunning, got {other:?}"),
        }
    }

    #[test]
    fn drop_releases_lock_allow_reacquire() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _lock = AgentLock::acquire(dir.path()).expect("first acquire should succeed");
        }
        // After the guard dropped, the same dir must be lockable again.
        let _lock2 = AgentLock::acquire(dir.path()).expect("reacquire after drop should succeed");
    }

    #[test]
    fn acquire_creates_data_dir_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b").join("agent");
        assert!(!nested.exists(), "nested data_dir should not exist yet");
        let _lock = AgentLock::acquire(&nested).expect("nested acquire should succeed");
        assert!(nested.exists(), "data_dir should be created");
        assert!(
            nested.join(".agent.lock").exists(),
            "agent lock file should exist inside created data_dir"
        );
    }

    /// Coexistence proof: the agent lock and the TUI lock are independent
    /// advisory locks on distinct inodes within the same directory. Holding
    /// one MUST NOT block the other — otherwise `ok agent` and `ok` (TUI)
    /// could not run simultaneously against the same vault.
    #[test]
    fn agent_lock_and_instance_lock_coexist_on_same_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Acquire both locks against the SAME dir; both must succeed.
        let _agent_lock = AgentLock::acquire(dir.path()).expect("agent lock should be acquired");
        let _tui_lock =
            InstanceLock::acquire(dir.path()).expect("TUI instance lock should coexist");
        // Distinct files in the same dir.
        assert!(dir.path().join(".agent.lock").exists());
        assert!(dir.path().join(".instance.lock").exists());
    }
}
