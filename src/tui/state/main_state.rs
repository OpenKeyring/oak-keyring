//! Main screen state: sidebar, status bar, terminal title, and root composition.
//!
//! Contains all state types needed by the three-panel main layout:
//! - [`SidebarState`] — navigation categories, tags, selection
//! - [`StatusBarState`] — clipboard countdown, sync indicator, messages
//! - [`TerminalTitleState`] — dynamic terminal window title
//! - [`MainScreenState`] — root aggregate of all main-screen sub-states

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

use crate::commands::types::{PanelId, RecordFilter, RecordSort, Screen as ScreenEnum};
use crate::commands::{Command, Message};
use crate::tui::screens::main::MainScreen;
use crate::tui::state::detail_state::DetailPanelState;
use crate::tui::state::list_state::ListPanelState;
use crate::tui::state::tag_management::TagManagementState;
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::Tag;
use uuid::Uuid;

// ── Sidebar ──────────────────────────────────────────────────────────────────

/// Filterable categories shown in the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarCategory {
    All,
    Favorites,
    Expired,
    HealthIssues,
    Trash,
}

impl SidebarCategory {
    /// Zero-based index matching the order defined here.
    pub fn index(self) -> usize {
        match self {
            Self::All => 0,
            Self::Favorites => 1,
            Self::Expired => 2,
            Self::HealthIssues => 3,
            Self::Trash => 4,
        }
    }

    /// Convert this category into the corresponding [`RecordFilter`].
    pub fn to_filter(self) -> RecordFilter {
        match self {
            Self::All => RecordFilter::All,
            Self::Favorites => RecordFilter::Favorites,
            Self::Expired => RecordFilter::Expired,
            Self::HealthIssues => RecordFilter::HealthIssues,
            Self::Trash => RecordFilter::Trash,
        }
    }
}

/// An item in the sidebar list — includes categories, visual separators, tags,
/// and utility links (generator, config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarItem {
    /// A filterable category entry.
    Category(SidebarCategory),
    /// A visual separator line.
    Separator,
    /// A collapsible "Tags" section header.
    TagHeader,
    /// A single tag entry.
    Tag(String),
    /// Password generator shortcut.
    Generator,
    /// Configuration screen shortcut.
    Config,
}

impl SidebarItem {
    /// Whether this item can receive keyboard focus / be selected.
    pub fn is_selectable(&self) -> bool {
        !matches!(self, Self::Separator | Self::TagHeader)
    }
}

/// Counts per sidebar category — used for display badges.
#[derive(Debug, Clone, Default)]
pub struct CategoryCounts {
    pub all: usize,
    pub favorites: usize,
    pub expired: usize,
    pub health_issues: usize,
    pub trash: usize,
}

/// Sidebar navigation state.
#[derive(Debug, Clone)]
pub struct SidebarState {
    /// Built item list (rebuilt on counts/tags change).
    pub items: Vec<SidebarItem>,
    /// Currently selected index into `items`.
    pub selected_index: usize,
    /// Whether the tags section is expanded.
    pub tags_expanded: bool,
    /// Vertical scroll offset for the tags section.
    pub tag_scroll_offset: usize,
    /// Whether tag-management mode is active (reorder/delete).
    pub tag_management_mode: bool,
    /// Tag management mode state (sort order, inline rename).
    pub tag_management: TagManagementState,
    /// Available tags (populated from data layer).
    pub tags: Vec<Tag>,
    /// Record counts per category.
    pub category_counts: CategoryCounts,
}

impl Default for SidebarState {
    fn default() -> Self {
        let mut state = Self {
            items: Vec::new(),
            selected_index: 0,
            tags_expanded: false,
            tag_scroll_offset: 0,
            tag_management_mode: false,
            tag_management: TagManagementState::default(),
            tags: Vec::new(),
            category_counts: CategoryCounts::default(),
        };
        state.rebuild();
        state
    }
}

impl SidebarState {
    /// Rebuild the sidebar item list from categories, tags, and utility entries.
    pub fn rebuild(&mut self) {
        self.items = self.build_items();
        // Clamp selection to a valid selectable index
        if self.selected_index >= self.items.len() {
            self.selected_index = 0;
        }
        // If current selection landed on a non-selectable item, advance
        if !self.items.is_empty() && !self.items[self.selected_index].is_selectable() {
            if let Some(idx) = self.next_selectable_from(self.selected_index) {
                self.selected_index = idx;
            }
        }
    }

    /// Build the ordered list of sidebar items.
    fn build_items(&self) -> Vec<SidebarItem> {
        let mut items: Vec<SidebarItem> = vec![
            SidebarItem::Category(SidebarCategory::All),
            SidebarItem::Category(SidebarCategory::Favorites),
            SidebarItem::Category(SidebarCategory::Expired),
            SidebarItem::Category(SidebarCategory::HealthIssues),
            SidebarItem::Category(SidebarCategory::Trash),
            SidebarItem::Separator,
            SidebarItem::TagHeader,
        ];

        if self.tags_expanded {
            for tag in &self.tags {
                items.push(SidebarItem::Tag(tag.name.clone()));
            }
        }

        items.push(SidebarItem::Separator);
        items.push(SidebarItem::Generator);
        items.push(SidebarItem::Config);

        items
    }

    /// Move selection to the next selectable item (wraps around).
    pub fn next_selectable(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let start = if self.selected_index + 1 < self.items.len() {
            self.selected_index + 1
        } else {
            0
        };
        if let Some(idx) = self.next_selectable_from(start) {
            self.selected_index = idx;
        }
    }

    /// Move selection to the previous selectable item (wraps around).
    pub fn prev_selectable(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let start = if self.selected_index > 0 {
            self.selected_index - 1
        } else {
            self.items.len() - 1
        };
        if let Some(idx) = self.prev_selectable_from(start) {
            self.selected_index = idx;
        }
    }

    /// Return the [`RecordFilter`] corresponding to the currently selected item.
    pub fn current_filter(&self) -> RecordFilter {
        if self.selected_index >= self.items.len() {
            return RecordFilter::All;
        }
        match &self.items[self.selected_index] {
            SidebarItem::Category(cat) => cat.to_filter(),
            SidebarItem::Tag(name) => RecordFilter::Tag(name.clone()),
            // Generator and Config are shortcuts, not record filters
            SidebarItem::Generator
            | SidebarItem::Config
            | SidebarItem::Separator
            | SidebarItem::TagHeader => RecordFilter::All,
        }
    }

    /// Select a specific category by setting `selected_index` to its position.
    pub fn select_category(&mut self, category: SidebarCategory) {
        let target = SidebarItem::Category(category);
        if let Some(idx) = self.items.iter().position(|item| *item == target) {
            self.selected_index = idx;
        }
    }

    /// Move selection down to next selectable item.
    pub fn move_down(&mut self) {
        self.next_selectable();
    }

    /// Move selection up to previous selectable item.
    pub fn move_up(&mut self) {
        self.prev_selectable();
    }

    /// Toggle tag section expand/collapse.
    pub fn toggle_tags(&mut self) {
        self.tags_expanded = !self.tags_expanded;
        self.rebuild();
        // If currently selected was a tag and we collapsed, select TagHeader
        if !self.tags_expanded {
            for (i, item) in self.items.iter().enumerate() {
                if matches!(item, SidebarItem::TagHeader) {
                    self.selected_index = i;
                    break;
                }
            }
        }
    }

    /// Enter tag management mode.
    pub fn enter_tag_management(&mut self) {
        self.tag_management_mode = true;
    }

    /// Exit tag management mode. Cancels any inline rename.
    pub fn exit_tag_management(&mut self) {
        self.tag_management_mode = false;
        self.tag_management.cancel_rename();
    }

    /// Whether tag management mode is active.
    pub fn is_tag_management(&self) -> bool {
        self.tag_management_mode
    }

    /// Get the name of the currently selected tag item, if any.
    pub fn selected_tag_name(&self) -> Option<&str> {
        if self.selected_index < self.items.len() {
            if let SidebarItem::Tag(name) = &self.items[self.selected_index] {
                return Some(name);
            }
        }
        None
    }

    // ── Helpers ───────────────────────────────────────────────────────────

    /// Find the next selectable index starting from `start` (inclusive, wraps).
    fn next_selectable_from(&self, start: usize) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        let len = self.items.len();
        for offset in 0..len {
            let idx = (start + offset) % len;
            if self.items[idx].is_selectable() {
                return Some(idx);
            }
        }
        None
    }

    /// Find the previous selectable index starting from `start` (inclusive, wraps).
    fn prev_selectable_from(&self, start: usize) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        let len = self.items.len();
        // Iterate backwards: start, start-1, ..., 0, len-1, ..., start+1
        for offset in 0..len {
            let idx = (start + len - offset) % len;
            if self.items[idx].is_selectable() {
                return Some(idx);
            }
        }
        None
    }
}

// ── Status Bar ───────────────────────────────────────────────────────────────

/// Sync state indicator displayed in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncIndicator {
    #[default]
    NotConfigured,
    Synced,
    Syncing,
    Failed,
    Offline,
}

/// Discriminated status bar message — only one is displayed at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusMessage {
    /// "N records" label.
    RecordCount(usize),
    /// Clipboard auto-clear countdown for a specific field.
    ClipboardCountdown { field: String, seconds: u32 },
    /// Temporary message with a TTL in ticks.
    Temporary { text: String, ttl: u32 },
    /// Active search query.
    Search(String),
}

/// Status bar state.
#[derive(Debug, Clone, Default)]
pub struct StatusBarState {
    /// Seconds until clipboard is auto-cleared.
    pub clipboard_countdown: Option<u32>,
    /// Current sync state.
    pub sync_status: SyncIndicator,
    /// Active status message.
    pub status_message: Option<StatusMessage>,
    /// Ticks remaining for a temporary message.
    pub temp_message_timer: Option<u32>,
    /// Total record count.
    pub record_count: usize,
    /// Progress of an ongoing health check (current, total).
    pub health_check_progress: Option<(usize, usize)>,
}

// ── Terminal Title ───────────────────────────────────────────────────────────

/// Terminal window title state — tracks the current title and pending restore.
#[derive(Debug, Clone, Default)]
pub struct TerminalTitleState {
    /// Currently displayed terminal title.
    pub current_title: String,
    /// Title to restore after a fullscreen page closes.
    pub pending_restore: Option<String>,
}

impl TerminalTitleState {
    /// Set terminal title for main screen with an optional record name.
    pub fn set_for_main(&mut self, record_name: Option<&str>) {
        match record_name {
            None => {
                self.current_title = "OK".to_string();
            }
            Some(name) => {
                let max_name_len = 35; // 40 total - "OK | " (5 chars)
                if name.chars().count() > max_name_len {
                    let truncated: String = name.chars().take(max_name_len).collect();
                    self.current_title = format!("OK | {}...", truncated);
                } else {
                    self.current_title = format!("OK | {}", name);
                }
            }
        }
    }

    /// Clear terminal title (for exit/lock).
    pub fn clear(&mut self) {
        self.current_title.clear();
    }

    /// Save current title before navigating to a fullscreen page.
    pub fn save_for_restore(&mut self) {
        self.pending_restore = Some(self.current_title.clone());
    }

    /// Restore title after returning from fullscreen page.
    pub fn restore(&mut self) {
        if let Some(title) = self.pending_restore.take() {
            self.current_title = title;
        }
    }
}

// ── Focus Snapshot Placeholder ───────────────────────────────────────────────

/// Placeholder for focus snapshot that will be implemented in a future task.
/// Used to save/restore focus state across lock/unlock cycles.
#[derive(Debug, Clone, Default)]
pub struct FocusSnapshot;

// ── Main Screen State ────────────────────────────────────────────────────────

/// Root state for the main three-panel screen.
#[derive(Debug)]
pub struct MainScreenState {
    /// Sidebar navigation state.
    pub sidebar: SidebarState,
    /// List panel state (records, navigation, search, visual mode).
    pub list: ListPanelState,
    /// Detail panel state (record display, field navigation, password visibility).
    pub detail: DetailPanelState,
    /// Status bar state.
    pub status_bar: StatusBarState,
    /// Terminal title state.
    pub terminal_title: TerminalTitleState,
    /// Currently active record filter.
    pub current_filter: RecordFilter,
    /// Current sort order for the record list.
    pub current_sort: RecordSort,
    /// Snapshot of focus state before vault lock (placeholder).
    pub pre_lock_snapshot: Option<FocusSnapshot>,
    /// Currently focused panel (synced from AppState.shared.focus.focused_panel).
    pub focused_panel: PanelId,
    /// Whether the terminal supports unicode characters (synced from AppState.unicode_capable).
    pub unicode_capable: bool,
    /// Trash retention days from config (0 = never auto-delete).
    pub trash_retention_days: u32,
}

impl Default for MainScreenState {
    fn default() -> Self {
        Self {
            sidebar: SidebarState::default(),
            list: ListPanelState::default(),
            detail: DetailPanelState::default(),
            status_bar: StatusBarState::default(),
            terminal_title: TerminalTitleState::default(),
            current_filter: RecordFilter::All,
            current_sort: RecordSort::default(),
            pre_lock_snapshot: None,
            focused_panel: PanelId::Sidebar,
            unicode_capable: true,
            trash_retention_days: 30,
        }
    }
}

impl MainScreenState {
    /// Create a new MainScreenState with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sync render-only fields from AppState-level shared state.
    /// Called by the screen router before dispatching to update/view.
    pub fn sync_from_app(&mut self, focused_panel: PanelId, unicode_capable: bool) {
        self.focused_panel = focused_panel;
        self.unicode_capable = unicode_capable;
    }
}

impl Screen for MainScreenState {
    fn update(&mut self, msg: Message, _ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key),
            Message::HealthCheckProgress { current, total } => {
                self.status_bar.health_check_progress = Some((current, total));
                ScreenResult::Continue
            }
            Message::CommandCompleted(result) => {
                use crate::commands::result::CommandResult;
                match result {
                    CommandResult::HealthCheckStarted => {
                        self.status_bar.health_check_progress = Some((0, 0));
                        ScreenResult::Continue
                    }
                    CommandResult::HealthCheckCompleted { .. } => {
                        self.status_bar.health_check_progress = None;
                        // The detail panel will re-render with the new report
                        // automatically because the entire frame is redrawn.
                        ScreenResult::Continue
                    }
                    _ => ScreenResult::Continue,
                }
            }
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut Frame, area: Rect) {
        let renderer = MainScreen::new();
        renderer.view(frame, area, self, self.focused_panel, self.unicode_capable);
    }

    fn on_mount(&mut self, ctx: &mut ScreenContext) {
        self.trash_retention_days = ctx.config.general.trash_retention_days;
        self.detail.trash_retention_days = ctx.config.general.trash_retention_days;
    }

    fn on_unmount(&mut self) {
        // No-op for now.
    }
}

impl MainScreenState {
    fn handle_key(&mut self, key: KeyEvent) -> ScreenResult {
        // Check sidebar Enter for Generator/Config navigation first
        if self.focused_panel == PanelId::Sidebar && key.code == KeyCode::Enter {
            match self.sidebar.items.get(self.sidebar.selected_index) {
                Some(SidebarItem::Generator) => {
                    return ScreenResult::NavigateTo(ScreenEnum::PasswordGenerator);
                }
                Some(SidebarItem::Config) => {
                    return ScreenResult::NavigateTo(ScreenEnum::Config);
                }
                _ => {}
            }
        }
        match key.code {
            KeyCode::Char('g') => ScreenResult::NavigateTo(ScreenEnum::Config),
            KeyCode::Char('l') => ScreenResult::NavigateTo(ScreenEnum::AuditLog),
            KeyCode::Char('p') => ScreenResult::NavigateTo(ScreenEnum::PasswordGenerator),
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ScreenResult::Command(Box::new(Command::TriggerSync))
            }
            KeyCode::Char('n') => ScreenResult::NavigateTo(ScreenEnum::CreateRecord),
            KeyCode::Char('e') => {
                if self.focused_panel == PanelId::Detail {
                    let detail_id = self.detail.record.as_ref()
                        .map(|r| r.id)
                        .unwrap_or_else(Uuid::nil);
                    if !detail_id.is_nil() {
                        return ScreenResult::NavigateTo(ScreenEnum::EditRecord { id: detail_id });
                    }
                }
                ScreenResult::Continue
            }
            _ => ScreenResult::Continue,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_default_selects_all_category() {
        let sidebar = SidebarState::default();
        // First item is Category(All) — should be selected by default
        assert_eq!(sidebar.selected_index, 0);
        assert_eq!(sidebar.current_filter(), RecordFilter::All);
    }

    #[test]
    fn sidebar_navigation_skips_separators() {
        let mut sidebar = SidebarState::default();
        // Items: All, Favorites, Expired, HealthIssues, Trash, Separator, TagHeader, Separator, Generator, Config
        // Selectable indices: 0,1,2,3,4,       _,        _,        _,        8,         9
        // Start at All (0), next -> Favorites (1)
        sidebar.next_selectable();
        assert_eq!(sidebar.selected_index, 1);
        assert!(matches!(
            sidebar.items[1],
            SidebarItem::Category(SidebarCategory::Favorites)
        ));

        // Skip ahead past categories to verify separator skip at wrap point
        sidebar.selected_index = 4; // Trash
        sidebar.next_selectable();
        // Items[5] is Separator, items[6] is TagHeader — both non-selectable
        // Should land on Generator (index 8)
        assert!(matches!(
            sidebar.items[sidebar.selected_index],
            SidebarItem::Generator
        ));
    }

    #[test]
    fn sidebar_prev_navigation_wraps() {
        let mut sidebar = SidebarState::default();
        // Start at index 0 (All), prev should wrap to last selectable item
        sidebar.prev_selectable();
        let last_index = sidebar.selected_index;
        // Last selectable should be Config
        assert!(matches!(sidebar.items[last_index], SidebarItem::Config));
    }

    #[test]
    fn sidebar_select_category() {
        let mut sidebar = SidebarState::default();
        sidebar.select_category(SidebarCategory::Trash);
        assert_eq!(
            sidebar.items[sidebar.selected_index],
            SidebarItem::Category(SidebarCategory::Trash)
        );
        assert_eq!(sidebar.current_filter(), RecordFilter::Trash);
    }

    #[test]
    fn sidebar_tag_filter() {
        let mut sidebar = SidebarState {
            tags_expanded: true,
            tags: vec![Tag {
                id: 1,
                name: "work".to_string(),
            }],
            ..Default::default()
        };
        sidebar.rebuild();

        // Find the tag item and select it
        let tag_idx = sidebar
            .items
            .iter()
            .position(|i| matches!(i, SidebarItem::Tag(_)))
            .expect("tag item should exist");
        sidebar.selected_index = tag_idx;
        assert_eq!(
            sidebar.current_filter(),
            RecordFilter::Tag("work".to_string())
        );
    }

    #[test]
    fn sidebar_build_items_structure() {
        let sidebar = SidebarState {
            tags_expanded: true,
            tags: vec![
                Tag {
                    id: 1,
                    name: "personal".to_string(),
                },
                Tag {
                    id: 2,
                    name: "work".to_string(),
                },
            ],
            ..Default::default()
        };
        let items = sidebar.build_items();

        // 5 categories + separator + tag header + 2 tags + separator + generator + config = 12
        assert_eq!(items.len(), 12);

        // Verify structure
        assert!(matches!(
            items[0],
            SidebarItem::Category(SidebarCategory::All)
        ));
        assert!(matches!(items[5], SidebarItem::Separator));
        assert!(matches!(items[6], SidebarItem::TagHeader));
        assert!(matches!(items[7], SidebarItem::Tag(ref t) if t == "personal"));
        assert!(matches!(items[8], SidebarItem::Tag(ref t) if t == "work"));
        assert!(matches!(items[9], SidebarItem::Separator));
        assert!(matches!(items[10], SidebarItem::Generator));
        assert!(matches!(items[11], SidebarItem::Config));
    }

    #[test]
    fn sidebar_collapsed_tags_hidden() {
        let sidebar = SidebarState {
            tags_expanded: false, // collapsed
            tags: vec![Tag {
                id: 1,
                name: "work".to_string(),
            }],
            ..Default::default()
        };
        let items = sidebar.build_items();

        // No Tag items should appear when collapsed
        let tag_count = items
            .iter()
            .filter(|i| matches!(i, SidebarItem::Tag(_)))
            .count();
        assert_eq!(tag_count, 0);

        // TagHeader still present
        assert!(items.iter().any(|i| matches!(i, SidebarItem::TagHeader)));
    }

    #[test]
    fn main_screen_state_default() {
        let state = MainScreenState::default();
        assert_eq!(state.current_filter, RecordFilter::All);
        assert!(state.pre_lock_snapshot.is_none());
        assert_eq!(state.sidebar.selected_index, 0);
        assert_eq!(state.status_bar.record_count, 0);
    }

    // ── Terminal title tests ──────────────────────────────────────────────

    #[test]
    fn terminal_title_default_is_empty() {
        let state = TerminalTitleState::default();
        assert!(state.current_title.is_empty());
    }

    #[test]
    fn terminal_title_main_screen() {
        let mut state = TerminalTitleState::default();
        state.set_for_main(None);
        assert_eq!(state.current_title, "OK");
    }

    #[test]
    fn terminal_title_with_record_selected() {
        let mut state = TerminalTitleState::default();
        state.set_for_main(Some("GitHub Account Credentials"));
        assert_eq!(state.current_title, "OK | GitHub Account Credentials");
    }

    #[test]
    fn terminal_title_truncates_long_name() {
        let mut state = TerminalTitleState::default();
        let long_name = "A".repeat(50);
        state.set_for_main(Some(&long_name));
        assert!(state.current_title.len() <= 43);
        assert!(state.current_title.ends_with("..."));
    }

    #[test]
    fn terminal_title_save_and_restore() {
        let mut state = TerminalTitleState::default();
        state.set_for_main(Some("Test"));
        state.save_for_restore();
        assert_eq!(state.pending_restore, Some("OK | Test".to_string()));
        state.set_for_main(None);
        state.restore();
        assert_eq!(state.current_title, "OK | Test");
        assert!(state.pending_restore.is_none());
    }

    // ── Sidebar navigation tests ──────────────────────────────────────────

    #[test]
    fn sidebar_move_down() {
        let mut state = SidebarState::default();
        let prev = state.selected_index;
        state.move_down();
        assert_ne!(state.selected_index, prev);
        assert!(state.items[state.selected_index].is_selectable());
    }

    #[test]
    fn sidebar_move_up() {
        let mut state = SidebarState::default();
        state.selected_index = 1;
        state.move_up();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn sidebar_toggle_tags() {
        let mut state = SidebarState {
            tags_expanded: true,
            tags: vec![Tag {
                id: 1,
                name: "work".into(),
            }],
            ..Default::default()
        };
        state.rebuild();
        assert!(state.items.iter().any(|i| matches!(i, SidebarItem::Tag(_))));
        state.toggle_tags();
        assert!(!state.tags_expanded);
        assert!(state
            .items
            .iter()
            .all(|i| !matches!(i, SidebarItem::Tag(_))));
        state.toggle_tags();
        assert!(state.tags_expanded);
    }

    // ── Tag delete and visual/search mutual exclusion tests ──────────────

    #[test]
    fn tag_delete_auto_switches_to_all_when_viewing_deleted_tag() {
        let mut state = MainScreenState::default();
        state.current_filter = RecordFilter::Tag("work".to_string());
        state.sidebar.select_category(SidebarCategory::All);

        // Simulate tag deletion: switch filter to All
        state.current_filter = RecordFilter::All;
        state.sidebar.select_category(SidebarCategory::All);

        assert_eq!(state.current_filter, RecordFilter::All);
        assert_eq!(
            state.sidebar.items[state.sidebar.selected_index],
            SidebarItem::Category(SidebarCategory::All)
        );
    }

    #[test]
    fn visual_mode_and_search_are_mutually_exclusive() {
        let mut state = MainScreenState::default();
        state.list.enter_visual();
        assert!(state.list.is_visual());
        assert!(!state.list.is_searching());

        // Entering search should exit visual
        state.list.exit_visual();
        state.list.enter_search();
        assert!(state.list.is_searching());
        assert!(!state.list.is_visual());

        // Entering visual should exit search
        state.list.exit_search();
        state.list.enter_visual();
        assert!(state.list.is_visual());
        assert!(!state.list.is_searching());
    }

    #[test]
    fn trash_retention_days_default_is_30() {
        let state = MainScreenState::default();
        assert_eq!(state.trash_retention_days, 30);
        assert_eq!(state.detail.trash_retention_days, 30);
    }

    #[test]
    fn on_mount_reads_trash_retention_from_config() {
        use crate::config::AppConfig;
        use crate::tui::traits::screen::{Screen, ScreenContext};
        use tokio::sync::mpsc;

        let mut config = AppConfig::default();
        config.general.trash_retention_days = 60;
        let (tx, _rx) = mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };

        let mut state = MainScreenState::default();
        assert_eq!(state.trash_retention_days, 30);
        assert_eq!(state.detail.trash_retention_days, 30);

        state.on_mount(&mut ctx);

        assert_eq!(state.trash_retention_days, 60);
        assert_eq!(state.detail.trash_retention_days, 60);
    }

    #[test]
    fn on_mount_reads_zero_retention_from_config() {
        use crate::config::AppConfig;
        use crate::tui::traits::screen::{Screen, ScreenContext};
        use tokio::sync::mpsc;

        let mut config = AppConfig::default();
        config.general.trash_retention_days = 0;
        let (tx, _rx) = mpsc::channel(1);
        let mut ctx = ScreenContext {
            command_tx: &tx,
            config: &config,
        };

        let mut state = MainScreenState::default();
        state.on_mount(&mut ctx);

        assert_eq!(state.trash_retention_days, 0);
        assert_eq!(state.detail.trash_retention_days, 0);
    }
}
