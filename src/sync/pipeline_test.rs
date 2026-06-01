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
            nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            dek_version: 1,
            aad: AadFields {
                record_id: id.to_string(),
                dek_version: 1,
            },
            metadata: RecordMetadata {
                name: format!("Test Record {}", id),
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
    async fn detect_classifies_pending_local_newer_than_remote_as_upload() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();
        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            1,
            "test_token".to_string(),
        );

        let record_id = "550e8400-e29b-41d4-a716-446655440010";
        let metadata = create_test_metadata_with_records("test_token", 1, vec![(record_id, 1)]);
        context.remote_metadata = Some(metadata);

        context.set_local_records(vec![LocalRecordInfo {
            record_id: record_id.to_string(),
            sync_status: SyncStatus::Pending,
            version: 2,
        }]);

        let stage = DetectStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(outcome, StageOutcome::Continue));
        assert_eq!(context.to_upload, vec![record_id.to_string()]);
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

        let record_id = "550e8400-e29b-41d4-a716-446655440121";
        let record = create_test_cloud_record(record_id, 1);
        context.to_upload.push(record_id.to_string());
        context.set_uploads(vec![record.clone()]);

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
    async fn pipeline_uploads_pending_local_update_over_older_remote_version() {
        let (storage, _temp_dir) = create_test_storage();

        let record_id = "550e8400-e29b-41d4-a716-446655440011";
        let remote_record = create_test_cloud_record(record_id, 1);
        storage
            .upload_record(record_id, &remote_record)
            .await
            .unwrap();

        let mut remote_metadata =
            create_test_metadata_with_records("test_token", 1, vec![(record_id, 1)]);
        remote_metadata.upsert_record(
            record_id.to_string(),
            RecordVersionInfo {
                version: 1,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: remote_record.compute_checksum().unwrap(),
                private_metadata_checksum: None,
                deleted: false,
            },
        );
        storage.upload_metadata(&remote_metadata).await.unwrap();

        let local_record = create_test_cloud_record(record_id, 2);
        let mut context = PipelineContext::new(
            storage.clone(),
            ConflictManager::new(),
            create_test_checkpoint(),
            1,
            "test_token".to_string(),
        );
        context.set_local_records(vec![LocalRecordInfo {
            record_id: record_id.to_string(),
            sync_status: SyncStatus::Pending,
            version: 2,
        }]);
        context.set_uploads(vec![local_record.clone()]);

        let pipeline = SyncPipeline::new();
        let result = pipeline.execute(&mut context).await;

        assert!(matches!(result, PipelineResult::Completed));
        assert_eq!(context.uploaded_ids, vec![record_id.to_string()]);

        let uploaded_record = storage
            .download_record(record_id)
            .await
            .unwrap()
            .expect("updated cloud record should exist");
        assert_eq!(uploaded_record.version, 2);
        assert_eq!(uploaded_record.encrypted_data, local_record.encrypted_data);

        let uploaded_metadata = storage
            .download_metadata()
            .await
            .unwrap()
            .expect("updated cloud metadata should exist");
        let record_info = uploaded_metadata
            .records
            .get(record_id)
            .expect("updated metadata should include record");
        assert_eq!(record_info.version, 2);
        assert_eq!(
            record_info.checksum,
            local_record.compute_checksum().unwrap()
        );
        assert!(uploaded_metadata.metadata_version > remote_metadata.metadata_version);
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
                private_metadata_checksum: None,
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

        let record_id = "550e8400-e29b-41d4-a716-446655440122";
        let record1 = create_test_cloud_record(record_id, 1);
        context.uploads.push(record1);
        context.to_upload.push(record_id.to_string());

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
    async fn full_pipeline_conflict_downloads_remote_conflict_data() {
        let (storage, _temp_dir) = create_test_storage();

        let record_id = "550e8400-e29b-41d4-a716-446655440012";
        let remote_record = create_test_cloud_record(record_id, 2);
        storage
            .upload_record(record_id, &remote_record)
            .await
            .unwrap();

        let mut metadata = create_test_metadata_with_records("test_token", 2, vec![(record_id, 2)]);
        metadata.upsert_record(
            record_id.to_string(),
            RecordVersionInfo {
                version: 2,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: remote_record.compute_checksum().unwrap(),
                private_metadata_checksum: None,
                deleted: false,
            },
        );
        storage.upload_metadata(&metadata).await.unwrap();

        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            create_test_checkpoint(),
            1,
            "test_token".to_string(),
        );
        context.set_local_records(vec![LocalRecordInfo {
            record_id: record_id.to_string(),
            sync_status: SyncStatus::Pending,
            version: 1,
        }]);
        context.set_uploads(vec![create_test_cloud_record(record_id, 1)]);

        let pipeline = SyncPipeline::new();
        let result = pipeline.execute(&mut context).await;

        assert!(matches!(
            result,
            PipelineResult::ConflictsDetected { conflict_ids } if conflict_ids == vec![record_id.to_string()]
        ));
        assert!(
            context.conflict_data_map.contains_key(record_id),
            "conflict detection must carry downloadable remote conflict data for resolution"
        );
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
            record_id: "550e8400-e29b-41d4-a716-446655440123".to_string(),
            sync_status: SyncStatus::Pending,
            version: 1,
        }]);

        let record = create_test_cloud_record("550e8400-e29b-41d4-a716-446655440123", 1);
        context.set_uploads(vec![record]);

        let pipeline = SyncPipeline::new();
        let result = pipeline.execute(&mut context).await;

        assert!(matches!(result, PipelineResult::Completed));
    }

    // ===== Health State Sync Tests =====

    fn create_test_cloud_record_with_health(
        id: &str,
        version: u64,
        weak: Option<bool>,
        dup_size: Option<u32>,
    ) -> CloudRecord {
        use crate::cloud::record::RecordHealthMetadata;

        CloudRecord {
            id: id.to_string(),
            version,
            encrypted_data: "dGVzdCBkYXRh".to_string(),
            nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            dek_version: 1,
            aad: AadFields {
                record_id: id.to_string(),
                dek_version: 1,
            },
            metadata: RecordMetadata {
                name: format!("Test Record {}", id),
                tags: vec!["test".to_string()],
                updated_at: Utc::now().to_rfc3339(),
                health: Some(RecordHealthMetadata {
                    evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
                    weak_password: weak,
                    duplicate_group_size: dup_size,
                    compromised: Some(false),
                    expired: None,
                }),
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn download_extracts_health_state_from_cloud_record() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        let record_id = "550e8400-e29b-41d4-a716-446655440000";
        let record = create_test_cloud_record_with_health(record_id, 2, Some(true), Some(3));
        let correct_checksum = record.compute_checksum().unwrap();
        storage.upload_record(record_id, &record).await.unwrap();

        let mut metadata = create_test_metadata_with_records("test_token", 1, vec![(record_id, 2)]);
        metadata.upsert_record(
            record_id.to_string(),
            RecordVersionInfo {
                version: 2,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: correct_checksum,
                private_metadata_checksum: record.compute_private_metadata_checksum().unwrap(),
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

        // Verify health state was extracted
        assert_eq!(context.downloaded_health_states.len(), 1);
        let health = &context.downloaded_health_states[0];
        assert_eq!(health.record_id.to_string(), record_id);
        assert_eq!(health.record_version, 2);
        assert_eq!(health.weak_password, Some(true));
        assert_eq!(health.duplicate_group_size, Some(3));
        assert!(health.evaluated_at.is_some());
        assert!(context.downloaded_health_deleted.is_empty());
    }

    #[tokio::test]
    async fn download_schedules_health_deletion_when_no_health_metadata() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        let record_id = "550e8400-e29b-41d4-a716-446655440001";
        // Record without health metadata (old-format or uploaded by older client)
        let record = create_test_cloud_record(record_id, 2);
        let correct_checksum = record.compute_checksum().unwrap();
        storage.upload_record(record_id, &record).await.unwrap();

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

        // No health states extracted
        assert!(context.downloaded_health_states.is_empty());
        // But the record ID should be scheduled for deletion
        assert_eq!(context.downloaded_health_deleted.len(), 1);
        assert_eq!(context.downloaded_health_deleted[0].to_string(), record_id);
    }

    #[tokio::test]
    async fn upload_preserves_health_metadata_in_cloud_record() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        let record_id = "550e8400-e29b-41d4-a716-446655440002";
        let record = create_test_cloud_record_with_health(record_id, 1, Some(false), None);

        let mut context = PipelineContext::new(
            storage.clone(),
            ConflictManager::new(),
            checkpoint,
            1,
            "test_token".to_string(),
        );

        context.to_upload.push(record_id.to_string());
        context.set_uploads(vec![record.clone()]);

        let stage = PushStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(outcome, StageOutcome::Continue));
        assert!(context.failed_ids.is_empty());

        // Download the uploaded record and verify health metadata survived
        let downloaded = storage.download_record(record_id).await.unwrap().unwrap();
        assert!(downloaded.metadata.health.is_some());
        let health = downloaded.metadata.health.unwrap();
        assert_eq!(health.weak_password, Some(false));
        assert_eq!(health.compromised, Some(false));
    }

    #[tokio::test]
    async fn round_trip_health_state_via_pipeline() {
        let (storage, _temp_dir) = create_test_storage();

        // ── Device A: Upload ──────────────────────────────────────
        let record_id = "550e8400-e29b-41d4-a716-446655440003";
        let record_a = create_test_cloud_record_with_health(record_id, 1, Some(true), Some(5));

        let checkpoint_a = create_test_checkpoint();
        let mut context_a = PipelineContext::new(
            storage.clone(),
            ConflictManager::new(),
            checkpoint_a,
            0,
            "test_token".to_string(),
        );

        context_a.set_local_records(vec![LocalRecordInfo {
            record_id: record_id.to_string(),
            sync_status: SyncStatus::Pending,
            version: 1,
        }]);
        context_a.set_uploads(vec![record_a]);

        let pipeline_a = SyncPipeline::new();
        let result_a = pipeline_a.execute(&mut context_a).await;
        assert!(matches!(result_a, PipelineResult::Completed));

        // ── Device B: Download ─────────────────────────────────────
        // First, upload metadata so Device B can see remote changes.
        let mut metadata = CloudMetadata::new("test_token".to_string());
        metadata.metadata_version = 1;
        metadata.add_device(DeviceInfo {
            device_id: "device-b".to_string(),
            platform: "macos".to_string(),
            device_name: "MacBook Air".to_string(),
            last_seen: Utc::now().to_rfc3339(),
            sync_count: 1,
        });
        // Need to compute the checksum of the uploaded record
        let uploaded_record = storage.download_record(record_id).await.unwrap().unwrap();
        let checksum = uploaded_record.compute_checksum().unwrap();
        metadata.upsert_record(
            record_id.to_string(),
            RecordVersionInfo {
                version: 1,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-a".to_string(),
                checksum,
                private_metadata_checksum: uploaded_record
                    .compute_private_metadata_checksum()
                    .unwrap(),
                deleted: false,
            },
        );
        storage.upload_metadata(&metadata).await.unwrap();

        let checkpoint_b = create_test_checkpoint();
        let mut context_b = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint_b,
            0,
            "test_token".to_string(),
        );

        context_b.set_local_records(vec![LocalRecordInfo {
            record_id: record_id.to_string(),
            sync_status: SyncStatus::Synced,
            version: 0, // Behind remote
        }]);

        let pipeline_b = SyncPipeline::new();
        let result_b = pipeline_b.execute(&mut context_b).await;
        assert!(matches!(result_b, PipelineResult::Completed));

        // Verify health state was extracted on Device B
        assert_eq!(context_b.downloaded_health_states.len(), 1);
        let health = &context_b.downloaded_health_states[0];
        assert_eq!(health.weak_password, Some(true));
        assert_eq!(health.duplicate_group_size, Some(5));
        assert!(health.evaluated_at.is_some());
    }

    #[tokio::test]
    async fn mixed_download_with_and_without_health() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        let record_with_health = "550e8400-e29b-41d4-a716-446655440010";
        let record_without_health = "550e8400-e29b-41d4-a716-446655440011";

        let rec_a = create_test_cloud_record_with_health(record_with_health, 2, Some(true), None);
        let rec_b = create_test_cloud_record(record_without_health, 2);

        storage
            .upload_record(record_with_health, &rec_a)
            .await
            .unwrap();
        storage
            .upload_record(record_without_health, &rec_b)
            .await
            .unwrap();

        let checksum_a = rec_a.compute_checksum().unwrap();
        let checksum_b = rec_b.compute_checksum().unwrap();

        let mut metadata = create_test_metadata_with_records(
            "test_token",
            1,
            vec![(record_with_health, 2), (record_without_health, 2)],
        );
        metadata.upsert_record(
            record_with_health.to_string(),
            RecordVersionInfo {
                version: 2,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: checksum_a,
                private_metadata_checksum: rec_a.compute_private_metadata_checksum().unwrap(),
                deleted: false,
            },
        );
        metadata.upsert_record(
            record_without_health.to_string(),
            RecordVersionInfo {
                version: 2,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "device-1".to_string(),
                checksum: checksum_b,
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
        context.to_download.push(record_with_health.to_string());
        context.to_download.push(record_without_health.to_string());

        let stage = PushStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(outcome, StageOutcome::Continue));

        // One health state extracted, one deletion scheduled
        assert_eq!(context.downloaded_health_states.len(), 1);
        assert_eq!(
            context.downloaded_health_states[0].record_id.to_string(),
            record_with_health
        );
        assert_eq!(context.downloaded_health_deleted.len(), 1);
        assert_eq!(
            context.downloaded_health_deleted[0].to_string(),
            record_without_health
        );
    }
}
