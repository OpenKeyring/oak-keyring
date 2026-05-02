//! Cloud storage abstraction layer wrapping OpenDAL Operator.
//!
//! Provides async methods for cloud sync operations with atomic writes
//! using write-to-temp + rename pattern.

use opendal::Operator;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::cloud::metadata::{deserialize_metadata, serialize_metadata, CloudMetadata};
use crate::cloud::record::CloudRecord;
use crate::cloud::schema::{METADATA_FILENAME, RECORDS_DIR};
use crate::errors::mapping::sync::SyncError;

/// Cloud storage wrapper around OpenDAL Operator.
#[derive(Debug, Clone)]
pub struct CloudStorage {
    operator: Operator,
    provider_name: String,
}

impl CloudStorage {
    /// Creates a new CloudStorage instance.
    pub fn new(operator: Operator, provider_name: String) -> Self {
        Self {
            operator,
            provider_name,
        }
    }

    /// Uploads metadata with atomic write (temp + rename).
    pub async fn upload_metadata(&self, metadata: &CloudMetadata) -> Result<(), SyncError> {
        let json = serialize_metadata(metadata).map_err(|e| SyncError::SerializationFailed {
            message: e.to_string(),
        })?;
        let bytes: Vec<u8> = json.into_bytes();

        // Atomic write: temp file + rename
        let temp_path = format!("{}.tmp.{}", METADATA_FILENAME, Uuid::new_v4());
        self.operator
            .write(&temp_path, bytes)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, &temp_path))?;

        self.operator
            .rename(&temp_path, METADATA_FILENAME)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, METADATA_FILENAME))?;

        Ok(())
    }

    /// Downloads and deserializes metadata.
    /// Returns Ok(None) if metadata file does not exist.
    pub async fn download_metadata(&self) -> Result<Option<CloudMetadata>, SyncError> {
        match self
            .operator
            .read(METADATA_FILENAME)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, METADATA_FILENAME))
        {
            Ok(buffer) => {
                let bytes = buffer.to_vec();
                let json =
                    String::from_utf8(bytes).map_err(|e| SyncError::DeserializationFailed {
                        message: format!("invalid UTF-8: {}", e),
                    })?;
                let metadata =
                    deserialize_metadata(&json).map_err(|e| SyncError::DeserializationFailed {
                        message: e.to_string(),
                    })?;
                metadata.validate()?;
                Ok(Some(metadata))
            }
            Err(SyncError::RecordNotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Checks if metadata exists.
    pub async fn metadata_exists(&self) -> Result<bool, SyncError> {
        match self
            .operator
            .stat(METADATA_FILENAME)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, METADATA_FILENAME))
        {
            Ok(_) => Ok(true),
            Err(SyncError::RecordNotFound { .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Gets metadata version for fast-path checks.
    /// Returns Ok(None) if no metadata exists.
    pub async fn get_metadata_version(&self) -> Result<Option<u64>, SyncError> {
        match self.download_metadata().await? {
            Some(metadata) => Ok(Some(metadata.metadata_version)),
            None => Ok(None),
        }
    }

    /// Uploads metadata only if the remote metadata_version matches the expected one.
    /// This provides CAS (Compare-And-Swap) behavior for multi-device coordination.
    /// Uploads metadata only if the remote metadata_version matches the expected one.
    /// This provides CAS (Compare-And-Swap) behavior for multi-device coordination.
    ///
    /// Note: The read-check-write sequence is not atomic over the wire. Between the
    /// version check and the upload, another device could push metadata. This is an
    /// inherent limitation of cloud object storage without conditional write support.
    pub async fn push_metadata_atomic(
        &self,
        metadata: &CloudMetadata,
        expected_version: u64,
    ) -> Result<(), SyncError> {
        // 1. Download current metadata to check version
        match self.download_metadata().await? {
            Some(remote) => {
                if remote.metadata_version != expected_version {
                    return Err(SyncError::LockAcquireFailed {
                        reason: format!(
                            "metadata version mismatch: expected {}, found {}",
                            expected_version, remote.metadata_version
                        ),
                    });
                }
            }
            None => {
                // If it doesn't exist, we can only push if we expected version 0 or it's a fresh push
                if expected_version != 0 {
                    return Err(SyncError::RecordNotFound {
                        record_id: METADATA_FILENAME.to_string(),
                    });
                }
            }
        }

        // 2. Perform the upload
        self.upload_metadata(metadata).await
    }

    /// Uploads a record with atomic write (temp + rename).
    pub async fn upload_record(
        &self,
        record_id: &str,
        record: &CloudRecord,
    ) -> Result<(), SyncError> {
        let json = serde_json::to_string(record).map_err(|e| SyncError::SerializationFailed {
            message: e.to_string(),
        })?;
        let bytes: Vec<u8> = json.into_bytes();

        let temp_path = format!("{}/{}.json.tmp.{}", RECORDS_DIR, record_id, Uuid::new_v4());
        let final_path = format!("{}/{}.json", RECORDS_DIR, record_id);

        self.operator
            .write(&temp_path, bytes)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, &temp_path))?;

        self.operator
            .rename(&temp_path, &final_path)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, &final_path))?;

        Ok(())
    }

    /// Downloads and deserializes a record.
    /// Returns Ok(None) if record does not exist.
    pub async fn download_record(&self, record_id: &str) -> Result<Option<CloudRecord>, SyncError> {
        let path = format!("{}/{}.json", RECORDS_DIR, record_id);

        match self
            .operator
            .read(&path)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, &path))
        {
            Ok(buffer) => {
                let bytes = buffer.to_vec();
                let record: CloudRecord = serde_json::from_slice(&bytes).map_err(|e| {
                    SyncError::DeserializationFailed {
                        message: format!("record {}: {}", record_id, e),
                    }
                })?;
                Ok(Some(record))
            }
            Err(SyncError::RecordNotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Lists all record IDs in the records directory.
    pub async fn list_records(&self) -> Result<Vec<String>, SyncError> {
        // Ensure records directory path has trailing slash for listing
        let records_path = format!("{}/", RECORDS_DIR);

        let entries = self
            .operator
            .list(&records_path)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, &records_path))?;

        let mut record_ids = Vec::new();
        for entry in entries {
            let path = entry.path();
            // Extract record ID from path like "records/{uuid}.json"
            if let Some(id) = extract_record_id(path) {
                record_ids.push(id);
            }
        }

        Ok(record_ids)
    }

    /// Downloads multiple records concurrently.
    /// Individual failures do not affect other downloads.
    pub async fn batch_download_records(
        &self,
        record_ids: &[String],
    ) -> Vec<(String, Result<Option<CloudRecord>, SyncError>)> {
        let mut join_set = JoinSet::new();

        for record_id in record_ids {
            let operator = self.operator.clone();
            let provider_name = self.provider_name.clone();
            let record_id_clone = record_id.clone();

            join_set.spawn(async move {
                let path = format!("{}/{}.json", RECORDS_DIR, record_id_clone);
                let record_id_for_error = record_id_clone.clone();

                match operator
                    .read(&path)
                    .await
                    .map_err(|e| map_opendal_error(e, &provider_name, &path))
                {
                    Ok(buffer) => {
                        let bytes = buffer.to_vec();
                        match serde_json::from_slice::<CloudRecord>(&bytes) {
                            Ok(record) => (record_id_clone, Ok(Some(record))),
                            Err(e) => (
                                record_id_clone,
                                Err(SyncError::DeserializationFailed {
                                    message: format!("record {}: {}", record_id_for_error, e),
                                }),
                            ),
                        }
                    }
                    Err(SyncError::RecordNotFound { .. }) => (record_id_clone, Ok(None)),
                    Err(e) => (record_id_clone, Err(e)),
                }
            });
        }

        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok((record_id, result)) => results.push((record_id, result)),
                Err(e) => {
                    results.push((
                        String::new(),
                        Err(SyncError::ProviderError {
                            provider: String::new(),
                            message: format!("join error: {}", e),
                        }),
                    ));
                }
            }
        }

        // Sort by original order of record_ids
        let mut ordered_results = Vec::with_capacity(record_ids.len());
        for record_id in record_ids {
            if let Some(pos) = results.iter().position(|(id, _)| id == record_id) {
                ordered_results.push(results.remove(pos));
            } else {
                ordered_results.push((
                    record_id.clone(),
                    Err(SyncError::RecordNotFound {
                        record_id: record_id.clone(),
                    }),
                ));
            }
        }

        ordered_results
    }

    /// Deletes a record. Idempotent - deleting non-existent record returns Ok.
    pub async fn delete_record(&self, record_id: &str) -> Result<(), SyncError> {
        let path = format!("{}/{}.json", RECORDS_DIR, record_id);

        match self
            .operator
            .delete(&path)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, &path))
        {
            Ok(()) => Ok(()),
            Err(SyncError::RecordNotFound { .. }) => Ok(()), // Idempotent
            Err(e) => Err(e),
        }
    }

    /// Checks connectivity by attempting to list records.
    pub async fn check_connectivity(&self) -> Result<(), SyncError> {
        // Try to list records directory as a connectivity check
        let records_path = format!("{}/", RECORDS_DIR);

        self.operator
            .list(&records_path)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, "connectivity check"))?;

        Ok(())
    }

    /// Low-level raw write for lock files and other arbitrary data.
    pub async fn upload_raw(&self, path: &str, data: &[u8]) -> Result<(), SyncError> {
        let bytes: Vec<u8> = data.to_vec();
        self.operator
            .write(path, bytes)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, path))?;
        Ok(())
    }

    /// Low-level raw read for lock files and other arbitrary data.
    /// Returns Ok(None) if file does not exist.
    pub async fn download_raw(&self, path: &str) -> Result<Option<Vec<u8>>, SyncError> {
        match self
            .operator
            .read(path)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, path))
        {
            Ok(buffer) => Ok(Some(buffer.to_vec())),
            Err(SyncError::RecordNotFound { .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Low-level raw delete. Idempotent - returns Ok even if file doesn't exist.
    pub async fn delete_raw(&self, path: &str) -> Result<(), SyncError> {
        match self
            .operator
            .delete(path)
            .await
            .map_err(|e| map_opendal_error(e, &self.provider_name, path))
        {
            Ok(()) => Ok(()),
            Err(SyncError::RecordNotFound { .. }) => Ok(()), // Idempotent
            Err(e) => Err(e),
        }
    }
}

/// Maps OpenDAL errors to SyncError variants.
fn map_opendal_error(err: opendal::Error, provider: &str, context: &str) -> SyncError {
    match err.kind() {
        opendal::ErrorKind::NotFound => SyncError::RecordNotFound {
            record_id: context.to_string(),
        },
        opendal::ErrorKind::PermissionDenied => SyncError::PermissionDenied {
            path: context.to_string(),
        },
        opendal::ErrorKind::RateLimited => SyncError::NetworkTimeout {
            message: context.to_string(),
        },
        _ => SyncError::ProviderError {
            provider: provider.to_string(),
            message: format!("{}: {}", context, err),
        },
    }
}

/// Extracts record ID from a path like "records/{uuid}.json".
fn extract_record_id(path: &str) -> Option<String> {
    // Path format: "records/{id}.json"
    let path = path.trim_end_matches('/');
    let prefix = format!("{}/", RECORDS_DIR);
    if !path.starts_with(&prefix) {
        return None;
    }

    let filename = &path[prefix.len()..];
    if !filename.ends_with(".json") {
        return None;
    }

    let id = &filename[..filename.len() - 5]; // Remove ".json" suffix
    if id.is_empty() {
        return None;
    }

    Some(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cloud::metadata::DeviceInfo;
    use crate::cloud::record::{AadFields, RecordMetadata};
    use chrono::Utc;
    use tempfile::TempDir;

    fn test_storage_memory() -> CloudStorage {
        let op = Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        CloudStorage::new(op, "memory".to_string())
    }

    fn test_storage_fs(temp_dir: &TempDir) -> CloudStorage {
        let op =
            Operator::new(opendal::services::Fs::default().root(temp_dir.path().to_str().unwrap()))
                .unwrap()
                .finish();
        CloudStorage::new(op, "fs".to_string())
    }

    fn create_test_metadata() -> CloudMetadata {
        let mut metadata = CloudMetadata::new("test_token_abc123".to_string());
        metadata.add_device(DeviceInfo {
            device_id: "device-1".to_string(),
            platform: "macos".to_string(),
            device_name: "MacBook Pro".to_string(),
            last_seen: Utc::now().to_rfc3339(),
            sync_count: 1,
        });
        metadata
    }

    fn create_test_record(id: &str) -> CloudRecord {
        CloudRecord {
            id: id.to_string(),
            version: 1,
            encrypted_data: "dGVzdCBkYXRh".to_string(),
            nonce: "bm9uY2U".to_string(),
            dek_version: 1,
            aad: AadFields {
                record_id: id.to_string(),
                dek_version: 1,
            },
            metadata: RecordMetadata {
                name: "Test Record".to_string(),
                tags: vec!["test".to_string()],
                updated_at: Utc::now().to_rfc3339(),
                health: None,
            },
            deleted: None,
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn test_upload_download_metadata_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let metadata = create_test_metadata();

        assert!(!storage.metadata_exists().await.unwrap());
        storage.upload_metadata(&metadata).await.unwrap();
        assert!(storage.metadata_exists().await.unwrap());

        let downloaded = storage.download_metadata().await.unwrap().unwrap();
        assert_eq!(
            downloaded.vault_identity_token,
            metadata.vault_identity_token
        );
        assert_eq!(downloaded.metadata_version, metadata.metadata_version);
        assert_eq!(downloaded.devices.len(), metadata.devices.len());
    }

    #[tokio::test]
    async fn test_metadata_exists_returns_false_initially() {
        let storage = test_storage_memory();
        assert!(!storage.metadata_exists().await.unwrap());
    }

    #[tokio::test]
    async fn test_metadata_exists_returns_true_after_upload() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let metadata = create_test_metadata();

        assert!(!storage.metadata_exists().await.unwrap());
        storage.upload_metadata(&metadata).await.unwrap();
        assert!(storage.metadata_exists().await.unwrap());
    }

    #[tokio::test]
    async fn test_get_metadata_version_returns_none_initially() {
        let storage = test_storage_memory();
        assert_eq!(storage.get_metadata_version().await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_get_metadata_version_returns_some_after_upload() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let metadata = create_test_metadata();

        assert_eq!(storage.get_metadata_version().await.unwrap(), None);
        storage.upload_metadata(&metadata).await.unwrap();
        assert_eq!(storage.get_metadata_version().await.unwrap(), Some(1));
    }

    #[tokio::test]
    async fn test_upload_download_record_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let record = create_test_record("550e8400-e29b-41d4-a716-446655440000");

        let result = storage.download_record(record.id.as_str()).await.unwrap();
        assert!(result.is_none());

        storage
            .upload_record(record.id.as_str(), &record)
            .await
            .unwrap();

        let downloaded = storage
            .download_record(record.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(downloaded.id, record.id);
        assert_eq!(downloaded.version, record.version);
        assert_eq!(downloaded.encrypted_data, record.encrypted_data);
        assert_eq!(downloaded.dek_version, record.dek_version);
    }

    #[tokio::test]
    async fn test_list_records_returns_empty_initially() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let records = storage.list_records().await.unwrap();
        assert!(records.is_empty());
    }

    #[tokio::test]
    async fn test_list_records_returns_after_uploads() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);

        let record1 = create_test_record("550e8400-e29b-41d4-a716-446655440001");
        let record2 = create_test_record("550e8400-e29b-41d4-a716-446655440002");

        storage
            .upload_record(record1.id.as_str(), &record1)
            .await
            .unwrap();
        storage
            .upload_record(record2.id.as_str(), &record2)
            .await
            .unwrap();

        let records = storage.list_records().await.unwrap();
        assert_eq!(records.len(), 2);
        assert!(records.contains(&"550e8400-e29b-41d4-a716-446655440001".to_string()));
        assert!(records.contains(&"550e8400-e29b-41d4-a716-446655440002".to_string()));
    }

    #[tokio::test]
    async fn test_batch_download_records_concurrent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);

        let record1 = create_test_record("550e8400-e29b-41d4-a716-446655440001");
        let record2 = create_test_record("550e8400-e29b-41d4-a716-446655440002");
        let _record3 = create_test_record("550e8400-e29b-41d4-a716-446655440003");

        storage
            .upload_record(record1.id.as_str(), &record1)
            .await
            .unwrap();
        storage
            .upload_record(record2.id.as_str(), &record2)
            .await
            .unwrap();

        let record_ids = vec![
            "550e8400-e29b-41d4-a716-446655440001".to_string(),
            "550e8400-e29b-41d4-a716-446655440002".to_string(),
            "550e8400-e29b-41d4-a716-446655440003".to_string(),
        ];

        let results = storage.batch_download_records(&record_ids).await;

        assert_eq!(results.len(), 3);
        assert!(results[0].1.as_ref().unwrap().is_some());
        assert!(results[1].1.as_ref().unwrap().is_some());
        assert!(results[2].1.as_ref().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_record_is_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let record = create_test_record("550e8400-e29b-41d4-a716-446655440000");

        storage.delete_record(record.id.as_str()).await.unwrap();

        storage
            .upload_record(record.id.as_str(), &record)
            .await
            .unwrap();
        storage.delete_record(record.id.as_str()).await.unwrap();

        storage.delete_record(record.id.as_str()).await.unwrap();
    }

    #[tokio::test]
    async fn test_upload_raw_download_raw_roundtrip() {
        let storage = test_storage_memory();
        let path = "test_file.bin";
        let data = b"test binary data with some extra bytes here and there";

        let result = storage.download_raw(path).await.unwrap();
        assert!(result.is_none());

        storage.upload_raw(path, data).await.unwrap();

        let downloaded = storage.download_raw(path).await.unwrap().unwrap();
        assert_eq!(downloaded, data);
    }

    #[tokio::test]
    async fn test_delete_raw_is_idempotent() {
        let storage = test_storage_memory();
        let path = "test_file.bin";
        let data = b"test data";

        storage.delete_raw(path).await.unwrap();

        storage.upload_raw(path, data).await.unwrap();
        storage.delete_raw(path).await.unwrap();

        storage.delete_raw(path).await.unwrap();
    }

    #[tokio::test]
    async fn test_check_connectivity_succeeds() {
        let storage = test_storage_memory();
        storage.check_connectivity().await.unwrap();
    }

    #[tokio::test]
    async fn test_atomic_write_temp_file_not_retained() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let metadata = create_test_metadata();

        storage.upload_metadata(&metadata).await.unwrap();

        let entries = storage
            .operator
            .list("")
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.path().to_string())
            .collect::<Vec<_>>();

        for path in &entries {
            assert!(
                !path.contains(".tmp."),
                "Temp file {} should not exist",
                path
            );
        }

        assert!(entries.iter().any(|p| p == METADATA_FILENAME));
    }

    #[tokio::test]
    async fn test_extract_record_id_valid() {
        assert_eq!(
            extract_record_id("records/550e8400-e29b-41d4-a716-446655440000.json"),
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[tokio::test]
    async fn test_extract_record_id_with_trailing_slash() {
        assert_eq!(
            extract_record_id("records/550e8400-e29b-41d4-a716-446655440000.json/"),
            Some("550e8400-e29b-41d4-a716-446655440000".to_string())
        );
    }

    #[tokio::test]
    async fn test_extract_record_id_invalid_no_json() {
        assert_eq!(extract_record_id("records/somefile.txt"), None);
    }

    #[tokio::test]
    async fn test_extract_record_id_invalid_wrong_prefix() {
        assert_eq!(
            extract_record_id("other/550e8400-e29b-41d4-a716-446655440000.json"),
            None
        );
    }

    #[tokio::test]
    async fn test_extract_record_id_empty_id() {
        assert_eq!(extract_record_id("records/.json"), None);
    }

    // ── push_metadata_atomic tests ───────────────────────────────────

    #[tokio::test]
    async fn test_push_metadata_atomic_succeeds_when_version_matches() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let mut metadata = create_test_metadata();

        // Initial upload — metadata_version starts at 1
        storage.upload_metadata(&metadata).await.unwrap();
        let expected_version = metadata.metadata_version;

        // CAS push with matching version should succeed
        metadata.metadata_version += 1;
        storage
            .push_metadata_atomic(&metadata, expected_version)
            .await
            .unwrap();

        // Verify the push took effect
        let downloaded = storage.download_metadata().await.unwrap().unwrap();
        assert_eq!(downloaded.metadata_version, expected_version + 1);
    }

    #[tokio::test]
    async fn test_push_metadata_atomic_fails_on_version_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let metadata = create_test_metadata();

        storage.upload_metadata(&metadata).await.unwrap();

        // CAS push with wrong version should fail
        let result = storage.push_metadata_atomic(&metadata, 999).await;
        assert!(result.is_err(), "CAS push with wrong version should fail");
    }

    #[tokio::test]
    async fn test_push_metadata_atomic_succeeds_when_no_remote_exists() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let metadata = create_test_metadata();

        // No remote metadata — push with expected_version=0 should succeed
        storage.push_metadata_atomic(&metadata, 0).await.unwrap();

        let downloaded = storage.download_metadata().await.unwrap().unwrap();
        assert_eq!(downloaded.metadata_version, metadata.metadata_version);
    }

    #[tokio::test]
    async fn test_push_metadata_atomic_fails_when_no_remote_and_nonzero_version() {
        let temp_dir = TempDir::new().unwrap();
        let storage = test_storage_fs(&temp_dir);
        let metadata = create_test_metadata();

        // No remote metadata but expected_version=5 should fail
        let result = storage.push_metadata_atomic(&metadata, 5).await;
        assert!(
            result.is_err(),
            "CAS push with nonzero version but no remote should fail"
        );
    }
}
