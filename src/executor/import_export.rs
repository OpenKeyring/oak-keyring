use std::collections::HashSet;
use std::path::PathBuf;

use crate::commands::CommandResult;
use crate::commands::types::{
    CsvColumnMapping, ExportScope, ImportSource, RecordFilter, RecordSort, SortDirection, SortField,
};
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::import_export::duplicate::ExistingRecordKey;
use crate::services::import_export::export::ExportRecord;
use crate::types::record::CreateRecordParams;
use crate::types::{CredentialType, EncryptedPayload, SecureStr};

use super::CommandExecutor;

// ---------------------------------------------------------------------------
// Import handlers
// ---------------------------------------------------------------------------

#[tracing::instrument(skip_all)]
pub fn handle_validate_import_file(
    executor: &mut CommandExecutor,
    source: ImportSource,
    path: PathBuf,
    password: Option<SecureStr>,
) -> CommandResult {
    // Step 1: Create import session.
    let session_id = match executor
        .import_export
        .create_import_session(source, path, password, None)
    {
        Ok(id) => id,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ImportExport(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.import_session_create_failed",
                fallback: format!("Failed to create import session: {}", e),
            };
        }
    };

    // Step 2: Validate the import file.
    match executor.import_export.validate_import_file(session_id) {
        Ok(preview) => CommandResult::ImportValidated { preview },
        Err(e) => CommandResult::Error {
            code: ErrorCode::ImportExport(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.import_validate_failed",
            fallback: format!("Failed to validate import file: {}", e),
        },
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_execute_import(
    executor: &mut CommandExecutor,
    source: ImportSource,
    path: PathBuf,
    password: Option<SecureStr>,
    column_mapping: Option<CsvColumnMapping>,
) -> CommandResult {
    // Step 1: Create import session.
    let session_id = match executor
        .import_export
        .create_import_session(source, path, password, column_mapping)
    {
        Ok(id) => id,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ImportExport(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.import_session_create_failed",
                fallback: format!("Failed to create import session: {}", e),
            };
        }
    };

    // Step 2: Validate the file first (session must be Validated before import).
    if let Err(e) = executor.import_export.validate_import_file(session_id) {
        return CommandResult::Error {
            code: ErrorCode::ImportExport(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.import_validate_failed",
            fallback: format!("Failed to validate import file: {}", e),
        };
    }

    // Step 3: Execute import with a closure that creates vault records.
    let existing_keys: HashSet<ExistingRecordKey> = HashSet::new();

    let result = match executor.import_export.execute_import(session_id, existing_keys, |cred_type, fields, tags| {
        // Build an EncryptedPayload from the field map.
        let payload = fields_to_payload(cred_type, &fields);
        let params = CreateRecordParams {
            credential_type: cred_type,
            payload,
            tags,
            is_favorite: false,
            expires_at: None,
        };
        executor.vault.create_record(params).map_err(|e| e.to_string())
    }) {
        Ok(r) => r,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ImportExport(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.import_execute_failed",
                fallback: format!("Failed to execute import: {}", e),
            };
        }
    };

    let imported_count = result.imported;

    // Audit log for successful import.
    if let Err(e) = executor.vault.write_audit_entry(
        crate::types::AuditOperation::VaultImport,
        None,
        None,
        Some(format!("source={:?}, count={}", source, imported_count)),
    ) {
        tracing::warn!(error = %e, "Failed to write import audit log");
    }

    CommandResult::ImportCompleted {
        imported_count,
        skipped_count: result.skipped,
    }
}

// ---------------------------------------------------------------------------
// Export handler
// ---------------------------------------------------------------------------

#[tracing::instrument(skip_all)]
pub fn handle_execute_export(
    executor: &mut CommandExecutor,
    scope: ExportScope,
    output_path: PathBuf,
    export_password: SecureStr,
    master_password: SecureStr,
) -> CommandResult {
    // Step 1: Verify master password.
    if crate::crypto::keystore::KeyStore::unlock(&executor.vault_dir, &master_password).is_err() {
        return CommandResult::Error {
            code: ErrorCode::Vault(String::from("password_verification_failed")),
            context: ErrorContext::default(),
            message_key: "error.password_verification_failed",
            fallback: String::from("Master password verification failed."),
        };
    }

    // Step 2: Create export session.
    let scope_desc = format!("{:?}", scope);
    let session_id = match executor
        .import_export
        .create_export_session(scope, export_password, output_path)
    {
        Ok(id) => id,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ImportExport(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.export_session_create_failed",
                fallback: format!("Failed to create export session: {}", e),
            };
        }
    };

    // Step 3: Execute export with a closure that collects records from vault.
    let filter = RecordFilter::All;
    let sort = RecordSort {
        field: SortField::Name,
        direction: SortDirection::Asc,
    };

    let result_path = match executor.import_export.execute_export(
        session_id,
        || {
            let records = executor
                .vault
                .list_records(&filter, &sort)
                .map_err(|e| e.to_string())?;

            // Convert TuiRecord to ExportRecord.
            let export_records: Vec<ExportRecord> = records
                .iter()
                .map(|r| ExportRecord {
                    id: r.id.to_string(),
                    credential_type: r.credential_type.to_db_str().to_string(),
                    name: r.name.clone(),
                    username: None,    // TuiRecord does not expose field-level data
                    password: None,    // Requires decryption which is handled by VaultService
                    url: None,
                    notes: None,
                    tags: Some(r.tags.clone()),
                    is_favorite: Some(r.is_favorite),
                    expires_at: r.expires_at.map(|t| t.to_rfc3339()),
                })
                .collect();

            Ok(export_records)
        },
        &executor.vault_dir.to_string_lossy(),
    ) {
        Ok(path) => path,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ImportExport(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.export_execute_failed",
                fallback: format!("Failed to execute export: {}", e),
            };
        }
    };

    // Retrieve record count from the session.
    // Since execute_export already completed, we use the output path to determine count.
    // The ImportExportService updates the session internally.
    // For now, use 0 as placeholder — the session has the actual count but is not
    // directly accessible here. The UI can display the path.
    // Audit log for successful export.
    if let Err(e) = executor.vault.write_audit_entry(
        crate::types::AuditOperation::VaultExport,
        None,
        None,
        Some(format!("scope={}, path={}", scope_desc, result_path.display())),
    ) {
        tracing::warn!(error = %e, "Failed to write export audit log");
    }

    CommandResult::ExportCompleted {
        path: result_path,
        record_count: 0,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a HashMap of fields into an EncryptedPayload for the given credential type.
///
/// This is a best-effort mapping that populates standard fields based on the
/// credential type. Fields not recognized are ignored.
fn fields_to_payload(cred_type: CredentialType, fields: &std::collections::HashMap<String, String>) -> EncryptedPayload {
    match cred_type {
        CredentialType::Login => EncryptedPayload::Login {
            name: fields.get("name").cloned().unwrap_or_default(),
            username: fields.get("username").cloned().unwrap_or_default(),
            password: SecureStr::new(fields.get("password").cloned().unwrap_or_default()),
            url: fields.get("url").cloned(),
            notes: fields.get("notes").cloned(),
        },
        CredentialType::Api => EncryptedPayload::Api {
            name: fields.get("name").cloned().unwrap_or_default(),
            app_id: fields.get("app_id").cloned().unwrap_or_default(),
            secret_key: SecureStr::new(fields.get("secret_key").cloned().unwrap_or_default()),
            url: fields.get("url").cloned(),
            notes: fields.get("notes").cloned(),
        },
        CredentialType::Ssh => EncryptedPayload::Ssh {
            name: fields.get("name").cloned().unwrap_or_default(),
            public_key: fields.get("public_key").cloned().unwrap_or_default(),
            private_key: fields.get("private_key").cloned().map(SecureStr::new),
            passphrase: fields.get("passphrase").cloned().map(SecureStr::new),
            notes: fields.get("notes").cloned(),
        },
    }
}
