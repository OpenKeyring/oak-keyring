use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::commands::types::ImportSource;
use crate::tui::theme::{
    self, Styles, PRIMARY, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, TEXT_SECONDARY, WARNING,
};

use super::screen::ImportExportScreen;
use super::types::*;

// ── View: Import SourceSelect ───────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_import_source_select(&self, frame: &mut ratatui::Frame, area: Rect) {
        // Vertical centering
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(20),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        // Title — varies by entry point (AC18)
        let title_text = match self.entry_point {
            ImportEntryPoint::ConfigPage => "Import Data",
            ImportEntryPoint::Onboarding { step } => {
                // Show step context in onboarding flow
                let _ = step; // used for step number display
                "Step 1/6 \u{00B7} Select Import Source"
            }
        };
        let title = Paragraph::new(title_text)
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Mode switch hint
        let mode_hint =
            Paragraph::new("Tab: switch fields | \u{2191}\u{2193}: navigate | 1=Import 2=Export")
                .style(ratatui::style::Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);

        // Source list header
        let source_header =
            Paragraph::new("Select Source:").style(ratatui::style::Style::default().fg(TEXT));

        // Source items
        let source_items: Vec<ratatui::text::Line> = IMPORT_SOURCES
            .iter()
            .enumerate()
            .map(|(i, (_, name, needs_pw, scope_hint))| {
                let prefix = if i == self.selected_source_idx {
                    " \u{25B6} "
                } else {
                    "   "
                };
                let style = if i == self.selected_source_idx
                    && self.import_focus == ImportFocus::SourceList
                {
                    Styles::selected_focused()
                } else if i == self.selected_source_idx {
                    ratatui::style::Style::default().fg(PRIMARY)
                } else {
                    ratatui::style::Style::default().fg(TEXT_SECONDARY)
                };
                let name_span = ratatui::text::Span::styled(format!("{}{}", prefix, name), style);
                let pw_hint = if *needs_pw {
                    ratatui::text::Span::styled(
                        format!(" {}", theme::ICON_KEY),
                        ratatui::style::Style::default().fg(WARNING),
                    )
                } else {
                    ratatui::text::Span::raw("")
                };
                let sep = ratatui::text::Span::styled("  ", ratatui::style::Style::default());
                let hint_span = ratatui::text::Span::styled(
                    *scope_hint,
                    ratatui::style::Style::default().fg(TEXT_MUTED),
                );
                ratatui::text::Line::from(vec![name_span, pw_hint, sep, hint_span])
            })
            .collect();

        let source_list = Paragraph::new(source_items);

        // File path field
        let file_border = if self.import_focus == ImportFocus::FilePath {
            Styles::focused_border()
        } else {
            Styles::unfocused_border()
        };
        let file_block = Block::default()
            .borders(Borders::ALL)
            .border_style(file_border)
            .title(" File Path ");
        let file_display = if self.file_path.is_empty() {
            let placeholder = "/path/to/import/file";
            Paragraph::new(placeholder).style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
        } else {
            Paragraph::new(&*self.file_path).style(ratatui::style::Style::default().fg(TEXT))
        };

        // Password field (only shown for sources that need it)
        let needs_password = source_needs_password(self.current_source());
        let password_block_maybe = if needs_password {
            let pw_border = if self.import_focus == ImportFocus::Password {
                Styles::focused_border()
            } else {
                Styles::unfocused_border()
            };
            Some(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(pw_border)
                    .title(" Decryption Password "),
            )
        } else {
            None
        };
        let pw_display_maybe = if needs_password {
            if self.decrypt_password.is_empty() {
                Some(
                    Paragraph::new("Enter password")
                        .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER)),
                )
            } else {
                Some(
                    Paragraph::new(Self::display_password(&self.decrypt_password))
                        .style(ratatui::style::Style::default().fg(TEXT)),
                )
            }
        } else {
            None
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        // Hint
        let hint = Paragraph::new("Enter: validate | Esc: back")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        // CSV mapping section
        let is_csv = self.current_source() == ImportSource::Csv;
        let csv_row_count = if is_csv { 8 } else { 0 }; // 6 fields + skip header + header label

        // Calculate row constraints
        let mut constraints = vec![
            Constraint::Length(1), // title
            Constraint::Length(1), // mode hint
            Constraint::Length(1), // gap
            Constraint::Length(1), // source header
            Constraint::Length(6), // source list (6 items)
            Constraint::Length(1), // gap
            Constraint::Length(3), // file path
        ];

        if needs_password {
            constraints.push(Constraint::Length(1)); // gap
            constraints.push(Constraint::Length(3)); // password
        }

        if is_csv {
            for _ in 0..csv_row_count {
                constraints.push(Constraint::Length(1));
            }
        }

        constraints.push(Constraint::Length(1)); // error or gap
        constraints.push(Constraint::Length(1)); // gap
        constraints.push(Constraint::Length(1)); // hint

        let rows = Layout::vertical(constraints).split(content_area);

        let mut row_idx = 0;
        frame.render_widget(title, rows[row_idx]);
        row_idx += 1;
        frame.render_widget(mode_hint, rows[row_idx]);
        row_idx += 1;
        row_idx += 1; // gap
        frame.render_widget(source_header, rows[row_idx]);
        row_idx += 1;
        frame.render_widget(source_list, rows[row_idx]);
        row_idx += 1;
        row_idx += 1; // gap
        frame.render_widget(file_block, rows[row_idx]);
        let file_inner = Layout::vertical([Constraint::Length(1)]).split(rows[row_idx])[0];
        let file_padded =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(file_inner);
        frame.render_widget(file_display, file_padded[1]);
        row_idx += 1;

        if needs_password {
            row_idx += 1; // gap
            if let Some(ref block) = password_block_maybe {
                frame.render_widget(block.clone(), rows[row_idx]);
                let pw_inner = Layout::vertical([Constraint::Length(1)]).split(rows[row_idx])[0];
                let pw_padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
                    .split(pw_inner);
                if let Some(ref display) = pw_display_maybe {
                    frame.render_widget(display.clone(), pw_padded[1]);
                }
            }
            row_idx += 1;
        }

        if is_csv {
            // CSV column mapping header
            let csv_header =
                Paragraph::new("Column Mapping:").style(ratatui::style::Style::default().fg(TEXT));
            frame.render_widget(csv_header, rows[row_idx]);
            row_idx += 1;

            // CSV fields
            let csv_fields: Vec<(&str, &str, ImportFocus)> = vec![
                (
                    "Name column:",
                    &self.csv_mapping.name_column,
                    ImportFocus::CsvName,
                ),
                (
                    "Username column:",
                    &self.csv_mapping.username_column,
                    ImportFocus::CsvUsername,
                ),
                (
                    "Password column:",
                    &self.csv_mapping.password_column,
                    ImportFocus::CsvPassword,
                ),
                (
                    "URL column:",
                    &self.csv_mapping.url_column,
                    ImportFocus::CsvUrl,
                ),
                (
                    "Notes column:",
                    &self.csv_mapping.notes_column,
                    ImportFocus::CsvNotes,
                ),
                (
                    "Tags column:",
                    self.csv_mapping.tags_column.as_deref().unwrap_or("(none)"),
                    ImportFocus::CsvTags,
                ),
            ];

            for (label, value, focus) in csv_fields {
                let style = if self.import_focus == focus {
                    ratatui::style::Style::default().fg(PRIMARY)
                } else {
                    ratatui::style::Style::default().fg(TEXT_SECONDARY)
                };
                let line = Paragraph::new(format!("  {}: {}", label, value)).style(style);
                frame.render_widget(line, rows[row_idx]);
                row_idx += 1;
            }

            // Skip header toggle
            let skip_style = if self.import_focus == ImportFocus::CsvSkipHeader {
                ratatui::style::Style::default().fg(PRIMARY)
            } else {
                ratatui::style::Style::default().fg(TEXT_SECONDARY)
            };
            let checkbox = if self.csv_mapping.skip_header {
                "[x]"
            } else {
                "[ ]"
            };
            let skip_line =
                Paragraph::new(format!("  {} Skip header row", checkbox)).style(skip_style);
            frame.render_widget(skip_line, rows[row_idx]);
            row_idx += 1;
        }

        // Error
        if let Some(ref el) = error_line {
            if row_idx < rows.len() {
                frame.render_widget(el.clone(), rows[row_idx]);
            }
        }
        row_idx += 1;

        // Hint (second to last = gap, last = hint)
        if row_idx + 1 < rows.len() {
            frame.render_widget(hint, rows[row_idx + 1]);
        }
    }
}

// ── View: Import Preview ────────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_import_preview(&self, frame: &mut ratatui::Frame, area: Rect) {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(10),
            Constraint::Fill(1),
        ])
        .split(area);

        let center_area = outer[1];

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(center_area);

        let content_area = h_layout[1];

        let title = Paragraph::new("Import Preview")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        let mut constraints = vec![
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
        ];

        if let Some(ref preview) = self.preview {
            // Summary
            constraints.push(Constraint::Length(1)); // importable
            constraints.push(Constraint::Length(1)); // needs review
            constraints.push(Constraint::Length(1)); // failed
            constraints.push(Constraint::Length(1)); // gap

            // Review items
            let review_count = preview.review_items.len().min(5);
            for _ in 0..review_count {
                constraints.push(Constraint::Length(1));
            }
            if preview.review_items.len() > 5 {
                constraints.push(Constraint::Length(1)); // "...and more"
            }

            // Failed items
            let failed_count = preview.failed_items.len().min(5);
            if failed_count > 0 {
                constraints.push(Constraint::Length(1)); // gap
                constraints.push(Constraint::Length(1)); // header
                for _ in 0..failed_count {
                    constraints.push(Constraint::Length(1));
                }
            }
        }

        constraints.push(Constraint::Length(1)); // gap
        constraints.push(Constraint::Length(1)); // hint

        let rows = Layout::vertical(constraints).split(content_area);

        let mut row_idx = 0;
        frame.render_widget(title, rows[row_idx]);
        row_idx += 1;
        row_idx += 1; // gap

        if let Some(ref preview) = self.preview {
            let importable_line = Paragraph::new(format!(
                "{} Importable: {}",
                theme::ICON_SUCCESS,
                preview.importable
            ))
            .style(Styles::success_text());
            frame.render_widget(importable_line, rows[row_idx]);
            row_idx += 1;

            let review_line = Paragraph::new(format!(
                "{} Needs review: {}",
                theme::ICON_WARNING,
                preview.needs_review
            ))
            .style(Styles::warning_text());
            frame.render_widget(review_line, rows[row_idx]);
            row_idx += 1;

            let failed_line =
                Paragraph::new(format!("{} Failed: {}", theme::ICON_ERROR, preview.failed))
                    .style(Styles::error_text());
            frame.render_widget(failed_line, rows[row_idx]);
            row_idx += 1;

            row_idx += 1; // gap

            // Review items
            if !preview.review_items.is_empty() {
                let header = Paragraph::new("Review items:")
                    .style(ratatui::style::Style::default().fg(TEXT));
                frame.render_widget(header, rows[row_idx]);
                row_idx += 1;

                for item in preview.review_items.iter().take(5) {
                    let line = Paragraph::new(format!("  - {} ({})", item.name, item.reason))
                        .style(ratatui::style::Style::default().fg(TEXT_SECONDARY));
                    frame.render_widget(line, rows[row_idx]);
                    row_idx += 1;
                }
                if preview.review_items.len() > 5 {
                    let more =
                        Paragraph::new(format!("  ...and {} more", preview.review_items.len() - 5))
                            .style(ratatui::style::Style::default().fg(TEXT_MUTED));
                    frame.render_widget(more, rows[row_idx]);
                    row_idx += 1;
                }
            }

            // Failed items
            if !preview.failed_items.is_empty() {
                row_idx += 1; // gap
                let header = Paragraph::new("Failed items:").style(Styles::error_text());
                frame.render_widget(header, rows[row_idx]);
                row_idx += 1;

                for item in preview.failed_items.iter().take(5) {
                    let line = Paragraph::new(format!("  - {} ({})", item.name, item.reason))
                        .style(Styles::error_text());
                    frame.render_widget(line, rows[row_idx]);
                    row_idx += 1;
                }
            }
        }

        // Hint
        let hint = Paragraph::new("Enter: start import | Esc: back")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        if row_idx < rows.len() {
            frame.render_widget(hint, rows[row_idx]);
        }
    }
}

// ── View: Importing progress ───────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_importing(&self, frame: &mut ratatui::Frame, area: Rect) {
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

        let title = Paragraph::new("Importing...")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Progress bar
        let total = if self.import_progress_total > 0 {
            self.import_progress_total
        } else {
            1
        };
        let ratio = self.import_progress_current as f64 / total as f64;
        let bar_width = 40usize;
        let filled = (ratio * bar_width as f64).round() as usize;
        let empty = bar_width - filled;
        let bar_str = format!(
            "[{}{}] {}/{}",
            theme::ICON_PROGRESS_FILL.repeat(filled),
            theme::ICON_PROGRESS_EMPTY.repeat(empty),
            self.import_progress_current,
            total,
        );
        let progress_bar =
            Paragraph::new(bar_str).style(ratatui::style::Style::default().fg(PRIMARY));

        // Current item
        let current_item = if self.import_progress_name.is_empty() {
            Paragraph::new("").style(ratatui::style::Style::default().fg(TEXT_MUTED))
        } else {
            Paragraph::new(format!("Processing: {}", self.import_progress_name))
                .style(ratatui::style::Style::default().fg(TEXT_SECONDARY))
        };

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // progress bar
            Constraint::Length(1), // current item
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(progress_bar, rows[2]);
        frame.render_widget(current_item, rows[3]);

        let hint = Paragraph::new("Please wait...")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);
    }
}

// ── View: Import Complete ──────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_import_complete(&self, frame: &mut ratatui::Frame, area: Rect) {
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

        let title = Paragraph::new("Import Complete")
            .style(Styles::success_text())
            .alignment(Alignment::Center);

        let imported_line = Paragraph::new(format!(
            "{} Records imported: {}",
            theme::ICON_SUCCESS,
            self.imported_count
        ))
        .style(Styles::success_text());

        let skipped_line = if self.skipped_count > 0 {
            Paragraph::new(format!(
                "{} Records skipped: {}",
                theme::ICON_WARNING,
                self.skipped_count
            ))
            .style(Styles::warning_text())
        } else {
            Paragraph::new("").style(ratatui::style::Style::default().fg(TEXT_MUTED))
        };

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // imported
            Constraint::Length(1), // skipped
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
        ])
        .split(content_area);

        frame.render_widget(title, rows[0]);
        frame.render_widget(imported_line, rows[2]);
        frame.render_widget(skipped_line, rows[3]);

        let hint = Paragraph::new("Enter: back to config")
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);
    }
}
