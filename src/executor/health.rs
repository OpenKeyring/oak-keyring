use uuid::Uuid;

use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};

use super::CommandExecutor;

pub fn handle_run_health_check(_executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Run health check not yet implemented."),
    }
}

pub fn handle_check_hibp(_executor: &mut CommandExecutor, _record_id: Uuid) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Check HIBP not yet implemented."),
    }
}
