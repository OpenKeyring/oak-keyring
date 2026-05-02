use uuid::Uuid;

use crate::commands::types::{FieldSelector, HealthReport};
use crate::commands::{CommandResult, Message};
use crate::errors::mapping::vault::VaultError;
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::health::{
    detect_duplicate_passwords, detect_expired_records, detect_weak_passwords, PasswordEntry,
};
use crate::types::credential::CredentialType;

use super::CommandExecutor;

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
pub fn handle_run_health_check(executor: &mut CommandExecutor) -> CommandResult {
    if executor.cancel_token().is_cancelled() {
        return CommandResult::cancelled("health_check");
    }

    // Check if health check should run (enabled + frequency).
    // Uses the actual last check time recorded when the previous check completed.
    if !crate::services::health::should_run(
        &executor.config.security,
        executor.last_health_check_time,
    ) {
        return CommandResult::HealthCheckSkipped;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use crate::crypto::bip39::{MnemonicLanguage, Passkey};
    use crate::executor::config_impl::ServiceNotificationImpl;
    use crate::services::clipboard::{ClipboardService, MockBackend};
    use crate::services::health::HealthService;
    use crate::services::import_export::ImportExportService;
    use crate::services::vault::VaultService;
    use crate::types::{CredentialType, EncryptedPayload, SecureStr};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn make_executor_with_one_login() -> CommandExecutor {
        let conn = crate::db::schema::init_db_in_memory();
        let mut vault = VaultService::new(conn);
        let mnemonic = Passkey::generate(24, MnemonicLanguage::English).expect("mnemonic");
        vault
            .unlock_with_mnemonic(&mnemonic)
            .expect("unlock with mnemonic");
        vault
            .create_record(crate::types::record::CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: "Example".to_string(),
                    username: "alice".to_string(),
                    password: SecureStr::new("password123".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("record");

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
            last_health_check_time: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            cancel_token: CancellationToken::new(),
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    #[tokio::test]
    async fn health_background_cancel_sends_cancelled_result() {
        let mut executor = make_executor_with_one_login();
        let mut result_rx = {
            let (result_tx, result_rx) = mpsc::channel(64);
            executor.result_tx = result_tx;
            result_rx
        };

        let started = handle_run_health_check(&mut executor);
        assert!(matches!(started, CommandResult::HealthCheckStarted));
        executor.cancel_token().cancel();

        let message = tokio::time::timeout(std::time::Duration::from_secs(1), result_rx.recv())
            .await
            .expect("health cancellation message")
            .expect("message");

        assert!(matches!(
            message,
            Message::CommandCompleted(CommandResult::Cancelled { ref operation, .. })
                if operation == "health_check"
        ));
    }

    // -- load_cached_health_report tests --------------------------------------

    /// Helper: create an executor with an unlocked vault (no records).
    fn make_executor_no_records() -> CommandExecutor {
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
            last_health_check_time: None,
            result_tx,
            internal_tx,
            internal_rx: Some(internal_rx),
            cancel_token: CancellationToken::new(),
            oauth2_token_store: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Helper: create a Login record and return its UUID.
    fn create_login_record(executor: &mut CommandExecutor, name: &str) -> Uuid {
        executor
            .vault
            .create_record(crate::types::record::CreateRecordParams {
                credential_type: CredentialType::Login,
                payload: EncryptedPayload::Login {
                    name: name.to_string(),
                    username: format!("user_{}", name),
                    password: SecureStr::new("password123".to_string()),
                    url: None,
                    notes: None,
                },
                tags: vec![],
                is_favorite: false,
                expires_at: None,
            })
            .expect("create record")
    }

    /// Helper: insert a health state via the VaultService wrapper.
    fn insert_health_state(
        executor: &mut CommandExecutor,
        state: crate::types::health::RecordHealthState,
    ) {
        executor
            .vault
            .upsert_record_health_state(&state)
            .expect("insert health state");
    }

    #[test]
    fn load_cached_report_returns_none_when_no_states() {
        let mut executor = make_executor_no_records();
        let result = load_cached_health_report(&mut executor).expect("load");
        assert!(
            result.is_none(),
            "empty DB should yield Ok(None), got {:?}",
            result
        );
    }

    #[test]
    fn load_cached_report_reconstructs_weak_passwords() {
        let mut executor = make_executor_no_records();
        let id_weak = create_login_record(&mut executor, "weak");
        let id_clean = create_login_record(&mut executor, "clean");

        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_weak,
                record_version: 1,
                evaluated_at: None,
                weak_password: Some(true),
                duplicate_group_size: None,
                compromised: None,
                expired: None,
            },
        );
        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_clean,
                record_version: 1,
                evaluated_at: None,
                weak_password: Some(false),
                duplicate_group_size: None,
                compromised: None,
                expired: None,
            },
        );

        let report = load_cached_health_report(&mut executor)
            .expect("load")
            .expect("report");

        assert_eq!(report.weak_passwords, vec![id_weak]);
        assert_eq!(report.total_checked, 2);
        assert!(report.compromised.is_empty());
        assert!(report.expired.is_empty());
        assert!(report.duplicate_passwords.is_empty());
    }

    #[test]
    fn load_cached_report_reconstructs_compromised() {
        let mut executor = make_executor_no_records();
        let id_compromised = create_login_record(&mut executor, "compromised");
        let id_safe = create_login_record(&mut executor, "safe");

        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_compromised,
                record_version: 1,
                evaluated_at: None,
                weak_password: None,
                duplicate_group_size: None,
                compromised: Some(true),
                expired: None,
            },
        );
        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_safe,
                record_version: 1,
                evaluated_at: None,
                weak_password: None,
                duplicate_group_size: None,
                compromised: Some(false),
                expired: None,
            },
        );

        let report = load_cached_health_report(&mut executor)
            .expect("load")
            .expect("report");

        assert_eq!(report.compromised, vec![id_compromised]);
    }

    #[test]
    fn load_cached_report_reconstructs_expired() {
        let mut executor = make_executor_no_records();
        let id_expired = create_login_record(&mut executor, "expired");

        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_expired,
                record_version: 1,
                evaluated_at: None,
                weak_password: None,
                duplicate_group_size: None,
                compromised: None,
                expired: Some(true),
            },
        );

        let report = load_cached_health_report(&mut executor)
            .expect("load")
            .expect("report");

        assert_eq!(report.expired, vec![id_expired]);
    }

    #[test]
    fn load_cached_report_reconstructs_duplicates_as_single_group() {
        let mut executor = make_executor_no_records();
        let id_dup1 = create_login_record(&mut executor, "dup1");
        let id_dup2 = create_login_record(&mut executor, "dup2");
        let id_unique = create_login_record(&mut executor, "unique");

        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_dup1,
                record_version: 1,
                evaluated_at: None,
                weak_password: None,
                duplicate_group_size: Some(2),
                compromised: None,
                expired: None,
            },
        );
        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_dup2,
                record_version: 1,
                evaluated_at: None,
                weak_password: None,
                duplicate_group_size: Some(2),
                compromised: None,
                expired: None,
            },
        );
        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_unique,
                record_version: 1,
                evaluated_at: None,
                weak_password: None,
                duplicate_group_size: Some(1),
                compromised: None,
                expired: None,
            },
        );

        let report = load_cached_health_report(&mut executor)
            .expect("load")
            .expect("report");

        assert_eq!(
            report.duplicate_passwords.len(),
            1,
            "should have exactly one group"
        );
        let group = &report.duplicate_passwords[0];
        assert_eq!(
            group.len(),
            2,
            "group should contain both duplicate records"
        );
        assert!(group.contains(&id_dup1));
        assert!(group.contains(&id_dup2));
    }

    #[test]
    fn load_cached_report_ignores_none_weak_password() {
        let mut executor = make_executor_no_records();
        let id = create_login_record(&mut executor, "unevaluated");

        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id,
                record_version: 1,
                evaluated_at: None,
                weak_password: None, // not yet evaluated
                duplicate_group_size: None,
                compromised: None,
                expired: None,
            },
        );

        let report = load_cached_health_report(&mut executor)
            .expect("load")
            .expect("report");

        assert!(
            report.weak_passwords.is_empty(),
            "None (not evaluated) should not be treated as weak"
        );
        assert_eq!(report.total_checked, 1);
    }

    #[test]
    fn load_cached_report_combines_all_categories() {
        let mut executor = make_executor_no_records();
        let id_all_issues = create_login_record(&mut executor, "all_issues");
        let id_clean = create_login_record(&mut executor, "clean");

        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_all_issues,
                record_version: 1,
                evaluated_at: None,
                weak_password: Some(true),
                duplicate_group_size: Some(2),
                compromised: Some(true),
                expired: Some(true),
            },
        );
        insert_health_state(
            &mut executor,
            crate::types::health::RecordHealthState {
                record_id: id_clean,
                record_version: 1,
                evaluated_at: None,
                weak_password: Some(false),
                duplicate_group_size: Some(1),
                compromised: Some(false),
                expired: Some(false),
            },
        );

        let report = load_cached_health_report(&mut executor)
            .expect("load")
            .expect("report");

        assert_eq!(report.total_checked, 2);
        assert_eq!(report.weak_passwords, vec![id_all_issues]);
        assert_eq!(report.compromised, vec![id_all_issues]);
        assert_eq!(report.expired, vec![id_all_issues]);
        // duplicate group: only id_all_issues has group_size >= 2
        assert_eq!(report.duplicate_passwords.len(), 1);
        assert!(report.duplicate_passwords[0].contains(&id_all_issues));
    }
}
