// Record CRUD operations (create, update, delete, restore, get, list, toggle_favorite)

use chrono::Utc;
use uuid::Uuid;

use super::VaultService;
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::audit::AuditOperation;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::record::{CreateRecordParams, DecryptedRecord, StoredRecord};

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
}

/// Map DbError to VaultError, preserving the rusqlite error when possible.
fn db_error_to_vault(e: queries::DbError) -> VaultError {
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
}
