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
    /// Last remote metadata snapshot persisted locally after a completed sync.
    pub last_remote_metadata: Option<CloudMetadata>,
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
    /// IDs of records successfully uploaded during Push stage.
    pub uploaded_ids: Vec<String>,
    /// Metadata that should be persisted locally after the sync completes.
    pub final_metadata: Option<CloudMetadata>,
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
}

impl fmt::Debug for PipelineContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PipelineContext")
            .field("local_metadata_version", &self.local_metadata_version)
            .field(
                "last_remote_metadata_version",
                &self
                    .last_remote_metadata
                    .as_ref()
                    .map(|m| m.metadata_version),
            )
            .field("local_vault_token", &self.local_vault_token)
            .field("to_upload", &self.to_upload.len())
            .field("to_download", &self.to_download.len())
            .field("conflicts", &self.conflicts.len())
            .field("failed_ids", &self.failed_ids.len())
            .field("uploads", &self.uploads.len())
            .field("downloads", &self.downloads.len())
            .field("uploaded_ids", &self.uploaded_ids.len())
            .field(
                "downloaded_health_states",
                &self.downloaded_health_states.len(),
            )
            .field(
                "downloaded_health_deleted",
                &self.downloaded_health_deleted.len(),
            )
            .finish()
    }
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
            last_remote_metadata: None,
            local_metadata_version,
            local_vault_token,
            to_upload: Vec::new(),
            to_download: Vec::new(),
            conflicts: Vec::new(),
            failed_ids: Vec::new(),
            uploads: Vec::new(),
            downloads: HashMap::new(),
            uploaded_ids: Vec::new(),
            final_metadata: None,
            conflict_data_map: HashMap::new(),
            local_records: Vec::new(),
            downloaded_health_states: Vec::new(),
            downloaded_health_deleted: Vec::new(),
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

    /// Sets the previous remote metadata snapshot used for same-version
    /// private metadata change detection.
    pub fn set_last_remote_metadata(&mut self, metadata: Option<CloudMetadata>) {
        self.last_remote_metadata = metadata;
    }

    fn private_metadata_changed_since_last_snapshot(
        &self,
        record_id: &str,
        remote: &crate::cloud::RecordVersionInfo,
    ) -> bool {
        let Some(remote_checksum) = remote.private_metadata_checksum.as_deref() else {
            return false;
        };

        let previous_checksum = self
            .last_remote_metadata
            .as_ref()
            .and_then(|metadata| metadata.records.get(record_id))
            .and_then(|info| info.private_metadata_checksum.as_deref());

        match previous_checksum {
            Some(previous) => previous != remote_checksum,
            None => self
                .remote_metadata
                .as_ref()
                .is_some_and(|metadata| metadata.metadata_version > self.local_metadata_version),
        }
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
                let has_pending_uploads = context
                    .local_records
                    .iter()
                    .any(|record| record.sync_status == SyncStatus::Pending);
                let local_record_ids = context
                    .local_records
                    .iter()
                    .map(|record| record.record_id.as_str())
                    .collect::<std::collections::HashSet<_>>();
                let has_missing_remote_records = remote_metadata
                    .records
                    .iter()
                    .any(|(id, info)| !info.deleted && !local_record_ids.contains(id.as_str()));

                if remote_metadata.metadata_version == context.local_metadata_version
                    && !has_pending_uploads
                    && !has_missing_remote_records
                {
                    return StageOutcome::NoChanges;
                }

                if remote_metadata.vault_identity_token != context.local_vault_token {
                    return StageOutcome::Error(Box::new(SyncError::VaultIdentityMismatch {
                        local_token: context.local_vault_token.clone(),
                        remote_token: remote_metadata.vault_identity_token.clone(),
                    }));
                }

                context.final_metadata = Some(remote_metadata.clone());
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

        let Some(remote_metadata) = context.remote_metadata.as_ref() else {
            return StageOutcome::Continue;
        };
        let remote_records = &remote_metadata.records;
        let local_record_ids = context
            .local_records
            .iter()
            .map(|record| record.record_id.as_str())
            .collect::<std::collections::HashSet<_>>();

        for local in &context.local_records {
            let remote_info = remote_records.get(&local.record_id);

            let action = match remote_info {
                Some(remote)
                    if local.sync_status == SyncStatus::Synced
                        && remote.version == local.version
                        && context.private_metadata_changed_since_last_snapshot(
                            &local.record_id,
                            remote,
                        ) =>
                {
                    ConflictAction::DownloadOnly
                }
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

        for (remote_id, remote_info) in remote_records {
            if remote_info.deleted || local_record_ids.contains(remote_id.as_str()) {
                continue;
            }
            context.to_download.push(remote_id.clone());
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
        let to_upload = context
            .to_upload
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();

        for record in context
            .uploads
            .iter()
            .filter(|record| to_upload.contains(record.id.as_str()))
        {
            match context.storage.upload_record(&record.id, record).await {
                Ok(()) => {
                    context.uploaded_ids.push(record.id.clone());
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
                                if let Some(expected) =
                                    remote_info.private_metadata_checksum.as_deref()
                                {
                                    match record.compute_private_metadata_checksum() {
                                        Ok(Some(computed)) if computed == expected => {}
                                        Ok(Some(computed)) => {
                                            tracing::warn!(
                                                record_id = %record_id,
                                                expected = %expected,
                                                actual = %computed,
                                                "Private metadata checksum mismatch on downloaded record, skipping"
                                            );
                                            context.failed_ids.push(record_id);
                                            continue;
                                        }
                                        Ok(None) => {
                                            tracing::warn!(
                                                record_id = %record_id,
                                                expected = %expected,
                                                "Downloaded record has no encrypted private metadata, skipping"
                                            );
                                            context.failed_ids.push(record_id);
                                            continue;
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                record_id = %record_id,
                                                error = %e,
                                                "Failed to compute private metadata checksum, skipping"
                                            );
                                            context.failed_ids.push(record_id);
                                            continue;
                                        }
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
                        } else if !record.has_encrypted_private_metadata() {
                            // Remote has no health metadata — schedule local deletion
                            // so stale health data doesn't linger. Encrypted
                            // private metadata is decoded later by the vault.
                            if let Ok(id) = Uuid::parse_str(&record_id) {
                                context.downloaded_health_deleted.push(id);
                            }
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

        // Update remote metadata so other devices can detect uploaded records.
        if !context.uploaded_ids.is_empty() {
            match Self::push_updated_metadata(context).await {
                Ok(updated) => {
                    context.final_metadata = Some(updated);
                }
                Err(e) => return StageOutcome::Error(Box::new(e)),
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

impl PushStage {
    /// Build updated CloudMetadata from successfully uploaded records and push
    /// atomically so remote devices can detect the new versions.
    async fn push_updated_metadata(
        context: &mut PipelineContext,
    ) -> Result<CloudMetadata, SyncError> {
        use crate::cloud::RecordVersionInfo;

        let now = chrono::Utc::now().to_rfc3339();
        let device_id = "local-device".to_string();

        // Start from existing remote metadata or create new one.
        let (mut updated_meta, expected_version) = if let Some(ref remote) = context.remote_metadata
        {
            (remote.clone(), remote.metadata_version)
        } else {
            (CloudMetadata::new(context.local_vault_token.clone()), 0)
        };

        // Add successfully uploaded records to metadata
        for record in &context.uploads {
            if !context.uploaded_ids.contains(&record.id) {
                continue;
            }
            let checksum = compute_checksum(&record.encrypted_data)?;
            let private_metadata_checksum = record.compute_private_metadata_checksum()?;
            updated_meta.upsert_record(
                record.id.clone(),
                RecordVersionInfo {
                    version: record.version,
                    updated_at: now.clone(),
                    updated_by: device_id.clone(),
                    checksum,
                    private_metadata_checksum,
                    deleted: record.deleted.unwrap_or(false),
                },
            );
        }

        updated_meta.increment_version();

        context
            .storage
            .push_metadata_atomic(&updated_meta, expected_version)
            .await?;

        Ok(updated_meta)
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
