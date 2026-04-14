//! Sidebar panel rendering for the main screen.
//!
//! Renders the navigation sidebar with category items (All, Favorites, Expired,
//! HealthIssues, Trash), collapsible tag section, and utility shortcuts
//! (Generator, Config). Uses a ratatui `List` widget with `ListState` for
//! selection highlighting.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
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
        SidebarItem::Tag(name) => {
            let display = format!("{}#{}", TAG_INDENT, name);
            ListItem::new(Line::from(Span::styled(
                display,
                Style::default().fg(theme::TEXT),
            )))
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
}
