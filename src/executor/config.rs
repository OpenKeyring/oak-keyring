use crate::commands::types::AuditFilter;
use crate::commands::CommandResult;
use crate::config::notification::ServiceNotification;
use crate::config::sync::ProviderConfig;
use crate::config::{AppConfig, ConfigManager};
use crate::errors::{ErrorCode, ErrorContext, ServiceError};
use std::future::Future;

use super::CommandExecutor;

#[tracing::instrument(skip_all)]
pub fn handle_load_config(executor: &mut CommandExecutor) -> CommandResult {
    CommandResult::ConfigLoaded {
        config: executor.config.get_config(),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_save_config(executor: &mut CommandExecutor, config: AppConfig) -> CommandResult {
    let old_config = executor.config.get_config();
    let changed = detect_changed_fields(&old_config, &config);

    match executor.config.save(&config, &executor.vault_dir) {
        Ok(()) => {
            let warnings = apply_config_changes(executor, &changed, &config);

            if !changed.is_empty() {
                tracing::info!(changed_fields = ?changed, warnings = warnings.len(), "Config saved and changes applied");
            }

            CommandResult::ConfigSaved { warnings }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::ConfigSaveFailed,
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
        || old.sync.provider_config != new.sync.provider_config
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
fn apply_config_changes(
    executor: &mut CommandExecutor,
    changed: &[&str],
    new_config: &AppConfig,
) -> Vec<String> {
    let mut warnings = Vec::new();

    if !changed.is_empty() {
        let results = executor
            .config_notifier
            .notify_config_change(new_config, &[]);
        for result in &results {
            if let Err(e) = result {
                tracing::error!(error = %e, "Service failed to reload config");
                warnings.push(format!("Service reload failed: {}", e));
            }
        }
    }

    // Handle non-service fields with inline logic.
    for &field in changed {
        match field {
            "general.auto_lock_seconds" => {
                executor.timer_rebuild_pending = true;
                tracing::info!("Auto-lock config changed — timer will rebuild on next tick");
            }
            "sync" => {
                executor.timer_rebuild_pending = true;
                use crate::cloud::provider::create_cloud_storage;
                match create_cloud_storage(&new_config.sync) {
                    Ok(storage) => {
                        executor.sync = Some(crate::services::sync::SyncService::new(storage));
                        tracing::info!("SyncService rebuilt with updated config");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "SyncService rebuild failed — sync disabled");
                        executor.sync = None;
                        warnings.push(format!("Sync service rebuild failed: {}", e));
                    }
                }
            }
            "general.vault_path" => {
                tracing::warn!("vault_path changed — requires application restart");
                warnings.push("vault_path changed — requires application restart".to_string());
            }
            _ => {}
        }
    }

    warnings
}

#[tracing::instrument(skip_all)]
pub async fn handle_test_sync_connection(
    executor: &mut CommandExecutor,
    provider_config: Option<ProviderConfig>,
) -> CommandResult {
    let cancel = executor.cancel_token().clone();
    if cancel.is_cancelled() {
        return CommandResult::cancelled("sync_connection_test");
    }

    use crate::cloud::provider::create_cloud_storage;
    use crate::config::sync::SyncConfig;

    // If caller provides a provider_config, test it without modifying executor.sync.
    if let Some(pc) = provider_config {
        let current_config = executor.config.get_config();
        let test_config = SyncConfig {
            provider: current_config.sync.provider,
            ..current_config.sync.clone()
        };
        // Override just the provider_config for testing.
        let mut test_sync = test_config;
        test_sync.provider_config = Some(pc);

        return match create_cloud_storage(&test_sync) {
            Ok(storage) => {
                let svc = crate::services::sync::SyncService::new(storage);
                connection_test_result_with_cancel(cancel, svc.test_connection()).await
            }
            Err(e) => CommandResult::SyncConnectionTested {
                success: false,
                message: e.to_string(),
            },
        };
    }

    // No provider_config — use existing SyncService.
    let sync = match executor.sync.as_ref() {
        Some(s) => s,
        None => {
            return CommandResult::Error {
                code: ErrorCode::SyncNotConfigured,
                context: ErrorContext::default(),
                message_key: "error.sync_not_configured",
                fallback: String::from("Sync is not configured."),
            };
        }
    };

    connection_test_result_with_cancel(cancel, sync.test_connection()).await
}

async fn connection_test_result_with_cancel<F>(
    cancel: tokio_util::sync::CancellationToken,
    fut: F,
) -> CommandResult
where
    F: Future<Output = Result<(bool, String), crate::errors::mapping::sync::SyncError>>,
{
    tokio::select! {
        _ = cancel.cancelled() => CommandResult::cancelled("sync_connection_test"),
        result = fut => match result {
            Ok((success, message)) => CommandResult::SyncConnectionTested { success, message },
            Err(crate::errors::mapping::sync::SyncError::Cancelled { .. }) => {
                CommandResult::cancelled("sync_connection_test")
            }
            Err(e) => CommandResult::SyncConnectionTested {
                success: false,
                message: e.to_string(),
            },
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
    CommandResult::ConfigSaved { warnings: vec![] }
}

#[tracing::instrument(skip_all)]
pub fn handle_load_audit_log(executor: &mut CommandExecutor, filter: AuditFilter) -> CommandResult {
    match executor.vault.query_audit_log(&filter) {
        Ok((entries, total)) => CommandResult::AuditLogLoaded { entries, total },
        Err(e) => {
            let err: &dyn ServiceError = &e;
            CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.audit_log_failed",
                fallback: format!("Failed to load audit log: {}", e),
            }
        }
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
            config: crate::executor::config_impl::ConfigManagerImpl::new(AppConfig::default()),
            config_notifier,
            vault_dir: std::path::PathBuf::from(":memory:"),
            health_report: None,
            last_health_check_time: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            shutdown_token: CancellationToken::new(),
            operation_cancel_token: CancellationToken::new(),
            timer_rebuild_pending: false,
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
            verified_master_password: None,
        }
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
    fn apply_config_changes_preserves_clipboard_timeout_for_unrelated_field_changes() {
        let mut executor = make_executor_with_clipboard(30);

        let new_config = AppConfig::default();
        apply_config_changes(&mut executor, &["general.auto_lock_seconds"], &new_config);

        assert_eq!(executor.clipboard.clear_timeout(), 30);
    }

    #[test]
    fn apply_config_changes_notifies_all_services_on_any_field_change() {
        let mut executor = make_executor_with_clipboard(30);

        let mut new_config = AppConfig::default();
        new_config.general.clipboard_clear_seconds = 90;

        apply_config_changes(&mut executor, &["general.auto_lock_seconds"], &new_config);

        assert_eq!(executor.clipboard.clear_timeout(), 90);
    }

    #[tokio::test]
    async fn connection_test_future_returns_cancelled_when_token_cancels_while_waiting() {
        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel_for_task = cancel.clone();
        tokio::spawn(async move {
            cancel_for_task.cancel();
        });

        let result = connection_test_result_with_cancel(
            cancel,
            std::future::pending::<Result<(bool, String), crate::errors::mapping::sync::SyncError>>(
            ),
        )
        .await;

        assert!(matches!(
            result,
            CommandResult::Cancelled { ref operation, .. } if operation == "sync_connection_test"
        ));
    }

    #[test]
    fn apply_config_changes_returns_empty_warnings_on_success() {
        let mut executor = make_executor_with_clipboard(30);
        let mut new_config = AppConfig::default();
        new_config.general.clipboard_clear_seconds = 90;

        let warnings = apply_config_changes(
            &mut executor,
            &["general.clipboard_clear_seconds"],
            &new_config,
        );

        assert!(warnings.is_empty());
    }

    #[test]
    fn apply_config_changes_returns_warning_on_vault_path_change() {
        let mut executor = make_executor_with_clipboard(30);
        let new_config = AppConfig::default();

        let warnings = apply_config_changes(&mut executor, &["general.vault_path"], &new_config);

        assert!(warnings
            .iter()
            .any(|w| w.contains("requires application restart")));
    }

    #[test]
    fn handle_save_config_includes_warnings_in_result() {
        let tmp = tempfile::tempdir().expect("tempdir failed");
        let mut executor = make_executor_with_clipboard(30);
        executor.vault_dir = tmp.path().to_path_buf();
        let mut new_config = AppConfig::default();
        new_config.general.clipboard_clear_seconds = 90;

        let result = handle_save_config(&mut executor, new_config);
        match result {
            CommandResult::ConfigSaved { warnings } => {
                assert!(warnings.is_empty());
            }
            _ => panic!("Expected ConfigSaved"),
        }
    }

    #[test]
    fn handle_save_config_includes_vault_path_warning() {
        let tmp = tempfile::tempdir().expect("tempdir failed");
        let mut executor = make_executor_with_clipboard(30);
        executor.vault_dir = tmp.path().to_path_buf();
        let mut new_config = AppConfig::default();
        new_config.general.vault_path = std::path::PathBuf::from("/totally/different/path");

        let result = handle_save_config(&mut executor, new_config);
        match result {
            CommandResult::ConfigSaved { warnings } => {
                assert!(warnings
                    .iter()
                    .any(|w| w.contains("requires application restart")));
            }
            _ => panic!("Expected ConfigSaved"),
        }
    }
}
