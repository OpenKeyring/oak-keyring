use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::components::notification::render_notification;
use oak_keyring::tui::state::notification::StatusMessage;

use crate::support::snapshot_locale;

fn render_notification_bar(msg: &StatusMessage) -> TestBackend {
    let _locale = snapshot_locale();
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_notification(frame, frame.area(), msg);
        })
        .unwrap();
    terminal.backend().clone()
}

#[test]
fn notification_success() {
    let msg = StatusMessage::success("Configuration saved".into());
    let backend = render_notification_bar(&msg);
    insta::assert_snapshot!("notification_success", backend);
}

#[test]
fn notification_warning() {
    let msg = StatusMessage::warning("Sync service rebuild failed: connection timeout".into());
    let backend = render_notification_bar(&msg);
    insta::assert_snapshot!("notification_warning", backend);
}

#[test]
fn notification_error() {
    let msg = StatusMessage::error("Failed to save config: permission denied".into());
    let backend = render_notification_bar(&msg);
    insta::assert_snapshot!("notification_error", backend);
}

#[test]
fn notification_operation() {
    let msg = StatusMessage::operation("Syncing...".into());
    let backend = render_notification_bar(&msg);
    insta::assert_snapshot!("notification_operation", backend);
}
