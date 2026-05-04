pub mod batch_tag;
pub mod confirm;
pub mod error_dialog;
pub mod generator;
pub mod help;
pub mod password_history;

use crossterm::event::KeyCode;
use ratatui::{layout::Rect, Frame};
use uuid::Uuid;

use crate::commands::types::{ConfirmButton, ConfirmVariant, Overlay};
use crate::tui::state::generator_state::GeneratorState;
use crate::tui::state::overlay_state::{
    BatchTagPanelFullState, ErrorDialogFullState, PasswordHistoryState,
};

/// Return a `Rect` of size `width × height` centred inside `area`.
pub(crate) fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area
        .x
        .checked_add((area.width.saturating_sub(width)) / 2)
        .unwrap_or(area.x);
    let y = area
        .y
        .checked_add((area.height.saturating_sub(height)) / 2)
        .unwrap_or(area.y);
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

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
    PasswordGenerator(GeneratorState),
}

/// Tells the caller which panel/element should receive keyboard focus after
/// an overlay is closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusRestoreTarget {
    /// Return focus to whatever had it before the overlay opened.
    PreOpenPosition,
    /// Move focus to the next item in the list panel.
    ListPanelNextItem,
    /// Move focus to the next item after a delete operation.
    ListPanelNextItemAfterDelete,
    /// The list panel is now empty.
    ListPanelEmpty,
    /// Return the list panel to normal (non-visual) mode.
    ListPanelNormalMode,
    /// Move focus to the next tag in the sidebar.
    SidebarNextTag,
    /// Focus the password field in the detail panel.
    DetailPanelPasswordField,
}

/// Result of dispatching a key event through the OverlayManager.
#[derive(Debug, Clone)]
pub enum OverlayKeyResult {
    /// Key was consumed internally (navigation, text input, etc.).
    Consumed,
    /// Overlay should be closed; restore focus to the specified target.
    Close { restore: FocusRestoreTarget },
    /// No overlay is active — key was not consumed.
    None,
    /// User confirmed the action described by `variant`.
    ConfirmAction { variant: ConfirmVariant },
    /// Copy a historical password entry to the clipboard.
    CopyHistoryPassword { history_id: i64 },
    /// Copy the generated password to the clipboard.
    CopyGeneratedPassword { password: String },
    /// Add a tag to the selected records.
    BatchAddTag {
        record_ids: Vec<Uuid>,
        tag_name: String,
    },
    /// Remove a tag from the selected records.
    BatchRemoveTag {
        record_ids: Vec<Uuid>,
        tag_name: String,
    },
    /// User wants to retry the failed operation.
    ErrorRetry,
    /// User wants to quit / dismiss the error dialog.
    ErrorQuit,
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

    /// Render the active overlay (if any) on top of the main screen.
    pub fn render(&self, frame: &mut Frame, area: Rect, unicode: bool) {
        if let Some(ref overlay) = self.active {
            match overlay {
                ActiveOverlay::Help => help::render_help(frame, area),
                ActiveOverlay::PasswordHistory(state) => {
                    password_history::render_password_history(frame, area, state);
                }
                ActiveOverlay::ConfirmDialog {
                    variant,
                    focused_button,
                } => confirm::render_confirm(frame, area, variant, *focused_button),
                ActiveOverlay::BatchTagPanel(state) => {
                    batch_tag::render_batch_tag(frame, area, state);
                }
                ActiveOverlay::ErrorDialog(state) => {
                    error_dialog::render_error_dialog(frame, area, state);
                }
                ActiveOverlay::PasswordGenerator(state) => {
                    generator::render_generator(frame, area, state, unicode);
                }
            }
        }
    }

    /// Dispatch a key event to the active overlay and return the result.
    ///
    /// Returns `OverlayKeyResult::None` when no overlay is active.
    pub fn handle_key(&mut self, key: KeyCode) -> OverlayKeyResult {
        let Some(overlay) = self.active.as_mut() else {
            return OverlayKeyResult::None;
        };

        match overlay {
            ActiveOverlay::Help => match key {
                KeyCode::F(1) | KeyCode::Esc => OverlayKeyResult::Close {
                    restore: FocusRestoreTarget::PreOpenPosition,
                },
                _ => OverlayKeyResult::Consumed,
            },

            ActiveOverlay::PasswordHistory(state) => {
                let action = password_history::handle_key(key, state);
                match action {
                    password_history::HistoryAction::Close => OverlayKeyResult::Close {
                        restore: FocusRestoreTarget::DetailPanelPasswordField,
                    },
                    password_history::HistoryAction::CopySelected => {
                        if state.selected_index < state.entries.len() {
                            OverlayKeyResult::CopyHistoryPassword {
                                history_id: state.entries[state.selected_index].id,
                            }
                        } else {
                            OverlayKeyResult::Consumed
                        }
                    }
                    password_history::HistoryAction::None
                    | password_history::HistoryAction::MoveUp
                    | password_history::HistoryAction::MoveDown => OverlayKeyResult::Consumed,
                }
            }

            ActiveOverlay::ConfirmDialog {
                variant,
                focused_button,
            } => {
                let result = confirm::handle_key(key, focused_button);
                match result {
                    Some(true) => OverlayKeyResult::ConfirmAction {
                        variant: variant.clone(),
                    },
                    Some(false) => OverlayKeyResult::Close {
                        restore: FocusRestoreTarget::PreOpenPosition,
                    },
                    None => OverlayKeyResult::Consumed,
                }
            }

            ActiveOverlay::BatchTagPanel(state) => {
                let action = batch_tag::handle_key(key, state);
                match action {
                    batch_tag::BatchTagAction::Close => OverlayKeyResult::Close {
                        restore: FocusRestoreTarget::PreOpenPosition,
                    },
                    batch_tag::BatchTagAction::AddTag(tag_name) => OverlayKeyResult::BatchAddTag {
                        record_ids: state.selected_record_ids.clone(),
                        tag_name,
                    },
                    batch_tag::BatchTagAction::RemoveTag(tag_name) => {
                        OverlayKeyResult::BatchRemoveTag {
                            record_ids: state.selected_record_ids.clone(),
                            tag_name,
                        }
                    }
                    batch_tag::BatchTagAction::None => OverlayKeyResult::Consumed,
                }
            }

            ActiveOverlay::ErrorDialog(state) => {
                let action = error_dialog::handle_key(key, state);
                match action {
                    error_dialog::ErrorDialogAction::Retry => OverlayKeyResult::ErrorRetry,
                    error_dialog::ErrorDialogAction::Quit => OverlayKeyResult::ErrorQuit,
                    error_dialog::ErrorDialogAction::None => OverlayKeyResult::Consumed,
                }
            }

            ActiveOverlay::PasswordGenerator(state) => {
                let action = generator::handle_key(key, state);
                match action {
                    generator::GeneratorAction::Close => OverlayKeyResult::Close {
                        restore: FocusRestoreTarget::PreOpenPosition,
                    },
                    generator::GeneratorAction::CopyToClipboard => {
                        OverlayKeyResult::CopyGeneratedPassword {
                            password: state.preview.clone(),
                        }
                    }
                    // Regenerate and None are consumed internally
                    _ => OverlayKeyResult::Consumed,
                }
            }
        }
    }

    /// Convert a command-layer `Overlay` into an `ActiveOverlay` with default state.
    fn into_active(overlay: Overlay) -> ActiveOverlay {
        match overlay {
            Overlay::Help => ActiveOverlay::Help,
            Overlay::PasswordHistory { record_id } => {
                ActiveOverlay::PasswordHistory(PasswordHistoryState {
                    record_id,
                    record_name: String::new(),
                    entries: Vec::new(),
                    selected_index: 0,
                })
            }
            Overlay::ConfirmDialog(state) => {
                let default_button = default_confirm_button(&state.variant);
                ActiveOverlay::ConfirmDialog {
                    variant: state.variant,
                    focused_button: default_button,
                }
            }
            Overlay::BatchTagPanel(state) => ActiveOverlay::BatchTagPanel(BatchTagPanelFullState {
                selected_record_ids: state.record_ids,
                selected_record_names: Vec::new(),
                input_text: String::new(),
                current_tags: Vec::new(),
                available_tags: Vec::new(),
                focus: Default::default(),
                tag_cursor: 0,
                current_tag: state.current_tag,
            }),
            Overlay::ErrorDialog(state) => ActiveOverlay::ErrorDialog(ErrorDialogFullState {
                title: state.title,
                message: state.message,
                detail: state.detail,
                actions: Default::default(),
                focused_button: 0,
            }),
            Overlay::PasswordGenerator => ActiveOverlay::PasswordGenerator(GeneratorState::new()),
        }
    }
}

/// Choose the safe default focus button for a confirm dialog.
/// Reversible actions default to Confirm; irreversible actions default to Cancel.
fn default_confirm_button(variant: &ConfirmVariant) -> ConfirmButton {
    match variant {
        // Irreversible actions — default to Cancel for safety.
        ConfirmVariant::HardDelete { .. }
        | ConfirmVariant::EmptyTrash { .. }
        | ConfirmVariant::TagDelete { .. } => ConfirmButton::Cancel,
        // Reversible actions — default to Confirm.
        ConfirmVariant::SoftDelete { .. }
        | ConfirmVariant::BatchSoftDelete { .. }
        | ConfirmVariant::Restore { .. } => ConfirmButton::Confirm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::{BatchTagPanelState, ConfirmDialogState, ErrorDialogState};
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
        mgr.close();

        // EmptyTrash is irreversible → default focus is Cancel.
        let state = ConfirmDialogState {
            variant: ConfirmVariant::EmptyTrash { count: 5 },
            focused_button: ConfirmButton::Confirm,
        };
        assert!(mgr.open(Overlay::ConfirmDialog(state)));
        match mgr.get() {
            Some(ActiveOverlay::ConfirmDialog { focused_button, .. }) => {
                assert_eq!(*focused_button, ConfirmButton::Cancel);
            }
            _ => panic!("Expected ConfirmDialog"),
        }
        mgr.close();

        // TagDelete is irreversible → default focus is Cancel.
        let state = ConfirmDialogState {
            variant: ConfirmVariant::TagDelete {
                tag_name: "work".to_string(),
                affected_count: 3,
            },
            focused_button: ConfirmButton::Confirm,
        };
        assert!(mgr.open(Overlay::ConfirmDialog(state)));
        match mgr.get() {
            Some(ActiveOverlay::ConfirmDialog { focused_button, .. }) => {
                assert_eq!(*focused_button, ConfirmButton::Cancel);
            }
            _ => panic!("Expected ConfirmDialog"),
        }
        mgr.close();

        // BatchSoftDelete is reversible → default focus is Confirm.
        let state = ConfirmDialogState {
            variant: ConfirmVariant::BatchSoftDelete {
                record_ids: vec![id],
                record_names: vec!["test".to_string()],
            },
            focused_button: ConfirmButton::Cancel,
        };
        assert!(mgr.open(Overlay::ConfirmDialog(state)));
        match mgr.get() {
            Some(ActiveOverlay::ConfirmDialog { focused_button, .. }) => {
                assert_eq!(*focused_button, ConfirmButton::Confirm);
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
