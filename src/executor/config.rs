use crate::commands::CommandResult;
use crate::commands::types::AuditFilter;
use crate::config::AppConfig;
use crate::config::sync::ProviderConfig;
use crate::errors::{ErrorCode, ErrorContext};

use super::CommandExecutor;

pub fn handle_load_config(_executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Load config not yet implemented."),
    }
}

pub fn handle_save_config(_executor: &mut CommandExecutor, _config: AppConfig) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Save config not yet implemented."),
    }
}

pub fn handle_test_sync_connection(
    _executor: &mut CommandExecutor,
    _provider_config: Option<ProviderConfig>,
) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Test sync connection not yet implemented."),
    }
}

pub fn handle_load_audit_log(_executor: &mut CommandExecutor, _filter: AuditFilter) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Executor(String::from("not_implemented")),
        context: ErrorContext::default(),
        message_key: "error.not_implemented",
        fallback: String::from("Load audit log not yet implemented."),
    }
}
