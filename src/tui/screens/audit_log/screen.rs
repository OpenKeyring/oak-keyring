//! Audit log screen -- browse and filter audit trail entries (U10).
//!
//! Displays a filterable list of audit events with color-coded operation
//! labels, time-range filtering, and search. Pressing Enter on a record-
//! related entry navigates to that record's detail view.

use std::cell::Cell;
use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::Frame;

use crate::commands::result::CommandResult;
use crate::commands::types::Screen as ScreenEnum;
use crate::commands::{Command, Message};
use crate::t;
use crate::tui::state::audit_state::{AuditFocus, AuditLogScreenState, AuditOperationFilter};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};

use super::filter::{FilterState, TIME_RANGES};

// ── AuditLogScreen ──────────────────────────────────────────────────────────

const AUDIT_PAGE_SIZE: usize = 50;

pub struct AuditLogScreen {
    pub state: AuditLogScreenState,
    pub(super) filter_debounce: FilterState,
    /// Index into [`AuditOperationFilter::all_variants()`].
    pub(super) operation_filter_idx: usize,
    /// Index into [`TIME_RANGES`].
    pub(super) time_filter_idx: usize,
    /// Last rendered visible row count for the log list.
    pub(super) visible_log_rows: Cell<usize>,
    /// Offset of an in-flight audit page request.
    pub(super) pending_load_offset: Option<usize>,
}

impl AuditLogScreen {
    pub fn new() -> Self {
        Self {
            state: AuditLogScreenState::default(),
            filter_debounce: FilterState::default(),
            operation_filter_idx: 0,
            time_filter_idx: 0,
            visible_log_rows: Cell::new(10),
            pending_load_offset: None,
        }
    }

    /// Build a command-layer filter from the current UI state.
    fn build_cmd_filter(&self, offset: usize) -> crate::commands::types::AuditFilter {
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
            limit: Some(AUDIT_PAGE_SIZE),
            offset,
        }
    }

    /// Dispatch a LoadAuditLog command with the current filter state.
    fn load_page(&mut self, ctx: &mut ScreenContext, offset: usize) {
        let cmd_filter = self.build_cmd_filter(offset);
        self.pending_load_offset = Some(offset);
        if ctx
            .command_tx
            .try_send(Command::LoadAuditLog { filter: cmd_filter })
            .is_err()
        {
            self.pending_load_offset = None;
        }
    }

    fn reload(&mut self, ctx: &mut ScreenContext) {
        self.load_page(ctx, 0);
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

    fn handle_mouse(
        &mut self,
        event: crossterm::event::MouseEvent,
        ctx: &mut ScreenContext,
    ) -> ScreenResult {
        match event.kind {
            MouseEventKind::ScrollDown => {
                self.state.focused_area = AuditFocus::LogList;
                self.scroll_log_list(3);
                self.maybe_load_more(ctx);
                ScreenResult::Continue
            }
            MouseEventKind::ScrollUp => {
                self.state.focused_area = AuditFocus::LogList;
                self.scroll_log_list(-3);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }

    fn handle_log_list_key(&mut self, key: KeyEvent, ctx: &mut ScreenContext) -> ScreenResult {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_log_list(-1);
                ScreenResult::Continue
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_log_list(1);
                self.maybe_load_more(ctx);
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                if let Some(entry) = self.filtered_entries().get(self.state.selected_index) {
                    if let Some(record_id) = entry.record_id {
                        // The record may have been hard-deleted; the detail
                        // screen will handle that case gracefully. For now we
                        // optimistically navigate.
                        return ScreenResult::Command(Box::new(Command::NavigateToRecord {
                            record_id,
                        }));
                    }
                    // No record_id (e.g. VaultLock) -- nothing to navigate to
                    self.state.hint_message = Some(t!("tui.audit.no_related_records").to_string());
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
                self.operation_filter_idx =
                    (self.operation_filter_idx + variants.len() - 1) % variants.len();
                ScreenResult::Continue
            }
            KeyCode::Down => {
                self.operation_filter_idx = (self.operation_filter_idx + 1) % variants.len();
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
                self.state.focused_area = AuditFocus::LogList;
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
                self.time_filter_idx =
                    (self.time_filter_idx + TIME_RANGES.len() - 1) % TIME_RANGES.len();
                ScreenResult::Continue
            }
            KeyCode::Down => {
                self.time_filter_idx = (self.time_filter_idx + 1) % TIME_RANGES.len();
                ScreenResult::Continue
            }
            KeyCode::Enter => {
                self.state.filter.time_range = Some(TIME_RANGES[self.time_filter_idx]);
                self.state.selected_index = 0;
                self.state.scroll_offset = 0;
                self.reload(ctx);
                self.state.focused_area = AuditFocus::LogList;
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

    fn visible_rows(&self) -> usize {
        self.visible_log_rows.get().max(1)
    }

    fn max_scroll_offset(&self, len: usize) -> usize {
        len.saturating_sub(self.visible_rows())
    }

    fn clamp_selection_and_scroll(&mut self) {
        let len = self.filtered_entries().len();
        if len == 0 {
            self.state.selected_index = 0;
            self.state.scroll_offset = 0;
            return;
        }

        self.state.selected_index = self.state.selected_index.min(len - 1);
        self.ensure_selection_visible();
        self.state.scroll_offset = self.state.scroll_offset.min(self.max_scroll_offset(len));
    }

    fn scroll_log_list(&mut self, delta: isize) {
        let len = self.filtered_entries().len();
        if len == 0 {
            self.state.selected_index = 0;
            self.state.scroll_offset = 0;
            return;
        }

        let current = self.state.selected_index.min(len - 1);
        let next = if delta.is_negative() {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta as usize).min(len - 1)
        };
        self.state.selected_index = next;
        self.ensure_selection_visible();
    }

    fn maybe_load_more(&mut self, ctx: &mut ScreenContext) {
        if self.pending_load_offset.is_some() || self.state.entries.len() >= self.state.total_count
        {
            return;
        }

        let filtered_len = self.filtered_entries().len();
        if filtered_len == 0 {
            self.load_page(ctx, self.state.entries.len());
            return;
        }

        let near_loaded_bottom =
            self.state.selected_index + self.visible_rows() >= filtered_len.saturating_sub(1);
        if near_loaded_bottom {
            self.load_page(ctx, self.state.entries.len());
        }
    }

    fn apply_loaded_entries(&mut self, entries: Vec<crate::types::AuditEntry>, total: usize) {
        let offset = self.pending_load_offset.take().unwrap_or(0);
        self.state.total_count = total;

        if offset == 0 {
            self.state.entries = entries;
            self.clamp_selection_and_scroll();
            return;
        }

        let mut loaded_ids: HashSet<i64> =
            self.state.entries.iter().map(|entry| entry.id).collect();
        self.state.entries.extend(
            entries
                .into_iter()
                .filter(|entry| loaded_ids.insert(entry.id)),
        );
        self.clamp_selection_and_scroll();
    }

    fn ensure_selection_visible(&mut self) {
        let visible = self.visible_rows();
        if self.state.selected_index < self.state.scroll_offset {
            self.state.scroll_offset = self.state.selected_index;
        }
        if self.state.selected_index >= self.state.scroll_offset + visible {
            self.state.scroll_offset = self.state.selected_index - visible + 1;
        }
    }

    // ── Client-side operation filter ─────────────────────────────────────

    /// Returns entries filtered by the selected operation category.
    pub(super) fn filtered_entries(&self) -> Vec<&crate::types::AuditEntry> {
        let op_filter = AuditOperationFilter::all_variants()[self.operation_filter_idx];
        let time_range = TIME_RANGES[self.time_filter_idx];
        let search = self.state.filter.search.trim().to_lowercase();
        self.state
            .entries
            .iter()
            .filter(|entry| op_filter.matches(&entry.operation))
            .filter(|entry| entry_matches_time_range(entry, time_range))
            .filter(|entry| entry_matches_search(entry, &search))
            .collect()
    }
}

fn entry_matches_time_range(
    entry: &crate::types::AuditEntry,
    time_range: crate::commands::types::AuditTimeRange,
) -> bool {
    use crate::commands::types::AuditTimeRange;

    match time_range {
        AuditTimeRange::All => true,
        AuditTimeRange::Today => {
            entry.occurred_at.with_timezone(&chrono::Local).date_naive()
                == chrono::Local::now().date_naive()
        }
        AuditTimeRange::LastWeek => {
            entry.occurred_at >= chrono::Utc::now() - chrono::Duration::days(7)
        }
        AuditTimeRange::LastMonth => {
            entry.occurred_at >= chrono::Utc::now() - chrono::Duration::days(30)
        }
        AuditTimeRange::LastYear => {
            entry.occurred_at >= chrono::Utc::now() - chrono::Duration::days(365)
        }
    }
}

fn entry_matches_search(entry: &crate::types::AuditEntry, search: &str) -> bool {
    if search.is_empty() {
        return true;
    }

    entry
        .record_name
        .as_deref()
        .is_some_and(|name| name.to_lowercase().contains(search))
        || entry
            .detail
            .as_deref()
            .is_some_and(|detail| detail.to_lowercase().contains(search))
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
            Message::MouseEvent(event) => self.handle_mouse(event, ctx),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            Message::AuditLogLoaded { entries, total } => {
                self.apply_loaded_entries(entries, total);
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
                    self.load_page(ctx, 0);
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

        self.load_page(ctx, 0);
    }

    fn on_unmount(&mut self) {
        // No-op
    }
}

impl AuditLogScreen {
    fn handle_command_result(&mut self, result: CommandResult) -> ScreenResult {
        match result {
            CommandResult::AuditLogLoaded { entries, total } => {
                self.apply_loaded_entries(entries, total);
                ScreenResult::Continue
            }
            CommandResult::RecordDetailLoaded { record, .. } => {
                ScreenResult::NavigateTo(ScreenEnum::EditRecord { id: record.id() })
            }
            CommandResult::Error { fallback, .. } => {
                self.state.hint_message = Some(fallback);
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}
