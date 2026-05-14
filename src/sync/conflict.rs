//! ConflictManager for sync conflict detection, storage, and resolution.
//!
//! This module provides stateless conflict management operations for the sync pipeline.
//! All methods operate on data passed as parameters - no persistent state is stored.

use uuid::Uuid;

use crate::cloud::record::{CloudRecord, ConflictPayload};
use crate::errors::mapping::sync::SyncError;
use crate::types::sync::SyncStatus;

/// Result of conflict detection between local and remote versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictAction {
    /// Local has pending changes, remote has newer version → conflict
    Conflict {
        local_version: u64,
        remote_version: u64,
    },
    /// Remote has newer version, local is synced → download only
    DownloadOnly,
    /// Local has pending changes, remote is same version → upload only
    UploadOnly,
    /// Both sides are in sync → no action needed
    NoAction,
}

/// Resolution action to take after resolving a conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolutionAction {
    /// Upload local data to cloud (version+1, overwrite remote)
    KeepLocal,
    /// Overwrite local data with remote data from conflict_data
    KeepRemote,
}

/// Result of resolving a conflict.
#[derive(Debug, Clone)]
pub struct ResolvedConflict {
    pub new_version: u64,
    pub action: ResolutionAction,
}

/// Data extracted from conflict_data when keeping remote.
#[derive(Debug, Clone)]
pub struct KeepRemoteData {
    /// The deserialized cloud record from conflict_data
    pub cloud_record: CloudRecord,
    /// Fields to write back to local records table
    pub encrypted_data_base64: String,
    pub nonce_base64: String,
    pub dek_version: u32,
    pub aad_json: String,
    pub version: u64,
    pub updated_at: String,
}

/// Individual conflict item for batch resolution.
#[derive(Debug, Clone)]
pub struct ConflictItem {
    pub record_id: Uuid,
    pub conflict_data: Vec<u8>,
    pub current_version: u64,
}

/// Strategy for batch conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionStrategy {
    KeepLocal,
    KeepRemote,
}

/// Outcome of resolving a single conflict in a batch.
#[derive(Debug)]
pub struct ResolveOutcome {
    pub record_id: Uuid,
    pub result: Result<ResolvedConflict, SyncError>,
}

/// Stateless ConflictManager for sync conflict operations.
///
/// All methods take the data they need as parameters - no persistent state.
/// This makes it easy to test and use from the sync pipeline.
#[derive(Debug, Clone)]
pub struct ConflictManager;

impl ConflictManager {
    /// Creates a new ConflictManager instance.
    pub fn new() -> Self {
        Self
    }

    /// Detects conflicts between local and remote versions.
    ///
    /// # Detection Rules
    /// - `local_sync_status == Pending AND remote_version > local_version` → `ConflictAction::Conflict`
    /// - `local_sync_status == Synced AND remote_version > local_version` → `ConflictAction::DownloadOnly`
    /// - `local_sync_status == Pending AND remote_version == local_version` → `ConflictAction::UploadOnly`
    /// - All other cases → `ConflictAction::NoAction`
    pub fn detect_conflicts(
        &self,
        local_sync_status: SyncStatus,
        local_version: u64,
        remote_version: u64,
    ) -> ConflictAction {
        match (local_sync_status, remote_version.cmp(&local_version)) {
            // Local has pending changes, remote has newer version → conflict
            (SyncStatus::Pending, std::cmp::Ordering::Greater) => ConflictAction::Conflict {
                local_version,
                remote_version,
            },
            // Remote has newer version, local is synced → download only
            (SyncStatus::Synced, std::cmp::Ordering::Greater) => ConflictAction::DownloadOnly,
            // Local has pending changes, remote is same version → upload only
            (SyncStatus::Pending, std::cmp::Ordering::Equal) => ConflictAction::UploadOnly,
            // All other cases → no action
            _ => ConflictAction::NoAction,
        }
    }

    /// Stores a conflict by creating a ConflictPayload and serializing it.
    ///
    /// This creates a ConflictPayload from the cloud record and checksum,
    /// validates it, and serializes to bytes. The bytes are what would be
    /// stored in `sync_state.conflict_data`.
    ///
    /// If serialization fails, logs a warning and returns the error.
    /// The caller should handle this by storing `NULL` in conflict_data
    /// and still marking the record as Conflict status.
    pub fn store_conflict(
        &self,
        cloud_record: &CloudRecord,
        checksum: &str,
    ) -> Result<Vec<u8>, SyncError> {
        let payload = ConflictPayload {
            cloud_record: cloud_record.clone(),
            checksum: checksum.to_string(),
        };

        // Validate before serializing
        if let Err(e) = payload.validate() {
            tracing::warn!("invalid conflict payload: {}", e);
            return Err(e);
        }

        payload.serialize().map_err(|e| {
            tracing::warn!("failed to serialize conflict payload: {}", e);
            e
        })
    }

    /// Resolves a conflict by keeping the local version.
    ///
    /// For KeepLocal: new_version = current_version + 1, action = ResolutionAction::KeepLocal.
    ///
    /// KeepLocal does NOT read conflict_data. It just bumps the version and returns the action.
    /// The caller is responsible for: uploading to cloud, clearing conflict_data,
    /// setting sync_status=Synced.
    pub fn resolve_keep_local(&self, current_version: u64) -> ResolvedConflict {
        ResolvedConflict {
            new_version: current_version + 1,
            action: ResolutionAction::KeepLocal,
        }
    }

    /// Resolves a conflict by keeping the remote version.
    ///
    /// For KeepRemote:
    /// 1. Deserialize conflict_data bytes as ConflictPayload
    /// 2. Validate the ConflictPayload (checksum + AAD checks)
    /// 3. Extract the fields needed to overwrite local records table
    /// 4. Return KeepRemoteData with all the fields
    pub fn resolve_keep_remote(&self, conflict_data: &[u8]) -> Result<KeepRemoteData, SyncError> {
        // Deserialize conflict_data bytes as ConflictPayload
        let payload = ConflictPayload::deserialize(conflict_data)?;

        // Validate the ConflictPayload (checksum + AAD checks)
        payload.validate()?;

        let cloud_record = payload.cloud_record;

        // Extract the fields needed to overwrite local records table
        let aad_json = serde_json::to_string(&cloud_record.aad).map_err(|e| {
            SyncError::SerializationFailed {
                message: format!("failed to serialize AAD: {}", e),
            }
        })?;

        Ok(KeepRemoteData {
            cloud_record: cloud_record.clone(),
            encrypted_data_base64: cloud_record.encrypted_data.clone(),
            nonce_base64: cloud_record.nonce.clone(),
            dek_version: cloud_record.dek_version,
            aad_json,
            version: cloud_record.version,
            updated_at: cloud_record.metadata.updated_at.clone(),
        })
    }

    /// Resolves multiple conflicts using the same strategy.
    ///
    /// Batch resolution iterates over items, applies the same strategy to each,
    /// and collects results. Individual failures do NOT stop the batch.
    /// Each item is independent. Returns all outcomes so the caller can see
    /// which succeeded and which failed.
    pub fn resolve_all_batch(
        &self,
        items: &[ConflictItem],
        strategy: ResolutionStrategy,
    ) -> Vec<ResolveOutcome> {
        items
            .iter()
            .map(|item| {
                let result = match strategy {
                    ResolutionStrategy::KeepLocal => {
                        Ok(self.resolve_keep_local(item.current_version))
                    }
                    ResolutionStrategy::KeepRemote => self
                        .resolve_keep_remote(&item.conflict_data)
                        .map(|keep_remote| ResolvedConflict {
                            new_version: keep_remote.version,
                            action: ResolutionAction::KeepRemote,
                        }),
                };
                ResolveOutcome {
                    record_id: item.record_id,
                    result,
                }
            })
            .collect()
    }
}

impl Default for ConflictManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_valid_cloud_record() -> CloudRecord {
        CloudRecord {
            id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
            version: 5,
            encrypted_data: "dGVzdCBkYXRh".to_string(),
            nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            dek_version: 1,
            aad: crate::cloud::record::AadFields {
                record_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
                dek_version: 1,
            },
            metadata: crate::cloud::record::RecordMetadata {
                name: "GitHub".to_string(),
                tags: vec!["dev".to_string()],
                updated_at: "2026-04-05T12:00:00Z".to_string(),
                health: None,
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        }
    }

    #[test]
    fn detect_conflict_both_modified() {
        let manager = ConflictManager::new();
        // Pending + remote > local → Conflict
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
        // Synced + remote > local → DownloadOnly
        let result = manager.detect_conflicts(SyncStatus::Synced, 5, 6);
        assert!(matches!(result, ConflictAction::DownloadOnly));
    }

    #[test]
    fn detect_conflict_local_only() {
        let manager = ConflictManager::new();
        // Pending + remote == local → UploadOnly
        let result = manager.detect_conflicts(SyncStatus::Pending, 5, 5);
        assert!(matches!(result, ConflictAction::UploadOnly));
    }

    #[test]
    fn detect_conflict_no_action() {
        let manager = ConflictManager::new();
        // Synced + remote == local → NoAction
        let result = manager.detect_conflicts(SyncStatus::Synced, 5, 5);
        assert!(matches!(result, ConflictAction::NoAction));
    }

    #[test]
    fn detect_conflict_synced_remote_lower() {
        let manager = ConflictManager::new();
        // Synced + remote < local → NoAction (shouldn't happen in practice)
        let result = manager.detect_conflicts(SyncStatus::Synced, 6, 5);
        assert!(matches!(result, ConflictAction::NoAction));
    }

    #[test]
    fn detect_conflict_pending_remote_lower() {
        let manager = ConflictManager::new();
        // Pending + remote < local → NoAction (local is ahead, will upload)
        let result = manager.detect_conflicts(SyncStatus::Pending, 6, 5);
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
        // Verify it can be deserialized back
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
            SyncError::ChecksumMismatch { .. }
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
            SyncError::DeserializationFailed { .. }
        ));
    }

    #[test]
    fn resolve_keep_remote_checksum_mismatch() {
        let manager = ConflictManager::new();
        let record = create_valid_cloud_record();
        // Create payload with wrong checksum
        let payload = ConflictPayload {
            cloud_record: record,
            checksum: "wrong_checksum".to_string(),
        };
        let conflict_data = payload.serialize().unwrap();

        let result = manager.resolve_keep_remote(&conflict_data);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            SyncError::ChecksumMismatch { .. }
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

        // First should succeed
        assert!(outcomes[0].result.is_ok());
        assert_eq!(outcomes[0].record_id, id1);

        // Second should fail (corrupted data)
        assert!(outcomes[1].result.is_err());
        assert_eq!(outcomes[1].record_id, id2);

        // Third should succeed
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

    #[test]
    fn conflict_manager_default() {
        let manager = ConflictManager::default();
        // Should work the same as new()
        let result = manager.detect_conflicts(SyncStatus::Synced, 1, 2);
        assert!(matches!(result, ConflictAction::DownloadOnly));
    }
}
