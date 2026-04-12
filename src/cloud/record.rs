//! Cloud record structures and conflict payload.

use serde::{Deserialize, Serialize};

use crate::errors::mapping::sync::SyncError;

use super::validation::{compute_checksum, validate_uuid};

/// AAD (Additional Authenticated Data) fields for encrypted records.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AadFields {
    pub record_id: String,
    pub dek_version: u32,
}

/// Metadata associated with a cloud record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordMetadata {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub updated_at: String,
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
}
