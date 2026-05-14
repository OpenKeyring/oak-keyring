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
use crate::db::schema::{init_db, init_db_in_memory};
use crate::services::clipboard::ClipboardService;
use crate::services::health::HealthService;
use crate::services::import_export::ImportExportService;
use crate::services::vault::VaultService;
use crate::types::SecureStr;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbStartupMode {
    FileBacked,
    DeferredInMemory,
}

impl DbStartupMode {
    pub fn from_vault_state(state: crate::app::VaultInitState) -> Self {
        if state.has_vault || state.vault_has_db_only {
            Self::FileBacked
        } else {
            Self::DeferredInMemory
        }
    }
}

/// Load OAuth2 tokens from TokenStore into the in-memory GoogleDriveConfig.
/// TokenStore is the single source of truth — config.toml never stores tokens.
fn load_oauth2_tokens_into_config(config: &mut AppConfig, config_dir: &std::path::Path) {
    if config.sync.provider != SyncProvider::GoogleDrive {
        return;
    }
    let base_path = config_dir.join("tokens");
    let store = TokenStore::new(base_path);
    let tokens = match store.load("google_drive") {
        Ok(Some(t)) => t,
        Ok(None) => {
            tracing::warn!(
                "Google Drive tokens not found at new path — re-authorization may be required"
            );
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to load Google Drive tokens");
            return;
        }
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
    /// Path to the config directory (contains config.toml).
    config_dir: PathBuf,
    /// Cached health report, updated after health check runs.
    pub(super) health_report: Option<HealthReport>,
    /// Timestamp of the most recent health check completion.
    pub(super) last_health_check_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Cached verified master password for the change-password flow.
    /// Set after successful `VerifyMasterPassword`, consumed by `ChangeMasterPassword`.
    pub(super) verified_master_password: Option<SecureStr>,
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
    /// Opens the SQLite database at the data directory, or uses an in-memory
    /// database for deferred startup modes, creates service instances, and
    /// returns a fully-constructed executor ready to run.
    ///
    /// # Arguments
    /// * `config` — Application configuration
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
        result_tx: mpsc::Sender<Message>,
        shutdown_token: CancellationToken,
        vault_dir: std::path::PathBuf,
        config_dir: std::path::PathBuf,
        db_startup_mode: DbStartupMode,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!(vault_dir = %vault_dir.display(), config_dir = %config_dir.display(), ?db_startup_mode, "initializing CommandExecutor");

        let conn = match db_startup_mode {
            DbStartupMode::FileBacked => init_db(&vault_dir)?,
            DbStartupMode::DeferredInMemory => {
                info!("using in-memory database until vault database is explicitly initialized");
                init_db_in_memory()
            }
        };

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
        load_oauth2_tokens_into_config(&mut config, &config_dir);

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
            config: config_impl::ConfigManagerImpl::new(config, config_dir.clone()),
            config_notifier,
            vault_dir,
            config_dir,
            health_report: None,
            last_health_check_time: None,
            verified_master_password: None,
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

    /// Replace the sync service with a pre-built instance.
    ///
    /// # Note
    ///
    /// This method is intended for testing purposes only, allowing injection of
    /// mock sync services. In production, the sync service is configured during
    /// executor construction.
    pub fn set_sync_service(&mut self, sync: Option<crate::services::sync::SyncService>) {
        self.sync = sync;
    }

    /// Open a speculative file-backed vault database.
    ///
    /// The returned guard must be committed after the restore/init flow has
    /// fully validated and applied data. Dropping it rolls back newly created
    /// database artifacts while preserving artifacts that existed beforehand.
    pub(super) fn begin_file_backed_vault_db(
        &mut self,
    ) -> Result<PendingFileBackedVaultDb<'_>, Box<dyn std::error::Error + Send + Sync>> {
        let existed_before = vault_db_paths(&self.vault_dir).map(|path| artifact_exists(&path));
        let conn = init_db(&self.vault_dir)?;
        self.vault = crate::services::vault::VaultService::new(conn);
        info!("opened pending file-backed vault database");
        Ok(PendingFileBackedVaultDb {
            executor: self,
            committed: false,
            existed_before,
        })
    }
}

pub(super) struct PendingFileBackedVaultDb<'a> {
    executor: &'a mut CommandExecutor,
    committed: bool,
    existed_before: [bool; 4],
}

impl PendingFileBackedVaultDb<'_> {
    pub(super) fn unlock(
        &mut self,
        master_password: &crate::types::SecureStr,
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        self.executor
            .vault
            .unlock(&self.executor.vault_dir, master_password)
    }

    pub(super) fn create_record(
        &mut self,
        params: crate::types::record::CreateRecordParams,
    ) -> Result<uuid::Uuid, crate::errors::mapping::vault::VaultError> {
        self.executor.vault.create_record(params)
    }

    pub(super) fn apply_downloaded_cloud_record(
        &mut self,
        record: &crate::cloud::CloudRecord,
    ) -> Result<bool, crate::errors::mapping::vault::VaultError> {
        self.executor.vault.apply_downloaded_cloud_record(record)
    }

    pub(super) fn upsert_record_health_state(
        &mut self,
        state: &crate::types::health::RecordHealthState,
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        self.executor.vault.upsert_record_health_state(state)
    }

    pub(super) fn delete_record_health_states(
        &mut self,
        record_ids: &[uuid::Uuid],
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        self.executor.vault.delete_record_health_states(record_ids)
    }

    pub(super) async fn restore_pull_only(
        &mut self,
    ) -> Result<crate::services::sync::SyncResult, crate::errors::mapping::sync::SyncError> {
        self.executor
            .sync
            .as_mut()
            .unwrap()
            .restore_pull_only()
            .await
    }

    pub(super) fn set_metadata(
        &mut self,
        key: &str,
        value: &str,
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        self.executor.vault.set_metadata(key, value)
    }

    pub(super) fn commit(mut self) {
        self.committed = true;
        info!("committed file-backed vault database");
    }
}

impl Drop for PendingFileBackedVaultDb<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }

        // Pending file-backed databases are speculative recovery outputs. If a
        // restore/init path returns before commit(), leaving those files on disk
        // would make the next startup treat an empty or partial database as a
        // real vault. Roll back to the in-memory placeholder first so the
        // executor cannot keep using a connection to files we are about to
        // remove.
        self.executor.vault = crate::services::vault::VaultService::new(init_db_in_memory());

        for (path, existed_before) in vault_db_paths(&self.executor.vault_dir)
            .into_iter()
            .zip(self.existed_before)
        {
            if existed_before {
                continue;
            }
            if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %path.display(), error = %e, "failed to remove uncommitted vault database file");
                }
            }
        }
        info!("rolled back uncommitted file-backed vault database");
    }
}

fn artifact_exists(path: &std::path::Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

fn vault_db_paths(vault_dir: &std::path::Path) -> [std::path::PathBuf; 4] {
    [
        vault_dir.join("vault.db"),
        vault_dir.join("vault.db-wal"),
        vault_dir.join("vault.db-shm"),
        vault_dir.join("vault.db.migration.bak"),
    ]
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

#[cfg(test)]
mod db_startup_mode_tests {
    use super::*;
    use crate::app::VaultInitState;

    #[test]
    fn db_startup_mode_uses_file_backed_when_db_exists() {
        assert_eq!(
            DbStartupMode::from_vault_state(VaultInitState {
                has_vault: true,
                vault_has_key_only: false,
                vault_has_db_only: false,
            }),
            DbStartupMode::FileBacked
        );
        assert_eq!(
            DbStartupMode::from_vault_state(VaultInitState {
                has_vault: false,
                vault_has_key_only: false,
                vault_has_db_only: true,
            }),
            DbStartupMode::FileBacked
        );
    }

    #[test]
    fn db_startup_mode_defers_when_db_is_missing() {
        assert_eq!(
            DbStartupMode::from_vault_state(VaultInitState {
                has_vault: false,
                vault_has_key_only: false,
                vault_has_db_only: false,
            }),
            DbStartupMode::DeferredInMemory
        );
        assert_eq!(
            DbStartupMode::from_vault_state(VaultInitState {
                has_vault: false,
                vault_has_key_only: true,
                vault_has_db_only: false,
            }),
            DbStartupMode::DeferredInMemory
        );
    }
}
