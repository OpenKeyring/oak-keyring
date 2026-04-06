use crate::commands::{Command, Message};
use crate::tui::traits::screen::ScreenContext;

pub trait Component {
    type State;
    fn update(state: &mut Self::State, msg: Message, ctx: &mut ScreenContext) -> Option<Command>;
    fn view(state: &Self::State, frame: &mut ratatui::Frame, area: ratatui::layout::Rect);
}
