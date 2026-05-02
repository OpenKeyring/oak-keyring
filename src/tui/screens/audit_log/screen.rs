//! Audit log screen -- browse and filter audit trail entries (U10).
//!
//! Displays a filterable list of audit events with color-coded operation
//! labels, time-range filtering, and search. Pressing Enter on a record-
//! related entry navigates to that record's detail view.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;

use crate::commands::result::CommandResult;
use crate::commands::types::Screen as ScreenEnum;
use crate::commands::{Command, Message};
use crate::tui::state::audit_state::{AuditFocus, AuditLogScreenState, AuditOperationFilter};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};

use super::filter::{FilterState, TIME_RANGES};

// ── AuditLogScreen ──────────────────────────────────────────────────────────

pub struct AuditLogScreen {
    pub state: AuditLogScreenState,
    pub(super) filter_debounce: FilterState,
    /// Index into [`AuditOperationFilter::all_variants()`].
    pub(super) operation_filter_idx: usize,
    /// Index into [`TIME_RANGES`].
    pub(super) time_filter_idx: usize,
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
    pub(super) fn filtered_entries(&self) -> Vec<&crate::types::AuditEntry> {
        let op_filter = AuditOperationFilter::all_variants()[self.operation_filter_idx];
        self.state
            .entries
            .iter()
            .filter(|e| op_filter.matches(&e.operation))
            .collect()
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
