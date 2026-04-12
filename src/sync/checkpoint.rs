//! Sync checkpoint for tracking sync progress across phases.
//!
//! Provides atomic persistence of sync state using write-to-temp-then-rename
//! pattern to ensure no partial/corrupt files on crash.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::mapping::sync::SyncError;

/// Checkpoint file name in vault directory.
const CHECKPOINT_FILENAME: &str = ".sync_checkpoint.json";
/// Temp file suffix used during atomic write.
const TEMP_SUFFIX: &str = ".tmp";

/// Conflict pending resolution between local and remote versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingConflict {
    pub record_id: Uuid,
    pub local_version: u64,
    pub remote_version: u64,
}

/// Sync checkpoint tracking completed phases and pending conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCheckpoint {
    /// Vault directory path for checkpoint file location.
    #[serde(skip)]
    vault_dir: PathBuf,

    /// Record IDs that have been successfully pushed to cloud.
    #[serde(default)]
    pub push_completed_ids: HashSet<Uuid>,

    /// Record IDs that have been successfully pulled from cloud.
    #[serde(default)]
    pub pull_completed_ids: HashSet<Uuid>,

    /// Whether the detect phase has completed.
    #[serde(default)]
    pub detect_completed: bool,

    /// Conflicts pending resolution.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_conflicts: Vec<PendingConflict>,
}

impl SyncCheckpoint {
    /// Checkpoint file path for a given vault directory.
    fn checkpoint_path(vault_dir: &Path) -> PathBuf {
        vault_dir.join(CHECKPOINT_FILENAME)
    }

    /// Temp file path for atomic write.
    fn temp_path(vault_dir: &Path) -> PathBuf {
        vault_dir.join(format!("{}{}", CHECKPOINT_FILENAME, TEMP_SUFFIX))
    }

    /// Creates a new empty checkpoint for the given vault directory.
    pub fn new(vault_dir: impl Into<PathBuf>) -> Self {
        Self {
            vault_dir: vault_dir.into(),
            push_completed_ids: HashSet::new(),
            pull_completed_ids: HashSet::new(),
            detect_completed: false,
            pending_conflicts: Vec::new(),
        }
    }

    /// Returns true if all sync phases are complete with no pending conflicts.
    pub fn is_complete(&self) -> bool {
        self.detect_completed && self.pending_conflicts.is_empty()
    }

    /// Records that a record has been successfully pushed to cloud.
    pub fn record_push_done(&mut self, id: Uuid) {
        self.push_completed_ids.insert(id);
    }

    /// Records that a record has been successfully pulled from cloud.
    pub fn record_pull_done(&mut self, id: Uuid) {
        self.pull_completed_ids.insert(id);
    }

    /// Adds a conflict pending resolution.
    pub fn add_pending_conflict(&mut self, conflict: PendingConflict) {
        self.pending_conflicts.push(conflict);
    }

    /// Resolves a conflict by removing it from pending_conflicts.
    /// Returns true if the conflict was found and removed, false if not found.
    pub fn resolve_conflict(&mut self, record_id: Uuid) -> bool {
        let initial_len = self.pending_conflicts.len();
        self.pending_conflicts.retain(|c| c.record_id != record_id);
        self.pending_conflicts.len() < initial_len
    }

    /// Saves checkpoint atomically: write to temp file then rename.
    /// This ensures no partial/corrupt file on crash.
    pub fn save(&self) -> Result<(), SyncError> {
        let temp_path = Self::temp_path(&self.vault_dir);
        let final_path = Self::checkpoint_path(&self.vault_dir);

        // Serialize to JSON
        let json =
            serde_json::to_string_pretty(self).map_err(|e| SyncError::SerializationFailed {
                message: e.to_string(),
            })?;

        // Write to temp file
        std::fs::write(&temp_path, json.as_bytes()).map_err(|e| {
            SyncError::SerializationFailed {
                message: format!("failed to write temp checkpoint: {}", e),
            }
        })?;

        // Atomic rename
        std::fs::rename(&temp_path, &final_path).map_err(|e| SyncError::SerializationFailed {
            message: format!("failed to rename temp to final checkpoint: {}", e),
        })?;

        Ok(())
    }

    /// Loads checkpoint from vault directory.
    /// Returns error if file exists but is corrupt or cannot be read.
    pub fn load(vault_dir: &Path) -> Result<Self, SyncError> {
        let path = Self::checkpoint_path(vault_dir);
        let json = std::fs::read_to_string(&path).map_err(|e| SyncError::SerializationFailed {
            message: format!("failed to read checkpoint: {}", e),
        })?;

        let mut checkpoint: SyncCheckpoint =
            serde_json::from_str(&json).map_err(|e| SyncError::DeserializationFailed {
                message: format!("checkpoint corrupted: {}", e),
            })?;

        checkpoint.vault_dir = vault_dir.to_path_buf();
        Ok(checkpoint)
    }

    /// Cleans up checkpoint file after successful sync completion.
    /// Also removes any leftover temp file.
    pub fn cleanup(vault_dir: &Path) -> Result<(), SyncError> {
        let final_path = Self::checkpoint_path(vault_dir);
        let temp_path = Self::temp_path(vault_dir);

        if final_path.exists() {
            std::fs::remove_file(&final_path).map_err(|e| SyncError::SerializationFailed {
                message: format!("failed to remove checkpoint: {}", e),
            })?;
        }

        if temp_path.exists() {
            std::fs::remove_file(&temp_path).map_err(|e| SyncError::SerializationFailed {
                message: format!("failed to remove temp checkpoint: {}", e),
            })?;
        }

        Ok(())
    }

    /// Loads checkpoint if exists and valid, otherwise returns new empty checkpoint.
    /// Never panics on corrupted data — logs warning and returns fresh checkpoint.
    pub fn from_corrupted_or_none(vault_dir: &Path) -> Self {
        let path = Self::checkpoint_path(vault_dir);

        if !path.exists() {
            return Self::new(vault_dir);
        }

        match Self::load(vault_dir) {
            Ok(checkpoint) => checkpoint,
            Err(e) => {
                tracing::warn!("checkpoint corrupted, starting fresh: {}", e);
                Self::new(vault_dir)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_new_creates_empty_checkpoint() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = SyncCheckpoint::new(temp_dir.path());

        assert!(checkpoint.push_completed_ids.is_empty());
        assert!(checkpoint.pull_completed_ids.is_empty());
        assert!(!checkpoint.detect_completed);
        assert!(checkpoint.pending_conflicts.is_empty());
        assert!(!checkpoint.is_complete());
    }

    #[test]
    fn test_record_push_done_adds_id() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
        let id = Uuid::new_v4();

        checkpoint.record_push_done(id);

        assert!(checkpoint.push_completed_ids.contains(&id));
        assert!(!checkpoint.pull_completed_ids.contains(&id));
    }

    #[test]
    fn test_record_pull_done_adds_id() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
        let id = Uuid::new_v4();

        checkpoint.record_pull_done(id);

        assert!(!checkpoint.push_completed_ids.contains(&id));
        assert!(checkpoint.pull_completed_ids.contains(&id));
    }

    #[test]
    fn test_is_complete_false_by_default() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = SyncCheckpoint::new(temp_dir.path());

        assert!(!checkpoint.is_complete());
    }

    #[test]
    fn test_is_complete_true_when_detect_done_and_no_conflicts() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
        checkpoint.detect_completed = true;

        assert!(checkpoint.is_complete());
    }

    #[test]
    fn test_is_complete_false_with_pending_conflicts() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
        checkpoint.detect_completed = true;
        checkpoint.add_pending_conflict(PendingConflict {
            record_id: Uuid::new_v4(),
            local_version: 1,
            remote_version: 2,
        });

        assert!(!checkpoint.is_complete());
    }

    #[test]
    fn test_add_pending_conflict() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
        let conflict = PendingConflict {
            record_id: Uuid::new_v4(),
            local_version: 1,
            remote_version: 2,
        };

        checkpoint.add_pending_conflict(conflict.clone());

        assert_eq!(checkpoint.pending_conflicts.len(), 1);
        assert_eq!(
            checkpoint.pending_conflicts[0].record_id,
            conflict.record_id
        );
    }

    #[test]
    fn test_resolve_conflict_removes_conflict() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
        let conflict = PendingConflict {
            record_id: Uuid::new_v4(),
            local_version: 1,
            remote_version: 2,
        };
        checkpoint.add_pending_conflict(conflict.clone());

        let result = checkpoint.resolve_conflict(conflict.record_id);

        assert!(result);
        assert!(checkpoint.pending_conflicts.is_empty());
    }

    #[test]
    fn test_resolve_conflict_not_found_returns_false() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());

        let result = checkpoint.resolve_conflict(Uuid::new_v4());

        assert!(!result);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        checkpoint.record_push_done(id1);
        checkpoint.record_pull_done(id2);
        checkpoint.detect_completed = true;
        checkpoint.add_pending_conflict(PendingConflict {
            record_id: Uuid::new_v4(),
            local_version: 3,
            remote_version: 4,
        });

        checkpoint.save().unwrap();

        let loaded = SyncCheckpoint::load(temp_dir.path()).unwrap();

        assert!(loaded.push_completed_ids.contains(&id1));
        assert!(loaded.pull_completed_ids.contains(&id2));
        assert!(loaded.detect_completed);
        assert_eq!(loaded.pending_conflicts.len(), 1);
    }

    #[test]
    fn test_save_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = SyncCheckpoint::new(temp_dir.path());
        let path = temp_dir.path().join(".sync_checkpoint.json");

        assert!(!path.exists());

        checkpoint.save().unwrap();

        assert!(path.exists());
    }

    #[test]
    fn test_atomic_write_no_residual_temp() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = SyncCheckpoint::new(temp_dir.path());
        let temp_path = temp_dir.path().join(".sync_checkpoint.json.tmp");

        assert!(!temp_path.exists());

        checkpoint.save().unwrap();

        assert!(!temp_path.exists());
    }

    #[test]
    fn test_cleanup_deletes_file() {
        let temp_dir = TempDir::new().unwrap();
        let checkpoint = SyncCheckpoint::new(temp_dir.path());
        let path = temp_dir.path().join(".sync_checkpoint.json");

        checkpoint.save().unwrap();
        assert!(path.exists());

        SyncCheckpoint::cleanup(temp_dir.path()).unwrap();

        assert!(!path.exists());
    }

    #[test]
    fn test_cleanup_deletes_temp_file() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().join(".sync_checkpoint.json.tmp");

        std::fs::write(&temp_path, b"temp").unwrap();
        assert!(temp_path.exists());

        SyncCheckpoint::cleanup(temp_dir.path()).unwrap();

        assert!(!temp_path.exists());
    }

    #[test]
    fn test_from_corrupted_or_none_no_file() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join(".sync_checkpoint.json");

        assert!(!path.exists());

        let checkpoint = SyncCheckpoint::from_corrupted_or_none(temp_dir.path());

        assert!(checkpoint.push_completed_ids.is_empty());
        assert!(checkpoint.pull_completed_ids.is_empty());
    }

    #[test]
    fn test_from_corrupted_or_none_corrupted_json() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join(".sync_checkpoint.json");

        std::fs::write(&path, b"not valid json {{{").unwrap();

        let checkpoint = SyncCheckpoint::from_corrupted_or_none(temp_dir.path());

        assert!(checkpoint.push_completed_ids.is_empty());
        assert!(checkpoint.pending_conflicts.is_empty());
    }

    #[test]
    fn test_from_corrupted_or_none_valid_file() {
        let temp_dir = TempDir::new().unwrap();
        let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
        let id = Uuid::new_v4();
        checkpoint.record_push_done(id);
        checkpoint.save().unwrap();

        let loaded = SyncCheckpoint::from_corrupted_or_none(temp_dir.path());

        assert!(loaded.push_completed_ids.contains(&id));
    }
}
