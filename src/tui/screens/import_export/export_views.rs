use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::commands::types::ExportFormat;
use crate::t;
use crate::tui::theme::{
    self, Styles, PRIMARY, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, TEXT_SECONDARY, WARNING,
};

use super::screen::ImportExportScreen;
use super::types::*;

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
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        // Title
        let title = Paragraph::new(t!("tui.import_export.export_title").to_string())
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Format selector
        let format_header = Paragraph::new(t!("tui.import_export.export_format_label").to_string())
            .style(ratatui::style::Style::default().fg(TEXT));
        let format_options = [
            (
                t!("tui.import_export.format_okb").to_string(),
                ExportFormat::Okb,
            ),
            (
                t!("tui.import_export.format_csv").to_string(),
                ExportFormat::Csv,
            ),
        ];
        let format_items: Vec<ratatui::text::Line> = format_options
            .iter()
            .map(|(label, opt)| {
                let is_selected = self.export_format == *opt;
                let marker = if is_selected { ">" } else { " " };
                let style = if is_selected && self.export_focus == ExportFocus::Format {
                    Styles::selected_focused()
                } else if is_selected {
                    ratatui::style::Style::default().fg(PRIMARY)
                } else {
                    ratatui::style::Style::default().fg(TEXT_SECONDARY)
                };
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    format!(" {} {}", marker, label),
                    style,
                ))
            })
            .collect();
        let format_list = Paragraph::new(format_items);

        // Security warning for CSV
        let csv_warning = if self.export_format == ExportFormat::Csv {
            Some(
                Paragraph::new(format!(
                    "{} {}",
                    theme::ICON_WARNING,
                    t!("tui.import_export.csv_security_warning")
                ))
                .style(ratatui::style::Style::default().fg(WARNING)),
            )
        } else {
            None
        };

        // Format info (AC17: recovery key mention) — only for Okb
        let format_info = if self.export_format == ExportFormat::Okb {
            Some(
                Paragraph::new(format!(
                    "{} {}",
                    theme::ICON_INFO,
                    t!("tui.import_export.format_info")
                ))
                .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
                .alignment(Alignment::Center),
            )
        } else {
            None
        };

        // Scope selector
        let scope_header = Paragraph::new(t!("tui.import_export.export_scope_label").to_string())
            .style(ratatui::style::Style::default().fg(TEXT));
        let scope_options = [
            (
                t!("tui.import_export.scope_all").to_string(),
                ExportScopeOption::All,
            ),
            (
                t!("tui.import_export.scope_current_filter").to_string(),
                ExportScopeOption::CurrentFilter,
            ),
            (
                t!("tui.import_export.scope_by_tag").to_string(),
                ExportScopeOption::ByTag,
            ),
        ];
        let scope_items: Vec<ratatui::text::Line> = scope_options
            .iter()
            .map(|(label, opt)| {
                let is_selected = self.export_scope_option == *opt;
                let marker = if is_selected { ">" } else { " " };
                let style = if is_selected && self.export_focus == ExportFocus::Scope {
                    Styles::selected_focused()
                } else if is_selected {
                    ratatui::style::Style::default().fg(PRIMARY)
                } else {
                    ratatui::style::Style::default().fg(TEXT_SECONDARY)
                };
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    format!(" {} {}", marker, label),
                    style,
                ))
            })
            .collect();
        let scope_list = Paragraph::new(scope_items);

        // Export password
        let pw_border = if self.export_focus == ExportFocus::ExportPassword {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let pw_block = Block::default()
            .borders(Borders::ALL)
            .border_style(pw_border)
            .title(t!("tui.import_export.export_password_label").to_string());
        let pw_display = if self.export_password.is_empty() {
            Paragraph::new(t!("tui.import_export.export_password_placeholder").to_string())
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            let masked = self
                .export_password
                .expose(|pw| crate::tui::theme::ICON_PASSWORD_MASK.repeat(pw.chars().count()));
            Paragraph::new(masked).style(ratatui::style::Style::default().fg(TEXT))
        };

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
            let label = format!("Strength: {} {}", s.level.label_zh(), bar_str);
            let color = Self::strength_color(&s.level);
            Paragraph::new(label).style(ratatui::style::Style::default().fg(color))
        } else {
            let label = t!("tui.import_export.strength_label_short").to_string();
            Paragraph::new(label).style(ratatui::style::Style::default().fg(TEXT_MUTED))
        };

        // Confirm password
        let confirm_border = if self.export_focus == ExportFocus::ConfirmPassword {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let confirm_block = Block::default()
            .borders(Borders::ALL)
            .border_style(confirm_border)
            .title(t!("tui.import_export.confirm_password_label").to_string());
        let confirm_display = if self.export_confirm_password.is_empty() {
            Paragraph::new(t!("tui.import_export.confirm_password_placeholder").to_string())
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            let masked = self
                .export_confirm_password
                .expose(|pw| crate::tui::theme::ICON_PASSWORD_MASK.repeat(pw.chars().count()));
            Paragraph::new(masked).style(ratatui::style::Style::default().fg(TEXT))
        };

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
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let path_block = Block::default()
            .borders(Borders::ALL)
            .border_style(path_border)
            .title(t!("tui.import_export.output_path_label").to_string());
        let path_display = if self.export_output_path.is_empty() {
            Paragraph::new(t!("tui.import_export.output_path_placeholder").to_string())
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(&*self.export_output_path)
                .style(ratatui::style::Style::default().fg(TEXT))
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        // Hint
        let hint = Paragraph::new(t!("tui.import_export.mode_hint_export").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        let is_okb = self.export_format == ExportFormat::Okb;
        let has_warning = csv_warning.is_some();

        let rows = Layout::vertical([
            Constraint::Length(1),                               // 0  title
            Constraint::Length(1),                               // 1  gap
            Constraint::Length(1),                               // 2  format header
            Constraint::Length(2),                               // 3  format list (2 items)
            Constraint::Length(if has_warning { 1 } else { 0 }), // 4 csv warning
            Constraint::Length(if is_okb { 1 } else { 0 }),      // 5  format info
            Constraint::Length(1),                               // 6  scope header
            Constraint::Length(3),                               // 7  scope list
            Constraint::Length(if is_okb { 3 } else { 0 }),      // 8  export password
            Constraint::Length(if is_okb { 1 } else { 0 }),      // 9  strength bar
            Constraint::Length(if is_okb { 3 } else { 0 }),      // 10 confirm password
            Constraint::Length(if is_okb { 1 } else { 0 }),      // 11 match indicator
            Constraint::Length(1),                               // 12 gap
            Constraint::Length(3),                               // 13 output path
            Constraint::Length(1),                               // 14 error or gap
            Constraint::Length(1),                               // 15 gap
            Constraint::Length(1),                               // 16 hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        // gap at rows[1]
        frame.render_widget(format_header, rows[2]);
        frame.render_widget(format_list, rows[3]);
        if let Some(ref w) = csv_warning {
            frame.render_widget(w.clone(), rows[4]);
        }
        if let Some(ref info) = format_info {
            frame.render_widget(info.clone(), rows[5]);
        }
        frame.render_widget(scope_header, rows[6]);
        frame.render_widget(scope_list, rows[7]);

        if is_okb {
            // Export password
            frame.render_widget(pw_block, rows[8]);
            let pw_inner = Layout::vertical([Constraint::Length(1)]).split(rows[8])[0];
            let pw_padded =
                Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(pw_inner);
            frame.render_widget(pw_display, pw_padded[1]);

            // Strength
            frame.render_widget(strength_line, rows[9]);

            // Confirm password
            frame.render_widget(confirm_block, rows[10]);
            let confirm_inner = Layout::vertical([Constraint::Length(1)]).split(rows[10])[0];
            let confirm_padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
                .split(confirm_inner);
            frame.render_widget(confirm_display, confirm_padded[1]);

            // Match indicator
            if let Some(ref ml) = match_line {
                frame.render_widget(ml.clone(), rows[11]);
            }
        }

        // Output path
        frame.render_widget(path_block, rows[13]);
        let path_inner = Layout::vertical([Constraint::Length(1)]).split(rows[13])[0];
        let path_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(path_inner);
        frame.render_widget(path_display, path_padded[1]);

        // Error
        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[14]);
        }

        // Hint
        frame.render_widget(hint, rows[16]);
    }
}

// ── View: Export Master Password Confirm ───────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_export_master_password_confirm(
        &self,
        frame: &mut ratatui::Frame,
        area: Rect,
    ) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(12),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new(t!("tui.import_export.authorize_title").to_string())
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let subtitle = Paragraph::new(
            t!(
                "tui.import_export.authorize_subtitle",
                scope = match self.export_scope_option {
                    ExportScopeOption::All => t!("tui.import_export.scope_all").to_string(),
                    ExportScopeOption::CurrentFilter => {
                        t!("tui.import_export.scope_current_filter").to_string()
                    }
                    ExportScopeOption::ByTag => t!("tui.import_export.scope_by_tag").to_string(),
                },
                path = self.export_output_path
            )
            .to_string(),
        )
        .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
        .alignment(Alignment::Center);

        // Master password input
        let pw_border = Styles::focused_border();
        let pw_block = Block::default()
            .borders(Borders::ALL)
            .border_style(pw_border)
            .title(t!("tui.import_export.master_password_label").to_string());

        let pw_display = if self.master_password.is_empty() {
            Paragraph::new(t!("tui.import_export.master_password_placeholder").to_string())
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            let masked = self
                .master_password
                .expose(|pw| crate::tui::theme::ICON_PASSWORD_MASK.repeat(pw.chars().count()));
            Paragraph::new(masked).style(ratatui::style::Style::default().fg(TEXT))
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        let hint = Paragraph::new(t!("tui.import_export.hint_export").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // subtitle
            Constraint::Length(1), // gap
            Constraint::Length(3), // password input
            Constraint::Length(1), // error or gap
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(subtitle, rows[2]);

        frame.render_widget(pw_block, rows[4]);
        let pw_inner = Layout::vertical([Constraint::Length(1)]).split(rows[4])[0];
        let pw_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(pw_inner);
        frame.render_widget(pw_display, pw_padded[1]);

        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[5]);
        }

        frame.render_widget(hint, rows[7]);
    }
}

// ── View: Exporting ─────────────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_exporting(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(6),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new(t!("tui.import_export.exporting_title").to_string())
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let progress = Paragraph::new(t!("tui.import_export.exporting_progress").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);

        let hint = Paragraph::new(t!("tui.import_export.hint_wait").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
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
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(8),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(50),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new(t!("tui.import_export.export_complete_title").to_string())
            .style(Styles::success_text())
            .alignment(Alignment::Center);

        let path_display = self
            .export_result_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| self.export_output_path.clone());

        let path_line = Paragraph::new(format!(
            "{} {}",
            theme::ICON_SUCCESS,
            t!("tui.import_export.saved_to", path = path_display)
        ))
        .style(Styles::success_text())
        .wrap(Wrap { trim: true });

        let count_line = Paragraph::new(format!(
            "{} {}",
            theme::ICON_SUCCESS,
            t!(
                "tui.import_export.records_exported",
                count = self.export_record_count
            )
        ))
        .style(Styles::success_text());

        let hint = Paragraph::new(t!("tui.import_export.hint_back_config").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
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
