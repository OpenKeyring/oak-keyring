use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::errors::mapping::sync::SyncError;

use super::record::AadFields;

pub fn compute_checksum(encrypted_data_base64: &str) -> Result<String, SyncError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encrypted_data_base64)
        .map_err(|e| SyncError::DeserializationFailed {
            message: format!("failed to decode encrypted_data as base64: {}", e),
        })?;

    let hash = Sha256::digest(&decoded);
    Ok(hex::encode(hash))
}

pub fn validate_aad(record_id: &str, dek_version: u32, aad: &AadFields) -> Result<(), SyncError> {
    if aad.record_id != record_id {
        return Err(SyncError::AadInconsistent {
            field: "record_id".to_string(),
            expected: record_id.to_string(),
            actual: aad.record_id.clone(),
        });
    }

    if aad.dek_version != dek_version {
        return Err(SyncError::AadInconsistent {
            field: "dek_version".to_string(),
            expected: dek_version.to_string(),
            actual: aad.dek_version.to_string(),
        });
    }

    Ok(())
}

pub fn validate_uuid(id: &str) -> bool {
    Uuid::parse_str(id).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_checksum_with_known_input() {
        let input = "dGVzdCBkYXRh";
        let result = compute_checksum(input).unwrap();
        assert_eq!(
            result,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
    }

    #[test]
    fn test_compute_checksum_with_empty_string() {
        let input = "";
        let result = compute_checksum(input);
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_compute_checksum_with_invalid_base64() {
        let input = "not-valid-base64!!!";
        let result = compute_checksum(input);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_aad_passes_when_consistent() {
        let aad = AadFields {
            record_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            dek_version: 1,
        };
        let result = validate_aad("550e8400-e29b-41d4-a716-446655440000", 1, &aad);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_aad_fails_when_record_id_differs() {
        let aad = AadFields {
            record_id: "different-id".to_string(),
            dek_version: 1,
        };
        let result = validate_aad("550e8400-e29b-41d4-a716-446655440000", 1, &aad);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::AadInconsistent { .. }
        ));
    }

    #[test]
    fn test_validate_aad_fails_when_dek_version_differs() {
        let aad = AadFields {
            record_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            dek_version: 2,
        };
        let result = validate_aad("550e8400-e29b-41d4-a716-446655440000", 1, &aad);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::AadInconsistent { .. }
        ));
    }

    #[test]
    fn test_validate_uuid_accepts_valid_uuids() {
        assert!(validate_uuid("550e8400-e29b-41d4-a716-446655440000"));
        assert!(validate_uuid("00000000-0000-0000-0000-000000000000"));
        assert!(validate_uuid("a1b2c3d4-e5f6-7890-abcd-ef1234567890"));
    }

    #[test]
    fn test_validate_uuid_rejects_invalid_uuids() {
        assert!(!validate_uuid(""));
        assert!(!validate_uuid("not-a-uuid"));
        assert!(!validate_uuid("550e8400-e29b-41d4-a716"));
        assert!(!validate_uuid("550e8400-e29b-41d4-a716-44665544000g"));
        assert!(!validate_uuid("gggggggg-gggg-gggg-gggg-gggggggggggg"));
    }
}
