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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SortField {
    Name,
    CreatedAt,
    UpdatedAt,
    UsageFrequency,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone)]
pub struct RecordSort {
    pub field: SortField,
    pub direction: SortDirection,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
