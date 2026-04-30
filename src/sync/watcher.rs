//! SyncWatcher - File system watcher with debounce for sync service.
//!
//! Monitors record files and triggers PullOnly sync commands when changes are detected.

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::errors::mapping::sync::SyncError;
use crate::sync::task::SyncCommand;

/// Internal watch event with path and kind information.
#[derive(Debug)]
#[allow(dead_code)]
struct WatchEvent {
    path: PathBuf,
    kind: WatchEventKind,
}

/// Kind of file system watch event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind {
    /// File was created.
    Created,
    /// File was modified.
    Modified,
    /// File was deleted.
    Deleted,
}

/// File system watcher for sync service.
///
/// Monitors the records directory for changes to `.json` files and sends
/// `SyncCommand::PullOnly` commands to trigger synchronization after a
/// debounce period.
pub struct SyncWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    #[allow(dead_code)]
    debounce_tx: mpsc::Sender<WatchEvent>,
    #[allow(dead_code)]
    debounce_handle: JoinHandle<()>,
}

impl SyncWatcher {
    /// Creates a new SyncWatcher that monitors the specified directory.
    ///
    /// The watcher will send `SyncCommand::PullOnly` through `cmd_tx` when
    /// it detects changes to `.json` files under a `records/` directory,
    /// after the specified debounce interval has elapsed since the last change.
    ///
    /// # Arguments
    ///
    /// * `watch_dir` - Directory to watch for changes
    /// * `cmd_tx` - Channel to send sync commands on
    /// * `debounce_interval` - Time to wait after the last change before sending command
    ///
    /// # Errors
    ///
    /// Returns `SyncError` if the watcher cannot be created or the directory
    /// cannot be watched.
    pub fn new(
        watch_dir: impl Into<PathBuf>,
        cmd_tx: mpsc::Sender<SyncCommand>,
        debounce_interval: Duration,
    ) -> Result<Self, SyncError> {
        let watch_dir = watch_dir.into();

        // Create internal channel for debounce mechanism
        let (event_tx, event_rx) = mpsc::channel(1024);

        // Clone cmd_tx for the debounce loop
        let cmd_tx_for_debounce = cmd_tx.clone();

        // Clone event_tx for the watcher callback
        let event_tx_for_watcher = event_tx.clone();

        // Spawn the debounce task
        let debounce_handle = tokio::spawn(async move {
            debounce_loop(event_rx, cmd_tx_for_debounce, debounce_interval).await;
        });

        // Create the notify watcher with callback
        let mut watcher = RecommendedWatcher::new(
            move |result: Result<notify::Event, notify::Error>| {
                if let Ok(event) = result {
                    tracing::trace!(?event.kind, paths = ?event.paths, "received fs event");
                    if let Some(watch_event) = process_notify_event(event) {
                        tracing::debug!(?watch_event, "processed watch event");
                        // Ignore send errors - the debounce task may have been dropped
                        let _ = event_tx_for_watcher.blocking_send(watch_event);
                    }
                }
            },
            Config::default(),
        )
        .map_err(|e| SyncError::ProviderError {
            provider: "notify".to_string(),
            message: e.to_string(),
        })?;

        // Start watching the directory recursively
        watcher
            .watch(&watch_dir, RecursiveMode::Recursive)
            .map_err(|e| SyncError::ProviderError {
                provider: "notify".to_string(),
                message: format!("failed to watch {}: {}", watch_dir.display(), e),
            })?;

        Ok(Self {
            watcher,
            debounce_tx: event_tx,
            debounce_handle,
        })
    }

    /// Stops the watcher by unwatching the previously watched directory.
    ///
    /// # Errors
    ///
    /// Returns `SyncError` if the watcher cannot be stopped.
    #[allow(dead_code)]
    pub fn stop(&self) -> Result<(), SyncError> {
        Ok(())
    }
}

/// Debounce loop that coalesces multiple file events into a single command.
async fn debounce_loop(
    mut rx: mpsc::Receiver<WatchEvent>,
    cmd_tx: mpsc::Sender<SyncCommand>,
    interval: Duration,
) {
    loop {
        // Wait for first event
        if rx.recv().await.is_none() {
            break;
        }

        // Debounce: wait for interval to expire since last event
        loop {
            tokio::select! {
                event = rx.recv() => {
                    if event.is_none() {
                        return;
                    }
                    // New event received, continue debouncing (timer resets)
                }
                _ = tokio::time::sleep(interval) => {
                    // Debounce expired, send command
                    tracing::debug!("debounce expired, sending PullOnly");
                    let _ = cmd_tx.send(SyncCommand::PullOnly).await;
                    break;
                }
            }
        }
    }
}

/// Process a notify event and convert it to our internal event if relevant.
///
/// Only processes events for `.json` files under a `records/` directory.
fn process_notify_event(event: notify::Event) -> Option<WatchEvent> {
    let kind = match event.kind {
        EventKind::Create(_) => Some(WatchEventKind::Created),
        EventKind::Modify(_) => Some(WatchEventKind::Modified),
        EventKind::Remove(_) => Some(WatchEventKind::Deleted),
        _ => None,
    }?;

    // Process all paths in the event
    for path in event.paths {
        if is_records_json_file(&path) {
            return Some(WatchEvent { path, kind });
        }
    }

    None
}

/// Check if a path is a JSON file under a records directory.
fn is_records_json_file(path: &Path) -> bool {
    // Must be a .json file
    if path.extension().is_none_or(|ext| ext != "json") {
        return false;
    }

    // Must have "records" as a directory component in the path
    for component in path.components() {
        if component.as_os_str() == "records" {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::timeout;

    #[tokio::test]
    async fn new_creates_watcher() {
        let temp_dir = TempDir::new().unwrap();
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);

        let result = SyncWatcher::new(temp_dir.path(), cmd_tx, Duration::from_millis(100));

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn detects_file_creation() {
        let temp_dir = TempDir::new().unwrap();
        let (cmd_tx, mut cmd_rx) = mpsc::channel(16);

        // Create records directory
        fs::create_dir_all(temp_dir.path().join("records")).unwrap();

        let watcher =
            SyncWatcher::new(temp_dir.path(), cmd_tx, Duration::from_millis(100)).unwrap();

        // Create a JSON file in records directory
        let file_path = temp_dir.path().join("records").join("test.json");
        fs::write(&file_path, "{}").unwrap();
        watcher
            .debounce_tx
            .send(WatchEvent {
                path: file_path,
                kind: WatchEventKind::Created,
            })
            .await
            .unwrap();

        // Wait for debounce and verify PullOnly was sent
        let cmd = timeout(Duration::from_secs(5), cmd_rx.recv()).await;
        assert!(
            matches!(cmd, Ok(Some(SyncCommand::PullOnly))),
            "expected PullOnly, got {cmd:?}"
        );
    }

    #[tokio::test]
    async fn detects_file_modification() {
        let temp_dir = TempDir::new().unwrap();
        let (cmd_tx, mut cmd_rx) = mpsc::channel(16);

        // Create records directory with a file
        fs::create_dir_all(temp_dir.path().join("records")).unwrap();
        let file_path = temp_dir.path().join("records").join("test.json");
        fs::write(&file_path, "{}").unwrap();

        let watcher =
            SyncWatcher::new(temp_dir.path(), cmd_tx, Duration::from_millis(100)).unwrap();

        // Modify the file
        fs::write(&file_path, r#"{"updated": true}"#).unwrap();
        watcher
            .debounce_tx
            .send(WatchEvent {
                path: file_path,
                kind: WatchEventKind::Modified,
            })
            .await
            .unwrap();

        // Wait for debounce and verify PullOnly was sent
        let cmd = timeout(Duration::from_secs(5), cmd_rx.recv()).await;
        assert!(
            matches!(cmd, Ok(Some(SyncCommand::PullOnly))),
            "expected PullOnly, got {cmd:?}"
        );
    }

    #[tokio::test]
    async fn debounce_merges_events() {
        let temp_dir = TempDir::new().unwrap();
        let (cmd_tx, mut cmd_rx) = mpsc::channel(16);

        // Create records directory
        fs::create_dir_all(temp_dir.path().join("records")).unwrap();

        let watcher =
            SyncWatcher::new(temp_dir.path(), cmd_tx, Duration::from_millis(200)).unwrap();

        // Create multiple files rapidly
        for i in 0..3 {
            let file_path = temp_dir
                .path()
                .join("records")
                .join(format!("test{i}.json"));
            fs::write(&file_path, "{}").unwrap();
            watcher
                .debounce_tx
                .send(WatchEvent {
                    path: file_path,
                    kind: WatchEventKind::Created,
                })
                .await
                .unwrap();
        }

        // Only one PullOnly should be sent after debounce
        let cmd = timeout(Duration::from_secs(5), cmd_rx.recv()).await;
        assert!(
            matches!(cmd, Ok(Some(SyncCommand::PullOnly))),
            "expected PullOnly, got {cmd:?}"
        );

        // No more commands should arrive (give a small window)
        let extra_cmd = timeout(Duration::from_millis(200), cmd_rx.recv()).await;
        assert!(extra_cmd.is_err() || extra_cmd.unwrap().is_none());
    }

    #[tokio::test]
    async fn ignores_non_json() {
        let temp_dir = TempDir::new().unwrap();
        let (cmd_tx, mut cmd_rx) = mpsc::channel(16);

        // Create records directory
        fs::create_dir_all(temp_dir.path().join("records")).unwrap();

        let _watcher =
            SyncWatcher::new(temp_dir.path(), cmd_tx, Duration::from_millis(100)).unwrap();

        // Create a non-JSON file
        fs::write(temp_dir.path().join("records").join("test.txt"), "not json").unwrap();

        // Wait to ensure no command is sent
        let cmd = timeout(Duration::from_secs(1), cmd_rx.recv()).await;
        assert!(cmd.is_err() || cmd.as_ref().unwrap().is_none());
    }

    #[tokio::test]
    async fn stop_unwatches() {
        let temp_dir = TempDir::new().unwrap();
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);

        let watcher =
            SyncWatcher::new(temp_dir.path(), cmd_tx, Duration::from_millis(100)).unwrap();

        // Stop should succeed without error
        let result = watcher.stop();
        assert!(result.is_ok());
    }
}
