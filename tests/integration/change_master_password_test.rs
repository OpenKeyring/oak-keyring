// Integration tests for the change-master-password pipeline.
//
// Covers: VerifyMasterPassword → ChangeMasterPassword through the full
// command → executor → handler → keystore → persisted file chain.
//
// Gap addressed: existing unit tests only cover KeyStore::change_cmk and
// screen state in isolation. These tests prove the wiring works end-to-end.

use oak_keyring::commands::{Command, CommandResult, Message};
use oak_keyring::config::AppConfig;
use oak_keyring::executor::{CommandExecutor, DbStartupMode};
use oak_keyring::types::SecureStr;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

struct Harness {
    command_tx: mpsc::Sender<Command>,
    result_rx: mpsc::Receiver<Message>,
    _temp: tempfile::TempDir,
}

impl Harness {
    async fn new() -> Self {
        let (result_tx, result_rx) = mpsc::channel(64);
        let (command_tx, command_rx) = mpsc::channel(64);
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
            DbStartupMode::FileBacked,
        )
        .unwrap();

        tokio::spawn(async move { executor.run(command_rx).await });

        Harness {
            command_tx,
            result_rx,
            _temp: temp,
        }
    }

    async fn send(&self, command: Command) {
        self.command_tx.send(command).await.unwrap();
    }

    async fn recv_result(&mut self) -> CommandResult {
        loop {
            let msg = self.result_rx.recv().await.expect("result channel closed");
            match msg {
                Message::CommandCompleted(result) => {
                    // Skip background health-check results injected by unlock.
                    if matches!(
                        result,
                        CommandResult::HealthCheckStarted
                            | CommandResult::HealthCheckCompleted { .. }
                            | CommandResult::Cancelled { .. }
                    ) {
                        continue;
                    }
                    return result;
                }
                other => panic!("unexpected message: {other:?}"),
            }
        }
    }
}

async fn init_and_unlock(harness: &mut Harness, password: &str) {
    harness
        .send(Command::InitializeVault {
            master_password: SecureStr::new(password.to_string()),
            recovery_words: None,
        })
        .await;

    let result = harness.recv_result().await;
    assert!(
        matches!(result, CommandResult::VaultInitialized),
        "expected VaultInitialized, got: {result:?}"
    );

    harness
        .send(Command::UnlockVault {
            master_password: SecureStr::new(password.to_string()),
        })
        .await;

    let result = harness.recv_result().await;
    assert!(
        matches!(result, CommandResult::VaultUnlocked),
        "expected VaultUnlocked, got: {result:?}"
    );
}

#[tokio::test]
async fn verify_then_change_master_password_pipeline_succeeds() {
    let mut h = Harness::new().await;
    let old_pw = "old-password-12345";
    let new_pw = "new-password-67890";

    // 1. Init vault and unlock with old password.
    init_and_unlock(&mut h, old_pw).await;

    // 2. Verify the current master password.
    h.send(Command::VerifyMasterPassword {
        password: SecureStr::new(old_pw.to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::MasterPasswordVerified),
        "expected MasterPasswordVerified, got: {result:?}"
    );

    // 3. Change master password (current_password: None uses cached verified password).
    h.send(Command::ChangeMasterPassword {
        current_password: None,
        new_password: SecureStr::new(new_pw.to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::MasterPasswordChanged),
        "expected MasterPasswordChanged, got: {result:?}"
    );

    // 4. Unlock with the new password must succeed.
    h.send(Command::LockVault).await;
    let lock_result = h.recv_result().await;
    assert!(
        matches!(lock_result, CommandResult::VaultLocked),
        "expected VaultLocked, got: {lock_result:?}"
    );

    h.send(Command::UnlockVault {
        master_password: SecureStr::new(new_pw.to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::VaultUnlocked),
        "unlocking with new password must succeed, got: {result:?}"
    );

    // 5. Unlock with the old password must fail.
    h.send(Command::LockVault).await;
    let lock_result = h.recv_result().await;
    assert!(
        matches!(lock_result, CommandResult::VaultLocked),
        "expected VaultLocked, got: {lock_result:?}"
    );

    h.send(Command::UnlockVault {
        master_password: SecureStr::new(old_pw.to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::VaultUnlockFailed { .. }),
        "unlocking with old password must fail, got: {result:?}"
    );
}

#[tokio::test]
async fn change_master_password_with_explicit_current_password() {
    let mut h = Harness::new().await;
    let old_pw = "explicit-old-pw";
    let new_pw = "explicit-new-pw";

    init_and_unlock(&mut h, old_pw).await;

    // Change with explicit current_password (no prior VerifyMasterPassword).
    h.send(Command::ChangeMasterPassword {
        current_password: Some(SecureStr::new(old_pw.to_string())),
        new_password: SecureStr::new(new_pw.to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::MasterPasswordChanged),
        "expected MasterPasswordChanged with explicit password, got: {result:?}"
    );

    // Verify new password works.
    h.send(Command::LockVault).await;
    h.recv_result().await;

    h.send(Command::UnlockVault {
        master_password: SecureStr::new(new_pw.to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::VaultUnlocked),
        "new password must work after change, got: {result:?}"
    );
}

#[tokio::test]
async fn change_master_password_without_verified_password_fails() {
    let mut h = Harness::new().await;
    let old_pw = "some-password";

    init_and_unlock(&mut h, old_pw).await;

    // Attempt change without VerifyMasterPassword and without explicit current_password.
    h.send(Command::ChangeMasterPassword {
        current_password: None,
        new_password: SecureStr::new("irrelevant".to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::Error { .. }),
        "expected error when no verified password cached, got: {result:?}"
    );
}

#[tokio::test]
async fn verify_master_password_rejects_wrong_password() {
    let mut h = Harness::new().await;
    let password = "correct-password";

    init_and_unlock(&mut h, password).await;

    h.send(Command::VerifyMasterPassword {
        password: SecureStr::new("wrong-password".to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::Error { .. }),
        "expected error for wrong password verification, got: {result:?}"
    );
}

#[tokio::test]
async fn change_master_password_wrong_old_password_fails() {
    let mut h = Harness::new().await;
    let old_pw = "real-old-password";

    init_and_unlock(&mut h, old_pw).await;

    // Provide wrong current_password explicitly.
    h.send(Command::ChangeMasterPassword {
        current_password: Some(SecureStr::new("wrong-old-password".to_string())),
        new_password: SecureStr::new("new-password".to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::Error { .. }),
        "expected error for wrong old password, got: {result:?}"
    );

    // Original password should still work.
    h.send(Command::LockVault).await;
    h.recv_result().await;

    h.send(Command::UnlockVault {
        master_password: SecureStr::new(old_pw.to_string()),
    })
    .await;

    let result = h.recv_result().await;
    assert!(
        matches!(result, CommandResult::VaultUnlocked),
        "original password must still work after failed change, got: {result:?}"
    );
}
