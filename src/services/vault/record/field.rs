// Field-level record access (decrypt_field)

use crate::services::vault::VaultServiceImpl;
use uuid::Uuid;

use crate::commands::types::FieldSelector;
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::audit::AuditOperation;
use crate::types::sensitive::SecureStr;

use super::helpers::{db_error_to_vault, extract_field};

impl VaultServiceImpl {
    fn decrypt_field_with_audit(
        &self,
        id: Uuid,
        field: FieldSelector,
        audit_operation: Option<AuditOperation>,
    ) -> Result<SecureStr, VaultError> {
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

        if let Some(operation) = audit_operation {
            queries::insert_audit_entry(&self.conn, operation, Some(&id), Some(&record_name), None)
                .map_err(db_error_to_vault)?;
        }

        Ok(value)
    }

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
        let audit_operation = matches!(field, FieldSelector::Password | FieldSelector::Passphrase)
            .then_some(AuditOperation::RecordViewPassword);
        self.decrypt_field_with_audit(id, field, audit_operation)
    }

    /// Decrypt a field for an explicit clipboard copy operation.
    ///
    /// This records copy-specific audit events instead of a generic password
    /// view event, so audit filters can separate user copy actions.
    pub fn decrypt_field_for_copy(
        &self,
        id: Uuid,
        field: FieldSelector,
    ) -> Result<SecureStr, VaultError> {
        let audit_operation = match field {
            FieldSelector::Password | FieldSelector::Passphrase => {
                AuditOperation::RecordCopyPassword
            }
            FieldSelector::Username | FieldSelector::Url | FieldSelector::Notes => {
                AuditOperation::RecordCopyField
            }
        };
        self.decrypt_field_with_audit(id, field, Some(audit_operation))
    }

    /// Decrypt a field WITHOUT writing any audit entry.
    ///
    /// This is the no-audit counterpart to [`decrypt_field`](Self::decrypt_field).
    /// The caller ASSUMES RESPONSIBILITY for writing an appropriate audit entry
    /// for whatever higher-level operation it performs with the plaintext.
    ///
    /// Used by the SSH agent sign path: a signature is a single user-facing
    /// action, so the agent writes one `AuditOperation::SshSign` row after a
    /// successful sign. A `RecordViewPassword` row for the same event would be
    /// misleading — the user never "viewed" the password; the agent used the
    /// private key internally to sign — so the sign path decrypts the key
    /// material through this no-audit method.
    pub fn decrypt_field_no_audit(
        &self,
        id: Uuid,
        field: FieldSelector,
    ) -> Result<SecureStr, VaultError> {
        self.decrypt_field_with_audit(id, field, None)
    }
}
