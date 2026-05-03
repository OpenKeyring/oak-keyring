//! Cloud record structures and conflict payload.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::mapping::sync::SyncError;
use crate::types::health::RecordHealthState;

use super::validation::{compute_checksum, validate_uuid};

/// AAD (Additional Authenticated Data) fields for encrypted records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AadFields {
    pub record_id: String,
    pub dek_version: u32,
}

/// Health metadata embedded in cloud record metadata for cross-device sync.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordHealthMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weak_password: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duplicate_group_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compromised: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expired: Option<bool>,
}

impl RecordHealthMetadata {
    /// Convert from the local `RecordHealthState` to the cloud-serializable form.
    pub fn from_state(state: &RecordHealthState) -> Self {
        Self {
            evaluated_at: state.evaluated_at.map(|dt| dt.to_rfc3339()),
            weak_password: state.weak_password,
            duplicate_group_size: state.duplicate_group_size.map(|v| v as u32),
            compromised: state.compromised,
            expired: state.expired,
        }
    }

    /// Convert back to the local `RecordHealthState`.
    ///
    /// The caller must supply `record_id` and `record_version` since those are
    /// not stored inside the health metadata itself.
    pub fn to_state(&self, record_id: Uuid, record_version: u64) -> RecordHealthState {
        RecordHealthState {
            record_id,
            record_version,
            evaluated_at: self.evaluated_at.as_ref().and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            weak_password: self.weak_password,
            duplicate_group_size: self.duplicate_group_size.map(|v| v as usize),
            compromised: self.compromised,
            expired: self.expired,
        }
    }
}

/// Metadata associated with a cloud record.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecordMetadata {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_type: Option<crate::types::credential::CredentialType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_favorite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<RecordHealthMetadata>,
}

/// Cloud record structure matching the cloud-storage-schema-arch §5.2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudRecord {
    pub id: String,
    pub version: u64,
    pub encrypted_data: String,
    pub nonce: String,
    pub dek_version: u32,
    pub aad: AadFields,
    pub metadata: RecordMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

impl CloudRecord {
    /// Validates the cloud record for structural and logical correctness.
    pub fn validate(&self) -> Result<(), SyncError> {
        // Validate id is non-empty and valid UUID format
        if self.id.is_empty() {
            return Err(SyncError::DeserializationFailed {
                message: "id must be non-empty".to_string(),
            });
        }
        if !validate_uuid(&self.id) {
            return Err(SyncError::DeserializationFailed {
                message: format!("id must be a valid UUID: {}", self.id),
            });
        }

        // Validate version >= 1
        if self.version < 1 {
            return Err(SyncError::DeserializationFailed {
                message: format!("version must be >= 1, got {}", self.version),
            });
        }

        // Validate encrypted_data is non-empty
        if self.encrypted_data.is_empty() {
            return Err(SyncError::DeserializationFailed {
                message: "encrypted_data must be non-empty".to_string(),
            });
        }

        // Validate nonce is non-empty
        if self.nonce.is_empty() {
            return Err(SyncError::DeserializationFailed {
                message: "nonce must be non-empty".to_string(),
            });
        }

        // Validate dek_version >= 1
        if self.dek_version < 1 {
            return Err(SyncError::DeserializationFailed {
                message: format!("dek_version must be >= 1, got {}", self.dek_version),
            });
        }

        // Validate aad.record_id == self.id
        if self.aad.record_id != self.id {
            return Err(SyncError::AadInconsistent {
                field: "record_id".to_string(),
                expected: self.id.clone(),
                actual: self.aad.record_id.clone(),
            });
        }

        // Validate aad.dek_version == self.dek_version
        if self.aad.dek_version != self.dek_version {
            return Err(SyncError::AadInconsistent {
                field: "dek_version".to_string(),
                expected: self.dek_version.to_string(),
                actual: self.aad.dek_version.to_string(),
            });
        }

        // Validate metadata.name is non-empty
        if self.metadata.name.is_empty() {
            return Err(SyncError::DeserializationFailed {
                message: "metadata.name must be non-empty".to_string(),
            });
        }

        // Validate metadata.updated_at is non-empty
        if self.metadata.updated_at.is_empty() {
            return Err(SyncError::DeserializationFailed {
                message: "metadata.updated_at must be non-empty".to_string(),
            });
        }

        Ok(())
    }

    /// Computes SHA-256 hash of the Base64-decoded encrypted_data.
    /// Returns hex string of the hash.
    pub fn compute_checksum(&self) -> Result<String, SyncError> {
        compute_checksum(&self.encrypted_data)
    }

    /// Extract the health metadata from this cloud record, if present.
    ///
    /// Returns `None` when the cloud record has no health metadata attached
    /// (e.g. records uploaded by an older client version).
    pub fn health_metadata(&self) -> Option<&RecordHealthMetadata> {
        self.metadata.health.as_ref()
    }

    /// Convert any embedded health metadata into a local `RecordHealthState`.
    ///
    /// Returns `None` when no health metadata is attached.
    pub fn to_health_state(&self) -> Option<RecordHealthState> {
        self.metadata
            .health
            .as_ref()
            .map(|h| h.to_state(Uuid::parse_str(&self.id).unwrap_or_default(), self.version))
    }
}

/// Build a `CloudRecord` from a `StoredRecord` with optional health state.
///
/// This is the upload-side bridge: the caller supplies the decrypted record
/// name (since `StoredRecord` keeps names encrypted) and the base64-encoded
/// encrypted data, nonce, and AAD. Health metadata is attached when available.
///
/// # Arguments
/// * `record` - The locally stored record
/// * `name` - The decrypted record name (used as display metadata)
/// * `encrypted_data_base64` - Base64-encoded encrypted payload
/// * `nonce_base64` - Base64-encoded nonce
/// * `aad` - AAD fields for the cloud record
/// * `health` - Optional health state to embed in metadata
pub fn build_cloud_record(
    record: &crate::types::record::StoredRecord,
    name: &str,
    encrypted_data_base64: &str,
    nonce_base64: &str,
    aad: AadFields,
    health: Option<&RecordHealthState>,
) -> CloudRecord {
    CloudRecord {
        id: record.id.to_string(),
        version: record.version,
        encrypted_data: encrypted_data_base64.to_string(),
        nonce: nonce_base64.to_string(),
        dek_version: record.dek_version,
        aad,
        metadata: RecordMetadata {
            name: name.to_string(),
            tags: record.tags.clone(),
            updated_at: record.updated_at.to_rfc3339(),
            credential_type: Some(record.credential_type),
            is_favorite: Some(record.is_favorite),
            expires_at: record.expires_at.map(|dt| dt.to_rfc3339()),
            updated_by: Some(record.updated_by.clone()),
            health: health.map(RecordHealthMetadata::from_state),
        },
        deleted: if record.deleted { Some(true) } else { None },
        deleted_at: record.deleted_at.map(|dt| dt.to_rfc3339()),
    }
}

/// Conflict payload containing a cloud record and its checksum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictPayload {
    pub cloud_record: CloudRecord,
    pub checksum: String,
}

impl ConflictPayload {
    /// Serializes the conflict payload to JSON bytes.
    pub fn serialize(&self) -> Result<Vec<u8>, SyncError> {
        serde_json::to_vec(self).map_err(|e| SyncError::SerializationFailed {
            message: e.to_string(),
        })
    }

    /// Deserializes a conflict payload from JSON bytes.
    /// Returns DeserializationFailed on corrupt JSON (never panics).
    pub fn deserialize(data: &[u8]) -> Result<Self, SyncError> {
        serde_json::from_slice(data).map_err(|e| SyncError::DeserializationFailed {
            message: e.to_string(),
        })
    }

    /// Validates the conflict payload:
    /// - Validates the inner cloud record
    /// - Computes checksum and compares with stored checksum
    pub fn validate(&self) -> Result<(), SyncError> {
        // First validate the cloud record structure
        self.cloud_record.validate()?;

        // Compute actual checksum and compare
        let computed = self.cloud_record.compute_checksum()?;
        if computed != self.checksum {
            return Err(SyncError::ChecksumMismatch {
                expected: self.checksum.clone(),
                actual: computed,
                record_id: self.cloud_record.id.clone(),
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_cloud_record() -> CloudRecord {
        CloudRecord {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            version: 5,
            encrypted_data: "dGVzdCBkYXRh".to_string(),
            nonce: "bm9uY2U".to_string(),
            dek_version: 1,
            aad: AadFields {
                record_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                dek_version: 1,
            },
            metadata: RecordMetadata {
                name: "GitHub".to_string(),
                tags: vec!["dev".to_string(), "work".to_string()],
                updated_at: "2026-04-05T12:00:00Z".to_string(),
                health: None,
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        }
    }

    #[test]
    fn test_cloud_record_serialize_deserialize_roundtrip() {
        let record = create_valid_cloud_record();
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: CloudRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, record.id);
        assert_eq!(deserialized.version, record.version);
        assert_eq!(deserialized.encrypted_data, record.encrypted_data);
        assert_eq!(deserialized.nonce, record.nonce);
        assert_eq!(deserialized.dek_version, record.dek_version);
        assert_eq!(deserialized.aad.record_id, record.aad.record_id);
        assert_eq!(deserialized.aad.dek_version, record.aad.dek_version);
        assert_eq!(deserialized.metadata.name, record.metadata.name);
        assert_eq!(deserialized.metadata.tags, record.metadata.tags);
        assert_eq!(deserialized.metadata.updated_at, record.metadata.updated_at);
    }

    #[test]
    fn test_validate_passes_for_valid_record() {
        let record = create_valid_cloud_record();
        assert!(record.validate().is_ok());
    }

    #[test]
    fn test_validate_rejects_empty_id() {
        let mut record = create_valid_cloud_record();
        record.id = "".to_string();
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_invalid_uuid() {
        let mut record = create_valid_cloud_record();
        record.id = "not-a-valid-uuid".to_string();
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_version_zero() {
        let mut record = create_valid_cloud_record();
        record.version = 0;
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_empty_encrypted_data() {
        let mut record = create_valid_cloud_record();
        record.encrypted_data = "".to_string();
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_empty_nonce() {
        let mut record = create_valid_cloud_record();
        record.nonce = "".to_string();
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_dek_version_zero() {
        let mut record = create_valid_cloud_record();
        record.dek_version = 0;
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_aad_record_id_mismatch() {
        let mut record = create_valid_cloud_record();
        record.aad.record_id = "different-id".to_string();
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::AadInconsistent { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_aad_dek_version_mismatch() {
        let mut record = create_valid_cloud_record();
        record.aad.dek_version = 999;
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::AadInconsistent { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_empty_metadata_name() {
        let mut record = create_valid_cloud_record();
        record.metadata.name = "".to_string();
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_validate_rejects_empty_metadata_updated_at() {
        let mut record = create_valid_cloud_record();
        record.metadata.updated_at = "".to_string();
        let result = record.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_compute_checksum_produces_correct_hex() {
        let record = CloudRecord {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            version: 1,
            encrypted_data: "dGVzdCBkYXRh".to_string(),
            nonce: "bm9uY2U".to_string(),
            dek_version: 1,
            aad: AadFields {
                record_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                dek_version: 1,
            },
            metadata: RecordMetadata {
                name: "Test".to_string(),
                tags: vec![],
                updated_at: "2026-04-05T12:00:00Z".to_string(),
                health: None,
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        };

        let checksum = record.compute_checksum().unwrap();
        assert_eq!(
            checksum,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
    }

    #[test]
    fn test_conflict_payload_serialize_deserialize_roundtrip() {
        let record = create_valid_cloud_record();
        let checksum = record.compute_checksum().unwrap();
        let payload = ConflictPayload {
            cloud_record: record.clone(),
            checksum: checksum.clone(),
        };

        let serialized = payload.serialize().unwrap();
        let deserialized = ConflictPayload::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.cloud_record.id, record.id);
        assert_eq!(deserialized.checksum, checksum);
    }

    #[test]
    fn test_conflict_payload_validate_catches_checksum_mismatch() {
        let record = create_valid_cloud_record();
        let payload = ConflictPayload {
            cloud_record: record,
            checksum: "invalid_checksum".to_string(),
        };

        let result = payload.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::ChecksumMismatch { .. }
        ));
    }

    #[test]
    fn test_conflict_payload_validate_passes_with_correct_checksum() {
        let record = create_valid_cloud_record();
        let checksum = record.compute_checksum().unwrap();
        let payload = ConflictPayload {
            cloud_record: record,
            checksum,
        };

        assert!(payload.validate().is_ok());
    }

    #[test]
    fn test_corrupt_json_deserialization_returns_error() {
        let corrupt_data = b"{ invalid json }";
        let result = ConflictPayload::deserialize(corrupt_data);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn test_deleted_fields_are_optional() {
        let json_with_deleted = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "version": 5,
            "encrypted_data": "base64_encrypted",
            "nonce": "base64_nonce",
            "dek_version": 1,
            "aad": {
                "record_id": "550e8400-e29b-41d4-a716-446655440000",
                "dek_version": 1
            },
            "metadata": {
                "name": "GitHub",
                "tags": ["dev"],
                "updated_at": "2026-04-05T12:00:00Z"
            },
            "deleted": true,
            "deleted_at": "2026-04-06T10:00:00Z"
        }"#;

        let record: CloudRecord = serde_json::from_str(json_with_deleted).unwrap();
        assert_eq!(record.deleted, Some(true));
        assert_eq!(record.deleted_at, Some("2026-04-06T10:00:00Z".to_string()));

        // Test without deleted fields
        let json_without_deleted = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "version": 5,
            "encrypted_data": "base64_encrypted",
            "nonce": "base64_nonce",
            "dek_version": 1,
            "aad": {
                "record_id": "550e8400-e29b-41d4-a716-446655440000",
                "dek_version": 1
            },
            "metadata": {
                "name": "GitHub",
                "tags": [],
                "updated_at": "2026-04-05T12:00:00Z"
            }
        }"#;

        let record: CloudRecord = serde_json::from_str(json_without_deleted).unwrap();
        assert_eq!(record.deleted, None);
        assert_eq!(record.deleted_at, None);
    }

    // ── RecordHealthMetadata tests ──────────────────────────────────────
    //
    // JSON format matches the spec (section 6):
    // ```json
    // {
    //   "evaluated_at": "2026-04-05T12:00:00Z",
    //   "weak_password": true,
    //   "duplicate_group_size": 3,
    //   "compromised": false,
    //   "expired": false
    // }
    // ```

    #[test]
    fn test_health_metadata_serialize_roundtrip() {
        let health = RecordHealthMetadata {
            evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
            weak_password: Some(true),
            duplicate_group_size: Some(3),
            compromised: Some(false),
            expired: Some(false),
        };
        let json = serde_json::to_string(&health).unwrap();
        let deserialized: RecordHealthMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, health);
    }

    #[test]
    fn test_health_metadata_omits_none_fields() {
        let health = RecordHealthMetadata {
            evaluated_at: None,
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: None,
            expired: None,
        };
        let json = serde_json::to_string(&health).unwrap();
        // Should not contain "evaluated_at", "duplicate_group_size", "compromised", "expired"
        assert!(!json.contains("evaluated_at"));
        assert!(!json.contains("duplicate_group_size"));
        assert!(!json.contains("compromised"));
        assert!(!json.contains("expired"));
        assert!(json.contains("weak_password"));
    }

    #[test]
    fn test_health_metadata_defaults_all_none() {
        let json = "{}";
        let health: RecordHealthMetadata = serde_json::from_str(json).unwrap();
        assert!(health.evaluated_at.is_none());
        assert!(health.weak_password.is_none());
        assert!(health.duplicate_group_size.is_none());
        assert!(health.compromised.is_none());
        assert!(health.expired.is_none());
    }

    #[test]
    fn test_health_metadata_backward_compatible_with_unknown_fields() {
        let json = r#"{
            "evaluated_at": "2026-04-05T12:00:00Z",
            "weak_password": true,
            "duplicate_group_size": 2,
            "compromised": false,
            "expired": true,
            "future_field": "should be ignored"
        }"#;
        let health: RecordHealthMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(health.weak_password, Some(true));
        assert_eq!(health.duplicate_group_size, Some(2));
        assert_eq!(health.expired, Some(true));
    }

    #[test]
    fn test_health_metadata_from_state() {
        let state = RecordHealthState {
            record_id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            record_version: 5,
            evaluated_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-04-05T12:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
            weak_password: Some(true),
            duplicate_group_size: Some(3),
            compromised: Some(false),
            expired: None,
        };

        let metadata = RecordHealthMetadata::from_state(&state);

        assert_eq!(
            metadata.evaluated_at.as_deref(),
            Some("2026-04-05T12:00:00+00:00")
        );
        assert_eq!(metadata.weak_password, Some(true));
        assert_eq!(metadata.duplicate_group_size, Some(3));
        assert_eq!(metadata.compromised, Some(false));
        assert_eq!(metadata.expired, None);
    }

    #[test]
    fn test_health_metadata_to_state() {
        let health = RecordHealthMetadata {
            evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
            weak_password: Some(true),
            duplicate_group_size: Some(3),
            compromised: Some(false),
            expired: None,
        };

        let record_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let state = health.to_state(record_id, 5);

        assert_eq!(state.record_id, record_id);
        assert_eq!(state.record_version, 5);
        assert_eq!(
            state.evaluated_at.unwrap().to_rfc3339(),
            "2026-04-05T12:00:00+00:00"
        );
        assert_eq!(state.weak_password, Some(true));
        assert_eq!(state.duplicate_group_size, Some(3));
        assert_eq!(state.compromised, Some(false));
        assert_eq!(state.expired, None);
    }

    #[test]
    fn test_health_metadata_roundtrip_via_state() {
        let original = RecordHealthState {
            record_id: Uuid::new_v4(),
            record_version: 7,
            evaluated_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-04-05T12:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
            weak_password: Some(false),
            duplicate_group_size: Some(5),
            compromised: None,
            expired: Some(true),
        };

        let cloud_meta = RecordHealthMetadata::from_state(&original);
        let restored = cloud_meta.to_state(original.record_id, original.record_version);

        assert_eq!(restored.record_id, original.record_id);
        assert_eq!(restored.record_version, original.record_version);
        assert_eq!(restored.weak_password, original.weak_password);
        assert_eq!(restored.duplicate_group_size, original.duplicate_group_size);
        assert_eq!(restored.compromised, original.compromised);
        assert_eq!(restored.expired, original.expired);
        // Timestamps survive the round-trip as RFC 3339 strings
        assert_eq!(
            restored.evaluated_at.unwrap().to_rfc3339(),
            original.evaluated_at.unwrap().to_rfc3339()
        );
    }

    #[test]
    fn test_cloud_record_with_health_metadata_roundtrip() {
        let health = RecordHealthMetadata {
            evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: Some(false),
            expired: None,
        };
        let mut record = create_valid_cloud_record();
        record.metadata.health = Some(health.clone());

        let json = serde_json::to_string(&record).unwrap();
        let deserialized: CloudRecord = serde_json::from_str(&json).unwrap();

        assert!(deserialized.metadata.health.is_some());
        let restored = deserialized.metadata.health.unwrap();
        assert_eq!(restored.weak_password, Some(true));
        assert_eq!(restored.compromised, Some(false));
    }

    #[test]
    fn test_cloud_record_without_health_deserializes_as_none() {
        // Old-format JSON without health field
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "version": 5,
            "encrypted_data": "dGVzdCBkYXRh",
            "nonce": "bm9uY2U",
            "dek_version": 1,
            "aad": {
                "record_id": "550e8400-e29b-41d4-a716-446655440000",
                "dek_version": 1
            },
            "metadata": {
                "name": "GitHub",
                "tags": ["dev"],
                "updated_at": "2026-04-05T12:00:00Z"
            }
        }"#;

        let record: CloudRecord = serde_json::from_str(json).unwrap();
        assert!(record.metadata.health.is_none());
    }

    #[test]
    fn test_conflict_payload_carries_health_metadata() {
        let health = RecordHealthMetadata {
            evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
            weak_password: Some(true),
            duplicate_group_size: Some(2),
            compromised: None,
            expired: None,
        };
        let mut record = create_valid_cloud_record();
        record.metadata.health = Some(health);

        let checksum = record.compute_checksum().unwrap();
        let payload = ConflictPayload {
            cloud_record: record.clone(),
            checksum,
        };

        let serialized = payload.serialize().unwrap();
        let deserialized = ConflictPayload::deserialize(&serialized).unwrap();

        let restored_health = deserialized.cloud_record.metadata.health.unwrap();
        assert_eq!(restored_health.weak_password, Some(true));
        assert_eq!(restored_health.duplicate_group_size, Some(2));
    }

    // ── build_cloud_record and convenience method tests ──────────────

    fn create_test_stored_record() -> crate::types::record::StoredRecord {
        use chrono::{DateTime, Utc};

        crate::types::record::StoredRecord {
            id: Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap(),
            credential_type: crate::types::credential::CredentialType::Login,
            encrypted_data: vec![1, 2, 3, 4],
            nonce: [0u8; 24],
            dek_version: 1,
            aad: vec![],
            is_favorite: false,
            expires_at: None,
            created_at: DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339("2026-04-05T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            updated_by: "device-1".to_string(),
            version: 5,
            deleted: false,
            deleted_at: None,
            tags: vec!["dev".to_string(), "work".to_string()],
        }
    }

    #[test]
    fn test_build_cloud_record_without_health() {
        let stored = create_test_stored_record();
        let aad = AadFields {
            record_id: stored.id.to_string(),
            dek_version: stored.dek_version,
        };

        let cloud = build_cloud_record(&stored, "GitHub", "dGVzdCBkYXRh", "bm9uY2U", aad, None);

        assert_eq!(cloud.id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(cloud.version, 5);
        assert_eq!(cloud.metadata.name, "GitHub");
        assert_eq!(cloud.metadata.tags, vec!["dev", "work"]);
        assert!(cloud.metadata.health.is_none());
        assert!(cloud.validate().is_ok());
    }

    #[test]
    fn test_build_cloud_record_with_health() {
        let stored = create_test_stored_record();
        let aad = AadFields {
            record_id: stored.id.to_string(),
            dek_version: stored.dek_version,
        };

        let health = RecordHealthState {
            record_id: stored.id,
            record_version: 5,
            evaluated_at: Some(
                chrono::DateTime::parse_from_rfc3339("2026-04-05T12:00:00Z")
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            ),
            weak_password: Some(true),
            duplicate_group_size: Some(3),
            compromised: Some(false),
            expired: None,
        };

        let cloud = build_cloud_record(
            &stored,
            "GitHub",
            "dGVzdCBkYXRh",
            "bm9uY2U",
            aad,
            Some(&health),
        );

        assert!(cloud.metadata.health.is_some());
        let h = cloud.metadata.health.as_ref().unwrap();
        assert_eq!(h.weak_password, Some(true));
        assert_eq!(h.duplicate_group_size, Some(3));
        assert_eq!(h.compromised, Some(false));
        assert!(cloud.validate().is_ok());
    }

    #[test]
    fn test_cloud_record_health_metadata_method() {
        let mut record = create_valid_cloud_record();
        assert!(record.health_metadata().is_none());

        let health = RecordHealthMetadata {
            evaluated_at: None,
            weak_password: Some(true),
            duplicate_group_size: None,
            compromised: None,
            expired: None,
        };
        record.metadata.health = Some(health);
        assert!(record.health_metadata().is_some());
        assert_eq!(record.health_metadata().unwrap().weak_password, Some(true));
    }

    #[test]
    fn test_cloud_record_to_health_state_method() {
        let mut record = create_valid_cloud_record();
        assert!(record.to_health_state().is_none());

        let health = RecordHealthMetadata {
            evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
            weak_password: Some(true),
            duplicate_group_size: Some(3),
            compromised: Some(false),
            expired: Some(true),
        };
        record.metadata.health = Some(health);

        let state = record.to_health_state().unwrap();
        assert_eq!(state.record_version, record.version);
        assert_eq!(state.weak_password, Some(true));
        assert_eq!(state.duplicate_group_size, Some(3));
        assert_eq!(state.compromised, Some(false));
        assert_eq!(state.expired, Some(true));
    }

    #[test]
    fn test_build_cloud_record_with_deleted_record() {
        use chrono::{DateTime, Utc};

        let mut stored = create_test_stored_record();
        stored.deleted = true;
        stored.deleted_at = Some(
            DateTime::parse_from_rfc3339("2026-04-06T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        );

        let aad = AadFields {
            record_id: stored.id.to_string(),
            dek_version: stored.dek_version,
        };

        let cloud = build_cloud_record(&stored, "GitHub", "dGVzdCBkYXRh", "bm9uY2U", aad, None);

        assert_eq!(cloud.deleted, Some(true));
        assert!(cloud.deleted_at.is_some());
    }
}
