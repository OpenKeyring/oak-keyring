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

use crate::commands::types::{
    BatchTagPanelState, ConfirmButton, ConfirmDialogState, ConfirmVariant, FieldSelector, Overlay,
    PanelId, RecordFilter,
};
use crate::commands::{Command, Message};
use crate::t;
use crate::tui::screens::main::layout::{calculate_layout, HORIZONTAL_SEPARATOR, PANEL_SEPARATOR};
use crate::tui::screens::main::sidebar::SidebarPanel;
use crate::tui::screens::main::status_bar::StatusBarPanel;
use crate::tui::state::main_state::{MainScreenState, SidebarCategory, SidebarItem};
use crate::tui::theme;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
            state.trash_retention_days,
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

        // 7. Overlay (rendered on top of all panels)
        if state.overlay_manager.is_active() {
            state.overlay_manager.render(frame, area, unicode);
        }
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
        let result_command: Option<Box<Command>> = None;

        if key.code == KeyCode::F(1) {
            return MainKeyResult {
                messages,
                overlay: Some(Overlay::Help),
                command: None,
                focused_panel: None,
            };
        }

        // If inline rename is active, route all keys to it first
        if state.sidebar.is_tag_management() && state.sidebar.tag_management.is_renaming() {
            match key.code {
                KeyCode::Char(c) => {
                    if let Some(edit) = state.sidebar.tag_management.inline_edit.as_mut() {
                        edit.insert_char(c);
                    }
                }
                KeyCode::Backspace => {
                    if let Some(edit) = state.sidebar.tag_management.inline_edit.as_mut() {
                        edit.backspace();
                    }
                }
                KeyCode::Left => {
                    if let Some(edit) = state.sidebar.tag_management.inline_edit.as_mut() {
                        edit.cursor_left();
                    }
                }
                KeyCode::Right => {
                    if let Some(edit) = state.sidebar.tag_management.inline_edit.as_mut() {
                        edit.cursor_right();
                    }
                }
                KeyCode::Enter => {
                    let existing_tags: Vec<String> =
                        state.sidebar.tags.iter().map(|t| t.name.clone()).collect();
                    let should_confirm =
                        if let Some(edit) = state.sidebar.tag_management.inline_edit.as_mut() {
                            edit.check_conflict(&existing_tags);
                            !edit.conflict && !edit.text.trim().is_empty()
                        } else {
                            false
                        };
                    if should_confirm {
                        let Some(edit_state) = state.sidebar.tag_management.take_rename() else {
                            return MainKeyResult {
                                messages,
                                overlay,
                                command: None,
                                focused_panel: None,
                            };
                        };
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
            return MainKeyResult {
                messages,
                overlay,
                command: None,
                focused_panel: None,
            };
        }

        match focused_panel {
            PanelId::List => {
                // v — visual mode toggle (works in all views including trash)
                if key.code == KeyCode::Char('v') {
                    if state.list.is_visual() {
                        state.list.exit_visual();
                        messages.push(Message::ExitVisualMode);
                    } else if !state.list.is_searching() {
                        state.list.enter_visual();
                        messages.push(Message::EnterVisualMode);
                    }
                    return MainKeyResult {
                        messages,
                        overlay: None,
                        command: None,
                        focused_panel: None,
                    };
                }

                // Esc — exit visual mode or search (all views)
                if key.code == KeyCode::Esc {
                    if state.list.is_visual() {
                        state.list.exit_visual();
                        messages.push(Message::ExitVisualMode);
                    } else if state.list.is_searching() {
                        state.list.exit_search();
                    }
                    return MainKeyResult {
                        messages,
                        overlay: None,
                        command: None,
                        focused_panel: None,
                    };
                }

                // Visual mode selection keys (Space, a) — work in all views
                if state.list.is_visual() {
                    match key.code {
                        KeyCode::Char(' ') => {
                            state.list.toggle_select_current();
                            messages.push(Message::ToggleSelectRecord {
                                id: state
                                    .list
                                    .selected_record()
                                    .map(|r| r.id)
                                    .unwrap_or_default(),
                            });
                            return MainKeyResult {
                                messages,
                                overlay: None,
                                command: None,
                                focused_panel: None,
                            };
                        }
                        KeyCode::Char('a') => {
                            if state.list.visual_selected_ids().len() == state.list.records.len() {
                                state.list.deselect_all();
                                messages.push(Message::DeselectAll);
                            } else {
                                state.list.select_all();
                                messages.push(Message::SelectAll);
                            }
                            return MainKeyResult {
                                messages,
                                overlay: None,
                                command: None,
                                focused_panel: None,
                            };
                        }
                        _ => {} // fall through to view-specific handling
                    }
                }

                // Trash mode — visual-aware dispatch for r/D/a/navigation
                if matches!(state.current_filter, RecordFilter::Trash) {
                    return Self::handle_trash_keys(key, state);
                }

                let mut result_command: Option<Box<Command>> = None;
                let mut focused_panel_result: Option<PanelId> = None;

                match key.code {
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
                    KeyCode::Char('k')
                        if is_search_shortcut(key)
                            && !state.list.is_searching()
                            && !state.list.is_visual() =>
                    {
                        state.list.enter_search();
                    }
                    KeyCode::Enter if !state.list.is_visual() && !state.list.is_searching() => {
                        state.focused_panel = PanelId::Detail;
                        focused_panel_result = Some(PanelId::Detail);
                        if let Some(record) = state.list.selected_record() {
                            result_command =
                                Some(Box::new(Command::LoadRecordDetail { id: record.id }));
                        }
                    }
                    KeyCode::Char('d') if !state.list.is_visual() && !state.list.is_searching() => {
                        if let Some(record) = state.list.selected_record() {
                            let record_id = record.id;
                            let record_name = record.name.clone();
                            overlay = Some(Overlay::ConfirmDialog(ConfirmDialogState {
                                variant: ConfirmVariant::SoftDelete {
                                    record_id,
                                    record_name,
                                    auto_delete_days: Some(state.trash_retention_days),
                                },
                                focused_button: ConfirmButton::Cancel,
                            }));
                        }
                    }
                    KeyCode::Char('f') if !state.list.is_visual() && !state.list.is_searching() => {
                        if let Some(record) = state.list.selected_record() {
                            result_command = Some(Box::new(Command::ToggleFavorite {
                                id: record.id,
                                is_favorite: !record.is_favorite,
                            }));
                        }
                    }
                    KeyCode::Char('c') if !state.list.is_visual() && !state.list.is_searching() => {
                        if let Some(record) = state.list.selected_record() {
                            result_command = Some(Box::new(Command::CopyToClipboard {
                                id: record.id,
                                field: FieldSelector::Password,
                            }));
                        }
                    }
                    KeyCode::Char('u') if !state.list.is_visual() && !state.list.is_searching() => {
                        if let Some(record) = state.list.selected_record() {
                            result_command = Some(Box::new(Command::CopyToClipboard {
                                id: record.id,
                                field: FieldSelector::Username,
                            }));
                        }
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        state.list.move_down();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        state.list.move_up();
                    }
                    KeyCode::Char('s') if !state.list.is_visual() && !state.list.is_searching() => {
                        state.list.toggle_sort_direction();
                        state.current_sort.direction = state.list.sort.direction;
                        let cmd = Box::new(Command::LoadRecordList {
                            filter: state.current_filter.clone(),
                            sort: state.current_sort.clone(),
                        });
                        return MainKeyResult {
                            messages,
                            overlay,
                            command: Some(cmd),
                            focused_panel: None,
                        };
                    }
                    KeyCode::Char('S') if !state.list.is_visual() && !state.list.is_searching() => {
                        state.list.cycle_sort_field();
                        state.current_sort.field = state.list.sort.field;
                        let cmd = Box::new(Command::LoadRecordList {
                            filter: state.current_filter.clone(),
                            sort: state.current_sort.clone(),
                        });
                        return MainKeyResult {
                            messages,
                            overlay,
                            command: Some(cmd),
                            focused_panel: None,
                        };
                    }
                    _ => {}
                }

                return MainKeyResult {
                    messages,
                    overlay,
                    command: result_command,
                    focused_panel: focused_panel_result,
                };
            }
            PanelId::Sidebar => match key.code {
                KeyCode::Enter => {
                    if matches!(
                        state.sidebar.items.get(state.sidebar.selected_index),
                        Some(SidebarItem::TagHeader)
                    ) {
                        state.sidebar.toggle_tags();
                    }
                }
                KeyCode::Char('m') => {
                    let is_on_tag_or_header = matches!(
                        state.sidebar.items.get(state.sidebar.selected_index),
                        Some(SidebarItem::Tag(_, _)) | Some(SidebarItem::TagHeader)
                    );
                    if state.sidebar.is_tag_management() {
                        state.sidebar.exit_tag_management();
                        messages.push(Message::ExitTagManagement);
                    } else if is_on_tag_or_header && state.sidebar.tags_expanded {
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
                        let affected_count = state
                            .list
                            .records
                            .iter()
                            .filter(|r| r.tags.contains(&name))
                            .count();
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
            },
            PanelId::Detail => match key.code {
                KeyCode::Esc if state.list.is_visual() => {
                    state.list.exit_visual();
                    messages.push(Message::ExitVisualMode);
                }
                _ => {}
            },
        }

        MainKeyResult {
            messages,
            overlay,
            command: result_command,
            focused_panel: None,
        }
    }

    /// Handle post-batch-delete cleanup.
    pub fn handle_batch_delete_result(state: &mut MainScreenState, deleted_count: usize) {
        let removed_ids: Vec<uuid::Uuid> = state.list.visual_selected_ids();
        state.list.cleanup_after_batch(&removed_ids);

        // Set temporary status bar message
        state.status_bar.status_message =
            Some(crate::tui::state::main_state::StatusMessage::Temporary {
                text: format!(
                    "\u{2713} {} {}",
                    t!("tui.notification.deleted"),
                    t!("tui.status_bar.record_count", count = deleted_count)
                ),
                ttl: 100,
            });
        state.status_bar.temp_message_timer = Some(100);
    }

    /// Handle post-tag-delete cleanup.
    /// If the user was viewing the deleted tag, auto-switch to "All".
    pub fn handle_tag_delete_result(state: &mut MainScreenState, deleted_tag_name: &str) {
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
        state.status_bar.status_message =
            Some(crate::tui::state::main_state::StatusMessage::Temporary {
                text: format!(
                    "\u{2713} {} \"{}\"",
                    t!("tui.tag.delete_body"),
                    deleted_tag_name
                ),
                ttl: 100,
            });
        state.status_bar.temp_message_timer = Some(100);
    }

    /// Handle tag rename result.
    pub fn handle_tag_rename_result(state: &mut MainScreenState, old_name: &str, new_name: &str) {
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
        state.status_bar.status_message =
            Some(crate::tui::state::main_state::StatusMessage::Temporary {
                text: format!(
                    "\u{2713} {}",
                    t!("tui.notification.renamed", old = old_name, new = new_name)
                ),
                ttl: 100,
            });
        state.status_bar.temp_message_timer = Some(100);
    }

    /// Handle trash-specific key bindings (r/D/a + navigation).
    ///
    /// Visual mode keys (v, Space, a) are handled before this function is called.
    /// Here, r and D dispatch to single or batch operations based on visual state.
    fn handle_trash_keys(key: KeyEvent, state: &mut MainScreenState) -> MainKeyResult {
        let messages = Vec::new();
        let mut overlay = None;

        match key.code {
            // r — restore (single or batch)
            KeyCode::Char('r') => {
                if state.list.is_visual() {
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
                            variant: ConfirmVariant::BatchRestore {
                                record_ids: ids,
                                record_names: names,
                            },
                            focused_button: ConfirmButton::Confirm,
                        }));
                    }
                } else if let Some(record) = state.list.selected_record() {
                    let record_id = record.id;
                    let record_name = record.name.clone();
                    overlay = Some(Overlay::ConfirmDialog(ConfirmDialogState {
                        variant: ConfirmVariant::Restore {
                            record_id,
                            record_name,
                        },
                        focused_button: ConfirmButton::Cancel,
                    }));
                }
            }
            // D (Shift+D) — hard delete (single or batch)
            KeyCode::Char('D') => {
                if state.list.is_visual() {
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
                            variant: ConfirmVariant::BatchHardDelete {
                                record_ids: ids,
                                record_names: names,
                            },
                            focused_button: ConfirmButton::Cancel,
                        }));
                    }
                } else if let Some(record) = state.list.selected_record() {
                    let record_id = record.id;
                    let record_name = record.name.clone();
                    overlay = Some(Overlay::ConfirmDialog(ConfirmDialogState {
                        variant: ConfirmVariant::HardDelete {
                            record_id,
                            record_name,
                        },
                        focused_button: ConfirmButton::Cancel,
                    }));
                }
            }
            // a — empty all trash (only in non-visual mode; visual 'a' handled above)
            KeyCode::Char('a') => {
                let count = state.list.records.len();
                if count > 0 {
                    overlay = Some(Overlay::ConfirmDialog(ConfirmDialogState {
                        variant: ConfirmVariant::EmptyTrash { count },
                        focused_button: ConfirmButton::Cancel,
                    }));
                }
            }
            // Navigation loads detail for the newly selected trash record
            KeyCode::Char('j') | KeyCode::Down => {
                state.list.move_down();
                if let Some(record) = state.list.selected_record() {
                    return MainKeyResult {
                        messages,
                        overlay,
                        command: Some(Box::new(Command::LoadRecordDetail { id: record.id })),
                        focused_panel: None,
                    };
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                state.list.move_up();
                if let Some(record) = state.list.selected_record() {
                    return MainKeyResult {
                        messages,
                        overlay,
                        command: Some(Box::new(Command::LoadRecordDetail { id: record.id })),
                        focused_panel: None,
                    };
                }
            }
            _ => {}
        }

        MainKeyResult {
            messages,
            overlay,
            command: None,
            focused_panel: None,
        }
    }
}

/// Result of handling a key event on the main screen.
pub struct MainKeyResult {
    pub messages: Vec<Message>,
    pub overlay: Option<Overlay>,
    pub command: Option<Box<crate::commands::Command>>,
    pub focused_panel: Option<PanelId>,
}

/// Sort the sidebar tags according to the current sort order.
fn sort_sidebar_tags(sidebar: &mut crate::tui::state::main_state::SidebarState) {
    sidebar.sort_tags_by_current_order();
}

fn is_search_shortcut(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('k')
        && key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER | KeyModifiers::META)
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
    let sep_char = PANEL_SEPARATOR.chars().next().unwrap_or('|');

    for sep_rect in [areas.sidebar_list_separator, areas.list_detail_separator] {
        if sep_rect.width == 0 || sep_rect.height == 0 {
            continue;
        }
        let line: String = std::iter::repeat_n(sep_char, sep_rect.height as usize).collect();
        let paragraph = Paragraph::new(Line::from(Span::styled(line, sep_style)));
        frame.render_widget(paragraph, sep_rect);
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::tui::state::tag_management::TagSortOrder;

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
            is_compromised: false,
            duplicate_group_size: None,
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
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('v')), &mut state, PanelId::List);
        assert!(state.list.is_visual());
        assert!(result
            .messages
            .iter()
            .any(|m| matches!(m, Message::EnterVisualMode)));
    }

    #[test]
    fn v_exits_visual_mode() {
        let mut state = MainScreenState::default();
        state.list.enter_visual();
        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('v')), &mut state, PanelId::List);
        assert!(!state.list.is_visual());
        assert!(result
            .messages
            .iter()
            .any(|m| matches!(m, Message::ExitVisualMode)));
    }

    #[test]
    fn esc_exits_visual_mode() {
        let mut state = MainScreenState::default();
        state.list.enter_visual();
        let screen = MainScreen::new();
        let _ = screen.handle_key_event(make_key(KeyCode::Esc), &mut state, PanelId::List);
        assert!(!state.list.is_visual());
    }

    #[test]
    fn space_toggles_selection_in_visual() {
        let record = make_test_record("Test");
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(vec![record]);
        state.list.enter_visual();

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char(' ')), &mut state, PanelId::List);
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
        screen.handle_key_event(make_key(KeyCode::Char('a')), &mut state, PanelId::List);
        assert_eq!(state.list.visual_selected_ids().len(), 3);
    }

    #[test]
    fn m_enters_tag_management_on_tag() {
        let mut state = MainScreenState::default();
        state.sidebar.tags_expanded = true;
        state.sidebar.tags = vec![Tag {
            id: 1,
            name: "work".into(),
        }];
        state.sidebar.rebuild();
        let tag_idx = state
            .sidebar
            .items
            .iter()
            .position(|i| matches!(i, SidebarItem::Tag(_, _)))
            .unwrap();
        state.sidebar.selected_index = tag_idx;

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char('m')), &mut state, PanelId::Sidebar);
        assert!(state.sidebar.is_tag_management());
    }

    #[test]
    fn m_exits_tag_management() {
        let mut state = MainScreenState::default();
        state.sidebar.tag_management_mode = true;
        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char('m')), &mut state, PanelId::Sidebar);
        assert!(!state.sidebar.is_tag_management());
    }

    #[test]
    fn sidebar_navigation_exits_visual_mode() {
        use crate::tui::traits::screen::Screen;

        let mut state = MainScreenState::default();
        state.list.enter_visual();
        state.focused_panel = PanelId::Sidebar;

        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        let config = crate::config::AppConfig::default();
        let mut ctx = crate::tui::traits::screen::ScreenContext {
            command_tx: &tx,
            config: &config,
        };

        state.update(Message::KeyEvent(make_key(KeyCode::Down)), &mut ctx);
        assert!(!state.list.is_visual());
    }

    // ── Normal mode j/k navigation tests ───────────────────────────────────────

    #[test]
    fn j_moves_down_in_normal_mode() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        assert_eq!(state.list.selected_index, Some(0));

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char('j')), &mut state, PanelId::List);
        assert_eq!(state.list.selected_index, Some(1));
    }

    #[test]
    fn k_moves_up_in_normal_mode() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.list.move_down(); // move to index 1

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char('k')), &mut state, PanelId::List);
        assert_eq!(state.list.selected_index, Some(0));
    }

    #[test]
    fn ctrl_k_enters_search_mode() {
        let mut state = MainScreenState::default();
        let screen = MainScreen::new();
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        screen.handle_key_event(key, &mut state, PanelId::List);
        assert!(state.list.is_searching());
    }

    #[test]
    fn super_k_enters_search_mode_when_terminal_sends_it() {
        let mut state = MainScreenState::default();
        let screen = MainScreen::new();
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::SUPER);
        screen.handle_key_event(key, &mut state, PanelId::List);
        assert!(state.list.is_searching());
    }

    #[test]
    fn f1_opens_help_overlay_from_list() {
        let mut state = MainScreenState::default();
        let screen = MainScreen::new();
        let key = KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE);

        let result = screen.handle_key_event(key, &mut state, PanelId::List);

        assert!(matches!(result.overlay, Some(Overlay::Help)));
    }

    #[test]
    fn plain_k_does_not_enter_search() {
        let mut state = MainScreenState::default();
        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char('k')), &mut state, PanelId::List);
        assert!(!state.list.is_searching());
    }

    // ── Action key tests ───────────────────────────────────────────────────────

    #[test]
    fn enter_focuses_detail_panel() {
        let records = vec![make_test_record("Test")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Enter), &mut state, PanelId::List);
        assert_eq!(state.focused_panel, PanelId::Detail);
    }

    #[test]
    fn d_normal_opens_soft_delete_confirm() {
        let records = vec![make_test_record("Test")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('d')), &mut state, PanelId::List);
        assert!(result.overlay.is_some());
        match result.overlay {
            Some(Overlay::ConfirmDialog(ref dlg)) => {
                assert!(matches!(dlg.variant, ConfirmVariant::SoftDelete { .. }));
            }
            _ => panic!("Expected confirm dialog"),
        }
    }

    #[test]
    fn f_returns_toggle_favorite_command() {
        let records = vec![make_test_record("Test")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('f')), &mut state, PanelId::List);
        assert!(result.command.is_some());
    }

    #[test]
    fn c_returns_copy_password_command() {
        let records = vec![make_test_record("Test")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('c')), &mut state, PanelId::List);
        assert!(result.command.is_some());
    }

    #[test]
    fn u_returns_copy_username_command() {
        let records = vec![make_test_record("Test")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('u')), &mut state, PanelId::List);
        assert!(result.command.is_some());
    }

    // ── Trash key tests ────────────────────────────────────────────────────────

    use crate::commands::types::RecordFilter;

    #[test]
    fn trash_r_opens_restore_confirm() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('r')), &mut state, PanelId::List);
        assert!(result.overlay.is_some());
        match result.overlay {
            Some(Overlay::ConfirmDialog(ref dlg)) => {
                assert!(matches!(dlg.variant, ConfirmVariant::Restore { .. }));
            }
            _ => panic!("Expected restore confirm dialog"),
        }
    }

    #[test]
    fn trash_shift_d_opens_hard_delete_confirm() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('D')), &mut state, PanelId::List);
        assert!(result.overlay.is_some());
        match result.overlay {
            Some(Overlay::ConfirmDialog(ref dlg)) => {
                assert!(matches!(dlg.variant, ConfirmVariant::HardDelete { .. }));
            }
            _ => panic!("Expected hard delete confirm dialog"),
        }
    }

    #[test]
    fn trash_a_opens_empty_trash_confirm() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('a')), &mut state, PanelId::List);
        assert!(result.overlay.is_some());
        match result.overlay {
            Some(Overlay::ConfirmDialog(ref dlg)) => {
                assert!(matches!(dlg.variant, ConfirmVariant::EmptyTrash { .. }));
            }
            _ => panic!("Expected empty trash confirm dialog"),
        }
    }

    #[test]
    fn trash_f_does_nothing() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('f')), &mut state, PanelId::List);
        assert!(result.command.is_none());
        assert!(result.overlay.is_none());
    }

    #[test]
    fn trash_jk_navigation_works() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char('j')), &mut state, PanelId::List);
        assert_eq!(state.list.selected_index, Some(1));

        screen.handle_key_event(make_key(KeyCode::Char('k')), &mut state, PanelId::List);
        assert_eq!(state.list.selected_index, Some(0));
    }

    #[test]
    fn trash_j_sends_load_record_detail() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let ids: Vec<_> = records.iter().map(|r| r.id).collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('j')), &mut state, PanelId::List);
        assert_eq!(state.list.selected_index, Some(1));
        assert!(
            result.command.is_some(),
            "trash j should send LoadRecordDetail"
        );
        match result.command {
            Some(cmd) => {
                assert!(
                    matches!(cmd.as_ref(), Command::LoadRecordDetail { id } if *id == ids[1]),
                    "expected LoadRecordDetail with id of second record"
                );
            }
            None => panic!("trash j should send LoadRecordDetail command"),
        }
    }

    #[test]
    fn trash_k_sends_load_record_detail() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let ids: Vec<_> = records.iter().map(|r| r.id).collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.move_down();
        assert_eq!(state.list.selected_index, Some(1));

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('k')), &mut state, PanelId::List);
        assert_eq!(state.list.selected_index, Some(0));
        assert!(
            result.command.is_some(),
            "trash k should send LoadRecordDetail"
        );
        match result.command {
            Some(cmd) => {
                assert!(
                    matches!(cmd.as_ref(), Command::LoadRecordDetail { id } if *id == ids[0]),
                    "expected LoadRecordDetail with id of first record"
                );
            }
            None => panic!("trash k should send LoadRecordDetail command"),
        }
    }

    // ── Trash visual mode tests ─────────────────────────────────────────────────

    #[test]
    fn trash_v_enters_visual_mode() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('v')), &mut state, PanelId::List);
        assert!(state.list.is_visual());
        assert!(result
            .messages
            .iter()
            .any(|m| matches!(m, Message::EnterVisualMode)));
    }

    #[test]
    fn trash_v_exits_visual_mode() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('v')), &mut state, PanelId::List);
        assert!(!state.list.is_visual());
        assert!(result
            .messages
            .iter()
            .any(|m| matches!(m, Message::ExitVisualMode)));
    }

    #[test]
    fn trash_esc_exits_visual_mode() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();

        let screen = MainScreen::new();
        let _ = screen.handle_key_event(make_key(KeyCode::Esc), &mut state, PanelId::List);
        assert!(!state.list.is_visual());
    }

    #[test]
    fn trash_space_toggles_selection_in_visual() {
        let records = vec![make_test_record("Deleted")];
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char(' ')), &mut state, PanelId::List);
        assert_eq!(state.list.visual_selected_ids().len(), 1);
    }

    #[test]
    fn trash_a_selects_all_in_visual() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();

        let screen = MainScreen::new();
        screen.handle_key_event(make_key(KeyCode::Char('a')), &mut state, PanelId::List);
        assert_eq!(state.list.visual_selected_ids().len(), 3);
    }

    #[test]
    fn trash_visual_r_opens_batch_restore_confirm() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();
        state.list.select_all();

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('r')), &mut state, PanelId::List);
        assert!(result.overlay.is_some());
        match result.overlay {
            Some(Overlay::ConfirmDialog(ref dlg)) => {
                assert!(matches!(dlg.variant, ConfirmVariant::BatchRestore { .. }));
            }
            _ => panic!("Expected batch restore confirm dialog"),
        }
    }

    #[test]
    fn trash_visual_shift_d_opens_batch_hard_delete_confirm() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();
        state.list.select_all();

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('D')), &mut state, PanelId::List);
        assert!(result.overlay.is_some());
        match result.overlay {
            Some(Overlay::ConfirmDialog(ref dlg)) => {
                assert!(matches!(
                    dlg.variant,
                    ConfirmVariant::BatchHardDelete { .. }
                ));
            }
            _ => panic!("Expected batch hard delete confirm dialog"),
        }
    }

    #[test]
    fn trash_visual_a_not_empty_trash() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('a')), &mut state, PanelId::List);
        // In visual mode, 'a' selects all — NOT empty trash
        assert!(result.overlay.is_none());
        assert_eq!(state.list.visual_selected_ids().len(), 3);
    }

    #[test]
    fn trash_visual_navigation_works() {
        let records: Vec<TuiRecord> = (0..3)
            .map(|i| make_test_record(&format!("R{}", i)))
            .collect();
        let mut state = MainScreenState::default();
        state.list = ListPanelState::with_records(records);
        state.current_filter = RecordFilter::Trash;
        state.list.enter_visual();

        let screen = MainScreen::new();
        let result =
            screen.handle_key_event(make_key(KeyCode::Char('j')), &mut state, PanelId::List);
        assert_eq!(state.list.selected_index, Some(1));
        assert!(
            result.command.is_some(),
            "trash visual j should send LoadRecordDetail"
        );
    }

    // ── Tag sorting tests ───────────────────────────────────────────────────────

    use crate::tui::state::main_state::SidebarState;
    use crate::types::tag::TagSortMeta;
    use std::collections::HashMap;

    #[test]
    fn sort_tags_by_frequency() {
        let mut sidebar = SidebarState {
            tags_expanded: true,
            tags: vec![
                Tag {
                    id: 1,
                    name: "work".into(),
                },
                Tag {
                    id: 2,
                    name: "personal".into(),
                },
                Tag {
                    id: 3,
                    name: "finance".into(),
                },
            ],
            tag_metadata: HashMap::from([
                (
                    1,
                    TagSortMeta {
                        record_count: 5,
                        last_used_at: 0,
                    },
                ),
                (
                    2,
                    TagSortMeta {
                        record_count: 2,
                        last_used_at: 0,
                    },
                ),
                (
                    3,
                    TagSortMeta {
                        record_count: 8,
                        last_used_at: 0,
                    },
                ),
            ]),
            ..Default::default()
        };
        sidebar.tag_management.sort_order = TagSortOrder::Frequency;
        sort_sidebar_tags(&mut sidebar);

        let names: Vec<&str> = sidebar.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["finance", "work", "personal"]); // 8, 5, 2 descending
    }

    #[test]
    fn sort_tags_by_frequency_tiebreak_by_name() {
        let mut sidebar = SidebarState {
            tags_expanded: true,
            tags: vec![
                Tag {
                    id: 1,
                    name: "beta".into(),
                },
                Tag {
                    id: 2,
                    name: "alpha".into(),
                },
            ],
            tag_metadata: HashMap::from([
                (
                    1,
                    TagSortMeta {
                        record_count: 5,
                        last_used_at: 0,
                    },
                ),
                (
                    2,
                    TagSortMeta {
                        record_count: 5,
                        last_used_at: 0,
                    },
                ),
            ]),
            ..Default::default()
        };
        sidebar.tag_management.sort_order = TagSortOrder::Frequency;
        sort_sidebar_tags(&mut sidebar);

        let names: Vec<&str> = sidebar.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta"]); // same count, alphabetical tiebreak
    }

    #[test]
    fn sort_tags_by_recently_used() {
        let mut sidebar = SidebarState {
            tags_expanded: true,
            tags: vec![
                Tag {
                    id: 1,
                    name: "work".into(),
                },
                Tag {
                    id: 2,
                    name: "personal".into(),
                },
                Tag {
                    id: 3,
                    name: "finance".into(),
                },
            ],
            tag_metadata: HashMap::from([
                (
                    1,
                    TagSortMeta {
                        record_count: 0,
                        last_used_at: 1000,
                    },
                ),
                (
                    2,
                    TagSortMeta {
                        record_count: 0,
                        last_used_at: 3000,
                    },
                ),
                (
                    3,
                    TagSortMeta {
                        record_count: 0,
                        last_used_at: 500,
                    },
                ),
            ]),
            ..Default::default()
        };
        sidebar.tag_management.sort_order = TagSortOrder::RecentlyUsed;
        sort_sidebar_tags(&mut sidebar);

        let names: Vec<&str> = sidebar.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["personal", "work", "finance"]); // 3000, 1000, 500 descending
    }

    #[test]
    fn sort_tags_by_recently_used_zero_goes_last() {
        let mut sidebar = SidebarState {
            tags_expanded: true,
            tags: vec![
                Tag {
                    id: 1,
                    name: "work".into(),
                },
                Tag {
                    id: 2,
                    name: "unused".into(),
                },
                Tag {
                    id: 3,
                    name: "personal".into(),
                },
            ],
            tag_metadata: HashMap::from([
                (
                    1,
                    TagSortMeta {
                        record_count: 1,
                        last_used_at: 1000,
                    },
                ),
                (
                    2,
                    TagSortMeta {
                        record_count: 0,
                        last_used_at: 0,
                    },
                ),
                (
                    3,
                    TagSortMeta {
                        record_count: 1,
                        last_used_at: 500,
                    },
                ),
            ]),
            ..Default::default()
        };
        sidebar.tag_management.sort_order = TagSortOrder::RecentlyUsed;
        sort_sidebar_tags(&mut sidebar);

        let names: Vec<&str> = sidebar.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["work", "personal", "unused"]); // 0 goes last, tiebreak by name
    }

    #[test]
    fn sort_tags_alphabetical_case_insensitive() {
        let mut sidebar = SidebarState {
            tags_expanded: true,
            tags: vec![
                Tag {
                    id: 1,
                    name: "Zebra".into(),
                },
                Tag {
                    id: 2,
                    name: "alpha".into(),
                },
                Tag {
                    id: 3,
                    name: "Beta".into(),
                },
            ],
            tag_metadata: HashMap::new(),
            ..Default::default()
        };
        sidebar.tag_management.sort_order = TagSortOrder::Alphabetical;
        sort_sidebar_tags(&mut sidebar);

        let names: Vec<&str> = sidebar.tags.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "Beta", "Zebra"]); // case-insensitive
    }
}
