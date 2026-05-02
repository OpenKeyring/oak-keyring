use crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::commands::types::Screen as ScreenEnum;
use crate::commands::Command;
use crate::t;
use crate::tui::state::config_state::{ConfigOverlay, ConfirmButton, DropdownField};
use crate::tui::theme;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};

use super::screen::ConfigScreen;

impl ConfigScreen {
    pub(super) fn handle_vault_path_dialog_key(
        &mut self,
        key: crossterm::event::KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        // Text input for new_path field
        match key.code {
            KeyCode::Char(c) => {
                if let Some(ref mut dialog) = self.state.vault_path_dialog {
                    dialog.new_path.push(c);
                }
                return ScreenResult::Continue;
            }
            KeyCode::Backspace => {
                if let Some(ref mut dialog) = self.state.vault_path_dialog {
                    dialog.new_path.pop();
                }
                return ScreenResult::Continue;
            }
            _ => {}
        }

        // Delegate to dialog's own button handling (Tab/Left/Right toggle, Enter/Esc)
        if let Some(ref mut dialog) = self.state.vault_path_dialog {
            match dialog.handle_key(key.code) {
                Some(true) => {
                    // Confirmed
                    if let Some(dialog) = self.state.vault_path_dialog.take() {
                        if !dialog.new_path.is_empty() {
                            self.state.general.vault_path =
                                std::path::PathBuf::from(&dialog.new_path);
                            self.state.mark_changed();
                            let config = self.state.to_app_config();
                            let _ = ctx.command_tx.try_send(Command::SaveConfig { config });
                        }
                    }
                }
                Some(false) => {
                    // Cancelled
                    self.state.vault_path_dialog = None;
                }
                None => {
                    // Focus toggle within dialog (no action needed)
                }
            }
        }
        ScreenResult::Continue
    }

    pub(super) fn handle_overlay_key(
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

pub(super) fn render_dropdown_overlay(
    frame: &mut Frame,
    area: Rect,
    field: &DropdownField,
    selected: usize,
) {
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

pub(super) fn render_unsaved_changes_dialog(
    frame: &mut Frame,
    area: Rect,
    focused_button: ConfirmButton,
) {
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
