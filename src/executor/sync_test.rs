use crate::commands::CommandResult;
use crate::config::AppConfig;
use crate::executor::sync::handle_trigger_sync;
use crate::executor::CommandExecutor;
use crate::services::clipboard::{ClipboardService, MockBackend};
use crate::services::vault::VaultServiceImpl;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Cancellation check happens before sync.as_mut(), so a test executor with
/// `sync: None` and a cancelled token still exercises the cancellation path.
#[tokio::test]
async fn trigger_sync_returns_cancelled_when_token_already_cancelled() {
    let conn = crate::db::schema::init_db_in_memory().unwrap();
    let vault = VaultServiceImpl::new(conn);
    let (result_tx, _) = mpsc::channel(64);

    let mut executor = CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .vault(Box::new(vault))
        .config(AppConfig::default())
        .result_tx(result_tx)
        .shutdown_token(CancellationToken::new())
        .clipboard(Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )))
        .build();
    executor.cancel_token().cancel();

    let result = handle_trigger_sync(&mut executor).await;

    assert!(matches!(
        result,
        CommandResult::Cancelled { ref operation, .. } if operation == "sync"
    ));
}

/// When app shuts down, it cancels the shutdown token. Since
/// operation_cancel_token is a child, any cancellable command must
/// observe cancellation immediately — even though the command itself
/// only checks `executor.cancel_token()`.
#[tokio::test]
async fn trigger_sync_returns_cancelled_when_shutdown_token_cancelled() {
    let shutdown_token = CancellationToken::new();

    let conn = crate::db::schema::init_db_in_memory().unwrap();
    let vault = VaultServiceImpl::new(conn);
    let (result_tx, _) = mpsc::channel(64);

    let mut executor = CommandExecutor::builder(":memory:".into(), ":memory:".into())
        .vault(Box::new(vault))
        .config(AppConfig::default())
        .result_tx(result_tx)
        .shutdown_token(shutdown_token)
        .clipboard(Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )))
        .build();

    // Simulate app shutdown: cancel the shutdown token.
    // operation_cancel_token (a child) should be auto-cancelled.
    executor.shutdown_token.cancel();

    let result = handle_trigger_sync(&mut executor).await;

    assert!(
        matches!(result, CommandResult::Cancelled { ref operation, .. } if operation == "sync"),
        "Expected Cancelled on shutdown, got {:?}",
        result
    );
}

#[test]
fn cancelled_helper_returns_correct_operation_name() {
    let result = CommandResult::cancelled("sync");
    assert!(matches!(
        result,
        CommandResult::Cancelled { ref operation, .. } if operation == "sync"
    ));
}
