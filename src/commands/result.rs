use std::collections::HashMap;
use std::path::PathBuf;

use uuid::Uuid;

use crate::commands::types::*;
use crate::config::AppConfig;
use crate::errors::{ErrorCode, ErrorContext};
use crate::types::tag::TagSortMeta;
use crate::types::{DecryptedRecord, PasswordHistoryView, SecureStr, SyncStats, Tag, TuiRecord};

/// Service → UI execution result.
///
/// Delivered via Message::CommandCompleted(CommandResult) to TEA update().
/// Variants carrying SecureStr do NOT impl Clone (security requirement).
/// Cancelled variant for long operation interruption (tea-command-pattern-arch §6.6.3).
#[derive(Debug)]
pub enum CommandResult {
    // ── Record CRUD Results ────────────────────
    RecordCreated {
        id: Uuid,
    },
    RecordUpdated {
        id: Uuid,
    },
    RecordDeleted {
        id: Uuid,
    },
    RecordRestored {
        id: Uuid,
    },
    RecordDestroyed {
        id: Uuid,
    },
    FavoriteToggled {
        id: Uuid,
        is_favorite: bool,
    },

    // ── Query Results ─────────────────────────
    RecordListLoaded {
        records: Vec<TuiRecord>,
        total: usize,
    },
    RecordDetailLoaded {
        record: DecryptedRecord,
        password_strength: Option<crate::crypto::strength::PasswordStrength>,
        health_issue: Option<HealthIssue>,
    },
    RecordForEditLoaded {
        record: DecryptedRecord,
    },
    FieldDecrypted {
        id: Uuid,
        field: FieldSelector,
        value: SecureStr,
    },

    // ── Clipboard Results ──────────────────────
    CopiedToClipboard {
        field: FieldSelector,
        clear_after_seconds: u64,
    },

    // ── Password History Results ──────────────
    PasswordHistoryLoaded {
        history: Vec<PasswordHistoryView>,
    },
    HistoryPasswordCopied {
        clear_after_seconds: u64,
    },

    // ── Tag Results ──────────────────────────
    TagsLoaded {
        tags: Vec<Tag>,
        tag_stats: HashMap<i64, TagSortMeta>,
    },
    TagRenamed {
        old_name: String,
        new_name: String,
    },
    TagDeleted {
        name: String,
    },
    BatchTagAdded {
        count: usize,
    },
    BatchTagRemoved {
        count: usize,
    },

    // ── Batch Results ─────────────────────────
    BatchDeleted {
        count: usize,
    },
    TrashEmptied {
        count: usize,
    },

    // ── Password Generation Results ───────────
    PasswordGenerated {
        password: SecureStr,
        strength: crate::crypto::strength::PasswordStrength,
    },

    // ── Health Check Results ──────────────────
    HealthCheckStarted,
    HealthCheckCompleted {
        report: HealthReport,
    },
    /// Health check was skipped — typically because it is disabled in config
    /// or the configured frequency window hasn't elapsed yet.
    HealthCheckSkipped,
    HibpCheckCompleted {
        record_id: Uuid,
        compromised: bool,
    },

    // ── Sync Results ─────────────────────────
    SyncCompleted {
        stats: SyncStats,
    },
    ConflictResolved {
        record_id: Uuid,
    },
    AllConflictsResolved {
        count: usize,
    },

    // ── Import/Export Results ─────────────────
    ImportValidated {
        session_id: Uuid,
        preview: ImportPreview,
    },
    ImportCompleted {
        imported_count: usize,
        reviewed_count: usize,
        skipped_count: usize,
        failed_count: usize,
        skip_breakdown: HashMap<SkipReason, usize>,
    },
    ExportCompleted {
        path: PathBuf,
        record_count: usize,
        format: ExportFormat,
    },

    // ── Vault Results ────────────────────────
    VaultUnlocked,
    VaultUnlockFailed {
        attempts_remaining: Option<u32>,
    },
    RecoveryKeyUnlocked,
    VaultLocked,
    MasterPasswordVerified,
    MasterPasswordChanged,
    VaultInitialized {
        recovery_words: Vec<String>,
    },

    // ── Audit Results ────────────────────────
    AuditLogLoaded {
        entries: Vec<crate::types::AuditEntry>,
        total: usize,
    },

    // ── DEK Rotation Results ─────────────────────
    /// DEK rotation completed successfully
    RotationCompleted {
        old_version: u32,
        new_version: u32,
        records_migrated: u32,
    },
    /// Rotation trigger check result
    RotationTriggerChecked {
        should_rotate: bool,
        reason: Option<String>,
    },

    // ── Config Results ───────────────────────
    ConfigLoaded {
        config: AppConfig,
    },
    ConfigSaved {
        warnings: Vec<String>,
    },
    SyncConnectionTested {
        success: bool,
        message: String,
    },

    /// OAuth2 authorization completed successfully.
    OAuth2Authorized {
        provider: String,
        access_token: String,
        refresh_token: Option<String>,
    },

    /// OAuth2 authorization failed.
    OAuth2Failed {
        provider: String,
        error: String,
    },

    // ── Errors (structured) ──────────────────
    /// Operational error: single record failure, decrypt failure, version conflict, etc.
    /// UI display: inline error (retry button) or StatusBar 5s temporary notification
    Error {
        code: ErrorCode,
        context: ErrorContext,
        message_key: &'static str,
        fallback: String,
    },

    /// Fatal error: database corruption, file permissions, unrecoverable scenarios
    /// UI display: fullscreen ErrorDialog (retry/exit)
    FatalError {
        code: ErrorCode,
        context: ErrorContext,
        message_key: &'static str,
        fallback: String,
        detail: Option<String>,
    },

    // ── Cancellation ─────────────────────────
    /// Long operation interrupted by CancellationToken.
    /// Partial progress preserved for resume capability.
    /// Follows tea-command-pattern-arch §6.6.3 cancellation semantics.
    Cancelled {
        operation: String,
        partial_progress: Option<String>,
    },
}

impl CommandResult {
    /// Create a `Cancelled` result for the given operation name.
    ///
    /// `partial_progress` is set to `None`, indicating no progress was made
    /// before the cancellation took effect.
    pub fn cancelled(operation: impl Into<String>) -> Self {
        Self::Cancelled {
            operation: operation.into(),
            partial_progress: None,
        }
    }
}

#[cfg(test)]
mod exhaustive_tests {
    use super::*;

    /// Compile-time exhaustiveness check.
    /// Adding a new CommandResult variant without updating this match will cause a compile error.
    #[test]
    fn command_result_exhaustive_match() {
        fn _assert_exhaustive(result: CommandResult) {
            match result {
                // Record CRUD Results
                CommandResult::RecordCreated { .. } => {}
                CommandResult::RecordUpdated { .. } => {}
                CommandResult::RecordDeleted { .. } => {}
                CommandResult::RecordRestored { .. } => {}
                CommandResult::RecordDestroyed { .. } => {}
                CommandResult::FavoriteToggled { .. } => {}
                // Query Results
                CommandResult::RecordListLoaded { .. } => {}
                CommandResult::RecordDetailLoaded { .. } => {}
                CommandResult::RecordForEditLoaded { .. } => {}
                CommandResult::FieldDecrypted { .. } => {}
                // Clipboard Results
                CommandResult::CopiedToClipboard { .. } => {}
                // Password History Results
                CommandResult::PasswordHistoryLoaded { .. } => {}
                CommandResult::HistoryPasswordCopied { .. } => {}
                // Tag Results
                CommandResult::TagsLoaded { .. } => {}
                CommandResult::TagRenamed { .. } => {}
                CommandResult::TagDeleted { .. } => {}
                CommandResult::BatchTagAdded { .. } => {}
                CommandResult::BatchTagRemoved { .. } => {}
                // Batch Results
                CommandResult::BatchDeleted { .. } => {}
                CommandResult::TrashEmptied { .. } => {}
                // Password Generation Results
                CommandResult::PasswordGenerated { .. } => {}
                // Health Check Results
                CommandResult::HealthCheckStarted => {}
                CommandResult::HealthCheckCompleted { .. } => {}
                CommandResult::HealthCheckSkipped => {}
                CommandResult::HibpCheckCompleted { .. } => {}
                // Sync Results
                CommandResult::SyncCompleted { .. } => {}
                CommandResult::ConflictResolved { .. } => {}
                CommandResult::AllConflictsResolved { .. } => {}
                // Import/Export Results
                CommandResult::ImportValidated { .. } => {}
                CommandResult::ImportCompleted {
                    imported_count,
                    reviewed_count,
                    failed_count,
                    skipped_count,
                    skip_breakdown,
                } => {
                    let _ = (
                        imported_count,
                        reviewed_count,
                        failed_count,
                        skipped_count,
                        skip_breakdown,
                    );
                }
                CommandResult::ExportCompleted { .. } => {}
                // Vault Results
                CommandResult::VaultUnlocked => {}
                CommandResult::VaultUnlockFailed { .. } => {}
                CommandResult::RecoveryKeyUnlocked => {}
                CommandResult::VaultLocked => {}
                CommandResult::MasterPasswordVerified => {}
                CommandResult::MasterPasswordChanged => {}
                CommandResult::VaultInitialized { .. } => {}
                // Audit Results
                CommandResult::AuditLogLoaded { .. } => {}
                // DEK Rotation Results
                CommandResult::RotationCompleted { .. } => {}
                CommandResult::RotationTriggerChecked { .. } => {}
                // Config Results
                CommandResult::ConfigLoaded { .. } => {}
                CommandResult::ConfigSaved { .. } => {}
                CommandResult::SyncConnectionTested { .. } => {}
                CommandResult::OAuth2Authorized { .. } => {}
                CommandResult::OAuth2Failed { .. } => {}
                // Errors
                CommandResult::Error { .. } => {}
                CommandResult::FatalError { .. } => {}
                // Cancellation
                CommandResult::Cancelled { .. } => {}
            }
        }
    }

    #[test]
    fn cancelled_helper_creates_correct_variant() {
        let result = CommandResult::cancelled("sync");
        assert!(matches!(
            result,
            CommandResult::Cancelled { ref operation, ref partial_progress }
                if operation == "sync" && partial_progress.is_none()
        ));
    }
}
