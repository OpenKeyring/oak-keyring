use std::path::PathBuf;

use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::SecureStr;

use super::CommandExecutor;

pub async fn handle_unlock(_executor: &mut CommandExecutor, _master_password: SecureStr) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Vault unlock not yet implemented."),
    }
}

pub async fn handle_unlock_with_recovery_key(_executor: &mut CommandExecutor, _words: Vec<String>) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Vault unlock with recovery key not yet implemented."),
    }
}

pub fn handle_lock(_executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Vault lock not yet implemented."),
    }
}

pub fn handle_verify_master_password(_executor: &mut CommandExecutor, _password: SecureStr) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Master password verification not yet implemented."),
    }
}

pub fn handle_change_master_password(
    _executor: &mut CommandExecutor,
    _current_password: SecureStr,
    _new_password: SecureStr,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Master password change not yet implemented."),
    }
}

pub async fn handle_initialize_vault(
    _executor: &mut CommandExecutor,
    _vault_path: PathBuf,
    _master_password: SecureStr,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Vault initialization not yet implemented."),
    }
}
