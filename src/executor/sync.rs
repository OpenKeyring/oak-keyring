use uuid::Uuid;

use crate::commands::types::ConflictResolution;
use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::sync::conflict::ResolutionStrategy;
use crate::types::SyncStats;

use super::CommandExecutor;

#[tracing::instrument(skip_all)]
pub async fn handle_trigger_sync(executor: &mut CommandExecutor) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("sync");
    }

    let cancel = executor.cancel_token().clone();

    let sync = match executor.sync.as_mut() {
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

    match sync.sync_with_cancel(cancel).await {
        Ok(report) => {
            if executor.cancel_token().is_cancelled() {
                return CommandResult::cancelled("sync");
            }
            CommandResult::SyncCompleted {
                stats: SyncStats {
                    total: (report.uploaded + report.downloaded) as i64,
                    pending: 0,
                    synced: (report.uploaded + report.downloaded) as i64,
                    conflicts: report.conflicts as i64,
                },
            }
        }
        Err(e) => {
            if executor.cancel_token().is_cancelled()
                || matches!(e, crate::errors::mapping::sync::SyncError::Cancelled { .. })
            {
                return CommandResult::cancelled("sync");
            }
            CommandResult::Error {
                code: ErrorCode::Sync(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.sync_failed",
                fallback: format!("Sync failed: {}", e),
            }
        }
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_resolve_conflict(
    executor: &mut CommandExecutor,
    record_id: Uuid,
    resolution: ConflictResolution,
) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("sync");
    }

    let sync = match executor.sync.as_mut() {
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

    let strategy = match resolution {
        ConflictResolution::KeepLocal => ResolutionStrategy::KeepLocal,
        ConflictResolution::KeepRemote => ResolutionStrategy::KeepRemote,
    };

    match sync.resolve_conflict(record_id.to_string(), strategy).await {
        Ok(()) => CommandResult::ConflictResolved { record_id },
        Err(e) => CommandResult::Error {
            code: ErrorCode::Sync(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.conflict_resolve_failed",
            fallback: format!("Failed to resolve conflict: {}", e),
        },
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_resolve_all_conflicts(
    executor: &mut CommandExecutor,
    resolution: ConflictResolution,
) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("sync");
    }

    let sync = match executor.sync.as_mut() {
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

    let strategy = match resolution {
        ConflictResolution::KeepLocal => ResolutionStrategy::KeepLocal,
        ConflictResolution::KeepRemote => ResolutionStrategy::KeepRemote,
    };

    match sync.resolve_all_conflicts(strategy).await {
        Ok(count) => CommandResult::AllConflictsResolved { count },
        Err(e) => CommandResult::Error {
            code: ErrorCode::Sync(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.conflict_resolve_all_failed",
            fallback: format!("Failed to resolve all conflicts: {}", e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cancellation check happens before sync.as_mut(), so a test executor with
    /// `sync: None` and a cancelled token still exercises the cancellation path.
    #[tokio::test]
    async fn trigger_sync_returns_cancelled_when_token_already_cancelled() {
        use crate::config::AppConfig;
        use crate::executor::config_impl::ServiceNotificationImpl;
        use crate::services::clipboard::{ClipboardService, MockBackend};
        use crate::services::health::HealthService;
        use crate::services::import_export::ImportExportService;
        use crate::services::vault::VaultService;
        use std::sync::Arc;
        use tokio::sync::mpsc;
        use tokio_util::sync::CancellationToken;

        let conn = crate::db::schema::init_db_in_memory();
        let vault = VaultService::new(conn);
        let (result_tx, _) = mpsc::channel(64);
        let (internal_tx, internal_rx) = mpsc::channel(64);

        let mut executor = CommandExecutor {
            vault,
            sync: None,
            health: HealthService::new(),
            clipboard: Arc::new(ClipboardService::with_backend(
                Box::new(MockBackend::new()),
                30,
            )),
            import_export: ImportExportService::new(),
            config: AppConfig::default(),
            config_notifier: ServiceNotificationImpl::new(),
            vault_dir: std::path::PathBuf::from(":memory:"),
            health_report: None,
            last_health_check_time: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            cancel_token: CancellationToken::new(),
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        };
        executor.cancel_token().cancel();

        let result = handle_trigger_sync(&mut executor).await;

        assert!(matches!(
            result,
            CommandResult::Cancelled { ref operation, .. } if operation == "sync"
        ));
    }

    #[test]
    fn cancelled_helper_returns_correct_operation_name() {
        let result = CommandResult::cancelled("sync");
        assert!(matches!(
            result,
            CommandResult::Cancelled { ref operation, .. } if operation == "sync"
        ));
    }
}
