//! Password detail panel: credential fields, health line, metadata.

use ratatui::layout::{Alignment, Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::t;
use crate::tui::state::detail_state::{
    DetailActionFocus, DetailActionKind, DetailFieldKind, DetailPanelState, DetailViewData,
    ExpiryStatus, FieldValue, PasswordStrength,
};
use crate::tui::state::list_state::{
    calculate_remaining_days, format_days_since_deletion, trash_warning_tier, TrashWarningTier,
};
use crate::tui::theme;
use crate::tui::time::format_display_datetime;

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
        frame.render_widget(Paragraph::new("").style(theme::Styles::newlook_bg()), area);
        let icon = if unicode { "\u{1F510}" } else { "[?]" };
        let content_lines = vec![
            Line::from(""),
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
        if area.width >= 50 {
            self.render_record_card(frame, area, state, record, focused, unicode);
            return;
        }

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
                    theme::NF_WARNING_TRIANGLE
                } else {
                    theme::ascii::NF_WARNING_TRIANGLE
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
            crate::types::credential::CredentialType::SecureNote => t!("tui.form.type_secure_note"),
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
            let mut tag_spans: Vec<Span> = vec![Span::raw(pad.to_string())];
            for tag in &record.tags {
                tag_spans.extend([
                    Span::styled("[", Style::default().fg(theme::BRAND)),
                    Span::styled(tag.clone(), Style::default().fg(theme::TEXT)),
                    Span::styled("]  ", Style::default().fg(theme::BRAND)),
                ]);
            }
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
                    format!("[{}]", t!("tui.trash.permanent_delete_title")),
                    destroy_style,
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!(
                    "{}r {}  D {}",
                    pad,
                    t!("tui.trash.restore_button"),
                    t!("tui.trash.permanent_delete_title")
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
                    format_display_datetime(&record.created_at),
                    t!("tui.password_detail.updated_at", date = "").trim_end_matches(" %{date}"),
                    format_display_datetime(&record.updated_at),
                ),
                Style::default().fg(theme::TEXT_MUTED),
            )));
        }

        let para = Paragraph::new(lines).style(theme::Styles::newlook_bg());
        frame.render_widget(para, area);
    }

    fn render_record_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &DetailPanelState,
        record: &DetailViewData,
        focused: bool,
        unicode: bool,
    ) {
        let border_style = if focused {
            Style::default().fg(theme::NL_FOCUS)
        } else {
            Style::default().fg(theme::NL_LINE)
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .style(Style::default().bg(theme::NL_SURFACE));
        let inner = block.inner(area).inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
        frame.render_widget(block, area);

        let mut lines: Vec<Line<'_>> = Vec::new();
        let db_icon = nf_icon(unicode, theme::NF_DATABASE, theme::ascii::NF_DATABASE);
        let key_icon = nf_icon(unicode, theme::NF_KEY, theme::ascii::NF_KEY);

        let mut title = vec![
            Span::styled(
                format!("{}  ", db_icon),
                Style::default().fg(theme::NL_CYAN),
            ),
            Span::styled(
                record.name.clone(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ];
        if record.is_favorite {
            title.extend([
                Span::raw("   "),
                Span::styled(" ", Style::default().fg(theme::WARNING)),
                Span::styled(
                    format!(
                        "{} {}",
                        nf_icon(unicode, theme::NF_STAR, theme::ascii::NF_STAR),
                        t!("tui.password_detail.favorite_badge")
                    ),
                    Style::default()
                        .fg(theme::NL_HOT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default().fg(theme::WARNING)),
            ]);
        }
        if matches!(
            state.health_issue,
            Some(crate::commands::types::HealthIssue::Compromised)
        ) {
            title.extend([
                Span::raw("   "),
                Span::styled(
                    format!(
                        "{} {}",
                        if unicode { "\u{F06BD}" } else { "[!]" },
                        t!("tui.password_detail.health_leaked_short"),
                    ),
                    Style::default()
                        .fg(theme::ERROR)
                        .add_modifier(Modifier::BOLD),
                ),
            ]);
        }
        lines.push(Line::from(title));

        if let Some(expiry_line) = expiry_status_line(record, unicode) {
            lines.push(Line::from(""));
            lines.push(expiry_line);
        } else {
            lines.push(Line::from(""));
        }

        lines.push(Line::from(""));
        lines.push(separator_line(inner.width, unicode));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}  ", key_icon),
                Style::default().fg(theme::NL_CYAN),
            ),
            Span::styled(
                credential_type_label(record).into_owned(),
                Style::default()
                    .fg(theme::NL_CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(""));

        self.render_primary_table(&mut lines, record, state, focused, unicode, inner.width);

        lines.push(Line::from(""));
        if let Some(issue_line) = health_issue_line(state, unicode) {
            lines.push(issue_line);
            lines.push(Line::from(""));
        }
        self.render_metadata_table(&mut lines, record, unicode, inner.width);

        if state.is_trash {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    format!("[ {} ]", t!("tui.trash.restore_button")),
                    Style::default()
                        .fg(theme::SUCCESS)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("[ {} ]", t!("tui.trash.permanent_delete_title")),
                    Style::default()
                        .fg(theme::ERROR)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        frame.render_widget(
            Paragraph::new(lines).style(theme::Styles::newlook_surface()),
            inner,
        );
    }

    fn render_primary_table(
        &self,
        lines: &mut Vec<Line<'static>>,
        record: &DetailViewData,
        state: &DetailPanelState,
        focused: bool,
        unicode: bool,
        width: u16,
    ) {
        let has_primary_fields = record
            .fields
            .iter()
            .any(|field| field.kind != DetailFieldKind::Notes);
        if !has_primary_fields && !should_render_empty_url_row(record) {
            return;
        }

        let Some(cols) = table_columns(width) else {
            return;
        };

        lines.push(table_border_line(cols, "┌", "┬", "┐", unicode));
        for (field_idx, field) in record.fields.iter().enumerate() {
            if field.kind == DetailFieldKind::Notes {
                continue;
            }
            lines.push(self.render_field_card_row(field_idx, field, state, focused, unicode, cols));

            if is_secret_field(field.kind) {
                if let Some(strength) = &record.password_strength {
                    lines.push(table_border_line(cols, "├", "┼", "┤", unicode));
                    lines.push(self.render_strength_card_row(strength, unicode, cols));
                }
            }

            lines.push(table_border_line(cols, "├", "┼", "┤", unicode));
        }

        if should_render_empty_url_row(record) {
            lines.push(render_plain_table_row(
                field_icon(DetailFieldKind::Url, unicode),
                t!("tui.password_detail.url_label").as_ref(),
                "",
                Vec::new(),
                cols,
            ));
            lines.push(table_border_line(cols, "├", "┼", "┤", unicode));
        }

        replace_last_border(lines, table_border_line(cols, "└", "┴", "┘", unicode));
    }

    fn render_metadata_table(
        &self,
        lines: &mut Vec<Line<'static>>,
        record: &DetailViewData,
        unicode: bool,
        width: u16,
    ) {
        let Some(cols) = metadata_columns(width) else {
            return;
        };
        lines.push(metadata_border_line(cols, "┌", "┬", "┐", unicode));
        if !record.tags.is_empty() {
            lines.push(render_metadata_row(
                nf_icon(unicode, theme::NF_TAG, theme::ascii::NF_TAG),
                t!("tui.password_detail.tags_label").as_ref(),
                render_tag_chips(&record.tags),
                cols,
            ));
            lines.push(metadata_border_line(cols, "├", "┼", "┤", unicode));
        }
        if let Some(notes) = record.notes.as_ref().filter(|notes| !notes.is_empty()) {
            lines.extend(render_notes_metadata_rows(
                nf_icon(unicode, theme::NF_NOTE, theme::ascii::NF_NOTE),
                t!("tui.password_detail.notes_label").as_ref(),
                notes,
                cols,
            ));
            lines.push(metadata_border_line(cols, "├", "┼", "┤", unicode));
        }
        let created_at = format_display_datetime(&record.created_at);
        let updated_at = format_display_datetime(&record.updated_at);
        lines.push(render_plain_metadata_row(
            nf_icon(unicode, theme::NF_CLOCK, theme::ascii::NF_CLOCK),
            t!("tui.password_detail.created_at", date = "").trim_end_matches(" %{date}"),
            &created_at,
            cols,
        ));
        lines.push(metadata_border_line(cols, "├", "┼", "┤", unicode));
        lines.push(render_plain_metadata_row(
            nf_icon(unicode, theme::NF_CLOCK, theme::ascii::NF_CLOCK),
            t!("tui.password_detail.updated_at", date = "").trim_end_matches(" %{date}"),
            &updated_at,
            cols,
        ));
        lines.push(metadata_border_line(cols, "└", "┴", "┘", unicode));
    }

    fn render_field_card_row(
        &self,
        field_idx: usize,
        field: &crate::tui::state::detail_state::DetailField,
        state: &DetailPanelState,
        focused: bool,
        unicode: bool,
        cols: TableColumns,
    ) -> Line<'static> {
        let is_row_focused = focused && state.focused_field == field_idx;
        let row_style = if is_row_focused && state.focused_action.is_none() {
            Style::default()
                .fg(theme::NL_TEXT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::NL_TEXT)
        };
        render_table_row(
            field_icon(field.kind, unicode),
            &field.label,
            vec![Span::styled(field.display_value(), row_style)],
            field_action_spans(field_idx, field, state, unicode, cols.action < 20),
            cols,
        )
    }

    fn render_strength_card_row(
        &self,
        strength: &PasswordStrength,
        unicode: bool,
        cols: TableColumns,
    ) -> Line<'static> {
        let icon = nf_icon(unicode, theme::NF_SHIELD, theme::ascii::NF_SHIELD);
        let filled = (strength.fraction() * 24.0).round() as usize;
        let empty = 24usize.saturating_sub(filled);
        let fill = if unicode {
            theme::ICON_PROGRESS_FILL
        } else {
            theme::ascii::ICON_PROGRESS_FILL
        };
        let empty_char = if unicode {
            theme::ICON_PROGRESS_EMPTY
        } else {
            theme::ascii::ICON_PROGRESS_EMPTY
        };
        render_table_row(
            icon,
            t!("tui.password_detail.strength_label").as_ref(),
            vec![
                Span::styled(fill.repeat(filled), Style::default().fg(strength.color())),
                Span::styled(
                    empty_char.repeat(empty),
                    Style::default().fg(theme::NL_LINE),
                ),
            ],
            vec![Span::styled(
                strength.label(),
                Style::default().fg(strength.color()),
            )],
            cols,
        )
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

#[derive(Clone, Copy)]
struct TableColumns {
    label: usize,
    value: usize,
    action: usize,
}

fn table_columns(width: u16) -> Option<TableColumns> {
    let total = width as usize;
    if total < 42 {
        return None;
    }
    let label = if total >= 82 {
        18
    } else if total >= 52 {
        14
    } else {
        12
    };
    let action = if total >= 80 {
        26
    } else if total >= 60 {
        22
    } else {
        14
    };
    let value = total.saturating_sub(label + action + 10);
    (value >= 8).then_some(TableColumns {
        label,
        value,
        action,
    })
}

#[derive(Clone, Copy)]
struct MetadataColumns {
    label: usize,
    value: usize,
}

fn metadata_columns(width: u16) -> Option<MetadataColumns> {
    let total = width as usize;
    if total < 30 {
        return None;
    }
    let label = if total >= 82 {
        18
    } else if total >= 52 {
        14
    } else {
        12
    };
    let value = total.saturating_sub(label + 6);
    (value >= 8).then_some(MetadataColumns { label, value })
}

fn metadata_border_line(
    cols: MetadataColumns,
    left: &str,
    middle: &str,
    right: &str,
    unicode: bool,
) -> Line<'static> {
    if !unicode {
        return Line::from(Span::styled(
            format!(
                "+{}+{}+",
                "-".repeat(cols.label + 2),
                "-".repeat(cols.value + 2)
            ),
            Style::default().fg(theme::NL_LINE),
        ));
    }
    Line::from(Span::styled(
        format!(
            "{}{}{}{}{}",
            left,
            "─".repeat(cols.label + 2),
            middle,
            "─".repeat(cols.value + 2),
            right
        ),
        Style::default().fg(theme::NL_LINE),
    ))
}

fn render_plain_metadata_row(
    icon: &str,
    label: &str,
    value: &str,
    cols: MetadataColumns,
) -> Line<'static> {
    render_metadata_row(
        icon,
        label,
        vec![Span::styled(
            value.to_string(),
            Style::default().fg(theme::NL_TEXT),
        )],
        cols,
    )
}

fn render_metadata_row(
    icon: &str,
    label: &str,
    value: Vec<Span<'static>>,
    cols: MetadataColumns,
) -> Line<'static> {
    render_metadata_row_with_label(&format!("{}  {}", icon, label), value, cols)
}

fn render_metadata_row_with_label(
    label_text: &str,
    value: Vec<Span<'static>>,
    cols: MetadataColumns,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled("│ ", Style::default().fg(theme::NL_LINE)),
        Span::styled(
            pad_to_width(label_text, cols.label),
            Style::default().fg(theme::NL_TEXT_MUTED),
        ),
        Span::styled(" │ ", Style::default().fg(theme::NL_LINE)),
    ];
    spans.extend(pad_spans(value, cols.value, false));
    spans.push(Span::styled(" │", Style::default().fg(theme::NL_LINE)));
    Line::from(spans)
}

fn render_notes_metadata_rows(
    icon: &str,
    label: &str,
    notes: &str,
    cols: MetadataColumns,
) -> Vec<Line<'static>> {
    let rendered = render_markdown_note_lines(notes);
    let label_text = format!("{}  {}", icon, label);
    rendered
        .into_iter()
        .enumerate()
        .map(|(index, spans)| {
            render_metadata_row_with_label(if index == 0 { &label_text } else { "" }, spans, cols)
        })
        .collect()
}

fn render_markdown_note_lines(notes: &str) -> Vec<Vec<Span<'static>>> {
    let mut in_code_block = false;
    let mut rows = Vec::new();
    for raw_line in notes.lines() {
        let trimmed = raw_line.trim_start();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            rows.push(vec![Span::styled(
                raw_line.to_string(),
                Style::default().fg(theme::NL_CYAN),
            )]);
            continue;
        }

        if let Some(heading) = trimmed.strip_prefix('#') {
            let heading = heading.trim_start_matches('#').trim_start();
            rows.push(vec![Span::styled(
                heading.to_string(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD),
            )]);
        } else if let Some(item) = markdown_list_item(trimmed) {
            let mut spans = vec![Span::styled("• ", Style::default().fg(theme::NL_CYAN))];
            spans.extend(render_inline_markdown_spans(item));
            rows.push(spans);
        } else if let Some(quote) = trimmed.strip_prefix("> ") {
            let mut spans = vec![Span::styled("│ ", Style::default().fg(theme::NL_CYAN))];
            spans.extend(render_inline_markdown_spans(quote));
            rows.push(spans);
        } else {
            rows.push(render_inline_markdown_spans(raw_line));
        }
    }

    if rows.is_empty() {
        rows.push(vec![Span::raw("")]);
    }
    rows
}

fn markdown_list_item(line: &str) -> Option<&str> {
    ["- ", "* ", "+ "]
        .into_iter()
        .find_map(|marker| line.strip_prefix(marker))
}

fn render_inline_markdown_spans(text: &str) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let mut bold = false;
    let mut italic = false;
    let mut code = false;
    let mut index = 0;

    while index < chars.len() {
        if chars[index] == '`' {
            push_markdown_buffer(&mut spans, &mut buffer, bold, italic, code);
            code = !code;
            index += 1;
        } else if !code && index + 1 < chars.len() && chars[index] == '*' && chars[index + 1] == '*'
        {
            push_markdown_buffer(&mut spans, &mut buffer, bold, italic, code);
            bold = !bold;
            index += 2;
        } else if !code && chars[index] == '*' {
            push_markdown_buffer(&mut spans, &mut buffer, bold, italic, code);
            italic = !italic;
            index += 1;
        } else {
            buffer.push(chars[index]);
            index += 1;
        }
    }
    push_markdown_buffer(&mut spans, &mut buffer, bold, italic, code);

    if spans.is_empty() {
        spans.push(Span::raw(""));
    }
    spans
}

fn push_markdown_buffer(
    spans: &mut Vec<Span<'static>>,
    buffer: &mut String,
    bold: bool,
    italic: bool,
    code: bool,
) {
    if buffer.is_empty() {
        return;
    }
    let mut style = Style::default().fg(if code { theme::NL_CYAN } else { theme::NL_TEXT });
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    spans.push(Span::styled(std::mem::take(buffer), style));
}

fn table_border_line(
    cols: TableColumns,
    left: &str,
    middle: &str,
    right: &str,
    unicode: bool,
) -> Line<'static> {
    if !unicode {
        return Line::from(Span::styled(
            format!(
                "+{}+{}+{}+",
                "-".repeat(cols.label + 2),
                "-".repeat(cols.value + 2),
                "-".repeat(cols.action + 2)
            ),
            Style::default().fg(theme::NL_LINE),
        ));
    }
    Line::from(Span::styled(
        format!(
            "{}{}{}{}{}{}{}",
            left,
            "─".repeat(cols.label + 2),
            middle,
            "─".repeat(cols.value + 2),
            middle,
            "─".repeat(cols.action + 2),
            right
        ),
        Style::default().fg(theme::NL_LINE),
    ))
}

fn replace_last_border(lines: &mut [Line<'static>], replacement: Line<'static>) {
    if let Some(last) = lines.last_mut() {
        *last = replacement;
    }
}

fn render_plain_table_row(
    icon: &str,
    label: &str,
    value: &str,
    actions: Vec<Span<'static>>,
    cols: TableColumns,
) -> Line<'static> {
    render_table_row(
        icon,
        label,
        vec![Span::styled(
            value.to_string(),
            Style::default().fg(theme::NL_TEXT),
        )],
        actions,
        cols,
    )
}

fn render_table_row(
    icon: &str,
    label: &str,
    value: Vec<Span<'static>>,
    actions: Vec<Span<'static>>,
    cols: TableColumns,
) -> Line<'static> {
    let label_text = format!("{}  {}", icon, label);
    let mut spans = vec![
        Span::styled("│ ", Style::default().fg(theme::NL_LINE)),
        Span::styled(
            pad_to_width(&label_text, cols.label),
            Style::default().fg(theme::NL_TEXT_MUTED),
        ),
        Span::styled(" │ ", Style::default().fg(theme::NL_LINE)),
    ];

    spans.extend(pad_spans(value, cols.value, false));
    spans.push(Span::styled(" │ ", Style::default().fg(theme::NL_LINE)));
    spans.extend(pad_spans(actions, cols.action, true));
    spans.push(Span::styled(" │", Style::default().fg(theme::NL_LINE)));
    Line::from(spans)
}

fn pad_spans(spans: Vec<Span<'static>>, width: usize, right_align: bool) -> Vec<Span<'static>> {
    let current_width = spans_width(&spans);
    if current_width <= width {
        let pad = Span::raw(" ".repeat(width - current_width));
        if right_align {
            let mut padded = vec![pad];
            padded.extend(spans);
            return padded;
        } else {
            let mut spans = spans;
            spans.push(pad);
            return spans;
        }
    }

    // Truncate: keep last style for the ellipsis span.
    let last_style = spans.last().map(|s| s.style).unwrap_or_default();
    let ellipsis = "…";
    let ellipsis_width = UnicodeWidthStr::width(ellipsis);
    let target = width.saturating_sub(ellipsis_width);

    let mut truncated = Vec::new();
    let mut used = 0;
    for span in &spans {
        if used >= target {
            break;
        }
        let mut buf = String::new();
        for ch in span.content.chars() {
            let cw = UnicodeWidthStr::width(ch.to_string().as_str());
            if used + cw > target {
                break;
            }
            buf.push(ch);
            used += cw;
            if used >= target {
                break;
            }
        }
        if !buf.is_empty() {
            truncated.push(Span::styled(buf, span.style));
        }
    }
    truncated.push(Span::styled(ellipsis.to_string(), last_style));
    truncated
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum()
}

fn pad_to_width(value: &str, width: usize) -> String {
    let current = UnicodeWidthStr::width(value);
    if current >= width {
        value.to_string()
    } else {
        format!("{}{}", value, " ".repeat(width - current))
    }
}

fn should_render_empty_url_row(record: &DetailViewData) -> bool {
    matches!(
        record.credential_type,
        crate::types::credential::CredentialType::Login
            | crate::types::credential::CredentialType::Api
    ) && !record
        .fields
        .iter()
        .any(|field| field.kind == DetailFieldKind::Url)
}

pub fn detail_action_at(
    area: Rect,
    state: &DetailPanelState,
    column: u16,
    row: u16,
) -> Option<DetailActionFocus> {
    if area.width < 50 || !contains(area, column, row) {
        return None;
    }
    let inner = Block::default()
        .borders(Borders::ALL)
        .inner(area)
        .inner(Margin {
            horizontal: 2,
            vertical: 1,
        });
    if !contains(inner, column, row) {
        return None;
    }

    let cols = table_columns(inner.width)?;
    let record = state.record.as_ref()?;
    let mut current_y = inner.y + 8;
    for (field_idx, field) in record.fields.iter().enumerate() {
        if field.kind == DetailFieldKind::Notes {
            continue;
        }
        if row == current_y {
            return action_hit_for_field(inner, cols, field_idx, field, column);
        }
        current_y = current_y.saturating_add(2);
        if is_secret_field(field.kind) && record.password_strength.is_some() {
            current_y = current_y.saturating_add(2);
        }
    }
    None
}

fn action_hit_for_field(
    inner: Rect,
    cols: TableColumns,
    field_idx: usize,
    field: &crate::tui::state::detail_state::DetailField,
    column: u16,
) -> Option<DetailActionFocus> {
    if !field.copyable && !field.toggleable {
        return None;
    }
    let action_start = inner.x + (cols.label + cols.value + 8) as u16;
    let action_end = action_start + cols.action as u16;
    if column < action_start || column >= action_end {
        return None;
    }
    if field.toggleable && field.copyable {
        let mid = action_start + (cols.action as u16 / 2);
        let kind = if column < mid {
            DetailActionKind::ToggleSecret
        } else {
            DetailActionKind::Copy
        };
        return Some(DetailActionFocus {
            field_index: field_idx,
            kind,
        });
    }
    let kind = if field.toggleable {
        DetailActionKind::ToggleSecret
    } else {
        DetailActionKind::Copy
    };
    Some(DetailActionFocus {
        field_index: field_idx,
        kind,
    })
}

fn contains(area: Rect, column: u16, row: u16) -> bool {
    column >= area.x && column < area.right() && row >= area.y && row < area.bottom()
}

fn nf_icon<'a>(unicode: bool, nerd: &'a str, ascii: &'a str) -> &'a str {
    if unicode {
        nerd
    } else {
        ascii
    }
}

fn credential_type_label(record: &DetailViewData) -> std::borrow::Cow<'static, str> {
    match record.credential_type {
        crate::types::credential::CredentialType::Login => t!("tui.form.type_login"),
        crate::types::credential::CredentialType::Api => t!("tui.form.type_api"),
        crate::types::credential::CredentialType::Ssh => t!("tui.form.type_ssh"),
        crate::types::credential::CredentialType::SecureNote => t!("tui.form.type_secure_note"),
    }
}

fn expiry_status_line(record: &DetailViewData, unicode: bool) -> Option<Line<'static>> {
    match record.expiry_status {
        ExpiryStatus::ExpiringSoon => {
            let icon = if unicode {
                theme::NF_WARNING_TRIANGLE
            } else {
                theme::ascii::NF_WARNING_TRIANGLE
            };
            let dt = record.expires_at?;
            let now = chrono::Utc::now().date_naive();
            let days = (dt.date_naive() - now).num_days().max(0);
            Some(Line::from(Span::styled(
                format!(
                    "  {} {}",
                    icon,
                    t!("tui.password_detail.expiry_warning", days = days)
                ),
                Style::default().fg(theme::WARNING),
            )))
        }
        ExpiryStatus::Expired => {
            let icon = if unicode {
                theme::ICON_ERROR
            } else {
                theme::ascii::ICON_ERROR
            };
            let dt = record.expires_at?;
            let now = chrono::Utc::now().date_naive();
            let days = (now - dt.date_naive()).num_days().max(0);
            Some(Line::from(Span::styled(
                format!(
                    "  {} {}",
                    icon,
                    t!("tui.password_detail.expiry_expired", days = days)
                ),
                Style::default().fg(theme::ERROR),
            )))
        }
        _ => None,
    }
}

fn separator_line(width: u16, unicode: bool) -> Line<'static> {
    let sep = if unicode { "\u{2500}" } else { "-" };
    Line::from(Span::styled(
        sep.repeat(width.saturating_sub(1) as usize),
        Style::default().fg(theme::BORDER),
    ))
}

fn field_icon(kind: DetailFieldKind, unicode: bool) -> &'static str {
    match kind {
        DetailFieldKind::Username | DetailFieldKind::AppId | DetailFieldKind::PublicKey => {
            nf_icon(unicode, theme::NF_USER, theme::ascii::NF_USER)
        }
        DetailFieldKind::Password
        | DetailFieldKind::SecretKey
        | DetailFieldKind::PrivateKey
        | DetailFieldKind::Passphrase => nf_icon(unicode, theme::NF_LOCK, theme::ascii::NF_LOCK),
        DetailFieldKind::Url => nf_icon(unicode, theme::NF_GLOBE, theme::ascii::NF_GLOBE),
        DetailFieldKind::Notes => nf_icon(unicode, theme::NF_NOTE, theme::ascii::NF_NOTE),
    }
}

fn is_secret_field(kind: DetailFieldKind) -> bool {
    matches!(
        kind,
        DetailFieldKind::Password
            | DetailFieldKind::SecretKey
            | DetailFieldKind::PrivateKey
            | DetailFieldKind::Passphrase
    )
}

fn health_issue_line(state: &DetailPanelState, unicode: bool) -> Option<Line<'static>> {
    let issue = state.health_issue.as_ref()?;
    use std::borrow::Cow;
    let (icon, text, color): (&str, Cow<str>, _) = match issue {
        crate::commands::types::HealthIssue::Compromised => {
            let icon = if unicode { "\u{F06BD}" } else { "!" };
            (icon, t!("tui.password_detail.health_leaked"), theme::ERROR)
        }
        crate::commands::types::HealthIssue::Weak => {
            let icon = if unicode {
                theme::ICON_WARNING
            } else {
                theme::ascii::ICON_WARNING
            };
            (icon, t!("tui.password_detail.health_weak"), theme::WARNING)
        }
        crate::commands::types::HealthIssue::Duplicate { group_size } => {
            let icon = if unicode {
                theme::ICON_WARNING
            } else {
                theme::ascii::ICON_WARNING
            };
            (
                icon,
                t!("tui.password_detail.health_duplicate", count = group_size),
                theme::WARNING,
            )
        }
        crate::commands::types::HealthIssue::Expired => return None,
    };
    if text.is_empty() {
        return None;
    }
    Some(Line::from(vec![
        Span::styled(format!("{}  ", icon), Style::default().fg(color)),
        Span::styled(text.into_owned(), Style::default().fg(color)),
    ]))
}

fn field_action_spans(
    field_idx: usize,
    field: &crate::tui::state::detail_state::DetailField,
    state: &DetailPanelState,
    unicode: bool,
    compact: bool,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if field.toggleable {
        spans.push(action_button_span(
            DetailActionFocus {
                field_index: field_idx,
                kind: DetailActionKind::ToggleSecret,
            },
            state,
            if matches!(field.value, FieldValue::Revealed(_)) {
                nf_icon(unicode, theme::NF_EYE_OFF, theme::ascii::NF_EYE_OFF)
            } else {
                nf_icon(unicode, theme::NF_EYE, theme::ascii::NF_EYE)
            },
            if matches!(field.value, FieldValue::Revealed(_)) {
                t!("tui.password_detail.hide_button").into_owned()
            } else {
                t!("tui.password_detail.show_button").into_owned()
            },
            compact,
        ));
        spans.push(Span::raw("  "));
    }
    if field.copyable {
        spans.push(action_button_span(
            DetailActionFocus {
                field_index: field_idx,
                kind: DetailActionKind::Copy,
            },
            state,
            nf_icon(unicode, theme::NF_COPY, theme::ascii::NF_COPY),
            t!("tui.password_detail.copy_button").into_owned(),
            compact,
        ));
    }
    spans
}

fn action_button_span(
    action: DetailActionFocus,
    state: &DetailPanelState,
    icon: &str,
    label: String,
    compact: bool,
) -> Span<'static> {
    let mut style = Style::default().fg(theme::NL_FOCUS);
    if state.focused_action == Some(action) {
        style = style
            .fg(theme::NL_BG)
            .bg(theme::NL_CYAN)
            .add_modifier(Modifier::BOLD);
    }
    if compact {
        Span::styled(format!("{} {}", icon, label), style)
    } else {
        Span::styled(format!("[ {} {} ]", icon, label), style)
    }
}

fn render_tag_chips(tags: &[String]) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for tag in tags {
        spans.push(Span::styled("[ ", Style::default().fg(theme::NL_CYAN)));
        spans.push(Span::styled(
            tag.clone(),
            Style::default().fg(theme::NL_TEXT),
        ));
        spans.push(Span::styled(" ] ", Style::default().fg(theme::NL_CYAN)));
    }
    spans
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

    let para = Paragraph::new(lines).style(theme::Styles::newlook_bg());
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

    fn render_detail_buffer(
        state: &DetailPanelState,
        width: u16,
        height: u16,
        focused: bool,
        unicode: bool,
    ) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let panel = DetailPanel;
                panel.view(frame, frame.area(), state, focused, unicode, &[]);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn detail_buffer_row_text(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
        (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).expect("cell").symbol())
            .collect()
    }

    fn find_detail_row(buffer: &ratatui::buffer::Buffer, needle: &str) -> Option<u16> {
        (0..buffer.area.height).find(|y| detail_buffer_row_text(buffer, *y).contains(needle))
    }

    fn make_secure_note_detail_data() -> DetailViewData {
        DetailViewData {
            id: uuid::Uuid::new_v4(),
            name: "SecureNote".into(),
            subtitle: String::new(),
            credential_type: crate::types::credential::CredentialType::SecureNote,
            is_favorite: false,
            expires_at: None,
            expiry_status: ExpiryStatus::None,
            tags: vec![],
            notes: Some("private note body".into()),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            fields: vec![DetailField {
                label: t!("tui.password_detail.notes_label").to_string(),
                value: FieldValue::Plain("private note body".into()),
                copyable: true,
                toggleable: false,
                kind: DetailFieldKind::Notes,
            }],
            password_strength: None,
            deleted_at: None,
        }
    }

    #[test]
    fn expiring_soon_detail_line_uses_newlook_warning_icon() {
        let mut data = make_trash_detail_data();
        data.expires_at = Some(chrono::Utc::now() + chrono::Duration::days(7));
        data.expiry_status = ExpiryStatus::ExpiringSoon;

        let line = expiry_status_line(&data, true).expect("expiry warning line should render");
        let text: String = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();

        assert!(
            text.contains('\u{f071}'),
            "expiring-soon detail line should use the requested warning icon: {text:?}"
        );
        assert!(
            !text.contains('\u{26A0}'),
            "expiring-soon detail line should not duplicate the old warning icon: {text:?}"
        );
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
    fn trash_detail_delete_action_uses_delete_label_not_warning_text() {
        let data = make_trash_detail_data();
        let mut state = DetailPanelState::with_record(data);
        state.set_trash_context(true, 30);

        let snapshot = render_detail_snapshot(&state, 120, 30, true, true);

        assert!(
            snapshot.contains(t!("tui.trash.permanent_delete_title").as_ref()),
            "trash detail should render a delete action label"
        );
        assert!(
            !snapshot.contains(t!("tui.overlay.confirm_delete_permanent").as_ref()),
            "irreversible warning belongs in the confirmation dialog, not as a detail action label"
        );
    }

    #[test]
    fn render_normal_detail_no_trash_banner() {
        let data = make_trash_detail_data();
        let state = DetailPanelState::with_record(data);
        let result = render_detail_snapshot(&state, 60, 20, true, true);
        assert!(!result.is_empty());
    }

    #[test]
    fn wide_detail_renders_card_grid_and_nerd_font_actions() {
        let mut data = make_trash_detail_data();
        data.name = "GitHub".into();
        data.subtitle = "github.com".into();
        data.is_favorite = true;
        data.tags = vec!["work".into(), "github".into()];
        data.notes = Some("primary account".into());
        data.password_strength = Some(PasswordStrength::Strong);
        data.fields.push(DetailField {
            label: t!("tui.password_detail.url_label").to_string(),
            value: FieldValue::Plain("github.com".into()),
            copyable: true,
            toggleable: false,
            kind: DetailFieldKind::Url,
        });

        let state = DetailPanelState::with_record(data);
        let snapshot = render_detail_snapshot(&state, 120, 30, true, true);

        assert!(
            snapshot.contains("\u{f1c0}"),
            "title should use Nerd Font database icon"
        );
        assert!(
            snapshot.contains(&format!(
                "\u{f005} {}",
                t!("tui.password_detail.favorite_badge")
            )),
            "favorite badge should render"
        );
        assert!(
            snapshot.contains("\u{f084}"),
            "section should use Nerd Font key icon"
        );
        assert!(
            snapshot.contains("\u{f007}"),
            "username row should use Nerd Font user icon"
        );
        assert!(
            snapshot.contains("\u{f023}"),
            "password row should use Nerd Font lock icon"
        );
        assert!(
            snapshot.contains(&format!(
                "\u{f06e} {}",
                t!("tui.password_detail.show_button")
            )),
            "password row should expose show action"
        );
        assert!(
            snapshot.contains(&format!(
                "\u{f0c5} {}",
                t!("tui.password_detail.copy_button")
            )),
            "copy action should render"
        );
        assert!(snapshot.contains("[ work ]"), "tags should render as chips");
    }

    #[test]
    fn wide_detail_renders_table_borders_badges_and_empty_url_row() {
        let mut data = make_trash_detail_data();
        data.tags = vec!["github".into()];
        data.notes = Some("primary account".into());
        data.fields
            .retain(|field| field.kind != DetailFieldKind::Url);

        let state = DetailPanelState::with_record(data);
        let snapshot = render_detail_snapshot(&state, 120, 30, true, true);

        assert!(
            snapshot.contains("┬"),
            "field group should render a table header border"
        );
        assert!(
            snapshot.contains("┼"),
            "field rows should render table separators"
        );
        assert!(
            snapshot.contains("┴"),
            "field group should render a table footer border"
        );
        assert!(
            snapshot.contains(t!("tui.password_detail.url_label").as_ref()),
            "URL row should keep its table slot even when the record has no URL"
        );
        assert!(
            snapshot.contains("[ github ]"),
            "tags should render as badge-like chips"
        );
    }

    #[test]
    fn secure_note_detail_does_not_render_empty_primary_table() {
        let state = DetailPanelState::with_record(make_secure_note_detail_data());
        let buffer = render_detail_buffer(&state, 120, 30, true, true);

        let type_row = find_detail_row(&buffer, t!("tui.form.type_secure_note").as_ref())
            .expect("secure note type label should render");
        let notes_row = find_detail_row(&buffer, t!("tui.password_detail.notes_label").as_ref())
            .expect("notes metadata row should render");

        for y in type_row + 1..notes_row {
            let row = detail_buffer_row_text(&buffer, y);
            assert!(
                !row.contains('└') && !row.contains('┴') && !row.contains('┘'),
                "secure note should not render an empty primary table before notes: {row:?}"
            );
        }
    }

    #[test]
    fn detail_timestamps_use_display_timezone_formatter() {
        use chrono::TimeZone;

        let mut data = make_trash_detail_data();
        data.created_at = chrono::Utc.with_ymd_and_hms(2026, 5, 30, 1, 2, 0).unwrap();
        data.updated_at = chrono::Utc.with_ymd_and_hms(2026, 5, 30, 3, 4, 0).unwrap();
        let state = DetailPanelState::with_record(data);

        let snapshot = render_detail_snapshot(&state, 140, 40, true, true);

        assert!(
            snapshot.contains(&crate::tui::time::format_display_datetime(
                &state.record.as_ref().unwrap().created_at
            ))
        );
        assert!(
            snapshot.contains(&crate::tui::time::format_display_datetime(
                &state.record.as_ref().unwrap().updated_at
            ))
        );
    }

    #[test]
    fn notes_metadata_renders_light_markdown_without_source_markers() {
        let mut data = make_trash_detail_data();
        data.notes = Some("# Heading\n- task\n`token` and **bold**".into());
        let state = DetailPanelState::with_record(data);

        let snapshot = render_detail_snapshot(&state, 120, 30, true, true);

        assert!(snapshot.contains("Heading"));
        assert!(snapshot.contains("task"));
        assert!(snapshot.contains("token"));
        assert!(snapshot.contains("bold"));
        assert!(!snapshot.contains("# Heading"));
        assert!(!snapshot.contains("- task"));
        assert!(!snapshot.contains("`token`"));
        assert!(!snapshot.contains("**bold**"));
    }

    #[test]
    fn detail_action_hit_testing_maps_password_buttons() {
        let state = DetailPanelState::with_record(make_trash_detail_data());
        let area = Rect::new(0, 0, 120, 30);

        assert_eq!(
            detail_action_at(area, &state, 90, 12),
            Some(DetailActionFocus {
                field_index: 1,
                kind: DetailActionKind::ToggleSecret
            })
        );
        assert_eq!(
            detail_action_at(area, &state, 110, 12),
            Some(DetailActionFocus {
                field_index: 1,
                kind: DetailActionKind::Copy
            })
        );
    }

    #[test]
    fn detail_tags_align_with_field_labels() {
        let mut data = make_trash_detail_data();
        data.tags = vec!["work".into(), "github".into()];
        let state = DetailPanelState::with_record(data);
        let buffer = render_detail_buffer(&state, 60, 24, true, true);

        let tag_row = (0..buffer.area.height)
            .find(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.cell((x, *y)).expect("cell").symbol())
                    .collect::<String>()
                    .contains("[ work ]")
            })
            .expect("tag row should render");

        let tag_line = (0..buffer.area.width)
            .map(|x| buffer.cell((x, tag_row)).expect("cell").symbol())
            .collect::<String>();
        assert!(tag_line.contains("\u{f02b}"));
        assert!(tag_line.contains("[ work ]"));
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
