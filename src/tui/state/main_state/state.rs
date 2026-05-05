use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

use crate::commands::types::{
    ConfirmVariant, FieldSelector, Overlay, PanelId, RecordFilter, RecordSort, Screen as ScreenEnum,
};
use crate::commands::{Command, Message};
use crate::tui::screens::main::overlay::{ActiveOverlay, OverlayKeyResult, OverlayManager};
use crate::tui::screens::main::MainScreen;
use crate::tui::state::animation::EffectKind;
use crate::tui::state::detail_state::{DetailFieldKind, DetailPanelState, FieldValue};
use crate::tui::state::list_state::{ListMode, ListPanelState};
use crate::tui::state::overlay_state::HistoryEntry;
use crate::tui::state::tag_management::TagManagementState;
use crate::tui::traits::screen::{Screen, ScreenContext, ScreenResult};
use crate::types::{SecureStr, Tag};

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
    /// Brand header ("OpenKeyring") rendered at the top.
    Brand,
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
        !matches!(self, Self::Brand | Self::Separator)
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
    pub(crate) fn build_items(&self) -> Vec<SidebarItem> {
        let mut items: Vec<SidebarItem> = vec![
            SidebarItem::Brand,
            SidebarItem::Separator,
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
            overlay_manager: OverlayManager::new(),
            pending_animation: None,
            list_auto_select: false,
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
}

impl Screen for MainScreenState {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
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
                            FieldSelector::Password => "密码",
                            FieldSelector::Username => "用户名",
                            FieldSelector::Url => "网址",
                            FieldSelector::Notes => "备注",
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
                            field: "历史密码".to_string(),
                            seconds: clear_after_seconds as u32,
                        });
                        self.status_bar.clipboard_countdown = Some(clear_after_seconds as u32);
                        ScreenResult::Continue
                    }
                    CommandResult::RecordCreated { .. } => {
                        // Auto-select the first record (newly created) when list reloads
                        self.list_auto_select = true;
                        let _ = ctx.command_tx.try_send(Command::LoadRecordList {
                            filter: self.current_filter.clone(),
                            sort: self.current_sort.clone(),
                        });
                        ScreenResult::Continue
                    }
                    CommandResult::RecordUpdated { id } => {
                        let was_showing_detail =
                            self.detail.record.as_ref().is_some_and(|r| r.id == id);
                        let _ = ctx.command_tx.try_send(Command::LoadRecordList {
                            filter: self.current_filter.clone(),
                            sort: self.current_sort.clone(),
                        });
                        // Refresh detail if it was showing the updated record
                        if was_showing_detail {
                            let _ = ctx.command_tx.try_send(Command::LoadRecordDetail { id });
                        }
                        ScreenResult::Continue
                    }
                    CommandResult::RecordListLoaded { records, total } => {
                        let prev_selected_index = self.list.selected_index;
                        self.list.records = records;
                        self.list.total_count = total;
                        self.status_bar.record_count = total;

                        if self.list_auto_select && !self.list.records.is_empty() {
                            // Auto-select first record (sidebar filter change or record creation)
                            self.list.selected_index = Some(0);
                            self.list.scroll_offset = 0;
                            self.list_auto_select = false;
                            let id = self.list.records[0].id;
                            let _ = ctx.command_tx.try_send(Command::LoadRecordDetail { id });
                        } else if self.list.records.is_empty() {
                            self.list.selected_index = None;
                            self.list.scroll_offset = 0;
                            self.detail.clear();
                            self.list_auto_select = false;
                        } else {
                            // Cursor recovery: keep selected_index, clamp if OOB
                            match prev_selected_index {
                                Some(idx) if idx < self.list.records.len() => {
                                    self.list.selected_index = Some(idx);
                                }
                                Some(_) => {
                                    // OOB — clamp to last
                                    self.list.selected_index = Some(self.list.records.len() - 1);
                                }
                                None => {
                                    // No previous selection (initial load) — keep None
                                    self.list.selected_index = None;
                                }
                            }
                            self.list.adjust_scroll();
                            self.list_auto_select = false;
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
                        let _ = ctx.command_tx.try_send(Command::LoadRecordList {
                            filter: self.current_filter.clone(),
                            sort: self.current_sort.clone(),
                        });
                        ScreenResult::Continue
                    }
                    CommandResult::RecordRestored { id } => {
                        // Clear detail if it shows the restored record
                        if self.detail.record.as_ref().is_some_and(|r| r.id == id) {
                            self.detail.clear();
                        }
                        self.list.records.retain(|r| r.id != id);
                        // Reload list after restore
                        let _ = ctx.command_tx.try_send(Command::LoadRecordList {
                            filter: self.current_filter.clone(),
                            sort: self.current_sort.clone(),
                        });
                        ScreenResult::Continue
                    }
                    CommandResult::RecordDestroyed { id } => {
                        self.list.records.retain(|r| r.id != id);
                        // Reload list after permanent delete
                        let _ = ctx.command_tx.try_send(Command::LoadRecordList {
                            filter: self.current_filter.clone(),
                            sort: self.current_sort.clone(),
                        });
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
                    CommandResult::FieldDecrypted {
                        id,
                        field: _,
                        value,
                    } => {
                        self.detail.password_visible = true;
                        if let Some(ref mut record) = self.detail.record {
                            if record.id == id {
                                for f in &mut record.fields {
                                    if f.toggleable && matches!(f.value, FieldValue::Masked) {
                                        f.value = FieldValue::Revealed(value.get().clone());
                                        break;
                                    }
                                }
                            }
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
                    _ => ScreenResult::Continue,
                }
            }
            Message::ShowOverlay(overlay) => {
                self.overlay_manager.open(overlay);
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
        self.detail.trash_retention_days = ctx.config.general.trash_retention_days;
        if !ctx.config.security.health_check_enabled {
            self.status_bar.health_check_phase = HealthCheckPhase::Skipped;
        }
        // Load initial record list
        let _ = ctx.command_tx.try_send(Command::LoadRecordList {
            filter: self.current_filter.clone(),
            sort: self.current_sort.clone(),
        });
    }

    fn on_unmount(&mut self) {
        // No-op for now.
    }
}

impl MainScreenState {
    fn handle_key(&mut self, key: KeyEvent) -> ScreenResult {
        // Layer 1: Overlay gets priority
        if self.overlay_manager.is_active() {
            let result = self.overlay_manager.handle_key(key.code);
            return self.handle_overlay_result(result);
        }

        // Layer 1.5: Search mode captures all keys before global shortcuts
        if self.focused_panel == PanelId::List && self.list.is_searching() {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.list.exit_search();
                    return ScreenResult::Continue;
                }
                KeyCode::Backspace => {
                    if let ListMode::Search(ref s) = self.list.mode {
                        let mut new_query = s.query.clone();
                        new_query.pop();
                        self.list.update_search_query(new_query);
                    }
                    return ScreenResult::Continue;
                }
                KeyCode::Char(c) => {
                    // Ignore Ctrl/Alt combinations — let them fall through to global shortcuts
                    if key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
                    {
                        return ScreenResult::Continue;
                    }
                    if let ListMode::Search(ref s) = self.list.mode {
                        let new_query = format!("{}{}", s.query, c);
                        self.list.update_search_query(new_query);
                    }
                    return ScreenResult::Continue;
                }
                _ => return ScreenResult::Continue, // consume all other keys
            }
        }

        // Check sidebar Enter for Generator/Config navigation
        if self.focused_panel == PanelId::Sidebar && key.code == KeyCode::Enter {
            match self.sidebar.items.get(self.sidebar.selected_index) {
                Some(SidebarItem::Generator) => {
                    self.overlay_manager.open(Overlay::PasswordGenerator);
                    self.pending_animation = Some(EffectKind::ModalAppear);
                    return ScreenResult::Continue;
                }
                Some(SidebarItem::Config) => {
                    return ScreenResult::NavigateTo(ScreenEnum::Config);
                }
                _ => {}
            }
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
                        // Exit visual mode on sidebar navigation (U3 Spec)
                        if self.list.is_visual() {
                            self.list.exit_visual();
                        }
                        let new_filter = self.sidebar.current_filter();
                        if new_filter != old_filter {
                            self.current_filter = new_filter.clone();
                            self.detail.clear();
                            self.list_auto_select = true;
                            return ScreenResult::Command(Box::new(Command::LoadRecordList {
                                filter: new_filter,
                                sort: self.current_sort.clone(),
                            }));
                        }
                        return ScreenResult::Continue;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        let old_filter = self.sidebar.current_filter();
                        self.sidebar.move_up();
                        if self.list.is_visual() {
                            self.list.exit_visual();
                        }
                        let new_filter = self.sidebar.current_filter();
                        if new_filter != old_filter {
                            self.current_filter = new_filter.clone();
                            self.detail.clear();
                            self.list_auto_select = true;
                            return ScreenResult::Command(Box::new(Command::LoadRecordList {
                                filter: new_filter,
                                sort: self.current_sort.clone(),
                            }));
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
                    // Step 1: p — toggle password visibility (context-sensitive)
                    KeyCode::Char('p') => {
                        let needs_decrypt = self.detail.toggle_password();
                        if needs_decrypt {
                            if let Some(field) = self.detail.password_field() {
                                let selector = detail_field_kind_to_selector(field.kind);
                                return ScreenResult::Command(Box::new(Command::DecryptField {
                                    id,
                                    field: selector,
                                }));
                            }
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
                        self.detail.move_field_up();
                        return ScreenResult::Continue;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        self.detail.move_field_down();
                        return ScreenResult::Continue;
                    }
                    _ => {}
                }
            }
        }

        // Layer 2: Global shortcuts
        match key.code {
            KeyCode::Char('g') => ScreenResult::NavigateTo(ScreenEnum::Config),
            KeyCode::Char('l') => ScreenResult::NavigateTo(ScreenEnum::AuditLog),
            KeyCode::Char('p') => {
                self.overlay_manager.open(Overlay::PasswordGenerator);
                self.pending_animation = Some(EffectKind::ModalAppear);
                ScreenResult::Continue
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                ScreenResult::Command(Box::new(Command::TriggerSync))
            }
            KeyCode::Char('n') => ScreenResult::NavigateTo(ScreenEnum::CreateRecord),
            KeyCode::Char('e') => {
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
                ScreenResult::Command(Box::new(Command::CopyRawToClipboard {
                    value: SecureStr::new(password),
                }))
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
                    ConfirmVariant::EmptyTrash { .. } => {
                        ScreenResult::Command(Box::new(Command::EmptyTrash))
                    }
                    ConfirmVariant::TagDelete { tag_name, .. } => {
                        ScreenResult::Command(Box::new(Command::DeleteTag { name: tag_name }))
                    }
                }
            }
            OverlayKeyResult::BatchAddTag {
                record_ids,
                tag_name,
            } => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                ScreenResult::Command(Box::new(Command::BatchAddTag {
                    record_ids,
                    tag_name,
                }))
            }
            OverlayKeyResult::BatchRemoveTag {
                record_ids,
                tag_name,
            } => {
                self.overlay_manager.close();
                self.pending_animation = Some(EffectKind::ModalDismiss);
                ScreenResult::Command(Box::new(Command::BatchRemoveTag {
                    record_ids,
                    tag_name,
                }))
            }
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

/// Convert a [`DetailFieldKind`] to the corresponding [`FieldSelector`] for
/// clipboard copy and field decryption commands.
fn detail_field_kind_to_selector(kind: DetailFieldKind) -> FieldSelector {
    match kind {
        DetailFieldKind::Username | DetailFieldKind::AppId | DetailFieldKind::PublicKey => {
            FieldSelector::Username
        }
        DetailFieldKind::Password
        | DetailFieldKind::SecretKey
        | DetailFieldKind::PrivateKey
        | DetailFieldKind::Passphrase => FieldSelector::Password,
        DetailFieldKind::Url => FieldSelector::Url,
        DetailFieldKind::Notes => FieldSelector::Notes,
    }
}
