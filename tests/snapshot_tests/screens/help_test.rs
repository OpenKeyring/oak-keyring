use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::main::overlay::help::render_help;

#[test]
fn help_overlay_wide_layout() {
    let backend = TestBackend::new(130, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_help(frame, frame.area());
        })
        .unwrap();
    insta::assert_snapshot!("help_overlay_wide", terminal.backend());
}

#[test]
fn help_overlay_medium_layout() {
    let backend = TestBackend::new(110, 35);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_help(frame, frame.area());
        })
        .unwrap();
    insta::assert_snapshot!("help_overlay_medium", terminal.backend());
}

#[test]
fn help_overlay_compact_layout() {
    let backend = TestBackend::new(90, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_help(frame, frame.area());
        })
        .unwrap();
    insta::assert_snapshot!("help_overlay_compact", terminal.backend());
}
