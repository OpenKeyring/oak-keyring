// Metadata operations (get/set/delete metadata)

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;

use super::VaultService;

impl VaultService {
    /// Retrieve a metadata value by key.
    ///
    /// Returns `Ok(None)` if the key does not exist.
    /// Returns `VaultError::DatabaseError` if the query fails.
    pub fn get_metadata(&self, key: &str) -> Result<Option<String>, VaultError> {
        queries::get_metadata(&self.conn, key).map_err(|e| match e {
            queries::DbError::Sqlite(se) => VaultError::DatabaseError(se),
            other => VaultError::CryptoError(other.to_string()),
        })
    }

    /// Set a metadata key-value pair (upsert).
    ///
    /// Uses `INSERT OR REPLACE` semantics — if the key already exists, its
    /// value is overwritten.
    pub fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), VaultError> {
        queries::set_metadata(&self.conn, key, value).map_err(|e| match e {
            queries::DbError::Sqlite(se) => VaultError::DatabaseError(se),
            other => VaultError::CryptoError(other.to_string()),
        })
    }

    /// Delete a metadata entry by key.
    ///
    /// Returns `Ok(())` whether the key existed or not (idempotent).
    pub fn delete_metadata(&mut self, key: &str) -> Result<(), VaultError> {
        queries::delete_metadata(&self.conn, key).map_err(|e| match e {
            queries::DbError::Sqlite(se) => VaultError::DatabaseError(se),
            other => VaultError::CryptoError(other.to_string()),
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use crate::db::schema::{initialize_metadata, initialize_schema};

    use super::*;

    /// Helper: create an in-memory VaultService with schema initialized.
    fn setup_service() -> VaultService {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn);
        initialize_metadata(&conn);
        VaultService::new(conn)
    }

    // =========================================================================
    // get_metadata tests
    // =========================================================================

    // --- get_metadata: returns value for existing key ---

    #[test]
    fn get_metadata_returns_value_for_existing_key() {
        let mut svc = setup_service();
        svc.set_metadata("test_key", "test_value")
            .expect("set_metadata must succeed");

        let result = svc
            .get_metadata("test_key")
            .expect("get_metadata must succeed");
        assert_eq!(result, Some("test_value".to_string()));
    }

    // --- get_metadata: returns None for nonexistent key ---

    #[test]
    fn get_metadata_returns_none_for_nonexistent_key() {
        let svc = setup_service();

        let result = svc
            .get_metadata("nonexistent_key")
            .expect("get_metadata must succeed");
        assert!(result.is_none());
    }

    // --- get_metadata: returns device_id from initialization ---

    #[test]
    fn get_metadata_returns_device_id_from_initialization() {
        let svc = setup_service();

        let device_id = svc
            .get_metadata("device_id")
            .expect("get_metadata must succeed");
        assert!(device_id.is_some(), "device_id should exist in metadata");
    }

    // --- get_metadata: returns DatabaseError on missing table ---

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

    // =========================================================================
    // set_metadata tests
    // =========================================================================

    // --- set_metadata: inserts new key-value pair ---

    #[test]
    fn set_metadata_inserts_new_key_value_pair() {
        let mut svc = setup_service();

        svc.set_metadata("new_key", "new_value")
            .expect("set_metadata must succeed");

        let result = svc
            .get_metadata("new_key")
            .expect("get_metadata must succeed");
        assert_eq!(result, Some("new_value".to_string()));
    }

    // --- set_metadata: overwrites existing value (upsert) ---

    #[test]
    fn set_metadata_overwrites_existing_value() {
        let mut svc = setup_service();

        svc.set_metadata("upsert_key", "v1")
            .expect("set_metadata must succeed");
        svc.set_metadata("upsert_key", "v2")
            .expect("set_metadata must succeed");

        let result = svc
            .get_metadata("upsert_key")
            .expect("get_metadata must succeed");
        assert_eq!(result, Some("v2".to_string()));
    }

    // =========================================================================
    // delete_metadata tests
    // =========================================================================

    // --- delete_metadata: removes existing key ---

    #[test]
    fn delete_metadata_removes_existing_key() {
        let mut svc = setup_service();
        svc.set_metadata("to_delete", "value")
            .expect("set_metadata must succeed");

        svc.delete_metadata("to_delete")
            .expect("delete_metadata must succeed");

        let result = svc
            .get_metadata("to_delete")
            .expect("get_metadata must succeed");
        assert!(result.is_none(), "key should be gone after delete_metadata");
    }

    // --- delete_metadata: is idempotent for nonexistent key ---

    #[test]
    fn delete_metadata_is_idempotent_for_nonexistent_key() {
        let mut svc = setup_service();

        // Deleting a key that does not exist should succeed silently.
        svc.delete_metadata("ghost_key")
            .expect("delete_metadata must succeed for nonexistent key");
    }
}
