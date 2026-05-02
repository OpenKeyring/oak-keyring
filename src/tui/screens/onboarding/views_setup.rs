use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::tui::theme::{
    self, Styles, BG, BG_SURFACE, BORDER, BRAND, PRIMARY, TEXT, TEXT_MUTED, TEXT_PLACEHOLDER,
    TEXT_SECONDARY,
};

use super::screen::OnboardingScreen;
use super::types::default_vault_path_display;

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
}
