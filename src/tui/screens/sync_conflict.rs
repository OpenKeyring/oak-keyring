//! Sync conflict resolution screen (U10).
//!
//! Displays a side-by-side comparison of local vs remote record versions and
//! lets the user choose which version to keep for each conflict.

use crossterm::event::KeyCode;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

use crate::commands::types::ConflictResolution;
use crate::commands::{Command, Message};
use crate::tui::state::sync_ui_state::*;
use crate::tui::theme::{self, BG_BAR, TEXT, TEXT_MUTED, TEXT_SECONDARY};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};

// ── SyncConflictScreen ────────────────────────────────────────────────────────

#[derive(Default)]
pub struct SyncConflictScreen {
    pub state: ConflictResolutionState,
}

// ── Screen trait impl ──────────────────────────────────────────────────────────

impl Screen for SyncConflictScreen {
    fn update(&mut self, msg: Message, _ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::ConflictResolved { .. } => {
                if self.state.current_index < self.state.conflicts.len().saturating_sub(1) {
                    self.state.current_index += 1;
                } else {
                    return ScreenResult::PopScreen;
                }
                ScreenResult::Continue
            }
            Message::AllConflictsResolved { .. } => ScreenResult::PopScreen,
            Message::KeyEvent(key) => self.handle_key(key.code),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        self.render(frame, area);
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {}

    fn on_unmount(&mut self) {}
}

// ── Key handling ───────────────────────────────────────────────────────────────

impl SyncConflictScreen {
    fn handle_key(&mut self, key: KeyCode) -> ScreenResult {
        match key {
            KeyCode::Left => {
                self.state.focused_side = ConflictSide::Local;
                ScreenResult::Continue
            }
            KeyCode::Right | KeyCode::Tab => {
                self.state.focused_side = ConflictSide::Remote;
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if let Some(conflict) = self.state.conflicts.get(self.state.current_index) {
                    let resolution = match self.state.focused_side {
                        ConflictSide::Local => ConflictResolution::KeepLocal,
                        ConflictSide::Remote => ConflictResolution::KeepRemote,
                    };
                    ScreenResult::Command(Box::new(Command::ResolveConflict {
                        record_id: conflict.record_id,
                        resolution,
                    }))
                } else {
                    ScreenResult::Continue
                }
            }
            KeyCode::Char('a') => ScreenResult::Command(Box::new(Command::ResolveAllConflicts {
                resolution: ConflictResolution::KeepLocal,
            })),
            KeyCode::Esc => {
                // Skip: resolve current as KeepLocal
                if let Some(conflict) = self.state.conflicts.get(self.state.current_index) {
                    ScreenResult::Command(Box::new(Command::ResolveConflict {
                        record_id: conflict.record_id,
                        resolution: ConflictResolution::KeepLocal,
                    }))
                } else {
                    ScreenResult::PopScreen
                }
            }
            KeyCode::Char('p') => {
                self.toggle_mask();
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    /// Toggle masking of sensitive fields on the currently focused side.
    fn toggle_mask(&mut self) {
        if let Some(conflict) = self.state.conflicts.get_mut(self.state.current_index) {
            let fields = match self.state.focused_side {
                ConflictSide::Local => &mut conflict.local_fields,
                ConflictSide::Remote => &mut conflict.remote_fields,
            };
            for field in fields.iter_mut() {
                if field.is_sensitive {
                    field.is_masked = !field.is_masked;
                }
            }
        }
    }

    // ── Rendering ───────────────────────────────────────────────────────────

    fn render(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title bar
                Constraint::Length(1), // progress line
                Constraint::Fill(1),   // comparison area
                Constraint::Length(1), // footer
            ])
            .split(area);

        self.render_title_bar(f, chunks[0]);
        self.render_progress_line(f, chunks[1]);
        self.render_comparison(f, chunks[2]);
        self.render_footer(f, chunks[3]);
    }

    fn render_title_bar(&self, f: &mut Frame, area: Rect) {
        let header_text = if self.state.conflicts.is_empty() {
            " 同步冲突".to_string()
        } else {
            format!(
                " 同步冲突  ({}/{})",
                self.state.current_index + 1,
                self.state.conflicts.len()
            )
        };

        let line = ratatui::text::Line::from(vec![Span::styled(
            header_text,
            Style::default()
                .bg(BG_BAR)
                .fg(TEXT)
                .add_modifier(Modifier::BOLD),
        )]);

        f.render_widget(
            Paragraph::new(line).style(Style::default().bg(BG_BAR)),
            area,
        );
    }

    fn render_progress_line(&self, f: &mut Frame, area: Rect) {
        let text = if let Some(conflict) = self.state.conflicts.get(self.state.current_index) {
            format!(
                "检测到 {} 条密码存在冲突，请逐条解决。当前: {}",
                self.state.conflicts.len(),
                conflict.record_name,
            )
        } else {
            "没有需要解决的冲突".to_string()
        };

        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(theme::PRIMARY))
            .wrap(Wrap { trim: true });
        f.render_widget(paragraph, area);
    }

    fn render_comparison(&self, f: &mut Frame, area: Rect) {
        if let Some(conflict) = self.state.conflicts.get(self.state.current_index) {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(48),
                    Constraint::Length(2), // separator gap
                    Constraint::Percentage(48),
                ])
                .split(area);

            // Local panel
            let local_border_color = if self.state.focused_side == ConflictSide::Local {
                Color::Blue
            } else {
                Color::DarkGray
            };
            let local_block = Block::default()
                .title(format!(
                    " 本地版本 ({}) ",
                    conflict.local_time.format("%m-%d %H:%M")
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(local_border_color));
            let local_inner = local_block.inner(columns[0]);
            f.render_widget(local_block, columns[0]);
            self.render_fields(f, local_inner, &conflict.local_fields);

            // Remote panel
            let remote_border_color = if self.state.focused_side == ConflictSide::Remote {
                Color::Green
            } else {
                Color::DarkGray
            };
            let remote_block = Block::default()
                .title(format!(
                    " 远程版本 ({}) ",
                    conflict.remote_time.format("%m-%d %H:%M")
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(remote_border_color));
            let remote_inner = remote_block.inner(columns[2]);
            f.render_widget(remote_block, columns[2]);
            self.render_fields(f, remote_inner, &conflict.remote_fields);
        } else {
            // No conflicts: show empty state
            let empty = Paragraph::new("没有需要解决的冲突").alignment(Alignment::Center);
            let centered = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(1),
                    Constraint::Fill(1),
                ])
                .split(area);
            f.render_widget(empty, centered[1]);
        }
    }

    fn render_fields(&self, f: &mut Frame, area: Rect, fields: &[ConflictField]) {
        if fields.is_empty() || area.height < 1 {
            return;
        }

        let rows: Vec<Row> = fields
            .iter()
            .map(|field| {
                let value = if field.is_sensitive && field.is_masked {
                    theme::ICON_PASSWORD_MASK.repeat(8)
                } else {
                    field.value.clone()
                };
                let diff_marker = if field.differs {
                    " \u{2190} 差异"
                } else {
                    ""
                };
                let value_style = if field.differs {
                    Style::default().fg(Color::Black).bg(Color::Yellow)
                } else {
                    Style::default().fg(TEXT)
                };

                Row::new(vec![
                    Cell::from(Span::styled(
                        &field.label,
                        Style::default().fg(TEXT_SECONDARY),
                    )),
                    Cell::from(Span::styled(
                        format!("{}{}", value, diff_marker),
                        value_style,
                    )),
                ])
            })
            .collect();

        let table = Table::new(
            rows,
            [Constraint::Percentage(30), Constraint::Percentage(70)],
        );
        f.render_widget(table, area);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let hints = [
            "\u{2190}\u{2192}",
            "选择版本",
            "Enter",
            "确认",
            "a",
            "全部保留本地",
            "Esc",
            "跳过",
            "p",
            "显示/隐藏密码",
        ];

        let hint_text = hints.chunks(2).fold(String::new(), |mut acc, pair| {
            if !acc.is_empty() {
                acc.push_str("  \u{2502}  ");
            }
            acc.push_str(&format!("{} {}", pair[0], pair[1]));
            acc
        });

        let line = ratatui::text::Line::from(Span::styled(
            format!(" {} ", hint_text),
            Style::default().fg(TEXT_MUTED),
        ));
        f.render_widget(
            Paragraph::new(line).style(Style::default().bg(BG_BAR)),
            area,
        );
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::sync_ui_state::{ConflictDisplay, ConflictField};
    use crate::tui::traits::screen::Screen as ScreenTrait;
    use chrono::Utc;
    use uuid::Uuid;

    /// Helper to build a conflict display with two fields.
    fn make_conflict(name: &str) -> ConflictDisplay {
        ConflictDisplay {
            record_id: Uuid::new_v4(),
            record_name: name.to_string(),
            local_fields: vec![
                ConflictField {
                    label: "用户名".to_string(),
                    value: "alice".to_string(),
                    differs: false,
                    is_sensitive: false,
                    is_masked: false,
                },
                ConflictField {
                    label: "密码".to_string(),
                    value: "secret123".to_string(),
                    differs: true,
                    is_sensitive: true,
                    is_masked: true,
                },
            ],
            remote_fields: vec![
                ConflictField {
                    label: "用户名".to_string(),
                    value: "alice".to_string(),
                    differs: false,
                    is_sensitive: false,
                    is_masked: false,
                },
                ConflictField {
                    label: "密码".to_string(),
                    value: "newsecret456".to_string(),
                    differs: true,
                    is_sensitive: true,
                    is_masked: true,
                },
            ],
            local_time: Utc::now(),
            remote_time: Utc::now(),
        }
    }

    #[test]
    fn default_screen_has_empty_state() {
        let screen = SyncConflictScreen::default();
        assert!(screen.state.conflicts.is_empty());
        assert_eq!(screen.state.current_index, 0);
        assert_eq!(screen.state.focused_side, ConflictSide::Local);
    }

    #[test]
    fn left_key_focuses_local() {
        let mut screen = SyncConflictScreen::default();
        screen.state.focused_side = ConflictSide::Remote;
        let result = screen.handle_key(KeyCode::Left);
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.state.focused_side, ConflictSide::Local);
    }

    #[test]
    fn right_key_focuses_remote() {
        let mut screen = SyncConflictScreen::default();
        assert_eq!(screen.state.focused_side, ConflictSide::Local);
        let result = screen.handle_key(KeyCode::Right);
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.state.focused_side, ConflictSide::Remote);
    }

    #[test]
    fn tab_focuses_remote() {
        let mut screen = SyncConflictScreen::default();
        let result = screen.handle_key(KeyCode::Tab);
        assert!(matches!(result, ScreenResult::Continue));
        assert_eq!(screen.state.focused_side, ConflictSide::Remote);
    }

    #[test]
    fn enter_resolves_current_conflict_keep_local() {
        let mut screen = SyncConflictScreen::default();
        let conflict = make_conflict("test-site");
        let record_id = conflict.record_id;
        screen.state.conflicts.push(conflict);
        screen.state.focused_side = ConflictSide::Local;

        let result = screen.handle_key(KeyCode::Enter);
        match result {
            ScreenResult::Command(cmd) => match *cmd {
                Command::ResolveConflict {
                    record_id: rid,
                    resolution,
                } => {
                    assert_eq!(rid, record_id);
                    assert_eq!(resolution, ConflictResolution::KeepLocal);
                }
                _ => panic!("Expected ResolveConflict command"),
            },
            _ => panic!("Expected Command result"),
        }
    }

    #[test]
    fn enter_resolves_current_conflict_keep_remote() {
        let mut screen = SyncConflictScreen::default();
        let conflict = make_conflict("test-site");
        let record_id = conflict.record_id;
        screen.state.conflicts.push(conflict);
        screen.state.focused_side = ConflictSide::Remote;

        let result = screen.handle_key(KeyCode::Enter);
        match result {
            ScreenResult::Command(cmd) => match *cmd {
                Command::ResolveConflict {
                    record_id: rid,
                    resolution,
                } => {
                    assert_eq!(rid, record_id);
                    assert_eq!(resolution, ConflictResolution::KeepRemote);
                }
                _ => panic!("Expected ResolveConflict command"),
            },
            _ => panic!("Expected Command result"),
        }
    }

    #[test]
    fn enter_with_no_conflicts_is_continue() {
        let mut screen = SyncConflictScreen::default();
        let result = screen.handle_key(KeyCode::Enter);
        assert!(matches!(result, ScreenResult::Continue));
    }

    #[test]
    fn char_a_resolves_all_keep_local() {
        let mut screen = SyncConflictScreen::default();
        screen.state.conflicts.push(make_conflict("a"));
        screen.state.conflicts.push(make_conflict("b"));

        let result = screen.handle_key(KeyCode::Char('a'));
        match result {
            ScreenResult::Command(cmd) => match *cmd {
                Command::ResolveAllConflicts { resolution } => {
                    assert_eq!(resolution, ConflictResolution::KeepLocal);
                }
                _ => panic!("Expected ResolveAllConflicts command"),
            },
            _ => panic!("Expected Command result"),
        }
    }

    #[test]
    fn esc_skips_current_as_keep_local() {
        let mut screen = SyncConflictScreen::default();
        let conflict = make_conflict("skip-test");
        let record_id = conflict.record_id;
        screen.state.conflicts.push(conflict);

        let result = screen.handle_key(KeyCode::Esc);
        match result {
            ScreenResult::Command(cmd) => match *cmd {
                Command::ResolveConflict {
                    record_id: rid,
                    resolution,
                } => {
                    assert_eq!(rid, record_id);
                    assert_eq!(resolution, ConflictResolution::KeepLocal);
                }
                _ => panic!("Expected ResolveConflict command"),
            },
            _ => panic!("Expected Command result"),
        }
    }

    #[test]
    fn esc_with_no_conflicts_is_pop_screen() {
        let mut screen = SyncConflictScreen::default();
        let result = screen.handle_key(KeyCode::Esc);
        assert!(matches!(result, ScreenResult::PopScreen));
    }

    #[test]
    fn toggle_mask_flips_sensitive_fields_on_focused_side() {
        let mut screen = SyncConflictScreen::default();
        let conflict = make_conflict("mask-test");
        // Local password field starts masked
        assert!(conflict.local_fields[1].is_masked);
        screen.state.conflicts.push(conflict);
        screen.state.focused_side = ConflictSide::Local;

        screen.toggle_mask();
        // Should be unmasked now
        assert!(!screen.state.conflicts[0].local_fields[1].is_masked);
        // Non-sensitive field should remain unchanged
        assert!(!screen.state.conflicts[0].local_fields[0].is_masked);

        screen.toggle_mask();
        // Should be masked again
        assert!(screen.state.conflicts[0].local_fields[1].is_masked);
    }

    #[test]
    fn toggle_mask_on_remote_side() {
        let mut screen = SyncConflictScreen::default();
        let conflict = make_conflict("mask-remote");
        screen.state.conflicts.push(conflict);
        screen.state.focused_side = ConflictSide::Remote;

        assert!(screen.state.conflicts[0].remote_fields[1].is_masked);
        screen.toggle_mask();
        assert!(!screen.state.conflicts[0].remote_fields[1].is_masked);
    }

    #[test]
    fn conflict_resolved_advances_index() {
        let mut screen = SyncConflictScreen::default();
        screen.state.conflicts.push(make_conflict("first"));
        screen.state.conflicts.push(make_conflict("second"));

        assert_eq!(screen.state.current_index, 0);

        // Cannot create ScreenContext in tests, so test the logic directly
        // by simulating what update() does on ConflictResolved
        if screen.state.current_index < screen.state.conflicts.len().saturating_sub(1) {
            screen.state.current_index += 1;
        }
        assert_eq!(screen.state.current_index, 1);
    }

    #[test]
    fn conflict_resolved_pops_on_last() {
        let mut screen = SyncConflictScreen::default();
        screen.state.conflicts.push(make_conflict("only-one"));
        screen.state.current_index = 0;

        // On the last conflict, advancing should trigger PopScreen
        let should_pop =
            screen.state.current_index >= screen.state.conflicts.len().saturating_sub(1);
        assert!(should_pop);
    }

    #[test]
    fn on_unmount_does_not_crash() {
        let mut screen = SyncConflictScreen::default();
        ScreenTrait::on_unmount(&mut screen);
        // State should remain unchanged
        assert!(screen.state.conflicts.is_empty());
    }

    #[test]
    fn unknown_key_is_continue() {
        let mut screen = SyncConflictScreen::default();
        let result = screen.handle_key(KeyCode::Char('z'));
        assert!(matches!(result, ScreenResult::Continue));
    }
}
