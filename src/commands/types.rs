use serde::{Deserialize, Serialize};

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
    EditRecord,
    Config,
    ImportExport,
    AuditLog,
    SyncConflict,
    ChangeMasterPassword,
    SetNewMasterPassword,
}
