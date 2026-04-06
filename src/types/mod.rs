pub mod audit;
pub mod credential;
pub mod history;
pub mod record;
pub mod sensitive;
pub mod sync;
pub mod tag;

pub use audit::{AuditEntry, AuditOperation};
pub use credential::{CredentialType, DataError, EncryptedPayload};
pub use history::{PasswordHistory, PasswordHistoryView};
pub use record::{DecryptedRecord, StoredRecord, TuiRecord};
pub use sensitive::{SecureBytes, SecureStr, SecureString};
pub use sync::{SyncState, SyncStats, SyncStatus};
pub use tag::Tag;

#[cfg(test)]
mod credential_test;
