use std::io;
use std::time::Duration;

use crate::instance_lock::InstanceLock;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::commands::types::AppPhase;
use crate::commands::{Command, Message};
use crate::config::AppConfig;
use crate::tui::animation::detect_animation_level;
use crate::tui::state::AppState;
use crate::tui::terminal;

pub mod signal;
pub mod update;
pub mod view;

/// Vault file existence state used to determine initial screen routing.
///
/// Four states per spec:
/// - `has_key && has_db` → full vault → UnlockScreen
/// - `!has_key && !has_db` → no vault → OnboardingScreen
/// - `!has_key && has_db` → key missing → RecoveryScreen
/// - `has_key && !has_db` → db missing → DB recovery
#[derive(Debug, Clone, Copy, Default)]
pub struct VaultInitState {
    /// Both key and db exist (full vault).
    pub has_vault: bool,
    /// Key exists but db is missing.
    pub vault_has_key_only: bool,
    /// Db exists but key is missing.
    pub vault_has_db_only: bool,
}

/// Channel buffer sizes.
const COMMAND_CHANNEL_SIZE: usize = 256;
const RESULT_CHANNEL_SIZE: usize = 256;
const EXECUTOR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// The top-level application struct. Owns all state and communication channels.
pub struct App {
    pub config: AppConfig,
    pub state: AppState,
    pub phase: AppPhase,
    /// Path to the vault data directory.
    pub vault_dir: std::path::PathBuf,
    /// Path to the config directory.
    pub config_dir: std::path::PathBuf,
    /// UI -> Executor: send commands from screens to the executor.
    pub command_tx: mpsc::Sender<Command>,
    /// Receiver half of the command channel, taken once in run().
    command_rx: Option<mpsc::Receiver<Command>>,
    /// Executor → UI: receive results from the executor.
    result_tx: mpsc::Sender<Message>,
    result_rx: mpsc::Receiver<Message>,
    /// Cancellation token for shutting down background tasks.
    pub cancel_token: CancellationToken,
    /// Instance lock to prevent multiple TUI instances from running.
    _instance_lock: InstanceLock,
    /// Startup vault file state used by UI routing and executor DB mode.
    pub vault_state: VaultInitState,
}

impl App {
    pub fn new(
        config: AppConfig,
        vault_state: VaultInitState,
        instance_lock: InstanceLock,
        vault_dir: std::path::PathBuf,
        config_dir: std::path::PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CHANNEL_SIZE);
        let (result_tx, result_rx) = mpsc::channel(RESULT_CHANNEL_SIZE);
        let cancel_token = CancellationToken::new();

        Ok(Self {
            config,
            state: AppState::new(
                vault_state.has_vault,
                vault_state.vault_has_key_only,
                vault_state.vault_has_db_only,
            ),
            phase: AppPhase::Initializing,
            vault_dir,
            config_dir,
            command_tx,
            command_rx: Some(command_rx),
            result_tx,
            result_rx,
            cancel_token,
            _instance_lock: instance_lock,
            vault_state,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Build a tokio runtime for async tasks (signal handler, executor).
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        let _guard = rt.enter();
        let mut executor_handle = None;

        // Instantiate and spawn the CommandExecutor.
        // It consumes command_rx and sends results back via result_tx.
        if let Some(command_rx) = self.command_rx.take() {
            let executor = crate::executor::CommandExecutor::new(
                self.config.clone(),
                self.result_tx.clone(),
                self.cancel_token.clone(), // shutdown_token for executor run loop
                self.vault_dir.clone(),
                self.config_dir.clone(),
                crate::executor::DbStartupMode::from_vault_state(self.vault_state),
            )?;

            executor_handle = Some(tokio::spawn(async move {
                executor.run(command_rx).await;
            }));
        }

        // Terminal setup.
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        // Detect terminal capabilities.
        let size = crossterm::terminal::size().unwrap_or((80, 24));
        let (width, height) = (size.0, size.1);
        self.state.update_size(width, height);
        self.state.unicode_capable = terminal::WidthTier::from_width(width)
            != terminal::WidthTier::TooSmall
            || std::env::var("TERM").unwrap_or_default().contains("utf");
        self.state.shared.animation.level = detect_animation_level();

        // Set terminal title.
        terminal::set_terminal_title("OpenKeyring");

        self.phase = AppPhase::Running;

        // Run the TEA event loop.
        let result = update::run(self, &mut terminal);
        self.cancel_token.cancel();

        // Terminal cleanup (always attempt even if loop errored).
        terminal::clear_terminal_title();
        disable_raw_mode()?;
        let stdout = terminal.backend_mut();
        crossterm::execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;

        if let Some(handle) = executor_handle {
            rt.block_on(wait_for_executor_shutdown(handle));
        }

        result
    }
}

async fn wait_for_executor_shutdown(handle: JoinHandle<()>) -> bool {
    match tokio::time::timeout(EXECUTOR_SHUTDOWN_TIMEOUT, handle).await {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "executor task failed during shutdown");
            false
        }
        Err(_) => {
            tracing::warn!("executor shutdown timed out");
            false
        }
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::wait_for_executor_shutdown;

    #[tokio::test]
    async fn wait_for_executor_shutdown_returns_true_when_task_finishes() {
        let handle = tokio::spawn(async {});

        assert!(wait_for_executor_shutdown(handle).await);
    }

    #[tokio::test]
    async fn wait_for_executor_shutdown_returns_false_when_task_times_out() {
        let handle = tokio::spawn(async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        });

        assert!(!wait_for_executor_shutdown(handle).await);
    }
}
