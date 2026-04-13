// Record CRUD operations (create, update, delete, restore, get, list, toggle_favorite)

use chrono::Utc;
use uuid::Uuid;

use super::search;
use super::VaultService;
use crate::commands::types::{FieldSelector, RecordFilter, RecordSort, SortDirection, SortField};
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::audit::AuditOperation;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::record::{
    CreateRecordParams, DecryptedRecord, StoredRecord, TuiRecord, UpdateRecordParams,
};
use crate::types::sensitive::SecureStr;

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

        // 3. If Login and password changed, save old password to history
        if stored.credential_type == CredentialType::Login {
            let old_payload = payload::decrypt_payload(
                &self.crypto,
                &stored.encrypted_data,
                &stored.nonce,
                &stored.aad,
                stored.credential_type,
                stored.dek_version,
            )
            .map_err(VaultError::CryptoError)?;

            if password_changed(&old_payload, &params.payload) {
                self._save_password_history(
                    params.id,
                    &stored.encrypted_data,
                    &stored.nonce,
                    stored.dek_version,
                )?;
            }
        }

        // 4. Encrypt new payload
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

        // 5. Update record in DB (with optimistic locking via WHERE version = ?)
        let updated = queries::update_record(&self.conn, &updated_record, params.expected_version)
            .map_err(db_error_to_vault)?;
        if !updated {
            return Err(VaultError::VersionConflict {
                expected: params.expected_version,
                actual: stored.version, // Best guess; the actual version may have changed
            });
        }

        // 6. Clear old tag associations and rebuild new ones
        queries::detach_all_tags_for_record(&self.conn, &params.id).map_err(db_error_to_vault)?;
        for tag_name in &params.tags {
            let tag =
                queries::get_or_create_tag(&self.conn, tag_name).map_err(db_error_to_vault)?;
            queries::attach_tag(&self.conn, &params.id, tag.id).map_err(db_error_to_vault)?;
        }

        // 7. Write audit entry
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

    /// List records matching a filter, with decryption and sorting.
    ///
    /// Queries encrypted records from the database, decrypts name and subtitle
    /// for each record, and applies the requested sort order.
    ///
    /// # Filter behavior
    /// - `All` — all active (non-deleted) records
    /// - `Favorites` — active records where `is_favorite = true`
    /// - `Expired` — active records where `expires_at < now`
    /// - `Trash` — soft-deleted records
    /// - `Tag(name)` — active records with the specified tag
    /// - `Search(query)` — delegates to search module (placeholder: returns empty)
    /// - `HealthIssues` — placeholder, returns empty (S3 implements)
    ///
    /// # Sort behavior
    /// - `Name` — sorted at application layer after decryption
    /// - `CreatedAt` / `UpdatedAt` — sorted at application layer on timestamps
    /// - `UsageFrequency` — no-op (not yet implemented)
    ///
    /// # Errors
    /// - `VaultError::NotUnlocked` if the vault is locked.
    /// - `VaultError::DatabaseError` if a query fails.
    /// - `VaultError::CryptoError` if decryption fails.
    pub fn list_records(
        &self,
        filter: &RecordFilter,
        sort: &RecordSort,
    ) -> Result<Vec<TuiRecord>, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        // Fetch raw records based on filter
        let stored_records = match filter {
            RecordFilter::Trash => {
                queries::list_deleted_records(&self.conn).map_err(db_error_to_vault)?
            }
            RecordFilter::Search(query) => {
                // Fetch all active records, decrypt names/subtitles, then apply search filter
                let stored_records =
                    queries::list_active_records(&self.conn).map_err(db_error_to_vault)?;

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

                    let is_expired = stored.expires_at.is_some_and(|t| t < Utc::now());

                    tui_records.push(TuiRecord {
                        id: stored.id,
                        credential_type: stored.credential_type,
                        name,
                        subtitle,
                        is_favorite: stored.is_favorite,
                        is_expired,
                        expires_at: stored.expires_at,
                        has_weak_password: false,
                        created_at: stored.created_at,
                        updated_at: stored.updated_at,
                        deleted: stored.deleted,
                        deleted_at: stored.deleted_at,
                        tags: stored.tags.clone(),
                        sync_status: None,
                    });
                }

                let mut filtered = search::search_records(&tui_records, query);
                apply_sort(&mut filtered, sort);
                return Ok(filtered);
            }
            RecordFilter::HealthIssues => {
                // Placeholder: S3 implements health-based filtering
                return Ok(vec![]);
            }
            _ => {
                // All, Favorites, Expired, Tag — start from active records
                queries::list_active_records(&self.conn).map_err(db_error_to_vault)?
            }
        };

        let now = Utc::now();

        // Decrypt and build TuiRecords, applying application-layer filters
        let mut tui_records: Vec<TuiRecord> = Vec::with_capacity(stored_records.len());
        for stored in &stored_records {
            // Application-layer filter for Favorites
            if matches!(filter, RecordFilter::Favorites) && !stored.is_favorite {
                continue;
            }

            // Application-layer filter for Expired
            if matches!(filter, RecordFilter::Expired) {
                let is_expired = stored.expires_at.is_some_and(|t| t < now);
                if !is_expired {
                    continue;
                }
            }

            // Application-layer filter for Tag
            if let RecordFilter::Tag(tag_name) = filter {
                if !stored.tags.contains(tag_name) {
                    continue;
                }
            }

            // Decrypt name and subtitle
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

            let is_expired = stored.expires_at.is_some_and(|t| t < now);

            tui_records.push(TuiRecord {
                id: stored.id,
                credential_type: stored.credential_type,
                name,
                subtitle,
                is_favorite: stored.is_favorite,
                is_expired,
                expires_at: stored.expires_at,
                has_weak_password: false, // S3 implements
                created_at: stored.created_at,
                updated_at: stored.updated_at,
                deleted: stored.deleted,
                deleted_at: stored.deleted_at,
                tags: stored.tags.clone(),
                sync_status: None,
            });
        }

        // Application-layer sort
        apply_sort(&mut tui_records, sort);

        Ok(tui_records)
    }

    /// List all records that need DEK migration (dek_version < target).
    pub fn list_records_for_migration(&self, target_dek_version: u32) -> Result<Vec<StoredRecord>, VaultError> {
        queries::list_records_by_dek_version(&self.conn, target_dek_version)
            .map_err(db_error_to_vault)
    }

    /// Re-encrypt a single record from old DEK version to current DEK version.
    /// This performs: decrypt with old DEK -> re-encrypt with current DEK -> update DB.
    pub fn re_encrypt_record(&mut self, record_id: Uuid, old_dek_version: u32) -> Result<(), VaultError> {
        // 1. Get the stored record
        let record = self.get_stored_record(record_id)?;

        // 2. Decrypt with old DEK version
        let plaintext = self.crypto.decrypt(
            &record.encrypted_data,
            &record.nonce,
            &record.aad,
            old_dek_version,
        ).map_err(VaultError::CryptoError)?;

        // 3. Re-encrypt with current DEK
        let (new_encrypted_data, new_nonce) = self.crypto.encrypt(&plaintext, &record.aad)
            .map_err(VaultError::CryptoError)?;

        // 4. Update the record in DB with new ciphertext and dek_version
        let current_version = self.crypto.current_dek_version();
        let mut updated_record = record.clone();
        updated_record.encrypted_data = new_encrypted_data;
        updated_record.nonce = new_nonce;
        updated_record.dek_version = current_version;
        updated_record.updated_at = Utc::now();
        updated_record.version = record.version + 1;

        let updated = queries::update_record(&self.conn, &updated_record, record.version)
            .map_err(db_error_to_vault)?;
        if !updated {
            return Err(VaultError::VersionConflict {
                expected: record.version,
                actual: record.version, // Best guess
            });
        }

        Ok(())
    }

    /// Decrypt and return a single field from a record.
    ///
    /// Unlike `get_decrypted_record`, this method provides fine-grained
    /// audit control: only `FieldSelector::Password` triggers a
    /// `RecordViewPassword` audit entry.
    ///
    /// Returns `VaultError::InvalidField` when the field does not exist
    /// for the credential type (e.g. `Url` on an SSH record) or when the
    /// optional field value is `None`.
    pub fn decrypt_field(&self, id: Uuid, field: FieldSelector) -> Result<SecureStr, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        let stored = self.get_stored_record(id)?;
        let decrypted_payload = payload::decrypt_payload(
            &self.crypto,
            &stored.encrypted_data,
            &stored.nonce,
            &stored.aad,
            stored.credential_type,
            stored.dek_version,
        )
        .map_err(VaultError::CryptoError)?;

        let record_name = decrypted_payload.name().to_string();
        let value = extract_field(stored.credential_type, decrypted_payload, field)?;

        if matches!(field, FieldSelector::Password) {
            queries::insert_audit_entry(
                &self.conn,
                AuditOperation::RecordViewPassword,
                Some(&id),
                Some(&record_name),
                None,
            )
            .map_err(db_error_to_vault)?;
        }

        Ok(value)
    }
}

/// Extract a single field value from an `EncryptedPayload` based on credential
/// type and field selector.
///
/// Takes ownership of `payload` so that `SecureStr` fields can be moved out
/// without cloning (which would panic — see `SecureString::clone`).
///
/// Maps fields according to:
///
/// | FieldSelector | Login      | Api         | Ssh            |
/// |---------------|------------|-------------|----------------|
/// | Password      | password   | secret_key  | private_key    |
/// | Username      | username   | app_id      | public_key     |
/// | Url           | url        | url         | InvalidField   |
/// | Notes         | notes      | notes       | notes          |
fn extract_field(
    ct: CredentialType,
    payload: EncryptedPayload,
    field: FieldSelector,
) -> Result<SecureStr, VaultError> {
    match (ct, payload, field) {
        // ── Login ──────────────────────────────────────────────────────
        (
            CredentialType::Login,
            EncryptedPayload::Login { password, .. },
            FieldSelector::Password,
        ) => Ok(password),
        (
            CredentialType::Login,
            EncryptedPayload::Login { username, .. },
            FieldSelector::Username,
        ) => Ok(SecureStr::new(username)),
        (
            CredentialType::Login,
            EncryptedPayload::Login { url: Some(url), .. },
            FieldSelector::Url,
        ) => Ok(SecureStr::new(url)),
        (
            CredentialType::Login,
            EncryptedPayload::Login {
                notes: Some(notes), ..
            },
            FieldSelector::Notes,
        ) => Ok(SecureStr::new(notes)),

        // ── Api ───────────────────────────────────────────────────────
        (
            CredentialType::Api,
            EncryptedPayload::Api { secret_key, .. },
            FieldSelector::Password,
        ) => Ok(secret_key),
        (CredentialType::Api, EncryptedPayload::Api { app_id, .. }, FieldSelector::Username) => {
            Ok(SecureStr::new(app_id))
        }
        (CredentialType::Api, EncryptedPayload::Api { url: Some(url), .. }, FieldSelector::Url) => {
            Ok(SecureStr::new(url))
        }
        (
            CredentialType::Api,
            EncryptedPayload::Api {
                notes: Some(notes), ..
            },
            FieldSelector::Notes,
        ) => Ok(SecureStr::new(notes)),

        // ── Ssh ───────────────────────────────────────────────────────
        (
            CredentialType::Ssh,
            EncryptedPayload::Ssh {
                private_key: Some(pk),
                ..
            },
            FieldSelector::Password,
        ) => Ok(pk),
        (
            CredentialType::Ssh,
            EncryptedPayload::Ssh { public_key, .. },
            FieldSelector::Username,
        ) => Ok(SecureStr::new(public_key)),
        // Ssh + Url is always invalid regardless of payload content
        (CredentialType::Ssh, _, FieldSelector::Url) => Err(VaultError::InvalidField {
            record_type: CredentialType::Ssh,
            field: FieldSelector::Url,
        }),
        (
            CredentialType::Ssh,
            EncryptedPayload::Ssh {
                notes: Some(notes), ..
            },
            FieldSelector::Notes,
        ) => Ok(SecureStr::new(notes)),

        // ── Catch-all: field missing or credential-type/payload mismatch ──
        _ => Err(VaultError::InvalidField {
            record_type: ct,
            field,
        }),
    }
}

/// Apply the requested sort order to a list of TuiRecords.
fn apply_sort(records: &mut [TuiRecord], sort: &RecordSort) {
    let direction_multiplier: i32 = match sort.direction {
        SortDirection::Asc => 1,
        SortDirection::Desc => -1,
    };

    records.sort_by(|a, b| {
        let cmp = match sort.field {
            SortField::Name => a.name.cmp(&b.name),
            SortField::CreatedAt => a.created_at.cmp(&b.created_at),
            SortField::UpdatedAt => a.updated_at.cmp(&b.updated_at),
            SortField::UsageFrequency => std::cmp::Ordering::Equal, // No-op
        };
        if direction_multiplier == -1 {
            cmp.reverse()
        } else {
            cmp
        }
    });
}

/// Decrypt just the record name from a stored record for audit logging purposes.
fn decrypt_record_name(
    crypto: &crate::crypto::CryptoManager,
    stored: &StoredRecord,
) -> Result<String, VaultError> {
    let payload = payload::decrypt_payload(
        crypto,
        &stored.encrypted_data,
        &stored.nonce,
        &stored.aad,
        stored.credential_type,
        stored.dek_version,
    )
    .map_err(VaultError::CryptoError)?;
    Ok(payload.name().to_string())
}

/// Check whether the password field changed between two Login payloads.
///
/// Returns `false` for non-Login payloads (they have no password field).
fn password_changed(old: &EncryptedPayload, new: &EncryptedPayload) -> bool {
    match (old, new) {
        (
            EncryptedPayload::Login {
                password: old_pw, ..
            },
            EncryptedPayload::Login {
                password: new_pw, ..
            },
        ) => old_pw.get() != new_pw.get(),
        _ => false,
    }
}

/// Map DbError to VaultError, preserving the rusqlite error when possible.
pub(crate) fn db_error_to_vault(e: queries::DbError) -> VaultError {
    match e {
        queries::DbError::Sqlite(se) => VaultError::DatabaseError(se),
        other => VaultError::CryptoError(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::{
        FieldSelector, RecordFilter, RecordSort, SortDirection, SortField,
    };
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::{initialize_metadata, initialize_schema};
    use crate::types::credential::{CredentialType, EncryptedPayload};
    use crate::types::sensitive::SecureStr;
    use rusqlite::Connection;

    /// Helper: create an in-memory VaultService with schema initialized.
    fn setup_service() -> VaultService {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn);
        initialize_metadata(&conn);
        VaultService::new(conn)
    }

    /// Helper: unlock the VaultService with a fresh mnemonic.
    fn unlock_service(svc: &mut VaultService) {
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        svc.crypto
            .unlock_with_mnemonic(&mnemonic)
            .expect("unlock_with_mnemonic must succeed in test");
    }

    /// Helper: create a Login-type EncryptedPayload for testing.
    fn sample_login_payload(name: &str) -> EncryptedPayload {
        EncryptedPayload::Login {
            name: name.to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("s3cret!".to_string()),
            url: Some("https://github.com".to_string()),
            notes: None,
        }
    }

    // --- NotUnlocked guard ---

    #[test]
    fn create_record_returns_not_unlocked_when_locked() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked(), "service must start locked");

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("Test"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        };

        let result = svc.create_record(params);
        assert!(result.is_err(), "create_record must fail when not unlocked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- Successful creation with tags and retrieval ---

    #[test]
    fn create_login_record_and_retrieve_via_queries() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("GitHub"),
            tags: vec!["work".to_string(), "dev".to_string()],
            is_favorite: true,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        // Verify record exists via queries::get_record
        let stored = queries::get_record(&svc.conn, &id)
            .expect("get_record query must succeed")
            .expect("record must exist in DB");

        assert_eq!(stored.id, id);
        assert_eq!(stored.credential_type, CredentialType::Login);
        assert!(stored.is_favorite);
        assert_eq!(stored.version, 1);
        assert!(!stored.deleted);
        // Tags are stored but may not be in insertion order (depends on DB indexing)
        let mut sorted_tags = stored.tags.clone();
        sorted_tags.sort();
        assert_eq!(sorted_tags, vec!["dev", "work"]);

        // Verify AAD is stored correctly
        let expected_aad = format!("record:{}", id);
        assert_eq!(stored.aad, expected_aad.as_bytes());
    }

    // --- Audit log verification ---

    #[test]
    fn create_record_writes_audit_entry() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("MySite"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        // Verify audit entry
        let audit_entries =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");

        assert_eq!(audit_entries.len(), 1, "expected exactly one audit entry");
        let entry = &audit_entries[0];
        assert_eq!(entry.operation, AuditOperation::RecordCreate);
        assert_eq!(entry.record_id, Some(id));
        assert_eq!(entry.record_name.as_deref(), Some("MySite"));
    }

    // --- Returned UUID is valid ---

    #[test]
    fn create_record_returns_valid_uuid() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("UUID Test"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        // Verify it is a valid UUID v4
        assert_eq!(id.get_version(), Some(uuid::Version::Random));
    }

    // --- Encrypted data is not empty and nonce is 24 bytes ---

    #[test]
    fn create_record_stores_encrypted_data_and_nonce() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("Encrypted Check"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        let stored = queries::get_record(&svc.conn, &id)
            .expect("query must succeed")
            .expect("record must exist");

        assert!(
            !stored.encrypted_data.is_empty(),
            "encrypted_data must not be empty"
        );
        assert_eq!(stored.nonce.len(), 24, "nonce must be 24 bytes");
    }

    // --- DEK version is stored correctly ---

    #[test]
    fn create_record_stores_correct_dek_version() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("DEK Test"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        let stored = queries::get_record(&svc.conn, &id)
            .expect("query must succeed")
            .expect("record must exist");

        assert_eq!(stored.dek_version, svc.crypto.current_dek_version());
    }

    // --- Roundtrip: create then decrypt ---

    #[test]
    fn create_record_roundtrip_encrypt_decrypt() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("Roundtrip"),
            tags: vec!["test".to_string()],
            is_favorite: false,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        let stored = queries::get_record(&svc.conn, &id)
            .expect("query must succeed")
            .expect("record must exist");

        // Decrypt using the same CryptoManager
        let aad = format!("record:{}", id);
        let decrypted = payload::decrypt_payload(
            &svc.crypto,
            &stored.encrypted_data,
            &stored.nonce,
            aad.as_bytes(),
            stored.credential_type,
            stored.dek_version,
        )
        .expect("decryption must succeed");

        assert_eq!(decrypted.name(), "Roundtrip");
        assert_eq!(decrypted.credential_type(), CredentialType::Login);
    }

    // =========================================================================
    // get_stored_record tests
    // =========================================================================

    // --- get_stored_record: nonexistent UUID returns RecordNotFound ---

    #[test]
    fn get_stored_record_returns_not_found_for_nonexistent_uuid() {
        let svc = setup_service();
        let nonexistent = Uuid::new_v4();

        let result = svc.get_stored_record(nonexistent);
        assert!(
            result.is_err(),
            "get_stored_record must fail for nonexistent ID"
        );
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound with the given UUID"
        );
    }

    // --- get_stored_record: returns tags matching creation ---

    #[test]
    fn get_stored_record_returns_tags_matching_creation() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("TagCheck"),
            tags: vec!["alpha".to_string(), "beta".to_string()],
            is_favorite: true,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        let stored = svc
            .get_stored_record(id)
            .expect("get_stored_record must succeed");

        assert_eq!(stored.id, id);
        let mut sorted_tags = stored.tags.clone();
        sorted_tags.sort();
        assert_eq!(sorted_tags, vec!["alpha", "beta"]);
    }

    // =========================================================================
    // get_decrypted_record tests
    // =========================================================================

    // --- get_decrypted_record: NotUnlocked guard ---

    #[test]
    fn get_decrypted_record_returns_not_unlocked_when_locked() {
        let mut svc = setup_service();
        // Not unlocked — service starts locked

        let result = svc.get_decrypted_record(Uuid::new_v4());
        assert!(
            result.is_err(),
            "get_decrypted_record must fail when locked"
        );
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- get_decrypted_record: decrypts Login username/password matching creation ---

    #[test]
    fn get_decrypted_record_decrypts_login_credentials() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("DecryptTest"),
            tags: vec!["secure".to_string()],
            is_favorite: false,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        let decrypted = svc
            .get_decrypted_record(id)
            .expect("get_decrypted_record must succeed");

        match decrypted {
            DecryptedRecord::Login {
                name,
                username,
                password,
                url,
                notes,
                tags,
                is_favorite,
                ..
            } => {
                assert_eq!(name, "DecryptTest");
                assert_eq!(username, "alice");
                assert_eq!(password.get(), "s3cret!");
                assert_eq!(url.as_deref(), Some("https://github.com"));
                assert!(notes.is_none());
                assert_eq!(tags, vec!["secure"]);
                assert!(!is_favorite);
            }
            other => panic!("expected DecryptedRecord::Login, got {:?}", other),
        }
    }

    // --- get_decrypted_record: writes audit RecordViewPassword ---

    #[test]
    fn get_decrypted_record_writes_audit_record_view_password() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let params = CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("AuditCheck"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        };

        let id = svc
            .create_record(params)
            .expect("create_record must succeed");

        // One audit entry from create_record
        let before =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(
            before.len(),
            1,
            "expected one audit entry from create_record"
        );

        svc.get_decrypted_record(id)
            .expect("get_decrypted_record must succeed");

        // Now two audit entries: RecordCreate + RecordViewPassword
        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(
            after.len(),
            2,
            "expected two audit entries after get_decrypted_record"
        );

        // Find the RecordViewPassword entry (order may vary if timestamps collide)
        let view_entry = after
            .iter()
            .find(|e| e.operation == AuditOperation::RecordViewPassword)
            .expect("expected a RecordViewPassword audit entry");
        assert_eq!(view_entry.record_id, Some(id));
        assert_eq!(view_entry.record_name.as_deref(), Some("AuditCheck"));
    }

    // --- get_decrypted_record: nonexistent UUID returns RecordNotFound ---

    #[test]
    fn get_decrypted_record_returns_not_found_for_nonexistent_uuid() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let nonexistent = Uuid::new_v4();

        let result = svc.get_decrypted_record(nonexistent);
        assert!(
            result.is_err(),
            "get_decrypted_record must fail for nonexistent ID"
        );
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound with the given UUID"
        );
    }

    // =========================================================================
    // update_record tests
    // =========================================================================

    /// Helper: create a Login record and return its ID.
    fn create_test_login_record(svc: &mut VaultService) -> Uuid {
        svc.create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: sample_login_payload("TestLogin"),
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed")
    }

    // --- update_record: NotUnlocked guard ---

    #[test]
    fn update_record_returns_not_unlocked_when_locked() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());

        let params = UpdateRecordParams {
            id: Uuid::new_v4(),
            payload: sample_login_payload("Test"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        };

        let result = svc.update_record(params);
        assert!(result.is_err(), "update_record must fail when locked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- update_record: version mismatch returns VersionConflict ---

    #[test]
    fn update_record_returns_version_conflict_on_mismatch() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        let params = UpdateRecordParams {
            id,
            payload: sample_login_payload("Updated"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 99, // Wrong version
        };

        let result = svc.update_record(params);
        assert!(
            result.is_err(),
            "update_record must fail on version mismatch"
        );
        match result.unwrap_err() {
            VaultError::VersionConflict { expected, actual } => {
                assert_eq!(expected, 99);
                assert_eq!(actual, 1);
            }
            other => panic!("expected VersionConflict, got {:?}", other),
        }
    }

    // --- update_record: password change creates password history entry ---

    #[test]
    fn update_record_saves_password_history_when_password_changes() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // Verify no history before update
        let count_before = queries::count_password_history(&svc.conn, &id).unwrap();
        assert_eq!(count_before, 0, "no history before update");

        // Update with a different password
        let new_payload = EncryptedPayload::Login {
            name: "TestLogin".to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("newP@ssw0rd!".to_string()),
            url: Some("https://github.com".to_string()),
            notes: None,
        };

        let params = UpdateRecordParams {
            id,
            payload: new_payload,
            tags: vec!["work".to_string()],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        };

        svc.update_record(params)
            .expect("update_record must succeed");

        // Verify password history has one entry
        let count_after = queries::count_password_history(&svc.conn, &id).unwrap();
        assert_eq!(count_after, 1, "one history entry after password change");
    }

    // --- update_record: no history when password unchanged ---

    #[test]
    fn update_record_skips_password_history_when_password_unchanged() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // Update with the same password (only name changes)
        let new_payload = EncryptedPayload::Login {
            name: "TestLoginRenamed".to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("s3cret!".to_string()), // Same password
            url: Some("https://github.com".to_string()),
            notes: None,
        };

        let params = UpdateRecordParams {
            id,
            payload: new_payload,
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        };

        svc.update_record(params)
            .expect("update_record must succeed");

        let count = queries::count_password_history(&svc.conn, &id).unwrap();
        assert_eq!(count, 0, "no history when password unchanged");
    }

    // --- update_record: tags are replaced ---

    #[test]
    fn update_record_replaces_tags() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // Verify initial tags
        let stored_before = svc.get_stored_record(id).unwrap();
        assert_eq!(stored_before.tags, vec!["work"]);

        // Update with different tags
        let params = UpdateRecordParams {
            id,
            payload: sample_login_payload("TestLogin"),
            tags: vec!["personal".to_string(), "email".to_string()],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        };

        svc.update_record(params)
            .expect("update_record must succeed");

        let stored_after = svc.get_stored_record(id).unwrap();
        let mut tags = stored_after.tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["email", "personal"]);
    }

    // --- update_record: audit log contains RecordUpdate ---

    #[test]
    fn update_record_writes_audit_entry() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // One audit entry from create
        let before =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(before.len(), 1);

        let params = UpdateRecordParams {
            id,
            payload: sample_login_payload("TestLoginRenamed"),
            tags: vec![],
            is_favorite: true,
            expires_at: None,
            expected_version: 1,
        };

        svc.update_record(params)
            .expect("update_record must succeed");

        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(after.len(), 2, "expected two audit entries after update");

        let update_entry = after
            .iter()
            .find(|e| e.operation == AuditOperation::RecordUpdate)
            .expect("expected a RecordUpdate audit entry");
        assert_eq!(update_entry.record_id, Some(id));
        assert_eq!(
            update_entry.record_name.as_deref(),
            Some("TestLoginRenamed")
        );
    }

    // --- update_record: version is incremented ---

    #[test]
    fn update_record_increments_version() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        let stored_before = svc.get_stored_record(id).unwrap();
        assert_eq!(stored_before.version, 1);

        let params = UpdateRecordParams {
            id,
            payload: sample_login_payload("TestLoginV2"),
            tags: vec![],
            is_favorite: true,
            expires_at: None,
            expected_version: 1,
        };

        svc.update_record(params)
            .expect("update_record must succeed");

        let stored_after = svc.get_stored_record(id).unwrap();
        assert_eq!(stored_after.version, 2);
        assert!(stored_after.is_favorite);
    }

    // --- update_record: encrypted payload roundtrips ---

    #[test]
    fn update_record_payload_decrypts_correctly() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        let new_payload = EncryptedPayload::Login {
            name: "UpdatedSite".to_string(),
            username: "bob".to_string(),
            password: SecureStr::new("n3wP@ss".to_string()),
            url: None,
            notes: Some("updated notes".to_string()),
        };

        let params = UpdateRecordParams {
            id,
            payload: new_payload,
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        };

        svc.update_record(params)
            .expect("update_record must succeed");

        // Decrypt and verify
        let decrypted = svc.get_decrypted_record(id).expect("must decrypt");
        match decrypted {
            DecryptedRecord::Login {
                name,
                username,
                password,
                url,
                notes,
                ..
            } => {
                assert_eq!(name, "UpdatedSite");
                assert_eq!(username, "bob");
                assert_eq!(password.get(), "n3wP@ss");
                assert!(url.is_none());
                assert_eq!(notes.as_deref(), Some("updated notes"));
            }
            other => panic!("expected Login, got {:?}", other),
        }
    }

    // --- update_record: nonexistent record returns RecordNotFound ---

    #[test]
    fn update_record_returns_not_found_for_nonexistent() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let nonexistent = Uuid::new_v4();

        let params = UpdateRecordParams {
            id: nonexistent,
            payload: sample_login_payload("Ghost"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        };

        let result = svc.update_record(params);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound"
        );
    }

    // --- update_record: double update (consecutive version bumps) ---

    #[test]
    fn update_record_supports_consecutive_updates() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // First update: v1 -> v2
        let params_v2 = UpdateRecordParams {
            id,
            payload: sample_login_payload("V2"),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        };
        svc.update_record(params_v2)
            .expect("first update must succeed");

        // Second update: v2 -> v3
        let params_v3 = UpdateRecordParams {
            id,
            payload: sample_login_payload("V3"),
            tags: vec!["final".to_string()],
            is_favorite: true,
            expires_at: None,
            expected_version: 2,
        };
        svc.update_record(params_v3)
            .expect("second update must succeed");

        let stored = svc.get_stored_record(id).unwrap();
        assert_eq!(stored.version, 3);
        assert!(stored.is_favorite);
        assert_eq!(stored.tags, vec!["final"]);
    }

    // =========================================================================
    // soft_delete_record tests
    // =========================================================================

    // --- soft_delete_record: NotUnlocked guard ---

    #[test]
    fn soft_delete_record_returns_not_unlocked_when_locked() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.soft_delete_record(Uuid::new_v4());
        assert!(result.is_err(), "soft_delete_record must fail when locked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- soft_delete_record: record not in list_records(All) after soft delete ---

    #[test]
    fn soft_delete_record_removes_from_active_listing() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // Record is in active listing before soft delete
        let active_before = queries::list_active_records(&svc.conn).unwrap();
        assert_eq!(active_before.len(), 1);

        svc.soft_delete_record(id)
            .expect("soft_delete_record must succeed");

        // Record is NOT in active listing after soft delete
        let active_after = queries::list_active_records(&svc.conn).unwrap();
        assert!(
            active_after.is_empty(),
            "soft-deleted record must not appear in active listing"
        );

        // Record still exists in DB with deleted = true
        let stored = svc.get_stored_record(id).unwrap();
        assert!(stored.deleted);
        assert!(stored.deleted_at.is_some());
    }

    // --- soft_delete_record: writes audit RecordDelete ---

    #[test]
    fn soft_delete_record_writes_audit_record_delete() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // One audit entry from create
        let before =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(before.len(), 1);

        svc.soft_delete_record(id)
            .expect("soft_delete_record must succeed");

        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(
            after.len(),
            2,
            "expected two audit entries after soft delete"
        );

        let delete_entry = after
            .iter()
            .find(|e| e.operation == AuditOperation::RecordDelete)
            .expect("expected a RecordDelete audit entry");
        assert_eq!(delete_entry.record_id, Some(id));
        assert_eq!(delete_entry.record_name.as_deref(), Some("TestLogin"));
    }

    // --- soft_delete_record: nonexistent record returns RecordNotFound ---

    #[test]
    fn soft_delete_record_returns_not_found_for_nonexistent() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let nonexistent = Uuid::new_v4();

        let result = svc.soft_delete_record(nonexistent);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound"
        );
    }

    // =========================================================================
    // restore_record tests
    // =========================================================================

    // --- restore_record: NotUnlocked guard ---

    #[test]
    fn restore_record_returns_not_unlocked_when_locked() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.restore_record(Uuid::new_v4());
        assert!(result.is_err(), "restore_record must fail when locked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- restore_record: record back in list_records(All) after restore ---

    #[test]
    fn restore_record_returns_to_active_listing() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // Soft delete first
        svc.soft_delete_record(id)
            .expect("soft_delete_record must succeed");
        let after_delete = queries::list_active_records(&svc.conn).unwrap();
        assert!(after_delete.is_empty());

        // Restore
        svc.restore_record(id).expect("restore_record must succeed");

        // Record is back in active listing
        let after_restore = queries::list_active_records(&svc.conn).unwrap();
        assert_eq!(
            after_restore.len(),
            1,
            "restored record must appear in active listing"
        );
        assert_eq!(after_restore[0].id, id);

        // Record fields reflect non-deleted state
        let stored = svc.get_stored_record(id).unwrap();
        assert!(!stored.deleted);
        assert!(stored.deleted_at.is_none());
    }

    // --- restore_record: writes audit RecordRestore ---

    #[test]
    fn restore_record_writes_audit_record_restore() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        svc.soft_delete_record(id)
            .expect("soft_delete_record must succeed");

        // Two audit entries: create + soft_delete
        let before_restore =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(before_restore.len(), 2);

        svc.restore_record(id).expect("restore_record must succeed");

        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(after.len(), 3, "expected three audit entries after restore");

        let restore_entry = after
            .iter()
            .find(|e| e.operation == AuditOperation::RecordRestore)
            .expect("expected a RecordRestore audit entry");
        assert_eq!(restore_entry.record_id, Some(id));
        assert_eq!(restore_entry.record_name.as_deref(), Some("TestLogin"));
    }

    // --- restore_record: nonexistent record returns RecordNotFound ---

    #[test]
    fn restore_record_returns_not_found_for_nonexistent() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let nonexistent = Uuid::new_v4();

        let result = svc.restore_record(nonexistent);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound"
        );
    }

    // =========================================================================
    // hard_delete_record tests
    // =========================================================================

    // --- hard_delete_record: NotUnlocked guard ---

    #[test]
    fn hard_delete_record_returns_not_unlocked_when_locked() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.hard_delete_record(Uuid::new_v4());
        assert!(result.is_err(), "hard_delete_record must fail when locked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- hard_delete_record: cascade deletes record_tags and password_history ---

    #[test]
    fn hard_delete_record_cascades_deletes_tags_and_password_history() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create a record with tags
        let id = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: sample_login_payload("CascadeTest"),
                tags: vec!["tag1".to_string(), "tag2".to_string()],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        // Create password history by updating with a new password
        let new_payload = EncryptedPayload::Login {
            name: "CascadeTest".to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("newPassword123!".to_string()),
            url: Some("https://github.com".to_string()),
            notes: None,
        };
        svc.update_record(UpdateRecordParams {
            id,
            payload: new_payload,
            tags: vec!["tag1".to_string()],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update_record must succeed");

        // Verify password history exists
        let history_count = queries::count_password_history(&svc.conn, &id).unwrap();
        assert_eq!(history_count, 1, "one password history entry should exist");

        // Verify tags exist
        let tags = queries::get_record_tags(&svc.conn, &id).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0], "tag1");

        // Hard delete
        svc.hard_delete_record(id)
            .expect("hard_delete_record must succeed");

        // Record is gone
        let record_result = queries::get_record(&svc.conn, &id).unwrap();
        assert!(
            record_result.is_none(),
            "record must be gone after hard delete"
        );

        // Password history cascade deleted
        let history_after = queries::count_password_history(&svc.conn, &id).unwrap();
        assert_eq!(history_after, 0, "password history must be cascade deleted");

        // Record tags cascade deleted
        let tags_after = queries::get_record_tags(&svc.conn, &id).unwrap();
        assert!(tags_after.is_empty(), "record_tags must be cascade deleted");
    }

    // --- hard_delete_record: writes audit RecordDestroy ---

    #[test]
    fn hard_delete_record_writes_audit_record_destroy() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // One audit entry from create
        let before =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(before.len(), 1);

        svc.hard_delete_record(id)
            .expect("hard_delete_record must succeed");

        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(
            after.len(),
            2,
            "expected two audit entries after hard delete"
        );

        let destroy_entry = after
            .iter()
            .find(|e| e.operation == AuditOperation::RecordDestroy)
            .expect("expected a RecordDestroy audit entry");
        assert_eq!(destroy_entry.record_id, Some(id));
        assert_eq!(destroy_entry.record_name.as_deref(), Some("TestLogin"));
    }

    // --- hard_delete_record: nonexistent record returns RecordNotFound ---

    #[test]
    fn hard_delete_record_returns_not_found_for_nonexistent() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let nonexistent = Uuid::new_v4();

        let result = svc.hard_delete_record(nonexistent);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound"
        );
    }

    // =========================================================================
    // toggle_favorite tests
    // =========================================================================

    // --- toggle_favorite: NotUnlocked guard ---

    #[test]
    fn toggle_favorite_returns_not_unlocked_when_locked() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.toggle_favorite(Uuid::new_v4(), true);
        assert!(result.is_err(), "toggle_favorite must fail when locked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- toggle_favorite: changes is_favorite but not version ---

    #[test]
    fn toggle_favorite_changes_favorite_without_version_increment() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // Initially not favorite, version 1
        let before = svc.get_stored_record(id).unwrap();
        assert!(!before.is_favorite);
        assert_eq!(before.version, 1);

        // Set to favorite
        svc.toggle_favorite(id, true)
            .expect("toggle_favorite must succeed");

        let after = svc.get_stored_record(id).unwrap();
        assert!(
            after.is_favorite,
            "is_favorite should be true after toggle_favorite(true)"
        );
        assert_eq!(
            after.version, 1,
            "version must NOT increment on toggle_favorite"
        );

        // Set back to not favorite
        svc.toggle_favorite(id, false)
            .expect("toggle_favorite must succeed");

        let after_unset = svc.get_stored_record(id).unwrap();
        assert!(
            !after_unset.is_favorite,
            "is_favorite should be false after toggle_favorite(false)"
        );
        assert_eq!(after_unset.version, 1, "version must still not increment");
    }

    // --- toggle_favorite: no audit entry written ---

    #[test]
    fn toggle_favorite_does_not_write_audit_entry() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // One audit entry from create
        let before =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(before.len(), 1);

        svc.toggle_favorite(id, true)
            .expect("toggle_favorite must succeed");

        // Still only one audit entry — no new audit for toggle_favorite
        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(
            after.len(),
            1,
            "toggle_favorite must not write an audit entry"
        );
    }

    // --- toggle_favorite: nonexistent record returns RecordNotFound ---

    #[test]
    fn toggle_favorite_returns_not_found_for_nonexistent() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let nonexistent = Uuid::new_v4();

        let result = svc.toggle_favorite(nonexistent, true);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound"
        );
    }

    // =========================================================================
    // list_records tests
    // =========================================================================

    /// Helper: create a Login record with a specific name and return its ID.
    fn create_named_record(svc: &mut VaultService, name: &str) -> Uuid {
        svc.create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: format!("user_{}", name.to_lowercase()),
                password: SecureStr::new("password123".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed")
    }

    // --- list_records: NotUnlocked guard ---

    #[test]
    fn list_records_returns_not_unlocked_when_locked() {
        let svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.list_records(
            &RecordFilter::All,
            &RecordSort {
                field: SortField::UpdatedAt,
                direction: SortDirection::Desc,
            },
        );
        assert!(result.is_err(), "list_records must fail when locked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- list_records: All filter returns all active records ---

    #[test]
    fn list_records_all_returns_all_active_records() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let _id_a = create_named_record(&mut svc, "Alpha");
        let _id_b = create_named_record(&mut svc, "Bravo");
        let _id_c = create_named_record(&mut svc, "Charlie");

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let records = svc
            .list_records(&RecordFilter::All, &sort)
            .expect("list_records must succeed");

        assert_eq!(records.len(), 3, "should return 3 active records");
    }

    // --- list_records: sort by UpdatedAt Desc uses correct ordering ---

    #[test]
    fn list_records_sorts_by_updated_at_desc() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id_first = create_named_record(&mut svc, "First");
        let _id_second = create_named_record(&mut svc, "Second");
        let _id_third = create_named_record(&mut svc, "Third");

        // Update id_first to give it the most recent updated_at
        svc.update_record(UpdateRecordParams {
            id: id_first,
            payload: EncryptedPayload::Login {
                name: "FirstUpdated".to_string(),
                username: "user_first".to_string(),
                password: SecureStr::new("password123".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update_record must succeed");

        let sort_desc = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let records = svc
            .list_records(&RecordFilter::All, &sort_desc)
            .expect("list_records must succeed");

        assert_eq!(records.len(), 3);
        // id_first was updated last, so it should come first in DESC order
        assert_eq!(records[0].id, id_first);
    }

    // --- list_records: sort by UpdatedAt Asc ---

    #[test]
    fn list_records_sorts_by_updated_at_asc() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let _id_first = create_named_record(&mut svc, "First");
        let _id_second = create_named_record(&mut svc, "Second");
        let id_third = create_named_record(&mut svc, "Third");

        // Update id_third to give it the most recent updated_at
        svc.update_record(UpdateRecordParams {
            id: id_third,
            payload: EncryptedPayload::Login {
                name: "ThirdUpdated".to_string(),
                username: "user_third".to_string(),
                password: SecureStr::new("password123".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update_record must succeed");

        let sort_asc = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Asc,
        };

        let records = svc
            .list_records(&RecordFilter::All, &sort_asc)
            .expect("list_records must succeed");

        assert_eq!(records.len(), 3);
        // id_third was updated last, so it should come last in ASC order
        assert_eq!(records[2].id, id_third);
    }

    // --- list_records: Favorites filter returns only is_favorite records ---

    #[test]
    fn list_records_favorites_returns_only_favorites() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create two non-favorite records
        let _id_a = create_named_record(&mut svc, "Alpha");
        let _id_b = create_named_record(&mut svc, "Bravo");

        // Create a favorite record
        let id_fav = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "FavoriteSite".to_string(),
                    username: "fav_user".to_string(),
                    password: SecureStr::new("fav_pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: true,
                expires_at: None,
            })
            .expect("create_record must succeed");

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let favorites = svc
            .list_records(&RecordFilter::Favorites, &sort)
            .expect("list_records must succeed");

        assert_eq!(favorites.len(), 1, "only favorite records returned");
        assert_eq!(favorites[0].id, id_fav);
        assert!(favorites[0].is_favorite);
    }

    // --- list_records: Tag filter returns only records with that tag ---

    #[test]
    fn list_records_tag_filter_returns_matching_records() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create a record with "work" tag
        let id_work = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "WorkSite".to_string(),
                    username: "work_user".to_string(),
                    password: SecureStr::new("work_pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec!["work".to_string()],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        // Create a record with "personal" tag
        let id_personal = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "PersonalSite".to_string(),
                    username: "personal_user".to_string(),
                    password: SecureStr::new("personal_pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec!["personal".to_string()],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        // Create an untagged record
        let _id_untagged = create_named_record(&mut svc, "Untagged");

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let work_records = svc
            .list_records(&RecordFilter::Tag("work".into()), &sort)
            .expect("list_records must succeed");
        assert_eq!(work_records.len(), 1);
        assert_eq!(work_records[0].id, id_work);

        let personal_records = svc
            .list_records(&RecordFilter::Tag("personal".into()), &sort)
            .expect("list_records must succeed");
        assert_eq!(personal_records.len(), 1);
        assert_eq!(personal_records[0].id, id_personal);

        // Non-existent tag returns empty
        let none = svc
            .list_records(&RecordFilter::Tag("nonexistent".into()), &sort)
            .expect("list_records must succeed");
        assert!(none.is_empty(), "non-existent tag should return empty");
    }

    // --- list_records: Expired filter returns only expired records ---

    #[test]
    fn list_records_expired_returns_only_expired() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let now = Utc::now();

        // Create an expired record (expires_at in the past)
        let id_expired = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "ExpiredSite".to_string(),
                    username: "expired_user".to_string(),
                    password: SecureStr::new("expired_pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: Some(now - chrono::Duration::seconds(1000)),
            })
            .expect("create_record must succeed");

        // Create a valid record (expires_at in the future)
        let _id_valid = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "ValidSite".to_string(),
                    username: "valid_user".to_string(),
                    password: SecureStr::new("valid_pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: Some(now + chrono::Duration::seconds(1000)),
            })
            .expect("create_record must succeed");

        // Create a record with no expiration
        let _id_no_exp = create_named_record(&mut svc, "NoExpiration");

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let expired = svc
            .list_records(&RecordFilter::Expired, &sort)
            .expect("list_records must succeed");

        assert_eq!(expired.len(), 1, "only expired record returned");
        assert_eq!(expired[0].id, id_expired);
        assert!(expired[0].is_expired);
    }

    // --- list_records: Trash filter returns soft-deleted records ---

    #[test]
    fn list_records_trash_returns_only_deleted() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id_active = create_named_record(&mut svc, "Active");
        let id_to_delete = create_named_record(&mut svc, "ToDelete");

        svc.soft_delete_record(id_to_delete)
            .expect("soft_delete must succeed");

        let sort = RecordSort {
            field: SortField::UpdatedAt,
            direction: SortDirection::Desc,
        };

        let trash = svc
            .list_records(&RecordFilter::Trash, &sort)
            .expect("list_records must succeed");

        assert_eq!(trash.len(), 1, "only soft-deleted record in trash");
        assert_eq!(trash[0].id, id_to_delete);
        assert!(trash[0].deleted);

        // Active record should not appear in trash
        assert!(trash.iter().all(|r| r.id != id_active));
    }

    // --- list_records: Name sort sorts by decrypted name alphabetically ---

    #[test]
    fn list_records_name_sort_sorts_by_decrypted_name() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        create_named_record(&mut svc, "Charlie");
        create_named_record(&mut svc, "Alpha");
        create_named_record(&mut svc, "Bravo");

        let sort_asc = RecordSort {
            field: SortField::Name,
            direction: SortDirection::Asc,
        };

        let records = svc
            .list_records(&RecordFilter::All, &sort_asc)
            .expect("list_records must succeed");

        assert_eq!(records.len(), 3);
        assert_eq!(records[0].name, "Alpha");
        assert_eq!(records[1].name, "Bravo");
        assert_eq!(records[2].name, "Charlie");

        // Desc order
        let sort_desc = RecordSort {
            field: SortField::Name,
            direction: SortDirection::Desc,
        };

        let records_desc = svc
            .list_records(&RecordFilter::All, &sort_desc)
            .expect("list_records must succeed");

        assert_eq!(records_desc[0].name, "Charlie");
        assert_eq!(records_desc[1].name, "Bravo");
        assert_eq!(records_desc[2].name, "Alpha");
    }

    // --- list_records: decrypted name and subtitle are populated ---

    #[test]
    fn list_records_populates_decrypted_name_and_subtitle() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let _id = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "GitHub".to_string(),
                    username: "alice".to_string(),
                    password: SecureStr::new("s3cret!".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec!["dev".to_string()],
                is_favorite: true,
                expires_at: None,
            })
            .expect("create_record must succeed");

        let records = svc
            .list_records(
                &RecordFilter::All,
                &RecordSort {
                    field: SortField::UpdatedAt,
                    direction: SortDirection::Desc,
                },
            )
            .expect("list_records must succeed");

        assert_eq!(records.len(), 1);
        let rec = &records[0];
        assert_eq!(rec.name, "GitHub", "name should be decrypted");
        assert_eq!(rec.subtitle, "alice", "subtitle should be decrypted");
        assert!(rec.is_favorite);
        assert_eq!(rec.tags, vec!["dev"]);
    }

    // --- list_records: Search filter returns matching records ---

    #[test]
    fn list_records_search_returns_matching_records() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        create_named_record(&mut svc, "Alpha");
        create_named_record(&mut svc, "Bravo");

        let result = svc.list_records(
            &RecordFilter::Search("Alpha".into()),
            &RecordSort {
                field: SortField::UpdatedAt,
                direction: SortDirection::Desc,
            },
        );

        assert!(result.is_ok());
        let records = result.unwrap();
        assert_eq!(records.len(), 1, "search should return 1 matching record");
        assert_eq!(records[0].name, "Alpha");
    }

    // --- list_records: Search is case-insensitive ---

    #[test]
    fn list_records_search_is_case_insensitive() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        create_named_record(&mut svc, "TestRecord");

        let result = svc.list_records(
            &RecordFilter::Search("testrecord".into()),
            &RecordSort {
                field: SortField::UpdatedAt,
                direction: SortDirection::Desc,
            },
        );

        assert!(result.is_ok());
        let records = result.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "TestRecord");
    }

    // --- list_records: Search with empty query returns all active records ---

    #[test]
    fn list_records_search_empty_returns_all() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        create_named_record(&mut svc, "Alpha");
        create_named_record(&mut svc, "Bravo");

        let result = svc.list_records(
            &RecordFilter::Search("".into()),
            &RecordSort {
                field: SortField::UpdatedAt,
                direction: SortDirection::Desc,
            },
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2, "empty search returns all records");
    }

    // --- list_records: HealthIssues returns empty (placeholder) ---

    #[test]
    fn list_records_health_issues_returns_empty_placeholder() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        create_named_record(&mut svc, "Alpha");

        let result = svc.list_records(
            &RecordFilter::HealthIssues,
            &RecordSort {
                field: SortField::UpdatedAt,
                direction: SortDirection::Desc,
            },
        );

        assert!(result.is_ok());
        assert!(
            result.unwrap().is_empty(),
            "HealthIssues placeholder returns empty"
        );
    }

    // =========================================================================
    // decrypt_field tests
    // =========================================================================

    /// Helper: create an Api record and return its ID.
    fn create_test_api_record(svc: &mut VaultService) -> Uuid {
        svc.create_record(CreateRecordParams {
            credential_type: CredentialType::Api,
            payload: EncryptedPayload::Api {
                name: "TestApi".to_string(),
                app_id: "app-12345".to_string(),
                secret_key: SecureStr::new("sk-secret-abc".to_string()),
                url: Some("https://api.example.com".to_string()),
                notes: Some("API notes here".to_string()),
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed")
    }

    /// Helper: create an Ssh record and return its ID.
    fn create_test_ssh_record(svc: &mut VaultService) -> Uuid {
        svc.create_record(CreateRecordParams {
            credential_type: CredentialType::Ssh,
            payload: EncryptedPayload::Ssh {
                name: "TestSsh".to_string(),
                public_key: "ssh-rsa AAAA...user@host".to_string(),
                private_key: Some(SecureStr::new(
                    "-----BEGIN OPENSSH PRIVATE KEY-----\nxyz\n-----END OPENSSH PRIVATE KEY-----"
                        .to_string(),
                )),
                passphrase: None,
                notes: Some("SSH key notes".to_string()),
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .expect("create_record must succeed")
    }

    // --- decrypt_field: NotUnlocked guard ---

    #[test]
    fn decrypt_field_returns_not_unlocked_when_locked() {
        let svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.decrypt_field(Uuid::new_v4(), FieldSelector::Password);
        assert!(result.is_err(), "decrypt_field must fail when locked");
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- decrypt_field: Login record Password returns correct value ---

    #[test]
    fn decrypt_field_login_password_returns_correct_value() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Password)
            .expect("decrypt_field must succeed");

        assert_eq!(value.get(), "s3cret!", "password value must match");
    }

    // --- decrypt_field: Login record Username returns correct value ---

    #[test]
    fn decrypt_field_login_username_returns_correct_value() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Username)
            .expect("decrypt_field must succeed");

        assert_eq!(value.get(), "alice", "username value must match");
    }

    // --- decrypt_field: Login record Url returns correct value ---

    #[test]
    fn decrypt_field_login_url_returns_correct_value() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Url)
            .expect("decrypt_field must succeed");

        assert_eq!(value.get(), "https://github.com", "url value must match");
    }

    // --- decrypt_field: Login record Url with None returns InvalidField ---

    #[test]
    fn decrypt_field_login_url_none_returns_invalid_field() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        // Create a Login record with url = None
        let id = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "NoUrl".to_string(),
                    username: "user".to_string(),
                    password: SecureStr::new("pass".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        let result = svc.decrypt_field(id, FieldSelector::Url);
        assert!(
            matches!(
                result,
                Err(VaultError::InvalidField {
                    record_type: CredentialType::Login,
                    field: FieldSelector::Url
                })
            ),
            "url=None should return InvalidField, got: {:?}",
            result
        );
    }

    // --- decrypt_field: Api record Username returns app_id ---

    #[test]
    fn decrypt_field_api_username_returns_app_id() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_api_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Username)
            .expect("decrypt_field must succeed");

        assert_eq!(value.get(), "app-12345", "Username should map to app_id");
    }

    // --- decrypt_field: Api record Password returns secret_key ---

    #[test]
    fn decrypt_field_api_password_returns_secret_key() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_api_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Password)
            .expect("decrypt_field must succeed");

        assert_eq!(
            value.get(),
            "sk-secret-abc",
            "Password should map to secret_key"
        );
    }

    // --- decrypt_field: Api record Notes returns notes ---

    #[test]
    fn decrypt_field_api_notes_returns_notes() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_api_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Notes)
            .expect("decrypt_field must succeed");

        assert_eq!(value.get(), "API notes here", "notes value must match");
    }

    // --- decrypt_field: Ssh record Url returns InvalidField ---

    #[test]
    fn decrypt_field_ssh_url_returns_invalid_field() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_ssh_record(&mut svc);

        let result = svc.decrypt_field(id, FieldSelector::Url);
        assert!(
            matches!(
                result,
                Err(VaultError::InvalidField {
                    record_type: CredentialType::Ssh,
                    field: FieldSelector::Url
                })
            ),
            "Ssh + Url should return InvalidField, got: {:?}",
            result
        );
    }

    // --- decrypt_field: Ssh record Password returns private_key ---

    #[test]
    fn decrypt_field_ssh_password_returns_private_key() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_ssh_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Password)
            .expect("decrypt_field must succeed");

        assert!(
            value.get().contains("BEGIN OPENSSH PRIVATE KEY"),
            "Password should map to private_key"
        );
    }

    // --- decrypt_field: Ssh record Username returns public_key ---

    #[test]
    fn decrypt_field_ssh_username_returns_public_key() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_ssh_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Username)
            .expect("decrypt_field must succeed");

        assert!(
            value.get().starts_with("ssh-rsa"),
            "Username should map to public_key"
        );
    }

    // --- decrypt_field: Ssh record Notes returns notes ---

    #[test]
    fn decrypt_field_ssh_notes_returns_notes() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_ssh_record(&mut svc);

        let value = svc
            .decrypt_field(id, FieldSelector::Notes)
            .expect("decrypt_field must succeed");

        assert_eq!(value.get(), "SSH key notes", "notes value must match");
    }

    // --- decrypt_field: Password field writes audit RecordViewPassword ---

    #[test]
    fn decrypt_field_password_writes_audit_record_view_password() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // One audit entry from create_record
        let before =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(before.len(), 1, "one audit entry from create_record");

        svc.decrypt_field(id, FieldSelector::Password)
            .expect("decrypt_field must succeed");

        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(
            after.len(),
            2,
            "two audit entries after decrypt_field(Password)"
        );

        let view_entry = after
            .iter()
            .find(|e| e.operation == AuditOperation::RecordViewPassword)
            .expect("expected a RecordViewPassword audit entry");
        assert_eq!(view_entry.record_id, Some(id));
        assert_eq!(view_entry.record_name.as_deref(), Some("TestLogin"));
    }

    // --- decrypt_field: non-Password fields do NOT write audit ---

    #[test]
    fn decrypt_field_non_password_does_not_write_audit() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let id = create_test_login_record(&mut svc);

        // One audit entry from create_record
        let before =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(before.len(), 1);

        svc.decrypt_field(id, FieldSelector::Username)
            .expect("decrypt_field must succeed");

        let after =
            queries::list_audit_entries(&svc.conn, 10, 0).expect("list_audit_entries must succeed");
        assert_eq!(
            after.len(),
            1,
            "decrypt_field(Username) must not write an audit entry"
        );
    }

    // --- decrypt_field: nonexistent record returns RecordNotFound ---

    #[test]
    fn decrypt_field_returns_not_found_for_nonexistent() {
        let svc = setup_service();
        let nonexistent = Uuid::new_v4();

        let result = svc.decrypt_field(nonexistent, FieldSelector::Password);
        // NotUnlocked since service is locked
        assert!(result.is_err());
    }

    // --- decrypt_field: nonexistent record returns RecordNotFound when unlocked ---

    #[test]
    fn decrypt_field_returns_not_found_for_nonexistent_when_unlocked() {
        let mut svc = setup_service();
        unlock_service(&mut svc);
        let nonexistent = Uuid::new_v4();

        let result = svc.decrypt_field(nonexistent, FieldSelector::Password);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(id) if id == nonexistent),
            "expected RecordNotFound"
        );
    }
}
