#[cfg(test)]
mod tests {
    use crate::sync::*;

    use crate::cloud::metadata::{DeviceInfo, RecordVersionInfo};
    use crate::cloud::record::{AadFields, RecordMetadata};
    use crate::cloud::{CloudMetadata, CloudRecord, CloudStorage};
    use crate::errors::mapping::sync::SyncError;
    use crate::types::sync::SyncStatus;
    use chrono::Utc;
    use tempfile::TempDir;

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
            nonce: "bm9uY2U".to_string(),
            dek_version: 1,
            aad: AadFields {
                record_id: id.to_string(),
                dek_version: 1,
            },
            metadata: RecordMetadata {
                name: format!("Test Record {}", id),
                tags: vec!["test".to_string()],
                updated_at: Utc::now().to_rfc3339(),
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
                    deleted: false,
                },
            );
        }
        metadata
    }

    // ===== PullMetadataStage Tests =====

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

        // Upload metadata first
        let metadata = create_test_metadata_with_records("test_token", 5, vec![]);
        storage.upload_metadata(&metadata).await.unwrap();

        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            5, // Same version as remote
            "test_token".to_string(),
        );

        let stage = PullMetadataStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(outcome, StageOutcome::NoChanges));
    }

    #[tokio::test]
    async fn pull_metadata_vault_identity_mismatch() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        // Upload metadata with different token
        let metadata = create_test_metadata_with_records("remote_token", 1, vec![]);
        storage.upload_metadata(&metadata).await.unwrap();

        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            0,
            "local_token".to_string(), // Different from remote
        );

        let stage = PullMetadataStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(
            matches!(outcome, StageOutcome::Error(e) if matches!(*e, SyncError::VaultIdentityMismatch { .. }))
        );
    }

    #[tokio::test]
    async fn pull_metadata_new_metadata() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        // Upload metadata with version 3
        let metadata = create_test_metadata_with_records("test_token", 3, vec![]);
        storage.upload_metadata(&metadata).await.unwrap();

        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            1, // Local version is 1, remote is 3
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

    // ===== DetectStage Tests =====

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

        // Remote has same version as local
        let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 1)]);
        context.remote_metadata = Some(metadata);

        // Local has Pending status at same version
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

        // Remote has newer version
        let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 2)]);
        context.remote_metadata = Some(metadata);

        // Local is Synced at older version
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

        // Remote has newer version
        let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 2)]);
        context.remote_metadata = Some(metadata);

        // Local has Pending at older version - should be conflict
        context.set_local_records(vec![LocalRecordInfo {
            record_id: "record-1".to_string(),
            sync_status: SyncStatus::Pending,
            version: 1,
        }]);

        let stage = DetectStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(
            outcome,
            StageOutcome::ConflictDetected { conflict_ids } if conflict_ids.contains(&"record-1".to_string())
        ));
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

        // First sync - no remote metadata
        context.remote_metadata = None;

        // Local has pending records
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

    // ===== PushStage Tests =====

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

        let record = create_test_cloud_record("record-1", 1);
        context.to_upload.push("record-1".to_string());
        context.set_uploads(vec![record.clone()]);

        let stage = PushStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(outcome, StageOutcome::Continue));
        assert!(context.failed_ids.is_empty());

        let downloaded = storage.download_record("record-1").await.unwrap();
        assert!(downloaded.is_some());
        assert_eq!(downloaded.unwrap().id, "record-1");
    }

    #[tokio::test]
    async fn push_downloads_records() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        // Use a valid UUID so record.validate() passes
        let record_id = "550e8400-e29b-41d4-a716-446655440000";
        let record = create_test_cloud_record(record_id, 2);
        storage.upload_record(record_id, &record).await.unwrap();

        // Compute the correct checksum for the uploaded record
        let correct_checksum = record.compute_checksum().unwrap();

        // Set up remote metadata with the matching checksum
        let mut metadata = create_test_metadata_with_records("test_token", 1, vec![(record_id, 2)]);
        metadata.upsert_record(
            record_id.to_string(),
            RecordVersionInfo {
                version: 2,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: correct_checksum,
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
                deleted: false,
            },
        );
        storage.upload_metadata(&metadata).await.unwrap();

        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            1,
            "test_token".to_string(),
        );

        context.remote_metadata = Some(metadata);
        context.conflicts.push(record_id.to_string());
        context.set_local_records(vec![LocalRecordInfo {
            record_id: record_id.to_string(),
            sync_status: SyncStatus::Pending,
            version: 1,
        }]);

        let stage = PushStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(
            outcome,
            StageOutcome::ConflictDetected { conflict_ids } if conflict_ids.contains(&record_id.to_string())
        ));
        assert!(context.conflict_data_map.contains_key(record_id));
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

        let record1 = create_test_cloud_record("record-1", 1);
        context.uploads.push(record1);
        context.to_upload.push("record-1".to_string());

        let stage = PushStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(outcome, StageOutcome::Continue));
        assert!(context.failed_ids.is_empty());
    }

    // ===== ResolveStage Tests =====

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

        // No conflicts
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

        // Has conflicts
        context.conflicts.push("record-1".to_string());

        let stage = ResolveStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(
            outcome,
            StageOutcome::ConflictDetected { conflict_ids } if conflict_ids.contains(&"record-1".to_string())
        ));
    }

    // ===== Full Pipeline Tests =====

    #[tokio::test]
    async fn full_pipeline_no_changes() {
        let (storage, _temp_dir) = create_test_storage();

        // Upload metadata
        let metadata = create_test_metadata_with_records("test_token", 5, vec![]);
        storage.upload_metadata(&metadata).await.unwrap();

        let checkpoint = create_test_checkpoint();
        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            5, // Same version
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

        // Set up context with records that need upload
        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            0,
            "test_token".to_string(),
        );

        // First sync - no remote metadata
        context.set_local_records(vec![LocalRecordInfo {
            record_id: "record-1".to_string(),
            sync_status: SyncStatus::Pending,
            version: 1,
        }]);

        let record = create_test_cloud_record("record-1", 1);
        context.set_uploads(vec![record]);

        let pipeline = SyncPipeline::new();
        let result = pipeline.execute(&mut context).await;

        assert!(matches!(result, PipelineResult::Completed));
    }
}
