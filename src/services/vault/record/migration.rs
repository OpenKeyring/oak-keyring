// DEK migration (list_records_for_migration, re_encrypt_record)

use crate::services::vault::VaultServiceImpl;
use chrono::Utc;
use uuid::Uuid;

use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::record::StoredRecord;

use super::helpers::db_error_to_vault;

impl VaultServiceImpl {
    /// List all records that need DEK migration (dek_version < target).
    pub fn list_records_for_migration(
        &self,
        target_dek_version: u32,
    ) -> Result<Vec<StoredRecord>, VaultError> {
        queries::list_records_by_dek_version(&self.conn, target_dek_version)
            .map_err(db_error_to_vault)
    }

    /// Re-encrypt a single record from old DEK version to current DEK version.
    /// This performs: decrypt with old DEK -> re-encrypt with current DEK -> update DB.
    pub fn re_encrypt_record(
        &mut self,
        record_id: Uuid,
        old_dek_version: u32,
    ) -> Result<(), VaultError> {
        // 1. Get the stored record
        let record = self.get_stored_record(record_id)?;

        // 2. Decrypt with old DEK version
        let plaintext = self
            .crypto
            .decrypt(
                &record.encrypted_data,
                &record.nonce,
                &record.aad,
                old_dek_version,
            )
            .map_err(VaultError::CryptoError)?;

        // 3. Re-encrypt with current DEK
        let (new_encrypted_data, new_nonce) = self
            .crypto
            .encrypt(&plaintext, &record.aad)
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
}
