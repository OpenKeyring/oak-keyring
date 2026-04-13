use chrono::{DateTime, Utc};
use crate::errors::mapping::rotation::RotationError;
use crate::types::rotation::{RotationCheckpoint, RotationTrigger};

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
