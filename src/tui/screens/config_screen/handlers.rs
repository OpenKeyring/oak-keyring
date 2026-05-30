use crossterm::event::{KeyCode, KeyModifiers};

use crate::commands::{types::Screen as ScreenEnum, Command};
use crate::config::{
    AliyunDriveConfig, AliyunOssConfig, AnimationMode, DropboxConfig, GoogleDriveConfig,
    HealthCheckFrequency, HuaweiObsConfig, OneDriveConfig, PasswordGenerationStyle, ProviderConfig,
    S3Config, SftpConfig, SyncMode, SyncProvider, TencentCosConfig, UpyunConfig, WebDavConfig,
};
use crate::tui::state::config_state::{
    ConfigOverlay, ConfigTab, ConfirmButton, DropdownField, SyncConnectionStatus,
};
use crate::tui::traits::screen::{ScreenContext, ScreenResult};

use super::screen::ConfigScreen;

impl ConfigScreen {
    pub(super) fn handle_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        // When overlay is active, delegate to overlay key handler
        if self.state.overlay.is_some() {
            return self.handle_overlay_key(key, ctx);
        }

        // When editing password length, delegate to slider handler
        if self.state.editing_length {
            return self.handle_length_edit_key(key, ctx);
        }

        // When footer has focus, delegate to footer key handler
        if self.state.footer_focus.is_some() {
            return self.handle_footer_key(key, ctx);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                if self.state.has_changes {
                    self.state.overlay = Some(ConfigOverlay::UnsavedChanges {
                        focused_button: ConfirmButton::Stay,
                    });
                    ScreenResult::Continue
                } else {
                    ScreenResult::NavigateTo(ScreenEnum::Main)
                }
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                let config = self.state.to_app_config();
                ctx.send_system_command(Command::SaveConfig { config });
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
            (KeyCode::Up, _) | (KeyCode::Char('k'), KeyModifiers::NONE) => {
                if self.state.footer_focus.is_some() {
                    // Return from footer to content (last item)
                    self.state.footer_focus = None;
                } else {
                    let count = self.state.active_tab.item_count();
                    if self.state.focus_prev(count) {
                        self.state.boundary_flash_at = Some(std::time::Instant::now());
                    }
                }
                ScreenResult::Continue
            }
            (KeyCode::Down, _) | (KeyCode::Char('j'), KeyModifiers::NONE) => {
                let count = self.state.active_tab.item_count();
                let was_at_bottom = self.state.focus_next(count);
                if was_at_bottom {
                    // At bottom of content — move focus to footer
                    self.state.footer_focus =
                        Some(crate::tui::state::config_state::FooterButton::Close);
                }
                ScreenResult::Continue
            }
            (KeyCode::Char('q'), KeyModifiers::NONE) => ScreenResult::Continue,
            (KeyCode::PageUp, _) => {
                if self.state.overlay.is_none() {
                    let visible_height = self.state.terminal_height.saturating_sub(4);
                    self.state.scroll_page_up(visible_height.max(5));
                }
                ScreenResult::Continue
            }
            (KeyCode::PageDown, _) => {
                if self.state.overlay.is_none() {
                    let visible_height = self.state.terminal_height.saturating_sub(4);
                    let total_height = self.state.active_tab.item_count() as u16 + 1; // +1 for title
                    self.state
                        .scroll_page_down(visible_height.max(5), total_height);
                }
                ScreenResult::Continue
            }
            (KeyCode::Left, _) | (KeyCode::Right, _) => {
                self.handle_sub_item_focus(key.code);
                ScreenResult::Continue
            }
            (KeyCode::Enter, _) => self.handle_item_enter(ctx),
            _ => ScreenResult::Continue,
        }
    }

    fn handle_sub_item_focus(&mut self, key: KeyCode) {
        let state = &mut self.state;
        if state.active_tab == ConfigTab::Security && state.focused_item == 3 {
            let current = state.sub_item_focus.unwrap_or(0);
            let new = match key {
                KeyCode::Left if current > 0 => Some(current - 1),
                KeyCode::Left => Some(1),
                KeyCode::Right if current < 1 => Some(current + 1),
                KeyCode::Right => Some(0),
                _ => return,
            };
            state.sub_item_focus = new;
        }
    }

    fn handle_item_enter(&mut self, ctx: &mut ScreenContext) -> ScreenResult {
        let tab = self.state.active_tab;
        let item = self.state.active_tab.clamp_item(self.state.focused_item);

        match tab {
            ConfigTab::General => match item {
                0 => self.open_dropdown(DropdownField::Language),
                1 => self.open_dropdown(DropdownField::AutoLock),
                2 => self.open_dropdown(DropdownField::ClipboardClear),
                3 => self.open_dropdown(DropdownField::TrashRetention),
                4 => self.open_dropdown(DropdownField::Animation),
                5 => {
                    self.state.pending_import_export_mode =
                        Some(crate::tui::screens::import_export::ImportExportMode::Import);
                    ScreenResult::NavigateTo(ScreenEnum::ImportExport)
                }
                6 => {
                    self.state.pending_import_export_mode =
                        Some(crate::tui::screens::import_export::ImportExportMode::Export);
                    ScreenResult::NavigateTo(ScreenEnum::ImportExport)
                }
                _ => ScreenResult::Continue,
            },
            ConfigTab::Sync => {
                let is_gdrive = self.state.sync.provider == SyncProvider::GoogleDrive;
                match item {
                    0 => self.open_dropdown(DropdownField::SyncProvider),
                    1 => self.open_dropdown(DropdownField::SyncMode),
                    2 => {
                        if self.state.sync.sync_mode == SyncMode::Manual {
                            ScreenResult::Continue
                        } else {
                            self.open_dropdown(DropdownField::SyncInterval)
                        }
                    }
                    3 => {
                        if is_gdrive {
                            use crate::tui::state::config_state::GDriveAuthStatus;
                            if self.state.gdrive_auth_status != GDriveAuthStatus::Authorizing {
                                let _ =
                                    ctx.command_tx.try_send(Command::OAuth2AuthorizeGoogleDrive);
                                self.state.gdrive_auth_status = GDriveAuthStatus::Authorizing;
                            }
                            ScreenResult::Continue
                        } else {
                            self.state.sync_status = SyncConnectionStatus::Testing;
                            ctx.send_system_command(Command::TestSyncConnection {
                                provider_config: self.state.sync.provider_config.clone(),
                            });
                            ScreenResult::Continue
                        }
                    }
                    4 => {
                        if is_gdrive {
                            self.state.sync_status = SyncConnectionStatus::Testing;
                            ctx.send_system_command(Command::TestSyncConnection {
                                provider_config: self.state.sync.provider_config.clone(),
                            });
                        }
                        ScreenResult::Continue
                    }
                    _ => ScreenResult::Continue,
                }
            }
            ConfigTab::Security => match item {
                0 => {
                    self.state.security.health_check_enabled =
                        !self.state.security.health_check_enabled;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                1 => self.open_dropdown(DropdownField::HealthFrequency),
                2 => ScreenResult::NavigateTo(ScreenEnum::ChangeMasterPassword),
                3 => {
                    match self.state.sub_item_focus.unwrap_or(0) {
                        0 => {
                            self.state.security.audit_enabled = !self.state.security.audit_enabled;
                            self.state.mark_changed();
                        }
                        1 => {
                            return ScreenResult::NavigateTo(ScreenEnum::AuditLog);
                        }
                        _ => {}
                    }
                    ScreenResult::Continue
                }
                4 => self.open_dropdown(DropdownField::AuditRetention),
                _ => ScreenResult::Continue,
            },
            ConfigTab::Password => match item {
                0 => self.open_dropdown(DropdownField::PasswordStyle),
                1 => {
                    self.state.editing_length_original = self.state.password.length;
                    self.state.editing_length = true;
                    ScreenResult::Continue
                }
                2 => {
                    self.state.password.include_lowercase = !self.state.password.include_lowercase;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                3 => {
                    self.state.password.include_uppercase = !self.state.password.include_uppercase;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                4 => {
                    self.state.password.include_digits = !self.state.password.include_digits;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                5 => {
                    self.state.password.include_special = !self.state.password.include_special;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                6 => {
                    self.state.editing_length_original = self.state.password.memorable_word_count;
                    self.state.editing_length = true;
                    ScreenResult::Continue
                }
                7 => {
                    self.state.password.memorable_capitalize =
                        !self.state.password.memorable_capitalize;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                8 => self.open_dropdown(DropdownField::MemorableSeparator),
                9 => {
                    self.state.editing_length_original = self.state.password.pin_length;
                    self.state.editing_length = true;
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            ConfigTab::About => ScreenResult::Continue,
        }
    }

    fn handle_footer_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        _ctx: &mut ScreenContext,
    ) -> ScreenResult {
        use crate::tui::state::config_state::FooterButton;
        match key.code {
            KeyCode::Tab | KeyCode::Right => {
                self.state.footer_focus = Some(FooterButton::Close);
                ScreenResult::Continue
            }
            KeyCode::BackTab | KeyCode::Left => {
                self.state.footer_focus = Some(FooterButton::Close);
                ScreenResult::Continue
            }
            KeyCode::Enter => match self.state.footer_focus {
                Some(FooterButton::Close) => {
                    if self.state.has_changes {
                        self.state.overlay = Some(ConfigOverlay::UnsavedChanges {
                            focused_button: ConfirmButton::Stay,
                        });
                        ScreenResult::Continue
                    } else {
                        ScreenResult::NavigateTo(ScreenEnum::Main)
                    }
                }
                None => ScreenResult::Continue,
            },
            KeyCode::Esc => {
                self.state.footer_focus = None;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn open_dropdown(&mut self, field: DropdownField) -> ScreenResult {
        let options = field.options();
        let current = self.find_current_index(field);
        self.state.overlay = Some(ConfigOverlay::Dropdown {
            field,
            options,
            selected: current,
        });
        ScreenResult::Continue
    }

    fn find_current_index(&self, field: DropdownField) -> usize {
        let options = field.options();
        let current_value = match field {
            DropdownField::Language => self.state.general.language.clone(),
            DropdownField::AutoLock => self.state.general.auto_lock_seconds.to_string(),
            DropdownField::ClipboardClear => self.state.general.clipboard_clear_seconds.to_string(),
            DropdownField::TrashRetention => self.state.general.trash_retention_days.to_string(),
            DropdownField::Animation => match self.state.general.animation {
                AnimationMode::Auto => "auto".to_string(),
                AnimationMode::On => "on".to_string(),
                AnimationMode::Off => "off".to_string(),
            },
            DropdownField::SyncProvider => match self.state.sync.provider {
                SyncProvider::Disabled => "Disabled".to_string(),
                SyncProvider::ICloud => "ICloud".to_string(),
                SyncProvider::GoogleDrive => "GoogleDrive".to_string(),
                SyncProvider::Dropbox => "Dropbox".to_string(),
                SyncProvider::OneDrive => "OneDrive".to_string(),
                SyncProvider::WebDav => "WebDav".to_string(),
                SyncProvider::Sftp => "Sftp".to_string(),
                SyncProvider::S3 => "S3".to_string(),
                SyncProvider::AliyunDrive => "AliyunDrive".to_string(),
                SyncProvider::AliyunOss => "AliyunOss".to_string(),
                SyncProvider::TencentCos => "TencentCos".to_string(),
                SyncProvider::HuaweiObs => "HuaweiObs".to_string(),
                SyncProvider::Upyun => "Upyun".to_string(),
            },
            DropdownField::SyncMode => match self.state.sync.sync_mode {
                SyncMode::Auto => "Auto".to_string(),
                SyncMode::Manual => "Manual".to_string(),
            },
            DropdownField::SyncInterval => self.state.sync.auto_interval_seconds.to_string(),
            DropdownField::HealthFrequency => match self.state.security.health_check_frequency {
                HealthCheckFrequency::OnStartup => "OnStartup".to_string(),
                HealthCheckFrequency::Daily => "Daily".to_string(),
                HealthCheckFrequency::Weekly => "Weekly".to_string(),
            },
            DropdownField::AuditRetention => self.state.security.audit_retention_days.to_string(),
            DropdownField::PasswordStyle => match self.state.password.style {
                PasswordGenerationStyle::Random => "Random".to_string(),
                PasswordGenerationStyle::Memorable => "Memorable".to_string(),
                PasswordGenerationStyle::Pin => "Pin".to_string(),
            },
            DropdownField::MemorableSeparator => self.state.password.memorable_separator.clone(),
        };

        options
            .iter()
            .position(|opt| *opt == current_value)
            .unwrap_or(0)
    }

    pub(super) fn apply_dropdown_value(&mut self, field: DropdownField, value: &str) {
        match field {
            DropdownField::Language => {
                self.state.general.language = value.to_string();
            }
            DropdownField::AutoLock => {
                self.state.general.auto_lock_seconds = value.parse().unwrap_or(300);
            }
            DropdownField::ClipboardClear => {
                self.state.general.clipboard_clear_seconds = value.parse().unwrap_or(30);
            }
            DropdownField::TrashRetention => {
                self.state.general.trash_retention_days = value.parse().unwrap_or(30);
            }
            DropdownField::Animation => {
                self.state.general.animation = match value {
                    "on" => AnimationMode::On,
                    "off" => AnimationMode::Off,
                    _ => AnimationMode::Auto,
                };
            }
            DropdownField::SyncProvider => {
                self.state.sync.provider = match value {
                    "ICloud" => SyncProvider::ICloud,
                    "GoogleDrive" => SyncProvider::GoogleDrive,
                    "Dropbox" => SyncProvider::Dropbox,
                    "OneDrive" => SyncProvider::OneDrive,
                    "WebDav" => SyncProvider::WebDav,
                    "Sftp" => SyncProvider::Sftp,
                    "S3" => SyncProvider::S3,
                    "AliyunDrive" => SyncProvider::AliyunDrive,
                    "AliyunOss" => SyncProvider::AliyunOss,
                    "TencentCos" => SyncProvider::TencentCos,
                    "HuaweiObs" => SyncProvider::HuaweiObs,
                    "Upyun" => SyncProvider::Upyun,
                    _ => SyncProvider::Disabled,
                };
                // Initialize default provider_config for the new provider
                self.state.sync.provider_config = match self.state.sync.provider {
                    SyncProvider::Disabled => None,
                    SyncProvider::ICloud => Some(ProviderConfig::ICloud),
                    SyncProvider::GoogleDrive => {
                        Some(ProviderConfig::GoogleDrive(GoogleDriveConfig::default()))
                    }
                    SyncProvider::Dropbox => {
                        Some(ProviderConfig::Dropbox(DropboxConfig::default()))
                    }
                    SyncProvider::OneDrive => {
                        Some(ProviderConfig::OneDrive(OneDriveConfig::default()))
                    }
                    SyncProvider::WebDav => Some(ProviderConfig::WebDav(WebDavConfig::default())),
                    SyncProvider::Sftp => Some(ProviderConfig::Sftp(SftpConfig {
                        server: String::new(),
                        root_path: "/".to_string(),
                        ssh_key_path: String::new(),
                        host_check: Default::default(),
                    })),
                    SyncProvider::S3 => Some(ProviderConfig::S3(S3Config {
                        endpoint: None,
                        bucket: String::new(),
                        region: None,
                        access_key_id: String::new(),
                        secret_access_key: String::new(),
                        root_path: "/".to_string(),
                    })),
                    SyncProvider::AliyunDrive => {
                        Some(ProviderConfig::AliyunDrive(AliyunDriveConfig::default()))
                    }
                    SyncProvider::AliyunOss => Some(ProviderConfig::AliyunOss(AliyunOssConfig {
                        endpoint: String::new(),
                        bucket: String::new(),
                        access_key_id: String::new(),
                        access_key_secret: String::new(),
                        root_path: "/".to_string(),
                    })),
                    SyncProvider::TencentCos => {
                        Some(ProviderConfig::TencentCos(TencentCosConfig {
                            endpoint: String::new(),
                            bucket: String::new(),
                            secret_id: String::new(),
                            secret_key: String::new(),
                            root_path: "/".to_string(),
                        }))
                    }
                    SyncProvider::HuaweiObs => Some(ProviderConfig::HuaweiObs(HuaweiObsConfig {
                        endpoint: String::new(),
                        bucket: String::new(),
                        access_key_id: String::new(),
                        secret_access_key: String::new(),
                        root_path: "/".to_string(),
                    })),
                    SyncProvider::Upyun => Some(ProviderConfig::Upyun(UpyunConfig {
                        bucket: String::new(),
                        operator: String::new(),
                        operator_password: String::new(),
                        root_path: "/".to_string(),
                    })),
                };
                self.state.sync_status = SyncConnectionStatus::NotConfigured;
                self.state.sync_error_message = None;
            }
            DropdownField::SyncMode => {
                self.state.sync.sync_mode = match value {
                    "Manual" => SyncMode::Manual,
                    _ => SyncMode::Auto,
                };
            }
            DropdownField::SyncInterval => {
                self.state.sync.auto_interval_seconds = value.parse().unwrap_or(600);
            }
            DropdownField::HealthFrequency => {
                self.state.security.health_check_frequency = match value {
                    "Daily" => HealthCheckFrequency::Daily,
                    "Weekly" => HealthCheckFrequency::Weekly,
                    _ => HealthCheckFrequency::OnStartup,
                };
            }
            DropdownField::AuditRetention => {
                self.state.security.audit_retention_days = value.parse().unwrap_or(365);
            }
            DropdownField::PasswordStyle => {
                self.state.password.style = match value {
                    "Memorable" => PasswordGenerationStyle::Memorable,
                    "Pin" => PasswordGenerationStyle::Pin,
                    _ => PasswordGenerationStyle::Random,
                };
            }
            DropdownField::MemorableSeparator => {
                self.state.password.memorable_separator = value.to_string();
            }
        }
        self.state.mark_changed();
    }

    fn editable_password_number_bounds(&self) -> Option<(usize, usize)> {
        match (self.state.active_tab, self.state.focused_item) {
            (ConfigTab::Password, 1) => Some((8, 128)),
            (ConfigTab::Password, 6) => Some((3, 12)),
            (ConfigTab::Password, 9) => Some((4, 16)),
            _ => None,
        }
    }

    fn adjust_editable_password_number(&mut self, increase: bool) {
        let Some((min, max)) = self.editable_password_number_bounds() else {
            return;
        };
        let value = match self.state.focused_item {
            1 => &mut self.state.password.length,
            6 => &mut self.state.password.memorable_word_count,
            9 => &mut self.state.password.pin_length,
            _ => return,
        };

        if increase {
            if *value < max {
                *value += 1;
                self.state.mark_changed();
            }
        } else if *value > min {
            *value -= 1;
            self.state.mark_changed();
        }
    }

    fn restore_editable_password_number(&mut self) {
        match self.state.focused_item {
            1 => self.state.password.length = self.state.editing_length_original,
            6 => self.state.password.memorable_word_count = self.state.editing_length_original,
            9 => self.state.password.pin_length = self.state.editing_length_original,
            _ => {}
        }
    }

    fn handle_length_edit_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match key.code {
            KeyCode::Left => {
                self.adjust_editable_password_number(false);
                ScreenResult::Continue
            }
            KeyCode::Right => {
                self.adjust_editable_password_number(true);
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                self.state.editing_length = false;
                let config = self.state.to_app_config();
                ctx.send_system_command(Command::SaveConfig { config });
                ScreenResult::Continue
            }
            KeyCode::Esc => {
                self.restore_editable_password_number();
                self.state.editing_length = false;
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}
