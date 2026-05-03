use uuid::Uuid;

use crate::commands::types::FieldSelector;
use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::SecureStr;

use super::CommandExecutor;

#[tracing::instrument(skip_all)]
pub async fn handle_copy_to_clipboard(
    executor: &mut CommandExecutor,
    id: Uuid,
    field: FieldSelector,
) -> CommandResult {
    // Step 1: Decrypt field via VaultService
    let plaintext = match executor.vault.decrypt_field(id, field) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::CryptoDecryptionFailed,
                context: ErrorContext::new().record_id(id),
                message_key: "tui.error.crypto_decryption_failed",
                fallback: format!("Failed to decrypt field: {}", e),
            }
        }
    };

    // Step 2: Copy plaintext to clipboard
    let clear_after = match executor.clipboard.copy(plaintext.get()) {
        Ok(secs) => secs,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ClipboardCopyFailed,
                context: ErrorContext::new()
                    .record_id(id)
                    .field_name(format!("{:?}", field)),
                message_key: "tui.error.clipboard_copy_failed",
                fallback: format!("Failed to copy to clipboard: {}", e),
            }
        }
    };
    // plaintext (SecureStr) is dropped here, zeroized automatically

    CommandResult::CopiedToClipboard {
        field,
        clear_after_seconds: clear_after,
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_copy_raw_to_clipboard(
    executor: &mut CommandExecutor,
    value: SecureStr,
) -> CommandResult {
    let clear_after = match executor.clipboard.copy(value.get()) {
        Ok(secs) => secs,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ClipboardCopyFailed,
                context: ErrorContext::new(),
                message_key: "tui.error.clipboard_copy_failed",
                fallback: format!("Failed to copy to clipboard: {}", e),
            }
        }
    };
    // value (SecureStr) is dropped here

    CommandResult::CopiedToClipboard {
        field: FieldSelector::Password,
        clear_after_seconds: clear_after,
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_copy_history_password(
    executor: &mut CommandExecutor,
    history_id: i64,
) -> CommandResult {
    let plaintext = match executor.vault.decrypt_history_password(history_id) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::CryptoDecryptionFailed,
                context: ErrorContext::new(),
                message_key: "tui.error.crypto_decryption_failed",
                fallback: format!("Failed to decrypt history password: {}", e),
            }
        }
    };

    let clear_after = match executor.clipboard.copy(plaintext.get()) {
        Ok(secs) => secs,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ClipboardCopyFailed,
                context: ErrorContext::new(),
                message_key: "tui.error.clipboard_copy_failed",
                fallback: format!("Failed to copy to clipboard: {}", e),
            }
        }
    };

    CommandResult::HistoryPasswordCopied {
        clear_after_seconds: clear_after,
    }
}
