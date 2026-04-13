use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::rotation::RotationService;
use crate::services::vault::VaultService;
use crate::types::rotation::RotationConfig;

use super::CommandExecutor;

/// Load rotation config from vault metadata.
///
/// Returns the default config if no config is stored or if the stored
/// value is empty/corrupt.
fn load_rotation_config(vault: &VaultService) -> Result<RotationConfig, String> {
    match vault.get_metadata("rotation_config") {
        Ok(Some(json)) if !json.is_empty() => {
            serde_json::from_str(&json).map_err(|e| e.to_string())
        }
        _ => Ok(RotationConfig::default()),
    }
}

#[tracing::instrument(skip_all)]
pub fn handle_trigger_rotation(executor: &mut CommandExecutor) -> CommandResult {
    // Move VaultService out of executor using std::mem::replace with a placeholder.
    // This is required because RotationService takes ownership of VaultService.
    let placeholder_conn = rusqlite::Connection::open_in_memory()
        .expect("in-memory SQLite should never fail");
    let placeholder = VaultService::new(placeholder_conn);
    let vault = std::mem::replace(&mut executor.vault, placeholder);

    // Construct RotationService and trigger rotation.
    let mut rotation_svc = RotationService::new(vault);
    match rotation_svc.trigger_rotation() {
        Ok(result) => {
            // Move vault back into executor.
            let vault = rotation_svc.into_vault();
            executor.vault = vault;
            CommandResult::RotationCompleted {
                old_version: result.old_dek_version,
                new_version: result.new_dek_version,
                records_migrated: result.records_migrated,
            }
        }
        Err(e) => {
            // Move vault back even on failure so the executor remains usable.
            let vault = rotation_svc.into_vault();
            executor.vault = vault;
            tracing::warn!(error = %e, "DEK rotation failed");
            CommandResult::Error {
                code: ErrorCode::Rotation(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.rotation_failed",
                fallback: format!("DEK rotation failed: {}", e),
            }
        }
    }
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
