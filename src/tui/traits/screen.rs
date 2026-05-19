use crate::commands::{Command, Message};
use crate::tui::state::notification::StatusMessage;

pub struct ScreenContext<'a> {
    pub command_tx: &'a tokio::sync::mpsc::Sender<Command>,
    pub config: &'a crate::config::AppConfig,
}

impl ScreenContext<'_> {
    /// Sends a user-triggered command, returning an error notification if the send fails.
    ///
    /// Use this for commands initiated by user actions (delete, create, copy, etc.).
    /// Returns `None` on success, or `Some(StatusMessage)` on failure.
    pub fn send_user_command(&self, cmd: Command) -> Option<StatusMessage> {
        if let Err(e) = self.command_tx.try_send(cmd) {
            tracing::warn!(error = %e, "failed to send user command");
            Some(StatusMessage::error(
                "Action failed: command queue full".to_string(),
            ))
        } else {
            None
        }
    }

    /// Sends an internal/system command, logging a warning on failure.
    ///
    /// Use this for non-user-initiated commands (auto-load on mount, background refresh, etc.).
    pub fn send_system_command(&self, cmd: Command) {
        if let Err(e) = self.command_tx.try_send(cmd) {
            tracing::warn!(error = %e, "failed to send system command");
        }
    }
}

#[derive(Debug)]
pub enum ScreenResult {
    Continue,
    NavigateTo(crate::commands::types::Screen),
    PopScreen,
    Command(Box<crate::commands::Command>),
    ExitApp,
}

pub trait Screen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult;
    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect);
    fn on_mount(&mut self, ctx: &mut ScreenContext);
    fn on_unmount(&mut self);
}
