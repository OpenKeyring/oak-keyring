mod audit;
mod health_state;
pub mod health_sync;
mod history;
mod metadata;
mod record;
mod search;
mod tag;
mod trash;

use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;

use crate::crypto::CryptoManager;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;

pub struct VaultService {
    conn: Connection,
    crypto: CryptoManager,
    device_id: String,
}

impl VaultService {
    pub fn new(conn: Connection) -> Self {
        let device_id = queries::get_metadata(&conn, "device_id")
            .ok()
            .flatten()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        Self {
            conn,
            crypto: CryptoManager::new(),
            device_id,
        }
    }

    pub fn unlock(&mut self, path: &Path, cmk: &crate::types::SecureStr) -> Result<(), VaultError> {
        self.crypto
            .unlock(path, cmk)
            .map_err(VaultError::CryptoError)
    }

    /// Unlock the vault using a BIP39 mnemonic (for testing and recovery flows).
    pub fn unlock_with_mnemonic(
        &mut self,
        mnemonic: &crate::crypto::bip39::Passkey,
    ) -> Result<(), VaultError> {
        self.crypto
            .unlock_with_mnemonic(mnemonic)
            .map_err(VaultError::CryptoError)
    }

    pub fn lock(&mut self) {
        self.crypto.lock();
    }

    pub fn is_unlocked(&self) -> bool {
        self.crypto.is_unlocked()
    }

    pub fn checkpoint_wal(&self) -> Result<(), VaultError> {
        crate::db::schema::checkpoint_wal(&self.conn).map_err(VaultError::DatabaseError)
    }

    /// Get current DEK version (delegates to CryptoManager).
    pub(crate) fn current_dek_version(&self) -> u32 {
        self.crypto.current_dek_version()
    }

    /// Write an audit log entry.
    ///
    /// Delegates to `queries::insert_audit_entry` so all SQL goes through the
    /// query layer.
    pub fn write_audit_entry(
        &self,
        operation: crate::types::AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<String>,
        detail: Option<String>,
    ) -> Result<(), VaultError> {
        queries::insert_audit_entry(
            &self.conn,
            operation,
            record_id.as_ref(),
            record_name.as_deref(),
            detail.as_deref(),
        )
        .map_err(record::db_error_to_vault)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::init_db_in_memory;

    /// Helper: create an in-memory VaultService with schema ready.
    fn setup_service() -> VaultService {
        let conn = init_db_in_memory();
        VaultService::new(conn)
    }

    // -- Lock / Unlock Lifecycle Tests -------------------------------------

    /// VaultService starts locked, and lock()/is_unlocked() reflect state correctly.
    #[test]
    fn vault_service_starts_locked() {
        let svc = setup_service();
        assert!(
            !svc.is_unlocked(),
            "new VaultService must start in locked state"
        );
    }

    /// lock() on a locked service is a no-op; is_unlocked() stays false.
    #[test]
    fn lock_when_already_locked_is_noop() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());
        svc.lock();
        assert!(
            !svc.is_unlocked(),
            "locking an already-locked service must remain locked"
        );
    }

    /// Full lifecycle: unlock with mnemonic -> is_unlocked(true) -> lock -> is_unlocked(false).
    /// This tests the delegation to CryptoManager without requiring a real keyfile on disk.
    #[test]
    fn unlock_with_mnemonic_then_lock_lifecycle() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked(), "must start locked");

        // Unlock via mnemonic (no file I/O needed).
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        svc.crypto
            .unlock_with_mnemonic(&mnemonic)
            .expect("unlock_with_mnemonic must succeed in test");
        assert!(
            svc.is_unlocked(),
            "is_unlocked must return true after unlock_with_mnemonic"
        );

        // Lock and verify.
        svc.lock();
        assert!(
            !svc.is_unlocked(),
            "is_unlocked must return false after lock()"
        );
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::VaultService;

    #[test]
    fn checkpoint_wal_delegates_to_database_layer() {
        let conn = crate::db::schema::init_db_in_memory();
        let vault = VaultService::new(conn);

        vault.checkpoint_wal().unwrap();
    }
}
