use chrono::{Duration, Utc};
use oak_keyring::cloud::CloudRecord;
use oak_keyring::crypto::bip39::{MnemonicLanguage, Passkey};
use oak_keyring::services::vault::VaultService;
use oak_keyring::types::credential::{CredentialType, EncryptedPayload};
use oak_keyring::types::record::CreateRecordParams;
use oak_keyring::types::sensitive::SecureStr;
use uuid::Uuid;

fn setup_unlocked_vault() -> VaultService {
    let conn = oak_keyring::db::schema::init_db_in_memory().unwrap();
    let mut vault = VaultService::new(conn);
    let mnemonic = Passkey::generate(24, MnemonicLanguage::English).unwrap();
    vault
        .unlock_with_mnemonic(&mnemonic)
        .expect("test vault must unlock");
    vault
}

fn create_login_record(vault: &mut VaultService, name: &str, tags: Vec<String>) -> Uuid {
    vault
        .create_record(CreateRecordParams {
            credential_type: CredentialType::Login,
            payload: EncryptedPayload::Login {
                name: name.to_string(),
                username: "sync@example.com".to_string(),
                password: SecureStr::new(Uuid::new_v4().to_string()),
                url: Some("https://example.com".to_string()),
                notes: None,
                totp: None,
            },
            tags,
            is_favorite: false,
            expires_at: None,
        })
        .expect("record should be created")
}

fn sorted(mut tags: Vec<String>) -> Vec<String> {
    tags.sort();
    tags
}

#[test]
fn cloud_record_json_round_trips_encrypted_private_metadata_for_existing_record() {
    let mut vault = setup_unlocked_vault();
    let record_id = create_login_record(
        &mut vault,
        "Encrypted Metadata Source",
        vec!["local-only".to_string()],
    );
    let mut remote = vault
        .get_stored_record(record_id)
        .expect("record should exist");
    remote.tags = vec!["remote-sync".to_string(), "work".to_string()];
    remote.is_favorite = true;
    remote.expires_at = Some(Utc::now() + Duration::days(30));
    remote.updated_at = Utc::now() + Duration::seconds(5);
    remote.updated_by = "remote-device".to_string();
    remote.version += 1;

    let cloud_record = vault
        .build_cloud_record_for_sync(&remote, None)
        .expect("cloud record should build");
    let cloud_json =
        serde_json::to_string_pretty(&cloud_record).expect("cloud record json should serialize");
    let cloud_value: serde_json::Value =
        serde_json::from_str(&cloud_json).expect("cloud record json should parse");

    assert_eq!(cloud_value["metadata"]["name"], "encrypted");
    assert_eq!(
        cloud_value["metadata"]["tags"]
            .as_array()
            .expect("metadata.tags should be an array")
            .len(),
        0,
        "plaintext cloud metadata must not expose private tags"
    );
    assert!(
        cloud_value["metadata"]["encrypted_metadata"]["encrypted_data"].is_string(),
        "private metadata should be encrypted in the JSON payload"
    );
    assert!(
        !cloud_json.contains("Encrypted Metadata Source"),
        "record name must not appear as plaintext in cloud JSON"
    );
    assert!(
        !cloud_json.contains("remote-sync"),
        "tags must not appear as plaintext in cloud JSON"
    );

    let downloaded: CloudRecord =
        serde_json::from_str(&cloud_json).expect("cloud record json should deserialize");
    let is_new = vault
        .apply_downloaded_cloud_record(&downloaded)
        .expect("downloaded cloud record should apply");
    assert!(!is_new, "record already exists locally");

    let restored = vault
        .get_stored_record(record_id)
        .expect("record should still exist");
    assert_eq!(sorted(restored.tags), vec!["remote-sync", "work"]);
    assert!(restored.is_favorite);
    assert_eq!(restored.updated_by, "remote-device");
    assert_eq!(restored.version, remote.version);
}

#[test]
fn downloaded_existing_cloud_record_applies_remote_deleted_state() {
    let mut vault = setup_unlocked_vault();
    let record_id = create_login_record(&mut vault, "Remote Deleted", vec!["keep".to_string()]);
    let mut remote = vault
        .get_stored_record(record_id)
        .expect("record should exist");
    remote.version += 1;
    remote.updated_at = Utc::now() + Duration::seconds(5);
    remote.deleted = true;
    remote.deleted_at = Some(Utc::now());

    let cloud_record = vault
        .build_cloud_record_for_sync(&remote, None)
        .expect("cloud record should build");
    let is_new = vault
        .apply_downloaded_cloud_record(&cloud_record)
        .expect("downloaded cloud record should apply");
    assert!(!is_new, "record already exists locally");

    let restored = vault
        .get_stored_record(record_id)
        .expect("record should still exist");
    assert!(restored.deleted, "remote deleted flag should be applied");
    assert!(
        restored.deleted_at.is_some(),
        "remote deleted_at should be applied"
    );
    assert!(
        vault
            .list_trash()
            .expect("trash records should load")
            .iter()
            .any(|record| record.id == record_id),
        "deleted records must appear in trash after sync apply"
    );
}
