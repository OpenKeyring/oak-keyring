use oak_keyring::commands::{Command, CommandResult, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::{argon2, xchacha20};
use oak_keyring::executor::{CommandExecutor, DbStartupMode};
use oak_keyring::types::SecureStr;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn write_malformed_decryptable_okb(path: &std::path::Path, password: &SecureStr) {
    write_decryptable_okb_payload(path, password, b"not valid okb json");
}

fn write_decryptable_okb_payload(path: &std::path::Path, password: &SecureStr, payload: &[u8]) {
    let salt = argon2::generate_salt();
    let dek = argon2::derive_key_locked(password, &salt, &argon2::Argon2Params::medium()).unwrap();
    let (ciphertext, nonce) = xchacha20::encrypt(payload, dek.expose()).unwrap();

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&salt);
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&ciphertext);
    std::fs::write(path, bytes).unwrap();
}

fn empty_okb_payload() -> Vec<u8> {
    br#"{
        "version": "1.0",
        "vault_id": "550e8400-e29b-41d4-a716-446655440000",
        "exported_at": "2026-05-14T00:00:00Z",
        "records": []
    }"#
    .to_vec()
}

#[tokio::test]
async fn executor_can_be_constructed() {
    let (result_tx, _result_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();
    let tmp_dir = tempfile::tempdir().unwrap();
    // Create oak-keyring subdirectories (paths::data_dir() appends "oak-keyring")
    let data_dir = tmp_dir.path().join("oak-keyring");
    let config_dir = tmp_dir.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        config,
        result_tx,
        cancel_token,
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
    );
    assert!(executor.is_ok());
    assert!(!executor.unwrap().is_unlocked());
}

#[test]
fn executor_without_existing_vault_does_not_create_vault_db_on_startup() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let (result_tx, _result_rx) = mpsc::channel(64);
    let _executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .expect("executor should construct");

    assert!(
        !data_dir.join("vault.db").exists(),
        "startup with no key and no db must not create vault.db"
    );
}

#[tokio::test]
async fn executor_run_loop_processes_commands() {
    let (result_tx, _result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();
    let tmp_dir = tempfile::tempdir().unwrap();
    // Create oak-keyring subdirectories (paths::data_dir() appends "oak-keyring")
    let data_dir = tmp_dir.path().join("oak-keyring");
    let config_dir = tmp_dir.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        config,
        result_tx,
        cancel_token.clone(),
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
    )
    .unwrap();

    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    // Send a few commands
    command_tx.send(Command::LoadConfig).await.unwrap();
    command_tx.send(Command::LoadTags).await.unwrap();

    // Drop sender to close channel -> executor should stop
    drop(command_tx);

    let result = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
    assert!(result.is_ok(), "Executor should stop when channel closes");
}

#[tokio::test]
async fn dispatch_validate_recovery_words_rejects_non_24_words() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::ValidateRecoveryWords {
            words: vec!["abandon".to_string(); 12],
        })
        .await
        .unwrap();
    let msg = result_rx.recv().await.expect("result");
    let result = match msg {
        Message::CommandCompleted(result) => result,
        other => panic!("unexpected message: {other:?}"),
    };
    assert!(matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains("24")));
    drop(command_tx);
    handle.await.unwrap();
}

#[tokio::test]
async fn restore_database_from_okb_rejects_empty_path() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: std::path::PathBuf::new(),
            password: SecureStr::new("test-password".to_string()),
            master_password: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    let result = match msg {
        Message::CommandCompleted(result) => result,
        other => panic!("unexpected message: {other:?}"),
    };
    assert!(
        matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains(".okb")),
        "expected .okb path error, got: {:?}",
        result
    );
    drop(command_tx);
    handle.await.unwrap();
}

#[tokio::test]
async fn restore_database_from_okb_rejects_missing_file() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir,
        config_dir,
        DbStartupMode::FileBacked,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: std::path::PathBuf::from("/definitely/missing/backup.okb"),
            password: SecureStr::new("test-password".to_string()),
            master_password: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    let result = match msg {
        Message::CommandCompleted(result) => result,
        other => panic!("unexpected message: {other:?}"),
    };
    assert!(
        matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains("does not exist")),
        "expected missing file error, got: {:?}",
        result
    );
    drop(command_tx);
    handle.await.unwrap();
}

#[tokio::test]
async fn restore_database_from_okb_wrong_password_does_not_create_vault_db() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: std::path::Path::new("tests/data")
                .join("okb_basic.okb")
                .canonicalize()
                .unwrap(),
            password: SecureStr::new("wrong-password".to_string()),
            master_password: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    let result = match msg {
        Message::CommandCompleted(result) => result,
        other => panic!("unexpected message: {other:?}"),
    };
    assert!(
        matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains("Failed to decrypt")),
        "expected decrypt error, got: {:?}",
        result
    );
    assert!(
        !data_dir.join("vault.db").exists(),
        "vault.db must not be created before .okb decrypt succeeds"
    );
    drop(command_tx);
    handle.await.unwrap();
}

#[tokio::test]
async fn restore_database_from_malformed_okb_does_not_create_vault_db() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let password = SecureStr::new("valid-password".to_string());
    let okb_path = temp.path().join("malformed.okb");
    write_malformed_decryptable_okb(&okb_path, &password);

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: okb_path,
            password,
            master_password: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    let result = match msg {
        Message::CommandCompleted(result) => result,
        other => panic!("unexpected message: {other:?}"),
    };
    assert!(
        matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains("Failed to decrypt .okb backup")),
        "expected parse error, got: {:?}",
        result
    );
    assert!(
        !data_dir.join("vault.db").exists(),
        "vault.db must not be created before .okb parses successfully"
    );
    drop(command_tx);
    handle.await.unwrap();
}

#[tokio::test]
async fn restore_database_from_empty_okb_does_not_create_vault_db() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let password = SecureStr::new("valid-password".to_string());
    let okb_path = temp.path().join("empty.okb");
    write_decryptable_okb_payload(&okb_path, &password, &empty_okb_payload());

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: okb_path,
            password,
            master_password: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    let result = match msg {
        Message::CommandCompleted(result) => result,
        other => panic!("unexpected message: {other:?}"),
    };
    assert!(
        matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains("No records")),
        "expected empty backup error, got: {:?}",
        result
    );
    assert!(
        !data_dir.join("vault.db").exists(),
        "vault.db must not be created when .okb contains no records"
    );
    drop(command_tx);
    handle.await.unwrap();
}

#[tokio::test]
async fn restore_database_from_okb_without_cached_master_password_does_not_create_vault_db() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: std::path::Path::new("tests/data")
                .join("okb_basic.okb")
                .canonicalize()
                .unwrap(),
            password: SecureStr::new("test-password".to_string()),
            master_password: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    let result = match msg {
        Message::CommandCompleted(result) => result,
        other => panic!("unexpected message: {other:?}"),
    };
    assert!(
        matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains("Master password")),
        "expected missing master password error, got: {:?}",
        result
    );
    assert!(
        !data_dir.join("vault.db").exists(),
        "vault.db must not be created before restore has a cached master password"
    );
    drop(command_tx);
    handle.await.unwrap();
}

#[test]
fn executor_with_key_only_does_not_create_vault_db() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let (result_tx, _result_rx) = mpsc::channel(64);
    let _executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .expect("executor should construct");

    assert!(
        !data_dir.join("vault.db").exists(),
        "vault.db must not be created during deferred in-memory startup"
    );
}

#[test]
fn executor_with_empty_vault_state_does_not_create_vault_db() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring-config");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let (result_tx, _result_rx) = tokio::sync::mpsc::channel(64);
    let _executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .expect("executor should construct without creating vault.db");

    assert!(
        !data_dir.join("vault.db").exists(),
        "empty first startup must not create vault.db before onboarding initializes the vault"
    );
}

#[tokio::test]
async fn initialize_vault_creates_file_backed_database_after_empty_startup() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring-config");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::InitializeVault {
            master_password: SecureStr::new("correct horse battery staple".to_string()),
            recovery_words: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    match msg {
        Message::CommandCompleted(CommandResult::VaultInitialized { .. }) => {}
        other => panic!("expected VaultInitialized, got {other:?}"),
    }

    assert!(
        data_dir.join("vault.db").exists(),
        "new-vault initialization must explicitly create the file-backed database"
    );

    drop(command_tx);
    handle.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn initialize_vault_db_failure_removes_new_key_file() {
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(64);
    let (command_tx, command_rx) = tokio::sync::mpsc::channel(64);
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring-config");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    let missing_target = temp.path().join("missing-parent").join("vault.db");
    std::os::unix::fs::symlink(&missing_target, data_dir.join("vault.db")).unwrap();
    assert!(!data_dir.join("vault.db").exists());
    assert!(!data_dir.join("wrapped_secret_key.json").exists());

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        DbStartupMode::DeferredInMemory,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::InitializeVault {
            master_password: SecureStr::new("correct horse battery staple".to_string()),
            recovery_words: None,
        })
        .await
        .unwrap();

    let msg = result_rx.recv().await.expect("result");
    match msg {
        Message::CommandCompleted(CommandResult::Error { .. }) => {}
        other => panic!("expected database reopen error, got {other:?}"),
    }

    assert!(
        !data_dir.join("wrapped_secret_key.json").exists(),
        "failed DB creation must not leave a newly created key file behind"
    );
    assert!(
        std::fs::symlink_metadata(data_dir.join("vault.db")).is_ok(),
        "failed DB creation must preserve pre-existing vault.db artifacts"
    );

    drop(command_tx);
    handle.await.unwrap();
}
