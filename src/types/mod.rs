pub mod audit;
pub mod credential;
pub mod history;
pub mod record;
pub mod rotation;
pub mod sensitive;
pub mod sync;
pub mod tag;

pub use audit::{AuditEntry, AuditOperation};
pub use credential::{CredentialType, DataError, EncryptedPayload};
pub use history::{PasswordHistory, PasswordHistoryView};
pub use record::{
    CreateRecordParams, DecryptedRecord, StoredRecord, TuiRecord, UpdateRecordParams,
};
pub use sensitive::{SecureBytes, SecureStr, SecureString};
pub use sync::{SyncState, SyncStats, SyncStatus};
pub use tag::Tag;
pub use rotation::{RotationConfig, RotationConstants, RotationState, RotationTrigger};

#[cfg(test)]
mod credential_test;
