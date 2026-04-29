//! Focus management: panel cycling, focus stack for overlay restoration.

use crate::commands::types::PanelId;

/// Three-level focus path: panel -> row -> sub-item
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusPath {
    pub panel: PanelId,
    pub row: Option<usize>,
    pub sub_item: Option<usize>,
}

impl FocusPath {
    pub fn new(panel: PanelId) -> Self {
        Self {
            panel,
            row: None,
            sub_item: None,
        }
    }

    pub fn with_row(mut self, row: usize) -> Self {
        self.row = Some(row);
        self
    }

    pub fn with_sub_item(mut self, sub_item: usize) -> Self {
        self.sub_item = Some(sub_item);
        self
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::PanelId;

    #[test]
    fn focus_path_new_defaults() {
        let path = FocusPath::new(PanelId::Sidebar);
        assert_eq!(path.panel, PanelId::Sidebar);
        assert_eq!(path.row, None);
        assert_eq!(path.sub_item, None);
    }

    #[test]
    fn focus_path_builder_pattern() {
        let path = FocusPath::new(PanelId::List).with_row(3).with_sub_item(1);
        assert_eq!(path.panel, PanelId::List);
        assert_eq!(path.row, Some(3));
        assert_eq!(path.sub_item, Some(1));
    }
}
