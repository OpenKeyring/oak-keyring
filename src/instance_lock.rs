use std::fs::{File, OpenOptions};
use std::path::Path;

use fs4::fs_std::FileExt;

const LOCK_FILENAME: &str = ".instance.lock";

#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

#[derive(Debug, thiserror::Error)]
pub enum InstanceLockError {
    #[error("Another oak-keyring instance is already running.\nPlease close the existing instance before starting a new one.")]
    AlreadyRunning,
    #[error("Failed to acquire instance lock: {0}")]
    Io(#[from] std::io::Error),
}

impl InstanceLock {
    pub fn acquire(vault_dir: &Path) -> Result<Self, InstanceLockError> {
        let lock_path = vault_dir.join(LOCK_FILENAME);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)?;

        match file.try_lock_exclusive() {
            Ok(true) => Ok(Self { _file: file }),
            Ok(false) => Err(InstanceLockError::AlreadyRunning),
            Err(e) => Err(InstanceLockError::Io(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_succeeds_when_no_other_instance() {
        let dir = tempfile::tempdir().unwrap();
        let lock = InstanceLock::acquire(dir.path()).unwrap();
        let lock_path = dir.path().join(".instance.lock");
        assert!(lock_path.exists(), "lock file should be created");
        drop(lock);
    }

    #[test]
    fn drop_releases_lock_allow_reacquire() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _lock = InstanceLock::acquire(dir.path()).unwrap();
        }
        let _lock2 = InstanceLock::acquire(dir.path()).unwrap();
    }

    #[test]
    fn second_acquire_in_same_process_fails() {
        let dir = tempfile::tempdir().unwrap();
        let _lock1 = InstanceLock::acquire(dir.path()).unwrap();
        let result = InstanceLock::acquire(dir.path());
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("already running"),
            "error message should mention 'already running', got: {err_msg}"
        );
    }

    #[test]
    fn acquire_creates_lock_file_in_vault_dir() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join(".instance.lock");
        assert!(!lock_path.exists());
        let _lock = InstanceLock::acquire(dir.path()).unwrap();
        assert!(lock_path.exists());
    }
}
