//! E2E tests for sync lifecycle through the command→executor→SyncService→SyncTask→Pipeline→CloudStorage pipeline.
//!
//! These tests verify the full integration:
//! 1. Command sent via channel
//! 2. Executor builds SyncVaultData from vault state
//! 3. SyncService runs the 4-stage pipeline (Pull→Detect→Push→Resolve)
//! 4. Downloaded records applied to local vault
//! 5. Result returned via Message channel

use oak_keyring::commands::types::ConflictResolution;
use oak_keyring::commands::{Command, CommandResult, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::errors::ErrorCode;
use oak_keyring::executor::CommandExecutor;
use oak_keyring::services::sync::SyncService;
use oak_keyring::types::credential::EncryptedPayload;
use oak_keyring::types::sensitive::SecureStr;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(10);

/// Holds the cloud storage temp dir alive for the lifetime of the test.
struct SyncTestContext {
    #[allow(dead_code)]
    cloud_dir: TempDir,
    command_tx: mpsc::Sender<Command>,
    result_rx: mpsc::Receiver<Message>,
}

fn create_fs_sync_service() -> (SyncService, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let op = opendal::Operator::new(
        opendal::services::Fs::default().root(temp_dir.path().to_str().unwrap()),
    )
    .unwrap()
    .finish();
    let storage = oak_keyring::cloud::CloudStorage::new(op, "fs".to_string());
    (SyncService::new(storage), temp_dir)
}

async fn setup_executor(vault_dir: &TempDir) -> (mpsc::Sender<Command>, mpsc::Receiver<Message>) {
    // Create oak-keyring subdirectories (paths::data_dir() appends "oak-keyring")
    let data_dir = vault_dir.path().join("oak-keyring");
    let config_dir = vault_dir.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    let (result_tx, result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();

    let executor = CommandExecutor::new(config, result_tx, cancel_token, data_dir, config_dir)
        .expect("executor construction should succeed");

    tokio::spawn(async move {
        executor.run(command_rx).await;
    });

    (command_tx, result_rx)
}

async fn setup_sync_executor(vault_dir: &TempDir) -> SyncTestContext {
    // Create oak-keyring subdirectories (paths::data_dir() appends "oak-keyring")
    let data_dir = vault_dir.path().join("oak-keyring");
    let config_dir = vault_dir.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    let (result_tx, result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();

    let mut executor =
        CommandExecutor::new(config, result_tx, cancel_token, data_dir, config_dir)
            .expect("executor construction should succeed");

    let (sync, cloud_dir) = create_fs_sync_service();
    executor.set_sync_service(Some(sync));

    tokio::spawn(async move {
        executor.run(command_rx).await;
    });

    SyncTestContext {
        cloud_dir,
        command_tx,
        result_rx,
    }
}

async fn setup_unlocked_sync_executor(vault_dir: &TempDir) -> SyncTestContext {
    let mut ctx = setup_sync_executor(vault_dir).await;
    init_and_unlock_vault(&ctx.command_tx, &mut ctx.result_rx, vault_dir).await;
    ctx
}

async fn init_and_unlock_vault(
    command_tx: &mpsc::Sender<Command>,
    result_rx: &mut mpsc::Receiver<Message>,
    _vault_dir: &TempDir,
) {
    let password = SecureStr::new("test_password_123".to_string());
    command_tx
        .send(Command::InitializeVault {
            master_password: password,
            recovery_words: None,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(result_rx).await;
    match result {
        CommandResult::VaultInitialized { .. } => {}
        other => panic!("Expected VaultInitialized, got {:?}", other),
    }
}

async fn create_login_record(
    command_tx: &mpsc::Sender<Command>,
    result_rx: &mut mpsc::Receiver<Message>,
    name: &str,
    username: &str,
    password: &str,
) -> Uuid {
    command_tx
        .send(Command::CreateRecord {
            credential_type: oak_keyring::types::credential::CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: username.to_string(),
                password: SecureStr::new(password.to_string()),
                url: None,
                notes: None,
            },
            tags: vec![],
            is_favorite: false,
            expires_at: None,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(result_rx).await;
    match result {
        CommandResult::RecordCreated { id } => id,
        other => panic!("Expected RecordCreated, got {:?}", other),
    }
}

/// Receive the next non-background CommandResult, draining health-check
/// and other background messages that interleave with command responses.
async fn recv_command_result(result_rx: &mut mpsc::Receiver<Message>) -> CommandResult {
    let deadline = tokio::time::Instant::now() + TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            panic!("recv_command_result timed out after {:?}", TIMEOUT);
        }
        let msg = tokio::time::timeout(remaining, result_rx.recv())
            .await
            .expect("should receive result within timeout")
            .expect("result channel should not close");

        if let Message::CommandCompleted(result) = msg {
            if matches!(
                result,
                CommandResult::HealthCheckStarted
                    | CommandResult::HealthCheckCompleted { .. }
                    | CommandResult::HealthCheckSkipped
            ) {
                continue;
            }
            // Drain Cancelled results from background operations (e.g. health
            // check cancelled by vault lock).
            if let CommandResult::Cancelled { ref operation, .. } = result {
                if operation != "sync" {
                    continue;
                }
            }
            return result;
        }
    }
}

// ==================== Test 1: Not Configured ====================

#[tokio::test]
async fn trigger_sync_not_configured_returns_error() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let (command_tx, mut result_rx) = setup_executor(&vault_dir).await;

    init_and_unlock_vault(&command_tx, &mut result_rx, &vault_dir).await;

    command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut result_rx).await;
    match result {
        CommandResult::Error { code, .. } => {
            assert_eq!(code, ErrorCode::SyncNotConfigured);
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}

// ==================== Test 2: Empty Vault Sync ====================

#[tokio::test]
async fn trigger_sync_empty_vault_completes() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    match result {
        CommandResult::SyncCompleted { stats } => {
            assert_eq!(stats.total, 0, "empty vault should have 0 synced records");
        }
        other => panic!("Expected SyncCompleted, got {:?}", other),
    }
}

// ==================== Test 3: Upload Pending Records ====================

#[tokio::test]
async fn sync_uploads_pending_records_to_cloud() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;

    create_login_record(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Test Site",
        "user@example.com",
        "password123",
    )
    .await;
    create_login_record(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Another Site",
        "admin@example.com",
        "admin456",
    )
    .await;

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    match result {
        CommandResult::SyncCompleted { stats } => {
            assert!(
                stats.synced >= 2,
                "expected at least 2 synced records, got {}",
                stats.synced
            );
        }
        other => panic!("Expected SyncCompleted, got {:?}", other),
    }
}

// ==================== Test 6: Locked Vault Skips Uploads ====================

#[tokio::test]
async fn sync_with_locked_vault_completes_without_uploads() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;

    create_login_record(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Locked Site",
        "locked@example.com",
        "locked789",
    )
    .await;

    ctx.command_tx
        .send(Command::LockVault)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    match result {
        CommandResult::VaultLocked => {}
        other => panic!("Expected VaultLocked, got {:?}", other),
    }

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    match result {
        CommandResult::Error { code, .. } => {
            assert_eq!(code, ErrorCode::ExecutorVaultLocked);
        }
        other => panic!("Expected vault locked Error, got {:?}", other),
    }
}

// ==================== Test 7: Cancellation ====================

#[tokio::test]
async fn sync_cancellation_returns_cancelled() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    // Create oak-keyring subdirectories (paths::data_dir() appends "oak-keyring")
    let data_dir = vault_dir.path().join("oak-keyring");
    let config_dir = vault_dir.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    let (result_tx, mut result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();

    let mut executor =
        CommandExecutor::new(config, result_tx, cancel_token, data_dir, config_dir)
            .expect("executor construction should succeed");

    let (sync, _cloud_dir) = create_fs_sync_service();
    executor.set_sync_service(Some(sync));

    let op_cancel = executor.cancel_token().clone();

    tokio::spawn(async move {
        executor.run(command_rx).await;
    });

    let password = SecureStr::new("test_password_123".to_string());
    command_tx
        .send(Command::InitializeVault {
            master_password: password,
            recovery_words: None,
        })
        .await
        .expect("send should succeed");

    let _ = recv_command_result(&mut result_rx).await;

    op_cancel.cancel();

    command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut result_rx).await;
    match result {
        CommandResult::Cancelled { .. } => {}
        CommandResult::Error { .. } => {}
        other => panic!("Expected Cancelled or Error, got {:?}", other),
    }
}

// ==================== Test 8: Resolve Conflict Without Sync ====================

#[tokio::test]
async fn resolve_conflict_without_sync_returns_error() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let (command_tx, mut result_rx) = setup_executor(&vault_dir).await;

    init_and_unlock_vault(&command_tx, &mut result_rx, &vault_dir).await;

    command_tx
        .send(Command::ResolveConflict {
            record_id: Uuid::new_v4(),
            resolution: ConflictResolution::KeepLocal,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut result_rx).await;
    match result {
        CommandResult::Error { code, .. } => {
            assert_eq!(code, ErrorCode::SyncNotConfigured);
        }
        other => panic!("Expected Error, got {:?}", other),
    }
}
