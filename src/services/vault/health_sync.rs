//! Health sync adapter that bridges VaultService to the sync pipeline.
//!
//! Provides a concrete `HealthSyncAdapter` implementation backed by
//! `VaultService`. Because the sync pipeline runs stages sequentially and
//! does not need concurrent DB access, the adapter reads the current snapshot
//! of health states at construction time and buffers writes that are flushed
//! back to the database when `flush()` is called.

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultService;
use crate::sync::pipeline::HealthSyncAdapter;
use crate::types::health::RecordHealthState;

/// Adapter that connects the sync pipeline to VaultService for health state
/// read/write operations.
///
/// # Lifecycle
///
/// 1. `new(vault)` — snapshots all current health states from the DB.
/// 2. Pipeline calls `get_health_state()` / `persist_health_states()` /
///    `delete_health_states()` during execution.
/// 3. Caller invokes `flush(vault)` to write buffered changes to the DB.
pub struct VaultHealthSyncAdapter {
    /// Snapshot of health states at construction time.
    states: HashMap<Uuid, RecordHealthState>,
    /// Buffered health states to upsert on flush.
    pending_upserts: Mutex<Vec<RecordHealthState>>,
    /// Buffered record IDs whose health states should be deleted on flush.
    pending_deletes: Mutex<Vec<Uuid>>,
}

impl VaultHealthSyncAdapter {
    /// Creates a new adapter by reading all current health states from the
    /// vault service.
    ///
    /// Returns an adapter with an empty snapshot if reading from the database
    /// fails (graceful degradation for sync scenarios).
    pub fn new(vault: &VaultService) -> Self {
        let states = match vault.list_record_health_states() {
            Ok(list) => list.into_iter().map(|s| (s.record_id, s)).collect(),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to load health states for sync adapter, using empty snapshot"
                );
                HashMap::new()
            }
        };

        Self {
            states,
            pending_upserts: Mutex::new(Vec::new()),
            pending_deletes: Mutex::new(Vec::new()),
        }
    }

    /// Flush buffered health state changes to the database.
    ///
    /// Must be called after the pipeline completes to persist:
    /// - Upserted health states (from download path)
    /// - Deleted health states (for records with no cloud health metadata)
    pub fn flush(&self, vault: &VaultService) -> Result<(), VaultError> {
        let upserts = self.pending_upserts.lock().unwrap();
        for state in upserts.iter() {
            vault.upsert_record_health_state(state)?;
        }

        let deletes = self.pending_deletes.lock().unwrap();
        if !deletes.is_empty() {
            vault.delete_record_health_states(&deletes)?;
        }

        if !upserts.is_empty() || !deletes.is_empty() {
            tracing::info!(
                upserted = upserts.len(),
                deleted = deletes.len(),
                "Flushed health state changes from sync pipeline"
            );
        }

        Ok(())
    }

    /// Returns the number of pending upserts (for testing).
    pub fn pending_upsert_count(&self) -> usize {
        self.pending_upserts.lock().unwrap().len()
    }

    /// Returns the number of pending deletes (for testing).
    pub fn pending_delete_count(&self) -> usize {
        self.pending_deletes.lock().unwrap().len()
    }
}

impl HealthSyncAdapter for VaultHealthSyncAdapter {
    fn get_health_state(&self, record_id: &Uuid) -> Option<RecordHealthState> {
        self.states.get(record_id).cloned()
    }

    fn persist_health_states(&self, states: &[RecordHealthState]) {
        self.pending_upserts
            .lock()
            .unwrap()
            .extend_from_slice(states);
    }

    fn delete_health_states(&self, record_ids: &[Uuid]) {
        self.pending_deletes
            .lock()
            .unwrap()
            .extend_from_slice(record_ids);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl std::fmt::Debug for VaultHealthSyncAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultHealthSyncAdapter")
            .field("snapshot_count", &self.states.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries;
    use crate::db::schema::init_db_in_memory;
    use crate::services::vault::VaultService;
    use crate::types::credential::CredentialType;
    use crate::types::record::StoredRecord;
    use chrono::Utc;

    fn setup_vault() -> VaultService {
        let conn = init_db_in_memory();
        VaultService::new(conn)
    }

    /// Insert a bare-minimum StoredRecord so FK constraints on
    /// `record_health_state.record_id` are satisfied.
    fn insert_stub_record(vault: &VaultService, id: Uuid) {
        let record = StoredRecord {
            id,
            credential_type: CredentialType::Login,
            encrypted_data: vec![0u8; 16],
            nonce: [0u8; 24],
            dek_version: 1,
            aad: vec![],
            is_favorite: false,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            updated_by: "test".to_string(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
        };
        queries::insert_record(vault.conn_ref(), &record).unwrap();
    }

    #[test]
    fn adapter_reads_existing_health_states() {
        let vault = setup_vault();

        let id = Uuid::new_v4();
        insert_stub_record(&vault, id);

        let state = RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: None,
        };
        vault.upsert_record_health_state(&state).unwrap();

        let adapter = VaultHealthSyncAdapter::new(&vault);
        let result = adapter.get_health_state(&id);
        assert!(result.is_some());
        assert_eq!(result.unwrap().weak_password, Some(true));
    }

    #[test]
    fn adapter_returns_none_for_missing_record() {
        let vault = setup_vault();
        let adapter = VaultHealthSyncAdapter::new(&vault);
        assert!(adapter.get_health_state(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn adapter_buffers_and_flushes_upserts() {
        let vault = setup_vault();

        let id = Uuid::new_v4();
        insert_stub_record(&vault, id);

        let state = RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(true),
            duplicate_group_size: Some(3),
            compromised: Some(false),
            expired: Some(false),
        };

        let adapter = VaultHealthSyncAdapter::new(&vault);
        adapter.persist_health_states(std::slice::from_ref(&state));
        assert_eq!(adapter.pending_upsert_count(), 1);

        // Not yet in the DB
        assert!(vault.get_record_health_state(&id).unwrap().is_none());

        // Flush writes to DB
        adapter.flush(&vault).unwrap();

        // Now it's in the DB
        let persisted = vault.get_record_health_state(&id).unwrap().unwrap();
        assert_eq!(persisted.weak_password, Some(true));
        assert_eq!(persisted.duplicate_group_size, Some(3));
    }

    #[test]
    fn adapter_buffers_and_flushes_deletes() {
        let vault = setup_vault();

        let id = Uuid::new_v4();
        insert_stub_record(&vault, id);

        let state = RecordHealthState {
            record_id: id,
            record_version: 1,
            evaluated_at: Some(Utc::now()),
            weak_password: Some(false),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: None,
        };
        vault.upsert_record_health_state(&state).unwrap();

        let adapter = VaultHealthSyncAdapter::new(&vault);
        adapter.delete_health_states(&[id]);
        assert_eq!(adapter.pending_delete_count(), 1);

        // Still in DB before flush
        assert!(vault.get_record_health_state(&id).unwrap().is_some());

        adapter.flush(&vault).unwrap();

        // Now deleted
        assert!(vault.get_record_health_state(&id).unwrap().is_none());
    }

    #[test]
    fn adapter_handles_empty_flush_gracefully() {
        let vault = setup_vault();
        let adapter = VaultHealthSyncAdapter::new(&vault);
        assert!(adapter.flush(&vault).is_ok());
    }
}
