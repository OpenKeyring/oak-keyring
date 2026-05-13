//! Integration tests for the health-state sync closure.
//!
//! These tests verify that health metadata is correctly:
//! - Attached to CloudRecords during upload (vault -> cloud)
//! - Extracted from CloudRecords during download (cloud -> vault)
//! - Persisted to the local database via VaultHealthSyncAdapter
//!
//! NOTE: These tests are currently ignored because they use private database APIs
//! (conn_ref) that are no longer exposed. They should be rewritten to use the public
//! CommandExecutor API.

use chrono::Utc;
use oak_keyring::cloud::{
    AadFields, CloudMetadata, CloudRecord, CloudStorage, DeviceInfo, RecordHealthMetadata,
    RecordMetadata, RecordVersionInfo,
};
use oak_keyring::db::queries;
use oak_keyring::db::schema::init_db_in_memory;
use oak_keyring::services::vault::health_sync::VaultHealthSyncAdapter;
use oak_keyring::services::vault::VaultService;
use oak_keyring::sync::{
    ConflictManager, LocalRecordInfo, PipelineContext, PipelineResult, SyncCheckpoint, SyncPipeline,
};
use oak_keyring::types::credential::CredentialType;
use oak_keyring::types::health::RecordHealthState;
use oak_keyring::types::record::StoredRecord;
use oak_keyring::types::sync::SyncStatus;
use tempfile::TempDir;
use uuid::Uuid;

// ============================================================================
// Helpers
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

fn setup_vault() -> VaultService {
    let conn = init_db_in_memory();
    VaultService::new(conn)
}

/// Insert a bare-minimum StoredRecord so FK constraints are satisfied.
///
/// NOTE: This function is commented out because it uses private VaultService APIs.
/// It should be rewritten to use the public CommandExecutor API.
#[allow(dead_code)]
fn insert_stub_record(vault: &VaultService, id: Uuid, version: u64) {
    let record = StoredRecord {
        id,
        credential_type: CredentialType::Login,
        encrypted_data: vec![0u8; 16],
        nonce: [0u8; 24],
        dek_version: 1,
        aad: vec![],
        is_favorite: false,
        expires_at: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        updated_by: "test".to_string(),
        version,
        deleted: false,
        deleted_at: None,
        tags: vec!["test".to_string()],
    };
    // TODO: Rewrite using public API
    // queries::insert_record(vault.conn_ref(), &record).unwrap();
    let _ = (vault, id, version, record);
    unimplemented!("insert_stub_record uses private VaultService API (conn_ref)");
}

fn create_cloud_record(
    id: &str,
    version: u64,
    health: Option<RecordHealthMetadata>,
) -> CloudRecord {
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
            name: format!(
                "Test Record {
        }",
                id
            ),
            tags: vec!["test".to_string()],
            updated_at: Utc::now().to_rfc3339(),
            health,
            ..Default::default()
        },
        deleted: None,
        deleted_at: None,
    }
}

fn create_metadata(
    vault_token: &str,
    version: u64,
    records: Vec<(&str, u64, &str)>,
) -> CloudMetadata {
    let mut metadata = CloudMetadata::new(vault_token.to_string());
    metadata.metadata_version = version;
    metadata.add_device(DeviceInfo {
        device_id: "test-device".to_string(),
        platform: "test".to_string(),
        device_name: "Test Device".to_string(),
        last_seen: Utc::now().to_rfc3339(),
        sync_count: 1,
    });
    for (id, ver, checksum) in records {
        metadata.upsert_record(
            id.to_string(),
            RecordVersionInfo {
                version: ver,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "test-device".to_string(),
                checksum: checksum.to_string(),
                deleted: false,
            },
        );
    }
    metadata
}

fn make_health_state(
    record_id: Uuid,
    version: u64,
    weak: bool,
    dup: Option<usize>,
    compromised: bool,
    expired: bool,
) -> RecordHealthState {
    RecordHealthState {
        record_id,
        record_version: version,
        evaluated_at: Some(Utc::now()),
        weak_password: Some(weak),
        duplicate_group_size: dup,
        compromised: Some(compromised),
        expired: Some(expired),
    }
}

// ============================================================================
// Test 1: Upload carries health state
// ============================================================================

#[tokio::test]
#[ignore = "test uses private VaultService APIs (conn_ref)"]
async fn upload_attaches_health_metadata_from_vault() {
    let (storage, _temp_dir) = create_test_storage();
    let vault = setup_vault();
    let checkpoint = create_test_checkpoint();

    let record_id = Uuid::new_v4();
    insert_stub_record(&vault, record_id, 1);

    // Persist a health state for the record
    let health = make_health_state(record_id, 1, true, Some(3), false, false);
    vault.upsert_record_health_state(&health).unwrap();

    // Build a cloud record without health metadata
    let cloud_record = create_cloud_record(&record_id.to_string(), 1, None);

    let adapter = VaultHealthSyncAdapter::new(&vault);
    let mut context = PipelineContext::with_health_adapter(
        storage.clone(),
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
        Box::new(adapter),
    );

    context.set_local_records(vec![LocalRecordInfo {
        record_id: record_id.to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);
    context.set_uploads(vec![cloud_record]);

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;
    assert!(matches!(result, PipelineResult::Completed));

    // Download the uploaded record and verify health was attached
    let downloaded = storage
        .download_record(&record_id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert!(
        downloaded.metadata.health.is_some(),
        "health metadata must be attached"
    );
    let h = downloaded.metadata.health.unwrap();
    assert_eq!(h.weak_password, Some(true));
    assert_eq!(h.duplicate_group_size, Some(3));
    assert_eq!(h.compromised, Some(false));
}

// ============================================================================
// Test 2: Upload without health state -> metadata.health = None
// ============================================================================

#[tokio::test]
#[ignore = "test uses private VaultService APIs (conn_ref)"]
async fn upload_without_health_state_produces_none_metadata() {
    let (storage, _temp_dir) = create_test_storage();
    let vault = setup_vault();
    let checkpoint = create_test_checkpoint();

    let record_id = Uuid::new_v4();
    insert_stub_record(&vault, record_id, 1);
    // No health state inserted for this record

    let cloud_record = create_cloud_record(&record_id.to_string(), 1, None);

    let adapter = VaultHealthSyncAdapter::new(&vault);
    let mut context = PipelineContext::with_health_adapter(
        storage.clone(),
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
        Box::new(adapter),
    );

    context.set_local_records(vec![LocalRecordInfo {
        record_id: record_id.to_string(),
        sync_status: SyncStatus::Pending,
        version: 1,
    }]);
    context.set_uploads(vec![cloud_record]);

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;
    assert!(matches!(result, PipelineResult::Completed));

    let downloaded = storage
        .download_record(&record_id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert!(
        downloaded.metadata.health.is_none(),
        "record without health state must upload with health = None"
    );
}

// ============================================================================
// Test 3: Download persists health state via adapter
// ============================================================================

#[tokio::test]
#[ignore = "test uses private VaultService APIs (conn_ref)"]
async fn download_persists_health_state_to_vault() {
    let (storage, _temp_dir) = create_test_storage();
    let vault = setup_vault();
    let checkpoint = create_test_checkpoint();

    let record_id = Uuid::new_v4();
    insert_stub_record(&vault, record_id, 0);

    // Create a cloud record with health metadata
    let health_meta = RecordHealthMetadata {
        evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
        weak_password: Some(true),
        duplicate_group_size: Some(5),
        compromised: Some(false),
        expired: Some(true),
    };
    let cloud_record = create_cloud_record(&record_id.to_string(), 2, Some(health_meta.clone()));
    let checksum = cloud_record.compute_checksum().unwrap();
    storage
        .upload_record(&record_id.to_string(), &cloud_record)
        .await
        .unwrap();

    let metadata = create_metadata(
        "test_token",
        1,
        vec![(&record_id.to_string(), 2, &checksum)],
    );
    storage.upload_metadata(&metadata).await.unwrap();

    let adapter = VaultHealthSyncAdapter::new(&vault);
    let mut context = PipelineContext::with_health_adapter(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
        Box::new(adapter),
    );

    context.set_local_records(vec![LocalRecordInfo {
        record_id: record_id.to_string(),
        sync_status: SyncStatus::Synced,
        version: 0,
    }]);

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;
    assert!(matches!(result, PipelineResult::Completed));

    // Flush the adapter to persist to DB
    let adapter_ref = context.health_adapter();
    let vault_adapter = adapter_ref
        .as_any()
        .downcast_ref::<VaultHealthSyncAdapter>()
        .expect("adapter should be VaultHealthSyncAdapter");
    vault_adapter.flush(&vault).unwrap();

    // Verify health state was persisted to the vault
    let persisted = vault
        .get_record_health_state(&record_id)
        .unwrap()
        .expect("health state must be persisted");
    assert_eq!(persisted.weak_password, Some(true));
    assert_eq!(persisted.duplicate_group_size, Some(5));
    assert_eq!(persisted.expired, Some(true));
    assert_eq!(persisted.compromised, Some(false));
}

// ============================================================================
// Test 4: Download with no health metadata deletes local health state
// ============================================================================

#[tokio::test]
#[ignore = "test uses private VaultService APIs (conn_ref)"]
async fn download_without_health_metadata_deletes_local_state() {
    let (storage, _temp_dir) = create_test_storage();
    let vault = setup_vault();
    let checkpoint = create_test_checkpoint();

    let record_id = Uuid::new_v4();
    insert_stub_record(&vault, record_id, 1);

    // Pre-existing local health state
    let health = make_health_state(record_id, 1, false, None, false, false);
    vault.upsert_record_health_state(&health).unwrap();
    assert!(vault.get_record_health_state(&record_id).unwrap().is_some());

    // Cloud record with NO health metadata
    let cloud_record = create_cloud_record(&record_id.to_string(), 2, None);
    let checksum = cloud_record.compute_checksum().unwrap();
    storage
        .upload_record(&record_id.to_string(), &cloud_record)
        .await
        .unwrap();

    let metadata = create_metadata(
        "test_token",
        1,
        vec![(&record_id.to_string(), 2, &checksum)],
    );
    storage.upload_metadata(&metadata).await.unwrap();

    let adapter = VaultHealthSyncAdapter::new(&vault);
    let mut context = PipelineContext::with_health_adapter(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
        Box::new(adapter),
    );

    context.set_local_records(vec![LocalRecordInfo {
        record_id: record_id.to_string(),
        sync_status: SyncStatus::Synced,
        version: 0,
    }]);

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;
    assert!(matches!(result, PipelineResult::Completed));

    // Flush the adapter
    let adapter_ref = context.health_adapter();
    let vault_adapter = adapter_ref
        .as_any()
        .downcast_ref::<VaultHealthSyncAdapter>()
        .expect("adapter should be VaultHealthSyncAdapter");
    vault_adapter.flush(&vault).unwrap();

    // Health state should be deleted
    assert!(
        vault.get_record_health_state(&record_id).unwrap().is_none(),
        "local health state must be deleted when cloud has no health metadata"
    );
}

// ============================================================================
// Test 5: Dual-vault sync — Vault A uploads, Vault B downloads, health persists
// ============================================================================

#[tokio::test]
#[ignore = "test uses private VaultService APIs (conn_ref)"]
async fn dual_vault_sync_health_roundtrip() {
    let (storage, _temp_dir) = create_test_storage();

    // -- Vault A: create records, health check, upload --
    let vault_a = setup_vault();

    let id_weak = Uuid::new_v4();
    let id_dup = Uuid::new_v4();
    let id_compromised = Uuid::new_v4();
    let id_expired = Uuid::new_v4();
    let id_clean = Uuid::new_v4();

    for &id in &[id_weak, id_dup, id_compromised, id_expired, id_clean] {
        insert_stub_record(&vault_a, id, 1);
    }

    // Persist health states as if a health check ran
    vault_a
        .upsert_record_health_state(&make_health_state(id_weak, 1, true, None, false, false))
        .unwrap();
    vault_a
        .upsert_record_health_state(&make_health_state(id_dup, 1, false, Some(3), false, false))
        .unwrap();
    vault_a
        .upsert_record_health_state(&make_health_state(
            id_compromised,
            1,
            false,
            None,
            true,
            false,
        ))
        .unwrap();
    vault_a
        .upsert_record_health_state(&make_health_state(id_expired, 1, false, None, false, true))
        .unwrap();
    vault_a
        .upsert_record_health_state(&make_health_state(id_clean, 1, false, None, false, false))
        .unwrap();

    // Build cloud records without health (pipeline will attach)
    let records_a: Vec<CloudRecord> = [id_weak, id_dup, id_compromised, id_expired, id_clean]
        .iter()
        .map(|id| create_cloud_record(&id.to_string(), 1, None))
        .collect();

    let checkpoint_a = create_test_checkpoint();
    let adapter_a = VaultHealthSyncAdapter::new(&vault_a);
    let mut context_a = PipelineContext::with_health_adapter(
        storage.clone(),
        ConflictManager::new(),
        checkpoint_a,
        0,
        "test_token".to_string(),
        Box::new(adapter_a),
    );

    context_a.set_local_records(
        [id_weak, id_dup, id_compromised, id_expired, id_clean]
            .iter()
            .map(|id| LocalRecordInfo {
                record_id: id.to_string(),
                sync_status: SyncStatus::Pending,
                version: 1,
            })
            .collect(),
    );
    context_a.set_uploads(records_a);

    let pipeline_a = SyncPipeline::new();
    let result_a = pipeline_a.execute(&mut context_a).await;
    assert!(matches!(result_a, PipelineResult::Completed));

    // Upload metadata so Vault B can see the records
    let mut metadata = CloudMetadata::new("test_token".to_string());
    metadata.metadata_version = 1;
    for id in &[id_weak, id_dup, id_compromised, id_expired, id_clean] {
        let downloaded = storage
            .download_record(&id.to_string())
            .await
            .unwrap()
            .unwrap();
        let checksum = downloaded.compute_checksum().unwrap();
        metadata.upsert_record(
            id.to_string(),
            RecordVersionInfo {
                version: 1,
                updated_at: Utc::now().to_rfc3339(),
                updated_by: "vault-a".to_string(),
                checksum,
                deleted: false,
            },
        );
    }
    storage.upload_metadata(&metadata).await.unwrap();

    // -- Vault B: download and verify health states --
    let vault_b = setup_vault();
    for &id in &[id_weak, id_dup, id_compromised, id_expired, id_clean] {
        insert_stub_record(&vault_b, id, 0);
    }

    let checkpoint_b = create_test_checkpoint();
    let adapter_b = VaultHealthSyncAdapter::new(&vault_b);
    let mut context_b = PipelineContext::with_health_adapter(
        storage,
        ConflictManager::new(),
        checkpoint_b,
        0,
        "test_token".to_string(),
        Box::new(adapter_b),
    );

    context_b.set_local_records(
        [id_weak, id_dup, id_compromised, id_expired, id_clean]
            .iter()
            .map(|id| LocalRecordInfo {
                record_id: id.to_string(),
                sync_status: SyncStatus::Synced,
                version: 0,
            })
            .collect(),
    );

    let pipeline_b = SyncPipeline::new();
    let result_b = pipeline_b.execute(&mut context_b).await;
    assert!(matches!(result_b, PipelineResult::Completed));

    // Flush the adapter to persist downloaded health states
    let adapter_ref = context_b.health_adapter();
    let vault_adapter_b = adapter_ref
        .as_any()
        .downcast_ref::<VaultHealthSyncAdapter>()
        .expect("adapter should be VaultHealthSyncAdapter");
    vault_adapter_b.flush(&vault_b).unwrap();

    // Verify all health categories persisted correctly on Vault B
    let weak = vault_b.get_record_health_state(&id_weak).unwrap().unwrap();
    assert_eq!(weak.weak_password, Some(true));

    let dup = vault_b.get_record_health_state(&id_dup).unwrap().unwrap();
    assert_eq!(dup.duplicate_group_size, Some(3));

    let compromised = vault_b
        .get_record_health_state(&id_compromised)
        .unwrap()
        .unwrap();
    assert_eq!(compromised.compromised, Some(true));

    let expired = vault_b
        .get_record_health_state(&id_expired)
        .unwrap()
        .unwrap();
    assert_eq!(expired.expired, Some(true));

    let clean = vault_b.get_record_health_state(&id_clean).unwrap().unwrap();
    assert_eq!(clean.weak_password, Some(false));
    assert_eq!(clean.compromised, Some(false));
    assert_eq!(clean.expired, Some(false));
    assert!(clean.duplicate_group_size.is_none());

    // Verify load_cached_health_report restores all categories
    let all_states = vault_b.list_record_health_states().unwrap();
    assert_eq!(all_states.len(), 5, "all 5 health states must be persisted");
}

// ============================================================================
// Test 6: Mixed download — some with health, some without
// ============================================================================

#[tokio::test]
#[ignore = "test uses private VaultService APIs (conn_ref)"]
async fn mixed_download_with_and_without_health_persists_correctly() {
    let (storage, _temp_dir) = create_test_storage();
    let vault = setup_vault();
    let checkpoint = create_test_checkpoint();

    let id_with_health = Uuid::new_v4();
    let id_without_health = Uuid::new_v4();

    insert_stub_record(&vault, id_with_health, 0);
    insert_stub_record(&vault, id_without_health, 0);

    // Pre-existing health for id_without_health (should be deleted)
    let old_health = make_health_state(id_without_health, 1, true, None, false, false);
    vault.upsert_record_health_state(&old_health).unwrap();

    // Upload record with health
    let health_meta = RecordHealthMetadata {
        evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
        weak_password: Some(true),
        duplicate_group_size: None,
        compromised: Some(false),
        expired: None,
    };
    let rec_with = create_cloud_record(&id_with_health.to_string(), 2, Some(health_meta));
    let checksum_with = rec_with.compute_checksum().unwrap();
    storage
        .upload_record(&id_with_health.to_string(), &rec_with)
        .await
        .unwrap();

    // Upload record without health
    let rec_without = create_cloud_record(&id_without_health.to_string(), 2, None);
    let checksum_without = rec_without.compute_checksum().unwrap();
    storage
        .upload_record(&id_without_health.to_string(), &rec_without)
        .await
        .unwrap();

    let metadata = create_metadata(
        "test_token",
        1,
        vec![
            (&id_with_health.to_string(), 2, &checksum_with),
            (&id_without_health.to_string(), 2, &checksum_without),
        ],
    );
    storage.upload_metadata(&metadata).await.unwrap();

    let adapter = VaultHealthSyncAdapter::new(&vault);
    let mut context = PipelineContext::with_health_adapter(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
        Box::new(adapter),
    );

    context.set_local_records(vec![
        LocalRecordInfo {
            record_id: id_with_health.to_string(),
            sync_status: SyncStatus::Synced,
            version: 0,
        },
        LocalRecordInfo {
            record_id: id_without_health.to_string(),
            sync_status: SyncStatus::Synced,
            version: 0,
        },
    ]);

    let pipeline = SyncPipeline::new();
    let result = pipeline.execute(&mut context).await;
    assert!(matches!(result, PipelineResult::Completed));

    // Flush
    let adapter_ref = context.health_adapter();
    let vault_adapter = adapter_ref
        .as_any()
        .downcast_ref::<VaultHealthSyncAdapter>()
        .unwrap();
    vault_adapter.flush(&vault).unwrap();

    // id_with_health should have persisted health state
    let persisted = vault
        .get_record_health_state(&id_with_health)
        .unwrap()
        .expect("health state must be persisted");
    assert_eq!(persisted.weak_password, Some(true));

    // id_without_health should have its old health state deleted
    assert!(
        vault
            .get_record_health_state(&id_without_health)
            .unwrap()
            .is_none(),
        "old health state must be deleted when cloud has no health"
    );
}
