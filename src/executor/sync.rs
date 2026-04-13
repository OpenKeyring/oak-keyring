use uuid::Uuid;

use crate::commands::types::ConflictResolution;
use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::sync::conflict::ResolutionStrategy;
use crate::types::SyncStats;

use super::CommandExecutor;

#[tracing::instrument(skip_all)]
pub async fn handle_trigger_sync(executor: &mut CommandExecutor) -> CommandResult {
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

    match sync.sync().await {
        Ok(report) => CommandResult::SyncCompleted {
            stats: SyncStats {
                total: (report.uploaded + report.downloaded) as i64,
                pending: 0,
                synced: (report.uploaded + report.downloaded) as i64,
                conflicts: report.conflicts as i64,
            },
        },
        Err(e) => CommandResult::Error {
            code: ErrorCode::Sync(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.sync_failed",
            fallback: format!("Sync failed: {}", e),
        },
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_resolve_conflict(
    executor: &mut CommandExecutor,
    record_id: Uuid,
    resolution: ConflictResolution,
) -> CommandResult {
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
