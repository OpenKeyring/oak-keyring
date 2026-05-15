//! Config screen state for U8.
//!
//! UI-layer state that maps to/from [`crate::config::AppConfig`].
//! Uses the real config types directly rather than String-based approximations.

use crate::config::*;

/// Focusable buttons in the config footer bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FooterButton {
    ExitProgram,
    Close,
}

/// Google Drive OAuth2 authorization status.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum GDriveAuthStatus {
    #[default]
    NotAuthorized,
    Authorizing,
    Authorized,
    Failed {
        reason: String,
    },
}

/// Scroll boundary flash duration in milliseconds.
pub const BOUNDARY_FLASH_DURATION_MS: u64 = 150;

// ── Config Overlay ────────────────────────────────────────────────────────────

/// Overlay state for the config screen (dropdowns, dialogs).
#[derive(Debug, Clone)]
pub enum ConfigOverlay {
    /// Dropdown selection overlay.
    Dropdown {
        field: DropdownField,
        options: Vec<String>,
        selected: usize,
    },
    /// Unsaved changes confirmation.
    UnsavedChanges { focused_button: ConfirmButton },
}

/// Buttons in the unsaved-changes confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmButton {
    Cancel,
    Confirm,
}

impl ConfirmButton {
    pub fn toggle(self) -> Self {
        match self {
            ConfirmButton::Cancel => ConfirmButton::Confirm,
            ConfirmButton::Confirm => ConfirmButton::Cancel,
        }
    }
}

/// Identifies which config field a dropdown is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropdownField {
    // General tab
    Language,
    AutoLock,
    ClipboardClear,
    TrashRetention,
    Animation,
    // Sync tab
    SyncProvider,
    SyncMode,
    SyncInterval,
    // Security tab
    HealthFrequency,
    AuditRetention,
}

impl DropdownField {
    /// Available options for this dropdown.
    pub fn options(self) -> Vec<String> {
        match self {
            DropdownField::Language => vec!["auto".into(), "zh-CN".into(), "en".into()],
            DropdownField::AutoLock => {
                vec![
                    "60".into(),
                    "300".into(),
                    "600".into(),
                    "1800".into(),
                    "0".into(),
                ]
            }
            DropdownField::ClipboardClear => {
                vec!["10".into(), "30".into(), "60".into(), "0".into()]
            }
            DropdownField::TrashRetention => {
                vec!["7".into(), "30".into(), "90".into(), "365".into()]
            }
            DropdownField::Animation => vec!["auto".into(), "on".into(), "off".into()],
            // Only expose providers with verified end-to-end closed loops.
            // Hidden providers retain their enum variants, adapter code, and
            // config deserialization — add them back here when closed-loop verified.
            DropdownField::SyncProvider => vec!["Disabled".into(), "GoogleDrive".into()],
            DropdownField::SyncMode => vec!["Auto".into(), "Manual".into()],
            DropdownField::SyncInterval => {
                vec![
                    "60".into(),
                    "300".into(),
                    "600".into(),
                    "1800".into(),
                    "3600".into(),
                ]
            }
            DropdownField::HealthFrequency => {
                vec!["OnStartup".into(), "Daily".into(), "Weekly".into()]
            }
            DropdownField::AuditRetention => {
                vec!["30".into(), "90".into(), "180".into(), "365".into()]
            }
        }
    }

    /// Human-readable label for the dropdown (i18n).
    pub fn label(self) -> String {
        match self {
            DropdownField::Language => crate::t!("tui.config.dropdown_language").to_string(),
            DropdownField::AutoLock => crate::t!("tui.config.dropdown_auto_lock").to_string(),
            DropdownField::ClipboardClear => {
                crate::t!("tui.config.dropdown_clipboard_clear").to_string()
            }
            DropdownField::TrashRetention => {
                crate::t!("tui.config.dropdown_trash_retention").to_string()
            }
            DropdownField::Animation => crate::t!("tui.config.dropdown_animation").to_string(),
            DropdownField::SyncProvider => {
                crate::t!("tui.config.dropdown_sync_provider").to_string()
            }
            DropdownField::SyncMode => crate::t!("tui.config.dropdown_sync_mode").to_string(),
            DropdownField::SyncInterval => {
                crate::t!("tui.config.dropdown_sync_interval").to_string()
            }
            DropdownField::HealthFrequency => {
                crate::t!("tui.config.dropdown_health_frequency").to_string()
            }
            DropdownField::AuditRetention => {
                crate::t!("tui.config.dropdown_audit_retention").to_string()
            }
        }
    }

    /// Translated display labels for each option in the dropdown.
    /// Returns labels in the same order as `options()`.
    pub fn display_labels(self) -> Vec<String> {
        match self {
            DropdownField::Language => vec![
                crate::t!("tui.config.opt_language_auto").to_string(),
                crate::t!("tui.config.opt_language_zh_cn").to_string(),
                crate::t!("tui.config.opt_language_en").to_string(),
            ],
            DropdownField::AutoLock => vec![
                crate::t!("tui.config.opt_auto_lock_60").to_string(),
                crate::t!("tui.config.opt_auto_lock_300").to_string(),
                crate::t!("tui.config.opt_auto_lock_600").to_string(),
                crate::t!("tui.config.opt_auto_lock_1800").to_string(),
                crate::t!("tui.config.opt_auto_lock_0").to_string(),
            ],
            DropdownField::ClipboardClear => vec![
                crate::t!("tui.config.opt_clipboard_10").to_string(),
                crate::t!("tui.config.opt_clipboard_30").to_string(),
                crate::t!("tui.config.opt_clipboard_60").to_string(),
                crate::t!("tui.config.opt_clipboard_0").to_string(),
            ],
            DropdownField::TrashRetention => vec![
                crate::t!("tui.config.opt_trash_7").to_string(),
                crate::t!("tui.config.opt_trash_30").to_string(),
                crate::t!("tui.config.opt_trash_90").to_string(),
                crate::t!("tui.config.opt_trash_365").to_string(),
            ],
            DropdownField::Animation => vec![
                crate::t!("tui.config.opt_animation_auto").to_string(),
                crate::t!("tui.config.opt_animation_on").to_string(),
                crate::t!("tui.config.opt_animation_off").to_string(),
            ],
            DropdownField::SyncProvider => vec![
                crate::t!("tui.config.opt_provider_disabled").to_string(),
                crate::t!("tui.config.opt_provider_google_drive").to_string(),
            ],
            DropdownField::SyncMode => vec![
                crate::t!("tui.config.opt_sync_mode_auto").to_string(),
                crate::t!("tui.config.opt_sync_mode_manual").to_string(),
            ],
            DropdownField::SyncInterval => vec![
                crate::t!("tui.config.opt_sync_interval_60").to_string(),
                crate::t!("tui.config.opt_sync_interval_300").to_string(),
                crate::t!("tui.config.opt_sync_interval_600").to_string(),
                crate::t!("tui.config.opt_sync_interval_1800").to_string(),
                crate::t!("tui.config.opt_sync_interval_3600").to_string(),
            ],
            DropdownField::HealthFrequency => vec![
                crate::t!("tui.config.opt_freq_on_startup").to_string(),
                crate::t!("tui.config.opt_freq_daily").to_string(),
                crate::t!("tui.config.opt_freq_weekly").to_string(),
            ],
            DropdownField::AuditRetention => vec![
                crate::t!("tui.config.opt_audit_30").to_string(),
                crate::t!("tui.config.opt_audit_90").to_string(),
                crate::t!("tui.config.opt_audit_180").to_string(),
                crate::t!("tui.config.opt_audit_365").to_string(),
            ],
        }
    }
}

// ── Config Tab ────────────────────────────────────────────────────────────────

/// Top-level tab in the config screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfigTab {
    #[default]
    General,
    Sync,
    Security,
    Password,
    About,
}

impl ConfigTab {
    /// All tabs in display order.
    pub fn all() -> &'static [ConfigTab] {
        &[
            ConfigTab::General,
            ConfigTab::Sync,
            ConfigTab::Security,
            ConfigTab::Password,
            ConfigTab::About,
        ]
    }

    /// Zero-based index of this tab.
    pub fn index(self) -> usize {
        match self {
            Self::General => 0,
            Self::Sync => 1,
            Self::Security => 2,
            Self::Password => 3,
            Self::About => 4,
        }
    }

    /// Returns the number of focusable items in this tab.
    pub fn item_count(self) -> usize {
        match self {
            Self::General => 7, // language, auto_lock, clipboard, trash, animation, import, export
            Self::Sync => 5,    // provider, sync_mode, interval, auth_button, test_button
            Self::Security => 5, // health_check, frequency, master_password, audit, retention
            Self::Password => 4, // length, digits, uppercase, special
            Self::About => 0,   // read-only, no focusable items
        }
    }

    /// Clamp a focused index to valid range for this tab.
    pub fn clamp_item(self, focused: usize) -> usize {
        let count = self.item_count();
        if count == 0 {
            return 0;
        }
        focused.min(count - 1)
    }
}

// ── General Config Form ──────────────────────────────────────────────────────

/// General config form state — mirrors [`GeneralConfig`] with UI-friendly fields.
#[derive(Debug, Clone)]
pub struct GeneralConfigForm {
    pub auto_lock_seconds: u64,
    pub clipboard_clear_seconds: u64,
    pub trash_retention_days: u32,
    pub animation: AnimationMode,
    pub language: String,
}

impl Default for GeneralConfigForm {
    fn default() -> Self {
        Self::from(GeneralConfig::default())
    }
}

impl From<GeneralConfig> for GeneralConfigForm {
    fn from(config: GeneralConfig) -> Self {
        Self {
            auto_lock_seconds: config.auto_lock_seconds,
            clipboard_clear_seconds: config.clipboard_clear_seconds,
            trash_retention_days: config.trash_retention_days,
            animation: config.animation,
            language: config.language,
        }
    }
}

impl From<&GeneralConfigForm> for GeneralConfig {
    fn from(form: &GeneralConfigForm) -> Self {
        Self {
            auto_lock_seconds: form.auto_lock_seconds,
            clipboard_clear_seconds: form.clipboard_clear_seconds,
            trash_retention_days: form.trash_retention_days,
            animation: form.animation,
            language: form.language.clone(),
        }
    }
}

// ── Sync Config Form ─────────────────────────────────────────────────────────

/// Sync config form state — mirrors [`SyncConfig`].
#[derive(Debug, Clone)]
pub struct SyncConfigForm {
    pub provider: SyncProvider,
    pub sync_mode: SyncMode,
    pub auto_interval_seconds: u64,
    pub provider_config: Option<ProviderConfig>,
}

impl Default for SyncConfigForm {
    fn default() -> Self {
        Self::from(SyncConfig::default())
    }
}

impl From<SyncConfig> for SyncConfigForm {
    fn from(config: SyncConfig) -> Self {
        Self {
            provider: config.provider,
            sync_mode: config.sync_mode,
            auto_interval_seconds: config.auto_interval_seconds,
            provider_config: config.provider_config,
        }
    }
}

impl From<&SyncConfigForm> for SyncConfig {
    fn from(form: &SyncConfigForm) -> Self {
        Self {
            provider: form.provider,
            sync_mode: form.sync_mode,
            auto_interval_seconds: form.auto_interval_seconds,
            provider_config: form.provider_config.clone(),
        }
    }
}

// ── Security Config Form ─────────────────────────────────────────────────────

/// Security config form state — mirrors [`SecurityConfig`].
/// Note: rotation config is not editable from the UI, so it is not included.
#[derive(Debug, Clone)]
pub struct SecurityConfigForm {
    pub health_check_enabled: bool,
    pub health_check_frequency: HealthCheckFrequency,
    pub audit_enabled: bool,
    pub audit_retention_days: u32,
}

impl Default for SecurityConfigForm {
    fn default() -> Self {
        Self::from(SecurityConfig::default())
    }
}

impl From<SecurityConfig> for SecurityConfigForm {
    fn from(config: SecurityConfig) -> Self {
        Self {
            health_check_enabled: config.health_check_enabled,
            health_check_frequency: config.health_check_frequency,
            audit_enabled: config.audit_enabled,
            audit_retention_days: config.audit_retention_days,
        }
    }
}

impl From<&SecurityConfigForm> for SecurityConfig {
    fn from(form: &SecurityConfigForm) -> Self {
        Self {
            health_check_enabled: form.health_check_enabled,
            health_check_frequency: form.health_check_frequency,
            audit_enabled: form.audit_enabled,
            audit_retention_days: form.audit_retention_days,
            rotation: crate::types::rotation::RotationConfig::default(),
        }
    }
}

// ── Password Defaults Form ───────────────────────────────────────────────────

/// Password defaults form state — mirrors [`PasswordDefaultsConfig`].
#[derive(Debug, Clone)]
pub struct PasswordDefaultsForm {
    pub length: usize,
    pub include_digits: bool,
    pub include_uppercase: bool,
    pub include_special: bool,
}

impl Default for PasswordDefaultsForm {
    fn default() -> Self {
        Self::from(PasswordDefaultsConfig::default())
    }
}

impl From<PasswordDefaultsConfig> for PasswordDefaultsForm {
    fn from(config: PasswordDefaultsConfig) -> Self {
        Self {
            length: config.length,
            include_digits: config.include_digits,
            include_uppercase: config.include_uppercase,
            include_special: config.include_special,
        }
    }
}

impl From<&PasswordDefaultsForm> for PasswordDefaultsConfig {
    fn from(form: &PasswordDefaultsForm) -> Self {
        Self {
            length: form.length,
            include_digits: form.include_digits,
            include_uppercase: form.include_uppercase,
            include_special: form.include_special,
        }
    }
}

// ── About Info ───────────────────────────────────────────────────────────────

/// Static about information (version, author, license).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutInfo {
    pub version: &'static str,
    pub author: &'static str,
    pub license: &'static str,
}

impl Default for AboutInfo {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            author: "OpenKeyring Contributors",
            license: "MIT OR Apache-2.0",
        }
    }
}

// ── Sync Connection Status ───────────────────────────────────────────────────

/// Connection status for the sync provider (UI indicator only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncConnectionStatus {
    #[default]
    NotConfigured,
    Connected,
    Disconnected,
    Testing,
}

// ── Config Screen State ──────────────────────────────────────────────────────

/// Root state for the config screen (U8).
#[derive(Debug, Default)]
pub struct ConfigScreenState {
    /// Currently active tab.
    pub active_tab: ConfigTab,
    /// Vertical scroll offset within the current tab's content area.
    pub scroll_offset: u16,
    /// Index of the currently focused item within the current tab.
    pub focused_item: usize,
    /// Whether any form field has been modified since load.
    pub has_changes: bool,
    /// General config form.
    pub general: GeneralConfigForm,
    /// Sync config form.
    pub sync: SyncConfigForm,
    /// Security config form.
    pub security: SecurityConfigForm,
    /// Password defaults form.
    pub password: PasswordDefaultsForm,
    /// About info (static).
    pub about: AboutInfo,
    /// Sync provider connection status (UI indicator).
    pub sync_status: SyncConnectionStatus,
    /// Error message from the last failed sync connection test.
    /// Displayed inline alongside the Disconnected status indicator.
    pub sync_error_message: Option<String>,
    /// Active overlay (dropdown or dialog), if any.
    pub overlay: Option<ConfigOverlay>,
    /// Pending mode for ImportExport screen navigation.
    /// Set by config screen before navigating, consumed by routing layer.
    pub pending_import_export_mode: Option<crate::tui::screens::import_export::ImportExportMode>,
    /// Last known terminal height for scroll calculations.
    /// Updated from AppState.terminal_size before each update() call.
    pub terminal_height: u16,
    /// Google Drive OAuth2 authorization status.
    pub gdrive_auth_status: GDriveAuthStatus,
    /// Timestamp of the last successful sync (mirrored from SharedState).
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
    /// Sub-item focus index within the focused row (None = whole row focused)
    pub sub_item_focus: Option<usize>,
    /// Whether the password length field is in edit mode (using LengthSlider)
    pub editing_length: bool,
    /// Original password length before editing (for Esc cancel)
    pub editing_length_original: usize,
    /// One-shot marker set when navigation state was restored from a screen snapshot.
    pub restored_from_snapshot: bool,
    /// Instant when the scroll boundary flash started, None = no flash active.
    pub boundary_flash_at: Option<std::time::Instant>,
    /// Focus state for the footer buttons. None = content area has focus.
    pub footer_focus: Option<FooterButton>,
}

impl ConfigScreenState {
    /// Create a new state with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Populate state from an [`AppConfig`].
    pub fn load_from_config(&mut self, config: &AppConfig) {
        self.general = GeneralConfigForm::from(config.general.clone());
        self.sync = SyncConfigForm::from(config.sync.clone());
        self.security = SecurityConfigForm::from(config.security.clone());
        self.password = PasswordDefaultsForm::from(config.password.clone());
        self.has_changes = false;
        self.scroll_offset = 0;
        self.focused_item = 0;
        self.sub_item_focus = None;

        // Derive initial sync connection status from provider
        self.sync_status = if config.sync.provider == SyncProvider::Disabled {
            SyncConnectionStatus::NotConfigured
        } else {
            SyncConnectionStatus::Disconnected
        };
        self.sync_error_message = None;
    }

    /// Populate config forms while preserving navigation state restored from history.
    pub fn load_from_config_preserving_restored_navigation(&mut self, config: &AppConfig) {
        if !self.restored_from_snapshot {
            self.load_from_config(config);
            return;
        }

        let restore = self.to_restore_state();
        self.load_from_config(config);
        self.restore_from(restore);
        self.restored_from_snapshot = false;
    }

    /// Convert the current form state back into an [`AppConfig`].
    ///
    /// Fields not edited by the config screen (e.g. `rotation`) use their
    /// defaults.
    pub fn to_app_config(&self) -> AppConfig {
        AppConfig {
            general: GeneralConfig::from(&self.general),
            sync: SyncConfig::from(&self.sync),
            security: SecurityConfig::from(&self.security),
            password: PasswordDefaultsConfig::from(&self.password),
        }
    }

    /// Switch to a different tab, resetting scroll and focus.
    pub fn switch_tab(&mut self, tab: ConfigTab) {
        if self.active_tab != tab {
            self.active_tab = tab;
            self.scroll_offset = 0;
            self.focused_item = 0;
            self.sub_item_focus = None;
        }
    }

    /// Move focus to the next item in the current tab.
    /// Returns true if at the bottom boundary (focused was last item).
    pub fn focus_next(&mut self, total_items: usize) -> bool {
        if total_items == 0 {
            return false;
        }
        self.sub_item_focus = None;
        let at_boundary = self.focused_item == total_items - 1;
        self.focused_item = (self.focused_item + 1) % total_items;
        at_boundary
    }

    /// Move focus to the previous item in the current tab.
    /// Returns true if at the top boundary (focused was 0).
    pub fn focus_prev(&mut self, total_items: usize) -> bool {
        if total_items == 0 {
            return false;
        }
        self.sub_item_focus = None;
        let at_boundary = self.focused_item == 0;
        self.focused_item = (self.focused_item + total_items - 1) % total_items;
        at_boundary
    }

    /// Mark that a change has been made.
    pub fn mark_changed(&mut self) {
        self.has_changes = true;
    }

    /// Returns true if the boundary flash is currently active.
    pub fn is_boundary_flash_active(&self) -> bool {
        self.boundary_flash_at.is_some_and(|instant| {
            instant.elapsed().as_millis() < BOUNDARY_FLASH_DURATION_MS as u128
        })
    }

    /// Reset changes flag (e.g. after save).
    pub fn clear_changes(&mut self) {
        self.has_changes = false;
    }

    /// Page up by half the visible height.
    pub fn scroll_page_up(&mut self, visible_height: u16) {
        let delta = (visible_height / 2).max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(delta);
    }

    /// Page down by half the visible height.
    pub fn scroll_page_down(&mut self, visible_height: u16, total_height: u16) {
        let delta = (visible_height / 2).max(1);
        let max_offset = total_height.saturating_sub(visible_height);
        self.scroll_offset = (self.scroll_offset + delta).min(max_offset);
    }

    /// Ensure focused item is visible by adjusting scroll_offset.
    /// Each item is 1 row tall; +1 for the title row in each tab.
    pub fn ensure_focused_visible(&mut self, focused: usize, visible_height: u16) {
        let visible_height = visible_height as usize;
        if visible_height == 0 {
            return;
        }
        // Item row index (accounting for title row at offset 0)
        let item_row = focused + 1;
        // If focused item is above the visible window
        if item_row < self.scroll_offset as usize {
            self.scroll_offset = item_row as u16;
        }
        // If focused item is below the visible window
        else if item_row >= self.scroll_offset as usize + visible_height {
            self.scroll_offset = (item_row - visible_height + 1) as u16;
        }
    }

    /// Capture the config screen state for later restoration.
    pub fn to_restore_state(&self) -> crate::tui::state::ConfigRestoreState {
        crate::tui::state::ConfigRestoreState {
            active_tab: self.active_tab,
            focused_item: self.focused_item,
            sub_item_focus: self.sub_item_focus,
            scroll_offset: self.scroll_offset,
        }
    }

    /// Restore config screen state from a snapshot, with bounds clamping.
    pub fn restore_from(&mut self, restore: crate::tui::state::ConfigRestoreState) {
        self.active_tab = restore.active_tab;
        let item_count = self.active_tab.item_count();
        self.focused_item = if item_count == 0 {
            0
        } else {
            restore.focused_item.min(item_count - 1)
        };
        self.sub_item_focus = if self.active_tab == ConfigTab::Security && self.focused_item == 3 {
            restore.sub_item_focus.filter(|idx| *idx <= 1)
        } else {
            None
        };
        self.scroll_offset = restore.scroll_offset;
        self.restored_from_snapshot = true;
    }
}
