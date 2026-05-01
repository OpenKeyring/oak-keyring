use std::io;

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::mpsc;
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

/// Channel buffer sizes.
const COMMAND_CHANNEL_SIZE: usize = 256;
const RESULT_CHANNEL_SIZE: usize = 256;

/// The top-level application struct. Owns all state and communication channels.
pub struct App {
    pub config: AppConfig,
    pub state: AppState,
    pub phase: AppPhase,
    /// Path to the vault data directory.
    pub vault_dir: std::path::PathBuf,
    /// UI -> Executor: send commands from screens to the executor.
    pub command_tx: mpsc::Sender<Command>,
    /// Receiver half of the command channel, taken once in run().
    command_rx: Option<mpsc::Receiver<Command>>,
    /// Executor → UI: receive results from the executor.
    result_tx: mpsc::Sender<Message>,
    result_rx: mpsc::Receiver<Message>,
    /// Cancellation token for shutting down background tasks.
    pub cancel_token: CancellationToken,
}

impl App {
    pub fn new(
        config: AppConfig,
        vault_dir: std::path::PathBuf,
        has_vault: bool,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (command_tx, command_rx) = mpsc::channel(COMMAND_CHANNEL_SIZE);
        let (result_tx, result_rx) = mpsc::channel(RESULT_CHANNEL_SIZE);
        let cancel_token = CancellationToken::new();

        Ok(Self {
            config,
            state: AppState::new(has_vault),
            phase: AppPhase::Initializing,
            vault_dir,
            command_tx,
            command_rx: Some(command_rx),
            result_tx,
            result_rx,
            cancel_token,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Build a tokio runtime for async tasks (signal handler, executor).
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
        let _guard = rt.enter();

        // Instantiate and spawn the CommandExecutor.
        // It consumes command_rx and sends results back via result_tx.
        if let Some(command_rx) = self.command_rx.take() {
            let executor = crate::executor::CommandExecutor::new(
                self.config.clone(),
                self.vault_dir.clone(),
                self.result_tx.clone(),
                self.cancel_token.clone(),
            )?;

            tokio::spawn(async move {
                executor.run(command_rx).await;
            });
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

        // Terminal cleanup (always attempt even if loop errored).
        terminal::clear_terminal_title();
        disable_raw_mode()?;
        let stdout = terminal.backend_mut();
        crossterm::execute!(stdout, LeaveAlternateScreen, DisableMouseCapture)?;

        result
    }
}
