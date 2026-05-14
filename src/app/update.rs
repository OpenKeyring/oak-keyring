//! TEA (The Elm Architecture) event loop — the heart of the application.
//!
//! Core 4-step loop:
//! 1. Drain executor result channel (non-blocking)
//! 2. Poll terminal event with ~50ms timeout
//! 3. Process timers (Tick when no terminal event)
//! 4. Render current frame

use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::signal::SignalHandler;
use crate::app::view;
use crate::app::App;
use crate::commands::types::{AppPhase, Screen};
use crate::commands::Message;
use crate::tui::traits::screen::{Screen as ScreenTrait, ScreenContext, ScreenResult};

/// Tick rate: how often we check for terminal events. Also drives timers/animations.
const TICK_RATE: Duration = Duration::from_millis(50);

fn start_screen_in_transition(state: &mut crate::tui::state::AppState) {
    crate::tui::animation::transitions::start_transition(
        &mut state.shared.animation,
        crate::tui::state::animation::EffectKind::ScreenIn,
    );
}

pub fn run(
    app: &mut App,
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Spawn signal handler.
    let _signal_handler = SignalHandler::spawn(app.result_tx.clone());

    // Initial render.
    terminal.draw(|f| view::render(f, app))?;

    // Main TEA loop.
    loop {
        // Step 1: Drain all pending results from the executor (non-blocking).
        while let Ok(msg) = app.result_rx.try_recv() {
            if handle_message(app, msg)? == LoopControl::Exit {
                return Ok(());
            }
        }

        // Step 2: Poll for terminal events with timeout.
        let has_event = event::poll(TICK_RATE)?;

        if has_event {
            match event::read()? {
                CrosstermEvent::Key(key_event) if key_event.kind == KeyEventKind::Press => {
                    // Ctrl+X: dismiss active notification.
                    if key_event.modifiers.contains(KeyModifiers::CONTROL)
                        && key_event.code == KeyCode::Char('x')
                    {
                        app.state.shared.notification.clear();
                    } else if handle_message(app, Message::KeyEvent(key_event))?
                        == LoopControl::Exit
                    {
                        return Ok(());
                    }
                }
                CrosstermEvent::Resize(width, height)
                    if handle_message(app, Message::Resize { width, height })?
                        == LoopControl::Exit =>
                {
                    return Ok(());
                }
                // Ignore mouse events and other crossterm events for now.
                _ => {}
            }
        } else {
            // Step 3: No terminal event — send Tick for animations/timers.
            if handle_message(app, Message::Tick)? == LoopControl::Exit {
                return Ok(());
            }
        }

        // Step 4: Render.
        terminal.draw(|f| view::render(f, app))?;
    }
}

/// Loop control flow returned by message handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopControl {
    Continue,
    Exit,
}

/// Dispatch a single Message. Returns Exit if the app should shut down.
fn handle_message(
    app: &mut App,
    msg: Message,
) -> Result<LoopControl, Box<dyn std::error::Error + Send + Sync>> {
    match &msg {
        // -- Shutdown handling (direct) ----
        Message::ShutdownRequested { force } => {
            app.phase = AppPhase::ShuttingDown;
            app.cancel_token.cancel();
            if *force {
                tracing::warn!("forced shutdown requested");
            } else {
                tracing::info!("graceful shutdown initiated");
            }
            return Ok(LoopControl::Exit);
        }

        // -- Navigation (direct) -----------
        Message::NavigateTo(screen) => {
            let screen = *screen;
            let command_tx = app.command_tx.clone();
            let mut ctx = ScreenContext {
                command_tx: &command_tx,
                config: &app.config,
            };
            // Keep screen lifecycle ordering explicit: unmount old screen, switch route, mount new screen.
            stage_recovery_words_for_navigation(&mut app.state, screen);
            route_on_unmount_from_state(&mut app.state);
            app.state.navigate_to(screen);
            route_on_mount_from_state(&mut app.state, &mut ctx);
            start_screen_in_transition(&mut app.state);
        }

        Message::GoBack => {
            route_on_unmount_from_state(&mut app.state);
            if !app.state.go_back() {
                // No previous screen remains; treat back as app exit.
                app.phase = AppPhase::ShuttingDown;
                app.cancel_token.cancel();
                return Ok(LoopControl::Exit);
            }
            let command_tx = app.command_tx.clone();
            let mut ctx = ScreenContext {
                command_tx: &command_tx,
                config: &app.config,
            };
            route_on_mount_from_state(&mut app.state, &mut ctx);
            start_screen_in_transition(&mut app.state);
        }

        // -- Tick (direct) ------------------
        Message::Tick => {
            // Tick notification state for auto-dismiss.
            app.state.shared.notification.tick();
            app.state.shared.animation.clear_finished();
        }

        // -- Resize (direct) ----------------
        Message::Resize { width, height } => {
            let w = *width;
            let h = *height;
            app.state.update_size(w, h);
        }

        // -- Command results: global handling + screen routing ----
        Message::CommandCompleted(ref result) => {
            use crate::commands::result::CommandResult;
            use crate::tui::state::notification::StatusMessage;

            match result {
                CommandResult::ConfigSaved { warnings } => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::success("Configuration saved".into()));
                    for w in warnings {
                        app.state
                            .shared
                            .notification
                            .enqueue(StatusMessage::warning(w.clone()));
                    }
                }
                CommandResult::ConfigLoaded { config } => {
                    // Language change
                    if config.general.language != app.config.general.language {
                        crate::tui::i18n::switch_locale(&config.general.language);
                    }
                    // Animation mode change
                    app.state.shared.animation.level = match config.general.animation {
                        crate::config::general::AnimationMode::On => {
                            crate::tui::animation::AnimationLevel::Full
                        }
                        crate::config::general::AnimationMode::Off => {
                            crate::tui::animation::AnimationLevel::None
                        }
                        crate::config::general::AnimationMode::Auto => {
                            crate::tui::animation::detect_animation_level()
                        }
                    };
                    app.config = config.clone();
                }
                CommandResult::SyncConnectionTested { success, message } => {
                    let msg = if *success {
                        StatusMessage::success(message.clone())
                    } else {
                        StatusMessage::error(message.clone())
                    };
                    app.state.shared.notification.enqueue(msg);
                }
                CommandResult::SyncCompleted { .. } => {
                    let now = chrono::Utc::now();
                    app.state.shared.last_sync = Some(now);
                    app.state.screens.config.state.last_sync = Some(now);
                    if let Screen::Main = app.state.current_screen {
                        app.state.screens.main.status_bar.sync_status =
                            crate::tui::state::main_state::SyncIndicator::Synced;
                    }
                }
                CommandResult::Error { code, fallback, .. } if code.module_prefix() == "sync" => {
                    if let Screen::Main = app.state.current_screen {
                        app.state.screens.main.status_bar.sync_status =
                            crate::tui::state::main_state::SyncIndicator::Failed;
                    }
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::error(fallback.clone()));
                }
                CommandResult::Error { fallback, .. } => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::error(fallback.clone()));
                }
                CommandResult::FatalError { fallback, .. } => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::error(fallback.clone()));
                }
                CommandResult::ExportCompleted { record_count, .. } => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::success(format!(
                            "Export completed: {} records saved",
                            record_count
                        )));
                }
                CommandResult::ImportCompleted {
                    imported_count,
                    skipped_count,
                    failed_count,
                    ..
                } => {
                    let mut parts = vec![format!(
                        "Import completed: {} records imported",
                        imported_count
                    )];
                    if *skipped_count > 0 {
                        parts.push(format!("{} skipped", skipped_count));
                    }
                    if *failed_count > 0 {
                        parts.push(format!("{} failed", failed_count));
                    }
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::success(parts.join(", ")));
                }
                CommandResult::Cancelled { operation, .. } => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::warning(format!("{} cancelled", operation)));
                }
                CommandResult::MasterPasswordChanged => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::success("Master password updated".into()));
                }
                _ => {} // Screen-specific results handled below
            }
            // Also route to current screen for screen-specific result handling.
            let command_tx = app.command_tx.clone();
            let mut ctx = ScreenContext {
                command_tx: &command_tx,
                config: &app.config,
            };
            let result = route_to_screen(&mut app.state, msg, &mut ctx);
            match result {
                ScreenResult::Continue => {}
                ScreenResult::NavigateTo(screen) => {
                    stage_recovery_words_for_navigation(&mut app.state, screen);
                    route_on_unmount_from_state(&mut app.state);
                    app.state.navigate_to(screen);
                    let mut ctx = ScreenContext {
                        command_tx: &app.command_tx,
                        config: &app.config,
                    };
                    route_on_mount_from_state(&mut app.state, &mut ctx);
                    start_screen_in_transition(&mut app.state);
                }
                ScreenResult::PopScreen => {
                    route_on_unmount_from_state(&mut app.state);
                    app.state.go_back();
                    let command_tx = app.command_tx.clone();
                    let mut ctx = ScreenContext {
                        command_tx: &command_tx,
                        config: &app.config,
                    };
                    route_on_mount_from_state(&mut app.state, &mut ctx);
                    start_screen_in_transition(&mut app.state);
                }
                ScreenResult::Command(cmd) => {
                    let _ = app.command_tx.try_send(*cmd);
                }
                ScreenResult::ExitApp => {
                    app.phase = AppPhase::ShuttingDown;
                    app.cancel_token.cancel();
                    return Ok(LoopControl::Exit);
                }
            }
        }

        // -- All other messages: route to current screen.
        _ => {
            let command_tx = app.command_tx.clone();
            let mut ctx = ScreenContext {
                command_tx: &command_tx,
                config: &app.config,
            };
            let result = route_to_screen(&mut app.state, msg, &mut ctx);
            match result {
                ScreenResult::Continue => {}
                ScreenResult::NavigateTo(screen) => {
                    stage_recovery_words_for_navigation(&mut app.state, screen);
                    route_on_unmount_from_state(&mut app.state);
                    app.state.navigate_to(screen);
                    let mut ctx = ScreenContext {
                        command_tx: &app.command_tx,
                        config: &app.config,
                    };
                    route_on_mount_from_state(&mut app.state, &mut ctx);
                    start_screen_in_transition(&mut app.state);
                }
                ScreenResult::PopScreen => {
                    route_on_unmount_from_state(&mut app.state);
                    app.state.go_back();
                    let command_tx = app.command_tx.clone();
                    let mut ctx = ScreenContext {
                        command_tx: &command_tx,
                        config: &app.config,
                    };
                    route_on_mount_from_state(&mut app.state, &mut ctx);
                    start_screen_in_transition(&mut app.state);
                }
                ScreenResult::Command(cmd) => {
                    let _ = app.command_tx.try_send(*cmd);
                }
                ScreenResult::ExitApp => {
                    app.phase = AppPhase::ShuttingDown;
                    app.cancel_token.cancel();
                    return Ok(LoopControl::Exit);
                }
            }
        }
    }

    // If phase was externally set to ShuttingDown, exit.
    if app.phase == AppPhase::ShuttingDown {
        return Ok(LoopControl::Exit);
    }

    Ok(LoopControl::Continue)
}

/// Route a message to the current screen's `update()` method.
fn route_to_screen(
    state: &mut crate::tui::state::AppState,
    msg: Message,
    ctx: &mut ScreenContext<'_>,
) -> ScreenResult {
    match state.current_screen {
        Screen::Main => {
            state
                .screens
                .main
                .sync_from_app(state.shared.focus.focused_panel, state.unicode_capable);
            let result = state.screens.main.update(msg, ctx);
            // Back-sync focus changes from MainScreen to AppState
            if state.shared.focus.focused_panel != state.screens.main.focused_panel {
                state.shared.focus.focused_panel = state.screens.main.focused_panel;
            }
            // Process pending overlay animation (ModalAppear/ModalDismiss)
            if let Some(kind) = state.screens.main.pending_animation.take() {
                crate::tui::animation::transitions::start_transition(
                    &mut state.shared.animation,
                    kind,
                );
            }
            result
        }
        Screen::Unlock => state.screens.unlock.update(msg, ctx),
        Screen::Onboarding => state.screens.onboarding.update(msg, ctx),
        Screen::KeyRecovery => state.screens.key_recovery.update(msg, ctx),
        Screen::DatabaseRecovery => state.screens.database_recovery.update(msg, ctx),
        Screen::Config => {
            state.screens.config.state.terminal_height = state.terminal_size.1;
            state.screens.config.update(msg, ctx)
        }
        Screen::ChangeMasterPassword => state.screens.change_master_password.update(msg, ctx),
        Screen::SetNewMasterPassword => state.screens.set_new_master_password.update(msg, ctx),
        Screen::ImportExport => state.screens.import_export.update(msg, ctx),
        Screen::AuditLog => state.screens.audit_log.update(msg, ctx),
        Screen::SyncConflict => state.screens.sync_conflict.update(msg, ctx),
        Screen::PasswordGenerator => state.screens.password_generator.update(msg, ctx),
        Screen::CreateRecord => state.screens.create_record.update(msg, ctx),
        Screen::EditRecord { .. } => state.screens.edit_record.update(msg, ctx),
    }
}

pub(crate) fn stage_recovery_words_for_navigation(
    state: &mut crate::tui::state::AppState,
    target: Screen,
) {
    if state.current_screen == Screen::KeyRecovery && target == Screen::SetNewMasterPassword {
        state.stage_pending_recovery_words(state.screens.key_recovery.words.collect_words());
    }
}

/// Call `on_mount()` on the current screen after navigation.
fn route_on_mount_from_state(state: &mut crate::tui::state::AppState, ctx: &mut ScreenContext<'_>) {
    match state.current_screen {
        Screen::Main => {
            state
                .screens
                .main
                .sync_from_app(state.shared.focus.focused_panel, state.unicode_capable);
            state.screens.main.on_mount(ctx)
        }
        Screen::Unlock => state.screens.unlock.on_mount(ctx),
        Screen::Onboarding => state.screens.onboarding.on_mount(ctx),
        Screen::KeyRecovery => {
            let origin = state
                .screen_history
                .last()
                .map(|s| s.screen)
                .and_then(|s| match s {
                    Screen::Onboarding => Some(
                        crate::tui::screens::key_recovery::KeyRecoveryOrigin::OnboardingRestore,
                    ),
                    _ => None,
                })
                .unwrap_or(crate::tui::screens::key_recovery::KeyRecoveryOrigin::StartupDbOnly);
            state.screens.key_recovery =
                crate::tui::screens::key_recovery::KeyRecoveryScreen::new(origin);
            state.screens.key_recovery.on_mount(ctx)
        }
        Screen::DatabaseRecovery => {
            let origin = state
                .screen_history
                .last()
                .map(|s| s.screen)
                .and_then(|s| match s {
                    Screen::KeyRecovery => Some(crate::tui::screens::database_recovery::DatabaseRecoveryOrigin::OnboardingRestore),
                    _ => None,
                })
                .unwrap_or(crate::tui::screens::database_recovery::DatabaseRecoveryOrigin::StartupKeyOnly);
            state.screens.database_recovery =
                crate::tui::screens::database_recovery::DatabaseRecoveryScreen::new(origin);
            state.screens.database_recovery.on_mount(ctx)
        }
        Screen::Config => state.screens.config.on_mount(ctx),
        Screen::ChangeMasterPassword => state.screens.change_master_password.on_mount(ctx),
        Screen::SetNewMasterPassword => {
            let context = match state.screen_history.last().map(|s| s.screen) {
                Some(Screen::Unlock) => {
                    crate::tui::screens::set_password::SetPasswordContext::PostRecovery
                }
                Some(Screen::KeyRecovery) => {
                    let words = state
                        .take_pending_recovery_words()
                        .unwrap_or_else(|| state.screens.key_recovery.words.collect_words());
                    // Determine next step from origin: startup → validate DB, onboarding → database recovery
                    let is_onboarding = matches!(
                        state.screens.key_recovery.origin,
                        crate::tui::screens::key_recovery::KeyRecoveryOrigin::OnboardingRestore
                    );
                    let next = if is_onboarding {
                        crate::tui::screens::set_password::RestoreNext::RestoreDatabase
                    } else {
                        crate::tui::screens::set_password::RestoreNext::ValidateExistingDatabase
                    };
                    crate::tui::screens::set_password::SetPasswordContext::RestoreExistingVault {
                        recovery_words: words,
                        next,
                    }
                }
                _ => match state.screens.onboarding.selected_path {
                    Some(crate::tui::screens::onboarding::OnboardingPath::Restore) => {
                        crate::tui::screens::set_password::SetPasswordContext::OnboardingRestore
                    }
                    _ => crate::tui::screens::set_password::SetPasswordContext::OnboardingCreate {
                        recovery_words: state.screens.onboarding.recovery_words.clone(),
                    },
                },
            };
            let screen = crate::tui::screens::set_password::SetPasswordScreen::new(context);
            state.screens.set_new_master_password = screen;
            state.screens.set_new_master_password.on_mount(ctx)
        }
        Screen::ImportExport => {
            // Consume pending mode from config screen if set
            if let Some(mode) = state.screens.config.state.pending_import_export_mode.take() {
                state.screens.import_export.mode = mode;
                state.screens.import_export.entry_point =
                    crate::tui::screens::import_export::ImportEntryPoint::ConfigPage;
            }
            // Check if navigating from Onboarding Import path (AC18)
            if matches!(
                state.screen_history.last().map(|s| s.screen),
                Some(Screen::Onboarding)
            ) {
                state.screens.import_export.mode =
                    crate::tui::screens::import_export::ImportExportMode::Import;
                state.screens.import_export.entry_point =
                    crate::tui::screens::import_export::ImportEntryPoint::Onboarding { step: 2 };
            }
            state.screens.import_export.on_mount(ctx)
        }
        Screen::AuditLog => state.screens.audit_log.on_mount(ctx),
        Screen::SyncConflict => state.screens.sync_conflict.on_mount(ctx),
        Screen::PasswordGenerator => state.screens.password_generator.on_mount(ctx),
        Screen::CreateRecord => state.screens.create_record.on_mount(ctx),
        Screen::EditRecord { id } => {
            state.screens.edit_record.record_id = Some(id);
            state.screens.edit_record.on_mount(ctx)
        }
    }
}

/// Call `on_unmount()` on the current screen before navigation.
fn route_on_unmount_from_state(state: &mut crate::tui::state::AppState) {
    match state.current_screen {
        Screen::Main => state.screens.main.on_unmount(),
        Screen::Unlock => state.screens.unlock.on_unmount(),
        Screen::Onboarding => state.screens.onboarding.on_unmount(),
        Screen::KeyRecovery => state.screens.key_recovery.on_unmount(),
        Screen::DatabaseRecovery => state.screens.database_recovery.on_unmount(),
        Screen::Config => state.screens.config.on_unmount(),
        Screen::ChangeMasterPassword => state.screens.change_master_password.on_unmount(),
        Screen::SetNewMasterPassword => {
            // Signal onboarding if returning from SetNewMasterPassword
            if state.screen_history.last().map(|s| s.screen) == Some(Screen::Onboarding) {
                state.screens.onboarding.returning_from_set_password = true;
            }
            state.screens.set_new_master_password.on_unmount()
        }
        Screen::ImportExport => {
            // Signal onboarding if returning from import (AC18)
            if matches!(
                state.screens.import_export.entry_point,
                crate::tui::screens::import_export::ImportEntryPoint::Onboarding { .. }
            ) {
                state.screens.onboarding.returning_from_import = true;
            }
            state.screens.import_export.on_unmount()
        }
        Screen::AuditLog => state.screens.audit_log.on_unmount(),
        Screen::SyncConflict => state.screens.sync_conflict.on_unmount(),
        Screen::PasswordGenerator => state.screens.password_generator.on_unmount(),
        Screen::CreateRecord => state.screens.create_record.on_unmount(),
        Screen::EditRecord { .. } => state.screens.edit_record.on_unmount(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::{AppPhase, PanelId};
    use crate::instance_lock::InstanceLock;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

    fn test_app() -> App {
        let vault_dir = tempfile::tempdir().unwrap();
        let instance_lock = InstanceLock::acquire(vault_dir.path()).unwrap();
        let vault_dir_path = vault_dir.path().to_path_buf();
        let config_dir_path = vault_dir.path().to_path_buf();
        let mut app = App::new(
            crate::config::AppConfig::default(),
            crate::app::VaultInitState {
                has_vault: true,
                vault_has_key_only: false,
                vault_has_db_only: false,
            },
            instance_lock,
            vault_dir_path,
            config_dir_path,
        )
        .expect("app");
        app.phase = AppPhase::Running;
        app
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: crossterm::event::KeyEventState::NONE,
        }
    }

    #[test]
    fn screen_result_navigate_to_starts_screen_in_animation() {
        let mut app = test_app();
        app.state.current_screen = Screen::Main;
        app.state.shared.focus.focused_panel = PanelId::Sidebar;

        let result = handle_message(&mut app, Message::KeyEvent(key(KeyCode::Char('g'))))
            .expect("message handled");

        assert_eq!(result, LoopControl::Continue);
        assert_eq!(app.state.current_screen, Screen::Config);
        assert!(app
            .state
            .shared
            .animation
            .has_active_kind(crate::tui::state::animation::EffectKind::ScreenIn));
    }

    #[test]
    fn key_recovery_words_are_captured_before_unmount_zeroizes_grid() {
        use crate::tui::screens::key_recovery::{KeyRecoveryOrigin, KeyRecoveryScreen};
        use crate::tui::screens::set_password::{RestoreNext, SetPasswordContext};
        use crate::tui::state::AppState;
        use zeroize::Zeroize;

        let original_words: Vec<String> = (0..24).map(|i| format!("word{}", i)).collect();
        let mut state = AppState::new(false, false, true);
        state.current_screen = Screen::KeyRecovery;
        state.screens.key_recovery = KeyRecoveryScreen::new(KeyRecoveryOrigin::StartupDbOnly);
        for (slot, word) in state
            .screens
            .key_recovery
            .words
            .words
            .iter_mut()
            .zip(original_words.iter())
        {
            *slot = word.clone();
        }

        stage_recovery_words_for_navigation(&mut state, Screen::SetNewMasterPassword);
        state.screens.key_recovery.words.zeroize();

        let words = state
            .take_pending_recovery_words()
            .expect("pending recovery words should survive key recovery unmount");
        assert_eq!(words[0], "word0");
        assert_eq!(words[23], "word23");
        assert_eq!(words, original_words);

        let context = SetPasswordContext::RestoreExistingVault {
            recovery_words: words,
            next: RestoreNext::ValidateExistingDatabase,
        };

        assert!(matches!(
            context,
            SetPasswordContext::RestoreExistingVault {
                next: RestoreNext::ValidateExistingDatabase,
                ..
            }
        ));
    }
}
