// ── Enums ──────────────────────────────────────────────────────────────────

/// The three onboarding paths a user can choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnboardingPath {
    #[default]
    CreateNew,
    Restore,
    Import,
}

/// Focusable elements within the RecoveryDisplay step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecoveryFocus {
    #[default]
    CopyButton,
    RegenerateButton,
    ConfirmCheckbox,
}

/// Steps within each onboarding path.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum OnboardingStep {
    /// Initial choice screen — pick a path.
    #[default]
    Welcome,
    /// Show 24 recovery words (read-only 4x6 grid).
    RecoveryDisplay,
    /// Verify 4 random positions from the recovery words.
    RecoveryVerify { positions: [usize; 4] },
    /// Input recovery key for restore (delegates to WordGridState).
    RecoveryInput,
    /// Post-restore security advisory.
    SecurityAdvisory,
    /// Choose import source.
    ImportSource,
    /// Preview import data.
    ImportPreview,
    /// Set master password (inline — navigates to SetNewMasterPassword on enter).
    SetPassword,
}
