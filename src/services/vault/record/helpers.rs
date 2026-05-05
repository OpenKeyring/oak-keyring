// Shared helper functions for record operations

use crate::commands::types::{FieldSelector, RecordSort, SortDirection, SortField};
use crate::crypto::payload;
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::credential::{CredentialType, EncryptedPayload};
use crate::types::record::{StoredRecord, TuiRecord};
use crate::types::sensitive::SecureStr;

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
pub(super) fn extract_field(
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

        (
            CredentialType::Ssh,
            EncryptedPayload::Ssh {
                passphrase: Some(pp),
                ..
            },
            FieldSelector::Passphrase,
        ) => Ok(pp),
        // Ssh + Passphrase with None value
        (CredentialType::Ssh, _, FieldSelector::Passphrase) => Err(VaultError::InvalidField {
            record_type: CredentialType::Ssh,
            field: FieldSelector::Passphrase,
        }),

        // ── Catch-all: field missing or credential-type/payload mismatch ──
        _ => Err(VaultError::InvalidField {
            record_type: ct,
            field,
        }),
    }
}

/// Apply the requested sort order to a list of TuiRecords.
///
/// When `freq_map` is `Some`, it maps record ID strings to their access count
/// (from `audit_log`). This is required for `SortField::UsageFrequency` to
/// produce meaningful order; without it, UsageFrequency falls back to equal ordering.
pub(super) fn apply_sort(
    records: &mut [TuiRecord],
    sort: &RecordSort,
    freq_map: Option<&std::collections::HashMap<String, i64>>,
) {
    let direction_multiplier: i32 = match sort.direction {
        SortDirection::Asc => 1,
        SortDirection::Desc => -1,
    };

    records.sort_by(|a, b| {
        let cmp = match sort.field {
            SortField::Name => a.name.cmp(&b.name),
            SortField::CreatedAt => a.created_at.cmp(&b.created_at),
            SortField::UpdatedAt => a.updated_at.cmp(&b.updated_at),
            SortField::UsageFrequency => {
                let freq_a = freq_map
                    .and_then(|m| m.get(&a.id.to_string()))
                    .copied()
                    .unwrap_or(0);
                let freq_b = freq_map
                    .and_then(|m| m.get(&b.id.to_string()))
                    .copied()
                    .unwrap_or(0);
                freq_a.cmp(&freq_b)
            }
        };
        if direction_multiplier == -1 {
            cmp.reverse()
        } else {
            cmp
        }
    });
}

/// Decrypt just the record name from a stored record for audit logging purposes.
pub(super) fn decrypt_record_name(
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
pub(super) fn password_changed(old: &EncryptedPayload, new: &EncryptedPayload) -> bool {
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

/// Check whether `expires_at` changed between two optional timestamps.
pub(super) fn expires_at_changed(
    old: Option<chrono::DateTime<chrono::Utc>>,
    new: Option<chrono::DateTime<chrono::Utc>>,
) -> bool {
    old != new
}

/// Map DbError to VaultError, preserving the rusqlite error when possible.
pub(crate) fn db_error_to_vault(e: queries::DbError) -> VaultError {
    match e {
        queries::DbError::Sqlite(se) => VaultError::DatabaseError(se),
        other => VaultError::CryptoError(other.to_string()),
    }
}
