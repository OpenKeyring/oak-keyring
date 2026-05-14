use oak_keyring::commands::{Command, CommandResult, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::executor::CommandExecutor;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

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

    let executor = CommandExecutor::new(config, result_tx, cancel_token, data_dir, config_dir);
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
    let vault_dir = temp.path().to_path_buf();
    std::env::set_var("OAK_VAULT_DIR", &vault_dir);
    std::env::set_var("OAK_CONFIG_DIR", &vault_dir);

    let executor = CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        CancellationToken::new(),
        oak_keyring::executor::DbStartupMode::FileBacked,
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
    assert!(
        matches!(&result, CommandResult::Error { fallback, .. } if fallback.contains("24"))
    );
    drop(command_tx);
    handle.await.unwrap();
}
