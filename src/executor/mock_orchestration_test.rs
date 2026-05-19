//! Mockall-based executor orchestration tests.
//!
//! These tests verify executor routing and result handling WITHOUT requiring
//! real DB/crypto/sync infrastructure. Each test uses mock services to prove
//! that the executor correctly dispatches commands to the appropriate service
//! methods and returns the expected results.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::cloud::CloudMetadata;
use crate::commands::CommandResult;
use crate::config::AppConfig;
use crate::executor::{health, rotation, sync, vault, CommandExecutor};
use crate::services::health::MockHealth;
use crate::services::sync::{MockSyncService, SyncResult};
use crate::services::vault::MockVault;
use crate::sync::task::SyncReport;
use crate::types::SecureStr;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn base_builder() -> crate::executor::ExecutorBuilder {
    let (result_tx, _) = mpsc::channel(64);
    CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .result_tx(result_tx)
        .shutdown_token(CancellationToken::new())
}

/// Mock vault that reports unlocked. Callers add specific method expectations
/// BEFORE passing to the builder.
fn mock_unlocked_vault() -> MockVault {
    let mut mock = MockVault::new();
    mock.expect_is_unlocked().returning(|| true);
    mock
}

/// Default vault mock that returns unlocked and handles the most common
/// get_metadata calls (returns None for any key).
fn permissive_unlocked_vault() -> MockVault {
    let mut mock = mock_unlocked_vault();
    mock.expect_get_metadata().returning(|_| Ok(None));
    mock.expect_set_metadata().returning(|_, _| Ok(()));
    mock.expect_load_sync_status_map().returning(HashMap::new);
    mock.expect_list_all_stored_records()
        .returning(|| Ok(vec![]));
    mock.expect_list_record_health_states()
        .returning(|| Ok(vec![]));
    mock
}

// ---------------------------------------------------------------------------
// Health orchestration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_check_returns_started_from_mock_service() {
    let mut mock_health = MockHealth::new();
    mock_health
        .expect_check_hibp_single()
        .returning(|_| Ok(false));

    let mut executor = base_builder()
        .vault(Box::new(permissive_unlocked_vault()))
        .health(Arc::new(mock_health))
        .config(AppConfig::default())
        .build();

    let result = health::handle_run_health_check(&mut executor, true);
    assert!(matches!(result, CommandResult::HealthCheckStarted));
}

#[tokio::test]
async fn health_check_skips_when_frequency_gate_blocks() {
    let mut mock_vault = permissive_unlocked_vault();
    mock_vault
        .expect_list_all_stored_records()
        .returning(|| Ok(vec![]));

    let mut config = AppConfig::default();
    config.security.health_check_frequency = crate::config::security::HealthCheckFrequency::Daily;

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .health(Arc::new(MockHealth::new()))
        .config(config)
        .last_health_check_time(chrono::Utc::now())
        .build();

    let result = health::handle_run_health_check(&mut executor, false);
    assert!(matches!(result, CommandResult::HealthCheckSkipped));
}

// ---------------------------------------------------------------------------
// Cloud restore orchestration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cloud_restore_preflight_checks_metadata_via_sync_trait() {
    let mut mock_sync = MockSyncService::new();
    mock_sync.expect_download_metadata().once().returning(|| {
        Box::pin(async {
            let mut metadata = CloudMetadata::new("test-vault".to_string());
            metadata.upsert_record(
                "record-1".to_string(),
                crate::cloud::RecordVersionInfo {
                    version: 1,
                    updated_at: "2026-05-14T00:00:00Z".to_string(),
                    updated_by: "test-device".to_string(),
                    checksum: "checksum".to_string(),
                    deleted: false,
                },
            );
            Ok(Some(metadata))
        })
    });

    let result = sync::ensure_cloud_restore_has_records(&mut mock_sync).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn cloud_restore_preflight_rejects_empty_metadata() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_download_metadata()
        .once()
        .returning(|| Box::pin(async { Ok(Some(CloudMetadata::new("test".to_string()))) }));

    let result = sync::ensure_cloud_restore_has_records(&mut mock_sync).await;
    assert!(matches!(
        result,
        Err(CommandResult::Error {
            message_key: "error.cloud_restore_empty",
            ..
        })
    ));
}

#[tokio::test]
async fn cloud_restore_preflight_rejects_missing_metadata() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_download_metadata()
        .once()
        .returning(|| Box::pin(async { Ok(None) }));

    let result = sync::ensure_cloud_restore_has_records(&mut mock_sync).await;
    assert!(matches!(
        result,
        Err(CommandResult::Error {
            message_key: "error.cloud_restore_empty",
            ..
        })
    ));
}

#[tokio::test]
async fn cloud_restore_preflight_rejects_metadata_download_failure() {
    let mut mock_sync = MockSyncService::new();
    mock_sync.expect_download_metadata().once().returning(|| {
        Box::pin(async {
            Err(crate::errors::mapping::sync::SyncError::ProviderError {
                provider: "test".to_string(),
                message: "network error".to_string(),
            })
        })
    });

    let result = sync::ensure_cloud_restore_has_records(&mut mock_sync).await;
    assert!(matches!(
        result,
        Err(CommandResult::Error {
            message_key: "error.cloud_restore_failed",
            ..
        })
    ));
}

// ---------------------------------------------------------------------------
// Vault handler dispatch tests
// ---------------------------------------------------------------------------

#[test]
fn vault_lock_calls_trait_method() {
    let mut mock_vault = MockVault::new();
    mock_vault.expect_lock().once().returning(|| ());

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .config(AppConfig::default())
        .verified_master_password(SecureStr::new("cached".to_string()))
        .build();

    let result = vault::handle_lock(&mut executor);
    assert!(matches!(result, CommandResult::VaultLocked));
    assert!(executor.verified_master_password.is_none());
}

#[cfg(not(feature = "sqlcipher"))]
#[tokio::test]
async fn vault_unlock_calls_unlock_on_trait() {
    let mut mock_vault = MockVault::new();
    mock_vault.expect_unlock().once().returning(|_, _| Ok(()));
    mock_vault.expect_is_unlocked().returning(|| true);
    mock_vault
        .expect_get_last_health_check_at()
        .returning(|| Ok(None));

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .config(AppConfig::default())
        .build();

    let password = SecureStr::new("test-password".to_string());
    let result = vault::handle_unlock(&mut executor, password).await;
    assert!(matches!(result, CommandResult::VaultUnlocked));
}

// ---------------------------------------------------------------------------
// Rotation orchestration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trigger_rotation_with_mock_sync_pause_resume_flow() {
    let mut mock_sync = MockSyncService::new();

    // Rotation protocol: pause → download_metadata (CAS version)
    mock_sync
        .expect_pause()
        .once()
        .returning(|| Box::pin(async { Ok(()) }));
    mock_sync.expect_download_metadata().once().returning(|| {
        Box::pin(async {
            let mut meta = CloudMetadata::new("test-vault".to_string());
            meta.metadata_version = 1;
            Ok(Some(meta))
        })
    });

    // After rotation: download_metadata (for CAS push), push, resume
    mock_sync.expect_download_metadata().once().returning(|| {
        Box::pin(async {
            let mut meta = CloudMetadata::new("test-vault".to_string());
            meta.metadata_version = 1;
            Ok(Some(meta))
        })
    });
    mock_sync
        .expect_push_metadata_atomic()
        .once()
        .returning(|_, _| Box::pin(async { Ok(()) }));
    mock_sync
        .expect_resume()
        .once()
        .returning(|| Box::pin(async { Ok(()) }));

    // Rotation service needs: unlocked vault, DEK version, migration list,
    // checkpoint get/set, rotation config metadata, audit entry
    let mut mock_vault = MockVault::new();
    mock_vault.expect_is_unlocked().returning(|| true);
    mock_vault.expect_current_dek_version().returning(|| 1);
    mock_vault
        .expect_list_records_for_migration()
        .returning(|_| Ok(vec![]));
    mock_vault.expect_get_metadata().returning(|key| {
        match key {
            "rotation_config" => Ok(Some(r#"{"auto_rotate":true,"rotate_after_days":90,"rotate_after_records":1000,"current_dek_record_count":0}"#.to_string())),
            "rotation_checkpoint" => Ok(None),
            _ => Ok(None),
        }
    });
    mock_vault.expect_set_metadata().returning(|_, _| Ok(()));
    mock_vault.expect_delete_metadata().returning(|_| Ok(()));
    mock_vault
        .expect_write_audit_entry()
        .returning(|_, _, _, _| Ok(()));
    mock_vault.expect_log_dek_rotated().returning(|_| Ok(()));

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = rotation::handle_trigger_rotation(&mut executor).await;
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

#[tokio::test]
async fn trigger_rotation_fails_when_sync_pause_fails() {
    let mut mock_sync = MockSyncService::new();
    mock_sync.expect_pause().once().returning(|| {
        Box::pin(async {
            Err(crate::errors::mapping::sync::SyncError::ProviderError {
                provider: "test".to_string(),
                message: "pause failed".to_string(),
            })
        })
    });

    let mut executor = base_builder()
        .vault(Box::new(permissive_unlocked_vault()))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = rotation::handle_trigger_rotation(&mut executor).await;
    assert!(matches!(
        result,
        CommandResult::Error {
            message_key: "error.sync_pause_failed",
            ..
        }
    ));
}

#[tokio::test]
async fn trigger_rotation_fails_when_metadata_download_fails() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_pause()
        .once()
        .returning(|| Box::pin(async { Ok(()) }));
    mock_sync.expect_download_metadata().once().returning(|| {
        Box::pin(async {
            Err(crate::errors::mapping::sync::SyncError::ProviderError {
                provider: "test".to_string(),
                message: "download failed".to_string(),
            })
        })
    });
    mock_sync
        .expect_resume()
        .once()
        .returning(|| Box::pin(async { Ok(()) }));

    let mut executor = base_builder()
        .vault(Box::new(permissive_unlocked_vault()))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = rotation::handle_trigger_rotation(&mut executor).await;
    assert!(matches!(
        result,
        CommandResult::Error {
            message_key: "error.download_metadata_failed",
            ..
        }
    ));
}

#[tokio::test]
async fn resume_rotation_returns_no_trigger_when_no_checkpoint() {
    let mock_vault = permissive_unlocked_vault();

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .config(AppConfig::default())
        .build();

    let result = rotation::handle_resume_rotation(&mut executor).await;
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

#[test]
fn check_rotation_trigger_returns_correct_result() {
    let mut mock_vault = MockVault::new();
    mock_vault.expect_get_metadata().returning(|_| Ok(None));

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .config(AppConfig::default())
        .build();

    let result = rotation::handle_check_rotation_trigger(&mut executor);
    match &result {
        CommandResult::RotationTriggerChecked { should_rotate, .. } => {
            assert!(!should_rotate);
        }
        _ => panic!("expected RotationTriggerChecked, got {:?}", result),
    }
}

// ---------------------------------------------------------------------------
// Sync orchestration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trigger_sync_returns_error_when_not_configured() {
    let mut mock_vault = permissive_unlocked_vault();
    mock_vault
        .expect_list_all_stored_records()
        .returning(|| Ok(vec![]));

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_trigger_sync(&mut executor).await;
    assert!(matches!(
        result,
        CommandResult::Error {
            message_key: "error.sync_not_configured",
            ..
        }
    ));
}

#[tokio::test]
async fn trigger_sync_calls_sync_service() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_sync_with_cancel()
        .once()
        .returning(|_, _| {
            Box::pin(async {
                Ok(SyncResult {
                    report: SyncReport {
                        uploaded: 0,
                        downloaded: 0,
                        conflicts: 0,
                        failed: 0,
                        duration_ms: 0,
                    },
                    downloaded_health_states: vec![],
                    downloaded_health_deleted: vec![],
                    downloaded_records: vec![],
                    remote_metadata: None,
                })
            })
        });

    let mut mock_vault = permissive_unlocked_vault();
    mock_vault
        .expect_list_all_stored_records()
        .returning(|| Ok(vec![]));
    mock_vault
        .expect_load_sync_status_map()
        .returning(HashMap::new);
    mock_vault
        .expect_list_record_health_states()
        .returning(|| Ok(vec![]));

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_trigger_sync(&mut executor).await;
    match &result {
        CommandResult::SyncCompleted { stats } => {
            assert_eq!(stats.total, 0);
            assert_eq!(stats.conflicts, 0);
        }
        _ => panic!("expected SyncCompleted, got {:?}", result),
    }
}

#[tokio::test]
async fn resolve_conflict_returns_error_when_not_configured() {
    let mut executor = base_builder()
        .vault(Box::new(MockVault::new()))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_resolve_conflict(
        &mut executor,
        Uuid::new_v4(),
        crate::commands::types::ConflictResolution::KeepLocal,
    )
    .await;
    assert!(matches!(
        result,
        CommandResult::Error {
            message_key: "error.sync_not_configured",
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Sync cancellation dynamic tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn trigger_sync_maps_service_cancelled_to_command_cancelled() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_sync_with_cancel()
        .once()
        .returning(|_, _| {
            Box::pin(async {
                Err(crate::errors::mapping::sync::SyncError::Cancelled {
                    operation: "sync".to_string(),
                })
            })
        });

    let mut mock_vault = permissive_unlocked_vault();
    mock_vault
        .expect_list_all_stored_records()
        .returning(|| Ok(vec![]));
    mock_vault
        .expect_load_sync_status_map()
        .returning(HashMap::new);

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_trigger_sync(&mut executor).await;
    assert!(
        matches!(result, CommandResult::Cancelled { ref operation, .. } if operation == "sync"),
        "Expected Cancelled when service returns SyncError::Cancelled, got {:?}",
        result
    );
}

#[tokio::test]
async fn trigger_sync_maps_network_timeout_to_error() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_sync_with_cancel()
        .once()
        .returning(|_, _| {
            Box::pin(async {
                Err(crate::errors::mapping::sync::SyncError::NetworkTimeout {
                    message: "connection timed out".to_string(),
                })
            })
        });

    let mut mock_vault = permissive_unlocked_vault();
    mock_vault
        .expect_list_all_stored_records()
        .returning(|| Ok(vec![]));
    mock_vault
        .expect_load_sync_status_map()
        .returning(HashMap::new);

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_trigger_sync(&mut executor).await;
    assert!(
        matches!(
            result,
            CommandResult::Error {
                message_key: "error.sync_failed",
                ..
            }
        ),
        "Expected Error for network timeout, got {:?}",
        result
    );
}

#[tokio::test]
async fn trigger_sync_maps_auth_failure_to_error() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_sync_with_cancel()
        .once()
        .returning(|_, _| {
            Box::pin(async {
                Err(
                    crate::errors::mapping::sync::SyncError::AuthenticationFailed {
                        reason: "token expired".to_string(),
                    },
                )
            })
        });

    let mut mock_vault = permissive_unlocked_vault();
    mock_vault
        .expect_list_all_stored_records()
        .returning(|| Ok(vec![]));
    mock_vault
        .expect_load_sync_status_map()
        .returning(HashMap::new);

    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_trigger_sync(&mut executor).await;
    assert!(
        matches!(
            result,
            CommandResult::Error {
                message_key: "error.sync_failed",
                ..
            }
        ),
        "Expected Error for auth failure, got {:?}",
        result
    );
}

#[tokio::test]
async fn restore_database_from_cloud_returns_needs_oauth_without_sync() {
    let mut executor = base_builder()
        .vault(Box::new(MockVault::new()))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_restore_database_from_cloud(&mut executor, None).await;
    assert!(matches!(result, CommandResult::DatabaseRestoreNeedsOAuth));
}

// ---------------------------------------------------------------------------
// Import orchestration: partial vault write failure
// ---------------------------------------------------------------------------

#[test]
fn import_reports_mixed_success_and_failure_counts() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::commands::types::ImportSource;
    use crate::errors::mapping::vault::VaultError;
    use crate::executor::import_export;
    use crate::services::import_export::service::{ImportableRecord, MockImportExport};
    use crate::services::import_export::types::ImportResult;
    use crate::types::CredentialType;

    // ── Mock vault: create_record fails for 1st call, succeeds for 2nd+3rd ──
    let call_count = Arc::new(AtomicUsize::new(0));
    let mut mock_vault = MockVault::new();
    mock_vault.expect_is_unlocked().returning(|| true);

    let cc = Arc::clone(&call_count);
    mock_vault
        .expect_create_record()
        .times(3)
        .returning(move |_params| {
            let n = cc.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(VaultError::NotUnlocked)
            } else {
                Ok(Uuid::new_v4())
            }
        });

    mock_vault
        .expect_write_audit_entry()
        .returning(|_, _, _, _| Ok(()));

    // ── Mock import_export: returns 3 importable records ──
    let mut mock_ie = MockImportExport::new();
    mock_ie.expect_execute_import().once().returning(|_| {
        let result = ImportResult {
            imported: 3,
            reviewed: 0,
            skipped: 0,
            failed: 0,
            validation_failed: 0,
            duration_ms: 0,
        };
        let records = vec![
            ImportableRecord {
                credential_type: CredentialType::Login,
                fields: Default::default(),
                tags: vec![],
                is_review: false,
            },
            ImportableRecord {
                credential_type: CredentialType::Login,
                fields: Default::default(),
                tags: vec![],
                is_review: false,
            },
            ImportableRecord {
                credential_type: CredentialType::Login,
                fields: Default::default(),
                tags: vec![],
                is_review: false,
            },
        ];
        Ok((result, records))
    });

    // ── Build executor ──
    let mut executor = base_builder()
        .vault(Box::new(mock_vault))
        .import_export(Box::new(mock_ie))
        .config(AppConfig::default())
        .build();

    // ── Call handler with existing session_id (skips session creation) ──
    let result = import_export::handle_execute_import(
        &mut executor,
        Some(Uuid::new_v4()),
        ImportSource::Csv,
        std::path::PathBuf::from("fake.csv"),
        None,
        None,
        false,
    );

    match result {
        CommandResult::ImportCompleted {
            imported_count,
            reviewed_count,
            failed_count,
            ..
        } => {
            assert_eq!(imported_count, 2, "2 records should succeed");
            assert_eq!(reviewed_count, 0, "no review records");
            assert_eq!(failed_count, 1, "1 record should fail");
        }
        other => panic!("expected ImportCompleted, got {:?}", other),
    }
}

// ---------------------------------------------------------------------------
// Provider fault path: resolve_conflict error mapping
// ---------------------------------------------------------------------------

#[tokio::test]
async fn resolve_conflict_maps_provider_error_to_command_error() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_resolve_conflict()
        .once()
        .returning(|_, _| {
            Box::pin(async {
                Err(crate::errors::mapping::sync::SyncError::ProviderError {
                    provider: "test".to_string(),
                    message: "upload failed".to_string(),
                })
            })
        });

    let mut executor = base_builder()
        .vault(Box::new(MockVault::new()))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_resolve_conflict(
        &mut executor,
        Uuid::new_v4(),
        crate::commands::types::ConflictResolution::KeepLocal,
    )
    .await;
    assert!(
        matches!(
            result,
            CommandResult::Error {
                message_key: "error.conflict_resolve_failed",
                ..
            }
        ),
        "Expected Error for provider error during conflict resolution, got {:?}",
        result
    );
}

#[tokio::test]
async fn resolve_all_conflicts_maps_quota_error_to_command_error() {
    let mut mock_sync = MockSyncService::new();
    mock_sync
        .expect_resolve_all_conflicts()
        .once()
        .returning(|_| {
            Box::pin(async {
                Err(crate::errors::mapping::sync::SyncError::QuotaExceeded {
                    provider: "test".to_string(),
                })
            })
        });

    let mut executor = base_builder()
        .vault(Box::new(MockVault::new()))
        .sync(Some(Box::new(mock_sync)))
        .config(AppConfig::default())
        .build();

    let result = sync::handle_resolve_all_conflicts(
        &mut executor,
        crate::commands::types::ConflictResolution::KeepLocal,
    )
    .await;
    assert!(
        matches!(
            result,
            CommandResult::Error {
                message_key: "error.conflict_resolve_all_failed",
                ..
            }
        ),
        "Expected Error for quota exceeded during resolve_all, got {:?}",
        result
    );
}

// ---------------------------------------------------------------------------
// Builder field tests
// ---------------------------------------------------------------------------

#[test]
fn builder_sets_vault_db_file_backed_flag() {
    let executor = base_builder()
        .vault(Box::new(MockVault::new()))
        .vault_db_file_backed(true)
        .config(AppConfig::default())
        .build();

    assert!(executor.vault_db_file_backed);
}

#[test]
fn builder_sets_verified_master_password() {
    let executor = base_builder()
        .vault(Box::new(MockVault::new()))
        .config(AppConfig::default())
        .verified_master_password(SecureStr::new("test-password".to_string()))
        .build();

    assert!(executor.verified_master_password.is_some());
    assert_eq!(
        executor.verified_master_password.as_ref().unwrap().expose(),
        "test-password"
    );
}
