use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::main::detail::DetailPanel;
use oak_keyring::tui::state::detail_state::DetailPanelState;

#[test]
fn detail_panel_empty() {
    let backend = TestBackend::new(50, 20);
    let mut terminal = Terminal::new(backend).unwrap();
    let state = DetailPanelState::default();
    let panel = DetailPanel;

    terminal
        .draw(|frame| {
            panel.view(frame, frame.area(), &state, false, true);
        })
        .unwrap();

    insta::assert_snapshot!("detail_empty", terminal.backend());
}
