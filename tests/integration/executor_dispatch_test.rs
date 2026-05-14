use oak_keyring::commands::{Command, CommandResult, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::crypto::{argon2, xchacha20};
use oak_keyring::executor::CommandExecutor;
use oak_keyring::types::SecureStr;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn write_malformed_decryptable_okb(path: &std::path::Path, password: &SecureStr) {
    let salt = argon2::generate_salt();
    let dek = argon2::derive_key_locked(password, &salt, &argon2::Argon2Params::medium()).unwrap();
    let (ciphertext, nonce) = xchacha20::encrypt(b"not valid okb json", dek.expose()).unwrap();

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&salt);
    bytes.extend_from_slice(&nonce);
    bytes.extend_from_slice(&ciphertext);
    std::fs::write(path, bytes).unwrap();
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

    let executor =
        CommandExecutor::new(config, result_tx, cancel_token, data_dir, config_dir, false);
    assert!(executor.is_ok());
    assert!(!executor.unwrap().is_unlocked());
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
        false,
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
        false,
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
        false,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: std::path::PathBuf::new(),
            password: SecureStr::new("test-password".to_string()),
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
        false,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: std::path::PathBuf::from("/definitely/missing/backup.okb"),
            password: SecureStr::new("test-password".to_string()),
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
        true,
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
        true,
    )
    .unwrap();
    let handle = tokio::spawn(async move { executor.run(command_rx).await });

    command_tx
        .send(Command::RestoreDatabaseFromOkb {
            path: okb_path,
            password,
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
        true, // vault_has_key_only
    )
    .expect("executor should construct");

    assert!(
        !data_dir.join("vault.db").exists(),
        "vault.db must not be created when vault_has_key_only=true"
    );
}
