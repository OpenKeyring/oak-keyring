use uuid::Uuid;

use crate::commands::CommandResult;
use crate::commands::types::FieldSelector;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::SecureStr;

use super::CommandExecutor;

pub async fn handle_copy_to_clipboard(
    _executor: &mut CommandExecutor,
    _id: Uuid,
    _field: FieldSelector,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Copy to clipboard not yet implemented."),
    }
}

pub async fn handle_copy_raw_to_clipboard(
    _executor: &mut CommandExecutor,
    _value: SecureStr,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Copy raw to clipboard not yet implemented."),
    }
}

pub async fn handle_copy_history_password(
    _executor: &mut CommandExecutor,
    _history_id: i64,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Copy history password not yet implemented."),
    }
}
