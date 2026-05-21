use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::t;
use crate::tui::theme::{
    self, Styles, BG_SURFACE, BORDER, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_SECONDARY,
    WARNING,
};

use super::screen::OnboardingScreen;
use super::types::RecoveryFocus;

impl OnboardingScreen {
    pub(crate) fn view_recovery_display(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 24);

        let rows = Layout::vertical([
            Constraint::Length(1),  // title
            Constraint::Length(1),  // gap
            Constraint::Length(12), // word grid (8 rows + 2 borders + padding)
            Constraint::Length(1),  // separator gap
            Constraint::Length(1),  // buttons row
            Constraint::Length(1),  // gap
            Constraint::Length(1),  // clipboard warning (conditional)
            Constraint::Length(1),  // gap
            Constraint::Length(1),  // checkbox
            Constraint::Length(1),  // gap
            Constraint::Length(1),  // next step button / hint
            Constraint::Length(1),  // hint
            Constraint::Length(1),  // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new(format!(
            "{} {}",
            theme::ICON_WARNING,
            t!("tui.entry.recovery_key_write_down")
        ))
        .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Word grid (read-only)
        if self.recovery_words.is_none() {
            let placeholder = Paragraph::new(t!("tui.entry.generating_recovery_key"))
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
            let grid_area = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER));
            frame.render_widget(grid_area, rows[2]);
            // Render placeholder centered inside grid
            let inner = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .split(rows[2]);
            frame.render_widget(placeholder, inner[1]);
        } else {
            // Build a read-only 4x6 grid showing the recovery words
            self.render_readonly_word_grid(frame, rows[2]);
        }

        // Buttons row: [ Copy to clipboard ]  [ Regenerate ]
        let btn_area = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(24),
            Constraint::Length(2),
            Constraint::Length(18),
            Constraint::Fill(1),
        ])
        .split(rows[4]);

        let copy_style = if self.recovery_focus == RecoveryFocus::CopyButton {
            Styles::button_primary()
        } else {
            Styles::button_secondary()
        };
        let copy_btn = Paragraph::new(t!("tui.entry.copy_to_clipboard_btn"))
            .style(copy_style)
            .alignment(Alignment::Center);
        frame.render_widget(copy_btn, btn_area[1]);

        let regen_style = if self.recovery_focus == RecoveryFocus::RegenerateButton {
            Styles::button_primary()
        } else {
            Styles::button_secondary()
        };
        let regen_btn = Paragraph::new(t!("tui.entry.regenerate_btn"))
            .style(regen_style)
            .alignment(Alignment::Center);
        frame.render_widget(regen_btn, btn_area[3]);

        // Clipboard clear warning (shown after copy)
        if self.clipboard_copied {
            let warning = Paragraph::new(format!(
                "{} {}",
                theme::ICON_WARNING,
                t!(
                    "tui.entry.clipboard_clear_warning",
                    seconds = self.clipboard_clear_seconds
                )
            ))
            .style(Styles::warning_text())
            .alignment(Alignment::Center);
            frame.render_widget(warning, rows[6]);
        }

        // Checkbox
        let check_icon = if self.recovery_confirmed {
            theme::ICON_CHECK
        } else {
            "[ ]"
        };
        let check_focused = self.recovery_focus == RecoveryFocus::ConfirmCheckbox;
        let check_style = if self.recovery_confirmed {
            Style::default().fg(SUCCESS)
        } else if check_focused {
            Style::default().fg(PRIMARY)
        } else {
            Style::default().fg(TEXT_SECONDARY)
        };
        let checkbox = Paragraph::new(format!(
            " {}{}",
            check_icon,
            t!("tui.entry.confirm_saved_key")
        ))
        .style(check_style)
        .alignment(Alignment::Center);
        frame.render_widget(checkbox, rows[8]);

        // Next step button or instruction
        if self.recovery_confirmed {
            let next_style = Styles::button_primary();
            let next_btn = Paragraph::new(t!("tui.entry.next_step"))
                .style(next_style)
                .alignment(Alignment::Center);
            frame.render_widget(next_btn, rows[10]);
        } else {
            let instruction = Paragraph::new(t!("tui.entry.check_box_to_continue"))
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
            frame.render_widget(instruction, rows[10]);
        }

        // Hint
        let hint = Paragraph::new(t!("tui.entry.recovery_display_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[11]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = step, total = total).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[12]);
    }

    /// Render a read-only 4x6 word grid from recovery_words.
    pub(crate) fn render_readonly_word_grid(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(BORDER));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let vertical = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(8), // 6 word rows + 2 spacer rows
            Constraint::Fill(1),
        ])
        .split(inner);
        let grid_area = vertical[1];

        // Compute per-column max content width: "{:02}. {word}"
        let col_max_len: [usize; 4] = std::array::from_fn(|c| {
            (0..6)
                .map(|r| {
                    let idx = r * 4 + c;
                    let num_str = format!("{:02}.", idx + 1);
                    let word = self
                        .recovery_words
                        .as_ref()
                        .and_then(|words| words.word(idx))
                        .unwrap_or("");
                    num_str.len() + 1 + word.len() // "01. word"
                })
                .max()
                .unwrap_or(0)
        });

        let gap: u16 = 4;
        let grid_content_width: u16 = col_max_len.iter().sum::<usize>() as u16 + gap * 3; // 3 gaps between 4 columns
        let block_width = grid_content_width + 2; // borders

        // Center the grid horizontally
        let h_chunks = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(block_width.min(grid_area.width)),
            Constraint::Min(0),
        ])
        .split(grid_area);

        // Build rows with spacers between groups of 2 word rows
        let mut row_constraints = Vec::with_capacity(8);
        for i in 0..6usize {
            if i == 2 || i == 4 {
                row_constraints.push(Constraint::Length(1)); // spacer
            }
            row_constraints.push(Constraint::Length(1)); // word row
        }
        let rows = Layout::vertical(row_constraints).split(h_chunks[1]);

        for row in 0..6 {
            let row_idx = row + row / 2; // skip spacer rows
            let mut col_constraints = Vec::with_capacity(8); // 4 cols + 4 gaps between them
            for (col, &max_len) in col_max_len.iter().enumerate() {
                if col > 0 {
                    col_constraints.push(Constraint::Length(gap));
                }
                col_constraints.push(Constraint::Length(max_len as u16));
            }
            let cols = Layout::horizontal(col_constraints).split(rows[row_idx]);
            let mut cell_idx = 0;
            for (col, &max_len) in col_max_len.iter().enumerate() {
                let idx = row * 4 + col;
                let num_str = format!("{:02}.", idx + 1);
                let word = self
                    .recovery_words
                    .as_ref()
                    .and_then(|words| words.word(idx))
                    .unwrap_or("");
                // Pad word to column max width so cells align within the column
                let word_pad = max_len - num_str.len() - 1; // -1 for the space
                let cell = Paragraph::new(Line::from(vec![
                    Span::styled(num_str, Style::default().fg(TEXT_SECONDARY)),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<width$}", word, width = word_pad),
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    ),
                ]));
                frame.render_widget(cell, cols[cell_idx]);
                cell_idx += 2; // skip gap column
            }
        }
    }

    pub(crate) fn view_recovery_verify(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 23);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // instruction
            Constraint::Length(1), // gap
            Constraint::Length(1), // label 0
            Constraint::Length(3), // input box 0
            Constraint::Length(1), // label 1
            Constraint::Length(3), // input box 1
            Constraint::Length(1), // label 2
            Constraint::Length(3), // input box 2
            Constraint::Length(1), // label 3
            Constraint::Length(3), // input box 3
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new(t!("tui.entry.verify_recovery_title"))
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Instruction
        let instruction = Paragraph::new(t!("tui.entry.enter_word_position"))
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[2]);

        // Verify input boxes
        for i in 0..4 {
            let pos = self.verify_positions[i] + 1; // 1-based display
            let is_focused = i == self.verify_focus_index;
            let has_error = self.verify_errors[i];

            // Label
            let label = Paragraph::new(t!("tui.entry.word_n_label", n = pos).to_string())
                .style(Style::default().fg(TEXT_SECONDARY));
            frame.render_widget(label, rows[4 + i * 2]);

            // Input box with border
            let border_color = if has_error {
                ERROR
            } else if is_focused {
                PRIMARY
            } else {
                BORDER
            };

            let input_text = if is_focused {
                if self.verify_inputs[i].is_empty() {
                    "_".to_string()
                } else {
                    let mut text = self.verify_inputs[i].expose(|s| s.to_string());
                    text.push('_');
                    text
                }
            } else if self.verify_inputs[i].is_empty() {
                String::new()
            } else {
                self.verify_inputs[i].expose(|s| s.to_string())
            };

            let text_style = if has_error {
                Style::default().fg(ERROR)
            } else if self.verify_inputs[i].is_empty() {
                Style::default().fg(TEXT_MUTED)
            } else {
                Style::default().fg(TEXT)
            };

            let input_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(BG_SURFACE));

            let box_area = rows[5 + i * 2];
            let para = Paragraph::new(input_text).style(text_style);
            let inner = input_block.inner(box_area);
            frame.render_widget(input_block, box_area);
            frame.render_widget(para, inner);
            self.verify_box_areas[i].set(box_area);
        }

        // Hint
        let hint = Paragraph::new(t!("tui.entry.verify_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[12]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = step, total = total).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[13]);
    }

    pub(crate) fn view_recovery_input(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 16);

        let rows = Layout::vertical([
            Constraint::Length(1),  // title
            Constraint::Length(1),  // gap
            Constraint::Length(10), // grid
            Constraint::Length(1),  // gap
            Constraint::Length(1),  // hint
            Constraint::Length(1),  // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new(t!("tui.entry.enter_recovery_title"))
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Grid
        self.recovery_grid.view(frame, rows[2]);

        // Hint
        let hint = Paragraph::new(t!("tui.entry.recovery_input_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[4]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = step, total = total).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[5]);
    }

    pub(crate) fn view_security_advisory(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 10);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(3), // notice
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new(format!(
            "{} {}",
            theme::ICON_WARNING,
            t!("tui.entry.security_notice")
        ))
        .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Notice
        let notice = Paragraph::new(t!("tui.entry.security_notice_body"))
            .style(Style::default().fg(TEXT))
            .wrap(Wrap { trim: true });
        frame.render_widget(notice, rows[2]);

        // Hint
        let hint = Paragraph::new(t!("tui.entry.security_notice_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[4]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = step, total = total).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[5]);
    }
}
