use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::crypto::strength::evaluate_strength;
use oak_keyring::tui::screens::password_generator::PasswordGeneratorScreen;
use oak_keyring::tui::state::generator_state::{GenerationStyle, GeneratorFocus};
use oak_keyring::tui::traits::screen::Screen;
use oak_keyring::types::sensitive::SensitiveInput;

use crate::support::snapshot_locale;

fn sensitive(s: &str) -> SensitiveInput {
    let mut input = SensitiveInput::new();
    for c in s.chars() {
        input.push_char(c);
    }
    input
}

fn render_screen(screen: &PasswordGeneratorScreen, width: u16, height: u16) -> TestBackend {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            screen.view(frame, frame.area());
        })
        .unwrap();
    terminal.backend().clone()
}

#[test]
fn password_generator_random_style_focused() {
    let _locale = snapshot_locale();
    let mut screen = PasswordGeneratorScreen::new();
    screen.state.style = GenerationStyle::Random;
    screen.state.focus = GeneratorFocus::StyleSelector;
    screen.state.preview = sensitive("j8#K9mP2!qR5$tW1");
    screen.state.strength = Some(evaluate_strength("j8#K9mP2!qR5$tW1"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("password_generator_random_style_focused", backend);
}

#[test]
fn password_generator_random_slider_focused() {
    let _locale = snapshot_locale();
    let mut screen = PasswordGeneratorScreen::new();
    screen.state.style = GenerationStyle::Random;
    screen.state.focus = GeneratorFocus::LengthSlider;
    screen.state.random_config.length = 20;
    screen.state.preview = sensitive("aB3#cD4$eF5%gH6&iJ7*");
    screen.state.strength = Some(evaluate_strength("aB3#cD4$eF5%gH6&iJ7*"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("password_generator_random_slider_focused", backend);
}

#[test]
fn password_generator_random_toggle_focused() {
    let _locale = snapshot_locale();
    let mut screen = PasswordGeneratorScreen::new();
    screen.state.style = GenerationStyle::Random;
    screen.state.focus = GeneratorFocus::Toggle(1); // Uppercase toggle
    screen.state.random_config.uppercase = false;
    screen.state.preview = sensitive("k9m2q5t1v8x3z7w6");
    screen.state.strength = Some(evaluate_strength("k9m2q5t1v8x3z7w6"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("password_generator_random_toggle_focused", backend);
}

#[test]
fn password_generator_memorable_style() {
    let _locale = snapshot_locale();
    let mut screen = PasswordGeneratorScreen::new();
    screen.state.style = GenerationStyle::Memorable;
    screen.state.focus = GeneratorFocus::StyleSelector;
    screen.state.memorable_config.word_count = 3;
    screen.state.preview = sensitive("Correct-Horse-Battery");
    screen.state.strength = Some(evaluate_strength("Correct-Horse-Battery"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("password_generator_memorable_style", backend);
}

#[test]
fn password_generator_memorable_separator_focused() {
    let _locale = snapshot_locale();
    let mut screen = PasswordGeneratorScreen::new();
    screen.state.style = GenerationStyle::Memorable;
    screen.state.focus = GeneratorFocus::SeparatorInput;
    screen.state.memorable_config.separator = "_".to_string();
    screen.state.preview = sensitive("Correct_Horse_Battery_Staple");
    screen.state.strength = Some(evaluate_strength("Correct_Horse_Battery_Staple"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("password_generator_memorable_separator_focused", backend);
}

#[test]
fn password_generator_pin_style() {
    let _locale = snapshot_locale();
    let mut screen = PasswordGeneratorScreen::new();
    screen.state.style = GenerationStyle::Pin;
    screen.state.focus = GeneratorFocus::LengthSlider;
    screen.state.pin_config.length = 8;
    screen.state.preview = sensitive("94820153");
    screen.state.strength = Some(evaluate_strength("94820153"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("password_generator_pin_style", backend);
}

#[test]
fn password_generator_with_hint() {
    let _locale = snapshot_locale();
    let mut screen = PasswordGeneratorScreen::new();
    screen.state.preview = sensitive("j8#K9mP2!qR5$tW1");
    screen.state.strength = Some(evaluate_strength("j8#K9mP2!qR5$tW1"));
    screen.hint_message = Some("Password copied to clipboard!".to_string());
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("password_generator_with_hint", backend);
}
