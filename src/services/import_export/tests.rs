use super::*;

use std::collections::HashSet;
use std::path::PathBuf;
use uuid::Uuid;

use crate::commands::types::{CsvColumnMapping, ExportFormat, ExportScope, ImportSource};
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::duplicate::ExistingRecordKey;
use crate::services::import_export::types::{ExportSessionStatus, ImportSessionStatus};
use crate::types::SecureStr;

// Import parameter structs and implementation for tests
use crate::services::import_export::{ExportParams, ImportExportServiceImpl, ImportParams};

// -- Helpers --

fn default_csv_mapping() -> CsvColumnMapping {
    CsvColumnMapping {
        name_column: "name".to_string(),
        username_column: "username".to_string(),
        password_column: "password".to_string(),
        url_column: "url".to_string(),
        notes_column: "notes".to_string(),
        tags_column: None,
        skip_header: true,
    }
}

fn create_csv_file(content: &str) -> tempfile::NamedTempFile {
    use std::io::Write;
    let mut f = tempfile::Builder::new()
        .suffix(".csv")
        .tempfile()
        .expect("create temp csv");
    f.write_all(content.as_bytes()).expect("write csv");
    f
}

fn simple_csv_content() -> &'static str {
    "name,username,password,url,notes\n\
     Gmail,user1@gmail.com,pass1,https://gmail.com,Personal email\n\
     GitHub,dev@github.com,pass2,https://github.com,Code repo\n\
     AWS,admin@aws.com,pass3,https://aws.amazon.com,Cloud account\n"
}

// -- Test 1: Create session ----------------------------------------------

#[test]
fn create_session_has_created_status() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    assert_eq!(
        service.import_session_status(id),
        Some(ImportSessionStatus::Created)
    );
}

// -- Test 2: Validate CSV ------------------------------------------------

#[test]
fn validate_csv_produces_correct_preview() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    let preview = service.validate_import_file(id).expect("validate");

    assert_eq!(
        service.import_session_status(id),
        Some(ImportSessionStatus::Validated)
    );
    assert_eq!(preview.importable, 3);
    assert_eq!(preview.failed, 0);
    assert!(preview.review_items.is_empty());
    assert!(preview.failed_items.is_empty());
}

// -- Test 3: Full import CSV ----------------------------------------------

#[test]
fn full_import_csv_produces_correct_result() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    service.validate_import_file(id).expect("validate");

    let existing: HashSet<ExistingRecordKey> = HashSet::new();
    let params = ImportParams {
        session_id: id,
        existing_keys: existing,
        import_as_notes: false,
        progress_fn: None,
    };
    let (result, _records) = service.execute_import(params).expect("execute import");

    assert_eq!(
        service.import_session_status(id),
        Some(ImportSessionStatus::Completed)
    );
    assert_eq!(result.imported, 3);
    assert_eq!(result.skipped, 0);
    assert_eq!(result.failed, 0);
}

// -- Test 4: Session not found --------------------------------------------

#[test]
fn operations_on_invalid_uuid_return_session_not_found() {
    let mut service = ImportExportServiceImpl::new();
    let bogus_id = Uuid::new_v4();

    let result = service.validate_import_file(bogus_id);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        ImportExportError::SessionNotFound(id) if id == bogus_id
    ));

    let result = service.get_import_preview(bogus_id);
    assert!(result.is_err());

    let existing: HashSet<ExistingRecordKey> = HashSet::new();
    let params = ImportParams {
        session_id: bogus_id,
        existing_keys: existing,
        import_as_notes: false,
        progress_fn: None,
    };
    let result = service.execute_import(params);
    assert!(result.is_err());

    let result = service.cancel_import(bogus_id);
    assert!(result.is_err());

    let result = service.cleanup_session(bogus_id);
    assert!(result.is_err());
}

// -- Test 5: Wrong status ------------------------------------------------

#[test]
fn execute_on_created_session_returns_invalid_status() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    // Do NOT validate first — attempt to import directly.
    let existing: HashSet<ExistingRecordKey> = HashSet::new();
    let params = ImportParams {
        session_id: id,
        existing_keys: existing,
        import_as_notes: false,
        progress_fn: None,
    };
    let result = service.execute_import(params);

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, ImportExportError::InvalidSessionStatus { ref expected, .. }
            if expected == "Validated"),
        "expected InvalidSessionStatus, got: {err:?}"
    );
}

// -- Test 6: Cancel import -----------------------------------------------

#[test]
fn cancel_changes_status_to_cancelled() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    service.cancel_import(id).expect("cancel");

    assert_eq!(
        service.import_session_status(id),
        Some(ImportSessionStatus::Cancelled)
    );
}

// -- Test 7: Cleanup removes session --------------------------------------

#[test]
fn cleanup_removes_session_from_map() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    assert_eq!(service.session_count(), 1);
    service.cleanup_session(id).expect("cleanup");
    assert_eq!(service.session_count(), 0);

    // Further operations on the same ID should fail.
    let result = service.get_import_preview(id);
    assert!(result.is_err());
}

// -- Test 8: All parsers registered ---------------------------------------

#[test]
fn all_six_parsers_registered_in_registry() {
    let service = ImportExportServiceImpl::new();

    let formats = [
        ImportSource::Csv,
        ImportSource::Bitwarden,
        ImportSource::KeePass,
        ImportSource::OnePassword1pux,
        ImportSource::OnePasswordOpvault,
        ImportSource::OpenKeyringBackup,
    ];

    for fmt in &formats {
        assert!(
            service.has_parser(*fmt),
            "parser for {fmt:?} should be registered"
        );
    }
}

// -- Test 9: Validate with validation failures ----------------------------

#[test]
fn validate_csv_with_missing_fields_reports_failures() {
    let mut service = ImportExportServiceImpl::new();
    // CSV with rows missing required fields (username, password).
    let csv = "name,username,password,url,notes\n\
               GoodEntry,user@example.com,pass123,https://example.com,\n\
               NoUser,,pass2,https://example.com,\n\
               NoPassword,user3@example.com,,,\n";
    let f = create_csv_file(csv);

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    let preview = service.validate_import_file(id).expect("validate");

    assert_eq!(preview.importable, 1);
    assert_eq!(preview.failed, 2);
    assert_eq!(preview.failed_items.len(), 2);
}

// -- Test 10: Import with duplicates skipped -------------------------------

#[test]
fn import_skips_duplicate_records() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    service.validate_import_file(id).expect("validate");

    // Create existing key matching the first CSV row (Gmail).
    let mut existing = HashSet::new();
    existing.insert(ExistingRecordKey {
        name: "gmail".to_string(),
        credential_type: "login".to_string(),
        core_field: "user1@gmail.com".to_string(),
    });

    let params = ImportParams {
        session_id: id,
        existing_keys: existing,
        import_as_notes: false,
        progress_fn: None,
    };
    let (result, _records) = service.execute_import(params).expect("execute import");

    assert_eq!(result.imported, 2);
    assert_eq!(result.skipped, 1);
    assert_eq!(result.failed, 0);
}

// -- Test 11: Import returns correct record counts --------------------------

#[test]
fn import_returns_correct_record_counts() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    service.validate_import_file(id).expect("validate");

    let existing: HashSet<ExistingRecordKey> = HashSet::new();
    let params = ImportParams {
        session_id: id,
        existing_keys: existing,
        import_as_notes: false,
        progress_fn: None,
    };
    let (result, records) = service.execute_import(params).expect("execute");

    assert_eq!(result.imported, 3);
    assert_eq!(result.failed, 0);
    assert_eq!(records.len(), 3);
}

// -- Test 12: Get import preview after validate ----------------------------

#[test]
fn get_import_preview_returns_validation_result() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    service.validate_import_file(id).expect("validate");

    let preview = service.get_import_preview(id).expect("preview");
    assert_eq!(preview.importable, 3);
}

// -- Test 13: Validate on already-validated session fails -----------------

#[test]
fn validate_on_validated_session_returns_invalid_status() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create session");

    service.validate_import_file(id).expect("first validate");

    let result = service.validate_import_file(id);
    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ImportExportError::InvalidSessionStatus { ref expected, .. }
            if expected == "Created"),
    );
}

// -- Test 14: Default trait -----------------------------------------------

#[test]
fn default_trait_creates_service_with_parsers() {
    let service = ImportExportService::default();
    assert!(service.has_parser(ImportSource::Csv));
    assert!(service.has_parser(ImportSource::KeePass));
}

// -- Test 15: session_status introspection ---------------------------------

#[test]
fn session_status_returns_status_for_existing_session() {
    let mut service = ImportExportServiceImpl::new();
    let f = create_csv_file(simple_csv_content());

    let id = service
        .create_import_session(
            ImportSource::Csv,
            f.path().to_path_buf(),
            None,
            Some(default_csv_mapping()),
            false,
        )
        .expect("create");

    assert_eq!(
        service.session_status(id),
        Some(ImportSessionStatus::Created)
    );
    assert_eq!(service.session_status(Uuid::new_v4()), None);
}

// =========================================================================
// Export session tests
// =========================================================================

// -- Export helpers --

fn valid_export_password() -> SecureStr {
    SecureStr::new("export_pass123".to_string())
}

fn valid_export_path(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("export.okb")
}

fn sample_export_records() -> Vec<super::export::ExportRecord> {
    vec![super::export::ExportRecord {
        id: Uuid::new_v4().to_string(),
        credential_type: "login".to_string(),
        name: "TestRecord".to_string(),
        username: Some("user@example.com".to_string()),
        password: Some("s3cret".to_string()),
        url: Some("https://example.com".to_string()),
        notes: None,
        totp: None,
        tags: Some(vec!["test".to_string()]),
        is_favorite: Some(false),
        expires_at: None,
        public_key: None,
        private_key: None,
        passphrase: None,
        app_id: None,
        secret_key: None,
    }]
}

// -- Test 16: create_export_session returns id --

#[test]
fn create_export_session_returns_id() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");

    let id = service
        .create_export_session(
            ExportScope::All,
            ExportFormat::Okb,
            valid_export_password(),
            valid_export_path(&dir),
        )
        .expect("create export session");

    assert_eq!(
        service.export_session_status(id),
        Some(ExportSessionStatus::Created)
    );
}

// -- Test 17: create_export_session rejects short password --

#[test]
fn create_export_session_rejects_short_password() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");
    let short_pw = SecureStr::new("1234567".to_string());

    let result = service.create_export_session(
        ExportScope::All,
        ExportFormat::Okb,
        short_pw,
        valid_export_path(&dir),
    );

    assert!(result.is_err());
    assert!(
        matches!(result.unwrap_err(), ImportExportError::InvalidPassword),
        "expected InvalidPassword for short password"
    );
}

// -- Test 18: create_export_session rejects non-.okb path --

#[test]
fn create_export_session_rejects_non_okb_path() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");
    let bad_path = dir.path().join("export.txt");

    let result = service.create_export_session(
        ExportScope::All,
        ExportFormat::Okb,
        valid_export_password(),
        bad_path,
    );

    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            ImportExportError::InvalidFormat(msg) if msg.contains(".okb")
        ),
        "expected InvalidFormat for non-.okb extension"
    );
}

// -- Test 19: execute_export writes file and completes --

#[test]
fn execute_export_writes_file_and_completes() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");
    let output_path = valid_export_path(&dir);

    let id = service
        .create_export_session(
            ExportScope::All,
            ExportFormat::Okb,
            valid_export_password(),
            output_path.clone(),
        )
        .expect("create session");

    let records = sample_export_records();
    let params = ExportParams {
        session_id: id,
        record_collector: Box::new(move || Ok(records)),
        vault_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
    };
    let (result_path, count) = service.execute_export(params).expect("execute export");

    assert_eq!(result_path, output_path);
    assert_eq!(count, 1, "record count should be 1");
    assert!(output_path.exists(), "output file should exist");
    assert_eq!(
        service.export_session_status(id),
        Some(ExportSessionStatus::Completed)
    );
}

// -- Test 20: execute_export rejects empty records --

#[test]
fn execute_export_rejects_empty_records() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");

    let id = service
        .create_export_session(
            ExportScope::All,
            ExportFormat::Okb,
            valid_export_password(),
            valid_export_path(&dir),
        )
        .expect("create session");

    let params = ExportParams {
        session_id: id,
        record_collector: Box::new(|| Ok(vec![])),
        vault_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
    };
    let result = service.execute_export(params);

    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            ImportExportError::VaultError(ref msg) if msg.contains("no records to export")
        ),
        "expected VaultError for empty records"
    );
}

// -- Test 21: execute_export rejects wrong status --

#[test]
fn execute_export_rejects_wrong_status() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");

    let id = service
        .create_export_session(
            ExportScope::All,
            ExportFormat::Okb,
            valid_export_password(),
            valid_export_path(&dir),
        )
        .expect("create session");

    // Execute once to complete the session.
    let records = sample_export_records();
    let params1 = ExportParams {
        session_id: id,
        record_collector: Box::new(move || Ok(records)),
        vault_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
    };
    service.execute_export(params1).expect("first export");

    // Try to execute again on completed session.
    let records2 = sample_export_records();
    let params2 = ExportParams {
        session_id: id,
        record_collector: Box::new(move || Ok(records2)),
        vault_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
    };
    let result = service.execute_export(params2);

    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            ImportExportError::InvalidSessionStatus { ref expected, .. }
                if expected == "Created"
        ),
        "expected InvalidSessionStatus for non-Created session"
    );
}

// -- Test 22: cancel_export_session changes status --

#[test]
fn cancel_export_session_changes_status() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");

    let id = service
        .create_export_session(
            ExportScope::All,
            ExportFormat::Okb,
            valid_export_password(),
            valid_export_path(&dir),
        )
        .expect("create session");

    service.cancel_export_session(id).expect("cancel");

    assert_eq!(
        service.export_session_status(id),
        Some(ExportSessionStatus::Failed)
    );
}

// -- Test 23: create_export_session with Csv format skips password validation --

#[test]
fn create_export_session_csv_skips_password_validation() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");
    let csv_path = dir.path().join("export.csv");

    let result = service.create_export_session(
        ExportScope::All,
        ExportFormat::Csv,
        SecureStr::new(String::new()), // empty password — should be OK for CSV
        csv_path,
    );

    assert!(result.is_ok(), "CSV session should accept empty password");
}

// -- Test 24: create_export_session with Csv rejects .okb path --

#[test]
fn create_export_session_csv_rejects_okb_path() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");
    let okb_path = dir.path().join("export.okb");

    let result = service.create_export_session(
        ExportScope::All,
        ExportFormat::Csv,
        SecureStr::new(String::new()),
        okb_path,
    );

    assert!(result.is_err(), "CSV session should reject .okb path");
}

// -- Test 25: execute_export with Csv writes valid CSV (integration) --

#[test]
fn execute_export_csv_writes_valid_csv() {
    let mut service = ImportExportServiceImpl::new();
    let dir = tempfile::tempdir().expect("create temp dir");
    let csv_path = dir.path().join("export.csv");

    let id = service
        .create_export_session(
            ExportScope::All,
            ExportFormat::Csv,
            SecureStr::new(String::new()),
            csv_path.clone(),
        )
        .expect("create CSV session");

    let records = sample_export_records();
    let params = ExportParams {
        session_id: id,
        record_collector: Box::new(move || Ok(records)),
        vault_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
    };
    let (result_path, count) = service.execute_export(params).expect("execute CSV export");

    assert_eq!(result_path, csv_path);
    assert_eq!(count, 1);
    assert!(csv_path.exists(), "CSV file should exist");

    // Verify output is valid CSV (not encrypted binary).
    let data = std::fs::read(&csv_path).expect("read CSV file");
    assert_eq!(
        &data[0..3],
        &[0xEF, 0xBB, 0xBF],
        "file should start with UTF-8 BOM"
    );

    let text = std::str::from_utf8(&data[3..]).expect("valid UTF-8");
    assert!(
        text.contains("credential_type"),
        "CSV should contain header"
    );
    assert!(
        text.contains("TestRecord"),
        "CSV should contain record name"
    );
    assert!(
        text.contains("user@example.com"),
        "CSV should contain record username"
    );
}
