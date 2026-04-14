use crate::commands::{Command, Message};

pub struct ScreenContext<'a> {
    pub command_tx: &'a tokio::sync::mpsc::Sender<Command>,
    pub config: &'a crate::config::AppConfig,
}

#[derive(Debug, PartialEq)]
pub enum ScreenResult {
    Continue,
    NavigateTo(crate::commands::types::Screen),
    ExitApp,
}

pub trait Screen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult;
    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect);
    fn on_mount(&mut self, ctx: &ScreenContext);
    fn on_unmount(&mut self);
}
