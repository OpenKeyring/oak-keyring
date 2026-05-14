use std::future::Future;
use std::pin::Pin;

use uuid::Uuid;

use crate::cloud::CloudMetadata;
use crate::commands::types::ConflictResolution;
use crate::commands::CommandResult;
use crate::errors::mapping::sync::SyncError;
use crate::errors::{ErrorCode, ErrorContext, ServiceError};
use crate::sync::conflict::ResolutionStrategy;
use crate::sync::task::SyncVaultData;
use crate::types::SyncStats;

use super::CommandExecutor;

/// Builds a SyncVaultData snapshot from the current vault state.
///
/// Reads local records, their sync status, the pending upload CloudRecords
/// (with health metadata pre-attached), the vault identity token, and the
/// current metadata version. Returns `None` if the vault is locked or a
/// required read fails.
fn build_sync_vault_data(executor: &CommandExecutor) -> Option<Box<SyncVaultData>> {
    use crate::cloud::record::build_cloud_record;
    use crate::sync::pipeline::LocalRecordInfo;
    use base64::Engine;

    // Check vault is unlocked
    if !executor.vault.is_unlocked() {
        tracing::warn!("Cannot build sync vault data: vault is locked");
        return None;
    }

    // Read local stored records
    let stored_records = match executor.vault.list_all_stored_records() {
        Ok(records) => records,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list stored records for sync");
            return None;
        }
    };

    // Read sync status map
    let sync_status_map = executor.vault.load_sync_status_map();

    // Build LocalRecordInfo per active record
    let local_records: Vec<LocalRecordInfo> = stored_records
        .iter()
        .map(|r| {
            let sync_status = sync_status_map
                .get(&r.id.to_string())
                .copied()
                .unwrap_or(crate::types::sync::SyncStatus::Pending);
            LocalRecordInfo {
                record_id: r.id.to_string(),
                sync_status,
                version: r.version,
            }
        })
        .collect();

    // Read health states for attaching to upload CloudRecords
    let health_states = match executor.vault.list_record_health_states() {
        Ok(list) => list
            .into_iter()
            .map(|s| (s.record_id, s))
            .collect::<std::collections::HashMap<Uuid, crate::types::health::RecordHealthState>>(),
        Err(_) => std::collections::HashMap::new(),
    };

    // Build upload CloudRecords for records with pending sync status.
    // Decrypt each record's name so that CloudRecord::validate() passes on
    // the remote side. Records whose name cannot be decrypted are skipped.
    let uploads: Vec<crate::cloud::CloudRecord> = stored_records
        .iter()
        .filter(|r| {
            sync_status_map
                .get(&r.id.to_string())
                .copied()
                .unwrap_or(crate::types::sync::SyncStatus::Pending)
                == crate::types::sync::SyncStatus::Pending
        })
        .filter_map(|r| {
            let name = match executor.vault.decrypt_record_name_for_sync(r) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(
                        record_id = %r.id,
                        error = %e,
                        "Failed to decrypt record name for sync upload, skipping"
                    );
                    return None;
                }
            };
            let encrypted_base64 =
                base64::engine::general_purpose::STANDARD.encode(&r.encrypted_data);
            let nonce_base64 = base64::engine::general_purpose::STANDARD.encode(r.nonce);
            let aad = crate::cloud::record::AadFields {
                record_id: r.id.to_string(),
                dek_version: r.dek_version,
            };
            let health = health_states.get(&r.id);
            Some(build_cloud_record(
                r,
                &name,
                &encrypted_base64,
                &nonce_base64,
                aad,
                health,
            ))
        })
        .collect();

    // Filter out structurally invalid CloudRecords that would be silently
    // rejected by the remote side during validation on download.
    let valid_uploads: Vec<crate::cloud::CloudRecord> = uploads
        .into_iter()
        .filter(|r| {
            if let Err(e) = r.validate() {
                tracing::error!(
                    record_id = %r.id,
                    error = %e,
                    "Upload CloudRecord failed validation — dropping from sync upload"
                );
                false
            } else {
                true
            }
        })
        .collect();

    // Read metadata version and vault token
    let metadata_version = executor
        .vault
        .get_metadata("metadata_version")
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);

    let vault_token = executor
        .vault
        .get_metadata("vault_identity_token")
        .ok()
        .flatten()
        .unwrap_or_default();

    Some(Box::new(SyncVaultData {
        local_records,
        uploads: valid_uploads,
        local_metadata_version: metadata_version,
        local_vault_token: vault_token,
    }))
}

type SyncFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg_attr(test, mockall::automock)]
trait CloudRestoreMetadataSource: Send {
    fn download_metadata<'a>(
        &'a mut self,
    ) -> SyncFuture<'a, Result<Option<CloudMetadata>, SyncError>>;
}

impl CloudRestoreMetadataSource for crate::services::sync::SyncService {
    fn download_metadata<'a>(
        &'a mut self,
    ) -> SyncFuture<'a, Result<Option<CloudMetadata>, SyncError>> {
        Box::pin(async move { crate::services::sync::SyncService::download_metadata(self).await })
    }
}

async fn ensure_cloud_restore_has_records(
    metadata_source: &mut dyn CloudRestoreMetadataSource,
) -> Result<(), CommandResult> {
    match metadata_source.download_metadata().await {
        Ok(Some(metadata)) if !metadata.records.is_empty() => Ok(()),
        Ok(_) => Err(CommandResult::Error {
            code: ErrorCode::SyncProviderError,
            context: ErrorContext::default(),
            message_key: "error.cloud_restore_empty",
            fallback: "No recoverable cloud sync data was found. Try restoring from a .okb backup."
                .to_string(),
        }),
        Err(e) => Err(CommandResult::Error {
            code: ErrorCode::SyncProviderError,
            context: ErrorContext::default(),
            message_key: "error.cloud_restore_failed",
            fallback: format!("Cloud database restore failed: {}", e),
        }),
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_trigger_sync(executor: &mut CommandExecutor) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("sync");
    }

    let cancel = executor.cancel_token().clone();
    let vault_data = build_sync_vault_data(executor);

    let sync = match executor.sync.as_mut() {
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

    match sync.sync_with_cancel(cancel, vault_data).await {
        Ok(sync_result) => {
            if executor.cancel_token().is_cancelled() {
                return CommandResult::cancelled("sync");
            }

            // Apply downloaded CloudRecords to local vault before health states,
            // so FK constraints on record_health_state.record_id are satisfied.
            for record in &sync_result.downloaded_records {
                if let Err(e) = executor.vault.apply_downloaded_cloud_record(record) {
                    tracing::warn!(
                        record_id = %record.id,
                        error = %e,
                        "Failed to apply downloaded record"
                    );
                }
            }

            // Apply downloaded health states to local vault
            for state in &sync_result.downloaded_health_states {
                if let Err(e) = executor.vault.upsert_record_health_state(state) {
                    tracing::warn!(
                        record_id = %state.record_id,
                        error = %e,
                        "Failed to persist downloaded health state"
                    );
                }
            }
            if !sync_result.downloaded_health_deleted.is_empty() {
                if let Err(e) = executor
                    .vault
                    .delete_record_health_states(&sync_result.downloaded_health_deleted)
                {
                    tracing::warn!(
                        error = %e,
                        "Failed to delete stale health states after sync"
                    );
                }
            }

            // Reload cached health report so UI reflects downloaded health state.
            match super::health::load_cached_health_report(executor) {
                Ok(Some(report)) => executor.health_report = Some(report),
                Ok(None) => executor.health_report = None,
                Err(e) => tracing::warn!(error = %e, "Failed to reload health report after sync"),
            }

            let report = sync_result.report;
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
            let err: &dyn ServiceError = &e;
            CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
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
                code: ErrorCode::SyncNotConfigured,
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
        Err(e) => {
            let err: &dyn ServiceError = &e;
            CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.conflict_resolve_failed",
                fallback: format!("Failed to resolve conflict: {}", e),
            }
        }
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
                code: ErrorCode::SyncNotConfigured,
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
        Err(e) => {
            let err: &dyn ServiceError = &e;
            CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.conflict_resolve_all_failed",
                fallback: format!("Failed to resolve all conflicts: {}", e),
            }
        }
    }
}

/// Restore vault.db from cloud sync (pull-only, never pushes empty local state).
///
/// Creates a file-backed vault.db, unlocks it with the cached master password,
/// then runs a full sync cycle. Since the local vault is empty, the sync
/// effectively becomes pull-only: all cloud records are downloaded and imported
/// with no push or deletion of remote data.
pub async fn handle_restore_database_from_cloud(executor: &mut CommandExecutor) -> CommandResult {
    if executor.sync.is_none() {
        return CommandResult::DatabaseRestoreNeedsOAuth;
    }

    let sync = executor.sync.as_mut().unwrap();
    if let Err(result) = ensure_cloud_restore_has_records(sync).await {
        return result;
    }

    let master_password = match executor.verified_master_password.take() {
        Some(pw) => pw,
        None => {
            return CommandResult::Error {
                code: ErrorCode::ExecutorMasterPasswordRequired,
                context: ErrorContext::default(),
                message_key: "error.password_required",
                fallback: "Master password not available for vault unlock.".to_string(),
            };
        }
    };

    // Create a pending file-backed vault.db. If unlock or sync fails, dropping
    // the guard removes the uncommitted database files.
    let mut pending = match executor.begin_file_backed_vault_db() {
        Ok(pending) => pending,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::VaultDatabaseIoError,
                context: ErrorContext::default(),
                message_key: "error.db_reopen_failed",
                fallback: format!("Failed to create vault database: {}", e),
            };
        }
    };

    if let Err(e) = pending.unlock(&master_password) {
        return CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.unlock_failed",
            fallback: format!("Failed to unlock vault: {}", e),
        };
    }
    drop(master_password);

    // Run a full sync cycle. With an empty local vault, this is effectively
    // pull-only: all cloud records are downloaded and no data is pushed.
    match pending.sync_restore().await {
        Ok(result) => {
            if result.downloaded_records.is_empty() {
                return CommandResult::Error {
                    code: ErrorCode::SyncProviderError,
                    context: ErrorContext::default(),
                    message_key: "error.cloud_restore_empty",
                    fallback:
                        "No recoverable cloud sync data was found. Try restoring from a .okb backup."
                            .to_string(),
                };
            }
            let mut apply_errors = 0usize;
            for record in &result.downloaded_records {
                if let Err(e) = pending.apply_downloaded_cloud_record(record) {
                    apply_errors += 1;
                    tracing::warn!(
                        record_id = %record.id,
                        error = %e,
                        "Failed to apply downloaded record during cloud restore"
                    );
                }
            }
            for state in &result.downloaded_health_states {
                if let Err(e) = pending.upsert_record_health_state(state) {
                    apply_errors += 1;
                    tracing::warn!(error = %e, "Failed to apply downloaded health state during cloud restore");
                }
            }
            if !result.downloaded_health_deleted.is_empty() {
                if let Err(e) =
                    pending.delete_record_health_states(&result.downloaded_health_deleted)
                {
                    apply_errors += 1;
                    tracing::warn!(error = %e, "Failed to delete stale health states during cloud restore");
                }
            }
            if apply_errors > 0 {
                return CommandResult::Error {
                    code: ErrorCode::SyncProviderError,
                    context: ErrorContext::default(),
                    message_key: "error.cloud_restore_failed",
                    fallback: format!(
                        "Cloud database restore failed while applying downloaded records: {} errors.",
                        apply_errors
                    ),
                };
            }
            pending.commit();
            tracing::info!(
                downloaded = result.downloaded_records.len(),
                "cloud restore sync complete"
            );
            CommandResult::DatabaseRestored {
                source: crate::commands::types::DatabaseRecoverySource::Cloud,
            }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::SyncProviderError,
            context: ErrorContext::default(),
            message_key: "error.cloud_restore_failed",
            fallback: format!("Cloud database restore failed: {}", e),
        },
    }
}

#[cfg(test)]
mod cloud_restore_metadata_tests {
    use super::*;
    use crate::cloud::RecordVersionInfo;

    fn metadata_with_record() -> CloudMetadata {
        let mut metadata = CloudMetadata::new("test-vault-token".to_string());
        metadata.upsert_record(
            "record-1".to_string(),
            RecordVersionInfo {
                version: 1,
                updated_at: "2026-05-14T00:00:00Z".to_string(),
                updated_by: "test-device".to_string(),
                checksum: "checksum".to_string(),
                deleted: false,
            },
        );
        metadata
    }

    #[tokio::test]
    async fn cloud_restore_preflight_rejects_missing_metadata() {
        let mut mock = MockCloudRestoreMetadataSource::new();
        mock.expect_download_metadata()
            .once()
            .returning(|| Box::pin(async { Ok(None) }));

        let result = ensure_cloud_restore_has_records(&mut mock).await;

        assert!(matches!(
            result,
            Err(CommandResult::Error {
                code: ErrorCode::SyncProviderError,
                message_key: "error.cloud_restore_empty",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn cloud_restore_preflight_rejects_metadata_without_records() {
        let mut mock = MockCloudRestoreMetadataSource::new();
        mock.expect_download_metadata().once().returning(|| {
            Box::pin(async { Ok(Some(CloudMetadata::new("test-vault-token".to_string()))) })
        });

        let result = ensure_cloud_restore_has_records(&mut mock).await;

        assert!(matches!(
            result,
            Err(CommandResult::Error {
                code: ErrorCode::SyncProviderError,
                message_key: "error.cloud_restore_empty",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn cloud_restore_preflight_rejects_metadata_download_failure() {
        let mut mock = MockCloudRestoreMetadataSource::new();
        mock.expect_download_metadata().once().returning(|| {
            Box::pin(async {
                Err(SyncError::ProviderError {
                    provider: "mock".to_string(),
                    message: "download failed".to_string(),
                })
            })
        });

        let result = ensure_cloud_restore_has_records(&mut mock).await;

        assert!(matches!(
            result,
            Err(CommandResult::Error {
                code: ErrorCode::SyncProviderError,
                message_key: "error.cloud_restore_failed",
                ..
            })
        ));
    }

    #[tokio::test]
    async fn cloud_restore_preflight_accepts_metadata_with_records() {
        let mut mock = MockCloudRestoreMetadataSource::new();
        mock.expect_download_metadata()
            .once()
            .returning(|| Box::pin(async { Ok(Some(metadata_with_record())) }));

        let result = ensure_cloud_restore_has_records(&mut mock).await;

        assert!(result.is_ok());
    }
}
