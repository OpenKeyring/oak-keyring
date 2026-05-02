use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::tui::theme::{
    self, Styles, BG, BG_SURFACE, BORDER, BRAND, ERROR, PRIMARY, SUCCESS, TEXT, TEXT_MUTED,
    TEXT_PLACEHOLDER, TEXT_SECONDARY, WARNING,
};

use super::screen::OnboardingScreen;
use super::types::{default_vault_path_display, RecoveryFocus};

impl OnboardingScreen {
    /// Render a centered content block with standard padding.
    pub(crate) fn centered_content(
        area: ratatui::layout::Rect,
        content_height: u16,
    ) -> ratatui::layout::Rect {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(content_height),
            Constraint::Fill(1),
        ])
        .split(area);

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(60),
            Constraint::Fill(1),
        ])
        .split(outer[1]);

        h_layout[1]
    }

    pub(crate) fn view_welcome(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 20);

        let rows = Layout::vertical([
            Constraint::Length(1), // brand
            Constraint::Length(1), // separator line
            Constraint::Length(1), // subtitle
            Constraint::Length(1), // gap
            Constraint::Length(3), // card 0 — CreateNew
            Constraint::Length(1), // gap
            Constraint::Length(3), // card 1 — Restore
            Constraint::Length(1), // gap
            Constraint::Length(3), // card 2 — Import
            Constraint::Length(2), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Brand
        let brand = Paragraph::new(Line::from(vec![
            Span::styled(format!("{} ", theme::ICON_LOCK), Style::default().fg(BRAND)),
            Span::styled(
                "OpenKeyring",
                Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(brand, rows[0]);

        // Separator
        let separator = Paragraph::new(Span::styled(
            "\u{2500}".repeat(40),
            Style::default().fg(BORDER),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(separator, rows[1]);

        // Subtitle
        let subtitle = Paragraph::new(Span::styled(
            "Secure, open-source terminal password manager",
            Style::default().fg(TEXT_SECONDARY),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(subtitle, rows[2]);

        // Cards
        let cards = [
            (
                "\u{2726}", // ✦
                "Create new vault",
                "Start fresh \u{2014} generate recovery key and set password",
            ),
            (
                "\u{21BB}", // ↻
                "Restore existing vault",
                "Recover an OpenKeyring vault using a recovery key",
            ),
            (
                "\u{2193}", // ↓
                "Import from other manager",
                "Migrate from KeePass, 1Password, Bitwarden, etc.",
            ),
        ];

        for (i, (icon, title, desc)) in cards.iter().enumerate() {
            let is_selected = i == self.welcome_selected;
            let card_row = rows[4 + i * 2];

            let border_color = if is_selected { PRIMARY } else { BORDER };
            let bg_color = if is_selected { BG_SURFACE } else { BG };

            let card_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(bg_color));

            let inner = card_block.inner(card_row);
            frame.render_widget(card_block, card_row);

            // Two lines inside the card: icon + title, then description
            let card_lines = Layout::vertical([
                Constraint::Length(1), // icon + title
                Constraint::Length(1), // description
            ])
            .split(inner);

            let title_line = Paragraph::new(Line::from(vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(BRAND)),
                Span::styled(
                    title.to_string(),
                    Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                ),
            ]));
            frame.render_widget(title_line, card_lines[0]);

            let desc_line = Paragraph::new(Line::from(Span::styled(
                format!("   {}", desc),
                Style::default().fg(TEXT_SECONDARY),
            )));
            frame.render_widget(desc_line, card_lines[1]);
        }

        // Hint
        let hint = Paragraph::new("\u{2191}\u{2193}/Tab: navigate  |  Enter: select  |  Esc: quit")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[10]);

        // Step indicator
        let step_text = Paragraph::new("Step 1/1")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[11]);
    }

    pub(crate) fn view_vault_path(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let content_area = Self::centered_content(area, 14);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(1), // gap
            Constraint::Length(1), // description
            Constraint::Length(1), // gap
            Constraint::Length(3), // path display / input with borders
            Constraint::Length(1), // gap
            Constraint::Length(1), // validation status
            Constraint::Length(1), // gap
            Constraint::Length(1), // buttons (non-editable) or hint (editable)
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Vault Storage")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Description
        let desc = Paragraph::new("Choose where to store the encrypted database and config files.")
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(desc, rows[2]);

        // Path display / input field
        let (border_style, display_text) = if self.vault_path_editable {
            // Editable mode — show actual input with cursor
            let input_style = if self.path_input.is_empty() {
                Style::default().fg(TEXT_PLACEHOLDER)
            } else {
                Style::default().fg(TEXT)
            };
            let text = if self.path_input.is_empty() {
                "Enter custom path...".to_string()
            } else {
                format!("{}_", self.path_input)
            };
            (
                Styles::focused_border(),
                Paragraph::new(text).style(input_style),
            )
        } else {
            // Read-only mode — show default or chosen path
            (
                Style::default().fg(BORDER),
                Paragraph::new(default_vault_path_display())
                    .style(Style::default().fg(TEXT_SECONDARY)),
            )
        };

        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(" Vault Path ");

        frame.render_widget(input_block, rows[4]);

        // Render text inside the bordered area
        let inner = Layout::vertical([Constraint::Length(1)]).split(rows[4])[0];
        let padded = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
        frame.render_widget(display_text, padded[1]);

        // Validation status (show when no command error is present)
        if self.error.is_none() {
            if let Some((msg, is_error)) = self.validate_vault_path() {
                let icon = if is_error {
                    theme::ICON_ERROR
                } else {
                    match msg.as_str() {
                        "Path is valid" => theme::ICON_SUCCESS,
                        _ => theme::ICON_WARNING,
                    }
                };
                let style = if is_error {
                    Styles::error_text()
                } else {
                    match msg.as_str() {
                        "Path is valid" => Styles::success_text(),
                        _ => Styles::warning_text(),
                    }
                };
                let status = Paragraph::new(format!("{} {}", icon, msg)).style(style);
                frame.render_widget(status, rows[6]);
            }
        }

        // Error from command result (takes precedence)
        if let Some(ref err) = self.error {
            let error_text = Paragraph::new(format!("{} {}", theme::ICON_ERROR, err))
                .style(Styles::error_text());
            frame.render_widget(error_text, rows[6]);
        }

        // Buttons or mode hint
        if self.vault_path_editable {
            let mode_hint = Paragraph::new("Enter: confirm  |  Esc: cancel custom path")
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
            frame.render_widget(mode_hint, rows[8]);
        } else {
            // Two side-by-side buttons
            let btn_area = Layout::horizontal([
                Constraint::Fill(1),
                Constraint::Length(22),
                Constraint::Length(2),
                Constraint::Length(20),
                Constraint::Fill(1),
            ])
            .split(rows[8]);

            let default_btn_style = if self.vault_path_focus == 0 {
                Styles::button_primary()
            } else {
                Styles::button_secondary()
            };
            let default_btn = Paragraph::new(" Use default path ")
                .style(default_btn_style)
                .alignment(Alignment::Center);
            frame.render_widget(default_btn, btn_area[1]);

            let custom_btn_style = if self.vault_path_focus == 1 {
                Styles::button_primary()
            } else {
                Styles::button_secondary()
            };
            let custom_btn = Paragraph::new(" Custom path... ")
                .style(custom_btn_style)
                .alignment(Alignment::Center);
            frame.render_widget(custom_btn, btn_area[3]);
        }

        // Hint
        let hint = if self.vault_path_editable {
            "Type a path  |  Enter: confirm  |  Esc: cancel"
        } else {
            "\u{2190}\u{2192}/Tab: switch  |  Enter: select  |  Esc: back"
        };
        let hint = Paragraph::new(hint)
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[10]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[11]);
    }

    pub(crate) fn view_recovery_display(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 22);

        let rows = Layout::vertical([
            Constraint::Length(1),  // title
            Constraint::Length(1),  // gap
            Constraint::Length(10), // word grid (4 rows x 6 cols = ~8 + borders)
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
            "{} Recovery Key - Write These Down!",
            theme::ICON_WARNING
        ))
        .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Word grid (read-only)
        if self.recovery_words.is_empty() {
            let placeholder = Paragraph::new("Generating recovery key...")
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
        let copy_btn = Paragraph::new(" Copy to clipboard ")
            .style(copy_style)
            .alignment(Alignment::Center);
        frame.render_widget(copy_btn, btn_area[1]);

        let regen_style = if self.recovery_focus == RecoveryFocus::RegenerateButton {
            Styles::button_primary()
        } else {
            Styles::button_secondary()
        };
        let regen_btn = Paragraph::new(" Regenerate ")
            .style(regen_style)
            .alignment(Alignment::Center);
        frame.render_widget(regen_btn, btn_area[3]);

        // Clipboard clear warning (shown after copy)
        if self.clipboard_copied {
            let warning = Paragraph::new(format!(
                "{} Clipboard will be cleared after {} seconds",
                theme::ICON_WARNING,
                self.clipboard_clear_seconds
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
        let checkbox = Paragraph::new(format!(" {} I have saved my recovery key", check_icon))
            .style(check_style)
            .alignment(Alignment::Center);
        frame.render_widget(checkbox, rows[8]);

        // Next step button or instruction
        if self.recovery_confirmed {
            let next_style = Styles::button_primary();
            let next_btn = Paragraph::new(" Next step ")
                .style(next_style)
                .alignment(Alignment::Center);
            frame.render_widget(next_btn, rows[10]);
        } else {
            let instruction = Paragraph::new("Check the box above to continue")
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
            frame.render_widget(instruction, rows[10]);
        }

        // Hint
        let hint = Paragraph::new(
            "\u{2190}\u{2192}/Tab: navigate  |  Enter: activate  |  Space: toggle  |  Esc: back",
        )
        .style(Style::default().fg(TEXT_MUTED))
        .alignment(Alignment::Center);
        frame.render_widget(hint, rows[11]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
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
        use ratatui::widgets::{Row, Table};

        let rows: Vec<Row> = (0..6)
            .map(|row| {
                let cells: Vec<Line> = (0..4)
                    .map(|col| {
                        let idx = row * 4 + col;
                        let num_str = format!("{:>2}.", idx + 1);
                        let word = if idx < self.recovery_words.len() {
                            self.recovery_words[idx].as_str()
                        } else {
                            "..."
                        };
                        Line::from(vec![
                            Span::styled(num_str, Style::default().fg(TEXT_SECONDARY)),
                            Span::raw(" "),
                            Span::styled(word.to_string(), Style::default().fg(TEXT)),
                        ])
                    })
                    .collect();
                Row::new(cells)
            })
            .collect();

        let widths = [
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ];

        let table = Table::new(rows, widths).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(BORDER)),
        );

        frame.render_widget(table, area);
    }

    pub(crate) fn view_recovery_verify(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 20);

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
        let title = Paragraph::new("Verify Recovery Key")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Instruction
        let instruction = Paragraph::new("Enter the word at each specified position:")
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[2]);

        // Verify input boxes
        for i in 0..4 {
            let pos = self.verify_positions[i] + 1; // 1-based display
            let is_focused = i == self.verify_focus_index;
            let has_error = self.verify_errors[i];

            // Label
            let label = Paragraph::new(format!("  Word #{}", pos))
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

            let input_text = if self.verify_inputs[i].is_empty() {
                String::new()
            } else if is_focused {
                format!("{}_", self.verify_inputs[i])
            } else {
                self.verify_inputs[i].clone()
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

            let para = Paragraph::new(input_text).style(text_style);
            let inner = input_block.inner(rows[5 + i * 2]);
            frame.render_widget(input_block, rows[5 + i * 2]);
            frame.render_widget(para, inner);
        }

        // Hint
        let hint = Paragraph::new("Tab/Shift+Tab: navigate  |  Enter: verify  |  Esc: back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[12]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
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
        let title = Paragraph::new("Enter Recovery Key")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Grid
        self.recovery_grid.view(frame, rows[2]);

        // Hint
        let hint = Paragraph::new("Tab: next word  |  Enter: submit  |  Esc: go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[4]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
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
        let title = Paragraph::new(format!("{} Security Notice", theme::ICON_WARNING))
            .style(Style::default().fg(WARNING).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Notice
        let notice = Paragraph::new(
            "Your vault has been restored from a recovery key.\n\
             We strongly recommend setting a new master password\n\
             and reviewing your security settings.",
        )
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: true });
        frame.render_widget(notice, rows[2]);

        // Hint
        let hint = Paragraph::new("Press Enter to set a new master password  |  Esc to go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[4]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[5]);
    }

    pub(crate) fn view_set_password(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let content_area = Self::centered_content(area, 8);

        let rows = Layout::vertical([
            Constraint::Length(1), // title
            Constraint::Length(2), // gap
            Constraint::Length(1), // instruction
            Constraint::Length(1), // gap
            Constraint::Length(1), // hint
            Constraint::Length(1), // step indicator
        ])
        .split(content_area);

        // Title
        let title = Paragraph::new("Set Master Password")
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[0]);

        // Instruction
        let instruction = Paragraph::new("You will be redirected to set your master password.")
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        // Hint
        let hint = Paragraph::new("Enter to continue  |  Esc to go back")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[5]);

        // Step indicator
        let step = self.current_step_number();
        let total = self.total_steps();
        let step_text = Paragraph::new(format!("Step {}/{}", step, total))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[6]);
    }

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

        for (i, (_, name, _, scope_hint)) in IMPORT_SOURCES.iter().enumerate() {
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
            let hint_style = Style::default().fg(TEXT_MUTED);
            lines.push(Line::from(vec![
                Span::styled(prefix.to_string(), name_style),
                Span::styled((*name).to_string(), name_style),
                Span::styled(format!("  {}", scope_hint), hint_style),
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
                    Span::styled("\u{26A0} ", Style::default().fg(WARNING)),
                    Span::raw(format!("{} \u{2014} {}", item.name, item.reason)),
                ]));
            }

            for item in preview.failed_items.iter().take(5) {
                lines.push(Line::from(vec![
                    Span::styled("\u{2717} ", Style::default().fg(ERROR)),
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
