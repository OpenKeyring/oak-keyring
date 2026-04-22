//! TEA (The Elm Architecture) event loop — the heart of the application.
//!
//! Core 4-step loop:
//! 1. Drain executor result channel (non-blocking)
//! 2. Poll terminal event with ~50ms timeout
//! 3. Process timers (Tick when no terminal event)
//! 4. Render current frame

use std::time::Duration;

use crossterm::event::{self, Event as CrosstermEvent, KeyEventKind};
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
                    if handle_message(app, Message::KeyEvent(key_event))? == LoopControl::Exit {
                        return Ok(());
                    }
                }
                CrosstermEvent::Resize(width, height) => {
                    if handle_message(app, Message::Resize { width, height })? == LoopControl::Exit
                    {
                        return Ok(());
                    }
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
            // Call on_unmount on the old screen before navigating.
            route_on_unmount_from_state(&mut app.state);
            app.state.navigate_to(screen);
            // Call on_mount for the new screen.
            route_on_mount_from_state(&mut app.state, &mut ctx);
        }

        Message::GoBack => {
            // Call on_unmount on the current screen.
            route_on_unmount_from_state(&mut app.state);
            if !app.state.go_back() {
                // Stack is empty — exit the app.
                app.phase = AppPhase::ShuttingDown;
                app.cancel_token.cancel();
                return Ok(LoopControl::Exit);
            }
        }

        // -- Tick (direct) ------------------
        Message::Tick => {
            // Tick notification state for auto-dismiss.
            app.state.shared.notification.tick();
            // AnimationState clears itself when expired via is_active().
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
                CommandResult::ConfigSaved => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::success("Configuration saved".into()));
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
                CommandResult::ImportCompleted { imported_count, .. } => {
                    app.state
                        .shared
                        .notification
                        .enqueue(StatusMessage::success(format!(
                            "Import completed: {} records imported",
                            imported_count
                        )));
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
                    route_on_unmount_from_state(&mut app.state);
                    app.state.navigate_to(screen);
                    let mut ctx = ScreenContext {
                        command_tx: &app.command_tx,
                        config: &app.config,
                    };
                    route_on_mount_from_state(&mut app.state, &mut ctx);
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
                    route_on_unmount_from_state(&mut app.state);
                    app.state.navigate_to(screen);
                    let mut ctx = ScreenContext {
                        command_tx: &app.command_tx,
                        config: &app.config,
                    };
                    route_on_mount_from_state(&mut app.state, &mut ctx);
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
            state.screens.main.update(msg, ctx)
        }
        Screen::Unlock => state.screens.unlock.update(msg, ctx),
        Screen::Onboarding => state.screens.onboarding.update(msg, ctx),
        Screen::Config => {
            state.screens.config.state.terminal_height = state.terminal_size.1;
            state.screens.config.update(msg, ctx)
        }
        Screen::ChangeMasterPassword => state.screens.change_master_password.update(msg, ctx),
        Screen::SetNewMasterPassword => state.screens.set_new_master_password.update(msg, ctx),
        Screen::ImportExport => state.screens.import_export.update(msg, ctx),
        Screen::AuditLog => state.screens.audit_log.update(msg, ctx),
        Screen::SyncConflict => state.screens.sync_conflict.update(msg, ctx),
        // Placeholder screens — ignore messages.
        _ => ScreenResult::Continue,
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
        Screen::Config => {
            // Save current main screen focus panel before entering Config
            let current_panel = state.shared.focus.focused_panel;
            state.shared.screen_focus_stack.push(current_panel);
            state.screens.config.on_mount(ctx)
        }
        Screen::ChangeMasterPassword => state.screens.change_master_password.on_mount(ctx),
        Screen::SetNewMasterPassword => {
            let context = match state.screen_stack.last() {
                Some(Screen::Unlock) => {
                    crate::tui::screens::set_password::SetPasswordContext::PostRecovery
                }
                _ => {
                    match state.screens.onboarding.selected_path {
                        Some(crate::tui::screens::onboarding::OnboardingPath::Restore) => {
                            crate::tui::screens::set_password::SetPasswordContext::OnboardingRestore
                        }
                        _ => {
                            crate::tui::screens::set_password::SetPasswordContext::OnboardingCreate
                        }
                    }
                }
            };
            let vault_path = if !state.screens.onboarding.path_input.is_empty() {
                Some(std::path::PathBuf::from(&state.screens.onboarding.path_input))
            } else {
                None
            };
            let screen =
                crate::tui::screens::set_password::SetPasswordScreen::new(context);
            state.screens.set_new_master_password = match vault_path {
                Some(p) => screen.with_vault_path(p),
                None => screen,
            };
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
            if matches!(state.screen_stack.last(), Some(Screen::Onboarding)) {
                state.screens.import_export.mode =
                    crate::tui::screens::import_export::ImportExportMode::Import;
                state.screens.import_export.entry_point =
                    crate::tui::screens::import_export::ImportEntryPoint::Onboarding { step: 2 };
            }
            let current_panel = state.shared.focus.focused_panel;
            state.shared.screen_focus_stack.push(current_panel);
            state.screens.import_export.on_mount(ctx)
        }
        Screen::AuditLog => {
            let current_panel = state.shared.focus.focused_panel;
            state.shared.screen_focus_stack.push(current_panel);
            state.screens.audit_log.on_mount(ctx)
        }
        Screen::SyncConflict => {
            let current_panel = state.shared.focus.focused_panel;
            state.shared.screen_focus_stack.push(current_panel);
            state.screens.sync_conflict.on_mount(ctx)
        }
        _ => {}
    }
}

/// Call `on_unmount()` on the current screen before navigation.
fn route_on_unmount_from_state(state: &mut crate::tui::state::AppState) {
    match state.current_screen {
        Screen::Main => state.screens.main.on_unmount(),
        Screen::Unlock => state.screens.unlock.on_unmount(),
        Screen::Onboarding => state.screens.onboarding.on_unmount(),
        Screen::Config => {
            // Restore focus panel when leaving Config
            if let Some(panel) = state.shared.screen_focus_stack.pop() {
                state.shared.focus.focused_panel = panel;
            }
            state.screens.config.on_unmount()
        }
        Screen::ChangeMasterPassword => state.screens.change_master_password.on_unmount(),
        Screen::SetNewMasterPassword => state.screens.set_new_master_password.on_unmount(),
        Screen::ImportExport => {
            // Signal onboarding if returning from import (AC18)
            if matches!(
                state.screens.import_export.entry_point,
                crate::tui::screens::import_export::ImportEntryPoint::Onboarding { .. }
            ) {
                state.screens.onboarding.returning_from_import = true;
            }
            if let Some(panel) = state.shared.screen_focus_stack.pop() {
                state.shared.focus.focused_panel = panel;
            }
            state.screens.import_export.on_unmount()
        }
        Screen::AuditLog => {
            if let Some(panel) = state.shared.screen_focus_stack.pop() {
                state.shared.focus.focused_panel = panel;
            }
            state.screens.audit_log.on_unmount()
        }
        Screen::SyncConflict => {
            if let Some(panel) = state.shared.screen_focus_stack.pop() {
                state.shared.focus.focused_panel = panel;
            }
            state.screens.sync_conflict.on_unmount()
        }
        _ => {}
    }
}
