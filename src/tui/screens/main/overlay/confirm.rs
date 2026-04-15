//! Confirmation dialog overlay — renders one of 5 confirm variants with Cancel/Confirm buttons.
//!
//! Variants:
//! - **SoftDelete**: move record to trash (reversible)
//! - **HardDelete**: permanently delete record (irreversible)
//! - **EmptyTrash**: empty the trash (irreversible)
//! - **BatchSoftDelete**: move multiple records to trash (reversible)
//! - **TagDelete**: delete a tag (irreversible)

use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::commands::types::{ConfirmButton, ConfirmVariant};
use crate::tui::theme;

// ── Colour / layout constants ────────────────────────────────────

const OVERLAY_BG: Color = Color::Rgb(26, 27, 38); // #1a1b26
const DIALOG_WIDTH: u16 = 48;

// ── Public API ───────────────────────────────────────────────────

/// Render the confirmation dialog centred within `area`.
pub fn render_confirm(
    frame: &mut Frame,
    area: Rect,
    variant: &ConfirmVariant,
    focused_button: ConfirmButton,
) {
    let width = DIALOG_WIDTH.min(area.width);
    let content_width = width.saturating_sub(2); // inside borders

    let is_danger = is_danger_variant(variant);
    let (title, body_lines, confirm_label) = build_dialog_parts(variant, content_width);

    let title_style = if is_danger {
        Style::default()
            .fg(theme::WARNING)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::PRIMARY)
            .add_modifier(Modifier::BOLD)
    };

    let block = Block::default()
        .title(Span::styled(title, title_style))
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER))
        .style(Style::default().bg(OVERLAY_BG));

    // body + separator + button line
    let mut all_lines = body_lines;
    all_lines.push(separator_line(content_width));
    all_lines.push(render_buttons(focused_button, confirm_label, is_danger));

    let total_lines = all_lines.len() as u16;
    let height = (total_lines + 2).min(area.height); // +2 border rows
    let overlay_rect = centered_rect(area, width, height);

    let inner = inner_area(overlay_rect);

    let paragraph = Paragraph::new(all_lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left);

    frame.render_widget(Clear, overlay_rect);
    frame.render_widget(paragraph, overlay_rect);

    // We keep a reference to inner_area for potential future use, but the
    // paragraph widget already handles the inner layout via the block.
    let _ = inner;
}

/// Handle a key press while the confirmation dialog is active.
///
/// Returns:
/// - `Some(true)` — user confirmed
/// - `Some(false)` — user cancelled
/// - `None` — key not consumed (focus toggled, etc.)
pub fn handle_key(key: crossterm::event::KeyCode, focused: &mut ConfirmButton) -> Option<bool> {
    use crossterm::event::KeyCode;

    match key {
        KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
            *focused = match focused {
                ConfirmButton::Cancel => ConfirmButton::Confirm,
                ConfirmButton::Confirm => ConfirmButton::Cancel,
            };
            None
        }

        KeyCode::Enter => Some(matches!(focused, ConfirmButton::Confirm)),

        KeyCode::Esc => Some(false),

        KeyCode::Char('y') => Some(true),

        KeyCode::Char('n') => Some(false),

        _ => None,
    }
}

// ── Variant metadata ─────────────────────────────────────────────

/// Returns `true` for irreversible (danger) variants.
fn is_danger_variant(variant: &ConfirmVariant) -> bool {
    matches!(
        variant,
        ConfirmVariant::HardDelete { .. }
            | ConfirmVariant::EmptyTrash { .. }
    )
}

/// Determine the confirm button label for the given variant.
fn confirm_label_for(variant: &ConfirmVariant) -> &'static str {
    match variant {
        ConfirmVariant::SoftDelete { .. } => "移到回收站",
        ConfirmVariant::HardDelete { .. } => "永久删除",
        ConfirmVariant::EmptyTrash { .. } => "清空回收站",
        ConfirmVariant::BatchSoftDelete { .. } => "移到回收站",
        ConfirmVariant::TagDelete { .. } => "删除标签",
    }
}

// ── Dialog content builder ───────────────────────────────────────

/// Build (title, body_lines, confirm_label) for a variant.
fn build_dialog_parts(variant: &ConfirmVariant, content_width: u16) -> (String, Vec<Line<'static>>, &'static str) {
    match variant {
        ConfirmVariant::SoftDelete {
            record_name,
            auto_delete_days,
            ..
        } => {
            let mut lines = vec![line_with_name(
                &format!("将 \"{}\" 移到回收站？", record_name),
                content_width,
            )];
            if let Some(days) = auto_delete_days {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  {} 天后自动永久删除", days),
                    Style::default().fg(theme::TEXT_SECONDARY),
                )));
            }
            (
                " 确认 ".to_string(),
                lines,
                confirm_label_for(variant),
            )
        }

        ConfirmVariant::HardDelete { record_name, .. } => {
            let lines = vec![line_with_name(
                &format!("确定永久删除 \"{}\"？", record_name),
                content_width,
            )];
            (
                format!(" {} 警告 ", theme::ICON_WARNING),
                lines,
                confirm_label_for(variant),
            )
        }

        ConfirmVariant::EmptyTrash { count } => {
            let lines = vec![Line::from(Span::styled(
                format!("确定清空回收站？共 {} 个密码。", count),
                Style::default().fg(theme::TEXT),
            ))];
            (
                format!(" {} 警告 ", theme::ICON_WARNING),
                lines,
                confirm_label_for(variant),
            )
        }

        ConfirmVariant::BatchSoftDelete {
            record_names, ..
        } => {
            let count = record_names.len();
            let mut lines = vec![Line::from(Span::styled(
                format!("将 {} 条密码移到回收站？", count),
                Style::default().fg(theme::TEXT),
            ))];
            lines.push(Line::from(""));
            for name in record_names.iter().take(5) {
                lines.push(Line::from(Span::styled(
                    format!("  - {}", name),
                    Style::default().fg(theme::TEXT_SECONDARY),
                )));
            }
            if record_names.len() > 5 {
                lines.push(Line::from(Span::styled(
                    format!("  ...等共 {} 条", record_names.len()),
                    Style::default().fg(theme::TEXT_MUTED),
                )));
            }
            (
                " 确认 ".to_string(),
                lines,
                confirm_label_for(variant),
            )
        }

        ConfirmVariant::TagDelete {
            tag_name,
            affected_count,
        } => {
            let mut lines = vec![line_with_name(
                &format!("确定删除标签 \"{}\"？", tag_name),
                content_width,
            )];
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                format!("  影响 {} 条密码", affected_count),
                Style::default().fg(theme::TEXT_SECONDARY),
            )));
            (
                " 确认 ".to_string(),
                lines,
                confirm_label_for(variant),
            )
        }
    }
}

// ── Rendering helpers ────────────────────────────────────────────

/// Build the button line with Cancel and Confirm buttons.
fn render_buttons(
    focused: ConfirmButton,
    confirm_label: &str,
    is_danger: bool,
) -> Line<'static> {
    let cancel_text = " 取消 ";
    let confirm_text = format!(" {} ", confirm_label);

    let cancel_style = if matches!(focused, ConfirmButton::Cancel) {
        Style::default()
            .fg(theme::TEXT_SECONDARY)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(theme::TEXT_SECONDARY)
    };

    let confirm_fg = if is_danger {
        theme::ERROR
    } else {
        theme::PRIMARY
    };

    let confirm_style = if matches!(focused, ConfirmButton::Confirm) {
        Style::default()
            .fg(confirm_fg)
            .add_modifier(Modifier::REVERSED)
    } else {
        Style::default().fg(confirm_fg)
    };

    Line::from(vec![
        Span::styled(cancel_text.to_string(), cancel_style),
        Span::raw("  "),
        Span::styled(confirm_text, confirm_style),
    ])
}

/// A line that respects content width (used for name-containing messages).
/// Currently a no-op wrapper but ensures consistent styling.
fn line_with_name(text: &str, _content_width: u16) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(theme::TEXT),
    ))
}

/// Horizontal separator line using dim dashes.
fn separator_line(content_width: u16) -> Line<'static> {
    let dash_count = content_width as usize;
    let sep = "-".repeat(dash_count);
    Line::from(Span::styled(
        sep,
        Style::default().fg(theme::BORDER),
    ))
}

/// Return a `Rect` of size `width x height` centred inside `area`.
fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area
        .x
        .checked_add((area.width.saturating_sub(width)) / 2)
        .unwrap_or(area.x);
    let y = area
        .y
        .checked_add((area.height.saturating_sub(height)) / 2)
        .unwrap_or(area.y);
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Return the inner content area (excluding borders).
fn inner_area(rect: Rect) -> Rect {
    Rect::new(
        rect.x + 1,
        rect.y + 1,
        rect.width.saturating_sub(2),
        rect.height.saturating_sub(2),
    )
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::ConfirmVariant;
    use uuid::Uuid;

    fn soft_delete_variant() -> ConfirmVariant {
        ConfirmVariant::SoftDelete {
            record_id: Uuid::new_v4(),
            record_name: "测试密码".to_string(),
            auto_delete_days: Some(30),
        }
    }

    #[test]
    fn handle_key_tab_toggles_focus() {
        let mut focused = ConfirmButton::Cancel;

        handle_key(crossterm::event::KeyCode::Tab, &mut focused);
        assert_eq!(focused, ConfirmButton::Confirm);

        handle_key(crossterm::event::KeyCode::Tab, &mut focused);
        assert_eq!(focused, ConfirmButton::Cancel);

        // Arrow keys also toggle
        handle_key(crossterm::event::KeyCode::Right, &mut focused);
        assert_eq!(focused, ConfirmButton::Confirm);

        handle_key(crossterm::event::KeyCode::Left, &mut focused);
        assert_eq!(focused, ConfirmButton::Cancel);
    }

    #[test]
    fn handle_key_enter_confirm() {
        let mut focused = ConfirmButton::Confirm;
        let result = handle_key(crossterm::event::KeyCode::Enter, &mut focused);
        assert_eq!(result, Some(true));
    }

    #[test]
    fn handle_key_enter_cancel() {
        let mut focused = ConfirmButton::Cancel;
        let result = handle_key(crossterm::event::KeyCode::Enter, &mut focused);
        assert_eq!(result, Some(false));
    }

    #[test]
    fn handle_key_esc_cancels() {
        let mut focused = ConfirmButton::Confirm;
        let result = handle_key(crossterm::event::KeyCode::Esc, &mut focused);
        assert_eq!(result, Some(false));
    }

    #[test]
    fn handle_key_y_confirms() {
        let mut focused = ConfirmButton::Cancel;
        let result = handle_key(crossterm::event::KeyCode::Char('y'), &mut focused);
        assert_eq!(result, Some(true));
        // 'y' confirms regardless of which button has focus
        let mut focused = ConfirmButton::Confirm;
        let result = handle_key(crossterm::event::KeyCode::Char('y'), &mut focused);
        assert_eq!(result, Some(true));
    }

    #[test]
    fn handle_key_n_cancels() {
        let mut focused = ConfirmButton::Confirm;
        let result = handle_key(crossterm::event::KeyCode::Char('n'), &mut focused);
        assert_eq!(result, Some(false));
        // 'n' cancels regardless of which button has focus
        let mut focused = ConfirmButton::Cancel;
        let result = handle_key(crossterm::event::KeyCode::Char('n'), &mut focused);
        assert_eq!(result, Some(false));
    }

    // ── Additional unit tests for helper functions ──

    #[test]
    fn danger_variant_detection() {
        assert!(!is_danger_variant(&soft_delete_variant()));
        assert!(is_danger_variant(&ConfirmVariant::HardDelete {
            record_id: Uuid::new_v4(),
            record_name: "x".to_string(),
        }));
        assert!(is_danger_variant(&ConfirmVariant::EmptyTrash { count: 3 }));
        assert!(!is_danger_variant(&ConfirmVariant::BatchSoftDelete {
            record_ids: vec![Uuid::new_v4()],
            record_names: vec!["a".to_string()],
        }));
        assert!(!is_danger_variant(&ConfirmVariant::TagDelete {
            tag_name: "work".to_string(),
            affected_count: 1,
        }));
    }

    #[test]
    fn confirm_labels_are_correct() {
        assert_eq!(
            confirm_label_for(&soft_delete_variant()),
            "移到回收站"
        );
        assert_eq!(
            confirm_label_for(&ConfirmVariant::HardDelete {
                record_id: Uuid::new_v4(),
                record_name: "x".to_string(),
            }),
            "永久删除"
        );
        assert_eq!(
            confirm_label_for(&ConfirmVariant::EmptyTrash { count: 1 }),
            "清空回收站"
        );
        assert_eq!(
            confirm_label_for(&ConfirmVariant::BatchSoftDelete {
                record_ids: vec![],
                record_names: vec![],
            }),
            "移到回收站"
        );
        assert_eq!(
            confirm_label_for(&ConfirmVariant::TagDelete {
                tag_name: "t".to_string(),
                affected_count: 0,
            }),
            "删除标签"
        );
    }

    #[test]
    fn centered_rect_within_bounds() {
        let area = Rect::new(0, 0, 100, 40);
        let rect = centered_rect(area, 48, 10);
        assert_eq!(rect.width, 48);
        assert_eq!(rect.height, 10);
        // Should be roughly centred
        assert_eq!(rect.x, (100 - 48) / 2);
        assert_eq!(rect.y, (40 - 10) / 2);
    }

    #[test]
    fn centered_rect_clamps_to_area() {
        let area = Rect::new(0, 0, 20, 5);
        let rect = centered_rect(area, 48, 10);
        assert_eq!(rect.width, 20);
        assert_eq!(rect.height, 5);
    }

    #[test]
    fn inner_area_subtracts_borders() {
        let rect = Rect::new(10, 5, 48, 10);
        let inner = inner_area(rect);
        assert_eq!(inner.x, 11);
        assert_eq!(inner.y, 6);
        assert_eq!(inner.width, 46);
        assert_eq!(inner.height, 8);
    }

    #[test]
    fn build_dialog_soft_delete_with_auto_days() {
        let variant = ConfirmVariant::SoftDelete {
            record_id: Uuid::new_v4(),
            record_name: "GitHub".to_string(),
            auto_delete_days: Some(30),
        };
        let (title, lines, label) = build_dialog_parts(&variant, 46);
        assert_eq!(title, " 确认 ");
        assert_eq!(label, "移到回收站");
        // Should have: message line + blank + hint line
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn build_dialog_soft_delete_without_auto_days() {
        let variant = ConfirmVariant::SoftDelete {
            record_id: Uuid::new_v4(),
            record_name: "GitHub".to_string(),
            auto_delete_days: None,
        };
        let (_, lines, _) = build_dialog_parts(&variant, 46);
        // Only the message line, no extra hint
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn build_dialog_batch_soft_delete_limits_five_names() {
        let variant = ConfirmVariant::BatchSoftDelete {
            record_ids: vec![Uuid::new_v4(); 8],
            record_names: (0..8).map(|i| format!("item-{}", i)).collect(),
        };
        let (_, lines, _) = build_dialog_parts(&variant, 46);
        // message + blank + 5 names + "...等共 8 条"
        assert_eq!(lines.len(), 8);
    }

    #[test]
    fn build_dialog_batch_soft_delete_under_five() {
        let variant = ConfirmVariant::BatchSoftDelete {
            record_ids: vec![Uuid::new_v4(); 3],
            record_names: vec!["a".to_string(), "b".to_string(), "c".to_string()],
        };
        let (_, lines, _) = build_dialog_parts(&variant, 46);
        // message + blank + 3 names (no overflow line)
        assert_eq!(lines.len(), 5);
    }
}
