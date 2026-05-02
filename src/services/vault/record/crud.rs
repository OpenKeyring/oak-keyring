// Core CRUD operations (create, read, update, delete)

use chrono::Utc;
use uuid::Uuid;

use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultService;
use crate::types::audit::AuditOperation;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::record::{CreateRecordParams, DecryptedRecord, StoredRecord, UpdateRecordParams};

use super::helpers::{
    db_error_to_vault, decrypt_record_name, expires_at_changed, password_changed,
};

impl VaultService {
    /// Create a new vault record with encryption, tags, and audit logging.
    ///
    /// Returns the UUID of the newly created record.
    pub fn create_record(&mut self, params: CreateRecordParams) -> Result<Uuid, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        let id = Uuid::new_v4();
        let aad = format!("record:{}", id);
        let (encrypted_data, nonce) =
            payload::encrypt_payload(&self.crypto, &params.payload, aad.as_bytes())
                .map_err(VaultError::CryptoError)?;

        let now = Utc::now();
        let record = StoredRecord {
            id,
            credential_type: params.credential_type,
            encrypted_data,
            nonce,
            dek_version: self.crypto.current_dek_version(),
            aad: aad.into_bytes(),
            is_favorite: params.is_favorite,
            expires_at: params.expires_at,
            created_at: now,
            updated_at: now,
            updated_by: self.device_id.clone(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: params.tags.clone(),
        };

        // Insert record (transaction includes record + tags)
        queries::insert_record(&self.conn, &record).map_err(db_error_to_vault)?;

        // Audit log entry
        let record_name = params.payload.name().to_string();
        queries::insert_audit_entry(
            &self.conn,
            AuditOperation::RecordCreate,
            Some(&id),
            Some(&record_name),
            None,
        )
        .map_err(db_error_to_vault)?;

        Ok(id)
    }

    /// Retrieve the stored (encrypted) record by ID.
    ///
    /// Returns `VaultError::RecordNotFound` if no record with the given UUID exists.
    pub fn get_stored_record(&self, id: Uuid) -> Result<StoredRecord, VaultError> {
        queries::get_record(&self.conn, &id)
            .map_err(db_error_to_vault)?
            .ok_or(VaultError::RecordNotFound(id))
    }

    /// Retrieve and decrypt a record by ID, writing an audit entry for the access.
    ///
    /// Returns `VaultError::RecordNotFound` if no record with the given UUID exists.
    /// Returns `VaultError::NotUnlocked` if the vault is locked.
    /// Returns `VaultError::CryptoError` if decryption fails or credential type / payload mismatch.
    pub fn get_decrypted_record(&mut self, id: Uuid) -> Result<DecryptedRecord, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        let stored = self.get_stored_record(id)?;

        let payload = payload::decrypt_payload(
            &self.crypto,
            &stored.encrypted_data,
            &stored.nonce,
            &stored.aad,
            stored.credential_type,
            stored.dek_version,
        )
        .map_err(VaultError::CryptoError)?;

        let decrypted = match (stored.credential_type, payload) {
            (
                CredentialType::Login,
                EncryptedPayload::Login {
                    name,
                    username,
                    password,
                    url,
                    notes,
                },
            ) => DecryptedRecord::Login {
                id: stored.id,
                is_favorite: stored.is_favorite,
                expires_at: stored.expires_at,
                created_at: stored.created_at,
                updated_at: stored.updated_at,
                version: stored.version,
                deleted: stored.deleted,
                deleted_at: stored.deleted_at,
                tags: stored.tags,
                name,
                username,
                password,
                url,
                notes,
            },
            (
                CredentialType::Api,
                EncryptedPayload::Api {
                    name,
                    app_id,
                    secret_key,
                    url,
                    notes,
                },
            ) => DecryptedRecord::Api {
                id: stored.id,
                is_favorite: stored.is_favorite,
                expires_at: stored.expires_at,
                created_at: stored.created_at,
                updated_at: stored.updated_at,
                version: stored.version,
                deleted: stored.deleted,
                deleted_at: stored.deleted_at,
                tags: stored.tags,
                name,
                app_id,
                secret_key,
                url,
                notes,
            },
            (
                CredentialType::Ssh,
                EncryptedPayload::Ssh {
                    name,
                    public_key,
                    private_key,
                    passphrase,
                    notes,
                },
            ) => DecryptedRecord::Ssh {
                id: stored.id,
                is_favorite: stored.is_favorite,
                expires_at: stored.expires_at,
                created_at: stored.created_at,
                updated_at: stored.updated_at,
                version: stored.version,
                deleted: stored.deleted,
                deleted_at: stored.deleted_at,
                tags: stored.tags,
                name,
                public_key,
                private_key,
                passphrase,
                notes,
            },
            _ => {
                return Err(VaultError::CryptoError(
                    "credential type / payload mismatch".into(),
                ))
            }
        };

        // Audit log entry
        queries::insert_audit_entry(
            &self.conn,
            AuditOperation::RecordViewPassword,
            Some(&stored.id),
            Some(decrypted.name()),
            None,
        )
        .map_err(db_error_to_vault)?;

        Ok(decrypted)
    }

    /// Update an existing vault record with optimistic locking.
    ///
    /// # Errors
    /// - `VaultError::NotUnlocked` if the vault is locked.
    /// - `VaultError::RecordNotFound` if no record with the given ID exists.
    /// - `VaultError::VersionConflict` if `expected_version` does not match the stored version.
    /// - `VaultError::CryptoError` if encryption or decryption fails.
    pub fn update_record(&mut self, params: UpdateRecordParams) -> Result<(), VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        // 1. Read current record
        let stored = self.get_stored_record(params.id)?;

        // 2. Version check (optimistic locking)
        if stored.version != params.expected_version {
            return Err(VaultError::VersionConflict {
                expected: params.expected_version,
                actual: stored.version,
            });
        }

        // 3. Decrypt old payload to detect changes
        let old_payload = payload::decrypt_payload(
            &self.crypto,
            &stored.encrypted_data,
            &stored.nonce,
            &stored.aad,
            stored.credential_type,
            stored.dek_version,
        )
        .map_err(VaultError::CryptoError)?;

        // 4. Detect password and expires_at changes for health state management
        let pw_changed = password_changed(&old_payload, &params.payload);
        let exp_changed = expires_at_changed(stored.expires_at, params.expires_at);

        // 5. If Login and password changed, save old password to history
        if stored.credential_type == CredentialType::Login && pw_changed {
            self._save_password_history(
                params.id,
                &stored.encrypted_data,
                &stored.nonce,
                stored.dek_version,
            )?;
        }

        // 6. Encrypt new payload
        let aad = format!("record:{}", params.id);
        let (encrypted_data, nonce) =
            payload::encrypt_payload(&self.crypto, &params.payload, aad.as_bytes())
                .map_err(VaultError::CryptoError)?;

        let now = Utc::now();
        let new_version = stored.version + 1;
        let updated_record = StoredRecord {
            id: params.id,
            credential_type: stored.credential_type,
            encrypted_data,
            nonce,
            dek_version: self.crypto.current_dek_version(),
            aad: aad.into_bytes(),
            is_favorite: params.is_favorite,
            expires_at: params.expires_at,
            created_at: stored.created_at,
            updated_at: now,
            updated_by: self.device_id.clone(),
            version: new_version,
            deleted: stored.deleted,
            deleted_at: stored.deleted_at,
            tags: params.tags.clone(),
        };

        // 7. Update record in DB (with optimistic locking via WHERE version = ?)
        let updated = queries::update_record(&self.conn, &updated_record, params.expected_version)
            .map_err(db_error_to_vault)?;
        if !updated {
            return Err(VaultError::VersionConflict {
                expected: params.expected_version,
                actual: stored.version, // Best guess; the actual version may have changed
            });
        }

        // 8. Clear old tag associations and rebuild new ones
        queries::detach_all_tags_for_record(&self.conn, &params.id).map_err(db_error_to_vault)?;
        for tag_name in &params.tags {
            let tag =
                queries::get_or_create_tag(&self.conn, tag_name).map_err(db_error_to_vault)?;
            queries::attach_tag(&self.conn, &params.id, tag.id).map_err(db_error_to_vault)?;
        }

        // 9. Health state management (spec section 7 lifecycle rules)
        if pw_changed || exp_changed {
            // Password or expires_at changed: delete health state, schedule rescan
            self.delete_record_health_state(&params.id)?;
        } else {
            // Cosmetic change only: carry forward health state to new version
            self.copy_health_state_to_version(&params.id, new_version)?;
        }

        // 10. Write audit entry
        let record_name = params.payload.name().to_string();
        queries::insert_audit_entry(
            &self.conn,
            AuditOperation::RecordUpdate,
            Some(&params.id),
            Some(&record_name),
            None,
        )
        .map_err(db_error_to_vault)?;

        Ok(())
    }

    // =========================================================================
    // Soft delete, restore, hard delete, toggle favorite
    // =========================================================================

    /// Soft-delete a record by ID.
    ///
    /// Sets `deleted = 1` and `deleted_at = now` so the record disappears from
    /// normal listing but can be restored later. Writes an audit entry with
    /// operation `RecordDelete`.
    ///
    /// # Errors
    /// - `VaultError::NotUnlocked` if the vault is locked.
    /// - `VaultError::RecordNotFound` if no record with the given UUID exists.
    pub fn soft_delete_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        // Fetch record to verify existence and capture name for audit.
        let stored = self.get_stored_record(id)?;
        let record_name = decrypt_record_name(&self.crypto, &stored)?;

        queries::soft_delete_record(&self.conn, &id).map_err(db_error_to_vault)?;

        // Delete health state: soft-deleted records should not carry stale health data.
        // A full health scan will be scheduled by the executor after this operation.
        self.delete_record_health_state(&id)?;

        queries::insert_audit_entry(
            &self.conn,
            AuditOperation::RecordDelete,
            Some(&id),
            Some(&record_name),
            None,
        )
        .map_err(db_error_to_vault)?;

        Ok(())
    }

    /// Restore a previously soft-deleted record.
    ///
    /// Sets `deleted = 0` and `deleted_at = NULL`. Writes an audit entry with
    /// operation `RecordRestore`.
    ///
    /// # Errors
    /// - `VaultError::NotUnlocked` if the vault is locked.
    /// - `VaultError::RecordNotFound` if no record with the given UUID exists.
    pub fn restore_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        // Fetch record to verify existence and capture name for audit.
        let stored = self.get_stored_record(id)?;
        let record_name = decrypt_record_name(&self.crypto, &stored)?;

        queries::restore_record(&self.conn, &id).map_err(db_error_to_vault)?;

        // No explicit health state change on restore — the executor will
        // schedule a full health scan so the restored record gets evaluated.

        queries::insert_audit_entry(
            &self.conn,
            AuditOperation::RecordRestore,
            Some(&id),
            Some(&record_name),
            None,
        )
        .map_err(db_error_to_vault)?;

        Ok(())
    }

    /// Permanently delete a record and all associated data.
    ///
    /// Cascade-deletes `record_tags` and `password_history` via FK constraints.
    /// Writes an audit entry with operation `RecordDestroy`.
    ///
    /// # Errors
    /// - `VaultError::NotUnlocked` if the vault is locked.
    /// - `VaultError::RecordNotFound` if no record with the given UUID exists.
    pub fn hard_delete_record(&mut self, id: Uuid) -> Result<(), VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        // Fetch record to verify existence and capture name for audit before deletion.
        let stored = self.get_stored_record(id)?;
        let record_name = decrypt_record_name(&self.crypto, &stored)?;

        queries::hard_delete_record(&self.conn, &id).map_err(db_error_to_vault)?;

        // Health state is cascade-deleted via FK, but delete explicitly in case
        // the FK constraint is deferred or absent.
        self.delete_record_health_state(&id)?;

        queries::insert_audit_entry(
            &self.conn,
            AuditOperation::RecordDestroy,
            Some(&id),
            Some(&record_name),
            None,
        )
        .map_err(db_error_to_vault)?;

        Ok(())
    }

    /// Toggle the favorite status of a record.
    ///
    /// Updates only the `is_favorite` field — no version increment, no audit entry.
    ///
    /// # Errors
    /// - `VaultError::NotUnlocked` if the vault is locked.
    /// - `VaultError::RecordNotFound` if no record with the given UUID exists.
    pub fn toggle_favorite(&mut self, id: Uuid, is_favorite: bool) -> Result<(), VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        // Verify the record exists.
        self.get_stored_record(id)?;

        self.conn
            .execute(
                "UPDATE records SET is_favorite = ?1 WHERE id = ?2",
                rusqlite::params![is_favorite as i64, id.to_string()],
            )
            .map_err(VaultError::DatabaseError)?;

        Ok(())
    }
}
