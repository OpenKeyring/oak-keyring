// Field-level record access (decrypt_field)

use uuid::Uuid;

use crate::commands::types::FieldSelector;
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::services::vault::VaultService;
use crate::types::audit::AuditOperation;
use crate::types::sensitive::SecureStr;

use super::helpers::{db_error_to_vault, extract_field};

impl VaultService {
    /// Decrypt and return a single field from a record.
    ///
    /// Unlike `get_decrypted_record`, this method provides fine-grained
    /// audit control: only `FieldSelector::Password` and
    /// `FieldSelector::Passphrase` trigger a `RecordViewPassword` audit entry.
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

        if matches!(field, FieldSelector::Password | FieldSelector::Passphrase) {
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
