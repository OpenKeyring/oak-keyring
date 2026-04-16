use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::commands::result::CommandResult;
use crate::commands::types::Screen as ScreenEnum;
use crate::commands::{Command, Message};
use crate::config::{
    AliyunDriveConfig, AliyunOssConfig, AnimationMode, DropboxConfig, GoogleDriveConfig,
    HealthCheckFrequency, HuaweiObsConfig, OneDriveConfig, ProviderConfig, S3Config, SftpConfig,
    SyncMode, SyncProvider, TencentCosConfig, UpyunConfig, WebDavConfig,
};
use crate::t;
use crate::tui::state::config_state::{
    ConfigOverlay, ConfigScreenState, ConfigTab, ConfirmButton, DropdownField, SyncConnectionStatus,
};
use crate::tui::theme;
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
        // TODO(U7.5/L1): ctx.focus_stack.push(ScreenSnapshot::Main);
        // Push main screen snapshot to focus stack for restoration on close.
        // Depends on Plan L1 (UI Infrastructure) focus stack implementation.
        let _ = ctx.command_tx.try_send(Command::LoadConfig);
    }

    fn on_unmount(&mut self) {
        // TODO(U7.5/L1): Focus stack pop handled by screen router.
    }
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
            CommandResult::SyncConnectionTested {
                success,
                message: _,
            } => {
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
        // When overlay is active, delegate to overlay key handler
        if self.state.overlay.is_some() {
            return self.handle_overlay_key(key, ctx);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                if self.state.has_changes {
                    self.state.overlay = Some(ConfigOverlay::UnsavedChanges {
                        focused_button: ConfirmButton::Cancel,
                    });
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
                let count = self.state.active_tab.item_count();
                self.state.focus_prev(count);
                ScreenResult::Continue
            }
            (KeyCode::Down, _) => {
                let count = self.state.active_tab.item_count();
                self.state.focus_next(count);
                ScreenResult::Continue
            }
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
            (KeyCode::Enter, _) => self.handle_item_enter(ctx),
            _ => ScreenResult::Continue,
        }
    }

    fn handle_item_enter(&mut self, ctx: &mut ScreenContext) -> ScreenResult {
        let tab = self.state.active_tab;
        let item = self.state.active_tab.clamp_item(self.state.focused_item);

        match tab {
            ConfigTab::General => match item {
                0 => self.open_dropdown(DropdownField::Language),
                1 => {
                    // TODO: VaultPathDialog — complex, deferred to a later task
                    ScreenResult::Continue
                }
                2 => self.open_dropdown(DropdownField::AutoLock),
                3 => self.open_dropdown(DropdownField::ClipboardClear),
                4 => self.open_dropdown(DropdownField::TrashRetention),
                5 => self.open_dropdown(DropdownField::Animation),
                6 => ScreenResult::NavigateTo(ScreenEnum::ImportExport),
                _ => ScreenResult::Continue,
            },
            ConfigTab::Sync => match item {
                0 => self.open_dropdown(DropdownField::SyncProvider),
                1 => self.open_dropdown(DropdownField::SyncMode),
                2 => {
                    // Skip interval dropdown when sync mode is Manual
                    if self.state.sync.sync_mode == SyncMode::Manual {
                        ScreenResult::Continue
                    } else {
                        self.open_dropdown(DropdownField::SyncInterval)
                    }
                }
                3 => {
                    self.state.sync_status = SyncConnectionStatus::Testing;
                    let _ = ctx.command_tx.try_send(Command::TestSyncConnection {
                        provider_config: self.state.sync.provider_config.clone(),
                    });
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
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
                    // TODO(U8): Audit log navigation — "查看记录" link shares this row with
                    // the audit toggle. When the UI supports sub-item focus, pressing Enter
                    // on the "查看记录" link should navigate to Screen::AuditLog.
                    // For now, Enter on this row toggles the audit switch.
                    self.state.security.audit_enabled = !self.state.security.audit_enabled;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                4 => self.open_dropdown(DropdownField::AuditRetention),
                _ => ScreenResult::Continue,
            },
            ConfigTab::Password => match item {
                0 => {
                    // Length is read-only for now
                    ScreenResult::Continue
                }
                1 => {
                    self.state.password.include_digits = !self.state.password.include_digits;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                2 => {
                    self.state.password.include_uppercase = !self.state.password.include_uppercase;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                3 => {
                    self.state.password.include_special = !self.state.password.include_special;
                    self.state.mark_changed();
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            ConfigTab::About => ScreenResult::Continue,
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
        };

        options
            .iter()
            .position(|opt| *opt == current_value)
            .unwrap_or(0)
    }

    fn apply_dropdown_value(&mut self, field: DropdownField, value: &str) {
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
        }
        self.state.mark_changed();
    }

    fn handle_overlay_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match self.state.overlay {
            Some(ConfigOverlay::Dropdown {
                ref mut selected,
                ref options,
                ..
            }) => match key.code {
                KeyCode::Up => {
                    if *selected > 0 {
                        *selected -= 1;
                    }
                    ScreenResult::Continue
                }
                KeyCode::Down => {
                    if *selected + 1 < options.len() {
                        *selected += 1;
                    }
                    ScreenResult::Continue
                }
                KeyCode::Enter => {
                    let (field, selected) = match self.state.overlay {
                        Some(ConfigOverlay::Dropdown {
                            field,
                            options: _,
                            selected,
                        }) => (field, selected),
                        _ => unreachable!(),
                    };
                    let options = field.options();
                    let value = options[selected].clone();
                    self.apply_dropdown_value(field, &value);
                    self.state.overlay = None;
                    ScreenResult::Continue
                }
                KeyCode::Esc => {
                    self.state.overlay = None;
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            Some(ConfigOverlay::UnsavedChanges {
                ref mut focused_button,
            }) => match key.code {
                KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
                    *focused_button = focused_button.toggle();
                    ScreenResult::Continue
                }
                KeyCode::Enter => {
                    let button = *focused_button;
                    self.state.overlay = None;
                    match button {
                        ConfirmButton::Cancel => ScreenResult::Continue,
                        ConfirmButton::Confirm => {
                            let config = self.state.to_app_config();
                            let _ = ctx.command_tx.try_send(Command::SaveConfig { config });
                            ScreenResult::NavigateTo(ScreenEnum::Main)
                        }
                    }
                }
                KeyCode::Esc => {
                    self.state.overlay = None;
                    ScreenResult::Continue
                }
                _ => ScreenResult::Continue,
            },
            None => ScreenResult::Continue,
        }
    }
}

// ── Overlay Rendering ─────────────────────────────────────────────────────────

fn render_dropdown_overlay(frame: &mut Frame, area: Rect, field: &DropdownField, selected: usize) {
    // Clear the area first
    frame.render_widget(Clear, area);

    // Get translated display labels
    let labels = field.display_labels();

    // Popup dimensions
    let max_visible = 8usize;
    let visible_count = labels.len().min(max_visible);
    let popup_height = visible_count as u16 + 2; // +2 for border
                                                 // Calculate popup width based on longest translated label
    let max_label_width = labels.iter().map(|l| l.len()).max().unwrap_or(10).max(10);
    let popup_width = (max_label_width as u16 + 6).min(area.width).max(20);

    // Center the popup
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let border_style = Style::default().fg(theme::PRIMARY);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", field.label()))
        .border_style(border_style);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    // Render option rows
    let row_heights: Vec<Constraint> = (0..visible_count).map(|_| Constraint::Length(1)).collect();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_heights)
        .split(inner);

    for (i, row_area) in rows.iter().enumerate() {
        if i >= labels.len() {
            break;
        }
        let is_selected = i == selected;
        let style = if is_selected {
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT)
        };
        let prefix = if is_selected { " > " } else { "   " };
        let text = format!("{}{}", prefix, labels[i]);
        frame.render_widget(Paragraph::new(text).style(style), *row_area);
    }
}

fn render_unsaved_changes_dialog(frame: &mut Frame, area: Rect, focused_button: ConfirmButton) {
    // Clear the area first
    frame.render_widget(Clear, area);

    let popup_height = 5u16;
    let popup_width = 40u16.min(area.width);

    // Center the popup
    let popup_x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = area.y + (area.height.saturating_sub(popup_height)) / 2;
    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    let border_style = Style::default().fg(theme::WARNING);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", t!("tui.config.unsaved_dialog_title")))
        .border_style(border_style);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Message
            Constraint::Length(1), // Buttons
        ])
        .split(inner);

    // Warning message
    let msg = Paragraph::new(format!(" {}", t!("tui.config.unsaved_dialog_message")))
        .style(Style::default().fg(theme::WARNING));
    frame.render_widget(msg, chunks[0]);

    // Buttons
    let cancel_style = if focused_button == ConfirmButton::Cancel {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };
    let confirm_style = if focused_button == ConfirmButton::Confirm {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().fg(theme::PRIMARY)
    };

    let buttons = Line::from(vec![
        Span::styled(format!(" <{}> ", t!("tui.config.cancel_btn")), cancel_style),
        Span::styled("   ", Style::default()),
        Span::styled(
            format!(" <{}> ", t!("tui.config.save_exit_btn")),
            confirm_style,
        ),
    ]);
    frame.render_widget(Paragraph::new(buttons), chunks[1]);
}
