use crate::commands::result::CommandResult;
use crate::commands::types::Screen as ScreenEnum;
use crate::commands::{Command, Message};
use crate::config::ProviderConfig;
use crate::tui::state::config_state::{ConfigOverlay, ConfigScreenState, SyncConnectionStatus};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use ratatui::layout::Rect;
use ratatui::Frame;

use super::config;
use super::overlay::{render_dropdown_overlay, render_unsaved_changes_dialog};

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

        if let Some(ref overlay) = self.state.overlay {
            match overlay {
                ConfigOverlay::Dropdown {
                    field,
                    options: _,
                    selected,
                } => {
                    render_dropdown_overlay(frame, area, field, *selected);
                }
                ConfigOverlay::UnsavedChanges { focused_button } => {
                    render_unsaved_changes_dialog(frame, area, *focused_button);
                }
            }
        }
    }

    fn on_mount(&mut self, ctx: &mut ScreenContext) {
        ctx.send_system_command(Command::LoadConfig);
    }

    fn on_unmount(&mut self) {}
}

impl ConfigScreen {
    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::ConfigLoaded { config } => {
                self.state
                    .load_from_config_preserving_restored_navigation(&config);
                ScreenResult::Continue
            }
            CommandResult::ConfigSaved { .. } => {
                self.state.clear_changes();
                ScreenResult::Continue
            }
            CommandResult::SyncConnectionTested { success, message } => {
                if success {
                    self.state.sync_status = SyncConnectionStatus::Connected;
                    self.state.sync_error_message = None;
                } else {
                    self.state.sync_status = SyncConnectionStatus::Disconnected;
                    self.state.sync_error_message = Some(message.clone());
                }
                ScreenResult::Continue
            }
            CommandResult::OAuth2Authorized {
                provider,
                access_token,
                refresh_token,
            } => {
                if provider == "google_drive" {
                    self.state.gdrive_auth_status =
                        crate::tui::state::config_state::GDriveAuthStatus::Authorized;
                    if let Some(ProviderConfig::GoogleDrive(ref mut cfg)) =
                        self.state.sync.provider_config
                    {
                        if let Some(rt) = refresh_token {
                            cfg.refresh_token = rt;
                            cfg.access_token.clear();
                        } else {
                            cfg.access_token = access_token;
                        }
                    }
                    // Auto-save config after OAuth2 success triggers SyncService rebuild
                    // via detect_changed_fields comparing provider_config (which includes tokens).
                    let config = self.state.to_app_config();
                    return ScreenResult::Command(Box::new(Command::SaveConfig { config }));
                }
                ScreenResult::Continue
            }
            CommandResult::OAuth2Failed { provider: _, error } => {
                self.state.gdrive_auth_status =
                    crate::tui::state::config_state::GDriveAuthStatus::Failed { reason: error };
                ScreenResult::Continue
            }
            CommandResult::VaultLocked => ScreenResult::NavigateTo(ScreenEnum::Unlock),
            _ => ScreenResult::Continue,
        }
    }
}
