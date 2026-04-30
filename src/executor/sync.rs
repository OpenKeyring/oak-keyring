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

    /// Tests that `handle_trigger_sync` returns `Cancelled` when the token is
    /// already cancelled. Marked `#[ignore]` because constructing a test
    /// executor with a SyncService requires a cloud storage backend.
    #[tokio::test]
    #[ignore]
    async fn trigger_sync_returns_cancelled_when_token_already_cancelled() {
        // TODO: construct a test executor with sync service
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
