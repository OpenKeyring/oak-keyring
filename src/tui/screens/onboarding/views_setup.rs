use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::t;
use crate::tui::theme::{
    self, Styles, BG, BG_SURFACE, BORDER, BRAND, PRIMARY, TEXT, TEXT_MUTED, TEXT_SECONDARY,
};

use super::screen::OnboardingScreen;
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
        let content_area = Self::centered_content(area, 21);

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
            Constraint::Length(1), // language hint
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
                t!("tui.entry.onboarding_create_card_title"),
                t!("tui.entry.onboarding_create_card_desc"),
            ),
            (
                "\u{21BB}", // ↻
                t!("tui.entry.onboarding_restore_card_title"),
                t!("tui.entry.onboarding_restore_card_desc"),
            ),
            (
                "\u{2193}", // ↓
                t!("tui.entry.onboarding_import_card_title"),
                t!("tui.entry.onboarding_import_card_desc"),
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

        // Language hint
        let languages = ["auto", "en", "zh-CN"];
        let lang_text = format!("L: Language ({})", languages[self.language_index]);
        let lang_hint = Paragraph::new(lang_text)
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(lang_hint, rows[10]);

        // Hint
        let hint = Paragraph::new("\u{2191}\u{2193}/Tab: navigate  |  Enter: select  |  Esc: quit")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[11]);

        // Step indicator
        let step_text = Paragraph::new("Step 1/1")
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[12]);
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
