use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordFilter {
    All,
    Favorites,
    Expired,
    HealthIssues,
    Trash,
    Tag(String),
    Search(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortField {
    Name,
    CreatedAt,
    UpdatedAt,
    UsageFrequency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSort {
    pub field: SortField,
    pub direction: SortDirection,
}

impl Default for RecordSort {
    fn default() -> Self {
        Self {
            field: SortField::Name,
            direction: SortDirection::Asc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldSelector {
    Password,
    Username,
    Url,
    Notes,
}

/// Application lifecycle phase (TEA state)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPhase {
    Initializing,
    Running,
    ShuttingDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Unlock,
    Onboarding,
    Main,
    CreateRecord,
    EditRecord { id: Uuid },
    Config,
    ImportExport,
    AuditLog,
    SyncConflict,
    ChangeMasterPassword,
    SetNewMasterPassword,
    PasswordGenerator,
}

/// Three-panel layout focus target
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelId {
    Sidebar,
    List,
    Detail,
}

/// Sync conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    KeepLocal,
    KeepRemote,
}

/// Supported import file formats
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportSource {
    KeePass,            // .kdbx
    OnePassword1pux,    // .1pux
    OnePasswordOpvault, // .opvault
    Bitwarden,          // .json
    Csv,                // .csv
    OpenKeyringBackup,  // .okb
}

/// Export record range
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportScope {
    All,
    CurrentFilter(RecordFilter),
    ByTag(String),
}

/// Audit log time range filter
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditTimeRange {
    Today,
    LastWeek,
    LastMonth,
    LastYear,
    All,
}

/// Notification severity for UI display
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Confirm dialog focused button
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmButton {
    Cancel,
    Confirm,
}

// ---------------------------------------------------------------------------
// Struct types
// ---------------------------------------------------------------------------

/// CSV column mapping for import
#[derive(Debug, Clone)]
pub struct CsvColumnMapping {
    pub name_column: String,
    pub username_column: String,
    pub password_column: String,
    pub url_column: String,
    pub notes_column: String,
    pub tags_column: Option<String>,
    pub skip_header: bool,
}

/// Audit log filter criteria
#[derive(Debug, Clone)]
pub struct AuditFilter {
    pub operation: Option<crate::types::AuditOperation>,
    pub time_range: Option<AuditTimeRange>,
    pub search: Option<String>,
}

/// Health check result report
#[derive(Debug, Clone)]
pub struct HealthReport {
    pub weak_passwords: Vec<Uuid>,
    pub duplicate_passwords: Vec<Vec<Uuid>>,
    pub compromised: Vec<Uuid>,
    pub expired: Vec<Uuid>,
    pub total_checked: usize,
}

/// Import file preview summary
#[derive(Debug, Clone)]
pub struct ImportPreview {
    pub importable: usize,
    pub needs_review: usize,
    pub failed: usize,
    pub review_items: Vec<ReviewItem>,
    pub failed_items: Vec<FailedItem>,
}

/// Item needing manual review during import
#[derive(Debug, Clone)]
pub struct ReviewItem {
    pub name: String,
    pub reason: String,
}

/// Item that failed import
#[derive(Debug, Clone)]
pub struct FailedItem {
    pub name: String,
    pub reason: String,
}

/// Confirm dialog action variants
#[derive(Debug, Clone)]
pub enum ConfirmVariant {
    SoftDelete {
        record_id: Uuid,
        record_name: String,
        auto_delete_days: Option<u32>,
    },
    HardDelete {
        record_id: Uuid,
        record_name: String,
    },
    EmptyTrash {
        count: usize,
    },
    BatchSoftDelete {
        record_ids: Vec<Uuid>,
        record_names: Vec<String>,
    },
    TagDelete {
        tag_name: String,
        affected_count: usize,
    },
}

/// Confirm dialog state
#[derive(Debug, Clone)]
pub struct ConfirmDialogState {
    pub variant: ConfirmVariant,
    pub focused_button: ConfirmButton,
}

/// Batch tag panel state
#[derive(Debug, Clone)]
pub struct BatchTagPanelState {
    pub record_ids: Vec<Uuid>,
    pub current_tag: String,
}

/// Error dialog state
#[derive(Debug, Clone)]
pub struct ErrorDialogState {
    pub title: String,
    pub message: String,
    pub detail: Option<String>,
}

/// Overlay types that can be displayed on top of main layout
#[derive(Debug, Clone)]
pub enum Overlay {
    Help,
    PasswordHistory { record_id: Uuid },
    PasswordGenerator,
    ConfirmDialog(ConfirmDialogState),
    BatchTagPanel(BatchTagPanelState),
    ErrorDialog(ErrorDialogState),
}

/// Health issue priority for a single record.
/// Compromised > Weak > Duplicate > Expired (matches S3 spec priority ordering).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthIssue {
    Compromised,
    Weak,
    Duplicate { group_size: usize },
    Expired,
}

impl HealthReport {
    /// Total number of distinct records with issues (de-duplicated across categories).
    pub fn issue_count(&self) -> usize {
        let mut ids = std::collections::HashSet::new();
        for id in &self.weak_passwords {
            ids.insert(*id);
        }
        for group in &self.duplicate_passwords {
            for id in group {
                ids.insert(*id);
            }
        }
        for id in &self.compromised {
            ids.insert(*id);
        }
        for id in &self.expired {
            ids.insert(*id);
        }
        ids.len()
    }

    /// Whether any health issue exists.
    pub fn has_issues(&self) -> bool {
        self.issue_count() > 0
    }

    /// Get the highest-priority health issue for a specific record.
    /// Priority: Compromised > Weak > Duplicate > Expired
    /// Returns None if the record has no issues.
    pub fn get_issue_for(&self, id: Uuid) -> Option<HealthIssue> {
        if self.compromised.contains(&id) {
            return Some(HealthIssue::Compromised);
        }
        if self.weak_passwords.contains(&id) {
            return Some(HealthIssue::Weak);
        }
        for group in &self.duplicate_passwords {
            if group.contains(&id) {
                return Some(HealthIssue::Duplicate {
                    group_size: group.len(),
                });
            }
        }
        if self.expired.contains(&id) {
            return Some(HealthIssue::Expired);
        }
        None
    }

    /// Create an empty report.
    pub fn empty() -> Self {
        Self {
            weak_passwords: Vec::new(),
            duplicate_passwords: Vec::new(),
            compromised: Vec::new(),
            expired: Vec::new(),
            total_checked: 0,
        }
    }
}

#[cfg(test)]
mod health_report_tests {
    use super::*;

    #[test]
    fn empty_report_has_no_issues() {
        let report = HealthReport::empty();
        assert_eq!(report.issue_count(), 0);
        assert!(!report.has_issues());
        assert_eq!(report.total_checked, 0);
    }

    #[test]
    fn issue_count_de_duplicates_across_categories() {
        let id = Uuid::new_v4();
        let report = HealthReport {
            weak_passwords: vec![id],
            duplicate_passwords: vec![vec![id, Uuid::new_v4()]],
            compromised: vec![id],
            expired: vec![id],
            total_checked: 2,
        };
        // Same UUID in all 4 categories → counts as 1 distinct record
        assert_eq!(report.issue_count(), 2); // id + the other UUID in duplicate group
    }

    #[test]
    fn get_issue_for_compromised_has_highest_priority() {
        let id = Uuid::new_v4();
        let report = HealthReport {
            weak_passwords: vec![id],
            compromised: vec![id],
            ..HealthReport::empty()
        };
        assert_eq!(report.get_issue_for(id), Some(HealthIssue::Compromised));
    }

    #[test]
    fn get_issue_for_weak_beats_duplicate() {
        let id = Uuid::new_v4();
        let report = HealthReport {
            weak_passwords: vec![id],
            duplicate_passwords: vec![vec![id, Uuid::new_v4()]],
            ..HealthReport::empty()
        };
        assert_eq!(report.get_issue_for(id), Some(HealthIssue::Weak));
    }

    #[test]
    fn get_issue_for_duplicate_includes_group_size() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let report = HealthReport {
            duplicate_passwords: vec![vec![id1, id2]],
            ..HealthReport::empty()
        };
        assert_eq!(
            report.get_issue_for(id1),
            Some(HealthIssue::Duplicate { group_size: 2 })
        );
    }

    #[test]
    fn get_issue_for_expired_is_lowest_priority() {
        let id = Uuid::new_v4();
        let report = HealthReport {
            expired: vec![id],
            ..HealthReport::empty()
        };
        assert_eq!(report.get_issue_for(id), Some(HealthIssue::Expired));
    }

    #[test]
    fn get_issue_for_returns_none_for_clean_record() {
        let report = HealthReport::empty();
        assert_eq!(report.get_issue_for(Uuid::new_v4()), None);
    }
}

// Re-export rotation progress for TUI consumption
pub use crate::types::rotation::RotationProgress;
