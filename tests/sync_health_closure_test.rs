//! Integration tests for the health-state sync closure.
//!
//! Health sync uses the same executor/vault boundary as private cloud metadata:
//! the vault encrypts health attributes into `CloudRecord.metadata.encrypted_metadata`
//! before upload, and applies downloaded encrypted metadata back into local
//! `record_health_state` rows after the sync service returns downloaded records.

use chrono::Utc;
use oak_keyring::cloud::{
    AadFields, CloudMetadata, CloudRecord, CloudStorage, DeviceInfo, RecordHealthMetadata,
    RecordMetadata, RecordVersionInfo,
};
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::services::vault::VaultService;
use oak_keyring::sync::{
    ConflictManager, LocalRecordInfo, PipelineContext, PipelineResult, SyncCheckpoint, SyncPipeline,
};
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::health::RecordHealthState;
use oak_keyring::types::record::CreateRecordParams;
use oak_keyring::types::sensitive::SecureStr;
use oak_keyring::types::sync::SyncStatus;
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

fn setup_unlocked_vault() -> VaultService {
    let conn = oak_keyring::db::schema::init_db_in_memory().unwrap();
    let mut vault = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    vault
        .unlock_with_mnemonic(&mnemonic)
        .expect("test vault must unlock");
    vault
}

fn create_login_record(vault: &mut VaultService, name: &str) -> Uuid {
    vault
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: "alice@example.com".to_string(),
                password: SecureStr::new("password123".to_string()),
                url: None,
                notes: None,
            },
            tags: vec!["sync-health".to_string()],
            is_favorite: false,
            expires_at: None,
        })
        .expect("record should be created")
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

fn create_cloud_record(
    id: &str,
    version: u64,
    health: Option<RecordHealthMetadata>,
) -> CloudRecord {
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
            name: format!("Test Record {id}"),
            tags: vec!["legacy-health".to_string()],
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

#[test]
fn vault_upload_record_round_trips_health_via_encrypted_private_metadata() {
    let mut vault = setup_unlocked_vault();
    let record_id = create_login_record(&mut vault, "Encrypted Health");
    let health = make_health_state(record_id, 1, true, Some(3), false, true);
    vault
        .upsert_record_health_state(&health)
        .expect("health state should persist locally");

    let stored = vault
        .list_all_stored_records()
        .unwrap()
        .into_iter()
        .find(|record| record.id == record_id)
        .expect("stored record should exist");
    let cloud_record = vault
        .build_cloud_record_for_sync(&stored, Some(health))
        .expect("cloud record should build");

    assert!(
        cloud_record.metadata.encrypted_metadata.is_some(),
        "health must be carried inside encrypted private metadata"
    );
    assert!(
        cloud_record.metadata.health.is_none(),
        "new uploads must not expose plaintext health metadata"
    );

    vault
        .hard_delete_record(record_id)
        .expect("local record should be removable before restore");
    assert!(
        vault.get_record_health_state(&record_id).unwrap().is_none(),
        "hard delete should remove local health state"
    );

    vault
        .apply_downloaded_cloud_record(&cloud_record)
        .expect("downloaded encrypted metadata should apply");
    let restored = vault
        .get_record_health_state(&record_id)
        .unwrap()
        .expect("health state should be restored from encrypted metadata");
    assert_eq!(restored.weak_password, Some(true));
    assert_eq!(restored.duplicate_group_size, Some(3));
    assert_eq!(restored.compromised, Some(false));
    assert_eq!(restored.expired, Some(true));
}

#[tokio::test]
async fn pipeline_surfaces_legacy_plaintext_health_for_executor_result() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let record_id = Uuid::new_v4();
    let health_meta = RecordHealthMetadata {
        evaluated_at: Some("2026-04-05T12:00:00Z".to_string()),
        weak_password: Some(true),
        duplicate_group_size: Some(5),
        compromised: Some(false),
        expired: Some(true),
    };
    let cloud_record = create_cloud_record(&record_id.to_string(), 2, Some(health_meta));
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

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
    );
    context.set_local_records(vec![LocalRecordInfo {
        record_id: record_id.to_string(),
        sync_status: SyncStatus::Synced,
        version: 0,
    }]);

    let result = SyncPipeline::new().execute(&mut context).await;

    assert!(matches!(result, PipelineResult::Completed));
    assert_eq!(context.downloaded_health_states.len(), 1);
    let restored = &context.downloaded_health_states[0];
    assert_eq!(restored.record_id, record_id);
    assert_eq!(restored.weak_password, Some(true));
    assert_eq!(restored.duplicate_group_size, Some(5));
    assert_eq!(restored.expired, Some(true));
    assert!(context.downloaded_health_deleted.is_empty());
}

#[tokio::test]
async fn encrypted_private_health_download_is_applied_by_vault_not_pipeline_adapter() {
    let (storage, _temp_dir) = create_test_storage();
    let checkpoint = create_test_checkpoint();
    let mut vault = setup_unlocked_vault();
    let record_id = create_login_record(&mut vault, "Encrypted Download Health");
    let health = make_health_state(record_id, 1, false, None, true, false);
    let stored = vault
        .list_all_stored_records()
        .unwrap()
        .into_iter()
        .find(|record| record.id == record_id)
        .expect("stored record should exist");
    let cloud_record = vault
        .build_cloud_record_for_sync(&stored, Some(health))
        .expect("cloud record should build");
    let checksum = cloud_record.compute_checksum().unwrap();
    storage
        .upload_record(&record_id.to_string(), &cloud_record)
        .await
        .unwrap();
    let metadata = create_metadata(
        "test_token",
        1,
        vec![(&record_id.to_string(), 1, &checksum)],
    );
    storage.upload_metadata(&metadata).await.unwrap();

    vault
        .hard_delete_record(record_id)
        .expect("local record should be removed before download apply");

    let mut context = PipelineContext::new(
        storage,
        ConflictManager::new(),
        checkpoint,
        0,
        "test_token".to_string(),
    );
    context.set_local_records(vec![]);

    let result = SyncPipeline::new().execute(&mut context).await;

    assert!(matches!(result, PipelineResult::Completed));
    assert!(
        context.downloaded_health_states.is_empty(),
        "encrypted private health is decoded later by the vault"
    );
    assert!(
        context.downloaded_health_deleted.is_empty(),
        "encrypted private metadata must not schedule stale-health deletion"
    );

    let downloaded = context
        .downloads
        .remove(&record_id.to_string())
        .expect("record should be downloaded by pipeline");
    vault
        .apply_downloaded_cloud_record(&downloaded)
        .expect("vault should apply downloaded record and private metadata");
    let restored = vault
        .get_record_health_state(&record_id)
        .unwrap()
        .expect("health should be restored by vault apply");
    assert_eq!(restored.compromised, Some(true));
    assert_eq!(restored.weak_password, Some(false));
    assert_eq!(restored.expired, Some(false));
}
