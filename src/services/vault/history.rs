// Password history (get_password_history, decrypt_history_password, save_conflict_history)

use uuid::Uuid;

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::record::db_error_to_vault;
use crate::types::history::PasswordHistory;
use crate::types::sensitive::SecureStr;

use super::VaultService;

impl VaultService {
    /// Retrieve password history for a record.
    ///
    /// Returns up to 10 entries ordered by `changed_at` descending (newest first).
    /// The vault must be unlocked.
    pub fn get_password_history(
        &self,
        record_id: Uuid,
    ) -> Result<Vec<PasswordHistory>, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        queries::get_password_history(&self.conn, &record_id, 10).map_err(db_error_to_vault)
    }

    /// Decrypt a single password history entry by its ID.
    ///
    /// The AAD used during encryption was `format!("record:{}", record_id)`,
    /// which is reconstructed from the history entry's `record_id` field.
    ///
    /// The encrypted blob stored in history is the full record payload
    /// (externally-tagged JSON). This method decrypts it and extracts the
    /// "password" field (which maps to the primary secret: password, secret_key,
    /// or private_key depending on credential type).
    ///
    /// Returns `VaultError::RecordNotFound` if no history entry with the given ID exists.
    pub fn decrypt_history_password(&self, history_id: i64) -> Result<SecureStr, VaultError> {
        if !self.crypto.is_unlocked() {
            return Err(VaultError::NotUnlocked);
        }

        let entry = queries::get_password_history_by_id(&self.conn, history_id)
            .map_err(db_error_to_vault)?
            .ok_or_else(|| VaultError::RecordNotFound(Uuid::nil()))?;

        let aad = format!("record:{}", entry.record_id);
        let plaintext = self
            .crypto
            .decrypt(
                &entry.encrypted_password,
                &entry.nonce,
                aad.as_bytes(),
                entry.dek_version,
            )
            .map_err(VaultError::CryptoError)?;

        // The encrypted blob is an externally-tagged EncryptedPayload JSON.
        // We only need the primary secret field ("password" for Login variants).
        let json_str = String::from_utf8(plaintext)
            .map_err(|e| VaultError::CryptoError(format!("invalid utf8: {}", e)))?;
        let value: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| VaultError::CryptoError(format!("failed to parse payload JSON: {}", e)))?;

        // Externally-tagged enum: {"Login": {...}} / {"Api": {...}} / {"Ssh": {...}}
        let inner = value
            .as_object()
            .and_then(|obj| obj.iter().next())
            .map(|(_, v)| v)
            .ok_or_else(|| {
                VaultError::CryptoError("expected externally-tagged enum JSON".into())
            })?;

        // Extract the primary secret field (present in all credential types)
        let password = inner
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                VaultError::CryptoError("password field missing in decrypted payload".into())
            })?;

        Ok(SecureStr::new(password.to_string()))
    }

    /// Save the old encrypted payload to password history.
    ///
    /// Stores the existing encrypted blob as-is, avoiding the need to decrypt
    /// and re-encrypt.  If the history for this record exceeds 10 entries,
    /// the oldest entry is pruned.
    pub(crate) fn _save_password_history(
        &mut self,
        record_id: Uuid,
        old_encrypted_data: &[u8],
        old_nonce: &[u8; 24],
        old_dek_version: u32,
    ) -> Result<(), VaultError> {
        // Enforce max 10 history entries
        let count =
            queries::count_password_history(&self.conn, &record_id).map_err(db_error_to_vault)?;
        if count >= 10 {
            queries::delete_oldest_password_history(&self.conn, &record_id)
                .map_err(db_error_to_vault)?;
        }

        let now_ts = chrono::Utc::now().timestamp();
        queries::insert_password_history(
            &self.conn,
            &record_id,
            old_encrypted_data,
            old_nonce,
            old_dek_version,
            now_ts,
        )
        .map_err(db_error_to_vault)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::{initialize_metadata, initialize_schema};
    use crate::types::credential::{CredentialType, EncryptedPayload};
    use crate::types::record::{CreateRecordParams, UpdateRecordParams};
    use crate::types::sensitive::SecureStr;

    use super::*;
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

    /// Helper: create a Login record with a specific password and return its ID.
    fn create_login_with_password(svc: &mut VaultService, name: &str, password: &str) -> Uuid {
        svc.create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: "testuser".to_string(),
                password: SecureStr::new(password.to_string()),
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
    // get_password_history tests
    // =========================================================================

    // --- update password -> get_password_history has new entry ---

    #[test]
    fn get_password_history_returns_entry_after_password_update() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let original_password = "originalP@ss!";
        let id = create_login_with_password(&mut svc, "HistoryTest", original_password);

        // No history before update
        let history_before = svc.get_password_history(id).unwrap();
        assert!(history_before.is_empty(), "no history before first update");

        // Update with a new password to trigger history save
        let new_payload = EncryptedPayload::Login {
            name: "HistoryTest".to_string(),
            username: "testuser".to_string(),
            password: SecureStr::new("newP@ssw0rd!".to_string()),
            url: None,
            notes: None,
        };
        svc.update_record(UpdateRecordParams {
            id,
            payload: new_payload,
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update_record must succeed");

        let history = svc.get_password_history(id).unwrap();
        assert_eq!(history.len(), 1, "one history entry after password change");
    }

    // --- update password -> decrypting history entry yields old password ---

    #[test]
    fn get_password_history_decrypts_to_old_password() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let original_password = "originalP@ss!";
        let id = create_login_with_password(&mut svc, "DecryptHistTest", original_password);

        let new_payload = EncryptedPayload::Login {
            name: "DecryptHistTest".to_string(),
            username: "testuser".to_string(),
            password: SecureStr::new("newP@ssw0rd!".to_string()),
            url: None,
            notes: None,
        };
        svc.update_record(UpdateRecordParams {
            id,
            payload: new_payload,
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update_record must succeed");

        let history = svc.get_password_history(id).unwrap();
        assert_eq!(history.len(), 1);

        // Decrypt the history entry and verify it yields the old password
        let decrypted = svc
            .decrypt_history_password(history[0].id)
            .expect("decrypt_history_password must succeed");
        assert_eq!(
            decrypted.get(),
            original_password,
            "decrypted history password must match original password"
        );
    }

    // --- 11 consecutive updates -> history still only 10 entries ---

    #[test]
    fn get_password_history_caps_at_10_entries() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id = create_login_with_password(&mut svc, "CapTest", "password0");

        // Perform 11 password updates (v1 -> v2 -> ... -> v12)
        for i in 1..=11 {
            let new_payload = EncryptedPayload::Login {
                name: "CapTest".to_string(),
                username: "testuser".to_string(),
                password: SecureStr::new(format!("password{}", i)),
                url: None,
                notes: None,
            };
            svc.update_record(UpdateRecordParams {
                id,
                payload: new_payload,
                tags: vec![],
                is_favorite: false,
                expires_at: None,
                expected_version: i as u64, // version starts at 1, incremented each update
            })
            .expect("update_record must succeed");
        }

        let history = svc.get_password_history(id).unwrap();
        assert_eq!(
            history.len(),
            10,
            "history must be capped at 10 entries after 11 updates"
        );
    }

    // =========================================================================
    // decrypt_history_password tests
    // =========================================================================

    // --- decrypt_history_password correctly decrypts, returns SecureStr ---

    #[test]
    fn decrypt_history_password_returns_secure_str_with_correct_value() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let old_password = "my0ldP@ssword!";
        let id = create_login_with_password(&mut svc, "DecryptTest", old_password);

        svc.update_record(UpdateRecordParams {
            id,
            payload: EncryptedPayload::Login {
                name: "DecryptTest".to_string(),
                username: "testuser".to_string(),
                password: SecureStr::new("br@ndNewP@ss!".to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            expected_version: 1,
        })
        .expect("update_record must succeed");

        let history = svc.get_password_history(id).unwrap();
        assert_eq!(history.len(), 1);

        let result = svc
            .decrypt_history_password(history[0].id)
            .expect("decrypt_history_password must succeed");

        assert_eq!(result.get(), old_password);
    }

    // --- nonexistent history_id returns appropriate error ---

    #[test]
    fn decrypt_history_password_returns_error_for_nonexistent_id() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let result = svc.decrypt_history_password(99999);
        assert!(result.is_err(), "should fail for nonexistent history_id");
        assert!(
            matches!(result.unwrap_err(), VaultError::RecordNotFound(_)),
            "expected RecordNotFound for nonexistent history_id"
        );
    }

    // --- get_password_history returns NotUnlocked when locked ---

    #[test]
    fn get_password_history_returns_not_unlocked_when_locked() {
        let svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.get_password_history(Uuid::new_v4());
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- decrypt_history_password returns NotUnlocked when locked ---

    #[test]
    fn decrypt_history_password_returns_not_unlocked_when_locked() {
        let svc = setup_service();
        assert!(!svc.is_unlocked());

        let result = svc.decrypt_history_password(1);
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), VaultError::NotUnlocked),
            "expected NotUnlocked error"
        );
    }

    // --- get_password_history returns empty for record with no history ---

    #[test]
    fn get_password_history_returns_empty_for_no_history() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id = create_login_with_password(&mut svc, "NoHistory", "pass123");

        let history = svc.get_password_history(id).unwrap();
        assert!(history.is_empty(), "no history for newly created record");
    }
}
