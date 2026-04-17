//! Sync UI state types (U10).
//!
//! UI-layer state for the sync status indicator (status bar) and the
//! conflict resolution screen. These types are used exclusively by the
//! TUI layer and do not map directly to service-layer types.

use chrono::{DateTime, Utc};
use uuid::Uuid;

// ── Sync Status Indicator ────────────────────────────────────────────────────

/// Sync status for display in the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncDisplayStatus {
    #[default]
    NotConfigured,
    Synced,
    Syncing,
    Failed,
    Offline,
    Rotating,
}

impl SyncDisplayStatus {
    /// Icon glyph for this status.
    pub fn icon(&self) -> &'static str {
        match self {
            SyncDisplayStatus::Synced => "\u{2713}",
            SyncDisplayStatus::Syncing => "\u{27F3}",
            SyncDisplayStatus::Failed => "\u{2717}",
            SyncDisplayStatus::NotConfigured => "\u{2014}",
            SyncDisplayStatus::Offline => "\u{25D0}",
            SyncDisplayStatus::Rotating => "\u{27F2}",
        }
    }
}

/// Sync progress info for ongoing operations.
#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub current: usize,
    pub total: usize,
}

/// Sync indicator state for the status bar.
#[derive(Debug, Clone, Default)]
pub struct SyncIndicatorState {
    /// Current sync display status.
    pub status: SyncDisplayStatus,
    /// Timestamp of last successful sync.
    pub last_sync: Option<DateTime<Utc>>,
    /// Progress info when syncing or rotating.
    pub progress: Option<SyncProgress>,
    /// Error message shown when status is Failed.
    pub error_message: Option<String>,
    /// Whether the detail popup is visible in the status bar.
    pub detail_visible: bool,
    /// Timer countdown (in ticks) for auto-hiding the detail popup.
    pub detail_timer: Option<usize>,
    /// Current animation frame index for the syncing spinner.
    pub animation_frame: usize,
}

// ── Conflict Resolution ──────────────────────────────────────────────────────

/// Conflict resolution screen state.
#[derive(Debug, Clone, Default)]
pub struct ConflictResolutionState {
    /// List of conflicts to resolve.
    pub conflicts: Vec<ConflictDisplay>,
    /// Index of the currently displayed conflict.
    pub current_index: usize,
    /// Which side of the conflict view is focused.
    pub focused_side: ConflictSide,
}

/// Display info for a single conflict.
#[derive(Debug, Clone)]
pub struct ConflictDisplay {
    /// Record identifier.
    pub record_id: Uuid,
    /// Record name for display.
    pub record_name: String,
    /// Local version field values.
    pub local_fields: Vec<ConflictField>,
    /// Remote version field values.
    pub remote_fields: Vec<ConflictField>,
    /// Timestamp of the local modification.
    pub local_time: DateTime<Utc>,
    /// Timestamp of the remote modification.
    pub remote_time: DateTime<Utc>,
}

/// A single field in conflict display.
#[derive(Debug, Clone)]
pub struct ConflictField {
    /// Field label (e.g. "Username", "Password").
    pub label: String,
    /// Field value (may be masked).
    pub value: String,
    /// Whether this field differs between local and remote.
    pub differs: bool,
    /// Whether this field contains sensitive data.
    pub is_sensitive: bool,
    /// Whether the value is currently masked (hidden).
    pub is_masked: bool,
}

/// Which side of the conflict view is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictSide {
    #[default]
    Local,
    Remote,
}

// ── Sync Queue (defer during editing) ────────────────────────────────────────

/// State for deferring sync results during active editing.
///
/// When the user is editing a record, incoming sync data is queued rather than
/// applied immediately to avoid overwriting the user's work. Once editing ends,
/// the queued data is processed for potential conflicts.
#[derive(Debug, Clone, Default)]
pub struct SyncQueueState {
    /// The record currently being edited, if any.
    pub editing_record_id: Option<Uuid>,
    /// Pending sync data queued while editing.
    pub pending_sync_data: Vec<String>,
}

impl SyncQueueState {
    /// Returns true if no record is being edited.
    pub fn is_idle(&self) -> bool {
        self.editing_record_id.is_none()
    }

    /// Returns true if a record is being edited.
    pub fn is_editing(&self) -> bool {
        self.editing_record_id.is_some()
    }

    /// Enter editing mode for the given record.
    pub fn enter_editing(&mut self, record_id: Uuid) {
        self.editing_record_id = Some(record_id);
    }

    /// Enqueue pending sync data (only while editing).
    pub fn enqueue_pending(&mut self, data: String) {
        if self.is_editing() {
            self.pending_sync_data.push(data);
        }
    }

    /// Number of pending sync data items.
    pub fn pending_count(&self) -> usize {
        self.pending_sync_data.len()
    }

    /// Exit editing mode. Returns pending sync data for conflict checking.
    pub fn exit_editing(&mut self) -> Vec<String> {
        self.editing_record_id = None;
        std::mem::take(&mut self.pending_sync_data)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_display_status_default() {
        assert_eq!(SyncDisplayStatus::default(), SyncDisplayStatus::NotConfigured);
    }

    #[test]
    fn sync_indicator_state_default() {
        let state = SyncIndicatorState::default();
        assert_eq!(state.status, SyncDisplayStatus::NotConfigured);
        assert!(state.last_sync.is_none());
        assert!(state.progress.is_none());
        assert!(state.error_message.is_none());
        assert!(!state.detail_visible);
        assert!(state.detail_timer.is_none());
        assert_eq!(state.animation_frame, 0);
    }

    #[test]
    fn conflict_resolution_state_default() {
        let state = ConflictResolutionState::default();
        assert!(state.conflicts.is_empty());
        assert_eq!(state.current_index, 0);
        assert_eq!(state.focused_side, ConflictSide::Local);
    }

    #[test]
    fn conflict_side_default() {
        assert_eq!(ConflictSide::default(), ConflictSide::Local);
    }

    #[test]
    fn sync_queue_idle_by_default() {
        let queue = SyncQueueState::default();
        assert!(queue.is_idle());
        assert!(!queue.is_editing());
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn sync_queue_enter_exit_editing() {
        let mut queue = SyncQueueState::default();
        let id = Uuid::new_v4();
        queue.enter_editing(id);

        assert!(queue.is_editing());
        assert!(!queue.is_idle());
        assert_eq!(queue.editing_record_id, Some(id));
    }

    #[test]
    fn sync_queue_enqueue_while_editing() {
        let mut queue = SyncQueueState::default();
        let id = Uuid::new_v4();
        queue.enter_editing(id);
        queue.enqueue_pending("data1".to_string());
        queue.enqueue_pending("data2".to_string());

        assert_eq!(queue.pending_count(), 2);
    }

    #[test]
    fn sync_queue_enqueue_ignored_when_idle() {
        let mut queue = SyncQueueState::default();
        queue.enqueue_pending("data1".to_string());

        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn sync_queue_exit_returns_pending_data() {
        let mut queue = SyncQueueState::default();
        let id = Uuid::new_v4();
        queue.enter_editing(id);
        queue.enqueue_pending("data1".to_string());
        queue.enqueue_pending("data2".to_string());

        let pending = queue.exit_editing();

        assert!(queue.is_idle());
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0], "data1");
        assert_eq!(pending[1], "data2");
        assert_eq!(queue.pending_count(), 0);
    }
}
