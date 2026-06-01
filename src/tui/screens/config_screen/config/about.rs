use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::{layout::Rect, Frame};

use crate::t;
use crate::tui::state::config_state::AboutInfo;
use crate::tui::theme;

pub fn render(frame: &mut Frame, area: Rect, info: &AboutInfo) {
    let title_area = Rect {
        height: area.height.min(2),
        ..area
    };
    super::render::render_section_title(frame, title_area, t!("tui.config.tab_about").as_ref());

    render_product_line(
        frame,
        line_area(area, 2),
        t!("tui.config.about_product").as_ref(),
    );
    render_text_line(
        frame,
        line_area(area, 3),
        t!("tui.config.about_tagline").as_ref(),
    );
    render_text_line(
        frame,
        line_area(area, 4),
        t!("tui.config.about_local_first").as_ref(),
    );
    render_text_line(
        frame,
        line_area(area, 5),
        t!("tui.config.about_encrypted_vault").as_ref(),
    );
    render_text_line(
        frame,
        line_area(area, 6),
        t!("tui.config.about_sync_control").as_ref(),
    );

    render_info_row(
        frame,
        line_area(area, 7),
        t!("tui.config.about_version").as_ref(),
        info.version,
    );
    render_info_row(
        frame,
        line_area(area, 8),
        t!("tui.config.about_authors").as_ref(),
        info.author,
    );
    render_info_row(
        frame,
        line_area(area, 9),
        t!("tui.config.about_license").as_ref(),
        info.license,
    );
}

fn render_product_line(frame: &mut Frame, area: Option<Rect>, text: &str) {
    render_line(
        frame,
        area,
        vec![
            Span::raw("   "),
            Span::styled(
                text.to_string(),
                Style::default()
                    .fg(theme::NL_TEXT)
                    .bg(theme::NL_BG)
                    .add_modifier(Modifier::BOLD),
            ),
        ],
    );
}

fn render_text_line(frame: &mut Frame, area: Option<Rect>, text: &str) {
    render_line(
        frame,
        area,
        vec![
            Span::raw("   "),
            Span::styled(
                text.to_string(),
                Style::default().fg(theme::NL_TEXT_MUTED).bg(theme::NL_BG),
            ),
        ],
    );
}

fn render_info_row(frame: &mut Frame, area: Option<Rect>, label: &str, value: &str) {
    render_line(
        frame,
        area,
        vec![
            Span::raw("   "),
            Span::styled(
                format!("{:<12}", label),
                Style::default().fg(theme::NL_TEXT_MUTED).bg(theme::NL_BG),
            ),
            Span::styled(
                value.to_string(),
                Style::default().fg(theme::NL_TEXT).bg(theme::NL_BG),
            ),
        ],
    );
}

fn render_line(frame: &mut Frame, area: Option<Rect>, spans: Vec<Span<'static>>) {
    let Some(area) = area else {
        return;
    };

    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(theme::Styles::newlook_bg()),
        area,
    );
}

fn line_area(area: Rect, row: u16) -> Option<Rect> {
    if row >= area.height {
        return None;
    }

    Some(Rect {
        y: area.y + row,
        height: 1,
        ..area
    })
}
