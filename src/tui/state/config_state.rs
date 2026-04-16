//! Config screen state for U8.
//!
//! UI-layer state that maps to/from [`crate::config::AppConfig`].
//! Uses the real config types directly rather than String-based approximations.

use std::path::PathBuf;

use crate::config::*;

// ── Config Tab ────────────────────────────────────────────────────────────────

/// Top-level tab in the config screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigTab {
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
}

impl Default for ConfigTab {
    fn default() -> Self {
        Self::General
    }
}

// ── General Config Form ──────────────────────────────────────────────────────

/// General config form state — mirrors [`GeneralConfig`] with UI-friendly fields.
#[derive(Debug, Clone)]
pub struct GeneralConfigForm {
    pub vault_path: PathBuf,
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
            vault_path: config.vault_path,
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
            vault_path: form.vault_path.clone(),
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
#[derive(Debug)]
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
}

impl Default for ConfigScreenState {
    fn default() -> Self {
        Self {
            active_tab: ConfigTab::default(),
            scroll_offset: 0,
            focused_item: 0,
            has_changes: false,
            general: GeneralConfigForm::default(),
            sync: SyncConfigForm::default(),
            security: SecurityConfigForm::default(),
            password: PasswordDefaultsForm::default(),
            about: AboutInfo::default(),
            sync_status: SyncConnectionStatus::default(),
        }
    }
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

        // Derive initial sync connection status from provider
        self.sync_status = if config.sync.provider == SyncProvider::Disabled {
            SyncConnectionStatus::NotConfigured
        } else {
            SyncConnectionStatus::Disconnected
        };
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
        }
    }

    /// Move focus to the next item in the current tab.
    pub fn focus_next(&mut self, total_items: usize) {
        if total_items == 0 {
            return;
        }
        self.focused_item = (self.focused_item + 1) % total_items;
    }

    /// Move focus to the previous item in the current tab.
    pub fn focus_prev(&mut self, total_items: usize) {
        if total_items == 0 {
            return;
        }
        self.focused_item = (self.focused_item + total_items - 1) % total_items;
    }

    /// Mark that a change has been made.
    pub fn mark_changed(&mut self) {
        self.has_changes = true;
    }

    /// Reset changes flag (e.g. after save).
    pub fn clear_changes(&mut self) {
        self.has_changes = false;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_starts_on_general_tab() {
        let state = ConfigScreenState::default();
        assert_eq!(state.active_tab, ConfigTab::General);
        assert_eq!(state.scroll_offset, 0);
        assert_eq!(state.focused_item, 0);
        assert!(!state.has_changes);
    }

    #[test]
    fn load_from_config_populates_forms() {
        let mut config = AppConfig::default();
        config.general.auto_lock_seconds = 600;
        config.general.language = "zh-CN".to_string();
        config.security.health_check_enabled = false;
        config.password.length = 24;

        let mut state = ConfigScreenState::default();
        state.load_from_config(&config);

        assert_eq!(state.general.auto_lock_seconds, 600);
        assert_eq!(state.general.language, "zh-CN");
        assert!(!state.security.health_check_enabled);
        assert_eq!(state.password.length, 24);
        assert!(!state.has_changes);
    }

    #[test]
    fn load_from_config_resets_scroll_and_focus() {
        let mut state = ConfigScreenState::default();
        state.scroll_offset = 10;
        state.focused_item = 5;
        state.has_changes = true;

        state.load_from_config(&AppConfig::default());

        assert_eq!(state.scroll_offset, 0);
        assert_eq!(state.focused_item, 0);
        assert!(!state.has_changes);
    }

    #[test]
    fn to_app_config_round_trip() {
        let mut state = ConfigScreenState::default();
        state.general.auto_lock_seconds = 120;
        state.general.clipboard_clear_seconds = 15;
        state.security.audit_retention_days = 180;
        state.password.length = 32;
        state.password.include_special = false;

        let config = state.to_app_config();

        assert_eq!(config.general.auto_lock_seconds, 120);
        assert_eq!(config.general.clipboard_clear_seconds, 15);
        assert_eq!(config.security.audit_retention_days, 180);
        assert_eq!(config.password.length, 32);
        assert!(!config.password.include_special);
    }

    #[test]
    fn to_app_config_uses_default_rotation() {
        let state = ConfigScreenState::default();
        let config = state.to_app_config();
        // Rotation config is not editable in the UI, so it should be default
        assert!(config.security.rotation.auto_rotate);
        assert_eq!(config.security.rotation.rotate_after_days, Some(90));
    }

    #[test]
    fn switch_tab_resets_scroll_and_focus() {
        let mut state = ConfigScreenState::default();
        state.scroll_offset = 5;
        state.focused_item = 3;

        state.switch_tab(ConfigTab::Sync);

        assert_eq!(state.active_tab, ConfigTab::Sync);
        assert_eq!(state.scroll_offset, 0);
        assert_eq!(state.focused_item, 0);
    }

    #[test]
    fn switch_tab_same_tab_is_noop() {
        let mut state = ConfigScreenState::default();
        state.scroll_offset = 5;
        state.focused_item = 3;

        state.switch_tab(ConfigTab::General);

        assert_eq!(state.scroll_offset, 5);
        assert_eq!(state.focused_item, 3);
    }

    #[test]
    fn focus_next_cycles() {
        let mut state = ConfigScreenState::default();
        state.focused_item = 4;
        state.focus_next(5);
        assert_eq!(state.focused_item, 0);
    }

    #[test]
    fn focus_prev_cycles() {
        let mut state = ConfigScreenState::default();
        state.focused_item = 0;
        state.focus_prev(5);
        assert_eq!(state.focused_item, 4);
    }

    #[test]
    fn focus_next_empty_is_noop() {
        let mut state = ConfigScreenState::default();
        state.focus_next(0);
        assert_eq!(state.focused_item, 0);
    }

    #[test]
    fn mark_and_clear_changes() {
        let mut state = ConfigScreenState::default();
        assert!(!state.has_changes);
        state.mark_changed();
        assert!(state.has_changes);
        state.clear_changes();
        assert!(!state.has_changes);
    }

    #[test]
    fn config_tab_all_returns_five() {
        assert_eq!(ConfigTab::all().len(), 5);
    }

    #[test]
    fn config_tab_index() {
        assert_eq!(ConfigTab::General.index(), 0);
        assert_eq!(ConfigTab::Sync.index(), 1);
        assert_eq!(ConfigTab::Security.index(), 2);
        assert_eq!(ConfigTab::Password.index(), 3);
        assert_eq!(ConfigTab::About.index(), 4);
    }

    #[test]
    fn sync_status_defaults_to_not_configured() {
        let state = ConfigScreenState::default();
        assert_eq!(state.sync_status, SyncConnectionStatus::NotConfigured);
    }

    #[test]
    fn load_from_config_disabled_sync_status() {
        let mut state = ConfigScreenState::default();
        state.load_from_config(&AppConfig::default());
        assert_eq!(state.sync_status, SyncConnectionStatus::NotConfigured);
    }

    #[test]
    fn load_from_config_enabled_sync_status() {
        let mut config = AppConfig::default();
        config.sync.provider = SyncProvider::S3;
        let mut state = ConfigScreenState::default();
        state.load_from_config(&config);
        assert_eq!(state.sync_status, SyncConnectionStatus::Disconnected);
    }

    #[test]
    fn about_info_default_version() {
        let about = AboutInfo::default();
        // Version should be a non-empty static string from CARGO_PKG_VERSION
        assert!(!about.version.is_empty());
    }

    #[test]
    fn general_form_default_values() {
        let form = GeneralConfigForm::default();
        assert_eq!(form.auto_lock_seconds, 300);
        assert_eq!(form.clipboard_clear_seconds, 30);
        assert_eq!(form.trash_retention_days, 30);
        assert_eq!(form.animation, AnimationMode::Auto);
        assert_eq!(form.language, "auto");
    }

    #[test]
    fn security_form_default_values() {
        let form = SecurityConfigForm::default();
        assert!(form.health_check_enabled);
        assert_eq!(form.health_check_frequency, HealthCheckFrequency::OnStartup);
        assert!(form.audit_enabled);
        assert_eq!(form.audit_retention_days, 365);
    }

    #[test]
    fn password_form_default_values() {
        let form = PasswordDefaultsForm::default();
        assert_eq!(form.length, 16);
        assert!(form.include_digits);
        assert!(form.include_uppercase);
        assert!(form.include_special);
    }
}
