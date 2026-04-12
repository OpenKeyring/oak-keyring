pub mod duplicate;
pub mod mapping;
pub mod parser;
pub mod parsers;
pub mod types;
pub mod validation;
// More modules will be added by later tasks:
// pub mod export;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

use crate::commands::types::{CsvColumnMapping, ImportPreview, ImportSource};
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::duplicate::{detect_duplicates, ExistingRecordKey};
use crate::services::import_export::mapping::map_parsed_item;
use crate::services::import_export::parser::FormatParserRegistry;
use crate::services::import_export::types::{
    ImportResult, ImportSession, ImportSessionStatus, MappedRecord, ValidationResult,
};
use crate::services::import_export::validation::{get_rules_for_format, validate_items};
use crate::types::{CredentialType, SecureStr};

// ---------------------------------------------------------------------------
// ImportExportService
// ---------------------------------------------------------------------------

/// Coordinates the full import pipeline: parsing, validation, mapping,
/// duplicate detection, and vault insertion.
///
/// Import sessions progress through a well-defined lifecycle:
/// `Created` -> `Validating` -> `Validated` -> `Importing` -> `Completed`
///
/// Any step can transition to `Cancelled` via [`cancel_import`](Self::cancel_import).
/// Fatal errors during validation or import transition to `Failed`.
pub struct ImportExportService {
    import_sessions: HashMap<Uuid, ImportSession>,
    // Export sessions will be used by Task 16 (export flow).
    #[allow(dead_code)]
    export_sessions: HashMap<Uuid, types::ExportSession>,
    parser_registry: FormatParserRegistry,
}

impl ImportExportService {
    pub fn new() -> Self {
        let mut registry = FormatParserRegistry::new();
        registry.register(Box::new(parsers::csv::CsvParser));
        registry.register(Box::new(parsers::bitwarden::BitwardenParser));
        registry.register(Box::new(parsers::keepass::KeePassParser));
        registry.register(Box::new(parsers::onepassword::OnePuxParser));
        registry.register(Box::new(parsers::onepassword::OpVaultParser));
        registry.register(Box::new(parsers::okb::OkbParser));

        Self {
            import_sessions: HashMap::new(),
            export_sessions: HashMap::new(),
            parser_registry: registry,
        }
    }

    // -----------------------------------------------------------------------
    // Import lifecycle
    // -----------------------------------------------------------------------

    /// Create a new import session in `Created` status.
    ///
    /// Returns the session UUID for use in subsequent operations.
    pub fn create_import_session(
        &mut self,
        source: ImportSource,
        file_path: PathBuf,
        decrypt_password: Option<SecureStr>,
        csv_mapping: Option<CsvColumnMapping>,
    ) -> Result<Uuid, ImportExportError> {
        let id = Uuid::new_v4();
        let session = ImportSession {
            id,
            source,
            file_path,
            decrypt_password,
            csv_mapping,
            status: ImportSessionStatus::Created,
            validation_result: None,
            mapped_records: Vec::new(),
            failed_items: Vec::new(),
            created_at: Utc::now(),
        };
        self.import_sessions.insert(id, session);
        Ok(id)
    }

    /// Parse, validate, and map the import file.
    ///
    /// Must be called when session is in `Created` status. On success the
    /// session transitions to `Validated` and a preview is available via
    /// [`get_import_preview`](Self::get_import_preview).
    pub fn validate_import_file(
        &mut self,
        session_id: Uuid,
    ) -> Result<ImportPreview, ImportExportError> {
        // 1. Extract data from session (drop borrow before calling parser).
        let (source, file_path, csv_mapping_ref) = {
            let session = self
                .import_sessions
                .get(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;

            if session.status != ImportSessionStatus::Created {
                return Err(ImportExportError::InvalidSessionStatus {
                    expected: "Created".to_string(),
                    actual: format!("{:?}", session.status),
                });
            }
            (
                session.source,
                session.file_path.clone(),
                session.csv_mapping.clone(),
            )
        };

        // 2. Transition to Validating.
        {
            let session = self
                .import_sessions
                .get_mut(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.status = ImportSessionStatus::Validating;
        }

        // 3. Get parser and parse the file.
        let parser =
            self.parser_registry
                .get(source)
                .ok_or(ImportExportError::UnsupportedFormat(format!(
                    "{:?}",
                    source
                )))?;

        // Borrow password from session for the parse call.
        let parsed_items = {
            let session = self
                .import_sessions
                .get(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            let pw_ref = session.decrypt_password.as_ref().map(|s| s.get() as &str);
            // We need Option<&SecureStr> for the parser.
            // Unfortunately we can't directly get Option<&SecureStr> because
            // SecureStr panics on clone. Pass None for password reference
            // and handle password separately through a temporary.
            let _ = pw_ref; // Password access is handled below.
            let pw: Option<&SecureStr> = session.decrypt_password.as_ref();

            parser.parse(&file_path, pw, csv_mapping_ref.as_ref())
        };

        let parsed_items = match parsed_items {
            Ok(items) => items,
            Err(e) => {
                if let Some(session) = self.import_sessions.get_mut(&session_id) {
                    session.status = ImportSessionStatus::Failed;
                }
                return Err(e);
            }
        };

        // 4. Validate parsed items against format-specific rules.
        let rules = get_rules_for_format(source);
        let summary = validate_items(&parsed_items, &rules);

        // 5. Map parsed items to vault records.
        let mapped_records: Vec<MappedRecord> = parsed_items
            .iter()
            .map(|item| map_parsed_item(item, source))
            .collect();

        // 6. Build validation result.
        let validation_result = ValidationResult {
            total_items: summary.total_items,
            importable: summary.importable,
            needs_review: summary.needs_review,
            failed: summary.failed,
            review_items: summary.review_items.clone(),
            failed_items: summary.failed_items.clone(),
        };

        let preview = ImportPreview {
            importable: summary.importable,
            needs_review: summary.needs_review,
            failed: summary.failed,
            review_items: summary.review_items,
            failed_items: summary.failed_items,
        };

        // 7. Update session.
        {
            let session = self
                .import_sessions
                .get_mut(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.status = ImportSessionStatus::Validated;
            session.validation_result = Some(validation_result);
            session.mapped_records = mapped_records;
        }

        Ok(preview)
    }

    /// Return the validation preview for a validated session.
    pub fn get_import_preview(&self, session_id: Uuid) -> Result<ImportPreview, ImportExportError> {
        let session = self
            .import_sessions
            .get(&session_id)
            .ok_or(ImportExportError::SessionNotFound(session_id))?;

        let vr =
            session
                .validation_result
                .as_ref()
                .ok_or(ImportExportError::InvalidSessionStatus {
                    expected: "Validated".to_string(),
                    actual: format!("{:?}", session.status),
                })?;

        Ok(ImportPreview {
            importable: vr.importable,
            needs_review: vr.needs_review,
            failed: vr.failed,
            review_items: vr.review_items.clone(),
            failed_items: vr.failed_items.clone(),
        })
    }

    /// Execute the import: create vault records for validated, non-duplicate items.
    ///
    /// The `vault_create_fn` closure handles actual record creation. This keeps
    /// the service decoupled from `VaultService`.
    ///
    /// **Closure signature:**
    /// `(CredentialType, HashMap<String, String>, Vec<String>) -> Result<Uuid, String>`
    pub fn execute_import<F>(
        &mut self,
        session_id: Uuid,
        existing_keys: HashSet<ExistingRecordKey>,
        mut vault_create_fn: F,
    ) -> Result<ImportResult, ImportExportError>
    where
        F: FnMut(CredentialType, HashMap<String, String>, Vec<String>) -> Result<Uuid, String>,
    {
        // 1. Verify session status is Validated.
        {
            let session = self
                .import_sessions
                .get(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;

            if session.status != ImportSessionStatus::Validated {
                return Err(ImportExportError::InvalidSessionStatus {
                    expected: "Validated".to_string(),
                    actual: format!("{:?}", session.status),
                });
            }
        }

        // 2. Transition to Importing.
        {
            let session = self
                .import_sessions
                .get_mut(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.status = ImportSessionStatus::Importing;
        }

        // 3. Extract mapped records from session.
        let records: Vec<MappedRecord> = {
            let session = self
                .import_sessions
                .get(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.mapped_records.clone()
        };

        // 4. Run duplicate detection against provided existing keys.
        // Build ParsedItems from mapped records for duplicate detection.
        let parsed_for_dup: Vec<crate::services::import_export::parser::ParsedItem> = records
            .iter()
            .map(|r| {
                let mut fields = r.fields.clone();
                fields.insert(
                    "credential_type".to_string(),
                    r.credential_type.to_db_str().to_string(),
                );
                crate::services::import_export::parser::ParsedItem {
                    source_id: r.source_item_id.clone(),
                    fields,
                    tags: r.tags.clone(),
                }
            })
            .collect();

        let dup_flags = detect_duplicates(&parsed_for_dup, &existing_keys);

        let start = std::time::Instant::now();
        let mut imported: usize = 0;
        let mut skipped: usize = 0;
        let mut failed: usize = 0;

        // 5. Import non-duplicate records.
        for (i, record) in records.iter().enumerate() {
            if dup_flags[i] {
                skipped += 1;
                continue;
            }

            match vault_create_fn(
                record.credential_type,
                record.fields.clone(),
                record.tags.clone(),
            ) {
                Ok(_uuid) => imported += 1,
                Err(_reason) => failed += 1,
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let result = ImportResult {
            imported,
            reviewed: 0,
            skipped,
            failed,
            duration_ms,
        };

        // 6. Transition to Completed.
        {
            let session = self
                .import_sessions
                .get_mut(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.status = ImportSessionStatus::Completed;
        }

        Ok(result)
    }

    /// Cancel an in-progress import session.
    pub fn cancel_import(&mut self, session_id: Uuid) -> Result<(), ImportExportError> {
        let session = self
            .import_sessions
            .get_mut(&session_id)
            .ok_or(ImportExportError::SessionNotFound(session_id))?;
        session.status = ImportSessionStatus::Cancelled;
        Ok(())
    }

    /// Remove a session from the internal map.
    pub fn cleanup_session(&mut self, session_id: Uuid) -> Result<(), ImportExportError> {
        self.import_sessions
            .remove(&session_id)
            .ok_or(ImportExportError::SessionNotFound(session_id))?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Introspection
    // -----------------------------------------------------------------------

    /// Check whether a parser is registered for the given format.
    pub fn has_parser(&self, source: ImportSource) -> bool {
        self.parser_registry.get(source).is_some()
    }

    /// Return the number of active import sessions.
    pub fn session_count(&self) -> usize {
        self.import_sessions.len()
    }

    /// Return the status of a session, if it exists.
    pub fn session_status(&self, session_id: Uuid) -> Option<ImportSessionStatus> {
        self.import_sessions.get(&session_id).map(|s| s.status)
    }
}

impl Default for ImportExportService {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Simple vault-create closure that always succeeds.
    fn success_create_fn(
    ) -> impl FnMut(CredentialType, HashMap<String, String>, Vec<String>) -> Result<Uuid, String>
    {
        move |_ct, _fields, _tags| Ok(Uuid::new_v4())
    }

    // -- Test 1: Create session ----------------------------------------------

    #[test]
    fn create_session_has_created_status() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create session");

        let session = service.import_sessions.get(&id).expect("session exists");
        assert_eq!(session.status, ImportSessionStatus::Created);
        assert_eq!(session.source, ImportSource::Csv);
    }

    // -- Test 2: Validate CSV ------------------------------------------------

    #[test]
    fn validate_csv_produces_correct_preview() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create session");

        let preview = service.validate_import_file(id).expect("validate");

        assert_eq!(preview.importable, 3);
        assert_eq!(preview.failed, 0);
        assert!(preview.review_items.is_empty());
        assert!(preview.failed_items.is_empty());

        // Session should now be Validated.
        let session = service.import_sessions.get(&id).expect("session exists");
        assert_eq!(session.status, ImportSessionStatus::Validated);
    }

    // -- Test 3: Full import CSV ----------------------------------------------

    #[test]
    fn full_import_csv_produces_correct_result() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create session");

        service.validate_import_file(id).expect("validate");

        let existing: HashSet<ExistingRecordKey> = HashSet::new();
        let result = service
            .execute_import(id, existing, success_create_fn())
            .expect("execute import");

        assert_eq!(result.imported, 3);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed, 0);

        // Session should be Completed.
        let session = service.import_sessions.get(&id).expect("session exists");
        assert_eq!(session.status, ImportSessionStatus::Completed);
    }

    // -- Test 4: Session not found --------------------------------------------

    #[test]
    fn operations_on_invalid_uuid_return_session_not_found() {
        let mut service = ImportExportService::new();
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
        let result = service.execute_import(bogus_id, existing, success_create_fn());
        assert!(result.is_err());

        let result = service.cancel_import(bogus_id);
        assert!(result.is_err());

        let result = service.cleanup_session(bogus_id);
        assert!(result.is_err());
    }

    // -- Test 5: Wrong status ------------------------------------------------

    #[test]
    fn execute_on_created_session_returns_invalid_status() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create session");

        // Do NOT validate first — attempt to import directly.
        let existing: HashSet<ExistingRecordKey> = HashSet::new();
        let result = service.execute_import(id, existing, success_create_fn());

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
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create session");

        service.cancel_import(id).expect("cancel");

        let session = service.import_sessions.get(&id).expect("session exists");
        assert_eq!(session.status, ImportSessionStatus::Cancelled);
    }

    // -- Test 7: Cleanup removes session --------------------------------------

    #[test]
    fn cleanup_removes_session_from_map() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
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
        let service = ImportExportService::new();

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
        let mut service = ImportExportService::new();
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
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
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

        let result = service
            .execute_import(id, existing, success_create_fn())
            .expect("execute import");

        assert_eq!(result.imported, 2);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.failed, 0);
    }

    // -- Test 11: Import with vault_create_fn failure --------------------------

    #[test]
    fn import_tracks_vault_create_failures() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create session");

        service.validate_import_file(id).expect("validate");

        let existing: HashSet<ExistingRecordKey> = HashSet::new();
        let mut call_count = 0;
        let fail_fn = |_: CredentialType, _: HashMap<String, String>, _: Vec<String>| {
            call_count += 1;
            if call_count > 1 {
                Ok(Uuid::new_v4())
            } else {
                Err("vault error".to_string())
            }
        };

        let result = service
            .execute_import(id, existing, fail_fn)
            .expect("execute");

        assert_eq!(result.imported, 2);
        assert_eq!(result.failed, 1);
    }

    // -- Test 12: Get import preview after validate ----------------------------

    #[test]
    fn get_import_preview_returns_validation_result() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create session");

        service.validate_import_file(id).expect("validate");

        let preview = service.get_import_preview(id).expect("preview");
        assert_eq!(preview.importable, 3);
    }

    // -- Test 13: Validate on already-validated session fails -----------------

    #[test]
    fn validate_on_validated_session_returns_invalid_status() {
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
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
        let mut service = ImportExportService::new();
        let f = create_csv_file(simple_csv_content());

        let id = service
            .create_import_session(
                ImportSource::Csv,
                f.path().to_path_buf(),
                None,
                Some(default_csv_mapping()),
            )
            .expect("create");

        assert_eq!(
            service.session_status(id),
            Some(ImportSessionStatus::Created)
        );
        assert_eq!(service.session_status(Uuid::new_v4()), None);
    }
}
