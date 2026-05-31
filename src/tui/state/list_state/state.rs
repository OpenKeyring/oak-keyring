//! List panel state: records display, navigation, search, visual selection, and sort.
//!
//! Contains:
//! - [`ListPanelState`] — main state for the password list panel
//! - [`ListMode`] — Normal / Search / Visual mode discriminated enum
//! - [`SearchState`] — search query and cursor position
//! - [`VisualState`] — multi-select via HashSet of record IDs
//! - Helper functions for formatting timestamps and credential type prefixes

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Local, TimeZone, Timelike, Utc};
use uuid::Uuid;

use crate::commands::types::{RecordSort, SortDirection, SortField};
use crate::t;
use crate::types::credential::CredentialType;
use crate::types::record::TuiRecord;

// ---------------------------------------------------------------------------
// Sub-states
// ---------------------------------------------------------------------------

/// Snapshot of list state before entering search, for Esc restoration.
#[derive(Debug, Clone)]
pub struct SearchSnapshot {
    pub records: Vec<TuiRecord>,
    pub selected_index: Option<usize>,
    pub scroll_offset: usize,
}

/// Search mode state: the current query string and cursor position within it.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub cursor: usize,
    /// Pre-search snapshot saved on enter, restored on Esc.
    pub pre_search: Option<SearchSnapshot>,
}

/// Visual (multi-select) mode state: the set of selected record IDs.
#[derive(Debug, Clone, Default)]
pub struct VisualState {
    pub selected_ids: HashSet<Uuid>,
}

// ---------------------------------------------------------------------------
// ListMode
// ---------------------------------------------------------------------------

/// Discriminated operating mode for the list panel.
#[derive(Debug, Clone, Default)]
pub enum ListMode {
    /// Default browsing mode.
    #[default]
    Normal,
    /// Search/filter mode with an active query.
    Search(SearchState),
    /// Visual multi-select mode.
    Visual(VisualState),
}

// ---------------------------------------------------------------------------
// ListPanelState
// ---------------------------------------------------------------------------

/// State for the password list panel (center column of the main layout).
#[derive(Debug, Clone)]
pub struct ListPanelState {
    /// Currently displayed records (after filtering/sorting).
    pub records: Vec<TuiRecord>,
    /// Index of the selected record within `records`, if any.
    pub selected_index: Option<usize>,
    /// Vertical scroll offset (index of the first visible record).
    pub scroll_offset: usize,
    /// Current operating mode (Normal / Search / Visual).
    pub mode: ListMode,
    /// Sort order for the record list.
    pub sort: RecordSort,
    /// Total record count (before filtering), for status display.
    pub total_count: usize,
    /// Page offset currently being fetched, if a record-list load is in flight.
    pub pending_load_offset: Option<usize>,
    /// Visible height of the list panel in rows, updated from layout.
    pub _visible_height: usize,
    /// Snapshot saved when search is committed via Enter, so Esc can restore.
    pub committed_search_snapshot: Option<SearchSnapshot>,
}

impl Default for ListPanelState {
    fn default() -> Self {
        Self {
            records: Vec::new(),
            selected_index: None,
            scroll_offset: 0,
            mode: ListMode::Normal,
            sort: RecordSort {
                field: SortField::CreatedAt,
                direction: SortDirection::Desc,
            },
            total_count: 0,
            pending_load_offset: None,
            _visible_height: 0,
            committed_search_snapshot: None,
        }
    }
}

impl ListPanelState {
    /// Create a new state pre-populated with records. Selects the first record.
    pub fn with_records(records: Vec<TuiRecord>) -> Self {
        let total_count = records.len();
        let selected_index = if records.is_empty() { None } else { Some(0) };
        Self {
            records,
            selected_index,
            total_count,
            ..Self::default()
        }
    }

    /// Get a reference to the currently selected record, if any.
    pub fn selected_record(&self) -> Option<&TuiRecord> {
        self.selected_index.and_then(|idx| self.records.get(idx))
    }

    /// Move selection down by one. Clamps at the last record.
    pub fn move_down(&mut self) {
        if self.records.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let next = current.saturating_add(1).min(self.records.len() - 1);
        self.selected_index = Some(next);
        self.adjust_scroll();
    }

    /// Move selection up by one. Clamps at the first record (index 0).
    pub fn move_up(&mut self) {
        if self.records.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let prev = current.saturating_sub(1);
        self.selected_index = Some(prev);
        self.adjust_scroll();
    }

    /// Adjust scroll offset so the selected item stays within the visible area.
    pub fn adjust_scroll(&mut self) {
        let visible = self.visible_items_count();
        if visible == 0 {
            return;
        }
        if let Some(idx) = self.selected_index {
            // Scroll up if selected is above the visible window
            if idx < self.scroll_offset {
                self.scroll_offset = idx;
            }
            // Scroll down if selected is below the visible window
            let last_visible = self.scroll_offset + visible - 1;
            if idx > last_visible {
                self.scroll_offset = idx - visible + 1;
            }
        }
    }

    /// Estimate of how many items are visible in the list panel.
    /// Falls back to 8 when `_visible_height` has not been set.
    pub fn visible_items_count(&self) -> usize {
        if self._visible_height > 0 {
            self._visible_height
        } else {
            8
        }
    }

    /// Update the visible height from a pixel/row height value.
    pub fn set_visible_height(&mut self, height: u16) {
        self._visible_height = height as usize;
    }

    /// Toggle sort direction between Ascending and Descending.
    pub fn toggle_sort_direction(&mut self) {
        self.sort.direction = match self.sort.direction {
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::Asc,
        };
    }

    /// Cycle through sort fields:
    /// CreatedAt -> UpdatedAt -> Name -> UsageFrequency -> CreatedAt
    pub fn cycle_sort_field(&mut self) {
        self.sort.field = match self.sort.field {
            SortField::CreatedAt => SortField::UpdatedAt,
            SortField::UpdatedAt => SortField::Name,
            SortField::Name => SortField::UsageFrequency,
            SortField::UsageFrequency => SortField::CreatedAt,
        };
    }

    // ── Mode transitions ───────────────────────────────────────────────────

    /// Enter search mode. Saves current list state as pre-search snapshot.
    pub fn enter_search(&mut self) {
        let snapshot = SearchSnapshot {
            records: self.records.clone(),
            selected_index: self.selected_index,
            scroll_offset: self.scroll_offset,
        };
        self.mode = ListMode::Search(SearchState {
            pre_search: Some(snapshot),
            ..Default::default()
        });
    }

    /// Commit search: exit search mode, keeping filtered results but saving
    /// the pre-search snapshot so Esc can later restore the original list.
    pub fn commit_search(&mut self) {
        if let ListMode::Search(ref mut state) = self.mode {
            self.committed_search_snapshot = state.pre_search.take();
        }
        self.mode = ListMode::Normal;
    }

    /// Restore the list from a committed search snapshot (Esc after Enter).
    /// Returns the restored selected record id, if any.
    pub fn restore_committed_search(&mut self) -> Option<Uuid> {
        let snapshot = self.committed_search_snapshot.take()?;
        let restored_id = snapshot
            .selected_index
            .and_then(|idx| snapshot.records.get(idx))
            .map(|r| r.id);
        self.records = snapshot.records;
        self.selected_index = snapshot.selected_index;
        self.scroll_offset = snapshot.scroll_offset;
        restored_id
    }

    /// Cancel search and restore pre-search snapshot (for Esc).
    /// Returns the restored selected record id, if any.
    pub fn cancel_search_restore(&mut self) -> Option<Uuid> {
        if let ListMode::Search(ref mut state) = self.mode {
            if let Some(snapshot) = state.pre_search.take() {
                let restored_id = snapshot
                    .selected_index
                    .and_then(|idx| snapshot.records.get(idx))
                    .map(|r| r.id);
                self.records = snapshot.records;
                self.selected_index = snapshot.selected_index;
                self.scroll_offset = snapshot.scroll_offset;
                self.mode = ListMode::Normal;
                return restored_id;
            }
        }
        self.mode = ListMode::Normal;
        None
    }

    /// Enter visual (multi-select) mode. Starts with an empty selection set.
    pub fn enter_visual(&mut self) {
        self.mode = ListMode::Visual(VisualState::default());
    }

    /// Exit visual mode back to normal browsing. Clears selection.
    pub fn exit_visual(&mut self) {
        self.mode = ListMode::Normal;
    }

    /// In visual mode, toggle selection of the currently focused record.
    /// After toggling, automatically advance to the next record.
    pub fn toggle_select_current(&mut self) {
        // First, get the ID of the current record (if any)
        let current_id = self
            .selected_index
            .and_then(|idx| self.records.get(idx))
            .map(|r| r.id);

        if let Some(id) = current_id {
            if let ListMode::Visual(ref mut vs) = self.mode {
                if vs.selected_ids.contains(&id) {
                    vs.selected_ids.remove(&id);
                } else {
                    vs.selected_ids.insert(id);
                }
            }
            // Auto-advance
            self.move_down();
        }
    }

    /// In visual mode, select all records.
    pub fn select_all(&mut self) {
        if let ListMode::Visual(ref mut vs) = self.mode {
            vs.selected_ids = self.records.iter().map(|r| r.id).collect();
        }
    }

    /// In visual mode, clear the selection set.
    pub fn deselect_all(&mut self) {
        if let ListMode::Visual(ref mut vs) = self.mode {
            vs.selected_ids.clear();
        }
    }

    /// Whether the list is currently in search mode.
    pub fn is_searching(&self) -> bool {
        matches!(self.mode, ListMode::Search(_))
    }

    /// Whether the list is currently in visual (multi-select) mode.
    pub fn is_visual(&self) -> bool {
        matches!(self.mode, ListMode::Visual(_))
    }

    /// Return the IDs of records selected in visual mode.
    pub fn visual_selected_ids(&self) -> Vec<Uuid> {
        match &self.mode {
            ListMode::Visual(vs) => vs.selected_ids.iter().copied().collect(),
            _ => Vec::new(),
        }
    }

    // ── Search filtering ───────────────────────────────────────────────────

    /// Get search terms (lowercase, whitespace-split) if in search mode.
    pub fn search_terms(&self) -> Option<Vec<String>> {
        match &self.mode {
            ListMode::Search(state) if !state.query.trim().is_empty() => Some(
                state
                    .query
                    .trim()
                    .to_lowercase()
                    .split_whitespace()
                    .map(String::from)
                    .collect(),
            ),
            _ => None,
        }
    }

    /// Filter records using multi-term AND logic on name + subtitle.
    /// Each whitespace-separated term must appear (case-insensitive) in either
    /// the record name or subtitle.
    pub fn apply_search_filter(&self, all_records: Vec<TuiRecord>) -> Vec<TuiRecord> {
        let terms = match self.search_terms() {
            Some(t) if !t.is_empty() => t,
            _ => return all_records,
        };

        all_records
            .into_iter()
            .filter(|record| {
                let name_lower = record.name.to_lowercase();
                let subtitle_lower = record.subtitle.to_lowercase();
                terms.iter().all(|term| {
                    name_lower.contains(term.as_str()) || subtitle_lower.contains(term.as_str())
                })
            })
            .collect()
    }

    // ── Batch cleanup ──────────────────────────────────────────────────────

    /// Remove records matching the given IDs, fix selection, and exit visual
    /// mode if no records remain selected.
    pub fn cleanup_after_batch(&mut self, removed_ids: &[Uuid]) {
        let removed_set: HashSet<Uuid> = removed_ids.iter().copied().collect();
        self.records.retain(|r| !removed_set.contains(&r.id));
        self.total_count = self.total_count.saturating_sub(removed_ids.len());

        // Fix selection
        if self.records.is_empty() {
            self.selected_index = None;
            self.scroll_offset = 0;
        } else if let Some(idx) = self.selected_index {
            if idx >= self.records.len() {
                self.selected_index = Some(self.records.len() - 1);
            }
        }

        // Exit visual mode if in it (batch operation completes)
        if self.is_visual() {
            self.exit_visual();
        }
    }

    // ── Search query update ────────────────────────────────────────────────

    /// Update the search query in search mode and reset cursor to end.
    pub fn update_search_query(&mut self, query: String) {
        if let ListMode::Search(ref mut state) = self.mode {
            state.cursor = query.len();
            state.query = query;
        }
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Format a UTC datetime as a human-readable relative time string.
///
/// - Today -> HH:MM
/// - Yesterday -> yesterday
/// - < 7 days -> N days ago
/// - < 30 days -> N weeks ago
/// - Same year -> MM-DD
/// - Older -> YYYY-MM-DD
pub fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let local_dt: chrono::DateTime<Local> = Local.from_utc_datetime(&dt.naive_utc());
    let local_now: chrono::DateTime<Local> = Local.from_utc_datetime(&now.naive_utc());

    let today = local_now.date_naive();
    let date = local_dt.date_naive();

    let day_diff = (today - date).num_days();

    if day_diff == 0 {
        // Today: show time HH:MM
        format!("{:02}:{:02}", local_dt.hour(), local_dt.minute())
    } else if day_diff == 1 {
        t!("tui.list_state.time_yesterday").to_string()
    } else if day_diff < 7 {
        t!("tui.list_state.time_days_ago", n = day_diff).to_string()
    } else if day_diff < 30 {
        let weeks = day_diff / 7;
        t!("tui.list_state.time_weeks_ago", n = weeks).to_string()
    } else if local_dt.year() == local_now.year() {
        format!("{:02}-{:02}", local_dt.month(), local_dt.day())
    } else {
        format!(
            "{}-{:02}-{:02}",
            local_dt.year(),
            local_dt.month(),
            local_dt.day()
        )
    }
}

/// Return a display prefix for a credential type in the list view.
///
/// - Login -> ""
/// - Api -> "[API] "
/// - Ssh -> "[SSH] "
/// - SecureNote -> "[N] "
pub fn format_type_prefix(cred_type: &CredentialType) -> String {
    match cred_type {
        CredentialType::Login => String::new(),
        CredentialType::Api => t!("tui.list_state.type_prefix_api").to_string(),
        CredentialType::Ssh => t!("tui.list_state.type_prefix_ssh").to_string(),
        CredentialType::SecureNote => t!("tui.list_state.type_prefix_note").to_string(),
    }
}

// ---------------------------------------------------------------------------
// Trash time helpers
// ---------------------------------------------------------------------------

/// Warning severity tier for trash items based on remaining days before
/// automatic permanent deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrashWarningTier {
    /// >15 days remaining — no warning color.
    Safe,
    /// 15-7 days remaining — single orange warning.
    Moderate,
    /// 7-3 days remaining — double orange bold warning.
    Urgent,
    /// <3 days remaining — triple red bold warning.
    Critical,
}

/// Determine the warning tier based on remaining days.
pub fn trash_warning_tier(remaining_days: i64) -> TrashWarningTier {
    if remaining_days > 15 {
        TrashWarningTier::Safe
    } else if remaining_days >= 7 {
        TrashWarningTier::Moderate
    } else if remaining_days >= 3 {
        TrashWarningTier::Urgent
    } else {
        TrashWarningTier::Critical
    }
}

/// Format "X days ago deletion" string from the deletion timestamp.
pub fn format_days_since_deletion(deleted_at: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let local_now: chrono::DateTime<Local> = Local.from_utc_datetime(&now.naive_utc());
    let local_deleted: chrono::DateTime<Local> = Local.from_utc_datetime(&deleted_at.naive_utc());

    let days = (local_now.date_naive() - local_deleted.date_naive()).num_days();
    t!("tui.list_state.deleted_days_ago", n = days.max(0)).to_string()
}

/// Format the remaining days string before automatic permanent deletion.
pub fn format_remaining_days(deleted_at: &DateTime<Utc>, retention_days: u32) -> String {
    if retention_days == 0 {
        return t!("tui.list_state.will_not_auto_delete").to_string();
    }

    let now = Utc::now();
    let local_now: chrono::DateTime<Local> = Local.from_utc_datetime(&now.naive_utc());
    let local_deleted: chrono::DateTime<Local> = Local.from_utc_datetime(&deleted_at.naive_utc());

    let days_since = (local_now.date_naive() - local_deleted.date_naive()).num_days();
    let remaining = (retention_days as i64) - days_since;
    t!("tui.list_state.remaining_days", n = remaining.max(0)).to_string()
}

/// Calculate the number of remaining days before automatic deletion.
pub fn calculate_remaining_days(deleted_at: &DateTime<Utc>, retention_days: u32) -> Option<i64> {
    if retention_days == 0 {
        return None;
    }

    let now = Utc::now();
    let local_now: chrono::DateTime<Local> = Local.from_utc_datetime(&now.naive_utc());
    let local_deleted: chrono::DateTime<Local> = Local.from_utc_datetime(&deleted_at.naive_utc());

    let days_since = (local_now.date_naive() - local_deleted.date_naive()).num_days();
    Some((retention_days as i64) - days_since)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
