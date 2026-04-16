use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{Frame, layout::Rect};

use crate::commands::{Command, Message};
use crate::commands::result::CommandResult;
use crate::commands::types::Screen as ScreenEnum;
use crate::tui::state::config_state::{ConfigTab, ConfigScreenState, SyncConnectionStatus};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};

mod config;

pub struct ConfigScreen {
    pub state: ConfigScreenState,
}

impl ConfigScreen {
    pub fn new() -> Self {
        Self {
            state: ConfigScreenState::default(),
        }
    }
}

impl Default for ConfigScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Screen for ConfigScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::CommandCompleted(result) => self.handle_command_result(result),
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        config::render::render(frame, area, &self.state);
    }

    fn on_mount(&mut self, ctx: &mut ScreenContext) {
        let _ = ctx.command_tx.try_send(Command::LoadConfig);
    }

    fn on_unmount(&mut self) {}
}

impl ConfigScreen {
    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::ConfigLoaded { config } => {
                self.state.load_from_config(&config);
                ScreenResult::Continue
            }
            CommandResult::ConfigSaved => {
                self.state.clear_changes();
                ScreenResult::Continue
            }
            CommandResult::SyncConnectionTested { success, message: _ } => {
                self.state.sync_status = if success {
                    SyncConnectionStatus::Connected
                } else {
                    SyncConnectionStatus::Disconnected
                };
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                if self.state.has_changes {
                    // TODO: show unsaved changes dialog
                    ScreenResult::Continue
                } else {
                    ScreenResult::NavigateTo(ScreenEnum::Main)
                }
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                let config = self.state.to_app_config();
                let _ = ctx.command_tx.try_send(Command::SaveConfig { config });
                ScreenResult::Continue
            }
            (KeyCode::Tab, _) => {
                let tabs = ConfigTab::all();
                let current_idx = self.state.active_tab.index();
                let next_idx = (current_idx + 1) % tabs.len();
                self.state.switch_tab(tabs[next_idx]);
                ScreenResult::Continue
            }
            (KeyCode::BackTab, _) => {
                let tabs = ConfigTab::all();
                let current_idx = self.state.active_tab.index();
                let prev_idx = (current_idx + tabs.len() - 1) % tabs.len();
                self.state.switch_tab(tabs[prev_idx]);
                ScreenResult::Continue
            }
            (KeyCode::Up, _) => {
                self.state.focus_prev(20); // approximate, each tab has different counts
                ScreenResult::Continue
            }
            (KeyCode::Down, _) => {
                self.state.focus_next(20);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}
