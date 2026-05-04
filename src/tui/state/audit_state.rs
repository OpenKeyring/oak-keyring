//! Audit log screen state types (U10).
//!
//! UI-layer state for the audit log screen. Contains its own [`AuditFilter`]
//! which differs from [`crate::commands::types::AuditFilter`] — the UI version
//! uses `String` for search (always present, possibly empty) while the command
//! version uses `Option<String>`. Conversion happens at the command boundary.

use crate::types::AuditOperation;

/// Audit log screen state.
#[derive(Debug, Clone)]
pub struct AuditLogScreenState {
    /// Loaded audit entries (filtered view).
    pub entries: Vec<crate::types::AuditEntry>,
    /// Total entry count (before filtering, for display).
    pub total_count: usize,
    /// Index of the currently selected entry in the log list.
    pub selected_index: usize,
    /// Vertical scroll offset for the log list.
    pub scroll_offset: usize,
    /// Active filter applied to the log view.
    pub filter: AuditFilter,
    /// Which area of the audit screen currently has focus.
    pub focused_area: AuditFocus,
    /// Whether the audit log feature is enabled in config.
    pub audit_enabled: bool,
    /// Transient hint message shown at the bottom of the screen.
    pub hint_message: Option<String>,
    /// One-shot marker set when navigation state was restored from a screen snapshot.
    pub restored_from_snapshot: bool,
}

impl Default for AuditLogScreenState {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            total_count: 0,
            selected_index: 0,
            scroll_offset: 0,
            filter: AuditFilter::default(),
            focused_area: AuditFocus::LogList,
            audit_enabled: true,
            hint_message: None,
            restored_from_snapshot: false,
        }
    }
}

/// UI-layer audit filter state.
///
/// Unlike [`crate::commands::types::AuditFilter`] which uses `Option<String>`
/// for search, this version uses `String` so the UI always has a buffer to
/// write into. Convert to the command version when dispatching queries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditFilter {
    /// Operation category filter (None = show all).
    pub operation: Option<AuditOperation>,
    /// Time range filter.
    pub time_range: Option<crate::commands::types::AuditTimeRange>,
    /// Search text (empty string = no search filter).
    pub search: String,
}

/// Which area of the audit screen is focused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuditFocus {
    #[default]
    LogList,
    OperationFilter,
    TimeFilter,
    SearchInput,
}

/// Operation type display categories (grouping of [`AuditOperation`] for filtering).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuditOperationFilter {
    #[default]
    All,
    /// RecordCopyPassword, RecordCopyField, RecordViewPassword
    Copy,
    /// RecordCreate, RecordRestore
    Create,
    /// RecordUpdate
    Modify,
    /// RecordDelete, RecordDestroy, TrashEmpty
    Delete,
    /// VaultUnlock, VaultLock, VaultExport, VaultImport, MasterPasswordChange, etc.
    System,
}

impl AuditOperationFilter {
    /// Human-readable display name for this category.
    pub fn display_name(&self) -> String {
        match self {
            AuditOperationFilter::All => crate::t!("tui.audit.filter_type_all").to_string(),
            AuditOperationFilter::Copy => crate::t!("tui.audit.filter_type_copy").to_string(),
            AuditOperationFilter::Create => crate::t!("tui.audit.filter_type_add").to_string(),
            AuditOperationFilter::Modify => crate::t!("tui.audit.filter_type_modify").to_string(),
            AuditOperationFilter::Delete => crate::t!("tui.audit.filter_type_delete").to_string(),
            AuditOperationFilter::System => crate::t!("tui.audit.filter_type_system").to_string(),
        }
    }

    /// All filter variants in display order.
    pub fn all_variants() -> &'static [AuditOperationFilter] {
        &[
            Self::All,
            Self::Copy,
            Self::Create,
            Self::Modify,
            Self::Delete,
            Self::System,
        ]
    }

    /// Check if an [`AuditOperation`] matches this filter category.
    pub fn matches(&self, op: &AuditOperation) -> bool {
        match self {
            AuditOperationFilter::All => true,
            AuditOperationFilter::Copy => matches!(
                op,
                AuditOperation::RecordCopyPassword
                    | AuditOperation::RecordCopyField
                    | AuditOperation::RecordViewPassword
            ),
            AuditOperationFilter::Create => matches!(
                op,
                AuditOperation::RecordCreate | AuditOperation::RecordRestore
            ),
            AuditOperationFilter::Modify => matches!(op, AuditOperation::RecordUpdate),
            AuditOperationFilter::Delete => matches!(
                op,
                AuditOperation::RecordDelete
                    | AuditOperation::RecordDestroy
                    | AuditOperation::TrashEmpty
            ),
            AuditOperationFilter::System => matches!(
                op,
                AuditOperation::VaultUnlock
                    | AuditOperation::VaultLock
                    | AuditOperation::VaultExport
                    | AuditOperation::VaultImport
                    | AuditOperation::MasterPasswordChange
                    | AuditOperation::SyncConflictResolved
                    | AuditOperation::SyncBatchConflictsResolved
                    | AuditOperation::DekRotated
                    | AuditOperation::DekRotationFailed
            ),
        }
    }
}

// ── AuditLogScreenState ──────────────────────────────────────────────────────

impl AuditLogScreenState {
    /// Capture reusable navigation state for this screen.
    pub fn to_restore_state(&self) -> crate::tui::state::AuditLogRestoreState {
        crate::tui::state::AuditLogRestoreState {
            focused_area: self.focused_area,
            selected_index: self.selected_index,
            scroll_offset: self.scroll_offset,
            filter: self.filter.clone(),
        }
    }

    /// Restore navigation state from a previously captured restore state.
    pub fn restore_from(&mut self, restore: crate::tui::state::AuditLogRestoreState) {
        self.focused_area = restore.focused_area;
        self.filter = restore.filter;
        if self.entries.is_empty() {
            self.selected_index = 0;
            self.scroll_offset = 0;
        } else {
            self.selected_index = restore.selected_index.min(self.entries.len() - 1);
            self.scroll_offset = restore.scroll_offset.min(self.selected_index);
        }
        self.restored_from_snapshot = true;
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_has_sensible_defaults() {
        let state = AuditLogScreenState::default();
        assert!(state.entries.is_empty());
        assert_eq!(state.total_count, 0);
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.scroll_offset, 0);
        assert_eq!(state.focused_area, AuditFocus::LogList);
        assert!(state.audit_enabled);
        assert!(state.hint_message.is_none());
    }

    #[test]
    fn default_filter_is_empty() {
        let filter = AuditFilter::default();
        assert!(filter.operation.is_none());
        assert!(filter.time_range.is_none());
        assert!(filter.search.is_empty());
    }

    #[test]
    fn operation_filter_all_variants_count() {
        assert_eq!(AuditOperationFilter::all_variants().len(), 6);
    }

    #[test]
    fn operation_filter_display_names() {
        // Test that labels are non-empty (content may be localized)
        assert!(!AuditOperationFilter::All.display_name().is_empty());
        assert!(!AuditOperationFilter::Copy.display_name().is_empty());
        assert!(!AuditOperationFilter::Create.display_name().is_empty());
        assert!(!AuditOperationFilter::Modify.display_name().is_empty());
        assert!(!AuditOperationFilter::Delete.display_name().is_empty());
        assert!(!AuditOperationFilter::System.display_name().is_empty());
    }

    #[test]
    fn operation_filter_matches_all() {
        let filter = AuditOperationFilter::All;
        assert!(filter.matches(&AuditOperation::RecordCreate));
        assert!(filter.matches(&AuditOperation::VaultUnlock));
        assert!(filter.matches(&AuditOperation::TrashEmpty));
    }

    #[test]
    fn operation_filter_matches_copy() {
        let filter = AuditOperationFilter::Copy;
        assert!(filter.matches(&AuditOperation::RecordCopyPassword));
        assert!(filter.matches(&AuditOperation::RecordCopyField));
        assert!(filter.matches(&AuditOperation::RecordViewPassword));
        assert!(!filter.matches(&AuditOperation::RecordCreate));
    }

    #[test]
    fn operation_filter_matches_create() {
        let filter = AuditOperationFilter::Create;
        assert!(filter.matches(&AuditOperation::RecordCreate));
        assert!(filter.matches(&AuditOperation::RecordRestore));
        assert!(!filter.matches(&AuditOperation::RecordUpdate));
    }

    #[test]
    fn operation_filter_matches_modify() {
        let filter = AuditOperationFilter::Modify;
        assert!(filter.matches(&AuditOperation::RecordUpdate));
        assert!(!filter.matches(&AuditOperation::RecordCreate));
    }

    #[test]
    fn operation_filter_matches_delete() {
        let filter = AuditOperationFilter::Delete;
        assert!(filter.matches(&AuditOperation::RecordDelete));
        assert!(filter.matches(&AuditOperation::RecordDestroy));
        assert!(filter.matches(&AuditOperation::TrashEmpty));
        assert!(!filter.matches(&AuditOperation::RecordCreate));
    }

    #[test]
    fn operation_filter_matches_system() {
        let filter = AuditOperationFilter::System;
        assert!(filter.matches(&AuditOperation::VaultUnlock));
        assert!(filter.matches(&AuditOperation::VaultLock));
        assert!(filter.matches(&AuditOperation::VaultExport));
        assert!(filter.matches(&AuditOperation::VaultImport));
        assert!(filter.matches(&AuditOperation::MasterPasswordChange));
        assert!(filter.matches(&AuditOperation::SyncConflictResolved));
        assert!(filter.matches(&AuditOperation::SyncBatchConflictsResolved));
        assert!(filter.matches(&AuditOperation::DekRotated));
        assert!(filter.matches(&AuditOperation::DekRotationFailed));
        assert!(!filter.matches(&AuditOperation::RecordCreate));
    }

    #[test]
    fn audit_log_restore_state_restores_focus_filter_selection_and_scroll() {
        let mut state = AuditLogScreenState::default();
        state.focused_area = AuditFocus::SearchInput;
        state.selected_index = 7;
        state.scroll_offset = 4;
        state.filter.search = "vault".to_string();

        let restore = state.to_restore_state();

        let mut restored = AuditLogScreenState::default();
        // Populate entries so that selected_index and scroll_offset aren't clamped to 0
        let now = chrono::Utc::now();
        restored.entries = (0..10)
            .map(|i| crate::types::AuditEntry {
                id: i as i64,
                operation: crate::types::AuditOperation::RecordCreate,
                record_id: None,
                record_name: None,
                detail: None,
                occurred_at: now,
            })
            .collect();
        restored.restore_from(restore);

        assert_eq!(restored.focused_area, AuditFocus::SearchInput);
        assert_eq!(restored.selected_index, 7);
        assert_eq!(restored.scroll_offset, 4);
        assert_eq!(restored.filter.search, "vault");
    }
}
