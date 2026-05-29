use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::crypto::strength::evaluate_strength;
use oak_keyring::tui::screens::create_record::CreateRecordScreen;
use oak_keyring::tui::state::form_state::{TagAutocompleteState, ValidationError};
use oak_keyring::tui::state::generator_state::GeneratorFocus;
use oak_keyring::tui::traits::screen::Screen;
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::sensitive::SensitiveInput;

use crate::support::snapshot_locale;

fn sensitive(s: &str) -> SensitiveInput {
    let mut input = SensitiveInput::new();
    for c in s.chars() {
        input.push_char(c);
    }
    input
}

fn render_screen(screen: &CreateRecordScreen, width: u16, height: u16) -> TestBackend {
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
fn create_record_login_empty() {
    let _locale = snapshot_locale();
    let screen = CreateRecordScreen::new();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_login_empty", backend);
}

#[test]
fn create_record_login_populated() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.fields.name = "My Personal Webmail".to_string();
    screen.form.fields.url = "https://mail.example.com".to_string();
    screen.form.fields.username = Some("alice_smith".to_string());
    screen.form.fields.password = Some(sensitive("supersecurepassword"));
    screen.form.fields.tags = vec!["work".to_string(), "personal".to_string()];
    screen.form.fields.set_notes_text("Do not share this login info with anyone.");
    screen.form.focused_field = 4; // Notes field
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_login_populated", backend);
}

#[test]
fn create_record_validation_errors() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.fields.name = "".to_string();
    screen.form.validation_errors = vec![ValidationError {
        field_index: 0,
        message: "Title cannot be empty".to_string(),
    }];
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_validation_errors", backend);
}

#[test]
fn create_record_credential_dropdown() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.credential_dropdown.expanded = true;
    screen.form.credential_dropdown.selected_index = 1; // API Key
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_credential_dropdown", backend);
}

#[test]
fn create_record_weak_password_dialog() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.show_weak_password_dialog = true;
    screen.form.weak_dialog_focus = 1; // "Save Anyway"
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_weak_password_dialog", backend);
}

#[test]
fn create_record_unsaved_dialog() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.fields.name = "Changed Account".to_string();
    screen.form.has_changes = true;
    screen.form.show_unsaved_dialog = true;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_unsaved_dialog", backend);
}

#[test]
fn create_record_api_key() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.switch_credential_type(CredentialType::Api);
    screen.form.fields.name = "Stripe API Key".to_string();
    screen.form.fields.app_id = Some("stripe_live_51".to_string());
    screen.form.fields.secret_key = Some(sensitive("sk_live_123456789"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_api_key", backend);
}

#[test]
fn create_record_ssh_key() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.switch_credential_type(CredentialType::Ssh);
    screen.form.fields.name = "GitHub Deployment Key".to_string();
    screen.form.fields.public_key = Some("ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC...".to_string());
    screen.form.fields.private_key = Some(sensitive("-----BEGIN OPENSSH PRIVATE KEY-----\n..."));
    screen.form.fields.passphrase = Some(sensitive("keypassphrase"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_ssh_key", backend);
}

#[test]
fn create_record_tag_autocomplete() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.form.focused_field = 5; // tags field
    screen.form.tag_autocomplete = Some(TagAutocompleteState {
        matches: vec!["finances".to_string(), "facebook".to_string()],
        selected_index: 0,
    });
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_tag_autocomplete", backend);
}

#[test]
fn create_record_embedded_generator() {
    let _locale = snapshot_locale();
    let mut screen = CreateRecordScreen::new();
    screen.generator.expand();
    screen.generator.generator.preview = sensitive("CorrectHorse42!");
    screen.generator.generator.strength = Some(evaluate_strength("CorrectHorse42!"));
    screen.generator.generator.focus = GeneratorFocus::ActionButton;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("create_record_embedded_generator", backend);
}
