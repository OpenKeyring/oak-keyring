use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::types::credential::DataError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AuditOperation {
    RecordCreate,
    RecordUpdate,
    RecordDelete,
    RecordRestore,
    RecordDestroy,
    RecordViewPassword,
    RecordCopyPassword,
    RecordCopyField,
    VaultUnlock,
    VaultLock,
    VaultExport,
    VaultImport,
    MasterPasswordChange,
    TrashEmpty,
    SyncConflictResolved,
    SyncBatchConflictsResolved,
    DekRotated,
    DekRotationFailed,
    SshSign,
}

impl AuditOperation {
    pub fn to_db_str(self) -> &'static str {
        match self {
            AuditOperation::RecordCreate => "record.create",
            AuditOperation::RecordUpdate => "record.update",
            AuditOperation::RecordDelete => "record.delete",
            AuditOperation::RecordRestore => "record.restore",
            AuditOperation::RecordDestroy => "record.destroy",
            AuditOperation::RecordViewPassword => "record.view_password",
            AuditOperation::RecordCopyPassword => "record.copy_password",
            AuditOperation::RecordCopyField => "record.copy_field",
            AuditOperation::VaultUnlock => "vault.unlock",
            AuditOperation::VaultLock => "vault.lock",
            AuditOperation::VaultExport => "vault.export",
            AuditOperation::VaultImport => "vault.import",
            AuditOperation::MasterPasswordChange => "master_password.change",
            AuditOperation::TrashEmpty => "trash.empty",
            AuditOperation::SyncConflictResolved => "sync.conflict_resolved",
            AuditOperation::SyncBatchConflictsResolved => "sync.batch_conflicts_resolved",
            AuditOperation::DekRotated => "dek.rotated",
            AuditOperation::DekRotationFailed => "dek.rotation_failed",
            AuditOperation::SshSign => "ssh.sign",
        }
    }

    pub fn from_db_str(s: &str) -> Result<Self, DataError> {
        match s {
            "record.create" => Ok(AuditOperation::RecordCreate),
            "record.update" => Ok(AuditOperation::RecordUpdate),
            "record.delete" => Ok(AuditOperation::RecordDelete),
            "record.restore" => Ok(AuditOperation::RecordRestore),
            "record.destroy" => Ok(AuditOperation::RecordDestroy),
            "record.view_password" => Ok(AuditOperation::RecordViewPassword),
            "record.copy_password" => Ok(AuditOperation::RecordCopyPassword),
            "record.copy_field" => Ok(AuditOperation::RecordCopyField),
            "vault.unlock" => Ok(AuditOperation::VaultUnlock),
            "vault.lock" => Ok(AuditOperation::VaultLock),
            "vault.export" => Ok(AuditOperation::VaultExport),
            "vault.import" => Ok(AuditOperation::VaultImport),
            "master_password.change" => Ok(AuditOperation::MasterPasswordChange),
            "trash.empty" => Ok(AuditOperation::TrashEmpty),
            "sync.conflict_resolved" => Ok(AuditOperation::SyncConflictResolved),
            "sync.batch_conflicts_resolved" => Ok(AuditOperation::SyncBatchConflictsResolved),
            "dek.rotated" => Ok(AuditOperation::DekRotated),
            "dek.rotation_failed" => Ok(AuditOperation::DekRotationFailed),
            "ssh.sign" => Ok(AuditOperation::SshSign),
            _ => Err(DataError::InvalidAuditOperation(s.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub id: i64,
    pub operation: AuditOperation,
    pub record_id: Option<Uuid>,
    pub record_name: Option<String>,
    pub detail: Option<String>,
    pub occurred_at: DateTime<Utc>,
}
