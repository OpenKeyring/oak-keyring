use chrono::{DateTime, Utc};
use std::collections::HashMap;
use uuid::Uuid;

use crate::commands::types::{FieldSelector, HealthReport};
use crate::commands::{CommandResult, Message};
use crate::config::ConfigManager;
use crate::errors::mapping::vault::VaultError;
use crate::errors::{ErrorCode, ErrorContext, ServiceError};
use crate::services::health::{
    detect_duplicate_passwords, detect_expired_records, detect_weak_passwords, PasswordEntry,
};
use crate::types::credential::CredentialType;
use crate::types::health::{HealthStateDelta, RecordHealthState};
use crate::types::record::StoredRecord;

use super::CommandExecutor;

// ---------------------------------------------------------------------------
// HIBP skip logic (spec section 8.2)
// ---------------------------------------------------------------------------

/// Determine whether a record can skip the HIBP check based on persisted
/// health state.
///
/// Per spec section 8.2, a record is skipped when ALL of these hold:
/// 1. A persisted `RecordHealthState` exists for this record
/// 2. `record_health_state.record_version == record_version`
/// 3. `compromised == Some(true)`
///
/// The password has not changed (version matches), and it was already known
/// to be compromised — re-checking would produce the same result.
pub fn should_skip_hibp(health_state: Option<&RecordHealthState>, record_version: u64) -> bool {
    match health_state {
        Some(state) => state.record_version == record_version && state.compromised == Some(true),
        None => false,
    }
}

/// Partition entries into those that can skip HIBP and those that need it.
///
/// Returns `(skipped_ids, entries_to_check)`.
/// - `skipped_ids`: record IDs where `should_skip_hibp` returned true;
///   these are already known-compromised and are added directly to the
///   compromised result list.
/// - `entries_to_check`: remaining `PasswordEntry`s that need a live HIBP call.
pub fn partition_entries_for_hibp(
    entries: Vec<PasswordEntry>,
    record_versions: &HashMap<Uuid, u64>,
    health_states: &HashMap<Uuid, RecordHealthState>,
) -> (Vec<Uuid>, Vec<PasswordEntry>) {
    let mut skipped_ids = Vec::new();
    let mut entries_to_check = Vec::new();

    for entry in entries {
        let version = record_versions.get(&entry.id).copied().unwrap_or(0);
        let state = health_states.get(&entry.id);

        if should_skip_hibp(state, version) {
            skipped_ids.push(entry.id);
            // entry (SecureStr) is dropped here, zeroizing the password.
        } else {
            entries_to_check.push(entry);
        }
    }

    (skipped_ids, entries_to_check)
}

// ---------------------------------------------------------------------------
// Health-check write-back functions (Task E)
// ---------------------------------------------------------------------------

/// Pure function to project a `HealthReport` into per-record `RecordHealthState`s.
///
/// For each active record, determines its health attributes from the report:
///
/// - `weak_password` = `true` if the record is in `report.weak_passwords`
/// - `duplicate_group_size` = group size if the record appears in any duplicate
///   group, `None` otherwise
/// - `compromised` = `true` if the record is in `report.compromised`
/// - `expired` = `true` if the record is in `report.expired`
/// - `evaluated_at` = the provided timestamp
/// - `record_version` = the record's current version
///
/// Records that have no health issues still get a state row with all flags set
/// to `Some(false)` / `None` so the UI can distinguish "not yet evaluated" from
/// "evaluated as clean".
pub fn project_health_report_to_states(
    records: &[StoredRecord],
    report: &HealthReport,
    evaluated_at: DateTime<Utc>,
) -> Vec<RecordHealthState> {
    // Build lookup maps for O(1) membership tests.
    let weak_set: HashMap<Uuid, bool> =
        report.weak_passwords.iter().map(|id| (*id, true)).collect();

    // Build duplicate-group-size map: each record in a group gets group.len().
    let mut dup_size_map: HashMap<Uuid, usize> = HashMap::new();
    for group in &report.duplicate_passwords {
        let size = group.len();
        for id in group {
            dup_size_map.insert(*id, size);
        }
    }

    let compromised_set: HashMap<Uuid, bool> =
        report.compromised.iter().map(|id| (*id, true)).collect();

    let expired_set: HashMap<Uuid, bool> = report.expired.iter().map(|id| (*id, true)).collect();

    records
        .iter()
        .map(|rec| RecordHealthState {
            record_id: rec.id,
            record_version: rec.version,
            evaluated_at: Some(evaluated_at),
            weak_password: Some(weak_set.contains_key(&rec.id)),
            duplicate_group_size: dup_size_map.get(&rec.id).copied(),
            compromised: Some(compromised_set.contains_key(&rec.id)),
            expired: Some(expired_set.contains_key(&rec.id)),
        })
        .collect()
}

/// Compute the delta between old and new health states for each record.
///
/// A delta is considered "changed" if any of the comparison fields differ:
/// `weak_password`, `duplicate_group_size`, `compromised`, `expired`.
pub(crate) fn health_state_changed(before: &RecordHealthState, after: &RecordHealthState) -> bool {
    before.weak_password != after.weak_password
        || before.duplicate_group_size != after.duplicate_group_size
        || before.compromised != after.compromised
        || before.expired != after.expired
}

/// Project `HealthReport` to per-record states and persist to the database.
///
/// Reads existing health states, computes new states via projection, performs a
/// transactional batch replace, and returns a list of deltas for records whose
/// health attributes actually changed.
///
/// # Errors
///
/// Returns `VaultError` if any database operation fails. The entire replace
/// operation is atomic (wrapped in a transaction by `replace_record_health_states`).
pub fn persist_health_report(
    executor: &mut CommandExecutor,
    report: &HealthReport,
    evaluated_at: DateTime<Utc>,
) -> Result<Vec<HealthStateDelta>, VaultError> {
    // Fetch all active records for projection.
    let records = executor.vault.list_all_stored_records()?;

    // Fetch existing health states to compute deltas.
    let old_states = executor.vault.list_record_health_states()?;
    let old_map: HashMap<Uuid, RecordHealthState> =
        old_states.into_iter().map(|s| (s.record_id, s)).collect();

    // Project report into new states.
    let new_states = project_health_report_to_states(&records, report, evaluated_at);

    // Compute deltas — only include records where health attributes changed.
    let deltas: Vec<HealthStateDelta> = new_states
        .iter()
        .filter_map(|after| {
            let before = old_map.get(&after.record_id).cloned();
            let changed = match &before {
                Some(b) => health_state_changed(b, after),
                None => true, // No previous state = new evaluation = changed
            };
            if changed {
                Some(HealthStateDelta {
                    record_id: after.record_id,
                    before,
                    after: Some(after.clone()),
                })
            } else {
                None
            }
        })
        .collect();

    // Transactional batch replace.
    executor.vault.replace_record_health_states(&new_states)?;

    Ok(deltas)
}

/// For records with changed health attributes, mark them as pending sync.
///
/// Health-only changes set `sync_state = Pending` so the sync pipeline
/// propagates the updated health attributes to the cloud. The record's
/// `version` is NOT bumped — health state changes do not represent content
/// changes, so bumping version would break the HIBP skip logic (the persisted
/// `record_health_state.record_version` would no longer match `records.version`).
pub fn schedule_health_resync_for_records(
    executor: &mut CommandExecutor,
    deltas: &[HealthStateDelta],
) -> Result<(), VaultError> {
    if deltas.is_empty() {
        return Ok(());
    }

    let record_ids: Vec<Uuid> = deltas.iter().map(|d| d.record_id).collect();

    // Mark sync_state = Pending for the next sync cycle.
    executor.vault.mark_records_pending_sync(&record_ids)?;

    tracing::info!(
        count = record_ids.len(),
        "Marked records as pending sync due to health attribute changes"
    );

    Ok(())
}

/// Load cached health report from persisted `record_health_state` rows.
///
/// Rebuilds a `HealthReport` by projecting the tri-state flags stored in the
/// `record_health_state` table back into the aggregate report structure used by
/// the UI. Returns `Ok(None)` when no persisted states exist.
///
/// # Projection rules (spec section 9)
///
/// - `weak_passwords`      = all record_ids where `weak_password == Some(true)`
/// - `duplicate_passwords` = single group of all record_ids where `duplicate_group_size >= 2`
/// - `compromised`         = all record_ids where `compromised == Some(true)`
/// - `expired`             = all record_ids where `expired == Some(true)`
/// - `total_checked`       = total number of records with a health state row
///
/// # Note on duplicate groups
///
/// The persisted state only stores `duplicate_group_size` per record, so we
/// cannot reconstruct the exact group boundaries. All duplicate records are
/// placed in a single group, which preserves "is duplicate" semantics for UI
/// filtering but loses the exact grouping.
pub fn load_cached_health_report(
    executor: &mut CommandExecutor,
) -> Result<Option<HealthReport>, VaultError> {
    let states = executor.vault.list_record_health_states()?;

    if states.is_empty() {
        return Ok(None);
    }

    let total_checked = states.len();

    let weak_passwords: Vec<Uuid> = states
        .iter()
        .filter(|s| s.weak_password == Some(true))
        .map(|s| s.record_id)
        .collect();

    let duplicate_ids: Vec<Uuid> = states
        .iter()
        .filter(|s| s.duplicate_group_size.is_some_and(|sz| sz >= 2))
        .map(|s| s.record_id)
        .collect();

    // Single group containing all duplicates — exact group boundaries are lost
    // after persistence since we only store group size per record.
    let duplicate_passwords: Vec<Vec<Uuid>> = if duplicate_ids.is_empty() {
        Vec::new()
    } else {
        vec![duplicate_ids]
    };

    let compromised: Vec<Uuid> = states
        .iter()
        .filter(|s| s.compromised == Some(true))
        .map(|s| s.record_id)
        .collect();

    let expired: Vec<Uuid> = states
        .iter()
        .filter(|s| s.expired == Some(true))
        .map(|s| s.record_id)
        .collect();

    Ok(Some(HealthReport {
        weak_passwords,
        duplicate_passwords,
        compromised,
        expired,
        total_checked,
    }))
}

#[tracing::instrument(skip_all)]
pub fn handle_run_health_check(executor: &mut CommandExecutor, force: bool) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("health_check");
    }

    // When force == false (unlock auto-scheduling), respect the frequency gate.
    // When force == true (mutation/import-triggered), skip the gate entirely.
    if !force {
        // Check if health check should run (enabled + frequency).
        // Uses the actual last check time recorded when the previous check completed.
        let config = executor.config.get_config();
        if !crate::services::health::should_run(&config.security, executor.last_health_check_time) {
            return CommandResult::HealthCheckSkipped;
        }
    }

    // Step 1: Fetch all active stored records (fast, local)
    let records = match executor.vault.list_all_stored_records() {
        Ok(r) => r,
        Err(e) => {
            let err: &dyn ServiceError = &e;
            return CommandResult::Error {
                code: err.to_error_code(),
                context: err.to_error_context(),
                message_key: "error.list_records_failed",
                fallback: format!("Failed to list records for health check: {}", e),
            };
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

    // Step 4: Load persisted health states and partition entries.
    //
    // Records where compromised == true AND record_version matches can skip
    // the HIBP call (spec section 8.2). We separate them before spawning
    // the background task so the async closure does not need DB access.
    let health_states = executor
        .vault
        .list_record_health_states()
        .unwrap_or_default();
    let health_map: HashMap<Uuid, RecordHealthState> = health_states
        .into_iter()
        .map(|s| (s.record_id, s))
        .collect();

    // Build version lookup from the records list for O(1) access.
    let record_versions: HashMap<Uuid, u64> = records.iter().map(|r| (r.id, r.version)).collect();

    let (skipped_compromised, entries_to_check) =
        partition_entries_for_hibp(entries, &record_versions, &health_map);

    if !skipped_compromised.is_empty() {
        tracing::info!(
            skip_count = skipped_compromised.len(),
            check_count = entries_to_check.len(),
            "Skipping HIBP for records with unchanged compromised state"
        );
    }

    // Step 5: Prepare background task for HIBP check (slow, network)
    let tx = executor.result_tx.clone();
    let self_tx = executor.internal_tx.clone(); // Self-sender for internal caching
    let health_service = executor.health.clone();
    let cancel_token = executor.cancel_token().clone();

    // Spawn the background task for HIBP check and final report assembly
    tokio::spawn(async move {
        // Pre-populate compromised list with records that were skipped.
        let mut compromised = skipped_compromised;
        let total = entries_to_check.len();

        // Security: Use into_iter to ensure each entry (SecureStr) is dropped
        // and zeroized IMMEDIATELY after its individual check is done.
        let mut entries_iter = entries_to_check.into_iter().enumerate();

        // 100ms rate limit ticker as recommended
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(100));

        loop {
            tokio::select! {
                biased;

                _ = cancel_token.cancelled() => {
                    tracing::info!("Health check cancelled: clearing remaining memory");
                    let _ = tx.send(Message::CommandCompleted(
                        CommandResult::cancelled("health_check")
                    )).await;
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
            .send(crate::commands::InternalCommand::HealthCheckCompleted { report })
            .await;
    });

    // Step 6: Return immediate "Started" result
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
                code: ErrorCode::CryptoDecryptionFailed,
                context: ErrorContext::default(),
                message_key: "error.decrypt_field_failed",
                fallback: format!("Failed to decrypt password for HIBP check: {}", e),
            };
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
            code: ErrorCode::HealthHibpApiError,
            context: ErrorContext::default(),
            message_key: "error.hibp_check_failed",
            fallback: format!("HIBP check failed: {}", e),
        },
        Err(e) => CommandResult::Error {
            code: ErrorCode::HealthHibpApiError,
            context: ErrorContext::default(),
            message_key: "error.hibp_check_failed",
            fallback: format!("HIBP check task panicked: {}", e),
        },
    }
}
