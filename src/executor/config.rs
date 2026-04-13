use crate::commands::CommandResult;
use crate::commands::types::AuditFilter;
use crate::config::AppConfig;
use crate::config::sync::ProviderConfig;
use crate::errors::{ErrorCode, ErrorContext};

use super::CommandExecutor;

#[tracing::instrument(skip_all)]
pub fn handle_load_config(executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::ConfigLoaded {
        config: executor.config.clone(),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_save_config(executor: &mut CommandExecutor, config: AppConfig) -> CommandResult {
    match config.save(&executor.vault_dir) {
        Ok(()) => {
            executor.config = config;
            CommandResult::ConfigSaved
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::Config(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.config_save_failed",
            fallback: format!("Failed to save config: {}", e),
        },
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_test_sync_connection(
    _executor: &mut CommandExecutor,
    _provider_config: Option<ProviderConfig>,
) -> CommandResult {
    // SyncService is not integrated yet — Task 22 will handle real implementation.
    CommandResult::Error {
        code: ErrorCode::Sync(String::from("not_configured")),
        context: ErrorContext::default(),
        message_key: "error.sync_not_configured",
        fallback: String::from("Sync service is not yet configured."),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_load_audit_log(executor: &mut CommandExecutor, filter: AuditFilter) -> CommandResult {
    match executor.vault.query_audit_log(&filter) {
        Ok((entries, total)) => CommandResult::AuditLogLoaded { entries, total },
        Err(e) => CommandResult::Error {
            code: ErrorCode::Vault(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.audit_log_failed",
            fallback: format!("Failed to load audit log: {}", e),
        },
    }
}
