//! SyncPipeline - 4-stage sync orchestration.

use std::collections::HashMap;
use std::fmt;

use uuid::Uuid;

use crate::cloud::validation::compute_checksum;
use crate::cloud::{CloudMetadata, CloudRecord, CloudStorage};
use crate::errors::mapping::sync::SyncError;
use crate::sync::checkpoint::{PendingConflict, SyncCheckpoint};
use crate::sync::conflict::{ConflictAction, ConflictManager};
use crate::types::health::RecordHealthState;
use crate::types::sync::SyncStatus;

#[derive(Debug)]
pub enum StageOutcome {
    Continue,
    ConflictDetected { conflict_ids: Vec<String> },
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

/// Adapter trait that bridges the sync pipeline to the vault's health state
/// storage. The pipeline calls these methods to read/write health states
/// without depending on VaultService directly.
///
/// Implementations are provided by the caller (SyncTask or a bridge layer)
/// at pipeline construction time.
pub trait HealthSyncAdapter: Send + Sync {
    /// Look up the persisted health state for a record about to be uploaded.
    ///
    /// Returns `None` when no health state exists for the given record.
    /// The pipeline attaches the returned state to the `CloudRecord` before
    /// uploading so that remote devices can restore it on download.
    fn get_health_state(&self, record_id: &Uuid) -> Option<RecordHealthState>;

    /// Persist downloaded health states into the local store.
    ///
    /// Called once after the Push stage completes with all health states
    /// extracted from downloaded CloudRecords. Each state is upserted so
    /// that local health report reflects the remote evaluation.
    fn persist_health_states(&self, states: &[RecordHealthState]);

    /// Delete local health states for records whose CloudRecord carried no
    /// health metadata (e.g. uploaded by an older client version).
    fn delete_health_states(&self, record_ids: &[Uuid]);

    /// Support downcasting for adapter-specific operations (e.g. flush).
    fn as_any(&self) -> &dyn std::any::Any;
}

/// A no-op adapter used when no vault is available (e.g. standalone tests).
pub struct NoOpHealthSyncAdapter;

impl HealthSyncAdapter for NoOpHealthSyncAdapter {
    fn get_health_state(&self, _record_id: &Uuid) -> Option<RecordHealthState> {
        None
    }

    fn persist_health_states(&self, _states: &[RecordHealthState]) {}

    fn delete_health_states(&self, _record_ids: &[Uuid]) {}

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl fmt::Debug for NoOpHealthSyncAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NoOpHealthSyncAdapter").finish()
    }
}

/// Shared context passed between pipeline stages.
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
    /// Health states extracted from downloaded CloudRecords.
    ///
    /// Populated by PushStage when downloading records that carry health
    /// metadata. The caller reads this after the pipeline completes and
    /// persists the health states to the local DB.
    pub downloaded_health_states: Vec<RecordHealthState>,
    /// Record IDs whose downloaded CloudRecord had `metadata.health = None`.
    ///
    /// The caller should delete local health states for these records so that
    /// stale health data from a previous version does not linger.
    pub downloaded_health_deleted: Vec<Uuid>,
    /// Health sync adapter for reading/writing health states from/to the
    /// local vault. Set to `NoOpHealthSyncAdapter` when no vault is available.
    health_adapter: Box<dyn HealthSyncAdapter>,
}

impl fmt::Debug for PipelineContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipelineContext")
            .field("local_metadata_version", &self.local_metadata_version)
            .field("local_vault_token", &self.local_vault_token)
            .field("to_upload", &self.to_upload.len())
            .field("to_download", &self.to_download.len())
            .field("conflicts", &self.conflicts.len())
            .field("failed_ids", &self.failed_ids.len())
            .field("uploads", &self.uploads.len())
            .field("downloads", &self.downloads.len())
            .field(
                "downloaded_health_states",
                &self.downloaded_health_states.len(),
            )
            .field(
                "downloaded_health_deleted",
                &self.downloaded_health_deleted.len(),
            )
            .field("health_adapter", &"<adapter>")
            .finish()
    }
}

impl PipelineContext {
    /// Creates a new PipelineContext with the given parameters and a no-op
    /// health adapter. Used in tests and when no vault is available.
    pub fn new(
        storage: CloudStorage,
        conflict_manager: ConflictManager,
        checkpoint: SyncCheckpoint,
        local_metadata_version: u64,
        local_vault_token: String,
    ) -> Self {
        Self::with_health_adapter(
            storage,
            conflict_manager,
            checkpoint,
            local_metadata_version,
            local_vault_token,
            Box::new(NoOpHealthSyncAdapter),
        )
    }

    /// Creates a new PipelineContext with a custom health adapter.
    ///
    /// Use this constructor when a vault is available and health states should
    /// be read/written during the sync pipeline.
    pub fn with_health_adapter(
        storage: CloudStorage,
        conflict_manager: ConflictManager,
        checkpoint: SyncCheckpoint,
        local_metadata_version: u64,
        local_vault_token: String,
        health_adapter: Box<dyn HealthSyncAdapter>,
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
            downloaded_health_states: Vec::new(),
            downloaded_health_deleted: Vec::new(),
            health_adapter,
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

    /// Returns a reference to the health adapter.
    pub fn health_adapter(&self) -> &dyn HealthSyncAdapter {
        self.health_adapter.as_ref()
    }

    /// Takes ownership of the health adapter, replacing it with a no-op.
    ///
    /// Used after pipeline execution to extract a buffering adapter (e.g.
    /// `VaultHealthSyncAdapter`) so the caller can flush writes to the vault.
    pub fn take_health_adapter(&mut self) -> Box<dyn HealthSyncAdapter> {
        std::mem::replace(&mut self.health_adapter, Box::new(NoOpHealthSyncAdapter))
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
                Some(remote) => context.conflict_manager.detect_conflicts(
                    local.sync_status,
                    local.version,
                    remote.version,
                ),
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
        // Upload path: attach health states from the adapter to CloudRecords
        // that do not already carry health metadata.
        for record in &mut context.uploads {
            if record.metadata.health.is_none() {
                if let Ok(id) = Uuid::parse_str(&record.id) {
                    if let Some(state) = context.health_adapter.get_health_state(&id) {
                        record.metadata.health =
                            Some(crate::cloud::RecordHealthMetadata::from_state(&state));
                    }
                }
            }
        }

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
                        // Validate AAD integrity (record_id + dek_version match)
                        if let Err(e) = record.validate() {
                            tracing::warn!(
                                record_id = %record_id,
                                error = %e,
                                "Downloaded record failed validation, skipping"
                            );
                            context.failed_ids.push(record_id);
                            continue;
                        }

                        // Validate checksum against remote metadata if available
                        if let Some(ref meta) = context.remote_metadata {
                            if let Some(remote_info) = meta.records.get(&record_id) {
                                match compute_checksum(&record.encrypted_data) {
                                    Ok(computed) if computed == remote_info.checksum => {}
                                    Ok(computed) => {
                                        tracing::warn!(
                                            record_id = %record_id,
                                            expected = %remote_info.checksum,
                                            actual = %computed,
                                            "Checksum mismatch on downloaded record, skipping"
                                        );
                                        context.failed_ids.push(record_id);
                                        continue;
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            record_id = %record_id,
                                            error = %e,
                                            "Failed to compute checksum, skipping"
                                        );
                                        context.failed_ids.push(record_id);
                                        continue;
                                    }
                                }
                            }
                        }

                        if let Ok(id) = Uuid::parse_str(&record_id) {
                            context.checkpoint.record_pull_done(id);
                        }

                        // Extract health metadata from the downloaded record.
                        if let Some(health_state) = record.to_health_state() {
                            context.downloaded_health_states.push(health_state);
                        } else if let Ok(id) = Uuid::parse_str(&record_id) {
                            // Remote has no health metadata — schedule local deletion
                            // so stale health data doesn't linger.
                            context.downloaded_health_deleted.push(id);
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
                                context
                                    .conflict_data_map
                                    .insert(conflict_id.clone(), conflict_data);
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

        // Download path: persist health states via the adapter.
        if !context.downloaded_health_states.is_empty() {
            context
                .health_adapter
                .persist_health_states(&context.downloaded_health_states);
        }
        if !context.downloaded_health_deleted.is_empty() {
            context
                .health_adapter
                .delete_health_states(&context.downloaded_health_deleted);
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
    ConflictsDetected { conflict_ids: Vec<String> },
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
