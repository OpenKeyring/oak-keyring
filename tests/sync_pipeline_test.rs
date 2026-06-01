use chrono::Utc;
use oak_keyring::cloud::{CloudMetadata, CloudRecord, CloudStorage, DeviceInfo, RecordVersionInfo};
use oak_keyring::sync::{
    ConflictManager, DetectStage, LocalRecordInfo, PipelineContext, PipelineResult,
    PullMetadataStage, PushStage, ResolveStage, StageOutcome, SyncCheckpoint, SyncPipeline,
    SyncStage,
};
use oak_keyring::types::SyncStatus;
use tempfile::TempDir;
use uuid::Uuid;

fn create_test_storage() -> (CloudStorage, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let op = opendal::Operator::new(
        opendal::services::Fs::default().root(temp_dir.path().to_str().unwrap()),
    )
    .unwrap()
    .finish();
    (CloudStorage::new(op, "fs".to_string()), temp_dir)
}

fn create_test_checkpoint() -> SyncCheckpoint {
    let temp_dir = TempDir::new().unwrap();
    SyncCheckpoint::new(temp_dir.path())
}

fn create_test_cloud_record(id: &str, version: u64) -> CloudRecord {
    CloudRecord {
        id: id.to_string(),
        version,
        encrypted_data: "dGVzdCBkYXRh".to_string(),
        nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
        dek_version: 1,
        aad: oak_keyring::cloud::AadFields {
            record_id: id.to_string(),
            dek_version: 1,
        },
        metadata: oak_keyring::cloud::RecordMetadata {
            name: format!(
                "Test Record {
        }",
                id
            ),
            tags: vec!["test".to_string()],
            updated_at: Utc::now().to_rfc3339(),
            health: None,
            ..Default::default()
        },
        deleted: None,
        deleted_at: None,
    }
}

fn create_test_metadata_with_records(
    vault_token: &str,
    version: u64,
    records: Vec<(&str, u64)>,
) -> CloudMetadata {
    let mut metadata = CloudMetadata::new(vault_token.to_string());
    metadata.metadata_version = version;
    metadata.add_device(DeviceInfo {
        device_id: "device-1".to_string(),
        platform: "macos".to_string(),
        device_name: "MacBook Pro".to_string(),
        last_seen: Utc::now().to_rfc3339(),
        sync_count: 1,
    });
    for (id, ver) in records {
        metadata.upsert_record(
            id.to_string(),
            RecordVersionInfo {
                version: ver,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: "test_checksum".to_string(),
                private_metadata_checksum: None,
                deleted: false,
            },
        );
    }
    metadata
}

fn create_test_metadata_from_cloud_records(
    vault_token: &str,
    version: u64,
    records: &[CloudRecord],
) -> CloudMetadata {
    let mut metadata = CloudMetadata::new(vault_token.to_string());
    metadata.metadata_version = version;
    for record in records {
        metadata.upsert_record(
            record.id.clone(),
            RecordVersionInfo {
                version: record.version,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: record.compute_checksum().unwrap(),
                private_metadata_checksum: record.compute_private_metadata_checksum().unwrap(),
                deleted: record.deleted.unwrap_or(false),
            },
        );
    }
    metadata
}

fn create_test_metadata_with_private_checksum(
    vault_token: &str,
    metadata_version: u64,
    record_id: &str,
    record_version: u64,
    private_metadata_checksum: &str,
) -> CloudMetadata {
    let mut metadata = CloudMetadata::new(vault_token.to_string());
    metadata.metadata_version = metadata_version;
    metadata.upsert_record(
        record_id.to_string(),
        RecordVersionInfo {
            version: record_version,
            updated_at: Utc::now().to_rfc3339(),
            updated_by: "device-1".to_string(),
            checksum: "test_checksum".to_string(),
            private_metadata_checksum: Some(private_metadata_checksum.to_string()),
            deleted: false,
        },
    );
    metadata
}

#[tokio::test]
async fn pull_metadata_first_sync() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
    );

    let stage = PullMetadataStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.remote_metadata.is_none());
}

#[tokio::test]
async fn pull_metadata_no_changes() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();

    let metadata = create_test_metadata_with_records("test_token", 5, vec![]);
    storage.upload_metadata(&metadata).await.unwrap();

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        5,
        "test_token".to_string(),
    );

    let stage = PullMetadataStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::NoChanges));
}

#[tokio::test]
async fn pull_metadata_same_version_continues_when_local_has_pending_upload() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let record_id = Uuid::new_v4().to_string();

    let metadata = create_test_metadata_with_records("test_token", 5, vec![]);
    storage.upload_metadata(&metadata).await.unwrap();

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        5,
        "test_token".to_string(),
    );
    context.set_local_records(vec![LocalRecordInfo {
        record_id,
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);

    let stage = PullMetadataStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(
        matches!(outcome, StageOutcome::Continue),
        "local pending uploads must bypass metadata-version fast path"
    );
    assert!(context.remote_metadata.is_some());
}

#[tokio::test]
async fn pull_metadata_vault_identity_mismatch() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();

    let metadata = create_test_metadata_with_records("remote_token", 1, vec![]);
    storage.upload_metadata(&metadata).await.unwrap();

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "local_token".to_string(),
    );

    let stage = PullMetadataStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(
        outcome,
        StageOutcome::Error(e) if matches!(*e, oak_keyring::errors::mapping::sync::SyncError::VaultIdentityMismatch { .. })
    ));
}

#[tokio::test]
async fn pull_metadata_new_metadata() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();

    let metadata = create_test_metadata_with_records("test_token", 3, vec![]);
    storage.upload_metadata(&metadata).await.unwrap();

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    let stage = PullMetadataStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.remote_metadata.is_some());
    assert_eq!(
        context.remote_metadata.as_ref().unwrap().metadata_version,
        3
    );
}

#[tokio::test]
async fn detect_classifies_upload() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 1)]);
    context.remote_metadata = Some(metadata);

    context.set_local_records(vec![LocalRecordInfo {
        record_id: "record-1".to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);

    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.to_upload.contains(&"record-1".to_string()));
}

#[tokio::test]
async fn detect_classifies_download() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 2)]);
    context.remote_metadata = Some(metadata);

    context.set_local_records(vec![LocalRecordInfo {
        record_id: "record-1".to_string(),
        sync_status: SyncStatus::Synced,
        version: 1,
    }]);

    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.to_download.contains(&"record-1".to_string()));
}

#[tokio::test]
async fn detect_classifies_conflict() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 2)]);
    context.remote_metadata = Some(metadata);

    context.set_local_records(vec![LocalRecordInfo {
        record_id: "record-1".to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);

    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert_eq!(context.conflicts, vec!["record-1".to_string()]);
}

#[tokio::test]
async fn detect_no_remote_metadata() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
    );

    context.remote_metadata = None;

    context.set_local_records(vec![LocalRecordInfo {
        record_id: "record-1".to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);

    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.to_upload.contains(&"record-1".to_string()));
}

#[tokio::test]
async fn detect_classifies_remote_record_missing_locally_as_download() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let remote_id = Uuid::new_v4().to_string();
    let metadata = create_test_metadata_with_records("test_token", 2, vec![(&remote_id, 1)]);
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );
    context.remote_metadata = Some(metadata);
    context.set_local_records(vec![]);

    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert_eq!(context.to_download, vec![remote_id]);
    assert!(context.to_upload.is_empty());
    assert!(context.conflicts.is_empty());
}

#[tokio::test]
async fn detect_downloads_same_version_record_when_private_metadata_checksum_changed() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let record_id = Uuid::new_v4().to_string();
    let previous_metadata =
        create_test_metadata_with_private_checksum("test_token", 4, &record_id, 1, "old-private");
    let remote_metadata =
        create_test_metadata_with_private_checksum("test_token", 5, &record_id, 1, "new-private");
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        4,
        "test_token".to_string(),
    );
    context.remote_metadata = Some(remote_metadata);
    context.set_last_remote_metadata(Some(previous_metadata));
    context.set_local_records(vec![LocalRecordInfo {
        record_id: record_id.clone(),
        sync_status: SyncStatus::Synced,
        version: 1,
    }]);

    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert_eq!(context.to_download, vec![record_id]);
    assert!(context.to_upload.is_empty());
    assert!(context.conflicts.is_empty());
}

#[tokio::test]
async fn detect_downloads_same_version_private_metadata_on_first_snapshot_upgrade() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let record_id = Uuid::new_v4().to_string();
    let remote_metadata =
        create_test_metadata_with_private_checksum("test_token", 5, &record_id, 1, "new-private");
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        4,
        "test_token".to_string(),
    );
    context.remote_metadata = Some(remote_metadata);
    context.set_local_records(vec![LocalRecordInfo {
        record_id: record_id.clone(),
        sync_status: SyncStatus::Synced,
        version: 1,
    }]);

    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert_eq!(context.to_download, vec![record_id]);
    assert!(context.to_upload.is_empty());
    assert!(context.conflicts.is_empty());
}

#[tokio::test]
async fn full_pipeline_downloads_remote_record_missing_locally() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let remote_id = Uuid::new_v4().to_string();
    let record = create_test_cloud_record(&remote_id, 1);
    let metadata =
        create_test_metadata_from_cloud_records("test_token", 2, std::slice::from_ref(&record));

    storage.upload_metadata(&metadata).await.unwrap();
    storage.upload_record(&remote_id, &record).await.unwrap();

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;

    assert!(matches!(result, PipelineResult::Completed));
    assert!(
        context.downloads.contains_key(&remote_id),
        "remote-only records must be downloaded into pipeline context"
    );
}

#[tokio::test]
async fn push_uploads_records() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage.clone(),
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    let record_id = "550e8400-e29b-41d4-a716-446655440221";
    context.to_upload.push(record_id.to_string());
    let record = create_test_cloud_record(record_id, 1);
    context.set_uploads(vec![record]);

    let stage = PushStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.failed_ids.is_empty());

    let downloaded = storage.download_record(record_id).await.unwrap();
    assert!(downloaded.is_some());
    assert_eq!(downloaded.unwrap().id, record_id);
}

#[tokio::test]
async fn push_downloads_records() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();

    let record_id = "550e8400-e29b-41d4-a716-446655440000";
    let record = create_test_cloud_record(record_id, 2);
    storage.upload_record(record_id, &record).await.unwrap();

    let correct_checksum = record.compute_checksum().unwrap();

    let mut metadata = create_test_metadata_with_records("test_token", 1, vec![(record_id, 2)]);
    metadata.upsert_record(
        record_id.to_string(),
        RecordVersionInfo {
            version: 2,
            updated_at: Utc::now().to_rfc3339(),
            updated_by: "device-1".to_string(),
            checksum: correct_checksum,
            private_metadata_checksum: None,
            deleted: false,
        },
    );

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    context.remote_metadata = Some(metadata);
    context.to_download.push(record_id.to_string());

    let stage = PushStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.downloads.contains_key(record_id));
}

#[tokio::test]
async fn push_handles_conflicts() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();

    let record = create_test_cloud_record("record-1", 2);
    storage.upload_record("record-1", &record).await.unwrap();

    let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 2)]);
    storage.upload_metadata(&metadata).await.unwrap();

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    context.remote_metadata = Some(metadata);
    context.conflicts.push("record-1".to_string());
    context.set_local_records(vec![LocalRecordInfo {
        record_id: "record-1".to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);

    let stage = PushStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(
        outcome,
        StageOutcome::ConflictDetected { conflict_ids } if conflict_ids.contains(&"record-1".to_string())
    ));
    assert!(context.failed_ids.contains(&"record-1".to_string()));
}

#[tokio::test]
async fn push_partial_failure() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    context
        .to_upload
        .push("550e8400-e29b-41d4-a716-446655440001".to_string());
    context
        .to_upload
        .push("550e8400-e29b-41d4-a716-446655440002".to_string());
    let record1 = create_test_cloud_record("550e8400-e29b-41d4-a716-446655440001", 1);
    let record2 = create_test_cloud_record("550e8400-e29b-41d4-a716-446655440002", 1);
    context.set_uploads(vec![record1, record2]);

    let stage = PushStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
    assert!(context.failed_ids.is_empty());
    assert!(!context.checkpoint.push_completed_ids.is_empty());
}

#[tokio::test]
async fn resolve_no_conflicts() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    context.conflicts.clear();

    let stage = ResolveStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(outcome, StageOutcome::Continue));
}

#[tokio::test]
async fn resolve_with_conflicts() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    context.conflicts.push("record-1".to_string());

    let stage = ResolveStage::new();
    let outcome = stage.execute(&mut context).await;

    assert!(matches!(
        outcome,
        StageOutcome::ConflictDetected { conflict_ids } if conflict_ids.contains(&"record-1".to_string())
    ));
}

#[tokio::test]
async fn full_pipeline_no_changes() {
    let (storage, _temp_dir) = create_test_storage();

    let metadata = create_test_metadata_with_records("test_token", 5, vec![]);
    storage.upload_metadata(&metadata).await.unwrap();

    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        5,
        "test_token".to_string(),
    );

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;

    assert!(matches!(result, PipelineResult::NoChanges));
}

#[tokio::test]
async fn full_pipeline_happy_path() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
    );

    context.set_local_records(vec![LocalRecordInfo {
        record_id: "550e8400-e29b-41d4-a716-446655440222".to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);

    let record = create_test_cloud_record("550e8400-e29b-41d4-a716-446655440222", 1);
    context.set_uploads(vec![record]);

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;

    assert!(matches!(result, PipelineResult::Completed));
}
