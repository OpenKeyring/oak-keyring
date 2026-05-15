use crate::errors::ErrorLevel;

/// Specific error codes for all error conditions in the application.
///
/// Each variant represents a distinct error condition with no dynamic payload.
/// Error messages are retrieved via i18n keys using `message_key()`.
///
/// # Error Categories
///
/// - **VAULT (S1)**: Vault service errors (8 variants)
/// - **DATA (D1)**: Data validation errors (6 variants)
/// - **CRYPTO (D2)**: Cryptographic operation errors (5 variants)
/// - **SYNC (S2)**: Synchronization errors (9 variants)
/// - **HEALTH (S3)**: Password health check errors (3 variants)
/// - **CLIPBOARD (S4)**: Clipboard operation errors (3 variants)
/// - **IMPORT/EXPORT**: Import/export errors (8 variants)
/// - **EXECUTOR (S5)**: Command executor errors (2 variants)
/// - **ROTATION (S6)**: Key rotation errors (2 variants)
/// - **CONFIG (D3)**: Configuration errors (3 variants)
///
/// # Total
///
/// 49 specific error variants (no catch-all variants with String payloads).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    // === VAULT (S1) ===
    /// Requested vault record does not exist in the database.
    VaultRecordNotFound,

    /// Concurrent modification detected: record version mismatch.
    VaultVersionConflict,

    /// Attempted to create a tag that already exists.
    VaultTagAlreadyExists,

    /// Requested tag does not exist in the vault.
    VaultTagNotFound,

    /// Operation requires vault to be unlocked (master password not provided).
    VaultNotUnlocked,

    /// Vault database file is corrupted and cannot be read.
    VaultDatabaseCorrupted,

    /// I/O error accessing vault database file.
    VaultDatabaseIoError,

    /// Invalid or unsupported field name in vault operation.
    VaultInvalidField,

    // === DATA (D1) ===
    /// Invalid credential type (e.g., not one of the supported types).
    DataInvalidCredentialType,

    /// Invalid audit operation type.
    DataInvalidAuditOperation,

    /// Invalid UUID format in input data.
    DataInvalidUuid,

    /// Required field is missing from input data.
    DataMissingField,

    /// Field value exceeds maximum allowed length.
    DataFieldTooLong,

    /// Field value cannot be empty.
    DataEmptyField,

    // === CRYPTO (D2) ===
    /// Decryption operation failed (wrong password or corrupted data).
    CryptoDecryptionFailed,

    /// Encryption operation failed (system error).
    CryptoEncryptionFailed,

    /// Key derivation failed (e.g., Argon2 error).
    CryptoKeyDerivationFailed,

    /// Invalid nonce for cryptographic operation.
    CryptoInvalidNonce,

    /// Additional authenticated data (AAD) mismatch during decryption.
    CryptoAadMismatch,

    // === SYNC (S2) ===
    /// Network connection timeout during sync operation.
    SyncConnectionTimeout,

    /// Authentication failed with cloud storage provider.
    SyncAuthenticationFailed,

    /// Cloud provider returned an error response.
    SyncProviderError,

    /// Conflict detected between local and remote vault versions.
    SyncConflictDetected,

    /// Network is unreachable (offline).
    SyncNetworkUnreachable,

    /// Cloud storage is full.
    SyncDiskFull,

    /// Sync metadata file is corrupted.
    SyncMetadataCorrupted,

    /// Vault identity mismatch (different vault on remote).
    SyncVaultIdentityMismatch,

    /// Sync is not configured (no cloud storage provider set).
    SyncNotConfigured,

    // === HEALTH (S3) ===
    /// Password health check operation failed.
    HealthCheckFailed,

    /// Have I Been Pwned (HIBP) API error.
    HealthHibpApiError,

    /// Have I Been Pwned (HIBP) API rate limit exceeded.
    HealthHibpRateLimited,

    // === CLIPBOARD (S4) ===
    /// System clipboard is not available.
    ClipboardUnavailable,

    /// Failed to copy data to clipboard.
    ClipboardCopyFailed,

    /// Failed to clear clipboard.
    ClipboardClearFailed,

    // === IMPORT/EXPORT ===
    /// Import file cannot be read (permissions, missing file).
    ImportFileUnreadable,

    /// Import file format is invalid or corrupted.
    ImportFileFormatInvalid,

    /// Import operation requires a password (e.g., encrypted backup).
    ImportPasswordRequired,

    /// Incorrect password for encrypted import file.
    ImportPasswordIncorrect,

    /// Invalid column mapping in CSV import.
    ImportColumnMappingInvalid,

    /// Import operation partially failed (some records imported).
    ImportPartialFailure,

    /// Failed to write export file.
    ExportWriteFailed,

    /// Invalid export path (e.g., directory doesn't exist).
    ExportPathInvalid,

    // === EXECUTOR (S5) ===
    /// Operation requires vault to be unlocked.
    ExecutorVaultLocked,

    /// Master password is required for this operation.
    ExecutorMasterPasswordRequired,

    // === ROTATION (S6) ===
    /// Data encryption key (DEK) rotation failed.
    DekRotationFailed,

    /// Conflict detected during key rotation operation.
    RotationConflictDetected,

    // === CONFIG (D3) ===
    /// Failed to load configuration file.
    ConfigLoadFailed,

    /// Failed to save configuration file.
    ConfigSaveFailed,

    /// Configuration validation failed.
    ConfigValidationFailed,
}

impl ErrorCode {
    /// Returns the error level for this error code.
    ///
    /// # Level Mapping
    ///
    /// - **Fatal**: System-level failures (corruption, crypto failures, identity mismatch)
    /// - **Operation**: User-actionable errors (validation, conflicts, missing credentials)
    /// - **Minor**: Temporary issues (network timeouts, rate limits, transient failures)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oak_keyring::errors::{ErrorCode, ErrorLevel};
    ///
    /// assert_eq!(ErrorCode::VaultDatabaseCorrupted.level(), ErrorLevel::Fatal);
    /// assert_eq!(ErrorCode::VaultRecordNotFound.level(), ErrorLevel::Operation);
    /// assert_eq!(ErrorCode::SyncConnectionTimeout.level(), ErrorLevel::Minor);
    /// ```
    pub fn level(&self) -> ErrorLevel {
        match self {
            // === Fatal Errors ===
            ErrorCode::VaultDatabaseCorrupted
            | ErrorCode::VaultDatabaseIoError
            | ErrorCode::CryptoEncryptionFailed
            | ErrorCode::CryptoAadMismatch
            | ErrorCode::SyncVaultIdentityMismatch => ErrorLevel::Fatal,

            // === Operation Errors ===
            ErrorCode::VaultRecordNotFound
            | ErrorCode::VaultVersionConflict
            | ErrorCode::VaultNotUnlocked
            | ErrorCode::VaultInvalidField
            | ErrorCode::CryptoDecryptionFailed
            | ErrorCode::CryptoKeyDerivationFailed
            | ErrorCode::CryptoInvalidNonce
            | ErrorCode::SyncConflictDetected
            | ErrorCode::SyncMetadataCorrupted
            | ErrorCode::SyncNotConfigured
            | ErrorCode::ImportFileFormatInvalid
            | ErrorCode::ImportPasswordRequired
            | ErrorCode::ImportPasswordIncorrect
            | ErrorCode::ImportColumnMappingInvalid
            | ErrorCode::ImportPartialFailure
            | ErrorCode::ExportWriteFailed
            | ErrorCode::ExportPathInvalid
            | ErrorCode::ExecutorVaultLocked
            | ErrorCode::ExecutorMasterPasswordRequired
            | ErrorCode::DekRotationFailed
            | ErrorCode::RotationConflictDetected => ErrorLevel::Operation,

            // === Minor Errors ===
            ErrorCode::VaultTagAlreadyExists
            | ErrorCode::VaultTagNotFound
            | ErrorCode::DataInvalidCredentialType
            | ErrorCode::DataInvalidAuditOperation
            | ErrorCode::DataInvalidUuid
            | ErrorCode::DataMissingField
            | ErrorCode::DataFieldTooLong
            | ErrorCode::DataEmptyField
            | ErrorCode::SyncConnectionTimeout
            | ErrorCode::SyncAuthenticationFailed
            | ErrorCode::SyncProviderError
            | ErrorCode::SyncNetworkUnreachable
            | ErrorCode::SyncDiskFull
            | ErrorCode::HealthCheckFailed
            | ErrorCode::HealthHibpApiError
            | ErrorCode::HealthHibpRateLimited
            | ErrorCode::ClipboardUnavailable
            | ErrorCode::ClipboardCopyFailed
            | ErrorCode::ClipboardClearFailed
            | ErrorCode::ImportFileUnreadable
            | ErrorCode::ConfigLoadFailed
            | ErrorCode::ConfigSaveFailed
            | ErrorCode::ConfigValidationFailed => ErrorLevel::Minor,
        }
    }

    /// Returns the i18n message key for this error code.
    ///
    /// Message keys follow the pattern: `"tui.error.{variant_name}"` in snake_case.
    /// These keys are used with the `rust-i18n` library to retrieve localized
    /// error messages.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oak_keyring::errors::ErrorCode;
    ///
    /// assert_eq!(
    ///     ErrorCode::VaultRecordNotFound.message_key(),
    ///     "tui.error.vault_record_not_found"
    /// );
    /// assert_eq!(
    ///     ErrorCode::CryptoDecryptionFailed.message_key(),
    ///     "tui.error.crypto_decryption_failed"
    /// );
    /// ```
    pub fn message_key(&self) -> &'static str {
        match self {
            // === VAULT (S1) ===
            ErrorCode::VaultRecordNotFound => "tui.error.vault_record_not_found",
            ErrorCode::VaultVersionConflict => "tui.error.vault_version_conflict",
            ErrorCode::VaultTagAlreadyExists => "tui.error.vault_tag_already_exists",
            ErrorCode::VaultTagNotFound => "tui.error.vault_tag_not_found",
            ErrorCode::VaultNotUnlocked => "tui.error.vault_not_unlocked",
            ErrorCode::VaultDatabaseCorrupted => "tui.error.vault_database_corrupted",
            ErrorCode::VaultDatabaseIoError => "tui.error.vault_database_io_error",
            ErrorCode::VaultInvalidField => "tui.error.vault_invalid_field",

            // === DATA (D1) ===
            ErrorCode::DataInvalidCredentialType => "tui.error.data_invalid_credential_type",
            ErrorCode::DataInvalidAuditOperation => "tui.error.data_invalid_audit_operation",
            ErrorCode::DataInvalidUuid => "tui.error.data_invalid_uuid",
            ErrorCode::DataMissingField => "tui.error.data_missing_field",
            ErrorCode::DataFieldTooLong => "tui.error.data_field_too_long",
            ErrorCode::DataEmptyField => "tui.error.data_empty_field",

            // === CRYPTO (D2) ===
            ErrorCode::CryptoDecryptionFailed => "tui.error.crypto_decryption_failed",
            ErrorCode::CryptoEncryptionFailed => "tui.error.crypto_encryption_failed",
            ErrorCode::CryptoKeyDerivationFailed => "tui.error.crypto_key_derivation_failed",
            ErrorCode::CryptoInvalidNonce => "tui.error.crypto_invalid_nonce",
            ErrorCode::CryptoAadMismatch => "tui.error.crypto_aad_mismatch",

            // === SYNC (S2) ===
            ErrorCode::SyncConnectionTimeout => "tui.error.sync_connection_timeout",
            ErrorCode::SyncAuthenticationFailed => "tui.error.sync_authentication_failed",
            ErrorCode::SyncProviderError => "tui.error.sync_provider_error",
            ErrorCode::SyncConflictDetected => "tui.error.sync_conflict_detected",
            ErrorCode::SyncNetworkUnreachable => "tui.error.sync_network_unreachable",
            ErrorCode::SyncDiskFull => "tui.error.sync_disk_full",
            ErrorCode::SyncMetadataCorrupted => "tui.error.sync_metadata_corrupted",
            ErrorCode::SyncVaultIdentityMismatch => "tui.error.sync_vault_identity_mismatch",
            ErrorCode::SyncNotConfigured => "tui.error.sync_not_configured",

            // === HEALTH (S3) ===
            ErrorCode::HealthCheckFailed => "tui.error.health_check_failed",
            ErrorCode::HealthHibpApiError => "tui.error.health_hibp_api_error",
            ErrorCode::HealthHibpRateLimited => "tui.error.health_hibp_rate_limited",

            // === CLIPBOARD (S4) ===
            ErrorCode::ClipboardUnavailable => "tui.error.clipboard_unavailable",
            ErrorCode::ClipboardCopyFailed => "tui.error.clipboard_copy_failed",
            ErrorCode::ClipboardClearFailed => "tui.error.clipboard_clear_failed",

            // === IMPORT/EXPORT ===
            ErrorCode::ImportFileUnreadable => "tui.error.import_file_unreadable",
            ErrorCode::ImportFileFormatInvalid => "tui.error.import_file_format_invalid",
            ErrorCode::ImportPasswordRequired => "tui.error.import_password_required",
            ErrorCode::ImportPasswordIncorrect => "tui.error.import_password_incorrect",
            ErrorCode::ImportColumnMappingInvalid => "tui.error.import_column_mapping_invalid",
            ErrorCode::ImportPartialFailure => "tui.error.import_partial_failure",
            ErrorCode::ExportWriteFailed => "tui.error.export_write_failed",
            ErrorCode::ExportPathInvalid => "tui.error.export_path_invalid",

            // === EXECUTOR (S5) ===
            ErrorCode::ExecutorVaultLocked => "tui.error.executor_vault_locked",
            ErrorCode::ExecutorMasterPasswordRequired => {
                "tui.error.executor_master_password_required"
            }

            // === ROTATION (S6) ===
            ErrorCode::DekRotationFailed => "tui.error.dek_rotation_failed",
            ErrorCode::RotationConflictDetected => "tui.error.rotation_conflict_detected",

            // === CONFIG (D3) ===
            ErrorCode::ConfigLoadFailed => "tui.error.config_load_failed",
            ErrorCode::ConfigSaveFailed => "tui.error.config_save_failed",
            ErrorCode::ConfigValidationFailed => "tui.error.config_validation_failed",
        }
    }

    /// Returns the module prefix for this error code.
    ///
    /// The module prefix indicates which part of the system generated the error.
    /// This is useful for logging and categorization.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use oak_keyring::errors::ErrorCode;
    ///
    /// assert_eq!(ErrorCode::VaultRecordNotFound.module_prefix(), "vault");
    /// assert_eq!(ErrorCode::SyncConflictDetected.module_prefix(), "sync");
    /// assert_eq!(ErrorCode::CryptoDecryptionFailed.module_prefix(), "crypto");
    /// ```
    pub fn module_prefix(&self) -> &'static str {
        match self {
            // === VAULT (S1) ===
            ErrorCode::VaultRecordNotFound
            | ErrorCode::VaultVersionConflict
            | ErrorCode::VaultTagAlreadyExists
            | ErrorCode::VaultTagNotFound
            | ErrorCode::VaultNotUnlocked
            | ErrorCode::VaultDatabaseCorrupted
            | ErrorCode::VaultDatabaseIoError
            | ErrorCode::VaultInvalidField => "vault",

            // === DATA (D1) ===
            ErrorCode::DataInvalidCredentialType
            | ErrorCode::DataInvalidAuditOperation
            | ErrorCode::DataInvalidUuid
            | ErrorCode::DataMissingField
            | ErrorCode::DataFieldTooLong
            | ErrorCode::DataEmptyField => "data",

            // === CRYPTO (D2) ===
            ErrorCode::CryptoDecryptionFailed
            | ErrorCode::CryptoEncryptionFailed
            | ErrorCode::CryptoKeyDerivationFailed
            | ErrorCode::CryptoInvalidNonce
            | ErrorCode::CryptoAadMismatch => "crypto",

            // === SYNC (S2) ===
            ErrorCode::SyncConnectionTimeout
            | ErrorCode::SyncAuthenticationFailed
            | ErrorCode::SyncProviderError
            | ErrorCode::SyncConflictDetected
            | ErrorCode::SyncNetworkUnreachable
            | ErrorCode::SyncDiskFull
            | ErrorCode::SyncMetadataCorrupted
            | ErrorCode::SyncVaultIdentityMismatch
            | ErrorCode::SyncNotConfigured => "sync",

            // === HEALTH (S3) ===
            ErrorCode::HealthCheckFailed
            | ErrorCode::HealthHibpApiError
            | ErrorCode::HealthHibpRateLimited => "health",

            // === CLIPBOARD (S4) ===
            ErrorCode::ClipboardUnavailable
            | ErrorCode::ClipboardCopyFailed
            | ErrorCode::ClipboardClearFailed => "clipboard",

            // === IMPORT/EXPORT ===
            ErrorCode::ImportFileUnreadable
            | ErrorCode::ImportFileFormatInvalid
            | ErrorCode::ImportPasswordRequired
            | ErrorCode::ImportPasswordIncorrect
            | ErrorCode::ImportColumnMappingInvalid
            | ErrorCode::ImportPartialFailure
            | ErrorCode::ExportWriteFailed
            | ErrorCode::ExportPathInvalid => "import_export",

            // === EXECUTOR (S5) ===
            ErrorCode::ExecutorVaultLocked | ErrorCode::ExecutorMasterPasswordRequired => {
                "executor"
            }

            // === ROTATION (S6) ===
            ErrorCode::DekRotationFailed | ErrorCode::RotationConflictDetected => "rotation",

            // === CONFIG (D3) ===
            ErrorCode::ConfigLoadFailed
            | ErrorCode::ConfigSaveFailed
            | ErrorCode::ConfigValidationFailed => "config",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = self.message_key();
        let suffix = key.strip_prefix("tui.error.").unwrap_or(key);
        write!(f, "{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Level Mapping Tests ===

    #[test]
    fn fatal_error_codes_have_correct_level() {
        let fatal_codes = [
            ErrorCode::VaultDatabaseCorrupted,
            ErrorCode::VaultDatabaseIoError,
            ErrorCode::CryptoEncryptionFailed,
            ErrorCode::CryptoAadMismatch,
            ErrorCode::SyncVaultIdentityMismatch,
        ];

        for code in fatal_codes {
            assert_eq!(
                code.level(),
                ErrorLevel::Fatal,
                "{:?} should be Fatal",
                code
            );
        }
    }

    #[test]
    fn operation_error_codes_have_correct_level() {
        let operation_codes = [
            ErrorCode::VaultRecordNotFound,
            ErrorCode::VaultVersionConflict,
            ErrorCode::VaultNotUnlocked,
            ErrorCode::VaultInvalidField,
            ErrorCode::CryptoDecryptionFailed,
            ErrorCode::CryptoKeyDerivationFailed,
            ErrorCode::CryptoInvalidNonce,
            ErrorCode::SyncConflictDetected,
            ErrorCode::SyncMetadataCorrupted,
            ErrorCode::SyncNotConfigured,
            ErrorCode::ImportFileFormatInvalid,
            ErrorCode::ImportPasswordRequired,
            ErrorCode::ImportPasswordIncorrect,
            ErrorCode::ImportColumnMappingInvalid,
            ErrorCode::ImportPartialFailure,
            ErrorCode::ExportWriteFailed,
            ErrorCode::ExportPathInvalid,
            ErrorCode::ExecutorVaultLocked,
            ErrorCode::ExecutorMasterPasswordRequired,
            ErrorCode::DekRotationFailed,
            ErrorCode::RotationConflictDetected,
        ];

        for code in operation_codes {
            assert_eq!(
                code.level(),
                ErrorLevel::Operation,
                "{:?} should be Operation",
                code
            );
        }
    }

    #[test]
    fn minor_error_codes_have_correct_level() {
        let minor_codes = [
            ErrorCode::VaultTagAlreadyExists,
            ErrorCode::VaultTagNotFound,
            ErrorCode::DataInvalidCredentialType,
            ErrorCode::DataInvalidAuditOperation,
            ErrorCode::DataInvalidUuid,
            ErrorCode::DataMissingField,
            ErrorCode::DataFieldTooLong,
            ErrorCode::DataEmptyField,
            ErrorCode::SyncConnectionTimeout,
            ErrorCode::SyncAuthenticationFailed,
            ErrorCode::SyncProviderError,
            ErrorCode::SyncNetworkUnreachable,
            ErrorCode::SyncDiskFull,
            ErrorCode::HealthCheckFailed,
            ErrorCode::HealthHibpApiError,
            ErrorCode::HealthHibpRateLimited,
            ErrorCode::ClipboardUnavailable,
            ErrorCode::ClipboardCopyFailed,
            ErrorCode::ClipboardClearFailed,
            ErrorCode::ImportFileUnreadable,
            ErrorCode::ConfigLoadFailed,
            ErrorCode::ConfigSaveFailed,
            ErrorCode::ConfigValidationFailed,
        ];

        for code in minor_codes {
            assert_eq!(
                code.level(),
                ErrorLevel::Minor,
                "{:?} should be Minor",
                code
            );
        }
    }

    // === Message Key Tests ===

    #[test]
    fn message_keys_follow_pattern() {
        let test_cases = [
            (
                ErrorCode::VaultRecordNotFound,
                "tui.error.vault_record_not_found",
            ),
            (
                ErrorCode::CryptoDecryptionFailed,
                "tui.error.crypto_decryption_failed",
            ),
            (
                ErrorCode::SyncConflictDetected,
                "tui.error.sync_conflict_detected",
            ),
            (
                ErrorCode::HealthHibpApiError,
                "tui.error.health_hibp_api_error",
            ),
            (
                ErrorCode::ImportPasswordRequired,
                "tui.error.import_password_required",
            ),
            (
                ErrorCode::ExecutorVaultLocked,
                "tui.error.executor_vault_locked",
            ),
            (
                ErrorCode::DekRotationFailed,
                "tui.error.dek_rotation_failed",
            ),
            (ErrorCode::ConfigLoadFailed, "tui.error.config_load_failed"),
        ];

        for (code, expected_key) in test_cases {
            assert_eq!(code.message_key(), expected_key);
        }
    }

    #[test]
    fn all_message_keys_start_with_tui_error() {
        let all_codes = [
            // VAULT
            ErrorCode::VaultRecordNotFound,
            ErrorCode::VaultVersionConflict,
            ErrorCode::VaultTagAlreadyExists,
            ErrorCode::VaultTagNotFound,
            ErrorCode::VaultNotUnlocked,
            ErrorCode::VaultDatabaseCorrupted,
            ErrorCode::VaultDatabaseIoError,
            ErrorCode::VaultInvalidField,
            // DATA
            ErrorCode::DataInvalidCredentialType,
            ErrorCode::DataInvalidAuditOperation,
            ErrorCode::DataInvalidUuid,
            ErrorCode::DataMissingField,
            ErrorCode::DataFieldTooLong,
            ErrorCode::DataEmptyField,
            // CRYPTO
            ErrorCode::CryptoDecryptionFailed,
            ErrorCode::CryptoEncryptionFailed,
            ErrorCode::CryptoKeyDerivationFailed,
            ErrorCode::CryptoInvalidNonce,
            ErrorCode::CryptoAadMismatch,
            // SYNC
            ErrorCode::SyncConnectionTimeout,
            ErrorCode::SyncAuthenticationFailed,
            ErrorCode::SyncProviderError,
            ErrorCode::SyncConflictDetected,
            ErrorCode::SyncNetworkUnreachable,
            ErrorCode::SyncDiskFull,
            ErrorCode::SyncMetadataCorrupted,
            ErrorCode::SyncVaultIdentityMismatch,
            ErrorCode::SyncNotConfigured,
            // HEALTH
            ErrorCode::HealthCheckFailed,
            ErrorCode::HealthHibpApiError,
            ErrorCode::HealthHibpRateLimited,
            // CLIPBOARD
            ErrorCode::ClipboardUnavailable,
            ErrorCode::ClipboardCopyFailed,
            ErrorCode::ClipboardClearFailed,
            // IMPORT/EXPORT
            ErrorCode::ImportFileUnreadable,
            ErrorCode::ImportFileFormatInvalid,
            ErrorCode::ImportPasswordRequired,
            ErrorCode::ImportPasswordIncorrect,
            ErrorCode::ImportColumnMappingInvalid,
            ErrorCode::ImportPartialFailure,
            ErrorCode::ExportWriteFailed,
            ErrorCode::ExportPathInvalid,
            // EXECUTOR
            ErrorCode::ExecutorVaultLocked,
            ErrorCode::ExecutorMasterPasswordRequired,
            // ROTATION
            ErrorCode::DekRotationFailed,
            ErrorCode::RotationConflictDetected,
            // CONFIG
            ErrorCode::ConfigLoadFailed,
            ErrorCode::ConfigSaveFailed,
            ErrorCode::ConfigValidationFailed,
        ];

        for code in all_codes {
            assert!(
                code.message_key().starts_with("tui.error."),
                "{:?} message key should start with 'tui.error.'",
                code
            );
        }
    }

    // === Module Prefix Tests ===

    #[test]
    fn module_prefixes_are_correct() {
        let test_cases = [
            (ErrorCode::VaultRecordNotFound, "vault"),
            (ErrorCode::DataInvalidUuid, "data"),
            (ErrorCode::CryptoDecryptionFailed, "crypto"),
            (ErrorCode::SyncConflictDetected, "sync"),
            (ErrorCode::HealthCheckFailed, "health"),
            (ErrorCode::ClipboardUnavailable, "clipboard"),
            (ErrorCode::ImportFileUnreadable, "import_export"),
            (ErrorCode::ExportWriteFailed, "import_export"),
            (ErrorCode::ExecutorVaultLocked, "executor"),
            (ErrorCode::DekRotationFailed, "rotation"),
            (ErrorCode::ConfigLoadFailed, "config"),
        ];

        for (code, expected_prefix) in test_cases {
            assert_eq!(code.module_prefix(), expected_prefix);
        }
    }

    // === Code Properties Tests ===

    #[test]
    fn error_code_is_copy() {
        let code = ErrorCode::VaultRecordNotFound;
        let copied = code;
        assert_eq!(code, copied);
    }

    #[test]
    fn error_code_is_clone() {
        let code = ErrorCode::CryptoDecryptionFailed;
        let cloned = code;
        assert_eq!(code, cloned);
    }

    #[test]
    fn error_code_supports_equality() {
        assert_eq!(
            ErrorCode::VaultRecordNotFound,
            ErrorCode::VaultRecordNotFound
        );
        assert_ne!(
            ErrorCode::VaultRecordNotFound,
            ErrorCode::CryptoDecryptionFailed
        );
    }

    #[test]
    fn error_code_supports_debug() {
        let code = ErrorCode::SyncConflictDetected;
        let debug_str = format!("{:?}", code);
        assert!(debug_str.contains("SyncConflictDetected"));
    }

    #[test]
    fn total_variant_count() {
        // Verify we have exactly 49 variants
        let all_codes = [
            // VAULT (8)
            ErrorCode::VaultRecordNotFound,
            ErrorCode::VaultVersionConflict,
            ErrorCode::VaultTagAlreadyExists,
            ErrorCode::VaultTagNotFound,
            ErrorCode::VaultNotUnlocked,
            ErrorCode::VaultDatabaseCorrupted,
            ErrorCode::VaultDatabaseIoError,
            ErrorCode::VaultInvalidField,
            // DATA (6)
            ErrorCode::DataInvalidCredentialType,
            ErrorCode::DataInvalidAuditOperation,
            ErrorCode::DataInvalidUuid,
            ErrorCode::DataMissingField,
            ErrorCode::DataFieldTooLong,
            ErrorCode::DataEmptyField,
            // CRYPTO (5)
            ErrorCode::CryptoDecryptionFailed,
            ErrorCode::CryptoEncryptionFailed,
            ErrorCode::CryptoKeyDerivationFailed,
            ErrorCode::CryptoInvalidNonce,
            ErrorCode::CryptoAadMismatch,
            // SYNC (9)
            ErrorCode::SyncConnectionTimeout,
            ErrorCode::SyncAuthenticationFailed,
            ErrorCode::SyncProviderError,
            ErrorCode::SyncConflictDetected,
            ErrorCode::SyncNetworkUnreachable,
            ErrorCode::SyncDiskFull,
            ErrorCode::SyncMetadataCorrupted,
            ErrorCode::SyncVaultIdentityMismatch,
            ErrorCode::SyncNotConfigured,
            // HEALTH (3)
            ErrorCode::HealthCheckFailed,
            ErrorCode::HealthHibpApiError,
            ErrorCode::HealthHibpRateLimited,
            // CLIPBOARD (3)
            ErrorCode::ClipboardUnavailable,
            ErrorCode::ClipboardCopyFailed,
            ErrorCode::ClipboardClearFailed,
            // IMPORT/EXPORT (8)
            ErrorCode::ImportFileUnreadable,
            ErrorCode::ImportFileFormatInvalid,
            ErrorCode::ImportPasswordRequired,
            ErrorCode::ImportPasswordIncorrect,
            ErrorCode::ImportColumnMappingInvalid,
            ErrorCode::ImportPartialFailure,
            ErrorCode::ExportWriteFailed,
            ErrorCode::ExportPathInvalid,
            // EXECUTOR (2)
            ErrorCode::ExecutorVaultLocked,
            ErrorCode::ExecutorMasterPasswordRequired,
            // ROTATION (2)
            ErrorCode::DekRotationFailed,
            ErrorCode::RotationConflictDetected,
            // CONFIG (3)
            ErrorCode::ConfigLoadFailed,
            ErrorCode::ConfigSaveFailed,
            ErrorCode::ConfigValidationFailed,
        ];

        assert_eq!(all_codes.len(), 49);
    }

    #[test]
    fn display_uses_message_key_suffix() {
        assert_eq!(
            ErrorCode::VaultRecordNotFound.to_string(),
            "vault_record_not_found"
        );
        assert_eq!(
            ErrorCode::SyncConflictDetected.to_string(),
            "sync_conflict_detected"
        );
        assert_eq!(
            ErrorCode::ExecutorVaultLocked.to_string(),
            "executor_vault_locked"
        );
    }
}
