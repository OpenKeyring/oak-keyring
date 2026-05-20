use uuid::Uuid;
use zeroize::Zeroize;

use crate::commands::types::ImportPreview;
use crate::commands::Message;
use crate::crypto::bip39::{MnemonicLanguage, Passkey};
use crate::t;
use crate::tui::screens::recovery_key::WordGridState;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::sensitive::SensitiveInput;
use crate::types::RecoveryWords;

use super::types::{OnboardingPath, OnboardingStep, RecoveryFocus};

// ── OnboardingScreen ──────────────────────────────────────────────────────

/// Onboarding wizard state: multi-path step-by-step initial setup flow.
#[derive(Debug)]
pub struct OnboardingScreen {
    pub current_step: OnboardingStep,
    pub selected_path: Option<OnboardingPath>,
    pub error: Option<String>,
    pub recovery_confirmed: bool,
    /// Currently highlighted card index on the Welcome step (0..3).
    pub welcome_selected: usize,
    /// Currently selected language index (0=auto, 1=en, 2=zh-CN).
    pub language_index: usize,
    /// Secure owner for generated 24-word recovery words.
    pub recovery_words: Option<RecoveryWords>,
    /// Embedded grid for RecoveryInput step.
    pub recovery_grid: WordGridState,
    /// Verify step inputs for 4 positions.
    pub verify_inputs: [SensitiveInput; 4],
    pub verify_errors: [bool; 4],
    pub verify_positions: [usize; 4],
    /// Currently focused verification input box index (0-3) on the RecoveryVerify step.
    pub verify_focus_index: usize,
    /// Signals that onboarding is returning from ImportExportScreen.
    /// When true, skip ImportSource step and go directly to RecoveryDisplay.
    pub returning_from_import: bool,
    /// Signals that onboarding is returning from SetNewMasterPassword.
    /// When true, skip reset and restore to SetPassword step.
    pub returning_from_set_password: bool,
    // Import state for ImportSource/ImportPreview steps
    pub selected_source_idx: usize,
    pub import_file_path: String,
    pub import_password: SensitiveInput,
    pub import_focus: crate::tui::screens::import_export::ImportFocus,
    pub import_preview: Option<ImportPreview>,
    pub import_session_id: Option<Uuid>,
    /// Whether to import problematic entries as notes instead of skipping them.
    pub import_as_notes: bool,
    /// Whether the checkbox on ImportPreview step is focused.
    pub import_preview_checkbox_focused: bool,
    // RecoveryDisplay step state
    /// Which element is focused on the RecoveryDisplay step.
    pub recovery_focus: RecoveryFocus,
    /// Whether recovery words have been copied to clipboard (show warning).
    pub clipboard_copied: bool,
    /// Clipboard clear timeout in seconds (captured from config when copying).
    pub clipboard_clear_seconds: u64,
    /// Rendered areas of the 4 verify input boxes (for mouse hit-testing).
    /// Uses `Cell` for interior mutability since `view()` takes `&self`.
    pub verify_box_areas: [std::cell::Cell<ratatui::layout::Rect>; 4],
}

impl Default for OnboardingScreen {
    fn default() -> Self {
        use crate::tui::screens::import_export::ImportFocus;
        Self {
            current_step: OnboardingStep::default(),
            selected_path: None,
            error: None,
            recovery_confirmed: false,
            welcome_selected: 0,
            language_index: 0,
            recovery_words: None,
            recovery_grid: WordGridState::default(),
            verify_inputs: std::array::from_fn(|_| SensitiveInput::new()),
            verify_errors: [false; 4],
            verify_positions: [0; 4],
            verify_focus_index: 0,
            returning_from_import: false,
            returning_from_set_password: false,
            selected_source_idx: 0,
            import_file_path: String::new(),
            import_password: SensitiveInput::new(),
            import_focus: ImportFocus::SourceList,
            import_preview: None,
            import_session_id: None,
            import_as_notes: false,
            import_preview_checkbox_focused: false,
            recovery_focus: RecoveryFocus::default(),
            clipboard_copied: false,
            clipboard_clear_seconds: 30,
            verify_box_areas: std::array::from_fn(|_| {
                std::cell::Cell::new(ratatui::layout::Rect::default())
            }),
        }
    }
}

impl OnboardingScreen {
    /// Generate a fresh 24-word BIP39 recovery key and store in `recovery_words`.
    pub(crate) fn generate_recovery_words(&mut self, config_language: &str) {
        let language = MnemonicLanguage::from_config_language(config_language);
        match Passkey::generate(24, language)
            .and_then(|pk| pk.to_recovery_words().map_err(|e| format!("{e:?}")))
        {
            Ok(words) => {
                self.recovery_words = Some(words);
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to generate recovery words");
                self.error = Some(
                    t!(
                        "tui.entry.recovery_key_generation_failed",
                        error = e.to_string()
                    )
                    .to_string(),
                );
            }
        }
    }

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
        self.verify_inputs = std::array::from_fn(|_| SensitiveInput::new());
        self.verify_errors = [false; 4];
        self.verify_focus_index = 0;
    }

    /// Total steps for the current path (including Welcome).
    pub(crate) fn total_steps(&self) -> usize {
        match self.selected_path {
            None => 1,
            Some(OnboardingPath::CreateNew) => 4, // Welcome + RecoveryDisplay + RecoveryVerify + SetPassword
            Some(OnboardingPath::Restore) => 3,   // Welcome + RecoveryInput + SecurityAdvisory
            Some(OnboardingPath::Import) => 6, // Welcome + ImportSource + ImportPreview + RecoveryDisplay + RecoveryVerify + SetPassword
        }
    }

    /// Current step number (1-based).
    pub(crate) fn current_step_number(&self) -> usize {
        match (&self.selected_path, &self.current_step) {
            (None, OnboardingStep::Welcome) => 1,
            // CreateNew path
            (Some(OnboardingPath::CreateNew), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::CreateNew), OnboardingStep::RecoveryDisplay) => 2,
            (Some(OnboardingPath::CreateNew), OnboardingStep::RecoveryVerify { .. }) => 3,
            (Some(OnboardingPath::CreateNew), OnboardingStep::SetPassword) => 4,
            // Restore path
            (Some(OnboardingPath::Restore), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::Restore), OnboardingStep::RecoveryInput) => 2,
            (Some(OnboardingPath::Restore), OnboardingStep::SecurityAdvisory) => 3,
            // Import path
            (Some(OnboardingPath::Import), OnboardingStep::Welcome) => 1,
            (Some(OnboardingPath::Import), OnboardingStep::ImportSource) => 2,
            (Some(OnboardingPath::Import), OnboardingStep::ImportPreview) => 3,
            (Some(OnboardingPath::Import), OnboardingStep::RecoveryDisplay) => 4,
            (Some(OnboardingPath::Import), OnboardingStep::RecoveryVerify { .. }) => 5,
            (Some(OnboardingPath::Import), OnboardingStep::SetPassword) => 6,
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
            Message::MouseEvent(event) => self.handle_mouse(event),
            Message::CommandCompleted(result) => self.handle_command_result(result),
            _ => ScreenResult::Continue,
        }
    }

    fn view(&self, frame: &mut ratatui::Frame, area: ratatui::layout::Rect) {
        match &self.current_step {
            OnboardingStep::Welcome => self.view_welcome(frame, area),
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
        // If returning from ImportExportScreen, resume at RecoveryDisplay step
        if self.returning_from_import {
            self.returning_from_import = false;
            self.current_step = OnboardingStep::RecoveryDisplay;
            return;
        }
        // If returning from SetNewMasterPassword, resume at SetPassword step
        if self.returning_from_set_password {
            self.returning_from_set_password = false;
            self.current_step = OnboardingStep::SetPassword;
            return;
        }
        self.current_step = OnboardingStep::Welcome;
        self.selected_path = None;
        self.error = None;
        self.recovery_confirmed = false;
        self.welcome_selected = 0;
        self.language_index = 0;
        self.recovery_words = None;
        self.recovery_grid.zeroize();
        self.verify_inputs = std::array::from_fn(|_| SensitiveInput::new());
        self.verify_errors = [false; 4];
        self.verify_positions = [0; 4];
        self.verify_focus_index = 0;
        self.selected_source_idx = 0;
        self.import_file_path.clear();
        self.import_password.clear();
        self.import_focus = crate::tui::screens::import_export::ImportFocus::SourceList;
        self.import_preview = None;
        self.import_session_id = None;
        self.import_as_notes = false;
        self.import_preview_checkbox_focused = false;
        self.recovery_focus = RecoveryFocus::default();
        self.clipboard_copied = false;
        self.clipboard_clear_seconds = 30;
    }

    fn on_unmount(&mut self) {
        self.error = None;
        self.recovery_confirmed = false;
        self.recovery_words = None;
        self.recovery_grid.zeroize();
        for input in &mut self.verify_inputs {
            input.clear();
        }
        self.verify_errors = [false; 4];
        self.verify_positions.zeroize();
        self.verify_focus_index = 0;
        self.import_file_path.zeroize();
        self.import_file_path.clear();
        self.import_password.clear();
        self.import_preview = None;
        self.import_session_id = None;
        self.import_as_notes = false;
        self.import_preview_checkbox_focused = false;
        self.recovery_focus = RecoveryFocus::default();
        self.clipboard_copied = false;
        self.clipboard_clear_seconds = 30;
    }
}
