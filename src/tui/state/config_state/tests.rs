use super::*;
use crate::config::sync::SyncProvider;
use crate::config::{AnimationMode, AppConfig, HealthCheckFrequency};

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
    let at_boundary = state.focus_next(5);
    assert_eq!(state.focused_item, 0);
    assert!(at_boundary);
}

#[test]
fn focus_prev_cycles() {
    let mut state = ConfigScreenState::default();
    state.focused_item = 0;
    let at_boundary = state.focus_prev(5);
    assert_eq!(state.focused_item, 4);
    assert!(at_boundary);
}

#[test]
fn focus_next_not_at_boundary() {
    let mut state = ConfigScreenState::default();
    state.focused_item = 2;
    let at_boundary = state.focus_next(5);
    assert_eq!(state.focused_item, 3);
    assert!(!at_boundary);
}

#[test]
fn focus_prev_not_at_boundary() {
    let mut state = ConfigScreenState::default();
    state.focused_item = 3;
    let at_boundary = state.focus_prev(5);
    assert_eq!(state.focused_item, 2);
    assert!(!at_boundary);
}

#[test]
fn focus_next_empty_is_noop() {
    let mut state = ConfigScreenState::default();
    let at_boundary = state.focus_next(0);
    assert_eq!(state.focused_item, 0);
    assert!(!at_boundary);
}

#[test]
fn boundary_flash_active_within_duration() {
    let mut state = ConfigScreenState::default();
    state.boundary_flash_at = Some(std::time::Instant::now());
    assert!(state.is_boundary_flash_active());
}

#[test]
fn boundary_flash_inactive_after_duration() {
    let mut state = ConfigScreenState::default();
    state.boundary_flash_at = Some(
        std::time::Instant::now()
            - std::time::Duration::from_millis(BOUNDARY_FLASH_DURATION_MS + 1),
    );
    assert!(!state.is_boundary_flash_active());
}

#[test]
fn boundary_flash_inactive_when_none() {
    let state = ConfigScreenState::default();
    assert!(!state.is_boundary_flash_active());
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
fn config_tab_item_count() {
    assert_eq!(ConfigTab::General.item_count(), 8);
    assert_eq!(ConfigTab::Sync.item_count(), 5);
    assert_eq!(ConfigTab::Security.item_count(), 5);
    assert_eq!(ConfigTab::Password.item_count(), 4);
    assert_eq!(ConfigTab::About.item_count(), 0);
}

#[test]
fn config_tab_clamp_item() {
    // General has 8 items, clamp to 0..7
    assert_eq!(ConfigTab::General.clamp_item(0), 0);
    assert_eq!(ConfigTab::General.clamp_item(7), 7);
    assert_eq!(ConfigTab::General.clamp_item(100), 7);

    // About has 0 items, always returns 0
    assert_eq!(ConfigTab::About.clamp_item(0), 0);
    assert_eq!(ConfigTab::About.clamp_item(5), 0);
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
    config.sync.provider = SyncProvider::GoogleDrive;
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

#[test]
fn scroll_page_up_clamps_to_zero() {
    let mut state = ConfigScreenState::default();
    state.scroll_offset = 3;
    state.scroll_page_up(10);
    assert_eq!(state.scroll_offset, 0);
}

#[test]
fn scroll_page_down_clamps_to_max() {
    let mut state = ConfigScreenState::default();
    // total_height=20, visible_height=20 => max_offset=0
    state.scroll_page_down(20, 20);
    assert_eq!(state.scroll_offset, 0);
}

#[test]
fn scroll_page_down_advances() {
    let mut state = ConfigScreenState::default();
    // total_height=40, visible_height=20 => max_offset=20
    // delta = 20/2 = 10
    state.scroll_page_down(20, 40);
    assert_eq!(state.scroll_offset, 10);
}

#[test]
fn ensure_focused_visible_adjusts_up() {
    let mut state = ConfigScreenState::default();
    state.scroll_offset = 10;
    // focused=5 => item_row=6, which is < scroll_offset(10)
    state.ensure_focused_visible(5, 10);
    assert_eq!(state.scroll_offset, 6);
}

#[test]
fn ensure_focused_visible_adjusts_down() {
    let mut state = ConfigScreenState::default();
    state.scroll_offset = 5;
    // focused=14 => item_row=15, visible window is [5..15), 15 >= 5+10
    state.ensure_focused_visible(14, 10);
    assert_eq!(state.scroll_offset, 6);
}

#[test]
fn ensure_focused_visible_no_adjust_needed() {
    let mut state = ConfigScreenState::default();
    state.scroll_offset = 5;
    // focused=7 => item_row=8, visible window is [5..15), 8 is inside
    state.ensure_focused_visible(7, 10);
    assert_eq!(state.scroll_offset, 5);
}

#[test]
fn sync_provider_dropdown_shows_only_visible_providers() {
    let options = DropdownField::SyncProvider.options();
    assert_eq!(options.len(), 2);
    assert_eq!(options[0], "Disabled");
    assert_eq!(options[1], "GoogleDrive");
}

#[test]
fn sync_provider_display_labels_matches_options_count() {
    let options = DropdownField::SyncProvider.options();
    let labels = DropdownField::SyncProvider.display_labels();
    assert_eq!(options.len(), labels.len());
}

#[test]
fn config_restore_state_restores_tab_row_sub_item_and_scroll() {
    let mut state = ConfigScreenState::default();
    state.active_tab = ConfigTab::General;
    state.focused_item = 0;
    state.sub_item_focus = None;
    state.scroll_offset = 0;

    state.restore_from(crate::tui::state::ConfigRestoreState {
        active_tab: ConfigTab::Security,
        focused_item: 3,
        sub_item_focus: Some(1),
        scroll_offset: 4,
    });

    assert_eq!(state.active_tab, ConfigTab::Security);
    assert_eq!(state.focused_item, 3);
    assert_eq!(state.sub_item_focus, Some(1));
    assert_eq!(state.scroll_offset, 4);
}

#[test]
fn config_restore_clamps_focused_item_to_active_tab_count() {
    let mut state = ConfigScreenState::default();

    state.restore_from(crate::tui::state::ConfigRestoreState {
        active_tab: ConfigTab::Security,
        focused_item: 99,
        sub_item_focus: Some(1),
        scroll_offset: 2,
    });

    assert_eq!(state.active_tab, ConfigTab::Security);
    assert_eq!(state.focused_item, ConfigTab::Security.item_count() - 1);
    assert_eq!(state.sub_item_focus, None);
    assert_eq!(state.scroll_offset, 2);
}
