use chrono::TimeZone;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use uuid::Uuid;

use oak_keyring::tui::screens::sync_conflict::SyncConflictScreen;
use oak_keyring::tui::state::sync_ui_state::{
    ConflictDisplay, ConflictField, ConflictResolutionState, ConflictSide,
};
use oak_keyring::tui::traits::screen::Screen;

use crate::support::snapshot_locale;

fn render_screen(screen: &SyncConflictScreen, width: u16, height: u16) -> TestBackend {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

fn sample_conflicts() -> Vec<ConflictDisplay> {
    vec![
        ConflictDisplay {
            record_id: Uuid::new_v4(),
            record_name: "Personal Gmail".to_string(),
            local_fields: vec![
                ConflictField {
                    label: "Username".to_string(),
                    value: "alice_local@gmail.com".to_string(),
                    differs: true,
                    is_sensitive: false,
                    is_masked: false,
                },
                ConflictField {
                    label: "Password".to_string(),
                    value: "••••••••••••".to_string(),
                    differs: false,
                    is_sensitive: true,
                    is_masked: true,
                },
            ],
            remote_fields: vec![
                ConflictField {
                    label: "Username".to_string(),
                    value: "alice_remote@gmail.com".to_string(),
                    differs: true,
                    is_sensitive: false,
                    is_masked: false,
                },
                ConflictField {
                    label: "Password".to_string(),
                    value: "••••••••••••".to_string(),
                    differs: false,
                    is_sensitive: true,
                    is_masked: true,
                },
            ],
            local_time: chrono::Utc.with_ymd_and_hms(2026, 5, 20, 10, 0, 0).unwrap(),
            remote_time: chrono::Utc
                .with_ymd_and_hms(2026, 5, 20, 10, 15, 0)
                .unwrap(),
        },
        ConflictDisplay {
            record_id: Uuid::new_v4(),
            record_name: "Github Account".to_string(),
            local_fields: vec![ConflictField {
                label: "Password".to_string(),
                value: "••••••••••••".to_string(),
                differs: true,
                is_sensitive: true,
                is_masked: true,
            }],
            remote_fields: vec![ConflictField {
                label: "Password".to_string(),
                value: "••••••••••••".to_string(),
                differs: true,
                is_sensitive: true,
                is_masked: true,
            }],
            local_time: chrono::Utc.with_ymd_and_hms(2026, 5, 20, 11, 0, 0).unwrap(),
            remote_time: chrono::Utc.with_ymd_and_hms(2026, 5, 20, 11, 5, 0).unwrap(),
        },
    ]
}

#[test]
fn sync_conflict_local_focused() {
    let _locale = snapshot_locale();
    let screen = SyncConflictScreen {
        state: ConflictResolutionState {
            conflicts: sample_conflicts(),
            current_index: 0,
            focused_side: ConflictSide::Local,
        },
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("sync_conflict_local_focused", backend);
}

#[test]
fn sync_conflict_remote_focused() {
    let _locale = snapshot_locale();
    let screen = SyncConflictScreen {
        state: ConflictResolutionState {
            conflicts: sample_conflicts(),
            current_index: 0,
            focused_side: ConflictSide::Remote,
        },
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("sync_conflict_remote_focused", backend);
}

#[test]
fn sync_conflict_second_in_queue() {
    let _locale = snapshot_locale();
    let screen = SyncConflictScreen {
        state: ConflictResolutionState {
            conflicts: sample_conflicts(),
            current_index: 1,
            focused_side: ConflictSide::Local,
        },
    };
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("sync_conflict_second_in_queue", backend);
}
