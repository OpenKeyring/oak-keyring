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

use crate::tui::theme;

// ── Colour constants ──────────────────────────────────────────

const OVERLAY_BG: Color = Color::Rgb(26, 27, 38); // #1a1b26

// ── Data model ────────────────────────────────────────────────

/// A single keyboard shortcut (key binding + description).
struct Shortcut {
    key: &'static str,
    desc: &'static str,
}

/// A named group of related shortcuts.
struct ShortcutGroup {
    label: &'static str,
    shortcuts: &'static [Shortcut],
}

/// Indices of groups hidden in compact (single-column) layout.
/// Group 3 = "回收站", Group 5 = "标签管理".
const COMPACT_HIDDEN: &[usize] = &[3, 5];

// ── Group definitions ─────────────────────────────────────────

fn all_groups() -> [ShortcutGroup; 7] {
    [
        // 0 — 导航
        ShortcutGroup {
            label: "导航",
            shortcuts: &[
                Shortcut {
                    key: "↑/k",
                    desc: "上移",
                },
                Shortcut {
                    key: "↓/j",
                    desc: "下移",
                },
                Shortcut {
                    key: "Tab",
                    desc: "切换面板",
                },
                Shortcut {
                    key: "Enter",
                    desc: "确认",
                },
                Shortcut {
                    key: "Esc",
                    desc: "返回/关闭",
                },
            ],
        },
        // 1 — 操作
        ShortcutGroup {
            label: "操作",
            shortcuts: &[
                Shortcut {
                    key: "Ctrl+K",
                    desc: "搜索",
                },
                Shortcut {
                    key: "n",
                    desc: "新建",
                },
                Shortcut {
                    key: "e",
                    desc: "编辑",
                },
                Shortcut {
                    key: "d",
                    desc: "删除",
                },
                Shortcut {
                    key: "f",
                    desc: "收藏",
                },
                Shortcut {
                    key: "s",
                    desc: "排序",
                },
                Shortcut {
                    key: "v",
                    desc: "多选模式",
                },
                Shortcut {
                    key: "g",
                    desc: "密码生成",
                },
                Shortcut {
                    key: "L",
                    desc: "锁定",
                },
                Shortcut {
                    key: "q",
                    desc: "退出",
                },
            ],
        },
        // 2 — 密码详情
        ShortcutGroup {
            label: "密码详情",
            shortcuts: &[
                Shortcut {
                    key: "c",
                    desc: "复制密码",
                },
                Shortcut {
                    key: "u",
                    desc: "复制用户名",
                },
                Shortcut {
                    key: "p",
                    desc: "显示/隐藏密码",
                },
                Shortcut {
                    key: "H",
                    desc: "密码历史",
                },
            ],
        },
        // 3 — 回收站
        ShortcutGroup {
            label: "回收站",
            shortcuts: &[
                Shortcut {
                    key: "r",
                    desc: "恢复记录",
                },
                Shortcut {
                    key: "D",
                    desc: "永久删除",
                },
                Shortcut {
                    key: "a",
                    desc: "清空回收站",
                },
            ],
        },
        // 4 — 多选模式(v)
        ShortcutGroup {
            label: "多选模式(v)",
            shortcuts: &[
                Shortcut {
                    key: "Space",
                    desc: "选中/取消",
                },
                Shortcut {
                    key: "j/k",
                    desc: "上下移动",
                },
                Shortcut {
                    key: "a",
                    desc: "全选/取消",
                },
                Shortcut {
                    key: "d",
                    desc: "批量删除",
                },
                Shortcut {
                    key: "t",
                    desc: "批量打标签",
                },
                Shortcut {
                    key: "Esc/v",
                    desc: "退出多选",
                },
            ],
        },
        // 5 — 标签管理(m)
        ShortcutGroup {
            label: "标签管理(m)",
            shortcuts: &[
                Shortcut {
                    key: "m",
                    desc: "添加标签",
                },
                Shortcut {
                    key: "r",
                    desc: "重命名标签",
                },
                Shortcut {
                    key: "d",
                    desc: "删除标签",
                },
                Shortcut {
                    key: "s",
                    desc: "保存",
                },
            ],
        },
        // 6 — 同步
        ShortcutGroup {
            label: "同步",
            shortcuts: &[
                Shortcut {
                    key: "Ctrl+R",
                    desc: "拉取",
                },
                Shortcut {
                    key: "Ctrl+S",
                    desc: "推送",
                },
            ],
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
        " F1/Esc 关闭",
        Style::default().fg(theme::TEXT_SECONDARY),
    )));

    let title = " 快捷键 ";
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
    let all_indices: Vec<usize> = (0..7).collect();

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
        // Single column — hide 回收站 and 标签管理.
        let w: u16 = 44;
        let h: u16 = 28;
        let visible: Vec<usize> = (0..7).filter(|i| !COMPACT_HIDDEN.contains(i)).collect();
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
    for sc in group.shortcuts {
        lines.push(format_line_spans(sc.key, sc.desc));
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
