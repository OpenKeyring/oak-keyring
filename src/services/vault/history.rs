// Password history (get_password_history, decrypt_history_password, save_conflict_history)

#[cfg(test)]
use crate::services::vault::VaultService;
use crate::services::vault::VaultServiceImpl;
use uuid::Uuid;

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::record::db_error_to_vault;
use crate::types::credential::CredentialType;
use crate::types::history::PasswordHistory;
use crate::types::sensitive::SecureStr;

impl VaultServiceImpl {
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

    /// Save the current encrypted payload to password history during S2 sync conflict.
    ///
    /// This method is designed for the sync service (S2) to call when a conflict
    /// is detected. It **never returns an error** to the caller — all errors are
    /// logged/swallowed so that conflict history saving never blocks the sync.
    ///
    /// Behavior:
    /// 1. If the vault is not unlocked, silently returns `Ok(())`.
    /// 2. If the record does not exist, silently returns `Ok(())`.
    /// 3. If the record's `credential_type` is not `Login`, silently returns `Ok(())`.
    /// 4. Otherwise, saves the current encrypted blob to password history (pruning
    ///    oldest entries when count exceeds 10).
    pub fn save_conflict_history(&mut self, record_id: Uuid) -> Result<(), VaultError> {
        // If not unlocked, silently return Ok
        if !self.crypto.is_unlocked() {
            return Ok(());
        }

        // Try to get the record — if not found, silently return Ok
        let stored = match self.get_stored_record(record_id) {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };

        // Only Login records get conflict history
        if stored.credential_type != CredentialType::Login {
            return Ok(());
        }

        // Try to save — wrap in catch-all to never block S2 callers
        let _ = self._save_password_history(
            record_id,
            &stored.encrypted_data,
            &stored.nonce,
            stored.dek_version,
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::init_db_in_memory;
    use crate::types::credential::{CredentialType, EncryptedPayload};
    use crate::types::record::{CreateRecordParams, UpdateRecordParams};
    use crate::types::sensitive::SecureStr;

    use super::*;

    /// Helper: create an in-memory VaultService with schema initialized.
    fn setup_service() -> VaultService {
        let conn = init_db_in_memory().unwrap();
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
            decrypted.expose(),
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

        assert_eq!(result.expose(), old_password);
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

    // =========================================================================
    // save_conflict_history tests
    // =========================================================================

    // --- save_conflict_history for Login appends history entry ---

    #[test]
    fn save_conflict_history_for_login_appends_history_entry() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id = create_login_with_password(&mut svc, "ConflictLogin", "oldP@ss!");

        // No history before conflict save
        let history_before = svc.get_password_history(id).unwrap();
        assert!(
            history_before.is_empty(),
            "no history before save_conflict_history"
        );

        // Call save_conflict_history
        svc.save_conflict_history(id)
            .expect("save_conflict_history must return Ok");

        // One history entry now
        let history_after = svc.get_password_history(id).unwrap();
        assert_eq!(
            history_after.len(),
            1,
            "save_conflict_history must append one history entry for Login"
        );
    }

    // --- save_conflict_history for Api silently skips ---

    #[test]
    fn save_conflict_history_for_api_silently_skips() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Api,
                payload: EncryptedPayload::Api {
                    name: "ConflictApi".to_string(),
                    app_id: "app-123".to_string(),
                    secret_key: SecureStr::new("sk-secret".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        svc.save_conflict_history(id)
            .expect("save_conflict_history must return Ok for Api records");

        // No history entries for Api records
        let count = queries::count_password_history(&svc.conn, &id).unwrap();
        assert_eq!(
            count, 0,
            "no history for Api record after save_conflict_history"
        );
    }

    // --- save_conflict_history for nonexistent record returns Ok ---

    #[test]
    fn save_conflict_history_for_nonexistent_record_returns_ok() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let nonexistent = Uuid::new_v4();
        svc.save_conflict_history(nonexistent)
            .expect("save_conflict_history must return Ok for nonexistent record");
    }

    // --- save_conflict_history when DEK not unlocked returns Ok ---

    #[test]
    fn save_conflict_history_when_not_unlocked_returns_ok() {
        let mut svc = setup_service();
        assert!(!svc.is_unlocked());

        svc.save_conflict_history(Uuid::new_v4())
            .expect("save_conflict_history must return Ok when not unlocked");
    }

    // --- save_conflict_history for Ssh silently skips ---

    #[test]
    fn save_conflict_history_for_ssh_silently_skips() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let id = svc
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Ssh,
                payload: EncryptedPayload::Ssh {
                    name: "ConflictSsh".to_string(),
                    public_key: "ssh-rsa AAAA...".to_string(),
                    private_key: Some(SecureStr::new("private-key-data".to_string())),
                    passphrase: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create_record must succeed");

        svc.save_conflict_history(id)
            .expect("save_conflict_history must return Ok for Ssh records");

        let count = queries::count_password_history(&svc.conn, &id).unwrap();
        assert_eq!(
            count, 0,
            "no history for Ssh record after save_conflict_history"
        );
    }

    // --- save_conflict_history decryptable to original password ---

    #[test]
    fn save_conflict_history_stores_decryptable_payload() {
        let mut svc = setup_service();
        unlock_service(&mut svc);

        let original_password = "conflictP@ss!";
        let id = create_login_with_password(&mut svc, "ConflictDecrypt", original_password);

        svc.save_conflict_history(id)
            .expect("save_conflict_history must succeed");

        let history = svc.get_password_history(id).unwrap();
        assert_eq!(history.len(), 1);

        // The history entry should decrypt to the original password
        let decrypted = svc
            .decrypt_history_password(history[0].id)
            .expect("decrypt_history_password must succeed");
        assert_eq!(
            decrypted.expose(),
            original_password,
            "conflict history must decrypt to original password"
        );
    }
}
