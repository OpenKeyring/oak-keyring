use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::screens::import_export::ScopeHintStyle;
use crate::tui::theme::{
    self, BORDER, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_SECONDARY, WARNING,
};

use super::screen::OnboardingScreen;

impl OnboardingScreen {
    pub(crate) fn view_import_source(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        use crate::tui::screens::import_export::{
            source_needs_password, ImportFocus, IMPORT_SOURCES,
        };

        let content_area = Self::centered_content(area, 20);
        let source = IMPORT_SOURCES[self.selected_source_idx].0;
        let needs_pw = source_needs_password(source);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                "Step 2/6 · Select Import Source",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        for (i, (_, name, _, (hint_text, hint_style_enum))) in IMPORT_SOURCES.iter().enumerate() {
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
                Span::styled((*name).to_string(), name_style),
                Span::styled(format!("  {}", hint_text), hint_style),
            ]));
        }

        lines.push(Line::raw(""));

        // Import Scope section
        let scope_separator = format!("\u{2500}\u{2500} Import Scope {}", "\u{2500}".repeat(45));
        lines.push(Line::from(Span::styled(
            scope_separator,
            Style::default().fg(BORDER),
        )));

        let scope_items: [(Color, &str, &str); 5] = [
            (
                SUCCESS,
                theme::ICON_SUCCESS,
                "Login items (name, account, password, URL, notes)",
            ),
            (
                ERROR,
                theme::ICON_ERROR,
                "TOTP / 2FA (not supported in current version, discarded during import)",
            ),
            (
                WARNING,
                theme::ICON_WARNING,
                "Custom fields (formatted and stored in notes field)",
            ),
            (SUCCESS, theme::ICON_SUCCESS, "Password history records"),
            (
                ERROR,
                theme::ICON_ERROR,
                "Attachments (ignored during import)",
            ),
        ];

        for (color, icon, text) in &scope_items {
            lines.push(Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(*color)),
                Span::styled(*text, Style::default().fg(TEXT_SECONDARY)),
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
            Span::styled("File Path: ", Style::default().fg(TEXT)),
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
                Span::styled("Password: ", Style::default().fg(TEXT)),
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
            "\u{2191}\u{2193}: navigate | Tab: cycle fields | Enter: validate | Esc: back",
            Style::default().fg(TEXT_MUTED),
        )));

        frame.render_widget(Paragraph::new(lines), content_area);
    }

    pub(crate) fn view_import_preview(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 18);

        let mut lines: Vec<Line> = vec![
            Line::from(Span::styled(
                "Step 3/6 · Import Preview",
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::raw(""),
        ];

        if let Some(ref preview) = self.import_preview {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("Importable: {}", preview.importable),
                    Style::default().fg(SUCCESS),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("Needs review: {}", preview.needs_review),
                    Style::default().fg(WARNING),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("Failed: {}", preview.failed),
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
            lines.push(Line::from("No preview data available"));
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
                " {} Import problematic entries as notes (instead of skipping)",
                check_icon
            ),
            check_style,
        )));

        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "Tab: toggle focus | Space/Enter: toggle checkbox | Enter: start import | Esc: back",
            Style::default().fg(TEXT_MUTED),
        )));

        frame.render_widget(Paragraph::new(lines), content_area);
    }
}
