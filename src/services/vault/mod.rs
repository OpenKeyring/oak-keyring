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

use crate::commands::{AuditFilter, FieldSelector, RecordFilter, RecordSort};
use crate::crypto::bip39::Passkey;
use crate::crypto::CryptoManager;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::tag::TagSortMeta;
use crate::types::{
    AuditOperation, PasswordHistory, RecordHealthState, SecureStr, StoredRecord, Tag,
};

/// Vault trait for executor-facing vault operations.
///
/// This trait defines the complete interface used by the executor layer
/// for vault operations including lifecycle, record CRUD, search, tags,
/// metadata, audit logging, password history, health state, and rotation support.
#[cfg_attr(test, mockall::automock)]
pub trait Vault: Send {
    // ------------------------------------------------------------------------
    // Lifecycle
    // ------------------------------------------------------------------------

    fn unlock(&mut self, path: &Path, cmk: &SecureStr) -> Result<(), VaultError>;
    fn unlock_with_mnemonic(&mut self, mnemonic: &Passkey) -> Result<(), VaultError>;
    fn lock(&mut self);
    fn is_unlocked(&self) -> bool;
    fn checkpoint_wal(&self) -> Result<(), VaultError>;

    /// Get current DEK version.
    /// Used by rotation service to determine if migration is needed.
    fn current_dek_version(&self) -> u32;

    // ------------------------------------------------------------------------
    // Record CRUD
    // ------------------------------------------------------------------------

    fn create_record(
        &mut self,
        params: crate::types::record::CreateRecordParams,
    ) -> Result<Uuid, VaultError>;
    fn update_record(
        &mut self,
        params: crate::types::record::UpdateRecordParams,
    ) -> Result<(), VaultError>;
    fn get_decrypted_record(
        &mut self,
        id: Uuid,
    ) -> Result<crate::types::record::DecryptedRecord, VaultError>;
    fn decrypt_field(&self, id: Uuid, field: FieldSelector) -> Result<SecureStr, VaultError>;
    fn soft_delete_record(&mut self, id: Uuid) -> Result<(), VaultError>;
    fn restore_record(&mut self, id: Uuid) -> Result<(), VaultError>;
    fn hard_delete_record(&mut self, id: Uuid) -> Result<(), VaultError>;
    fn toggle_favorite(&mut self, id: Uuid, is_favorite: bool) -> Result<(), VaultError>;
    fn get_stored_record(&self, id: Uuid)
        -> Result<crate::types::record::StoredRecord, VaultError>;

    // ------------------------------------------------------------------------
    // Listing/Search
    // ------------------------------------------------------------------------

    fn list_records(
        &self,
        filter: &RecordFilter,
        sort: &RecordSort,
    ) -> Result<Vec<crate::types::record::TuiRecord>, VaultError>;
    fn list_all_stored_records(&self) -> Result<Vec<StoredRecord>, VaultError>;

    // ------------------------------------------------------------------------
    // Password History
    // ------------------------------------------------------------------------

    fn get_password_history(&self, record_id: Uuid) -> Result<Vec<PasswordHistory>, VaultError>;
    fn decrypt_history_password(&self, history_id: i64) -> Result<SecureStr, VaultError>;

    // ------------------------------------------------------------------------
    // Tags
    // ------------------------------------------------------------------------

    fn list_tags_with_stats(&self) -> Result<Vec<(Tag, TagSortMeta)>, VaultError>;
    fn rename_tag(&mut self, old_name: &str, new_name: &str) -> Result<(), VaultError>;
    fn delete_tag(&mut self, name: &str) -> Result<(), VaultError>;
    fn batch_add_tag(&mut self, record_ids: &[Uuid], tag_name: &str) -> Result<usize, VaultError>;
    fn batch_remove_tag(
        &mut self,
        record_ids: &[Uuid],
        tag_name: &str,
    ) -> Result<usize, VaultError>;

    // ------------------------------------------------------------------------
    // Batch Operations
    // ------------------------------------------------------------------------

    fn batch_soft_delete(&mut self, record_ids: &[Uuid]) -> Result<usize, VaultError>;
    fn empty_trash(&mut self) -> Result<usize, VaultError>;

    // ------------------------------------------------------------------------
    // Health State
    // ------------------------------------------------------------------------

    fn list_record_health_states(&self) -> Result<Vec<RecordHealthState>, VaultError>;
    fn get_record_health_state(
        &self,
        record_id: &Uuid,
    ) -> Result<Option<RecordHealthState>, VaultError>;
    fn upsert_record_health_state(&self, state: &RecordHealthState) -> Result<(), VaultError>;
    fn replace_record_health_states(
        &self,
        new_states: &[RecordHealthState],
    ) -> Result<(), VaultError>;
    fn delete_record_health_state(&self, record_id: &Uuid) -> Result<(), VaultError>;
    fn delete_record_health_states(&self, record_ids: &[Uuid]) -> Result<(), VaultError>;
    fn copy_health_state_to_version(
        &self,
        record_id: &Uuid,
        new_record_version: u64,
    ) -> Result<(), VaultError>;
    fn mark_records_pending_sync(&self, record_ids: &[Uuid]) -> Result<(), VaultError>;

    // ------------------------------------------------------------------------
    // Metadata
    // ------------------------------------------------------------------------

    fn get_metadata(&self, key: &str) -> Result<Option<String>, VaultError>;
    fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), VaultError>;
    fn get_last_health_check_at(&self)
        -> Result<Option<chrono::DateTime<chrono::Utc>>, VaultError>;
    fn set_last_health_check_at(
        &mut self,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), VaultError>;

    // ------------------------------------------------------------------------
    // Audit
    // ------------------------------------------------------------------------

    fn write_audit_entry(
        &self,
        operation: AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<String>,
        detail: Option<String>,
    ) -> Result<(), VaultError>;
    fn query_audit_log(
        &self,
        filter: &AuditFilter,
    ) -> Result<(Vec<crate::types::audit::AuditEntry>, usize), VaultError>;

    // ------------------------------------------------------------------------
    // Sync
    // ------------------------------------------------------------------------

    fn apply_downloaded_cloud_record(
        &mut self,
        record: &crate::cloud::CloudRecord,
    ) -> Result<bool, VaultError>;
    fn decrypt_record_name_for_sync(&self, record: &StoredRecord) -> Result<String, VaultError>;
    fn load_sync_status_map(
        &self,
    ) -> std::collections::HashMap<String, crate::types::sync::SyncStatus>;

    // ------------------------------------------------------------------------
    // Rotation
    // ------------------------------------------------------------------------

    fn re_encrypt_record(
        &mut self,
        record_id: Uuid,
        old_dek_version: u32,
    ) -> Result<(), VaultError>;
    fn list_records_for_migration(
        &self,
        target_dek_version: u32,
    ) -> Result<Vec<StoredRecord>, VaultError>;
    fn log_dek_rotated(&self, detail: &str) -> Result<(), VaultError>;
    fn delete_metadata(&mut self, key: &str) -> Result<(), VaultError>;
}

pub struct VaultServiceImpl {
    conn: Connection,
    crypto: CryptoManager,
    device_id: String,
}

/// Temporary type alias for migration.
/// Prefer VaultServiceImpl for construction and Vault for trait usage.
pub type VaultService = VaultServiceImpl;

impl Vault for Box<dyn Vault> {
    fn unlock(&mut self, path: &Path, cmk: &SecureStr) -> Result<(), VaultError> {
        (**self).unlock(path, cmk)
    }
    fn unlock_with_mnemonic(&mut self, mnemonic: &Passkey) -> Result<(), VaultError> {
        (**self).unlock_with_mnemonic(mnemonic)
    }
    fn lock(&mut self) {
        (**self).lock();
    }
    fn is_unlocked(&self) -> bool {
        (**self).is_unlocked()
    }
    fn checkpoint_wal(&self) -> Result<(), VaultError> {
        (**self).checkpoint_wal()
    }
    fn current_dek_version(&self) -> u32 {
        (**self).current_dek_version()
    }
    fn create_record(
        &mut self,
        params: crate::types::record::CreateRecordParams,
    ) -> Result<Uuid, VaultError> {
        (**self).create_record(params)
    }
    fn update_record(
        &mut self,
        params: crate::types::record::UpdateRecordParams,
    ) -> Result<(), VaultError> {
        (**self).update_record(params)
    }
    fn get_decrypted_record(
        &mut self,
        id: Uuid,
    ) -> Result<crate::types::record::DecryptedRecord, VaultError> {
        (**self).get_decrypted_record(id)
    }
    fn decrypt_field(&self, id: Uuid, field: FieldSelector) -> Result<SecureStr, VaultError> {
        (**self).decrypt_field(id, field)
    }
    fn soft_delete_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        (**self).soft_delete_record(id)
    }
    fn restore_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        (**self).restore_record(id)
    }
    fn hard_delete_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        (**self).hard_delete_record(id)
    }
    fn toggle_favorite(&mut self, id: Uuid, is_favorite: bool) -> Result<(), VaultError> {
        (**self).toggle_favorite(id, is_favorite)
    }
    fn get_stored_record(
        &self,
        id: Uuid,
    ) -> Result<crate::types::record::StoredRecord, VaultError> {
        (**self).get_stored_record(id)
    }
    fn list_records(
        &self,
        filter: &RecordFilter,
        sort: &RecordSort,
    ) -> Result<Vec<crate::types::record::TuiRecord>, VaultError> {
        (**self).list_records(filter, sort)
    }
    fn list_all_stored_records(&self) -> Result<Vec<StoredRecord>, VaultError> {
        (**self).list_all_stored_records()
    }
    fn get_password_history(&self, record_id: Uuid) -> Result<Vec<PasswordHistory>, VaultError> {
        (**self).get_password_history(record_id)
    }
    fn decrypt_history_password(&self, history_id: i64) -> Result<SecureStr, VaultError> {
        (**self).decrypt_history_password(history_id)
    }
    fn list_tags_with_stats(&self) -> Result<Vec<(Tag, TagSortMeta)>, VaultError> {
        (**self).list_tags_with_stats()
    }
    fn rename_tag(&mut self, old_name: &str, new_name: &str) -> Result<(), VaultError> {
        (**self).rename_tag(old_name, new_name)
    }
    fn delete_tag(&mut self, name: &str) -> Result<(), VaultError> {
        (**self).delete_tag(name)
    }
    fn batch_add_tag(&mut self, record_ids: &[Uuid], tag_name: &str) -> Result<usize, VaultError> {
        (**self).batch_add_tag(record_ids, tag_name)
    }
    fn batch_remove_tag(
        &mut self,
        record_ids: &[Uuid],
        tag_name: &str,
    ) -> Result<usize, VaultError> {
        (**self).batch_remove_tag(record_ids, tag_name)
    }
    fn batch_soft_delete(&mut self, record_ids: &[Uuid]) -> Result<usize, VaultError> {
        (**self).batch_soft_delete(record_ids)
    }
    fn empty_trash(&mut self) -> Result<usize, VaultError> {
        (**self).empty_trash()
    }
    fn list_record_health_states(&self) -> Result<Vec<RecordHealthState>, VaultError> {
        (**self).list_record_health_states()
    }
    fn get_record_health_state(
        &self,
        record_id: &Uuid,
    ) -> Result<Option<RecordHealthState>, VaultError> {
        (**self).get_record_health_state(record_id)
    }
    fn upsert_record_health_state(&self, state: &RecordHealthState) -> Result<(), VaultError> {
        (**self).upsert_record_health_state(state)
    }
    fn replace_record_health_states(
        &self,
        new_states: &[RecordHealthState],
    ) -> Result<(), VaultError> {
        (**self).replace_record_health_states(new_states)
    }
    fn delete_record_health_state(&self, record_id: &Uuid) -> Result<(), VaultError> {
        (**self).delete_record_health_state(record_id)
    }
    fn delete_record_health_states(&self, record_ids: &[Uuid]) -> Result<(), VaultError> {
        (**self).delete_record_health_states(record_ids)
    }
    fn copy_health_state_to_version(
        &self,
        record_id: &Uuid,
        new_record_version: u64,
    ) -> Result<(), VaultError> {
        (**self).copy_health_state_to_version(record_id, new_record_version)
    }
    fn mark_records_pending_sync(&self, record_ids: &[Uuid]) -> Result<(), VaultError> {
        (**self).mark_records_pending_sync(record_ids)
    }
    fn get_metadata(&self, key: &str) -> Result<Option<String>, VaultError> {
        (**self).get_metadata(key)
    }
    fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), VaultError> {
        (**self).set_metadata(key, value)
    }
    fn get_last_health_check_at(
        &self,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, VaultError> {
        (**self).get_last_health_check_at()
    }
    fn set_last_health_check_at(
        &mut self,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), VaultError> {
        (**self).set_last_health_check_at(at)
    }
    fn write_audit_entry(
        &self,
        operation: AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<String>,
        detail: Option<String>,
    ) -> Result<(), VaultError> {
        (**self).write_audit_entry(operation, record_id, record_name, detail)
    }
    fn query_audit_log(
        &self,
        filter: &AuditFilter,
    ) -> Result<(Vec<crate::types::audit::AuditEntry>, usize), VaultError> {
        (**self).query_audit_log(filter)
    }
    fn apply_downloaded_cloud_record(
        &mut self,
        record: &crate::cloud::CloudRecord,
    ) -> Result<bool, VaultError> {
        (**self).apply_downloaded_cloud_record(record)
    }
    fn decrypt_record_name_for_sync(&self, record: &StoredRecord) -> Result<String, VaultError> {
        (**self).decrypt_record_name_for_sync(record)
    }
    fn load_sync_status_map(
        &self,
    ) -> std::collections::HashMap<String, crate::types::sync::SyncStatus> {
        (**self).load_sync_status_map()
    }
    fn re_encrypt_record(
        &mut self,
        record_id: Uuid,
        old_dek_version: u32,
    ) -> Result<(), VaultError> {
        (**self).re_encrypt_record(record_id, old_dek_version)
    }
    fn list_records_for_migration(
        &self,
        target_dek_version: u32,
    ) -> Result<Vec<StoredRecord>, VaultError> {
        (**self).list_records_for_migration(target_dek_version)
    }
    fn log_dek_rotated(&self, detail: &str) -> Result<(), VaultError> {
        (**self).log_dek_rotated(detail)
    }
    fn delete_metadata(&mut self, key: &str) -> Result<(), VaultError> {
        (**self).delete_metadata(key)
    }
}

impl Vault for VaultServiceImpl {
    // Delegate all trait methods to inherent implementations
    // These are defined across multiple impl blocks in submodules
    // The inherent impl blocks use VaultServiceImpl, so we call them directly

    fn unlock(&mut self, path: &Path, cmk: &SecureStr) -> Result<(), VaultError> {
        VaultServiceImpl::unlock(self, path, cmk)
    }

    fn unlock_with_mnemonic(&mut self, mnemonic: &Passkey) -> Result<(), VaultError> {
        VaultServiceImpl::unlock_with_mnemonic(self, mnemonic)
    }

    fn lock(&mut self) {
        VaultServiceImpl::lock(self)
    }

    fn is_unlocked(&self) -> bool {
        VaultServiceImpl::is_unlocked(self)
    }

    fn checkpoint_wal(&self) -> Result<(), VaultError> {
        VaultServiceImpl::checkpoint_wal(self)
    }

    fn current_dek_version(&self) -> u32 {
        VaultServiceImpl::current_dek_version(self)
    }

    fn create_record(
        &mut self,
        params: crate::types::record::CreateRecordParams,
    ) -> Result<Uuid, VaultError> {
        VaultServiceImpl::create_record(self, params)
    }

    fn update_record(
        &mut self,
        params: crate::types::record::UpdateRecordParams,
    ) -> Result<(), VaultError> {
        VaultServiceImpl::update_record(self, params)
    }

    fn get_decrypted_record(
        &mut self,
        id: Uuid,
    ) -> Result<crate::types::record::DecryptedRecord, VaultError> {
        VaultServiceImpl::get_decrypted_record(self, id)
    }

    fn decrypt_field(&self, id: Uuid, field: FieldSelector) -> Result<SecureStr, VaultError> {
        VaultServiceImpl::decrypt_field(self, id, field)
    }

    fn soft_delete_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        VaultServiceImpl::soft_delete_record(self, id)
    }

    fn restore_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        VaultServiceImpl::restore_record(self, id)
    }

    fn hard_delete_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        VaultServiceImpl::hard_delete_record(self, id)
    }

    fn toggle_favorite(&mut self, id: Uuid, is_favorite: bool) -> Result<(), VaultError> {
        VaultServiceImpl::toggle_favorite(self, id, is_favorite)
    }

    fn get_stored_record(
        &self,
        id: Uuid,
    ) -> Result<crate::types::record::StoredRecord, VaultError> {
        VaultServiceImpl::get_stored_record(self, id)
    }

    fn list_records(
        &self,
        filter: &RecordFilter,
        sort: &RecordSort,
    ) -> Result<Vec<crate::types::record::TuiRecord>, VaultError> {
        VaultServiceImpl::list_records(self, filter, sort)
    }

    fn list_all_stored_records(
        &self,
    ) -> Result<Vec<crate::types::record::StoredRecord>, VaultError> {
        VaultServiceImpl::list_all_stored_records(self)
    }

    fn get_password_history(&self, record_id: Uuid) -> Result<Vec<PasswordHistory>, VaultError> {
        VaultServiceImpl::get_password_history(self, record_id)
    }

    fn decrypt_history_password(&self, history_id: i64) -> Result<SecureStr, VaultError> {
        VaultServiceImpl::decrypt_history_password(self, history_id)
    }

    fn list_tags_with_stats(&self) -> Result<Vec<(Tag, TagSortMeta)>, VaultError> {
        VaultServiceImpl::list_tags_with_stats(self)
    }

    fn rename_tag(&mut self, old_name: &str, new_name: &str) -> Result<(), VaultError> {
        VaultServiceImpl::rename_tag(self, old_name, new_name)
    }

    fn delete_tag(&mut self, name: &str) -> Result<(), VaultError> {
        VaultServiceImpl::delete_tag(self, name)
    }

    fn batch_add_tag(&mut self, record_ids: &[Uuid], tag_name: &str) -> Result<usize, VaultError> {
        VaultServiceImpl::batch_add_tag(self, record_ids, tag_name)
    }

    fn batch_remove_tag(
        &mut self,
        record_ids: &[Uuid],
        tag_name: &str,
    ) -> Result<usize, VaultError> {
        VaultServiceImpl::batch_remove_tag(self, record_ids, tag_name)
    }

    fn batch_soft_delete(&mut self, record_ids: &[Uuid]) -> Result<usize, VaultError> {
        VaultServiceImpl::batch_soft_delete(self, record_ids)
    }

    fn empty_trash(&mut self) -> Result<usize, VaultError> {
        VaultServiceImpl::empty_trash(self)
    }

    fn list_record_health_states(&self) -> Result<Vec<RecordHealthState>, VaultError> {
        VaultServiceImpl::list_record_health_states(self)
    }

    fn get_record_health_state(
        &self,
        record_id: &Uuid,
    ) -> Result<Option<RecordHealthState>, VaultError> {
        VaultServiceImpl::get_record_health_state(self, record_id)
    }

    fn upsert_record_health_state(&self, state: &RecordHealthState) -> Result<(), VaultError> {
        VaultServiceImpl::upsert_record_health_state(self, state)
    }

    fn replace_record_health_states(
        &self,
        new_states: &[RecordHealthState],
    ) -> Result<(), VaultError> {
        VaultServiceImpl::replace_record_health_states(self, new_states)
    }

    fn delete_record_health_state(&self, record_id: &Uuid) -> Result<(), VaultError> {
        VaultServiceImpl::delete_record_health_state(self, record_id)
    }

    fn delete_record_health_states(&self, record_ids: &[Uuid]) -> Result<(), VaultError> {
        VaultServiceImpl::delete_record_health_states(self, record_ids)
    }

    fn copy_health_state_to_version(
        &self,
        record_id: &Uuid,
        new_record_version: u64,
    ) -> Result<(), VaultError> {
        VaultServiceImpl::copy_health_state_to_version(self, record_id, new_record_version)
    }

    fn mark_records_pending_sync(&self, record_ids: &[Uuid]) -> Result<(), VaultError> {
        VaultServiceImpl::mark_records_pending_sync(self, record_ids)
    }

    fn get_metadata(&self, key: &str) -> Result<Option<String>, VaultError> {
        VaultServiceImpl::get_metadata(self, key)
    }

    fn set_metadata(&mut self, key: &str, value: &str) -> Result<(), VaultError> {
        VaultServiceImpl::set_metadata(self, key, value)
    }

    fn get_last_health_check_at(
        &self,
    ) -> Result<Option<chrono::DateTime<chrono::Utc>>, VaultError> {
        VaultServiceImpl::get_last_health_check_at(self)
    }

    fn set_last_health_check_at(
        &mut self,
        at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), VaultError> {
        VaultServiceImpl::set_last_health_check_at(self, at)
    }

    fn write_audit_entry(
        &self,
        operation: AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<String>,
        detail: Option<String>,
    ) -> Result<(), VaultError> {
        VaultServiceImpl::write_audit_entry(self, operation, record_id, record_name, detail)
    }

    fn query_audit_log(
        &self,
        filter: &AuditFilter,
    ) -> Result<(Vec<crate::types::audit::AuditEntry>, usize), VaultError> {
        VaultServiceImpl::query_audit_log(self, filter)
    }

    fn apply_downloaded_cloud_record(
        &mut self,
        record: &crate::cloud::CloudRecord,
    ) -> Result<bool, VaultError> {
        VaultServiceImpl::apply_downloaded_cloud_record(self, record)
    }

    fn decrypt_record_name_for_sync(
        &self,
        record: &crate::types::record::StoredRecord,
    ) -> Result<String, VaultError> {
        VaultServiceImpl::decrypt_record_name_for_sync(self, record)
    }

    fn load_sync_status_map(
        &self,
    ) -> std::collections::HashMap<String, crate::types::sync::SyncStatus> {
        VaultServiceImpl::load_sync_status_map(self)
    }

    fn re_encrypt_record(
        &mut self,
        record_id: Uuid,
        old_dek_version: u32,
    ) -> Result<(), VaultError> {
        VaultServiceImpl::re_encrypt_record(self, record_id, old_dek_version)
    }

    fn list_records_for_migration(
        &self,
        target_dek_version: u32,
    ) -> Result<Vec<crate::types::record::StoredRecord>, VaultError> {
        VaultServiceImpl::list_records_for_migration(self, target_dek_version)
    }

    fn log_dek_rotated(&self, detail: &str) -> Result<(), VaultError> {
        VaultServiceImpl::log_dek_rotated(self, detail)
    }

    fn delete_metadata(&mut self, key: &str) -> Result<(), VaultError> {
        VaultServiceImpl::delete_metadata(self, key)
    }
}

impl VaultServiceImpl {
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
    pub fn current_dek_version(&self) -> u32 {
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
