//! Integration tests for UI entry screens and state management.
//!
//! Covers: App/AppState navigation, notification priority, lockout escalation,
//! recovery key grid, set password screen, onboarding defaults, loading indicators.

use oak_keyring::app::{App, VaultInitState};
use oak_keyring::commands::types::Screen;
use oak_keyring::config::AppConfig;
use oak_keyring::instance_lock::InstanceLock;
use oak_keyring::tui::screens::onboarding::{OnboardingPath, OnboardingScreen};
use oak_keyring::tui::screens::recovery_key::WordGridState;
use oak_keyring::tui::screens::set_password::{SetPasswordContext, SetPasswordScreen};
use oak_keyring::tui::screens::unlock::lockout_duration;
use oak_keyring::tui::state::loading::{ProgressBarState, SpinnerState};
use oak_keyring::tui::state::notification::{NotificationState, StatusMessage};

// ── App and AppState ────────────────────────────────────────────────────────

#[test]
fn app_starts_at_unlock_screen_when_vault_exists() {
    let vault_dir = tempfile::tempdir().unwrap();
    let instance_lock = InstanceLock::acquire(vault_dir.path()).unwrap();
    let vault_dir_path = vault_dir.path().to_path_buf();
    let config_dir_path = vault_dir.path().to_path_buf();
    let app = App::new(
        AppConfig::default(),
        VaultInitState {
            has_vault: true,
            vault_has_key_only: false,
            vault_has_db_only: false,
        },
        instance_lock,
        vault_dir_path,
        config_dir_path,
    )
    .expect("App::new should succeed");
    assert_eq!(app.state.current_screen, Screen::Unlock);
}

#[test]
fn app_starts_at_onboarding_screen_when_no_vault() {
    let vault_dir = tempfile::tempdir().unwrap();
    let instance_lock = InstanceLock::acquire(vault_dir.path()).unwrap();
    let vault_dir_path = vault_dir.path().to_path_buf();
    let config_dir_path = vault_dir.path().to_path_buf();
    let app = App::new(
        AppConfig::default(),
        VaultInitState {
            has_vault: false,
            vault_has_key_only: false,
            vault_has_db_only: false,
        },
        instance_lock,
        vault_dir_path,
        config_dir_path,
    )
    .expect("App::new should succeed");
    assert_eq!(app.state.current_screen, Screen::Onboarding);
}

#[test]
fn app_state_navigate_to_pushes_stack() {
    let mut state = oak_keyring::tui::state::AppState::default();
    assert_eq!(state.current_screen, Screen::Unlock);
    assert!(state.screen_history.is_empty());

    state.navigate_to(Screen::Onboarding);

    assert_eq!(state.current_screen, Screen::Onboarding);
    assert_eq!(state.screen_history.len(), 1);
    assert_eq!(state.screen_history[0].screen, Screen::Unlock);
}

#[test]
fn app_state_go_back_pops_stack() {
    let mut state = oak_keyring::tui::state::AppState::default();
    state.navigate_to(Screen::Onboarding);
    assert_eq!(state.current_screen, Screen::Onboarding);

    let result = state.go_back();

    assert!(result);
    assert_eq!(state.current_screen, Screen::Unlock);
    assert!(state.screen_history.is_empty());
}

#[test]
fn app_state_go_back_at_root_returns_false() {
    let mut state = oak_keyring::tui::state::AppState::default();
    assert_eq!(state.current_screen, Screen::Unlock);
    assert!(state.screen_history.is_empty());

    let result = state.go_back();

    assert!(!result);
    assert_eq!(state.current_screen, Screen::Unlock);
}

#[test]
fn terminal_size_update_sets_too_small() {
    let mut state = oak_keyring::tui::state::AppState::default();
    // Default: 80x24 should not be too small
    assert!(!state.too_small);

    // Below minimum: 60x20
    state.update_size(60, 20);
    assert!(state.too_small);
    assert_eq!(state.terminal_size, (60, 20));

    // Back to valid: 120x30
    state.update_size(120, 30);
    assert!(!state.too_small);
    assert_eq!(state.terminal_size, (120, 30));

    // Width OK but height too small: 80x20
    state.update_size(80, 20);
    assert!(state.too_small);

    // Width too small but height OK: 60x24
    state.update_size(60, 24);
    assert!(state.too_small);
}

// ── Notification priority ───────────────────────────────────────────────────

#[test]
fn notification_priority_queue() {
    let mut ns = NotificationState::default();

    // Enqueue success first
    ns.enqueue(StatusMessage::success("saved".to_string()));
    assert!(ns.current_message.as_ref().unwrap().is_success());

    // Enqueue error — should preempt (error has higher priority)
    ns.enqueue(StatusMessage::error("failed".to_string()));
    let current = ns
        .current_message
        .as_ref()
        .expect("should have a current message");
    assert!(current.is_error());
    assert_eq!(current.text, "failed");

    // The success message should have been moved to pending
    assert!(ns.pending_message.is_some());
    assert!(ns.pending_message.as_ref().unwrap().is_success());
}

// ── Screen component tests ──────────────────────────────────────────────────

#[test]
fn unlock_lockout_escalation() {
    assert_eq!(lockout_duration(0), 0);
    assert_eq!(lockout_duration(4), 0);
    assert_eq!(lockout_duration(5), 30);
    assert_eq!(lockout_duration(7), 300);
    assert_eq!(lockout_duration(99), 900);
    // Additional boundary checks
    assert_eq!(lockout_duration(6), 60);
    assert_eq!(lockout_duration(8), 900);
}

#[test]
fn recovery_key_grid_default() {
    let grid = WordGridState::default();
    assert_eq!(grid.focused_index, 0);
    assert!(!grid.all_filled());
    assert!(grid.words.iter().all(|w| w.is_empty()));
    assert!(grid.errors.iter().all(|&e| !e));
}

#[test]
fn set_password_screen_new() {
    let screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate {
        recovery_words: Vec::new(),
    });
    assert!(screen.new_password.is_empty());
    assert!(screen.confirm_password.is_empty());
    assert!(screen.strength.is_none());
    assert!(screen.error.is_none());
    assert!(!screen.password_visible);
}

#[test]
fn onboarding_welcome_defaults() {
    let screen = OnboardingScreen::default();
    assert!(screen.selected_path.is_none());
    assert!(screen.error.is_none());
    assert!(screen.recovery_words.is_empty());
    assert!(!screen.recovery_confirmed);
}

#[test]
fn loading_spinner_frames() {
    let mut spinner = SpinnerState::new("Loading vault");
    assert_eq!(spinner.label, "Loading vault");
    assert_eq!(spinner.frame_index, 0);

    // Capture the initial frame
    let frame_0 = spinner.frame().to_string();
    assert!(!frame_0.is_empty());

    // Tick should change the frame
    spinner.tick();
    assert_eq!(spinner.frame_index, 1);
    let frame_1 = spinner.frame().to_string();
    assert_ne!(frame_0, frame_1, "tick should advance to a different frame");

    // Tick several more times
    spinner.tick();
    spinner.tick();
    assert_eq!(spinner.frame_index, 3);
}

#[test]
fn progress_bar_calculation() {
    let mut bar = ProgressBarState::new(100, "Importing");
    assert_eq!(bar.label, "Importing");
    assert_eq!(bar.current, 0);
    assert_eq!(bar.total, 100);
    assert_eq!(bar.percentage(), 0);

    bar.current = 50;
    let progress = bar.progress();
    assert!(
        (progress - 0.5).abs() < f64::EPSILON,
        "progress should be 0.5"
    );
    assert_eq!(bar.percentage(), 50);
}

// ── SetNewMasterPassword routing context tests (Issue #8) ───────────────────

/// Simulates the route_on_mount_from_state context detection logic.
/// When navigating from Onboarding → SetNewMasterPassword, the screen should
/// receive the correct SetPasswordContext based on the onboarding path.
#[test]
fn set_password_context_from_onboarding_create_path() {
    let mut state = oak_keyring::tui::state::AppState::default();
    // Simulate: user is on Onboarding screen with CreateNew path selected
    state.navigate_to(Screen::Onboarding);
    state.screens.onboarding.selected_path = Some(OnboardingPath::CreateNew);

    // Navigate to SetNewMasterPassword (simulating route_on_mount logic)
    state.navigate_to(Screen::SetNewMasterPassword);
    // Verify: screen_history has Onboarding, selected_path is CreateNew
    assert_eq!(
        state.screen_history.last().map(|s| s.screen),
        Some(Screen::Onboarding)
    );
    assert_eq!(
        state.screens.onboarding.selected_path,
        Some(OnboardingPath::CreateNew)
    );

    // Apply context detection logic (mirrors route_on_mount_from_state)
    let context = match state.screen_history.last().map(|s| s.screen) {
        Some(Screen::Unlock) => SetPasswordContext::PostRecovery,
        _ => match state.screens.onboarding.selected_path {
            Some(OnboardingPath::Restore) => SetPasswordContext::OnboardingRestore,
            _ => SetPasswordContext::OnboardingCreate {
                recovery_words: Vec::new(),
            },
        },
    };
    assert_eq!(
        context,
        SetPasswordContext::OnboardingCreate {
            recovery_words: Vec::new()
        }
    );
    state.screens.set_new_master_password = SetPasswordScreen::new(context);
    assert_eq!(
        state.screens.set_new_master_password.context,
        SetPasswordContext::OnboardingCreate {
            recovery_words: Vec::new()
        }
    );
}

#[test]
fn set_password_context_from_onboarding_restore_path() {
    let mut state = oak_keyring::tui::state::AppState::default();
    state.navigate_to(Screen::Onboarding);
    state.screens.onboarding.selected_path = Some(OnboardingPath::Restore);

    state.navigate_to(Screen::SetNewMasterPassword);

    let context = match state.screen_history.last().map(|s| s.screen) {
        Some(Screen::Unlock) => SetPasswordContext::PostRecovery,
        _ => match state.screens.onboarding.selected_path {
            Some(OnboardingPath::Restore) => SetPasswordContext::OnboardingRestore,
            _ => SetPasswordContext::OnboardingCreate {
                recovery_words: Vec::new(),
            },
        },
    };
    assert_eq!(context, SetPasswordContext::OnboardingRestore);
}

#[test]
fn set_password_context_from_onboarding_import_path() {
    let mut state = oak_keyring::tui::state::AppState::default();
    state.navigate_to(Screen::Onboarding);
    state.screens.onboarding.selected_path = Some(OnboardingPath::Import);

    state.navigate_to(Screen::SetNewMasterPassword);

    // Import path should use OnboardingCreate context
    let context = match state.screen_history.last().map(|s| s.screen) {
        Some(Screen::Unlock) => SetPasswordContext::PostRecovery,
        _ => match state.screens.onboarding.selected_path {
            Some(OnboardingPath::Restore) => SetPasswordContext::OnboardingRestore,
            _ => SetPasswordContext::OnboardingCreate {
                recovery_words: Vec::new(),
            },
        },
    };
    assert_eq!(
        context,
        SetPasswordContext::OnboardingCreate {
            recovery_words: Vec::new()
        }
    );
}

#[test]
fn set_password_context_from_unlock_post_recovery() {
    let mut state = oak_keyring::tui::state::AppState::default();
    // Simulate: user unlocks via recovery key on Unlock screen
    // screen_history will have Unlock when navigating to SetNewMasterPassword
    state.navigate_to(Screen::SetNewMasterPassword);

    let context = match state.screen_history.last().map(|s| s.screen) {
        Some(Screen::Unlock) => SetPasswordContext::PostRecovery,
        _ => match state.screens.onboarding.selected_path {
            Some(OnboardingPath::Restore) => SetPasswordContext::OnboardingRestore,
            _ => SetPasswordContext::OnboardingCreate {
                recovery_words: Vec::new(),
            },
        },
    };
    assert_eq!(context, SetPasswordContext::PostRecovery);
}

#[test]
fn set_password_screen_in_screen_states() {
    let state = oak_keyring::tui::state::AppState::default();
    // Verify SetNewMasterPassword screen is registered in ScreenStates
    assert_eq!(
        state.screens.set_new_master_password.context,
        SetPasswordContext::OnboardingCreate {
            recovery_words: Vec::new()
        }
    );
}

// ── Partial vault startup routing ───────────────────────────────────────────

#[test]
fn key_only_routes_to_database_recovery() {
    let state = oak_keyring::tui::state::AppState::new(false, true, false);
    assert_eq!(state.current_screen, Screen::DatabaseRecovery);
}

#[test]
fn db_only_routes_to_key_recovery() {
    let state = oak_keyring::tui::state::AppState::new(false, false, true);
    assert_eq!(state.current_screen, Screen::KeyRecovery);
}

#[test]
fn key_only_startup_does_not_create_empty_database() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("oak-keyring");
    let config_dir = temp.path().join("oak-keyring-config");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();

    // Simulate key-only state: keyfile exists but no vault.db
    std::fs::write(data_dir.join("wrapped_secret_key.json"), "{}").unwrap();

    let instance_lock = InstanceLock::acquire(&data_dir).unwrap();
    let app = App::new(
        AppConfig::default(),
        VaultInitState {
            has_vault: false,
            vault_has_key_only: true,
            vault_has_db_only: false,
        },
        instance_lock,
        data_dir.clone(),
        config_dir.clone(),
    )
    .expect("app");

    assert_eq!(app.state.current_screen, Screen::DatabaseRecovery);

    // Build executor to verify no vault.db is created
    let (result_tx, _result_rx) = tokio::sync::mpsc::channel(64);
    let _executor = oak_keyring::executor::CommandExecutor::new(
        AppConfig::default(),
        result_tx,
        tokio_util::sync::CancellationToken::new(),
        data_dir.clone(),
        config_dir,
        true, // vault_has_key_only
    )
    .expect("executor");

    // vault.db must NOT exist on disk
    assert!(
        !data_dir.join("vault.db").exists(),
        "vault.db should not be created in key-only state"
    );
}
