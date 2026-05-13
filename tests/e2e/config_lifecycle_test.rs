//! E2E tests for config lifecycle through the command→executor→ConfigManager→disk→response pipeline.
//!
//! These tests verify the full integration:
//! 1. Command sent via channel
//! 2. Executor processes command
//! 3. ConfigManager performs operation
//! 4. Disk persistence (for save)
//! 5. Result returned via Message channel

use oak_keyring::commands::{Command, CommandResult, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::executor::CommandExecutor;
use oak_keyring::types::sensitive::SecureStr;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Helper to construct executor with channels and spawn the run loop
async fn setup_executor(_vault_dir: &TempDir) -> (mpsc::Sender<Command>, mpsc::Receiver<Message>) {
    let (result_tx, result_rx) = mpsc::channel(64);
    let (command_tx, command_rx) = mpsc::channel(64);
    let config = AppConfig::default();
    let cancel_token = CancellationToken::new();

    let executor = CommandExecutor::new(
        config,
        result_tx,
        cancel_token,
    )
    .expect("executor construction should succeed");

    // Spawn the executor run loop
    tokio::spawn(async move {
        executor.run(command_rx).await;
    });

    (command_tx, result_rx)
}

/// Helper to initialize and unlock a vault for tests that need vault unlocked
async fn setup_unlocked_executor(
    vault_dir: &TempDir,
) -> (mpsc::Sender<Command>, mpsc::Receiver<Message>) {
    let (command_tx, mut result_rx) = setup_executor(vault_dir).await;

    // Initialize vault
    let password = SecureStr::new("test_password_123".to_string());
    command_tx
        .send(Command::InitializeVault {
            master_password: password,
            recovery_words: None,
        })
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut result_rx).await;
    match result {
        CommandResult::VaultInitialized { .. } => {
            // Vault initialized successfully
        }
        other => panic!("Expected VaultInitialized, got {:?}", other),
    }

    (command_tx, result_rx)
}

/// Helper to receive the next CommandCompleted message with timeout
async fn recv_command_result(result_rx: &mut mpsc::Receiver<Message>) -> CommandResult {
    tokio::time::timeout(Duration::from_secs(5), result_rx.recv())
        .await
        .expect("should receive result within timeout")
        .expect("result channel should not close")
        .into_command_result()
}

/// Extension trait to extract CommandResult from Message
trait IntoCommandResult {
    fn into_command_result(self) -> CommandResult;
}

impl IntoCommandResult for Message {
    fn into_command_result(self) -> CommandResult {
        match self {
            Message::CommandCompleted(result) => result,
            other => panic!("Expected CommandCompleted, got {:?}", other),
        }
    }
}

#[tokio::test]
async fn load_config_returns_default_config() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let (command_tx, mut result_rx) = setup_executor(&vault_dir).await;

    // Send LoadConfig command
    command_tx
        .send(Command::LoadConfig)
        .await
        .expect("send should succeed");

    // Receive result
    let result = recv_command_result(&mut result_rx).await;

    // Verify ConfigLoaded with default config
    match result {
        CommandResult::ConfigLoaded { config } => {
            assert_eq!(
                config.general.clipboard_clear_seconds, 30,
                "default clipboard_clear_seconds should be 30"
            );
            assert_eq!(
                config.general.auto_lock_seconds, 300,
                "default auto_lock_seconds should be 300"
            );
            assert_eq!(
                config.general.language, "auto",
                "default language should be 'auto'"
            );
        }
        other => panic!("Expected ConfigLoaded, got {:?}", other),
    }

    // Clean up
    drop(command_tx);
}

#[tokio::test]
async fn save_config_persists_to_disk() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let (command_tx, mut result_rx) = setup_unlocked_executor(&vault_dir).await;

    // Create a modified config
    let mut modified_config = AppConfig::default();
    modified_config.general.clipboard_clear_seconds = 60;

    // Send SaveConfig command
    command_tx
        .send(Command::SaveConfig {
            config: modified_config.clone(),
        })
        .await
        .expect("send should succeed");

    // Receive result
    let result = recv_command_result(&mut result_rx).await;

    // Verify ConfigSaved with no warnings
    match result {
        CommandResult::ConfigSaved { warnings } => {
            assert!(
                warnings.is_empty(),
                "saving with default vault_path should produce no warnings, got: {:?}",
                warnings
            );
        }
        other => panic!("Expected ConfigSaved, got {:?}", other),
    }

    // Verify file was written to disk
    let config_path = vault_dir.path().join("config.toml");
    assert!(config_path.exists(), "config.toml should exist after save");

    // Read and verify the persisted config
    let persisted_config = AppConfig::load().expect("load should succeed");
    assert_eq!(
        persisted_config.general.clipboard_clear_seconds, 60,
        "persisted config should have updated clipboard_clear_seconds"
    );

    // Clean up
    drop(command_tx);
}

#[tokio::test]
async fn save_config_updates_in_memory() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let (command_tx, mut result_rx) = setup_unlocked_executor(&vault_dir).await;

    // Save a config with modified value
    let mut modified_config = AppConfig::default();
    modified_config.general.clipboard_clear_seconds = 90;

    command_tx
        .send(Command::SaveConfig {
            config: modified_config,
        })
        .await
        .expect("send should succeed");

    // Wait for save to complete
    let _save_result = recv_command_result(&mut result_rx).await;

    // Now load config again to verify in-memory was updated
    command_tx
        .send(Command::LoadConfig)
        .await
        .expect("send should succeed");

    let load_result = recv_command_result(&mut result_rx).await;

    // Verify the loaded config has the updated value
    match load_result {
        CommandResult::ConfigLoaded { config } => {
            assert_eq!(
                config.general.clipboard_clear_seconds, 90,
                "loaded config should have the updated clipboard_clear_seconds from save"
            );
        }
        other => panic!("Expected ConfigLoaded, got {:?}", other),
    }

    // Clean up
    drop(command_tx);
}


#[tokio::test]
async fn config_lifecycle_in_run_loop() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let (command_tx, mut result_rx) = setup_unlocked_executor(&vault_dir).await;

    // Step 1: Load initial config
    command_tx
        .send(Command::LoadConfig)
        .await
        .expect("send should succeed");

    let load_result = recv_command_result(&mut result_rx).await;
    match &load_result {
        CommandResult::ConfigLoaded { config } => {
            assert_eq!(config.general.clipboard_clear_seconds, 30);
        }
        other => panic!("Expected ConfigLoaded, got {:?}", other),
    }

    // Step 2: Save modified config
    let mut modified_config = AppConfig::default();
    modified_config.general.clipboard_clear_seconds = 45;

    command_tx
        .send(Command::SaveConfig {
            config: modified_config,
        })
        .await
        .expect("send should succeed");

    let save_result = recv_command_result(&mut result_rx).await;
    match save_result {
        CommandResult::ConfigSaved { warnings } => {
            assert!(warnings.is_empty());
        }
        other => panic!("Expected ConfigSaved, got {:?}", other),
    }

    // Step 3: Load config again to verify update
    command_tx
        .send(Command::LoadConfig)
        .await
        .expect("send should succeed");

    let load_result2 = recv_command_result(&mut result_rx).await;
    match load_result2 {
        CommandResult::ConfigLoaded { config } => {
            assert_eq!(
                config.general.clipboard_clear_seconds, 45,
                "loaded config should reflect the saved changes"
            );
        }
        other => panic!("Expected ConfigLoaded, got {:?}", other),
    }

    // Clean up
    drop(command_tx);
}

#[tokio::test]
async fn save_and_reload_preserves_config() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");

    // First executor: save a config
    {
        let (command_tx, mut result_rx) = setup_unlocked_executor(&vault_dir).await;

        let mut config = AppConfig::default();
        config.general.clipboard_clear_seconds = 75;
        config.general.auto_lock_seconds = 600;

        command_tx
            .send(Command::SaveConfig { config })
            .await
            .expect("send should succeed");

        let _result = recv_command_result(&mut result_rx).await;

        // Drop channels to stop executor
        drop(command_tx);
        drop(result_rx);
    }

    // Second executor: load the persisted config
    {
        // Load config from disk first to pass to the new executor
        let disk_config = AppConfig::load().expect("load should succeed");

        let (result_tx, result_rx) = mpsc::channel(64);
        let (command_tx, command_rx) = mpsc::channel(64);
        let cancel_token = CancellationToken::new();

        let executor = CommandExecutor::new(
            disk_config,
            result_tx,
            cancel_token,
        )
        .expect("executor construction should succeed");

        tokio::spawn(async move {
            executor.run(command_rx).await;
        });

        // Load config in new executor (should return the loaded config)
        command_tx
            .send(Command::LoadConfig)
            .await
            .expect("send should succeed");

        let mut result_rx = result_rx;
        let result = recv_command_result(&mut result_rx).await;

        match result {
            CommandResult::ConfigLoaded { config } => {
                assert_eq!(
                    config.general.clipboard_clear_seconds, 75,
                    "reloaded config should have the saved clipboard_clear_seconds"
                );
                assert_eq!(
                    config.general.auto_lock_seconds, 600,
                    "reloaded config should have the saved auto_lock_seconds"
                );
            }
            other => panic!("Expected ConfigLoaded, got {:?}", other),
        }

        // Clean up
        drop(command_tx);
    }
}

#[tokio::test]
async fn load_config_creates_default_on_missing_file() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");

    // Don't create config.toml — it should be missing
    let config_path = vault_dir.path().join("config.toml");
    assert!(
        !config_path.exists(),
        "config.toml should not exist initially"
    );

    let (command_tx, mut result_rx) = setup_executor(&vault_dir).await;

    // Load should succeed with default config even when file is missing
    command_tx
        .send(Command::LoadConfig)
        .await
        .expect("send should succeed");

    let result = recv_command_result(&mut result_rx).await;

    match result {
        CommandResult::ConfigLoaded { config } => {
            assert_eq!(config.general.clipboard_clear_seconds, 30);
            assert_eq!(config.general.auto_lock_seconds, 300);
        }
        other => panic!("Expected ConfigLoaded, got {:?}", other),
    }

    // File should still not exist (LoadConfig doesn't create it)
    assert!(
        !config_path.exists(),
        "config.toml should not be created by LoadConfig alone"
    );

    // Clean up
    drop(command_tx);
}

#[tokio::test]
async fn multiple_config_operations_sequence() {
    let vault_dir = tempfile::tempdir().expect("tempdir should succeed");
    let (command_tx, mut result_rx) = setup_unlocked_executor(&vault_dir).await;

    // Operation 1: Load initial
    command_tx
        .send(Command::LoadConfig)
        .await
        .expect("send should succeed");
    let _ = recv_command_result(&mut result_rx).await;

    // Operation 2: Save with modification 1
    let mut config1 = AppConfig::default();
    config1.general.clipboard_clear_seconds = 40;
    command_tx
        .send(Command::SaveConfig { config: config1 })
        .await
        .expect("send should succeed");
    let save_result1 = recv_command_result(&mut result_rx).await;
    match save_result1 {
        CommandResult::ConfigSaved { .. } => {}
        other => panic!("Expected ConfigSaved, got {:?}", other),
    }

    // Operation 3: Save with modification 2
    let mut config2 = AppConfig::default();
    config2.general.auto_lock_seconds = 500;
    command_tx
        .send(Command::SaveConfig { config: config2 })
        .await
        .expect("send should succeed");
    let save_result2 = recv_command_result(&mut result_rx).await;
    match save_result2 {
        CommandResult::ConfigSaved { .. } => {}
        other => panic!("Expected ConfigSaved, got {:?}", other),
    }

    // Operation 4: Load final state
    command_tx
        .send(Command::LoadConfig)
        .await
        .expect("send should succeed");
    let result = recv_command_result(&mut result_rx).await;

    // Verify final state reflects the last save
    // Note: config2 overwrites all fields with defaults except auto_lock_seconds
    match result {
        CommandResult::ConfigLoaded { config } => {
            assert_eq!(
                config.general.clipboard_clear_seconds, 30,
                "last save should determine clipboard_clear_seconds (default after overwrite)"
            );
            assert_eq!(
                config.general.auto_lock_seconds, 500,
                "last save should have the updated auto_lock_seconds"
            );
        }
        other => panic!("Expected ConfigLoaded, got {:?}", other),
    }

    // Clean up
    drop(command_tx);
}
