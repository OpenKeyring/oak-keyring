use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::commands::Message;
use oak_keyring::config::AppConfig;
use oak_keyring::tui::screens::set_password::{
    PasswordField, SetPasswordContext, SetPasswordScreen,
};
use oak_keyring::tui::traits::screen::{Screen as ScreenTrait, ScreenContext};

fn render_screen(screen: &SetPasswordScreen, width: u16, height: u16) -> TestBackend {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

fn dummy_ctx() -> ScreenContext<'static> {
    static ONCE: std::sync::Once = std::sync::Once::new();
    static mut TX: Option<tokio::sync::mpsc::Sender<oak_keyring::commands::Command>> = None;

    ONCE.call_once(|| {
        let (tx, _rx) = tokio::sync::mpsc::channel(16);
        unsafe { TX = Some(tx) };
    });

    let tx = unsafe { TX.as_ref().unwrap() };
    static DUMMY_CONFIG: std::sync::OnceLock<AppConfig> = std::sync::OnceLock::new();
    let config = DUMMY_CONFIG.get_or_init(AppConfig::default);

    ScreenContext {
        command_tx: tx,
        config,
    }
}

fn type_into_field(screen: &mut SetPasswordScreen, text: &str, ctx: &mut ScreenContext<'_>) {
    for ch in text.chars() {
        screen.update(
            Message::KeyEvent(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
            ctx,
        );
    }
}

#[test]
fn set_password_empty_state() {
    let screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
    let backend = render_screen(&screen, 60, 16);
    insta::assert_snapshot!("set_password_empty", backend);
}

#[test]
fn set_password_with_input_masked() {
    let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
    let mut ctx = dummy_ctx();
    type_into_field(&mut screen, "mysecretpassword", &mut ctx);
    let backend = render_screen(&screen, 60, 16);
    insta::assert_snapshot!("set_password_with_input_masked", backend);
}

#[test]
fn set_password_with_input_visible() {
    let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
    screen.password_visible = true;
    let mut ctx = dummy_ctx();
    type_into_field(&mut screen, "mysecretpassword", &mut ctx);
    let backend = render_screen(&screen, 60, 16);
    insta::assert_snapshot!("set_password_visible", backend);
}

#[test]
fn set_password_confirm_focused_with_match() {
    let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingRestore);
    let mut ctx = dummy_ctx();
    type_into_field(&mut screen, "strongpassword", &mut ctx);
    // Tab to confirm field
    screen.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        &mut ctx,
    );
    type_into_field(&mut screen, "strongpassword", &mut ctx);
    let backend = render_screen(&screen, 60, 16);
    insta::assert_snapshot!("set_password_confirm_focused_with_match", backend);
}

#[test]
fn set_password_with_error() {
    let mut screen = SetPasswordScreen::new(SetPasswordContext::OnboardingCreate);
    let mut ctx = dummy_ctx();
    // Type a short password then submit to trigger error
    type_into_field(&mut screen, "short", &mut ctx);
    screen.update(
        Message::KeyEvent(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &mut ctx,
    );
    let backend = render_screen(&screen, 60, 16);
    insta::assert_snapshot!("set_password_with_error", backend);
}
