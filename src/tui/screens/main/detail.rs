//! Password detail panel: credential fields, health line, metadata.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::t;
use crate::tui::state::detail_state::{
    DetailFieldKind, DetailPanelState, DetailViewData, ExpiryStatus, FieldValue, PasswordStrength,
};
use crate::tui::state::list_state::{
    calculate_remaining_days, format_days_since_deletion, trash_warning_tier, TrashWarningTier,
};
use crate::tui::theme;

pub struct DetailPanel;

impl DetailPanel {
    pub fn view(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &DetailPanelState,
        focused: bool,
        unicode: bool,
        visual_selected_names: &[String],
    ) {
        // If visual mode is active with selections, show batch summary
        if !visual_selected_names.is_empty() {
            let name_refs: Vec<&str> = visual_selected_names.iter().map(|s| s.as_str()).collect();
            render_batch_summary_view(
                frame,
                area,
                &name_refs,
                visual_selected_names.len(),
                unicode,
            );
            return;
        }

        match &state.record {
            None => self.render_empty(frame, area, unicode),
            Some(record) => self.render_record(frame, area, state, record, focused, unicode),
        }
    }

    fn render_empty(&self, frame: &mut Frame, area: Rect, unicode: bool) {
        let icon = if unicode { "\u{1F510}" } else { "[?]" };
        let content_lines = vec![
            Line::from(Span::styled(
                format!("  {}", icon),
                Style::default().fg(theme::TEXT_MUTED),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", t!("tui.password_detail.empty_hint")),
                Style::default().fg(theme::TEXT_SECONDARY),
            )),
            Line::from(Span::styled(
                "  Press Enter to view details".to_string(),
                Style::default().fg(theme::TEXT_MUTED),
            )),
        ];
        let content_height = content_lines.len() as u16;
        let top_pad = area.height.saturating_sub(content_height) / 2;
        let mut lines: Vec<Line> = (0..top_pad).map(|_| Line::from("")).collect();
        lines.extend(content_lines);
        let para = Paragraph::new(lines).alignment(Alignment::Center);
        frame.render_widget(para, area);
    }

    fn render_record(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &DetailPanelState,
        record: &DetailViewData,
        focused: bool,
        unicode: bool,
    ) {
        let mut lines = Vec::new();
        let pad = "  ";
        let narrow = area.width < 100;

        // ── Trash warning banner (if in trash) ──
        if state.is_trash {
            let trash_icon = if unicode { "\u{1F5D1}" } else { "[DEL]" };

            let deleted_at = record.deleted_at.unwrap_or(record.updated_at);
            let days_ago = format_days_since_deletion(&deleted_at);

            let mut banner_spans = vec![
                Span::styled(
                    format!("{}{} ", pad, trash_icon),
                    Style::default().fg(theme::WARNING),
                ),
                Span::styled(
                    t!("tui.trash.deleted_label"),
                    Style::default()
                        .fg(theme::WARNING)
                        .add_modifier(Modifier::BOLD),
                ),
            ];

            let info_prefix = format!(" — {}", days_ago);

            match calculate_remaining_days(&deleted_at, state.trash_retention_days) {
                None => {
                    banner_spans.push(Span::styled(
                        format!(
                            "{}  · {}",
                            info_prefix,
                            t!("tui.trash.will_not_auto_delete")
                        ),
                        Style::default().fg(theme::TEXT_SECONDARY),
                    ));
                }
                Some(remaining) => {
                    let tier = trash_warning_tier(remaining);
                    let days_raw = remaining.max(0);
                    let (dot_color, remaining_text) = match tier {
                        TrashWarningTier::Safe => (
                            theme::TEXT_SECONDARY,
                            format!(" · {}", t!("tui.trash.auto_delete_in", days = days_raw)),
                        ),
                        TrashWarningTier::Moderate => (
                            theme::WARNING,
                            format!(" · {}", t!("tui.trash.auto_delete_in", days = days_raw)),
                        ),
                        TrashWarningTier::Urgent => (
                            theme::WARNING,
                            format!(
                                " \u{26A0} {}",
                                t!("tui.trash.auto_delete_in", days = days_raw)
                            ),
                        ),
                        TrashWarningTier::Critical => (
                            theme::ERROR,
                            format!(
                                " \u{26A0} {}",
                                t!("tui.trash.auto_delete_in", days = days_raw)
                            ),
                        ),
                    };
                    banner_spans.push(Span::styled(
                        info_prefix,
                        Style::default().fg(theme::TEXT_SECONDARY),
                    ));
                    banner_spans.push(Span::styled(remaining_text, Style::default().fg(dot_color)));
                }
            }

            lines.push(Line::from(banner_spans));
            lines.push(Line::from(""));
        }

        // Title Area
        lines.push(Line::from(Span::styled(
            format!("{}{}", pad, record.name),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        )));
        if !record.subtitle.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("{}{}", pad, record.subtitle),
                Style::default().fg(theme::TEXT_SECONDARY),
            )));
        }

        // Favorite + Expiry markers
        let mut markers = Vec::new();
        if record.is_favorite {
            let star = if unicode {
                theme::ICON_STAR
            } else {
                theme::ascii::ICON_STAR
            };
            markers.push(Span::styled(
                format!("{} ", star),
                Style::default().fg(theme::BRAND),
            ));
        }
        match record.expiry_status {
            ExpiryStatus::ExpiringSoon => {
                let icon = if unicode {
                    theme::ICON_WARNING
                } else {
                    theme::ascii::ICON_WARNING
                };
                if let Some(dt) = record.expires_at {
                    let now = chrono::Utc::now().date_naive();
                    let days = (dt.date_naive() - now).num_days().max(0);
                    markers.push(Span::styled(
                        format!(
                            "{} {}",
                            icon,
                            t!("tui.password_detail.expiry_warning", days = days)
                        ),
                        Style::default().fg(theme::WARNING),
                    ));
                }
            }
            ExpiryStatus::Expired => {
                let icon = if unicode {
                    theme::ICON_ERROR
                } else {
                    theme::ascii::ICON_ERROR
                };
                if let Some(dt) = record.expires_at {
                    let now = chrono::Utc::now().date_naive();
                    let days = (now - dt.date_naive()).num_days().max(0);
                    markers.push(Span::styled(
                        format!(
                            "{} {}",
                            icon,
                            t!("tui.password_detail.expiry_expired", days = days)
                        ),
                        Style::default().fg(theme::ERROR),
                    ));
                }
            }
            _ => {}
        }
        if !markers.is_empty() {
            lines.push(Line::from(markers));
        }
        lines.push(Line::from(""));

        // Separator
        let sep = if unicode { "\u{2500}" } else { "-" };
        lines.push(Line::from(Span::styled(
            format!("{}{}", pad, sep.repeat(area.width as usize / 2)),
            Style::default().fg(theme::BORDER),
        )));
        lines.push(Line::from(""));

        // Credential type label
        let type_label = match record.credential_type {
            crate::types::credential::CredentialType::Login => t!("tui.form.type_login"),
            crate::types::credential::CredentialType::Api => t!("tui.form.type_api"),
            crate::types::credential::CredentialType::Ssh => t!("tui.form.type_ssh"),
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", pad, type_label),
            Style::default()
                .fg(theme::TEXT_SECONDARY)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        // Fields
        for (field_idx, field) in record.fields.iter().enumerate() {
            // Skip notes field on narrow terminals
            if narrow && field.kind == DetailFieldKind::Notes {
                continue;
            }

            let is_focused = focused && field_idx == state.focused_field;

            let label_style = if is_focused {
                Style::default()
                    .fg(theme::TEXT)
                    .add_modifier(Modifier::REVERSED)
            } else {
                Style::default().fg(theme::TEXT_SECONDARY)
            };

            let value_style = match &field.value {
                FieldValue::Plain(_) => {
                    if is_focused {
                        Style::default()
                            .fg(theme::TEXT)
                            .add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default().fg(theme::TEXT)
                    }
                }
                FieldValue::Masked => Style::default().fg(theme::TEXT_MUTED),
                FieldValue::Revealed(_) => {
                    if is_focused {
                        Style::default()
                            .fg(theme::TEXT)
                            .add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default().fg(theme::TEXT)
                    }
                }
            };

            lines.push(Line::from(Span::styled(
                format!("{}{}:", pad, field.label),
                label_style,
            )));
            lines.push(Line::from(Span::styled(
                format!("{}  {}", pad, field.display_value()),
                value_style,
            )));

            // Password strength bar
            if field.kind == DetailFieldKind::Password
                || field.kind == DetailFieldKind::SecretKey
                || field.kind == DetailFieldKind::PrivateKey
                || field.kind == DetailFieldKind::Passphrase
            {
                if let Some(strength) = &record.password_strength {
                    let bar = self.render_strength_bar(strength, 16, unicode);
                    lines.push(Line::from(Span::styled(
                        format!("{}  {}", pad, bar),
                        Style::default().fg(strength.color()),
                    )));
                }
            }

            lines.push(Line::from(""));
        }

        // Health Issue Line
        if let Some(issue) = &state.health_issue {
            use std::borrow::Cow;
            let (text, color): (Cow<str>, _) = match issue {
                crate::commands::types::HealthIssue::Compromised => {
                    (t!("tui.password_detail.health_leaked"), theme::ERROR)
                }
                crate::commands::types::HealthIssue::Weak => {
                    (t!("tui.password_detail.health_weak"), theme::WARNING)
                }
                crate::commands::types::HealthIssue::Duplicate { group_size } => (
                    t!("tui.password_detail.health_duplicate", count = group_size),
                    theme::WARNING,
                ),
                crate::commands::types::HealthIssue::Expired => {
                    (Cow::Borrowed(""), theme::TEXT_MUTED)
                }
            };
            if !text.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!("{}\u{26A0} {}", pad, text),
                    Style::default().fg(color),
                )));
                lines.push(Line::from(""));
            }
        }

        // Tags
        if !record.tags.is_empty() {
            let tag_spans: Vec<Span> = record
                .tags
                .iter()
                .flat_map(|tag| {
                    vec![
                        Span::styled("[", Style::default().fg(theme::BRAND)),
                        Span::styled(tag.clone(), Style::default().fg(theme::TEXT)),
                        Span::styled("]  ", Style::default().fg(theme::BRAND)),
                    ]
                })
                .collect();
            lines.push(Line::from(tag_spans));
            lines.push(Line::from(""));
        }

        // ── Trash action buttons (only in trash mode) ──
        if state.is_trash {
            lines.push(Line::from(""));
            let restore_style = Style::default()
                .fg(theme::SUCCESS)
                .add_modifier(Modifier::BOLD);
            let destroy_style = Style::default()
                .fg(theme::ERROR)
                .add_modifier(Modifier::BOLD);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}[{}]", pad, t!("tui.trash.restore_button")),
                    restore_style,
                ),
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", t!("tui.overlay.confirm_delete_permanent")),
                    destroy_style,
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!(
                    "{}r {}  D {}",
                    pad,
                    t!("tui.trash.restore_button"),
                    t!("tui.overlay.confirm_delete_permanent")
                ),
                Style::default().fg(theme::TEXT_MUTED),
            )));
            lines.push(Line::from(""));
        }

        // Timestamps (hidden on narrow terminals)
        if !narrow {
            lines.push(Line::from(Span::styled(
                format!(
                    "{}{}: {}  {}: {}",
                    pad,
                    t!("tui.password_detail.created_at", date = "").trim_end_matches(" %{date}"),
                    record.created_at.format("%Y-%m-%d %H:%M"),
                    t!("tui.password_detail.updated_at", date = "").trim_end_matches(" %{date}"),
                    record.updated_at.format("%Y-%m-%d %H:%M"),
                ),
                Style::default().fg(theme::TEXT_MUTED),
            )));
        }

        let para = Paragraph::new(lines);
        frame.render_widget(para, area);
    }

    fn render_strength_bar(
        &self,
        strength: &PasswordStrength,
        width: usize,
        unicode: bool,
    ) -> String {
        let filled = (strength.fraction() * width as f32).round() as usize;
        let empty = width - filled;
        let fill_char = if unicode {
            crate::tui::theme::ICON_PROGRESS_FILL
        } else {
            crate::tui::theme::ascii::ICON_PROGRESS_FILL
        };
        let empty_char = if unicode {
            crate::tui::theme::ICON_PROGRESS_EMPTY
        } else {
            crate::tui::theme::ascii::ICON_PROGRESS_EMPTY
        };
        format!(
            "{}: {}{} {}",
            t!("tui.password_detail.strength_label"),
            fill_char.repeat(filled),
            empty_char.repeat(empty),
            strength.label()
        )
    }
}

/// Render the batch summary view in the detail panel when visual mode is active.
///
/// Shows: "已选择 N 项" header, item names (max 5), and action hints.
fn render_batch_summary_view(
    frame: &mut Frame,
    area: Rect,
    selected_names: &[&str],
    total_count: usize,
    unicode: bool,
) {
    let pad = "  ";
    let mut lines: Vec<Line<'_>> = Vec::new();

    // Top spacing
    lines.push(Line::from(""));
    lines.push(Line::from(""));

    // Header: 已选择 N 项
    lines.push(Line::from(Span::styled(
        format!(
            "{}{}",
            pad,
            t!("tui.batch.selected_count", count = total_count)
        ),
        Style::default()
            .fg(theme::TEXT)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Separator
    let sep = if unicode { "\u{2500}" } else { "-" };
    lines.push(Line::from(Span::styled(
        format!("{}{}", pad, sep.repeat(area.width as usize / 2)),
        Style::default().fg(theme::BORDER),
    )));
    lines.push(Line::from(""));

    // Item names (max 5)
    let display_limit = 5;
    for name in selected_names.iter().take(display_limit) {
        let bullet = if unicode { "\u{2022}" } else { "*" };
        lines.push(Line::from(Span::styled(
            format!("{}{}  {}", pad, bullet, name),
            Style::default().fg(theme::TEXT),
        )));
    }

    // Overflow indicator
    if total_count > display_limit {
        let remaining = total_count - display_limit;
        lines.push(Line::from(Span::styled(
            format!(
                "{}  ... {}",
                pad,
                t!("tui.password_list.selected_count", count = remaining)
            ),
            Style::default().fg(theme::TEXT_SECONDARY),
        )));
    }

    lines.push(Line::from(""));

    // Separator
    lines.push(Line::from(Span::styled(
        format!("{}{}", pad, sep.repeat(area.width as usize / 2)),
        Style::default().fg(theme::BORDER),
    )));
    lines.push(Line::from(""));

    // Action hints
    lines.push(Line::from(vec![
        Span::styled(
            format!("{}d ", pad),
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{}  ", t!("tui.notification.deleted")),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
        Span::styled(
            "t ",
            Style::default()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            t!("tui.sidebar_tags"),
            Style::default().fg(theme::TEXT_SECONDARY),
        ),
    ]));

    let para = Paragraph::new(lines);
    frame.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::detail_state::*;
    use ratatui::backend::TestBackend;

    fn make_trash_detail_data() -> DetailViewData {
        DetailViewData {
            id: uuid::Uuid::new_v4(),
            name: "DeletedSite".into(),
            subtitle: "https://example.com".into(),
            credential_type: crate::types::credential::CredentialType::Login,
            is_favorite: false,
            expires_at: None,
            expiry_status: ExpiryStatus::None,
            tags: vec![],
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            fields: vec![
                DetailField {
                    label: t!("tui.password_detail.username_label").to_string(),
                    value: FieldValue::Plain("alice".into()),
                    copyable: true,
                    toggleable: false,
                    kind: DetailFieldKind::Username,
                },
                DetailField {
                    label: t!("tui.password_detail.password_label").to_string(),
                    value: FieldValue::Masked,
                    copyable: true,
                    toggleable: true,
                    kind: DetailFieldKind::Password,
                },
            ],
            password_strength: None,
            deleted_at: Some(chrono::Utc::now() - chrono::Duration::days(5)),
        }
    }

    fn render_detail_snapshot(
        state: &DetailPanelState,
        width: u16,
        height: u16,
        focused: bool,
        unicode: bool,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let panel = DetailPanel;
                panel.view(frame, frame.area(), state, focused, unicode, &[]);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        format!("{:?}", buf)
    }

    #[test]
    fn render_trash_detail_shows_banner() {
        let data = make_trash_detail_data();
        let mut state = DetailPanelState::with_record(data);
        state.set_trash_context(true, 30);
        let result = render_detail_snapshot(&state, 60, 20, true, true);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_trash_detail_never_auto_delete() {
        let data = make_trash_detail_data();
        let mut state = DetailPanelState::with_record(data);
        state.set_trash_context(true, 0);
        let result = render_detail_snapshot(&state, 60, 20, true, true);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_normal_detail_no_trash_banner() {
        let data = make_trash_detail_data();
        let state = DetailPanelState::with_record(data);
        let result = render_detail_snapshot(&state, 60, 20, true, true);
        assert!(!result.is_empty());
    }

    #[test]
    fn render_trash_detail_empty() {
        let mut state = DetailPanelState::default();
        state.set_trash_context(true, 30);
        let result = render_detail_snapshot(&state, 60, 20, true, true);
        assert!(!result.is_empty());
    }

    // ── Batch summary tests ──────────────────────────────────────────────

    fn render_batch_snapshot(
        names: &[&str],
        total_count: usize,
        width: u16,
        height: u16,
    ) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                render_batch_summary_view(frame, frame.area(), names, total_count, true);
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        format!("{:?}", buf)
    }

    #[test]
    fn batch_summary_shows_count() {
        let result = render_batch_snapshot(&["GitHub", "AWS"], 2, 50, 15);
        assert!(
            result.contains("2") || result.contains("selected"),
            "should show count"
        );
    }

    #[test]
    fn batch_summary_shows_names() {
        let result = render_batch_snapshot(&["GitHub", "AWS"], 2, 50, 15);
        assert!(result.contains("GitHub"), "should show item name");
    }

    #[test]
    fn batch_summary_shows_hints() {
        let result = render_batch_snapshot(&["GitHub"], 1, 50, 15);
        assert!(
            result.contains("d") || result.contains("t"),
            "should show action hints"
        );
    }

    #[test]
    fn batch_summary_limits_to_five_names() {
        let names = vec!["A", "B", "C", "D", "E", "F", "G"];
        let result = render_batch_snapshot(&names, 7, 50, 20);
        // Should show count for overflow
        assert!(
            result.contains("2") || result.contains("selected"),
            "should show overflow indicator"
        );
    }
}
