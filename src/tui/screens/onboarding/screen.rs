use zeroize::Zeroize;

use crate::commands::types::ImportPreview;
use crate::commands::Message;
use crate::tui::screens::recovery_key::WordGridState;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};

use super::types::{OnboardingPath, OnboardingStep, RecoveryFocus};

// ── OnboardingScreen ──────────────────────────────────────────────────────

/// Onboarding wizard state: multi-path step-by-step initial setup flow.
#[derive(Debug)]
pub struct OnboardingScreen {
    pub current_step: OnboardingStep,
    pub selected_path: Option<OnboardingPath>,
    pub path_input: String,
    pub error: Option<String>,
    pub recovery_confirmed: bool,
    /// Currently highlighted card index on the Welcome step (0..3).
    pub welcome_selected: usize,
    /// 24 recovery words populated after VaultInitialized command result.
    pub recovery_words: Vec<String>,
    /// Embedded grid for RecoveryInput step.
    pub recovery_grid: WordGridState,
    /// Verify step inputs for 4 positions.
    pub verify_inputs: [String; 4],
    pub verify_errors: [bool; 4],
    pub verify_positions: [usize; 4],
    /// Currently focused verification input box index (0-3) on the RecoveryVerify step.
    pub verify_focus_index: usize,
    /// Signals that onboarding is returning from ImportExportScreen.
    /// When true, skip ImportSource step and go directly to VaultPath.
    pub returning_from_import: bool,
    // Import state for ImportSource/ImportPreview steps
    pub selected_source_idx: usize,
    pub import_file_path: String,
    pub import_password: String,
    pub import_focus: crate::tui::screens::import_export::ImportFocus,
    pub import_preview: Option<ImportPreview>,
    /// Whether to import problematic entries as notes instead of skipping them.
    pub import_as_notes: bool,
    /// Whether the checkbox on ImportPreview step is focused.
    pub import_preview_checkbox_focused: bool,
    // VaultPath step state
    /// Whether the path input is in editable (custom) mode.
    pub vault_path_editable: bool,
    /// Focus index for VaultPath step: 0=Use default button, 1=Custom button, 2=Path input (when editable).
    pub vault_path_focus: usize,
    // RecoveryDisplay step state
    /// Which element is focused on the RecoveryDisplay step.
    pub recovery_focus: RecoveryFocus,
    /// Whether recovery words have been copied to clipboard (show warning).
    pub clipboard_copied: bool,
    /// Clipboard clear timeout in seconds (captured from config when copying).
    pub clipboard_clear_seconds: u64,
}

impl Default for OnboardingScreen {
    fn default() -> Self {
        use crate::tui::screens::import_export::ImportFocus;
        Self {
            current_step: OnboardingStep::default(),
            selected_path: None,
            path_input: String::new(),
            error: None,
            recovery_confirmed: false,
            welcome_selected: 0,
            recovery_words: Vec::new(),
            recovery_grid: WordGridState::default(),
            verify_inputs: std::array::from_fn(|_| String::new()),
            verify_errors: [false; 4],
            verify_positions: [0; 4],
            verify_focus_index: 0,
            returning_from_import: false,
            selected_source_idx: 0,
            import_file_path: String::new(),
            import_password: String::new(),
            import_focus: ImportFocus::SourceList,
            import_preview: None,
            import_as_notes: false,
            import_preview_checkbox_focused: false,
            vault_path_editable: false,
            vault_path_focus: 0,
            recovery_focus: RecoveryFocus::default(),
            clipboard_copied: false,
            clipboard_clear_seconds: 30,
        }
    }
}

impl OnboardingScreen {
    /// Generate 4 random positions for recovery verification.
    pub(crate) fn generate_verify_positions(&mut self) {
        use std::collections::HashSet;
        let mut positions = [0usize; 4];
        let mut used = HashSet::new();
        let mut rng = rand::rng();
        for slot in &mut positions {
            loop {
                let idx = rand::Rng::random_range(&mut rng, 0..24);
                if used.insert(idx) {
                    *slot = idx;
                    break;
                }
            }
        }
        positions.sort();
        self.verify_positions = positions;
        self.verify_inputs = std::array::from_fn(|_| String::new());
        self.verify_errors = [false; 4];
        self.verify_focus_index = 0;
    }

    /// Validate the current vault path and return a status message and severity.
    ///
    /// Returns `Some((message, is_error))` where `is_error` is true for blocking errors.
    pub(crate) fn validate_vault_path(&self) -> Option<(String, bool)> {
        let path = self.resolved_vault_pathbuf();
        if path.as_os_str().is_empty() {
            return None;
        }

        if path.exists() {
            if !path.is_dir() {
                return Some(("Path exists but is not a directory".to_string(), true));
            }

            // Check write permission
            let write_target = path.join(".oak_write_test_tmp");
            let writable = std::fs::write(&write_target, b"").is_ok();
            if writable {
                let _ = std::fs::remove_file(&write_target);
            } else {
                return Some(("No write permission for this directory".to_string(), true));
            }

            // Check if directory is non-empty
            match std::fs::read_dir(path) {
                Ok(mut entries) => {
                    if entries.next().is_some() {
                        return Some((
                            "Directory is not empty — files may be overwritten".to_string(),
                            false,
                        ));
                    }
                }
                Err(e) => {
                    return Some((format!("Cannot read directory: {}", e), true));
                }
            }

            Some(("Path is valid".to_string(), false))
        } else {
            // Path does not exist — check if parent is writable
            match path.parent() {
                Some(parent) if !parent.as_os_str().is_empty() => {
                    if parent.exists() {
                        let write_target = parent.join(".oak_write_test_tmp");
                        let writable = std::fs::write(&write_target, b"").is_ok();
                        if writable {
                            let _ = std::fs::remove_file(&write_target);
                        }
                        if !writable {
                            return Some((
                                "No write permission for parent directory".to_string(),
                                true,
                            ));
                        }
                        Some(("Directory will be created automatically".to_string(), false))
                    } else {
                        // Parent also does not exist — check ancestor chain
                        match parent.parent() {
                            Some(grandparent) if !grandparent.as_os_str().is_empty() => {
                                let write_target = grandparent.join(".oak_write_test_tmp");
                                let writable = std::fs::write(&write_target, b"").is_ok();
                                if writable {
                                    let _ = std::fs::remove_file(&write_target);
                                }
                                if !writable {
                                    return Some((
                                        "Cannot create directory path".to_string(),
                                        true,
                                    ));
                                }
                                Some(("Directory will be created automatically".to_string(), false))
                            }
                            _ => Some(("Invalid path".to_string(), true)),
                        }
                    }
                }
                _ => Some(("Invalid path".to_string(), true)),
            }
        }
    }

    /// Resolve the actual vault path as a PathBuf for filesystem operations.
    pub(crate) fn resolved_vault_pathbuf(&self) -> std::path::PathBuf {
        if self.path_input.is_empty() {
            crate::config::general::default_vault_pathbuf()
        } else {
            std::path::PathBuf::from(&self.path_input)
        }
    }

    /// Total steps for the current path (including Welcome).
    pub(crate) fn total_steps(&self) -> usize {
        match self.selected_path {
            None => 1,
            Some(OnboardingPath::CreateNew) => 5, // Welcome + VaultPath + RecoveryDisplay + RecoveryVerify + SetPassword
            Some(OnboardingPath::Restore) => 4, // Welcome + RecoveryInput + VaultPath + SecurityAdvisory + SetPassword = 5... but spec says 3
            Some(OnboardingPath::Import) => 6, // Welcome + ImportSource + ImportPreview + VaultPath + RecoveryDisplay + RecoveryVerify + SetPassword
        }
    }

    /// Current step number (1-based).
    pub(crate) fn current_step_number(&self) -> usize {
        match (&self.selected_path, &self.current_step) {
            (None, OnboardingStep::Welcome) => 1,
            // CreateNew path
            (Some(OnboardingPath::CreateNew), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::CreateNew), OnboardingStep::VaultPath) => 2,
            (Some(OnboardingPath::CreateNew), OnboardingStep::RecoveryDisplay) => 3,
            (Some(OnboardingPath::CreateNew), OnboardingStep::RecoveryVerify { .. }) => 4,
            (Some(OnboardingPath::CreateNew), OnboardingStep::SetPassword) => 5,
            // Restore path
            (Some(OnboardingPath::Restore), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::Restore), OnboardingStep::RecoveryInput) => 2,
            (Some(OnboardingPath::Restore), OnboardingStep::VaultPath) => 3,
            (Some(OnboardingPath::Restore), OnboardingStep::SecurityAdvisory) => 4,
            // Import path
            (Some(OnboardingPath::Import), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::Import), OnboardingStep::ImportSource) => 2,
            (Some(OnboardingPath::Import), OnboardingStep::ImportPreview) => 3,
            (Some(OnboardingPath::Import), OnboardingStep::VaultPath) => 4,
            (Some(OnboardingPath::Import), OnboardingStep::RecoveryDisplay) => 5,
            (Some(OnboardingPath::Import), OnboardingStep::RecoveryVerify { .. }) => 6,
            // Fallback
            _ => 1,
        }
    }
}

// ── Screen trait ──────────────────────────────────────────────────────────

impl crate::tui::traits::screen::Screen for OnboardingScreen {
    fn update(&mut self, msg: Message, ctx: &mut ScreenContext) -> ScreenResult {
        match msg {
            Message::KeyEvent(key) => self.handle_key(key, ctx),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        match &self.current_step {
            OnboardingStep::Welcome => self.view_welcome(frame, area),
            OnboardingStep::VaultPath => self.view_vault_path(frame, area),
            OnboardingStep::RecoveryDisplay => self.view_recovery_display(frame, area),
            OnboardingStep::RecoveryVerify { .. } => self.view_recovery_verify(frame, area),
            OnboardingStep::RecoveryInput => self.view_recovery_input(frame, area),
            OnboardingStep::SecurityAdvisory => self.view_security_advisory(frame, area),
            OnboardingStep::ImportSource => self.view_import_source(frame, area),
            OnboardingStep::ImportPreview => self.view_import_preview(frame, area),
            OnboardingStep::SetPassword => self.view_set_password(frame, area),
        }
    }

    fn on_mount(&mut self, _ctx: &mut ScreenContext) {
        // If returning from ImportExportScreen, resume at VaultPath step
        if self.returning_from_import {
            self.returning_from_import = false;
            self.current_step = OnboardingStep::VaultPath;
            return;
        }
        self.current_step = OnboardingStep::Welcome;
        self.selected_path = None;
        self.path_input.clear();
        self.error = None;
        self.recovery_confirmed = false;
        self.welcome_selected = 0;
        self.recovery_words.zeroize();
        self.recovery_words.clear();
        self.recovery_grid.zeroize();
        self.verify_inputs = std::array::from_fn(|_| String::new());
        self.verify_errors = [false; 4];
        self.verify_positions = [0; 4];
        self.verify_focus_index = 0;
        self.selected_source_idx = 0;
        self.import_file_path.clear();
        self.import_password.zeroize();
        self.import_password.clear();
        self.import_focus = crate::tui::screens::import_export::ImportFocus::SourceList;
        self.import_preview = None;
        self.import_as_notes = false;
        self.import_preview_checkbox_focused = false;
        self.vault_path_editable = false;
        self.vault_path_focus = 0;
        self.recovery_focus = RecoveryFocus::default();
        self.clipboard_copied = false;
        self.clipboard_clear_seconds = 30;
    }

    fn on_unmount(&mut self) {
        self.path_input.zeroize();
        self.path_input.clear();
        self.error = None;
        self.recovery_confirmed = false;
        self.vault_path_editable = false;
        self.vault_path_focus = 0;
        self.recovery_words.zeroize();
        self.recovery_words.clear();
        self.recovery_grid.zeroize();
        for input in &mut self.verify_inputs {
            input.zeroize();
            input.clear();
        }
        self.verify_errors = [false; 4];
        self.verify_positions.zeroize();
        self.verify_focus_index = 0;
        self.import_file_path.zeroize();
        self.import_file_path.clear();
        self.import_password.zeroize();
        self.import_password.clear();
        self.import_preview = None;
        self.import_as_notes = false;
        self.import_preview_checkbox_focused = false;
        self.recovery_focus = RecoveryFocus::default();
        self.clipboard_copied = false;
        self.clipboard_clear_seconds = 30;
    }
}
