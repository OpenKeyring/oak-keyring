pub mod detail;
pub mod layout;
pub mod list;
pub mod overlay;
pub mod sidebar;
pub mod status_bar;

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::commands::types::{ConfirmButton, ConfirmDialogState, ConfirmVariant, Overlay, BatchTagPanelState, PanelId, RecordFilter};
use crate::commands::Message;
use crate::tui::screens::main::layout::{calculate_layout, HORIZONTAL_SEPARATOR, PANEL_SEPARATOR};
use crate::tui::screens::main::sidebar::SidebarPanel;
use crate::tui::screens::main::status_bar::StatusBarPanel;
use crate::tui::state::main_state::{MainScreenState, SidebarCategory, SidebarItem};
use crate::tui::state::tag_management::TagSortOrder;
use crate::tui::theme;
use crossterm::event::{KeyCode, KeyEvent};

/// Main three-panel screen: sidebar | list | detail, with a status bar.
pub struct MainScreen {
    #[allow(dead_code)]
    sidebar: SidebarPanel,
    #[allow(dead_code)]
    list: list::ListPanel,
    #[allow(dead_code)]
    detail: detail::DetailPanel,
    #[allow(dead_code)]
    status_bar: StatusBarPanel,
}

impl Default for MainScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl MainScreen {
    /// Create a new MainScreen with default sub-panels.
    pub fn new() -> Self {
        Self {
            sidebar: SidebarPanel,
            list: list::ListPanel,
            detail: detail::DetailPanel,
            status_bar: StatusBarPanel,
        }
    }

    /// Render the full main screen layout.
    ///
    /// # Arguments
    /// * `frame` - The ratatui frame to render into.
    /// * `area` - The total area available for the main screen.
    /// * `state` - The current main screen state (sidebar, status bar, etc.).
    /// * `focused_panel` - Which panel currently has keyboard focus.
    /// * `unicode` - Whether to use unicode characters (vs ASCII fallbacks).
    pub fn view(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &MainScreenState,
        focused_panel: PanelId,
        unicode: bool,
    ) {
        let terminal_width = frame.area().width;
        let areas = calculate_layout(area, terminal_width);

        // 1. Sidebar
        let sidebar_focused = focused_panel == PanelId::Sidebar;
        SidebarPanel::view(
            frame,
            areas.sidebar,
            &state.sidebar,
            sidebar_focused,
            unicode,
        );

        // 2. List panel
        let list_focused = focused_panel == PanelId::List;
        list::ListPanel::view(
            frame,
            areas.list,
            &state.list,
            list_focused,
            unicode,
            state.current_filter.clone(),
            30, // TODO: read from config general.trash_retention_days
        );

        // 3. Detail panel
        let detail_focused = focused_panel == PanelId::Detail;
        let visual_selected_names: Vec<String> = if state.list.is_visual() {
            let selected_ids = state.list.visual_selected_ids();
            state
                .list
                .records
                .iter()
                .filter(|r| selected_ids.contains(&r.id))
                .map(|r| r.name.clone())
                .collect()
        } else {
            Vec::new()
        };
        self.detail.view(
            frame,
            areas.detail,
            &state.detail,
            detail_focused,
            unicode,
            &visual_selected_names,
        );

        // 4. Horizontal separator between content and status bar
        render_horizontal_separator(frame, areas.status_separator, unicode);

        // 5. Vertical separators between panels (only in unicode mode)
        if unicode {
            render_vertical_separators(frame, &areas);
        }

        // Determine if we are viewing trash
        let is_trash = matches!(state.current_filter, RecordFilter::Trash);

        // 6. Status bar
        StatusBarPanel::view(
            frame,
            areas.status_bar,
            &state.status_bar,
            focused_panel,
            unicode,
            is_trash,
            state.list.is_visual(),
        );
    }

    /// Advance focus to next panel: Sidebar -> List -> Detail -> Sidebar.
    pub fn cycle_focus(&self, current: PanelId) -> PanelId {
        match current {
            PanelId::Sidebar => PanelId::List,
            PanelId::List => PanelId::Detail,
            PanelId::Detail => PanelId::Sidebar,
        }
    }

    /// Move focus to previous panel: Sidebar -> Detail -> List -> Sidebar.
    pub fn cycle_focus_reverse(&self, current: PanelId) -> PanelId {
        match current {
            PanelId::Sidebar => PanelId::Detail,
            PanelId::List => PanelId::Sidebar,
            PanelId::Detail => PanelId::List,
        }
    }

    /// Handle a key event for the main screen.
    pub fn handle_key_event(
        &self,
        key: KeyEvent,
        state: &mut MainScreenState,
        focused_panel: PanelId,
    ) -> MainKeyResult {
        let mut messages = Vec::new();
        let mut overlay = None;

        // If inline rename is active, route all keys to it first
        if state.sidebar.is_tag_management()
            && state.sidebar.tag_management.is_renaming()
        {
            match key.code {
                KeyCode::Char(c) => {
                    state.sidebar.tag_management.inline_edit.as_mut().unwrap().insert_char(c);
                }
                KeyCode::Backspace => {
                    state.sidebar.tag_management.inline_edit.as_mut().unwrap().backspace();
                }
                KeyCode::Left => {
                    state.sidebar.tag_management.inline_edit.as_mut().unwrap().cursor_left();
                }
                KeyCode::Right => {
                    state.sidebar.tag_management.inline_edit.as_mut().unwrap().cursor_right();
                }
                KeyCode::Enter => {
                    let existing_tags: Vec<String> =
                        state.sidebar.tags.iter().map(|t| t.name.clone()).collect();
                    let edit = state.sidebar.tag_management.inline_edit.as_mut().unwrap();
                    edit.check_conflict(&existing_tags);
                    if !edit.conflict && !edit.text.trim().is_empty() {
                        let edit_state = state.sidebar.tag_management.take_rename().unwrap();
                        let old_name = edit_state.original_name.clone();
                        let new_name = edit_state.confirm();
                        messages.push(Message::RenameTagConfirm { old_name, new_name });
                    }
                }
                KeyCode::Esc => {
                    state.sidebar.tag_management.cancel_rename();
                    messages.push(Message::RenameTagCancel);
                }
                _ => {}
            }
            return MainKeyResult { messages, overlay };
        }

        match focused_panel {
            PanelId::List => {
                match key.code {
                    KeyCode::Char('v') => {
                        if state.list.is_visual() {
                            state.list.exit_visual();
                            messages.push(Message::ExitVisualMode);
                        } else if !state.list.is_searching() {
                            state.list.enter_visual();
                            messages.push(Message::EnterVisualMode);
                        }
                    }
                    KeyCode::Char(' ') if state.list.is_visual() => {
                        state.list.toggle_select_current();
                        messages.push(Message::ToggleSelectRecord {
                            id: state.list.selected_record().map(|r| r.id).unwrap_or_default(),
                        });
                    }
                    KeyCode::Char('a') if state.list.is_visual() => {
                        if state.list.visual_selected_ids().len() == state.list.records.len() {
                            state.list.deselect_all();
                            messages.push(Message::DeselectAll);
                        } else {
                            state.list.select_all();
                            messages.push(Message::SelectAll);
                        }
                    }
                    KeyCode::Char('d') if state.list.is_visual() => {
                        let ids = state.list.visual_selected_ids();
                        if !ids.is_empty() {
                            let names: Vec<String> = state
                                .list
                                .records
                                .iter()
                                .filter(|r| ids.contains(&r.id))
                                .map(|r| r.name.clone())
                                .collect();
                            overlay = Some(Overlay::ConfirmDialog(ConfirmDialogState {
                                variant: ConfirmVariant::BatchSoftDelete {
                                    record_ids: ids,
                                    record_names: names,
                                },
                                focused_button: ConfirmButton::Confirm,
                            }));
                        }
                    }
                    KeyCode::Char('t') if state.list.is_visual() => {
                        let ids = state.list.visual_selected_ids();
                        if !ids.is_empty() {
                            let current_tag = match &state.current_filter {
                                RecordFilter::Tag(name) => name.clone(),
                                _ => String::new(),
                            };
                            overlay = Some(Overlay::BatchTagPanel(BatchTagPanelState {
                                record_ids: ids,
                                current_tag,
                            }));
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down if state.list.is_visual() => {
                        state.list.move_down();
                    }
                    KeyCode::Char('k') | KeyCode::Up if state.list.is_visual() => {
                        state.list.move_up();
                    }
                    KeyCode::Esc if state.list.is_visual() => {
                        state.list.exit_visual();
                        messages.push(Message::ExitVisualMode);
                    }
                    KeyCode::Char('s') if !state.list.is_visual() => {
                        state.list.toggle_sort_direction();
                    }
                    _ => {}
                }
            }
            PanelId::Sidebar => {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        state.sidebar.move_down();
                        if state.list.is_visual() {
                            state.list.exit_visual();
                            messages.push(Message::ExitVisualMode);
                        }
                        let filter = state.sidebar.current_filter();
                        state.current_filter = filter;
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        state.sidebar.move_up();
                        if state.list.is_visual() {
                            state.list.exit_visual();
                            messages.push(Message::ExitVisualMode);
                        }
                        let filter = state.sidebar.current_filter();
                        state.current_filter = filter;
                    }
                    KeyCode::Enter => {
                        if matches!(
                            state.sidebar.items.get(state.sidebar.selected_index),
                            Some(SidebarItem::TagHeader)
                        ) {
                            state.sidebar.toggle_tags();
                        }
                    }
                    KeyCode::Char('m') => {
                        let is_on_tag = state.sidebar.selected_tag_name().is_some();
                        if state.sidebar.is_tag_management() {
                            state.sidebar.exit_tag_management();
                            messages.push(Message::ExitTagManagement);
                        } else if is_on_tag && state.sidebar.tags_expanded {
                            state.sidebar.enter_tag_management();
                            messages.push(Message::EnterTagManagement);
                        }
                    }
                    KeyCode::Char('r') if state.sidebar.is_tag_management() => {
                        if let Some(name) = state.sidebar.selected_tag_name().map(|s| s.to_string()) {
                            state.sidebar.tag_management.start_rename(&name);
                            messages.push(Message::RenameTagStart);
                        }
                    }
                    KeyCode::Char('d') if state.sidebar.is_tag_management() => {
                        if let Some(name) = state.sidebar.selected_tag_name().map(|s| s.to_string()) {
                            let affected_count = state.list.records.iter().filter(|r| r.tags.contains(&name)).count();
                            overlay = Some(Overlay::ConfirmDialog(ConfirmDialogState {
                                variant: ConfirmVariant::TagDelete {
                                    tag_name: name.clone(),
                                    affected_count,
                                },
                                focused_button: ConfirmButton::Cancel,
                            }));
                            messages.push(Message::DeleteTagFromManagement);
                        }
                    }
                    KeyCode::Char('s') if state.sidebar.is_tag_management() => {
                        state.sidebar.tag_management.sort_order.cycle();
                        sort_sidebar_tags(&mut state.sidebar);
                        messages.push(Message::CycleTagSort);
                    }
                    KeyCode::Esc if state.sidebar.is_tag_management() => {
                        state.sidebar.exit_tag_management();
                        messages.push(Message::ExitTagManagement);
                    }
                    _ => {}
                }
            }
            PanelId::Detail => {
                match key.code {
                    KeyCode::Esc if state.list.is_visual() => {
                        state.list.exit_visual();
                        messages.push(Message::ExitVisualMode);
                    }
                    _ => {}
                }
            }
        }

        MainKeyResult { messages, overlay }
    }

    /// Handle post-batch-delete cleanup.
    pub fn handle_batch_delete_result(
        state: &mut MainScreenState,
        deleted_count: usize,
    ) {
        let removed_ids: Vec<uuid::Uuid> = state.list.visual_selected_ids();
        state.list.cleanup_after_batch(&removed_ids);

        // Set temporary status bar message
        state.status_bar.status_message = Some(
            crate::tui::state::main_state::StatusMessage::Temporary {
                text: format!(
                    "\u{2713} \u{5DF2}\u{5220}\u{9664} {} \u{6761}\u{5BC6}\u{7801}",
                    deleted_count
                ),
                ttl: 100,
            },
        );
        state.status_bar.temp_message_timer = Some(100);
    }

    /// Handle post-tag-delete cleanup.
    /// If the user was viewing the deleted tag, auto-switch to "All".
    pub fn handle_tag_delete_result(
        state: &mut MainScreenState,
        deleted_tag_name: &str,
    ) {
        // Remove tag from sidebar
        state.sidebar.tags.retain(|t| t.name != deleted_tag_name);
        state.sidebar.rebuild();

        // If currently viewing the deleted tag, switch to All
        if let RecordFilter::Tag(ref name) = state.current_filter {
            if name == deleted_tag_name {
                state.current_filter = RecordFilter::All;
                state.sidebar.select_category(SidebarCategory::All);
            }
        }

        // Status bar message
        state.status_bar.status_message = Some(
            crate::tui::state::main_state::StatusMessage::Temporary {
                text: format!(
                    "\u{2713} \u{5DF2}\u{5220}\u{9664}\u{6807}\u{7B7E} \"{}\"",
                    deleted_tag_name
                ),
                ttl: 100,
            },
        );
        state.status_bar.temp_message_timer = Some(100);
    }

    /// Handle tag rename result.
    pub fn handle_tag_rename_result(
        state: &mut MainScreenState,
        old_name: &str,
        new_name: &str,
    ) {
        // Update tag in sidebar
        for tag in &mut state.sidebar.tags {
            if tag.name == old_name {
                tag.name = new_name.to_string();
            }
        }
        state.sidebar.rebuild();

        // Update current filter if viewing the renamed tag
        if let RecordFilter::Tag(ref name) = state.current_filter {
            if name == old_name {
                state.current_filter = RecordFilter::Tag(new_name.to_string());
            }
        }

        // Status bar message
        state.status_bar.status_message = Some(
            crate::tui::state::main_state::StatusMessage::Temporary {
                text: format!(
                    "\u{2713} \u{5DF2}\u{91CD}\u{547D}\u{540D} \"{}\" \u{2192} \"{}\"",
                    old_name, new_name
                ),
                ttl: 100,
            },
        );
        state.status_bar.temp_message_timer = Some(100);
    }
}

/// Result of handling a key event on the main screen.
pub struct MainKeyResult {
    pub messages: Vec<Message>,
    pub overlay: Option<Overlay>,
}

/// Sort the sidebar tags according to the current sort order.
fn sort_sidebar_tags(sidebar: &mut crate::tui::state::main_state::SidebarState) {
    let sort_order = sidebar.tag_management.sort_order;
    sidebar.tags.sort_by(|a, b| match sort_order {
        TagSortOrder::Frequency => a.name.cmp(&b.name),
        TagSortOrder::Alphabetical => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        TagSortOrder::RecentlyUsed => a.name.cmp(&b.name),
    });
    sidebar.rebuild();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycle_forward() {
        let screen = MainScreen::new();
        assert_eq!(screen.cycle_focus(PanelId::Sidebar), PanelId::List);
        assert_eq!(screen.cycle_focus(PanelId::List), PanelId::Detail);
        assert_eq!(screen.cycle_focus(PanelId::Detail), PanelId::Sidebar);
    }

    #[test]
    fn focus_cycle_reverse() {
        let screen = MainScreen::new();
        assert_eq!(
            screen.cycle_focus_reverse(PanelId::Sidebar),
            PanelId::Detail
        );
        assert_eq!(screen.cycle_focus_reverse(PanelId::Detail), PanelId::List);
        assert_eq!(screen.cycle_focus_reverse(PanelId::List), PanelId::Sidebar);
    }

    // ── Keyboard routing tests ─────────────────────────────────────────────

    use crate::tui::state::list_state::ListPanelState;
    use crate::types::credential::CredentialType;
    use crate::types::record::TuiRecord;
    use crate::types::Tag;
    use chrono::Utc;
    use crossterm::event::KeyModifiers;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn make_test_record(name: &str) -> TuiRecord {
        TuiRecord {
            id: uuid::Uuid::new_v4(),
            credential_type: CredentialType::Login,
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
    fn v_enters_visual_mode() {
        let mut state = MainScreenState::default();
        let screen = MainScreen::new();
        let result = screen.handle_key_event(
            make_key(KeyCode::Char('v')),
            &mut state,
            PanelId::List,
        );
        assert!(state.list.is_visual());
        assert!(result.messages.iter().any(|m| matches!(m, Message::EnterVisualMode)));
    }

    #[test]
    fn v_exits_visual_mode() {
        let mut state = MainScreenState::default();
        state.list.enter_visual();
        let screen = MainScreen::new();
        let result = screen.handle_key_event(
            make_key(KeyCode::Char('v')),
            &mut state,
            PanelId::List,
        );
        assert!(!state.list.is_visual());
        assert!(result.messages.iter().any(|m| matches!(m, Message::ExitVisualMode)));
    }

    #[test]
    fn esc_exits_visual_mode() {
        let mut state = MainScreenState::default();
        state.list.enter_visual();
        let screen = MainScreen::new();
        let result = screen.handle_key_event(
            make_key(KeyCode::Esc),
            &mut state,
            PanelId::List,
        );
        assert!(!state.list.is_visual());
    }

    #[test]
    fn space_toggles_selection_in_visual() {
        let record = make_test_record("Test");
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(vec![record]);
        state.list.enter_visual();

        let screen = MainScreen::new();
        screen.handle_key_event(
            make_key(KeyCode::Char(' ')),
            &mut state,
            PanelId::List,
        );
        assert_eq!(state.list.visual_selected_ids().len(), 1);
    }

    #[test]
    fn a_selects_all_in_visual() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.list.enter_visual();

        let screen = MainScreen::new();
        screen.handle_key_event(
            make_key(KeyCode::Char('a')),
            &mut state,
            PanelId::List,
        );
        assert_eq!(state.list.visual_selected_ids().len(), 3);
    }

    #[test]
    fn m_enters_tag_management_on_tag() {
        let mut state = MainScreenState::default();
        state.sidebar.tags_expanded = true;
        state.sidebar.tags = vec![Tag { id: 1, name: "work".into() }];
        state.sidebar.rebuild();
        let tag_idx = state.sidebar.items.iter().position(|i| matches!(i, SidebarItem::Tag(_))).unwrap();
        state.sidebar.selected_index = tag_idx;

        let screen = MainScreen::new();
        screen.handle_key_event(
            make_key(KeyCode::Char('m')),
            &mut state,
            PanelId::Sidebar,
        );
        assert!(state.sidebar.is_tag_management());
    }

    #[test]
    fn m_exits_tag_management() {
        let mut state = MainScreenState::default();
        state.sidebar.tag_management_mode = true;
        let screen = MainScreen::new();
        screen.handle_key_event(
            make_key(KeyCode::Char('m')),
            &mut state,
            PanelId::Sidebar,
        );
        assert!(!state.sidebar.is_tag_management());
    }

    #[test]
    fn sidebar_navigation_exits_visual_mode() {
        let mut state = MainScreenState::default();
        state.list.enter_visual();
        let screen = MainScreen::new();
        screen.handle_key_event(
            make_key(KeyCode::Down),
            &mut state,
            PanelId::Sidebar,
        );
        assert!(!state.list.is_visual());
    }
}

/// Render the horizontal separator line between content panels and the status bar.
fn render_horizontal_separator(frame: &mut Frame, area: Rect, unicode: bool) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let sep_char = if unicode { HORIZONTAL_SEPARATOR } else { "-" };
    let line: String =
        std::iter::repeat_n(sep_char.chars().next().unwrap_or('-'), area.width as usize).collect();

    let paragraph = Paragraph::new(Line::from(Span::styled(
        line,
        Style::default().fg(theme::BORDER),
    )));
    frame.render_widget(paragraph, area);
}

/// Render vertical separator characters ("│") between the three panels.
///
/// Draws separator lines at the boundaries between sidebar|list and list|detail.
fn render_vertical_separators(frame: &mut Frame, areas: &layout::MainLayoutAreas) {
    let sep_style = Style::default().fg(theme::BORDER);

    // Separator between sidebar and list
    if areas.sidebar.width > 0 && areas.list.width > 0 {
        let x = areas.sidebar.x + areas.sidebar.width;
        // Only render if there is no overlap (the separator column was not
        // allocated to any panel — it visually sits on the border).
        // We render into a 1-column-wide strip at the panel boundary.
        let sep_rect = Rect::new(
            x.saturating_sub(1),
            areas.sidebar.y,
            1,
            areas.sidebar.height,
        );
        let line: String = std::iter::repeat_n(
            PANEL_SEPARATOR.chars().next().unwrap(),
            sep_rect.height as usize,
        )
        .collect();
        let paragraph = Paragraph::new(Line::from(Span::styled(line, sep_style)));
        frame.render_widget(paragraph, sep_rect);
    }

    // Separator between list and detail
    if areas.list.width > 0 && areas.detail.width > 0 {
        let x = areas.list.x + areas.list.width;
        let sep_rect = Rect::new(x.saturating_sub(1), areas.list.y, 1, areas.list.height);
        let line: String = std::iter::repeat_n(
            PANEL_SEPARATOR.chars().next().unwrap(),
            sep_rect.height as usize,
        )
        .collect();
        let paragraph = Paragraph::new(Line::from(Span::styled(line, sep_style)));
        frame.render_widget(paragraph, sep_rect);
    }
}
