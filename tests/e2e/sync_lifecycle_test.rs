//! E2E tests for sync lifecycle through the command→executor→SyncService→SyncTask→Pipeline→CloudStorage pipeline.
//!
//! These tests verify the full integration:
//! 1. Command sent via channel
//! 2. Executor builds SyncVaultData from vault state
//! 3. SyncService runs the 4-stage pipeline (Pull→Detect→Push→Resolve)
//! 4. Downloaded records applied to local vault
//! 5. Result returned via Message channel

use oak_keyring::cloud::metadata::{serialize_metadata, CloudMetadata};
use oak_keyring::cloud::schema::METADATA_FILENAME;
use oak_keyring::commands::types::{
    ConflictResolution, RecordFilter, RecordSort, DEFAULT_RECORD_LIST_PAGE_SIZE,
};
use oak_keyring::commands::{Command, CommandResult, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::errors::ErrorCode;
use oak_keyring::executor::{ActivityTracker, CommandExecutor, DbStartupMode};
use oak_keyring::services::clipboard::ClipboardService;
use oak_keyring::services::sync::SyncService;
use oak_keyring::services::sync::SyncServiceImpl;
use oak_keyring::services::vault::VaultServiceImpl;
use oak_keyring::types::credential::EncryptedPayload;
use oak_keyring::types::sensitive::SecureStr;
use oak_keyring::types::SyncStatus;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const TIMEOUT: Duration = Duration::from_secs(10);

fn synthetic_credential_value() -> String {
    Uuid::new_v4().to_string()
}

/// Holds the cloud storage temp dir alive for the lifetime of the test.
struct SyncTestContext {
    #[allow(dead_code)]
    cloud_dir: TempDir,
    command_tx: mpsc::Sender<Command>,
    result_rx: mpsc::Receiver<Message>,
}

fn create_fs_sync_service() -> (SyncServiceImpl, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let op = opendal::Operator::new(
        opendal::services::Fs::default().root(temp_dir.path().to_str().unwrap()),
    )
    .unwrap()
    .finish();
    let storage = oak_keyring::cloud::CloudStorage::new(op, "fs".to_string());
    (SyncServiceImpl::new(storage), temp_dir)
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

    let executor = CommandExecutor::new(
        config,
        result_tx,
        cancel_token,
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
        ActivityTracker::new(),
    )
    .expect("executor construction should succeed");

    tokio::spawn(async move {
        executor
            .run(command_rx)
            .await
            .expect("executor run should succeed");
    });

    (command_tx, result_rx)
}

/// Build a CommandExecutor with a custom SyncService via ExecutorBuilder.
fn build_executor_with_sync(
    config: AppConfig,
    result_tx: mpsc::Sender<Message>,
    cancel_token: CancellationToken,
    data_dir: std::path::PathBuf,
    config_dir: std::path::PathBuf,
    db_mode: DbStartupMode,
    sync: Option<Box<dyn SyncService>>,
) -> CommandExecutor {
    use oak_keyring::db::schema::init_db_in_memory;

    let (vault_runtime, vault_db_file_backed) = match db_mode {
        #[cfg(feature = "sqlcipher")]
        DbStartupMode::FileBacked => {
            // SQLCipher production: start locked. The test will send
            // InitializeVault or UnlockVault to open the encrypted DB.
            (
                oak_keyring::executor::runtime::VaultRuntime::locked(),
                false,
            )
        }
        #[cfg(not(feature = "sqlcipher"))]
        DbStartupMode::FileBacked => {
            use oak_keyring::db::schema::init_db;
            let conn = init_db(&data_dir).expect("db init should succeed");
            (
                oak_keyring::executor::runtime::VaultRuntime::open(Box::new(
                    VaultServiceImpl::new(conn),
                )),
                true,
            )
        }
        DbStartupMode::DeferredInMemory => {
            let conn = init_db_in_memory().unwrap();
            (
                oak_keyring::executor::runtime::VaultRuntime::open(Box::new(
                    VaultServiceImpl::new(conn),
                )),
                false,
            )
        }
    };
    let clipboard = Arc::new(
        ClipboardService::new_safe(config.general.clipboard_clear_seconds)
            .expect("clipboard should initialize"),
    );

    CommandExecutor::builder(data_dir, config_dir)
        .vault_runtime(vault_runtime)
        .vault_db_file_backed(vault_db_file_backed)
        .sync(sync)
        .config(config)
        .result_tx(result_tx)
        .shutdown_token(cancel_token)
        .clipboard(clipboard)
        .activity(ActivityTracker::new())
        .build()
        .expect("executor should build")
}

async fn setup_sync_executor(vault_dir: &TempDir) -> SyncTestContext {
    let data_dir = vault_dir.path().join("oak-keyring");
    let config_dir = vault_dir.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    let (result_tx, result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();

    let (sync, cloud_dir) = create_fs_sync_service();

    let executor = build_executor_with_sync(
        config,
        result_tx,
        cancel_token,
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
        Some(Box::new(sync)),
    );

    tokio::spawn(async move {
        executor
            .run(command_rx)
            .await
            .expect("executor run should succeed");
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

async fn setup_key_only_sync_executor(vault_dir: &TempDir) -> SyncTestContext {
    let data_dir = vault_dir.path().join("oak-keyring");
    let config_dir = vault_dir.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    let (result_tx, result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();

    let (sync, cloud_dir) = create_fs_sync_service();

    let executor = build_executor_with_sync(
        config,
        result_tx,
        cancel_token,
        data_dir,
        config_dir,
        DbStartupMode::DeferredInMemory,
        Some(Box::new(sync)),
    );

    tokio::spawn(async move {
        executor
            .run(command_rx)
            .await
            .expect("executor run should succeed");
    });

    SyncTestContext {
        cloud_dir,
        command_tx,
        result_rx,
    }
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
        CommandResult::VaultInitialized => {}
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
    create_login_record_with_tags(command_tx, result_rx, name, username, password, vec![]).await
}

async fn create_login_record_with_tags(
    command_tx: &mpsc::Sender<Command>,
    result_rx: &mut mpsc::Receiver<Message>,
    name: &str,
    username: &str,
    password: &str,
    tags: Vec<String>,
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
            tags,
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

fn read_uploaded_cloud_record_json(ctx: &SyncTestContext) -> String {
    let records_dir = ctx.cloud_dir.path().join("records");
    let mut entries = std::fs::read_dir(&records_dir)
        .expect("records dir should exist")
        .collect::<Result<Vec<_>, _>>()
        .expect("record entries should be readable");
    entries.sort_by_key(|entry| entry.path());
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one uploaded cloud record"
    );
    std::fs::read_to_string(entries[0].path()).expect("cloud record should be readable")
}

async fn rebuild_keyfile_from_recovery(ctx: &mut SyncTestContext) {
    let passkey = Passkey::generate(24, MnemonicLanguage::English).expect("passkey");
    ctx.command_tx
        .send(Command::RebuildKeyFileFromRecovery {
            master_password: SecureStr::new("test_password_123".to_string()),
            recovery_words: passkey.to_recovery_words().expect("recovery words"),
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    match result {
        CommandResult::KeyFileRebuilt => {}
        other => panic!("Expected KeyFileRebuilt, got {:?}", other),
    }
}

fn write_empty_cloud_metadata(ctx: &SyncTestContext) {
    let metadata = CloudMetadata::new("test-vault-token".to_string());
    let json = serialize_metadata(&metadata).expect("serialize metadata");
    std::fs::write(ctx.cloud_dir.path().join(METADATA_FILENAME), json).unwrap();
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

async fn drain_background_results(result_rx: &mut mpsc::Receiver<Message>) {
    loop {
        match tokio::time::timeout(Duration::from_millis(100), result_rx.recv()).await {
            Ok(Some(Message::CommandCompleted(
                CommandResult::HealthCheckStarted
                | CommandResult::HealthCheckCompleted { .. }
                | CommandResult::HealthCheckSkipped,
            ))) => continue,
            Ok(Some(_)) => continue,
            Ok(None) | Err(_) => break,
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

#[tokio::test]
async fn restore_database_from_empty_cloud_does_not_create_vault_db() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let data_dir = vault_dir.path().join("oak-keyring");
    let mut ctx = setup_key_only_sync_executor(&vault_dir).await;

    rebuild_keyfile_from_recovery(&mut ctx).await;

    ctx.command_tx
        .send(Command::RestoreDatabaseFromCloud {
            master_password: None,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    assert!(
        matches!(
            &result,
            CommandResult::Error { fallback, .. }
                if fallback.contains("No recoverable cloud sync data")
        ),
        "Expected empty-cloud restore error, got {:?}",
        result
    );
    assert!(
        !data_dir.join("vault.db").exists(),
        "empty cloud restore must not create vault.db"
    );
}

#[tokio::test]
async fn restore_database_from_cloud_metadata_without_records_does_not_create_vault_db() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let data_dir = vault_dir.path().join("oak-keyring");
    let mut ctx = setup_key_only_sync_executor(&vault_dir).await;
    write_empty_cloud_metadata(&ctx);
    rebuild_keyfile_from_recovery(&mut ctx).await;

    ctx.command_tx
        .send(Command::RestoreDatabaseFromCloud {
            master_password: None,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    assert!(
        matches!(
            &result,
            CommandResult::Error { fallback, .. }
                if fallback.contains("No recoverable cloud sync data")
        ),
        "Expected empty-cloud restore error, got {:?}",
        result
    );
    assert!(
        !data_dir.join("vault.db").exists(),
        "cloud metadata without records must not create vault.db"
    );
}

// ==================== Test 3: Upload Pending Records ====================

#[tokio::test]
async fn sync_uploads_pending_records_to_cloud() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;
    let first_credential = synthetic_credential_value();
    let second_credential = synthetic_credential_value();

    create_login_record(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Test Site",
        "user@example.com",
        &first_credential,
    )
    .await;
    create_login_record(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Another Site",
        "admin@example.com",
        &second_credential,
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

    ctx.command_tx
        .send(Command::LoadRecordList {
            filter: RecordFilter::All,
            sort: RecordSort::default(),
            limit: DEFAULT_RECORD_LIST_PAGE_SIZE,
            offset: 0,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    match result {
        CommandResult::RecordListLoaded { records, .. } => {
            assert_eq!(records.len(), 2);
            assert!(
                records
                    .iter()
                    .all(|record| record.sync_status == Some(SyncStatus::Synced)),
                "uploaded records must be marked Synced locally"
            );
        }
        other => panic!("Expected RecordListLoaded, got {:?}", other),
    }
}

#[tokio::test]
async fn sync_upload_encrypts_private_cloud_metadata() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;
    let credential = synthetic_credential_value();

    create_login_record_with_tags(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Sensitive Payroll",
        "payroll@example.com",
        &credential,
        vec!["finance-secret".to_string()],
    )
    .await;

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    assert!(
        matches!(result, CommandResult::SyncCompleted { .. }),
        "Expected SyncCompleted, got {:?}",
        result
    );

    let cloud_json = read_uploaded_cloud_record_json(&ctx);
    assert!(
        cloud_json.contains("encrypted_metadata"),
        "cloud record should carry encrypted private metadata"
    );
    assert!(
        !cloud_json.contains("Sensitive Payroll"),
        "record name must not be plaintext in cloud record"
    );
    assert!(
        !cloud_json.contains("finance-secret"),
        "tags must not be plaintext in cloud record"
    );
    assert!(
        !cloud_json.contains("\"credential_type\""),
        "credential type must not be plaintext in cloud record"
    );
}

#[tokio::test]
async fn sync_download_restores_encrypted_private_cloud_metadata() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;
    let credential = synthetic_credential_value();

    let record_id = create_login_record_with_tags(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Download Secret",
        "download@example.com",
        &credential,
        vec!["restore-private".to_string()],
    )
    .await;

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");
    assert!(
        matches!(
            recv_command_result(&mut ctx.result_rx).await,
            CommandResult::SyncCompleted { .. }
        ),
        "initial upload should complete"
    );

    ctx.command_tx
        .send(Command::HardDeleteRecord { id: record_id })
        .await
        .expect("send should succeed");
    assert!(
        matches!(
            recv_command_result(&mut ctx.result_rx).await,
            CommandResult::RecordDestroyed { .. }
        ),
        "local hard delete should complete"
    );
    drain_background_results(&mut ctx.result_rx).await;

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");
    let second_sync = recv_command_result(&mut ctx.result_rx).await;
    assert!(
        matches!(second_sync, CommandResult::SyncCompleted { .. }),
        "remote-only download should complete, got {:?}",
        second_sync
    );

    ctx.command_tx
        .send(Command::LoadRecordList {
            filter: RecordFilter::All,
            sort: RecordSort::default(),
            limit: DEFAULT_RECORD_LIST_PAGE_SIZE,
            offset: 0,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    match result {
        CommandResult::RecordListLoaded { records, .. } => {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].name, "Download Secret");
            assert_eq!(records[0].tags, vec!["restore-private".to_string()]);
            assert_eq!(records[0].sync_status, Some(SyncStatus::Synced));
        }
        other => panic!("Expected RecordListLoaded, got {:?}", other),
    }
}

#[tokio::test]
async fn sync_uploads_soft_deleted_records_as_cloud_tombstones() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;
    let credential = synthetic_credential_value();

    let record_id = create_login_record_with_tags(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Delete Me",
        "delete@example.com",
        &credential,
        vec!["delete-sync".to_string()],
    )
    .await;

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");
    assert!(
        matches!(
            recv_command_result(&mut ctx.result_rx).await,
            CommandResult::SyncCompleted { .. }
        ),
        "initial upload should complete"
    );

    ctx.command_tx
        .send(Command::SoftDeleteRecord { id: record_id })
        .await
        .expect("send should succeed");
    assert!(
        matches!(
            recv_command_result(&mut ctx.result_rx).await,
            CommandResult::RecordDeleted { .. }
        ),
        "soft delete should complete"
    );
    drain_background_results(&mut ctx.result_rx).await;

    ctx.command_tx
        .send(Command::TriggerSync)
        .await
        .expect("send should succeed");
    assert!(
        matches!(
            recv_command_result(&mut ctx.result_rx).await,
            CommandResult::SyncCompleted { .. }
        ),
        "deleted-state upload should complete"
    );

    let cloud_json = read_uploaded_cloud_record_json(&ctx);
    let cloud_record: oak_keyring::cloud::CloudRecord =
        serde_json::from_str(&cloud_json).expect("cloud record should parse");
    assert_eq!(cloud_record.id, record_id.to_string());
    assert_eq!(cloud_record.deleted, Some(true));
    assert!(
        cloud_record.deleted_at.is_some(),
        "soft-deleted cloud record should carry deleted_at"
    );
    assert!(
        cloud_record.metadata.encrypted_metadata.is_some(),
        "soft-deleted uploads should keep private metadata encrypted"
    );
}

// ==================== Test 6: Locked Vault Skips Uploads ====================

#[tokio::test]
async fn sync_with_locked_vault_completes_without_uploads() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let mut ctx = setup_unlocked_sync_executor(&vault_dir).await;
    let credential = synthetic_credential_value();

    create_login_record(
        &ctx.command_tx,
        &mut ctx.result_rx,
        "Locked Site",
        "locked@example.com",
        &credential,
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

    let (sync, _cloud_dir) = create_fs_sync_service();

    let executor = build_executor_with_sync(
        config,
        result_tx,
        cancel_token,
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
        Some(Box::new(sync)),
    );

    let op_cancel = executor.cancel_token().clone();

    tokio::spawn(async move {
        executor
            .run(command_rx)
            .await
            .expect("executor run should succeed");
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

// ==================== Test 9: Cloud Restore Happy Path ====================

#[tokio::test]
async fn restore_database_from_cloud_with_valid_records_creates_vault_db() {
    use oak_keyring::cloud::metadata::serialize_metadata;
    use oak_keyring::cloud::{CloudRecord, RecordVersionInfo};

    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let data_dir = vault_dir.path().join("oak-keyring");

    // Set up a key-only executor with a FS-backed cloud storage.
    let mut ctx = setup_key_only_sync_executor(&vault_dir).await;
    rebuild_keyfile_from_recovery(&mut ctx).await;

    // Write valid cloud metadata + a valid record directly to cloud storage.
    let record_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let nonce_b64 = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode([0u8; 24])
    };
    let encrypted_data_b64 = {
        use base64::Engine;
        // 32 bytes of dummy ciphertext (passes base64 decode, not real encryption)
        base64::engine::general_purpose::STANDARD.encode([0xABu8; 48])
    };
    let record = CloudRecord {
        id: record_id.clone(),
        version: 1,
        encrypted_data: encrypted_data_b64.clone(),
        nonce: nonce_b64,
        dek_version: 1,
        aad: oak_keyring::cloud::AadFields {
            record_id: record_id.clone(),
            dek_version: 1,
        },
        metadata: oak_keyring::cloud::RecordMetadata {
            name: "Test Record".to_string(),
            tags: vec!["test".to_string()],
            updated_at: now.clone(),
            health: None,
            ..Default::default()
        },
        deleted: None,
        deleted_at: None,
    };
    let checksum = record.compute_checksum().expect("checksum");

    let mut metadata = CloudMetadata::new("test-vault-token".to_string());
    metadata.upsert_record(
        record_id.clone(),
        RecordVersionInfo {
            version: 1,
            updated_at: now,
            updated_by: "test-device".to_string(),
            checksum,
            private_metadata_checksum: record.compute_private_metadata_checksum().unwrap(),
            deleted: false,
        },
    );

    // Write metadata and record to cloud storage.
    let metadata_json = serialize_metadata(&metadata).expect("serialize metadata");
    std::fs::write(ctx.cloud_dir.path().join(METADATA_FILENAME), metadata_json).unwrap();
    let records_dir = ctx.cloud_dir.path().join("records");
    std::fs::create_dir_all(&records_dir).unwrap();
    let record_json = serde_json::to_string_pretty(&record).expect("serialize record");
    std::fs::write(records_dir.join(format!("{record_id}.json")), record_json).unwrap();

    // Trigger restore from cloud.
    ctx.command_tx
        .send(Command::RestoreDatabaseFromCloud {
            master_password: None,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut ctx.result_rx).await;
    assert!(
        matches!(
            &result,
            CommandResult::DatabaseRestored {
                source: oak_keyring::commands::types::DatabaseRecoverySource::Cloud
            }
        ),
        "expected successful cloud restore, got: {:?}",
        result
    );
    assert!(
        data_dir.join("vault.db").exists(),
        "vault.db must exist after successful cloud restore"
    );
}
