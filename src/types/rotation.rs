use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// DEK rotation trigger types (spec §4.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotationTrigger {
    AutoTime { days_since_last: u32 },
    AutoCount { record_count: u32 },
    PostRecovery,
    Manual,
}

/// DEK rotation state machine states (spec §4.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RotationState {
    Idle,
    Pending { triggered_at: DateTime<Utc>, trigger: RotationTrigger },
    Rotating { checkpoint: RotationCheckpoint },
    Completed { completed_at: DateTime<Utc>, records_migrated: u32 },
    Failed { error: String, checkpoint: RotationCheckpoint },
}

/// DEK rotation checkpoint for crash recovery (spec §4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RotationCheckpoint {
    pub trigger: RotationTrigger,
    pub old_dek_version: u32,
    pub new_dek_version: u32,
    pub total_records: u32,
    pub migrated_records: u32,
    pub last_migrated_record_id: Option<String>,
    pub started_at: DateTime<Utc>,
    pub cloud_metadata_revision: String,
}

/// DEK rotation configuration stored in cloud metadata (spec §4.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationConfig {
    pub auto_rotate: bool,
    pub rotate_after_days: Option<u32>,
    pub rotate_after_records: Option<u32>,
    pub last_rotation_at: Option<DateTime<Utc>>,
    pub current_dek_record_count: u32,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            auto_rotate: true,
            rotate_after_days: Some(90),
            rotate_after_records: Some(1000),
            last_rotation_at: None,
            current_dek_record_count: 0,
        }
    }
}

/// DEK rotation configuration update (spec §6.2).
#[derive(Debug, Clone, Default)]
pub struct RotationConfigUpdate {
    pub auto_rotate: Option<bool>,
    pub rotate_after_days: Option<Option<u32>>,
    pub rotate_after_records: Option<Option<u32>>,
}

/// DEK rotation result after successful completion (spec §6.4).
#[derive(Debug, Clone)]
pub struct RotationResult {
    pub old_dek_version: u32,
    pub new_dek_version: u32,
    pub records_migrated: u32,
    pub duration_secs: u64,
    pub trigger: RotationTrigger,
}

/// DEK rotation progress events for TUI feedback (spec §6.5).
#[derive(Debug, Clone)]
pub enum RotationProgress {
    Started { total_records: u32, trigger: RotationTrigger },
    RecordMigrated { current: u32, total: u32, record_id: String },
    PushingMetadata { new_dek_version: u32 },
    Completed { result: RotationResult },
    Failed { error: String, migrated: u32, total: u32, checkpoint_preserved: bool },
}

/// DEK rotation constants (spec §4.5).
pub struct RotationConstants;

impl RotationConstants {
    pub const DEFAULT_ROTATE_AFTER_DAYS: u32 = 90;
    pub const DEFAULT_ROTATE_AFTER_RECORDS: u32 = 1000;
    pub const GRACE_PERIOD_HOURS: u32 = 24;
    pub const MAX_DEK_VERSION: u32 = 10_000;
    pub const SYNC_PAUSE_TIMEOUT_SECS: u64 = 30;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_config_default_auto_rotate_is_true() {
        let config = RotationConfig::default();
        assert!(config.auto_rotate);
    }

    #[test]
    fn rotation_config_default_rotate_after_days_is_90() {
        let config = RotationConfig::default();
        assert_eq!(config.rotate_after_days, Some(90));
    }

    #[test]
    fn rotation_config_default_rotate_after_records_is_1000() {
        let config = RotationConfig::default();
        assert_eq!(config.rotate_after_records, Some(1000));
    }

    #[test]
    fn rotation_constants_max_dek_version_is_10000() {
        assert_eq!(RotationConstants::MAX_DEK_VERSION, 10_000);
    }

    #[test]
    fn rotation_constants_grace_period_is_24_hours() {
        assert_eq!(RotationConstants::GRACE_PERIOD_HOURS, 24);
    }

    #[test]
    fn rotation_config_serialize_deserialize() {
        let config = RotationConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let restored: RotationConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.auto_rotate, config.auto_rotate);
        assert_eq!(restored.rotate_after_days, config.rotate_after_days);
    }

    #[test]
    fn rotation_checkpoint_serialize_deserialize() {
        let checkpoint = RotationCheckpoint {
            trigger: RotationTrigger::Manual,
            old_dek_version: 1,
            new_dek_version: 2,
            total_records: 100,
            migrated_records: 50,
            last_migrated_record_id: Some("test-uuid".to_string()),
            started_at: Utc::now(),
            cloud_metadata_revision: "test-revision".to_string(),
        };
        let json = serde_json::to_string(&checkpoint).unwrap();
        let restored: RotationCheckpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.old_dek_version, checkpoint.old_dek_version);
        assert_eq!(restored.new_dek_version, checkpoint.new_dek_version);
    }

    #[test]
    fn rotation_trigger_auto_time_serialization() {
        let trigger = RotationTrigger::AutoTime { days_since_last: 30 };
        let json = serde_json::to_string(&trigger).unwrap();
        let restored: RotationTrigger = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, RotationTrigger::AutoTime { days_since_last: 30 }));
    }

    #[test]
    fn rotation_state_idle_is_idle() {
        assert!(matches!(RotationState::Idle, RotationState::Idle));
    }

    #[test]
    fn rotation_state_pending_contains_trigger() {
        let state = RotationState::Pending {
            triggered_at: Utc::now(),
            trigger: RotationTrigger::Manual,
        };
        assert!(matches!(state, RotationState::Pending { trigger: RotationTrigger::Manual, .. }));
    }
}
