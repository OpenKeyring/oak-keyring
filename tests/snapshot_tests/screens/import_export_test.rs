use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use oak_keyring::commands::types::{
    ExportFormat, FailedItem, ImportPreview, ImportSource, ReviewItem,
};
use oak_keyring::tui::screens::import_export::{
    ExportFocus, ExportStep, ImportExportMode, ImportExportScreen, ImportFocus, ImportStep,
};
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

fn render_screen(screen: &ImportExportScreen, width: u16, height: u16) -> TestBackend {
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
fn import_source_select_default() {
    let _locale = snapshot_locale();
    let screen = ImportExportScreen::new();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("import_source_select_default", backend);
}

#[test]
fn import_source_select_file_path() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.import_focus = ImportFocus::FilePath;
    screen.file_path = "/path/to/passwords.kdbx".to_string();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("import_source_select_file_path", backend);
}

#[test]
fn import_source_select_csv() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.selected_source_idx = 4; // CSV
    screen.source = Some(ImportSource::Csv);
    screen.import_focus = ImportFocus::CsvName;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("import_source_select_csv", backend);
}

#[test]
fn import_preview_state() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.import_step = ImportStep::Preview;
    screen.preview = Some(ImportPreview {
        importable: 30,
        needs_review: 1,
        failed: 1,
        review_items: vec![ReviewItem {
            name: "GitHub".to_string(),
            reason: "Duplicate name found".to_string(),
        }],
        failed_items: vec![FailedItem {
            name: "SSH Key".to_string(),
            reason: "Corrupted private key file".to_string(),
        }],
        csv_headers: vec![],
    });
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("import_preview_state", backend);
}

#[test]
fn import_progress_state() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.import_step = ImportStep::Importing;
    screen.import_progress_current = 25;
    screen.import_progress_total = 100;
    screen.import_progress_name = "Work Email".to_string();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("import_progress_state", backend);
}

#[test]
fn import_complete_state() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.import_step = ImportStep::Complete;
    screen.imported_count = 15;
    screen.reviewed_count = 2;
    screen.skipped_count = 1;
    screen.failed_count = 0;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("import_complete_state", backend);
}

#[test]
fn export_form_default() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.mode = ImportExportMode::Export;
    screen.export_step = ExportStep::Form;
    screen.export_focus = ExportFocus::Scope;
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("export_form_default", backend);
}

#[test]
fn export_form_csv_warning() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.mode = ImportExportMode::Export;
    screen.export_step = ExportStep::Form;
    screen.export_format = ExportFormat::Csv;
    screen.export_focus = ExportFocus::OutputPath;
    screen.export_output_path = "/path/to/plain_export.csv".to_string();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("export_form_csv_warning", backend);
}

#[test]
fn export_password_confirm_state() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.mode = ImportExportMode::Export;
    screen.export_step = ExportStep::MasterPasswordConfirm;
    screen.master_password = sensitive("masterpass");
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("export_password_confirm_state", backend);
}

#[test]
fn export_exporting_state() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.mode = ImportExportMode::Export;
    screen.export_step = ExportStep::Exporting;
    screen.export_output_path = "/path/to/keyring-backup.okb".to_string();
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("export_exporting_state", backend);
}

#[test]
fn export_complete_state() {
    let _locale = snapshot_locale();
    let mut screen = ImportExportScreen::new();
    screen.mode = ImportExportMode::Export;
    screen.export_step = ExportStep::Complete;
    screen.export_record_count = 42;
    screen.export_result_path = Some(PathBuf::from("/path/to/keyring-backup.okb"));
    let backend = render_screen(&screen, 80, 24);
    insta::assert_snapshot!("export_complete_state", backend);
}
