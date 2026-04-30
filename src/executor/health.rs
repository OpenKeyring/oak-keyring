use uuid::Uuid;

use crate::commands::types::{FieldSelector, HealthReport};
use crate::commands::{CommandResult, Message};
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::health::{
    detect_duplicate_passwords, detect_expired_records, detect_weak_passwords, PasswordEntry,
};
use crate::types::credential::CredentialType;

use super::CommandExecutor;

#[tracing::instrument(skip_all)]
pub fn handle_run_health_check(executor: &mut CommandExecutor) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("health_check");
    }

    // Step 1: Fetch all active stored records (fast, local)
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

    // Step 2: Decrypt passwords (relatively fast, local)
    // We only care about non-deleted Login records for password-based checks.
    let login_records: Vec<_> = records
        .iter()
        .filter(|r| !r.deleted && r.credential_type == CredentialType::Login)
        .collect();

    let mut entries = Vec::with_capacity(login_records.len());
    for record in &login_records {
        match executor
            .vault
            .decrypt_field(record.id, FieldSelector::Password)
        {
            Ok(password) => entries.push(PasswordEntry {
                id: record.id,
                password,
            }),
            Err(_) => {
                // Skip records that fail decryption
                tracing::debug!(record_id = %record.id, "skipping record: decryption failed");
            }
        }
    }

    // Step 3: Run fast local detections (weak, duplicates, expired)
    let weak_passwords = detect_weak_passwords(&entries);
    let duplicate_passwords = detect_duplicate_passwords(&entries);
    let expired = detect_expired_records(&records);
    let total_checked = entries.len(); // Fix AC: only count actual decrypted entries

    // Step 4: Prepare background task for HIBP check (slow, network)
    let tx = executor.result_tx.clone();
    let self_tx = executor.internal_tx.clone(); // Self-sender for internal caching
    let health_service = executor.health.clone();
    let cancel_token = executor.cancel_token().clone();

    // Spawn the background task for HIBP check and final report assembly
    tokio::spawn(async move {
        let mut compromised = Vec::new();
        let total = entries.len();

        // Security: Use into_iter to ensure each entry (SecureStr) is dropped
        // and zeroized IMMEDIATELY after its individual check is done.
        let mut entries_iter = entries.into_iter().enumerate();

        // 100ms rate limit ticker as recommended
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(100));

        loop {
            tokio::select! {
                biased;

                _ = cancel_token.cancelled() => {
                    tracing::info!("Health check cancelled: clearing remaining memory");
                    return; // Remaining entries in entries_iter will be dropped/zeroized here
                }
                _ = ticker.tick() => {
                    let (i, entry) = match entries_iter.next() {
                        Some(val) => val,
                        None => break, // All done
                    };

                    // Perform HIBP check via spawn_blocking to avoid blocking runtime
                    let hs = health_service.clone();
                    let is_compromised = tokio::task::spawn_blocking(move || {
                        hs.check_hibp_single(&entry.password)
                    }).await;

                    match is_compromised {
                        Ok(Ok(true)) => compromised.push(entry.id),
                        Ok(Ok(false)) => {}
                        Ok(Err(e)) => {
                            tracing::debug!(record_id = %entry.id, error = %e, "HIBP check failed, skipping record");
                        }
                        Err(e) => {
                            tracing::error!(record_id = %entry.id, error = %e, "HIBP task panicked");
                        }
                    }

                    // Report progress
                    if tx.send(Message::HealthCheckProgress {
                        current: i + 1,
                        total,
                    }).await.is_err() {
                        tracing::warn!("Health check: result channel closed, terminating task");
                        return; // Security: Exit immediately if UI is gone
                    }

                    // entry is dropped here, triggering zeroize for this specific password
                }
            }
        }

        // Final assembly of the report
        let report = HealthReport {
            weak_passwords,
            duplicate_passwords,
            compromised,
            expired,
            total_checked,
        };

        // Spec Compliance S5: Send internal signal to Executor to update its cache
        // This will also trigger the UI message via the Executor's standard execute flow.
        let _ = self_tx
            .send(crate::commands::Command::InternalHealthCheckCompleted { report })
            .await;
    });

    // Step 5: Return immediate "Started" result
    CommandResult::HealthCheckStarted
}

#[tracing::instrument(skip_all)]
pub async fn handle_check_hibp(executor: &mut CommandExecutor, record_id: Uuid) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("hibp_check");
    }

    // Step 1: Decrypt the record's password
    let password = match executor
        .vault
        .decrypt_field(record_id, FieldSelector::Password)
    {
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

    // Step 2: Check against HIBP via spawn_blocking to avoid blocking the async runtime
    let health_service = executor.health.clone();
    let compromised =
        tokio::task::spawn_blocking(move || health_service.check_hibp_single(&password)).await;

    match compromised {
        Ok(Ok(c)) => CommandResult::HibpCheckCompleted {
            record_id,
            compromised: c,
        },
        Ok(Err(e)) => CommandResult::Error {
            code: ErrorCode::Health(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.hibp_check_failed",
            fallback: format!("HIBP check failed: {}", e),
        },
        Err(e) => CommandResult::Error {
            code: ErrorCode::Health(e.to_string()),
            context: ErrorContext::default(),
            message_key: "error.hibp_check_failed",
            fallback: format!("HIBP check task panicked: {}", e),
        },
    }
}
