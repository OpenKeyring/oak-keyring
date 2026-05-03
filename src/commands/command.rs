use std::path::PathBuf;

use uuid::Uuid;

use crate::commands::types::*;
use crate::types::{CredentialType, EncryptedPayload, SecureStr};

/// UI → Executor business command.
///
/// Fire-and-forget: UI sends and doesn't block. Results come back via Message::CommandCompleted.
/// Variants carrying SecureStr do NOT impl Clone (security requirement).
#[derive(Debug)]
pub enum Command {
    // ── Vault Operations ──────────────────────────
    UnlockVault {
        master_password: SecureStr,
    },

    UnlockWithRecoveryKey {
        words: Vec<String>,
    },

    LockVault,

    VerifyMasterPassword {
        password: SecureStr,
    },

    ChangeMasterPassword {
        current_password: SecureStr,
        new_password: SecureStr,
    },

    InitializeVault {
        vault_path: PathBuf,
        master_password: SecureStr,
    },

    // ── Record CRUD ──────────────────────────────
    CreateRecord {
        credential_type: CredentialType,
        payload: EncryptedPayload,
        tags: Vec<String>,
        is_favorite: bool,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    },

    UpdateRecord {
        id: Uuid,
        payload: EncryptedPayload,
        tags: Vec<String>,
        is_favorite: bool,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        expected_version: u64,
    },

    SoftDeleteRecord {
        id: Uuid,
    },

    RestoreRecord {
        id: Uuid,
    },

    HardDeleteRecord {
        id: Uuid,
    },

    ToggleFavorite {
        id: Uuid,
        is_favorite: bool,
    },

    // ── Record Query ──────────────────────────────
    LoadRecordList {
        filter: RecordFilter,
        sort: RecordSort,
    },

    LoadRecordDetail {
        id: Uuid,
    },

    LoadRecordForEdit {
        id: Uuid,
    },

    DecryptField {
        id: Uuid,
        field: FieldSelector,
    },

    // ── Clipboard Operations ────────────────────
    CopyToClipboard {
        id: Uuid,
        field: FieldSelector,
    },

    CopyRawToClipboard {
        value: SecureStr,
    },

    // ── Password History ──────────────────────────
    LoadPasswordHistory {
        record_id: Uuid,
    },

    CopyHistoryPassword {
        history_id: i64,
    },

    // ── Tag Operations ────────────────────────────
    LoadTags,

    RenameTag {
        old_name: String,
        new_name: String,
    },

    DeleteTag {
        name: String,
    },

    BatchAddTag {
        record_ids: Vec<Uuid>,
        tag_name: String,
    },

    BatchRemoveTag {
        record_ids: Vec<Uuid>,
        tag_name: String,
    },

    // ── Batch Operations ──────────────────────────
    BatchSoftDelete {
        record_ids: Vec<Uuid>,
    },

    EmptyTrash,

    // ── Password Generation ───────────────────────
    GeneratePassword {
        length: usize,
        include_digits: bool,
        include_uppercase: bool,
        include_special: bool,
    },

    GenerateMemorablePassword {
        word_count: usize,
    },

    GeneratePin {
        length: usize,
    },

    // ── Health Check ──────────────────────────────
    RunHealthCheck {
        force: bool,
    },

    CheckHibp {
        record_id: Uuid,
    },

    // ── Sync Operations ──────────────────────────
    TriggerSync,

    ResolveConflict {
        record_id: Uuid,
        resolution: ConflictResolution,
    },

    ResolveAllConflicts {
        resolution: ConflictResolution,
    },

    // ── Import/Export ─────────────────────────────
    ValidateImportFile {
        source: ImportSource,
        path: PathBuf,
        password: Option<SecureStr>,
    },

    ExecuteImport {
        source: ImportSource,
        path: PathBuf,
        password: Option<SecureStr>,
        column_mapping: Option<CsvColumnMapping>,
        import_as_notes: bool,
    },

    ExecuteExport {
        scope: ExportScope,
        output_path: PathBuf,
        export_password: SecureStr,
        master_password: SecureStr,
    },

    // ── Config Operations ─────────────────────────
    LoadConfig,

    SaveConfig {
        config: crate::config::AppConfig,
    },

    TestSyncConnection {
        provider_config: Option<crate::config::sync::ProviderConfig>,
    },

    /// Start OAuth2 authorization flow for Google Drive.
    OAuth2AuthorizeGoogleDrive,

    // ── Audit Log ─────────────────────────────────
    LoadAuditLog {
        filter: AuditFilter,
    },

    NavigateToRecord {
        record_id: Uuid,
    },

    // ── DEK Rotation ─────────────────────────────
    /// Trigger a manual DEK rotation
    TriggerRotation,
    /// Check if rotation should be triggered
    CheckRotationTrigger,

    // ── Internal (Hidden) ──────────────────────────
    /// Internal signal that background health check completed.
    /// Used to update Executor's internal cache.
    InternalHealthCheckCompleted {
        report: HealthReport,
    },
}

#[cfg(test)]
mod exhaustive_tests {
    use super::*;

    /// Compile-time exhaustiveness check.
    /// Adding a new Command variant without updating this match will cause a compile error.
    #[test]
    fn command_exhaustive_match() {
        fn _assert_exhaustive(cmd: Command) {
            match cmd {
                // Vault Operations
                Command::UnlockVault { .. } => {}
                Command::UnlockWithRecoveryKey { .. } => {}
                Command::LockVault => {}
                Command::VerifyMasterPassword { .. } => {}
                Command::ChangeMasterPassword { .. } => {}
                Command::InitializeVault { .. } => {}
                // Record CRUD
                Command::CreateRecord { .. } => {}
                Command::UpdateRecord { .. } => {}
                Command::SoftDeleteRecord { .. } => {}
                Command::RestoreRecord { .. } => {}
                Command::HardDeleteRecord { .. } => {}
                Command::ToggleFavorite { .. } => {}
                // Record Query
                Command::LoadRecordList { .. } => {}
                Command::LoadRecordDetail { .. } => {}
                Command::LoadRecordForEdit { .. } => {}
                Command::DecryptField { .. } => {}
                // Clipboard Operations
                Command::CopyToClipboard { .. } => {}
                Command::CopyRawToClipboard { .. } => {}
                // Password History
                Command::LoadPasswordHistory { .. } => {}
                Command::CopyHistoryPassword { .. } => {}
                // Tag Operations
                Command::LoadTags => {}
                Command::RenameTag { .. } => {}
                Command::DeleteTag { .. } => {}
                Command::BatchAddTag { .. } => {}
                Command::BatchRemoveTag { .. } => {}
                // Batch Operations
                Command::BatchSoftDelete { .. } => {}
                Command::EmptyTrash => {}
                // Password Generation
                Command::GeneratePassword { .. } => {}
                Command::GenerateMemorablePassword { .. } => {}
                Command::GeneratePin { .. } => {}
                // Health Check
                Command::RunHealthCheck { .. } => {}
                Command::CheckHibp { .. } => {}
                // Sync Operations
                Command::TriggerSync => {}
                Command::ResolveConflict { .. } => {}
                Command::ResolveAllConflicts { .. } => {}
                // Import/Export
                Command::ValidateImportFile { .. } => {}
                Command::ExecuteImport { .. } => {}
                Command::ExecuteExport { .. } => {}
                // Config Operations
                Command::LoadConfig => {}
                Command::SaveConfig { .. } => {}
                Command::TestSyncConnection { .. } => {}
                Command::OAuth2AuthorizeGoogleDrive => {}
                // Audit Log
                Command::LoadAuditLog { .. } => {}
                Command::NavigateToRecord { .. } => {}
                // DEK Rotation
                Command::TriggerRotation => {}
                Command::CheckRotationTrigger => {}
                // Internal
                Command::InternalHealthCheckCompleted { .. } => {}
            }
        }
    }
}
