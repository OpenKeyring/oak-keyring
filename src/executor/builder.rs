//! ExecutorBuilder — the single construction and test-injection boundary.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::commands::Message;
use crate::config::notification::ServiceNotification;
use crate::config::AppConfig;
use crate::services::clipboard::{Clipboard, ClipboardService};
use crate::services::health::{Health, HealthServiceImpl};
use crate::services::import_export::{ImportExport, ImportExportServiceImpl};
use crate::services::sync::SyncService;
use crate::services::vault::Vault;

use super::config_impl::{ClipboardConfigAdapter, ConfigManagerImpl, ServiceNotificationImpl};
use super::runtime;
use super::CommandExecutor;

#[derive(Debug, thiserror::Error)]
pub enum ExecutorBuildError {
    #[error("ExecutorBuilder requires {field}")]
    MissingField { field: &'static str },
}

fn required<T>(value: Option<T>, field: &'static str) -> Result<T, ExecutorBuildError> {
    value.ok_or(ExecutorBuildError::MissingField { field })
}

/// Builder for [`CommandExecutor`] — the single dependency-injection boundary.
///
/// The builder holds all the same fields as CommandExecutor but provides setter
/// methods for test injection. Production construction typically uses only the
/// required fields (vault, config, result_tx, shutdown_token) and relies on
/// builder defaults for services (health, clipboard, import_export).
pub struct ExecutorBuilder {
    vault_runtime: Option<runtime::VaultRuntime>,
    vault_db_file_backed: bool,
    sync: Option<Box<dyn SyncService>>,
    health: Option<Arc<dyn Health>>,
    clipboard: Option<Arc<dyn Clipboard>>,
    import_export: Option<Box<dyn ImportExport>>,
    config: Option<AppConfig>,
    config_dir: PathBuf,
    vault_dir: PathBuf,
    result_tx: Option<mpsc::Sender<Message>>,
    shutdown_token: Option<CancellationToken>,
    health_report: Option<crate::commands::types::HealthReport>,
    last_health_check_time: Option<chrono::DateTime<chrono::Utc>>,
    verified_master_password: Option<crate::types::SecureStr>,
    oauth2_token_store: Option<Arc<tokio::sync::Mutex<Option<crate::cloud::oauth2::TokenStore>>>>,
}

impl ExecutorBuilder {
    /// Create a new builder with minimal required fields.
    ///
    /// # Arguments
    /// * `vault_dir` — Path to the vault directory (contains vault.db, config.toml, etc.)
    /// * `config_dir` — Path to the config directory (contains config.toml)
    ///
    /// All other fields are optional and will use sensible defaults.
    #[must_use]
    pub fn new(vault_dir: PathBuf, config_dir: PathBuf) -> Self {
        Self {
            vault_runtime: None,
            vault_db_file_backed: false,
            sync: None,
            health: None,
            clipboard: None,
            import_export: None,
            config: None,
            config_dir,
            vault_dir,
            result_tx: None,
            shutdown_token: None,
            health_report: None,
            last_health_check_time: None,
            verified_master_password: None,
            oauth2_token_store: None,
        }
    }

    /// Set the vault service (required) — wraps in `VaultRuntime::open`.
    #[must_use]
    pub fn vault(mut self, vault: Box<dyn Vault>) -> Self {
        self.vault_runtime = Some(runtime::VaultRuntime::open(vault));
        self
    }

    /// Set the vault runtime directly.
    #[must_use]
    pub fn vault_runtime(mut self, vault_runtime: runtime::VaultRuntime) -> Self {
        self.vault_runtime = Some(vault_runtime);
        self
    }

    /// Set whether the vault database is file-backed (default: false).
    #[must_use]
    pub const fn vault_db_file_backed(mut self, backed: bool) -> Self {
        self.vault_db_file_backed = backed;
        self
    }

    /// Set the sync service (optional, for test injection).
    #[must_use]
    pub fn sync(mut self, sync: Option<Box<dyn SyncService>>) -> Self {
        self.sync = sync;
        self
    }

    /// Set the health service (optional, defaults to `HealthServiceImpl`).
    #[must_use]
    pub fn health(mut self, health: Arc<dyn Health>) -> Self {
        self.health = Some(health);
        self
    }

    /// Set the clipboard service (optional, defaults to `ClipboardService::new_safe`).
    #[must_use]
    pub fn clipboard(mut self, clipboard: Arc<dyn Clipboard>) -> Self {
        self.clipboard = Some(clipboard);
        self
    }

    /// Set the import/export service (optional, defaults to `ImportExportServiceImpl`).
    #[must_use]
    pub fn import_export(mut self, ie: Box<dyn ImportExport>) -> Self {
        self.import_export = Some(ie);
        self
    }

    /// Set the application configuration (required).
    #[must_use]
    pub fn config(mut self, config: AppConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the result channel sender (required).
    #[must_use]
    pub fn result_tx(mut self, tx: mpsc::Sender<Message>) -> Self {
        self.result_tx = Some(tx);
        self
    }

    /// Set the shutdown token (required).
    #[must_use]
    pub fn shutdown_token(mut self, token: CancellationToken) -> Self {
        self.shutdown_token = Some(token);
        self
    }

    /// Set the cached health report (optional, for test fixtures).
    #[must_use]
    pub fn health_report(mut self, report: crate::commands::types::HealthReport) -> Self {
        self.health_report = Some(report);
        self
    }

    /// Set the last health check time (optional, for test fixtures).
    #[must_use]
    pub fn last_health_check_time(mut self, time: chrono::DateTime<chrono::Utc>) -> Self {
        self.last_health_check_time = Some(time);
        self
    }

    /// Set the verified master password (optional, for test fixtures).
    #[must_use]
    pub fn verified_master_password(mut self, pw: crate::types::SecureStr) -> Self {
        self.verified_master_password = Some(pw);
        self
    }

    /// Set the OAuth2 token store (optional, defaults to empty store).
    #[must_use]
    pub fn oauth2_token_store(
        mut self,
        store: Arc<tokio::sync::Mutex<Option<crate::cloud::oauth2::TokenStore>>>,
    ) -> Self {
        self.oauth2_token_store = Some(store);
        self
    }

    /// Build the [`CommandExecutor`].
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing:
    /// - `vault`
    /// - `config`
    /// - `result_tx`
    /// - `shutdown_token`
    ///
    /// All other fields use sensible defaults:
    /// - `health`: defaults to `HealthServiceImpl`
    /// - `import_export`: defaults to `ImportExportServiceImpl`
    /// - `clipboard`: defaults to `ClipboardService::new_safe` with fallback to disabled backend
    /// - `vault_db_file_backed`: defaults to `false`
    /// - `sync`: defaults to `None`
    /// - `health_report`, `last_health_check_time`, `verified_master_password`: default to `None`
    /// - `oauth2_token_store`: defaults to empty store
    ///
    /// # Note
    ///
    /// The builder does NOT create the sync service from config. The caller (including
    /// [`CommandExecutor::new`]) is responsible for creating sync and passing it via
    /// `.sync()`. This keeps the builder pure and testable.
    pub fn build(self) -> Result<CommandExecutor, ExecutorBuildError> {
        let vault_runtime = required(self.vault_runtime, "a vault")?;
        let config = required(self.config, "config")?;
        let result_tx = required(self.result_tx, "result_tx")?;
        let shutdown_token = required(self.shutdown_token, "shutdown_token")?;

        // Default services if not explicitly set
        let health = self
            .health
            .unwrap_or_else(|| Arc::new(HealthServiceImpl::new()));
        let import_export = self
            .import_export
            .unwrap_or_else(|| Box::new(ImportExportServiceImpl::new()));
        let clipboard = self.clipboard.unwrap_or_else(|| {
            // Default clipboard - may fail in headless, so use disabled backend
            let timeout = config.general.clipboard_clear_seconds;
            match ClipboardService::new_safe(timeout) {
                Ok(svc) => Arc::new(svc) as Arc<dyn Clipboard>,
                Err(_) => {
                    // Fallback to a disabled clipboard for headless/CI environments
                    Arc::new(ClipboardService::with_backend(
                        Box::new(crate::services::clipboard::MockBackend::new()),
                        timeout,
                    )) as Arc<dyn Clipboard>
                }
            }
        });

        // Create internal signaling channel
        let (internal_tx, internal_rx) = mpsc::channel(64);

        // Operation cancel token is a child of shutdown token
        let operation_cancel_token = shutdown_token.child_token();

        // Register clipboard service for config-change notifications
        let mut config_notifier = ServiceNotificationImpl::new();
        config_notifier.register_service(Box::new(ClipboardConfigAdapter::new(Arc::clone(
            &clipboard,
        ))));

        Ok(CommandExecutor {
            vault_runtime,
            vault_db_file_backed: self.vault_db_file_backed,
            sync: self.sync,
            health,
            clipboard,
            import_export,
            config: ConfigManagerImpl::new(config, self.config_dir.clone()),
            config_notifier,
            vault_dir: self.vault_dir,
            config_dir: self.config_dir,
            health_report: self.health_report,
            last_health_check_time: self.last_health_check_time,
            verified_master_password: self.verified_master_password,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            shutdown_token,
            operation_cancel_token,
            timer_rebuild_pending: false,
            oauth2_token_store: self
                .oauth2_token_store
                .unwrap_or_else(|| Arc::new(tokio::sync::Mutex::new(None))),
        })
    }
}
