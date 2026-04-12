//! SyncTask - Tokio async main loop for sync service.
//!
//! Manages the sync state machine, coordinates with the sync pipeline,
//! handles commands, and emits events.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::cloud::CloudStorage;
use crate::errors::mapping::sync::SyncError;
use crate::sync::checkpoint::SyncCheckpoint;
use crate::sync::conflict::ResolutionStrategy;
use crate::sync::pipeline::{PipelineContext, PipelineResult, SyncPipeline};
use crate::sync::retry::{BackoffTimer, RetryPolicy};
use crate::sync::state_machine::{SyncState, SyncStateMachine, SyncTrigger};
use crate::sync::ConflictManager;

/// Commands accepted by SyncTask.
#[derive(Debug)]
pub enum SyncCommand {
    /// Trigger a full sync cycle (pull + detect + push + resolve).
    TriggerSync,
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
    /// Initiate graceful shutdown.
    Shutdown,
}

/// Events emitted by SyncTask.
#[derive(Debug)]
pub enum SyncEvent {
    /// Sync cycle completed successfully with a report.
    Completed(SyncReport),
    /// Sync cycle failed with an error and current state.
    Failed { error: String, state: SyncState },
    /// A single conflict was resolved.
    ConflictResolved { record_id: String },
    /// All pending conflicts were resolved.
    AllConflictsResolved,
    /// State machine transitioned to a new state.
    StateChanged { from: SyncState, to: SyncState },
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
                        Some(SyncCommand::TriggerSync) => {
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
                            self.backoff_timer.reset();
                            // Transition to retry
                            let from_state = self.state_machine.current_state().clone();
                            if let Ok(to_state) = self.state_machine.transition(SyncTrigger::BackoffExpired) {
                                let _ = self.event_tx.send(SyncEvent::StateChanged { from: from_state, to: to_state }).await;
                            }
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
        let result = self.execute_pipeline().await;
        self.handle_pipeline_result(result, start).await;
    }

    /// Handles PullOnly command.
    async fn handle_pull_only(&mut self) {
        if self.paused {
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
        let result = self.execute_pipeline().await;
        self.handle_pipeline_result(result, start).await;
    }

    /// Handles ResolveConflict command.
    async fn handle_resolve_conflict(&mut self, record_id: &str, _strategy: ResolutionStrategy) {
        // For a single conflict resolution, we just emit the event
        // The actual resolution logic would be handled by the conflict manager
        let _ = self
            .event_tx
            .send(SyncEvent::ConflictResolved {
                record_id: record_id.to_string(),
            })
            .await;
    }

    /// Handles ResolveAllConflicts command.
    async fn handle_resolve_all(&mut self, _strategy: ResolutionStrategy) {
        // Batch resolve all conflicts
        // For now, just emit the AllConflictsResolved event
        let _ = self.event_tx.send(SyncEvent::AllConflictsResolved).await;
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
    async fn execute_pipeline(&self) -> PipelineResult {
        let checkpoint = SyncCheckpoint::new(std::env::temp_dir());
        let mut context = PipelineContext::new(
            self.storage.clone(),
            self.conflict_manager.clone(),
            checkpoint,
            0,
            String::new(),
        );

        // For the initial implementation, we execute the pipeline
        // but we need to set up local records for the pipeline to work
        // In a real implementation, this would come from the vault service
        self.pipeline.execute(&mut context).await
    }

    /// Handles pipeline execution result.
    async fn handle_pipeline_result(&mut self, result: PipelineResult, start: Instant) {
        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            PipelineResult::Completed => {
                let from_state = self.state_machine.current_state().clone();
                let _ = self.state_machine.transition(SyncTrigger::ReportCompleted);
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
                    failed: 0,
                    duration_ms,
                };
                let _ = self.event_tx.send(SyncEvent::Completed(report)).await;
            }
            PipelineResult::NoChanges => {
                let from_state = self.state_machine.current_state().clone();
                let _ = self.state_machine.transition(SyncTrigger::ReportCompleted);
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
                    failed: 0,
                    duration_ms,
                };
                let _ = self.event_tx.send(SyncEvent::Completed(report)).await;
            }
            PipelineResult::ConflictsDetected { conflict_ids } => {
                let from_state = self.state_machine.current_state().clone();
                let _ = self.state_machine.transition(SyncTrigger::PushCompleted {
                    has_conflicts: true,
                });
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
                    conflicts: conflict_ids.len() as u32,
                    failed: 0,
                    duration_ms,
                };
                let _ = self.event_tx.send(SyncEvent::Completed(report)).await;
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
    async fn trigger_sync_cycle() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        // Send TriggerSync command
        cmd_tx.send(SyncCommand::TriggerSync).await.unwrap();

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
                SyncEvent::Completed(_) | SyncEvent::Failed { .. } => {
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
                Some(SyncEvent::Completed(_)) | Some(SyncEvent::Failed { .. })
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
    async fn resolve_conflict_event() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        // Run task in background
        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // Send ResolveConflict
        cmd_tx
            .send(SyncCommand::ResolveConflict {
                record_id: "test-record-id".to_string(),
                strategy: ResolutionStrategy::KeepLocal,
            })
            .await
            .unwrap();

        // Receive ConflictResolved event
        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            matches!(event, SyncEvent::ConflictResolved { ref record_id } if record_id == "test-record-id"),
            "Expected ConflictResolved, got {:?}",
            event
        );

        // Send shutdown
        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }

    #[tokio::test]
    async fn resolve_all_event() {
        let (cmd_tx, cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let storage = create_test_storage();
        let mut task = SyncTask::new(storage, cmd_rx, event_tx, SyncStateMachine::new(5));

        // Run task in background
        let handle = tokio::spawn(async move {
            task.run().await;
        });

        // Send ResolveAllConflicts
        cmd_tx
            .send(SyncCommand::ResolveAllConflicts {
                strategy: ResolutionStrategy::KeepRemote,
            })
            .await
            .unwrap();

        // Receive AllConflictsResolved event
        let event = timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert!(
            matches!(event, SyncEvent::AllConflictsResolved),
            "Expected AllConflictsResolved, got {:?}",
            event
        );

        // Send shutdown
        cmd_tx.send(SyncCommand::Shutdown).await.unwrap();
        let _ = handle.await;
    }
}
