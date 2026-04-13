use oak_keyring::executor::CommandExecutor;
use oak_keyring::config::AppConfig;
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
        vault_dir.path().to_path_buf(),
        result_tx,
        cancel_token,
    );
    assert!(executor.is_ok());
    assert!(!executor.unwrap().is_unlocked());
}
