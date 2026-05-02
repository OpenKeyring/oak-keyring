use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use chrono::Utc;
use uuid::Uuid;

use crate::commands::types::{CsvColumnMapping, ExportScope, ImportPreview, ImportSource};
use crate::errors::mapping::import_export::ImportExportError;
use crate::services::import_export::duplicate::{detect_duplicates, ExistingRecordKey};
use crate::services::import_export::mapping::map_parsed_item;
use crate::services::import_export::parser::FormatParserRegistry;
use crate::services::import_export::types::{
    ExportSession, ExportSessionStatus, ImportResult, ImportSession, ImportSessionStatus,
    MappedRecord, ValidationResult,
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
    pub(crate) import_sessions: HashMap<Uuid, ImportSession>,
    export_sessions: HashMap<Uuid, ExportSession>,
    parser_registry: FormatParserRegistry,
}

impl ImportExportService {
    pub fn new() -> Self {
        let mut registry = FormatParserRegistry::new();
        registry.register(Box::new(super::parsers::csv::CsvParser));
        registry.register(Box::new(super::parsers::bitwarden::BitwardenParser));
        registry.register(Box::new(super::parsers::keepass::KeePassParser));
        registry.register(Box::new(super::parsers::onepassword::OnePuxParser));
        registry.register(Box::new(super::parsers::onepassword::OpVaultParser));
        registry.register(Box::new(super::parsers::okb::OkbParser));

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
        import_as_notes: bool,
    ) -> Result<Uuid, ImportExportError> {
        let id = Uuid::new_v4();
        let session = ImportSession {
            id,
            source,
            file_path,
            decrypt_password,
            csv_mapping,
            import_as_notes,
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
        let import_as_notes = {
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
            session.import_as_notes
        };

        // 2. Transition to Importing.
        {
            let session = self
                .import_sessions
                .get_mut(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.status = ImportSessionStatus::Importing;
        }

        // 3. Extract mapped records and review items from session.
        let (records, review_records): (Vec<MappedRecord>, Vec<MappedRecord>) = {
            let session = self
                .import_sessions
                .get(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            let mapped = session.mapped_records.clone();

            // When import_as_notes is enabled, convert review_items into
            // additional MappedRecords so they are imported as note entries
            // instead of being silently skipped.
            let review_mapped = if import_as_notes {
                session
                    .validation_result
                    .as_ref()
                    .map(|vr| {
                        vr.review_items
                            .iter()
                            .map(|item| {
                                let mut fields = HashMap::new();
                                fields.insert("name".to_string(), item.name.clone());
                                MappedRecord {
                                    id: Uuid::new_v4(),
                                    credential_type: CredentialType::Login,
                                    fields,
                                    tags: Vec::new(),
                                    is_favorite: false,
                                    expires_at: None,
                                    source_item_id: item.name.clone(),
                                    notes: Some(format!("[Imported as notes] {}", item.reason)),
                                    is_duplicate: false,
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            (mapped, review_mapped)
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
        let mut reviewed: usize = 0;
        let mut skipped: usize = 0;
        let mut failed: usize = 0;

        // 5. Import non-duplicate records (including review_records as notes).
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

        // 5b. Import review_records as notes — count as reviewed.
        for record in &review_records {
            match vault_create_fn(
                record.credential_type,
                record.fields.clone(),
                record.tags.clone(),
            ) {
                Ok(_uuid) => reviewed += 1,
                Err(_reason) => failed += 1,
            }
        }

        let duration_ms = start.elapsed().as_millis() as u64;

        let result = ImportResult {
            imported,
            reviewed,
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
    // Export lifecycle
    // -----------------------------------------------------------------------

    /// Create a new export session in `Created` status.
    ///
    /// Validates password length (>= 8) and output path extension (.okb).
    /// Returns the session UUID for use in subsequent operations.
    pub fn create_export_session(
        &mut self,
        scope: ExportScope,
        export_password: SecureStr,
        output_path: PathBuf,
    ) -> Result<Uuid, ImportExportError> {
        // Validate password.
        super::export::validate_export_password(&export_password)?;

        // Validate output path.
        super::export::validate_export_path(&output_path)?;

        let id = Uuid::new_v4();
        let session = ExportSession {
            id,
            scope,
            export_password,
            output_path,
            status: ExportSessionStatus::Created,
            record_count: 0,
            encrypted_size: None,
            created_at: Utc::now(),
            completed_at: None,
        };
        self.export_sessions.insert(id, session);
        Ok(id)
    }

    /// Execute the export: collect records, serialize, encrypt, write.
    ///
    /// The `record_collector` closure fetches records from the vault, keeping
    /// the service decoupled from `VaultService`.
    pub fn execute_export<F>(
        &mut self,
        session_id: Uuid,
        record_collector: F,
        vault_id: &str,
    ) -> Result<(PathBuf, usize), ImportExportError>
    where
        F: FnOnce() -> Result<Vec<super::export::ExportRecord>, String>,
    {
        // 1. Verify session exists and status is Created.
        {
            let session = self
                .export_sessions
                .get(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            if session.status != ExportSessionStatus::Created {
                return Err(ImportExportError::InvalidSessionStatus {
                    expected: "Created".to_string(),
                    actual: format!("{:?}", session.status),
                });
            }
        }

        // 2. Transition to Exporting.
        {
            let session = self
                .export_sessions
                .get_mut(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.status = ExportSessionStatus::Exporting;
        }

        // 3. Collect records via closure.
        let records = record_collector().map_err(ImportExportError::VaultError)?;

        // 4. Check records is not empty.
        if records.is_empty() {
            // Transition to Failed.
            if let Some(session) = self.export_sessions.get_mut(&session_id) {
                session.status = ExportSessionStatus::Failed;
            }
            return Err(ImportExportError::VaultError(
                "no records to export".to_string(),
            ));
        }

        let record_count = records.len();

        // 5. Build payload.
        let payload = super::export::ExportPayload {
            version: "1.0".to_string(),
            vault_id: vault_id.to_string(),
            exported_at: Utc::now().to_rfc3339(),
            records,
        };

        // 6. Encrypt and write. Borrow password directly from session to avoid
        //    creating an unprotected intermediate String copy (C1 fix).
        let (output_path, encrypted_size) = {
            let session = self
                .export_sessions
                .get(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;

            let path = session.output_path.clone();
            let size =
                super::export::encrypt_and_write_okb(&payload, &session.export_password, &path)?;
            (path, size)
        };

        // 7. Update session: Completed.
        {
            let session = self
                .export_sessions
                .get_mut(&session_id)
                .ok_or(ImportExportError::SessionNotFound(session_id))?;
            session.status = ExportSessionStatus::Completed;
            session.record_count = record_count;
            session.encrypted_size = Some(encrypted_size);
            session.completed_at = Some(Utc::now());
        }

        Ok((output_path, record_count))
    }

    /// Cancel an export session.
    pub fn cancel_export_session(&mut self, session_id: Uuid) -> Result<(), ImportExportError> {
        let session = self
            .export_sessions
            .get_mut(&session_id)
            .ok_or(ImportExportError::SessionNotFound(session_id))?;
        session.status = ExportSessionStatus::Failed;
        Ok(())
    }

    /// Get export session status.
    pub fn export_session_status(&self, session_id: Uuid) -> Option<ExportSessionStatus> {
        self.export_sessions.get(&session_id).map(|s| s.status)
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
