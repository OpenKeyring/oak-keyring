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

use crate::t;
use crate::tui::state::main_state::{SidebarCategory, SidebarItem, SidebarState};
use crate::tui::theme;

/// Separator character for unicode-capable terminals.
const SEPARATOR_UNICODE: char = '\u{2500}'; // ─
/// Separator character for ASCII-only terminals.
const SEPARATOR_ASCII: char = '-';

/// Indentation prefix for tag items.
const TAG_INDENT: &str = "  ";
const BADGE_RIGHT_PADDING: usize = 2;

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

        let footer_start = state
            .items
            .iter()
            .position(|item| matches!(item, SidebarItem::Generator))
            .and_then(|idx| idx.checked_sub(1))
            .unwrap_or(state.items.len());
        let footer_height = if state.items.len() > footer_start {
            (state.items.len() - footer_start).min(area.height as usize) as u16
        } else {
            0
        };
        let nav_area = Rect::new(
            area.x,
            area.y,
            area.width,
            area.height.saturating_sub(footer_height),
        );
        let footer_area = Rect::new(
            area.x,
            area.y + area.height.saturating_sub(footer_height),
            area.width,
            footer_height,
        );

        let items: Vec<ListItem<'_>> = state.items[..footer_start]
            .iter()
            .map(|item| build_list_item(item, state, unicode, area.width))
            .collect();

        let list = List::new(items);

        let mut list_state = ListState::default();
        list_state.select((state.selected_index < footer_start).then_some(state.selected_index));

        frame.render_stateful_widget(list, nav_area, &mut list_state);

        if footer_height > 0 {
            let footer_items: Vec<ListItem<'_>> = state.items[footer_start..]
                .iter()
                .map(|item| build_list_item(item, state, unicode, area.width))
                .collect();
            let footer_list = List::new(footer_items);
            let mut footer_state = ListState::default();
            footer_state.select(
                (state.selected_index >= footer_start).then(|| state.selected_index - footer_start),
            );
            frame.render_stateful_widget(footer_list, footer_area, &mut footer_state);
        }

        // Read the scroll offset after rendering to position inline rename correctly
        let list_offset = list_state.offset();

        // Render inline rename edit box overlay if active
        if state.tag_management_mode {
            if let Some(ref edit) = state.tag_management.inline_edit {
                render_inline_rename(frame, area, state, edit, unicode, list_offset);
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
        SidebarItem::Spacer => ListItem::new(Line::from("")),
        SidebarItem::Brand => ListItem::new(Line::from(Span::styled(
            if unicode {
                format!("  {} OpenKeyring", theme::ICON_LOCK)
            } else {
                format!("  {} OpenKeyring", theme::ascii::ICON_LOCK)
            },
            Style::default()
                .fg(theme::BRAND)
                .add_modifier(Modifier::BOLD),
        ))),
        SidebarItem::Category(category) => {
            let label = category_label(category, unicode);
            let count = category_count(category, &state.category_counts);

            if is_selected(item, state) {
                selected_category_item(label, count, area_width, unicode)
            } else {
                ListItem::new(category_line(label, count, area_width, unicode, false))
            }
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
                let down_icon = if unicode {
                    theme::ICON_DROPDOWN
                } else {
                    theme::ascii::ICON_DROPDOWN
                };
                let tags_label = t!("tui.main.sidebar_tags");
                let manage_label = t!("tui.main.sidebar_manage_mode");
                let sort_by_label = t!("tui.main.sidebar_sort_by", sort = &sort_label);
                let header_text = if unicode {
                    format!(
                        "\u{25BE} # {} ({}) {} {}",
                        tags_label, manage_label, sort_by_label, down_icon
                    )
                } else {
                    format!(
                        "v # {} ({}) {} {}",
                        tags_label, manage_label, sort_by_label, down_icon
                    )
                };
                if is_selected(item, state) {
                    selected_list_item(format!("  {}", header_text), area_width)
                } else {
                    ListItem::new(Line::from(Span::styled(
                        format!("  {}", header_text),
                        Style::default().fg(theme::TEXT_SECONDARY),
                    )))
                }
            } else {
                let (icon, label_key) = if unicode {
                    (
                        if state.tags_expanded {
                            "\u{25BE}"
                        } else {
                            "\u{25B8}"
                        },
                        if state.tags_expanded {
                            "tui.main.sidebar_tags"
                        } else {
                            "tui.main.sidebar_tags_collapsed"
                        },
                    )
                } else {
                    (
                        if state.tags_expanded { "v" } else { ">" },
                        if state.tags_expanded {
                            "tui.main.sidebar_tags"
                        } else {
                            "tui.main.sidebar_tags_collapsed"
                        },
                    )
                };
                let label = format!("{} # {}", icon, t!(label_key));
                if is_selected(item, state) {
                    selected_list_item(format!("  {}", label), area_width)
                } else {
                    ListItem::new(Line::from(Span::styled(
                        format!("  {}", label),
                        Style::default().fg(theme::TEXT_SECONDARY),
                    )))
                }
            }
        }
        SidebarItem::Tag(name, count) => {
            if state.tag_management_mode {
                let display = format!("{}{}", TAG_INDENT, name);
                let edit_icon = if unicode { "\u{270E}" } else { "[e]" };
                let delete_icon = if unicode { theme::ICON_ERROR } else { "[x]" };

                let name_chars = display.chars().count();
                let padding_width = (area_width as usize)
                    .saturating_sub(name_chars)
                    .saturating_sub(edit_icon.chars().count() + delete_icon.chars().count() + 4);

                if is_selected(item, state) {
                    selected_list_item(
                        format!("{}{} {}", display, " ".repeat(padding_width), edit_icon),
                        area_width,
                    )
                } else {
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
                }
            } else {
                let display = format!("{}{}{} ({})", TAG_INDENT, TAG_INDENT, name, count);
                if is_selected(item, state) {
                    selected_list_item(display, area_width)
                } else {
                    ListItem::new(Line::from(Span::styled(
                        display,
                        Style::default().fg(theme::TEXT),
                    )))
                }
            }
        }
        SidebarItem::Generator => {
            let label = format!("  {}", t!("tui.main.sidebar_generator"));
            if is_selected(item, state) {
                selected_list_item(label, area_width)
            } else {
                ListItem::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(theme::TEXT),
                )))
            }
        }
        SidebarItem::Config => {
            let label = format!("  {}", t!("tui.main.sidebar_config"));
            if is_selected(item, state) {
                selected_list_item(label, area_width)
            } else {
                ListItem::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(theme::TEXT),
                )))
            }
        }
    }
}

fn is_selected(item: &SidebarItem, state: &SidebarState) -> bool {
    item.is_selectable() && state.items.get(state.selected_index) == Some(item)
}

fn selected_list_item(text: String, area_width: u16) -> ListItem<'static> {
    let text_width = display_width(&text);
    let padding = (area_width as usize).saturating_sub(text_width);
    let full_text = format!("{}{}", text, " ".repeat(padding));
    let blank_text = " ".repeat(area_width as usize);
    let style = Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD);

    ListItem::new(vec![
        Line::from(Span::styled(blank_text.clone(), style)),
        Line::from(Span::styled(full_text, style)),
        Line::from(Span::styled(blank_text, style)),
    ])
    .style(style)
}

fn selected_category_item(
    label: String,
    count: usize,
    area_width: u16,
    unicode: bool,
) -> ListItem<'static> {
    let blank_text = " ".repeat(area_width as usize);
    let style = Style::default()
        .fg(theme::TEXT)
        .add_modifier(Modifier::REVERSED | Modifier::BOLD);

    ListItem::new(vec![
        Line::from(Span::styled(blank_text.clone(), style)),
        category_line(label, count, area_width, unicode, true),
        Line::from(Span::styled(blank_text, style)),
    ])
    .style(style)
}

fn category_line(
    label: String,
    count: usize,
    area_width: u16,
    unicode: bool,
    selected: bool,
) -> Line<'static> {
    let prefix = "  ";
    let badge = format_count(count, unicode);
    let used_width = display_width(prefix) + display_width(&label) + display_width(&badge);
    let padding_width = (area_width as usize)
        .saturating_sub(used_width)
        .saturating_sub(BADGE_RIGHT_PADDING);

    let text_style = if selected {
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT)
    };

    let mut spans = vec![
        Span::styled(prefix, text_style),
        Span::styled(label, text_style),
        Span::styled(" ".repeat(padding_width), text_style),
    ];
    spans.extend(count_badge_spans(count, unicode, selected));
    spans.push(Span::styled(" ".repeat(BADGE_RIGHT_PADDING), text_style));
    Line::from(spans)
}

fn display_width(text: &str) -> usize {
    Line::from(text.to_string()).width()
}

/// Return the display label for a sidebar category.
fn category_label(category: &SidebarCategory, unicode: bool) -> String {
    let _ = unicode;
    match category {
        SidebarCategory::All => format!("[1] {}", t!("tui.main.sidebar_all")),
        SidebarCategory::Favorites => format!("[2] {}", t!("tui.main.sidebar_favorites")),
        SidebarCategory::Expired => format!("[3] {}", t!("tui.main.sidebar_expired")),
        SidebarCategory::HealthIssues => format!("[4] {}", t!("tui.main.sidebar_health")),
        SidebarCategory::Trash => format!("[5] {}", t!("tui.main.sidebar_trash")),
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

/// Format a count as a compact badge string, capped at 99+.
fn format_count(count: usize, unicode: bool) -> String {
    let count_label = count_label(count);
    if unicode {
        format!("\u{e0b6}{}\u{e0b4}", count_label)
    } else {
        format!("[{}]", count_label)
    }
}

fn count_label(count: usize) -> String {
    if count > 99 {
        "99+".to_string()
    } else {
        count.to_string()
    }
}

fn count_badge_spans(count: usize, unicode: bool, _selected: bool) -> Vec<Span<'static>> {
    let label = count_label(count);
    let badge_reset = Modifier::REVERSED | Modifier::BOLD;
    let badge_edge_style = Style::default()
        .fg(theme::WARNING)
        .remove_modifier(badge_reset);
    let badge_text_style = Style::default()
        .fg(theme::BG)
        .bg(theme::WARNING)
        .add_modifier(Modifier::BOLD)
        .remove_modifier(Modifier::REVERSED);
    if unicode {
        vec![
            Span::styled("\u{e0b6}", badge_edge_style),
            Span::styled(label, badge_text_style),
            Span::styled("\u{e0b4}", badge_edge_style),
        ]
    } else {
        vec![Span::styled(format!("[{}]", label), badge_text_style)]
    }
}

/// Render the inline rename edit box overlay on top of the current tag item.
fn render_inline_rename(
    frame: &mut Frame,
    area: Rect,
    state: &SidebarState,
    edit: &crate::tui::state::tag_management::InlineEditState,
    unicode: bool,
    list_offset: usize,
) {
    // Find the visual row position of the currently selected tag
    let tag_idx = state.selected_index;
    if tag_idx == 0 || tag_idx >= state.items.len() {
        return;
    }

    // Calculate the y position for the overlay, accounting for scroll offset
    let y_offset = area.y + tag_idx.saturating_sub(list_offset) as u16;
    if y_offset >= area.y + area.height {
        return; // Out of visible area
    }

    let base_x = area.x + TAG_INDENT.len() as u16;
    let max_width = area.width.saturating_sub(TAG_INDENT.len() as u16);

    // Build the edit box text with cursor
    let text_before_cursor = &edit.text[..edit.cursor];
    let text_after_cursor = &edit.text[edit.cursor..];
    let cursor_char = if unicode {
        theme::ICON_PROGRESS_FILL
    } else {
        "_"
    };

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
            let error_icon = if unicode {
                theme::ICON_ERROR
            } else {
                theme::ascii::ICON_ERROR
            };
            let error_line = Line::from(Span::styled(
                format!(
                    "  {} {}",
                    error_icon,
                    t!("tui.form.validation_tag_exists", name = edit.text.trim())
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
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_sidebar(state: &SidebarState, width: u16, height: u16) -> String {
        let buffer = render_sidebar_buffer(state, width, height);
        format!("{:?}", buffer)
    }

    fn render_sidebar_buffer(
        state: &SidebarState,
        width: u16,
        height: u16,
    ) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                SidebarPanel::view(frame, frame.area(), state, true, true);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).expect("cell").symbol())
            .collect()
    }

    #[test]
    fn format_count_nonzero() {
        assert_eq!(format_count(42, true), "\u{e0b6}42\u{e0b4}");
    }

    #[test]
    fn format_count_zero() {
        assert_eq!(format_count(0, true), "\u{e0b6}0\u{e0b4}");
    }

    #[test]
    fn format_count_caps_at_99_plus() {
        assert_eq!(format_count(128, true), "\u{e0b6}99+\u{e0b4}");
        assert_eq!(format_count(128, false), "[99+]");
    }

    #[test]
    fn category_labels_unicode() {
        let all_label = category_label(&SidebarCategory::All, true);
        assert!(!all_label.is_empty());

        let fav_label = category_label(&SidebarCategory::Favorites, true);
        assert!(!fav_label.contains('\u{2606}'));
        assert_eq!(
            fav_label,
            format!("[2] {}", t!("tui.main.sidebar_favorites"))
        );

        let expired_label = category_label(&SidebarCategory::Expired, true);
        assert!(!expired_label.is_empty());

        let health_label = category_label(&SidebarCategory::HealthIssues, true);
        assert!(!health_label.is_empty());
    }

    #[test]
    fn sidebar_renders_branded_spacious_categories() {
        let mut state = SidebarState {
            category_counts: CategoryCounts {
                all: 128,
                favorites: 12,
                expired: 3,
                health_issues: 3,
                trash: 5,
            },
            ..Default::default()
        };
        state.select_category(SidebarCategory::All);
        state.rebuild();

        let rendered = render_sidebar(&state, 36, 24);

        assert!(rendered.contains("🔐 OpenKeyring"));
        assert!(rendered.contains("All"));
        assert!(rendered.contains("99+"));
        assert!(rendered.contains("Favorites"));
        assert!(rendered.contains("12"));
        assert!(!rendered.contains("◄"));
        assert!(!rendered.contains("☆"));
        assert!(!rendered.contains("🗑"));
    }

    #[test]
    fn category_badge_is_right_aligned_with_orange_fill() {
        let mut state = SidebarState {
            category_counts: CategoryCounts {
                all: 128,
                favorites: 12,
                expired: 3,
                health_issues: 3,
                trash: 5,
            },
            ..Default::default()
        };
        state.select_category(SidebarCategory::All);
        state.rebuild();

        let buffer = render_sidebar_buffer(&state, 36, 16);
        let row = row_text(&buffer, 7);

        assert!(
            row.ends_with("12  "),
            "badge should be right-aligned with two columns of right padding: {row:?}"
        );

        let left = buffer.cell((30, 7)).expect("badge left edge");
        let label = buffer.cell((31, 7)).expect("badge text");
        let right = buffer.cell((33, 7)).expect("badge right edge");
        assert_eq!(left.style().fg, Some(theme::WARNING));
        assert_eq!(label.style().bg, Some(theme::WARNING));
        assert_eq!(right.style().fg, Some(theme::WARNING));
    }

    #[test]
    fn selected_category_has_no_marker_and_keeps_badge_style() {
        let mut state = SidebarState {
            category_counts: CategoryCounts {
                all: 128,
                favorites: 12,
                expired: 3,
                health_issues: 3,
                trash: 5,
            },
            ..Default::default()
        };
        state.select_category(SidebarCategory::All);
        state.rebuild();

        let buffer = render_sidebar_buffer(&state, 36, 10);
        let selected_top = 3;
        let selected_center = 4;
        let selected_bottom = 5;
        let row = row_text(&buffer, selected_center);

        assert!(
            !row.contains('\u{25C4}'),
            "selected row should not show a marker"
        );
        assert!(
            row.ends_with("99+  "),
            "selected badge should be right-aligned with right padding: {row:?}"
        );

        let badge_left = buffer
            .cell((29, selected_center))
            .expect("selected badge left edge should exist");
        let badge_text = buffer
            .cell((30, selected_center))
            .expect("selected badge text should exist");
        let badge_right = buffer
            .cell((33, selected_center))
            .expect("selected badge right edge should exist");

        assert_eq!(badge_left.style().fg, Some(theme::WARNING));
        assert_eq!(badge_text.style().fg, Some(theme::BG));
        assert_eq!(badge_text.style().bg, Some(theme::WARNING));
        assert_eq!(badge_right.style().fg, Some(theme::WARNING));
        for (x, cell) in [(29, badge_left), (30, badge_text), (33, badge_right)] {
            assert!(
                !cell.style().add_modifier.contains(Modifier::REVERSED),
                "selected badge cell {} should keep normal badge styling",
                x
            );
        }

        for y in selected_top..=selected_bottom {
            for x in 0..36 {
                if y == selected_center && (29..=33).contains(&x) {
                    continue;
                }
                let cell = buffer
                    .cell((x, y))
                    .unwrap_or_else(|| panic!("cell ({}, {}) missing", x, y));
                assert!(
                    cell.style().add_modifier.contains(Modifier::REVERSED),
                    "selected block cell ({}, {}) should be reversed",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn category_labels_ascii() {
        let all_label = category_label(&SidebarCategory::All, false);
        assert!(!all_label.is_empty());

        let fav_label = category_label(&SidebarCategory::Favorites, false);
        assert!(!fav_label.contains('*'));

        let expired_label = category_label(&SidebarCategory::Expired, false);
        assert!(!expired_label.is_empty());

        let trash_label = category_label(&SidebarCategory::Trash, false);
        assert!(!trash_label.contains("[DEL]"));
        assert_eq!(trash_label, format!("[5] {}", t!("tui.main.sidebar_trash")));
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
    fn build_list_item_tag_has_content() {
        let state = SidebarState::default();
        let item = build_list_item(&SidebarItem::Tag("work".into(), 0), &state, true, 50);
        // Tag item should have non-zero width (indent + "work" + count)
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
            &SidebarItem::Brand,
            &SidebarItem::Category(SidebarCategory::All),
            &SidebarItem::Category(SidebarCategory::Favorites),
            &SidebarItem::Separator,
            &SidebarItem::TagHeader,
            &SidebarItem::Tag("test".into(), 0),
            &SidebarItem::Generator,
            &SidebarItem::Config,
        ];
        for item in items_to_test {
            let list_item = build_list_item(item, &state, true, 50);
            let expected_height = if is_selected(item, &state) { 3 } else { 1 };
            assert_eq!(
                list_item.height(),
                expected_height,
                "unexpected height for {:?}",
                item
            );
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
        // Check that the header contains some content (may be localized)
        assert!(
            result.contains("Tags") || result.contains('\u{6807}'), // Tags or 标签
            "header should contain tag label"
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

        let backend = ratatui::backend::TestBackend::new(40, 24);
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
        // Check that sort indicator is present (may be localized)
        assert!(
            result.contains("Sort") || result.contains('\u{6309}') || result.contains("按"),
            "should show sort indicator"
        );
    }

    #[test]
    fn inline_rename_accounts_for_scroll_offset() {
        use crate::tui::state::tag_management::InlineEditState;
        use crate::types::Tag;

        // Unique marker that does NOT appear in any tag name.
        const MARKER: &str = "RENAMING_VISIBLE_MARKER";

        // Build a sidebar with enough tags to fill a small terminal and cause scrolling.
        // Layout: Brand, Sep, All, Favorites, Expired, Health, Trash, Sep, TagHeader,
        // then 10 tags, Sep, Generator, Config = 24 items.
        let tags: Vec<Tag> = (0..10)
            .map(|i| Tag {
                id: i + 1,
                name: format!("tag_{:02}", i),
            })
            .collect();

        let mut state = SidebarState {
            tags_expanded: true,
            tags,
            tag_management_mode: true,
            tag_management: crate::tui::state::tag_management::TagManagementState {
                // Use the marker as the edit text so the overlay is distinguishable
                // from the regular tag list item text.
                inline_edit: Some(InlineEditState {
                    original_name: "tag_09".to_string(),
                    text: MARKER.to_string(),
                    cursor: MARKER.len(),
                    conflict: false,
                }),
                ..Default::default()
            },
            ..Default::default()
        };
        state.rebuild();

        // Select the last tag (index 18: Brand(0), Sep(1), All(2), Fav(3), Exp(4),
        // Health(5), Trash(6), Sep(7), TagHeader(8), tag_00(9)..tag_09(18))
        state.selected_index = 18;

        // Render into a short area (height 12) so the list must scroll.
        // With 24 items and height 12, offset should be > 0 after rendering.
        let backend = ratatui::backend::TestBackend::new(40, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                SidebarPanel::view(frame, frame.area(), &state, true, true);
            })
            .unwrap();

        let buf = terminal.backend().buffer().clone();
        let result = format!("{:?}", buf);

        // The overlay must render the unique marker in the visible area.
        // Without the scroll-offset fix, y_offset would be out of bounds and
        // the overlay would not be drawn at all.
        assert!(
            result.contains(MARKER),
            "inline rename overlay should be visible with scroll offset, \
             marker '{MARKER}' not found in rendered buffer"
        );

        // Verify the marker appears on a specific visible row.
        // Iterate rows using content() and area().
        let area = buf.area();
        let width = area.width as usize;
        let marker_row = (0..area.height as usize).find(|row| {
            let start = row * width;
            let end = start + width;
            let row_str: String = buf.content()[start..end]
                .iter()
                .map(|c| c.symbol())
                .collect();
            row_str.contains(MARKER)
        });
        assert!(
            marker_row.is_some(),
            "marker should appear in a visible row"
        );
        // The marker row must be within the rendered area
        let row = marker_row.unwrap();
        assert!(
            row < area.height as usize,
            "marker row {} should be within visible area (0..{})",
            row,
            area.height
        );
    }
}
