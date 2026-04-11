mod audit;
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
use crate::errors::mapping::vault::VaultError;

pub struct VaultService {
    conn: Connection,
    crypto: CryptoManager,
    device_id: String,
}

impl VaultService {
    pub fn new(conn: Connection) -> Self {
        let device_id = conn
            .query_row(
                "SELECT value FROM metadata WHERE key = 'device_id'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_else(|_| Uuid::new_v4().to_string());
        Self {
            conn,
            crypto: CryptoManager::new(),
            device_id,
        }
    }

    pub fn unlock(&mut self, path: &Path, cmk: &str) -> Result<(), VaultError> {
        self.crypto
            .unlock(path, cmk)
            .map_err(VaultError::CryptoError)
    }

    pub fn lock(&mut self) {
        self.crypto.lock();
    }

    pub fn is_unlocked(&self) -> bool {
        self.crypto.is_unlocked()
    }

    pub fn soft_delete(&mut self, id: Uuid) -> Result<(), VaultError> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "UPDATE records SET deleted = 1, deleted_at = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3 AND deleted = 0",
            rusqlite::params![now, now, id.to_string()],
        )?;
        Ok(())
    }

    pub fn restore(&mut self, id: Uuid) -> Result<(), VaultError> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "UPDATE records SET deleted = 0, deleted_at = NULL, updated_at = ?1, version = version + 1 WHERE id = ?2 AND deleted = 1",
            rusqlite::params![now, id.to_string()],
        )?;
        Ok(())
    }

    pub fn write_audit_entry(
        &mut self,
        operation: crate::types::AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<String>,
        detail: Option<String>,
    ) -> Result<(), VaultError> {
        let now = chrono::Utc::now().timestamp();
        self.conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                operation.to_db_str(),
                record_id.map(|id| id.to_string()),
                record_name,
                detail,
                now,
            ],
        )?;
        Ok(())
    }

    pub fn create_tag(&mut self, name: &str) -> Result<i64, VaultError> {
        self.conn.execute(
            "INSERT INTO tags (name) VALUES (?1)",
            rusqlite::params![name],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn add_tag_to_record(&mut self, record_id: Uuid, tag_id: i64) -> Result<(), VaultError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![record_id.to_string(), tag_id],
        )?;
        Ok(())
    }

    pub fn get_metadata(&self, key: &str) -> Result<Option<String>, VaultError> {
        let result = self.conn.query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            rusqlite::params![key],
            |r| r.get::<_, String>(0),
        );
        match result {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(VaultError::DatabaseError(e)),
        }
    }

    pub fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), VaultError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
            rusqlite::params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::{initialize_metadata, initialize_schema};

    /// Helper: create an in-memory VaultService with schema ready.
    fn setup_service() -> VaultService {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn);
        initialize_metadata(&conn);
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

    // -- Error Type Migration Tests ----------------------------------------

    /// Methods returning errors must produce VaultError variants, not String.
    #[test]
    fn get_metadata_returns_vault_error_on_db_failure() {
        // Use a connection without schema to trigger a database error.
        let conn = Connection::open_in_memory().unwrap();
        let svc = VaultService::new(conn);

        let result = svc.get_metadata("no_schema_key");
        // Without schema, the metadata table doesn't exist -> DatabaseError variant.
        assert!(
            result.is_err(),
            "get_metadata on missing table must return error"
        );
        let err = result.unwrap_err();
        assert!(
            matches!(err, VaultError::DatabaseError(_)),
            "expected DatabaseError variant, got: {:?}",
            err
        );
    }
}
