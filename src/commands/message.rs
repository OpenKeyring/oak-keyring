use crossterm::event::{KeyEvent, MouseEvent};
use uuid::Uuid;

use crate::commands::result::CommandResult;
use crate::commands::types::*;

/// TEA event enum driving state transitions.
///
/// Four categories: CommandResult callback + Terminal events + Navigation + UI interaction.
/// Variants carrying SecureStr (via CommandCompleted) do NOT impl Clone.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum Message {
    // -- CommandResult Callback --------------------------
    /// Unified entry point for all Command execution results
    CommandCompleted(CommandResult),

    // -- Terminal Events ---------------------------------
    KeyEvent(KeyEvent),
    MouseEvent(MouseEvent),
    Resize {
        width: u16,
        height: u16,
    },
    Tick,

    // -- Navigation --------------------------------------
    NavigateTo(Screen),
    GoBack,
    FocusPanel(PanelId),

    // -- List Interaction --------------------------------
    SelectRecord {
        id: Uuid,
    },
    ClearSelection,
    ToggleSelectRecord {
        id: Uuid,
    },
    SelectAll,
    DeselectAll,
    EnterVisualMode,
    ExitVisualMode,

    // -- Search ------------------------------------------
    OpenSearch,
    CloseSearch,
    UpdateSearchQuery(String),

    // -- Sorting -----------------------------------------
    SetSort(RecordSort),

    // -- Filter ------------------------------------------
    SetFilter(RecordFilter),

    // -- Overlay -----------------------------------------
    ShowOverlay(Overlay),
    CloseOverlay,

    // -- Notifications -----------------------------------
    ShowNotification {
        level: NotificationLevel,
        message: String,
        duration_ms: u64,
    },

    // -- Internal State ----------------------------------
    ClipboardTimerTick,
    AutoLockTimerTick,
    SyncTimerTick,

    // -- Tag Management ----------------------------------
    EnterTagManagement,
    ExitTagManagement,
    RenameTagStart,
    RenameTagConfirm {
        old_name: String,
        new_name: String,
    },
    RenameTagCancel,
    CycleTagSort,
    DeleteTagFromManagement,

    ImportProgress {
        current: usize,
        total: usize,
        current_name: String,
    },
    NavigateToRecord {
        record_id: Uuid,
    },

    // -- U10 Audit Log ---------------------------------
    AuditLogLoaded {
        entries: Vec<crate::types::AuditEntry>,
        total: usize,
    },

    // -- U10 Sync UI -----------------------------------
    SyncTriggered,
    SyncCompleted {
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    SyncFailed {
        error: String,
    },
    SyncProgress {
        current: usize,
        total: usize,
    },

    // -- U10 Sync Conflict -----------------------------
    ConflictDetected {
        count: usize,
    },
    ConflictResolved {
        record_id: Uuid,
    },
    AllConflictsResolved {
        count: usize,
    },

    // -- U10 DEK Rotation ------------------------------
    DekRotationStarted,
    DekRotationProgress {
        current: usize,
        total: usize,
    },
    DekRotationCompleted,
    DekRotationFailed {
        error: String,
    },

    // -- Health Check ------------------------------------
    HealthCheckProgress {
        current: usize,
        total: usize,
    },

    // -- Shutdown ----------------------------------------
    ShutdownRequested {
        force: bool,
    },
}

#[cfg(test)]
mod exhaustive_tests {
    use super::*;

    /// Compile-time exhaustiveness check.
    /// Adding a new Message variant without updating this match will cause a compile error.
    #[test]
    fn message_exhaustive_match() {
        fn _assert_exhaustive(msg: Message) {
            match msg {
                // CommandResult Callback
                Message::CommandCompleted(_) => {}
                // Terminal Events
                Message::KeyEvent(_) => {}
                Message::MouseEvent(_) => {}
                Message::Resize { .. } => {}
                Message::Tick => {}
                // Navigation
                Message::NavigateTo(_) => {}
                Message::GoBack => {}
                Message::FocusPanel(_) => {}
                // List Interaction
                Message::SelectRecord { .. } => {}
                Message::ClearSelection => {}
                Message::ToggleSelectRecord { .. } => {}
                Message::SelectAll => {}
                Message::DeselectAll => {}
                Message::EnterVisualMode => {}
                Message::ExitVisualMode => {}
                // Search
                Message::OpenSearch => {}
                Message::CloseSearch => {}
                Message::UpdateSearchQuery(_) => {}
                // Sorting
                Message::SetSort(_) => {}
                // Filter
                Message::SetFilter(_) => {}
                // Overlay
                Message::ShowOverlay(_) => {}
                Message::CloseOverlay => {}
                // Notifications
                Message::ShowNotification { .. } => {}
                // Internal State
                Message::ClipboardTimerTick => {}
                Message::AutoLockTimerTick => {}
                Message::SyncTimerTick => {}
                // Tag Management
                Message::EnterTagManagement => {}
                Message::ExitTagManagement => {}
                Message::RenameTagStart => {}
                Message::RenameTagConfirm { .. } => {}
                Message::RenameTagCancel => {}
                Message::CycleTagSort => {}
                Message::DeleteTagFromManagement => {}
                Message::ImportProgress { .. } => {}
                Message::NavigateToRecord { .. } => {}
                // U10 Audit Log
                Message::AuditLogLoaded { .. } => {}
                // U10 Sync UI
                Message::SyncTriggered => {}
                Message::SyncCompleted { .. } => {}
                Message::SyncFailed { .. } => {}
                Message::SyncProgress { .. } => {}
                // U10 Sync Conflict
                Message::ConflictDetected { .. } => {}
                Message::ConflictResolved { .. } => {}
                Message::AllConflictsResolved { .. } => {}
                // U10 DEK Rotation
                Message::DekRotationStarted => {}
                Message::DekRotationProgress { .. } => {}
                Message::DekRotationCompleted => {}
                Message::DekRotationFailed { .. } => {}
                // Health Check
                Message::HealthCheckProgress { .. } => {}
                // Shutdown
                Message::ShutdownRequested { .. } => {}
            }
        }
    }
}
