//! SyncService - Public interface for cloud sync operations.
//!
//! A thin wrapper around SyncTask's channel-based command interface.
//! Provides async methods that send commands and wait for corresponding events.

use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::cloud::CloudStorage;
use crate::errors::mapping::sync::SyncError;
use crate::sync::conflict::ResolutionStrategy;
use crate::sync::state_machine::SyncStateMachine;
use crate::sync::task::{SyncCommand, SyncEvent, SyncReport, SyncTask};

/// Channel capacity for command and event queues.
const CHANNEL_CAPACITY: usize = 16;

/// SyncService timeout durations.
const SYNC_TIMEOUT: Duration = Duration::from_secs(60);
const CONFLICT_TIMEOUT: Duration = Duration::from_secs(30);
const PAUSE_RESUME_TIMEOUT: Duration = Duration::from_secs(10);

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
    pub async fn sync(&mut self) -> Result<SyncReport, SyncError> {
        self.cmd_tx
            .send(SyncCommand::TriggerSync)
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send TriggerSync command".to_string(),
            })?;

        let event = self
            .wait_for_event(SYNC_TIMEOUT, |event| {
                matches!(event, SyncEvent::Completed(_) | SyncEvent::Failed { .. })
            })
            .await?;

        match event {
            SyncEvent::Completed(report) => Ok(report),
            SyncEvent::Failed { error, state: _ } => Err(SyncError::ProviderError {
                provider: "sync".to_string(),
                message: error,
            }),
            _ => unreachable!(),
        }
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

    /// Pauses sync processing.
    ///
    /// Sends `Pause` command and waits for `Paused` event.
    ///
    /// # Errors
    /// Returns `SyncError` if timeout (10s) expires before `Paused` event.
    pub async fn pause(&mut self) -> Result<(), SyncError> {
        self.cmd_tx
            .send(SyncCommand::Pause)
            .await
            .map_err(|_| SyncError::ProviderError {
                provider: "sync".to_string(),
                message: "failed to send Pause command".to_string(),
            })?;

        let _event = self
            .wait_for_event(PAUSE_RESUME_TIMEOUT, |event| {
                matches!(event, SyncEvent::Paused)
            })
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
            .wait_for_event(PAUSE_RESUME_TIMEOUT, |event| {
                matches!(event, SyncEvent::Resumed)
            })
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
            .wait_for_event(PAUSE_RESUME_TIMEOUT, |event| {
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
        let result = svc.sync().await;
        assert!(
            result.is_ok(),
            "sync() should return Ok result, got: {:?}",
            result
        );

        // Clean shutdown
        let _ = svc.shutdown().await;
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
}
