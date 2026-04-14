// TODO: Implement onboarding wizard per U1 spec

use crate::commands::Message;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};

/// Onboarding wizard state: step-by-step initial setup flow.
#[derive(Debug, Default)]
pub struct OnboardingScreen {
    pub current_step: u8,
    pub password_input: String,
    pub confirm_input: String,
}

impl crate::tui::traits::screen::Screen for OnboardingScreen {
    fn update(&mut self, _msg: Message, _ctx: &mut ScreenContext) -> ScreenResult {
        // TODO: implement onboarding screen wizard (Task 17)
        ScreenResult::Continue
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        // TODO: implement onboarding screen view (Task 17)
        use ratatui::layout::Alignment;
        use ratatui::widgets::{Block, Borders, Paragraph};

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(crate::tui::theme::Styles::focused_border())
            .title(" Welcome to OpenKeyring ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let text = format!("Onboarding - Step {}\n\nSet up your vault to get started.", self.current_step);
        let paragraph = Paragraph::new(text)
            .style(crate::tui::theme::Styles::password_input())
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
    }

    fn on_mount(&mut self, _ctx: &ScreenContext) {
        self.current_step = 0;
        self.password_input.clear();
        self.confirm_input.clear();
    }

    fn on_unmount(&mut self) {}
}
