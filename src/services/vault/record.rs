// Record CRUD operations (create, update, delete, restore, get, list, toggle_favorite)

use chrono::Utc;
use uuid::Uuid;

use super::VaultService;
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::audit::AuditOperation;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::record::{CreateRecordParams, DecryptedRecord, StoredRecord, UpdateRecordParams};

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
}
