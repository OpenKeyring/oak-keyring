use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::Paragraph;
use ratatui::{Frame, layout::Rect};

use crate::tui::state::config_state::AboutInfo;

pub fn render(frame: &mut Frame, area: Rect, info: &AboutInfo) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Version
            Constraint::Length(1), // Author
            Constraint::Length(1), // License
        ])
        .split(area);

    let title = Paragraph::new("关于").style(Style::default().fg(Color::Rgb(86, 95, 137)).bold());
    frame.render_widget(title, chunks[0]);

    let version = format!("版本                {}", info.version);
    frame.render_widget(
        Paragraph::new(version).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[1],
    );

    let author = format!("作者                {}", info.author);
    frame.render_widget(
        Paragraph::new(author).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[2],
    );

    let license = format!("许可证              {}", info.license);
    frame.render_widget(
        Paragraph::new(license).style(Style::default().fg(Color::Rgb(192, 202, 245))),
        chunks[3],
    );
}
