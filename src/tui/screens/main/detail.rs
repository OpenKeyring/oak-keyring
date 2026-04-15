//! Password detail panel: credential fields, health line, metadata.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::state::detail_state::{
    DetailFieldKind, DetailPanelState, DetailViewData, ExpiryStatus, FieldValue, PasswordStrength,
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
    ) {
        match &state.record {
            None => self.render_empty(frame, area, unicode),
            Some(record) => self.render_record(frame, area, state, record, focused, unicode),
        }
    }

    fn render_empty(&self, frame: &mut Frame, area: Rect, unicode: bool) {
        let icon = if unicode { "\u{1F510}" } else { "[?]" };
        let lines = vec![
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", icon),
                Style::default().fg(theme::TEXT_MUTED),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Select an item to view details".to_string(),
                Style::default().fg(theme::TEXT_SECONDARY),
            )),
            Line::from(Span::styled(
                "  Press Enter to view details".to_string(),
                Style::default().fg(theme::TEXT_MUTED),
            )),
        ];
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
            let star = if unicode { "\u{2605}" } else { "*" };
            markers.push(Span::styled(
                format!("{} ", star),
                Style::default().fg(theme::BRAND),
            ));
        }
        match record.expiry_status {
            ExpiryStatus::ExpiringSoon => {
                let icon = if unicode { "\u{26A0}" } else { "!" };
                if let Some(dt) = record.expires_at {
                    markers.push(Span::styled(
                        format!("{} 即将过期（{}）", icon, dt.format("%Y-%m-%d")),
                        Style::default().fg(theme::WARNING),
                    ));
                }
            }
            ExpiryStatus::Expired => {
                let icon = if unicode { "\u{2717}" } else { "x" };
                if let Some(dt) = record.expires_at {
                    markers.push(Span::styled(
                        format!("{} 已过期（{}）", icon, dt.format("%Y-%m-%d")),
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
            crate::types::credential::CredentialType::Login => "\u{767B}\u{5F55}\u{4FE1}\u{606F}",
            crate::types::credential::CredentialType::Api => "API \u{51ED}\u{8BC1}",
            crate::types::credential::CredentialType::Ssh => "SSH \u{5BC6}\u{94A5}",
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
                    let bar = self.render_strength_bar(strength, 16);
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
            let (text, color) = match issue {
                crate::commands::types::HealthIssue::Compromised => {
                    (
                        "\u{6B64}\u{5BC6}\u{7801}\u{5DF2}\u{5728}\u{6570}\u{636E}\u{6CC4}\u{9732}\u{4E2D}\u{53D1}\u{73B0} \u{2014} \u{8BF7}\u{7ACB}\u{5373}\u{4FEE}\u{6539}".to_string(),
                        theme::ERROR,
                    )
                }
                crate::commands::types::HealthIssue::Weak => {
                    (
                        "\u{5F31}\u{5BC6}\u{7801} \u{2014} \u{5EFA}\u{8BAE}\u{66F4}\u{65B0}\u{4E3A}\u{66F4}\u{5F3A}\u{7684}\u{5BC6}\u{7801}".to_string(),
                        theme::WARNING,
                    )
                }
                crate::commands::types::HealthIssue::Duplicate { group_size } => {
                    (
                        format!(
                            "\u{8BE5}\u{5BC6}\u{7801}\u{4E0E}\u{5176}\u{4ED6} {} \u{6761}\u{8BB0}\u{5F55}\u{91CD}\u{590D} \u{2014} \u{5EFA}\u{8BAE}\u{4F7F}\u{7528}\u{72EC}\u{7ACB}\u{5BC6}\u{7801}",
                            group_size
                        ),
                        theme::WARNING,
                    )
                }
                crate::commands::types::HealthIssue::Expired => (String::new(), theme::TEXT_MUTED),
            };
            if !text.is_empty() {
                lines.push(Line::from(Span::styled(
                    format!(
                        "{}\u{26A0} {}",
                        pad, text
                    ),
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

        // Timestamps
        lines.push(Line::from(Span::styled(
            format!(
                "{}\u{521B}\u{5EFA}: {}  \u{66F4}\u{65B0}: {}",
                pad,
                record.created_at.format("%Y-%m-%d %H:%M"),
                record.updated_at.format("%Y-%m-%d %H:%M"),
            ),
            Style::default().fg(theme::TEXT_MUTED),
        )));

        let para = Paragraph::new(lines);
        frame.render_widget(para, area);
    }

    fn render_strength_bar(&self, strength: &PasswordStrength, width: usize) -> String {
        let filled = (strength.fraction() * width as f32).round() as usize;
        let empty = width - filled;
        format!(
            "\u{5F3A}\u{5EA6}: {}{} {}",
            "\u{2588}".repeat(filled),
            "\u{2591}".repeat(empty),
            strength.label()
        )
    }
}
