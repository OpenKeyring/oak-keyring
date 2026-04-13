use uuid::Uuid;

use crate::commands::CommandResult;
use crate::commands::types::FieldSelector;
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
                code: ErrorCode::Vault(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.decrypt_field_failed",
                fallback: format!("Failed to decrypt field: {}", e),
            }
        }
    };

    // Step 2: Copy plaintext to clipboard
    let clear_after = match executor.clipboard.copy(plaintext.get()) {
        Ok(secs) => secs,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::Clipboard(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.clipboard_copy_failed",
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
                code: ErrorCode::Clipboard(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.clipboard_copy_failed",
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
                code: ErrorCode::Vault(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.decrypt_history_failed",
                fallback: format!("Failed to decrypt history password: {}", e),
            }
        }
    };

    let clear_after = match executor.clipboard.copy(plaintext.get()) {
        Ok(secs) => secs,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::Clipboard(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.clipboard_copy_failed",
                fallback: format!("Failed to copy to clipboard: {}", e),
            }
        }
    };

    CommandResult::HistoryPasswordCopied {
        clear_after_seconds: clear_after,
    }
}
