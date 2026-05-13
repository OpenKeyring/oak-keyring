use super::*;
use crate::commands::types::ImportSource;
use crate::crypto::strength::StrengthLevel;
use crate::tui::theme::{ERROR, PRIMARY, SUCCESS, WARNING};
use crate::tui::traits::screen::Screen as ScreenTrait;
use crate::tui::traits::screen::{ScreenContext, ScreenResult};
use crate::types::sensitive::SensitiveInput;

fn sensitive(s: &str) -> SensitiveInput {
    let mut input = SensitiveInput::new();
    for c in s.chars() {
        input.push_char(c);
    }
    input
}

#[test]
fn new_screen_defaults_to_import() {
    let screen = ImportExportScreen::new();
    assert_eq!(screen.mode, ImportExportMode::Import);
    assert_eq!(screen.import_step, ImportStep::SourceSelect);
    assert_eq!(screen.export_step, ExportStep::Form);
    assert!(screen.file_path.is_empty());
    assert!(screen.decrypt_password.is_empty());
    assert!(screen.error_message.is_none());
    assert!(screen.preview.is_none());
}

#[test]
fn on_mount_resets_state() {
    let mut screen = ImportExportScreen::new();
    screen.file_path = "/some/path".to_string();
    screen.decrypt_password = sensitive("secret");
    screen.error_message = Some("error".to_string());

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };
    ScreenTrait::on_mount(&mut screen, &mut ctx);

    assert!(screen.file_path.is_empty());
    assert!(screen.decrypt_password.is_empty());
    assert!(screen.error_message.is_none());
    assert_eq!(screen.import_step, ImportStep::SourceSelect);
}

#[test]
fn on_unmount_clears_sensitive() {
    let mut screen = ImportExportScreen::new();
    screen.file_path = "sensitive_path".to_string();
    screen.decrypt_password = sensitive("sensitive_pw");
    screen.export_password = sensitive("export_pw");
    screen.export_confirm_password = sensitive("confirm_pw");
    screen.master_password = sensitive("master_pw");

    ScreenTrait::on_unmount(&mut screen);

    assert!(screen.file_path.is_empty());
    assert!(screen.decrypt_password.is_empty());
    assert!(screen.export_password.is_empty());
    assert!(screen.export_confirm_password.is_empty());
    assert!(screen.master_password.is_empty());
}

#[test]
fn source_needs_password_correct() {
    assert!(source_needs_password(ImportSource::KeePass));
    assert!(!source_needs_password(ImportSource::OnePassword1pux));
    assert!(source_needs_password(ImportSource::OnePasswordOpvault));
    assert!(source_needs_password(ImportSource::Bitwarden));
    assert!(!source_needs_password(ImportSource::Csv));
    assert!(!source_needs_password(ImportSource::OpenKeyringBackup));
}

#[test]
fn source_display_names() {
    assert_eq!(source_display(ImportSource::KeePass), "KeePass (.kdbx)");
    assert_eq!(source_display(ImportSource::Csv), "CSV");
    assert_eq!(
        source_display(ImportSource::OpenKeyringBackup),
        "OpenKeyring Backup (.okb)"
    );
}

#[test]
fn display_password_masks() {
    let displayed = ImportExportScreen::display_password("hello");
    assert_eq!(displayed, "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}");
}

#[test]
fn strength_color_mapping() {
    assert_eq!(
        ImportExportScreen::strength_color(&StrengthLevel::VeryWeak),
        ERROR
    );
    assert_eq!(
        ImportExportScreen::strength_color(&StrengthLevel::Weak),
        ERROR
    );
    assert_eq!(
        ImportExportScreen::strength_color(&StrengthLevel::Fair),
        WARNING
    );
    assert_eq!(
        ImportExportScreen::strength_color(&StrengthLevel::Strong),
        PRIMARY
    );
    assert_eq!(
        ImportExportScreen::strength_color(&StrengthLevel::VeryStrong),
        SUCCESS
    );
}

#[test]
fn csv_mapping_defaults() {
    let screen = ImportExportScreen::new();
    assert_eq!(screen.csv_mapping.name_column, "Title");
    assert_eq!(screen.csv_mapping.username_column, "Username");
    assert_eq!(screen.csv_mapping.password_column, "Password");
    assert_eq!(screen.csv_mapping.url_column, "URL");
    assert_eq!(screen.csv_mapping.notes_column, "Notes");
    assert!(screen.csv_mapping.tags_column.is_none());
    assert!(screen.csv_mapping.skip_header);
}

#[test]
fn export_strength_updates() {
    let mut screen = ImportExportScreen::new();
    assert!(screen.export_password_strength.is_none());

    screen.export_password = sensitive("a");
    screen.update_export_strength();
    assert_eq!(
        screen.export_password_strength.as_ref().unwrap().level,
        StrengthLevel::VeryWeak
    );

    screen.export_password.clear();
    screen.update_export_strength();
    assert!(screen.export_password_strength.is_none());
}

#[test]
fn import_focus_cycle() {
    let mut screen = ImportExportScreen::new();
    // Default: KeePass (needs password, not CSV)
    screen.selected_source_idx = 0;
    assert_eq!(screen.import_focus, ImportFocus::SourceList);

    screen.import_focus_cycle_next();
    assert_eq!(screen.import_focus, ImportFocus::FilePath);

    screen.import_focus_cycle_next();
    assert_eq!(screen.import_focus, ImportFocus::Password);

    screen.import_focus_cycle_next();
    assert_eq!(screen.import_focus, ImportFocus::SourceList);

    // CSV: has csv fields
    screen.selected_source_idx = 4; // CSV
    screen.import_focus = ImportFocus::SourceList;
    screen.import_focus_cycle_next();
    assert_eq!(screen.import_focus, ImportFocus::FilePath);

    screen.import_focus_cycle_next();
    assert_eq!(screen.import_focus, ImportFocus::CsvName);

    screen.import_focus_cycle_next();
    assert_eq!(screen.import_focus, ImportFocus::CsvUsername);
}

#[test]
fn export_focus_cycle() {
    let mut screen = ImportExportScreen::new();
    assert_eq!(screen.export_focus, ExportFocus::Scope);

    screen.export_focus = ExportFocus::ExportPassword;
    screen.export_focus = ExportFocus::ConfirmPassword;
    screen.export_focus = ExportFocus::OutputPath;
    screen.export_focus = ExportFocus::Scope;
}

#[test]
fn export_scope_options() {
    let screen = ImportExportScreen::new();
    assert_eq!(screen.export_scope_option, ExportScopeOption::All);
}

#[test]
fn import_export_restore_state_restores_navigation_without_sensitive_buffers() {
    let mut screen = ImportExportScreen::new();
    screen.mode = ImportExportMode::Export;
    screen.entry_point = ImportEntryPoint::ConfigPage;
    screen.import_step = ImportStep::Preview;
    screen.selected_source_idx = 4;
    screen.import_focus = ImportFocus::CsvUsername;
    screen.export_step = ExportStep::MasterPasswordConfirm;
    screen.export_focus = ExportFocus::ConfirmPassword;
    screen.export_scope_option = ExportScopeOption::CurrentFilter;
    screen.decrypt_password = sensitive("secret");
    screen.export_password = sensitive("export-secret");
    screen.master_password = sensitive("master-secret");

    let restore = screen.to_restore_state();

    let mut restored = ImportExportScreen::new();
    restored.restore_from(restore);

    assert_eq!(restored.mode, ImportExportMode::Export);
    assert_eq!(restored.import_step, ImportStep::Preview);
    assert_eq!(restored.selected_source_idx, 4);
    assert_eq!(restored.import_focus, ImportFocus::CsvUsername);
    assert_eq!(restored.export_step, ExportStep::MasterPasswordConfirm);
    assert_eq!(restored.export_focus, ExportFocus::ConfirmPassword);
    assert_eq!(
        restored.export_scope_option,
        ExportScopeOption::CurrentFilter
    );
    assert!(restored.decrypt_password.is_empty());
    assert!(restored.export_password.is_empty());
    assert!(restored.master_password.is_empty());
}

#[test]
fn on_mount_resets_reviewed_and_failed_counts() {
    let mut screen = ImportExportScreen::new();
    screen.reviewed_count = 5;
    screen.failed_count = 2;

    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let config = crate::config::AppConfig::default();
    let mut ctx = ScreenContext {
        command_tx: &tx,
        config: &config,
    };
    ScreenTrait::on_mount(&mut screen, &mut ctx);

    assert_eq!(screen.reviewed_count, 0);
    assert_eq!(screen.failed_count, 0);
}

#[test]
fn esc_from_config_entry_uses_pop_screen_not_forward_navigation() {
    let mut screen = ImportExportScreen::new();
    screen.entry_point = ImportEntryPoint::ConfigPage;
    screen.export_step = ExportStep::Form;

    let result = screen.go_back();

    assert!(matches!(result, ScreenResult::PopScreen));
}

#[test]
fn import_sources_have_scope_hint_styles() {
    use super::ScopeHintStyle;

    assert_eq!(IMPORT_SOURCES[0].3 .1, ScopeHintStyle::Full); // KeePass
    assert_eq!(IMPORT_SOURCES[1].3 .1, ScopeHintStyle::Partial); // 1Password 1pux
    assert_eq!(IMPORT_SOURCES[2].3 .1, ScopeHintStyle::Partial); // 1Password opvault
    assert_eq!(IMPORT_SOURCES[3].3 .1, ScopeHintStyle::Limited); // Bitwarden
    assert_eq!(IMPORT_SOURCES[4].3 .1, ScopeHintStyle::Full); // CSV
    assert_eq!(IMPORT_SOURCES[5].3 .1, ScopeHintStyle::Full); // OpenKeyring Backup
}
