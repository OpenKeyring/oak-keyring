use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::t;
use crate::tui::screens::import_export::ScopeHintStyle;
use crate::tui::terminal::WidthTier;
use crate::tui::theme::{
    self, BORDER, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_SECONDARY, WARNING,
};

use super::screen::OnboardingScreen;
use super::views_setup::{render_header, header_rows};

impl OnboardingScreen {
    pub(crate) fn view_import_source(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        use crate::tui::screens::import_export::{
            import_sources, source_needs_password, ImportFocus,
        };

        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let content_area = Self::centered_content(area, hdr + 20, 60);

        let rows = Layout::vertical([
            Constraint::Length(hdr),     // logo or brand
            Constraint::Min(0),          // content
        ])
        .split(content_area);

        render_header(frame, rows[0], wide);

        let content_area = rows[1];
        let sources = import_sources();
        let source = sources[self.selected_source_idx].0;
        let needs_pw = source_needs_password(source);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                t!("tui.entry.step_onboarding_import_source"),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        for (i, (_, name, _, (hint_text, hint_style_enum))) in sources.iter().enumerate() {
            let prefix = if i == self.selected_source_idx {
                " \u{25B6} "
            } else {
                "   "
            };
            let is_focused =
                i == self.selected_source_idx && self.import_focus == ImportFocus::SourceList;
            let name_style = if is_focused {
                Style::default().fg(PRIMARY).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            let hint_color = match hint_style_enum {
                ScopeHintStyle::Full => SUCCESS,
                ScopeHintStyle::Partial => WARNING,
                ScopeHintStyle::Limited => ERROR,
            };
            let hint_style = Style::default().fg(hint_color);
            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), name_style),
                Span::styled(name.as_str(), name_style),
                Span::styled(format!("  {}", hint_text), hint_style),
            ]));
        }

        lines.push(Line::raw(""));

        // Import Scope section
        let scope_separator = format!(
            "{} {}",
            t!("tui.entry.import_scope_separator"),
            "\u{2500}".repeat(30)
        );
        lines.push(Line::from(Span::styled(
            scope_separator,
            Style::default().fg(BORDER),
        )));

        let scope_items: [(Color, &str, String); 5] = [
            (
                SUCCESS,
                theme::ICON_SUCCESS,
                t!("tui.entry.import_scope_logins").to_string(),
            ),
            (
                ERROR,
                theme::ICON_ERROR,
                t!("tui.entry.import_scope_totp").to_string(),
            ),
            (
                WARNING,
                theme::ICON_WARNING,
                t!("tui.entry.import_scope_custom").to_string(),
            ),
            (
                SUCCESS,
                theme::ICON_SUCCESS,
                t!("tui.entry.import_scope_history").to_string(),
            ),
            (
                ERROR,
                theme::ICON_ERROR,
                t!("tui.entry.import_scope_attachments").to_string(),
            ),
        ];

        for (color, icon, text) in &scope_items {
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(*color)),
                Span::styled(text.as_str(), Style::default().fg(TEXT_SECONDARY)),
            ]));
        }

        lines.push(Line::raw(""));

        let fp_style = if self.import_focus == ImportFocus::FilePath {
            Style::default().fg(PRIMARY)
        } else {
            Style::default().fg(TEXT_MUTED)
        };
        let fp_text = if self.import_file_path.is_empty() {
            "/path/to/file".to_string()
        } else {
            self.import_file_path.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(
                t!("tui.entry.import_file_path_label"),
                Style::default().fg(TEXT),
            ),
            Span::styled(fp_text, fp_style),
        ]));

        if needs_pw {
            let pw_style = if self.import_focus == ImportFocus::Password {
                Style::default().fg(PRIMARY)
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            let pw_display = if self.import_password.is_empty() {
                "password".to_string()
            } else {
                "*".repeat(self.import_password.len())
            };
            lines.push(Line::from(vec![
                Span::styled(
                    t!("tui.entry.import_password_label"),
                    Style::default().fg(TEXT),
                ),
                Span::styled(pw_display, pw_style),
            ]));
        }

        if let Some(ref err) = self.error {
            lines.push(Line::from(Span::styled(
                err.clone(),
                Style::default().fg(ERROR),
            )));
        }

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            t!("tui.entry.import_source_navigate_hint"),
            Style::default().fg(TEXT_MUTED),
        )));

        frame.render_widget(Paragraph::new(lines), content_area);
    }

    pub(crate) fn view_import_preview(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let content_area = Self::centered_content(area, hdr + 18, 60);

        let rows = Layout::vertical([
            Constraint::Length(hdr),     // logo or brand
            Constraint::Min(0),          // content
        ])
        .split(content_area);

        render_header(frame, rows[0], wide);

        let content_area = rows[1];

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                t!("tui.entry.step_onboarding_import_preview"),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        if let Some(ref preview) = self.import_preview {
            lines.push(Line::from(vec![
                Span::styled(
                    t!("tui.entry.importable_label", count = preview.importable).to_string(),
                    Style::default().fg(SUCCESS),
                ),
                Span::raw("  "),
                Span::styled(
                    t!("tui.entry.needs_review_label", count = preview.needs_review).to_string(),
                    Style::default().fg(WARNING),
                ),
                Span::raw("  "),
                Span::styled(
                    t!("tui.entry.failed_label", count = preview.failed).to_string(),
                    Style::default().fg(ERROR),
                ),
            ]));
            lines.push(Line::raw(""));

            for item in preview.review_items.iter().take(5) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", theme::ICON_WARNING),
                        Style::default().fg(WARNING),
                    ),
                    Span::raw(format!("{} \u{2014} {}", item.name, item.reason)),
                ]));
            }

            for item in preview.failed_items.iter().take(5) {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", theme::ICON_ERROR),
                        Style::default().fg(ERROR),
                    ),
                    Span::raw(format!("{} \u{2014} {}", item.name, item.reason)),
                ]));
            }
        } else {
            lines.push(Line::from(t!("tui.entry.no_preview_data")));
        }

        lines.push(Line::raw(""));

        // Checkbox: "Import problematic entries as notes (instead of skipping)"
        let check_icon = if self.import_as_notes {
            theme::ICON_CHECK // ☑
        } else {
            "\u{2610}" // ☐
        };
        let check_style = if self.import_as_notes {
            Style::default().fg(SUCCESS)
        } else if self.import_preview_checkbox_focused {
            Style::default().fg(PRIMARY)
        } else {
            Style::default().fg(TEXT_SECONDARY)
        };
        lines.push(Line::from(Span::styled(
            format!(
                " {}{}",
                check_icon,
                t!("tui.entry.import_as_notes_checkbox")
            ),
            check_style,
        )));

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            t!("tui.entry.import_preview_toggle_hint"),
            Style::default().fg(TEXT_MUTED),
        )));

        frame.render_widget(Paragraph::new(lines), content_area);
    }
}
