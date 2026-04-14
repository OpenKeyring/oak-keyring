// TODO: Implement unlock screen per U1 spec

use crate::commands::Message;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};

/// Unlock screen state: master password input with error display.
#[derive(Debug, Default)]
pub struct UnlockScreen {
    pub password_input: String,
    pub show_error: bool,
    pub error_message: String,
}

impl crate::tui::traits::screen::Screen for UnlockScreen {
    fn update(&mut self, _msg: Message, _ctx: &mut ScreenContext) -> ScreenResult {
        // TODO: implement unlock screen state machine (Task 14)
        ScreenResult::Continue
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        // TODO: implement unlock screen view (Task 14)
        use ratatui::layout::Alignment;
        use ratatui::widgets::{Block, Borders, Paragraph};

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(crate::tui::theme::Styles::focused_border())
            .title(" OpenKeyring ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = if self.show_error {
            format!("Password: {}\n\nError: {}", self.password_input, self.error_message)
        } else {
            "Enter master password to unlock".to_string()
        };
        let paragraph = Paragraph::new(text)
            .style(crate::tui::theme::Styles::password_input())
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
    }

    fn on_mount(&mut self, _ctx: &ScreenContext) {
        self.password_input.clear();
        self.show_error = false;
        self.error_message.clear();
    }

    fn on_unmount(&mut self) {}
}
