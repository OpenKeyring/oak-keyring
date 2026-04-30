use std::collections::HashMap;

use crate::commands::types::{PanelId, RecordFilter, RecordSort, Screen};
use crate::tui::screens::import_export::{
    ExportFocus, ExportScopeOption, ExportStep, ImportEntryPoint, ImportExportMode, ImportFocus,
    ImportStep,
};
use crate::tui::state::audit_state::{AuditFilter, AuditFocus};
use crate::tui::state::config_state::ConfigTab;
use crate::tui::state::focus::FocusPath;

// ── Screen Snapshot ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ScreenSnapshot {
    pub screen: Screen,
    pub focus_path: FocusPath,
    pub scroll_positions: HashMap<PanelId, usize>,
    pub restore_state: ScreenRestoreState,
}

// ── Restore State Variants ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ScreenRestoreState {
    None,
    Config(ConfigRestoreState),
    Main(MainRestoreState),
    AuditLog(AuditLogRestoreState),
    ImportExport(ImportExportRestoreState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRestoreState {
    pub active_tab: ConfigTab,
    pub focused_item: usize,
    pub sub_item_focus: Option<usize>,
    pub scroll_offset: u16,
}

#[derive(Debug, Clone)]
pub struct MainRestoreState {
    pub focused_panel: PanelId,
    pub sidebar_selected_index: usize,
    pub sidebar_tags_expanded: bool,
    pub sidebar_tag_scroll_offset: usize,
    pub list_selected_index: Option<usize>,
    pub list_scroll_offset: usize,
    pub current_filter: RecordFilter,
    pub current_sort: RecordSort,
    pub detail_focused_field: usize,
}

#[derive(Debug, Clone)]
pub struct AuditLogRestoreState {
    pub focused_area: AuditFocus,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub filter: AuditFilter,
}

#[derive(Debug, Clone)]
pub struct ImportExportRestoreState {
    pub mode: ImportExportMode,
    pub entry_point: ImportEntryPoint,
    pub import_step: ImportStep,
    pub selected_source_idx: usize,
    pub import_focus: ImportFocus,
    pub export_step: ExportStep,
    pub export_focus: ExportFocus,
    pub export_scope_option: ExportScopeOption,
}

// ── AppState Navigation Methods ─────────────────────────────────────────────

impl crate::tui::state::AppState {
    /// Capture a snapshot of the current screen for restoration.
    pub fn snapshot_current_screen(&self) -> ScreenSnapshot {
        let focus_path = FocusPath::new(self.shared.focus.focused_panel);
        ScreenSnapshot {
            screen: self.current_screen,
            focus_path,
            scroll_positions: HashMap::new(),
            restore_state: ScreenRestoreState::None,
        }
    }

    /// Navigate to a new screen, saving current screen state as a snapshot.
    pub fn navigate_to(&mut self, screen: Screen) {
        let snapshot = self.snapshot_current_screen();
        self.screen_history.push(snapshot);
        self.current_screen = screen;
    }

    /// Go back to the previous screen by restoring the last snapshot.
    /// Returns false if history is empty.
    pub fn go_back(&mut self) -> bool {
        let Some(snapshot) = self.screen_history.pop() else {
            return false;
        };
        self.current_screen = snapshot.screen;
        self.restore_snapshot(snapshot);
        true
    }

    /// Restore focus from a screen snapshot.
    pub fn restore_snapshot(&mut self, snapshot: ScreenSnapshot) {
        self.shared.focus.focused_panel = snapshot.focus_path.panel;
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::commands::types::{PanelId, Screen};
    use crate::tui::state::AppState;

    #[test]
    fn navigate_to_pushes_full_snapshot_without_parallel_stack() {
        let mut state = AppState::default();
        state.current_screen = Screen::Main;
        state.shared.focus.focused_panel = PanelId::Detail;

        state.navigate_to(Screen::Config);

        assert_eq!(state.current_screen, Screen::Config);
        assert_eq!(state.screen_history.len(), 1);
        assert_eq!(state.screen_history[0].screen, Screen::Main);
        assert_eq!(state.screen_history[0].focus_path.panel, PanelId::Detail);
    }

    #[test]
    fn go_back_restores_screen_from_snapshot() {
        let mut state = AppState::default();
        state.current_screen = Screen::Main;
        state.shared.focus.focused_panel = PanelId::List;
        state.navigate_to(Screen::Config);
        state.shared.focus.focused_panel = PanelId::Sidebar;

        assert!(state.go_back());

        assert_eq!(state.current_screen, Screen::Main);
        assert_eq!(state.shared.focus.focused_panel, PanelId::List);
        assert!(state.screen_history.is_empty());
    }

    #[test]
    fn go_back_returns_false_when_history_is_empty() {
        let mut state = AppState::default();
        state.screen_history.clear();

        assert!(!state.go_back());
    }
}
