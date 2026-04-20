use crate::commands::CommandResult;
use crate::errors::{ErrorCode, ErrorContext};
use crate::services::rotation::RotationService;
use crate::services::sync::SyncService;
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

/// CAS conflict resolution per R1-A1 §4.2.
///
/// On CAS push failure, pulls cloud metadata and checks DEK version alignment.
/// Returns Ok(()) if local DEK is compatible with cloud (same or cloud accepted),
/// Err with details if an anomaly is detected.
async fn resolve_cas_conflict(
    sync_svc: &mut SyncService,
    _local_old_version: u32,
    local_new_version: u32,
) -> Result<u32, String> {
    match sync_svc.download_metadata().await {
        Ok(Some(cloud_meta)) => {
            let cloud_dek = cloud_meta.current_dek_version;
            if cloud_dek == local_new_version {
                // Both devices rotated to the same DEK version — only metadata_version
                // conflicted. Local records are already correctly encrypted.
                tracing::info!(
                    cloud_dek,
                    local_new_version,
                    "CAS conflict resolved: cloud DEK matches local, records already aligned"
                );
                Ok(cloud_dek)
            } else if cloud_dek > local_new_version {
                // Another device rotated further. Per spec §4.2 option 2:
                // accept cloud version, lazy migration will re-encrypt on next read/sync.
                tracing::warn!(
                    cloud_dek,
                    local_new_version,
                    "CAS conflict: cloud DEK is higher, lazy migration will align on next sync"
                );
                Ok(cloud_dek)
            } else {
                // cloud_dek < local_new_version: anomaly with CAS protocol
                let msg = format!(
                    "CAS conflict anomaly: cloud dek_version {} < local {}",
                    cloud_dek, local_new_version
                );
                tracing::error!(cloud_dek, local_new_version, "{}", msg);
                Err(msg)
            }
        }
        Ok(None) => {
            tracing::warn!("CAS conflict but no cloud metadata exists for alignment check");
            Ok(0)
        }
        Err(e) => {
            let msg = format!(
                "CAS conflict and failed to download cloud metadata for alignment: {}",
                e
            );
            tracing::warn!("{}", msg);
            Err(msg)
        }
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

    // 2. Fetch current cloud revision for CAS
    let expected_version = if let Some(sync_svc) = &mut executor.sync {
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

    let vault = rotation_svc.into_vault();
    executor.vault = vault;

    let result = match rotation_result {
        Ok(res) => {
            // 4. Atomic Push Metadata (CAS)
            if let Some(sync_svc) = &mut executor.sync {
                match sync_svc.download_metadata().await {
                    Ok(Some(mut meta)) => {
                        meta.current_dek_version = res.new_dek_version;
                        meta.metadata_version += 1;

                        if let Err(cas_err) =
                            sync_svc.push_metadata_atomic(meta, expected_version).await
                        {
                            // R1-A1 §4.2: CAS conflict — pull cloud version and re-align
                            let alignment = resolve_cas_conflict(
                                sync_svc,
                                res.old_dek_version,
                                res.new_dek_version,
                            )
                            .await;

                            // Resume sync before returning
                            if let Some(sync_svc) = &mut executor.sync {
                                let _ = sync_svc.resume().await;
                            }

                            return match alignment {
                                Ok(cloud_dek) => CommandResult::Error {
                                    code: ErrorCode::Sync(format!(
                                        "CAS conflict during rotation v{} -> v{}: {}. \
                                         Cloud DEK version: {}. Local records are aligned \
                                         and will sync on next cycle.",
                                        res.old_dek_version, res.new_dek_version, cas_err, cloud_dek
                                    )),
                                    context: ErrorContext::default(),
                                    message_key: "error.rotation_cas_conflict",
                                    fallback: format!(
                                        "DEK rotation v{} -> v{} completed locally but \
                                         another device updated cloud simultaneously. \
                                         Local data is safe (cloud DEK v{}). \
                                         Records will sync on next cycle.",
                                        res.old_dek_version, res.new_dek_version, cloud_dek
                                    ),
                                },
                                Err(align_err) => CommandResult::Error {
                                    code: ErrorCode::Sync(format!(
                                        "CAS conflict during rotation v{} -> v{}: {}. \
                                         Alignment check failed: {}",
                                        res.old_dek_version, res.new_dek_version, cas_err, align_err
                                    )),
                                    context: ErrorContext::default(),
                                    message_key: "error.rotation_cas_conflict",
                                    fallback: format!(
                                        "DEK rotation v{} -> v{} completed locally but \
                                         cloud push failed: {}. Alignment check also failed. \
                                         Local data is safe. Manual sync recommended.",
                                        res.old_dek_version, res.new_dek_version, cas_err
                                    ),
                                },
                            };
                        }
                    }
                    Ok(None) => {
                        // No cloud metadata — offline-first, local rotation is fine
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to download metadata for CAS push");
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

/// Resume an interrupted DEK rotation (crash recovery).
///
/// Follows the same sync mutex protocol as `handle_trigger_rotation`:
/// pause sync → fetch CAS version → resume rotation → atomic push → resume sync.
#[tracing::instrument(skip_all)]
pub async fn handle_resume_rotation(executor: &mut CommandExecutor) -> CommandResult {
    // Check if there's actually a pending checkpoint
    {
        let placeholder_conn =
            rusqlite::Connection::open_in_memory().expect("in-memory SQLite should never fail");
        let placeholder = VaultService::new(placeholder_conn);
        let vault = std::mem::replace(&mut executor.vault, placeholder);

        let rotation_svc = RotationService::new(vault);
        let has_checkpoint = match rotation_svc.has_pending_checkpoint() {
            Ok(true) => true,
            _ => false,
        };

        let vault = rotation_svc.into_vault();
        executor.vault = vault;

        if !has_checkpoint {
            return CommandResult::RotationTriggerChecked {
                should_rotate: false,
                reason: Some("no_pending_checkpoint".to_string()),
            };
        }
    }

    // 1. Sync Mutex: Pause sync pipeline
    if let Some(sync_svc) = &mut executor.sync {
        if let Err(e) = sync_svc.pause().await {
            return CommandResult::Error {
                code: ErrorCode::Sync(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.sync_pause_failed",
                fallback: format!("Failed to pause sync for rotation resume: {}", e),
            };
        }
    }

    // 2. Fetch current cloud revision for CAS
    let expected_version = if let Some(sync_svc) = &mut executor.sync {
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
                    fallback: format!("Failed to download cloud metadata for resume: {}", e),
                };
            }
        }
    } else {
        0
    };

    // 3. Resume rotation from checkpoint
    let placeholder_conn =
        rusqlite::Connection::open_in_memory().expect("in-memory SQLite should never fail");
    let placeholder = VaultService::new(placeholder_conn);
    let vault = std::mem::replace(&mut executor.vault, placeholder);

    let mut rotation_svc = RotationService::new(vault);
    let rotation_result = rotation_svc.resume_rotation();

    let vault = rotation_svc.into_vault();
    executor.vault = vault;

    let result = match rotation_result {
        Ok(res) => {
            // 4. Atomic Push Metadata (CAS)
            if let Some(sync_svc) = &mut executor.sync {
                match sync_svc.download_metadata().await {
                    Ok(Some(mut meta)) => {
                        meta.current_dek_version = res.new_dek_version;
                        meta.metadata_version += 1;

                        if let Err(cas_err) =
                            sync_svc.push_metadata_atomic(meta, expected_version).await
                        {
                            // R1-A1 §4.2: CAS conflict — pull cloud version and re-align
                            let alignment = resolve_cas_conflict(
                                sync_svc,
                                res.old_dek_version,
                                res.new_dek_version,
                            )
                            .await;

                            if let Some(sync_svc) = &mut executor.sync {
                                let _ = sync_svc.resume().await;
                            }

                            return match alignment {
                                Ok(cloud_dek) => CommandResult::Error {
                                    code: ErrorCode::Sync(format!(
                                        "CAS conflict during rotation resume v{} -> v{}: {}. \
                                         Cloud DEK version: {}. Local records are aligned \
                                         and will sync on next cycle.",
                                        res.old_dek_version, res.new_dek_version, cas_err, cloud_dek
                                    )),
                                    context: ErrorContext::default(),
                                    message_key: "error.rotation_cas_conflict",
                                    fallback: format!(
                                        "DEK rotation resume v{} -> v{} completed locally but \
                                         another device updated cloud simultaneously. \
                                         Local data is safe (cloud DEK v{}). \
                                         Records will sync on next cycle.",
                                        res.old_dek_version, res.new_dek_version, cloud_dek
                                    ),
                                },
                                Err(align_err) => CommandResult::Error {
                                    code: ErrorCode::Sync(format!(
                                        "CAS conflict during rotation resume v{} -> v{}: {}. \
                                         Alignment check failed: {}",
                                        res.old_dek_version, res.new_dek_version, cas_err, align_err
                                    )),
                                    context: ErrorContext::default(),
                                    message_key: "error.rotation_cas_conflict",
                                    fallback: format!(
                                        "DEK rotation resume v{} -> v{} completed locally but \
                                         cloud push failed: {}. Alignment check also failed. \
                                         Local data is safe. Manual sync recommended.",
                                        res.old_dek_version, res.new_dek_version, cas_err
                                    ),
                                },
                            };
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to download metadata for CAS push during resume");
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
            tracing::warn!(error = %e, "DEK rotation resume failed");
            CommandResult::Error {
                code: ErrorCode::Rotation(e.to_string()),
                context: ErrorContext::default(),
                message_key: "error.rotation_resume_failed",
                fallback: format!("DEK rotation resume failed: {}", e),
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
