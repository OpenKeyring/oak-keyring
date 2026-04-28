use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::commands::types::{FieldSelector, RecordFilter, RecordSort};
use crate::commands::CommandResult;
use crate::crypto::password::{
    generate_memorable_password, generate_pin, generate_random_password,
};
use crate::crypto::strength::evaluate_strength;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::record::{CreateRecordParams, UpdateRecordParams};
use crate::types::{CredentialType, EncryptedPayload, PasswordHistoryView, SecureStr};

use super::CommandExecutor;

/// Helper: build a standard error CommandResult from a VaultError.
fn vault_error(e: crate::errors::mapping::vault::VaultError, msg: &str) -> CommandResult {
    CommandResult::Error {
        code: ErrorCode::Vault(e.to_string()),
        context: ErrorContext::default(),
        message_key: "error.record_operation",
        fallback: format!("{}: {}", msg, e),
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
        Ok(id) => CommandResult::RecordCreated { id },
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
        Ok(()) => CommandResult::RecordUpdated { id },
        Err(e) => vault_error(e, "Failed to update record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_soft_delete_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.soft_delete_record(id) {
        Ok(()) => CommandResult::RecordDeleted { id },
        Err(e) => vault_error(e, "Failed to delete record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_restore_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.restore_record(id) {
        Ok(()) => CommandResult::RecordRestored { id },
        Err(e) => vault_error(e, "Failed to restore record"),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_hard_delete_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.hard_delete_record(id) {
        Ok(()) => CommandResult::RecordDestroyed { id },
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
            // Spec Compliance: Populate has_weak_password from cached health_report
            if let Some(report) = &executor.health_report {
                for record in &mut records {
                    record.has_weak_password = report.weak_passwords.contains(&record.id);
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
        Ok(record) => CommandResult::RecordDetailLoaded { record },
        Err(e) => vault_error(e, "Failed to load record detail"),
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
    match executor.vault.list_tags() {
        Ok(tags_with_counts) => {
            let tags: Vec<_> = tags_with_counts.into_iter().map(|(t, _)| t).collect();
            CommandResult::TagsLoaded { tags }
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
            let strength = evaluate_strength(password.get());
            CommandResult::PasswordGenerated { password, strength }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::Crypto(e.clone()),
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
            let strength = evaluate_strength(password.get());
            CommandResult::PasswordGenerated { password, strength }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::Crypto(e.clone()),
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
            let strength = evaluate_strength(password.get());
            CommandResult::PasswordGenerated { password, strength }
        }
        Err(e) => CommandResult::Error {
            code: ErrorCode::Crypto(e.clone()),
            context: ErrorContext::default(),
            message_key: "error.password_generation",
            fallback: format!("Failed to generate PIN: {}", e),
        },
    }
}
