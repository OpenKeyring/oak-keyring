//! Vault path change confirmation dialog.
//!
//! Rendered as a centered overlay when the user changes the vault path in the
//! config screen. Shows the current path, new path, explains which files are
//! stored in the vault directory, and displays a warning before confirming.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::tui::theme;

// ── Colour / layout constants ────────────────────────────────────

const OVERLAY_BG: Color = Color::Rgb(26, 27, 38); // #1a1b26
const DIALOG_WIDTH: u16 = 50;
const DIALOG_HEIGHT: u16 = 16;

const LABEL_COLOR: Color = Color::Rgb(86, 95, 137); // #565f89
const VALUE_COLOR: Color = Color::Rgb(192, 202, 245); // #c0caf5
const WARNING_COLOR: Color = Color::Rgb(255, 158, 100); // #ff9e64
const BUTTON_COLOR: Color = Color::Rgb(122, 162, 247); // #7aa2f7

// ── Public types ─────────────────────────────────────────────────

/// Which button is currently focused in the dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultPathButton {
    Cancel,
    Confirm,
}

impl VaultPathButton {
    /// Toggle between Cancel and Confirm.
    pub fn toggle(self) -> Self {
        match self {
            Self::Cancel => Self::Confirm,
            Self::Confirm => Self::Cancel,
        }
    }
}

/// Reusable vault-path confirmation dialog component.
pub struct VaultPathDialog {
    pub current_path: String,
    pub new_path: String,
    pub focused_button: VaultPathButton,
}

impl VaultPathDialog {
    pub fn new(current: String, new_path: String) -> Self {
        Self {
            current_path: current,
            new_path,
            focused_button: VaultPathButton::Cancel, // safe default
        }
    }

    // ── Rendering ────────────────────────────────────────────────

    /// Render the dialog centred within `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let width = DIALOG_WIDTH.min(area.width);
        let height = DIALOG_HEIGHT.min(area.height);
        let overlay_rect = centered_rect(area, width, height);
        let content_width = width.saturating_sub(2); // inside borders

        let block = Block::default()
            .title(Span::styled(
                " 修改 Vault 路径 ",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER))
            .style(Style::default().bg(OVERLAY_BG));

        let lines = self.build_lines(content_width);

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        frame.render_widget(Clear, overlay_rect);
        frame.render_widget(paragraph, overlay_rect);
    }

    // ── Key handling ─────────────────────────────────────────────

    /// Handle a key press while the dialog is active.
    ///
    /// Returns:
    /// - `Some(true)` -- user confirmed
    /// - `Some(false)` -- user cancelled
    /// - `None` -- key not consumed (focus toggled)
    pub fn handle_key(&mut self, key: crossterm::event::KeyCode) -> Option<bool> {
        use crossterm::event::KeyCode;

        match key {
            KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                self.focused_button = self.focused_button.toggle();
                None
            }
            KeyCode::Enter => Some(matches!(self.focused_button, VaultPathButton::Confirm)),
            KeyCode::Esc => Some(false),
            _ => None,
        }
    }

    // ── Internal helpers ─────────────────────────────────────────

    fn build_lines(&self, content_width: u16) -> Vec<Line<'static>> {
        let label = Style::default().fg(LABEL_COLOR);
        let value = Style::default().fg(VALUE_COLOR);
        let warning = Style::default().fg(WARNING_COLOR);

        let current_display = truncate_path(&self.current_path, content_width);
        let new_display = truncate_path(&self.new_path, content_width);

        let lines = vec![
            // Current path
            label_line("当前路径:", label),
            value_line(&current_display, value),
            Line::raw(""),
            // New path
            label_line("新路径:", label),
            value_line(&new_display, value),
            Line::raw(""),
            // Vault directory contents explanation
            label_line("Vault 目录包含以下文件:", label),
            file_list_line("vault.db", "加密 SQLite 数据库（所有密码数据）", value),
            file_list_line("metadata.json", "Vault 元信息（版本、设备 ID 等）", value),
            file_list_line("config.toml", "客户端配置（同步、快捷键等）", value),
            Line::raw(""),
            // Warning
            Line::from(Span::styled(
                " \u{26A0} 修改路径后需要重启应用才能生效。",
                warning,
            )),
            Line::from(Span::styled(
                "    现有数据不会自动迁移。",
                warning,
            )),
            // Separator
            separator_line(content_width),
            // Buttons
            render_buttons(self.focused_button),
        ];

        lines
    }
}

// ── Rendering helpers ────────────────────────────────────────────

fn label_line(text: &str, style: Style) -> Line<'static> {
    Line::from(Span::styled(format!(" {}", text), style))
}

fn value_line(text: &str, style: Style) -> Line<'static> {
    Line::from(Span::styled(format!("   {}", text), style))
}

fn file_list_line(name: &str, desc: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled("   ".to_string(), Style::default()),
        Span::styled(name.to_string(), style),
        Span::styled(format!(" — {}", desc), Style::default().fg(LABEL_COLOR)),
    ])
}

fn separator_line(content_width: u16) -> Line<'static> {
    let dash_count = content_width as usize;
    Line::from(Span::styled(
        "─".repeat(dash_count),
        Style::default().fg(theme::BORDER),
    ))
}

fn render_buttons(focused: VaultPathButton) -> Line<'static> {
    let cancel_style = if matches!(focused, VaultPathButton::Cancel) {
        Style::default()
            .fg(theme::TEXT_SECONDARY)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let confirm_style = if matches!(focused, VaultPathButton::Confirm) {
        Style::default()
            .fg(BUTTON_COLOR)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(BUTTON_COLOR)
    };

    Line::from(vec![
        Span::raw("     "),
        Span::styled(" [ 取消 ] ".to_string(), cancel_style),
        Span::raw("  "),
        Span::styled(" [ 确认修改 ] ".to_string(), confirm_style),
    ])
}

/// Truncate a path for display if it exceeds `max_width` characters.
fn truncate_path(path: &str, max_width: u16) -> String {
    let max = max_width as usize;
    if path.len() <= max {
        path.to_string()
    } else {
        let prefix_len = max.saturating_sub(3);
        format!("{}...", &path[..prefix_len])
    }
}

/// Return a `Rect` of size `width x height` centred inside `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area
        .x
        .checked_add((area.width.saturating_sub(width)) / 2)
        .unwrap_or(area.x);
    let y = area
        .y
        .checked_add((area.height.saturating_sub(height)) / 2)
        .unwrap_or(area.y);
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_toggle() {
        assert_eq!(VaultPathButton::Cancel.toggle(), VaultPathButton::Confirm);
        assert_eq!(VaultPathButton::Confirm.toggle(), VaultPathButton::Cancel);
    }

    #[test]
    fn handle_key_tab_toggles_focus() {
        let mut dialog = VaultPathDialog::new("/old".into(), "/new".into());
        assert_eq!(dialog.focused_button, VaultPathButton::Cancel);

        dialog.handle_key(crossterm::event::KeyCode::Tab);
        assert_eq!(dialog.focused_button, VaultPathButton::Confirm);

        dialog.handle_key(crossterm::event::KeyCode::Tab);
        assert_eq!(dialog.focused_button, VaultPathButton::Cancel);

        // Arrow keys also toggle
        dialog.handle_key(crossterm::event::KeyCode::Right);
        assert_eq!(dialog.focused_button, VaultPathButton::Confirm);

        dialog.handle_key(crossterm::event::KeyCode::Left);
        assert_eq!(dialog.focused_button, VaultPathButton::Cancel);
    }

    #[test]
    fn handle_key_enter_on_cancel() {
        let mut dialog = VaultPathDialog::new("/old".into(), "/new".into());
        let result = dialog.handle_key(crossterm::event::KeyCode::Enter);
        assert_eq!(result, Some(false));
    }

    #[test]
    fn handle_key_enter_on_confirm() {
        let mut dialog = VaultPathDialog::new("/old".into(), "/new".into());
        dialog.focused_button = VaultPathButton::Confirm;
        let result = dialog.handle_key(crossterm::event::KeyCode::Enter);
        assert_eq!(result, Some(true));
    }

    #[test]
    fn handle_key_esc_cancels() {
        let mut dialog = VaultPathDialog::new("/old".into(), "/new".into());
        dialog.focused_button = VaultPathButton::Confirm;
        let result = dialog.handle_key(crossterm::event::KeyCode::Esc);
        assert_eq!(result, Some(false));
    }

    #[test]
    fn handle_key_unknown_returns_none() {
        let mut dialog = VaultPathDialog::new("/old".into(), "/new".into());
        let result = dialog.handle_key(crossterm::event::KeyCode::Char('a'));
        assert_eq!(result, None);
    }

    #[test]
    fn default_focus_is_cancel() {
        let dialog = VaultPathDialog::new("/old".into(), "/new".into());
        assert_eq!(dialog.focused_button, VaultPathButton::Cancel);
    }

    #[test]
    fn centered_rect_within_bounds() {
        let area = Rect::new(0, 0, 100, 40);
        let rect = centered_rect(area, 50, 16);
        assert_eq!(rect.width, 50);
        assert_eq!(rect.height, 16);
        assert_eq!(rect.x, (100 - 50) / 2);
        assert_eq!(rect.y, (40 - 16) / 2);
    }

    #[test]
    fn centered_rect_clamps_to_area() {
        let area = Rect::new(0, 0, 20, 5);
        let rect = centered_rect(area, 50, 16);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 5);
    }

    #[test]
    fn truncate_path_short_enough() {
        assert_eq!(truncate_path("/short/path", 48), "/short/path");
    }

    #[test]
    fn truncate_path_too_long() {
        let long = "/a/very/long/path/that/exceeds/the/maximum/width/of/the/dialog";
        let result = truncate_path(long, 20);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 20);
    }

    #[test]
    fn build_lines_contains_expected_sections() {
        let dialog = VaultPathDialog::new("/old/path".into(), "/new/path".into());
        let lines = dialog.build_lines(48);

        // Should contain labels for current path, new path, file list, warning, separator, buttons
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.clone()))
            .collect::<Vec<_>>()
            .join("");

        assert!(text.contains("当前路径"));
        assert!(text.contains("新路径"));
        assert!(text.contains("vault.db"));
        assert!(text.contains("metadata.json"));
        assert!(text.contains("config.toml"));
        assert!(text.contains("重启应用"));
        assert!(text.contains("取消"));
        assert!(text.contains("确认修改"));
    }

    #[test]
    fn render_buttons_cancel_focused() {
        let line = render_buttons(VaultPathButton::Cancel);
        let has_reversed = line.spans.iter().any(|s| {
            s.style.add_modifier == Modifier::REVERSED
                || s.style.add_modifier.contains(Modifier::REVERSED)
        });
        // The cancel button should have REVERSED when focused
        assert!(has_reversed);
    }

    #[test]
    fn render_buttons_confirm_focused() {
        let line = render_buttons(VaultPathButton::Confirm);
        let has_reversed = line.spans.iter().any(|s| {
            s.style.add_modifier == Modifier::REVERSED
                || s.style.add_modifier.contains(Modifier::REVERSED)
        });
        assert!(has_reversed);
    }
}
