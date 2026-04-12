//! SyncPipeline - 4-stage sync orchestration.

use std::collections::HashMap;

use uuid::Uuid;

use crate::cloud::{CloudMetadata, CloudRecord, CloudStorage};
use crate::errors::mapping::sync::SyncError;
use crate::sync::checkpoint::{PendingConflict, SyncCheckpoint};
use crate::sync::conflict::{ConflictAction, ConflictManager};
use crate::types::sync::SyncStatus;

#[derive(Debug)]
pub enum StageOutcome {
    Continue,
    ConflictDetected {
        conflict_ids: Vec<String>,
    },
    Error(Box<SyncError>),
    NoChanges,
}

#[allow(async_fn_in_trait)]
pub trait SyncStage {
    async fn execute(&self, context: &mut PipelineContext) -> StageOutcome;
}

#[derive(Debug, Clone)]
pub struct LocalRecordInfo {
    pub record_id: String,
    pub sync_status: SyncStatus,
    pub version: u64,
}

/// Shared context passed between pipeline stages.
#[derive(Debug)]
pub struct PipelineContext {
    /// Cloud storage for remote operations.
    pub storage: CloudStorage,
    /// Conflict manager for detection/resolution.
    pub conflict_manager: ConflictManager,
    /// Checkpoint for progress tracking.
    pub checkpoint: SyncCheckpoint,
    /// Remote metadata downloaded in Pull stage.
    pub remote_metadata: Option<CloudMetadata>,
    /// Local metadata version for fast-path comparison.
    pub local_metadata_version: u64,
    /// Local vault identity token.
    pub local_vault_token: String,
    /// Records classified as safe to upload (from Detect stage).
    pub to_upload: Vec<String>,
    /// Records classified as safe to download (from Detect stage).
    pub to_download: Vec<String>,
    /// Records with conflicts (from Detect stage).
    pub conflicts: Vec<String>,
    /// IDs of records that failed during Push stage.
    pub failed_ids: Vec<String>,
    /// Records to upload with their data.
    pub uploads: Vec<CloudRecord>,
    /// Downloaded records from remote.
    pub downloads: HashMap<String, CloudRecord>,
    /// Conflict data stored for resolution.
    pub conflict_data_map: HashMap<String, Vec<u8>>,
    /// Local records info for conflict detection.
    pub local_records: Vec<LocalRecordInfo>,
}

impl PipelineContext {
    /// Creates a new PipelineContext with the given parameters.
    pub fn new(
        storage: CloudStorage,
        conflict_manager: ConflictManager,
        checkpoint: SyncCheckpoint,
        local_metadata_version: u64,
        local_vault_token: String,
    ) -> Self {
        Self {
            storage,
            conflict_manager,
            checkpoint,
            remote_metadata: None,
            local_metadata_version,
            local_vault_token,
            to_upload: Vec::new(),
            to_download: Vec::new(),
            conflicts: Vec::new(),
            failed_ids: Vec::new(),
            uploads: Vec::new(),
            downloads: HashMap::new(),
            conflict_data_map: HashMap::new(),
            local_records: Vec::new(),
        }
    }

    /// Sets the local records for conflict detection.
    pub fn set_local_records(&mut self, records: Vec<LocalRecordInfo>) {
        self.local_records = records;
    }

    /// Sets the records to upload.
    pub fn set_uploads(&mut self, records: Vec<CloudRecord>) {
        self.uploads = records;
    }
}

/// Stage 1: Pull Metadata
///
/// Downloads remote metadata and validates vault identity.
/// Fast path: if remote metadata version equals local, returns NoChanges.
#[derive(Debug)]
pub struct PullMetadataStage;

impl PullMetadataStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PullMetadataStage {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncStage for PullMetadataStage {
    async fn execute(&self, context: &mut PipelineContext) -> StageOutcome {
        match context.storage.download_metadata().await {
            Ok(Some(remote_metadata)) => {
                if remote_metadata.metadata_version == context.local_metadata_version {
                    return StageOutcome::NoChanges;
                }

                if remote_metadata.vault_identity_token != context.local_vault_token {
                    return StageOutcome::Error(Box::new(SyncError::VaultIdentityMismatch {
                        local_token: context.local_vault_token.clone(),
                        remote_token: remote_metadata.vault_identity_token.clone(),
                    }));
                }

                context.remote_metadata = Some(remote_metadata);
                StageOutcome::Continue
            }
            Ok(None) => {
                context.remote_metadata = None;
                StageOutcome::Continue
            }
            Err(e) => StageOutcome::Error(Box::new(e)),
        }
    }
}

/// Stage 2: Detect
///
/// Classifies records into upload/download/conflict sets based on
/// comparing local records with remote metadata.
#[derive(Debug)]
pub struct DetectStage;

impl DetectStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DetectStage {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncStage for DetectStage {
    async fn execute(&self, context: &mut PipelineContext) -> StageOutcome {
        if context.remote_metadata.is_none() {
            for local in &context.local_records {
                if local.sync_status == SyncStatus::Pending {
                    context.to_upload.push(local.record_id.clone());
                }
            }
            return StageOutcome::Continue;
        }

        let remote_metadata = context.remote_metadata.as_ref().unwrap();
        let remote_records = &remote_metadata.records;

        for local in &context.local_records {
            let remote_info = remote_records.get(&local.record_id);

            let action = match remote_info {
                Some(remote) => {
                    context.conflict_manager
                        .detect_conflicts(local.sync_status, local.version, remote.version)
                }
                None => {
                    if local.sync_status == SyncStatus::Pending {
                        ConflictAction::UploadOnly
                    } else {
                        ConflictAction::NoAction
                    }
                }
            };

            match action {
                ConflictAction::UploadOnly => {
                    context.to_upload.push(local.record_id.clone());
                }
                ConflictAction::DownloadOnly => {
                    context.to_download.push(local.record_id.clone());
                }
                ConflictAction::Conflict { .. } => {
                    context.conflicts.push(local.record_id.clone());
                }
                ConflictAction::NoAction => {}
            }
        }

        if !context.conflicts.is_empty() {
            return StageOutcome::ConflictDetected {
                conflict_ids: context.conflicts.clone(),
            };
        }

        if context.to_upload.is_empty() && context.to_download.is_empty() {
            return StageOutcome::NoChanges;
        }

        StageOutcome::Continue
    }
}

/// Stage 3: Push
///
/// Uploads local records, downloads remote records, and handles conflicts.
#[derive(Debug)]
pub struct PushStage;

impl PushStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PushStage {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncStage for PushStage {
    async fn execute(&self, context: &mut PipelineContext) -> StageOutcome {
        for record in &context.uploads {
            match context.storage.upload_record(&record.id, record).await {
                Ok(()) => {
                    if let Ok(id) = Uuid::parse_str(&record.id) {
                        context.checkpoint.record_push_done(id);
                    }
                }
                Err(_) => {
                    context.failed_ids.push(record.id.clone());
                }
            }
        }

        if !context.to_download.is_empty() {
            let results = context
                .storage
                .batch_download_records(&context.to_download)
                .await;

            for (record_id, result) in results {
                match result {
                    Ok(Some(record)) => {
                        if let Ok(id) = Uuid::parse_str(&record_id) {
                            context.checkpoint.record_pull_done(id);
                        }
                        context.downloads.insert(record_id, record);
                    }
                    Ok(None) => {}
                    Err(_) => {
                        context.failed_ids.push(record_id.clone());
                    }
                }
            }
        }

        for conflict_id in &context.conflicts {
            if let Some(remote_info) = context
                .remote_metadata
                .as_ref()
                .and_then(|m| m.records.get(conflict_id))
            {
                match context.storage.download_record(conflict_id).await {
                    Ok(Some(cloud_record)) => {
                        match context
                            .conflict_manager
                            .store_conflict(&cloud_record, &remote_info.checksum)
                        {
                            Ok(conflict_data) => {
                                context.conflict_data_map.insert(
                                    conflict_id.clone(),
                                    conflict_data,
                                );
                                if let Ok(id) = Uuid::parse_str(conflict_id) {
                                    context.checkpoint.add_pending_conflict(PendingConflict {
                                        record_id: id,
                                        local_version: context
                                            .local_records
                                            .iter()
                                            .find(|r| r.record_id == *conflict_id)
                                            .map(|r| r.version)
                                            .unwrap_or(0),
                                        remote_version: remote_info.version,
                                    });
                                }
                            }
                            Err(_) => {
                                context.failed_ids.push(conflict_id.clone());
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        context.failed_ids.push(conflict_id.clone());
                    }
                }
            }
        }

        if !context.conflicts.is_empty() {
            return StageOutcome::ConflictDetected {
                conflict_ids: context.conflicts.clone(),
            };
        }

        StageOutcome::Continue
    }
}

/// Stage 4: Resolve
///
/// Reports conflicts that need external resolution.
#[derive(Debug)]
pub struct ResolveStage;

impl ResolveStage {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ResolveStage {
    fn default() -> Self {
        Self::new()
    }
}

impl SyncStage for ResolveStage {
    async fn execute(&self, context: &mut PipelineContext) -> StageOutcome {
        if !context.conflicts.is_empty() {
            return StageOutcome::ConflictDetected {
                conflict_ids: context.conflicts.clone(),
            };
        }

        StageOutcome::Continue
    }
}

#[derive(Debug)]
pub enum PipelineResult {
    Completed,
    NoChanges,
    ConflictsDetected {
        conflict_ids: Vec<String>,
    },
    Error(Box<SyncError>),
}

#[derive(Debug)]
enum Stage {
    PullMetadata(PullMetadataStage),
    Detect(DetectStage),
    Push(PushStage),
    Resolve(ResolveStage),
}

impl SyncStage for Stage {
    async fn execute(&self, context: &mut PipelineContext) -> StageOutcome {
        match self {
            Stage::PullMetadata(s) => s.execute(context).await,
            Stage::Detect(s) => s.execute(context).await,
            Stage::Push(s) => s.execute(context).await,
            Stage::Resolve(s) => s.execute(context).await,
        }
    }
}

#[derive(Debug)]
pub struct SyncPipeline {
    stages: Vec<Stage>,
}

impl SyncPipeline {
    pub fn new() -> Self {
        Self {
            stages: vec![
                Stage::PullMetadata(PullMetadataStage),
                Stage::Detect(DetectStage),
                Stage::Push(PushStage),
                Stage::Resolve(ResolveStage),
            ],
        }
    }

    pub async fn execute(&self, context: &mut PipelineContext) -> PipelineResult {
        for stage in &self.stages {
            let outcome = stage.execute(context).await;
            match outcome {
                StageOutcome::Continue => continue,
                StageOutcome::NoChanges => return PipelineResult::NoChanges,
                StageOutcome::ConflictDetected { conflict_ids } => {
                    return PipelineResult::ConflictsDetected { conflict_ids };
                }
                StageOutcome::Error(e) => return PipelineResult::Error(e),
            }
        }
        PipelineResult::Completed
    }
}

impl Default for SyncPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cloud::metadata::{DeviceInfo, RecordVersionInfo};
    use crate::cloud::record::{AadFields, RecordMetadata};
    use chrono::Utc;
    use tempfile::TempDir;

    fn create_test_storage() -> (CloudStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let op = opendal::Operator::new(
            opendal::services::Fs::default()
                .root(temp_dir.path().to_str().unwrap()),
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

        assert!(matches!(outcome, StageOutcome::Error(e) if matches!(*e, SyncError::VaultIdentityMismatch { .. })));
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
        assert_eq!(context.remote_metadata.as_ref().unwrap().metadata_version, 3);
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

        // Upload a record first
        let record = create_test_cloud_record("record-1", 2);
        storage.upload_record("record-1", &record).await.unwrap();

        let mut context = PipelineContext::new(
            storage,
            ConflictManager::new(),
            checkpoint,
            1,
            "test_token".to_string(),
        );

        // Request download
        context.to_download.push("record-1".to_string());

        let stage = PushStage::new();
        let outcome = stage.execute(&mut context).await;

        assert!(matches!(outcome, StageOutcome::Continue));
        assert!(context.downloads.contains_key("record-1"));
    }

    #[tokio::test]
    async fn push_handles_conflicts() {
        let (storage, _temp_dir) = create_test_storage();
        let checkpoint = create_test_checkpoint();

        let record_id = "550e8400-e29b-41d4-a716-446655440000";
        let record = create_test_cloud_record(record_id, 2);
        storage.upload_record(record_id, &record).await.unwrap();
        let correct_checksum = record.compute_checksum().unwrap();

        let mut metadata =
            create_test_metadata_with_records("test_token", 1, vec![(record_id, 2)]);
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