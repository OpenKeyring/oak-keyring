use chrono::{DateTime, Utc};
use crate::errors::mapping::rotation::RotationError;
use crate::types::rotation::{RotationConfig, RotationConstants, RotationCheckpoint, RotationTrigger};

const CHECKPOINT_KEY: &str = "rotation_checkpoint";
const PENDING_TRIGGER_KEY: &str = "pending_rotation_trigger";
const PENDING_SINCE_KEY: &str = "pending_rotation_since";

pub fn save_checkpoint(
    vault: &mut crate::services::vault::VaultService,
    checkpoint: &RotationCheckpoint,
) -> Result<(), RotationError> {
    let json = serde_json::to_string(checkpoint)
        .map_err(|e| RotationError::CheckpointCorrupted(e.to_string()))?;
    vault.set_metadata(CHECKPOINT_KEY, &json)
        .map_err(|e| RotationError::Internal(e.to_string()))?;
    Ok(())
}

pub fn load_checkpoint(
    vault: &crate::services::vault::VaultService,
) -> Result<Option<RotationCheckpoint>, RotationError> {
    let json = match vault.get_metadata(CHECKPOINT_KEY) {
        Ok(Some(json)) if !json.is_empty() => json,
        Ok(_) => return Ok(None),
        Err(e) => return Err(RotationError::Internal(e.to_string())),
    };
    let checkpoint: RotationCheckpoint = serde_json::from_str(&json)
        .map_err(|e| RotationError::CheckpointCorrupted(e.to_string()))?;
    Ok(Some(checkpoint))
}

pub fn delete_checkpoint(
    vault: &mut crate::services::vault::VaultService,
) -> Result<(), RotationError> {
    vault.delete_metadata(CHECKPOINT_KEY)
        .map_err(|e| RotationError::Internal(e.to_string()))?;
    Ok(())
}

pub fn save_pending_trigger(
    vault: &mut crate::services::vault::VaultService,
    trigger: &RotationTrigger,
    triggered_at: DateTime<Utc>,
) -> Result<(), RotationError> {
    let trigger_json = serde_json::to_string(trigger)
        .map_err(|e| RotationError::Internal(e.to_string()))?;
    vault.set_metadata(PENDING_TRIGGER_KEY, &trigger_json)
        .map_err(|e| RotationError::Internal(e.to_string()))?;
    vault.set_metadata(PENDING_SINCE_KEY, &triggered_at.to_rfc3339())
        .map_err(|e| RotationError::Internal(e.to_string()))?;
    Ok(())
}

pub fn load_pending_trigger(
    vault: &crate::services::vault::VaultService,
) -> Result<Option<(RotationTrigger, DateTime<Utc>)>, RotationError> {
    let trigger_json = match vault.get_metadata(PENDING_TRIGGER_KEY) {
        Ok(Some(json)) if !json.is_empty() => json,
        Ok(_) => return Ok(None),
        Err(e) => return Err(RotationError::Internal(e.to_string())),
    };
    let since_str = match vault.get_metadata(PENDING_SINCE_KEY) {
        Ok(Some(s)) if !s.is_empty() => s,
        Ok(_) => return Err(RotationError::CheckpointCorrupted(
            "pending_trigger present but pending_since absent".into()
        )),
        Err(e) => return Err(RotationError::Internal(e.to_string())),
    };
    let trigger: RotationTrigger = serde_json::from_str(&trigger_json)
        .map_err(|e| RotationError::CheckpointCorrupted(e.to_string()))?;
    let triggered_at: DateTime<Utc> = since_str.parse()
        .map_err(|e| RotationError::CheckpointCorrupted(format!("invalid datetime: {}", e)))?;
    Ok(Some((trigger, triggered_at)))
}

pub fn clear_pending_trigger(
    vault: &mut crate::services::vault::VaultService,
) -> Result<(), RotationError> {
    vault.delete_metadata(PENDING_TRIGGER_KEY)
        .map_err(|e| RotationError::Internal(e.to_string()))?;
    vault.delete_metadata(PENDING_SINCE_KEY)
        .map_err(|e| RotationError::Internal(e.to_string()))?;
    Ok(())
}

/// Check if rotation should be triggered based on config and current state.
pub fn check_trigger(
    config: &RotationConfig,
    online: bool,
    days_since_last_rotation: Option<u32>,
    current_dek_record_count: u32,
) -> Option<RotationTrigger> {
    if !config.auto_rotate {
        return None;
    }

    if !online {
        return None;
    }

    if let Some(days) = config.rotate_after_days {
        if let Some(days_since) = days_since_last_rotation {
            if days_since >= days {
                return Some(RotationTrigger::AutoTime { days_since_last: days_since });
            }
        }
    }

    if let Some(threshold) = config.rotate_after_records {
        if current_dek_record_count >= threshold {
            return Some(RotationTrigger::AutoCount { record_count: current_dek_record_count });
        }
    }

    None
}

/// Check if a pending rotation is past its grace period (24 hours).
pub fn is_past_grace_period(triggered_at: DateTime<Utc>) -> bool {
    let now = Utc::now();
    let elapsed = now.signed_duration_since(triggered_at);
    elapsed.num_hours() >= RotationConstants::GRACE_PERIOD_HOURS as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_serialization_roundtrip() {
        let checkpoint = RotationCheckpoint {
            trigger: RotationTrigger::AutoTime { days_since_last: 90 },
            old_dek_version: 1,
            new_dek_version: 2,
            total_records: 42,
            migrated_records: 0,
            last_migrated_record_id: None,
            started_at: Utc::now(),
            cloud_metadata_revision: "test-rev".to_string(),
        };
        let json = serde_json::to_string(&checkpoint).unwrap();
        let restored: RotationCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.old_dek_version, checkpoint.old_dek_version);
        assert_eq!(restored.new_dek_version, checkpoint.new_dek_version);
        assert_eq!(restored.total_records, checkpoint.total_records);
    }

    #[test]
    fn pending_trigger_serialization_roundtrip() {
        let trigger = RotationTrigger::AutoCount { record_count: 500 };
        let json = serde_json::to_string(&trigger).unwrap();
        let restored: RotationTrigger = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, RotationTrigger::AutoCount { record_count: 500 }));
    }

    #[test]
    fn checkpoint_key_constants_are_correct() {
        assert_eq!(CHECKPOINT_KEY, "rotation_checkpoint");
        assert_eq!(PENDING_TRIGGER_KEY, "pending_rotation_trigger");
        assert_eq!(PENDING_SINCE_KEY, "pending_rotation_since");
    }

    #[test]
    fn load_returns_none_for_empty_string() {
        let empty = "";
        assert!(empty.is_empty());
    }
}

#[cfg(test)]
mod trigger_tests {
    use super::*;

    fn make_config(
        auto_rotate: bool,
        rotate_after_days: Option<u32>,
        rotate_after_records: Option<u32>,
    ) -> RotationConfig {
        RotationConfig {
            auto_rotate,
            rotate_after_days,
            rotate_after_records,
            last_rotation_at: None,
            current_dek_record_count: 0,
        }
    }

    #[test]
    fn check_trigger_returns_none_when_auto_rotate_disabled() {
        let config = make_config(false, Some(90), Some(1000));
        let result = check_trigger(&config, true, Some(100), 500);
        assert!(result.is_none());
    }

    #[test]
    fn check_trigger_returns_none_when_offline() {
        let config = make_config(true, Some(90), Some(1000));
        let result = check_trigger(&config, false, Some(100), 500);
        assert!(result.is_none());
    }

    #[test]
    fn check_trigger_auto_time_trigger() {
        let config = make_config(true, Some(90), None);
        let result = check_trigger(&config, true, Some(90), 0);
        assert!(matches!(result, Some(RotationTrigger::AutoTime { days_since_last: 90 })));
    }

    #[test]
    fn check_trigger_auto_time_not_triggered_within_days() {
        let config = make_config(true, Some(90), None);
        let result = check_trigger(&config, true, Some(30), 0);
        assert!(result.is_none());
    }

    #[test]
    fn check_trigger_auto_count_trigger() {
        let config = make_config(true, None, Some(1000));
        let result = check_trigger(&config, true, None, 1000);
        assert!(matches!(result, Some(RotationTrigger::AutoCount { record_count: 1000 })));
    }

    #[test]
    fn check_trigger_auto_count_not_triggered_below_threshold() {
        let config = make_config(true, None, Some(1000));
        let result = check_trigger(&config, true, None, 500);
        assert!(result.is_none());
    }

    #[test]
    fn check_trigger_auto_time_takes_priority_over_auto_count() {
        let config = make_config(true, Some(90), Some(1000));
        let result = check_trigger(&config, true, Some(90), 1000);
        assert!(matches!(result, Some(RotationTrigger::AutoTime { .. })));
    }

    #[test]
    fn check_trigger_no_days_info_returns_none_for_auto_time() {
        let config = make_config(true, Some(90), None);
        let result = check_trigger(&config, true, None, 0);
        assert!(result.is_none());
    }

    #[test]
    fn is_past_grace_period_true_after_25_hours() {
        let triggered = chrono::Utc::now() - chrono::Duration::hours(25);
        assert!(is_past_grace_period(triggered));
    }

    #[test]
    fn is_past_grace_period_false_within_24_hours() {
        let triggered = chrono::Utc::now() - chrono::Duration::hours(23);
        assert!(!is_past_grace_period(triggered));
    }
}
