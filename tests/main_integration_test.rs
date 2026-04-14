//! Integration snapshot test for the MainScreen three-panel layout.
//!
//! Renders the full MainScreen with populated state data into a TestBackend
//! and compares the output against a golden snapshot via insta.

use oak_keyring::commands::types::PanelId;
use oak_keyring::tui::screens::main::MainScreen;
use oak_keyring::tui::state::main_state::{CategoryCounts, MainScreenState};
use oak_keyring::types::tag::Tag;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

#[test]
fn main_screen_renders_with_data() {
    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    let screen = MainScreen::new();

    let mut state = MainScreenState::default();
    state.sidebar.category_counts = CategoryCounts {
        all: 42,
        favorites: 5,
        expired: 2,
        health_issues: 1,
        trash: 3,
    };
    state.sidebar.tags = vec![
        Tag {
            id: 1,
            name: "work".into(),
        },
        Tag {
            id: 2,
            name: "personal".into(),
        },
        Tag {
            id: 3,
            name: "finance".into(),
        },
    ];
    state.sidebar.tags_expanded = true;
    state.sidebar.rebuild();
    state.status_bar.record_count = 42;

    terminal
        .draw(|frame| {
            screen.view(frame, frame.area(), &state, PanelId::List, true);
        })
        .unwrap();

    insta::assert_snapshot!(terminal.backend());
}
