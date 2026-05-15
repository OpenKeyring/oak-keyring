use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::commands::types::{
    CsvColumnMapping, ExportFormat, ExportScope, ImportSource, RecordFilter, RecordSort,
    SkipReason, SortDirection, SortField,
};
use crate::commands::{CommandResult, Message};
use crate::errors::{ErrorCode, ErrorContext, ServiceError};
use crate::services::import_export::duplicate::ExistingRecordKey;
use crate::services::import_export::export::ExportRecord;
use crate::services::import_export::parser::FormatParser;
use crate::services::import_export::types::ImportResult;
use crate::types::record::{CreateRecordParams, DecryptedRecord};
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
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("import_validate");
    }

    // Step 1: Create import session.
    let session_id = match executor
        .import_export
        .create_import_session(source, path, password, None, false)
    {
        Ok(id) => id,
        Err(e) => {
            let err: &dyn ServiceError = &e;
            return CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.import_session_create_failed",
                fallback: format!("Failed to create import session: {}", e),
            };
        }
    };

    // Step 2: Validate the import file.
    match executor.import_export.validate_import_file(session_id) {
        Ok(preview) => CommandResult::ImportValidated {
            session_id,
            preview,
        },
        Err(e) => {
            let err: &dyn ServiceError = &e;
            CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.import_validate_failed",
                fallback: format!("Failed to validate import file: {}", e),
            }
        }
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_execute_import(
    executor: &mut CommandExecutor,
    session_id: Option<uuid::Uuid>,
    source: ImportSource,
    path: PathBuf,
    password: Option<SecureStr>,
    column_mapping: Option<CsvColumnMapping>,
    import_as_notes: bool,
) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("import_execute");
    }

    let session_id = if let Some(id) = session_id {
        if let Some(session) = executor.import_export.import_sessions.get_mut(&id) {
            session.import_as_notes = import_as_notes;
        }
        id
    } else {
        let id = match executor.import_export.create_import_session(
            source,
            path,
            password,
            column_mapping,
            import_as_notes,
        ) {
            Ok(id) => id,
            Err(e) => {
                let err: &dyn ServiceError = &e;
                return CommandResult::Error {
                    code: err.to_error_code(),
                    context: err.to_error_context(),
                    message_key: "error.import_session_create_failed",
                    fallback: format!("Failed to create import session: {}", e),
                };
            }
        };

        if let Err(e) = executor.import_export.validate_import_file(id) {
            let err: &dyn ServiceError = &e;
            return CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.import_validate_failed",
                fallback: format!("Failed to validate import file: {}", e),
            };
        }
        id
    };

    // Step 3: Execute import with a closure that creates vault records.
    let existing_keys: HashSet<ExistingRecordKey> = HashSet::new();
    let import_cancel = executor.cancel_token().clone();
    let progress_tx = executor.result_tx.clone();

    let result = match executor.import_export.execute_import(
        session_id,
        existing_keys,
        |cred_type, fields, tags| {
            if import_cancel.is_cancelled() {
                return Err("cancelled".to_string());
            }
            // Build an EncryptedPayload from the field map.
            let payload = fields_to_payload(cred_type, &fields);
            let params = CreateRecordParams {
                credential_type: cred_type,
                payload,
                tags,
                is_favorite: false,
                expires_at: None,
            };
            executor
                .vault
                .create_record(params)
                .map_err(|e| e.to_string())
        },
        Some(move |current, total, name: &str| {
            let _ = progress_tx.try_send(Message::ImportProgress {
                current,
                total,
                current_name: name.to_string(),
            });
        }),
    ) {
        Ok(r) => {
            if executor.cancel_token().is_cancelled() {
                return CommandResult::cancelled("import_execute");
            }
            r
        }
        Err(e) => {
            if executor.cancel_token().is_cancelled() {
                return CommandResult::cancelled("import_execute");
            }
            let err: &dyn ServiceError = &e;
            return CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.import_execute_failed",
                fallback: format!("Failed to execute import: {}", e),
            };
        }
    };

    let imported_count = result.imported;
    let reviewed_count = result.reviewed;
    let failed_count = result.failed;

    // Audit log for successful import.
    if let Err(e) = executor.vault.write_audit_entry(
        crate::types::AuditOperation::VaultImport,
        None,
        None,
        Some(format!(
            "source={:?}, imported={}, reviewed={}, failed={}, skipped={}",
            source, imported_count, reviewed_count, failed_count, result.skipped
        )),
    ) {
        tracing::warn!(error = %e, "Failed to write import audit log");
    }

    // Schedule a full health scan to evaluate newly imported records.
    if imported_count > 0 {
        if let Err(e) = executor
            .internal_tx
            .try_send(crate::commands::InternalCommand::ScheduleHealthCheck { force: true })
        {
            tracing::warn!(error = %e, "Failed to schedule post-import health scan");
        }
    }

    CommandResult::ImportCompleted {
        imported_count,
        reviewed_count,
        skipped_count: result.skipped,
        failed_count,
        skip_breakdown: build_skip_breakdown(&result),
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
    format: ExportFormat,
) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("export_execute");
    }

    // Step 1: Verify master password.
    if crate::crypto::keystore::KeyStore::unlock(&executor.vault_dir, &master_password).is_err() {
        return CommandResult::Error {
            code: ErrorCode::ExecutorMasterPasswordRequired,
            context: ErrorContext::default(),
            message_key: "error.password_verification_failed",
            fallback: String::from("Master password verification failed."),
        };
    }

    // Step 2: Create export session.
    let scope_desc = format!("{:?}", scope);
    let session_id = match executor.import_export.create_export_session(
        scope,
        format,
        export_password,
        output_path,
    ) {
        Ok(id) => id,
        Err(e) => {
            let err: &dyn ServiceError = &e;
            return CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
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

    let cancel_token = executor.cancel_token().clone();

    let (result_path, record_count) = match executor.import_export.execute_export(
        session_id,
        || {
            let records = executor
                .vault
                .list_records(&filter, &sort)
                .map_err(|e| e.to_string())?;

            // Decrypt each record fully to populate export fields.
            let mut export_records = Vec::with_capacity(records.len());
            for r in &records {
                if cancel_token.is_cancelled() {
                    return Err("cancelled".to_string());
                }
                let decrypted = executor
                    .vault
                    .get_decrypted_record(r.id)
                    .map_err(|e| e.to_string())?;
                export_records.push(decrypted_record_to_export(&decrypted));
            }

            Ok(export_records)
        },
        &executor.vault_dir.to_string_lossy(),
    ) {
        Ok(result) => {
            if cancel_token.is_cancelled() {
                return CommandResult::cancelled("export_execute");
            }
            result
        }
        Err(e) => {
            if cancel_token.is_cancelled() {
                return CommandResult::cancelled("export_execute");
            }
            let err: &dyn ServiceError = &e;
            return CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.export_execute_failed",
                fallback: format!("Failed to execute export: {}", e),
            };
        }
    };

    // Audit log for successful export.
    if let Err(e) = executor.vault.write_audit_entry(
        crate::types::AuditOperation::VaultExport,
        None,
        None,
        Some(format!(
            "scope={}, path={}, count={}",
            scope_desc,
            result_path.display(),
            record_count
        )),
    ) {
        tracing::warn!(error = %e, "Failed to write export audit log");
    }

    CommandResult::ExportCompleted {
        path: result_path,
        record_count,
        format,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a fully decrypted record to the export format.
///
/// Field mapping uses type-specific fields:
/// - Login: username, password, url, notes
/// - Api: app_id, secret_key, url, notes (username/password are None)
/// - Ssh: public_key, private_key, passphrase, notes (username/password are None)
fn decrypted_record_to_export(record: &DecryptedRecord) -> ExportRecord {
    match record {
        DecryptedRecord::Login {
            id,
            is_favorite,
            expires_at,
            tags,
            name,
            username,
            password,
            url,
            notes,
            ..
        } => ExportRecord {
            id: id.to_string(),
            credential_type: CredentialType::Login.to_db_str().to_string(),
            name: name.clone(),
            username: Some(username.clone()),
            password: Some(password.expose().to_string()),
            url: url.clone(),
            notes: notes.clone(),
            tags: Some(tags.clone()),
            is_favorite: Some(*is_favorite),
            expires_at: expires_at.map(|t| t.to_rfc3339()),
            public_key: None,
            private_key: None,
            passphrase: None,
            app_id: None,
            secret_key: None,
        },
        DecryptedRecord::Api {
            id,
            is_favorite,
            expires_at,
            tags,
            name,
            app_id,
            secret_key,
            url,
            notes,
            ..
        } => ExportRecord {
            id: id.to_string(),
            credential_type: CredentialType::Api.to_db_str().to_string(),
            name: name.clone(),
            username: None,
            password: None,
            url: url.clone(),
            notes: notes.clone(),
            tags: Some(tags.clone()),
            is_favorite: Some(*is_favorite),
            expires_at: expires_at.map(|t| t.to_rfc3339()),
            public_key: None,
            private_key: None,
            passphrase: None,
            app_id: Some(app_id.clone()),
            secret_key: Some(secret_key.expose().to_string()),
        },
        DecryptedRecord::Ssh {
            id,
            is_favorite,
            expires_at,
            tags,
            name,
            public_key,
            private_key,
            passphrase,
            notes,
            ..
        } => ExportRecord {
            id: id.to_string(),
            credential_type: CredentialType::Ssh.to_db_str().to_string(),
            name: name.clone(),
            username: None,
            password: None,
            url: None,
            notes: notes.clone(),
            tags: Some(tags.clone()),
            is_favorite: Some(*is_favorite),
            expires_at: expires_at.map(|t| t.to_rfc3339()),
            public_key: Some(public_key.clone()),
            private_key: private_key.as_ref().map(|pk| pk.expose().to_string()),
            passphrase: passphrase.as_ref().map(|p| p.expose().to_string()),
            app_id: None,
            secret_key: None,
        },
    }
}

/// Convert a HashMap of fields into an EncryptedPayload for the given credential type.
///
/// This is a best-effort mapping that populates standard fields based on the
/// credential type. Fields not recognized are ignored.
fn fields_to_payload(
    cred_type: CredentialType,
    fields: &std::collections::HashMap<String, String>,
) -> EncryptedPayload {
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

/// Build a `SkipReason` → count breakdown from an `ImportResult`.
fn build_skip_breakdown(result: &ImportResult) -> HashMap<SkipReason, usize> {
    let mut breakdown = HashMap::new();
    if result.skipped > 0 {
        breakdown.insert(SkipReason::Duplicate, result.skipped);
    }
    if result.validation_failed > 0 {
        breakdown.insert(SkipReason::ValidationFailed, result.validation_failed);
    }
    breakdown
}

/// Restore vault.db from a local .okb backup file.
///
/// Parses the encrypted OKB file using the export password, imports all records
/// into the vault, then reopens the vault as file-backed.
pub async fn handle_restore_database_from_okb(
    executor: &mut CommandExecutor,
    path: PathBuf,
    password: SecureStr,
    master_password: Option<SecureStr>,
) -> CommandResult {
    // Path guards
    if path.as_os_str().is_empty() {
        return CommandResult::Error {
            code: ErrorCode::DataEmptyField,
            context: ErrorContext::default(),
            message_key: "error.okb_path_empty",
            fallback: "Enter a .okb path.".to_string(),
        };
    }
    if path.extension().and_then(|e| e.to_str()) != Some("okb") {
        return CommandResult::Error {
            code: ErrorCode::ImportFileFormatInvalid,
            context: ErrorContext::default(),
            message_key: "error.okb_invalid_extension",
            fallback: "Path must end with .okb.".to_string(),
        };
    }
    if !path.exists() {
        return CommandResult::Error {
            code: ErrorCode::ImportFileUnreadable,
            context: ErrorContext::default(),
            message_key: "error.okb_missing",
            fallback: format!("Backup file does not exist: {}", path.display()),
        };
    }

    // Parse the OKB file with the export password.
    let parser = crate::services::import_export::parsers::okb::OkbParser;
    let items = match parser.parse(&path, Some(&password), None) {
        Ok(items) => items,
        Err(e) => {
            drop(password);
            return CommandResult::Error {
                code: ErrorCode::ImportFileUnreadable,
                context: ErrorContext::default(),
                message_key: "error.okb_decrypt_failed",
                fallback: format!("Failed to decrypt .okb backup: {}", e),
            };
        }
    };
    // Export password no longer needed — drop immediately.
    drop(password);

    if items.is_empty() {
        return CommandResult::Error {
            code: ErrorCode::ImportFileFormatInvalid,
            context: ErrorContext::default(),
            message_key: "error.okb_empty",
            fallback: "No records found in .okb backup.".to_string(),
        };
    }

    // Unlock with the provided startup password or the cached onboarding password.
    let master_password = match master_password.or_else(|| executor.verified_master_password.take())
    {
        Some(pw) => pw,
        None => {
            return CommandResult::Error {
                code: ErrorCode::ExecutorMasterPasswordRequired,
                context: ErrorContext::default(),
                message_key: "error.password_required",
                fallback: "Master password is required to unlock the recovered vault.".to_string(),
            };
        }
    };

    // Create a pending file-backed vault.db. If any later step fails, dropping
    // the guard restores the executor to an in-memory vault and removes the
    // uncommitted database files.
    let mut pending = match executor.begin_file_backed_vault_db() {
        Ok(pending) => pending,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::VaultDatabaseIoError,
                context: ErrorContext::default(),
                message_key: "error.db_reopen_failed",
                fallback: format!("Failed to create vault database: {}", e),
            };
        }
    };

    if let Err(e) = pending.unlock(&master_password) {
        return CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.unlock_failed",
            fallback: format!("Failed to unlock vault: {}", e),
        };
    }
    drop(master_password);

    // Import all parsed records into the vault.
    let mut imported = 0usize;
    let mut errors = 0usize;
    for item in items {
        let cred_type =
            crate::services::import_export::mapping::infer_credential_type(&item.fields);
        let payload = fields_to_payload(cred_type, &item.fields);
        let params = CreateRecordParams {
            credential_type: cred_type,
            payload,
            tags: item.tags.clone(),
            is_favorite: false,
            expires_at: None,
        };
        match pending.create_record(params) {
            Ok(_) => imported += 1,
            Err(_) => errors += 1,
        }
    }

    if errors > 0 || imported == 0 {
        return CommandResult::Error {
            code: ErrorCode::ImportPartialFailure,
            context: ErrorContext::default(),
            message_key: "error.okb_restore_import_failed",
            fallback: format!(
                "Failed to restore .okb backup: imported {}, failed {}.",
                imported, errors
            ),
        };
    }

    pending.commit();
    tracing::info!(imported, errors, "OKB restore complete");

    CommandResult::DatabaseRestored {
        source: crate::commands::types::DatabaseRecoverySource::Okb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::sensitive::SecureStr;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_test_executor() -> CommandExecutor {
        use crate::config::AppConfig;
        use crate::executor::config_impl::ServiceNotificationImpl;
        use crate::services::clipboard::{ClipboardService, MockBackend};
        use crate::services::health::HealthService;
        use crate::services::import_export::ImportExportService;
        use crate::services::vault::VaultService;
        use std::sync::Arc;
        use tokio::sync::mpsc;
        use tokio_util::sync::CancellationToken;

        let conn = crate::db::schema::init_db_in_memory();
        let vault = VaultService::new(conn);
        let (result_tx, _) = mpsc::channel(64);
        let (internal_tx, internal_rx) = mpsc::channel(64);

        CommandExecutor {
            vault,
            vault_db_file_backed: false,
            sync: None,
            health: HealthService::new(),
            clipboard: Arc::new(ClipboardService::with_backend(
                Box::new(MockBackend::new()),
                30,
            )),
            import_export: ImportExportService::new(),
            config: crate::executor::config_impl::ConfigManagerImpl::new(
                AppConfig::default(),
                std::path::PathBuf::from(":memory:"),
            ),
            config_notifier: ServiceNotificationImpl::new(),
            vault_dir: std::path::PathBuf::from(":memory:"),
            config_dir: std::path::PathBuf::from(":memory:"),
            health_report: None,
            last_health_check_time: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            shutdown_token: CancellationToken::new(),
            operation_cancel_token: CancellationToken::new(),
            timer_rebuild_pending: false,
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
            verified_master_password: None,
        }
    }

    #[test]
    fn import_validate_returns_cancelled_when_token_already_cancelled() {
        let mut executor = make_test_executor();
        executor.cancel_token().cancel();

        let result = handle_validate_import_file(
            &mut executor,
            ImportSource::Csv,
            std::path::PathBuf::from("sample.csv"),
            None,
        );

        assert!(matches!(
            result,
            CommandResult::Cancelled { ref operation, .. } if operation == "import_validate"
        ));
    }

    #[test]
    fn import_execute_returns_cancelled_when_token_already_cancelled() {
        let mut executor = make_test_executor();
        executor.cancel_token().cancel();

        let result = handle_execute_import(
            &mut executor,
            None,
            ImportSource::Csv,
            std::path::PathBuf::from("sample.csv"),
            None,
            None,
            false,
        );

        assert!(matches!(
            result,
            CommandResult::Cancelled { ref operation, .. } if operation == "import_execute"
        ));
    }

    #[test]
    fn export_execute_returns_cancelled_when_token_already_cancelled() {
        let mut executor = make_test_executor();
        executor.cancel_token().cancel();

        let result = handle_execute_export(
            &mut executor,
            ExportScope::All,
            std::path::PathBuf::from("export.okb"),
            SecureStr::new("export_pass".to_string()),
            SecureStr::new("master_pass".to_string()),
            ExportFormat::Okb,
        );

        assert!(matches!(
            result,
            CommandResult::Cancelled { ref operation, .. } if operation == "export_execute"
        ));
    }

    // --- existing tests below ---

    fn uuid() -> Uuid {
        Uuid::new_v4()
    }

    #[test]
    fn login_fields_populated_in_export() {
        let id = uuid();
        let record = DecryptedRecord::Login {
            id,
            is_favorite: true,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec!["work".to_string()],
            name: "GitHub".to_string(),
            username: "alice".to_string(),
            password: SecureStr::new("s3cret!".to_string()),
            url: Some("https://github.com".to_string()),
            notes: Some("personal account".to_string()),
        };

        let export = decrypted_record_to_export(&record);

        assert_eq!(export.id, id.to_string());
        assert_eq!(export.credential_type, "login");
        assert_eq!(export.name, "GitHub");
        assert_eq!(export.username.as_deref(), Some("alice"));
        assert_eq!(export.password.as_deref(), Some("s3cret!"));
        assert_eq!(export.url.as_deref(), Some("https://github.com"));
        assert_eq!(export.notes.as_deref(), Some("personal account"));
        assert_eq!(export.tags, Some(vec!["work".to_string()]));
        assert_eq!(export.is_favorite, Some(true));
    }

    #[test]
    fn api_fields_mapped_to_export() {
        let id = uuid();
        let record = DecryptedRecord::Api {
            id,
            is_favorite: false,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "AWS".to_string(),
            app_id: "AKIA123".to_string(),
            secret_key: SecureStr::new("secret456".to_string()),
            url: None,
            notes: None,
        };

        let export = decrypted_record_to_export(&record);

        assert_eq!(export.credential_type, "api");
        assert!(export.username.is_none());
        assert!(export.password.is_none());
        assert_eq!(export.app_id.as_deref(), Some("AKIA123"));
        assert_eq!(export.secret_key.as_deref(), Some("secret456"));
        assert!(export.url.is_none());
        assert!(export.notes.is_none());
    }

    #[test]
    fn ssh_fields_mapped_to_export() {
        let id = uuid();
        let record = DecryptedRecord::Ssh {
            id,
            is_favorite: false,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "Server".to_string(),
            public_key: "ssh-rsa AAA...".to_string(),
            private_key: Some(SecureStr::new("-----BEGIN RSA...".to_string())),
            passphrase: None,
            notes: Some("production".to_string()),
        };

        let export = decrypted_record_to_export(&record);

        assert_eq!(export.credential_type, "ssh");
        assert!(export.username.is_none());
        assert!(export.password.is_none());
        assert_eq!(export.public_key.as_deref(), Some("ssh-rsa AAA..."));
        assert_eq!(export.private_key.as_deref(), Some("-----BEGIN RSA..."));
        assert!(export.url.is_none());
        assert_eq!(export.notes.as_deref(), Some("production"));
    }

    #[test]
    fn ssh_without_private_key_exports_none_password() {
        let id = uuid();
        let record = DecryptedRecord::Ssh {
            id,
            is_favorite: false,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "PubOnly".to_string(),
            public_key: "ssh-ed25519 AAA".to_string(),
            private_key: None,
            passphrase: None,
            notes: None,
        };

        let export = decrypted_record_to_export(&record);

        assert!(export.private_key.is_none());
    }

    #[test]
    fn ssh_credential_roundtrip_preserves_fields() {
        use std::collections::HashMap;

        // Step 1: Create DecryptedRecord::Ssh with all fields
        let id = uuid();
        let record = DecryptedRecord::Ssh {
            id,
            is_favorite: false,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec!["server".to_string()],
            name: "Production Server".to_string(),
            public_key: "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...".to_string(),
            private_key: Some(SecureStr::new(
                "-----BEGIN OPENSSH PRIVATE KEY-----\n...".to_string(),
            )),
            passphrase: Some(SecureStr::new("my-passphrase".to_string())),
            notes: Some("production key".to_string()),
        };

        // Step 2: Export direction: decrypted_record_to_export
        let export = decrypted_record_to_export(&record);
        assert_eq!(export.credential_type, "ssh");
        assert!(export.username.is_none());
        assert!(export.password.is_none());
        assert_eq!(
            export.public_key.as_deref(),
            Some("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...")
        );
        assert_eq!(
            export.private_key.as_deref(),
            Some("-----BEGIN OPENSSH PRIVATE KEY-----\n...")
        );
        assert_eq!(export.passphrase.as_deref(), Some("my-passphrase"));

        // Step 3: Simulate OKB parser output (HashMap)
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), export.name.clone());
        fields.insert(
            "credential_type".to_string(),
            export.credential_type.clone(),
        );
        if let Some(pk) = &export.public_key {
            fields.insert("public_key".to_string(), pk.clone());
        }
        if let Some(pk) = &export.private_key {
            fields.insert("private_key".to_string(), pk.clone());
        }
        if let Some(p) = &export.passphrase {
            fields.insert("passphrase".to_string(), p.clone());
        }
        if let Some(n) = &export.notes {
            fields.insert("notes".to_string(), n.clone());
        }

        // Step 4: Import direction: fields_to_payload
        let payload = fields_to_payload(CredentialType::Ssh, &fields);
        match payload {
            EncryptedPayload::Ssh {
                name,
                public_key,
                private_key,
                passphrase,
                notes,
            } => {
                assert_eq!(name, "Production Server");
                assert_eq!(public_key, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI...");
                assert!(private_key.is_some());
                assert_eq!(
                    private_key.unwrap().expose(),
                    "-----BEGIN OPENSSH PRIVATE KEY-----\n..."
                );
                assert!(passphrase.is_some());
                assert_eq!(passphrase.unwrap().expose(), "my-passphrase");
                assert_eq!(notes, Some("production key".to_string()));
            }
            _ => panic!("expected Ssh payload"),
        }
    }

    #[test]
    fn api_credential_roundtrip_preserves_fields() {
        use std::collections::HashMap;

        // Step 1: Create DecryptedRecord::Api with all fields
        let id = uuid();
        let record = DecryptedRecord::Api {
            id,
            is_favorite: true,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec!["cloud".to_string()],
            name: "AWS Production".to_string(),
            app_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_key: SecureStr::new("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string()),
            url: Some("https://aws.amazon.com".to_string()),
            notes: Some("production account".to_string()),
        };

        // Step 2: Export direction: decrypted_record_to_export
        let export = decrypted_record_to_export(&record);
        assert_eq!(export.credential_type, "api");
        assert!(export.username.is_none());
        assert!(export.password.is_none());
        assert_eq!(export.app_id.as_deref(), Some("AKIAIOSFODNN7EXAMPLE"));
        assert_eq!(
            export.secret_key.as_deref(),
            Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY")
        );
        assert_eq!(export.url.as_deref(), Some("https://aws.amazon.com"));
        assert_eq!(export.notes.as_deref(), Some("production account"));

        // Step 3: Simulate OKB parser output (HashMap)
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), export.name.clone());
        fields.insert(
            "credential_type".to_string(),
            export.credential_type.clone(),
        );
        if let Some(app_id) = &export.app_id {
            fields.insert("app_id".to_string(), app_id.clone());
        }
        if let Some(secret_key) = &export.secret_key {
            fields.insert("secret_key".to_string(), secret_key.clone());
        }
        if let Some(url) = &export.url {
            fields.insert("url".to_string(), url.clone());
        }
        if let Some(notes) = &export.notes {
            fields.insert("notes".to_string(), notes.clone());
        }

        // Step 4: Import direction: fields_to_payload
        let payload = fields_to_payload(CredentialType::Api, &fields);
        match payload {
            EncryptedPayload::Api {
                name,
                app_id,
                secret_key,
                url,
                notes,
            } => {
                assert_eq!(name, "AWS Production");
                assert_eq!(app_id, "AKIAIOSFODNN7EXAMPLE");
                assert_eq!(
                    secret_key.expose(),
                    "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
                );
                assert_eq!(url, Some("https://aws.amazon.com".to_string()));
                assert_eq!(notes, Some("production account".to_string()));
            }
            _ => panic!("expected Api payload"),
        }
    }

    #[test]
    fn ssh_without_passphrase_roundtrip() {
        use std::collections::HashMap;

        // Step 1: Create DecryptedRecord::Ssh without passphrase
        let id = uuid();
        let record = DecryptedRecord::Ssh {
            id,
            is_favorite: false,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "Test Server".to_string(),
            public_key: "ssh-rsa AAAAB3NzaC1yc2E...".to_string(),
            private_key: Some(SecureStr::new(
                "-----BEGIN RSA PRIVATE KEY-----\n...".to_string(),
            )),
            passphrase: None, // No passphrase
            notes: None,
        };

        // Step 2: Export direction: decrypted_record_to_export
        let export = decrypted_record_to_export(&record);
        assert_eq!(export.credential_type, "ssh");
        assert_eq!(
            export.public_key.as_deref(),
            Some("ssh-rsa AAAAB3NzaC1yc2E...")
        );
        assert_eq!(
            export.private_key.as_deref(),
            Some("-----BEGIN RSA PRIVATE KEY-----\n...")
        );
        assert!(export.passphrase.is_none()); // Verify passphrase is None

        // Step 3: Simulate OKB parser output (HashMap)
        let mut fields = HashMap::new();
        fields.insert("name".to_string(), export.name.clone());
        fields.insert(
            "credential_type".to_string(),
            export.credential_type.clone(),
        );
        if let Some(pk) = &export.public_key {
            fields.insert("public_key".to_string(), pk.clone());
        }
        if let Some(pk) = &export.private_key {
            fields.insert("private_key".to_string(), pk.clone());
        }
        // Note: passphrase is None, so we don't insert it into fields

        // Step 4: Import direction: fields_to_payload
        let payload = fields_to_payload(CredentialType::Ssh, &fields);
        match payload {
            EncryptedPayload::Ssh {
                name,
                public_key,
                private_key,
                passphrase,
                notes,
            } => {
                assert_eq!(name, "Test Server");
                assert_eq!(public_key, "ssh-rsa AAAAB3NzaC1yc2E...");
                assert!(private_key.is_some());
                assert_eq!(
                    private_key.unwrap().expose(),
                    "-----BEGIN RSA PRIVATE KEY-----\n..."
                );
                assert!(passphrase.is_none()); // Verify passphrase remains None
                assert!(notes.is_none());
            }
            _ => panic!("expected Ssh payload"),
        }
    }

    #[test]
    fn build_skip_breakdown_excludes_vault_write_error() {
        use crate::services::import_export::types::ImportResult;

        let result = ImportResult {
            imported: 5,
            reviewed: 0,
            skipped: 3,
            failed: 2,
            validation_failed: 1,
            duration_ms: 100,
        };
        let breakdown = build_skip_breakdown(&result);

        assert_eq!(breakdown.get(&SkipReason::Duplicate), Some(&3));
        assert_eq!(breakdown.get(&SkipReason::ValidationFailed), Some(&1));
        assert_eq!(breakdown.get(&SkipReason::VaultWriteError), None);
    }
}
