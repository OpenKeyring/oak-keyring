use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::commands::types::ImportSource;
use crate::t;
use crate::tui::theme::{
    self, Styles, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER, TEXT_SECONDARY,
    TEXT_TERTIARY, WARNING,
};

use super::screen::ImportExportScreen;
use super::types::ScopeHintStyle;
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
            ImportEntryPoint::ConfigPage => t!("tui.import_export.import_title").to_string(),
            ImportEntryPoint::Onboarding { step } => {
                // Show step context in onboarding flow
                let _ = step; // used for step number display
                t!("tui.import_export.step_onboarding_source").to_string()
            }
        };
        let title = Paragraph::new(title_text)
            .style(Styles::brand_text())
            .alignment(Alignment::Center);

        // Mode switch hint
        let mode_hint = Paragraph::new(t!("tui.import_export.mode_hint_import").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        // Source list header
        let source_header = Paragraph::new(t!("tui.import_export.select_source").to_string())
            .style(ratatui::style::Style::default().fg(TEXT));

        // Source items
        let sources = import_sources();
        let source_items: Vec<ratatui::text::Line> = sources
            .iter()
            .enumerate()
            .map(|(i, (_, name, needs_pw, (hint_text, hint_style)))| {
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
                let hint_color = match hint_style {
                    ScopeHintStyle::Full => SUCCESS,
                    ScopeHintStyle::Partial => WARNING,
                    ScopeHintStyle::Limited => ERROR,
                };
                let hint_span = ratatui::text::Span::styled(
                    hint_text.as_str(),
                    ratatui::style::Style::default().fg(hint_color),
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
            .title(t!("tui.import_export.file_path_label").to_string());
        let file_display = if self.file_path.is_empty() {
            Paragraph::new(t!("tui.import_export.file_path_placeholder").to_string())
                .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER))
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
                    .title(t!("tui.import_export.password_label").to_string()),
            )
        } else {
            None
        };
        let pw_display_maybe = if needs_password {
            if self.decrypt_password.is_empty() {
                Some(
                    Paragraph::new(t!("tui.import_export.password_placeholder").to_string())
                        .style(ratatui::style::Style::default().fg(TEXT_PLACEHOLDER)),
                )
            } else {
                let masked = self
                    .decrypt_password
                    .expose(|pw| crate::tui::theme::ICON_PASSWORD_MASK.repeat(pw.chars().count()));
                Some(Paragraph::new(masked).style(ratatui::style::Style::default().fg(TEXT)))
            }
        } else {
            None
        };

        // Error
        let error_line = self.error_message.as_ref().map(|msg| {
            Paragraph::new(format!("{} {}", theme::ICON_ERROR, msg)).style(Styles::error_text())
        });

        // Hint
        let hint = Paragraph::new(t!("tui.import_export.hint_validate").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);

        // CSV mapping section
        let is_csv = self.current_source() == ImportSource::Csv;
        let csv_row_count = if is_csv {
            8 + if !self.csv_headers.is_empty() { 1 } else { 0 }
        } else {
            0
        }; // 6 fields + skip header + header label + (optional detected headers)

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
            // Show detected CSV headers if available from validation
            if !self.csv_headers.is_empty() {
                let headers_text = t!(
                    "tui.import_export.detected_columns",
                    columns = self.csv_headers.join(", ")
                )
                .to_string();
                let detected = Paragraph::new(headers_text)
                    .style(ratatui::style::Style::default().fg(TEXT_TERTIARY));
                frame.render_widget(detected, rows[row_idx]);
                row_idx += 1;
            }

            // CSV column mapping header
            let csv_header = Paragraph::new(t!("tui.import_export.column_mapping").to_string())
                .style(ratatui::style::Style::default().fg(TEXT));
            frame.render_widget(csv_header, rows[row_idx]);
            row_idx += 1;

            // CSV fields
            let none_label = t!("tui.import_export.none").to_string();
            let csv_fields: Vec<(String, &str, ImportFocus)> = vec![
                (
                    t!("tui.import_export.column_name").to_string(),
                    &self.csv_mapping.name_column,
                    ImportFocus::CsvName,
                ),
                (
                    t!("tui.import_export.column_username").to_string(),
                    &self.csv_mapping.username_column,
                    ImportFocus::CsvUsername,
                ),
                (
                    t!("tui.import_export.column_password").to_string(),
                    &self.csv_mapping.password_column,
                    ImportFocus::CsvPassword,
                ),
                (
                    t!("tui.import_export.column_url").to_string(),
                    &self.csv_mapping.url_column,
                    ImportFocus::CsvUrl,
                ),
                (
                    t!("tui.import_export.column_notes").to_string(),
                    &self.csv_mapping.notes_column,
                    ImportFocus::CsvNotes,
                ),
                (
                    t!("tui.import_export.column_tags").to_string(),
                    self.csv_mapping
                        .tags_column
                        .as_deref()
                        .unwrap_or(&none_label),
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
            let skip_line = Paragraph::new(format!(
                "  {} {}",
                checkbox,
                t!("tui.import_export.skip_header")
            ))
            .style(skip_style);
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

        let title = Paragraph::new(t!("tui.import_export.preview_title").to_string())
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
                "{} {}",
                theme::ICON_SUCCESS,
                t!(
                    "tui.import_export.importable_count",
                    count = preview.importable
                )
            ))
            .style(Styles::success_text());
            frame.render_widget(importable_line, rows[row_idx]);
            row_idx += 1;

            let review_line = Paragraph::new(format!(
                "{} {}",
                theme::ICON_WARNING,
                t!(
                    "tui.import_export.needs_review_count",
                    count = preview.needs_review
                )
            ))
            .style(Styles::warning_text());
            frame.render_widget(review_line, rows[row_idx]);
            row_idx += 1;

            let failed_line = Paragraph::new(format!(
                "{} {}",
                theme::ICON_ERROR,
                t!("tui.import_export.failed_count", count = preview.failed)
            ))
            .style(Styles::error_text());
            frame.render_widget(failed_line, rows[row_idx]);
            row_idx += 1;

            row_idx += 1; // gap

            // Review items
            if !preview.review_items.is_empty() {
                let header =
                    Paragraph::new(t!("tui.import_export.review_items_header").to_string())
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
                    let more = Paragraph::new(format!(
                        "  {}",
                        t!(
                            "tui.import_export.review_more",
                            count = preview.review_items.len() - 5
                        )
                    ))
                    .style(ratatui::style::Style::default().fg(TEXT_MUTED));
                    frame.render_widget(more, rows[row_idx]);
                    row_idx += 1;
                }
            }

            // Failed items
            if !preview.failed_items.is_empty() {
                row_idx += 1; // gap
                let header =
                    Paragraph::new(t!("tui.import_export.failed_items_header").to_string())
                        .style(Styles::error_text());
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
        let hint = Paragraph::new(t!("tui.import_export.hint_import").to_string())
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

        let title = Paragraph::new(t!("tui.import_export.importing_title").to_string())
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
            Paragraph::new(
                t!(
                    "tui.import_export.processing_item",
                    name = self.import_progress_name.as_str()
                )
                .to_string(),
            )
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

        let hint = Paragraph::new(t!("tui.import_export.hint_wait").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);
    }
}

// ── View: Import Complete ──────────────────────────────────────────────────

impl ImportExportScreen {
    pub(super) fn view_import_complete(&self, frame: &mut ratatui::Frame, area: Rect) {
        use crate::commands::types::SkipReason;

        // Count breakdown lines to compute layout height.
        let mut line_count: u16 = 3; // title + gaps + hint
        if self.imported_count > 0 {
            line_count += 1;
        }
        if self.reviewed_count > 0 {
            line_count += 1;
        }
        if self.skipped_count > 0 {
            line_count += 1;
        }
        for &count in self.skip_breakdown.values() {
            if count > 0 {
                line_count += 1;
            }
        }
        if self.failed_count > 0 {
            line_count += 1;
        }

        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Min(line_count),
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

        let mut constraints = vec![
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
        ];

        if self.imported_count > 0 {
            constraints.push(Constraint::Length(1));
        }
        if self.reviewed_count > 0 {
            constraints.push(Constraint::Length(1));
        }
        if self.skipped_count > 0 {
            constraints.push(Constraint::Length(1));
        }
        let breakdown_keys: Vec<SkipReason> = self
            .skip_breakdown
            .iter()
            .filter(|(_, &c)| c > 0)
            .map(|(&k, _)| k)
            .collect();
        for _ in &breakdown_keys {
            constraints.push(Constraint::Length(1));
        }
        if self.failed_count > 0 {
            constraints.push(Constraint::Length(1));
        }
        constraints.push(Constraint::Length(1)); // gap
        constraints.push(Constraint::Length(1)); // hint

        let rows = Layout::vertical(constraints).split(content_area);

        let mut row_idx = 0usize;

        let title = Paragraph::new(t!("tui.import_export.complete_title").to_string())
            .style(Styles::success_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[row_idx]);
        row_idx += 2; // skip title + gap

        // Imported
        if self.imported_count > 0 {
            let line = Paragraph::new(format!(
                "{} {}",
                theme::ICON_SUCCESS,
                t!(
                    "tui.import_export.records_imported",
                    count = self.imported_count
                )
            ))
            .style(Styles::success_text());
            frame.render_widget(line, rows[row_idx]);
            row_idx += 1;
        }

        // Reviewed (imported as notes)
        if self.reviewed_count > 0 {
            let line = Paragraph::new(format!(
                "{} {}",
                theme::ICON_WARNING,
                t!(
                    "tui.import_export.partially_imported",
                    count = self.reviewed_count
                )
            ))
            .style(Styles::warning_text());
            frame.render_widget(line, rows[row_idx]);
            row_idx += 1;
        }

        // Skipped
        if self.skipped_count > 0 {
            let line = Paragraph::new(format!(
                "{} {}",
                theme::ICON_WARNING,
                t!(
                    "tui.import_export.records_skipped",
                    count = self.skipped_count
                )
            ))
            .style(Styles::warning_text());
            frame.render_widget(line, rows[row_idx]);
            row_idx += 1;

            // Breakdown by reason
            for reason in &breakdown_keys {
                let count = self.skip_breakdown.get(reason).copied().unwrap_or(0);
                let label = match reason {
                    SkipReason::Duplicate => {
                        t!("tui.import_export.skip_reason_duplicates").to_string()
                    }
                    SkipReason::ValidationFailed => {
                        t!("tui.import_export.skip_reason_validation_failed").to_string()
                    }
                    _ => continue,
                };
                let style = match reason {
                    SkipReason::Duplicate => ratatui::style::Style::default().fg(TEXT_SECONDARY),
                    SkipReason::ValidationFailed => ratatui::style::Style::default().fg(WARNING),
                    _ => continue,
                };
                let detail = Paragraph::new(
                    t!(
                        "tui.import_export.skip_records_count",
                        label = label,
                        count = count
                    )
                    .to_string(),
                )
                .style(style);
                frame.render_widget(detail, rows[row_idx]);
                row_idx += 1;
            }
        }

        // Failed
        if self.failed_count > 0 {
            let line = Paragraph::new(format!(
                "{} {}",
                theme::ICON_ERROR,
                t!(
                    "tui.import_export.records_failed",
                    count = self.failed_count
                )
            ))
            .style(Styles::error_text());
            frame.render_widget(line, rows[row_idx]);
            row_idx += 1;
        }

        // Skip gap
        row_idx += 1;

        let hint = Paragraph::new(t!("tui.import_export.hint_back_config").to_string())
            .style(ratatui::style::Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        if row_idx < rows.len() {
            frame.render_widget(hint, rows[row_idx]);
        }
    }
}
