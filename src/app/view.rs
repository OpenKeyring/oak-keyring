//! View dispatch — routes rendering to the current screen.

use ratatui::Frame;

use crate::app::App;
use crate::commands::types::Screen;
use crate::tui::theme;
use crate::tui::traits::screen::Screen as ScreenTrait;

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Show "terminal too small" warning if below minimum size.
    if app.state.too_small {
        render_too_small(frame, area);
        return;
    }

    // Route to current screen's view().
    match app.state.current_screen {
        Screen::Main => {
            app.state.screens.main.view(frame, area);
        }
        Screen::Unlock => {
            app.state.screens.unlock.view(frame, area);
        }
        Screen::Onboarding => {
            app.state.screens.onboarding.view(frame, area);
        }
        Screen::Config => {
            app.state.screens.config.view(frame, area);
        }
        Screen::ChangeMasterPassword => {
            app.state.screens.change_master_password.view(frame, area);
        }
        Screen::ImportExport => {
            app.state.screens.import_export.view(frame, area);
        }
        // Placeholder for unimplemented screens.
        _ => {
            render_placeholder(frame, area, &format!("{:?}", app.state.current_screen));
        }
    }
}

fn render_too_small(frame: &mut Frame, area: ratatui::layout::Rect) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let (w, h) = (area.width, area.height);
    let text = format!("Terminal too small: {}x{}\nMinimum required: 80x24", w, h);
    let paragraph = Paragraph::new(text)
        .style(theme::Styles::warning_text())
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::Styles::error_border()),
        );
    frame.render_widget(paragraph, area);
}

fn render_placeholder(frame: &mut Frame, area: ratatui::layout::Rect, screen_name: &str) {
    use ratatui::layout::Alignment;
    use ratatui::widgets::{Block, Borders, Paragraph};

    let text = format!("Screen: {} (not yet implemented)", screen_name);
    let paragraph = Paragraph::new(text)
        .style(theme::Styles::dim_text())
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme::Styles::unfocused_border()),
        );
    frame.render_widget(paragraph, area);
}
