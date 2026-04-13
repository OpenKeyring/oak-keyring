use uuid::Uuid;

use crate::commands::CommandResult;
use crate::commands::types::ConflictResolution;
use crate::errors::{ErrorCode, ErrorContext};

use super::CommandExecutor;

pub fn handle_trigger_sync(_executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Trigger sync not yet implemented."),
    }
}

pub fn handle_resolve_conflict(
    _executor: &mut CommandExecutor,
    _record_id: Uuid,
    _resolution: ConflictResolution,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Resolve conflict not yet implemented."),
    }
}

pub fn handle_resolve_all_conflicts(
    _executor: &mut CommandExecutor,
    _resolution: ConflictResolution,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Resolve all conflicts not yet implemented."),
    }
}
