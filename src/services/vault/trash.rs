// Trash bin operations (list_trash, empty_trash, cleanup_expired_trash, batch_soft_delete)

#[cfg(test)]
use crate::services::vault::VaultService;
use crate::services::vault::VaultServiceImpl;
use chrono::Utc;
use uuid::Uuid;

use super::record::db_error_to_vault;
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::audit::AuditOperation;
use crate::types::record::TuiRecord;

impl VaultServiceImpl {
    /// List all soft-deleted records, decrypting name and subtitle for TUI display.
    ///
    /// Returns `VaultError::NotUnlocked` if the vault is locked.
    pub fn list_trash(&self) -> Result<Vec<TuiRecord>, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        let stored_records =
            queries::list_deleted_records(&self.conn).map_err(db_error_to_vault)?;

        let mut tui_records: Vec<TuiRecord> = Vec::with_capacity(stored_records.len());
        for stored in &stored_records {
            let aad = format!("record:{}", stored.id);
            let name = payload::decrypt_name_only(
                &self.crypto,
                &stored.encrypted_data,
                &stored.nonce,
                aad.as_bytes(),
                stored.dek_version,
            )
            .map_err(VaultError::CryptoError)?;

            let subtitle = payload::decrypt_subtitle(
                &self.crypto,
                &stored.encrypted_data,
                &stored.nonce,
                aad.as_bytes(),
                stored.credential_type,
                stored.dek_version,
            )
            .map_err(VaultError::CryptoError)?;

            tui_records.push(TuiRecord {
                id: stored.id,
                credential_type: stored.credential_type,
                name,
                subtitle,
                is_favorite: stored.is_favorite,
                is_expired: false, // populated by executor from health_report
                expires_at: stored.expires_at,
                has_weak_password: false,
                is_compromised: false,
                duplicate_group_size: None,
                created_at: stored.created_at,
                updated_at: stored.updated_at,
                deleted: stored.deleted,
                deleted_at: stored.deleted_at,
                tags: stored.tags.clone(),
                sync_status: None,
            });
        }

        Ok(tui_records)
    }

    /// Permanently delete all soft-deleted records and write a `TrashEmpty` audit entry.
    ///
    /// Returns the count of permanently deleted records.
    ///
    /// Returns `VaultError::NotUnlocked` if the vault is locked.
    pub fn empty_trash(&mut self) -> Result<usize, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        let deleted_records =
            queries::list_deleted_records(&self.conn).map_err(db_error_to_vault)?;
        let count = deleted_records.len();

        for record in &deleted_records {
            queries::hard_delete_record(&self.conn, &record.id).map_err(db_error_to_vault)?;
        }

        if count > 0 {
            let detail = format!("permanently deleted {} record(s)", count);
            queries::insert_audit_entry(
                &self.conn,
                AuditOperation::TrashEmpty,
                None,
                None,
                Some(&detail),
            )
            .map_err(db_error_to_vault)?;
        }

        Ok(count)
    }

    /// Permanently delete soft-deleted records whose `deleted_at` is older than
    /// `retention_days` days ago.
    ///
    /// Returns the count of expired records that were removed.
    ///
    /// Returns `VaultError::NotUnlocked` if the vault is locked.
    pub fn cleanup_expired_trash(&mut self, retention_days: u32) -> Result<usize, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        let deleted_records =
            queries::list_deleted_records(&self.conn).map_err(db_error_to_vault)?;
        let cutoff = Utc::now() - chrono::Duration::days(retention_days as i64);

        let mut count = 0;
        for record in &deleted_records {
            let is_expired = record
                .deleted_at
                .is_some_and(|deleted_at| deleted_at < cutoff);
            if is_expired {
                queries::hard_delete_record(&self.conn, &record.id).map_err(db_error_to_vault)?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Soft-delete multiple records in batch, writing a `RecordDelete` audit entry for each.
    ///
    /// Returns the number of records that were soft-deleted.
    ///
    /// Returns `VaultError::NotUnlocked` if the vault is locked.
    pub fn batch_soft_delete(&mut self, record_ids: &[Uuid]) -> Result<usize, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        if record_ids.is_empty() {
            return Ok(0);
        }

        let affected = queries::batch_soft_delete_records(&self.conn, record_ids, &self.device_id)
            .map_err(db_error_to_vault)?;

        // Write per-record audit entries
        for id in record_ids {
            // Attempt to decrypt the record name for the audit log.
            // If the record was not found or decryption fails, log with a placeholder.
            let record_name = self
                .get_stored_record(*id)
                .ok()
                .and_then(|stored| {
                    let aad = format!("record:{}", stored.id);
                    payload::decrypt_name_only(
                        &self.crypto,
                        &stored.encrypted_data,
                        &stored.nonce,
                        aad.as_bytes(),
                        stored.dek_version,
                    )
                    .ok()
                })
                .unwrap_or_else(|| "<unknown>".to_string());

            queries::insert_audit_entry(
                &self.conn,
                AuditOperation::RecordDelete,
                Some(id),
                Some(&record_name),
                None,
            )
            .map_err(db_error_to_vault)?;
        }

        Ok(affected)
    }

    /// Batch restore multiple soft-deleted records.
    ///
    /// Pre-filters to only soft-deleted records. Captures names before the mutation
    /// so audit entries are accurate.
    ///
    /// Returns the number of records restored.
    ///
    /// Returns `VaultError::NotUnlocked` if the vault is locked.
    pub fn batch_restore(&mut self, record_ids: &[Uuid]) -> Result<usize, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        if record_ids.is_empty() {
            return Ok(0);
        }

        // Pre-filter: capture names only for soft-deleted records
        let deleted_with_names: Vec<(Uuid, String)> = record_ids
            .iter()
            .filter_map(|id| {
                let stored = self.get_stored_record(*id).ok()?;
                if !stored.deleted {
                    return None;
                }
                let aad = format!("record:{}", stored.id);
                let name = payload::decrypt_name_only(
                    &self.crypto,
                    &stored.encrypted_data,
                    &stored.nonce,
                    aad.as_bytes(),
                    stored.dek_version,
                )
                .ok()
                .unwrap_or_else(|| "<unknown>".to_string());
                Some((*id, name))
            })
            .collect();

        let affected =
            queries::batch_restore_records(&self.conn, record_ids).map_err(db_error_to_vault)?;

        for (id, name) in &deleted_with_names {
            queries::insert_audit_entry(
                &self.conn,
                AuditOperation::RecordRestore,
                Some(id),
                Some(name),
                None,
            )
            .map_err(db_error_to_vault)?;
        }

        Ok(affected)
    }

    /// Batch hard-delete multiple soft-deleted records (permanently).
    ///
    /// Pre-filters to only soft-deleted records. Captures names before deletion
    /// so audit entries are accurate.
    ///
    /// Returns the number of records destroyed.
    ///
    /// Returns `VaultError::NotUnlocked` if the vault is locked.
    pub fn batch_hard_delete(&mut self, record_ids: &[Uuid]) -> Result<usize, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        if record_ids.is_empty() {
            return Ok(0);
        }

        // Pre-filter: capture names before deletion, only for soft-deleted records
        let deleted_with_names: Vec<(Uuid, String)> = record_ids
            .iter()
            .filter_map(|id| {
                let stored = self.get_stored_record(*id).ok()?;
                if !stored.deleted {
                    return None;
                }
                let aad = format!("record:{}", stored.id);
                let name = payload::decrypt_name_only(
                    &self.crypto,
                    &stored.encrypted_data,
                    &stored.nonce,
                    aad.as_bytes(),
                    stored.dek_version,
                )
                .ok()
                .unwrap_or_else(|| "<unknown>".to_string());
                Some((*id, name))
            })
            .collect();

        let affected = queries::batch_hard_delete_records(&self.conn, record_ids)
            .map_err(db_error_to_vault)?;

        // Audit with captured names
        for (id, name) in &deleted_with_names {
            queries::insert_audit_entry(
                &self.conn,
                AuditOperation::RecordDestroy,
                Some(id),
                Some(name),
                None,
            )
            .map_err(db_error_to_vault)?;
        }

        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::queries as q;
    use crate::db::schema::init_db_in_memory;
    use crate::types::credential::{CredentialType, EncryptedPayload};
    use crate::types::record::CreateRecordParams;
    use crate::types::sensitive::SecureStr;

    /// Helper: create an in-memory VaultService with schema initialized and unlocked.
    fn setup_unlocked_vault() -> VaultService {
        let conn = init_db_in_memory();
        let mut svc = VaultService::new(conn);
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        svc.crypto
            .unlock_with_mnemonic(&mnemonic)
            .expect("unlock_with_mnemonic must succeed");
        svc
    }

    /// Helper: create a Login record and return its ID.
    fn create_login(svc: &mut VaultService, name: &str) -> Uuid {
        svc.create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: "user".to_string(),
                password: SecureStr::new("pass".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed")
    }

    // =========================================================================
    // list_trash tests
    // =========================================================================

    #[test]
    fn list_trash_returns_not_unlocked_when_locked() {
        let conn = init_db_in_memory();
        let svc = VaultService::new(conn);
        assert!(!svc.is_unlocked());

        let result = svc.list_trash();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked"
        );
    }

    #[test]
    fn list_trash_contains_soft_deleted_record() {
        let mut svc = setup_unlocked_vault();
        let id = create_login(&mut svc, "TrashItem");

        // Not in trash before delete
        let before = svc.list_trash().expect("list_trash must succeed");
        assert!(
            before.is_empty(),
            "trash should be empty before soft delete"
        );

        svc.soft_delete_record(id)
            .expect("soft_delete_record must succeed");

        let trash = svc.list_trash().expect("list_trash must succeed");
        assert_eq!(trash.len(), 1, "trash should contain one record");
        assert_eq!(trash[0].id, id);
        assert!(trash[0].deleted);
        assert_eq!(trash[0].name, "TrashItem", "name should be decrypted");
        assert_eq!(trash[0].subtitle, "user", "subtitle should be decrypted");
    }

    #[test]
    fn list_trash_empty_when_no_deleted_records() {
        let mut svc = setup_unlocked_vault();
        let _id = create_login(&mut svc, "Active");

        let trash = svc.list_trash().expect("list_trash must succeed");
        assert!(
            trash.is_empty(),
            "trash should be empty with no deleted records"
        );
    }

    // =========================================================================
    // empty_trash tests
    // =========================================================================

    #[test]
    fn empty_trash_returns_not_unlocked_when_locked() {
        let conn = init_db_in_memory();
        let mut svc = VaultService::new(conn);

        let result = svc.empty_trash();
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked"
        );
    }

    #[test]
    fn empty_trash_deletes_all_deleted_records_and_returns_count() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "Del1");
        let id2 = create_login(&mut svc, "Del2");
        let _id_active = create_login(&mut svc, "Active");

        svc.soft_delete_record(id1)
            .expect("soft_delete must succeed");
        svc.soft_delete_record(id2)
            .expect("soft_delete must succeed");

        // 2 in trash, 1 active
        let trash_before = svc.list_trash().expect("list_trash must succeed");
        assert_eq!(trash_before.len(), 2);

        let count = svc.empty_trash().expect("empty_trash must succeed");
        assert_eq!(count, 2, "empty_trash should return count of 2");

        // Trash is now empty
        let trash_after = svc.list_trash().expect("list_trash must succeed");
        assert!(
            trash_after.is_empty(),
            "trash should be empty after empty_trash"
        );

        // Deleted records are gone from DB entirely
        assert!(
            q::get_record(&svc.conn, &id1).unwrap().is_none(),
            "id1 should be hard-deleted"
        );
        assert!(
            q::get_record(&svc.conn, &id2).unwrap().is_none(),
            "id2 should be hard-deleted"
        );

        // Active record still exists
        assert!(
            q::get_record(&svc.conn, &_id_active).unwrap().is_some(),
            "active record must survive empty_trash"
        );
    }

    #[test]
    fn empty_trash_writes_trash_empty_audit_with_count() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "Del1");
        let id2 = create_login(&mut svc, "Del2");

        svc.soft_delete_record(id1)
            .expect("soft_delete must succeed");
        svc.soft_delete_record(id2)
            .expect("soft_delete must succeed");

        // 4 audit entries: 2x RecordCreate + 2x RecordDelete
        let before = q::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(before.len(), 4);

        svc.empty_trash().expect("empty_trash must succeed");

        let after = q::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(after.len(), 5, "one TrashEmpty audit entry added");

        let trash_empty = after
            .iter()
            .find(|e| e.operation == AuditOperation::TrashEmpty)
            .expect("expected TrashEmpty audit entry");
        assert!(trash_empty.detail.is_some());
        assert!(
            trash_empty.detail.as_deref().unwrap().contains("2"),
            "detail should contain the count 2"
        );
    }

    #[test]
    fn empty_trash_returns_zero_when_no_deleted_records() {
        let mut svc = setup_unlocked_vault();
        let _id = create_login(&mut svc, "Active");

        let count = svc.empty_trash().expect("empty_trash must succeed");
        assert_eq!(
            count, 0,
            "empty_trash should return 0 when no deleted records"
        );

        // No TrashEmpty audit entry written when count is 0
        let audits = q::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert!(
            audits
                .iter()
                .all(|e| e.operation != AuditOperation::TrashEmpty),
            "no TrashEmpty audit entry when no records to delete"
        );
    }

    // =========================================================================
    // cleanup_expired_trash tests
    // =========================================================================

    #[test]
    fn cleanup_expired_trash_returns_not_unlocked_when_locked() {
        let conn = init_db_in_memory();
        let mut svc = VaultService::new(conn);

        let result = svc.cleanup_expired_trash(30);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked"
        );
    }

    #[test]
    fn cleanup_expired_trash_only_deletes_records_older_than_retention() {
        let mut svc = setup_unlocked_vault();
        let id_old = create_login(&mut svc, "Old");
        let id_recent = create_login(&mut svc, "Recent");

        svc.soft_delete_record(id_old)
            .expect("soft_delete must succeed");
        svc.soft_delete_record(id_recent)
            .expect("soft_delete must succeed");

        // Manually set deleted_at for id_old to 31 days ago
        let old_ts = Utc::now() - chrono::Duration::days(31);
        let old_ts_unix = old_ts.timestamp();
        svc.conn
            .execute(
                "UPDATE records SET deleted_at = ?1 WHERE id = ?2",
                rusqlite::params![old_ts_unix, id_old.to_string()],
            )
            .unwrap();

        // Keep id_recent's deleted_at as-is (just now, well within 30 days)

        let count = svc
            .cleanup_expired_trash(30)
            .expect("cleanup_expired_trash must succeed");
        assert_eq!(count, 1, "only the old record should be cleaned up");

        // Old record is gone
        assert!(
            q::get_record(&svc.conn, &id_old).unwrap().is_none(),
            "old expired record should be hard-deleted"
        );

        // Recent record still exists (still soft-deleted)
        let recent = q::get_record(&svc.conn, &id_recent)
            .unwrap()
            .expect("recent record must still exist");
        assert!(recent.deleted, "recent record should still be soft-deleted");
    }

    #[test]
    fn cleanup_expired_trash_returns_zero_when_none_expired() {
        let mut svc = setup_unlocked_vault();
        let id = create_login(&mut svc, "Recent");

        svc.soft_delete_record(id)
            .expect("soft_delete must succeed");

        // All records were just deleted — none should be older than 30 days
        let count = svc
            .cleanup_expired_trash(30)
            .expect("cleanup_expired_trash must succeed");
        assert_eq!(count, 0, "no records should be expired");
    }

    // =========================================================================
    // batch_soft_delete tests
    // =========================================================================

    #[test]
    fn batch_soft_delete_returns_not_unlocked_when_locked() {
        let conn = init_db_in_memory();
        let mut svc = VaultService::new(conn);

        let result = svc.batch_soft_delete(&[Uuid::new_v4()]);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked"
        );
    }

    #[test]
    fn batch_soft_delete_marks_both_records_as_deleted() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "Batch1");
        let id2 = create_login(&mut svc, "Batch2");

        // One audit entry per create
        let before = q::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(before.len(), 2);

        let count = svc
            .batch_soft_delete(&[id1, id2])
            .expect("batch_soft_delete must succeed");
        assert_eq!(count, 2, "both records should be soft-deleted");

        // Both records are now soft-deleted
        let stored1 = svc.get_stored_record(id1).unwrap();
        assert!(stored1.deleted);
        assert!(stored1.deleted_at.is_some());

        let stored2 = svc.get_stored_record(id2).unwrap();
        assert!(stored2.deleted);
        assert!(stored2.deleted_at.is_some());

        // Both appear in trash
        let trash = svc.list_trash().expect("list_trash must succeed");
        assert_eq!(trash.len(), 2);
    }

    #[test]
    fn batch_soft_delete_writes_per_record_audit() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "BatchAudit1");
        let id2 = create_login(&mut svc, "BatchAudit2");

        // 2 audit entries from create
        let before = q::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(before.len(), 2);

        svc.batch_soft_delete(&[id1, id2])
            .expect("batch_soft_delete must succeed");

        // 2 creates + 2 deletes = 4
        let after = q::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(after.len(), 4, "two per-record audit entries added");

        let delete_entries: Vec<_> = after
            .iter()
            .filter(|e| e.operation == AuditOperation::RecordDelete)
            .collect();
        assert_eq!(delete_entries.len(), 2, "two RecordDelete audit entries");

        let ids_in_audit: Vec<Uuid> = delete_entries.iter().filter_map(|e| e.record_id).collect();
        assert!(ids_in_audit.contains(&id1), "audit should contain id1");
        assert!(ids_in_audit.contains(&id2), "audit should contain id2");
    }

    #[test]
    fn batch_soft_delete_with_empty_ids_returns_zero() {
        let mut svc = setup_unlocked_vault();
        let _id = create_login(&mut svc, "Noop");

        let before = q::list_audit_entries(&svc.conn, 10, 0).unwrap();

        let count = svc
            .batch_soft_delete(&[])
            .expect("batch_soft_delete with empty must succeed");
        assert_eq!(count, 0, "empty input returns 0");

        let after = q::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(
            after.len(),
            before.len(),
            "no audit entries written for empty batch"
        );
    }

    // =========================================================================
    // batch_restore tests
    // =========================================================================

    #[test]
    fn batch_restore_returns_not_unlocked_when_locked() {
        let conn = init_db_in_memory();
        let mut svc = VaultService::new(conn);

        let result = svc.batch_restore(&[Uuid::new_v4()]);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked"
        );
    }

    #[test]
    fn batch_restore_restores_soft_deleted_records() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "Del1");
        let id2 = create_login(&mut svc, "Del2");

        svc.soft_delete_record(id1)
            .expect("soft_delete must succeed");
        svc.soft_delete_record(id2)
            .expect("soft_delete must succeed");

        let count = svc
            .batch_restore(&[id1, id2])
            .expect("batch_restore must succeed");
        assert_eq!(count, 2, "both records should be restored");

        let stored1 = svc.get_stored_record(id1).unwrap();
        assert!(!stored1.deleted, "id1 should be active");

        let stored2 = svc.get_stored_record(id2).unwrap();
        assert!(!stored2.deleted, "id2 should be active");
    }

    #[test]
    fn batch_restore_only_affects_deleted_records() {
        let mut svc = setup_unlocked_vault();
        let id_active = create_login(&mut svc, "Active");
        let id_deleted = create_login(&mut svc, "Deleted");

        svc.soft_delete_record(id_deleted)
            .expect("soft_delete must succeed");

        // Request both — only the deleted one should be restored
        let count = svc
            .batch_restore(&[id_active, id_deleted])
            .expect("batch_restore must succeed");
        assert_eq!(count, 1, "only deleted record restored");
    }

    #[test]
    fn batch_restore_writes_audit_with_names() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "AuditRestore1");
        let id2 = create_login(&mut svc, "AuditRestore2");

        svc.soft_delete_record(id1)
            .expect("soft_delete must succeed");
        svc.soft_delete_record(id2)
            .expect("soft_delete must succeed");

        svc.batch_restore(&[id1, id2])
            .expect("batch_restore must succeed");

        let entries = q::list_audit_entries(&svc.conn, 20, 0).unwrap();
        let restore_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == AuditOperation::RecordRestore)
            .collect();
        assert_eq!(restore_entries.len(), 2, "two RecordRestore audit entries");

        // Verify names were captured
        for entry in &restore_entries {
            assert!(
                entry.record_name.is_some(),
                "audit entry should have record name"
            );
        }
    }

    #[test]
    fn batch_restore_skips_audit_for_non_deleted() {
        let mut svc = setup_unlocked_vault();
        let id_active = create_login(&mut svc, "Active");
        let _id_deleted = create_login(&mut svc, "Deleted");

        // Request active record — no audit should be written
        svc.batch_restore(&[id_active])
            .expect("batch_restore must succeed");

        let entries = q::list_audit_entries(&svc.conn, 20, 0).unwrap();
        let restore_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == AuditOperation::RecordRestore)
            .collect();
        assert_eq!(restore_entries.len(), 0, "no audit for non-deleted record");
    }

    #[test]
    fn batch_restore_with_empty_ids_returns_zero() {
        let mut svc = setup_unlocked_vault();
        let count = svc
            .batch_restore(&[])
            .expect("batch_restore with empty must succeed");
        assert_eq!(count, 0);
    }

    // =========================================================================
    // batch_hard_delete tests
    // =========================================================================

    #[test]
    fn batch_hard_delete_returns_not_unlocked_when_locked() {
        let conn = init_db_in_memory();
        let mut svc = VaultService::new(conn);

        let result = svc.batch_hard_delete(&[Uuid::new_v4()]);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked"
        );
    }

    #[test]
    fn batch_hard_delete_destroys_soft_deleted_records() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "Destroy1");
        let id2 = create_login(&mut svc, "Destroy2");

        svc.soft_delete_record(id1)
            .expect("soft_delete must succeed");
        svc.soft_delete_record(id2)
            .expect("soft_delete must succeed");

        let count = svc
            .batch_hard_delete(&[id1, id2])
            .expect("batch_hard_delete must succeed");
        assert_eq!(count, 2, "both records should be destroyed");

        // Records are gone entirely
        assert!(
            q::get_record(&svc.conn, &id1).unwrap().is_none(),
            "id1 should be gone"
        );
        assert!(
            q::get_record(&svc.conn, &id2).unwrap().is_none(),
            "id2 should be gone"
        );
    }

    #[test]
    fn batch_hard_delete_only_affects_deleted_records() {
        let mut svc = setup_unlocked_vault();
        let id_active = create_login(&mut svc, "Active");
        let id_deleted = create_login(&mut svc, "Deleted");

        svc.soft_delete_record(id_deleted)
            .expect("soft_delete must succeed");

        let count = svc
            .batch_hard_delete(&[id_active, id_deleted])
            .expect("batch_hard_delete must succeed");
        assert_eq!(count, 1, "only deleted record destroyed");

        // Active record is untouched
        let active = svc.get_stored_record(id_active).unwrap();
        assert!(!active.deleted, "active record should remain active");
    }

    #[test]
    fn batch_hard_delete_writes_audit_with_names() {
        let mut svc = setup_unlocked_vault();
        let id1 = create_login(&mut svc, "AuditDestroy1");
        let id2 = create_login(&mut svc, "AuditDestroy2");

        svc.soft_delete_record(id1)
            .expect("soft_delete must succeed");
        svc.soft_delete_record(id2)
            .expect("soft_delete must succeed");

        svc.batch_hard_delete(&[id1, id2])
            .expect("batch_hard_delete must succeed");

        let entries = q::list_audit_entries(&svc.conn, 20, 0).unwrap();
        let destroy_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == AuditOperation::RecordDestroy)
            .collect();
        assert_eq!(destroy_entries.len(), 2, "two RecordDestroy audit entries");

        // Verify names were captured before deletion
        let names: Vec<&str> = destroy_entries
            .iter()
            .filter_map(|e| e.record_name.as_deref())
            .collect();
        assert!(
            names.contains(&"AuditDestroy1"),
            "audit should contain name AuditDestroy1"
        );
        assert!(
            names.contains(&"AuditDestroy2"),
            "audit should contain name AuditDestroy2"
        );
    }

    #[test]
    fn batch_hard_delete_skips_audit_for_non_deleted() {
        let mut svc = setup_unlocked_vault();
        let id_active = create_login(&mut svc, "Active");

        // Request active record — no audit should be written
        svc.batch_hard_delete(&[id_active])
            .expect("batch_hard_delete must succeed");

        let entries = q::list_audit_entries(&svc.conn, 20, 0).unwrap();
        let destroy_entries: Vec<_> = entries
            .iter()
            .filter(|e| e.operation == AuditOperation::RecordDestroy)
            .collect();
        assert_eq!(destroy_entries.len(), 0, "no audit for non-deleted record");
    }

    #[test]
    fn batch_hard_delete_with_empty_ids_returns_zero() {
        let mut svc = setup_unlocked_vault();
        let count = svc
            .batch_hard_delete(&[])
            .expect("batch_hard_delete with empty must succeed");
        assert_eq!(count, 0);
    }
}
