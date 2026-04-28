use crate::commands::types::AuditFilter;
use crate::commands::CommandResult;
use crate::config::notification::ServiceNotification;
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

/// Map config field paths to service IDs for the notification system.
fn map_to_service_ids<'a>(changed: &[&'a str]) -> Vec<&'a str> {
    changed
        .iter()
        .filter_map(|&field| match field {
            "general.clipboard_clear_seconds" => Some("clipboard"),
            _ => None,
        })
        .collect()
}

/// Apply runtime config changes that take effect immediately.
fn apply_config_changes(executor: &mut CommandExecutor, changed: &[&str], new_config: &AppConfig) {
    // Notify registered services via the notification system.
    let service_ids = map_to_service_ids(changed);
    if !service_ids.is_empty() {
        let results = executor
            .config_notifier
            .notify_config_change(new_config, &service_ids);
        for result in &results {
            if let Err(e) = result {
                tracing::error!(error = %e, "Service failed to reload config");
            }
        }
    }

    // Handle non-service fields with inline logic.
    for &field in changed {
        match field {
            "general.auto_lock_seconds" => {
                tracing::info!("Auto-lock config changed — timer will rebuild on next tick");
            }
            "sync" => {
                use crate::cloud::provider::create_cloud_storage;
                match create_cloud_storage(&new_config.sync) {
                    Ok(storage) => {
                        executor.sync = Some(crate::services::sync::SyncService::new(storage));
                        tracing::info!("SyncService rebuilt with updated config");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "SyncService rebuild failed — sync disabled");
                        executor.sync = None;
                    }
                }
            }
            "general.vault_path" => {
                tracing::warn!("vault_path changed — requires application restart");
            }
            _ => {}
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
pub async fn handle_oauth2_authorize_google_drive(executor: &mut CommandExecutor) -> CommandResult {
    use crate::cloud::oauth2::{OAuth2Engine, TokenStore};
    use crate::cloud::providers::GoogleDriveProvider;

    let token_store = {
        let mut ts_guard = executor.oauth2_token_store.lock().await;
        if ts_guard.is_none() {
            let base_path = dirs::config_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("open-keyring")
                .join("tokens");
            *ts_guard = Some(TokenStore::new(base_path));
        }
        ts_guard.clone().unwrap()
    };

    let provider = GoogleDriveProvider::new();
    let cancel = executor.cancel_token().clone();
    let result_tx = executor.result_tx.clone();

    tokio::spawn(async move {
        match OAuth2Engine::authorize(&provider, &token_store, cancel).await {
            Ok(token) => {
                let _ = result_tx
                    .send(crate::commands::Message::CommandCompleted(
                        CommandResult::OAuth2Authorized {
                            provider: "google_drive".to_string(),
                            access_token: token.access_token.clone(),
                            refresh_token: token.refresh_token.clone(),
                        },
                    ))
                    .await;
            }
            Err(e) => {
                let _ = result_tx
                    .send(crate::commands::Message::CommandCompleted(
                        CommandResult::OAuth2Failed {
                            provider: "google_drive".to_string(),
                            error: e.to_string(),
                        },
                    ))
                    .await;
            }
        }
    });

    // Fire-and-forget: the actual result comes back via the spawned task.
    // Return a neutral result so the UI doesn't prematurely set Authorized state.
    CommandResult::ConfigSaved
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::notification::ServiceNotification;
    use crate::executor::config_impl::{ClipboardConfigAdapter, ServiceNotificationImpl};
    use crate::services::clipboard::{ClipboardService, MockBackend};
    use std::sync::Arc;

    fn make_executor_with_clipboard(timeout: u64) -> super::super::CommandExecutor {
        use crate::services::health::HealthService;
        use crate::services::import_export::ImportExportService;
        use crate::services::vault::VaultService;
        use rusqlite::Connection;
        use tokio::sync::mpsc;
        use tokio_util::sync::CancellationToken;

        let conn = crate::db::schema::init_db_in_memory();
        let vault = VaultService::new(conn);
        let (result_tx, _) = mpsc::channel(64);
        let (internal_tx, internal_rx) = mpsc::channel(64);

        let clipboard = Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            timeout,
        ));
        let mut config_notifier = ServiceNotificationImpl::new();
        config_notifier.register_service(Box::new(ClipboardConfigAdapter::new(Arc::clone(
            &clipboard,
        ))));

        super::super::CommandExecutor {
            vault,
            sync: None,
            health: HealthService::new(),
            clipboard,
            import_export: ImportExportService::new(),
            config: AppConfig::default(),
            config_notifier,
            vault_dir: std::path::PathBuf::from(":memory:"),
            health_report: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            cancel_token: CancellationToken::new(),
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    #[test]
    fn map_to_service_ids_clipboard_field() {
        let ids = map_to_service_ids(&["general.clipboard_clear_seconds"]);
        assert_eq!(ids, vec!["clipboard"]);
    }

    #[test]
    fn map_to_service_ids_no_match() {
        let ids = map_to_service_ids(&["general.auto_lock_seconds", "sync"]);
        assert!(ids.is_empty());
    }

    #[test]
    fn map_to_service_ids_mixed() {
        let ids = map_to_service_ids(&[
            "general.clipboard_clear_seconds",
            "sync",
            "general.auto_lock_seconds",
        ]);
        assert_eq!(ids, vec!["clipboard"]);
    }

    #[test]
    fn apply_config_changes_updates_clipboard_via_notifier() {
        let mut executor = make_executor_with_clipboard(30);
        assert_eq!(executor.clipboard.clear_timeout(), 30);

        let mut new_config = AppConfig::default();
        new_config.general.clipboard_clear_seconds = 90;

        apply_config_changes(
            &mut executor,
            &["general.clipboard_clear_seconds"],
            &new_config,
        );

        assert_eq!(executor.clipboard.clear_timeout(), 90);
    }

    #[test]
    fn apply_config_changes_ignores_non_service_fields() {
        let mut executor = make_executor_with_clipboard(30);

        let new_config = AppConfig::default();
        apply_config_changes(&mut executor, &["general.auto_lock_seconds"], &new_config);

        assert_eq!(executor.clipboard.clear_timeout(), 30); // unchanged
    }
}
