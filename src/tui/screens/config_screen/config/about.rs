use ratatui::layout::Constraint;
use ratatui::{layout::Rect, Frame};

use crate::t;
use crate::tui::state::config_state::AboutInfo;

pub fn render(frame: &mut Frame, area: Rect, info: &AboutInfo) {
    let chunks = super::render::vertical_chunks(
        area,
        &[
            Constraint::Length(2), // Title
            Constraint::Length(1), // Version
            Constraint::Length(1), // Author
            Constraint::Length(1), // License
            Constraint::Min(0),
        ],
    );

    super::render::render_section_title(frame, chunks[0], t!("tui.config.tab_about").as_ref());
    super::render::muted_info_row(
        frame,
        chunks[1],
        t!("tui.config.about_version").as_ref(),
        info.version,
    );
    super::render::muted_info_row(
        frame,
        chunks[2],
        t!("tui.config.about_authors").as_ref(),
        info.author,
    );
    super::render::muted_info_row(
        frame,
        chunks[3],
        t!("tui.config.about_license").as_ref(),
        info.license,
    );
}
