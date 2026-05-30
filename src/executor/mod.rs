pub mod builder;
pub mod clipboard;
pub mod config;
pub mod config_impl;
pub mod execute;
pub mod health;
pub mod import_export;
pub mod record;
pub mod rotation;
pub mod runtime;
pub mod sync;
pub mod timer;
pub mod vault;

pub use timer::ActivityTracker;

pub use builder::ExecutorBuilder;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::cloud::oauth2::TokenStore;
use crate::cloud::provider::create_cloud_storage;
use crate::commands::types::HealthReport;
use crate::commands::{Command, InternalCommand, Message};
use crate::config::sync::{ProviderConfig, SyncProvider};
use crate::config::{AppConfig, ConfigManager};
#[cfg(not(feature = "sqlcipher"))]
use crate::db::schema::init_db;
use crate::db::schema::init_db_in_memory;
use crate::services::clipboard::{Clipboard, ClipboardService};
use crate::services::health::Health;
use crate::services::vault::{Vault, VaultServiceImpl};
use crate::types::SecureStr;

use config_impl::ServiceNotificationImpl;

#[cfg(test)]
mod health_test;

#[cfg(test)]
mod timer_test;

#[cfg(test)]
mod vault_test;

#[cfg(test)]
mod sync_test;

#[cfg(test)]
mod mock_orchestration_test;

#[cfg(test)]
mod clipboard_test;

const SYNC_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShutdownReport {
    pub sync_shutdown: ShutdownStepStatus,
    pub wal_checkpoint: ShutdownStepStatus,
}

impl Default for ShutdownReport {
    fn default() -> Self {
        Self {
            sync_shutdown: ShutdownStepStatus::NotApplicable,
            wal_checkpoint: ShutdownStepStatus::NotApplicable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShutdownStepStatus {
    NotApplicable,
    Completed,
    Failed(String),
    TimedOut,
}

impl ShutdownStepStatus {
    fn has_failure(&self) -> bool {
        matches!(self, Self::Failed(_) | Self::TimedOut)
    }
}

/// Command executor that bridges the UI layer to service layer.
///
/// Holds references to all services and dispatches incoming commands
/// to the appropriate handler. The run loop receives commands via
/// an mpsc channel and sends results back through a separate channel.
pub struct CommandExecutor {
    /// S1: Vault service — SQLite CRUD + encryption, tracked by runtime state.
    vault_runtime: runtime::VaultRuntime,
    /// True when `vault_runtime` wraps an on-disk vault.db rather than recovery-only memory state.
    vault_db_file_backed: bool,
    /// S2: Sync service — cloud sync (None when no provider configured).
    sync: Option<Box<dyn crate::services::sync::SyncService>>,
    /// S3: Health service — password security analysis.
    health: Arc<dyn Health>,
    /// S4: Clipboard service — system clipboard with auto-clear.
    #[allow(dead_code)]
    clipboard: Arc<dyn Clipboard>,
    /// S6: Import/Export service — file parsing and vault export.
    #[allow(dead_code)]
    import_export: Box<dyn crate::services::import_export::ImportExport>,
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
    /// Shared activity tracker for auto-lock idle detection.
    activity: timer::ActivityTracker,
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
        activity: timer::ActivityTracker,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        info!(vault_dir = %vault_dir.display(), config_dir = %config_dir.display(), ?db_startup_mode, "initializing CommandExecutor");

        let vault_db_file_backed = matches!(db_startup_mode, DbStartupMode::FileBacked);

        #[cfg(not(feature = "sqlcipher"))]
        let vault_runtime = {
            let conn = match db_startup_mode {
                DbStartupMode::FileBacked => init_db(&vault_dir)?,
                DbStartupMode::DeferredInMemory => {
                    info!(
                        "using in-memory database until vault database is explicitly initialized"
                    );
                    init_db_in_memory()?
                }
            };

            let vault = Box::new(VaultServiceImpl::new(conn)) as Box<dyn Vault>;
            runtime::VaultRuntime::open(vault)
        };

        #[cfg(feature = "sqlcipher")]
        let vault_runtime = match db_startup_mode {
            DbStartupMode::FileBacked => {
                // Production SQLCipher: start locked. Unlock must provide the key.
                runtime::VaultRuntime::locked()
            }
            DbStartupMode::DeferredInMemory => {
                info!("using in-memory database until vault database is explicitly initialized");
                let conn = init_db_in_memory()?;
                let vault = Box::new(VaultServiceImpl::new(conn)) as Box<dyn Vault>;
                runtime::VaultRuntime::open(vault)
            }
        };

        // Load OAuth2 tokens from TokenStore (runtime-only, not in config.toml).
        load_oauth2_tokens_into_config(&mut config, &config_dir);

        let sync = match create_cloud_storage(&config.sync) {
            Ok(storage) => {
                info!("SyncService initialized for {:?}", config.sync.provider);
                Some(
                    Box::new(crate::services::sync::SyncServiceImpl::new(storage))
                        as Box<dyn crate::services::sync::SyncService>,
                )
            }
            Err(e) => {
                info!(error = %e, "SyncService not initialized — sync features disabled");
                None
            }
        };

        let clipboard_clear_seconds = config.general.clipboard_clear_seconds;
        let clipboard =
            Arc::new(ClipboardService::new_safe(clipboard_clear_seconds)?) as Arc<dyn Clipboard>;

        info!("CommandExecutor initialized successfully");

        Ok(Self::builder(vault_dir.clone(), config_dir.clone())
            .vault_runtime(vault_runtime)
            .vault_db_file_backed(vault_db_file_backed)
            .sync(sync)
            .config(config)
            .result_tx(result_tx)
            .shutdown_token(shutdown_token)
            .clipboard(clipboard)
            .activity(activity)
            .build()?)
    }

    /// Check whether the vault is currently unlocked.
    pub fn is_unlocked(&self) -> bool {
        self.vault_runtime.is_open() && self.vault().map(|v| v.is_unlocked()).unwrap_or(false)
    }

    fn should_run_auto_sync_timer(&self) -> bool {
        self.vault_db_file_backed && self.is_unlocked()
    }

    fn should_run_auto_lock_timer(&self) -> bool {
        self.vault_db_file_backed && self.is_unlocked()
    }

    /// Get a reference to the vault (read-only operations).
    ///
    /// Returns an error if the vault runtime is not `Open`.
    pub(super) fn vault(&self) -> Result<&dyn Vault, crate::errors::mapping::vault::VaultError> {
        Ok(self.vault_runtime.open_vault()?)
    }

    /// Get a mutable reference to the vault (read-write operations).
    ///
    /// Returns an error if the vault runtime is not `Open`.
    pub(super) fn vault_mut(
        &mut self,
    ) -> Result<&mut dyn Vault, crate::errors::mapping::vault::VaultError> {
        Ok(self.vault_runtime.open_vault_mut()?)
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
    pub async fn run(
        mut self,
        mut command_rx: mpsc::Receiver<Command>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("CommandExecutor started");

        let Some(mut internal_rx) = self.internal_rx.take() else {
            return Err(Box::new(std::io::Error::other(
                "CommandExecutor cannot run without internal_rx",
            )));
        };
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
                    if self.should_run_auto_sync_timer() {
                        info!("Auto-sync timer triggered");
                        self.execute(Command::TriggerSync).await;
                    } else {
                        tracing::debug!("Auto-sync timer skipped because vault is not unlocked");
                    }
                }

                // Priority 5: Auto-lock timer
                _ = timer::tick_opt(auto_lock_interval), if auto_lock_active => {
                    if self.should_run_auto_lock_timer() {
                        let configured = self.config.get_config().general.auto_lock_seconds as i64;
                        let idle = self.activity.idle_seconds();
                        if idle >= configured {
                            info!(idle_seconds = idle, "Auto-lock timer triggered");
                            self.execute(Command::LockVault).await;
                        } else {
                            tracing::debug!(idle_seconds = idle, "Auto-lock skipped — user active");
                            timers.reset_auto_lock();
                        }
                    } else {
                        tracing::debug!("Auto-lock timer skipped because vault is not unlocked");
                    }
                }

            }

            // Rebuild timers if config changed since last iteration.
            // Must happen after select! — destructure borrows are released here.
            if self.timer_rebuild_pending {
                timers.rebuild(&self.config.get_config(), self.sync.is_some());
                self.timer_rebuild_pending = false;
            }
        }

        let report = self.shutdown_gracefully().await;
        if report.sync_shutdown.has_failure() || report.wal_checkpoint.has_failure() {
            tracing::error!(?report, "CommandExecutor stopped with shutdown failures");
        } else {
            info!("CommandExecutor stopped");
        }

        Ok(())
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

    /// Create an [`ExecutorBuilder`] for constructing a CommandExecutor.
    ///
    /// This is the primary method for test fixtures and custom construction scenarios.
    /// Production code should use [`CommandExecutor::new`] instead.
    #[must_use]
    pub fn builder(
        vault_dir: std::path::PathBuf,
        config_dir: std::path::PathBuf,
    ) -> ExecutorBuilder {
        ExecutorBuilder::new(vault_dir, config_dir)
    }

    pub(crate) async fn shutdown_gracefully(&mut self) -> ShutdownReport {
        self.operation_cancel_token.cancel();

        let mut report = ShutdownReport::default();

        if let Some(sync) = self.sync.take() {
            report.sync_shutdown =
                match tokio::time::timeout(SYNC_SHUTDOWN_TIMEOUT, sync.shutdown_box()).await {
                    Ok(Ok(())) => ShutdownStepStatus::Completed,
                    Ok(Err(e)) => ShutdownStepStatus::Failed(e.to_string()),
                    Err(_) => ShutdownStepStatus::TimedOut,
                };
        }

        if self.vault_db_file_backed {
            report.wal_checkpoint = match self.vault() {
                Ok(vault) => match vault.checkpoint_wal() {
                    Ok(()) => ShutdownStepStatus::Completed,
                    Err(e) => ShutdownStepStatus::Failed(e.to_string()),
                },
                Err(_) => ShutdownStepStatus::NotApplicable,
            };
        }
        self.vault_runtime = crate::executor::runtime::VaultRuntime::locked();
        self.vault_db_file_backed = false;

        tracing::info!(?report, "executor graceful shutdown completed");
        report
    }

    /// Open a speculative file-backed vault database encrypted with SQLCipher.
    ///
    /// The returned guard must be committed after the restore/init flow has
    /// fully validated and applied data. Dropping it rolls back newly created
    /// database artifacts while preserving artifacts that existed beforehand.
    #[cfg(feature = "sqlcipher")]
    pub fn begin_file_backed_vault_db(
        &mut self,
        key: &crate::crypto::db_page_key::DbPageKey,
    ) -> Result<PendingFileBackedVaultDb<'_>, Box<dyn std::error::Error + Send + Sync>> {
        let existed_before = vault_db_paths(&self.vault_dir).map(|path| artifact_exists(&path));
        let conn =
            crate::db::vault_db::VaultDbFactory::create_sqlcipher_vault(&self.vault_dir, key)?;
        self.vault_runtime = runtime::VaultRuntime::open(Box::new(
            crate::services::vault::VaultServiceImpl::new(conn),
        ));
        info!("opened pending file-backed SQLCipher vault database");
        Ok(PendingFileBackedVaultDb {
            executor: self,
            committed: false,
            existed_before,
        })
    }

    /// Open a speculative file-backed vault database (plain SQLite).
    ///
    /// The returned guard must be committed after the restore/init flow has
    /// fully validated and applied data. Dropping it rolls back newly created
    /// database artifacts while preserving artifacts that existed beforehand.
    #[cfg(not(feature = "sqlcipher"))]
    pub fn begin_file_backed_vault_db(
        &mut self,
    ) -> Result<PendingFileBackedVaultDb<'_>, Box<dyn std::error::Error + Send + Sync>> {
        let existed_before = vault_db_paths(&self.vault_dir).map(|path| artifact_exists(&path));
        let conn = init_db(&self.vault_dir)?;
        self.vault_runtime = runtime::VaultRuntime::open(Box::new(
            crate::services::vault::VaultServiceImpl::new(conn),
        ));
        info!("opened pending file-backed vault database");
        Ok(PendingFileBackedVaultDb {
            executor: self,
            committed: false,
            existed_before,
        })
    }
}

pub struct PendingFileBackedVaultDb<'a> {
    executor: &'a mut CommandExecutor,
    committed: bool,
    existed_before: [bool; 4],
}

impl PendingFileBackedVaultDb<'_> {
    pub(super) fn unlock(
        &mut self,
        master_password: &crate::types::SecureStr,
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        let vault_dir = self.executor.vault_dir.clone();
        self.executor
            .vault_mut()?
            .unlock(&vault_dir, master_password)
    }

    pub(super) fn create_record(
        &mut self,
        params: crate::types::record::CreateRecordParams,
    ) -> Result<uuid::Uuid, crate::errors::mapping::vault::VaultError> {
        self.executor.vault_mut()?.create_record(params)
    }

    pub(super) fn apply_downloaded_cloud_record(
        &mut self,
        record: &crate::cloud::CloudRecord,
    ) -> Result<bool, crate::errors::mapping::vault::VaultError> {
        self.executor
            .vault_mut()?
            .apply_downloaded_cloud_record(record)
    }

    pub(super) fn upsert_record_health_state(
        &mut self,
        state: &crate::types::health::RecordHealthState,
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        self.executor.vault_mut()?.upsert_record_health_state(state)
    }

    pub(super) fn delete_record_health_states(
        &mut self,
        record_ids: &[uuid::Uuid],
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        self.executor
            .vault_mut()?
            .delete_record_health_states(record_ids)
    }

    pub(super) async fn restore_pull_only(
        &mut self,
    ) -> Result<crate::services::sync::SyncResult, crate::errors::mapping::sync::SyncError> {
        let sync = self.executor.sync.as_mut().ok_or_else(|| {
            crate::errors::mapping::sync::SyncError::ProviderError {
                provider: "none".to_string(),
                message: "Sync service not configured".to_string(),
            }
        })?;
        sync.restore_pull_only().await
    }

    pub(super) fn set_metadata(
        &mut self,
        key: &str,
        value: &str,
    ) -> Result<(), crate::errors::mapping::vault::VaultError> {
        self.executor.vault_mut()?.set_metadata(key, value)
    }

    pub(super) fn commit(mut self) {
        self.committed = true;
        self.executor.vault_db_file_backed = true;
        info!("committed file-backed vault database");
    }
}

#[cfg(feature = "sqlcipher")]
impl PendingFileBackedVaultDb<'_> {
    /// Explicitly roll back this pending guard, removing any newly created
    /// database artifacts and returning the executor to a locked state.
    ///
    /// Returns `DbRollbackFailed` if any file removal fails. The error carries
    /// the path of the file that could not be removed.
    pub(super) fn rollback(mut self) -> Result<(), crate::db::vault_db::VaultDbError> {
        self.committed = true; // skip duplicate cleanup in Drop
        self.executor.vault_runtime = runtime::VaultRuntime::locked();
        self.executor.vault_db_file_backed = false;

        let mut first_error: Option<(std::path::PathBuf, std::io::Error)> = None;
        for (path, existed_before) in vault_db_paths(&self.executor.vault_dir)
            .into_iter()
            .zip(self.existed_before)
        {
            if existed_before {
                continue;
            }
            if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    tracing::error!(path = %path.display(), error = %e, "rollback: failed to remove uncommitted file");
                    first_error.get_or_insert((path, e));
                }
            }
        }
        if let Some((path, error)) = first_error {
            Err(crate::db::vault_db::VaultDbError::DbRollbackFailed(
                format!("{}: {}", path.display(), error),
            ))
        } else {
            info!("rolled back uncommitted file-backed vault database");
            Ok(())
        }
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
        // real vault. Drop the open vault runtime first so the executor cannot
        // keep using a connection to files we are about to remove.
        self.executor.vault_runtime = runtime::VaultRuntime::locked();
        self.executor.vault_db_file_backed = false;

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
    use super::{ActivityTracker, CommandExecutor, DbStartupMode, ShutdownStepStatus};
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn shutdown_token_cancels_operation_token() {
        let shutdown = CancellationToken::new();
        let operation = shutdown.child_token();

        assert!(!operation.is_cancelled());
        shutdown.cancel();
        assert!(operation.is_cancelled());
    }

    #[tokio::test]
    async fn shutdown_report_marks_sync_and_wal_not_applicable_for_in_memory_vault() {
        let shutdown = CancellationToken::new();
        let dir = tempfile::tempdir().unwrap();
        let executor = CommandExecutor::new(
            crate::config::AppConfig::default_config(),
            mpsc::channel(8).0,
            shutdown,
            dir.path().to_path_buf(),
            dir.path().to_path_buf(),
            DbStartupMode::DeferredInMemory,
            ActivityTracker::new(),
        )
        .expect("executor should construct with in-memory vault");

        let mut executor = executor;
        let report = executor.shutdown_gracefully().await;

        assert_eq!(report.sync_shutdown, ShutdownStepStatus::NotApplicable);
        assert_eq!(report.wal_checkpoint, ShutdownStepStatus::NotApplicable);
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

#[cfg(all(test, feature = "sqlcipher"))]
mod rollback_tests {
    use super::*;
    use crate::crypto::db_page_key::test_db_page_key;
    use crate::db::vault_db::VaultDbError;

    #[test]
    fn rollback_reports_database_rollback_failed_when_file_removal_fails() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (tx, _rx) = tokio::sync::mpsc::channel(8);
        let mut executor = CommandExecutor::new(
            crate::config::AppConfig::default_config(),
            tx,
            tokio_util::sync::CancellationToken::new(),
            dir.path().to_path_buf(),
            dir.path().to_path_buf(),
            DbStartupMode::DeferredInMemory,
            ActivityTracker::new(),
        )
        .expect("executor");

        let key = test_db_page_key([0xee; 32]);
        let guard = executor
            .begin_file_backed_vault_db(&key)
            .expect("begin pending");

        // Replace vault.db with a directory so std::fs::remove_file fails
        // (it cannot remove directories). This forces rollback() to surface
        // DbRollbackFailed.
        let vault_db = dir.path().join("vault.db");
        std::fs::remove_file(&vault_db).expect("remove vault.db");
        std::fs::create_dir(&vault_db).expect("create dir in place of vault.db");

        let result = guard.rollback();
        assert!(
            matches!(result, Err(VaultDbError::DbRollbackFailed(_))),
            "rollback must return DbRollbackFailed, got {:?}",
            result
        );

        // Clean up
        std::fs::remove_dir(&vault_db).ok();
    }
}
