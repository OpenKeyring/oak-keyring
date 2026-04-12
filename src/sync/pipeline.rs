use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug, Clone)]
pub enum SyncError {
    #[error("Vault identity mismatch: expected {expected}, got {actual}")]
    VaultIdentityMismatch { expected: String, actual: String },

    #[error("Cloud operation failed: {0}")]
    CloudError(String),

    #[error("Record not found: {0}")]
    RecordNotFound(Uuid),

    #[error("Conflict detected for record: {0}")]
    ConflictDetected(Uuid),

    #[error("Metadata corrupted or missing required fields")]
    MetadataCorrupted,

    #[error("Fast-path: local and remote in sync")]
    FastPathNoChanges,

    #[error("Unexpected error: {0}")]
    Unexpected(String),
}

#[derive(Debug, Clone)]
pub enum StageOutcome {
    Continue,
    ConflictDetected,
    NoChanges,
    Error(Box<SyncError>),
}

#[derive(Debug, Clone)]
pub struct PipelineContext {
    pub local_vault_identity: String,
    pub remote_vault_identity: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub local_only_records: Vec<LocalRecordInfo>,
    pub remote_only_records: Vec<RemoteRecordInfo>,
    pub conflict_records: Vec<ConflictRecordInfo>,
    pub uploaded_records: Vec<Uuid>,
    pub downloaded_records: Vec<Uuid>,
    pub unresolved_conflicts: Vec<ConflictRecordInfo>,
    pub fast_path: bool,
}

impl Default for PipelineContext {
    fn default() -> Self {
        Self {
            local_vault_identity: Uuid::new_v4().to_string(),
            remote_vault_identity: None,
            last_sync_at: None,
            local_only_records: Vec::new(),
            remote_only_records: Vec::new(),
            conflict_records: Vec::new(),
            uploaded_records: Vec::new(),
            downloaded_records: Vec::new(),
            unresolved_conflicts: Vec::new(),
            fast_path: false,
        }
    }
}

impl PipelineContext {
    pub fn new(local_vault_identity: String) -> Self {
        Self {
            local_vault_identity,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalRecordInfo {
    pub id: Uuid,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
    pub encrypted_data: Vec<u8>,
    pub nonce: [u8; 24],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRecordInfo {
    pub id: Uuid,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
    pub encrypted_data: Vec<u8>,
    pub nonce: [u8; 24],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecordInfo {
    pub id: Uuid,
    pub local_updated_at: DateTime<Utc>,
    pub remote_updated_at: DateTime<Utc>,
    pub local_version: u64,
    pub remote_version: u64,
    pub local_encrypted_data: Vec<u8>,
    pub remote_encrypted_data: Vec<u8>,
    pub local_nonce: [u8; 24],
    pub remote_nonce: [u8; 24],
}

#[derive(Debug, Clone)]
pub struct PullMetadataResult {
    pub remote_vault_identity: String,
    pub remote_last_sync: Option<DateTime<Utc>>,
    pub remote_records: Vec<RemoteRecordInfo>,
    pub has_changes: bool,
}

pub trait CloudStorage: Send + Sync {
    fn pull_metadata(
        &self,
    ) -> impl std::future::Future<Output = Result<PullMetadataResult, SyncError>> + Send;
    fn upload_record(
        &self,
        id: Uuid,
        encrypted_data: &[u8],
        nonce: &[u8; 24],
        updated_at: DateTime<Utc>,
        version: u64,
    ) -> impl std::future::Future<Output = Result<(), SyncError>> + Send;
    fn download_record(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<RemoteRecordInfo, SyncError>> + Send;
    fn delete_record(
        &self,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<(), SyncError>> + Send;
}

pub trait SyncStage: Send + Sync {
    fn execute(
        &self,
        ctx: &mut PipelineContext,
    ) -> impl std::future::Future<Output = StageOutcome> + Send;
    fn name(&self) -> &'static str;
}

pub struct PullMetadataStage<S: CloudStorage> {
    storage: S,
}

impl<S: CloudStorage> PullMetadataStage<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S: CloudStorage + Clone> SyncStage for PullMetadataStage<S> {
    fn name(&self) -> &'static str {
        "PullMetadata"
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> StageOutcome {
        let metadata = match self.storage.pull_metadata().await {
            Ok(m) => m,
            Err(e) => return StageOutcome::Error(Box::new(e)),
        };

        if metadata.remote_vault_identity != ctx.local_vault_identity {
            return StageOutcome::Error(Box::new(SyncError::VaultIdentityMismatch {
                expected: ctx.local_vault_identity.clone(),
                actual: metadata.remote_vault_identity,
            }));
        }

        ctx.remote_vault_identity = Some(metadata.remote_vault_identity);
        ctx.last_sync_at = metadata.remote_last_sync;

        if !metadata.has_changes && metadata.remote_last_sync.is_some() {
            ctx.fast_path = true;
            return StageOutcome::NoChanges;
        }

        ctx.remote_only_records = metadata.remote_records;
        StageOutcome::Continue
    }
}

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
    fn name(&self) -> &'static str {
        "Detect"
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> StageOutcome {
        let local_ids: std::collections::HashSet<Uuid> =
            ctx.local_only_records.iter().map(|r| r.id).collect();

        let remote_ids: std::collections::HashSet<Uuid> =
            ctx.remote_only_records.iter().map(|r| r.id).collect();

        let remote_map: std::collections::HashMap<Uuid, RemoteRecordInfo> = ctx
            .remote_only_records
            .iter()
            .map(|r| (r.id, r.clone()))
            .collect();

        let common_ids: Vec<Uuid> = local_ids.intersection(&remote_ids).copied().collect();

        for local_record in &ctx.local_only_records {
            if !common_ids.contains(&local_record.id) {
                continue;
            }

            if let Some(remote_record) = remote_map.get(&local_record.id) {
                if local_record.version != remote_record.version
                    || local_record.updated_at != remote_record.updated_at
                {
                    ctx.conflict_records.push(ConflictRecordInfo {
                        id: local_record.id,
                        local_updated_at: local_record.updated_at,
                        remote_updated_at: remote_record.updated_at,
                        local_version: local_record.version,
                        remote_version: remote_record.version,
                        local_encrypted_data: local_record.encrypted_data.clone(),
                        remote_encrypted_data: remote_record.encrypted_data.clone(),
                        local_nonce: local_record.nonce,
                        remote_nonce: remote_record.nonce,
                    });
                }
            }
        }

        if !ctx.conflict_records.is_empty() {
            StageOutcome::ConflictDetected
        } else {
            StageOutcome::Continue
        }
    }
}

pub struct PushStage<S: CloudStorage> {
    storage: S,
}

impl<S: CloudStorage + Clone> PushStage<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }
}

impl<S: CloudStorage + Clone> SyncStage for PushStage<S> {
    fn name(&self) -> &'static str {
        "Push"
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> StageOutcome {
        for record in &ctx.local_only_records {
            if ctx.conflict_records.iter().any(|c| c.id == record.id) {
                continue;
            }

            match self
                .storage
                .upload_record(
                    record.id,
                    &record.encrypted_data,
                    &record.nonce,
                    record.updated_at,
                    record.version,
                )
                .await
            {
                Ok(_) => ctx.uploaded_records.push(record.id),
                Err(e) => {
                    return StageOutcome::Error(Box::new(e));
                }
            }
        }

        for record in &ctx.remote_only_records {
            if !ctx.conflict_records.iter().any(|c| c.id == record.id) {
                ctx.downloaded_records.push(record.id);
            }
        }

        if !ctx.conflict_records.is_empty() {
            ctx.unresolved_conflicts = ctx.conflict_records.clone();
            StageOutcome::ConflictDetected
        } else {
            StageOutcome::Continue
        }
    }
}

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
    fn name(&self) -> &'static str {
        "Resolve"
    }

    async fn execute(&self, ctx: &mut PipelineContext) -> StageOutcome {
        if ctx.unresolved_conflicts.is_empty() {
            StageOutcome::Continue
        } else {
            StageOutcome::ConflictDetected
        }
    }
}

pub enum Stage<S: CloudStorage> {
    PullMetadata(PullMetadataStage<S>),
    Detect(DetectStage),
    Push(PushStage<S>),
    Resolve(ResolveStage),
}

impl<S: CloudStorage + Clone> Stage<S> {
    pub async fn execute(&self, ctx: &mut PipelineContext) -> StageOutcome {
        match self {
            Stage::PullMetadata(s) => s.execute(ctx).await,
            Stage::Detect(s) => s.execute(ctx).await,
            Stage::Push(s) => s.execute(ctx).await,
            Stage::Resolve(s) => s.execute(ctx).await,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Stage::PullMetadata(s) => s.name(),
            Stage::Detect(s) => s.name(),
            Stage::Push(s) => s.name(),
            Stage::Resolve(s) => s.name(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum PipelineResult {
    Completed {
        uploaded: Vec<Uuid>,
        downloaded: Vec<Uuid>,
        conflicts: Vec<ConflictRecordInfo>,
        fast_path: bool,
    },
    NoChanges {
        fast_path: bool,
    },
    ConflictsDetected {
        conflicts: Vec<ConflictRecordInfo>,
    },
    Error {
        stage: &'static str,
        message: String,
    },
}

pub struct SyncPipeline<S: CloudStorage> {
    stages: Vec<Stage<S>>,
}

impl<S: CloudStorage + Clone> SyncPipeline<S> {
    pub fn new(storage: S) -> Self {
        let stages = vec![
            Stage::PullMetadata(PullMetadataStage::new(storage.clone())),
            Stage::Detect(DetectStage::new()),
            Stage::Push(PushStage::new(storage.clone())),
            Stage::Resolve(ResolveStage::new()),
        ];
        Self { stages }
    }

    pub async fn execute(&self, mut ctx: PipelineContext) -> PipelineResult {
        for stage in &self.stages {
            let stage_name = stage.name();
            let outcome = stage.execute(&mut ctx).await;

            match outcome {
                StageOutcome::Continue => continue,
                StageOutcome::NoChanges => {
                    return PipelineResult::NoChanges {
                        fast_path: ctx.fast_path,
                    };
                }
                StageOutcome::ConflictDetected => {
                    if stage_name == "Resolve" {
                        return PipelineResult::ConflictsDetected {
                            conflicts: ctx.unresolved_conflicts,
                        };
                    }
                    continue;
                }
                StageOutcome::Error(e) => {
                    return PipelineResult::Error {
                        stage: stage_name,
                        message: e.to_string(),
                    };
                }
            }
        }

        PipelineResult::Completed {
            uploaded: ctx.uploaded_records,
            downloaded: ctx.downloaded_records,
            conflicts: ctx.unresolved_conflicts,
            fast_path: ctx.fast_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct MockCloudStorage {
        metadata: Mutex<Option<PullMetadataResult>>,
        uploads: Mutex<Vec<Uuid>>,
        downloads: Mutex<Vec<Uuid>>,
    }

    impl MockCloudStorage {
        fn new(metadata: PullMetadataResult) -> Self {
            Self {
                metadata: Mutex::new(Some(metadata)),
                uploads: Mutex::new(Vec::new()),
                downloads: Mutex::new(Vec::new()),
            }
        }
    }

    impl CloudStorage for Arc<MockCloudStorage> {
        fn pull_metadata(
            &self,
        ) -> impl std::future::Future<Output = Result<PullMetadataResult, SyncError>> + Send
        {
            async move {
                self.metadata
                    .lock()
                    .unwrap()
                    .take()
                    .ok_or_else(|| SyncError::Unexpected("Already pulled".to_string()))
            }
        }

        fn upload_record(
            &self,
            id: Uuid,
            _encrypted_data: &[u8],
            _nonce: &[u8; 24],
            _updated_at: DateTime<Utc>,
            _version: u64,
        ) -> impl std::future::Future<Output = Result<(), SyncError>> + Send {
            async move {
                self.uploads.lock().unwrap().push(id);
                Ok(())
            }
        }

        fn download_record(
            &self,
            id: Uuid,
        ) -> impl std::future::Future<Output = Result<RemoteRecordInfo, SyncError>> + Send {
            async move {
                self.downloads.lock().unwrap().push(id);
                Err(SyncError::RecordNotFound(id))
            }
        }

        fn delete_record(
            &self,
            _id: Uuid,
        ) -> impl std::future::Future<Output = Result<(), SyncError>> + Send {
            async move { Ok(()) }
        }
    }

    #[tokio::test]
    async fn test_pipeline_no_changes_fast_path() {
        let metadata = PullMetadataResult {
            remote_vault_identity: "test-vault".to_string(),
            remote_last_sync: Some(Utc::now()),
            remote_records: vec![],
            has_changes: false,
        };

        let storage = Arc::new(MockCloudStorage::new(metadata));
        let pipeline = SyncPipeline::new(storage);
        let ctx = PipelineContext::new("test-vault".to_string());

        let result = pipeline.execute(ctx).await;
        assert!(matches!(result, PipelineResult::NoChanges { .. }));
    }

    #[tokio::test]
    async fn test_pipeline_vault_identity_mismatch() {
        let metadata = PullMetadataResult {
            remote_vault_identity: "different-vault".to_string(),
            remote_last_sync: None,
            remote_records: vec![],
            has_changes: true,
        };

        let storage = Arc::new(MockCloudStorage::new(metadata));
        let pipeline = SyncPipeline::new(storage);
        let ctx = PipelineContext::new("test-vault".to_string());

        let result = pipeline.execute(ctx).await;
        assert!(matches!(
            result,
            PipelineResult::Error { stage, .. } if stage == "PullMetadata"
        ));
    }

    #[tokio::test]
    async fn test_pipeline_local_records_upload() {
        let record_id = Uuid::new_v4();
        let metadata = PullMetadataResult {
            remote_vault_identity: "test-vault".to_string(),
            remote_last_sync: None,
            remote_records: vec![],
            has_changes: true,
        };

        let storage = Arc::new(MockCloudStorage::new(metadata));
        let pipeline = SyncPipeline::new(storage);

        let mut ctx = PipelineContext::new("test-vault".to_string());
        ctx.local_only_records.push(LocalRecordInfo {
            id: record_id,
            updated_at: Utc::now(),
            version: 1,
            encrypted_data: vec![1, 2, 3],
            nonce: [0u8; 24],
        });

        let result = pipeline.execute(ctx).await;
        assert!(matches!(
            result,
            PipelineResult::Completed { uploaded, .. } if uploaded.contains(&record_id)
        ));
    }
}
