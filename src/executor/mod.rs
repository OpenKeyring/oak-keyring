pub mod cancellation;
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

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::commands::{Command, Message};
use crate::commands::types::HealthReport;
use crate::config::AppConfig;
use crate::db::schema::init_db;
use crate::services::clipboard::ClipboardService;
use crate::services::health::HealthService;
use crate::services::import_export::ImportExportService;
use crate::services::vault::VaultService;

/// Command executor that bridges the UI layer to service layer.
///
/// Holds references to all services and dispatches incoming commands
/// to the appropriate handler. The run loop receives commands via
/// an mpsc channel and sends results back through a separate channel.
pub struct CommandExecutor {
    /// S1: Vault service — SQLite CRUD + encryption.
    vault: VaultService,
    /// S2: Sync service — cloud sync (None until configured via Task 22).
    #[allow(dead_code)]
    sync: Option<crate::services::sync::SyncService>,
    /// S3: Health service — password security analysis.
    health: HealthService,
    /// S4: Clipboard service — system clipboard with auto-clear.
    #[allow(dead_code)]
    clipboard: ClipboardService,
    /// S6: Import/Export service — file parsing and vault export.
    #[allow(dead_code)]
    import_export: ImportExportService,
    /// Application configuration.
    config: AppConfig,
    /// Path to the vault directory (contains vault.db, config.toml, etc.).
    vault_dir: PathBuf,
    /// Cached health report, updated after health check runs.
    health_report: Option<HealthReport>,
    /// Channel for sending messages (results) back to the UI layer.
    result_tx: mpsc::Sender<Message>,
    /// Cancellation token for graceful shutdown and operation cancellation.
    cancel_token: CancellationToken,
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
    /// * `cancel_token` — Token for cooperative cancellation / shutdown
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened or the clipboard
    /// backend is unavailable in the current environment.
    #[tracing::instrument(skip(result_tx, cancel_token))]
    pub fn new(
        config: AppConfig,
        vault_dir: PathBuf,
        result_tx: mpsc::Sender<Message>,
        cancel_token: CancellationToken,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!(vault_dir = %vault_dir.display(), "initializing CommandExecutor");

        // Open and initialize the SQLite database.
        let conn = init_db(&vault_dir);

        // Create service instances.
        let vault = VaultService::new(conn);
        let health = HealthService::new();
        let import_export = ImportExportService::new();

        // Clipboard may fail in headless/CI environments — propagate the error
        // so callers can decide how to handle it.
        let clipboard_clear_seconds = config.general.clipboard_clear_seconds;
        let clipboard = ClipboardService::new_safe(clipboard_clear_seconds)?;

        // Sync is not yet wired up; will be integrated in Task 22.
        let sync = None;

        info!("CommandExecutor initialized successfully");

        Ok(Self {
            vault,
            sync,
            health,
            clipboard,
            import_export,
            config,
            vault_dir,
            health_report: None,
            result_tx,
            cancel_token,
        })
    }

    /// Check whether the vault is currently unlocked.
    pub fn is_unlocked(&self) -> bool {
        self.vault.is_unlocked()
    }

    /// Get a reference to the cancellation token.
    pub fn cancel_token(&self) -> &CancellationToken {
        &self.cancel_token
    }

    /// Main executor run loop.
    ///
    /// Receives commands from the UI layer and dispatches them to the
    /// appropriate handler. The loop exits when the command channel is
    /// closed or the cancellation token is triggered.
    pub async fn run(mut self, mut command_rx: mpsc::Receiver<Command>) {
        info!("CommandExecutor started");

        let mut timers = timer::ExecutorTimers::new(&self.config);

        loop {
            // Destructure into individual fields so tokio::select! can take
            // disjoint mutable borrows.
            let timer::ExecutorTimers {
                ref mut sync_interval,
                ref mut auto_lock_interval,
                ref mut clipboard_clear_interval,
                sync_active,
                auto_lock_active,
            } = timers;

            tokio::select! {
                // Priority 1: Cancellation signal
                biased;

                _ = self.cancel_token.cancelled() => {
                    info!("Executor received cancellation signal, shutting down");
                    break;
                }

                // Priority 2: Command processing
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

                // Priority 3: Auto-sync timer
                _ = timer::tick_opt(sync_interval), if sync_active => {
                    info!("Auto-sync timer triggered");
                    self.execute(Command::TriggerSync).await;
                }

                // Priority 4: Auto-lock timer
                _ = timer::tick_opt(auto_lock_interval), if auto_lock_active => {
                    info!("Auto-lock timer triggered");
                    self.execute(Command::LockVault).await;
                }

                // Priority 5: Clipboard clear timer
                _ = timer::tick_opt(clipboard_clear_interval) => {
                    info!("Clipboard clear timer triggered");
                    let _ = self.clipboard.clear();
                }
            }
        }

        info!("CommandExecutor stopped");
    }

    /// Execute a single command.
    ///
    /// Dispatches the command to the appropriate handler module based
    /// on the command variant.
    pub async fn execute(&mut self, _command: Command) {
        // TODO: Implement full dispatch in Task 3
        // For now, just log that we received a command
        info!("Received command (dispatch pending Task 3)");
    }
}
