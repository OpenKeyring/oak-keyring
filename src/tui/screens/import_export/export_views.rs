use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::theme::{
    self, Styles, PRIMARY, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, TEXT_SECONDARY,
};

use super::screen::ImportExportScreen;
use super::types::*;

// ── View: Export Form ──────────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_export_form(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(20),
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
        let title = Paragraph::new("Export Data")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Format info (AC17: recovery key mention)
        let format_info = Paragraph::new(format!(
            "{} Only .okb format. Decryptable via password or Recovery Key",
            theme::ICON_INFO
        ))
        .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
        .alignment(Alignment::Center);

        // Scope selector
        let scope_header =
            Paragraph::new("Export Scope:").style(ratatui::style::Style::default().fg(TEXT));
        let scope_options = [
            ("All records", ExportScopeOption::All),
            ("Current filter", ExportScopeOption::CurrentFilter),
            ("By tag", ExportScopeOption::ByTag),
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
            .title(" Export Password ");
        let pw_display = if self.export_password.is_empty() {
            Paragraph::new("Enter export password")
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(Self::display_password(&self.export_password))
                .style(ratatui::style::Style::default().fg(TEXT))
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
            Paragraph::new("Strength: ").style(ratatui::style::Style::default().fg(TEXT_MUTED))
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
            .title(" Confirm Password ");
        let confirm_display = if self.export_confirm_password.is_empty() {
            Paragraph::new("Confirm export password")
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(Self::display_password(&self.export_confirm_password))
                .style(ratatui::style::Style::default().fg(TEXT))
        };

        // Match indicator
        let match_line =
            if !self.export_password.is_empty() && !self.export_confirm_password.is_empty() {
                if self.export_password == self.export_confirm_password {
                    Some(
                        Paragraph::new(format!("{} Passwords match", theme::ICON_SUCCESS))
                            .style(Styles::success_text()),
                    )
                } else {
                    Some(
                        Paragraph::new(format!("{} Passwords do not match", theme::ICON_ERROR))
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
            .title(" Output Path ");
        let path_display = if self.export_output_path.is_empty() {
            Paragraph::new("~/Documents/keyring-backup.okb")
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
        let hint =
            Paragraph::new("Tab: switch fields | \u{2191}\u{2193}: select scope | Esc: back")
                .style(ratatui::style::Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // scope header
            Constraint::Length(3), // scope list
            Constraint::Length(3), // export password
            Constraint::Length(1), // strength bar
            Constraint::Length(3), // confirm password
            Constraint::Length(1), // match indicator
            Constraint::Length(1), // gap
            Constraint::Length(3), // output path
            Constraint::Length(1), // error or gap
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        // gap at rows[1]
        frame.render_widget(format_info, rows[1]);
        frame.render_widget(scope_header, rows[2]);
        frame.render_widget(scope_list, rows[3]);

        // Export password
        frame.render_widget(pw_block, rows[4]);
        let pw_inner = Layout::vertical([Constraint::Length(1)]).split(rows[4])[0];
        let pw_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(pw_inner);
        frame.render_widget(pw_display, pw_padded[1]);

        // Strength
        frame.render_widget(strength_line, rows[5]);

        // Confirm password
        frame.render_widget(confirm_block, rows[6]);
        let confirm_inner = Layout::vertical([Constraint::Length(1)]).split(rows[6])[0];
        let confirm_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(confirm_inner);
        frame.render_widget(confirm_display, confirm_padded[1]);

        // Match indicator
        if let Some(ref ml) = match_line {
            frame.render_widget(ml.clone(), rows[7]);
        }

        // Output path
        frame.render_widget(path_block, rows[9]);
        let path_inner = Layout::vertical([Constraint::Length(1)]).split(rows[9])[0];
        let path_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(path_inner);
        frame.render_widget(path_display, path_padded[1]);

        // Error
        if let Some(ref el) = error_line {
            frame.render_widget(el.clone(), rows[10]);
        }

        // Hint
        frame.render_widget(hint, rows[12]);
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

        let title = Paragraph::new("Authorize Export")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let subtitle = Paragraph::new(format!(
            "Export {} to: {}",
            match self.export_scope_option {
                ExportScopeOption::All => "all records",
                ExportScopeOption::CurrentFilter => "filtered records",
                ExportScopeOption::ByTag => "tagged records",
            },
            self.export_output_path,
        ))
        .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
        .alignment(Alignment::Center);

        // Master password input
        let pw_border = Styles::focused_border();
        let pw_block = Block::default()
            .borders(Borders::ALL)
            .border_style(pw_border)
            .title(" Master Password ");

        let pw_display = if self.master_password.is_empty() {
            Paragraph::new("Enter master password to authorize")
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(Self::display_password(&self.master_password))
                .style(ratatui::style::Style::default().fg(TEXT))
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        let hint = Paragraph::new("Enter: export | Esc: back")
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

        let title = Paragraph::new("Exporting...")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let progress = Paragraph::new("Encrypting and writing export file...")
            .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);

        let hint = Paragraph::new("Please wait...")
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

        let title = Paragraph::new("Export Complete")
            .style(Styles::success_text())
            .alignment(Alignment::Center);

        let path_display = self
            .export_result_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| self.export_output_path.clone());

        let path_line = Paragraph::new(format!(
            "{} Saved to: {}",
            theme::ICON_SUCCESS,
            path_display
        ))
        .style(Styles::success_text())
        .wrap(Wrap { trim: true });

        let count_line = Paragraph::new(format!(
            "{} Records exported: {}",
            theme::ICON_SUCCESS,
            self.export_record_count
        ))
        .style(Styles::success_text());

        let hint = Paragraph::new("Enter: back to config")
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
