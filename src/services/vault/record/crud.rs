// Core CRUD operations (create, read, update, delete)

use crate::services::vault::VaultServiceImpl;
use chrono::Utc;
use uuid::Uuid;

use crate::cloud::{
    AadFields, CloudPrivateMetadata, CloudRecord, EncryptedRecordMetadata, RecordHealthMetadata,
    RecordMetadata,
};
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::audit::AuditOperation;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::health::RecordHealthState;
use crate::types::record::{CreateRecordParams, DecryptedRecord, StoredRecord, UpdateRecordParams};

use super::helpers::{
    db_error_to_vault, decrypt_record_name, expires_at_changed, password_changed,
};

impl VaultServiceImpl {
    /// Build a sync upload record with private metadata encrypted for cloud.
    pub fn build_cloud_record_for_sync(
        &self,
        record: &StoredRecord,
        health: Option<RecordHealthState>,
    ) -> Result<CloudRecord, VaultError> {
        use base64::Engine;

        let name = self.decrypt_record_name_for_sync(record)?;
        let encrypted_data_base64 =
            base64::engine::general_purpose::STANDARD.encode(&record.encrypted_data);
        let nonce_base64 = base64::engine::general_purpose::STANDARD.encode(record.nonce);
        let private_metadata = CloudPrivateMetadata {
            name,
            tags: record.tags.clone(),
            credential_type: Some(record.credential_type),
            is_favorite: Some(record.is_favorite),
            expires_at: record.expires_at.map(|dt| dt.to_rfc3339()),
            updated_by: Some(record.updated_by.clone()),
            health: health.as_ref().map(RecordHealthMetadata::from_state),
        };
        let private_json = serde_json::to_vec(&private_metadata)
            .map_err(|e| VaultError::CryptoError(e.to_string()))?;
        let metadata_aad = format!("cloud-metadata:{}", record.id);
        let (private_encrypted, private_nonce) = self
            .crypto
            .encrypt(&private_json, metadata_aad.as_bytes())
            .map_err(VaultError::CryptoError)?;

        Ok(CloudRecord {
            id: record.id.to_string(),
            version: record.version,
            encrypted_data: encrypted_data_base64,
            nonce: nonce_base64,
            dek_version: record.dek_version,
            aad: AadFields {
                record_id: record.id.to_string(),
                dek_version: record.dek_version,
            },
            metadata: RecordMetadata {
                name: "encrypted".to_string(),
                tags: Vec::new(),
                updated_at: record.updated_at.to_rfc3339(),
                encrypted_metadata: Some(EncryptedRecordMetadata {
                    encrypted_data: base64::engine::general_purpose::STANDARD
                        .encode(private_encrypted),
                    nonce: base64::engine::general_purpose::STANDARD.encode(private_nonce),
                    dek_version: self.crypto.current_dek_version(),
                }),
                ..Default::default()
            },
            deleted: if record.deleted { Some(true) } else { None },
            deleted_at: record.deleted_at.map(|dt| dt.to_rfc3339()),
        })
    }

    fn decrypt_cloud_private_metadata(
        &self,
        cloud_record: &CloudRecord,
    ) -> Result<Option<CloudPrivateMetadata>, VaultError> {
        use base64::Engine;

        let Some(encrypted_metadata) = cloud_record.metadata.encrypted_metadata.as_ref() else {
            return Ok(None);
        };

        let encrypted_data = base64::engine::general_purpose::STANDARD
            .decode(&encrypted_metadata.encrypted_data)
            .map_err(|e| {
                VaultError::CryptoError(format!("invalid encrypted_metadata base64: {}", e))
            })?;
        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(&encrypted_metadata.nonce)
            .map_err(|e| {
                VaultError::CryptoError(format!("invalid encrypted_metadata nonce base64: {}", e))
            })?;
        let nonce: [u8; 24] = nonce_bytes.try_into().map_err(|_| {
            VaultError::CryptoError("encrypted_metadata nonce must be 24 bytes".to_string())
        })?;
        let metadata_aad = format!("cloud-metadata:{}", cloud_record.id);
        let plaintext = self
            .crypto
            .decrypt(
                &encrypted_data,
                &nonce,
                metadata_aad.as_bytes(),
                encrypted_metadata.dek_version,
            )
            .map_err(VaultError::CryptoError)?;
        serde_json::from_slice(&plaintext)
            .map(Some)
            .map_err(|e| VaultError::CryptoError(format!("invalid encrypted_metadata json: {}", e)))
    }

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

    /// Apply a downloaded `CloudRecord` to the local vault.
    ///
    /// Decodes the base64-encoded encrypted data and nonce, constructs a
    /// `StoredRecord`, and upserts it (INSERT OR REPLACE). This is used by
    /// the sync download path to persist record bodies before health states,
    /// so FK constraints on `record_health_state.record_id` are satisfied.
    ///
    /// Returns `true` if a new record was inserted, `false` if an existing one
    /// was replaced.
    pub fn apply_downloaded_cloud_record(
        &self,
        cloud_record: &crate::cloud::CloudRecord,
    ) -> Result<bool, VaultError> {
        use base64::Engine;

        let id = Uuid::parse_str(&cloud_record.id)
            .map_err(|e| VaultError::CryptoError(format!("invalid record id: {}", e)))?;

        let encrypted_data = base64::engine::general_purpose::STANDARD
            .decode(&cloud_record.encrypted_data)
            .map_err(|e| {
                VaultError::CryptoError(format!("invalid encrypted_data base64: {}", e))
            })?;

        let nonce_bytes = base64::engine::general_purpose::STANDARD
            .decode(&cloud_record.nonce)
            .map_err(|e| VaultError::CryptoError(format!("invalid nonce base64: {}", e)))?;
        let nonce: [u8; 24] = nonce_bytes
            .try_into()
            .map_err(|_| VaultError::CryptoError("nonce must be 24 bytes".to_string()))?;

        let existing =
            crate::db::queries::get_record(&self.conn, &id).map_err(db_error_to_vault)?;
        let is_new = existing.is_none();

        let aad: Vec<u8> = format!("record:{}", id).into_bytes();

        let now = chrono::Utc::now();

        // Restore the remote updated_at to preserve sorting/display semantics.
        // Falls back to now for malformed or legacy timestamps.
        let remote_updated_at =
            chrono::DateTime::parse_from_rfc3339(&cloud_record.metadata.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        record_id = %id,
                        error = %e,
                        raw = %cloud_record.metadata.updated_at,
                        "Failed to parse metadata.updated_at, using current time"
                    );
                    now
                });

        let deleted = cloud_record.deleted.unwrap_or(false);
        let deleted_at = if deleted {
            cloud_record
                .deleted_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
        } else {
            None
        };

        let private_metadata = self.decrypt_cloud_private_metadata(cloud_record)?;

        // Preserve record attributes from encrypted private metadata when
        // present; fall back to plaintext legacy metadata for older clients.
        let credential_type = private_metadata
            .as_ref()
            .and_then(|m| m.credential_type)
            .or(cloud_record.metadata.credential_type)
            .unwrap_or(crate::types::credential::CredentialType::Login);
        let is_favorite = private_metadata
            .as_ref()
            .and_then(|m| m.is_favorite)
            .or(cloud_record.metadata.is_favorite)
            .unwrap_or(false);
        let expires_at_raw = private_metadata
            .as_ref()
            .and_then(|m| m.expires_at.as_deref())
            .or(cloud_record.metadata.expires_at.as_deref());
        let expires_at = expires_at_raw
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&chrono::Utc));
        let updated_by = private_metadata
            .as_ref()
            .and_then(|m| m.updated_by.clone())
            .or_else(|| cloud_record.metadata.updated_by.clone())
            .unwrap_or_else(|| "sync".to_string());
        let tags = private_metadata
            .as_ref()
            .map(|m| m.tags.clone())
            .unwrap_or_else(|| cloud_record.metadata.tags.clone());

        let stored = crate::types::record::StoredRecord {
            id,
            credential_type,
            encrypted_data,
            nonce,
            dek_version: cloud_record.dek_version,
            aad,
            is_favorite,
            expires_at,
            created_at: existing.as_ref().map_or(now, |r| r.created_at),
            updated_at: remote_updated_at,
            updated_by,
            version: cloud_record.version,
            deleted,
            deleted_at,
            tags,
        };

        if is_new {
            crate::db::queries::insert_record(&self.conn, &stored).map_err(db_error_to_vault)?;
        } else {
            let local_version = existing.as_ref().map_or(0, |r| r.version);
            crate::db::queries::update_record(&self.conn, &stored, local_version)
                .map_err(db_error_to_vault)?;
        }

        if let Some(private_metadata) = private_metadata {
            if let Some(health) = private_metadata.health {
                let health_state = health.to_state(id, cloud_record.version);
                crate::db::queries::upsert_record_health_state(&self.conn, &health_state)
                    .map_err(db_error_to_vault)?;
            } else {
                crate::db::queries::delete_record_health_state(&self.conn, &id)
                    .map_err(db_error_to_vault)?;
            }
        }

        Ok(is_new)
    }
}
