use std::collections::HashMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::errors::mapping::sync::SyncError;
use crate::errors::ServiceErrorBox;

use super::schema::{FORMAT_VERSION, SCHEMA_VERSION};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: String,
    pub platform: String,
    pub device_name: String,
    pub last_seen: String,
    pub sync_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordVersionInfo {
    pub version: u64,
    pub updated_at: String,
    pub updated_by: String,
    pub checksum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_metadata_checksum: Option<String>,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudMetadata {
    pub version: u32,
    pub schema_version: String,
    pub current_dek_version: u32,
    pub min_supported_dek_version: u32,
    pub vault_identity_token: String,
    pub metadata_version: u64,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub devices: Vec<DeviceInfo>,
    #[serde(default)]
    pub records: HashMap<String, RecordVersionInfo>,
}

impl CloudMetadata {
    pub fn new(vault_identity_token: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            version: FORMAT_VERSION,
            schema_version: SCHEMA_VERSION.to_string(),
            current_dek_version: 1,
            min_supported_dek_version: 1,
            vault_identity_token,
            metadata_version: 1,
            created_at: now.clone(),
            updated_at: now,
            devices: Vec::new(),
            records: HashMap::new(),
        }
    }

    pub fn validate(&self) -> Result<(), SyncError> {
        if self.version != FORMAT_VERSION {
            return Err(SyncError::DeserializationFailed {
                message: format!(
                    "version mismatch: expected {}, got {}",
                    FORMAT_VERSION, self.version
                ),
            });
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(SyncError::DeserializationFailed {
                message: format!(
                    "schema_version mismatch: expected {}, got {}",
                    SCHEMA_VERSION, self.schema_version
                ),
            });
        }
        if self.current_dek_version < 1 {
            return Err(SyncError::DeserializationFailed {
                message: "current_dek_version must be >= 1".to_string(),
            });
        }
        if self.min_supported_dek_version < 1 {
            return Err(SyncError::DeserializationFailed {
                message: "min_supported_dek_version must be >= 1".to_string(),
            });
        }
        if self.current_dek_version < self.min_supported_dek_version {
            return Err(SyncError::DeserializationFailed {
                message: format!(
                    "current_dek_version ({}) must be >= min_supported_dek_version ({})",
                    self.current_dek_version, self.min_supported_dek_version
                ),
            });
        }
        if self.metadata_version < 1 {
            return Err(SyncError::DeserializationFailed {
                message: "metadata_version must be >= 1".to_string(),
            });
        }
        if self.vault_identity_token.is_empty() {
            return Err(SyncError::DeserializationFailed {
                message: "vault_identity_token must be non-empty".to_string(),
            });
        }
        if chrono::DateTime::parse_from_rfc3339(&self.created_at).is_err() {
            return Err(SyncError::DeserializationFailed {
                message: format!("invalid created_at ISO 8601: {}", self.created_at),
            });
        }
        if chrono::DateTime::parse_from_rfc3339(&self.updated_at).is_err() {
            return Err(SyncError::DeserializationFailed {
                message: format!("invalid updated_at ISO 8601: {}", self.updated_at),
            });
        }
        Ok(())
    }

    pub fn increment_version(&mut self) {
        self.metadata_version += 1;
        self.updated_at = Utc::now().to_rfc3339();
    }

    pub fn has_remote_changes(&self, local_version: u64) -> bool {
        self.metadata_version > local_version
    }

    pub fn add_device(&mut self, device: DeviceInfo) {
        if !self.devices.iter().any(|d| d.device_id == device.device_id) {
            self.devices.push(device);
        }
    }

    pub fn update_device(&mut self, device: DeviceInfo) {
        if let Some(existing) = self
            .devices
            .iter_mut()
            .find(|d| d.device_id == device.device_id)
        {
            *existing = device;
        }
    }

    pub fn upsert_record(&mut self, record_id: String, info: RecordVersionInfo) {
        self.records.insert(record_id, info);
    }
}

pub fn serialize_metadata(metadata: &CloudMetadata) -> Result<String, ServiceErrorBox> {
    serde_json::to_string(metadata).map_err(|e| {
        SyncError::SerializationFailed {
            message: e.to_string(),
        }
        .into()
    })
}

pub fn deserialize_metadata(json: &str) -> Result<CloudMetadata, ServiceErrorBox> {
    serde_json::from_str(json).map_err(|e| {
        SyncError::DeserializationFailed {
            message: e.to_string(),
        }
        .into()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_metadata() -> CloudMetadata {
        CloudMetadata::new("test_token_abc123".to_string())
    }

    #[test]
    fn test_new_metadata() {
        let metadata = CloudMetadata::new("token123".to_string());
        assert_eq!(metadata.version, FORMAT_VERSION);
        assert_eq!(metadata.schema_version, SCHEMA_VERSION);
        assert_eq!(metadata.current_dek_version, 1);
        assert_eq!(metadata.min_supported_dek_version, 1);
        assert_eq!(metadata.vault_identity_token, "token123");
        assert_eq!(metadata.metadata_version, 1);
        assert!(!metadata.devices.is_empty() || metadata.devices.is_empty());
        assert!(metadata.records.is_empty());
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let mut metadata = create_valid_metadata();
        metadata.add_device(DeviceInfo {
            device_id: "device-1".to_string(),
            platform: "macos".to_string(),
            device_name: "MacBook Pro".to_string(),
            last_seen: "2026-04-05T12:00:00Z".to_string(),
            sync_count: 10,
        });
        metadata.upsert_record(
            "record-1".to_string(),
            RecordVersionInfo {
                version: 1,
                updated_at: "2026-04-05T12:00:00Z".to_string(),
                updated_by: "device-1".to_string(),
                checksum: "abc123".to_string(),
                private_metadata_checksum: None,
                deleted: false,
            },
        );

        let json = serialize_metadata(&metadata).unwrap();
        let deserialized: CloudMetadata = deserialize_metadata(&json).unwrap();

        assert_eq!(deserialized.version, metadata.version);
        assert_eq!(deserialized.schema_version, metadata.schema_version);
        assert_eq!(
            deserialized.current_dek_version,
            metadata.current_dek_version
        );
        assert_eq!(
            deserialized.min_supported_dek_version,
            metadata.min_supported_dek_version
        );
        assert_eq!(
            deserialized.vault_identity_token,
            metadata.vault_identity_token
        );
        assert_eq!(deserialized.metadata_version, metadata.metadata_version);
        assert_eq!(deserialized.devices.len(), metadata.devices.len());
        assert_eq!(deserialized.records.len(), metadata.records.len());
    }

    #[test]
    fn test_validate_accepts_valid_metadata() {
        let metadata = create_valid_metadata();
        assert!(metadata.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_wrong_version() {
        let mut metadata = create_valid_metadata();
        metadata.version = 999;
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_wrong_schema_version() {
        let mut metadata = create_valid_metadata();
        metadata.schema_version = "unknown-schema".to_string();
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_empty_token() {
        let mut metadata = create_valid_metadata();
        metadata.vault_identity_token = "".to_string();
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_zero_dek_version() {
        let mut metadata = create_valid_metadata();
        metadata.current_dek_version = 0;
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_zero_metadata_version() {
        let mut metadata = create_valid_metadata();
        metadata.metadata_version = 0;
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_invalid_created_at() {
        let mut metadata = create_valid_metadata();
        metadata.created_at = "invalid-date".to_string();
        let result = metadata.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_increment_version() {
        let mut metadata = create_valid_metadata();
        let original_version = metadata.metadata_version;
        let original_updated_at = metadata.updated_at.clone();

        std::thread::sleep(std::time::Duration::from_millis(10));
        metadata.increment_version();

        assert_eq!(metadata.metadata_version, original_version + 1);
        assert_ne!(metadata.updated_at, original_updated_at);
    }

    #[test]
    fn test_has_remote_changes() {
        let metadata = create_valid_metadata();
        assert!(!metadata.has_remote_changes(1));
        assert!(!metadata.has_remote_changes(2));
        assert!(metadata.has_remote_changes(0));
    }

    #[test]
    fn test_has_remote_changes_equal() {
        let mut metadata = create_valid_metadata();
        metadata.metadata_version = 5;
        assert!(!metadata.has_remote_changes(5));
        assert!(metadata.has_remote_changes(4));
    }

    #[test]
    fn test_add_device() {
        let mut metadata = create_valid_metadata();
        let device = DeviceInfo {
            device_id: "device-1".to_string(),
            platform: "macos".to_string(),
            device_name: "MacBook Pro".to_string(),
            last_seen: "2026-04-05T12:00:00Z".to_string(),
            sync_count: 5,
        };
        metadata.add_device(device.clone());
        assert_eq!(metadata.devices.len(), 1);
        metadata.add_device(device);
        assert_eq!(metadata.devices.len(), 1);
    }

    #[test]
    fn test_update_device() {
        let mut metadata = create_valid_metadata();
        let device1 = DeviceInfo {
            device_id: "device-1".to_string(),
            platform: "macos".to_string(),
            device_name: "MacBook Pro".to_string(),
            last_seen: "2026-04-05T12:00:00Z".to_string(),
            sync_count: 5,
        };
        metadata.add_device(device1);

        let device2 = DeviceInfo {
            device_id: "device-1".to_string(),
            platform: "macos".to_string(),
            device_name: "MacBook Pro Updated".to_string(),
            last_seen: "2026-04-05T13:00:00Z".to_string(),
            sync_count: 10,
        };
        metadata.update_device(device2);
        assert_eq!(metadata.devices.len(), 1);
        assert_eq!(metadata.devices[0].device_name, "MacBook Pro Updated");
        assert_eq!(metadata.devices[0].sync_count, 10);
    }

    #[test]
    fn test_upsert_record() {
        let mut metadata = create_valid_metadata();
        let info1 = RecordVersionInfo {
            version: 1,
            updated_at: "2026-04-05T12:00:00Z".to_string(),
            updated_by: "device-1".to_string(),
            checksum: "checksum1".to_string(),
            private_metadata_checksum: None,
            deleted: false,
        };
        metadata.upsert_record("record-1".to_string(), info1);
        assert_eq!(metadata.records.len(), 1);

        let info2 = RecordVersionInfo {
            version: 2,
            updated_at: "2026-04-05T13:00:00Z".to_string(),
            updated_by: "device-1".to_string(),
            checksum: "checksum2".to_string(),
            private_metadata_checksum: None,
            deleted: false,
        };
        metadata.upsert_record("record-1".to_string(), info2);
        assert_eq!(metadata.records.len(), 1);
        assert_eq!(metadata.records.get("record-1").unwrap().version, 2);
    }

    #[test]
    fn test_forward_compatibility_unknown_fields() {
        let json_with_extra_field = r#"{
            "version": 1,
            "schema_version": "open-keyring-v1",
            "current_dek_version": 1,
            "min_supported_dek_version": 1,
            "vault_identity_token": "token123",
            "metadata_version": 1,
            "created_at": "2026-04-05T10:00:00Z",
            "updated_at": "2026-04-05T12:00:00Z",
            "devices": [],
            "records": {},
            "unknown_future_field": "should_be_ignored"
        }"#;

        let result: Result<CloudMetadata, _> = serde_json::from_str(json_with_extra_field);
        assert!(
            result.is_ok(),
            "Expected forward compatibility with unknown field"
        );
        let metadata = result.unwrap();
        assert_eq!(metadata.vault_identity_token, "token123");
    }

    #[test]
    fn test_deleted_field_defaults_to_false() {
        let json_without_deleted = r#"{
            "version": 1,
            "schema_version": "open-keyring-v1",
            "current_dek_version": 1,
            "min_supported_dek_version": 1,
            "vault_identity_token": "token123",
            "metadata_version": 1,
            "created_at": "2026-04-05T10:00:00Z",
            "updated_at": "2026-04-05T12:00:00Z",
            "devices": [],
            "records": {
                "record-1": {
                    "version": 1,
                    "updated_at": "2026-04-05T12:00:00Z",
                    "updated_by": "device-1",
                    "checksum": "abc123"
                }
            }
        }"#;

        let metadata: CloudMetadata = serde_json::from_str(json_without_deleted).unwrap();
        let record = metadata.records.get("record-1").unwrap();
        assert!(!record.deleted, "deleted should default to false");
    }

    #[test]
    fn test_deserialize_devices() {
        let json = r#"{
            "version": 1,
            "schema_version": "open-keyring-v1",
            "current_dek_version": 1,
            "min_supported_dek_version": 1,
            "vault_identity_token": "token123",
            "metadata_version": 1,
            "created_at": "2026-04-05T10:00:00Z",
            "updated_at": "2026-04-05T12:00:00Z",
            "devices": [
                {
                    "device_id": "macos-mbp-abc123",
                    "platform": "macos",
                    "device_name": "MacBook Pro",
                    "last_seen": "2026-04-05T12:00:00Z",
                    "sync_count": 42
                }
            ],
            "records": {}
        }"#;

        let metadata: CloudMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(metadata.devices.len(), 1);
        let device = &metadata.devices[0];
        assert_eq!(device.device_id, "macos-mbp-abc123");
        assert_eq!(device.platform, "macos");
        assert_eq!(device.device_name, "MacBook Pro");
        assert_eq!(device.last_seen, "2026-04-05T12:00:00Z");
        assert_eq!(device.sync_count, 42);
    }

    #[test]
    fn test_deserialize_records() {
        let json = r#"{
            "version": 1,
            "schema_version": "open-keyring-v1",
            "current_dek_version": 1,
            "min_supported_dek_version": 1,
            "vault_identity_token": "token123",
            "metadata_version": 1,
            "created_at": "2026-04-05T10:00:00Z",
            "updated_at": "2026-04-05T12:00:00Z",
            "devices": [],
            "records": {
                "550e8400-e29b-41d4-a716-446655440000": {
                    "version": 5,
                    "updated_at": "2026-04-05T12:00:00Z",
                    "updated_by": "macos-mbp-abc123",
                    "checksum": "sha256_hex_of_encrypted_data",
                    "deleted": false
                }
            }
        }"#;

        let metadata: CloudMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(metadata.records.len(), 1);
        let record = metadata
            .records
            .get("550e8400-e29b-41d4-a716-446655440000")
            .unwrap();
        assert_eq!(record.version, 5);
        assert_eq!(record.updated_by, "macos-mbp-abc123");
        assert!(!record.deleted);
    }
}
