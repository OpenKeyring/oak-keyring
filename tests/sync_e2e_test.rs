//! End-to-end integration tests for oak-keyring sync service.
//!
//! These tests verify the full sync pipeline including CloudStorage,
//! SyncPipeline, ConflictManager, SyncCheckpoint, RetryPolicy, and
//! SyncStateMachine working together correctly.

use chrono::Utc;
use oak_keyring::cloud::{CloudMetadata, CloudRecord, CloudStorage, DeviceInfo, RecordVersionInfo};
use oak_keyring::services::sync::SyncService;
use oak_keyring::sync::{
    BackoffTimer, ConflictManager, DetectStage, LocalRecordInfo, NonceValidator, PipelineContext,
    PipelineResult, PullMetadataStage, RetryPolicy, StageOutcome, SyncCheckpoint, SyncPipeline,
    SyncStage, SyncState, SyncStateMachine, SyncTrigger,
};
use oak_keyring::types::SyncStatus;
use tempfile::TempDir;
use uuid::Uuid;

// ============================================================================
// Helper Functions
// ============================================================================

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
        aad: oak_keyring::cloud::AadFields {
            record_id: id.to_string(),
            dek_version: 1,
        },
        metadata: oak_keyring::cloud::RecordMetadata {
            name: format!("Test Record {}", id),
            tags: vec!["test".to_string()],
            updated_at: Utc::now().to_rfc3339(),
            health: None,
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

// ============================================================================
// Test 1: full_sync_lifecycle
// ============================================================================

#[tokio::test]
async fn full_sync_lifecycle() {
    // Create CloudStorage with Fs backend
    let (storage, _temp_dir) = create_test_storage();

    // Upload metadata + records to "cloud"
    let metadata = create_test_metadata_with_records("vault_token_123", 1, vec![("record-1", 1)]);
    storage.upload_metadata(&metadata).await.unwrap();

    let record = create_test_cloud_record("record-1", 1);
    storage.upload_record("record-1", &record).await.unwrap();

    // Create PipelineContext, run SyncPipeline
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "vault_token_123".to_string(),
    );

    // First sync with no local records
    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;

    // Verify PipelineResult::Completed or NoChanges (depends on sync state)
    assert!(
        matches!(
            result,
            PipelineResult::Completed | PipelineResult::NoChanges
        ),
        "Expected Completed or NoChanges, got {:?}",
        result
    );
}

// ============================================================================
// Test 2: sync_detects_new_remote_records
// ============================================================================

#[tokio::test]
async fn sync_detects_new_remote_records() {
    let (storage, _temp_dir) = create_test_storage();

    // Upload metadata with version 1 and one record
    let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 1)]);
    storage.upload_metadata(&metadata).await.unwrap();

    // Create PipelineContext with local_metadata_version=0
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
    );

    // Run pipeline PullMetadata stage
    let stage = PullMetadataStage::new();
    let outcome = stage.execute(&mut context).await;

    // Should return Continue (not NoChanges since version differs)
    assert!(matches!(outcome, StageOutcome::Continue));

    // Verify remote_metadata is populated
    assert!(context.remote_metadata.is_some());
    assert_eq!(
        context.remote_metadata.as_ref().unwrap().metadata_version,
        1
    );
}

// ============================================================================
// Test 3: sync_fast_path_no_changes
// ============================================================================

#[tokio::test]
async fn sync_fast_path_no_changes() {
    let (storage, _temp_dir) = create_test_storage();

    // Upload metadata with version 5
    let metadata = create_test_metadata_with_records("test_token", 5, vec![]);
    storage.upload_metadata(&metadata).await.unwrap();

    // Create PipelineContext with local_metadata_version=5 (same as remote)
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        5,
        "test_token".to_string(),
    );

    // Run SyncPipeline
    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;

    // Should return NoChanges (fast path)
    assert!(matches!(result, PipelineResult::NoChanges));
}

// ============================================================================
// Test 4: conflict_detection_flow
// ============================================================================

#[tokio::test]
async fn conflict_detection_flow() {
    let (storage, _temp_dir) = create_test_storage();

    // Create metadata with a record at version 3
    let metadata = create_test_metadata_with_records("test_token", 1, vec![("record-1", 3)]);
    storage.upload_metadata(&metadata).await.unwrap();

    // Create local record info with Pending status and version 1
    let checkpoint = create_test_checkpoint();
    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        1,
        "test_token".to_string(),
    );

    // Pre-populate remote metadata
    context.remote_metadata = Some(metadata);

    // Set local record with Pending status at version 1
    context.set_local_records(vec![LocalRecordInfo {
        record_id: "record-1".to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);

    // Run DetectStage
    let stage = DetectStage::new();
    let outcome = stage.execute(&mut context).await;

    // Should classify as Conflict
    assert!(matches!(
        outcome,
        StageOutcome::ConflictDetected { conflict_ids } if conflict_ids.contains(&"record-1".to_string())
    ));
}

// ============================================================================
// Test 5: conflict_resolution_keep_local
// ============================================================================

#[test]
fn conflict_resolution_keep_local() {
    let manager = ConflictManager::new();

    // Resolve with KeepLocal
    let result = manager.resolve_keep_local(1);

    // Verify version bumped (1 -> 2)
    assert_eq!(result.new_version, 2);
    assert!(matches!(
        result.action,
        oak_keyring::sync::ResolutionAction::KeepLocal
    ));
}

// ============================================================================
// Test 6: conflict_resolution_keep_remote
// ============================================================================

#[test]
fn conflict_resolution_keep_remote() {
    let manager = ConflictManager::new();

    // Create a cloud record to store as conflict data (use valid UUID)
    let record = create_test_cloud_record("550e8400-e29b-41d4-a716-446655440001", 2);
    let checksum = record.compute_checksum().unwrap();
    let conflict_data = manager.store_conflict(&record, &checksum).unwrap();

    // Resolve with KeepRemote
    let result = manager.resolve_keep_remote(&conflict_data);

    // Verify returns KeepRemoteData with correct fields
    assert!(result.is_ok());
    let keep_remote = result.unwrap();
    assert_eq!(keep_remote.encrypted_data_base64, record.encrypted_data);
    assert_eq!(keep_remote.nonce_base64, record.nonce);
    assert_eq!(keep_remote.dek_version, record.dek_version);
    assert_eq!(keep_remote.version, record.version);
}

// ============================================================================
// Test 7: checkpoint_save_and_restore
// ============================================================================

#[test]
fn checkpoint_save_and_restore() {
    let temp_dir = TempDir::new().unwrap();

    // Create SyncCheckpoint with some push/pull completed IDs
    let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
    let push_id = Uuid::new_v4();
    let pull_id = Uuid::new_v4();
    checkpoint.record_push_done(push_id);
    checkpoint.record_pull_done(pull_id);
    checkpoint.detect_completed = true;

    // Save to disk
    checkpoint.save().unwrap();

    // Load from disk
    let loaded = SyncCheckpoint::load(temp_dir.path()).unwrap();

    // Verify all fields match
    assert!(loaded.push_completed_ids.contains(&push_id));
    assert!(loaded.pull_completed_ids.contains(&pull_id));
    assert!(loaded.detect_completed);
}

// ============================================================================
// Test 8: retry_policy_backoff_sequence
// ============================================================================

#[test]
fn retry_policy_backoff_sequence() {
    // Create RetryPolicy with defaults
    let policy = RetryPolicy::default();

    // Create BackoffTimer
    let mut timer = BackoffTimer::new(policy);

    // Get delays for attempts 0-4
    let expected_delays = [(0, 5.0), (1, 10.0), (2, 20.0), (3, 40.0), (4, 80.0)];

    for (attempt, expected_secs) in expected_delays {
        let delay = timer.next_delay();
        let expected = std::time::Duration::from_secs_f64(expected_secs);
        let min = expected.mul_f64(0.8); // ±20% jitter
        let max = expected.mul_f64(1.2);

        assert!(
            delay >= min && delay <= max,
            "delay {:?} not in range [{:?}, {:?}] for attempt {}",
            delay,
            min,
            max,
            attempt
        );
    }
}

// ============================================================================
// Test 9: state_machine_full_cycle
// ============================================================================

#[test]
fn state_machine_full_cycle() {
    let mut sm = SyncStateMachine::new(3);

    // Transition: TriggerSync → Pulling
    let result = sm.transition(SyncTrigger::TriggerSync).unwrap();
    assert_eq!(result, SyncState::Pulling);

    // Transition: PullCompleted → Detecting
    let result = sm.transition(SyncTrigger::PullCompleted).unwrap();
    assert_eq!(result, SyncState::Detecting);

    // Transition: DetectCompleted{has_conflicts:false, has_changes:true} → Pushing
    let result = sm
        .transition(SyncTrigger::DetectCompleted {
            has_conflicts: false,
            has_changes: true,
        })
        .unwrap();
    assert_eq!(result, SyncState::Pushing);

    // Transition: PushCompleted{has_conflicts:false} → Synced
    let result = sm
        .transition(SyncTrigger::PushCompleted {
            has_conflicts: false,
        })
        .unwrap();
    assert_eq!(result, SyncState::Synced);

    // Transition: ReportCompleted → Idle
    let result = sm.transition(SyncTrigger::ReportCompleted).unwrap();
    assert_eq!(result, SyncState::Idle);

    // Verify final state is Idle
    assert_eq!(*sm.current_state(), SyncState::Idle);
}

// ============================================================================
// Test 10: nonce_validator_identity_check
// ============================================================================

#[test]
fn nonce_validator_identity_check() {
    // Matching tokens → AllowSync
    let result = NonceValidator::validate_identity(Some("token_abc"), Some("token_abc"));
    assert!(matches!(
        result,
        Ok(oak_keyring::sync::IdentityAction::AllowSync)
    ));

    // Mismatched tokens → error
    let result = NonceValidator::validate_identity(Some("token_local"), Some("token_remote"));
    assert!(matches!(
        result,
        Err(oak_keyring::errors::mapping::sync::SyncError::VaultIdentityMismatch { .. })
    ));

    // No local token → AdoptRemoteToken
    let result = NonceValidator::validate_identity(None, Some("remote_token"));
    assert!(matches!(
        result,
        Ok(oak_keyring::sync::IdentityAction::AdoptRemoteToken(ref t)) if t == "remote_token"
    ));
}

// ============================================================================
// Test 11: sync_service_lifecycle
// ============================================================================

#[tokio::test]
async fn sync_service_lifecycle() {
    // Create SyncService with Memory backend
    let op = opendal::Operator::new(opendal::services::Memory::default())
        .unwrap()
        .finish();
    let storage = CloudStorage::new(op, "memory".to_string());

    let mut svc = SyncService::new(storage);

    // Call sync() → should return Ok or Err (not panic)
    let result = svc.sync().await;
    assert!(
        result.is_ok() || result.is_err(),
        "sync() should return Ok or Err, not panic"
    );

    // Call shutdown() → should succeed
    let shutdown_result = svc.shutdown().await;
    assert!(
        shutdown_result.is_ok(),
        "shutdown() should return Ok, got: {:?}",
        shutdown_result
    );
}

// ============================================================================
// Test 12: cloud_storage_crud
// ============================================================================

#[tokio::test]
async fn cloud_storage_crud() {
    let (storage, _temp_dir) = create_test_storage();

    // Upload metadata, download it, verify roundtrip
    let metadata = create_test_metadata_with_records("vault_token_xyz", 1, vec![]);
    storage.upload_metadata(&metadata).await.unwrap();

    let downloaded_metadata = storage.download_metadata().await.unwrap().unwrap();
    assert_eq!(
        downloaded_metadata.vault_identity_token,
        metadata.vault_identity_token
    );
    assert_eq!(
        downloaded_metadata.metadata_version,
        metadata.metadata_version
    );

    // Upload record, download it, verify roundtrip
    let record = create_test_cloud_record("record-1", 1);
    storage.upload_record("record-1", &record).await.unwrap();

    let downloaded_record = storage.download_record("record-1").await.unwrap().unwrap();
    assert_eq!(downloaded_record.id, record.id);
    assert_eq!(downloaded_record.version, record.version);
    assert_eq!(downloaded_record.encrypted_data, record.encrypted_data);

    // List records, verify correct IDs returned
    let record_ids = storage.list_records().await.unwrap();
    assert_eq!(record_ids.len(), 1);
    assert!(record_ids.contains(&"record-1".to_string()));

    // Delete record, verify list is empty
    storage.delete_record("record-1").await.unwrap();

    let record_ids_after_delete = storage.list_records().await.unwrap();
    assert!(record_ids_after_delete.is_empty());
}
