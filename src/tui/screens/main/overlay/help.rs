//! Help Cheatsheet overlay — keyboard shortcut reference.
//!
//! Renders a centred overlay with 7 shortcut groups arranged in up to 2 columns.
//! Three responsive breakpoints control layout and which groups are visible.

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::t;
use crate::tui::theme;

// ── Colour constants ──────────────────────────────────────────

const OVERLAY_BG: Color = Color::Rgb(31, 35, 53); // #1f2335

// ── Data model ────────────────────────────────────────────────

/// A single keyboard shortcut (key binding + description).
struct Shortcut {
    key: String,
    desc: String,
}

/// A named group of related shortcuts.
struct ShortcutGroup {
    label: String,
    shortcuts: Vec<Shortcut>,
}

/// Indices of groups hidden in compact (single-column) layout.
/// Group 3 = "回收站", Group 5 = "标签管理".
const COMPACT_HIDDEN: &[usize] = &[3, 5];

// ── Group definitions ─────────────────────────────────────────

fn all_groups() -> Vec<ShortcutGroup> {
    vec![
        // 0 — Navigation
        ShortcutGroup {
            label: t!("tui.help.category_nav").to_string(),
            shortcuts: vec![
                Shortcut {
                    key: "↑/k".to_string(),
                    desc: t!("tui.help.nav_up").to_string(),
                },
                Shortcut {
                    key: "↓/j".to_string(),
                    desc: t!("tui.help.nav_down").to_string(),
                },
                Shortcut {
                    key: "Tab".to_string(),
                    desc: t!("tui.help.nav_tab").to_string(),
                },
                Shortcut {
                    key: "Enter".to_string(),
                    desc: t!("tui.help.nav_enter").to_string(),
                },
                Shortcut {
                    key: "Esc".to_string(),
                    desc: t!("tui.help.nav_esc").to_string(),
                },
            ],
        },
        // 1 — Actions
        ShortcutGroup {
            label: t!("tui.help.category_action").to_string(),
            shortcuts: vec![
                Shortcut {
                    key: "Ctrl+K".to_string(),
                    desc: t!("tui.help.action_search").to_string(),
                },
                Shortcut {
                    key: "n".to_string(),
                    desc: t!("tui.help.action_new").to_string(),
                },
                Shortcut {
                    key: "e".to_string(),
                    desc: t!("tui.help.action_edit").to_string(),
                },
                Shortcut {
                    key: "d".to_string(),
                    desc: t!("tui.help.action_delete").to_string(),
                },
                Shortcut {
                    key: "f".to_string(),
                    desc: t!("tui.help.action_favorite").to_string(),
                },
                Shortcut {
                    key: "s".to_string(),
                    desc: t!("tui.help.action_sort").to_string(),
                },
                Shortcut {
                    key: "v".to_string(),
                    desc: t!("tui.help.action_multiselect").to_string(),
                },
                Shortcut {
                    key: "g".to_string(),
                    desc: t!("tui.help.action_config").to_string(),
                },
                Shortcut {
                    key: "l".to_string(),
                    desc: t!("tui.help.action_audit").to_string(),
                },
                Shortcut {
                    key: "q".to_string(),
                    desc: t!("tui.help.action_quit").to_string(),
                },
            ],
        },
        // 2 — Password Details
        ShortcutGroup {
            label: t!("tui.help.section_password_details").to_string(),
            shortcuts: vec![
                Shortcut {
                    key: "c".to_string(),
                    desc: t!("tui.help.pwd_copy").to_string(),
                },
                Shortcut {
                    key: "u".to_string(),
                    desc: t!("tui.help.pwd_copy_username").to_string(),
                },
                Shortcut {
                    key: "p".to_string(),
                    desc: t!("tui.help.pwd_toggle").to_string(),
                },
                Shortcut {
                    key: "H".to_string(),
                    desc: t!("tui.help.pwd_history").to_string(),
                },
            ],
        },
        // 3 — Trash
        ShortcutGroup {
            label: t!("tui.help.section_trash").to_string(),
            shortcuts: vec![
                Shortcut {
                    key: "r".to_string(),
                    desc: t!("tui.help.trash_restore").to_string(),
                },
                Shortcut {
                    key: "D".to_string(),
                    desc: t!("tui.help.trash_permanent_delete").to_string(),
                },
                Shortcut {
                    key: "a".to_string(),
                    desc: t!("tui.help.trash_empty").to_string(),
                },
            ],
        },
        // 4 — Multiselect(v)
        ShortcutGroup {
            label: t!("tui.help.section_multiselect").to_string(),
            shortcuts: vec![
                Shortcut {
                    key: "Space".to_string(),
                    desc: t!("tui.help.ms_toggle").to_string(),
                },
                Shortcut {
                    key: "j/k".to_string(),
                    desc: t!("tui.help.ms_move").to_string(),
                },
                Shortcut {
                    key: "a".to_string(),
                    desc: t!("tui.help.ms_select_all").to_string(),
                },
                Shortcut {
                    key: "d".to_string(),
                    desc: t!("tui.help.ms_batch_delete").to_string(),
                },
                Shortcut {
                    key: "t".to_string(),
                    desc: t!("tui.help.ms_batch_tag").to_string(),
                },
                Shortcut {
                    key: "Esc/v".to_string(),
                    desc: t!("tui.help.ms_exit").to_string(),
                },
            ],
        },
        // 5 — Tag Management(m)
        ShortcutGroup {
            label: t!("tui.help.section_tags").to_string(),
            shortcuts: vec![
                Shortcut {
                    key: "m".to_string(),
                    desc: t!("tui.help.tag_add").to_string(),
                },
                Shortcut {
                    key: "r".to_string(),
                    desc: t!("tui.help.tag_rename").to_string(),
                },
                Shortcut {
                    key: "d".to_string(),
                    desc: t!("tui.help.tag_delete").to_string(),
                },
                Shortcut {
                    key: "s".to_string(),
                    desc: t!("tui.help.tag_save").to_string(),
                },
            ],
        },
        // 6 — Sync
        ShortcutGroup {
            label: t!("tui.help.section_sync").to_string(),
            shortcuts: vec![Shortcut {
                key: "Ctrl+R".to_string(),
                desc: t!("tui.help.sync_manual").to_string(),
            }],
        },
    ]
}

// ── Public API ────────────────────────────────────────────────

/// Render the Help Cheatsheet overlay, centred within `area`.
pub fn render_help(frame: &mut Frame, area: Rect) {
    let groups = all_groups();
    let (overlay_rect, visible_indices) = layout_for(area);

    // Build all lines for the overlay body.
    let mut lines = render_groups(&groups, &visible_indices, overlay_rect.width);

    // Footer hint line.
    lines.push(Line::from(Span::styled(
        format!(" {} ", t!("tui.help.close_hint")),
        Style::default().fg(theme::TEXT_SECONDARY),
    )));

    let title = format!(" {} ", t!("tui.help.title"));
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(OVERLAY_BG));

    let paragraph = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false });

    // Clear behind the overlay first.
    frame.render_widget(Clear, overlay_rect);
    frame.render_widget(paragraph, overlay_rect);
}

// ── Layout helpers ────────────────────────────────────────────

/// Choose overlay dimensions and visible group indices based on terminal width.
fn layout_for(area: Rect) -> (Rect, Vec<usize>) {
    let group_count = 7;
    let all_indices: Vec<usize> = (0..group_count).collect();

    if area.width >= 120 {
        // Two-column, full width.
        let w: u16 = 64;
        let h: u16 = 30;
        (centered_rect(area, w, h), all_indices)
    } else if area.width >= 100 {
        // Two-column, narrower.
        let w: u16 = 56;
        let h: u16 = 30;
        (centered_rect(area, w, h), all_indices)
    } else {
        // Single column — hide Trash and Tag Management.
        let w: u16 = 44;
        let h: u16 = 28;
        let visible: Vec<usize> = (0..group_count)
            .filter(|i| !COMPACT_HIDDEN.contains(i))
            .collect();
        (centered_rect(area, w, h), visible)
    }
}

/// Return a `Rect` of size `width x height` centred inside `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    super::centered_rect(area, width, height)
}

// ── Rendering helpers ─────────────────────────────────────────

/// Build the body lines for all visible groups.
fn render_groups(
    groups: &[ShortcutGroup],
    visible: &[usize],
    content_width: u16,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Determine column layout.
    let use_two_columns = content_width >= 56;
    let mid = visible.len().div_ceil(2);
    let (left, right): (&[_], &[_]) = visible.split_at(mid);

    if use_two_columns && !right.is_empty() {
        let left_lines = column_lines(groups, left);
        let right_lines = column_lines(groups, right);

        let col_width = ((content_width as usize).saturating_sub(1)) / 2; // leave 1 for separator
        let max_rows = left_lines.len().max(right_lines.len());

        for row in 0..max_rows {
            let l = left_lines.get(row);
            let r = right_lines.get(row);

            let left_span: Vec<Span<'static>> = match l {
                Some(line) => line.spans.clone(),
                None => vec![Span::raw(" ")],
            };

            let sep = Span::styled("│", Style::default().fg(theme::BORDER));

            let right_span: Vec<Span<'static>> = match r {
                Some(line) => line.spans.clone(),
                None => vec![Span::raw(" ")],
            };

            // Pad left column to col_width.
            let left_len: usize = left_span.iter().map(|s| s.content.chars().count()).sum();
            let padding = col_width.saturating_sub(left_len);

            let mut spans = left_span;
            spans.push(Span::raw(" ".repeat(padding)));
            spans.push(sep);
            spans.push(Span::raw(" "));
            spans.extend(right_span);

            lines.push(Line::from(spans));
        }
    } else {
        // Single column.
        for &idx in visible {
            append_group_lines(&mut lines, &groups[idx]);
        }
    }

    lines
}

/// Build lines for a single column of groups.
fn column_lines(groups: &[ShortcutGroup], indices: &[usize]) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for &idx in indices {
        append_group_lines(&mut lines, &groups[idx]);
    }
    lines
}

/// Append a group header + shortcut lines into `lines`.
fn append_group_lines(lines: &mut Vec<Line<'static>>, group: &ShortcutGroup) {
    // Group header.
    lines.push(Line::from(Span::styled(
        format!(" {} ", group.label),
        Style::default()
            .fg(theme::TEXT_SECONDARY)
            .add_modifier(Modifier::BOLD),
    )));

    // Shortcut lines.
    for sc in &group.shortcuts {
        lines.push(format_line_spans(&sc.key, &sc.desc));
    }

    // Blank line after group.
    lines.push(Line::from(Span::raw(" ")));
}

/// Build a single shortcut line: `  key   description`.
fn format_line_spans(key: &str, desc: &str) -> Line<'static> {
    let key_col_width = 10;
    let key_padded = format!("{key:width$}", width = key_col_width);
    Line::from(vec![
        Span::raw("  "),
        Span::styled(key_padded, Style::default().fg(theme::PRIMARY)),
        Span::styled(desc.to_string(), Style::default().fg(theme::TEXT)),
    ])
}

// ── Tests ─────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_groups_count() {
        let groups = all_groups();
        assert_eq!(groups.len(), 7, "expected exactly 7 shortcut groups");
    }

    #[test]
    fn compact_hidden_indices_valid() {
        let groups = all_groups();
        for &idx in COMPACT_HIDDEN {
            assert!(
                idx < groups.len(),
                "COMPACT_HIDDEN index {idx} out of bounds (max {})",
                groups.len() - 1,
            );
        }
        // Also check that the indices are distinct.
        let mut sorted = COMPACT_HIDDEN.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            COMPACT_HIDDEN.len(),
            "COMPACT_HIDDEN contains duplicate indices"
        );
    }

    #[test]
    fn groups_have_shortcuts() {
        let groups = all_groups();
        for (i, g) in groups.iter().enumerate() {
            assert!(
                !g.shortcuts.is_empty(),
                "group {i} ({}) has no shortcuts",
                g.label,
            );
        }
    }
}
