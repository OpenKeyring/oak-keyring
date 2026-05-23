use ratatui::layout::{Alignment, Constraint, Layout, Position};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::t;
use crate::tui::terminal::WidthTier;
use crate::tui::theme::{
    self, Styles, BG_SURFACE, BORDER, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED, TEXT_SECONDARY,
    WARNING,
};

use super::screen::OnboardingScreen;
use super::types::RecoveryFocus;
use super::views_setup::{render_header, header_rows};

impl OnboardingScreen {
    pub(crate) fn view_recovery_display(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let learn_extra = if self.learn_more_expanded { 3 } else { 0 };
        let content_area = Self::centered_content(area, hdr + 19 + learn_extra, 72);

        let mut constraints = vec![
            Constraint::Length(hdr),     // 0: logo or brand
            Constraint::Length(1),       // 1: title
            Constraint::Length(1),       // 2: instruction
            Constraint::Length(10),      // 3: word grid (was 12)
            Constraint::Length(1),       // 4: buttons row
            Constraint::Length(1),       // 5: clipboard warning
            Constraint::Length(1),       // 6: learn more toggle
        ];
        if self.learn_more_expanded {
            constraints.push(Constraint::Length(1)); // 7: learn more l1
            constraints.push(Constraint::Length(1)); // 8: learn more l2
            constraints.push(Constraint::Length(1)); // 9: learn more l3
        }
        let offset = if self.learn_more_expanded { 3 } else { 0 };
        constraints.push(Constraint::Length(1));    // checkbox
        constraints.push(Constraint::Length(1));    // next step / instruction
        constraints.push(Constraint::Length(1));    // hint
        constraints.push(Constraint::Length(1));    // step indicator
        let rows = Layout::vertical(constraints).split(content_area);

        render_header(frame, rows[0], wide);

        // Title
        let title = Paragraph::new(t!("tui.entry.recovery_key_write_down"))
            .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[1]);

        // Instruction
        let instruction = Paragraph::new(t!("tui.entry.recovery_key_instruction"))
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[2]);

        // Word grid (read-only)
        if self.recovery_words.is_none() {
            let placeholder = Paragraph::new(t!("tui.entry.generating_recovery_key"))
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
            let grid_area = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER));
            frame.render_widget(grid_area, rows[3]);
            // Render placeholder centered inside grid
            let inner = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(1),
                Constraint::Fill(1),
            ])
            .split(rows[3]);
            frame.render_widget(placeholder, inner[1]);
        } else {
            // Build a read-only 4x6 grid showing the recovery words
            self.render_readonly_word_grid(frame, rows[3]);
        }

        // Buttons row: [ Copy to clipboard ]  [ Regenerate ]
        let btn_area = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Length(22),
            Constraint::Length(4),
            Constraint::Length(16),
            Constraint::Fill(1),
        ])
        .split(rows[4]);

        let copy_focused = self.recovery_focus == RecoveryFocus::CopyButton;
        let copy_style = if copy_focused {
            Styles::button_primary()
        } else {
            Style::default().fg(TEXT)
        };
        let copy_text = format!("[{}]", t!("tui.entry.copy_to_clipboard_btn"));
        let copy_btn = Paragraph::new(copy_text)
            .style(copy_style)
            .alignment(Alignment::Center);
        frame.render_widget(copy_btn, btn_area[1]);
        self.recovery_action_areas[0].set(btn_area[1]);

        let regen_focused = self.recovery_focus == RecoveryFocus::RegenerateButton;
        let regen_style = if regen_focused {
            Styles::button_primary()
        } else {
            Style::default().fg(TEXT)
        };
        let regen_text = format!("[{}]", t!("tui.entry.regenerate_btn"));
        let regen_btn = Paragraph::new(regen_text)
            .style(regen_style)
            .alignment(Alignment::Center);
        frame.render_widget(regen_btn, btn_area[3]);
        self.recovery_action_areas[1].set(btn_area[3]);

        // Clipboard clear warning (shown after copy)
        if self.clipboard_copied {
            let warning = Paragraph::new(
                t!(
                    "tui.entry.clipboard_clear_warning",
                    seconds = self.clipboard_clear_seconds
                )
                .to_string(),
            )
            .style(Styles::warning_text())
            .alignment(Alignment::Center);
            frame.render_widget(warning, rows[5]);
        }

        // Learn more toggle
        let (toggle_text, toggle_style) = if self.learn_more_expanded {
            let focused = self.recovery_focus == RecoveryFocus::LearnMoreToggle;
            let style = if focused {
                Style::default().fg(PRIMARY)
            } else {
                Style::default().fg(TEXT_SECONDARY)
            };
            (t!("tui.entry.recovery_learn_more_expanded").to_string(), style)
        } else {
            let focused = self.recovery_focus == RecoveryFocus::LearnMoreToggle;
            let style = if focused {
                Style::default().fg(PRIMARY)
            } else {
                Style::default().fg(TEXT_MUTED)
            };
            (t!("tui.entry.recovery_learn_more_collapsed").to_string(), style)
        };
        let toggle = Paragraph::new(toggle_text)
            .style(toggle_style)
            .alignment(Alignment::Center);
        frame.render_widget(toggle, rows[6]);
        self.recovery_action_areas[2].set(rows[6]);

        // Learn more content (expanded)
        if self.learn_more_expanded {
            let lines = [
                t!("tui.entry.recovery_learn_more_l1"),
                t!("tui.entry.recovery_learn_more_l2"),
                t!("tui.entry.recovery_learn_more_l3"),
            ];
            for (i, line) in lines.iter().enumerate() {
                let para = Paragraph::new(line.to_string())
                    .style(Style::default().fg(TEXT_SECONDARY))
                    .alignment(Alignment::Center);
                frame.render_widget(para, rows[7 + i]);
            }
        }

        // Checkbox
        let check_icon = if self.recovery_confirmed {
            theme::ICON_CHECK
        } else {
            "[ ]"
        };
        let check_focused = self.recovery_focus == RecoveryFocus::ConfirmCheckbox;
        let check_style = if self.recovery_confirmed && check_focused {
            Style::default().fg(PRIMARY)
        } else if self.recovery_confirmed {
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
        frame.render_widget(checkbox, rows[7 + offset]);
        self.recovery_action_areas[3].set(rows[7 + offset]);

        // Next step button or instruction
        if self.recovery_confirmed {
            let next_focused = check_focused;
            let next_style = if next_focused {
                Styles::button_primary()
            } else {
                Style::default().fg(TEXT)
            };
            let next_text = format!("[{}]", t!("tui.entry.next_step"));
            let next_btn = Paragraph::new(next_text)
                .style(next_style)
                .alignment(Alignment::Center);
            frame.render_widget(next_btn, rows[8 + offset]);
            self.recovery_action_areas[4].set(rows[8 + offset]);
        } else {
            let instruction = Paragraph::new(t!("tui.entry.check_box_to_continue"))
                .style(Style::default().fg(TEXT_SECONDARY))
                .alignment(Alignment::Center);
            frame.render_widget(instruction, rows[8 + offset]);
        }

        // Hint
        let hint = Paragraph::new(t!("tui.entry.recovery_display_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[9 + offset]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = step, total = total).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[10 + offset]);
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
            Constraint::Length(6), // 6 word rows (no spacers)
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

        // Build rows without spacers
        let row_constraints: [Constraint; 6] = std::array::from_fn(|_: usize| Constraint::Length(1));
        let rows = Layout::vertical(row_constraints).split(h_chunks[1]);

        for row in 0..6 {
            let row_idx = row; // no spacers
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
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let content_area = Self::centered_content(area, hdr + 20, 60);

        let rows = Layout::vertical([
            Constraint::Length(hdr),  // logo or brand
            Constraint::Length(1),    // title
            Constraint::Length(1),    // instruction
            Constraint::Length(1),    // label 0
            Constraint::Length(3),    // input box 0
            Constraint::Length(1),    // label 1
            Constraint::Length(3),    // input box 1
            Constraint::Length(1),    // label 2
            Constraint::Length(3),    // input box 2
            Constraint::Length(1),    // label 3
            Constraint::Length(3),    // input box 3
            Constraint::Length(1),    // hint
            Constraint::Length(1),    // step indicator
        ])
        .split(content_area);

        render_header(frame, rows[0], wide);

        // Title
        let title = Paragraph::new(t!("tui.entry.verify_recovery_title"))
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[1]);

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
            frame.render_widget(label, rows[3 + i * 2]);

            // Input box with border
            let border_color = if has_error {
                ERROR
            } else if is_focused {
                PRIMARY
            } else {
                BORDER
            };

            let input_text = if self.verify_inputs[i].is_empty() {
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

            let box_area = rows[4 + i * 2];
            let para = Paragraph::new(input_text).style(text_style);
            let inner = input_block.inner(box_area);
            frame.render_widget(input_block, box_area);
            frame.render_widget(para, inner);
            if is_focused && inner.width > 0 && inner.height > 0 {
                let cursor_offset = self.verify_inputs[i].expose(|s| s.chars().count() as u16);
                let cursor_x = inner.x + cursor_offset.min(inner.width.saturating_sub(1));
                frame.set_cursor_position(Position::new(cursor_x, inner.y));
            }
            self.verify_box_areas[i].set(box_area);
        }

        // Hint
        let hint = Paragraph::new(t!("tui.entry.verify_hint"))
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

    pub(crate) fn view_recovery_input(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let content_area = Self::centered_content(area, hdr + 16, 60);

        let rows = Layout::vertical([
            Constraint::Length(hdr),  // logo or brand
            Constraint::Length(1),    // title
            Constraint::Length(1),    // gap
            Constraint::Length(10),   // grid
            Constraint::Length(1),    // gap
            Constraint::Length(1),    // hint
            Constraint::Length(1),    // step indicator
        ])
        .split(content_area);

        render_header(frame, rows[0], wide);

        // Title
        let title = Paragraph::new(t!("tui.entry.enter_recovery_title"))
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[1]);

        // Grid
        self.recovery_grid.view(frame, rows[3]);

        // Hint
        let hint = Paragraph::new(t!("tui.entry.recovery_input_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = step, total = total).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[6]);
    }

    pub(crate) fn view_security_advisory(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let content_area = Self::centered_content(area, hdr + 10, 60);

        let rows = Layout::vertical([
            Constraint::Length(hdr),  // logo or brand
            Constraint::Length(1),    // title
            Constraint::Length(1),    // gap
            Constraint::Length(3),    // notice
            Constraint::Length(1),    // gap
            Constraint::Length(1),    // hint
            Constraint::Length(1),    // step indicator
        ])
        .split(content_area);

        render_header(frame, rows[0], wide);

        // Title
        let title = Paragraph::new(format!(
            "{} {}",
            theme::ICON_WARNING,
            t!("tui.entry.security_notice")
        ))
        .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[1]);

        // Notice
        let notice = Paragraph::new(t!("tui.entry.security_notice_body"))
            .style(Style::default().fg(TEXT))
            .wrap(Wrap { trim: true });
        frame.render_widget(notice, rows[3]);

        // Hint
        let hint = Paragraph::new(t!("tui.entry.security_notice_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = step, total = total).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[6]);
    }
}
