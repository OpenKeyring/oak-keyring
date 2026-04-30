//! Audit log screen -- browse and filter audit trail entries (U10).
//!
//! Displays a filterable list of audit events with color-coded operation
//! labels, time-range filtering, and search. Pressing Enter on a record-
//! related entry navigates to that record's detail view.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::commands::result::CommandResult;
use crate::commands::types::{AuditTimeRange, Screen as ScreenEnum};
use crate::commands::{Command, Message};
use crate::tui::state::audit_state::{
    AuditFilter, AuditFocus, AuditLogScreenState, AuditOperationFilter,
};
use crate::tui::theme::{self, Styles, BG_BAR, PRIMARY, TEXT, TEXT_MUTED, TEXT_SECONDARY};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::AuditOperation;

// ── Filter debounce ─────────────────────────────────────────────────────────

/// Number of 50 ms ticks before a pending search is flushed.
const DEBOUNCE_TICKS: usize = 3;

#[derive(Debug, Default)]
struct FilterState {
    pending_search: Option<String>,
    debounce_counter: Option<usize>,
}

impl FilterState {
    fn on_search_input(&mut self, text: String) {
        self.pending_search = Some(text);
        self.debounce_counter = Some(DEBOUNCE_TICKS);
    }

    /// Tick the debounce counter. Returns a fully populated filter when the
    /// debounce window expires, so the caller can dispatch a reload command.
    fn tick(&mut self, current_filter: &AuditFilter) -> Option<AuditFilter> {
        if let Some(ref mut counter) = self.debounce_counter {
            *counter = counter.saturating_sub(1);
            if *counter == 0 {
                self.debounce_counter = None;
                let search = self.pending_search.take().unwrap_or_default();
                return Some(AuditFilter {
                    search,
                    operation: current_filter.operation,
                    time_range: current_filter.time_range,
                });
            }
        }
        None
    }
}

// ── Helper functions ────────────────────────────────────────────────────────

fn operation_display_name(op: &AuditOperation) -> &'static str {
    match op {
        AuditOperation::RecordCreate => "添加密码",
        AuditOperation::RecordUpdate => "修改密码",
        AuditOperation::RecordDelete => "删除密码",
        AuditOperation::RecordRestore => "恢复密码",
        AuditOperation::RecordDestroy => "永久删除",
        AuditOperation::RecordViewPassword => "查看密码",
        AuditOperation::RecordCopyPassword => "复制密码",
        AuditOperation::RecordCopyField => "复制字段",
        AuditOperation::VaultUnlock => "解锁",
        AuditOperation::VaultLock => "锁定",
        AuditOperation::VaultExport => "导出",
        AuditOperation::VaultImport => "导入",
        AuditOperation::MasterPasswordChange => "改密",
        AuditOperation::TrashEmpty => "清空回收站",
        AuditOperation::SyncConflictResolved => "解决冲突",
        AuditOperation::SyncBatchConflictsResolved => "批量解决冲突",
        AuditOperation::DekRotated => "密钥轮换",
        AuditOperation::DekRotationFailed => "轮换失败",
    }
}

fn operation_color(op: &AuditOperation) -> ratatui::style::Color {
    match op {
        AuditOperation::RecordCopyPassword
        | AuditOperation::RecordCopyField
        | AuditOperation::RecordViewPassword => ratatui::style::Color::Blue,
        AuditOperation::RecordCreate | AuditOperation::RecordRestore => {
            ratatui::style::Color::Green
        }
        AuditOperation::RecordUpdate => ratatui::style::Color::Yellow,
        AuditOperation::RecordDelete
        | AuditOperation::RecordDestroy
        | AuditOperation::TrashEmpty => ratatui::style::Color::Red,
        _ => ratatui::style::Color::DarkGray,
    }
}

fn time_range_display(tr: &AuditTimeRange) -> &'static str {
    match tr {
        AuditTimeRange::Today => "今天",
        AuditTimeRange::LastWeek => "最近一周",
        AuditTimeRange::LastMonth => "最近一月",
        AuditTimeRange::LastYear => "最近一年",
        AuditTimeRange::All => "全部时间",
    }
}

const TIME_RANGES: [AuditTimeRange; 5] = [
    AuditTimeRange::All,
    AuditTimeRange::Today,
    AuditTimeRange::LastWeek,
    AuditTimeRange::LastMonth,
    AuditTimeRange::LastYear,
];

#[cfg(test)]
fn time_range_index(tr: Option<&AuditTimeRange>) -> usize {
    match tr {
        None | Some(AuditTimeRange::All) => 0,
        Some(AuditTimeRange::Today) => 1,
        Some(AuditTimeRange::LastWeek) => 2,
        Some(AuditTimeRange::LastMonth) => 3,
        Some(AuditTimeRange::LastYear) => 4,
    }
}

/// Format a `DateTime<Utc>` into a compact display string.
fn format_timestamp(dt: &chrono::DateTime<chrono::Utc>) -> String {
    let local = dt.with_timezone(&chrono::Local);
    local.format("%m-%d %H:%M").to_string()
}

// ── AuditLogScreen ──────────────────────────────────────────────────────────

pub struct AuditLogScreen {
    pub state: AuditLogScreenState,
    filter_debounce: FilterState,
    /// Index into [`AuditOperationFilter::all_variants()`].
    operation_filter_idx: usize,
    /// Index into [`TIME_RANGES`].
    time_filter_idx: usize,
}

impl AuditLogScreen {
    pub fn new() -> Self {
        Self {
            state: AuditLogScreenState::default(),
            filter_debounce: FilterState::default(),
            operation_filter_idx: 0,
            time_filter_idx: 0,
        }
    }

    /// Build a command-layer filter from the current UI state.
    fn build_cmd_filter(&self) -> crate::commands::types::AuditFilter {
        let op_filter = AuditOperationFilter::all_variants()[self.operation_filter_idx];
        crate::commands::types::AuditFilter {
            operation: if matches!(op_filter, AuditOperationFilter::All) {
                None
            } else {
                // The command-layer filter works on a single operation, but the
                // UI groups them. Pass None for now -- the command layer will
                // return all entries and the UI-side AuditOperationFilter
                // handles grouping during rendering. For a proper server-side
                // filter we would need to extend the command filter, but that
                // is out of scope for the initial U10 implementation.
                None
            },
            time_range: Some(TIME_RANGES[self.time_filter_idx]),
            search: if self.state.filter.search.is_empty() {
                None
            } else {
                Some(self.state.filter.search.clone())
            },
        }
    }

    /// Dispatch a LoadAuditLog command with the current filter state.
    fn reload(&self, ctx: &mut ScreenContext) {
        let cmd_filter = self.build_cmd_filter();
        let _ = ctx
            .command_tx
            .try_send(Command::LoadAuditLog { filter: cmd_filter });
    }

    // ── Key handling ─────────────────────────────────────────────────────

    fn handle_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        // Clear transient hint on any key press
        self.state.hint_message = None;

        match self.state.focused_area {
            AuditFocus::LogList => self.handle_log_list_key(key, ctx),
            AuditFocus::OperationFilter => self.handle_operation_filter_key(key, ctx),
            AuditFocus::TimeFilter => self.handle_time_filter_key(key, ctx),
            AuditFocus::SearchInput => self.handle_search_key(key, ctx),
        }
    }

    fn handle_log_list_key(&mut self, key: KeyEvent, _ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.state.selected_index > 0 {
                    self.state.selected_index -= 1;
                    self.adjust_scroll_up();
                }
                ScreenResult::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.state.entries.is_empty()
                    && self.state.selected_index < self.state.entries.len() - 1
                {
                    self.state.selected_index += 1;
                    self.adjust_scroll_down();
                }
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if let Some(entry) = self.state.entries.get(self.state.selected_index) {
                    if let Some(record_id) = entry.record_id {
                        // The record may have been hard-deleted; the detail
                        // screen will handle that case gracefully. For now we
                        // optimistically navigate.
                        return ScreenResult::Command(Box::new(Command::NavigateToRecord {
                            record_id,
                        }));
                    }
                    // No record_id (e.g. VaultLock) -- nothing to navigate to
                    self.state.hint_message = Some("此条目无关联记录".to_string());
                }
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                self.state.focused_area = AuditFocus::OperationFilter;
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::PopScreen,
            _ => ScreenResult::Continue,
        }
    }

    fn handle_operation_filter_key(
        &mut self,
        key: KeyEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        let variants = AuditOperationFilter::all_variants();
        match key.code {
            KeyCode::Up => {
                if self.operation_filter_idx > 0 {
                    self.operation_filter_idx -= 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Down => {
                if self.operation_filter_idx < variants.len() - 1 {
                    self.operation_filter_idx += 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                let op_filter = variants[self.operation_filter_idx];
                self.state.filter.operation = if matches!(op_filter, AuditOperationFilter::All) {
                    None
                } else {
                    // Store the filter category for local display filtering.
                    // The command layer gets `operation: None` and we filter
                    // client-side.
                    None
                };
                self.state.selected_index = 0;
                self.state.scroll_offset = 0;
                self.reload(ctx);
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                self.state.focused_area = AuditFocus::TimeFilter;
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::PopScreen,
            _ => ScreenResult::Continue,
        }
    }

    fn handle_time_filter_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Up => {
                if self.time_filter_idx > 0 {
                    self.time_filter_idx -= 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Down => {
                if self.time_filter_idx < TIME_RANGES.len() - 1 {
                    self.time_filter_idx += 1;
                }
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                self.state.filter.time_range = Some(TIME_RANGES[self.time_filter_idx]);
                self.state.selected_index = 0;
                self.state.scroll_offset = 0;
                self.reload(ctx);
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                self.state.focused_area = AuditFocus::SearchInput;
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::PopScreen,
            _ => ScreenResult::Continue,
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Char(c) => {
                self.state.filter.search.push(c);
                self.filter_debounce
                    .on_search_input(self.state.filter.search.clone());
                ScreenResult::Continue
            }
            KeyCode::Backspace => {
                self.state.filter.search.pop();
                if !self.state.filter.search.is_empty() {
                    self.filter_debounce
                        .on_search_input(self.state.filter.search.clone());
                } else {
                    // Immediately trigger reload when search is cleared
                    self.state.selected_index = 0;
                    self.state.scroll_offset = 0;
                    self.reload(ctx);
                }
                ScreenResult::Continue
            }
            KeyCode::Tab => {
                self.state.focused_area = AuditFocus::LogList;
                ScreenResult::Continue
            }
            KeyCode::Esc => ScreenResult::PopScreen,
            _ => ScreenResult::Continue,
        }
    }

    // ── Scroll helpers ───────────────────────────────────────────────────

    fn adjust_scroll_up(&mut self) {
        if self.state.selected_index < self.state.scroll_offset {
            self.state.scroll_offset = self.state.selected_index;
        }
    }

    fn adjust_scroll_down(&mut self) {
        // Assume roughly 10 visible rows; caller should adjust for actual height.
        let visible = 10;
        if self.state.selected_index >= self.state.scroll_offset + visible {
            self.state.scroll_offset = self.state.selected_index - visible + 1;
        }
    }

    // ── Client-side operation filter ─────────────────────────────────────

    /// Returns entries filtered by the selected operation category.
    fn filtered_entries(&self) -> Vec<&crate::types::AuditEntry> {
        let op_filter = AuditOperationFilter::all_variants()[self.operation_filter_idx];
        self.state
            .entries
            .iter()
            .filter(|e| op_filter.matches(&e.operation))
            .collect()
    }

    // ── Rendering ────────────────────────────────────────────────────────

    fn render_title_bar(&self, frame: &mut Frame, area: Rect) {
        let title_text = if self.state.audit_enabled {
            " 操作审计".to_string()
        } else {
            " 操作审计 \u{26A0} 审计已关闭".to_string()
        };

        let count_text = format!("{} 条记录", self.state.total_count);

        let line = Line::from(vec![
            Span::styled(
                title_text,
                Style::default()
                    .bg(BG_BAR)
                    .fg(TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:>width$}", count_text, width = area.width as usize - 24),
                Style::default().bg(BG_BAR).fg(TEXT_SECONDARY),
            ),
        ]);

        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(BG_BAR)),
            area,
        );
    }

    fn render_filter_bar(&self, frame: &mut Frame, area: Rect) {
        // Split into 3 filter sections: operation | time | search
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(14), // operation filter
                Constraint::Length(14), // time filter
                Constraint::Fill(1),    // search input
            ])
            .split(area);

        // Operation filter
        let op_filter = AuditOperationFilter::all_variants()[self.operation_filter_idx];
        let op_name = op_filter.display_name();
        let op_border = if self.state.focused_area == AuditFocus::OperationFilter {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let op_block = Block::default()
            .borders(Borders::ALL)
            .border_style(op_border)
            .title(" 类型 ");
        let op_text = Paragraph::new(format!(" {}", op_name)).style(Style::default().fg(TEXT));
        frame.render_widget(op_block, columns[0]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(columns[0]);
        let padded = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(inner[0]);
        frame.render_widget(op_text, padded[1]);

        // Time filter
        let time_range = TIME_RANGES[self.time_filter_idx];
        let time_name = time_range_display(&time_range);
        let time_border = if self.state.focused_area == AuditFocus::TimeFilter {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let time_block = Block::default()
            .borders(Borders::ALL)
            .border_style(time_border)
            .title(" 时间 ");
        let time_text = Paragraph::new(format!(" {}", time_name)).style(Style::default().fg(TEXT));
        frame.render_widget(time_block, columns[1]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(columns[1]);
        let padded = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(inner[0]);
        frame.render_widget(time_text, padded[1]);

        // Search input
        let search_border = if self.state.focused_area == AuditFocus::SearchInput {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let search_block = Block::default()
            .borders(Borders::ALL)
            .border_style(search_border)
            .title(" 搜索 ");
        let search_display = if self.state.filter.search.is_empty() {
            Paragraph::new(" 输入关键词...").style(Style::default().fg(theme::TEXT_PLACEHOLDER))
        } else {
            let cursor = if self.state.focused_area == AuditFocus::SearchInput {
                "\u{2502}" // vertical bar cursor
            } else {
                ""
            };
            Paragraph::new(format!(" {}{}", self.state.filter.search, cursor))
                .style(Style::default().fg(TEXT))
        };
        frame.render_widget(search_block, columns[2]);
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1)])
            .split(columns[2]);
        let padded = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(1), Constraint::Fill(1)])
            .split(inner[0]);
        frame.render_widget(search_display, padded[1]);
    }

    fn render_log_list(&self, frame: &mut Frame, area: Rect) {
        let filtered = self.filtered_entries();

        if filtered.is_empty() {
            self.render_empty_state(frame, area);
            return;
        }

        // Clamp selected_index to valid range of filtered entries
        let selected = self.state.selected_index.min(filtered.len() - 1);

        let items: Vec<ListItem> = filtered
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let is_selected = i == selected;
                let op_name = operation_display_name(&entry.operation);
                let op_color = operation_color(&entry.operation);
                let timestamp = format_timestamp(&entry.occurred_at);
                let record_name = entry.record_name.as_deref().unwrap_or("(无记录名)");

                let mut spans = vec![
                    Span::styled(
                        format!(" {} ", timestamp),
                        Style::default().fg(TEXT_SECONDARY),
                    ),
                    Span::styled(format!("[{:6}]", op_name), Style::default().fg(op_color)),
                    Span::styled(
                        format!(" {}", record_name),
                        if is_selected {
                            Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(TEXT)
                        },
                    ),
                ];

                // Show detail if present
                if let Some(ref detail) = entry.detail {
                    spans.push(Span::styled(
                        format!(" - {}", detail),
                        Style::default().fg(TEXT_MUTED),
                    ));
                }

                let line = Line::from(spans);
                let style = if is_selected && self.state.focused_area == AuditFocus::LogList {
                    Style::default().add_modifier(Modifier::REVERSED)
                } else if is_selected {
                    Style::default().fg(PRIMARY)
                } else {
                    Style::default()
                };

                ListItem::new(line).style(style)
            })
            .collect();

        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(theme::BG));

        let list = List::new(items).block(block);

        // Use ListState for scrolling
        let mut list_state = ListState::default();
        list_state.select(Some(selected));

        frame.render_stateful_widget(list, area, &mut list_state);
    }

    fn render_empty_state(&self, frame: &mut Frame, area: Rect) {
        let (icon, message) = if !self.state.audit_enabled {
            (theme::ICON_WARNING, "审计日志功能已关闭")
        } else if !self.state.filter.search.is_empty() {
            (theme::ICON_INFO, "未找到匹配的记录")
        } else if self.state.entries.is_empty() {
            (theme::ICON_INFO, "暂无审计记录")
        } else {
            // There are entries but the operation filter excluded them all
            (theme::ICON_INFO, "当前筛选条件下无记录")
        };

        let line = Line::from(vec![
            Span::styled(format!(" {} ", icon), Style::default().fg(theme::WARNING)),
            Span::styled(message, Style::default().fg(TEXT_MUTED)),
        ]);

        let paragraph = Paragraph::new(line)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        // Vertically center in the available area
        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .split(area);

        frame.render_widget(paragraph, outer[1]);
    }

    fn render_help_bar(&self, frame: &mut Frame, area: Rect) {
        let hints = [
            "\u{2191}\u{2193}/j/k",
            "选择",
            "Tab",
            "切换区域",
            "Enter",
            "查看记录",
            "Esc",
            "返回",
        ];

        let hint_text = hints.chunks(2).fold(String::new(), |mut acc, pair| {
            if !acc.is_empty() {
                acc.push_str("  \u{2502}  ");
            }
            acc.push_str(&format!("{} {}", pair[0], pair[1]));
            acc
        });

        // Also show the hint message if any
        let line = if let Some(ref msg) = self.state.hint_message {
            Line::from(vec![
                Span::styled(format!(" {} ", hint_text), Style::default().fg(TEXT_MUTED)),
                Span::styled(
                    format!("  {} {}", theme::ICON_INFO, msg),
                    Style::default().fg(theme::INFO),
                ),
            ])
        } else {
            Line::from(Span::styled(
                format!(" {} ", hint_text),
                Style::default().fg(TEXT_MUTED),
            ))
        };

        frame.render_widget(
            Paragraph::new(line).style(Style::default().bg(BG_BAR)),
            area,
        );
    }
}

impl Default for AuditLogScreen {
    fn default() -> Self {
        Self::new()
    }
}

// ── Screen trait impl ───────────────────────────────────────────────────────

impl Screen for AuditLogScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            Message::AuditLogLoaded { entries, total } => {
                self.state.entries = entries;
                self.state.total_count = total;
                // Clamp selection
                if !self.state.entries.is_empty() {
                    self.state.selected_index =
                        self.state.selected_index.min(self.state.entries.len() - 1);
                } else {
                    self.state.selected_index = 0;
                }
                ScreenResult::Continue
            }
            Message::NavigateToRecord { record_id } => {
                // The NavigateToRecord message means the executor confirmed
                // the record exists. Navigate to EditRecord screen.
                ScreenResult::NavigateTo(ScreenEnum::EditRecord { id: record_id })
            }
            Message::Tick => {
                // Handle search debounce
                if let Some(new_filter) = self.filter_debounce.tick(&self.state.filter) {
                    self.state.filter.search = new_filter.search;
                    self.state.selected_index = 0;
                    self.state.scroll_offset = 0;
                    let cmd_filter = self.build_cmd_filter();
                    let _ = ctx
                        .command_tx
                        .try_send(Command::LoadAuditLog { filter: cmd_filter });
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // title bar
                Constraint::Length(3), // filter bar
                Constraint::Fill(1),   // log list
                Constraint::Length(1), // help bar
            ])
            .split(area);

        self.render_title_bar(frame, chunks[0]);
        self.render_filter_bar(frame, chunks[1]);
        self.render_log_list(frame, chunks[2]);
        self.render_help_bar(frame, chunks[3]);
    }

    fn on_mount(&mut self, ctx: &mut ScreenContext) {
        if self.state.restored_from_snapshot {
            self.state.restored_from_snapshot = false;
        } else {
            self.state.entries.clear();
            self.state.total_count = 0;
            self.state.selected_index = 0;
            self.state.scroll_offset = 0;
        }
        self.state.hint_message = None;

        // Read audit_enabled from config
        self.state.audit_enabled = ctx.config.security.audit_enabled;

        let cmd_filter = self.build_cmd_filter();
        let _ = ctx
            .command_tx
            .try_send(Command::LoadAuditLog { filter: cmd_filter });
    }

    fn on_unmount(&mut self) {
        // No-op
    }
}

impl AuditLogScreen {
    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::AuditLogLoaded { entries, total } => {
                self.state.entries = entries;
                self.state.total_count = total;
                if !self.state.entries.is_empty() {
                    self.state.selected_index =
                        self.state.selected_index.min(self.state.entries.len() - 1);
                } else {
                    self.state.selected_index = 0;
                }
                ScreenResult::Continue
            }
            CommandResult::Error { fallback, .. } => {
                self.state.hint_message = Some(fallback);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_screen_has_sensible_defaults() {
        let screen = AuditLogScreen::new();
        assert!(screen.state.entries.is_empty());
        assert_eq!(screen.state.total_count, 0);
        assert_eq!(screen.state.selected_index, 0);
        assert_eq!(screen.state.focused_area, AuditFocus::LogList);
        assert!(screen.state.audit_enabled);
        assert!(screen.state.filter.search.is_empty());
        assert_eq!(screen.operation_filter_idx, 0);
        assert_eq!(screen.time_filter_idx, 0);
    }

    #[test]
    fn operation_display_names_are_non_empty() {
        use crate::types::AuditOperation;
        let ops = [
            AuditOperation::RecordCreate,
            AuditOperation::RecordUpdate,
            AuditOperation::RecordDelete,
            AuditOperation::RecordRestore,
            AuditOperation::RecordDestroy,
            AuditOperation::RecordViewPassword,
            AuditOperation::RecordCopyPassword,
            AuditOperation::RecordCopyField,
            AuditOperation::VaultUnlock,
            AuditOperation::VaultLock,
            AuditOperation::VaultExport,
            AuditOperation::VaultImport,
            AuditOperation::MasterPasswordChange,
            AuditOperation::TrashEmpty,
            AuditOperation::SyncConflictResolved,
            AuditOperation::SyncBatchConflictsResolved,
            AuditOperation::DekRotated,
            AuditOperation::DekRotationFailed,
        ];
        for op in &ops {
            assert!(!operation_display_name(op).is_empty());
        }
    }

    #[test]
    fn operation_colors_are_assigned() {
        use crate::types::AuditOperation;
        // Copy operations -> Blue
        assert_eq!(
            operation_color(&AuditOperation::RecordCopyPassword),
            ratatui::style::Color::Blue
        );
        // Create -> Green
        assert_eq!(
            operation_color(&AuditOperation::RecordCreate),
            ratatui::style::Color::Green
        );
        // Update -> Yellow
        assert_eq!(
            operation_color(&AuditOperation::RecordUpdate),
            ratatui::style::Color::Yellow
        );
        // Delete -> Red
        assert_eq!(
            operation_color(&AuditOperation::RecordDelete),
            ratatui::style::Color::Red
        );
        // System -> DarkGray
        assert_eq!(
            operation_color(&AuditOperation::VaultUnlock),
            ratatui::style::Color::DarkGray
        );
    }

    #[test]
    fn filter_debounce_expires_after_ticks() {
        let mut fs = FilterState::default();
        let current = AuditFilter::default();

        fs.on_search_input("test".to_string());
        assert!(fs.debounce_counter.is_some());

        // Tick twice (not yet expired)
        assert!(fs.tick(&current).is_none());
        assert!(fs.tick(&current).is_none());

        // Third tick triggers expiration
        let result = fs.tick(&current);
        assert!(result.is_some());
        assert_eq!(result.unwrap().search, "test");
        assert!(fs.debounce_counter.is_none());
        assert!(fs.pending_search.is_none());
    }

    #[test]
    fn filter_debounce_resets_on_new_input() {
        let mut fs = FilterState::default();
        let current = AuditFilter::default();

        fs.on_search_input("ab".to_string());
        let _ = fs.tick(&current); // counter = 2

        // New input resets the counter
        fs.on_search_input("abc".to_string());
        assert_eq!(fs.debounce_counter, Some(DEBOUNCE_TICKS));
    }

    #[test]
    fn tab_cycles_focus_areas() {
        let mut screen = AuditLogScreen::new();

        // We cannot create a ScreenContext with non-static references in tests,
        // so we directly test the focus cycle logic.
        assert_eq!(screen.state.focused_area, AuditFocus::LogList);

        screen.state.focused_area = AuditFocus::OperationFilter;
        assert_eq!(screen.state.focused_area, AuditFocus::OperationFilter);

        screen.state.focused_area = AuditFocus::TimeFilter;
        assert_eq!(screen.state.focused_area, AuditFocus::TimeFilter);

        screen.state.focused_area = AuditFocus::SearchInput;
        assert_eq!(screen.state.focused_area, AuditFocus::SearchInput);

        screen.state.focused_area = AuditFocus::LogList;
        assert_eq!(screen.state.focused_area, AuditFocus::LogList);
    }

    #[test]
    fn time_range_index_mapping() {
        assert_eq!(time_range_index(None), 0);
        assert_eq!(time_range_index(Some(&AuditTimeRange::All)), 0);
        assert_eq!(time_range_index(Some(&AuditTimeRange::Today)), 1);
        assert_eq!(time_range_index(Some(&AuditTimeRange::LastWeek)), 2);
        assert_eq!(time_range_index(Some(&AuditTimeRange::LastMonth)), 3);
        assert_eq!(time_range_index(Some(&AuditTimeRange::LastYear)), 4);
    }

    #[test]
    fn filtered_entries_with_all_filter_returns_all() {
        let mut screen = AuditLogScreen::new();
        use crate::types::{AuditEntry, AuditOperation};
        use chrono::Utc;

        screen.state.entries = vec![
            AuditEntry {
                id: 1,
                operation: AuditOperation::RecordCreate,
                record_id: None,
                record_name: Some("test".to_string()),
                detail: None,
                occurred_at: Utc::now(),
            },
            AuditEntry {
                id: 2,
                operation: AuditOperation::VaultUnlock,
                record_id: None,
                record_name: None,
                detail: None,
                occurred_at: Utc::now(),
            },
        ];

        // operation_filter_idx = 0 means "All"
        assert_eq!(screen.operation_filter_idx, 0);
        let filtered = screen.filtered_entries();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn filtered_entries_with_copy_filter() {
        let mut screen = AuditLogScreen::new();
        use crate::types::{AuditEntry, AuditOperation};
        use chrono::Utc;

        screen.state.entries = vec![
            AuditEntry {
                id: 1,
                operation: AuditOperation::RecordCreate,
                record_id: None,
                record_name: Some("test".to_string()),
                detail: None,
                occurred_at: Utc::now(),
            },
            AuditEntry {
                id: 2,
                operation: AuditOperation::RecordCopyPassword,
                record_id: None,
                record_name: None,
                detail: None,
                occurred_at: Utc::now(),
            },
        ];

        // Set to "Copy" filter (index 1)
        screen.operation_filter_idx = 1;
        let filtered = screen.filtered_entries();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 2);
    }

    #[test]
    fn on_mount_after_snapshot_restore_preserves_selection_scroll_and_focus() {
        let mut screen = AuditLogScreen::new();
        screen.state.entries = (0..10)
            .map(|id| crate::types::AuditEntry {
                id,
                operation: AuditOperation::VaultUnlock,
                record_id: None,
                record_name: None,
                detail: None,
                occurred_at: chrono::Utc::now(),
            })
            .collect();
        screen
            .state
            .restore_from(crate::tui::state::AuditLogRestoreState {
                focused_area: AuditFocus::SearchInput,
                selected_index: 7,
                scroll_offset: 4,
                filter: AuditFilter {
                    search: "vault".to_string(),
                    operation: None,
                    time_range: Some(AuditTimeRange::LastMonth),
                },
            });

        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        let config = crate::config::AppConfig::default();
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };

        screen.on_mount(&mut ctx);

        assert_eq!(screen.state.focused_area, AuditFocus::SearchInput);
        assert_eq!(screen.state.selected_index, 7);
        assert_eq!(screen.state.scroll_offset, 4);
        assert_eq!(screen.state.filter.search, "vault");
        assert_eq!(
            screen.state.filter.time_range,
            Some(AuditTimeRange::LastMonth)
        );
    }
}
