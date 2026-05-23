use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::t;
use crate::tui::screens::onboarding::logo;
use crate::tui::terminal::WidthTier;
use crate::tui::theme::{
    self, Styles, BG, BG_SURFACE, BORDER, BRAND, PRIMARY, TEXT, TEXT_MUTED, TEXT_SECONDARY,
};

use super::screen::OnboardingScreen;

/// Render the logo (wide) or brand text + separator (narrow) into the given area.
/// Returns the number of rows consumed: `LOGO_HEIGHT + 1` for wide, `2` for narrow.
pub(crate) fn render_header(frame: &mut ratatui::Frame, area: ratatui::layout::Rect, wide: bool) {
    if wide {
        let chunks =
            Layout::vertical([Constraint::Length(logo::LOGO_HEIGHT), Constraint::Length(1)])
                .split(area);

        let logo_widget = Paragraph::new(logo::ascii_logo_lines()).alignment(Alignment::Center);
        frame.render_widget(logo_widget, chunks[0]);
    } else {
        let chunks = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

        let brand = Paragraph::new(Line::from(vec![
            Span::styled(format!("{} ", theme::ICON_LOCK), Style::default().fg(BRAND)),
            Span::styled(
                t!("tui.entry.brand"),
                Style::default().fg(BRAND).add_modifier(Modifier::BOLD),
            ),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(brand, chunks[0]);

        let separator = Paragraph::new(Span::styled(
            "\u{2500}".repeat(40),
            Style::default().fg(BORDER),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(separator, chunks[1]);
    }
}

/// Header rows: wide = LOGO_HEIGHT + 1 gap, narrow = 1 brand + 1 separator.
pub(crate) const fn header_rows(wide: bool) -> u16 {
    if wide {
        logo::LOGO_HEIGHT + 1
    } else {
        2
    }
}

impl OnboardingScreen {
    /// Render a centered content block with standard padding.
    pub(crate) fn centered_content(
        area: ratatui::layout::Rect,
        content_height: u16,
        content_width: u16,
    ) -> ratatui::layout::Rect {
        let outer = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(content_height),
            Constraint::Fill(1),
        ])
        .split(area);

        let h_layout = Layout::horizontal([
            Constraint::Fill(1),
            Constraint::Max(content_width),
            Constraint::Fill(1),
        ])
        .split(outer[1]);

        h_layout[1]
    }

    pub(crate) fn view_welcome(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let content_area = Self::centered_content(area, hdr + 18, 60);

        let rows = Layout::vertical(vec![
            Constraint::Length(hdr), // logo or brand
            Constraint::Length(1),   // subtitle
            Constraint::Length(1),   // gap
            Constraint::Length(3),   // card 0 — CreateNew
            Constraint::Length(1),   // gap
            Constraint::Length(3),   // card 1 — Restore
            Constraint::Length(1),   // gap
            Constraint::Length(3),   // card 2 — Import
            Constraint::Length(2),   // gap
            Constraint::Length(1),   // language hint
            Constraint::Length(1),   // hint
            Constraint::Length(1),   // step indicator
        ])
        .split(content_area);

        render_header(frame, rows[0], wide);

        // Subtitle
        let subtitle = Paragraph::new(Span::styled(
            t!("tui.entry.onboarding_subtitle"),
            Style::default().fg(TEXT_SECONDARY),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(subtitle, rows[1]);

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
            let card_row = rows[3 + i * 2];
            self.welcome_card_areas[i].set(card_row);

            let border_color = if is_selected { PRIMARY } else { BORDER };
            let bg_color = if is_selected { BG_SURFACE } else { BG };

            let card_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(bg_color));

            let inner = card_block.inner(card_row);
            frame.render_widget(card_block, card_row);

            let card_lines =
                Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(inner);

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
        let lang_text = t!(
            "tui.entry.language_hint",
            lang = languages[self.language_index]
        )
        .to_string();
        let lang_hint = Paragraph::new(lang_text)
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(lang_hint, rows[9]);

        // Hint
        let hint = Paragraph::new(t!("tui.entry.onboarding_welcome_hint"))
            .style(Style::default().fg(TEXT_MUTED))
            .alignment(Alignment::Center);
        frame.render_widget(hint, rows[10]);

        // Step indicator
        let step_text =
            Paragraph::new(t!("tui.entry.step_n_of_n", current = 1, total = 1).to_string())
                .style(Style::default().fg(TEXT_MUTED))
                .alignment(Alignment::Center);
        frame.render_widget(step_text, rows[11]);
    }

    pub(crate) fn view_set_password(
        &self,
        frame: &mut ratatui::Frame,
        area: ratatui::layout::Rect,
    ) {
        let wide = WidthTier::from_width(area.width) != WidthTier::TooSmall;
        let hdr = header_rows(wide);
        let content_area = Self::centered_content(area, hdr + 7, 60);

        let rows = Layout::vertical([
            Constraint::Length(hdr), // logo or brand
            Constraint::Length(1),   // title
            Constraint::Length(1),   // gap
            Constraint::Length(1),   // instruction
            Constraint::Length(1),   // gap
            Constraint::Length(1),   // hint
            Constraint::Length(1),   // step indicator
        ])
        .split(content_area);

        render_header(frame, rows[0], wide);

        // Title
        let title = Paragraph::new(t!("tui.entry.set_password_title"))
            .style(Styles::brand_text())
            .alignment(Alignment::Center);
        frame.render_widget(title, rows[1]);

        // Instruction
        let instruction = Paragraph::new(t!("tui.entry.set_password_redirect"))
            .style(Style::default().fg(TEXT_SECONDARY))
            .alignment(Alignment::Center);
        frame.render_widget(instruction, rows[3]);

        // Hint
        let hint = Paragraph::new(t!("tui.entry.enter_to_continue_esc_back"))
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
