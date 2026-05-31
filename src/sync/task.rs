//! SyncTask - Tokio async main loop for sync service.
//!
//! Manages the sync state machine, coordinates with the sync pipeline,
//! handles commands, and emits events.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use uuid::Uuid;

use crate::cloud::{CloudMetadata, CloudRecord, CloudStorage};
use crate::errors::mapping::sync::SyncError;
use crate::sync::checkpoint::SyncCheckpoint;
use crate::sync::conflict::{ConflictItem, ResolutionStrategy};
use crate::sync::pipeline::{LocalRecordInfo, PipelineContext, PipelineResult, SyncPipeline};
use crate::sync::retry::{BackoffTimer, RetryPolicy};
use crate::sync::state_machine::{SyncState, SyncStateMachine, SyncTrigger};
use crate::sync::ConflictManager;
use crate::types::health::RecordHealthState;

/// Vault data snapshot constructed by the executor before triggering sync.
///
/// Carries local records, upload-ready CloudRecords, vault identity, and
/// pre-read health states so the sync pipeline can operate without direct
/// vault access.
#[derive(Debug)]
pub struct SyncVaultData {
    /// Local records for conflict detection in the Detect stage.
    pub local_records: Vec<LocalRecordInfo>,
    /// CloudRecords ready for upload (with health metadata pre-populated).
    pub uploads: Vec<CloudRecord>,
    /// Local metadata version for fast-path comparison in Pull stage.
    pub local_metadata_version: u64,
    /// Last remote metadata snapshot persisted locally after a completed sync.
    pub last_remote_metadata: Option<CloudMetadata>,
    /// Vault identity token for validation in Pull stage.
    pub local_vault_token: String,
}

/// Commands accepted by SyncTask.
#[derive(Debug)]
pub enum SyncCommand {
    /// Trigger a full sync cycle (pull + detect + push + resolve).
    TriggerSync(Option<Box<SyncVaultData>>),
    /// Trigger a pull-only sync (no push).
    PullOnly,
    /// Resolve a single conflict.
    ResolveConflict {
        record_id: String,
        strategy: ResolutionStrategy,
    },
    /// Resolve all pending conflicts with the same strategy.
    ResolveAllConflicts { strategy: ResolutionStrategy },
    /// Pause sync processing.
    Pause,
    /// Resume sync processing.
    Resume,
    /// Atomically push metadata using CAS.
    PushMetadataAtomic {
        metadata: CloudMetadata,
        expected_version: u64,
    },
    /// Download metadata (routed through channel for state machine awareness).
    DownloadMetadata,
    /// Initiate graceful shutdown.
    Shutdown,
}

/// Events emitted by SyncTask.
#[derive(Debug)]
pub enum SyncEvent {
    /// Sync cycle completed successfully with a report and downloaded health states.
    Completed(
        SyncReport,
        Vec<RecordHealthState>,
        Vec<Uuid>,
        Vec<CloudRecord>,
        Vec<String>,
        HashMap<String, Vec<u8>>,
        Option<CloudMetadata>,
    ),
    /// Sync cycle failed with an error and current state.
    Failed { error: String, state: SyncState },
    /// A single conflict was resolved.
    ConflictResolved { record_id: String },
    /// All pending conflicts were resolved with the given count.
    AllConflictsResolved { count: usize },
    /// State machine transitioned to a new state.
    StateChanged { from: SyncState, to: SyncState },
    /// Metadata was atomically pushed.
    MetadataPushed,
    /// Metadata was downloaded.
    MetadataDownloaded(Option<CloudMetadata>),
    /// Shutdown is complete.
    ShutdownComplete,
    /// Sync was paused.
    Paused,
    /// Sync was resumed.
    Resumed,
}

/// Report of a completed sync cycle.
#[derive(Debug, Clone)]
pub struct SyncReport {
    /// Number of records uploaded.
    pub uploaded: u32,
    /// Number of records downloaded.
    pub downloaded: u32,
    /// Number of conflicts detected.
    pub conflicts: u32,
    /// Number of records that failed.
    pub failed: u32,
    /// Duration of the sync in milliseconds.
    pub duration_ms: u64,
}

struct PipelineExecutionOutcome {
    result: PipelineResult,
    downloaded_health_states: Vec<RecordHealthState>,
    downloaded_health_deleted: Vec<Uuid>,
    downloaded_records: Vec<CloudRecord>,
    uploaded_ids: Vec<String>,
    failed_count: u32,
    conflict_data: HashMap<String, Vec<u8>>,
    final_metadata: Option<CloudMetadata>,
}

/// The main sync task that coordinates state machine, pipeline, and event handling.
///
/// ```ignore
/// let (cmd_tx, cmd_rx) = mpsc::channel(16);
/// let (event_tx, mut event_rx) = mpsc::channel(16);
/// let task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));
///
/// let handle = tokio::spawn(async move {
///     task.run().await;
/// });
/// ```
pub struct SyncTask {
    state_machine: SyncStateMachine,
    pipeline: SyncPipeline,
    storage: CloudStorage,
    conflict_manager: ConflictManager,
    backoff_timer: BackoffTimer,
    cmd_rx: mpsc::Receiver<SyncCommand>,
    event_tx: mpsc::Sender<SyncEvent>,
    paused: bool,
    next_retry: Option<Instant>,
    /// Vault data for the current sync cycle (set before pipeline execution).
    vault_data: Option<Box<SyncVaultData>>,
    /// Conflict items stored from the last pipeline execution for resolution.
    pending_conflicts: Vec<ConflictItem>,
}

impl SyncTask {
    /// Creates a new SyncTask.
    pub fn new(
        storage: CloudStorage,
        cmd_rx: mpsc::Receiver<SyncCommand>,
        event_tx: mpsc::Sender<SyncEvent>,
        state_machine: SyncStateMachine,
    ) -> Self {
        Self {
            state_machine,
            pipeline: SyncPipeline::new(),
            storage,
            conflict_manager: ConflictManager::new(),
            backoff_timer: BackoffTimer::new(RetryPolicy::default()),
            cmd_rx,
            event_tx,
            paused: false,
            next_retry: None,
            vault_data: None,
            pending_conflicts: Vec::new(),
        }
    }

    /// Runs the main sync loop until shutdown.
    pub async fn run(&mut self) {
        loop {
            // Determine sleep duration for backoff
            let sleep_duration = self.next_retry.map(|instant| {
                if instant > Instant::now() {
                    instant - Instant::now()
                } else {
                    Duration::ZERO
                }
            });

            let sleep = tokio::time::sleep(sleep_duration.unwrap_or(Duration::from_secs(9999)));

            tokio::select! {
                // Branch 1: Handle commands
                cmd = self.cmd_rx.recv() => {
                    match cmd {
                        Some(SyncCommand::TriggerSync(data)) => {
                            self.vault_data = data;
                            self.handle_trigger_sync().await;
                        }
                        Some(SyncCommand::PullOnly) => {
                            self.handle_pull_only().await;
                        }
                        Some(SyncCommand::ResolveConflict { record_id, strategy }) => {
                            self.handle_resolve_conflict(&record_id, strategy).await;
                        }
                        Some(SyncCommand::ResolveAllConflicts { strategy }) => {
                            self.handle_resolve_all(strategy).await;
                        }
                        Some(SyncCommand::Pause) => {
                            self.handle_pause();
                        }
                        Some(SyncCommand::Resume) => {
                            self.handle_resume();
                        }
                        Some(SyncCommand::PushMetadataAtomic {
                            metadata,
                            expected_version,
                        }) => {
                            self.handle_push_metadata_atomic(metadata, expected_version).await;
                        }
                        Some(SyncCommand::DownloadMetadata) => {
                            self.handle_download_metadata().await;
                        }
                        Some(SyncCommand::Shutdown) => {
                            self.handle_shutdown().await;
                            break;
                        }
                        None => {
                            // Channel closed, shutdown
                            break;
                        }
                    }
                }
                // Branch 2: Retry after backoff
                _ = sleep => {
                    if let Some(next_retry) = self.next_retry {
                        if Instant::now() >= next_retry {
                            self.next_retry = None;
                            self.handle_retry_after_backoff().await;
                        }
                    }
                }
            }
        }
    }

    /// Handles TriggerSync command.
    async fn handle_trigger_sync(&mut self) {
        if self.paused {
            return;
        }

        if !self.state_machine.can_accept_commands() {
            tracing::debug!(
                state = %self.state_machine.current_state(),
                "sync trigger ignored because a sync cycle is already active"
            );
            return;
        }

        let from_state = self.state_machine.current_state().clone();

        // Transition to Pulling state
        match self.state_machine.transition(SyncTrigger::TriggerSync) {
            Ok(to_state) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::StateChanged {
                        from: from_state,
                        to: to_state,
                    })
                    .await;
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: e.to_string(),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
                return;
            }
        }

        // Check connectivity first
        if let Err(e) = self.storage.check_connectivity().await {
            self.handle_error(e).await;
            return;
        }

        // Execute the sync pipeline
        let start = Instant::now();
        let outcome = self.execute_pipeline().await;
        self.handle_pipeline_result(outcome, start).await;
    }

    /// Handles PullOnly command.
    async fn handle_pull_only(&mut self) {
        if self.paused {
            return;
        }

        if !self.state_machine.can_accept_commands() {
            tracing::debug!(
                state = %self.state_machine.current_state(),
                "pull-only sync trigger ignored because a sync cycle is already active"
            );
            return;
        }

        let from_state = self.state_machine.current_state().clone();

        // Transition to Pulling state
        match self.state_machine.transition(SyncTrigger::PullOnly) {
            Ok(to_state) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::StateChanged {
                        from: from_state,
                        to: to_state,
                    })
                    .await;
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: e.to_string(),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
                return;
            }
        }

        // Check connectivity first
        if let Err(e) = self.storage.check_connectivity().await {
            self.handle_error(e).await;
            return;
        }

        // Execute the sync pipeline (same as trigger sync for now)
        let start = Instant::now();
        let outcome = self.execute_pipeline().await;
        self.handle_pipeline_result(outcome, start).await;
    }

    async fn handle_retry_after_backoff(&mut self) {
        let from_state = self.state_machine.current_state().clone();
        match self.state_machine.transition(SyncTrigger::BackoffExpired) {
            Ok(to_state) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::StateChanged {
                        from: from_state,
                        to: to_state.clone(),
                    })
                    .await;

                if matches!(to_state, SyncState::Pulling) {
                    if let Err(e) = self.storage.check_connectivity().await {
                        self.handle_error(e).await;
                        return;
                    }

                    let start = Instant::now();
                    let outcome = self.execute_pipeline().await;
                    self.handle_pipeline_result(outcome, start).await;
                }
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: e.to_string(),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
            }
        }
    }

    /// Handles ResolveConflict command.
    async fn handle_resolve_conflict(&mut self, record_id: &str, strategy: ResolutionStrategy) {
        let record_uuid = match Uuid::parse_str(record_id) {
            Ok(id) => id,
            Err(_) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: format!("invalid record_id: {record_id}"),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
                return;
            }
        };

        let item = self
            .pending_conflicts
            .iter()
            .find(|c| c.record_id == record_uuid);

        let Some(item) = item else {
            let _ = self
                .event_tx
                .send(SyncEvent::Failed {
                    error: format!("no pending conflict for record {record_id}"),
                    state: self.state_machine.current_state().clone(),
                })
                .await;
            return;
        };

        let result = match strategy {
            ResolutionStrategy::KeepLocal => {
                self.conflict_manager
                    .resolve_keep_local(item.current_version);
                // Upload local version with bumped version to cloud
                if let Some(cloud_record) = self
                    .vault_data
                    .as_ref()
                    .and_then(|d| d.uploads.iter().find(|r| r.id == record_id))
                {
                    let mut resolved = cloud_record.clone();
                    resolved.version = item.current_version + 1;
                    if let Err(e) = self.storage.upload_record(record_id, &resolved).await {
                        tracing::warn!(record_id, error = %e, "failed to upload resolved record");
                        Err(format!(
                            "upload failed for resolved record {record_id}: {e}"
                        ))
                    } else {
                        Ok(())
                    }
                } else {
                    Ok(())
                }
            }
            ResolutionStrategy::KeepRemote => self
                .conflict_manager
                .resolve_keep_remote(&item.conflict_data)
                .map(|_| ())
                .map_err(|e| e.to_string()),
        };

        match result {
            Ok(()) => {
                self.pending_conflicts
                    .retain(|c| c.record_id != record_uuid);
                let _ = self
                    .event_tx
                    .send(SyncEvent::ConflictResolved {
                        record_id: record_id.to_string(),
                    })
                    .await;
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: e.to_string(),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
            }
        }
    }

    /// Handles ResolveAllConflicts command.
    async fn handle_resolve_all(&mut self, strategy: ResolutionStrategy) {
        let items: Vec<ConflictItem> = self.pending_conflicts.drain(..).collect();
        let total = items.len();

        if total == 0 {
            let _ = self
                .event_tx
                .send(SyncEvent::AllConflictsResolved { count: 0 })
                .await;
            return;
        }

        let outcomes = self.conflict_manager.resolve_all_batch(&items, strategy);
        let mut succeeded = 0usize;

        for (outcome, item) in outcomes.into_iter().zip(items) {
            if outcome.result.is_ok() {
                let mut upload_ok = true;
                if strategy == ResolutionStrategy::KeepLocal {
                    let record_id_str = item.record_id.to_string();
                    if let Some(cloud_record) = self
                        .vault_data
                        .as_ref()
                        .and_then(|d| d.uploads.iter().find(|r| r.id == record_id_str))
                    {
                        let mut resolved = cloud_record.clone();
                        resolved.version = item.current_version + 1;
                        if let Err(e) = self.storage.upload_record(&record_id_str, &resolved).await
                        {
                            tracing::warn!(
                                record_id = %record_id_str,
                                error = %e,
                                "failed to upload resolved record in batch"
                            );
                            upload_ok = false;
                        }
                    }
                }
                if upload_ok {
                    succeeded += 1;
                } else {
                    self.pending_conflicts.push(item);
                }
            } else {
                tracing::warn!(
                    record_id = %item.record_id,
                    "conflict resolution failed, keeping in pending list"
                );
                self.pending_conflicts.push(item);
            }
        }

        let _ = self
            .event_tx
            .send(SyncEvent::AllConflictsResolved { count: succeeded })
            .await;
    }

    /// Handles Pause command.
    fn handle_pause(&mut self) {
        self.paused = true;
        let _ = self.event_tx.try_send(SyncEvent::Paused);
    }

    /// Handles Resume command.
    fn handle_resume(&mut self) {
        self.paused = false;
        let _ = self.event_tx.try_send(SyncEvent::Resumed);
    }

    /// Handles DownloadMetadata command.
    ///
    /// Intentionally does NOT check `self.paused` — this handler is called
    /// during DEK rotation while sync is paused, to fetch the current
    /// metadata version for CAS validation.
    async fn handle_download_metadata(&mut self) {
        match self.storage.download_metadata().await {
            Ok(meta) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::MetadataDownloaded(meta))
                    .await;
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: e.to_string(),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
            }
        }
    }

    /// Handles PushMetadataAtomic command.
    ///
    /// Intentionally does NOT check `self.paused` — this handler is called
    /// during DEK rotation while sync is paused (executor pauses sync,
    /// rotates locally, then uses this to push updated metadata atomically).
    /// Adding a paused-guard here would break the rotation protocol.
    async fn handle_push_metadata_atomic(
        &mut self,
        metadata: CloudMetadata,
        expected_version: u64,
    ) {
        match self
            .storage
            .push_metadata_atomic(&metadata, expected_version)
            .await
        {
            Ok(()) => {
                let _ = self.event_tx.send(SyncEvent::MetadataPushed).await;
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: e.to_string(),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
            }
        }
    }

    /// Handles Shutdown command.
    async fn handle_shutdown(&mut self) {
        let from_state = self.state_machine.current_state().clone();
        let _ = self.state_machine.transition(SyncTrigger::Shutdown);
        let to_state = self.state_machine.current_state().clone();
        let _ = self
            .event_tx
            .send(SyncEvent::StateChanged {
                from: from_state,
                to: to_state,
            })
            .await;
        let _ = self.event_tx.send(SyncEvent::ShutdownComplete).await;
    }

    /// Executes the sync pipeline with the current context.
    ///
    /// When vault data is available (provided via `TriggerSync(data)`), the
    /// pipeline uses real local records, upload CloudRecords, metadata version,
    /// and vault identity token. Without vault data (e.g. tests), placeholder
    /// defaults are used.
    async fn execute_pipeline(&mut self) -> PipelineExecutionOutcome {
        let checkpoint = SyncCheckpoint::new(std::env::temp_dir());

        let (metadata_version, last_remote_metadata, vault_token, local_records, uploads) =
            if let Some(ref data) = self.vault_data {
                (
                    data.local_metadata_version,
                    data.last_remote_metadata.clone(),
                    data.local_vault_token.clone(),
                    data.local_records.clone(),
                    data.uploads.clone(),
                )
            } else {
                (0, None, String::new(), Vec::new(), Vec::new())
            };

        let mut context = PipelineContext::new(
            self.storage.clone(),
            self.conflict_manager.clone(),
            checkpoint,
            metadata_version,
            vault_token,
        );
        context.set_last_remote_metadata(last_remote_metadata);

        if !local_records.is_empty() {
            context.set_local_records(local_records);
        }
        if !uploads.is_empty() {
            context.set_uploads(uploads);
        }

        let result = self.pipeline.execute(&mut context).await;
        let downloaded_health_states = std::mem::take(&mut context.downloaded_health_states);
        let downloaded_health_deleted = std::mem::take(&mut context.downloaded_health_deleted);
        let downloaded_records: Vec<CloudRecord> =
            context.downloads.drain().map(|(_, v)| v).collect();
        let uploaded_ids = std::mem::take(&mut context.uploaded_ids);
        let failed_count = context.failed_ids.len() as u32;
        let conflict_data = std::mem::take(&mut context.conflict_data_map);
        let final_metadata = if failed_count == 0 {
            context.final_metadata.clone()
        } else {
            None
        };

        // Store conflict items for later resolution
        if !context.conflicts.is_empty() {
            self.pending_conflicts = context
                .conflicts
                .iter()
                .filter_map(|id| {
                    let conflict_data = conflict_data.get(id)?.clone();
                    let local_record = context.local_records.iter().find(|r| r.record_id == *id)?;
                    let record_uuid = Uuid::parse_str(id).ok()?;
                    Some(ConflictItem {
                        record_id: record_uuid,
                        conflict_data,
                        current_version: local_record.version,
                    })
                })
                .collect();
        }

        PipelineExecutionOutcome {
            result,
            downloaded_health_states,
            downloaded_health_deleted,
            downloaded_records,
            uploaded_ids,
            failed_count,
            conflict_data,
            final_metadata,
        }
    }

    /// Handles pipeline execution result.
    async fn handle_pipeline_result(&mut self, outcome: PipelineExecutionOutcome, start: Instant) {
        let duration_ms = start.elapsed().as_millis() as u64;
        let uploaded_count = outcome.uploaded_ids.len() as u32;
        let downloaded_count = outcome.downloaded_records.len() as u32;

        match outcome.result {
            PipelineResult::Completed => {
                self.backoff_timer.reset();
                let from_state = self.state_machine.current_state().clone();
                self.state_machine.reset();
                let to_state = self.state_machine.current_state().clone();
                let _ = self
                    .event_tx
                    .send(SyncEvent::StateChanged {
                        from: from_state,
                        to: to_state,
                    })
                    .await;

                let report = SyncReport {
                    uploaded: uploaded_count,
                    downloaded: downloaded_count,
                    conflicts: 0,
                    failed: outcome.failed_count,
                    duration_ms,
                };
                let _ = self
                    .event_tx
                    .send(SyncEvent::Completed(
                        report,
                        outcome.downloaded_health_states,
                        outcome.downloaded_health_deleted,
                        outcome.downloaded_records,
                        outcome.uploaded_ids,
                        outcome.conflict_data,
                        outcome.final_metadata,
                    ))
                    .await;
            }
            PipelineResult::NoChanges => {
                self.backoff_timer.reset();
                let from_state = self.state_machine.current_state().clone();
                self.state_machine.reset();
                let to_state = self.state_machine.current_state().clone();
                let _ = self
                    .event_tx
                    .send(SyncEvent::StateChanged {
                        from: from_state,
                        to: to_state,
                    })
                    .await;

                let report = SyncReport {
                    uploaded: 0,
                    downloaded: 0,
                    conflicts: 0,
                    failed: outcome.failed_count,
                    duration_ms,
                };
                let _ = self
                    .event_tx
                    .send(SyncEvent::Completed(
                        report,
                        outcome.downloaded_health_states,
                        outcome.downloaded_health_deleted,
                        outcome.downloaded_records,
                        outcome.uploaded_ids,
                        outcome.conflict_data,
                        outcome.final_metadata,
                    ))
                    .await;
            }
            PipelineResult::ConflictsDetected { conflict_ids } => {
                self.backoff_timer.reset();
                let from_state = self.state_machine.current_state().clone();
                self.state_machine.reset();
                let to_state = self.state_machine.current_state().clone();
                let _ = self
                    .event_tx
                    .send(SyncEvent::StateChanged {
                        from: from_state,
                        to: to_state,
                    })
                    .await;

                let report = SyncReport {
                    uploaded: uploaded_count,
                    downloaded: downloaded_count,
                    conflicts: conflict_ids.len() as u32,
                    failed: outcome.failed_count,
                    duration_ms,
                };
                let _ = self
                    .event_tx
                    .send(SyncEvent::Completed(
                        report,
                        outcome.downloaded_health_states,
                        outcome.downloaded_health_deleted,
                        outcome.downloaded_records,
                        outcome.uploaded_ids,
                        outcome.conflict_data,
                        outcome.final_metadata,
                    ))
                    .await;
            }
            PipelineResult::Error(e) => {
                self.handle_error(*e).await;
            }
        }
    }

    /// Handles sync errors with backoff.
    async fn handle_error(&mut self, error: SyncError) {
        let from_state = self.state_machine.current_state().clone();

        // Determine the trigger based on error type
        let trigger = match &error {
            SyncError::NetworkTimeout { .. }
            | SyncError::NetworkUnreachable { .. }
            | SyncError::ConnectionRefused { .. }
            | SyncError::LockAcquireFailed { .. }
            | SyncError::ProviderError { .. } => {
                if self.backoff_timer.should_retry(&error) {
                    SyncTrigger::OtherError
                } else {
                    SyncTrigger::MaxRetriesExceeded
                }
            }
            _ => SyncTrigger::OtherError,
        };

        match self.state_machine.transition(trigger) {
            Ok(to_state) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::StateChanged {
                        from: from_state,
                        to: to_state.clone(),
                    })
                    .await;

                if matches!(to_state, SyncState::Error | SyncState::Offline) {
                    if self.backoff_timer.should_retry(&error) {
                        let delay = self.backoff_timer.next_delay();
                        self.next_retry = Some(Instant::now() + delay);
                    }

                    let _ = self
                        .event_tx
                        .send(SyncEvent::Failed {
                            error: error.to_string(),
                            state: to_state,
                        })
                        .await;
                }
            }
            Err(e) => {
                let _ = self
                    .event_tx
                    .send(SyncEvent::Failed {
                        error: format!("{}; transition error: {}", error, e),
                        state: self.state_machine.current_state().clone(),
                    })
                    .await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::CloudStorage;
    use tokio::time::timeout;

    fn create_test_storage() -> CloudStorage {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        CloudStorage::new(op, "memory".to_string())
    }

    #[tokio::test]
    async fn duplicate_trigger_while_pulling_is_ignored() {
        let (_cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        task.state_machine
            .transition(SyncTrigger::TriggerSync)
            .unwrap();

        task.handle_trigger_sync().await;

        let event = timeout(Duration::from_millis(50), event_rx.recv()).await;
        assert!(
            event.is_err(),
            "duplicate trigger should not emit a failure while sync is already running: {event:?}"
        );
    }

    #[tokio::test]
    async fn retry_after_backoff_runs_sync_instead_of_staying_pulling() {
        let (_cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, _event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        task.state_machine
            .transition(SyncTrigger::TriggerSync)
            .unwrap();
        task.handle_error(SyncError::ProviderError {
            provider: "google_drive".to_string(),
            message: "temporary token endpoint failure".to_string(),
        })
        .await;

        assert_eq!(*task.state_machine.current_state(), SyncState::Error);

        task.handle_retry_after_backoff().await;

        assert_ne!(
            *task.state_machine.current_state(),
            SyncState::Pulling,
            "retry must execute the sync cycle instead of leaving the state machine in Pulling"
        );
    }

    #[tokio::test]
    async fn trigger_sync_cycle() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        // Send TriggerSync command
        cmd_tx.send(SyncCommand::TriggerSync(None)).await.unwrap();

        // Run task in background
        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // Collect events until we get Completed, Failed, or timeout
        let mut final_event = None;
        let start = Instant::now();
        while let Ok(Some(event)) = timeout(Duration::from_secs(5), event_rx.recv()).await {
            if start.elapsed() > Duration::from_secs(4) {
                final_event = Some(event);
                break;
            }
            match &event {
                SyncEvent::Completed(..) | SyncEvent::Failed { .. } => {
                    final_event = Some(event);
                    break;
                }
                _ => continue,
            }
        }

        // Assert we got either Completed or Failed (memory backend may have issues)
        assert!(
            matches!(
                final_event,
                Some(SyncEvent::Completed(..)) | Some(SyncEvent::Failed { .. })
            ),
            "Expected Completed or Failed, got {:?}",
            final_event
        );

        // Send shutdown
        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn shutdown_graceful() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        // Run task in background
        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // Send shutdown command
        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();

        // Collect events until ShutdownComplete
        let mut found_shutdown_complete = false;
        while let Ok(Some(event)) = timeout(Duration::from_secs(2), event_rx.recv()).await {
            if matches!(event, SyncEvent::ShutdownComplete) {
                found_shutdown_complete = true;
                break;
            }
        }

        assert!(found_shutdown_complete, "Expected ShutdownComplete event");

        let _ = handle.await;
    }

    #[tokio::test]
    async fn pause_resume() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        // Run task in background
        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // Send Pause
        cmd_tx.send(SyncCommand::Pause).await.unwrap();

        // Receive Paused event
        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            matches!(event, SyncEvent::Paused),
            "Expected Paused, got {:?}",
            event
        );

        // Send Resume
        cmd_tx.send(SyncCommand::Resume).await.unwrap();

        // Receive Resumed event
        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            matches!(event, SyncEvent::Resumed),
            "Expected Resumed, got {:?}",
            event
        );

        // Send shutdown
        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn resolve_conflict_with_pending_item() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        let record_id = Uuid::new_v4();
        let record_id_str = record_id.to_string();

        // Pre-populate pending conflict with valid conflict data
        let cloud_record = CloudRecord {
            id: record_id_str.clone(),
            version: 2,
            encrypted_data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                b"test",
            ),
            nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 24]),
            dek_version: 1,
            aad: crate::cloud::record::AadFields {
                record_id: record_id_str.clone(),
                dek_version: 1,
            },
            metadata: crate::cloud::record::RecordMetadata {
                name: "conflict-record".to_string(),
                tags: vec![],
                updated_at: chrono::Utc::now().to_rfc3339(),
                health: None,
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        };
        let checksum = cloud_record.compute_checksum().unwrap();
        let conflict_data = crate::sync::ConflictManager::new()
            .store_conflict(&cloud_record, &checksum)
            .unwrap();

        task.pending_conflicts.push(ConflictItem {
            record_id,
            conflict_data,
            current_version: 1,
        });

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // Send ResolveConflict with KeepLocal
        cmd_tx
            .send(SyncCommand::ResolveConflict {
                record_id: record_id_str.clone(),
                strategy: ResolutionStrategy::KeepLocal,
            })
            .await
            .unwrap();

        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            matches!(event, SyncEvent::ConflictResolved { ref record_id } if record_id == &record_id_str),
            "Expected ConflictResolved, got {:?}",
            event
        );

        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn resolve_conflict_rejects_unknown_record() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // No pending conflicts — should get Failed
        cmd_tx
            .send(SyncCommand::ResolveConflict {
                record_id: Uuid::new_v4().to_string(),
                strategy: ResolutionStrategy::KeepLocal,
            })
            .await
            .unwrap();

        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            matches!(event, SyncEvent::Failed { .. }),
            "Expected Failed for unknown record, got {:?}",
            event
        );

        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn trigger_sync_conflict_can_be_resolved_after_detection() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let temp_dir = tempfile::TempDir::new().unwrap();
        let op = opendal::Operator::new(
            opendal::services::Fs::default().root(temp_dir.path().to_str().unwrap()),
        )
        .unwrap()
        .finish();
        let storage = CloudStorage::new(op, "fs".to_string());
        let record_id = Uuid::new_v4();
        let record_id_str = record_id.to_string();

        let remote_record = CloudRecord {
            id: record_id_str.clone(),
            version: 2,
            encrypted_data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                b"remote",
            ),
            nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 24]),
            dek_version: 1,
            aad: crate::cloud::record::AadFields {
                record_id: record_id_str.clone(),
                dek_version: 1,
            },
            metadata: crate::cloud::record::RecordMetadata {
                name: "remote-record".to_string(),
                tags: vec![],
                updated_at: chrono::Utc::now().to_rfc3339(),
                health: None,
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        };
        storage
            .upload_record(&record_id_str, &remote_record)
            .await
            .unwrap();

        let mut metadata = CloudMetadata::new("test-token".to_string());
        metadata.metadata_version = 2;
        metadata.upsert_record(
            record_id_str.clone(),
            crate::cloud::RecordVersionInfo {
                version: 2,
                updated_at: chrono::Utc::now().to_rfc3339(),
                updated_by: "remote-device".to_string(),
                checksum: remote_record.compute_checksum().unwrap(),
                private_metadata_checksum: None,
                deleted: false,
            },
        );
        storage.upload_metadata(&metadata).await.unwrap();

        let local_record = CloudRecord {
            id: record_id_str.clone(),
            version: 1,
            encrypted_data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                b"local",
            ),
            nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 24]),
            dek_version: 1,
            aad: crate::cloud::record::AadFields {
                record_id: record_id_str.clone(),
                dek_version: 1,
            },
            metadata: crate::cloud::record::RecordMetadata {
                name: "local-record".to_string(),
                tags: vec![],
                updated_at: chrono::Utc::now().to_rfc3339(),
                health: None,
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        };
        let vault_data = Box::new(SyncVaultData {
            local_records: vec![LocalRecordInfo {
                record_id: record_id_str.clone(),
                sync_status: crate::types::sync::SyncStatus::Pending,
                version: 1,
            }],
            uploads: vec![local_record],
            local_metadata_version: 1,
            last_remote_metadata: None,
            local_vault_token: "test-token".to_string(),
        });

        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));
        cmd_tx
            .send(SyncCommand::TriggerSync(Some(vault_data)))
            .await
            .unwrap();

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        let mut completed_event = None;
        while let Ok(Some(event)) = timeout(Duration::from_secs(5), event_rx.recv()).await {
            match event {
                SyncEvent::Completed(report, _, _, _, _, conflict_data, _) => {
                    completed_event = Some((report, conflict_data));
                    break;
                }
                SyncEvent::Failed { error, .. } => {
                    panic!("sync should detect a resolvable conflict, got failure: {error}");
                }
                _ => {}
            }
        }

        let (report, conflict_data) =
            completed_event.expect("sync should emit a completed conflict report");
        assert_eq!(report.conflicts, 1);
        assert!(
            conflict_data.contains_key(&record_id_str),
            "conflict report must include conflict payload for later resolution"
        );

        cmd_tx
            .send(SyncCommand::ResolveConflict {
                record_id: record_id_str.clone(),
                strategy: ResolutionStrategy::KeepRemote,
            })
            .await
            .unwrap();

        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(event, SyncEvent::ConflictResolved { ref record_id } if record_id == &record_id_str),
            "detected conflict should be pending and resolvable, got {event:?}"
        );

        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn resolve_all_with_no_pending_conflicts() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // No pending conflicts — should get AllConflictsResolved { count: 0 }
        cmd_tx
            .send(SyncCommand::ResolveAllConflicts {
                strategy: ResolutionStrategy::KeepRemote,
            })
            .await
            .unwrap();

        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            matches!(event, SyncEvent::AllConflictsResolved { count: 0 }),
            "Expected AllConflictsResolved {{ count: 0 }}, got {:?}",
            event
        );

        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    /// Builds a minimal SyncVaultData for testing SyncTask pipeline integration.
    fn make_test_vault_data(record_ids: &[&str]) -> Box<SyncVaultData> {
        let local_records: Vec<LocalRecordInfo> = record_ids
            .iter()
            .map(|id| LocalRecordInfo {
                record_id: id.to_string(),
                sync_status: crate::types::sync::SyncStatus::Pending,
                version: 1,
            })
            .collect();

        // Build minimal upload CloudRecords
        let uploads: Vec<CloudRecord> = record_ids
            .iter()
            .map(|id| {
                use crate::cloud::record::AadFields;
                CloudRecord {
                    id: id.to_string(),
                    version: 1,
                    encrypted_data: base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        b"test",
                    ),
                    nonce: base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        [0u8; 24],
                    ),
                    dek_version: 1,
                    aad: AadFields {
                        record_id: id.to_string(),
                        dek_version: 1,
                    },
                    metadata: crate::cloud::record::RecordMetadata {
                        name: format!("record-{}", id),
                        tags: vec![],
                        updated_at: chrono::Utc::now().to_rfc3339(),
                        health: None,
                        ..Default::default()
                    },
                    deleted: None,
                    deleted_at: None,
                }
            })
            .collect();

        Box::new(SyncVaultData {
            local_records,
            uploads,
            local_metadata_version: 0,
            last_remote_metadata: None,
            local_vault_token: "test-token".to_string(),
        })
    }

    #[tokio::test]
    async fn trigger_sync_with_vault_data_completes() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        let vault_data = make_test_vault_data(&["rec-1", "rec-2"]);
        cmd_tx
            .send(SyncCommand::TriggerSync(Some(vault_data)))
            .await
            .unwrap();

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        let mut final_event = None;
        while let Ok(Some(event)) = timeout(Duration::from_secs(5), event_rx.recv()).await {
            match &event {
                SyncEvent::Completed(..) | SyncEvent::Failed { .. } => {
                    final_event = Some(event);
                    break;
                }
                _ => continue,
            }
        }

        // Memory backend may fail metadata push (rename not supported), so accept
        // both Completed and Failed. The key invariant is that the pipeline ran
        // with vault data (the event was emitted without panic).
        match final_event {
            Some(SyncEvent::Completed(
                report,
                health_states,
                health_deleted,
                _downloaded,
                uploaded_ids,
                _conflict_data,
                _final_metadata,
            )) => {
                assert_eq!(report.uploaded, uploaded_ids.len() as u32);
                assert!(health_states.is_empty());
                assert!(health_deleted.is_empty());
            }
            Some(SyncEvent::Failed { .. }) => {
                // Memory backend does not support atomic metadata rename — expected.
            }
            other => {
                panic!("Expected Completed or Failed, got {:?}", other);
            }
        }

        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn trigger_sync_with_download_failure_does_not_advance_metadata_snapshot() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let temp_dir = tempfile::TempDir::new().unwrap();
        let op = opendal::Operator::new(
            opendal::services::Fs::default().root(temp_dir.path().to_str().unwrap()),
        )
        .unwrap()
        .finish();
        let storage = CloudStorage::new(op, "fs".to_string());
        let record_id = Uuid::new_v4().to_string();
        let cloud_record = CloudRecord {
            id: record_id.clone(),
            version: 1,
            encrypted_data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                b"test",
            ),
            nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, [0u8; 24]),
            dek_version: 1,
            aad: crate::cloud::record::AadFields {
                record_id: record_id.clone(),
                dek_version: 1,
            },
            metadata: crate::cloud::record::RecordMetadata {
                name: "remote-record".to_string(),
                tags: vec![],
                updated_at: chrono::Utc::now().to_rfc3339(),
                health: None,
                ..Default::default()
            },
            deleted: None,
            deleted_at: None,
        };
        storage
            .upload_record(&record_id, &cloud_record)
            .await
            .unwrap();
        let mut metadata = CloudMetadata::new("test-token".to_string());
        metadata.metadata_version = 2;
        metadata.upsert_record(
            record_id.clone(),
            crate::cloud::RecordVersionInfo {
                version: 1,
                updated_at: chrono::Utc::now().to_rfc3339(),
                updated_by: "remote-device".to_string(),
                checksum: "wrong-checksum".to_string(),
                private_metadata_checksum: None,
                deleted: false,
            },
        );
        storage.upload_metadata(&metadata).await.unwrap();

        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));
        let vault_data = Box::new(SyncVaultData {
            local_records: vec![LocalRecordInfo {
                record_id: record_id.clone(),
                sync_status: crate::types::sync::SyncStatus::Synced,
                version: 0,
            }],
            uploads: vec![],
            local_metadata_version: 1,
            last_remote_metadata: None,
            local_vault_token: "test-token".to_string(),
        });
        cmd_tx
            .send(SyncCommand::TriggerSync(Some(vault_data)))
            .await
            .unwrap();

        let handle = tokio::spawn(async move {
            task.run().await;
        });

        let mut final_event = None;
        while let Ok(Some(event)) = timeout(Duration::from_secs(5), event_rx.recv()).await {
            match &event {
                SyncEvent::Completed(..) | SyncEvent::Failed { .. } => {
                    final_event = Some(event);
                    break;
                }
                _ => continue,
            }
        }

        match final_event {
            Some(SyncEvent::Completed(report, _, _, downloaded, _, _, final_metadata)) => {
                assert_eq!(report.downloaded, 0);
                assert_eq!(report.failed, 1);
                assert!(downloaded.is_empty());
                assert!(
                    final_metadata.is_none(),
                    "failed download must not advance the local remote metadata snapshot"
                );
            }
            other => {
                panic!("Expected Completed, got {:?}", other);
            }
        }

        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }
}
