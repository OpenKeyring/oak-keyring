use crate::commands::CommandResult;
use crate::config::AppConfig;
use crate::executor::config_impl::ServiceNotificationImpl;
use crate::executor::sync::handle_trigger_sync;
use crate::executor::CommandExecutor;
use crate::services::clipboard::{ClipboardService, MockBackend};
use crate::services::health::HealthService;
use crate::services::import_export::ImportExportService;
use crate::services::vault::VaultService;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// Cancellation check happens before sync.as_mut(), so a test executor with
/// `sync: None` and a cancelled token still exercises the cancellation path.
#[tokio::test]
async fn trigger_sync_returns_cancelled_when_token_already_cancelled() {
    let conn = crate::db::schema::init_db_in_memory();
    let vault = VaultService::new(conn);
    let (result_tx, _) = mpsc::channel(64);
    let (internal_tx, internal_rx) = mpsc::channel(64);

    let mut executor = CommandExecutor {
        vault,
        vault_db_file_backed: false,
        sync: None,
        health: HealthService::new(),
        clipboard: Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )),
        import_export: ImportExportService::new(),
        config: crate::executor::config_impl::ConfigManagerImpl::new(
            AppConfig::default(),
            std::path::PathBuf::from(":memory:"),
        ),
        config_notifier: ServiceNotificationImpl::new(),
        vault_dir: std::path::PathBuf::from(":memory:"),
        config_dir: std::path::PathBuf::from(":memory:"),
        health_report: None,
        last_health_check_time: None,
        result_tx,
        internal_tx,
        internal_rx: Some(internal_rx),
        shutdown_token: CancellationToken::new(),
        operation_cancel_token: CancellationToken::new(),
        timer_rebuild_pending: false,
        oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        verified_master_password: None,
    };
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
    let operation_cancel_token = shutdown_token.child_token();

    let conn = crate::db::schema::init_db_in_memory();
    let vault = VaultService::new(conn);
    let (result_tx, _) = mpsc::channel(64);
    let (internal_tx, internal_rx) = mpsc::channel(64);

    let mut executor = CommandExecutor {
        vault,
        vault_db_file_backed: false,
        sync: None,
        health: HealthService::new(),
        clipboard: Arc::new(ClipboardService::with_backend(
            Box::new(MockBackend::new()),
            30,
        )),
        import_export: ImportExportService::new(),
        config: crate::executor::config_impl::ConfigManagerImpl::new(
            AppConfig::default(),
            std::path::PathBuf::from(":memory:"),
        ),
        config_notifier: ServiceNotificationImpl::new(),
        vault_dir: std::path::PathBuf::from(":memory:"),
        config_dir: std::path::PathBuf::from(":memory:"),
        health_report: None,
        last_health_check_time: None,
        result_tx,
        internal_tx,
        internal_rx: Some(internal_rx),
        shutdown_token,
        operation_cancel_token,
        timer_rebuild_pending: false,
        oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        verified_master_password: None,
    };

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
