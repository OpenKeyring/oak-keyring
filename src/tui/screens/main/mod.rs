pub mod detail;
pub mod layout;
pub mod list;
pub mod overlay;
pub mod sidebar;
pub mod status_bar;

use ratatui::layout::Rect;

pub struct MainScreen;

impl Default for MainScreen {
    fn default() -> Self {
        Self
    }
}

impl MainScreen {
    pub fn new() -> Self {
        Self
    }
    pub fn view(&self, _frame: &mut ratatui::Frame, _area: Rect) {
        // TODO: Render three-panel layout
    }
}
