//! SyncService - Public interface for cloud sync operations.
//!
//! A thin wrapper around SyncTask's channel-based command interface.
//! Provides async methods that send commands and wait for corresponding events.

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use uuid::Uuid;

use crate::cloud::{CloudMetadata, CloudRecord, CloudStorage};
use crate::errors::mapping::sync::SyncError;
use crate::sync::conflict::ResolutionStrategy;
use crate::sync::state_machine::SyncStateMachine;
use crate::sync::task::{SyncCommand, SyncEvent, SyncReport, SyncTask, SyncVaultData};
use crate::types::health::RecordHealthState;

/// Channel capacity for command and event queues.
const CHANNEL_CAPACITY: usize = 16;

/// SyncService timeout durations.
const SYNC_TIMEOUT: Duration = Duration::from_secs(60);
const CONFLICT_TIMEOUT: Duration = Duration::from_secs(30);
const PAUSE_TIMEOUT: Duration = Duration::from_secs(30);
const RESUME_TIMEOUT: Duration = Duration::from_secs(10);

/// Result of a sync operation, bundling the report with health state vectors
/// and downloaded CloudRecords extracted from the pipeline.
#[derive(Debug)]
pub struct SyncResult {
    pub report: SyncReport,
    pub downloaded_health_states: Vec<RecordHealthState>,
    pub downloaded_health_deleted: Vec<Uuid>,
    pub downloaded_records: Vec<CloudRecord>,
    pub remote_metadata: Option<CloudMetadata>,
}

/// Public interface to the sync subsystem.
///
/// SyncService wraps a SyncTask running in a background tokio task and
/// exposes a request/response API via channels.
///
/// ```ignore
/// let storage = CloudStorage::new(operator, "memory".to_string());
/// let mut svc = SyncService::new(storage);
///
/// // Trigger a sync and wait for completion
/// let report = svc.sync().await.unwrap();
///
/// // Pause sync processing
/// svc.pause().await.unwrap();
///
/// // Resume sync processing
/// svc.resume().await.unwrap();
///
/// // Shutdown the service
/// svc.shutdown().await.unwrap();
/// ```
pub struct SyncService {
    /// Command sender to the background SyncTask.
    cmd_tx: mpsc::Sender<SyncCommand>,
    /// Event receiver from the background SyncTask.
    event_rx: mpsc::Receiver<SyncEvent>,
    /// Handle to the background task running SyncTask::run().
    task_handle: JoinHandle<()>,
    /// CloudStorage clone for connectivity checks.
    storage: CloudStorage,
}

impl SyncService {
    /// Creates a new SyncService, spawning a SyncTask in a background tokio task.
    ///
    /// The returned SyncService holds the send half of the command channel,
    /// the receive half of the event channel, and a handle to the background task.
    pub fn new(storage: CloudStorage) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel(CHANNEL_CAPACITY);
        let (event_tx, event_rx) = mpsc::channel(CHANNEL_CAPACITY);

        // Store storage clone for test_connection
        let storage_for_connection_check = storage.clone();

        // Create and spawn the background task
        let task_handle = tokio::spawn(async move {
            let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));
            task.run().await;
        });

        Self {
            cmd_tx,
            event_rx,
            task_handle,
            storage: storage_for_connection_check,
        }
    }

    /// Triggers a full sync cycle (pull + detect + push + resolve).
    ///
    /// Sends `TriggerSync` command and waits for `Completed` event.
    ///
    /// # Errors
    /// Returns `SyncError` if:
    /// - Timeout (60s) expires before `Completed` or `Failed` event
    /// - `Failed` event is received
    pub async fn sync(
        &mut self,
        vault_data: Option<Box<SyncVaultData>>,
    ) -> Result<SyncResult, SyncError> {
        self.sync_with_cancel(CancellationToken::new(), vault_data)
            .await
    }

    /// Triggers a full sync cycle and returns promptly if `cancel` is triggered.
    pub async fn sync_with_cancel(
        &mut self,
        cancel: CancellationToken,
        vault_data: Option<Box<SyncVaultData>>,
    ) -> Result<SyncResult, SyncError> {
        if cancel.is_cancelled() {
            return Err(SyncError::Cancelled {
                operation: "sync".to_string(),
            });
        }

        self.cmd_tx
            .send(SyncCommand::TriggerSync(vault_data))
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send TriggerSync command".to_string(),
            })?;

        let event = self
            .wait_for_event_or_cancel(SYNC_TIMEOUT, cancel, "sync", |event| {
                matches!(event, SyncEvent::Completed(..) | SyncEvent::Failed { .. })
            })
            .await?;

        match event {
            SyncEvent::Completed(report, health_states, health_deleted, records) => {
                Ok(SyncResult {
                    report,
                    downloaded_health_states: health_states,
                    downloaded_health_deleted: health_deleted,
                    downloaded_records: records,
                    remote_metadata: None,
                })
            }
            SyncEvent::Failed { error, state: _ } => Err(SyncError::ProviderError {
                provider: "sync".to_string(),
                message: error,
            }),
            _ => unreachable!(),
        }
    }

    /// Restores cloud state by pulling remote metadata and records only.
    ///
    /// This bypasses the SyncTask command loop so recovery never sends
    /// `TriggerSync` and never uploads local empty state back to cloud storage.
    pub async fn restore_pull_only(&mut self) -> Result<SyncResult, SyncError> {
        self.storage.check_connectivity().await?;

        let metadata =
            self.storage
                .download_metadata()
                .await?
                .ok_or_else(|| SyncError::RecordNotFound {
                    record_id: crate::cloud::schema::METADATA_FILENAME.to_string(),
                })?;
        metadata.validate()?;

        let mut ids: Vec<String> = Vec::with_capacity(metadata.records.len());
        for id in metadata.records.keys() {
            Uuid::parse_str(id).map_err(|_| SyncError::DeserializationFailed {
                message: format!("metadata record key must be a UUID: {}", id),
            })?;
            ids.push(id.clone());
        }
        ids.sort();

        let mut downloaded_records = Vec::new();
        let mut downloaded_health_states = Vec::new();
        let mut downloaded_health_deleted = Vec::new();

        for (record_id, result) in self.storage.batch_download_records(&ids).await {
            match result? {
                Some(record) => {
                    if record.id != record_id {
                        return Err(SyncError::AadInconsistent {
                            field: "record_id".to_string(),
                            expected: record_id,
                            actual: record.id,
                        });
                    }
                    record.validate()?;
                    if let Some(remote_info) = metadata.records.get(&record_id) {
                        if record.version != remote_info.version {
                            return Err(SyncError::MetadataVersionConflict {
                                local: remote_info.version,
                                remote: record.version,
                            });
                        }
                        let computed = record.compute_checksum()?;
                        if computed != remote_info.checksum {
                            return Err(SyncError::ChecksumMismatch {
                                expected: remote_info.checksum.clone(),
                                actual: computed,
                                record_id,
                            });
                        }
                    }

                    if let Some(health_state) = record.to_health_state() {
                        downloaded_health_states.push(health_state);
                    } else if let Ok(uuid) = Uuid::parse_str(&record_id) {
                        downloaded_health_deleted.push(uuid);
                    }
                    downloaded_records.push(record);
                }
                None => {
                    return Err(SyncError::RecordNotFound { record_id });
                }
            }
        }

        Ok(SyncResult {
            report: SyncReport {
                downloaded: downloaded_records.len() as u32,
                uploaded: 0,
                conflicts: 0,
                failed: 0,
                duration_ms: 0,
            },
            downloaded_health_states,
            downloaded_health_deleted,
            downloaded_records,
            remote_metadata: Some(metadata),
        })
    }

    /// Resolves a single conflict by record ID.
    ///
    /// Sends `ResolveConflict` command and waits for `ConflictResolved` event.
    ///
    /// # Errors
    /// Returns `SyncError` if:
    /// - Timeout (30s) expires before `ConflictResolved` event
    /// - Event's record_id does not match the requested record_id
    pub async fn resolve_conflict(
        &mut self,
        record_id: String,
        strategy: ResolutionStrategy,
    ) -> Result<(), SyncError> {
        self.cmd_tx
            .send(SyncCommand::ResolveConflict {
                record_id: record_id.clone(),
                strategy,
            })
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send ResolveConflict command".to_string(),
            })?;

        let event = self
            .wait_for_event(CONFLICT_TIMEOUT, |event| {
                matches!(event, SyncEvent::ConflictResolved { .. })
            })
            .await?;

        if let SyncEvent::ConflictResolved {
            record_id: actual_id,
        } = event
        {
            if actual_id == record_id {
                Ok(())
            } else {
                Err(SyncError::ProviderError {
                    provider: "sync".to_string(),
                    message: format!(
                        "record_id mismatch: expected {}, got {}",
                        record_id, actual_id
                    ),
                })
            }
        } else {
            unreachable!()
        }
    }

    /// Resolves all pending conflicts with the same strategy.
    ///
    /// Sends `ResolveAllConflicts` command and waits for `AllConflictsResolved` event.
    ///
    /// # Errors
    /// Returns `SyncError` if timeout (60s) expires before `AllConflictsResolved` event.
    ///
    /// # Returns
    /// Returns `Ok(0)` — the exact count of resolved conflicts is not available
    /// from the event.
    pub async fn resolve_all_conflicts(
        &mut self,
        strategy: ResolutionStrategy,
    ) -> Result<usize, SyncError> {
        self.cmd_tx
            .send(SyncCommand::ResolveAllConflicts { strategy })
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send ResolveAllConflicts command".to_string(),
            })?;

        let _event = self
            .wait_for_event(SYNC_TIMEOUT, |event| {
                matches!(event, SyncEvent::AllConflictsResolved)
            })
            .await?;

        Ok(0)
    }

    /// Downloads and deserializes cloud metadata.
    ///
    /// Routes through SyncTask channel so the state machine is aware of
    /// metadata reads (consistent with push_metadata_atomic).
    pub async fn download_metadata(&mut self) -> Result<Option<CloudMetadata>, SyncError> {
        self.cmd_tx
            .send(SyncCommand::DownloadMetadata)
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send DownloadMetadata command".to_string(),
            })?;

        let event = self
            .wait_for_event(SYNC_TIMEOUT, |event| {
                matches!(
                    event,
                    SyncEvent::MetadataDownloaded(_) | SyncEvent::Failed { .. }
                )
            })
            .await?;

        match event {
            SyncEvent::MetadataDownloaded(meta) => Ok(meta),
            SyncEvent::Failed { error, .. } => Err(SyncError::ProviderError {
                provider: "sync".to_string(),
                message: error,
            }),
            _ => unreachable!(),
        }
    }

    /// Atomically pushes metadata using CAS.
    pub async fn push_metadata_atomic(
        &mut self,
        metadata: CloudMetadata,
        expected_version: u64,
    ) -> Result<(), SyncError> {
        self.cmd_tx
            .send(SyncCommand::PushMetadataAtomic {
                metadata,
                expected_version,
            })
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send PushMetadataAtomic command".to_string(),
            })?;

        let event = self
            .wait_for_event(SYNC_TIMEOUT, |event| {
                matches!(event, SyncEvent::MetadataPushed | SyncEvent::Failed { .. })
            })
            .await?;

        match event {
            SyncEvent::MetadataPushed => Ok(()),
            SyncEvent::Failed { error, .. } => Err(SyncError::ProviderError {
                provider: "sync".to_string(),
                message: error,
            }),
            _ => unreachable!(),
        }
    }

    /// Checks cloud storage connectivity.
    ///
    /// Does NOT go through SyncTask — calls CloudStorage directly.
    ///
    /// # Returns
    /// - `(true, "OK")` on success
    /// - `(false, error_message)` on failure
    pub async fn test_connection(&self) -> Result<(bool, String), SyncError> {
        match self.storage.check_connectivity().await {
            Ok(()) => Ok((true, "OK".to_string())),
            Err(e) => Ok((false, e.to_string())),
        }
    }

    /// Checks cloud storage connectivity and returns promptly if cancelled.
    pub async fn test_connection_with_cancel(
        &self,
        cancel: CancellationToken,
    ) -> Result<(bool, String), SyncError> {
        tokio::select! {
            _ = cancel.cancelled() => Err(SyncError::Cancelled {
                operation: "sync_connection_test".to_string(),
            }),
            result = self.test_connection() => result,
        }
    }

    /// Pauses sync processing, waiting up to 30s for any in-progress Push to complete.
    ///
    /// Sends `Pause` command and waits for `Paused` event. If a sync cycle is
    /// in progress (e.g., Push stage), waits for the cycle to finish or the
    /// 30s timeout to expire.
    ///
    /// # Errors
    /// Returns `SyncError` if timeout (30s) expires before `Paused` event.
    pub async fn pause(&mut self) -> Result<(), SyncError> {
        self.cmd_tx
            .send(SyncCommand::Pause)
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send Pause command".to_string(),
            })?;

        let _event = self
            .wait_for_event(PAUSE_TIMEOUT, |event| matches!(event, SyncEvent::Paused))
            .await?;

        Ok(())
    }

    /// Resumes sync processing.
    ///
    /// Sends `Resume` command and waits for `Resumed` event.
    ///
    /// # Errors
    /// Returns `SyncError` if timeout (10s) expires before `Resumed` event.
    pub async fn resume(&mut self) -> Result<(), SyncError> {
        self.cmd_tx
            .send(SyncCommand::Resume)
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send Resume command".to_string(),
            })?;

        let _event = self
            .wait_for_event(RESUME_TIMEOUT, |event| matches!(event, SyncEvent::Resumed))
            .await?;

        Ok(())
    }

    /// Initiates graceful shutdown.
    ///
    /// Sends `Shutdown` command and waits for `ShutdownComplete` event.
    /// Then aborts the background task handle.
    ///
    /// # Errors
    /// Returns `SyncError` if timeout (10s) expires before `ShutdownComplete` event.
    pub async fn shutdown(mut self) -> Result<(), SyncError> {
        self.cmd_tx
            .send(SyncCommand::Shutdown)
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send Shutdown command".to_string(),
            })?;

        let _event = self
            .wait_for_event(RESUME_TIMEOUT, |event| {
                matches!(event, SyncEvent::ShutdownComplete)
            })
            .await?;

        self.task_handle.abort();
        Ok(())
    }

    /// Internal helper: waits for an event matching the predicate.
    ///
    /// Drains non-matching events (like `StateChanged`) and returns when
    /// either a matching event is found or the timeout expires.
    async fn wait_for_event<F>(
        &mut self,
        timeout: Duration,
        predicate: F,
    ) -> Result<SyncEvent, SyncError>
    where
        F: Fn(&SyncEvent) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(SyncError::NetworkTimeout {
                    message: format!("wait_for_event timed out after {:?}", timeout),
                });
            }

            match tokio::time::timeout(remaining, self.event_rx.recv()).await {
                Ok(Some(event)) => {
                    if predicate(&event) {
                        return Ok(event);
                    }
                    // Drain non-matching events (e.g., StateChanged)
                    continue;
                }
                Ok(None) => {
                    return Err(SyncError::ProviderError {
                        provider: "sync".to_string(),
                        message: "event channel closed unexpectedly".to_string(),
                    });
                }
                Err(_) => {
                    return Err(SyncError::NetworkTimeout {
                        message: format!("wait_for_event timed out after {:?}", timeout),
                    });
                }
            }
        }
    }

    async fn wait_for_event_or_cancel<F>(
        &mut self,
        timeout: Duration,
        cancel: CancellationToken,
        operation: &'static str,
        predicate: F,
    ) -> Result<SyncEvent, SyncError>
    where
        F: Fn(&SyncEvent) -> bool,
    {
        tokio::select! {
            result = self.wait_for_event(timeout, predicate) => result,
            _ = cancel.cancelled() => Err(SyncError::Cancelled {
                operation: operation.to_string(),
            }),
        }
    }
}

impl Drop for SyncService {
    fn drop(&mut self) {
        // Best-effort shutdown on drop
        let _ = self.cmd_tx.try_send(SyncCommand::Shutdown);
        self.task_handle.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_storage() -> CloudStorage {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        CloudStorage::new(op, "memory".to_string())
    }

    fn restore_test_record(record_id: String, now: String) -> CloudRecord {
        CloudRecord {
            id: record_id.clone(),
            version: 1,
            encrypted_data: "ZW5jcnlwdGVk".to_string(),
            nonce: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            dek_version: 1,
            aad: crate::cloud::record::AadFields {
                record_id,
                dek_version: 1,
            },
            metadata: crate::cloud::record::RecordMetadata {
                name: "Restored Login".to_string(),
                tags: Vec::new(),
                updated_at: now,
                credential_type: Some(crate::types::CredentialType::Login),
                is_favorite: Some(false),
                expires_at: None,
                updated_by: Some("test-device".to_string()),
                health: None,
            },
            deleted: Some(false),
            deleted_at: None,
        }
    }

    #[tokio::test]
    async fn new_creates_service() {
        let storage = create_test_storage();
        let svc = SyncService::new(storage);
        drop(svc);
    }

    #[tokio::test]
    async fn sync_returns_report() {
        let storage = create_test_storage();
        let mut svc = SyncService::new(storage);

        // sync() may return Completed or Failed depending on whether
        // the pipeline has data to work with. Both are acceptable.
        let result = svc.sync(None).await;
        assert!(
            result.is_ok(),
            "sync() should return Ok result, got: {:?}",
            result
        );

        // Clean shutdown
        let _ = svc.shutdown().await;
    }

    #[tokio::test]
    async fn sync_with_cancel_returns_cancelled_when_token_is_cancelled() {
        let storage = create_test_storage();
        let mut svc = SyncService::new(storage);
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel();

        let result = svc.sync_with_cancel(cancel, None).await;

        assert!(matches!(result, Err(SyncError::Cancelled { .. })));
    }

    #[tokio::test]
    async fn resolve_conflict_sends_command() {
        let storage = create_test_storage();
        let mut svc = SyncService::new(storage);

        let result = svc
            .resolve_conflict("test-record-id".to_string(), ResolutionStrategy::KeepLocal)
            .await;

        // Expect Ok since SyncTask handles ResolveConflict by immediately emitting ConflictResolved
        assert!(
            result.is_ok(),
            "resolve_conflict should return Ok, got: {:?}",
            result
        );

        // Clean shutdown
        let _ = svc.shutdown().await;
    }

    #[tokio::test]
    async fn resolve_all_sends_command() {
        let storage = create_test_storage();
        let mut svc = SyncService::new(storage);

        let result = svc
            .resolve_all_conflicts(ResolutionStrategy::KeepRemote)
            .await;

        // Expect Ok(0) since SyncTask handles ResolveAllConflicts by immediately emitting AllConflictsResolved
        assert!(
            result.is_ok(),
            "resolve_all_conflicts should return Ok, got: {:?}",
            result
        );

        // Clean shutdown
        let _ = svc.shutdown().await;
    }

    #[tokio::test]
    async fn pause_resume_cycle() {
        let storage = create_test_storage();
        let mut svc = SyncService::new(storage);

        let pause_result = svc.pause().await;
        assert!(
            pause_result.is_ok(),
            "pause() should return Ok, got: {:?}",
            pause_result
        );

        let resume_result = svc.resume().await;
        assert!(
            resume_result.is_ok(),
            "resume() should return Ok, got: {:?}",
            resume_result
        );

        // Clean shutdown
        let _ = svc.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_clean() {
        let storage = create_test_storage();
        let svc = SyncService::new(storage);

        let result = svc.shutdown().await;
        assert!(
            result.is_ok(),
            "shutdown should return Ok, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_connection_success() {
        let storage = create_test_storage();
        let svc = SyncService::new(storage);

        let (ok, message) = svc.test_connection().await.unwrap();
        assert!(ok, "test_connection should succeed for Memory backend");
        assert_eq!(message, "OK");

        // Clean shutdown
        drop(svc);
    }

    #[tokio::test]
    async fn restore_pull_only_downloads_remote_records_without_uploads() {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = CloudStorage::new(op.clone(), "memory".to_string());
        let record_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let record = restore_test_record(record_id.clone(), now.clone());
        let checksum = record.compute_checksum().unwrap();

        let mut metadata = CloudMetadata::new("remote-token".to_string());
        metadata.upsert_record(
            record_id.clone(),
            crate::cloud::metadata::RecordVersionInfo {
                version: 1,
                updated_at: now.clone(),
                updated_by: "test-device".to_string(),
                checksum,
                deleted: false,
            },
        );

        let metadata_json = crate::cloud::metadata::serialize_metadata(&metadata).unwrap();
        op.write(crate::cloud::schema::METADATA_FILENAME, metadata_json)
            .await
            .unwrap();
        op.write(
            &format!("{}/{}.json", crate::cloud::schema::RECORDS_DIR, record_id),
            serde_json::to_string(&record).unwrap(),
        )
        .await
        .unwrap();

        let mut service = SyncService::new(storage);
        let result = service.restore_pull_only().await.unwrap();

        assert_eq!(result.report.uploaded, 0);
        assert_eq!(result.report.downloaded, 1);
        assert_eq!(result.report.conflicts, 0);
        assert_eq!(result.report.failed, 0);
        assert_eq!(result.downloaded_records.len(), 1);
        assert_eq!(
            result
                .remote_metadata
                .as_ref()
                .unwrap()
                .vault_identity_token,
            "remote-token"
        );
    }

    #[tokio::test]
    async fn restore_pull_only_rejects_checksum_mismatch() {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = CloudStorage::new(op.clone(), "memory".to_string());
        let record_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let record = restore_test_record(record_id.clone(), now.clone());

        let mut metadata = CloudMetadata::new("remote-token".to_string());
        metadata.upsert_record(
            record_id.clone(),
            crate::cloud::metadata::RecordVersionInfo {
                version: 1,
                updated_at: now,
                updated_by: "test-device".to_string(),
                checksum: "wrong-checksum".to_string(),
                deleted: false,
            },
        );

        op.write(
            crate::cloud::schema::METADATA_FILENAME,
            crate::cloud::metadata::serialize_metadata(&metadata).unwrap(),
        )
        .await
        .unwrap();
        op.write(
            &format!("{}/{}.json", crate::cloud::schema::RECORDS_DIR, record_id),
            serde_json::to_string(&record).unwrap(),
        )
        .await
        .unwrap();

        let mut service = SyncService::new(storage);
        let result = service.restore_pull_only().await;

        assert!(matches!(
            result,
            Err(SyncError::ChecksumMismatch {
                record_id: id,
                ..
            }) if id == record_id
        ));
    }

    #[tokio::test]
    async fn restore_pull_only_rejects_record_version_mismatch() {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = CloudStorage::new(op.clone(), "memory".to_string());
        let record_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let record = restore_test_record(record_id.clone(), now.clone());
        let checksum = record.compute_checksum().unwrap();

        let mut metadata = CloudMetadata::new("remote-token".to_string());
        metadata.upsert_record(
            record_id.clone(),
            crate::cloud::metadata::RecordVersionInfo {
                version: record.version + 1,
                updated_at: now,
                updated_by: "test-device".to_string(),
                checksum,
                deleted: false,
            },
        );

        op.write(
            crate::cloud::schema::METADATA_FILENAME,
            crate::cloud::metadata::serialize_metadata(&metadata).unwrap(),
        )
        .await
        .unwrap();
        op.write(
            &format!("{}/{}.json", crate::cloud::schema::RECORDS_DIR, record_id),
            serde_json::to_string(&record).unwrap(),
        )
        .await
        .unwrap();

        let mut service = SyncService::new(storage);
        let result = service.restore_pull_only().await;

        assert!(matches!(
            result,
            Err(SyncError::MetadataVersionConflict {
                local: 2,
                remote: 1
            })
        ));
    }

    #[tokio::test]
    async fn restore_pull_only_rejects_invalid_record() {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = CloudStorage::new(op.clone(), "memory".to_string());
        let record_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let mut record = restore_test_record(record_id.clone(), now.clone());
        record.aad.record_id = Uuid::new_v4().to_string();
        let checksum = record.compute_checksum().unwrap();

        let mut metadata = CloudMetadata::new("remote-token".to_string());
        metadata.upsert_record(
            record_id.clone(),
            crate::cloud::metadata::RecordVersionInfo {
                version: 1,
                updated_at: now,
                updated_by: "test-device".to_string(),
                checksum,
                deleted: false,
            },
        );

        op.write(
            crate::cloud::schema::METADATA_FILENAME,
            crate::cloud::metadata::serialize_metadata(&metadata).unwrap(),
        )
        .await
        .unwrap();
        op.write(
            &format!("{}/{}.json", crate::cloud::schema::RECORDS_DIR, record_id),
            serde_json::to_string(&record).unwrap(),
        )
        .await
        .unwrap();

        let mut service = SyncService::new(storage);
        let result = service.restore_pull_only().await;

        assert!(matches!(
            result,
            Err(SyncError::AadInconsistent { field, .. }) if field == "record_id"
        ));
    }

    #[tokio::test]
    async fn restore_pull_only_rejects_invalid_nonce_before_local_apply() {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = CloudStorage::new(op.clone(), "memory".to_string());
        let record_id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let mut record = restore_test_record(record_id.clone(), now.clone());
        record.nonce = "bm9uY2U=".to_string();
        let checksum = record.compute_checksum().unwrap();

        let mut metadata = CloudMetadata::new("remote-token".to_string());
        metadata.upsert_record(
            record_id.clone(),
            crate::cloud::metadata::RecordVersionInfo {
                version: 1,
                updated_at: now,
                updated_by: "test-device".to_string(),
                checksum,
                deleted: false,
            },
        );

        op.write(
            crate::cloud::schema::METADATA_FILENAME,
            crate::cloud::metadata::serialize_metadata(&metadata).unwrap(),
        )
        .await
        .unwrap();
        op.write(
            &format!("{}/{}.json", crate::cloud::schema::RECORDS_DIR, record_id),
            serde_json::to_string(&record).unwrap(),
        )
        .await
        .unwrap();

        let mut service = SyncService::new(storage);
        let result = service.restore_pull_only().await;

        assert!(matches!(
            result,
            Err(SyncError::DeserializationFailed { message }) if message.contains("nonce")
        ));
    }

    #[tokio::test]
    async fn restore_pull_only_rejects_invalid_metadata_record_key() {
        let op = opendal::Operator::new(opendal::services::Memory::default())
            .unwrap()
            .finish();
        let storage = CloudStorage::new(op.clone(), "memory".to_string());
        let mut metadata = CloudMetadata::new("remote-token".to_string());
        metadata.upsert_record(
            "not-a-uuid".to_string(),
            crate::cloud::metadata::RecordVersionInfo {
                version: 1,
                updated_at: chrono::Utc::now().to_rfc3339(),
                updated_by: "test-device".to_string(),
                checksum: "unused".to_string(),
                deleted: false,
            },
        );

        op.write(
            crate::cloud::schema::METADATA_FILENAME,
            crate::cloud::metadata::serialize_metadata(&metadata).unwrap(),
        )
        .await
        .unwrap();

        let mut service = SyncService::new(storage);
        let result = service.restore_pull_only().await;

        assert!(matches!(
            result,
            Err(SyncError::DeserializationFailed { message })
                if message.contains("metadata record key") && message.contains("not-a-uuid")
        ));
    }
}
