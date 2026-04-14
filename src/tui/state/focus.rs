//! Focus management: panel cycling, focus stack for overlay restoration.

use crate::commands::types::PanelId;

#[derive(Debug, Clone)]
pub struct FocusState {
    /// Currently focused panel (main layout)
    pub focused_panel: PanelId,
    /// Focus stack for overlay restoration
    pub focus_stack: Vec<PanelId>,
    /// Whether we're in multi-select (visual) mode
    pub visual_mode: bool,
}

impl Default for FocusState {
    fn default() -> Self {
        Self {
            focused_panel: PanelId::Sidebar,
            focus_stack: Vec::new(),
            visual_mode: false,
        }
    }
}

impl FocusState {
    /// Advance focus to the next panel: Sidebar -> List -> Detail -> Sidebar
    pub fn cycle_next(&mut self) {
        self.focused_panel = match self.focused_panel {
            PanelId::Sidebar => PanelId::List,
            PanelId::List => PanelId::Detail,
            PanelId::Detail => PanelId::Sidebar,
        };
    }

    /// Move focus to the previous panel: Sidebar -> Detail -> List -> Sidebar
    pub fn cycle_prev(&mut self) {
        self.focused_panel = match self.focused_panel {
            PanelId::Sidebar => PanelId::Detail,
            PanelId::List => PanelId::Sidebar,
            PanelId::Detail => PanelId::List,
        };
    }

    /// Push the current focus onto the stack (used before opening overlays)
    pub fn push_focus(&mut self) {
        self.focus_stack.push(self.focused_panel);
    }

    /// Pop the last focused panel from the stack (used when closing overlays)
    pub fn pop_focus(&mut self) -> Option<PanelId> {
        self.focus_stack.pop()
    }
}
