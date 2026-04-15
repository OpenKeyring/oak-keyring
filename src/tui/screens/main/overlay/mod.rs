pub mod batch_tag;
pub mod confirm;
pub mod error_dialog;
pub mod help;
pub mod password_history;

use crate::commands::types::{ConfirmButton, ConfirmVariant, Overlay};
use crate::tui::state::overlay_state::{
    BatchTagPanelFullState, ErrorDialogFullState, PasswordHistoryState,
};

/// Active overlay with its full internal state.
#[derive(Debug, Clone)]
pub enum ActiveOverlay {
    Help,
    PasswordHistory(PasswordHistoryState),
    ConfirmDialog {
        variant: ConfirmVariant,
        focused_button: ConfirmButton,
    },
    BatchTagPanel(BatchTagPanelFullState),
    ErrorDialog(ErrorDialogFullState),
    PasswordGenerator,
}

/// Manages which overlay (if any) is currently displayed.
/// Overlay nesting is blocked — only one overlay can be active at a time.
#[derive(Debug, Clone, Default)]
pub struct OverlayManager {
    active: Option<ActiveOverlay>,
}

impl OverlayManager {
    /// Create a new OverlayManager with no active overlay.
    pub fn new() -> Self {
        Self { active: None }
    }

    /// Returns `true` if an overlay is currently displayed.
    pub fn is_active(&self) -> bool {
        self.active.is_some()
    }

    /// Open an overlay. Returns `true` on success.
    /// Returns `false` if an overlay is already active (nesting is blocked).
    pub fn open(&mut self, overlay: Overlay) -> bool {
        if self.active.is_some() {
            return false;
        }
        self.active = Some(Self::into_active(overlay));
        true
    }

    /// Close the current overlay and return it, if any.
    pub fn close(&mut self) -> Option<ActiveOverlay> {
        self.active.take()
    }

    /// Get a reference to the active overlay.
    pub fn get(&self) -> Option<&ActiveOverlay> {
        self.active.as_ref()
    }

    /// Get a mutable reference to the active overlay.
    pub fn get_mut(&mut self) -> Option<&mut ActiveOverlay> {
        self.active.as_mut()
    }

    /// Convert a command-layer `Overlay` into an `ActiveOverlay` with default state.
    fn into_active(overlay: Overlay) -> ActiveOverlay {
        match overlay {
            Overlay::Help => ActiveOverlay::Help,
            Overlay::PasswordHistory { record_id } => ActiveOverlay::PasswordHistory(
                PasswordHistoryState {
                    record_id,
                    record_name: String::new(),
                    entries: Vec::new(),
                    selected_index: 0,
                },
            ),
            Overlay::ConfirmDialog(state) => {
                let default_button = default_confirm_button(&state.variant);
                ActiveOverlay::ConfirmDialog {
                    variant: state.variant,
                    focused_button: default_button,
                }
            }
            Overlay::BatchTagPanel(state) => ActiveOverlay::BatchTagPanel(
                BatchTagPanelFullState {
                    selected_record_ids: state.record_ids,
                    selected_record_names: Vec::new(),
                    input_text: String::new(),
                    current_tags: Vec::new(),
                    available_tags: Vec::new(),
                    focus: Default::default(),
                    tag_cursor: 0,
                    current_tag: state.current_tag,
                },
            ),
            Overlay::ErrorDialog(state) => ActiveOverlay::ErrorDialog(ErrorDialogFullState {
                title: state.title,
                message: state.message,
                detail: state.detail,
                actions: Default::default(),
                focused_button: 0,
            }),
            Overlay::PasswordGenerator => ActiveOverlay::PasswordGenerator,
        }
    }
}

/// Choose the safe default focus button for a confirm dialog.
/// Reversible actions default to Confirm; irreversible actions default to Cancel.
fn default_confirm_button(variant: &ConfirmVariant) -> ConfirmButton {
    match variant {
        // HardDelete is irreversible — default to Cancel for safety.
        ConfirmVariant::HardDelete { .. } => ConfirmButton::Cancel,
        // All others are reversible — default to Confirm.
        ConfirmVariant::SoftDelete { .. }
        | ConfirmVariant::EmptyTrash { .. }
        | ConfirmVariant::BatchSoftDelete { .. }
        | ConfirmVariant::TagDelete { .. } => ConfirmButton::Confirm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::{
        BatchTagPanelState, ConfirmDialogState, ErrorDialogState,
    };
    use uuid::Uuid;

    #[test]
    fn overlay_manager_new_has_no_active() {
        let mgr = OverlayManager::new();
        assert!(!mgr.is_active());
        assert!(mgr.get().is_none());
    }

    #[test]
    fn open_help_sets_active() {
        let mut mgr = OverlayManager::new();
        assert!(mgr.open(Overlay::Help));
        assert!(mgr.is_active());
        assert!(matches!(mgr.get(), Some(ActiveOverlay::Help)));
    }

    #[test]
    fn nesting_blocked_when_active() {
        let mut mgr = OverlayManager::new();
        assert!(mgr.open(Overlay::Help));
        // Second open should fail.
        assert!(!mgr.open(Overlay::PasswordGenerator));
        // Still Help, not PasswordGenerator.
        assert!(matches!(mgr.get(), Some(ActiveOverlay::Help)));
    }

    #[test]
    fn close_returns_active_and_clears() {
        let mut mgr = OverlayManager::new();
        mgr.open(Overlay::Help);
        let closed = mgr.close();
        assert!(matches!(closed, Some(ActiveOverlay::Help)));
        assert!(!mgr.is_active());
        // Double-close returns None.
        assert!(mgr.close().is_none());
    }

    #[test]
    fn open_confirm_dialog_default_focus() {
        let mut mgr = OverlayManager::new();

        // SoftDelete is reversible → default focus is Confirm.
        let id = Uuid::new_v4();
        let state = ConfirmDialogState {
            variant: ConfirmVariant::SoftDelete {
                record_id: id,
                record_name: "test".to_string(),
                auto_delete_days: None,
            },
            focused_button: ConfirmButton::Cancel, // intentionally wrong; manager overrides
        };
        assert!(mgr.open(Overlay::ConfirmDialog(state)));
        match mgr.get() {
            Some(ActiveOverlay::ConfirmDialog { focused_button, .. }) => {
                assert_eq!(*focused_button, ConfirmButton::Confirm);
            }
            _ => panic!("Expected ConfirmDialog"),
        }
        mgr.close();

        // HardDelete is irreversible → default focus is Cancel.
        let state = ConfirmDialogState {
            variant: ConfirmVariant::HardDelete {
                record_id: id,
                record_name: "test".to_string(),
            },
            focused_button: ConfirmButton::Confirm, // intentionally wrong; manager overrides
        };
        assert!(mgr.open(Overlay::ConfirmDialog(state)));
        match mgr.get() {
            Some(ActiveOverlay::ConfirmDialog { focused_button, .. }) => {
                assert_eq!(*focused_button, ConfirmButton::Cancel);
            }
            _ => panic!("Expected ConfirmDialog"),
        }
    }

    #[test]
    fn open_batch_tag_panel_preserves_tag() {
        let mut mgr = OverlayManager::new();
        let state = BatchTagPanelState {
            record_ids: vec![Uuid::new_v4()],
            current_tag: "work".to_string(),
        };
        assert!(mgr.open(Overlay::BatchTagPanel(state)));
        match mgr.get() {
            Some(ActiveOverlay::BatchTagPanel(full)) => {
                assert_eq!(full.current_tag, "work");
                assert_eq!(full.selected_record_ids.len(), 1);
            }
            _ => panic!("Expected BatchTagPanel"),
        }
    }

    #[test]
    fn open_error_dialog_preserves_fields() {
        let mut mgr = OverlayManager::new();
        let state = ErrorDialogState {
            title: "Oops".to_string(),
            message: "Something failed".to_string(),
            detail: Some("stack trace here".to_string()),
        };
        assert!(mgr.open(Overlay::ErrorDialog(state)));
        match mgr.get() {
            Some(ActiveOverlay::ErrorDialog(full)) => {
                assert_eq!(full.title, "Oops");
                assert_eq!(full.message, "Something failed");
                assert_eq!(full.detail.as_deref(), Some("stack trace here"));
                assert_eq!(full.focused_button, 0);
            }
            _ => panic!("Expected ErrorDialog"),
        }
    }

    #[test]
    fn open_password_history_initial_state() {
        let mut mgr = OverlayManager::new();
        let id = Uuid::new_v4();
        assert!(mgr.open(Overlay::PasswordHistory { record_id: id }));
        match mgr.get() {
            Some(ActiveOverlay::PasswordHistory(state)) => {
                assert_eq!(state.record_id, id);
                assert!(state.entries.is_empty());
                assert_eq!(state.selected_index, 0);
            }
            _ => panic!("Expected PasswordHistory"),
        }
    }

    #[test]
    fn get_mut_allows_modification() {
        let mut mgr = OverlayManager::new();
        mgr.open(Overlay::Help);
        if let Some(overlay) = mgr.get_mut() {
            // We can't mutate Help, but the method works — just verify we got mutable access.
            assert!(matches!(overlay, ActiveOverlay::Help));
        } else {
            panic!("Expected Some");
        }
    }
}
