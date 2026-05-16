use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use crate::commands::types::{FieldSelector, RecordFilter, RecordSort};
use crate::commands::{CommandResult, InternalCommand};
use crate::crypto::password::{
    generate_memorable_password, generate_pin, generate_random_password,
};
use crate::crypto::strength::evaluate_strength;
use crate::errors::{ErrorCode, ErrorContext, ServiceError};
use crate::types::record::{CreateRecordParams, UpdateRecordParams};
use crate::types::tag::TagSortMeta;
use crate::types::{
    CredentialType, DecryptedRecord, EncryptedPayload, PasswordHistoryView, SecureStr,
};

use super::CommandExecutor;

/// Helper: build a standard error CommandResult from a VaultError.
fn vault_error(e: crate::errors::mapping::vault::VaultError, msg: &str) -> CommandResult {
    CommandResult::Error {
        code: e.to_error_code(),
        context: e.to_error_context(),
        message_key: "error.record_operation",
        fallback: format!("{}: {}", msg, e.to_fallback_message()),
    }
}

/// Schedule a full health scan by sending `RunHealthCheck` through the
/// internal command channel. Failures are logged but do not block the
/// caller — the health scan is advisory.
fn schedule_health_scan(executor: &CommandExecutor) {
    if let Err(e) = executor
        .internal_tx
        .try_send(InternalCommand::ScheduleHealthCheck { force: true })
    {
        tracing::warn!(error = %e, "Failed to schedule health scan");
    }
}

/// Attempt lazy DEK migration for a record if it is on an older version.
///
/// This is a best-effort operation: if the stored record's DEK version is
/// older than the current DEK version, we re-encrypt it with the current key.
/// Failures are logged but do not block the read — the record is still
/// readable with the old DEK version.
fn attempt_lazy_migration(vault: &mut crate::services::vault::VaultService, id: Uuid) {
    // Get the stored record to check its DEK version.
    let dek_version = match vault.get_stored_record(id) {
        Ok(stored) => stored.dek_version,
        Err(_) => return, // Record not found or other error; let the main call handle it.
    };

    let current_version = vault.current_dek_version();
    if dek_version < current_version {
        if let Err(e) = crate::services::rotation::lazy_migrate_record(vault, id, dek_version) {
            tracing::warn!(
                record_id = %id,
                dek_version = dek_version,
                current_version = current_version,
                error = %e,
                "Lazy migration failed, record still readable with old DEK"
            );
        }
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_create_record(
    executor: &mut CommandExecutor,
    credential_type: CredentialType,
    payload: EncryptedPayload,
    tags: Vec<String>,
    is_favorite: bool,
    expires_at: Option<DateTime<Utc>>,
) -> CommandResult {
    let params = CreateRecordParams {
        credential_type,
        payload,
        tags,
        is_favorite,
        expires_at,
    };

    match executor.vault.create_record(params) {
        Ok(id) => {
            // New records need a health evaluation — schedule a full scan.
            schedule_health_scan(executor);
            CommandResult::RecordCreated { id }
        }
        Err(e) => vault_error(e, "Failed to create record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_update_record(
    executor: &mut CommandExecutor,
    id: Uuid,
    payload: EncryptedPayload,
    tags: Vec<String>,
    is_favorite: bool,
    expires_at: Option<DateTime<Utc>>,
    expected_version: u64,
) -> CommandResult {
    let params = UpdateRecordParams {
        id,
        payload,
        tags,
        is_favorite,
        expires_at,
        expected_version,
    };

    match executor.vault.update_record(params) {
        Ok(()) => {
            // VaultService manages health state internally (delete or carry-forward).
            // Schedule a health scan so that deleted health states get re-evaluated.
            schedule_health_scan(executor);
            CommandResult::RecordUpdated { id }
        }
        Err(e) => vault_error(e, "Failed to update record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_soft_delete_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.soft_delete_record(id) {
        Ok(()) => {
            schedule_health_scan(executor);
            CommandResult::RecordDeleted { id }
        }
        Err(e) => vault_error(e, "Failed to delete record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_restore_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.restore_record(id) {
        Ok(()) => {
            schedule_health_scan(executor);
            CommandResult::RecordRestored { id }
        }
        Err(e) => vault_error(e, "Failed to restore record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_hard_delete_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.hard_delete_record(id) {
        Ok(()) => {
            schedule_health_scan(executor);
            CommandResult::RecordDestroyed { id }
        }
        Err(e) => vault_error(e, "Failed to destroy record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_toggle_favorite(
    executor: &mut CommandExecutor,
    id: Uuid,
    is_favorite: bool,
) -> CommandResult {
    match executor.vault.toggle_favorite(id, is_favorite) {
        Ok(()) => CommandResult::FavoriteToggled { id, is_favorite },
        Err(e) => vault_error(e, "Failed to toggle favorite"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_load_record_list(
    executor: &mut CommandExecutor,
    filter: RecordFilter,
    sort: RecordSort,
) -> CommandResult {
    match executor.vault.list_records(&filter, &sort) {
        Ok(mut records) => {
            // Spec Compliance: Populate health fields from cached health_report.
            // When no health report exists, is_expired stays false (set by vault
            // service) — we do NOT fall back to expires_at < now. Per spec §11.2,
            // "已过期" depends solely on persisted health state.
            if let Some(report) = &executor.health_report {
                for record in &mut records {
                    record.has_weak_password = report.weak_passwords.contains(&record.id);
                    record.is_compromised = report.compromised.contains(&record.id);
                    record.duplicate_group_size = report
                        .duplicate_passwords
                        .iter()
                        .find(|group| group.contains(&record.id))
                        .map(|group| group.len());
                    record.is_expired = report.expired.contains(&record.id);
                }
            }

            // Expired filter: use report.expired instead of expires_at < now.
            // When no health report is available, no records appear in the
            // expired category — per spec §11.2.
            if matches!(filter, RecordFilter::Expired) {
                if let Some(report) = &executor.health_report {
                    let expired_set: std::collections::HashSet<Uuid> =
                        report.expired.iter().copied().collect();
                    records.retain(|r| expired_set.contains(&r.id));
                } else {
                    // No health report — no expired records to show.
                    records.clear();
                }
            }

            // HealthIssues filter: the vault service returns all active records;
            // we filter here where the health_report is available.
            if matches!(filter, RecordFilter::HealthIssues) {
                if let Some(report) = &executor.health_report {
                    records.retain(|r| {
                        report.weak_passwords.contains(&r.id)
                            || report
                                .duplicate_passwords
                                .iter()
                                .any(|group| group.contains(&r.id))
                            || report.compromised.contains(&r.id)
                            || report.expired.contains(&r.id)
                    });
                } else {
                    // No health report available — no health issues to show
                    records.clear();
                }
            }

            let total = records.len();
            CommandResult::RecordListLoaded { records, total }
        }
        Err(e) => vault_error(e, "Failed to load record list"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_load_record_detail(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    // Lazy migration: check DEK version before decrypting.
    // If the record is on an older DEK version, migrate it first.
    attempt_lazy_migration(&mut executor.vault, id);

    match executor.vault.get_decrypted_record(id) {
        Ok(record) => {
            // Compute password strength based on credential type.
            let password_strength = compute_password_strength(&record);

            // Query health issue from cached health report (if available).
            // Security: password plaintext is dropped after evaluate_strength() returns.
            let health_issue = executor
                .health_report
                .as_ref()
                .and_then(|report| report.get_issue_for(record.id()));

            CommandResult::RecordDetailLoaded {
                record,
                password_strength,
                health_issue,
            }
        }
        Err(e) => vault_error(e, "Failed to load record detail"),
    }
}

/// Compute password strength based on the credential type.
///
/// - Login: evaluates the `password` field
/// - API: evaluates the `secret_key` field
/// - SSH: returns `None` (SSH key material is not a password)
fn compute_password_strength(
    record: &DecryptedRecord,
) -> Option<crate::crypto::strength::PasswordStrength> {
    match record {
        DecryptedRecord::Login { password, .. } => Some(
            crate::crypto::strength::evaluate_strength(password.expose()),
        ),
        DecryptedRecord::Api { secret_key, .. } => Some(
            crate::crypto::strength::evaluate_strength(secret_key.expose()),
        ),
        DecryptedRecord::Ssh { .. } => None,
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_load_record_for_edit(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    // Lazy migration: check DEK version before decrypting.
    attempt_lazy_migration(&mut executor.vault, id);

    match executor.vault.get_decrypted_record(id) {
        Ok(record) => CommandResult::RecordForEditLoaded { record },
        Err(e) => vault_error(e, "Failed to load record for edit"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_decrypt_field(
    executor: &mut CommandExecutor,
    id: Uuid,
    field: FieldSelector,
) -> CommandResult {
    match executor.vault.decrypt_field(id, field) {
        Ok(value) => CommandResult::FieldDecrypted { id, field, value },
        Err(e) => vault_error(e, "Failed to decrypt field"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_load_password_history(
    executor: &mut CommandExecutor,
    record_id: Uuid,
) -> CommandResult {
    match executor.vault.get_password_history(record_id) {
        Ok(entries) => {
            // Vault returns Vec<PasswordHistory> (encrypted).
            // Decrypt each entry to build PasswordHistoryView for the UI.
            let mut views = Vec::with_capacity(entries.len());
            for entry in &entries {
                let password = match executor.vault.decrypt_history_password(entry.id) {
                    Ok(p) => p,
                    Err(_) => SecureStr::new(String::from("***")),
                };
                views.push(PasswordHistoryView {
                    id: entry.id,
                    password,
                    changed_at: entry.changed_at,
                });
            }
            CommandResult::PasswordHistoryLoaded { history: views }
        }
        Err(e) => vault_error(e, "Failed to load password history"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_load_tags(executor: &mut CommandExecutor) -> CommandResult {
    match executor.vault.list_tags_with_stats() {
        Ok(tags_with_stats) => {
            let tags: Vec<_> = tags_with_stats.iter().map(|(t, _)| t.clone()).collect();
            let tag_stats: HashMap<i64, TagSortMeta> = tags_with_stats
                .into_iter()
                .map(|(t, meta)| (t.id, meta))
                .collect();
            CommandResult::TagsLoaded { tags, tag_stats }
        }
        Err(e) => vault_error(e, "Failed to load tags"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_rename_tag(
    executor: &mut CommandExecutor,
    old_name: String,
    new_name: String,
) -> CommandResult {
    match executor.vault.rename_tag(&old_name, &new_name) {
        Ok(()) => CommandResult::TagRenamed { old_name, new_name },
        Err(e) => vault_error(e, "Failed to rename tag"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_delete_tag(executor: &mut CommandExecutor, name: String) -> CommandResult {
    match executor.vault.delete_tag(&name) {
        Ok(()) => CommandResult::TagDeleted { name },
        Err(e) => vault_error(e, "Failed to delete tag"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_batch_add_tag(
    executor: &mut CommandExecutor,
    record_ids: Vec<Uuid>,
    tag_name: String,
) -> CommandResult {
    match executor.vault.batch_add_tag(&record_ids, &tag_name) {
        Ok(count) => CommandResult::BatchTagAdded { count },
        Err(e) => vault_error(e, "Failed to add tag to records"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_batch_remove_tag(
    executor: &mut CommandExecutor,
    record_ids: Vec<Uuid>,
    tag_name: String,
) -> CommandResult {
    match executor.vault.batch_remove_tag(&record_ids, &tag_name) {
        Ok(count) => CommandResult::BatchTagRemoved { count },
        Err(e) => vault_error(e, "Failed to remove tag from records"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_batch_soft_delete(
    executor: &mut CommandExecutor,
    record_ids: Vec<Uuid>,
) -> CommandResult {
    match executor.vault.batch_soft_delete(&record_ids) {
        Ok(count) => CommandResult::BatchDeleted { count },
        Err(e) => vault_error(e, "Failed to batch delete records"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_empty_trash(executor: &mut CommandExecutor) -> CommandResult {
    match executor.vault.empty_trash() {
        Ok(count) => CommandResult::TrashEmptied { count },
        Err(e) => vault_error(e, "Failed to empty trash"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_generate_password(
    _executor: &mut CommandExecutor,
    length: usize,
    include_digits: bool,
    include_uppercase: bool,
    include_special: bool,
) -> CommandResult {
    let result = if include_digits && include_uppercase && include_special {
        generate_random_password(length)
    } else {
        // Use the policy-based generator with appropriate min counts.
        let min_digits = if include_digits { 1 } else { 0 };
        let min_special = if include_special { 1 } else { 0 };
        let min_lowercase = 1;
        let min_uppercase = if include_uppercase { 1 } else { 0 };
        crate::crypto::password::generate_random_password_with_policy(
            length,
            min_digits,
            min_special,
            min_lowercase,
            min_uppercase,
        )
    };

    match result {
        Ok(password) => {
            let strength = evaluate_strength(password.expose());
            CommandResult::PasswordGenerated { password, strength }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.password_generation",
            fallback: format!("Failed to generate password: {}", e),
        },
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_generate_memorable_password(
    _executor: &mut CommandExecutor,
    word_count: usize,
) -> CommandResult {
    match generate_memorable_password(word_count) {
        Ok(password) => {
            let strength = evaluate_strength(password.expose());
            CommandResult::PasswordGenerated { password, strength }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.password_generation",
            fallback: format!("Failed to generate memorable password: {}", e),
        },
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_generate_pin(_executor: &mut CommandExecutor, length: usize) -> CommandResult {
    match generate_pin(length) {
        Ok(password) => {
            let strength = evaluate_strength(password.expose());
            CommandResult::PasswordGenerated { password, strength }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::CryptoEncryptionFailed,
            context: ErrorContext::default(),
            message_key: "error.password_generation",
            fallback: format!("Failed to generate PIN: {}", e),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::commands::types::{HealthIssue, HealthReport};
    use crate::config::AppConfig;
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::executor::config_impl::ServiceNotificationImpl;
    use crate::executor::CommandExecutor;
    use crate::services::clipboard::{ClipboardService, MockBackend};
    use crate::services::health::HealthServiceImpl;
    use crate::services::import_export::ImportExportServiceImpl;
    use crate::services::vault::VaultService;
    use crate::types::{CredentialType, EncryptedPayload, SecureStr};

    use super::*;

    /// Create a basic unlocked executor with no records.
    fn make_unlocked_executor() -> CommandExecutor {
        let conn = crate::db::schema::init_db_in_memory();
        let mut vault = VaultService::new(conn);
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
        vault
            .unlock_with_mnemonic(&mnemonic)
            .expect("unlock with mnemonic");

        let (result_tx, _) = mpsc::channel(64);
        let (internal_tx, internal_rx) = mpsc::channel(64);

        CommandExecutor {
            vault,
            vault_db_file_backed: false,
            sync: None,
            health: Arc::new(HealthServiceImpl::new()),
            clipboard: Arc::new(ClipboardService::with_backend(
                Box::new(MockBackend::new()),
                30,
            )),
            import_export: Box::new(ImportExportServiceImpl::new()),
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

    /// Helper: create a Login record and return its UUID.
    fn create_login_record(executor: &mut CommandExecutor, name: &str, password: &str) -> Uuid {
        executor
            .vault
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: name.to_string(),
                    username: format!("user_{}", name),
                    password: SecureStr::new(password.to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create record")
    }

    /// Helper: create an API record and return its UUID.
    fn create_api_record(executor: &mut CommandExecutor, name: &str, secret_key: &str) -> Uuid {
        executor
            .vault
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Api,
                payload: EncryptedPayload::Api {
                    name: name.to_string(),
                    app_id: format!("app_{}", name),
                    secret_key: SecureStr::new(secret_key.to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create record")
    }

    /// Helper: create an SSH record and return its UUID.
    fn create_ssh_record(executor: &mut CommandExecutor, name: &str) -> Uuid {
        executor
            .vault
            .create_record(CreateRecordParams {
                credential_type: CredentialType::Ssh,
                payload: EncryptedPayload::Ssh {
                    name: name.to_string(),
                    public_key: "ssh-rsa AAA...".to_string(),
                    private_key: None,
                    passphrase: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create record")
    }

    // =========================================================================
    // handle_load_record_detail tests
    // =========================================================================

    #[test]
    fn login_record_returns_password_strength() {
        let mut executor = make_unlocked_executor();
        let id = create_login_record(&mut executor, "test", "MyP@ssw0rd!23");

        let result = handle_load_record_detail(&mut executor, id);

        match result {
            CommandResult::RecordDetailLoaded {
                password_strength, ..
            } => {
                assert!(
                    password_strength.is_some(),
                    "Login record should have password strength"
                );
                let strength = password_strength.unwrap();
                assert_eq!(strength.char_types, 4);
            }
            other => panic!("Expected RecordDetailLoaded, got {:?}", other),
        }
    }

    #[test]
    fn api_record_returns_password_strength() {
        let mut executor = make_unlocked_executor();
        let id = create_api_record(&mut executor, "test", "sk-secret-key-12345!@");

        let result = handle_load_record_detail(&mut executor, id);

        match result {
            CommandResult::RecordDetailLoaded {
                password_strength, ..
            } => {
                assert!(
                    password_strength.is_some(),
                    "API record should have password strength from secret_key"
                );
            }
            other => panic!("Expected RecordDetailLoaded, got {:?}", other),
        }
    }

    #[test]
    fn ssh_record_returns_no_password_strength() {
        let mut executor = make_unlocked_executor();
        let id = create_ssh_record(&mut executor, "test-host");

        let result = handle_load_record_detail(&mut executor, id);

        match result {
            CommandResult::RecordDetailLoaded {
                password_strength, ..
            } => {
                assert!(
                    password_strength.is_none(),
                    "SSH record should not have password strength"
                );
            }
            other => panic!("Expected RecordDetailLoaded, got {:?}", other),
        }
    }

    #[test]
    fn health_issue_is_present_when_record_has_weak_password() {
        let mut executor = make_unlocked_executor();
        let id = create_login_record(&mut executor, "test", "weak");

        // Set a health report with the record in weak_passwords
        executor.health_report = Some(HealthReport {
            weak_passwords: vec![id],
            duplicate_passwords: vec![],
            compromised: vec![],
            expired: vec![],
            total_checked: 1,
        });

        let result = handle_load_record_detail(&mut executor, id);

        match result {
            CommandResult::RecordDetailLoaded { health_issue, .. } => {
                assert_eq!(
                    health_issue,
                    Some(HealthIssue::Weak),
                    "Expected Weak health issue"
                );
            }
            other => panic!("Expected RecordDetailLoaded, got {:?}", other),
        }
    }

    #[test]
    fn health_issue_is_none_when_no_health_report() {
        let mut executor = make_unlocked_executor();
        let id = create_login_record(&mut executor, "test", "MyP@ssw0rd!23");

        // No health_report set on executor
        assert!(executor.health_report.is_none());

        let result = handle_load_record_detail(&mut executor, id);

        match result {
            CommandResult::RecordDetailLoaded { health_issue, .. } => {
                assert!(
                    health_issue.is_none(),
                    "health_issue should be None when no health report exists"
                );
            }
            other => panic!("Expected RecordDetailLoaded, got {:?}", other),
        }
    }

    #[test]
    fn health_issue_is_none_when_record_has_no_issues() {
        let mut executor = make_unlocked_executor();
        let id = create_login_record(&mut executor, "test", "MyP@ssw0rd!23");

        // Set an empty health report (record not in any issue list)
        executor.health_report = Some(HealthReport {
            weak_passwords: vec![],
            duplicate_passwords: vec![],
            compromised: vec![],
            expired: vec![],
            total_checked: 1,
        });

        let result = handle_load_record_detail(&mut executor, id);

        match result {
            CommandResult::RecordDetailLoaded { health_issue, .. } => {
                assert!(
                    health_issue.is_none(),
                    "health_issue should be None when record has no issues"
                );
            }
            other => panic!("Expected RecordDetailLoaded, got {:?}", other),
        }
    }

    #[test]
    fn login_weak_password_has_low_strength_level() {
        let mut executor = make_unlocked_executor();
        // "a" is VeryWeak (len < 8, char_types <= 1)
        let id = create_login_record(&mut executor, "weak", "a");

        let result = handle_load_record_detail(&mut executor, id);

        match result {
            CommandResult::RecordDetailLoaded {
                password_strength, ..
            } => {
                let strength = password_strength.expect("should have strength");
                assert_eq!(
                    strength.level,
                    crate::crypto::strength::StrengthLevel::VeryWeak
                );
            }
            other => panic!("Expected RecordDetailLoaded, got {:?}", other),
        }
    }

    // =========================================================================
    // compute_password_strength unit tests
    // =========================================================================

    #[test]
    fn compute_password_strength_for_login_uses_password_field() {
        let record = DecryptedRecord::Login {
            id: Uuid::new_v4(),
            is_favorite: false,
            expires_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "test".to_string(),
            username: "user".to_string(),
            password: SecureStr::new("StrongP@ss1".to_string()),
            url: None,
            notes: None,
        };

        let strength = compute_password_strength(&record);
        assert!(strength.is_some());
        assert_eq!(
            strength.unwrap().level,
            crate::crypto::strength::StrengthLevel::Strong
        );
    }

    #[test]
    fn compute_password_strength_for_api_uses_secret_key_field() {
        let record = DecryptedRecord::Api {
            id: Uuid::new_v4(),
            is_favorite: false,
            expires_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "test".to_string(),
            app_id: "app".to_string(),
            secret_key: SecureStr::new("sk-test-key-ABC!123".to_string()),
            url: None,
            notes: None,
        };

        let strength = compute_password_strength(&record);
        assert!(strength.is_some());
    }

    #[test]
    fn compute_password_strength_for_ssh_returns_none() {
        let record = DecryptedRecord::Ssh {
            id: Uuid::new_v4(),
            is_favorite: false,
            expires_at: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            version: 1,
            deleted: false,
            deleted_at: None,
            tags: vec![],
            name: "test".to_string(),
            public_key: "ssh-rsa AAA...".to_string(),
            private_key: None,
            passphrase: None,
            notes: None,
        };

        let strength = compute_password_strength(&record);
        assert!(strength.is_none());
    }
}
