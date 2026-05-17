use crate::commands::{CommandResult, InternalCommand};
use crate::config::ConfigManager;
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::errors::service_error::ServiceError;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::{RecoveryWords, SecureStr};

use super::CommandExecutor;

fn artifact_existed_before(path: &std::path::Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn remove_initialized_artifact(path: &std::path::Path, label: &str) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(
                path = %path.display(),
                artifact = label,
                error = %e,
                "failed to clean up partial vault initialization artifact"
            );
        }
    }
}

fn cleanup_failed_new_vault_initialization(
    vault_dir: &std::path::Path,
    key_existed_before: bool,
    vault_db_existed_before: bool,
    wal_existed_before: bool,
    shm_existed_before: bool,
) {
    if !key_existed_before {
        remove_initialized_artifact(&vault_dir.join("wrapped_secret_key.json"), "key_file");
    }

    if !vault_db_existed_before {
        for (path, existed_before, label) in [
            (vault_dir.join("vault.db"), false, "database"),
            (
                vault_dir.join("vault.db-wal"),
                wal_existed_before,
                "database_wal",
            ),
            (
                vault_dir.join("vault.db-shm"),
                shm_existed_before,
                "database_shm",
            ),
        ] {
            if !existed_before {
                remove_initialized_artifact(&path, label);
            }
        }
    }
}

/// Derive a database page key from the keystore for a pending vault initialization.
///
/// Unlocks the existing keystore (created by [`KeyStore::initialize`]) and
/// derives the [`DbPageKey`] needed to create a SQLCipher-encrypted database.
#[cfg(feature = "sqlcipher")]
pub(super) fn derive_db_page_key_for_pending(
    vault_dir: &std::path::Path,
    master_password: &SecureStr,
) -> Result<crate::crypto::db_page_key::DbPageKey, String> {
    crate::crypto::keystore::KeyStore::unlock(vault_dir, master_password)?.db_page_key()
}

/// Schedule health check after a successful unlock.
///
/// Reads the persisted `last_health_check_at` from metadata, evaluates
/// `should_run`, and either:
/// - Sends `RunHealthCheck` via the internal channel (non-blocking), or
/// - Loads the cached health report into `executor.health_report`.
///
/// Errors during scheduling are logged and silently ignored — unlock must
/// never fail due to health check scheduling issues.
pub(super) fn schedule_health_check_after_unlock(executor: &mut CommandExecutor) {
    let vault = match executor.vault() {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "vault runtime not open, skipping health check scheduling");
            return;
        }
    };
    let last_check = match vault.get_last_health_check_at() {
        Ok(ts) => ts,
        Err(e) => {
            tracing::warn!(error = %e, "failed to read last_health_check_at, skipping scheduling");
            return;
        }
    };

    let config = executor.config.get_config();
    let should_run = crate::services::health::should_run(&config.security, last_check);

    if should_run {
        // Non-blocking: send via internal channel. The executor loop will
        // pick it up after returning VaultUnlocked to the UI.
        let _ = executor
            .internal_tx
            .try_send(InternalCommand::ScheduleHealthCheck { force: false });
        tracing::info!("health check scheduled after unlock");
    } else {
        // Restore cached report from persisted health state rows.
        match super::health::load_cached_health_report(executor) {
            Ok(Some(report)) => {
                executor.health_report = Some(report);
                executor.last_health_check_time = last_check;
                tracing::debug!("restored cached health report after unlock");
            }
            Ok(None) => {
                tracing::debug!("no cached health report to restore");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to load cached health report");
            }
        }
    }
}

#[cfg(feature = "sqlcipher")]
fn vault_db_error_result(err: crate::db::vault_db::VaultDbError) -> CommandResult {
    use crate::db::vault_db::VaultDbError;
    use crate::errors::{ErrorCode, ErrorContext};

    match err {
        VaultDbError::PlaintextDatabaseUnsupported => CommandResult::Error {
            code: ErrorCode::VaultDatabaseIoError,
            context: ErrorContext::default(),
            message_key: "error.plaintext_database_unsupported",
            fallback: "Plaintext SQLite vault databases are not supported. This vault cannot be opened with SQLCipher.".to_string(),
        },
        VaultDbError::WrongDbPageKey => CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.wrong_db_key",
            fallback: "The database page key does not match. The vault database may be from a different master password or may be corrupt.".to_string(),
        },
        VaultDbError::CorruptDatabase(msg) => CommandResult::Error {
            code: ErrorCode::VaultDatabaseIoError,
            context: ErrorContext::default(),
            message_key: "error.corrupt_database",
            fallback: format!("The vault database is corrupt: {}", msg),
        },
        VaultDbError::UnsupportedSchemaVersion { current, supported } => CommandResult::Error {
            code: ErrorCode::VaultDatabaseIoError,
            context: ErrorContext::default(),
            message_key: "error.unsupported_schema",
            fallback: format!("Database schema version {} is newer than this build supports ({})", current, supported),
        },
        VaultDbError::DbOpenIo(msg) => CommandResult::Error {
            code: ErrorCode::VaultDatabaseIoError,
            context: ErrorContext::default(),
            message_key: "error.db_open_io",
            fallback: format!("Failed to open vault database: {}", msg),
        },
        VaultDbError::DbMigrationFailed(msg) => CommandResult::Error {
            code: ErrorCode::VaultDatabaseIoError,
            context: ErrorContext::default(),
            message_key: "error.db_migration_failed",
            fallback: format!("Database migration failed: {}", msg),
        },
        VaultDbError::DbRollbackFailed(msg) => CommandResult::Error {
            code: ErrorCode::VaultDatabaseIoError,
            context: ErrorContext::default(),
            message_key: "error.db_rollback_failed",
            fallback: format!("Database rollback failed: {}", msg),
        },
    }
}

#[tracing::instrument(skip(executor, master_password))]
pub async fn handle_unlock(
    executor: &mut CommandExecutor,
    master_password: SecureStr,
) -> CommandResult {
    #[cfg(feature = "sqlcipher")]
    {
        // Key-first unlock for SQLCipher production
        let keystore = match crate::crypto::keystore::KeyStore::unlock(
            &executor.vault_dir,
            &master_password,
        ) {
            Ok(ks) => ks,
            Err(_) => {
                return CommandResult::VaultUnlockFailed {
                    attempts_remaining: None,
                };
            }
        };

        let db_page_key = match keystore.db_page_key() {
            Ok(key) => key,
            Err(e) => {
                return CommandResult::Error {
                    code: crate::errors::ErrorCode::CryptoKeyDerivationFailed,
                    context: crate::errors::ErrorContext::default(),
                    message_key: "error.db_key_derivation_failed",
                    fallback: format!("Failed to derive database page key: {}", e),
                };
            }
        };

        let conn = match crate::db::vault_db::VaultDbFactory::open_sqlcipher_vault(
            &executor.vault_dir,
            &db_page_key,
        ) {
            Ok(conn) => conn,
            Err(e) => return vault_db_error_result(e),
        };

        let crypto = crate::crypto::CryptoManager::from_unlocked_keystore(keystore);
        executor.vault_runtime = crate::executor::runtime::VaultRuntime::open(Box::new(
            crate::services::vault::VaultServiceImpl::new_unlocked(conn, crypto),
        ));
        executor.vault_db_file_backed = true;
        schedule_health_check_after_unlock(executor);
        return CommandResult::VaultUnlocked;
    }
    #[cfg(not(feature = "sqlcipher"))]
    {
        let vault_dir = executor.vault_dir.clone();
        let unlock_result = {
            let vault = match executor.vault_mut() {
                Ok(v) => v,
                Err(e) => {
                    return CommandResult::Error {
                        code: e.to_error_code(),
                        context: e.to_error_context(),
                        message_key: "error.vault_not_available",
                        fallback: format!("Vault is not available for unlock: {}", e),
                    };
                }
            };
            vault.unlock(&vault_dir, &master_password)
        };
        match unlock_result {
            Ok(()) => {
                schedule_health_check_after_unlock(executor);
                CommandResult::VaultUnlocked
            }
            Err(_) => CommandResult::VaultUnlockFailed {
                attempts_remaining: None,
            },
        }
    }
}

#[tracing::instrument(skip(executor, words))]
pub async fn handle_unlock_with_recovery_key(
    executor: &mut CommandExecutor,
    words: RecoveryWords,
) -> CommandResult {
    let passkey = match reconstruct_passkey(&words) {
        Ok(pk) => pk,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::CryptoKeyDerivationFailed,
                context: ErrorContext::default(),
                message_key: "error.invalid_recovery_key",
                fallback: format!("Invalid recovery key: {}", e),
            };
        }
    };

    let unlock_result = {
        let vault = match executor.vault_mut() {
            Ok(v) => v,
            Err(e) => {
                return CommandResult::Error {
                    code: e.to_error_code(),
                    context: e.to_error_context(),
                    message_key: "error.vault_not_available",
                    fallback: format!("Vault is not available for unlock: {}", e),
                };
            }
        };
        vault.unlock_with_mnemonic(&passkey)
    };
    match unlock_result {
        Ok(()) => {
            // Post-recovery: resume interrupted rotation or trigger new one.
            // Prefer resume_rotation if a checkpoint exists (crash recovery),
            // otherwise trigger a fresh rotation (security best practice).
            tracing::info!("Post-recovery: checking for interrupted rotation");
            let rotation_result = super::rotation::handle_resume_rotation(executor).await;
            if let CommandResult::RotationTriggerChecked { .. } = &rotation_result {
                // No pending checkpoint — trigger fresh rotation
                tracing::info!("No pending checkpoint, triggering fresh DEK rotation");
                let fresh_result = super::rotation::handle_trigger_rotation(executor).await;
                if let CommandResult::Error { .. } = &fresh_result {
                    tracing::warn!("Post-recovery DEK rotation failed, vault is still usable");
                }
            } else if let CommandResult::Error { .. } = &rotation_result {
                tracing::warn!("Post-recovery rotation resume failed, vault is still usable");
            }

            // Schedule health check after successful recovery unlock
            schedule_health_check_after_unlock(executor);

            CommandResult::RecoveryKeyUnlocked
        }
        Err(_) => CommandResult::VaultUnlockFailed {
            attempts_remaining: None,
        },
    }
}

#[tracing::instrument(skip(executor))]
pub fn handle_lock(executor: &mut CommandExecutor) -> CommandResult {
    if let Ok(vault) = executor.vault_mut() {
        vault.lock();
    }
    executor.verified_master_password = None;
    #[cfg(feature = "sqlcipher")]
    {
        executor.vault_runtime = crate::executor::runtime::VaultRuntime::locked();
        executor.vault_db_file_backed = false;
    }
    CommandResult::VaultLocked
}

#[tracing::instrument(skip(executor, password))]
pub fn handle_verify_master_password(
    executor: &mut CommandExecutor,
    password: SecureStr,
) -> CommandResult {
    // Verify by attempting to unlock the keystore file with the password
    match crate::crypto::keystore::KeyStore::unlock(&executor.vault_dir, &password) {
        Ok(_) => {
            // Cache the verified password so the subsequent ChangeMasterPassword
            // command can use it without asking the user again.
            executor.verified_master_password = Some(password);
            CommandResult::MasterPasswordVerified
        }
        Err(_) => CommandResult::Error {
            code: ErrorCode::ExecutorMasterPasswordRequired,
            context: ErrorContext::default(),
            message_key: "error.password_verification_failed",
            fallback: String::from("Master password verification failed."),
        },
    }
}

#[tracing::instrument(skip(executor, current_password, new_password))]
pub fn handle_change_master_password(
    executor: &mut CommandExecutor,
    current_password: Option<SecureStr>,
    new_password: SecureStr,
) -> CommandResult {
    // Use the provided password, or fall back to the cached verified password.
    let current_password = match current_password {
        Some(pw) => pw,
        None => match executor.verified_master_password.take() {
            Some(pw) => pw,
            None => {
                return CommandResult::Error {
                    code: ErrorCode::ExecutorMasterPasswordRequired,
                    context: ErrorContext::default(),
                    message_key: "error.password_verification_failed",
                    fallback: String::from(
                        "Current master password not available. Please verify first.",
                    ),
                };
            }
        },
    };

    match crate::crypto::keystore::KeyStore::change_cmk(
        &executor.vault_dir,
        &current_password,
        &new_password,
    ) {
        Ok(()) => {
            // Re-unlock with new password if currently unlocked
            if let Ok(vault) = executor.vault() {
                if vault.is_unlocked() {
                    let vault_dir = executor.vault_dir.clone();
                    let _ = executor
                        .vault_mut()
                        .and_then(|v| v.unlock(&vault_dir, &new_password));
                }
            }
            CommandResult::MasterPasswordChanged
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.change_password_failed",
            fallback: format!("Failed to change master password: {}", e),
        },
    }
}

#[tracing::instrument(skip(executor, master_password, recovery_words))]
pub async fn handle_initialize_vault(
    executor: &mut CommandExecutor,
    master_password: SecureStr,
    recovery_words: Option<RecoveryWords>,
) -> CommandResult {
    // Step 1: Obtain Passkey — either from pre-generated recovery words or
    // by generating a fresh mnemonic.
    let config = executor.config.get_config();
    let language = MnemonicLanguage::from_config_language(&config.general.language);
    let passkey = match recovery_words {
        Some(words) => match reconstruct_passkey(&words) {
            Ok(pk) => pk,
            Err(e) => {
                return CommandResult::Error {
                    code: ErrorCode::CryptoDecryptionFailed,
                    context: ErrorContext::default(),
                    message_key: "error.passkey_reconstruction_failed",
                    fallback: format!("Failed to reconstruct recovery key: {}", e),
                };
            }
        },
        None => match Passkey::generate(24, language) {
            Ok(pk) => pk,
            Err(e) => {
                return CommandResult::Error {
                    code: ErrorCode::CryptoEncryptionFailed,
                    context: ErrorContext::default(),
                    message_key: "error.passkey_generation_failed",
                    fallback: format!("Failed to generate recovery key: {}", e),
                };
            }
        },
    };

    // Step 2: Derive secret key from mnemonic seed
    let seed = match passkey.to_seed(None) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::CryptoKeyDerivationFailed,
                context: ErrorContext::default(),
                message_key: "error.seed_derivation_failed",
                fallback: format!("Failed to derive seed: {}", e),
            };
        }
    };
    let mut sk_bytes = seed.to_secret_key();

    let vault_path = executor.vault_dir.clone();
    let key_path = vault_path.join("wrapped_secret_key.json");
    let db_path = vault_path.join("vault.db");
    let wal_path = vault_path.join("vault.db-wal");
    let shm_path = vault_path.join("vault.db-shm");
    let key_existed_before = artifact_existed_before(&key_path);
    let vault_db_existed_before = artifact_existed_before(&db_path);
    let wal_existed_before = artifact_existed_before(&wal_path);
    let shm_existed_before = artifact_existed_before(&shm_path);

    // Step 3: Initialize keystore (creates wrapped_secret_key.json)
    match crate::crypto::keystore::KeyStore::initialize(
        &vault_path,
        &mut sk_bytes,
        &master_password,
        &crate::crypto::argon2::Argon2Params::medium(),
        language,
    ) {
        Ok(_) => {}
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::CryptoEncryptionFailed,
                context: ErrorContext::default(),
                message_key: "error.keystore_init_failed",
                fallback: format!("Failed to initialize keystore: {}", e),
            };
        }
    }

    // Step 3a (SQLCipher): Derive database page key from initialized keystore.
    #[cfg(feature = "sqlcipher")]
    let db_page_key = match derive_db_page_key_for_pending(&vault_path, &master_password) {
        Ok(key) => key,
        Err(e) => {
            cleanup_failed_new_vault_initialization(
                &vault_path,
                key_existed_before,
                vault_db_existed_before,
                wal_existed_before,
                shm_existed_before,
            );
            return CommandResult::Error {
                code: ErrorCode::CryptoKeyDerivationFailed,
                context: ErrorContext::default(),
                message_key: "error.db_key_derivation_failed",
                fallback: format!(
                    "Failed to derive database page key after initialization: {}",
                    e
                ),
            };
        }
    };

    // Step 4: Commit the database only after the keystore exists and the new
    // password can unlock the file-backed vault.
    let begin_result = {
        #[cfg(feature = "sqlcipher")]
        {
            executor.begin_file_backed_vault_db(&db_page_key)
        }
        #[cfg(not(feature = "sqlcipher"))]
        {
            executor.begin_file_backed_vault_db()
        }
    };
    let mut pending = match begin_result {
        Ok(pending) => pending,
        Err(e) => {
            cleanup_failed_new_vault_initialization(
                &vault_path,
                key_existed_before,
                vault_db_existed_before,
                wal_existed_before,
                shm_existed_before,
            );
            return CommandResult::Error {
                code: ErrorCode::VaultDatabaseIoError,
                context: ErrorContext::default(),
                message_key: "error.db_reopen_failed",
                fallback: format!("Failed to create vault database: {}", e),
            };
        }
    };

    match pending.unlock(&master_password) {
        Ok(()) => {
            pending.commit();
            CommandResult::VaultInitialized
        }
        Err(e) => {
            drop(pending);
            cleanup_failed_new_vault_initialization(
                &vault_path,
                key_existed_before,
                vault_db_existed_before,
                wal_existed_before,
                shm_existed_before,
            );
            CommandResult::Error {
                code: ErrorCode::CryptoEncryptionFailed,
                context: ErrorContext::default(),
                message_key: "error.unlock_failed",
                fallback: format!("Failed to unlock vault: {}", e),
            }
        }
    }
}

/// Reconstruct a Passkey from pre-generated recovery words.
/// Tries English first, then Chinese Simplified.
fn reconstruct_passkey(words: &RecoveryWords) -> Result<Passkey, String> {
    let english = Passkey::from_recovery_words(words, MnemonicLanguage::English);
    if english.is_ok() {
        return english;
    }
    Passkey::from_recovery_words(words, MnemonicLanguage::ChineseSimplified)
}

/// Validate BIP39 recovery words (must be exactly 24).
pub async fn handle_validate_recovery_words(words: RecoveryWords) -> CommandResult {
    if words.len() != 24 {
        return CommandResult::Error {
            code: ErrorCode::CryptoKeyDerivationFailed,
            context: ErrorContext::default(),
            message_key: "error.invalid_recovery_key",
            fallback: "Recovery key must contain 24 words.".to_string(),
        };
    }
    match reconstruct_passkey(&words) {
        Ok(_) => CommandResult::RecoveryWordsValidated,
        Err(e) => CommandResult::Error {
            code: ErrorCode::CryptoKeyDerivationFailed,
            context: ErrorContext::default(),
            message_key: "error.invalid_recovery_key",
            fallback: format!("Invalid recovery key: {}", e),
        },
    }
}

/// Rebuild wrapped_secret_key.json from recovery words + new master password.
pub async fn handle_rebuild_keyfile_from_recovery(
    executor: &mut CommandExecutor,
    master_password: crate::types::SecureStr,
    recovery_words: RecoveryWords,
) -> CommandResult {
    let passkey = match reconstruct_passkey(&recovery_words) {
        Ok(pk) => pk,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::CryptoKeyDerivationFailed,
                context: ErrorContext::default(),
                message_key: "error.invalid_recovery_key",
                fallback: format!("Invalid recovery key: {}", e),
            };
        }
    };

    let seed = match passkey.to_seed(None) {
        Ok(seed) => seed,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::CryptoKeyDerivationFailed,
                context: ErrorContext::default(),
                message_key: "error.seed_derivation_failed",
                fallback: format!("Failed to derive seed: {}", e),
            };
        }
    };

    let mut sk_bytes = seed.to_secret_key();
    let language = crate::crypto::bip39::MnemonicLanguage::English;
    match crate::crypto::keystore::KeyStore::initialize(
        &executor.vault_dir,
        &mut sk_bytes,
        &master_password,
        &crate::crypto::argon2::Argon2Params::medium(),
        language,
    ) {
        Ok(_) => {
            // Cache the master password so subsequent restore handlers can
            // unlock the vault after reopening the file-backed database.
            executor.verified_master_password = Some(master_password);
            CommandResult::KeyFileRebuilt
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.keystore_init_failed",
            fallback: format!("Failed to rebuild key file: {}", e),
        },
    }
}

/// Validate that the existing vault.db can be decrypted with the rebuilt key.
///
/// In SQLCipher production builds, validates by deriving the DbPageKey from the
/// cached master password and opening the encrypted database through VaultDbFactory.
/// In non-SQLCipher builds, uses the vault trait's unlock method.
pub async fn handle_validate_restored_database(executor: &mut CommandExecutor) -> CommandResult {
    if !executor.vault_dir.join("vault.db").exists() {
        return CommandResult::DatabaseValidationFailed {
            reason: "vault.db was not found.".to_string(),
        };
    }

    let master_password = match executor.verified_master_password.take() {
        Some(pw) => pw,
        None => {
            return CommandResult::DatabaseValidationFailed {
                reason: "Master password not available for validation.".to_string(),
            };
        }
    };

    let vault_dir = executor.vault_dir.clone();

    #[cfg(feature = "sqlcipher")]
    {
        let db_page_key = match derive_db_page_key_for_pending(&vault_dir, &master_password) {
            Ok(key) => key,
            Err(e) => {
                drop(master_password);
                return CommandResult::DatabaseValidationFailed {
                    reason: format!("Failed to derive database page key for validation: {}", e),
                };
            }
        };
        match crate::db::vault_db::VaultDbFactory::open_sqlcipher_vault(&vault_dir, &db_page_key) {
            Ok(_conn) => {
                drop(master_password);
                drop(_conn);
                CommandResult::DatabaseRestored {
                    source: crate::commands::types::DatabaseRecoverySource::Okb,
                }
            }
            Err(e) => {
                drop(master_password);
                CommandResult::DatabaseValidationFailed {
                    reason: format!("Restored database does not match current key: {}", e),
                }
            }
        }
    }
    #[cfg(not(feature = "sqlcipher"))]
    {
        let unlock_result = {
            let vault = match executor.vault_mut() {
                Ok(v) => v,
                Err(e) => {
                    drop(master_password);
                    return CommandResult::DatabaseValidationFailed {
                        reason: format!("Vault not available for validation: {}", e),
                    };
                }
            };
            vault.unlock(&vault_dir, &master_password)
        };
        match unlock_result {
            Ok(_) => {
                drop(master_password);
                CommandResult::DatabaseRestored {
                    source: crate::commands::types::DatabaseRecoverySource::Okb,
                }
            }
            Err(e) => {
                drop(master_password);
                CommandResult::DatabaseValidationFailed {
                    reason: format!("Restored database does not match current key: {}", e),
                }
            }
        }
    }
}

#[cfg(test)]
mod rollback_tests {
    use super::*;

    #[test]
    fn cleanup_removes_newly_created_artifacts() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault_dir = dir.path();

        std::fs::write(vault_dir.join("wrapped_secret_key.json"), b"key").expect("write key");
        std::fs::write(vault_dir.join("vault.db"), b"db").expect("write db");
        std::fs::write(vault_dir.join("vault.db-wal"), b"wal").expect("write wal");
        std::fs::write(vault_dir.join("vault.db-shm"), b"shm").expect("write shm");

        cleanup_failed_new_vault_initialization(vault_dir, false, false, false, false);

        assert!(
            !vault_dir.join("wrapped_secret_key.json").exists(),
            "new key must be removed"
        );
        assert!(
            !vault_dir.join("vault.db").exists(),
            "new db must be removed"
        );
        assert!(
            !vault_dir.join("vault.db-wal").exists(),
            "new wal must be removed"
        );
        assert!(
            !vault_dir.join("vault.db-shm").exists(),
            "new shm must be removed"
        );
    }

    #[test]
    fn cleanup_preserves_preexisting_key() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault_dir = dir.path();

        std::fs::write(vault_dir.join("wrapped_secret_key.json"), b"original key")
            .expect("write key");

        cleanup_failed_new_vault_initialization(vault_dir, true, false, false, false);

        assert!(
            vault_dir.join("wrapped_secret_key.json").exists(),
            "preexisting key must survive cleanup"
        );
    }

    #[test]
    fn cleanup_preserves_preexisting_database() {
        let dir = tempfile::tempdir().expect("temp dir");
        let vault_dir = dir.path();

        std::fs::write(vault_dir.join("vault.db"), b"original db").expect("write db");

        cleanup_failed_new_vault_initialization(vault_dir, false, true, false, false);

        assert!(
            vault_dir.join("vault.db").exists(),
            "preexisting db must survive cleanup"
        );
    }

    #[test]
    fn cleanup_missing_artifacts_is_noop() {
        let dir = tempfile::tempdir().expect("temp dir");

        cleanup_failed_new_vault_initialization(dir.path(), false, false, false, false);

        // No files should exist after cleanup when none existed before.
        assert!(!dir.path().join("wrapped_secret_key.json").exists());
        assert!(!dir.path().join("vault.db").exists());
    }
}
