use crate::commands::CommandResult;
use crate::errors::mapping::rotation::RotationError;
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::rotation::RotationService;
use crate::services::sync::SyncService;
use crate::services::vault::VaultService;
use crate::types::rotation::{RotationConfig, RotationResult};

use super::CommandExecutor;

/// Load rotation config from vault metadata.
fn load_rotation_config(vault: &VaultService) -> Result<RotationConfig, String> {
    match vault.get_metadata("rotation_config") {
        Ok(Some(json)) if !json.is_empty() => {
            serde_json::from_str(&json).map_err(|e| e.to_string())
        }
        _ => Ok(RotationConfig::default()),
    }
}

/// CAS conflict resolution per R1-A1 §4.2.
///
/// On CAS push failure, pulls cloud metadata and checks DEK version alignment.
/// Returns Ok(cloud_dek_version) if compatible, Err with details if anomaly detected.
async fn resolve_cas_conflict(
    sync_svc: &mut SyncService,
    _local_old_version: u32,
    local_new_version: u32,
) -> Result<u32, String> {
    match sync_svc.download_metadata().await {
        Ok(Some(cloud_meta)) => {
            let cloud_dek = cloud_meta.current_dek_version;
            if cloud_dek == local_new_version {
                tracing::info!(
                    cloud_dek,
                    local_new_version,
                    "CAS conflict resolved: cloud DEK matches local, records already aligned"
                );
                Ok(cloud_dek)
            } else if cloud_dek > local_new_version {
                tracing::warn!(
                    cloud_dek,
                    local_new_version,
                    "CAS conflict: cloud DEK is higher, lazy migration will align on next sync"
                );
                Ok(cloud_dek)
            } else {
                let msg = format!(
                    "CAS conflict anomaly: cloud dek_version {} < local {}",
                    cloud_dek, local_new_version
                );
                tracing::error!(cloud_dek, local_new_version, "{}", msg);
                Err(msg)
            }
        }
        Ok(None) => {
            let msg = "CAS conflict but cloud metadata vanished — transient inconsistency";
            tracing::warn!("{}", msg);
            Err(msg.to_string())
        }
        Err(e) => {
            let msg = format!(
                "CAS conflict and failed to download cloud metadata for alignment: {}",
                e
            );
            tracing::warn!("{}", msg);
            Err(msg)
        }
    }
}

/// Shared sync mutex protocol for DEK rotation operations.
///
/// Handles the full lifecycle: pause sync → fetch CAS version → execute
/// rotation via closure → CAS push with conflict resolution → resume sync.
///
/// Eliminates duplication between trigger and resume rotation paths, which
/// differ only in the rotation action and error message labels.
async fn execute_rotation_protocol<F>(
    executor: &mut CommandExecutor,
    label: &'static str,
    rotation_error_key: &'static str,
    rotation_fn: F,
) -> CommandResult
where
    F: FnOnce(&mut RotationService, u64) -> Result<RotationResult, RotationError>,
{
    // 1. Sync Mutex: Pause sync pipeline
    if let Some(sync_svc) = &mut executor.sync {
        if let Err(e) = sync_svc.pause().await {
            return CommandResult::Error {
                code: ErrorCode::SyncPauseFailed,
                context: ErrorContext::new(),
                message_key: "tui.error.sync_pause_failed",
                fallback: format!("Failed to pause sync for {}: {}", label, e),
            };
        }
    }

    // 2. Fetch current cloud revision for CAS
    let expected_version = if let Some(sync_svc) = &mut executor.sync {
        match sync_svc.download_metadata().await {
            Ok(Some(meta)) => meta.metadata_version,
            Ok(None) => 0,
            Err(e) => {
                if let Some(sync_svc) = &mut executor.sync {
                    let _ = sync_svc.resume().await;
                }
                return CommandResult::Error {
                    code: ErrorCode::SyncProviderError,
                    context: ErrorContext::new(),
                    message_key: "tui.error.sync_provider_error",
                    fallback: format!("Failed to download cloud metadata for {}: {}", label, e),
                };
            }
        }
    } else {
        0
    };

    // 3. Execute rotation
    let placeholder_conn =
        rusqlite::Connection::open_in_memory().expect("in-memory SQLite should never fail");
    let placeholder = VaultService::new(placeholder_conn);
    let vault = std::mem::replace(&mut executor.vault, placeholder);
    let mut rotation_svc = RotationService::new(vault);
    let rotation_result = rotation_fn(&mut rotation_svc, expected_version);
    let vault = rotation_svc.into_vault();
    executor.vault = vault;

    // 4. Process result: CAS push with conflict resolution
    let result = match rotation_result {
        Ok(res) => {
            if let Some(sync_svc) = &mut executor.sync {
                match sync_svc.download_metadata().await {
                    Ok(Some(mut meta)) => {
                        meta.current_dek_version = res.new_dek_version;
                        meta.metadata_version += 1;

                        if let Err(cas_err) =
                            sync_svc.push_metadata_atomic(meta, expected_version).await
                        {
                            // R1-A1 §4.2: CAS conflict — pull cloud version and re-align
                            let alignment = resolve_cas_conflict(
                                sync_svc,
                                res.old_dek_version,
                                res.new_dek_version,
                            )
                            .await;

                            if let Some(sync_svc) = &mut executor.sync {
                                let _ = sync_svc.resume().await;
                            }

                            return match alignment {
                                Ok(cloud_dek) => CommandResult::Error {
                                    code: ErrorCode::SyncConflictDetected,
                                    context: ErrorContext::new()
                                        .expected_version(u64::from(res.old_dek_version))
                                        .actual_version(u64::from(res.new_dek_version)),
                                    message_key: "tui.error.sync_conflict_detected",
                                    fallback: format!(
                                        "DEK {} v{} -> v{} completed locally but \
                                         another device updated cloud simultaneously. \
                                         Local data is safe (cloud DEK v{}). \
                                         Records will sync on next cycle.",
                                        label, res.old_dek_version, res.new_dek_version, cloud_dek
                                    ),
                                },
                                Err(_align_err) => CommandResult::Error {
                                    code: ErrorCode::SyncConflictDetected,
                                    context: ErrorContext::new()
                                        .expected_version(u64::from(res.old_dek_version))
                                        .actual_version(u64::from(res.new_dek_version)),
                                    message_key: "tui.error.sync_conflict_detected",
                                    fallback: format!(
                                        "DEK {} v{} -> v{} completed locally but \
                                         cloud push failed: {}. Alignment check also failed. \
                                         Local data is safe. Manual sync recommended.",
                                        label, res.old_dek_version, res.new_dek_version, cas_err
                                    ),
                                },
                            };
                        }
                    }
                    Ok(None) => {
                        // No cloud metadata — offline-first, local rotation is fine
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "Failed to download metadata for CAS push ({})",
                            label
                        );
                    }
                }
            }

            CommandResult::RotationCompleted {
                old_version: res.old_dek_version,
                new_version: res.new_dek_version,
                records_migrated: res.records_migrated,
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "DEK {} failed", label);
            CommandResult::Error {
                code: ErrorCode::DekRotationFailed,
                context: ErrorContext::new(),
                message_key: rotation_error_key,
                fallback: format!("DEK {} failed: {}", label, e),
            }
        }
    };

    // 5. Resume sync
    if let Some(sync_svc) = &mut executor.sync {
        let _ = sync_svc.resume().await;
    }

    result
}

#[tracing::instrument(skip_all)]
pub async fn handle_trigger_rotation(executor: &mut CommandExecutor) -> CommandResult {
    execute_rotation_protocol(
        executor,
        "rotation",
        "error.rotation_failed",
        |svc, version| svc.trigger_rotation(version),
    )
    .await
}

/// Resume an interrupted DEK rotation (crash recovery).
///
/// Checks for a pending checkpoint, then delegates to the shared
/// sync mutex protocol via `execute_rotation_protocol`.
#[tracing::instrument(skip_all)]
pub async fn handle_resume_rotation(executor: &mut CommandExecutor) -> CommandResult {
    // Check if there's actually a pending checkpoint
    {
        let placeholder_conn =
            rusqlite::Connection::open_in_memory().expect("in-memory SQLite should never fail");
        let placeholder = VaultService::new(placeholder_conn);
        let vault = std::mem::replace(&mut executor.vault, placeholder);

        let rotation_svc = RotationService::new(vault);
        let has_checkpoint = matches!(rotation_svc.has_pending_checkpoint(), Ok(true));

        let vault = rotation_svc.into_vault();
        executor.vault = vault;

        if !has_checkpoint {
            return CommandResult::RotationTriggerChecked {
                should_rotate: false,
                reason: Some("no_pending_checkpoint".to_string()),
            };
        }
    }

    execute_rotation_protocol(
        executor,
        "rotation resume",
        "error.rotation_resume_failed",
        |svc, _version| svc.resume_rotation(),
    )
    .await
}

#[tracing::instrument(skip_all)]
pub fn handle_check_rotation_trigger(executor: &mut CommandExecutor) -> CommandResult {
    let config = match load_rotation_config(&executor.vault) {
        Ok(c) => c,
        Err(_e) => {
            return CommandResult::Error {
                code: ErrorCode::DekRotationFailed,
                context: ErrorContext::new(),
                message_key: "tui.error.rotation_config_load_failed",
                fallback: String::from("Failed to load rotation config"),
            };
        }
    };

    let online = executor.sync.is_some();

    let days_since = config.last_rotation_at.map(|last| {
        let duration = chrono::Utc::now().signed_duration_since(last);
        duration.num_days().max(0) as u32
    });

    let trigger = crate::services::rotation::check_trigger(
        &config,
        online,
        days_since,
        config.current_dek_record_count,
    );

    match trigger {
        Some(t) => CommandResult::RotationTriggerChecked {
            should_rotate: true,
            reason: Some(format!("{:?}", t)),
        },
        None => CommandResult::RotationTriggerChecked {
            should_rotate: false,
            reason: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::CloudMetadata;
    use crate::config::notification::ServiceNotification;
    use crate::config::AppConfig;
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::db::schema::{initialize_metadata, initialize_schema};
    use crate::executor::config_impl::{ClipboardConfigAdapter, ServiceNotificationImpl};
    use crate::services::clipboard::{ClipboardService, MockBackend};
    use crate::services::health::HealthService;
    use crate::services::import_export::ImportExportService;
    use crate::services::rotation::save_checkpoint;
    use crate::services::sync::SyncService;
    use crate::types::rotation::{RotationCheckpoint, RotationTrigger};
    use rusqlite::Connection;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn setup_vault_unlocked() -> VaultService {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn);
        initialize_metadata(&conn);
        let mut vault = VaultService::new(conn);
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
        vault.unlock_with_mnemonic(&mnemonic).unwrap();
        vault
    }

    fn setup_executor() -> CommandExecutor {
        setup_executor_with_sync(None)
    }

    fn setup_executor_with_sync(sync: Option<SyncService>) -> CommandExecutor {
        let vault = setup_vault_unlocked();
        let (result_tx, _) = mpsc::channel(64);
        let (internal_tx, internal_rx) = mpsc::channel(64);
        let clipboard = Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        ));
        let mut config_notifier = ServiceNotificationImpl::new();
        config_notifier.register_service(Box::new(ClipboardConfigAdapter::new(Arc::clone(
            &clipboard,
        ))));

        CommandExecutor {
            vault,
            sync,
            health: HealthService::new(),
            clipboard,
            import_export: ImportExportService::new(),
            config: AppConfig::default(),
            config_notifier,
            vault_dir: PathBuf::from(":memory:"),
            health_report: None,
            last_health_check_time: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            cancel_token: CancellationToken::new(),
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Create a SyncService backed by a real filesystem (required for atomic
    /// rename-based uploads). Returns (TempDir, SyncService) — caller must
    /// keep TempDir alive for the test duration.
    fn create_sync_service() -> (TempDir, SyncService) {
        let temp_dir = TempDir::new().unwrap();
        let op = opendal::Operator::new(
            opendal::services::Fs::default().root(temp_dir.path().to_str().unwrap()),
        )
        .unwrap()
        .finish();
        let storage = crate::cloud::CloudStorage::new(op, "fs".to_string());
        (temp_dir, SyncService::new(storage))
    }

    async fn seed_cloud_metadata(sync: &mut SyncService, dek_version: u32) {
        let mut meta = CloudMetadata::new("test-vault".to_string());
        meta.current_dek_version = dek_version;
        sync.push_metadata_atomic(meta, 0).await.unwrap();
    }

    // -----------------------------------------------------------------------
    // execute_rotation_protocol: trigger rotation path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn trigger_rotation_no_sync_success() {
        let mut executor = setup_executor();
        let result = handle_trigger_rotation(&mut executor).await;

        match &result {
            CommandResult::RotationCompleted {
                old_version,
                new_version,
                records_migrated,
            } => {
                assert_eq!(*old_version, 1);
                assert_eq!(*new_version, 2);
                assert_eq!(*records_migrated, 0);
            }
            _ => panic!("expected RotationCompleted, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn trigger_rotation_with_sync_no_prior_metadata() {
        let (_dir, sync) = create_sync_service();
        let mut executor = setup_executor_with_sync(Some(sync));
        let result = handle_trigger_rotation(&mut executor).await;

        // No cloud metadata → second download returns None → CAS push skipped
        match &result {
            CommandResult::RotationCompleted { .. } => {}
            _ => panic!("expected RotationCompleted, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn trigger_rotation_with_sync_and_prior_metadata() {
        let (_dir, mut sync) = create_sync_service();
        seed_cloud_metadata(&mut sync, 1).await;

        let mut executor = setup_executor_with_sync(Some(sync));
        let result = handle_trigger_rotation(&mut executor).await;

        // Cloud metadata exists → CAS push attempted and should succeed
        match &result {
            CommandResult::RotationCompleted {
                old_version,
                new_version,
                ..
            } => {
                assert_eq!(*old_version, 1);
                assert_eq!(*new_version, 2);
            }
            _ => panic!("expected RotationCompleted, got {:?}", result),
        }
    }

    // -----------------------------------------------------------------------
    // execute_rotation_protocol: resume rotation path
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn resume_rotation_no_checkpoint() {
        let mut executor = setup_executor();
        let result = handle_resume_rotation(&mut executor).await;

        match &result {
            CommandResult::RotationTriggerChecked {
                should_rotate,
                reason,
            } => {
                assert!(!should_rotate);
                assert_eq!(reason.as_deref(), Some("no_pending_checkpoint"));
            }
            _ => panic!("expected RotationTriggerChecked, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn resume_rotation_with_checkpoint() {
        let mut executor = setup_executor();

        // Simulate interrupted rotation by writing a checkpoint
        let checkpoint = RotationCheckpoint {
            trigger: RotationTrigger::Manual,
            old_dek_version: 1,
            new_dek_version: 2,
            total_records: 0,
            migrated_records: 0,
            last_migrated_record_id: None,
            started_at: chrono::Utc::now(),
            cloud_metadata_revision: "0".to_string(),
        };
        save_checkpoint(&mut executor.vault, &checkpoint).unwrap();

        let result = handle_resume_rotation(&mut executor).await;

        match &result {
            CommandResult::RotationCompleted {
                old_version,
                new_version,
                records_migrated,
            } => {
                assert_eq!(*old_version, 1);
                assert_eq!(*new_version, 2);
                assert_eq!(*records_migrated, 0);
            }
            _ => panic!("expected RotationCompleted, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn resume_rotation_with_checkpoint_and_sync() {
        let (_dir, mut sync) = create_sync_service();
        seed_cloud_metadata(&mut sync, 1).await;

        let mut executor = setup_executor_with_sync(Some(sync));

        let checkpoint = RotationCheckpoint {
            trigger: RotationTrigger::Manual,
            old_dek_version: 1,
            new_dek_version: 2,
            total_records: 0,
            migrated_records: 0,
            last_migrated_record_id: None,
            started_at: chrono::Utc::now(),
            cloud_metadata_revision: "0".to_string(),
        };
        save_checkpoint(&mut executor.vault, &checkpoint).unwrap();

        let result = handle_resume_rotation(&mut executor).await;

        match &result {
            CommandResult::RotationCompleted { .. } => {}
            _ => panic!("expected RotationCompleted, got {:?}", result),
        }
    }

    // -----------------------------------------------------------------------
    // resolve_cas_conflict: unit tests for CAS conflict resolution
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn resolve_cas_conflict_cloud_matches_local() {
        let (_dir, mut sync) = create_sync_service();
        let mut meta = CloudMetadata::new("test-vault".to_string());
        meta.current_dek_version = 5;
        sync.push_metadata_atomic(meta, 0).await.unwrap();

        let result = resolve_cas_conflict(&mut sync, 4, 5).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }

    #[tokio::test]
    async fn resolve_cas_conflict_cloud_higher() {
        let (_dir, mut sync) = create_sync_service();
        let mut meta = CloudMetadata::new("test-vault".to_string());
        meta.current_dek_version = 10;
        sync.push_metadata_atomic(meta, 0).await.unwrap();

        let result = resolve_cas_conflict(&mut sync, 4, 5).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 10);
    }

    #[tokio::test]
    async fn resolve_cas_conflict_cloud_lower_anomaly() {
        let (_dir, mut sync) = create_sync_service();
        let mut meta = CloudMetadata::new("test-vault".to_string());
        meta.current_dek_version = 3;
        sync.push_metadata_atomic(meta, 0).await.unwrap();

        let result = resolve_cas_conflict(&mut sync, 1, 5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("anomaly"));
    }

    #[tokio::test]
    async fn resolve_cas_conflict_no_cloud_metadata() {
        let (_dir, mut sync) = create_sync_service();

        let result = resolve_cas_conflict(&mut sync, 1, 5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("vanished"));
    }

    // -----------------------------------------------------------------------
    // handle_check_rotation_trigger
    // -----------------------------------------------------------------------

    #[test]
    fn check_rotation_trigger_default_config_no_sync() {
        let mut executor = setup_executor();
        let result = handle_check_rotation_trigger(&mut executor);

        match &result {
            CommandResult::RotationTriggerChecked { should_rotate, .. } => {
                // Default: auto_rotate=true, but no sync (offline) → no trigger
                assert!(!should_rotate);
            }
            _ => panic!("expected RotationTriggerChecked, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn check_rotation_trigger_with_sync_online() {
        let (_dir, sync) = create_sync_service();
        let mut executor = setup_executor_with_sync(Some(sync));
        let result = handle_check_rotation_trigger(&mut executor);

        match &result {
            CommandResult::RotationTriggerChecked { should_rotate, .. } => {
                // Default config with fresh vault: no last_rotation_at → time trigger
                // check_trigger with days_since=None (no last rotation) should not trigger
                // because there's nothing to compare against
                assert!(!should_rotate);
            }
            _ => panic!("expected RotationTriggerChecked, got {:?}", result),
        }
    }
}
