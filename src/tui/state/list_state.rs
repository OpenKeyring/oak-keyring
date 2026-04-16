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
use crate::types::credential::CredentialType;
use crate::types::record::TuiRecord;

// ---------------------------------------------------------------------------
// Sub-states
// ---------------------------------------------------------------------------

/// Search mode state: the current query string and cursor position within it.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    pub query: String,
    pub cursor: usize,
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
    /// Visible height of the list panel in rows, updated from layout.
    pub _visible_height: usize,
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
            _visible_height: 0,
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

    /// Enter search mode. Resets query and cursor.
    pub fn enter_search(&mut self) {
        self.mode = ListMode::Search(SearchState::default());
    }

    /// Exit search mode back to normal browsing.
    pub fn exit_search(&mut self) {
        self.mode = ListMode::Normal;
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

    /// Filter records using multi-term AND logic on name + subtitle.
    /// Each whitespace-separated term must appear (case-insensitive) in either
    /// the record name or subtitle.
    pub fn apply_search_filter(&self, all_records: Vec<TuiRecord>) -> Vec<TuiRecord> {
        let query = match &self.mode {
            ListMode::Search(state) => state.query.trim().to_lowercase(),
            _ => return all_records,
        };

        if query.is_empty() {
            return all_records;
        }

        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() {
            return all_records;
        }

        all_records
            .into_iter()
            .filter(|record| {
                let name_lower = record.name.to_lowercase();
                let subtitle_lower = record.subtitle.to_lowercase();
                terms
                    .iter()
                    .all(|term| name_lower.contains(term) || subtitle_lower.contains(term))
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
/// - Yesterday -> 昨天
/// - < 7 days -> N天前
/// - < 30 days -> N周前
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
        "昨天".to_string()
    } else if day_diff < 7 {
        format!("{}天前", day_diff)
    } else if day_diff < 30 {
        let weeks = day_diff / 7;
        format!("{}周前", weeks)
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
pub fn format_type_prefix(cred_type: &CredentialType) -> &'static str {
    match cred_type {
        CredentialType::Login => "",
        CredentialType::Api => "[API] ",
        CredentialType::Ssh => "[SSH] ",
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

/// Format "X 天前删除" string from the deletion timestamp.
pub fn format_days_since_deletion(deleted_at: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let local_now: chrono::DateTime<Local> = Local.from_utc_datetime(&now.naive_utc());
    let local_deleted: chrono::DateTime<Local> = Local.from_utc_datetime(&deleted_at.naive_utc());

    let days = (local_now.date_naive() - local_deleted.date_naive()).num_days();
    format!("{} 天前删除", days.max(0))
}

/// Format the remaining days string before automatic permanent deletion.
pub fn format_remaining_days(
    deleted_at: &DateTime<Utc>,
    retention_days: u32,
) -> String {
    if retention_days == 0 {
        return "不会自动删除".to_string();
    }

    let now = Utc::now();
    let local_now: chrono::DateTime<Local> = Local.from_utc_datetime(&now.naive_utc());
    let local_deleted: chrono::DateTime<Local> = Local.from_utc_datetime(&deleted_at.naive_utc());

    let days_since = (local_now.date_naive() - local_deleted.date_naive()).num_days();
    let remaining = (retention_days as i64) - days_since;
    format!("剩余 {} 天", remaining.max(0))
}

/// Calculate the number of remaining days before automatic deletion.
pub fn calculate_remaining_days(
    deleted_at: &DateTime<Utc>,
    retention_days: u32,
) -> Option<i64> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a TuiRecord with minimal fields for testing.
    fn make_record(id: Uuid, name: &str, subtitle: &str) -> TuiRecord {
        TuiRecord {
            id,
            credential_type: CredentialType::Login,
            name: name.to_string(),
            subtitle: subtitle.to_string(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
            deleted_at: None,
            tags: Vec::new(),
            sync_status: None,
        }
    }

    #[allow(dead_code)]
    fn make_record_with_type(id: Uuid, name: &str, cred_type: CredentialType) -> TuiRecord {
        TuiRecord {
            id,
            credential_type: cred_type,
            name: name.to_string(),
            subtitle: String::new(),
            is_favorite: false,
            is_expired: false,
            expires_at: None,
            has_weak_password: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted: false,
            deleted_at: None,
            tags: Vec::new(),
            sync_status: None,
        }
    }

    #[test]
    fn list_state_default_empty() {
        let state = ListPanelState::default();
        assert!(state.records.is_empty());
        assert_eq!(state.selected_index, None);
        assert_eq!(state.scroll_offset, 0);
        assert!(matches!(state.mode, ListMode::Normal));
        assert_eq!(state.sort.field, SortField::CreatedAt);
        assert_eq!(state.sort.direction, SortDirection::Desc);
        assert_eq!(state.total_count, 0);
        assert_eq!(state._visible_height, 0);
    }

    #[test]
    fn list_state_with_records_selects_first() {
        let r1 = make_record(Uuid::new_v4(), "Alpha", "");
        let r2 = make_record(Uuid::new_v4(), "Beta", "");
        let state = ListPanelState::with_records(vec![r1.clone(), r2]);
        assert_eq!(state.records.len(), 2);
        assert_eq!(state.selected_index, Some(0));
        assert_eq!(state.total_count, 2);
        assert_eq!(state.selected_record().unwrap().name, "Alpha");
    }

    #[test]
    fn list_state_move_down() {
        let r1 = make_record(Uuid::new_v4(), "A", "");
        let r2 = make_record(Uuid::new_v4(), "B", "");
        let r3 = make_record(Uuid::new_v4(), "C", "");
        let mut state = ListPanelState::with_records(vec![r1, r2, r3]);

        assert_eq!(state.selected_index, Some(0));
        state.move_down();
        assert_eq!(state.selected_index, Some(1));
        state.move_down();
        assert_eq!(state.selected_index, Some(2));
        // Clamp at last record
        state.move_down();
        assert_eq!(state.selected_index, Some(2));
    }

    #[test]
    fn list_state_move_up() {
        let r1 = make_record(Uuid::new_v4(), "A", "");
        let r2 = make_record(Uuid::new_v4(), "B", "");
        let mut state = ListPanelState::with_records(vec![r1, r2]);

        state.selected_index = Some(1);
        state.move_up();
        assert_eq!(state.selected_index, Some(0));
        // Clamp at first record
        state.move_up();
        assert_eq!(state.selected_index, Some(0));
    }

    #[test]
    fn sort_toggle_direction() {
        let mut state = ListPanelState::default();
        assert_eq!(state.sort.direction, SortDirection::Desc);
        state.toggle_sort_direction();
        assert_eq!(state.sort.direction, SortDirection::Asc);
        state.toggle_sort_direction();
        assert_eq!(state.sort.direction, SortDirection::Desc);
    }

    #[test]
    fn sort_cycle_field() {
        let mut state = ListPanelState::default();
        assert_eq!(state.sort.field, SortField::CreatedAt);

        state.cycle_sort_field();
        assert_eq!(state.sort.field, SortField::UpdatedAt);

        state.cycle_sort_field();
        assert_eq!(state.sort.field, SortField::Name);

        state.cycle_sort_field();
        assert_eq!(state.sort.field, SortField::UsageFrequency);

        // Full cycle back to CreatedAt
        state.cycle_sort_field();
        assert_eq!(state.sort.field, SortField::CreatedAt);
    }

    #[test]
    fn search_mode_enter_exit() {
        let mut state = ListPanelState::default();
        assert!(!state.is_searching());

        state.enter_search();
        assert!(state.is_searching());
        assert!(!state.is_visual());

        state.exit_search();
        assert!(!state.is_searching());
    }

    #[test]
    fn visual_mode_toggle_select() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let r1 = make_record(id1, "A", "");
        let r2 = make_record(id2, "B", "");
        let mut state = ListPanelState::with_records(vec![r1, r2]);

        state.enter_visual();
        assert!(state.is_visual());
        assert!(state.visual_selected_ids().is_empty());

        // Toggle selects id1 and auto-advances to index 1
        state.toggle_select_current();
        assert_eq!(state.selected_index, Some(1));
        assert!(state.visual_selected_ids().contains(&id1));
        assert!(!state.visual_selected_ids().contains(&id2));

        // Toggle again selects id2 and clamps at last index
        state.toggle_select_current();
        assert!(state.visual_selected_ids().contains(&id2));
        assert_eq!(state.selected_index, Some(1)); // clamped at last

        // Toggle id2 off
        state.toggle_select_current();
        assert!(!state.visual_selected_ids().contains(&id2));
    }

    #[test]
    fn visual_select_all_deselect_all() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let r1 = make_record(id1, "A", "");
        let r2 = make_record(id2, "B", "");
        let mut state = ListPanelState::with_records(vec![r1, r2]);

        state.enter_visual();
        state.select_all();
        let ids = state.visual_selected_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));

        state.deselect_all();
        assert!(state.visual_selected_ids().is_empty());
    }

    #[test]
    fn search_filter_fuzzy_match() {
        let r1 = make_record(Uuid::new_v4(), "GitHub Login", "user@github.com");
        let r2 = make_record(Uuid::new_v4(), "AWS Credentials", "admin@aws.com");
        let r3 = make_record(Uuid::new_v4(), "GitLab Token", "dev@gitlab.com");
        let all = vec![r1, r2, r3];

        let mut state = ListPanelState::default();
        state.enter_search();
        state.update_search_query("git".to_string());

        let filtered = state.apply_search_filter(all);
        assert_eq!(filtered.len(), 2); // GitHub and GitLab
        assert!(filtered.iter().any(|r| r.name == "GitHub Login"));
        assert!(filtered.iter().any(|r| r.name == "GitLab Token"));
    }

    #[test]
    fn search_filter_multi_term_and_logic() {
        let r1 = make_record(Uuid::new_v4(), "GitHub Personal Token", "dev@github.com");
        let r2 = make_record(Uuid::new_v4(), "GitHub Login", "user@github.com");
        let r3 = make_record(Uuid::new_v4(), "GitLab Token", "dev@gitlab.com");
        let all = vec![r1, r2, r3];

        let mut state = ListPanelState::default();
        state.enter_search();
        // Both "github" AND "token" must match
        state.update_search_query("github token".to_string());

        let filtered = state.apply_search_filter(all);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "GitHub Personal Token");
    }

    #[test]
    fn batch_cleanup_removes_records() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let r1 = make_record(id1, "A", "");
        let r2 = make_record(id2, "B", "");
        let r3 = make_record(id3, "C", "");
        let mut state = ListPanelState::with_records(vec![r1, r2, r3]);
        assert_eq!(state.records.len(), 3);

        // Select the second record, then remove the first two
        state.selected_index = Some(1);
        state.enter_visual();
        state.cleanup_after_batch(&[id1, id2]);

        // After cleanup: only r3 remains, mode is back to Normal
        assert_eq!(state.records.len(), 1);
        assert_eq!(state.records[0].id, id3);
        assert!(!state.is_visual());
        // Selection should be clamped to the remaining last item
        assert_eq!(state.selected_index, Some(0));
        assert_eq!(state.total_count, 1);
    }

    #[test]
    fn format_type_prefix_variants() {
        assert_eq!(format_type_prefix(&CredentialType::Login), "");
        assert_eq!(format_type_prefix(&CredentialType::Api), "[API] ");
        assert_eq!(format_type_prefix(&CredentialType::Ssh), "[SSH] ");
    }

    #[test]
    fn format_relative_time_today() {
        let now = Utc::now();
        let result = format_relative_time(&now);
        // Should be HH:MM format
        let parts: Vec<&str> = result.split(':').collect();
        assert_eq!(parts.len(), 2);
        let hour: u32 = parts[0].parse().unwrap();
        let min: u32 = parts[1].parse().unwrap();
        assert!(hour < 24);
        assert!(min < 60);
    }

    #[test]
    fn format_relative_time_yesterday() {
        let yesterday = Utc::now() - chrono::Duration::try_days(1).unwrap();
        let result = format_relative_time(&yesterday);
        assert_eq!(result, "昨天");
    }

    #[test]
    fn format_relative_time_days_ago() {
        let dt = Utc::now() - chrono::Duration::try_days(3).unwrap();
        let result = format_relative_time(&dt);
        assert!(result.contains("天前"));
    }

    #[test]
    fn format_relative_time_weeks_ago() {
        let dt = Utc::now() - chrono::Duration::try_days(14).unwrap();
        let result = format_relative_time(&dt);
        assert!(result.contains("周前"));
    }

    #[test]
    fn update_search_query_sets_cursor() {
        let mut state = ListPanelState::default();
        state.enter_search();
        state.update_search_query("hello world".to_string());

        if let ListMode::Search(ref s) = state.mode {
            assert_eq!(s.query, "hello world");
            assert_eq!(s.cursor, 11); // len of "hello world"
        } else {
            panic!("Expected search mode");
        }
    }

    #[test]
    fn adjust_scroll_keeps_selected_visible() {
        let records: Vec<TuiRecord> = (0..20)
            .map(|i| make_record(Uuid::new_v4(), &format!("R{}", i), ""))
            .collect();
        let mut state = ListPanelState::with_records(records);
        state._visible_height = 5; // only 5 rows visible

        // Jump to near the end
        state.selected_index = Some(18);
        state.adjust_scroll();
        // scroll_offset should be 18 - 5 + 1 = 14
        assert_eq!(state.scroll_offset, 14);

        // Jump back to top
        state.selected_index = Some(0);
        state.adjust_scroll();
        assert_eq!(state.scroll_offset, 0);
    }

    // ── Trash time helper tests ────────────────────────────────────────────

    #[test]
    fn format_days_since_deletion_today() {
        let now = Utc::now();
        let result = format_days_since_deletion(&now);
        assert_eq!(result, "0 天前删除");
    }

    #[test]
    fn format_days_since_deletion_yesterday() {
        let yesterday = Utc::now() - chrono::Duration::try_days(1).unwrap();
        let result = format_days_since_deletion(&yesterday);
        assert_eq!(result, "1 天前删除");
    }

    #[test]
    fn format_days_since_deletion_week() {
        let dt = Utc::now() - chrono::Duration::try_days(7).unwrap();
        let result = format_days_since_deletion(&dt);
        assert_eq!(result, "7 天前删除");
    }

    #[test]
    fn format_remaining_days_normal() {
        let deleted_at = Utc::now() - chrono::Duration::try_days(10).unwrap();
        let retention_days = 30;
        let result = format_remaining_days(&deleted_at, retention_days);
        assert!(result.contains("剩余"));
        assert!(result.contains("天"));
    }

    #[test]
    fn format_remaining_days_never_delete() {
        let deleted_at = Utc::now() - chrono::Duration::try_days(10).unwrap();
        let result = format_remaining_days(&deleted_at, 0);
        assert_eq!(result, "不会自动删除");
    }

    #[test]
    fn format_remaining_days_expired() {
        let deleted_at = Utc::now() - chrono::Duration::try_days(31).unwrap();
        let retention_days = 30;
        let result = format_remaining_days(&deleted_at, retention_days);
        assert!(result.contains("剩余"));
    }

    #[test]
    fn trash_warning_tier_safe() {
        let tier = trash_warning_tier(20);
        assert_eq!(tier, TrashWarningTier::Safe);
    }

    #[test]
    fn trash_warning_tier_moderate() {
        let tier = trash_warning_tier(10);
        assert_eq!(tier, TrashWarningTier::Moderate);
    }

    #[test]
    fn trash_warning_tier_urgent() {
        let tier = trash_warning_tier(5);
        assert_eq!(tier, TrashWarningTier::Urgent);
    }

    #[test]
    fn trash_warning_tier_critical() {
        let tier = trash_warning_tier(2);
        assert_eq!(tier, TrashWarningTier::Critical);
    }

    #[test]
    fn trash_warning_tier_zero() {
        let tier = trash_warning_tier(0);
        assert_eq!(tier, TrashWarningTier::Critical);
    }

    // ── Post-batch cleanup tests ─────────────────────────────────────────

    #[test]
    fn cleanup_after_batch_exits_visual_and_positions_cursor() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let r1 = make_record(id1, "A", "");
        let r2 = make_record(id2, "B", "");
        let r3 = make_record(id3, "C", "");
        let mut state = ListPanelState::with_records(vec![r1, r2, r3]);

        // Enter visual, select id1 and id2, cursor at index 1
        state.enter_visual();
        if let ListMode::Visual(ref mut vs) = state.mode {
            vs.selected_ids.insert(id1);
            vs.selected_ids.insert(id2);
        }
        state.selected_index = Some(1);

        // Batch delete id1 and id2
        state.cleanup_after_batch(&[id1, id2]);

        // Should exit visual mode
        assert!(!state.is_visual());
        // Only r3 remains
        assert_eq!(state.records.len(), 1);
        // Cursor should be clamped to valid index
        assert!(state.selected_index.unwrap() < state.records.len());
        // Total count updated
        assert_eq!(state.total_count, 1);
    }

    #[test]
    fn cleanup_preserves_cursor_if_record_remains() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();
        let r1 = make_record(id1, "A", "");
        let r2 = make_record(id2, "B", "");
        let r3 = make_record(id3, "C", "");
        let mut state = ListPanelState::with_records(vec![r1, r2, r3]);

        // Cursor at r3 (index 2), delete r1 only
        state.selected_index = Some(2);
        state.enter_visual();
        if let ListMode::Visual(ref mut vs) = state.mode {
            vs.selected_ids.insert(id1);
        }
        state.cleanup_after_batch(&[id1]);

        // r3 (index 1 after removal) should still be selected
        assert_eq!(state.records.len(), 2);
        assert!(state.selected_index.unwrap() < state.records.len());
    }

    #[test]
    fn cleanup_empty_list_sets_none_selection() {
        let id1 = Uuid::new_v4();
        let r1 = make_record(id1, "A", "");
        let mut state = ListPanelState::with_records(vec![r1]);

        state.enter_visual();
        state.cleanup_after_batch(&[id1]);

        assert!(state.records.is_empty());
        assert_eq!(state.selected_index, None);
        assert!(!state.is_visual());
    }
}
