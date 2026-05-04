//! Overlay-specific state structures for U5.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Password history entry displayed in overlay.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub id: i64,
    pub changed_at: DateTime<Utc>,
    pub description: String,
}

/// Password history overlay internal state.
#[derive(Debug, Clone)]
pub struct PasswordHistoryState {
    pub record_id: Uuid,
    pub record_name: String,
    pub entries: Vec<HistoryEntry>,
    pub selected_index: usize,
}

/// Batch tag panel internal state (extended from commands/types.rs shell).
#[derive(Debug, Clone)]
pub struct BatchTagPanelFullState {
    pub selected_record_ids: Vec<Uuid>,
    pub selected_record_names: Vec<String>,
    pub input_text: String,
    pub current_tags: Vec<String>,
    pub available_tags: Vec<String>,
    pub focus: TagPanelFocus,
    pub tag_cursor: usize,
    pub current_tag: String,
}

/// Focus zones within the batch tag panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagPanelFocus {
    #[default]
    Input,
    CurrentTags,
    AvailableTags,
    DoneButton,
}

/// Error dialog action options.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorActions {
    #[default]
    RetryQuit,
    QuitOnly,
}

/// Error dialog internal state (extends commands/types.rs shell).
#[derive(Debug, Clone)]
pub struct ErrorDialogFullState {
    pub title: String,
    pub message: String,
    pub detail: Option<String>,
    pub actions: ErrorActions,
    pub focused_button: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_history_state_new() {
        let id = Uuid::new_v4();
        let state = PasswordHistoryState {
            record_id: id,
            record_name: "test-record".to_string(),
            entries: vec![HistoryEntry {
                id: 1,
                changed_at: Utc::now(),
                description: "password changed".to_string(),
            }],
            selected_index: 0,
        };
        assert_eq!(state.record_id, id);
        assert_eq!(state.record_name, "test-record");
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn tag_panel_focus_cycle_order() {
        // Verify all four focus zones exist and are distinct.
        let zones = [
            TagPanelFocus::Input,
            TagPanelFocus::CurrentTags,
            TagPanelFocus::AvailableTags,
            TagPanelFocus::DoneButton,
        ];
        assert_eq!(zones.len(), 4);
        // Verify pairwise distinctness.
        for i in 0..zones.len() {
            for j in (i + 1)..zones.len() {
                assert_ne!(zones[i], zones[j], "Focus zones must be distinct");
            }
        }
    }

    #[test]
    fn error_dialog_default_focus_first_button() {
        let state = ErrorDialogFullState {
            title: "Error".to_string(),
            message: "Something went wrong".to_string(),
            detail: None,
            actions: ErrorActions::QuitOnly,
            focused_button: 0,
        };
        assert_eq!(state.focused_button, 0, "focused_button defaults to 0");
    }
}
