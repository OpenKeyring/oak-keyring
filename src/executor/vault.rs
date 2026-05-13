use crate::commands::{CommandResult, InternalCommand};
use crate::config::ConfigManager;
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::SecureStr;

use super::CommandExecutor;

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
    let last_check = match executor.vault.get_last_health_check_at() {
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

#[tracing::instrument(skip(executor, master_password))]
pub async fn handle_unlock(
    executor: &mut CommandExecutor,
    master_password: SecureStr,
) -> CommandResult {
    match executor.vault.unlock(&executor.vault_dir, &master_password) {
        Ok(()) => {
            schedule_health_check_after_unlock(executor);
            CommandResult::VaultUnlocked
        }
        Err(_) => CommandResult::VaultUnlockFailed {
            attempts_remaining: None,
        },
    }
}

#[tracing::instrument(skip(executor, words))]
pub async fn handle_unlock_with_recovery_key(
    executor: &mut CommandExecutor,
    words: Vec<String>,
) -> CommandResult {
    // Try English first (most common)
    let passkey = match Passkey::from_words(&words, MnemonicLanguage::English) {
        Ok(pk) => pk,
        Err(_) => {
            // Try Chinese
            match Passkey::from_words(&words, MnemonicLanguage::ChineseSimplified) {
                Ok(pk) => pk,
                Err(e) => {
                    return CommandResult::Error {
                        code: ErrorCode::CryptoKeyDerivationFailed,
                        context: ErrorContext::default(),
                        message_key: "error.invalid_recovery_key",
                        fallback: format!("Invalid recovery key: {}", e),
                    };
                }
            }
        }
    };

    match executor.vault.unlock_with_mnemonic(&passkey) {
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
    executor.vault.lock();
    executor.verified_master_password = None;
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
            if executor.vault.is_unlocked() {
                let _ = executor.vault.unlock(&executor.vault_dir, &new_password);
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
    recovery_words: Option<Vec<String>>,
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
    let recovery_words = passkey.to_words();

    // Step 3: Initialize keystore (creates wrapped_secret_key.json)
    let vault_path = crate::paths::data_dir();
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

    // Step 4: Unlock the vault with the new password
    match executor.vault.unlock(&vault_path, &master_password) {
        Ok(()) => CommandResult::VaultInitialized { recovery_words },
        Err(_) => {
            // Keystore was created but vault unlock failed
            // Still return initialized since keystore exists
            tracing::warn!("Vault initialized but auto-unlock failed");
            CommandResult::VaultInitialized { recovery_words }
        }
    }
}

/// Reconstruct a Passkey from pre-generated recovery words.
/// Tries English first, then Chinese Simplified.
fn reconstruct_passkey(words: &[String]) -> Result<Passkey, String> {
    let english = Passkey::from_words(words, MnemonicLanguage::English);
    if english.is_ok() {
        return english;
    }
    Passkey::from_words(words, MnemonicLanguage::ChineseSimplified)
}
