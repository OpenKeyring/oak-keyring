//! Sidebar panel rendering for the main screen.
//!
//! Renders the navigation sidebar with category items (All, Favorites, Expired,
//! HealthIssues, Trash), collapsible tag section, and utility shortcuts
//! (Generator, Config). Uses a ratatui `List` widget with `ListState` for
//! selection highlighting.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tui::state::main_state::{SidebarCategory, SidebarItem, SidebarState};
use crate::tui::theme;

/// Separator character for unicode-capable terminals.
const SEPARATOR_UNICODE: char = '\u{2500}'; // ─
/// Separator character for ASCII-only terminals.
const SEPARATOR_ASCII: char = '-';

/// Tag header label when expanded (unicode).
const TAG_HEADER_EXPANDED_UNICODE: &str = "\u{25BE} # \u{6807}\u{7B7E}"; // ▾ # 标签
/// Tag header label when collapsed (unicode).
const TAG_HEADER_COLLAPSED_UNICODE: &str = "\u{25B8} # \u{6807}\u{7B7E}"; // ▸ # 标签
/// Tag header label when expanded (ASCII fallback).
const TAG_HEADER_EXPANDED_ASCII: &str = "v # Tags";
/// Tag header label when collapsed (ASCII fallback).
const TAG_HEADER_COLLAPSED_ASCII: &str = "> # Tags";

/// Indentation prefix for tag items.
const TAG_INDENT: &str = "    ";

/// Generator label (unicode).
const GENERATOR_LABEL_UNICODE: &str = "\u{2726} \u{751F}\u{6210}\u{5668}"; // ✦ 生成器
/// Config label (unicode).
const CONFIG_LABEL_UNICODE: &str = "\u{2726} \u{914D}\u{7F6E}"; // ✦ 配置
/// Generator label (ASCII fallback).
const GENERATOR_LABEL_ASCII: &str = "* Generator";
/// Config label (ASCII fallback).
const CONFIG_LABEL_ASCII: &str = "* Config";

/// Panel responsible for rendering the sidebar navigation.
pub struct SidebarPanel;

impl SidebarPanel {
    /// Render the sidebar into the given frame area.
    ///
    /// # Arguments
    /// * `frame` - The ratatui frame to render into.
    /// * `area` - The rectangular area allocated to the sidebar.
    /// * `state` - The current sidebar state (items, selection, counts).
    /// * `focused` - Whether the sidebar currently has keyboard focus.
    /// * `unicode` - Whether to use unicode characters (vs ASCII fallbacks).
    pub fn view(
        frame: &mut Frame,
        area: Rect,
        state: &SidebarState,
        _focused: bool,
        unicode: bool,
    ) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let items: Vec<ListItem<'_>> = state
            .items
            .iter()
            .map(|item| build_list_item(item, state, unicode, area.width))
            .collect();

        let highlight_style = Style::default().add_modifier(Modifier::REVERSED | Modifier::BOLD);

        let list = List::new(items).highlight_style(highlight_style);

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_index));

        frame.render_stateful_widget(list, area, &mut list_state);

        // Render inline rename edit box overlay if active
        if state.tag_management_mode {
            if let Some(ref edit) = state.tag_management.inline_edit {
                render_inline_rename(frame, area, state, edit, unicode);
            }
        }
    }
}

/// Build a single `ListItem` from a `SidebarItem`.
fn build_list_item<'a>(
    item: &SidebarItem,
    state: &SidebarState,
    unicode: bool,
    area_width: u16,
) -> ListItem<'a> {
    match item {
        SidebarItem::Category(category) => {
            let label = category_label(category, unicode);
            let count = category_count(category, &state.category_counts);
            let count_str = format_count(count);

            ListItem::new(Line::from(vec![
                Span::styled(label, Style::default().fg(theme::TEXT)),
                Span::styled(count_str, Style::default().fg(theme::TEXT_SECONDARY)),
            ]))
        }
        SidebarItem::Separator => {
            let sep_char = if unicode {
                SEPARATOR_UNICODE
            } else {
                SEPARATOR_ASCII
            };
            let width = area_width as usize;
            let sep_text: String = std::iter::repeat_n(sep_char, width).collect();
            ListItem::new(Line::from(Span::styled(
                sep_text,
                Style::default().fg(theme::BORDER),
            )))
        }
        SidebarItem::TagHeader => {
            if state.tag_management_mode {
                let sort_label = state.tag_management.sort_order.label();
                let down_icon = if unicode { '\u{25BC}' } else { 'v' };
                let header_text = if unicode {
                    format!(
                        "\u{25BE} # \u{6807}\u{7B7E} (\u{7BA1}\u{7406}\u{6A21}\u{5F0F}) \u{6309}: {} {}",
                        sort_label, down_icon
                    )
                } else {
                    format!("v # Tags (manage) by: {} {}", sort_label, down_icon)
                };
                ListItem::new(Line::from(Span::styled(
                    header_text,
                    Style::default().fg(theme::TEXT_SECONDARY),
                )))
            } else {
                let label = if unicode {
                    if state.tags_expanded {
                        TAG_HEADER_EXPANDED_UNICODE
                    } else {
                        TAG_HEADER_COLLAPSED_UNICODE
                    }
                } else if state.tags_expanded {
                    TAG_HEADER_EXPANDED_ASCII
                } else {
                    TAG_HEADER_COLLAPSED_ASCII
                };

                ListItem::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(theme::TEXT_SECONDARY),
                )))
            }
        }
        SidebarItem::Tag(name) => {
            if state.tag_management_mode {
                let display = format!("{}#{}", TAG_INDENT, name);
                let edit_icon = if unicode { "\u{270E}" } else { "[e]" };
                let delete_icon = if unicode { "\u{2717}" } else { "[x]" };

                let name_chars = display.chars().count();
                let padding_width = (area_width as usize)
                    .saturating_sub(name_chars)
                    .saturating_sub(edit_icon.chars().count() + delete_icon.chars().count() + 4);

                ListItem::new(Line::from(vec![
                    Span::styled(display, Style::default().fg(theme::TEXT)),
                    Span::styled(" ".repeat(padding_width), Style::default().fg(theme::TEXT)),
                    Span::styled(
                        format!(" {}", edit_icon),
                        Style::default().fg(theme::PRIMARY),
                    ),
                    Span::styled(
                        format!(" {}", delete_icon),
                        Style::default().fg(theme::ERROR),
                    ),
                ]))
            } else {
                let display = format!("{}#{}", TAG_INDENT, name);
                ListItem::new(Line::from(Span::styled(
                    display,
                    Style::default().fg(theme::TEXT),
                )))
            }
        }
        SidebarItem::Generator => {
            let label = if unicode {
                GENERATOR_LABEL_UNICODE
            } else {
                GENERATOR_LABEL_ASCII
            };
            ListItem::new(Line::from(Span::styled(
                label,
                Style::default().fg(theme::BRAND),
            )))
        }
        SidebarItem::Config => {
            let label = if unicode {
                CONFIG_LABEL_UNICODE
            } else {
                CONFIG_LABEL_ASCII
            };
            ListItem::new(Line::from(Span::styled(
                label,
                Style::default().fg(theme::BRAND),
            )))
        }
    }
}

/// Return the display label for a sidebar category.
fn category_label(category: &SidebarCategory, unicode: bool) -> &'static str {
    match category {
        SidebarCategory::All => "所有",
        SidebarCategory::Favorites => {
            if unicode {
                "\u{2606} \u{6536}\u{85CF}" // ☆ 收藏
            } else {
                "* Favorites"
            }
        }
        SidebarCategory::Expired => "已过期",
        SidebarCategory::HealthIssues => "健康问题",
        SidebarCategory::Trash => {
            if unicode {
                "\u{1F5D1} \u{56DE}\u{6536}\u{7AD9}" // 🗑 回收站
            } else {
                "[DEL] Trash"
            }
        }
    }
}

/// Look up the record count for a category.
fn category_count(
    category: &SidebarCategory,
    counts: &crate::tui::state::main_state::CategoryCounts,
) -> usize {
    match category {
        SidebarCategory::All => counts.all,
        SidebarCategory::Favorites => counts.favorites,
        SidebarCategory::Expired => counts.expired,
        SidebarCategory::HealthIssues => counts.health_issues,
        SidebarCategory::Trash => counts.trash,
    }
}

/// Format a count as a right-aligned badge string (e.g. " 42").
fn format_count(count: usize) -> String {
    if count > 0 {
        format!(" {}", count)
    } else {
        String::new()
    }
}

/// Render the inline rename edit box overlay on top of the current tag item.
fn render_inline_rename(
    frame: &mut Frame,
    area: Rect,
    state: &SidebarState,
    edit: &crate::tui::state::tag_management::InlineEditState,
    unicode: bool,
) {
    // Find the visual row position of the currently selected tag
    let tag_idx = state.selected_index;
    if tag_idx == 0 || tag_idx >= state.items.len() {
        return;
    }

    // Calculate the y position for the overlay
    let y_offset = area.y + tag_idx as u16;
    if y_offset >= area.y + area.height {
        return; // Out of visible area
    }

    let base_x = area.x + TAG_INDENT.len() as u16;
    let max_width = area.width.saturating_sub(TAG_INDENT.len() as u16);

    // Build the edit box text with cursor
    let text_before_cursor = &edit.text[..edit.cursor];
    let text_after_cursor = &edit.text[edit.cursor..];
    let cursor_char = if unicode { "\u{2588}" } else { "_" };

    let edit_line = Line::from(vec![
        Span::styled(
            text_before_cursor.to_string(),
            Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE),
        ),
        Span::styled(
            cursor_char.to_string(),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::REVERSED)
                .bg(theme::BG_SURFACE),
        ),
        Span::styled(
            text_after_cursor.to_string(),
            Style::default().fg(theme::TEXT).bg(theme::BG_SURFACE),
        ),
    ]);

    let edit_area = Rect::new(base_x, y_offset, max_width.min(30), 1);
    let para = Paragraph::new(edit_line);
    frame.render_widget(para, edit_area);

    // Conflict error line (render below if space available)
    if edit.conflict {
        let error_y = y_offset + 1;
        if error_y < area.y + area.height {
            let error_icon = if unicode { "\u{2717}" } else { "x" };
            let error_line = Line::from(Span::styled(
                format!(
                    "  {} \u{6807}\u{7B7E}\"{}\"\u{5DF2}\u{5B58}\u{5728}",
                    error_icon,
                    edit.text.trim()
                ),
                Style::default().fg(theme::ERROR),
            ));
            let error_area = Rect::new(area.x, error_y, area.width.min(30), 1);
            let error_para = Paragraph::new(error_line);
            frame.render_widget(error_para, error_area);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::main_state::{CategoryCounts, SidebarState};

    #[test]
    fn format_count_nonzero() {
        assert_eq!(format_count(42), " 42");
    }

    #[test]
    fn format_count_zero() {
        assert_eq!(format_count(0), "");
    }

    #[test]
    fn category_labels_unicode() {
        assert_eq!(category_label(&SidebarCategory::All, true), "所有");
        assert_eq!(
            category_label(&SidebarCategory::Favorites, true),
            "\u{2606} \u{6536}\u{85CF}"
        );
        assert_eq!(category_label(&SidebarCategory::Expired, true), "已过期");
        assert_eq!(
            category_label(&SidebarCategory::HealthIssues, true),
            "健康问题"
        );
    }

    #[test]
    fn category_labels_ascii() {
        assert_eq!(category_label(&SidebarCategory::All, false), "所有");
        assert_eq!(
            category_label(&SidebarCategory::Favorites, false),
            "* Favorites"
        );
        assert_eq!(category_label(&SidebarCategory::Expired, false), "已过期");
        assert_eq!(
            category_label(&SidebarCategory::Trash, false),
            "[DEL] Trash"
        );
    }

    #[test]
    fn category_count_lookup() {
        let counts = CategoryCounts {
            all: 10,
            favorites: 3,
            expired: 1,
            health_issues: 0,
            trash: 2,
        };
        assert_eq!(category_count(&SidebarCategory::All, &counts), 10);
        assert_eq!(category_count(&SidebarCategory::Favorites, &counts), 3);
        assert_eq!(category_count(&SidebarCategory::Expired, &counts), 1);
        assert_eq!(category_count(&SidebarCategory::HealthIssues, &counts), 0);
        assert_eq!(category_count(&SidebarCategory::Trash, &counts), 2);
    }

    #[test]
    fn build_list_item_separator_unicode_width() {
        let state = SidebarState::default();
        let item = build_list_item(&SidebarItem::Separator, &state, true, 30);
        // Separator should span the full width
        assert_eq!(item.width(), 30);
        assert_eq!(item.height(), 1);
    }

    #[test]
    fn build_list_item_separator_ascii_width() {
        let state = SidebarState::default();
        let item = build_list_item(&SidebarItem::Separator, &state, false, 25);
        assert_eq!(item.width(), 25);
    }

    #[test]
    fn tag_header_label_expanded() {
        assert!(TAG_HEADER_EXPANDED_UNICODE.starts_with('\u{25BE}')); // ▾
        assert!(TAG_HEADER_EXPANDED_ASCII.starts_with('v'));
    }

    #[test]
    fn tag_header_label_collapsed() {
        assert!(TAG_HEADER_COLLAPSED_UNICODE.starts_with('\u{25B8}')); // ▸
        assert!(TAG_HEADER_COLLAPSED_ASCII.starts_with('>'));
    }

    #[test]
    fn build_list_item_tag_has_content() {
        let state = SidebarState::default();
        let item = build_list_item(&SidebarItem::Tag("work".into()), &state, true, 50);
        // Tag item should have non-zero width (indent + "#work")
        assert!(item.width() > 0);
    }

    #[test]
    fn build_list_item_generator_has_content() {
        let state = SidebarState::default();
        let item = build_list_item(&SidebarItem::Generator, &state, true, 50);
        assert!(item.width() > 0);
    }

    #[test]
    fn build_list_item_config_has_content() {
        let state = SidebarState::default();
        let item = build_list_item(&SidebarItem::Config, &state, false, 50);
        assert!(item.width() > 0);
    }

    #[test]
    fn build_list_item_all_items_single_height() {
        let state = SidebarState::default();
        let items_to_test = [
            &SidebarItem::Category(SidebarCategory::All),
            &SidebarItem::Category(SidebarCategory::Favorites),
            &SidebarItem::Separator,
            &SidebarItem::TagHeader,
            &SidebarItem::Tag("test".into()),
            &SidebarItem::Generator,
            &SidebarItem::Config,
        ];
        for item in items_to_test {
            let list_item = build_list_item(item, &state, true, 50);
            assert_eq!(list_item.height(), 1, "expected height 1 for {:?}", item);
        }
    }

    #[test]
    fn separator_fills_full_width() {
        let state = SidebarState::default();
        for width in [10u16, 30, 50] {
            let item = build_list_item(&SidebarItem::Separator, &state, true, width);
            assert_eq!(
                item.width(),
                width as usize,
                "separator should fill width {}",
                width
            );
        }
    }

    #[test]
    fn tag_management_mode_changes_header() {
        use crate::types::Tag;

        let mut state = SidebarState {
            tags_expanded: true,
            tags: vec![Tag {
                id: 1,
                name: "work".to_string(),
            }],
            tag_management_mode: true,
            ..Default::default()
        };
        state.rebuild();

        let backend = ratatui::backend::TestBackend::new(30, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                SidebarPanel::view(frame, frame.area(), &state, true, true);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let result = format!("{:?}", buf);
        assert!(
            result.contains("\u{7BA1}\u{7406}\u{6A21}\u{5F0F}"),
            "header should contain '管理模式'"
        );
    }

    #[test]
    fn tag_management_shows_edit_icon() {
        use crate::types::Tag;

        let mut state = SidebarState {
            tags_expanded: true,
            tags: vec![Tag {
                id: 1,
                name: "work".to_string(),
            }],
            tag_management_mode: true,
            ..Default::default()
        };
        state.rebuild();

        let backend = ratatui::backend::TestBackend::new(40, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                SidebarPanel::view(frame, frame.area(), &state, true, true);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let result = format!("{:?}", buf);
        assert!(result.contains('\u{270E}'), "should show edit icon");
    }

    #[test]
    fn tag_management_sort_indicator() {
        use crate::tui::state::tag_management::{TagManagementState, TagSortOrder};
        use crate::types::Tag;

        let mut state = SidebarState {
            tags_expanded: true,
            tags: vec![Tag {
                id: 1,
                name: "work".to_string(),
            }],
            tag_management_mode: true,
            tag_management: TagManagementState {
                sort_order: TagSortOrder::Alphabetical,
                ..Default::default()
            },
            ..Default::default()
        };
        state.rebuild();

        let backend = ratatui::backend::TestBackend::new(40, 20);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                SidebarPanel::view(frame, frame.area(), &state, true, true);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let result = format!("{:?}", buf);
        assert!(
            result.contains("\u{540D}\u{79F0}"),
            "should show sort order label '名称'"
        );
    }
}
