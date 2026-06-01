//! Tag management state: sort order, inline rename, management mode aggregation.
//!
//! Provides the state layer for tag management mode in the sidebar:
//! - [`TagSortOrder`] — cycling sort order (frequency / alphabetical / recently used)
//! - [`InlineEditState`] — inline rename edit box state
//! - [`TagManagementState`] — aggregated management mode state

/// Tag sort order, cycled by pressing `s` in management mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagSortOrder {
    /// Sort by frequency descending (password count, default).
    #[default]
    Frequency,
    /// Sort by name alphabetically.
    Alphabetical,
    /// Sort by most recently used.
    RecentlyUsed,
}

impl TagSortOrder {
    /// Cycle to the next sort order: Frequency -> Alphabetical -> RecentlyUsed -> Frequency
    pub fn cycle(&mut self) {
        *self = match self {
            TagSortOrder::Frequency => TagSortOrder::Alphabetical,
            TagSortOrder::Alphabetical => TagSortOrder::RecentlyUsed,
            TagSortOrder::RecentlyUsed => TagSortOrder::Frequency,
        };
    }

    /// Return the display label for the current sort order.
    pub fn label(&self) -> String {
        match self {
            TagSortOrder::Frequency => crate::t!("tui.main.sidebar_sort_frequency").to_string(),
            TagSortOrder::Alphabetical => crate::t!("tui.main.sidebar_sort_name").to_string(),
            TagSortOrder::RecentlyUsed => crate::t!("tui.main.sidebar_sort_recent").to_string(),
        }
    }

    /// Return all sort orders in cycle order.
    pub fn all() -> &'static [TagSortOrder] {
        &[
            TagSortOrder::Frequency,
            TagSortOrder::Alphabetical,
            TagSortOrder::RecentlyUsed,
        ]
    }
}

/// Inline rename edit box state for a tag.
#[derive(Debug, Clone)]
pub struct InlineEditState {
    /// The original tag name before editing.
    pub original_name: String,
    /// Current text in the edit box.
    pub text: String,
    /// Cursor position within `text`.
    pub cursor: usize,
    /// Whether a duplicate-name conflict has been detected.
    pub conflict: bool,
}

impl InlineEditState {
    /// Create a new inline edit state pre-filled with the current tag name.
    pub fn new(tag_name: &str) -> Self {
        let cursor = tag_name.len();
        Self {
            original_name: tag_name.to_string(),
            text: tag_name.to_string(),
            cursor,
            conflict: false,
        }
    }

    /// Insert a character at the cursor position and advance the cursor.
    pub fn insert_char(&mut self, ch: char) {
        if crate::types::record_limits::char_count(&self.text)
            >= crate::types::record_limits::MAX_TAG_CHARS
        {
            return;
        }
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
        self.conflict = false;
    }

    /// Delete the character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.drain(prev..self.cursor);
            self.cursor = prev;
            self.conflict = false;
        }
    }

    /// Move cursor left by one character.
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.cursor = prev;
        }
    }

    /// Move cursor right by one character.
    pub fn cursor_right(&mut self) {
        if self.cursor < self.text.len() {
            let next = self.text[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.text.len());
            self.cursor = next;
        }
    }

    /// Confirm the edit. Returns the new name.
    pub fn confirm(self) -> String {
        self.text.trim().to_string()
    }

    /// Check if the current text conflicts with an existing tag name.
    pub fn check_conflict(&mut self, existing_tags: &[String]) {
        let trimmed = self.text.trim().to_lowercase();
        self.conflict = existing_tags
            .iter()
            .any(|t| t.to_lowercase() == trimmed && *t != self.original_name);
    }
}

/// Aggregated state for tag management mode in the sidebar.
#[derive(Debug, Clone, Default)]
pub struct TagManagementState {
    /// Current sort order.
    pub sort_order: TagSortOrder,
    /// Active inline edit, if any.
    pub inline_edit: Option<InlineEditState>,
}

impl TagManagementState {
    /// Create a new management state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter inline rename mode for a tag.
    pub fn start_rename(&mut self, tag_name: &str) {
        self.inline_edit = Some(InlineEditState::new(tag_name));
    }

    /// Cancel inline rename mode.
    pub fn cancel_rename(&mut self) {
        self.inline_edit = None;
    }

    /// Whether inline rename is active.
    pub fn is_renaming(&self) -> bool {
        self.inline_edit.is_some()
    }

    /// Take the inline edit state, consuming it.
    pub fn take_rename(&mut self) -> Option<InlineEditState> {
        self.inline_edit.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_sort_order_cycle() {
        let mut order = TagSortOrder::default();
        assert_eq!(order, TagSortOrder::Frequency);
        order.cycle();
        assert_eq!(order, TagSortOrder::Alphabetical);
        order.cycle();
        assert_eq!(order, TagSortOrder::RecentlyUsed);
        order.cycle();
        assert_eq!(order, TagSortOrder::Frequency);
    }

    #[test]
    fn tag_sort_order_labels() {
        // Tests now check that labels are non-empty (content may be localized)
        assert!(!TagSortOrder::Frequency.label().is_empty());
        assert!(!TagSortOrder::Alphabetical.label().is_empty());
        assert!(!TagSortOrder::RecentlyUsed.label().is_empty());
    }

    #[test]
    fn tag_sort_order_all_count() {
        assert_eq!(TagSortOrder::all().len(), 3);
    }

    #[test]
    fn inline_edit_new_prefilled() {
        let edit = InlineEditState::new("work");
        assert_eq!(edit.text, "work");
        assert_eq!(edit.cursor, 4);
        assert_eq!(edit.original_name, "work");
        assert!(!edit.conflict);
    }

    #[test]
    fn inline_edit_insert_char() {
        let mut edit = InlineEditState::new("wor");
        edit.cursor = 3;
        edit.insert_char('k');
        assert_eq!(edit.text, "work");
        assert_eq!(edit.cursor, 4);
    }

    #[test]
    fn inline_edit_insert_unicode() {
        let mut edit = InlineEditState::new("工");
        edit.insert_char('作');
        assert_eq!(edit.text, "工作");
    }

    #[test]
    fn inline_edit_insert_stops_at_tag_limit() {
        let mut edit =
            InlineEditState::new(&"a".repeat(crate::types::record_limits::MAX_TAG_CHARS));
        edit.insert_char('b');
        assert_eq!(
            edit.text.chars().count(),
            crate::types::record_limits::MAX_TAG_CHARS
        );
    }

    #[test]
    fn inline_edit_backspace() {
        let mut edit = InlineEditState::new("work");
        edit.cursor = 4;
        edit.backspace();
        assert_eq!(edit.text, "wor");
        assert_eq!(edit.cursor, 3);
    }

    #[test]
    fn inline_edit_backspace_at_start() {
        let mut edit = InlineEditState::new("work");
        edit.cursor = 0;
        edit.backspace();
        assert_eq!(edit.text, "work");
        assert_eq!(edit.cursor, 0);
    }

    #[test]
    fn inline_edit_cursor_movement() {
        let mut edit = InlineEditState::new("abc");
        edit.cursor = 3;
        edit.cursor_left();
        assert_eq!(edit.cursor, 2);
        edit.cursor_right();
        assert_eq!(edit.cursor, 3);
        edit.cursor_right();
        assert_eq!(edit.cursor, 3);
    }

    #[test]
    fn inline_edit_cursor_unicode() {
        let mut edit = InlineEditState::new("你好");
        edit.cursor = "你好".len();
        edit.cursor_left();
        assert_eq!(edit.cursor, 3);
        edit.cursor_left();
        assert_eq!(edit.cursor, 0);
    }

    #[test]
    fn inline_edit_conflict_detection() {
        let mut edit = InlineEditState::new("personal");
        let existing = vec!["work".to_string(), "personal".to_string()];
        edit.check_conflict(&existing);
        assert!(!edit.conflict);

        edit.text = "work".to_string();
        edit.cursor = 4;
        edit.check_conflict(&existing);
        assert!(edit.conflict);
    }

    #[test]
    fn inline_edit_conflict_case_insensitive() {
        let mut edit = InlineEditState::new("personal");
        let existing = vec!["Work".to_string()];
        edit.text = "work".to_string();
        edit.cursor = 4;
        edit.check_conflict(&existing);
        assert!(edit.conflict);
    }

    #[test]
    fn inline_edit_conflict_cleared_on_edit() {
        let mut edit = InlineEditState::new("personal");
        let existing = vec!["work".to_string()];
        edit.text = "work".to_string();
        edit.cursor = 4;
        edit.check_conflict(&existing);
        assert!(edit.conflict);
        edit.insert_char('x');
        assert!(!edit.conflict);
    }

    #[test]
    fn inline_edit_confirm() {
        let edit = InlineEditState::new("old");
        let result = edit.confirm();
        assert_eq!(result, "old");
    }

    #[test]
    fn inline_edit_confirm_trims() {
        let mut edit = InlineEditState::new("old");
        edit.text = "  new  ".to_string();
        let result = edit.confirm();
        assert_eq!(result, "new");
    }

    #[test]
    fn tag_management_state_default() {
        let state = TagManagementState::default();
        assert_eq!(state.sort_order, TagSortOrder::Frequency);
        assert!(state.inline_edit.is_none());
        assert!(!state.is_renaming());
    }

    #[test]
    fn tag_management_start_cancel_rename() {
        let mut state = TagManagementState::new();
        assert!(!state.is_renaming());
        state.start_rename("work");
        assert!(state.is_renaming());
        assert_eq!(state.inline_edit.as_ref().unwrap().text, "work");
        state.cancel_rename();
        assert!(!state.is_renaming());
    }

    #[test]
    fn tag_management_take_rename() {
        let mut state = TagManagementState::new();
        state.start_rename("work");
        let edit = state.take_rename();
        assert!(edit.is_some());
        assert_eq!(edit.unwrap().text, "work");
        assert!(!state.is_renaming());
    }
}
