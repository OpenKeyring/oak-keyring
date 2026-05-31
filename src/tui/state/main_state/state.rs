use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{layout::Rect, Frame};
use uuid::Uuid;

use crate::commands::types::{
    ConfirmVariant, FieldSelector, Overlay, PanelId, RecordFilter, RecordSort,
    Screen as ScreenEnum, SortDirection, SortField, DEFAULT_RECORD_LIST_PAGE_SIZE,
};
use crate::commands::{Command, Message};
use crate::config::PasswordDefaultsConfig;
use crate::t;
use crate::tui::screens::main::overlay::{ActiveOverlay, OverlayKeyResult, OverlayManager};
use crate::tui::screens::main::MainScreen;
use crate::tui::state::animation::EffectKind;
use crate::tui::state::detail_state::{
    DetailActionFocus, DetailActionKind, DetailFieldKind, DetailPanelState, FieldValue,
};
use crate::tui::state::list_state::{ListMode, ListPanelState};
use crate::tui::state::overlay_state::HistoryEntry;
use crate::tui::state::tag_management::{TagManagementState, TagSortOrder};
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::{CredentialType, Tag};

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

/// An item in the sidebar list — includes brand header, categories, visual
/// separators, tags, and utility links (generator, config).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidebarItem {
    /// Blank spacing row.
    Spacer,
    /// Brand header ("OpenKeyring") rendered at the top.
    Brand,
    /// A filterable category entry.
    Category(SidebarCategory),
    /// A visual separator line.
    Separator,
    /// A collapsible "Tags" section header.
    TagHeader,
    /// A single tag entry with its associated record count.
    Tag(String, usize), // (name, record_count)
    /// Password generator shortcut.
    Generator,
    /// Configuration screen shortcut.
    Config,
}

impl SidebarItem {
    /// Whether this item can receive keyboard focus / be selected.
    pub fn is_selectable(&self) -> bool {
        !matches!(self, Self::Spacer | Self::Brand | Self::Separator)
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
    /// Tag metadata for sorting (populated from TagsLoaded command result).
    pub tag_metadata: std::collections::HashMap<i64, crate::types::tag::TagSortMeta>,
    /// Record counts per category.
    pub category_counts: CategoryCounts,
}

impl Default for SidebarState {
    fn default() -> Self {
        let mut state = Self {
            items: Vec::new(),
            selected_index: 0,
            tags_expanded: true,
            tag_scroll_offset: 0,
            tag_management_mode: false,
            tag_management: TagManagementState::default(),
            tags: Vec::new(),
            tag_metadata: std::collections::HashMap::new(),
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
        self.tag_scroll_offset = self.tag_scroll_offset.min(self.footer_start_index());
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
    pub(crate) fn build_items(&self) -> Vec<SidebarItem> {
        let mut items: Vec<SidebarItem> = vec![
            SidebarItem::Spacer,
            SidebarItem::Brand,
            SidebarItem::Separator,
            SidebarItem::Category(SidebarCategory::All),
            SidebarItem::Separator,
            SidebarItem::Category(SidebarCategory::Favorites),
            SidebarItem::Separator,
            SidebarItem::Category(SidebarCategory::Expired),
            SidebarItem::Separator,
            SidebarItem::Category(SidebarCategory::HealthIssues),
            SidebarItem::Separator,
            SidebarItem::Category(SidebarCategory::Trash),
            SidebarItem::Separator,
            SidebarItem::TagHeader,
        ];

        if self.tags_expanded {
            for tag in &self.tags {
                let count = self
                    .tag_metadata
                    .get(&tag.id)
                    .map(|m| m.record_count)
                    .unwrap_or(0);
                items.push(SidebarItem::Separator);
                items.push(SidebarItem::Tag(tag.name.clone(), count));
            }
        }

        items.push(SidebarItem::Separator);
        items.push(SidebarItem::Generator);
        items.push(SidebarItem::Separator);
        items.push(SidebarItem::Config);

        items
    }

    /// Index where fixed footer shortcuts begin.
    pub fn footer_start_index(&self) -> usize {
        self.items
            .iter()
            .position(|item| matches!(item, SidebarItem::Generator))
            .and_then(|idx| idx.checked_sub(1))
            .unwrap_or(self.items.len())
    }

    pub fn tag_header_index(&self) -> Option<usize> {
        self.items
            .iter()
            .position(|item| matches!(item, SidebarItem::TagHeader))
    }

    pub fn tag_scroll_start_index(&self) -> usize {
        self.tag_header_index()
            .map(|index| index.saturating_add(1))
            .unwrap_or_else(|| self.footer_start_index())
            .min(self.footer_start_index())
    }

    pub fn tag_scroll_item_count(&self) -> usize {
        self.footer_start_index()
            .saturating_sub(self.tag_scroll_start_index())
    }

    pub fn fixed_top_height(&self) -> usize {
        let end = self.tag_scroll_start_index();
        (0..end)
            .map(|index| sidebar_item_render_height(self, index))
            .sum()
    }

    pub fn footer_render_height(&self) -> usize {
        let start = self.footer_start_index();
        (start..self.items.len())
            .map(|index| sidebar_item_render_height(self, index))
            .sum()
    }

    /// Number of rows/items available for the scrollable tag area.
    pub fn nav_visible_items_for_height(&self, sidebar_height: u16) -> usize {
        let footer_height = self.footer_render_height().min(sidebar_height as usize) as u16;
        let remaining = sidebar_height.saturating_sub(footer_height);
        let top_height = self.fixed_top_height().min(remaining as usize) as u16;
        remaining.saturating_sub(top_height).max(1) as usize
    }

    /// Maximum item offset for the scrollable tag area.
    pub fn max_tag_scroll_offset(&self, visible_items: usize) -> usize {
        self.tag_scroll_item_count()
            .saturating_sub(visible_items.max(1))
    }

    /// Scroll the expandable tag/navigation region by `delta` items.
    pub fn scroll_tags_by(&mut self, delta: isize, visible_items: usize) {
        let max_offset = self.max_tag_scroll_offset(visible_items);
        if delta < 0 {
            self.tag_scroll_offset = self.tag_scroll_offset.saturating_sub(delta.unsigned_abs());
        } else {
            self.tag_scroll_offset = self
                .tag_scroll_offset
                .saturating_add(delta as usize)
                .min(max_offset);
        }
    }

    /// Ensure the current sidebar selection is visible in the scrollable region.
    pub fn ensure_selected_visible(&mut self, visible_items: usize) {
        let scroll_start = self.tag_scroll_start_index();
        let scroll_end = self.footer_start_index();
        if self.selected_index < scroll_start || self.selected_index >= scroll_end {
            return;
        }

        let visible_items = visible_items.max(1);
        let relative_index = self.selected_index.saturating_sub(scroll_start);
        if relative_index < self.tag_scroll_offset {
            self.tag_scroll_offset = relative_index;
        } else if relative_index >= self.tag_scroll_offset.saturating_add(visible_items) {
            self.tag_scroll_offset = relative_index + 1 - visible_items;
        }
        self.tag_scroll_offset = self
            .tag_scroll_offset
            .min(self.max_tag_scroll_offset(visible_items));
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
            SidebarItem::Tag(name, _) => RecordFilter::Tag(name.clone()),
            // Generator and Config are shortcuts, not record filters
            SidebarItem::Generator
            | SidebarItem::Config
            | SidebarItem::Spacer
            | SidebarItem::Brand
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
        if matches!(
            self.items.get(self.selected_index),
            Some(SidebarItem::TagHeader)
        ) {
            self.select_first_tag();
        }
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

    /// Sort the tags vector according to the current sort order, then rebuild items.
    pub fn sort_tags_by_current_order(&mut self) {
        let sort_order = self.tag_management.sort_order;
        self.tags.sort_by(|a, b| {
            let meta_a = self.tag_metadata.get(&a.id);
            let meta_b = self.tag_metadata.get(&b.id);
            match sort_order {
                TagSortOrder::Frequency => {
                    let count_a = meta_a.map(|m| m.record_count).unwrap_or(0);
                    let count_b = meta_b.map(|m| m.record_count).unwrap_or(0);
                    count_b
                        .cmp(&count_a)
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                }
                TagSortOrder::Alphabetical => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                TagSortOrder::RecentlyUsed => {
                    let time_a = meta_a.map(|m| m.last_used_at).unwrap_or(0);
                    let time_b = meta_b.map(|m| m.last_used_at).unwrap_or(0);
                    match (time_a, time_b) {
                        (0, 0) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                        (0, _) => std::cmp::Ordering::Greater,
                        (_, 0) => std::cmp::Ordering::Less,
                        _ => time_b
                            .cmp(&time_a)
                            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                    }
                }
            }
        });
        self.rebuild();
    }

    /// Get the name of the currently selected tag item, if any.
    pub fn selected_tag_name(&self) -> Option<&str> {
        if self.selected_index < self.items.len() {
            if let SidebarItem::Tag(name, _) = &self.items[self.selected_index] {
                return Some(name);
            }
        }
        None
    }

    /// Select the tag section header, if present.
    pub fn select_tag_header(&mut self) {
        if let Some(index) = self
            .items
            .iter()
            .position(|item| matches!(item, SidebarItem::TagHeader))
        {
            self.selected_index = index;
        }
    }

    /// Select the first concrete tag item, if present.
    pub fn select_first_tag(&mut self) -> bool {
        if let Some(index) = self
            .items
            .iter()
            .position(|item| matches!(item, SidebarItem::Tag(_, _)))
        {
            self.selected_index = index;
            return true;
        }
        false
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
    Configured,
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

/// Phase of the health check lifecycle displayed in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HealthCheckPhase {
    #[default]
    Inactive,
    /// Health check is running: "检查中..."
    Checking,
    /// Health check found issues: "有需注意"
    NeedsAttention {
        weak: usize,
        compromised: usize,
        duplicate_groups: usize,
    },
    /// Health check passed: "全部安全"
    AllSecure,
    /// Leak detection was skipped: "跳过泄露检测"
    Skipped,
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
    /// Current phase of the health check display cycle.
    pub health_check_phase: HealthCheckPhase,
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
    /// Password generator defaults from config.
    pub password_defaults: PasswordDefaultsConfig,
    /// Overlay manager for modal dialogs (help, generator, confirm, etc.).
    pub overlay_manager: OverlayManager,
    /// Animation effect to trigger on the next update cycle.
    /// Set when overlay opens/closes, consumed by update.rs.
    pub pending_animation: Option<EffectKind>,
    /// Controls auto-selection on the next `RecordListLoaded`.
    /// When true, the handler auto-selects index 0 and sends `LoadRecordDetail`.
    /// Set to true by sidebar filter changes and record creation;
    /// reset to false after the flag is consumed.
    pub list_auto_select: bool,
    /// Pending record ID to refresh in detail after the next list reload.
    /// Set by `RecordUpdated` when the detail panel was showing the updated
    /// record; consumed and cleared by `RecordListLoaded`. If the record no
    /// longer appears in the reloaded list (e.g. filtered view), detail is
    /// cleared instead.
    pub pending_detail_refresh: Option<Uuid>,
    /// Remaining hidden fields to reveal after a multi-field show command.
    pub pending_reveal_fields: Vec<(Uuid, FieldSelector)>,
    /// Last known terminal area, updated during view(). Used by handle_mouse
    /// for hit-testing without calling crossterm::terminal::size() at event time.
    pub terminal_area: Rect,
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
            current_sort: RecordSort {
                field: SortField::CreatedAt,
                direction: SortDirection::Desc,
            },
            pre_lock_snapshot: None,
            focused_panel: PanelId::Sidebar,
            unicode_capable: true,
            trash_retention_days: 30,
            password_defaults: PasswordDefaultsConfig::default(),
            overlay_manager: OverlayManager::new(),
            pending_animation: None,
            list_auto_select: false,
            pending_detail_refresh: None,
            pending_reveal_fields: Vec::new(),
            terminal_area: Rect::new(0, 0, 100, 24),
        }
    }
}

impl MainScreenState {
    /// Create a new MainScreenState with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply client-side search filter: replace displayed records with
    /// filtered subset from the pre-search snapshot.
    /// Apply client-side search filter and return the currently selected record ID.
    /// Returns `None` when not in search mode, no snapshot, or no record selected.
    fn apply_search_filter_to_records(&mut self) -> Option<Uuid> {
        if let ListMode::Search(ref search_state) = self.list.mode {
            if let Some(ref snapshot) = search_state.pre_search {
                let filtered = self.list.apply_search_filter(snapshot.records.clone());
                let prev_selected_id = self
                    .list
                    .selected_index
                    .and_then(|idx| self.list.records.get(idx))
                    .map(|r| r.id);
                self.list.records = filtered;
                // Recover selection by record id
                match prev_selected_id {
                    Some(prev_id) => {
                        if let Some(idx) = self.list.records.iter().position(|r| r.id == prev_id) {
                            self.list.selected_index = Some(idx);
                        } else {
                            // Previously selected record filtered out — select first
                            self.list.selected_index = if self.list.records.is_empty() {
                                None
                            } else {
                                Some(0)
                            };
                        }
                    }
                    None => {
                        self.list.selected_index = if self.list.records.is_empty() {
                            None
                        } else {
                            Some(0)
                        };
                    }
                }
                self.list.adjust_scroll();
                return self.list.selected_record().map(|r| r.id);
            }
        }
        None
    }

    /// Sync render-only fields from AppState-level shared state.
    /// Called by the screen router before dispatching to update/view.
    pub fn sync_from_app(
        &mut self,
        focused_panel: PanelId,
        unicode_capable: bool,
        terminal_size: (u16, u16),
    ) {
        self.focused_panel = focused_panel;
        self.unicode_capable = unicode_capable;
        self.terminal_area = Rect::new(0, 0, terminal_size.0, terminal_size.1);
    }

    /// Capture reusable navigation state for this screen.
    pub fn to_restore_state(&self, focused_panel: PanelId) -> crate::tui::state::MainRestoreState {
        crate::tui::state::MainRestoreState {
            focused_panel,
            sidebar_selected_index: self.sidebar.selected_index,
            sidebar_tags_expanded: self.sidebar.tags_expanded,
            sidebar_tag_scroll_offset: self.sidebar.tag_scroll_offset,
            list_selected_index: self.list.selected_index,
            list_scroll_offset: self.list.scroll_offset,
            current_filter: self.current_filter.clone(),
            current_sort: self.current_sort.clone(),
            detail_focused_field: self.detail.focused_field,
        }
    }

    /// Restore navigation state from a previously captured restore state.
    pub fn restore_from(&mut self, restore: crate::tui::state::MainRestoreState) {
        self.sidebar.tags_expanded = restore.sidebar_tags_expanded;
        self.sidebar.tag_scroll_offset = restore.sidebar_tag_scroll_offset;
        self.sidebar.rebuild();
        self.sidebar.selected_index = restore
            .sidebar_selected_index
            .min(self.sidebar.items.len().saturating_sub(1));
        self.list.selected_index = restore.list_selected_index;
        self.list.scroll_offset = restore.list_scroll_offset;
        self.current_filter = restore.current_filter;
        self.current_sort = restore.current_sort;
        self.list.sort = self.current_sort.clone();
        self.detail.focused_field = restore.detail_focused_field;
        self.focused_panel = restore.focused_panel;
    }

    fn load_record_list_command(&mut self, offset: usize) -> Command {
        self.list.pending_load_offset = Some(offset);
        Command::LoadRecordList {
            filter: self.current_filter.clone(),
            sort: self.current_sort.clone(),
            limit: DEFAULT_RECORD_LIST_PAGE_SIZE,
            offset,
        }
    }

    fn reload_record_list_command(&mut self) -> Command {
        self.load_record_list_command(0)
    }

    fn maybe_load_more_records_command(&mut self) -> Option<Command> {
        if self.list.pending_load_offset.is_some()
            || self.list.records.len() >= self.list.total_count
            || self.list.records.is_empty()
        {
            return None;
        }

        let selected = self.list.selected_index?;
        let remaining_loaded = self.list.records.len().saturating_sub(selected + 1);
        if remaining_loaded <= self.list.visible_items_count().max(1) {
            return Some(self.load_record_list_command(self.list.records.len()));
        }

        None
    }
}

impl Screen for MainScreenState {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key),
            Message::MouseEvent(event) => self.handle_mouse(event, self.terminal_area),
            Message::Resize { width, height } => {
                self.terminal_area = Rect::new(0, 0, width, height);
                ScreenResult::Continue
            }
            Message::HealthCheckProgress { current, total } => {
                self.status_bar.health_check_progress = Some((current, total));
                ScreenResult::Continue
            }
            Message::CommandCompleted(result) => {
                use crate::commands::result::CommandResult;
                match result {
                    CommandResult::HealthCheckStarted => {
                        self.status_bar.health_check_progress = Some((0, 0));
                        self.status_bar.health_check_phase = HealthCheckPhase::Checking;
                        ScreenResult::Continue
                    }
                    CommandResult::HealthCheckCompleted { report } => {
                        self.status_bar.health_check_progress = None;
                        let has_issues = !report.compromised.is_empty()
                            || !report.weak_passwords.is_empty()
                            || !report.duplicate_passwords.is_empty();
                        if has_issues {
                            self.status_bar.health_check_phase = HealthCheckPhase::NeedsAttention {
                                weak: report.weak_passwords.len(),
                                compromised: report.compromised.len(),
                                duplicate_groups: report.duplicate_passwords.len(),
                            };
                        } else {
                            self.status_bar.health_check_phase = HealthCheckPhase::AllSecure;
                        }
                        // Refresh list to populate health fields and sidebar counts
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        // Refresh detail if a record is selected
                        if let Some(record) = self.list.selected_record() {
                            ctx.send_system_command(Command::LoadRecordDetail { id: record.id });
                        }
                        ScreenResult::Continue
                    }
                    CommandResult::HealthCheckSkipped => {
                        self.status_bar.health_check_progress = None;
                        self.status_bar.health_check_phase = HealthCheckPhase::Skipped;
                        ScreenResult::Continue
                    }
                    CommandResult::CopiedToClipboard {
                        field,
                        clear_after_seconds,
                    } => {
                        let field_name = match field {
                            FieldSelector::Password => t!("tui.field_selector.password"),
                            FieldSelector::Username => t!("tui.field_selector.username"),
                            FieldSelector::Url => t!("tui.field_selector.url"),
                            FieldSelector::Notes => t!("tui.field_selector.notes"),
                            FieldSelector::Passphrase => t!("tui.field_selector.passphrase"),
                        };
                        self.status_bar.status_message = Some(StatusMessage::ClipboardCountdown {
                            field: field_name.to_string(),
                            seconds: clear_after_seconds as u32,
                        });
                        self.status_bar.clipboard_countdown = Some(clear_after_seconds as u32);
                        ScreenResult::Continue
                    }
                    CommandResult::HistoryPasswordCopied {
                        clear_after_seconds,
                    } => {
                        self.status_bar.status_message = Some(StatusMessage::ClipboardCountdown {
                            field: t!("tui.field_selector.history_password").to_string(),
                            seconds: clear_after_seconds as u32,
                        });
                        self.status_bar.clipboard_countdown = Some(clear_after_seconds as u32);
                        ScreenResult::Continue
                    }
                    CommandResult::RecordCreated { .. } => {
                        // Auto-select the first record (newly created) when list reloads
                        self.list_auto_select = true;
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    CommandResult::RecordUpdated { id } => {
                        let was_showing_detail =
                            self.detail.record.as_ref().is_some_and(|r| r.id == id);
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        // Defer detail refresh until list reload completes.
                        // If the updated record no longer matches the current
                        // filter, the list reload will clear detail instead
                        // of sending a stale LoadRecordDetail.
                        if was_showing_detail {
                            self.pending_detail_refresh = Some(id);
                        }
                        ScreenResult::Continue
                    }
                    CommandResult::RecordListLoaded {
                        records,
                        total,
                        category_counts,
                    } => {
                        let loaded_offset = self.list.pending_load_offset.take().unwrap_or(0);
                        // Save previous selected record id for id-based recovery
                        let prev_selected_id = self
                            .list
                            .selected_index
                            .and_then(|idx| self.list.records.get(idx))
                            .map(|r| r.id);
                        self.list.total_count = total;
                        self.status_bar.record_count = total;
                        self.sidebar.category_counts = CategoryCounts {
                            all: category_counts.all,
                            favorites: category_counts.favorites,
                            expired: category_counts.expired,
                            health_issues: category_counts.health_issues,
                            trash: category_counts.trash,
                        };
                        self.sidebar.rebuild();

                        if loaded_offset > 0 {
                            let existing: std::collections::HashSet<_> =
                                self.list.records.iter().map(|record| record.id).collect();
                            self.list.records.extend(
                                records
                                    .into_iter()
                                    .filter(|record| !existing.contains(&record.id)),
                            );
                            return ScreenResult::Continue;
                        }

                        self.list.records = records;

                        if self.list_auto_select && !self.list.records.is_empty() {
                            // Auto-select first record (sidebar filter change or record creation)
                            self.list.selected_index = Some(0);
                            self.list.scroll_offset = 0;
                            self.list_auto_select = false;
                            let id = self.list.records[0].id;
                            ctx.send_system_command(Command::LoadRecordDetail { id });
                        } else if self.list.records.is_empty() {
                            self.list.selected_index = None;
                            self.list.scroll_offset = 0;
                            self.detail.clear();
                            self.list_auto_select = false;
                        } else {
                            // Record-id selection recovery
                            let recovered = match prev_selected_id {
                                Some(prev_id) => {
                                    // Try to find same record by id
                                    match self.list.records.iter().position(|r| r.id == prev_id) {
                                        Some(idx) => Some((idx, false)), // same id, no detail reload needed
                                        None => {
                                            // Id disappeared — fall back to first row
                                            Some((0, true))
                                        }
                                    }
                                }
                                None => {
                                    // No previous selection — keep None
                                    None
                                }
                            };
                            match recovered {
                                Some((idx, _needs_reload)) => {
                                    self.list.selected_index = Some(idx);
                                }
                                None => {
                                    self.list.selected_index = None;
                                }
                            }
                            self.list.adjust_scroll();
                            self.list_auto_select = false;

                            // If selection changed (different record id or cleared), reload detail
                            let new_selected_id = self
                                .list
                                .selected_index
                                .and_then(|idx| self.list.records.get(idx))
                                .map(|r| r.id);
                            if new_selected_id != prev_selected_id {
                                if let Some(id) = new_selected_id {
                                    let _ =
                                        ctx.command_tx.try_send(Command::LoadRecordDetail { id });
                                } else {
                                    self.detail.clear();
                                }
                            }
                        }

                        // Handle pending detail refresh from RecordUpdated.
                        // Only refresh if the record is still in the filtered list.
                        if let Some(refresh_id) = self.pending_detail_refresh.take() {
                            if self.list.records.iter().any(|r| r.id == refresh_id) {
                                let _ = ctx
                                    .command_tx
                                    .try_send(Command::LoadRecordDetail { id: refresh_id });
                            } else {
                                self.detail.clear();
                            }
                        }

                        ScreenResult::Continue
                    }
                    CommandResult::RecordDeleted { id } => {
                        // Clear detail if it shows the deleted record
                        if self.detail.record.as_ref().is_some_and(|r| r.id == id) {
                            self.detail.clear();
                        }
                        self.list.records.retain(|r| r.id != id);
                        self.list.cleanup_after_batch(&[id]);
                        // Reload to get accurate counts
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    CommandResult::RecordRestored { id } => {
                        // Clear detail if it shows the restored record
                        if self.detail.record.as_ref().is_some_and(|r| r.id == id) {
                            self.detail.clear();
                        }
                        self.list.records.retain(|r| r.id != id);
                        // Reload list after restore
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    CommandResult::RecordDestroyed { id } => {
                        // Clear detail immediately if it shows the destroyed record
                        if self.detail.record.as_ref().is_some_and(|r| r.id == id) {
                            self.detail.clear();
                        }
                        self.list.records.retain(|r| r.id != id);
                        // Reload list after permanent delete
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    CommandResult::FavoriteToggled { id, is_favorite } => {
                        // Update the list record
                        if let Some(record) = self.list.records.iter_mut().find(|r| r.id == id) {
                            record.is_favorite = is_favorite;
                        }
                        // Also update the detail record if currently displayed
                        if let Some(record) = self.detail.record.as_mut() {
                            if record.id == id {
                                record.is_favorite = is_favorite;
                            }
                        }
                        // When viewing Favorites, unfavorite should remove the row
                        if matches!(self.current_filter, RecordFilter::Favorites) && !is_favorite {
                            self.list.records.retain(|r| r.id != id);
                            self.list.selected_index = if self.list.records.is_empty() {
                                None
                            } else {
                                Some(
                                    self.list
                                        .selected_index
                                        .unwrap_or(0)
                                        .min(self.list.records.len() - 1),
                                )
                            };
                        }
                        ScreenResult::Continue
                    }
                    CommandResult::RecordDetailLoaded {
                        record,
                        password_strength,
                        health_issue,
                    } => {
                        self.pending_reveal_fields.clear();
                        let view_data =
                            DetailPanelState::build_from_record(&record, password_strength);
                        self.detail = DetailPanelState::with_record(view_data);
                        self.detail.health_issue = health_issue;
                        self.detail.is_trash = self.current_filter == RecordFilter::Trash;
                        // Reset password visibility on new detail load
                        self.detail.password_visible = false;
                        ScreenResult::Continue
                    }
                    // Handle FieldDecrypted — reveal field value
                    CommandResult::FieldDecrypted { id, field, value } => {
                        self.detail.password_visible = true;
                        if let Some(ref mut record) = self.detail.record {
                            if record.id == id {
                                // Map FieldSelector back to DetailFieldKind based on
                                // credential type, so the revealed value goes to the
                                // correct field (not just the first masked toggleable).
                                let target_kind = match field {
                                    FieldSelector::Username => match record.credential_type {
                                        CredentialType::Login | CredentialType::Ssh => {
                                            Some(DetailFieldKind::Username)
                                        }
                                        CredentialType::Api => Some(DetailFieldKind::AppId),
                                        CredentialType::SecureNote => None, // No username field
                                    },
                                    FieldSelector::Password => match record.credential_type {
                                        CredentialType::Login => Some(DetailFieldKind::Password),
                                        CredentialType::Api => Some(DetailFieldKind::SecretKey),
                                        CredentialType::Ssh => Some(DetailFieldKind::PrivateKey),
                                        CredentialType::SecureNote => None, // No password field
                                    },
                                    FieldSelector::Passphrase => Some(DetailFieldKind::Passphrase),
                                    // Url and Notes are not decryptable/toggleable
                                    FieldSelector::Url | FieldSelector::Notes => None,
                                };
                                if let Some(target_kind) = target_kind {
                                    for f in &mut record.fields {
                                        if f.kind == target_kind && f.toggleable {
                                            f.value =
                                                FieldValue::Revealed(value.expose().to_string());
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        if let Some(pos) = self
                            .pending_reveal_fields
                            .iter()
                            .position(|(pending_id, _)| *pending_id == id)
                        {
                            let (_, next_field) = self.pending_reveal_fields.remove(pos);
                            ctx.send_system_command(Command::DecryptField {
                                id,
                                field: next_field,
                            });
                        }
                        ScreenResult::Continue
                    }
                    // Handle PasswordHistoryLoaded — open overlay
                    CommandResult::PasswordHistoryLoaded { history } => {
                        let record_info =
                            self.detail.record.as_ref().map(|r| (r.id, r.name.clone()));
                        let entries: Vec<HistoryEntry> = history
                            .into_iter()
                            .map(|entry| {
                                // Discard SecureStr password — copying uses CopyHistoryPassword by id
                                let _ = entry.password;
                                HistoryEntry {
                                    id: entry.id,
                                    changed_at: entry.changed_at,
                                    description: crate::t!("tui.entry.password_label").to_string(),
                                }
                            })
                            .collect();
                        if let Some((record_id, record_name)) = record_info {
                            self.overlay_manager
                                .open(Overlay::PasswordHistory { record_id });
                            if let Some(ActiveOverlay::PasswordHistory(state)) =
                                self.overlay_manager.get_mut()
                            {
                                state.entries = entries;
                                state.record_name = record_name;
                            }
                        }
                        ScreenResult::Continue
                    }
                    // Handle TagsLoaded — populate sidebar tags, apply sort, rebuild
                    CommandResult::TagsLoaded { tags, tag_stats } => {
                        self.sidebar.tags = tags;
                        self.sidebar.tag_metadata = tag_stats;
                        self.sidebar.sort_tags_by_current_order();
                        ScreenResult::Continue
                    }
                    // Handle TagRenamed — reload tags and record list
                    CommandResult::TagRenamed { .. } => {
                        ctx.send_system_command(Command::LoadTags);
                        // Also reload record list in case tag filter is active
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    // Handle TagDeleted — reload tags and record list
                    CommandResult::TagDeleted { .. } => {
                        ctx.send_system_command(Command::LoadTags);
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    // Handle BatchTagAdded — reload tags and list, exit visual, clear selection
                    CommandResult::BatchTagAdded { .. } => {
                        let batch_tag_panel_open = self.batch_tag_panel_open();
                        if self.list.is_visual() && !batch_tag_panel_open {
                            self.list.exit_visual();
                        }
                        ctx.send_system_command(Command::LoadTags);
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        if !batch_tag_panel_open {
                            self.detail.clear();
                        }
                        ScreenResult::Continue
                    }
                    // Handle BatchTagRemoved — reload tags and list, exit visual, clear selection
                    CommandResult::BatchTagRemoved { .. } => {
                        let batch_tag_panel_open = self.batch_tag_panel_open();
                        if self.list.is_visual() && !batch_tag_panel_open {
                            self.list.exit_visual();
                        }
                        ctx.send_system_command(Command::LoadTags);
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        if !batch_tag_panel_open {
                            self.detail.clear();
                        }
                        ScreenResult::Continue
                    }
                    // Handle BatchDeleted — exit visual, reload list/tags/counts
                    CommandResult::BatchDeleted { count: _ } => {
                        let removed_ids = self.list.visual_selected_ids();
                        self.list.cleanup_after_batch(&removed_ids);
                        self.detail.clear();
                        ctx.send_system_command(Command::LoadTags);
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    // Handle BatchRestored — exit visual, reload list/tags/counts
                    CommandResult::BatchRestored { count: _ } => {
                        let removed_ids = self.list.visual_selected_ids();
                        self.list.cleanup_after_batch(&removed_ids);
                        self.detail.clear();
                        ctx.send_system_command(Command::LoadTags);
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    // Handle BatchDestroyed — exit visual, reload list
                    CommandResult::BatchDestroyed { count: _ } => {
                        let removed_ids = self.list.visual_selected_ids();
                        self.list.cleanup_after_batch(&removed_ids);
                        self.detail.clear();
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    // Handle TrashEmptied — clear list and detail, reload counts
                    CommandResult::TrashEmptied { count: _ } => {
                        self.list.records.clear();
                        self.list.selected_index = None;
                        self.list.scroll_offset = 0;
                        self.detail.clear();
                        let cmd = self.reload_record_list_command();
                        ctx.send_system_command(cmd);
                        ScreenResult::Continue
                    }
                    CommandResult::VaultLocked => {
                        // Security: clear all sensitive state on vault lock.
                        self.pending_reveal_fields.clear();
                        self.list.mode = ListMode::Normal;
                        self.list.records.clear();
                        self.list.selected_index = None;
                        self.list.scroll_offset = 0;
                        self.detail.clear();
                        self.overlay_manager.close();
                        self.status_bar.record_count = 0;
                        ScreenResult::NavigateTo(ScreenEnum::Unlock)
                    }
                    _ => ScreenResult::Continue,
                }
            }
            Message::ShowOverlay(overlay) => {
                if matches!(overlay, Overlay::PasswordGenerator) {
                    self.overlay_manager
                        .open_password_generator(&ctx.config.password);
                } else {
                    self.overlay_manager.open(overlay);
                }
                ScreenResult::Continue
            }
            Message::CloseOverlay => {
                self.overlay_manager.close();
                ScreenResult::Continue
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
        self.password_defaults = ctx.config.password.clone();
        self.detail.trash_retention_days = ctx.config.general.trash_retention_days;
        if !ctx.config.security.health_check_enabled {
            self.status_bar.health_check_phase = HealthCheckPhase::Skipped;
        }
        // Load initial record list
        let cmd = self.reload_record_list_command();
        ctx.send_system_command(cmd);
        // Load tags for sidebar
        ctx.send_system_command(Command::LoadTags);
    }

    fn on_unmount(&mut self) {
        // No-op for now.
    }
}

impl MainScreenState {
    fn batch_tag_panel_open(&self) -> bool {
        matches!(
            self.overlay_manager.get(),
            Some(ActiveOverlay::BatchTagPanel(_))
        )
    }

    fn handle_key(&mut self, key: KeyEvent) -> ScreenResult {
        // Layer 1: Overlay gets priority
        if self.overlay_manager.is_active() {
            let result = self.overlay_manager.handle_key(key.code);
            return self.handle_overlay_result(result);
        }

        // Layer 1.5: Search mode captures all keys before global shortcuts
        if self.focused_panel == PanelId::List && self.list.is_searching() {
            match key.code {
                KeyCode::Enter => {
                    // Commit search: keep filtered results, save snapshot for Esc restore
                    let id = self.list.selected_record().map(|r| r.id);
                    self.list.commit_search();
                    if let Some(id) = id {
                        return ScreenResult::Command(Box::new(Command::LoadRecordDetail { id }));
                    }
                    return ScreenResult::Continue;
                }
                KeyCode::Esc => {
                    // Cancel search: restore pre-search snapshot and reload detail
                    if let Some(id) = self.list.cancel_search_restore() {
                        return ScreenResult::Command(Box::new(Command::LoadRecordDetail { id }));
                    }
                    return ScreenResult::Continue;
                }
                KeyCode::Backspace => {
                    if let ListMode::Search(ref s) = self.list.mode {
                        let mut new_query = s.query.clone();
                        new_query.pop();
                        self.list.update_search_query(new_query);
                        if let Some(id) = self.apply_search_filter_to_records() {
                            return ScreenResult::Command(Box::new(Command::LoadRecordDetail {
                                id,
                            }));
                        }
                        self.detail.clear();
                    }
                    return ScreenResult::Continue;
                }
                KeyCode::Char(c) => {
                    // Ignore Ctrl/Alt combinations — let them fall through to global shortcuts
                    if key.modifiers.intersects(
                        KeyModifiers::CONTROL
                            | KeyModifiers::ALT
                            | KeyModifiers::SUPER
                            | KeyModifiers::META,
                    ) {
                        return ScreenResult::Continue;
                    }
                    if let ListMode::Search(ref s) = self.list.mode {
                        let new_query = format!("{}{}", s.query, c);
                        self.list.update_search_query(new_query);
                        if let Some(id) = self.apply_search_filter_to_records() {
                            return ScreenResult::Command(Box::new(Command::LoadRecordDetail {
                                id,
                            }));
                        }
                        self.detail.clear();
                    }
                    return ScreenResult::Continue;
                }
                KeyCode::Down => {
                    self.list.move_down();
                    if let Some(record) = self.list.selected_record() {
                        return ScreenResult::Command(Box::new(Command::LoadRecordDetail {
                            id: record.id,
                        }));
                    }
                    self.detail.clear();
                    return ScreenResult::Continue;
                }
                KeyCode::Up => {
                    self.list.move_up();
                    if let Some(record) = self.list.selected_record() {
                        return ScreenResult::Command(Box::new(Command::LoadRecordDetail {
                            id: record.id,
                        }));
                    }
                    self.detail.clear();
                    return ScreenResult::Continue;
                }
                _ => return ScreenResult::Continue, // consume all other keys
            }
        }

        if self.is_global_search_shortcut(key) {
            self.focused_panel = PanelId::List;
            self.list.enter_search();
            return ScreenResult::Continue;
        }

        // Check sidebar Enter for Generator/Config navigation
        if self.focused_panel == PanelId::Sidebar && key.code == KeyCode::Enter {
            match self.sidebar.items.get(self.sidebar.selected_index) {
                Some(SidebarItem::Generator) => {
                    self.overlay_manager
                        .open_password_generator(&self.password_defaults);
                    self.pending_animation = Some(EffectKind::ModalAppear);
                    return ScreenResult::Continue;
                }
                Some(SidebarItem::Config) => {
                    return ScreenResult::NavigateTo(ScreenEnum::Config);
                }
                _ => {}
            }
        }

        if let Some(result) = self.handle_global_navigation_key(key) {
            return result;
        }

        // Sidebar j/k — filter change triggers LoadRecordList
        if self.focused_panel == PanelId::Sidebar {
            let is_renaming =
                self.sidebar.is_tag_management() && self.sidebar.tag_management.is_renaming();
            if !is_renaming {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        let old_filter = self.sidebar.current_filter();
                        self.sidebar.move_down();
                        self.ensure_sidebar_selection_visible();
                        // Exit visual mode on sidebar navigation (U3 Spec)
                        if self.list.is_visual() {
                            self.list.exit_visual();
                        }
                        let new_filter = self.sidebar.current_filter();
                        if new_filter != old_filter {
                            self.current_filter = new_filter.clone();
                            self.detail.clear();
                            self.list_auto_select = true;
                            let cmd = self.reload_record_list_command();
                            return ScreenResult::Command(Box::new(cmd));
                        }
                        return ScreenResult::Continue;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        let old_filter = self.sidebar.current_filter();
                        self.sidebar.move_up();
                        self.ensure_sidebar_selection_visible();
                        if self.list.is_visual() {
                            self.list.exit_visual();
                        }
                        let new_filter = self.sidebar.current_filter();
                        if new_filter != old_filter {
                            self.current_filter = new_filter.clone();
                            self.detail.clear();
                            self.list_auto_select = true;
                            let cmd = self.reload_record_list_command();
                            return ScreenResult::Command(Box::new(cmd));
                        }
                        return ScreenResult::Continue;
                    }
                    _ => {}
                }
            }
        }

        // ── Task 6: List normal mode j/k navigation with detail loading ──
        if self.focused_panel == PanelId::List
            && !self.list.is_searching()
            && !self.list.is_visual()
        {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.list.move_down();
                    if let Some(cmd) = self.maybe_load_more_records_command() {
                        return ScreenResult::Command(Box::new(cmd));
                    }
                    if let Some(record) = self.list.selected_record() {
                        return ScreenResult::Command(Box::new(Command::LoadRecordDetail {
                            id: record.id,
                        }));
                    }
                    return ScreenResult::Continue;
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.list.move_up();
                    if let Some(record) = self.list.selected_record() {
                        return ScreenResult::Command(Box::new(Command::LoadRecordDetail {
                            id: record.id,
                        }));
                    }
                    return ScreenResult::Continue;
                }
                _ => {}
            }
        }

        // ── Task 7: Detail panel keyboard shortcuts ──
        if self.focused_panel == PanelId::Detail {
            if let Some(ref record) = self.detail.record {
                let id = record.id;
                let is_favorite = record.is_favorite;
                let record_name = record.name.clone();

                match key.code {
                    KeyCode::Enter if self.detail.focused_action.is_some() => {
                        let action = self.detail.focused_action.expect("checked above");
                        return self.execute_detail_action(id, action);
                    }
                    // Step 1: p — toggle password visibility (context-sensitive)
                    KeyCode::Char('p') => {
                        let reveal_fields = reveal_field_selectors(&self.detail);
                        if reveal_fields.is_empty() {
                            return ScreenResult::Continue;
                        }
                        let needs_decrypt = self.detail.toggle_password();
                        if needs_decrypt {
                            if let Some((&selector, remaining)) = reveal_fields.split_first() {
                                self.pending_reveal_fields =
                                    remaining.iter().map(|field| (id, *field)).collect();
                                return ScreenResult::Command(Box::new(Command::DecryptField {
                                    id,
                                    field: selector,
                                }));
                            }
                        } else {
                            self.pending_reveal_fields.clear();
                        }
                        return ScreenResult::Continue;
                    }
                    // Step 2a: c — copy password field
                    KeyCode::Char('c') => {
                        if let Some(field) = self.detail.password_field() {
                            let selector = detail_field_kind_to_selector(field.kind);
                            return ScreenResult::Command(Box::new(Command::CopyToClipboard {
                                id,
                                field: selector,
                            }));
                        }
                        return ScreenResult::Continue;
                    }
                    // Step 2b: u — copy username field
                    KeyCode::Char('u') => {
                        if let Some(field) = self.detail.username_field() {
                            let selector = detail_field_kind_to_selector(field.kind);
                            return ScreenResult::Command(Box::new(Command::CopyToClipboard {
                                id,
                                field: selector,
                            }));
                        }
                        return ScreenResult::Continue;
                    }
                    // Step 2c: Enter — copy currently focused field
                    KeyCode::Enter => {
                        if let Some(field) = self.detail.current_field() {
                            let selector = detail_field_kind_to_selector(field.kind);
                            return ScreenResult::Command(Box::new(Command::CopyToClipboard {
                                id,
                                field: selector,
                            }));
                        }
                        return ScreenResult::Continue;
                    }
                    // Step 3: f — toggle favorite
                    KeyCode::Char('f') => {
                        return ScreenResult::Command(Box::new(Command::ToggleFavorite {
                            id,
                            is_favorite: !is_favorite,
                        }));
                    }
                    // Step 4: d — delete with confirm
                    KeyCode::Char('d') => {
                        self.overlay_manager.open(Overlay::ConfirmDialog(
                            crate::commands::types::ConfirmDialogState {
                                variant: ConfirmVariant::SoftDelete {
                                    record_id: id,
                                    record_name,
                                    auto_delete_days: None,
                                },
                                focused_button: crate::commands::types::ConfirmButton::Cancel,
                            },
                        ));
                        self.pending_animation = Some(EffectKind::ModalAppear);
                        return ScreenResult::Continue;
                    }
                    // Step 5: H — password history
                    KeyCode::Char('H') => {
                        return ScreenResult::Command(Box::new(Command::LoadPasswordHistory {
                            record_id: id,
                        }));
                    }
                    // Step 6: field navigation (up/k, down/j)
                    KeyCode::Char('k') | KeyCode::Up => {
                        if self.detail.focused_action.is_some() {
                            self.detail.move_action_up();
                            return ScreenResult::Continue;
                        }
                        self.detail.move_field_up();
                        return ScreenResult::Continue;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if self.detail.focused_action.is_some() {
                            self.detail.move_action_down();
                            return ScreenResult::Continue;
                        }
                        self.detail.move_field_down();
                        return ScreenResult::Continue;
                    }
                    _ => {}
                }
            }
        }

        // Layer 2: Global shortcuts
        match key.code {
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.overlay_manager
                    .open_password_generator(&self.password_defaults);
                self.pending_animation = Some(EffectKind::ModalAppear);
                ScreenResult::Continue
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ScreenResult::NavigateTo(ScreenEnum::Config)
            }
            KeyCode::Char('g') => ScreenResult::NavigateTo(ScreenEnum::Config),
            KeyCode::Char('l') => ScreenResult::NavigateTo(ScreenEnum::AuditLog),
            KeyCode::F(1) => {
                self.overlay_manager.open(Overlay::Help);
                self.pending_animation = Some(EffectKind::ModalAppear);
                ScreenResult::Continue
            }
            KeyCode::Char('q') if key.modifiers.is_empty() => {
                self.overlay_manager.open(Overlay::ConfirmDialog(
                    crate::commands::types::ConfirmDialogState {
                        variant: ConfirmVariant::QuitApp,
                        focused_button: crate::commands::types::ConfirmButton::Cancel,
                    },
                ));
                self.pending_animation = Some(EffectKind::ModalAppear);
                ScreenResult::Continue
            }
            KeyCode::Char('?') => {
                self.overlay_manager.open(Overlay::Help);
                self.pending_animation = Some(EffectKind::ModalAppear);
                ScreenResult::Continue
            }
            KeyCode::Char('p') => {
                self.overlay_manager
                    .open_password_generator(&self.password_defaults);
                self.pending_animation = Some(EffectKind::ModalAppear);
                ScreenResult::Continue
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ScreenResult::Command(Box::new(Command::TriggerSync))
            }
            KeyCode::Char('n') if self.current_filter != RecordFilter::Trash => {
                ScreenResult::NavigateTo(ScreenEnum::CreateRecord)
            }
            KeyCode::Char('e') if self.current_filter != RecordFilter::Trash => {
                match self.focused_panel {
                    PanelId::Detail => {
                        if let Some(record) = self.detail.record.as_ref() {
                            return ScreenResult::NavigateTo(ScreenEnum::EditRecord {
                                id: record.id,
                            });
                        }
                    }
                    PanelId::List => {
                        if let Some(record) = self.list.selected_record() {
                            return ScreenResult::NavigateTo(ScreenEnum::EditRecord {
                                id: record.id,
                            });
                        }
                    }
                    _ => {}
                }
                ScreenResult::Continue
            }
            // Layer 3: Panel routing
            _ => {
                let screen = MainScreen::new();
                let result = screen.handle_key_event(key, self, self.focused_panel);
                for msg in result.messages {
                    // Messages from panel routing are UI state updates;
                    // they don't need Command dispatch in this context.
                    let _ = msg;
                }
                if let Some(overlay) = result.overlay {
                    self.overlay_manager.open(overlay);
                    self.pending_animation = Some(EffectKind::ModalAppear);
                }
                if let Some(cmd) = result.command {
                    return ScreenResult::Command(cmd);
                }
                if let Some(panel) = result.focused_panel {
                    self.focused_panel = panel;
                }
                ScreenResult::Continue
            }
        }
    }

    fn is_global_search_shortcut(&self, key: KeyEvent) -> bool {
        is_search_shortcut(key) && !self.list.is_searching() && !self.list.is_visual()
    }

    fn handle_global_navigation_key(&mut self, key: KeyEvent) -> Option<ScreenResult> {
        let sidebar_is_renaming =
            self.sidebar.is_tag_management() && self.sidebar.tag_management.is_renaming();
        if sidebar_is_renaming {
            return None;
        }

        match key.code {
            KeyCode::Left if key.modifiers.is_empty() => {
                if self.focused_panel == PanelId::Detail && self.detail.focused_action.is_some() {
                    self.detail.move_action_left();
                    return Some(ScreenResult::Continue);
                }
                self.focused_panel = match self.focused_panel {
                    PanelId::Sidebar => PanelId::Sidebar,
                    PanelId::List => PanelId::Sidebar,
                    PanelId::Detail => PanelId::List,
                };
                Some(ScreenResult::Continue)
            }
            KeyCode::Right if key.modifiers.is_empty() => {
                if self.focused_panel == PanelId::Detail {
                    if self.detail.focused_action.is_some() {
                        self.detail.move_action_right();
                    } else {
                        self.detail.focus_first_action();
                    }
                    return Some(ScreenResult::Continue);
                }
                if self.focused_panel == PanelId::Sidebar {
                    self.focused_panel = PanelId::List;
                    if self.list.selected_index.is_none() && !self.list.records.is_empty() {
                        self.list.selected_index = Some(0);
                        self.list.adjust_scroll();
                        if let Some(record) = self.list.selected_record() {
                            return Some(ScreenResult::Command(Box::new(
                                Command::LoadRecordDetail { id: record.id },
                            )));
                        }
                    }
                    return Some(ScreenResult::Continue);
                }
                self.focused_panel = match self.focused_panel {
                    PanelId::Sidebar => PanelId::List,
                    PanelId::List => PanelId::Detail,
                    PanelId::Detail => PanelId::Detail,
                };
                Some(ScreenResult::Continue)
            }
            KeyCode::Char(ch) if key.modifiers.is_empty() => {
                if ch == '0' {
                    self.focused_panel = PanelId::Sidebar;
                    self.sidebar.select_tag_header();
                    return Some(ScreenResult::Continue);
                }
                if ch == '6' {
                    self.overlay_manager
                        .open_password_generator(&self.password_defaults);
                    self.pending_animation = Some(EffectKind::ModalAppear);
                    return Some(ScreenResult::Continue);
                }
                if ch == '7' {
                    return Some(ScreenResult::NavigateTo(ScreenEnum::Config));
                }
                let category = match ch {
                    '1' => Some(SidebarCategory::All),
                    '2' => Some(SidebarCategory::Favorites),
                    '3' => Some(SidebarCategory::Expired),
                    '4' => Some(SidebarCategory::HealthIssues),
                    '5' => Some(SidebarCategory::Trash),
                    _ => None,
                }?;
                self.focused_panel = PanelId::Sidebar;
                self.sidebar.select_category(category);
                let filter = category.to_filter();
                if filter != self.current_filter {
                    self.current_filter = filter.clone();
                    self.detail.clear();
                    self.list_auto_select = true;
                    let cmd = self.reload_record_list_command();
                    Some(ScreenResult::Command(Box::new(cmd)))
                } else {
                    Some(ScreenResult::Continue)
                }
            }
            _ => None,
        }
    }

    fn handle_mouse(&mut self, event: MouseEvent, terminal_area: Rect) -> ScreenResult {
        let is_hover = matches!(event.kind, MouseEventKind::Moved);
        let is_click = matches!(event.kind, MouseEventKind::Down(MouseButton::Left));
        let is_scroll_up = matches!(event.kind, MouseEventKind::ScrollUp);
        let is_scroll_down = matches!(event.kind, MouseEventKind::ScrollDown);

        if !is_hover && !is_click && !is_scroll_up && !is_scroll_down {
            return ScreenResult::Continue;
        }

        // Handle scroll events — move selection so ratatui List follows
        if is_scroll_up || is_scroll_down {
            let layout = crate::tui::screens::main::layout::calculate_layout(
                terminal_area,
                terminal_area.width,
            );
            let list_rect = top_padded_rect(layout.list, 1);
            let detail_rect = top_padded_rect(layout.detail, 1);

            if contains_rect(layout.sidebar, event.column, event.row) {
                if self.sidebar.tags_expanded
                    && !self.sidebar.tags.is_empty()
                    && sidebar_tag_scroll_region_contains(&self.sidebar, layout.sidebar, event.row)
                {
                    self.focused_panel = PanelId::Sidebar;
                    let visible = self
                        .sidebar
                        .nav_visible_items_for_height(layout.sidebar.height);
                    let delta = if is_scroll_up { -3 } else { 3 };
                    self.sidebar.scroll_tags_by(delta, visible);
                }
            } else if contains_rect(list_rect, event.column, event.row) {
                self.focused_panel = PanelId::List;
                let steps = 3;
                for _ in 0..steps {
                    if is_scroll_up {
                        self.list.move_up();
                    } else {
                        self.list.move_down();
                    }
                }
                if is_scroll_down {
                    if let Some(cmd) = self.maybe_load_more_records_command() {
                        return ScreenResult::Command(Box::new(cmd));
                    }
                }
                if let Some(record) = self.list.selected_record() {
                    let id = record.id;
                    return ScreenResult::Command(Box::new(Command::LoadRecordDetail { id }));
                }
            } else if contains_rect(detail_rect, event.column, event.row) {
                self.focused_panel = PanelId::Detail;
            }
            return ScreenResult::Continue;
        }

        let layout =
            crate::tui::screens::main::layout::calculate_layout(terminal_area, terminal_area.width);
        let list_rect = top_padded_rect(layout.list, 1);
        let detail_rect = top_padded_rect(layout.detail, 1);

        if contains_rect(layout.sidebar, event.column, event.row) {
            self.focused_panel = PanelId::Sidebar;
            if is_click {
                if let Some(index) = sidebar_item_index_at(&self.sidebar, layout.sidebar, event.row)
                {
                    if self
                        .sidebar
                        .items
                        .get(index)
                        .is_some_and(SidebarItem::is_selectable)
                    {
                        let old_filter = self.sidebar.current_filter();
                        self.sidebar.selected_index = index;
                        self.ensure_sidebar_selection_visible();
                        if self.list.is_visual() {
                            self.list.exit_visual();
                        }
                        let new_filter = self.sidebar.current_filter();
                        if new_filter != old_filter {
                            self.current_filter = new_filter.clone();
                            self.detail.clear();
                            self.list_auto_select = true;
                            let cmd = self.reload_record_list_command();
                            return ScreenResult::Command(Box::new(cmd));
                        }
                    }
                }
            }
            return ScreenResult::Continue;
        }

        if contains_rect(list_rect, event.column, event.row) {
            self.focused_panel = PanelId::List;
            if is_hover {
                return ScreenResult::Continue;
            }
            let row_in_list = event.row.saturating_sub(list_rect.y);
            if row_in_list == 0 {
                if is_click && !self.list.is_searching() && !self.list.is_visual() {
                    let col_in_list = event.column.saturating_sub(list_rect.x);
                    if col_in_list < list_rect.width / 2 {
                        self.list.cycle_sort_field();
                        self.current_sort.field = self.list.sort.field;
                    } else {
                        self.list.toggle_sort_direction();
                        self.current_sort.direction = self.list.sort.direction;
                    }
                    let cmd = self.reload_record_list_command();
                    return ScreenResult::Command(Box::new(cmd));
                }
                return ScreenResult::Continue;
            }
            if row_in_list == 1 {
                return ScreenResult::Continue;
            }

            let item_height = if crate::tui::terminal::WidthTier::from_width(list_rect.width)
                == crate::tui::terminal::WidthTier::Minimum
            {
                2
            } else {
                3
            };
            let index = ((row_in_list - 2) / item_height) as usize
                + rendered_list_offset(&self.list, list_rect);
            if index < self.list.records.len() && self.list.selected_index != Some(index) {
                self.list.selected_index = Some(index);
                self.list.adjust_scroll();
                let id = self.list.records[index].id;
                return ScreenResult::Command(Box::new(Command::LoadRecordDetail { id }));
            }
            return ScreenResult::Continue;
        }

        if contains_rect(detail_rect, event.column, event.row) {
            self.focused_panel = PanelId::Detail;
            if let Some(action) = crate::tui::screens::main::detail::detail_action_at(
                detail_rect,
                &self.detail,
                event.column,
                event.row,
            ) {
                self.detail.set_action_focus(action);
                if is_click {
                    if let Some(record) = self.detail.record.as_ref() {
                        return self.execute_detail_action(record.id, action);
                    }
                }
            }
        }
        ScreenResult::Continue
    }

    fn ensure_sidebar_selection_visible(&mut self) {
        let layout = crate::tui::screens::main::layout::calculate_layout(
            self.terminal_area,
            self.terminal_area.width,
        );
        let visible = self
            .sidebar
            .nav_visible_items_for_height(layout.sidebar.height);
        self.sidebar.ensure_selected_visible(visible);
    }

    fn execute_detail_action(&mut self, id: Uuid, action: DetailActionFocus) -> ScreenResult {
        let Some(field) = self
            .detail
            .record
            .as_ref()
            .and_then(|record| record.fields.get(action.field_index))
        else {
            return ScreenResult::Continue;
        };
        let selector = detail_field_kind_to_selector(field.kind);
        match action.kind {
            DetailActionKind::Copy => ScreenResult::Command(Box::new(Command::CopyToClipboard {
                id,
                field: selector,
            })),
            DetailActionKind::ToggleSecret => {
                if !field.toggleable {
                    return ScreenResult::Continue;
                }
                let needs_decrypt = self.detail.toggle_password();
                if needs_decrypt {
                    ScreenResult::Command(Box::new(Command::DecryptField {
                        id,
                        field: selector,
                    }))
                } else {
                    ScreenResult::Continue
                }
            }
        }
    }

    fn handle_overlay_result(&mut self, result: OverlayKeyResult) -> ScreenResult {
        match result {
            OverlayKeyResult::Consumed => ScreenResult::Continue,
            OverlayKeyResult::None => ScreenResult::Continue,
            OverlayKeyResult::Close { .. } => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                ScreenResult::Continue
            }
            OverlayKeyResult::CopyGeneratedPassword { password } => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                ScreenResult::Command(Box::new(Command::CopyRawToClipboard { value: password }))
            }
            OverlayKeyResult::CopyHistoryPassword { history_id } => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                ScreenResult::Command(Box::new(Command::CopyHistoryPassword { history_id }))
            }
            OverlayKeyResult::ConfirmAction { variant } => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                match variant {
                    ConfirmVariant::SoftDelete { record_id, .. } => {
                        ScreenResult::Command(Box::new(Command::SoftDeleteRecord { id: record_id }))
                    }
                    ConfirmVariant::Restore { record_id, .. } => {
                        ScreenResult::Command(Box::new(Command::RestoreRecord { id: record_id }))
                    }
                    ConfirmVariant::HardDelete { record_id, .. } => {
                        ScreenResult::Command(Box::new(Command::HardDeleteRecord { id: record_id }))
                    }
                    ConfirmVariant::BatchSoftDelete { record_ids, .. } => {
                        ScreenResult::Command(Box::new(Command::BatchSoftDelete { record_ids }))
                    }
                    ConfirmVariant::BatchRestore { record_ids, .. } => {
                        ScreenResult::Command(Box::new(Command::BatchRestore { record_ids }))
                    }
                    ConfirmVariant::BatchHardDelete { record_ids, .. } => {
                        ScreenResult::Command(Box::new(Command::BatchHardDelete { record_ids }))
                    }
                    ConfirmVariant::EmptyTrash { .. } => {
                        ScreenResult::Command(Box::new(Command::EmptyTrash))
                    }
                    ConfirmVariant::TagDelete { tag_name, .. } => {
                        ScreenResult::Command(Box::new(Command::DeleteTag { name: tag_name }))
                    }
                    ConfirmVariant::QuitApp => ScreenResult::ExitApp,
                }
            }
            OverlayKeyResult::BatchAddTag {
                record_ids,
                tag_name,
            } => ScreenResult::Command(Box::new(Command::BatchAddTag {
                record_ids,
                tag_name,
            })),
            OverlayKeyResult::BatchRemoveTag {
                record_ids,
                tag_name,
            } => ScreenResult::Command(Box::new(Command::BatchRemoveTag {
                record_ids,
                tag_name,
            })),
            OverlayKeyResult::ErrorRetry => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                ScreenResult::Continue
            }
            OverlayKeyResult::ErrorQuit => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                ScreenResult::Continue
            }
        }
    }
}

fn is_search_shortcut(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('k')
        && key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META)
}

fn contains_rect(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x.saturating_add(rect.width)
        && row >= rect.y
        && row < rect.y.saturating_add(rect.height)
}

fn top_padded_rect(area: Rect, padding: u16) -> Rect {
    let applied = padding.min(area.height);
    Rect::new(
        area.x,
        area.y + applied,
        area.width,
        area.height.saturating_sub(applied),
    )
}

fn reveal_field_selectors(detail: &DetailPanelState) -> Vec<FieldSelector> {
    let Some(record) = detail.record.as_ref() else {
        return Vec::new();
    };
    let mut ordered: Vec<(usize, FieldSelector)> = record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.toggleable)
        .map(|(index, field)| (index, detail_field_kind_to_selector(field.kind)))
        .collect();

    if let Some(current_pos) = ordered
        .iter()
        .position(|(index, _)| *index == detail.focused_field)
    {
        ordered.rotate_left(current_pos);
    }

    ordered.into_iter().map(|(_, selector)| selector).collect()
}

fn rendered_list_offset(list: &ListPanelState, list_rect: Rect) -> usize {
    let item_height = if crate::tui::terminal::WidthTier::from_width(list_rect.width)
        == crate::tui::terminal::WidthTier::Minimum
    {
        2
    } else {
        3
    };
    let body_height = list_rect.height.saturating_sub(2);
    let visible = (body_height / item_height).max(1) as usize;
    list.scroll_offset
        .min(list.records.len().saturating_sub(visible))
}

fn sidebar_item_index_at(sidebar: &SidebarState, sidebar_rect: Rect, row: u16) -> Option<usize> {
    if row < sidebar_rect.y || row >= sidebar_rect.y.saturating_add(sidebar_rect.height) {
        return None;
    }

    let footer_start = sidebar.footer_start_index();
    let footer_height = sidebar
        .footer_render_height()
        .min(sidebar_rect.height as usize) as u16;
    let footer_top = sidebar_rect
        .y
        .saturating_add(sidebar_rect.height.saturating_sub(footer_height));

    if row >= footer_top {
        let relative_row = row.saturating_sub(footer_top) as usize;
        return sidebar_item_index_in_range(
            sidebar,
            footer_start,
            sidebar.items.len(),
            relative_row,
        );
    }

    let scroll_start = sidebar.tag_scroll_start_index();
    let top_height = sidebar
        .fixed_top_height()
        .min(footer_top.saturating_sub(sidebar_rect.y) as usize) as u16;
    let top_bottom = sidebar_rect.y.saturating_add(top_height);

    if row < top_bottom {
        let relative_row = row.saturating_sub(sidebar_rect.y) as usize;
        return sidebar_item_index_in_range(sidebar, 0, scroll_start, relative_row);
    }

    let tag_area_height = footer_top.saturating_sub(top_bottom);
    if tag_area_height == 0 {
        return None;
    }

    let visible_items = tag_area_height.max(1) as usize;
    let offset = sidebar
        .tag_scroll_offset
        .min(sidebar.max_tag_scroll_offset(visible_items));
    let relative_row = row.saturating_sub(top_bottom) as usize;
    sidebar_item_index_in_range(
        sidebar,
        scroll_start.saturating_add(offset),
        footer_start,
        relative_row,
    )
}

fn sidebar_tag_scroll_region_contains(
    sidebar: &SidebarState,
    sidebar_rect: Rect,
    row: u16,
) -> bool {
    let Some(index) = sidebar_item_index_at(sidebar, sidebar_rect, row) else {
        return false;
    };
    index >= sidebar.tag_scroll_start_index() && index < sidebar.footer_start_index()
}

fn sidebar_item_index_in_range(
    sidebar: &SidebarState,
    start: usize,
    end: usize,
    relative_row: usize,
) -> Option<usize> {
    let mut y = 0usize;
    for index in start..end {
        let height = sidebar_item_render_height(sidebar, index);
        if relative_row < y.saturating_add(height) {
            return Some(index);
        }
        y = y.saturating_add(height);
    }
    None
}

fn sidebar_item_render_height(sidebar: &SidebarState, index: usize) -> usize {
    let Some(item) = sidebar.items.get(index) else {
        return 1;
    };
    if sidebar.selected_index == index && item.is_selectable() {
        match item {
            SidebarItem::Generator | SidebarItem::Config => 1,
            _ => 3,
        }
    } else {
        1
    }
}

/// Convert a [`DetailFieldKind`] to the corresponding [`FieldSelector`] for
/// clipboard copy and field decryption commands.
pub(super) fn detail_field_kind_to_selector(kind: DetailFieldKind) -> FieldSelector {
    match kind {
        DetailFieldKind::Username | DetailFieldKind::AppId | DetailFieldKind::PublicKey => {
            FieldSelector::Username
        }
        DetailFieldKind::Password | DetailFieldKind::SecretKey | DetailFieldKind::PrivateKey => {
            FieldSelector::Password
        }
        DetailFieldKind::Passphrase => FieldSelector::Passphrase,
        DetailFieldKind::Url => FieldSelector::Url,
        DetailFieldKind::Notes => FieldSelector::Notes,
    }
}
