use crate::errors::ErrorLevel;

/// Unified error code enum for the entire application.
///
/// Single enum flattened with 43 variants, organized by module prefix.
/// Enables exhaustive match checking for compile-time safety.
///
/// Per spec §5.7, each variant maps to a specific `ErrorLevel` and i18n key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // === VAULT (S1) — 8 variants ===
    VaultRecordNotFound,
    VaultVersionConflict,
    VaultTagAlreadyExists,
    VaultTagNotFound,
    VaultNotUnlocked,
    VaultDatabaseCorrupted,
    VaultDatabaseIoError,
    VaultInvalidField,

    // === DATA (D1) — 6 variants ===
    DataInvalidCredentialType,
    DataInvalidAuditOperation,
    DataInvalidUuid,
    DataMissingField,
    DataFieldTooLong,
    DataEmptyField,

    // === CRYPTO (D2) — 5 variants ===
    CryptoDecryptionFailed,
    CryptoEncryptionFailed,
    CryptoKeyDerivationFailed,
    CryptoInvalidNonce,
    CryptoAadMismatch,

    // === SYNC (S2) — 8 variants ===
    SyncConnectionTimeout,
    SyncAuthenticationFailed,
    SyncProviderError,
    SyncConflictDetected,
    SyncNetworkUnreachable,
    SyncDiskFull,
    SyncMetadataCorrupted,
    SyncVaultIdentityMismatch,
    SyncNotConfigured,
    SyncPauseFailed,

    // === ROTATION (S6) — 2 variants ===
    DekRotationFailed,
    ExportSessionCreateFailed,

    // === HEALTH (S3) — 3 variants ===
    HealthCheckFailed,
    HealthHibpApiError,
    HealthHibpRateLimited,

    // === CLIPBOARD (S4) — 3 variants ===
    ClipboardUnavailable,
    ClipboardCopyFailed,
    ClipboardClearFailed,

    // === IMPORT/EXPORT — 8 variants ===
    ImportFileUnreadable,
    ImportFileFormatInvalid,
    ImportPasswordRequired,
    ImportPasswordIncorrect,
    ImportColumnMappingInvalid,
    ImportPartialFailure,
    ExportWriteFailed,
    ExportPathInvalid,

    // === EXECUTOR (S5) — 2 variants ===
    ExecutorVaultLocked,
    ExecutorMasterPasswordRequired,
}

impl ErrorCode {
    /// Returns the error level per spec §5.7 static mapping table.
    pub fn level(&self) -> ErrorLevel {
        match self {
            // Fatal: database corruption, file I/O unrecoverable, vault root data corruption
            ErrorCode::VaultDatabaseCorrupted => ErrorLevel::Fatal,
            ErrorCode::VaultDatabaseIoError => ErrorLevel::Fatal,
            ErrorCode::CryptoEncryptionFailed => ErrorLevel::Fatal,
            ErrorCode::CryptoAadMismatch => ErrorLevel::Fatal,
            ErrorCode::SyncVaultIdentityMismatch => ErrorLevel::Fatal,

            // Operation: single record failure, decryption failure, version conflict
            ErrorCode::VaultRecordNotFound => ErrorLevel::Operation,
            ErrorCode::VaultVersionConflict => ErrorLevel::Operation,
            ErrorCode::VaultNotUnlocked => ErrorLevel::Operation,
            ErrorCode::VaultInvalidField => ErrorLevel::Operation,
            ErrorCode::CryptoDecryptionFailed => ErrorLevel::Operation,
            ErrorCode::CryptoKeyDerivationFailed => ErrorLevel::Operation,
            ErrorCode::CryptoInvalidNonce => ErrorLevel::Operation,
            ErrorCode::SyncConflictDetected => ErrorLevel::Operation,
            ErrorCode::SyncMetadataCorrupted => ErrorLevel::Operation,
            ErrorCode::ImportFileUnreadable => ErrorLevel::Operation,
            ErrorCode::ImportFileFormatInvalid => ErrorLevel::Operation,
            ErrorCode::ImportPasswordRequired => ErrorLevel::Operation,
            ErrorCode::ImportPasswordIncorrect => ErrorLevel::Operation,
            ErrorCode::ImportColumnMappingInvalid => ErrorLevel::Operation,
            ErrorCode::ImportPartialFailure => ErrorLevel::Operation,
            ErrorCode::ExportWriteFailed => ErrorLevel::Operation,
            ErrorCode::ExportPathInvalid => ErrorLevel::Operation,
            ErrorCode::ExecutorVaultLocked => ErrorLevel::Operation,
            ErrorCode::ExecutorMasterPasswordRequired => ErrorLevel::Operation,

            // Minor: clipboard unavailable, sync timeout, HIBP rate limited, tag already exists
            ErrorCode::VaultTagAlreadyExists => ErrorLevel::Minor,
            ErrorCode::VaultTagNotFound => ErrorLevel::Minor,
            ErrorCode::DataInvalidCredentialType => ErrorLevel::Minor,
            ErrorCode::DataInvalidAuditOperation => ErrorLevel::Minor,
            ErrorCode::DataInvalidUuid => ErrorLevel::Minor,
            ErrorCode::DataMissingField => ErrorLevel::Minor,
            ErrorCode::DataFieldTooLong => ErrorLevel::Minor,
            ErrorCode::DataEmptyField => ErrorLevel::Minor,
            ErrorCode::SyncConnectionTimeout => ErrorLevel::Minor,
            ErrorCode::SyncAuthenticationFailed => ErrorLevel::Minor,
            ErrorCode::SyncProviderError => ErrorLevel::Minor,
            ErrorCode::SyncNetworkUnreachable => ErrorLevel::Minor,
            ErrorCode::SyncDiskFull => ErrorLevel::Minor,
            ErrorCode::SyncNotConfigured => ErrorLevel::Minor,
            ErrorCode::SyncPauseFailed => ErrorLevel::Minor,
            ErrorCode::DekRotationFailed => ErrorLevel::Operation,
            ErrorCode::ExportSessionCreateFailed => ErrorLevel::Operation,
            ErrorCode::HealthCheckFailed => ErrorLevel::Minor,
            ErrorCode::HealthHibpApiError => ErrorLevel::Minor,
            ErrorCode::HealthHibpRateLimited => ErrorLevel::Minor,
            ErrorCode::ClipboardUnavailable => ErrorLevel::Minor,
            ErrorCode::ClipboardCopyFailed => ErrorLevel::Minor,
            ErrorCode::ClipboardClearFailed => ErrorLevel::Minor,
        }
    }

    /// Returns the i18n message key per spec §6.7.
    /// Keys follow `tui.error.<module>_<specific_error>` naming convention.
    pub fn message_key(&self) -> &'static str {
        match self {
            // VAULT
            ErrorCode::VaultRecordNotFound => "tui.error.vault_record_not_found",
            ErrorCode::VaultVersionConflict => "tui.error.vault_version_conflict",
            ErrorCode::VaultTagAlreadyExists => "tui.error.vault_tag_already_exists",
            ErrorCode::VaultTagNotFound => "tui.error.vault_tag_not_found",
            ErrorCode::VaultNotUnlocked => "tui.error.vault_not_unlocked",
            ErrorCode::VaultDatabaseCorrupted => "tui.error.vault_database_corrupted",
            ErrorCode::VaultDatabaseIoError => "tui.error.vault_database_io_error",
            ErrorCode::VaultInvalidField => "tui.error.vault_invalid_field",

            // DATA
            ErrorCode::DataInvalidCredentialType => "tui.error.data_invalid_credential_type",
            ErrorCode::DataInvalidAuditOperation => "tui.error.data_invalid_audit_operation",
            ErrorCode::DataInvalidUuid => "tui.error.data_invalid_uuid",
            ErrorCode::DataMissingField => "tui.error.data_missing_field",
            ErrorCode::DataFieldTooLong => "tui.error.data_field_too_long",
            ErrorCode::DataEmptyField => "tui.error.data_empty_field",

            // CRYPTO
            ErrorCode::CryptoDecryptionFailed => "tui.error.crypto_decryption_failed",
            ErrorCode::CryptoEncryptionFailed => "tui.error.crypto_encryption_failed",
            ErrorCode::CryptoKeyDerivationFailed => "tui.error.crypto_key_derivation_failed",
            ErrorCode::CryptoInvalidNonce => "tui.error.crypto_invalid_nonce",
            ErrorCode::CryptoAadMismatch => "tui.error.crypto_aad_mismatch",

            // SYNC
            ErrorCode::SyncConnectionTimeout => "tui.error.sync_connection_timeout",
            ErrorCode::SyncAuthenticationFailed => "tui.error.sync_authentication_failed",
            ErrorCode::SyncProviderError => "tui.error.sync_provider_error",
            ErrorCode::SyncConflictDetected => "tui.error.sync_conflict_detected",
            ErrorCode::SyncNetworkUnreachable => "tui.error.sync_network_unreachable",
            ErrorCode::SyncDiskFull => "tui.error.sync_disk_full",
            ErrorCode::SyncMetadataCorrupted => "tui.error.sync_metadata_corrupted",
            ErrorCode::SyncVaultIdentityMismatch => "tui.error.sync_vault_identity_mismatch",
            ErrorCode::SyncNotConfigured => "tui.error.sync_not_configured",
            ErrorCode::SyncPauseFailed => "tui.error.sync_pause_failed",

            // ROTATION
            ErrorCode::DekRotationFailed => "tui.error.dek_rotation_failed",
            ErrorCode::ExportSessionCreateFailed => "tui.error.export_session_create_failed",

            // HEALTH
            ErrorCode::HealthCheckFailed => "tui.error.health_check_failed",
            ErrorCode::HealthHibpApiError => "tui.error.health_hibp_api_error",
            ErrorCode::HealthHibpRateLimited => "tui.error.health_hibp_rate_limited",

            // CLIPBOARD
            ErrorCode::ClipboardUnavailable => "tui.error.clipboard_unavailable",
            ErrorCode::ClipboardCopyFailed => "tui.error.clipboard_copy_failed",
            ErrorCode::ClipboardClearFailed => "tui.error.clipboard_clear_failed",

            // IMPORT/EXPORT
            ErrorCode::ImportFileUnreadable => "tui.error.import_file_unreadable",
            ErrorCode::ImportFileFormatInvalid => "tui.error.import_file_format_invalid",
            ErrorCode::ImportPasswordRequired => "tui.error.import_password_required",
            ErrorCode::ImportPasswordIncorrect => "tui.error.import_password_incorrect",
            ErrorCode::ImportColumnMappingInvalid => "tui.error.import_column_mapping_invalid",
            ErrorCode::ImportPartialFailure => "tui.error.import_partial_failure",
            ErrorCode::ExportWriteFailed => "tui.error.export_write_failed",
            ErrorCode::ExportPathInvalid => "tui.error.export_path_invalid",

            // EXECUTOR
            ErrorCode::ExecutorVaultLocked => "tui.error.executor_vault_locked",
            ErrorCode::ExecutorMasterPasswordRequired => {
                "tui.error.executor_master_password_required"
            }
        }
    }

    /// Returns the module prefix for grouping: "vault", "sync", "crypto", etc.
    pub fn module_prefix(&self) -> &'static str {
        match self {
            ErrorCode::VaultRecordNotFound
            | ErrorCode::VaultVersionConflict
            | ErrorCode::VaultTagAlreadyExists
            | ErrorCode::VaultTagNotFound
            | ErrorCode::VaultNotUnlocked
            | ErrorCode::VaultDatabaseCorrupted
            | ErrorCode::VaultDatabaseIoError
            | ErrorCode::VaultInvalidField => "vault",

            ErrorCode::DataInvalidCredentialType
            | ErrorCode::DataInvalidAuditOperation
            | ErrorCode::DataInvalidUuid
            | ErrorCode::DataMissingField
            | ErrorCode::DataFieldTooLong
            | ErrorCode::DataEmptyField => "data",

            ErrorCode::CryptoDecryptionFailed
            | ErrorCode::CryptoEncryptionFailed
            | ErrorCode::CryptoKeyDerivationFailed
            | ErrorCode::CryptoInvalidNonce
            | ErrorCode::CryptoAadMismatch => "crypto",

            ErrorCode::SyncConnectionTimeout
            | ErrorCode::SyncAuthenticationFailed
            | ErrorCode::SyncProviderError
            | ErrorCode::SyncConflictDetected
            | ErrorCode::SyncNetworkUnreachable
            | ErrorCode::SyncDiskFull
            | ErrorCode::SyncMetadataCorrupted
            | ErrorCode::SyncVaultIdentityMismatch
            | ErrorCode::SyncNotConfigured
            | ErrorCode::SyncPauseFailed => "sync",

            ErrorCode::DekRotationFailed | ErrorCode::ExportSessionCreateFailed => "rotation",

            ErrorCode::HealthCheckFailed
            | ErrorCode::HealthHibpApiError
            | ErrorCode::HealthHibpRateLimited => "health",

            ErrorCode::ClipboardUnavailable
            | ErrorCode::ClipboardCopyFailed
            | ErrorCode::ClipboardClearFailed => "clipboard",

            ErrorCode::ImportFileUnreadable
            | ErrorCode::ImportFileFormatInvalid
            | ErrorCode::ImportPasswordRequired
            | ErrorCode::ImportPasswordIncorrect
            | ErrorCode::ImportColumnMappingInvalid
            | ErrorCode::ImportPartialFailure
            | ErrorCode::ExportWriteFailed
            | ErrorCode::ExportPathInvalid => "import_export",

            ErrorCode::ExecutorVaultLocked | ErrorCode::ExecutorMasterPasswordRequired => {
                "executor"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fatal_level_mapping() {
        assert_eq!(ErrorCode::VaultDatabaseCorrupted.level(), ErrorLevel::Fatal);
        assert_eq!(ErrorCode::VaultDatabaseIoError.level(), ErrorLevel::Fatal);
        assert_eq!(ErrorCode::CryptoEncryptionFailed.level(), ErrorLevel::Fatal);
        assert_eq!(ErrorCode::CryptoAadMismatch.level(), ErrorLevel::Fatal);
        assert_eq!(
            ErrorCode::SyncVaultIdentityMismatch.level(),
            ErrorLevel::Fatal
        );
    }

    #[test]
    fn operation_level_mapping() {
        assert_eq!(
            ErrorCode::VaultRecordNotFound.level(),
            ErrorLevel::Operation
        );
        assert_eq!(
            ErrorCode::VaultVersionConflict.level(),
            ErrorLevel::Operation
        );
        assert_eq!(
            ErrorCode::CryptoDecryptionFailed.level(),
            ErrorLevel::Operation
        );
        assert_eq!(
            ErrorCode::SyncConflictDetected.level(),
            ErrorLevel::Operation
        );
        assert_eq!(
            ErrorCode::ExecutorVaultLocked.level(),
            ErrorLevel::Operation
        );
    }

    #[test]
    fn minor_level_mapping() {
        assert_eq!(ErrorCode::VaultTagAlreadyExists.level(), ErrorLevel::Minor);
        assert_eq!(ErrorCode::SyncConnectionTimeout.level(), ErrorLevel::Minor);
        assert_eq!(ErrorCode::ClipboardUnavailable.level(), ErrorLevel::Minor);
        assert_eq!(ErrorCode::HealthHibpRateLimited.level(), ErrorLevel::Minor);
    }

    #[test]
    fn message_key_format() {
        assert_eq!(
            ErrorCode::VaultRecordNotFound.message_key(),
            "tui.error.vault_record_not_found"
        );
        assert_eq!(
            ErrorCode::SyncConnectionTimeout.message_key(),
            "tui.error.sync_connection_timeout"
        );
        assert_eq!(
            ErrorCode::CryptoDecryptionFailed.message_key(),
            "tui.error.crypto_decryption_failed"
        );
    }

    #[test]
    fn module_prefix_returns_correct_group() {
        assert_eq!(ErrorCode::VaultRecordNotFound.module_prefix(), "vault");
        assert_eq!(ErrorCode::SyncConnectionTimeout.module_prefix(), "sync");
        assert_eq!(ErrorCode::CryptoDecryptionFailed.module_prefix(), "crypto");
        assert_eq!(ErrorCode::ClipboardUnavailable.module_prefix(), "clipboard");
        assert_eq!(ErrorCode::ExecutorVaultLocked.module_prefix(), "executor");
    }

    #[test]
    fn exhaustiveness_check() {
        // This test ensures all variants are handled in level(), message_key(), and module_prefix()
        // Adding a new variant without updating these methods will cause a compile error.
        let all_variants = [
            ErrorCode::VaultRecordNotFound,
            ErrorCode::VaultVersionConflict,
            ErrorCode::VaultTagAlreadyExists,
            ErrorCode::VaultTagNotFound,
            ErrorCode::VaultNotUnlocked,
            ErrorCode::VaultDatabaseCorrupted,
            ErrorCode::VaultDatabaseIoError,
            ErrorCode::VaultInvalidField,
            ErrorCode::DataInvalidCredentialType,
            ErrorCode::DataInvalidAuditOperation,
            ErrorCode::DataInvalidUuid,
            ErrorCode::DataMissingField,
            ErrorCode::DataFieldTooLong,
            ErrorCode::DataEmptyField,
            ErrorCode::CryptoDecryptionFailed,
            ErrorCode::CryptoEncryptionFailed,
            ErrorCode::CryptoKeyDerivationFailed,
            ErrorCode::CryptoInvalidNonce,
            ErrorCode::CryptoAadMismatch,
            ErrorCode::SyncConnectionTimeout,
            ErrorCode::SyncAuthenticationFailed,
            ErrorCode::SyncProviderError,
            ErrorCode::SyncConflictDetected,
            ErrorCode::SyncNetworkUnreachable,
            ErrorCode::SyncDiskFull,
            ErrorCode::SyncMetadataCorrupted,
            ErrorCode::SyncVaultIdentityMismatch,
            ErrorCode::SyncNotConfigured,
            ErrorCode::SyncPauseFailed,
            ErrorCode::HealthCheckFailed,
            ErrorCode::HealthHibpApiError,
            ErrorCode::HealthHibpRateLimited,
            ErrorCode::ClipboardUnavailable,
            ErrorCode::ClipboardCopyFailed,
            ErrorCode::ClipboardClearFailed,
            ErrorCode::ImportFileUnreadable,
            ErrorCode::ImportFileFormatInvalid,
            ErrorCode::ImportPasswordRequired,
            ErrorCode::ImportPasswordIncorrect,
            ErrorCode::ImportColumnMappingInvalid,
            ErrorCode::ImportPartialFailure,
            ErrorCode::ExportWriteFailed,
            ErrorCode::ExportPathInvalid,
            ErrorCode::ExportSessionCreateFailed,
            ErrorCode::ExecutorVaultLocked,
            ErrorCode::ExecutorMasterPasswordRequired,
            ErrorCode::DekRotationFailed,
        ];

        for code in &all_variants {
            let _ = code.level();
            let _ = code.message_key();
            let _ = code.module_prefix();
        }
    }
}
