use crate::commands::types::AuditFilter;
use crate::commands::CommandResult;
use crate::config::sync::ProviderConfig;
use crate::config::AppConfig;
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
    let changed = detect_changed_fields(&executor.config, &config);

    match config.save(&executor.vault_dir) {
        Ok(()) => {
            apply_config_changes(executor, &changed, &config);
            executor.config = config;

            if !changed.is_empty() {
                tracing::info!(changed_fields = ?changed, "Config saved and changes applied");
            }

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

/// Detect which config fields changed between old and new configurations.
fn detect_changed_fields(old: &AppConfig, new: &AppConfig) -> Vec<&'static str> {
    let mut changed = Vec::new();

    if old.general.language != new.general.language {
        changed.push("general.language");
    }
    if old.general.auto_lock_seconds != new.general.auto_lock_seconds {
        changed.push("general.auto_lock_seconds");
    }
    if old.general.clipboard_clear_seconds != new.general.clipboard_clear_seconds {
        changed.push("general.clipboard_clear_seconds");
    }
    if old.general.vault_path != new.general.vault_path {
        changed.push("general.vault_path");
    }
    if old.sync.provider != new.sync.provider
        || old.sync.auto_interval_seconds != new.sync.auto_interval_seconds
    {
        changed.push("sync");
    }
    if old.security.health_check_enabled != new.security.health_check_enabled
        || old.security.health_check_frequency != new.security.health_check_frequency
        || old.security.audit_enabled != new.security.audit_enabled
        || old.security.audit_retention_days != new.security.audit_retention_days
    {
        changed.push("security");
    }
    if old.password.length != new.password.length
        || old.password.include_digits != new.password.include_digits
        || old.password.include_uppercase != new.password.include_uppercase
        || old.password.include_special != new.password.include_special
    {
        changed.push("password");
    }

    changed
}

/// Apply runtime config changes that take effect immediately.
fn apply_config_changes(executor: &mut CommandExecutor, changed: &[&str], _new_config: &AppConfig) {
    for &field in changed {
        match field {
            "general.auto_lock_seconds" => {
                tracing::info!("Auto-lock config changed — timer will rebuild on next tick");
            }
            "general.clipboard_clear_seconds" => {
                executor
                    .clipboard
                    .set_clear_timeout(_new_config.general.clipboard_clear_seconds);
                tracing::info!("Clipboard clear timeout updated");
            }
            "sync" => {
                tracing::info!(
                    "Sync config changed — SyncService rebuild deferred to next startup"
                );
            }
            "general.vault_path" => {
                tracing::warn!("vault_path changed — requires application restart");
            }
            _ => {
                tracing::info!(
                    field = field,
                    "Config field changed (no special runtime handling)"
                );
            }
        }
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_test_sync_connection(
    executor: &mut CommandExecutor,
    _provider_config: Option<ProviderConfig>,
) -> CommandResult {
    let sync = match executor.sync.as_ref() {
        Some(s) => s,
        None => {
            return CommandResult::Error {
                code: ErrorCode::Sync(String::from("not_configured")),
                context: ErrorContext::default(),
                message_key: "error.sync_not_configured",
                fallback: String::from("Sync is not configured."),
            };
        }
    };

    match sync.test_connection().await {
        Ok((success, message)) => CommandResult::SyncConnectionTested { success, message },
        Err(e) => CommandResult::SyncConnectionTested {
            success: false,
            message: e.to_string(),
        },
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
