use oak_keyring::commands::Command;
use oak_keyring::config::AppConfig;
use oak_keyring::executor::CommandExecutor;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn executor_can_be_constructed() {
    let (result_tx, _result_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();
    let vault_dir = tempfile::tempdir().unwrap();

    let executor = CommandExecutor::new(
        config,
        result_tx,
        cancel_token,
    );
    assert!(executor.is_ok());
    assert!(!executor.unwrap().is_unlocked());
}

#[tokio::test]
async fn executor_run_loop_processes_commands() {
    let (result_tx, _result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();
    let vault_dir = tempfile::tempdir().unwrap();

    let executor = CommandExecutor::new(
        config,
        result_tx,
        cancel_token.clone(),
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
