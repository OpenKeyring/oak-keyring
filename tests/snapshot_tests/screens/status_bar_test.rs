use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::commands::types::PanelId;
use oak_keyring::tui::screens::main::status_bar::StatusBarPanel;
use oak_keyring::tui::state::main_state::{
    HealthCheckPhase, StatusBarState, StatusMessage, SyncIndicator,
};

fn render_status_bar(state: &StatusBarState) -> TestBackend {
    let backend = TestBackend::new(120, 1);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            StatusBarPanel::view(
                frame,
                frame.area(),
                state,
                PanelId::Sidebar,
                true,
                false,
                false,
            );
        })
        .unwrap();
    terminal.backend().clone()
}

#[test]
fn status_bar_health_inactive() {
    let state = StatusBarState {
        health_check_phase: HealthCheckPhase::Inactive,
        sync_status: SyncIndicator::Synced,
        record_count: 10,
        ..Default::default()
    };
    let backend = render_status_bar(&state);
    insta::assert_snapshot!("status_bar_health_inactive", backend);
}

#[test]
fn status_bar_health_checking() {
    let state = StatusBarState {
        health_check_phase: HealthCheckPhase::Checking,
        health_check_progress: Some((3, 42)),
        sync_status: SyncIndicator::Synced,
        record_count: 10,
        ..Default::default()
    };
    let backend = render_status_bar(&state);
    insta::assert_snapshot!("status_bar_health_checking", backend);
}

#[test]
fn status_bar_health_needs_attention() {
    let state = StatusBarState {
        health_check_phase: HealthCheckPhase::NeedsAttention {
            weak: 3,
            compromised: 1,
            duplicate_groups: 2,
        },
        sync_status: SyncIndicator::Synced,
        record_count: 42,
        ..Default::default()
    };
    let backend = render_status_bar(&state);
    insta::assert_snapshot!("status_bar_health_needs_attention", backend);
}

#[test]
fn status_bar_health_all_secure() {
    let state = StatusBarState {
        health_check_phase: HealthCheckPhase::AllSecure,
        sync_status: SyncIndicator::Synced,
        record_count: 42,
        ..Default::default()
    };
    let backend = render_status_bar(&state);
    insta::assert_snapshot!("status_bar_health_all_secure", backend);
}

#[test]
fn status_bar_health_skipped() {
    let state = StatusBarState {
        health_check_phase: HealthCheckPhase::Skipped,
        sync_status: SyncIndicator::Synced,
        record_count: 10,
        ..Default::default()
    };
    let backend = render_status_bar(&state);
    insta::assert_snapshot!("status_bar_health_skipped", backend);
}

#[test]
fn status_bar_with_clipboard_countdown() {
    let state = StatusBarState {
        health_check_phase: HealthCheckPhase::AllSecure,
        sync_status: SyncIndicator::Syncing,
        status_message: Some(StatusMessage::ClipboardCountdown {
            field: "\u{5BC6}\u{7801}".to_string(), // 密码
            seconds: 30,
        }),
        ..Default::default()
    };
    let backend = render_status_bar(&state);
    insta::assert_snapshot!("status_bar_clipboard_countdown", backend);
}
