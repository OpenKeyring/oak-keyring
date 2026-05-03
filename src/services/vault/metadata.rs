// Metadata operations (get/set/delete metadata)

use chrono::{DateTime, Utc};

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;

use super::VaultService;

/// Metadata key for the last successful health check timestamp.
const LAST_HEALTH_CHECK_AT_KEY: &str = "last_health_check_at";

/// Metadata key for the timestamp when HIBP entered degraded mode.
const LAST_HIBP_DEGRADED_AT_KEY: &str = "last_hibp_degraded_at";

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

    /// Get the timestamp of the last successful health check.
    ///
    /// Returns `Ok(None)` if the key does not exist.
    /// If the stored value is corrupted (not a valid RFC 3339 timestamp),
    /// logs a warning and returns `Ok(None)` instead of erroring.
    pub fn get_last_health_check_at(&self) -> Result<Option<DateTime<Utc>>, VaultError> {
        match self.get_metadata(LAST_HEALTH_CHECK_AT_KEY)? {
            Some(raw) => match raw.parse::<DateTime<Utc>>() {
                Ok(dt) => Ok(Some(dt)),
                Err(e) => {
                    tracing::warn!(
                        key = LAST_HEALTH_CHECK_AT_KEY,
                        error = %e,
                        raw_value = %raw,
                        "corrupted metadata value, treating as missing"
                    );
                    Ok(None)
                }
            },
            None => Ok(None),
        }
    }

    /// Set the timestamp of the last successful health check.
    ///
    /// The timestamp is stored as UTC RFC 3339.
    pub fn set_last_health_check_at(&mut self, at: DateTime<Utc>) -> Result<(), VaultError> {
        self.set_metadata(LAST_HEALTH_CHECK_AT_KEY, &at.to_rfc3339())
    }

    /// Get the timestamp when HIBP last entered degraded mode.
    ///
    /// Returns `Ok(None)` if the key does not exist.
    /// Corrupted values are treated as missing with a warning.
    pub fn get_last_hibp_degraded_at(&self) -> Result<Option<DateTime<Utc>>, VaultError> {
        match self.get_metadata(LAST_HIBP_DEGRADED_AT_KEY)? {
            Some(raw) => match raw.parse::<DateTime<Utc>>() {
                Ok(dt) => Ok(Some(dt)),
                Err(e) => {
                    tracing::warn!(
                        key = LAST_HIBP_DEGRADED_AT_KEY,
                        error = %e,
                        raw_value = %raw,
                        "corrupted metadata value, treating as missing"
                    );
                    Ok(None)
                }
            },
            None => Ok(None),
        }
    }

    /// Set the timestamp when HIBP entered degraded mode.
    ///
    /// The timestamp is stored as UTC RFC 3339.
    pub fn set_last_hibp_degraded_at(&mut self, at: DateTime<Utc>) -> Result<(), VaultError> {
        self.set_metadata(LAST_HIBP_DEGRADED_AT_KEY, &at.to_rfc3339())
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

    // =========================================================================
    // get_last_health_check_at / set_last_health_check_at tests
    // =========================================================================

    // --- round-trip: set then get preserves the timestamp ---

    #[test]
    fn last_health_check_round_trip() {
        let mut svc = setup_service();
        let ts = Utc::now();

        svc.set_last_health_check_at(ts)
            .expect("set_last_health_check_at must succeed");

        let result = svc
            .get_last_health_check_at()
            .expect("get_last_health_check_at must succeed");
        assert!(result.is_some(), "should return Some after set");
        // RFC 3339 round-trip may lose sub-second precision, compare within 1s.
        let stored = result.unwrap();
        let diff = (stored - ts).num_seconds().abs();
        assert!(
            diff <= 1,
            "round-tripped timestamp should be within 1s, got diff={diff}"
        );
    }

    // --- missing key returns None ---

    #[test]
    fn last_health_check_returns_none_when_missing() {
        let svc = setup_service();

        let result = svc
            .get_last_health_check_at()
            .expect("get_last_health_check_at must succeed");
        assert!(
            result.is_none(),
            "should return None when key has never been set"
        );
    }

    // --- corrupted value returns None (not error) ---

    #[test]
    fn last_health_check_returns_none_on_corrupted_value() {
        let mut svc = setup_service();
        // Write garbage directly via set_metadata.
        svc.set_metadata("last_health_check_at", "not-a-valid-timestamp")
            .expect("set_metadata must succeed");

        let result = svc
            .get_last_health_check_at()
            .expect("get_last_health_check_at must succeed on corrupted value");
        assert!(
            result.is_none(),
            "corrupted value should be treated as missing"
        );
    }

    // --- set overwrites previous value ---

    #[test]
    fn last_health_check_overwrites_previous() {
        let mut svc = setup_service();
        let ts1 = Utc::now() - chrono::Duration::hours(1);
        let ts2 = Utc::now();

        svc.set_last_health_check_at(ts1)
            .expect("first set must succeed");
        svc.set_last_health_check_at(ts2)
            .expect("second set must succeed");

        let result = svc
            .get_last_health_check_at()
            .expect("get must succeed")
            .expect("should be Some");
        let diff = (result - ts2).num_seconds().abs();
        assert!(diff <= 1, "should return the second (latest) timestamp");
    }

    // =========================================================================
    // get_last_hibp_degraded_at / set_last_hibp_degraded_at tests
    // =========================================================================

    // --- round-trip: set then get preserves the timestamp ---

    #[test]
    fn last_hibp_degraded_round_trip() {
        let mut svc = setup_service();
        let ts = Utc::now();

        svc.set_last_hibp_degraded_at(ts)
            .expect("set_last_hibp_degraded_at must succeed");

        let result = svc
            .get_last_hibp_degraded_at()
            .expect("get_last_hibp_degraded_at must succeed");
        assert!(result.is_some(), "should return Some after set");
        let stored = result.unwrap();
        let diff = (stored - ts).num_seconds().abs();
        assert!(
            diff <= 1,
            "round-tripped timestamp should be within 1s, got diff={diff}"
        );
    }

    // --- missing key returns None ---

    #[test]
    fn last_hibp_degraded_returns_none_when_missing() {
        let svc = setup_service();

        let result = svc
            .get_last_hibp_degraded_at()
            .expect("get_last_hibp_degraded_at must succeed");
        assert!(
            result.is_none(),
            "should return None when key has never been set"
        );
    }

    // --- corrupted value returns None (not error) ---

    #[test]
    fn last_hibp_degraded_returns_none_on_corrupted_value() {
        let mut svc = setup_service();
        svc.set_metadata("last_hibp_degraded_at", "garbage!!!")
            .expect("set_metadata must succeed");

        let result = svc
            .get_last_hibp_degraded_at()
            .expect("get_last_hibp_degraded_at must succeed on corrupted value");
        assert!(
            result.is_none(),
            "corrupted value should be treated as missing"
        );
    }
}
