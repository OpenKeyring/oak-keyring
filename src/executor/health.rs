use uuid::Uuid;

use crate::commands::CommandResult;
use crate::commands::types::FieldSelector;
use crate::errors::{ErrorCode, ErrorContext};

use super::CommandExecutor;

#[tracing::instrument(skip_all)]
pub fn handle_run_health_check(executor: &mut CommandExecutor) -> CommandResult {
    // Step 1: Fetch all active stored records
    let records = match executor.vault.list_all_stored_records() {
        Ok(r) => r,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::Vault(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.list_records_failed",
                fallback: format!("Failed to list records for health check: {}", e),
            }
        }
    };

    // Step 2: Create a decrypt closure that captures &mut VaultService.
    // The closure borrows executor.vault mutably (decrypt_field needs &self for
    // decrypt_field which takes &self). Since executor is already &mut, we can
    // safely borrow vault through it.
    let vault = &executor.vault;
    let decrypt_fn = |id: Uuid| -> Result<crate::types::SecureStr, String> {
        vault
            .decrypt_field(id, FieldSelector::Password)
            .map_err(|e| e.to_string())
    };

    // Step 3: Run full health check
    let report = executor.health.run_full_check(&records, decrypt_fn);

    // Step 4: Cache the report for future reference
    executor.health_report = Some(report.clone());

    CommandResult::HealthCheckCompleted { report }
}

#[tracing::instrument(skip_all)]
pub fn handle_check_hibp(executor: &mut CommandExecutor, record_id: Uuid) -> CommandResult {
    // Step 1: Decrypt the record's password
    let password = match executor.vault.decrypt_field(record_id, FieldSelector::Password) {
        Ok(s) => s,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::Vault(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.decrypt_field_failed",
                fallback: format!("Failed to decrypt password for HIBP check: {}", e),
            }
        }
    };

    // Step 2: Check against HIBP
    let compromised = match executor.health.check_hibp_single(&password) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::Health(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.hibp_check_failed",
                fallback: format!("HIBP check failed: {}", e),
            }
        }
    };
    // password (SecureStr) is dropped here, zeroized automatically

    CommandResult::HibpCheckCompleted {
        record_id,
        compromised,
    }
}
