use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::tui::screens::config_screen::ConfigScreen;
use oak_keyring::tui::state::config_state::FooterButton;

fn render_config(screen: &ConfigScreen) -> TestBackend {
    let backend = TestBackend::new(80, 16);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            use oak_keyring::tui::traits::screen::Screen;
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

#[test]
fn config_footer_no_focus() {
    let screen = ConfigScreen::new();
    let backend = render_config(&screen);
    insta::assert_snapshot!("config_footer_no_focus", backend);
}

#[test]
fn config_footer_exit_program_focused() {
    let mut screen = ConfigScreen::new();
    screen.state.footer_focus = Some(FooterButton::ExitProgram);
    let backend = render_config(&screen);
    insta::assert_snapshot!("config_footer_exit_program_focused", backend);
}

#[test]
fn config_footer_close_focused() {
    let mut screen = ConfigScreen::new();
    screen.state.footer_focus = Some(FooterButton::Close);
    let backend = render_config(&screen);
    insta::assert_snapshot!("config_footer_close_focused", backend);
}
