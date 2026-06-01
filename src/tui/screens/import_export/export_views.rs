use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::t;
use crate::tui::components::text_input;
use crate::tui::theme::{self, Styles, TEXT_MUTED, TEXT_PLACEHOLDER};

use super::screen::ImportExportScreen;
use super::types::*;

fn localized_strength_level(level: &crate::crypto::strength::StrengthLevel) -> String {
    match level {
        crate::crypto::strength::StrengthLevel::VeryWeak => {
            t!("tui.generator.strength_too_weak").to_string()
        }
        crate::crypto::strength::StrengthLevel::Weak => {
            t!("tui.generator.strength_weak").to_string()
        }
        crate::crypto::strength::StrengthLevel::Fair => {
            t!("tui.generator.strength_fair").to_string()
        }
        crate::crypto::strength::StrengthLevel::Strong => {
            t!("tui.generator.strength_strong").to_string()
        }
        crate::crypto::strength::StrengthLevel::VeryStrong => {
            t!("tui.generator.strength_very_strong").to_string()
        }
    }
}

// ── View: Export Form ──────────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_export_form(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(22),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(74),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::newlook_focused_border())
            .style(Styles::newlook_bg());
        let mut inner = block.inner(content_area);
        if inner.width > 4 {
            inner.x += 2;
            inner.width -= 4;
        }
        frame.render_widget(block, content_area);

        let title = Paragraph::new(Line::from(vec![
            Span::styled(theme::NF_DOWNLOAD, Style::default().fg(theme::NL_CYAN)),
            Span::raw("  "),
            Span::styled(
                t!("tui.import_export.export_title").to_string(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);

        let format_info = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{}  ", theme::NF_INFO),
                Style::default().fg(theme::NL_CYAN),
            ),
            Span::styled(
                t!("tui.import_export.export_format_label").to_string(),
                Style::default().fg(theme::NL_TEXT_MUTED),
            ),
            Span::raw(" "),
            Span::styled(
                t!("tui.import_export.format_okb").to_string(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .style(Styles::newlook_bg());
        let format_note = Paragraph::new(format!(
            "{} {}",
            theme::ICON_INFO,
            t!("tui.import_export.format_info")
        ))
        .style(Style::default().fg(theme::NL_TEXT_MUTED))
        .wrap(Wrap { trim: true });

        // Export password
        let pw_border = if self.export_focus == ExportFocus::ExportPassword {
            Styles::newlook_focused_border()
        } else {
            Styles::newlook_border()
        };
        let pw_block = Block::default()
            .borders(Borders::ALL)
            .border_style(pw_border)
            .style(Styles::newlook_bg())
            .title(t!("tui.import_export.export_password_label").to_string());
        let pw_value = self
            .export_password
            .expose(|pw| crate::tui::theme::ICON_PASSWORD_MASK.repeat(pw.chars().count()));

        // Strength bar
        let strength_line = if let Some(ref s) = self.export_password_strength {
            let bar_total = 16u8;
            let filled = s.bar_fill.min(bar_total);
            let empty = bar_total - filled;
            let bar_str = format!(
                "{}{}",
                theme::ICON_PROGRESS_FILL.repeat(filled as usize),
                theme::ICON_PROGRESS_EMPTY.repeat(empty as usize)
            );
            let label = format!(
                "{}{} {}",
                t!("tui.import_export.strength_label_short"),
                localized_strength_level(&s.level),
                bar_str
            );
            let color = Self::strength_color(&s.level);
            Paragraph::new(label).style(Style::default().fg(color).bg(theme::NL_BG))
        } else {
            let label = t!("tui.import_export.strength_label_short").to_string();
            Paragraph::new(label).style(Style::default().fg(TEXT_MUTED).bg(theme::NL_BG))
        };

        // Confirm password
        let confirm_border = if self.export_focus == ExportFocus::ConfirmPassword {
            Styles::newlook_focused_border()
        } else {
            Styles::newlook_border()
        };
        let confirm_block = Block::default()
            .borders(Borders::ALL)
            .border_style(confirm_border)
            .style(Styles::newlook_bg())
            .title(t!("tui.import_export.confirm_password_label").to_string());
        let confirm_value = self
            .export_confirm_password
            .expose(|pw| crate::tui::theme::ICON_PASSWORD_MASK.repeat(pw.chars().count()));

        // Match indicator
        let match_line =
            if !self.export_password.is_empty() && !self.export_confirm_password.is_empty() {
                let passwords_match = self
                    .export_password
                    .expose(|pw1| self.export_confirm_password.expose(|pw2| pw1 == pw2));
                if passwords_match {
                    Some(
                        Paragraph::new(format!(
                            "{} {}",
                            theme::ICON_SUCCESS,
                            t!("tui.import_export.passwords_match")
                        ))
                        .style(Styles::success_text()),
                    )
                } else {
                    Some(
                        Paragraph::new(format!(
                            "{} {}",
                            theme::ICON_ERROR,
                            t!("tui.import_export.passwords_mismatch")
                        ))
                        .style(Styles::error_text()),
                    )
                }
            } else {
                None
            };

        // Output path
        let path_border = if self.export_focus == ExportFocus::OutputPath {
            Styles::newlook_focused_border()
        } else {
            Styles::newlook_border()
        };
        let path_block = Block::default()
            .borders(Borders::ALL)
            .border_style(path_border)
            .style(Styles::newlook_bg())
            .title(t!("tui.import_export.output_path_label").to_string());
        let path_value = self.export_output_path.clone();

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        // Hint
        let hint = Paragraph::new(t!("tui.import_export.mode_hint_export").to_string())
            .style(Style::default().fg(TEXT_MUTED).bg(theme::NL_BG))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // 0 title
            Constraint::Length(1), // 1 gap
            Constraint::Length(1), // 2 format
            Constraint::Length(2), // 3 note
            Constraint::Length(3), // 4 export password
            Constraint::Length(1), // 5 strength
            Constraint::Length(3), // 6 confirm password
            Constraint::Length(1), // 7 match indicator
            Constraint::Length(1), // 8 gap
            Constraint::Length(3), // 9 output path
            Constraint::Length(1), // 10 error or gap
            Constraint::Length(1), // 11 hint
        ])
        .split(inner);

        frame.render_widget(title, rows[0]);
        frame.render_widget(format_info, rows[2]);
        frame.render_widget(format_note, rows[3]);

        let pw_inner = pw_block.inner(rows[4]);
        frame.render_widget(pw_block, rows[4]);
        let pw_display = export_input_paragraph(
            &pw_value,
            &t!("tui.import_export.export_password_placeholder").to_string(),
            pw_value.len(),
            self.export_focus == ExportFocus::ExportPassword,
            pw_inner.width,
        );
        frame.render_widget(pw_display, pw_inner);

        frame.render_widget(strength_line, rows[5]);

        let confirm_inner = confirm_block.inner(rows[6]);
        frame.render_widget(confirm_block, rows[6]);
        let confirm_display = export_input_paragraph(
            &confirm_value,
            &t!("tui.import_export.confirm_password_placeholder").to_string(),
            confirm_value.len(),
            self.export_focus == ExportFocus::ConfirmPassword,
            confirm_inner.width,
        );
        frame.render_widget(confirm_display, confirm_inner);

        if let Some(ref ml) = match_line {
            frame.render_widget(ml.clone(), rows[7]);
        }

        let path_inner = path_block.inner(rows[9]);
        frame.render_widget(path_block, rows[9]);
        let path_display = export_input_paragraph(
            &path_value,
            &t!("tui.import_export.output_path_placeholder").to_string(),
            self.export_output_path_cursor,
            self.export_focus == ExportFocus::OutputPath,
            path_inner.width,
        );
        frame.render_widget(path_display, path_inner);

        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[10]);
        }

        frame.render_widget(hint, rows[11]);
    }
}

fn export_input_paragraph(
    value: &str,
    placeholder: &str,
    cursor: usize,
    focused: bool,
    width: u16,
) -> Paragraph<'static> {
    let spans = text_input::render_bare_input_spans_at_cursor(
        value,
        placeholder,
        cursor,
        width as usize,
        focused,
        Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG),
        Style::default().fg(TEXT_PLACEHOLDER).bg(theme::NL_BG),
    );
    Paragraph::new(Line::from(spans)).style(Styles::newlook_bg())
}

fn render_newlook_center_panel(
    frame: &mut ratatui::Frame,
    area: Rect,
    height: u16,
    max_width: u16,
) -> Rect {
    let outer = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);
    let center_area = outer[1];
    let h_layout = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Max(max_width),
        Constraint::Fill(1),
    ])
    .split(center_area);
    let content_area = h_layout[1];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Styles::newlook_focused_border())
        .style(Styles::newlook_bg());
    let mut inner = block.inner(content_area);
    if inner.width > 4 {
        inner.x += 2;
        inner.width -= 4;
    }
    frame.render_widget(block, content_area);
    inner
}

// ── View: Export Master Password Confirm ───────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_export_master_password_confirm(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
    ) {
        let content_area = render_newlook_center_panel(frame, area, 14, 74);

        let title = Paragraph::new(Line::from(vec![
            Span::styled(theme::NF_DOWNLOAD, Style::default().fg(theme::NL_CYAN)),
            Span::raw("  "),
            Span::styled(
                t!("tui.import_export.authorize_title").to_string(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);

        let subtitle = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{}  ", theme::NF_INFO),
                Style::default().fg(theme::NL_CYAN),
            ),
            Span::styled(
                t!(
                    "tui.import_export.authorize_subtitle",
                    scope = t!("tui.import_export.scope_all").to_string(),
                    path = self.export_output_path
                )
                .to_string(),
                Style::default().fg(theme::NL_TEXT_MUTED),
            ),
        ]))
        .style(Styles::newlook_bg())
        .wrap(Wrap { trim: true });

        let pw_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Styles::newlook_focused_border())
            .style(Styles::newlook_bg())
            .title(t!("tui.import_export.master_password_label").to_string());

        let pw_value = self
            .master_password
            .expose(|pw| crate::tui::theme::ICON_PASSWORD_MASK.repeat(pw.chars().count()));

        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        let hint = Paragraph::new(t!("tui.import_export.hint_export").to_string())
            .style(Style::default().fg(TEXT_MUTED).bg(theme::NL_BG))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(2), // subtitle
            Constraint::Length(1), // gap
            Constraint::Length(3), // password input
            Constraint::Length(1), // error or gap
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(subtitle, rows[2]);

        let pw_inner = pw_block.inner(rows[4]);
        frame.render_widget(pw_block, rows[4]);
        let pw_display = export_input_paragraph(
            &pw_value,
            &t!("tui.import_export.master_password_placeholder").to_string(),
            pw_value.len(),
            true,
            pw_inner.width,
        );
        frame.render_widget(pw_display, pw_inner);

        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[5]);
        }

        frame.render_widget(hint, rows[7]);
    }
}

// ── View: Exporting ─────────────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_exporting(&self, frame: &mut ratatui::Frame, area: Rect) {
        let content_area = render_newlook_center_panel(frame, area, 9, 74);

        let title = Paragraph::new(Line::from(vec![
            Span::styled(theme::NF_DOWNLOAD, Style::default().fg(theme::NL_CYAN)),
            Span::raw("  "),
            Span::styled(
                t!("tui.import_export.exporting_title").to_string(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);

        let progress = Paragraph::new(Line::from(vec![
            Span::styled(
                theme::SPINNER_FRAMES[0],
                Style::default().fg(theme::NL_CYAN),
            ),
            Span::raw("  "),
            Span::styled(
                t!("tui.import_export.exporting_progress").to_string(),
                Style::default().fg(theme::NL_TEXT_MUTED),
            ),
        ]))
        .alignment(Alignment::Center);

        let hint = Paragraph::new(t!("tui.import_export.hint_wait").to_string())
            .style(Style::default().fg(TEXT_MUTED).bg(theme::NL_BG))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // progress text
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(progress, rows[2]);
        frame.render_widget(hint, rows[4]);
    }
}

// ── View: Export Complete ──────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_export_complete(&self, frame: &mut ratatui::Frame, area: Rect) {
        let content_area = render_newlook_center_panel(frame, area, 10, 74);

        let title = Paragraph::new(Line::from(vec![
            Span::styled(
                theme::NF_CHECK_CIRCLE,
                Style::default().fg(theme::NL_SUCCESS),
            ),
            Span::raw("  "),
            Span::styled(
                t!("tui.import_export.export_complete_title").to_string(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);

        let path_display = self
            .export_result_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| self.export_output_path.clone());

        let path_line = Paragraph::new(format!(
            "{} {}",
            theme::NF_CHECK_CIRCLE,
            t!("tui.import_export.saved_to", path = path_display)
        ))
        .style(Style::default().fg(theme::NL_SUCCESS).bg(theme::NL_BG))
        .wrap(Wrap { trim: true });

        let count_line = Paragraph::new(format!(
            "{} {}",
            theme::NF_CHECK_CIRCLE,
            t!(
                "tui.import_export.records_exported",
                count = self.export_record_count
            )
        ))
        .style(Style::default().fg(theme::NL_SUCCESS).bg(theme::NL_BG));

        let hint = Paragraph::new(t!("tui.import_export.hint_back_config").to_string())
            .style(Style::default().fg(TEXT_MUTED).bg(theme::NL_BG))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(2), // path (might wrap)
            Constraint::Length(1), // count
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(path_line, rows[2]);
        frame.render_widget(count_line, rows[3]);
        frame.render_widget(hint, rows[5]);
    }
}
