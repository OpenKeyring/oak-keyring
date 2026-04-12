//! Integration tests for SyncCheckpoint with full file system isolation.

use oak_keyring::sync::{PendingConflict, SyncCheckpoint};
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn save_load_roundtrip() {
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
    assert_eq!(loaded.pending_conflicts[0].local_version, 3);
    assert_eq!(loaded.pending_conflicts[0].remote_version, 4);
}

#[test]
fn save_creates_file() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint = SyncCheckpoint::new(temp_dir.path());

    checkpoint.save().unwrap();

    let loaded = SyncCheckpoint::load(temp_dir.path()).unwrap();
    assert!(loaded.push_completed_ids.is_empty());
}

#[test]
fn cleanup_deletes_file() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint = SyncCheckpoint::new(temp_dir.path());

    checkpoint.save().unwrap();

    SyncCheckpoint::cleanup(temp_dir.path()).unwrap();

    let result = SyncCheckpoint::load(temp_dir.path());
    assert!(result.is_err());
}

#[test]
fn cleanup_deletes_temp_file() {
    let temp_dir = TempDir::new().unwrap();

    SyncCheckpoint::cleanup(temp_dir.path()).unwrap();
}

#[test]
fn from_corrupted_or_none_no_file() {
    let temp_dir = TempDir::new().unwrap();

    let checkpoint = SyncCheckpoint::from_corrupted_or_none(temp_dir.path());

    assert!(checkpoint.push_completed_ids.is_empty());
    assert!(checkpoint.pull_completed_ids.is_empty());
    assert!(!checkpoint.detect_completed);
    assert!(checkpoint.pending_conflicts.is_empty());
}

#[test]
fn from_corrupted_or_none_corrupted_json() {
    let temp_dir = TempDir::new().unwrap();
    let path = temp_dir.path().join(".sync_checkpoint.json");

    std::fs::write(&path, b"not valid json {{{").unwrap();

    let checkpoint = SyncCheckpoint::from_corrupted_or_none(temp_dir.path());

    assert!(checkpoint.push_completed_ids.is_empty());
    assert!(checkpoint.pending_conflicts.is_empty());
}

#[test]
fn from_corrupted_or_none_valid_file() {
    let temp_dir = TempDir::new().unwrap();
    let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
    let id = Uuid::new_v4();
    checkpoint.record_push_done(id);
    checkpoint.save().unwrap();

    let loaded = SyncCheckpoint::from_corrupted_or_none(temp_dir.path());

    assert!(loaded.push_completed_ids.contains(&id));
}

#[test]
fn is_complete() {
    let temp_dir = TempDir::new().unwrap();
    let mut checkpoint = SyncCheckpoint::new(temp_dir.path());

    assert!(!checkpoint.is_complete());

    checkpoint.detect_completed = true;

    assert!(checkpoint.is_complete());
}

#[test]
fn is_complete_false_with_conflicts() {
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
fn record_push_done_and_pull_done() {
    let temp_dir = TempDir::new().unwrap();
    let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
    let push_id = Uuid::new_v4();
    let pull_id = Uuid::new_v4();

    checkpoint.record_push_done(push_id);
    checkpoint.record_pull_done(pull_id);

    assert!(checkpoint.push_completed_ids.contains(&push_id));
    assert!(!checkpoint.push_completed_ids.contains(&pull_id));
    assert!(checkpoint.pull_completed_ids.contains(&pull_id));
    assert!(!checkpoint.pull_completed_ids.contains(&push_id));
}

#[test]
fn resolve_conflict() {
    let temp_dir = TempDir::new().unwrap();
    let mut checkpoint = SyncCheckpoint::new(temp_dir.path());
    let conflict_id = Uuid::new_v4();
    checkpoint.add_pending_conflict(PendingConflict {
        record_id: conflict_id,
        local_version: 1,
        remote_version: 2,
    });

    assert_eq!(checkpoint.pending_conflicts.len(), 1);

    let result = checkpoint.resolve_conflict(conflict_id);

    assert!(result);
    assert!(checkpoint.pending_conflicts.is_empty());
}

#[test]
fn resolve_conflict_not_found() {
    let temp_dir = TempDir::new().unwrap();
    let mut checkpoint = SyncCheckpoint::new(temp_dir.path());

    let result = checkpoint.resolve_conflict(Uuid::new_v4());

    assert!(!result);
}

#[test]
fn atomic_write_no_residual_temp() {
    let temp_dir = TempDir::new().unwrap();
    let checkpoint = SyncCheckpoint::new(temp_dir.path());
    let temp_path = temp_dir.path().join(".sync_checkpoint.json.tmp");

    checkpoint.save().unwrap();

    assert!(!temp_path.exists());
}
