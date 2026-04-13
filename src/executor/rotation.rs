use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};

use super::CommandExecutor;

pub fn handle_trigger_rotation(_executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Trigger rotation not yet implemented."),
    }
}

pub fn handle_check_rotation_trigger(_executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Check rotation trigger not yet implemented."),
    }
}
