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
