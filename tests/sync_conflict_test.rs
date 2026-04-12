//! Integration tests for ConflictManager with full conflict detection, storage, and resolution.

use oak_keyring::cloud::record::{AadFields, CloudRecord, ConflictPayload, RecordMetadata};
use oak_keyring::sync::{
    ConflictAction, ConflictItem, ConflictManager, ResolutionAction, ResolutionStrategy,
};
use oak_keyring::types::sync::SyncStatus;
use uuid::Uuid;

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
            tags: vec!["dev".to_string()],
            updated_at: "2026-04-05T12:00:00Z".to_string(),
        },
        deleted: None,
        deleted_at: None,
    }
}

#[test]
fn detect_conflict_both_modified() {
    let manager = ConflictManager::new();
    let result = manager.detect_conflicts(SyncStatus::Pending, 5, 6);
    assert!(matches!(
        result,
        ConflictAction::Conflict {
            local_version: 5,
            remote_version: 6
        }
    ));
}

#[test]
fn detect_conflict_remote_only() {
    let manager = ConflictManager::new();
    let result = manager.detect_conflicts(SyncStatus::Synced, 5, 6);
    assert!(matches!(result, ConflictAction::DownloadOnly));
}

#[test]
fn detect_conflict_local_only() {
    let manager = ConflictManager::new();
    let result = manager.detect_conflicts(SyncStatus::Pending, 5, 5);
    assert!(matches!(result, ConflictAction::UploadOnly));
}

#[test]
fn detect_conflict_no_action() {
    let manager = ConflictManager::new();
    let result = manager.detect_conflicts(SyncStatus::Synced, 5, 5);
    assert!(matches!(result, ConflictAction::NoAction));
}

#[test]
fn store_conflict_valid() {
    let manager = ConflictManager::new();
    let record = create_valid_cloud_record();
    let checksum = record.compute_checksum().unwrap();

    let result = manager.store_conflict(&record, &checksum);

    assert!(result.is_ok());
    let bytes = result.unwrap();
    let payload = ConflictPayload::deserialize(&bytes).unwrap();
    assert_eq!(payload.cloud_record.id, record.id);
    assert_eq!(payload.checksum, checksum);
}

#[test]
fn store_conflict_invalid_checksum() {
    let manager = ConflictManager::new();
    let record = create_valid_cloud_record();

    let result = manager.store_conflict(&record, "invalid_checksum");

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        oak_keyring::errors::mapping::sync::SyncError::ChecksumMismatch { .. }
    ));
}

#[test]
fn resolve_keep_local_bumps_version() {
    let manager = ConflictManager::new();
    let result = manager.resolve_keep_local(5);

    assert_eq!(result.new_version, 6);
    assert!(matches!(result.action, ResolutionAction::KeepLocal));
}

#[test]
fn resolve_keep_remote_valid_data() {
    let manager = ConflictManager::new();
    let record = create_valid_cloud_record();
    let checksum = record.compute_checksum().unwrap();
    let payload = ConflictPayload {
        cloud_record: record.clone(),
        checksum,
    };
    let conflict_data = payload.serialize().unwrap();

    let result = manager.resolve_keep_remote(&conflict_data);

    assert!(result.is_ok());
    let keep_remote = result.unwrap();
    assert_eq!(keep_remote.encrypted_data_base64, record.encrypted_data);
    assert_eq!(keep_remote.nonce_base64, record.nonce);
    assert_eq!(keep_remote.dek_version, record.dek_version);
    assert_eq!(keep_remote.version, record.version);
    assert_eq!(keep_remote.updated_at, record.metadata.updated_at);
}

#[test]
fn resolve_keep_remote_corrupted_json() {
    let manager = ConflictManager::new();
    let corrupt_data = b"{ invalid json }";

    let result = manager.resolve_keep_remote(corrupt_data);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        oak_keyring::errors::mapping::sync::SyncError::DeserializationFailed { .. }
    ));
}

#[test]
fn resolve_keep_remote_checksum_mismatch() {
    let manager = ConflictManager::new();
    let record = create_valid_cloud_record();
    let payload = ConflictPayload {
        cloud_record: record,
        checksum: "wrong_checksum".to_string(),
    };
    let conflict_data = payload.serialize().unwrap();

    let result = manager.resolve_keep_remote(&conflict_data);

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        oak_keyring::errors::mapping::sync::SyncError::ChecksumMismatch { .. }
    ));
}

#[test]
fn resolve_all_batch_keep_local() {
    let manager = ConflictManager::new();
    let items = vec![
        ConflictItem {
            record_id: Uuid::new_v4(),
            conflict_data: vec![],
            current_version: 1,
        },
        ConflictItem {
            record_id: Uuid::new_v4(),
            conflict_data: vec![],
            current_version: 2,
        },
        ConflictItem {
            record_id: Uuid::new_v4(),
            conflict_data: vec![],
            current_version: 3,
        },
    ];

    let outcomes = manager.resolve_all_batch(&items, ResolutionStrategy::KeepLocal);

    assert_eq!(outcomes.len(), 3);
    for (i, outcome) in outcomes.iter().enumerate() {
        assert!(outcome.result.is_ok());
        let resolved = outcome.result.as_ref().unwrap();
        assert_eq!(resolved.new_version, (i + 1) as u64 + 1);
    }
}

#[test]
fn resolve_all_batch_partial_failure() {
    let manager = ConflictManager::new();
    let record = create_valid_cloud_record();
    let checksum = record.compute_checksum().unwrap();
    let payload = ConflictPayload {
        cloud_record: record,
        checksum,
    };
    let good_conflict_data = payload.serialize().unwrap();

    let id1 = Uuid::new_v4();
    let id2 = Uuid::new_v4();
    let id3 = Uuid::new_v4();

    let items = vec![
        ConflictItem {
            record_id: id1,
            conflict_data: good_conflict_data.clone(),
            current_version: 1,
        },
        ConflictItem {
            record_id: id2,
            conflict_data: b"corrupted data".to_vec(),
            current_version: 2,
        },
        ConflictItem {
            record_id: id3,
            conflict_data: good_conflict_data,
            current_version: 3,
        },
    ];

    let outcomes = manager.resolve_all_batch(&items, ResolutionStrategy::KeepRemote);

    assert_eq!(outcomes.len(), 3);
    assert!(outcomes[0].result.is_ok());
    assert_eq!(outcomes[0].record_id, id1);
    assert!(outcomes[1].result.is_err());
    assert_eq!(outcomes[1].record_id, id2);
    assert!(outcomes[2].result.is_ok());
    assert_eq!(outcomes[2].record_id, id3);
}

#[test]
fn resolve_all_batch_empty() {
    let manager = ConflictManager::new();
    let items: Vec<ConflictItem> = vec![];

    let outcomes = manager.resolve_all_batch(&items, ResolutionStrategy::KeepLocal);

    assert!(outcomes.is_empty());
}
