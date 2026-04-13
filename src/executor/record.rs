use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::commands::CommandResult;
use crate::commands::types::{FieldSelector, RecordFilter, RecordSort};
use crate::crypto::password::{generate_pin, generate_memorable_password, generate_random_password};
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

pub fn handle_soft_delete_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.soft_delete_record(id) {
        Ok(()) => CommandResult::RecordDeleted { id },
        Err(e) => vault_error(e, "Failed to delete record"),
    }
}

pub fn handle_restore_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.restore_record(id) {
        Ok(()) => CommandResult::RecordRestored { id },
        Err(e) => vault_error(e, "Failed to restore record"),
    }
}

pub fn handle_hard_delete_record(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.hard_delete_record(id) {
        Ok(()) => CommandResult::RecordDestroyed { id },
        Err(e) => vault_error(e, "Failed to destroy record"),
    }
}

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

pub fn handle_load_record_list(
    executor: &mut CommandExecutor,
    filter: RecordFilter,
    sort: RecordSort,
) -> CommandResult {
    match executor.vault.list_records(&filter, &sort) {
        Ok(records) => {
            let total = records.len();
            CommandResult::RecordListLoaded { records, total }
        }
        Err(e) => vault_error(e, "Failed to load record list"),
    }
}

pub fn handle_load_record_detail(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.get_decrypted_record(id) {
        Ok(record) => CommandResult::RecordDetailLoaded { record },
        Err(e) => vault_error(e, "Failed to load record detail"),
    }
}

pub fn handle_load_record_for_edit(executor: &mut CommandExecutor, id: Uuid) -> CommandResult {
    match executor.vault.get_decrypted_record(id) {
        Ok(record) => CommandResult::RecordForEditLoaded { record },
        Err(e) => vault_error(e, "Failed to load record for edit"),
    }
}

pub fn handle_decrypt_field(
    executor: &mut CommandExecutor,
    id: Uuid,
    field: FieldSelector,
) -> CommandResult {
    match executor.vault.decrypt_field(id, field.clone()) {
        Ok(value) => CommandResult::FieldDecrypted { id, field, value },
        Err(e) => vault_error(e, "Failed to decrypt field"),
    }
}

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

pub fn handle_load_tags(executor: &mut CommandExecutor) -> CommandResult {
    match executor.vault.list_tags() {
        Ok(tags_with_counts) => {
            let tags: Vec<_> = tags_with_counts.into_iter().map(|(t, _)| t).collect();
            CommandResult::TagsLoaded { tags }
        }
        Err(e) => vault_error(e, "Failed to load tags"),
    }
}

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

pub fn handle_delete_tag(executor: &mut CommandExecutor, name: String) -> CommandResult {
    match executor.vault.delete_tag(&name) {
        Ok(()) => CommandResult::TagDeleted { name },
        Err(e) => vault_error(e, "Failed to delete tag"),
    }
}

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

pub fn handle_batch_soft_delete(
    executor: &mut CommandExecutor,
    record_ids: Vec<Uuid>,
) -> CommandResult {
    match executor.vault.batch_soft_delete(&record_ids) {
        Ok(count) => CommandResult::BatchDeleted { count },
        Err(e) => vault_error(e, "Failed to batch delete records"),
    }
}

pub fn handle_empty_trash(executor: &mut CommandExecutor) -> CommandResult {
    match executor.vault.empty_trash() {
        Ok(count) => CommandResult::TrashEmptied { count },
        Err(e) => vault_error(e, "Failed to empty trash"),
    }
}

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
