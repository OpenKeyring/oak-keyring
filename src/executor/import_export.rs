use std::collections::HashSet;
use std::path::PathBuf;

use crate::commands::types::{
    CsvColumnMapping, ExportScope, ImportSource, RecordFilter, RecordSort, SortDirection, SortField,
};
use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::import_export::duplicate::ExistingRecordKey;
use crate::services::import_export::export::ExportRecord;
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
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("import_execute");
    }

    // Step 1: Create import session.
    let session_id =
        match executor
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

    let result = match executor.import_export.execute_import(
        session_id,
        existing_keys,
        |cred_type, fields, tags| {
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
    ) {
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
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("export_execute");
    }

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
    let session_id =
        match executor
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
        Ok(result) => result,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::ImportExport(e.to_string()),
                context: ErrorContext::default(),
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
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Map a fully decrypted record to the flat export format.
///
/// Field mapping follows the same convention as `VaultService::decrypt_field`:
/// - Login: username→username, password→password, url→url, notes→notes
/// - Api: app_id→username, secret_key→password, url→url, notes→notes
/// - Ssh: public_key→username, private_key→password, notes→notes
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
            password: Some(password.get().clone()),
            url: url.clone(),
            notes: notes.clone(),
            tags: Some(tags.clone()),
            is_favorite: Some(*is_favorite),
            expires_at: expires_at.map(|t| t.to_rfc3339()),
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
            username: Some(app_id.clone()),
            password: Some(secret_key.get().clone()),
            url: url.clone(),
            notes: notes.clone(),
            tags: Some(tags.clone()),
            is_favorite: Some(*is_favorite),
            expires_at: expires_at.map(|t| t.to_rfc3339()),
        },
        DecryptedRecord::Ssh {
            id,
            is_favorite,
            expires_at,
            tags,
            name,
            public_key,
            private_key,
            notes,
            ..
        } => ExportRecord {
            id: id.to_string(),
            credential_type: CredentialType::Ssh.to_db_str().to_string(),
            name: name.clone(),
            username: Some(public_key.clone()),
            password: private_key.as_ref().map(|pk| pk.get().clone()),
            url: None,
            notes: notes.clone(),
            tags: Some(tags.clone()),
            is_favorite: Some(*is_favorite),
            expires_at: expires_at.map(|t| t.to_rfc3339()),
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
            sync: None,
            health: HealthService::new(),
            clipboard: Arc::new(ClipboardService::with_backend(
                Box::new(MockBackend::new()),
                30,
            )),
            import_export: ImportExportService::new(),
            config: AppConfig::default(),
            config_notifier: ServiceNotificationImpl::new(),
            vault_dir: std::path::PathBuf::from(":memory:"),
            health_report: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            cancel_token: CancellationToken::new(),
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
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
            ImportSource::Csv,
            std::path::PathBuf::from("sample.csv"),
            None,
            None,
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
        assert_eq!(export.username.as_deref(), Some("AKIA123"));
        assert_eq!(export.password.as_deref(), Some("secret456"));
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
        assert_eq!(export.username.as_deref(), Some("ssh-rsa AAA..."));
        assert_eq!(export.password.as_deref(), Some("-----BEGIN RSA..."));
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

        assert!(export.password.is_none());
    }
}
