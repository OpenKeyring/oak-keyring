pub mod clipboard;
pub mod config;
pub mod config_impl;
pub mod execute;
pub mod health;
pub mod import_export;
pub mod record;
pub mod rotation;
pub mod sync;
pub mod timer;
pub mod vault;

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::cloud::oauth2::TokenStore;
use crate::cloud::provider::create_cloud_storage;
use crate::commands::types::HealthReport;
use crate::commands::{Command, InternalCommand, Message};
use crate::config::sync::{ProviderConfig, SyncProvider};
use crate::config::{AppConfig, ConfigManager};
use crate::db::schema::init_db;
use crate::services::clipboard::ClipboardService;
use crate::services::health::HealthService;
use crate::services::import_export::ImportExportService;
use crate::services::vault::VaultService;

use crate::config::notification::ServiceNotification;
use config_impl::{ClipboardConfigAdapter, ServiceNotificationImpl};

#[cfg(test)]
mod health_test;

#[cfg(test)]
mod timer_test;

#[cfg(test)]
mod vault_test;

#[cfg(test)]
mod sync_test;

/// Load OAuth2 tokens from TokenStore into the in-memory GoogleDriveConfig.
/// TokenStore is the single source of truth — config.toml never stores tokens.
fn load_oauth2_tokens_into_config(config: &mut AppConfig) {
    if config.sync.provider != SyncProvider::GoogleDrive {
        return;
    }
    let base_path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("open-keyring")
        .join("tokens");
    let store = TokenStore::new(base_path);
    let tokens = match store.load("google_drive") {
        Ok(Some(t)) => t,
        _ => return,
    };
    if let Some(ProviderConfig::GoogleDrive(ref mut cfg)) = config.sync.provider_config {
        cfg.access_token = tokens.access_token;
        if let Some(rt) = tokens.refresh_token {
            cfg.refresh_token = rt;
        }
    }
}

/// Command executor that bridges the UI layer to service layer.
///
/// Holds references to all services and dispatches incoming commands
/// to the appropriate handler. The run loop receives commands via
/// an mpsc channel and sends results back through a separate channel.
pub struct CommandExecutor {
    /// S1: Vault service — SQLite CRUD + encryption.
    vault: VaultService,
    /// S2: Sync service — cloud sync (None when no provider configured).
    sync: Option<crate::services::sync::SyncService>,
    /// S3: Health service — password security analysis.
    health: HealthService,
    /// S4: Clipboard service — system clipboard with auto-clear.
    #[allow(dead_code)]
    clipboard: Arc<ClipboardService>,
    /// S6: Import/Export service — file parsing and vault export.
    #[allow(dead_code)]
    import_export: ImportExportService,
    /// Application configuration manager.
    config: config_impl::ConfigManagerImpl,
    /// Notifier that dispatches config changes to registered services.
    config_notifier: ServiceNotificationImpl,
    /// Path to the vault directory (contains vault.db, config.toml, etc.).
    pub(super) vault_dir: PathBuf,
    /// Cached health report, updated after health check runs.
    pub(super) health_report: Option<HealthReport>,
    /// Timestamp of the most recent health check completion.
    pub(super) last_health_check_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Channel for sending messages (results) back to the UI layer.
    pub(super) result_tx: mpsc::Sender<Message>,
    /// Internal channel for background tasks to send system-level signals.
    /// This prevents closure issues on the main command_rx.
    pub(super) internal_tx: mpsc::Sender<InternalCommand>,
    /// Internal receiver (temporary storage until run() is called).
    internal_rx: Option<mpsc::Receiver<InternalCommand>>,
    /// Token for graceful executor shutdown (app exit). Only run() listens to this.
    shutdown_token: CancellationToken,
    /// Token for cancelling in-flight operations (LockVault, sync, health, etc.).
    operation_cancel_token: CancellationToken,
    /// Set by apply_config_changes when timer intervals changed and need rebuilding.
    timer_rebuild_pending: bool,
    /// OAuth2 token store for Google Drive authorization.
    oauth2_token_store: Arc<tokio::sync::Mutex<Option<crate::cloud::oauth2::TokenStore>>>,
}

impl CommandExecutor {
    /// Create a new CommandExecutor, initializing all services.
    ///
    /// Opens the SQLite database at `vault_dir/vault.db`, creates service
    /// instances, and returns a fully-constructed executor ready to run.
    ///
    /// # Arguments
    /// * `config` — Application configuration
    /// * `vault_dir` — Path to the vault directory
    /// * `result_tx` — Channel sender for dispatching messages to the UI
    /// * `shutdown_token` — Token for graceful executor shutdown
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened.
    ///
    /// Clipboard initialization degrades to a disabled backend when the
    /// platform clipboard is unavailable, so executor startup remains usable
    /// in headless and CI environments.
    #[tracing::instrument(skip(result_tx, shutdown_token))]
    pub fn new(
        mut config: AppConfig,
        vault_dir: PathBuf,
        result_tx: mpsc::Sender<Message>,
        shutdown_token: CancellationToken,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!(vault_dir = %vault_dir.display(), "initializing CommandExecutor");

        // Open and initialize the SQLite database.
        let conn = init_db(&vault_dir)?;

        // Create service instances.
        let vault = VaultService::new(conn);
        let health = HealthService::new();
        let import_export = ImportExportService::new();

        // Clipboard degrades to a disabled backend in headless/CI environments.
        let clipboard_clear_seconds = config.general.clipboard_clear_seconds;
        let clipboard = Arc::new(ClipboardService::new_safe(clipboard_clear_seconds)?);

        // Register clipboard service for config-change notifications.
        let mut config_notifier = ServiceNotificationImpl::new();
        config_notifier.register_service(Box::new(ClipboardConfigAdapter::new(Arc::clone(
            &clipboard,
        ))));

        // Load OAuth2 tokens from TokenStore (runtime-only, not in config.toml).
        load_oauth2_tokens_into_config(&mut config);

        let sync = match create_cloud_storage(&config.sync) {
            Ok(storage) => {
                info!("SyncService initialized for {:?}", config.sync.provider);
                Some(crate::services::sync::SyncService::new(storage))
            }
            Err(e) => {
                info!(error = %e, "SyncService not initialized — sync features disabled");
                None
            }
        };

        // Create internal signaling channel
        let (internal_tx, internal_rx) = mpsc::channel(64);

        info!("CommandExecutor initialized successfully");

        let operation_cancel_token = shutdown_token.child_token();

        Ok(Self {
            vault,
            sync,
            health,
            clipboard,
            import_export,
            config: config_impl::ConfigManagerImpl::new(config),
            config_notifier,
            vault_dir,
            health_report: None,
            last_health_check_time: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            shutdown_token,
            operation_cancel_token,
            timer_rebuild_pending: false,
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    /// Check whether the vault is currently unlocked.
    pub fn is_unlocked(&self) -> bool {
        self.vault.is_unlocked()
    }

    /// Get a reference to the operation cancellation token.
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.operation_cancel_token
    }

    /// Main executor run loop.
    ///
    /// Receives commands from the UI layer and dispatches them to the
    /// appropriate handler. The loop exits when the command channel is
    /// closed or the cancellation token is triggered.
    pub async fn run(mut self, mut command_rx: mpsc::Receiver<Command>) {
        info!("CommandExecutor started");

        let mut internal_rx = self.internal_rx.take().expect("internal_rx must be set");
        let mut timers = timer::ExecutorTimers::new(&self.config.get_config(), self.sync.is_some());

        loop {
            // Destructure into individual fields so tokio::select! can take
            // disjoint mutable borrows.
            let timer::ExecutorTimers {
                ref mut sync_interval,
                ref mut auto_lock_interval,
                sync_active,
                auto_lock_active,
            } = timers;

            tokio::select! {
                // Priority 1: Cancellation signal
                biased;

                _ = self.shutdown_token.cancelled() => {
                    self.operation_cancel_token.cancel();
                    info!("Executor received shutdown signal, shutting down");
                    break;
                }

                // Priority 2: Command processing (external)
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(command) => {
                            self.execute(command).await;
                            timers.reset_auto_lock();
                        }
                        None => {
                            info!("Command channel closed, executor shutting down");
                            break;
                        }
                    }
                }

                // Priority 3: Internal command processing (background tasks)
                cmd = internal_rx.recv() => {
                    if let Some(internal_cmd) = cmd {
                        let resets_lock = internal_cmd.resets_auto_lock();
                        self.execute_internal(internal_cmd).await;
                        if resets_lock {
                            timers.reset_auto_lock();
                        }
                    }
                }

                // Priority 4: Auto-sync timer
                _ = timer::tick_opt(sync_interval), if sync_active => {
                    info!("Auto-sync timer triggered");
                    self.execute(Command::TriggerSync).await;
                }

                // Priority 5: Auto-lock timer
                _ = timer::tick_opt(auto_lock_interval), if auto_lock_active => {
                    info!("Auto-lock timer triggered");
                    self.execute(Command::LockVault).await;
                }

            }

            // Rebuild timers if config changed since last iteration.
            // Must happen after select! — destructure borrows are released here.
            if self.timer_rebuild_pending {
                timers.rebuild(&self.config.get_config(), self.sync.is_some());
                self.timer_rebuild_pending = false;
            }
        }

        info!("CommandExecutor stopped");
    }

    /// Execute an internal command from a background task.
    ///
    /// All internal commands bypass `pre_check` (no vault-lock gate).
    /// `HealthCheckCompleted` handles cache updates internally.
    /// `ScheduleHealthCheck` delegates to `post_hook` for error logging
    /// and cache refresh.
    async fn execute_internal(&mut self, cmd: InternalCommand) {
        match cmd {
            InternalCommand::HealthCheckCompleted { report } => {
                let result =
                    crate::executor::execute::handle_internal_health_check_completed(self, report);
                let _ = self.result_tx.send(Message::CommandCompleted(result)).await;
            }
            InternalCommand::ScheduleHealthCheck { force } => {
                let result = health::handle_run_health_check(self, force);
                self.post_hook(&result);
                let _ = self.result_tx.send(Message::CommandCompleted(result)).await;
            }
        }
    }

    // execute(), pre_check(), post_hook(), and dispatch() are defined in execute.rs

    /// Replace the sync service with a pre-built instance (test-only).
    #[cfg(feature = "test-helpers")]
    pub fn set_sync_service(&mut self, sync: Option<crate::services::sync::SyncService>) {
        self.sync = sync;
    }
}

#[cfg(test)]
mod shutdown_tests {
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn shutdown_token_cancels_operation_token() {
        let shutdown = CancellationToken::new();
        let operation = shutdown.child_token();

        assert!(!operation.is_cancelled());
        shutdown.cancel();
        assert!(operation.is_cancelled());
    }
}
