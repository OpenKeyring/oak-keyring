use std::time::Duration;

use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::rotation::RotationService;
use crate::services::vault::VaultService;
use crate::types::rotation::RotationConfig;

use super::CommandExecutor;

/// Load rotation config from vault metadata.
fn load_rotation_config(vault: &VaultService) -> Result<RotationConfig, String> {
    match vault.get_metadata("rotation_config") {
        Ok(Some(json)) if !json.is_empty() => {
            serde_json::from_str(&json).map_err(|e| e.to_string())
        }
        _ => Ok(RotationConfig::default()),
    }
}

#[tracing::instrument(skip_all)]
pub async fn handle_trigger_rotation(executor: &mut CommandExecutor) -> CommandResult {
    // 1. Sync Mutex: Pause sync pipeline
    if let Some(sync_svc) = &mut executor.sync {
        if let Err(e) = sync_svc.pause().await {
            return CommandResult::Error {
                code: ErrorCode::Sync(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.sync_pause_failed",
                fallback: format!("Failed to pause sync for rotation: {}", e),
            };
        }
    }

    // Ensure sync is resumed regardless of outcome
    // (In a real implementation, we'd use a ScopeGuard, but here we'll handle manually)

    // 2. Fetch current cloud revision for CAS
    let expected_version = if let Some(sync_svc) = &executor.sync {
        match sync_svc.download_metadata().await {
            Ok(Some(meta)) => meta.metadata_version,
            Ok(None) => 0,
            Err(e) => {
                if let Some(sync_svc) = &mut executor.sync {
                    let _ = sync_svc.resume().await;
                }
                return CommandResult::Error {
                    code: ErrorCode::Sync(e.to_string()),
                    context: ErrorContext::default(),
                    message_key: "error.download_metadata_failed",
                    fallback: format!("Failed to download cloud metadata: {}", e),
                };
            }
        }
    } else {
        0
    };

    // 3. Perform Local Rotation
    let placeholder_conn =
        rusqlite::Connection::open_in_memory().expect("in-memory SQLite should never fail");
    let placeholder = VaultService::new(placeholder_conn);
    let vault = std::mem::replace(&mut executor.vault, placeholder);

    let mut rotation_svc = RotationService::new(vault);
    let rotation_result = rotation_svc.trigger_rotation(expected_version);

    // Move vault back
    let vault = rotation_svc.into_vault();
    executor.vault = vault;

    let result = match rotation_result {
        Ok(res) => {
            // 4. Atomic Push Metadata (CAS)
            if let Some(sync_svc) = &mut executor.sync {
                if let Ok(Some(mut meta)) = sync_svc.download_metadata().await {
                    meta.current_dek_version = res.new_dek_version;
                    meta.metadata_version += 1;

                    if let Err(e) = sync_svc.push_metadata_atomic(meta, expected_version).await {
                        tracing::error!(error = %e, "Atomic push metadata failed after rotation");
                        // Note: Local rotation is committed, but cloud is out of sync.
                        // Lazy migration on other devices will eventually reconcile this.
                    }
                }
            }

            CommandResult::RotationCompleted {
                old_version: res.old_dek_version,
                new_version: res.new_dek_version,
                records_migrated: res.records_migrated,
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, "DEK rotation failed");
            CommandResult::Error {
                code: ErrorCode::Rotation(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.rotation_failed",
                fallback: format!("DEK rotation failed: {}", e),
            }
        }
    };

    // 5. Resume sync
    if let Some(sync_svc) = &mut executor.sync {
        let _ = sync_svc.resume().await;
    }

    result
}

#[tracing::instrument(skip_all)]
pub fn handle_check_rotation_trigger(executor: &mut CommandExecutor) -> CommandResult {
    // Load rotation config from vault metadata.
    let config = match load_rotation_config(&executor.vault) {
        Ok(c) => c,
        Err(e) => {
            return CommandResult::Error {
                code: ErrorCode::Rotation(e),
                context: ErrorContext::default(),
                message_key: "error.rotation_config_load_failed",
                fallback: String::from("Failed to load rotation config"),
            };
        }
    };

    let online = executor.sync.is_some();

    let days_since = config.last_rotation_at.map(|last| {
        let duration = chrono::Utc::now().signed_duration_since(last);
        duration.num_days().max(0) as u32
    });

    let trigger = crate::services::rotation::check_trigger(
        &config,
        online,
        days_since,
        config.current_dek_record_count,
    );

    match trigger {
        Some(t) => CommandResult::RotationTriggerChecked {
            should_rotate: true,
            reason: Some(format!("{:?}", t)),
        },
        None => CommandResult::RotationTriggerChecked {
            should_rotate: false,
            reason: None,
        },
    }
}
