// Health state query wrappers for VaultService.

use uuid::Uuid;

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultService;
use crate::types::health::RecordHealthState;

use super::record::db_error_to_vault;

impl VaultService {
    /// Get the health state for a single record.
    ///
    /// Returns `None` when no health state row exists for the given record.
    pub fn get_record_health_state(
        &self,
        record_id: &Uuid,
    ) -> Result<Option<RecordHealthState>, VaultError> {
        queries::get_record_health_state(&self.conn, record_id).map_err(db_error_to_vault)
    }

    /// List all persisted health states from the `record_health_state` table.
    ///
    /// Returns an empty vector when no rows exist.
    pub fn list_record_health_states(&self) -> Result<Vec<RecordHealthState>, VaultError> {
        queries::list_record_health_states(&self.conn).map_err(db_error_to_vault)
    }

    /// Insert or update a single health state row (test helper, also used by
    /// health-check completion to persist results).
    pub fn upsert_record_health_state(&self, state: &RecordHealthState) -> Result<(), VaultError> {
        queries::upsert_record_health_state(&self.conn, state).map_err(db_error_to_vault)
    }

    /// Atomically replace all health states in a single transaction.
    ///
    /// Deletes every existing row, then inserts the provided slice. Use this
    /// after a full health-check pass to atomically swap the old states for new
    /// ones.
    pub fn replace_record_health_states(
        &self,
        states: &[RecordHealthState],
    ) -> Result<(), VaultError> {
        queries::replace_record_health_states(&self.conn, states).map_err(db_error_to_vault)
    }

    /// Delete the health state for a single record.
    ///
    /// Used when a password or `expires_at` changes, invalidating the previous
    /// health evaluation.
    pub fn delete_record_health_state(&self, record_id: &Uuid) -> Result<(), VaultError> {
        queries::delete_record_health_state(&self.conn, record_id).map_err(db_error_to_vault)?;
        Ok(())
    }

    /// Delete health states for multiple records in a single operation.
    ///
    /// Used by the sync download path to remove stale health states for records
    /// whose cloud record carries no health metadata. An empty `record_ids`
    /// slice is a no-op.
    pub fn delete_record_health_states(&self, record_ids: &[Uuid]) -> Result<(), VaultError> {
        queries::delete_record_health_states(&self.conn, record_ids).map_err(db_error_to_vault)?;
        Ok(())
    }

    /// Advance the `record_version` on an existing health state row.
    ///
    /// Used when a record is updated *without* a password or `expires_at` change
    /// (e.g. editing name, notes, tags) so the existing health state carries
    /// forward to the new version. No-op if no health state row exists.
    pub fn copy_health_state_to_version(
        &self,
        record_id: &Uuid,
        new_record_version: u64,
    ) -> Result<(), VaultError> {
        queries::copy_record_health_state_version(&self.conn, record_id, new_record_version)
            .map_err(db_error_to_vault)?;
        Ok(())
    }

    /// Mark a batch of records as pending sync in the `sync_state` table.
    ///
    /// For each record, upserts a row with `sync_status = Pending` and
    /// `local_updated_at = now`. Existing rows are overwritten; new rows are
    /// created if no prior sync state exists.
    pub fn mark_records_pending_sync(&self, record_ids: &[Uuid]) -> Result<(), VaultError> {
        for id in record_ids {
            queries::upsert_sync_state_pending(&self.conn, id).map_err(db_error_to_vault)?;
        }
        Ok(())
    }

    /// Get a reference to the underlying connection for testing purposes.
    #[cfg(test)]
    pub fn conn_ref(&self) -> &rusqlite::Connection {
        &self.conn
    }
}
