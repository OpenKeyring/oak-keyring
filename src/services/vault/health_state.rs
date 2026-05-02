// Health state query wrappers for VaultService.

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultService;
use crate::types::health::RecordHealthState;

use super::record::db_error_to_vault;

impl VaultService {
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
}
