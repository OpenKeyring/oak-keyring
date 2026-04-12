use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::commands::types::{CsvColumnMapping, ExportScope, FailedItem, ImportSource, ReviewItem};
use crate::types::{CredentialType, SecureStr};

// ---------------------------------------------------------------------------
// Session status enums
// ---------------------------------------------------------------------------

/// Lifecycle states for an import session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSessionStatus {
    Created,
    Validating,
    Validated,
    Importing,
    Completed,
    Cancelled,
    Failed,
}

/// Lifecycle states for an export session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportSessionStatus {
    Created,
    Exporting,
    Completed,
    Failed,
}

/// Union status covering both import and export sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Import(ImportSessionStatus),
    Export(ExportSessionStatus),
}

// ---------------------------------------------------------------------------
// Import session
// ---------------------------------------------------------------------------

/// Represents an in-progress or completed import operation.
///
/// Holds the source file, parsing options, validation results, and mapped
/// records ready for insertion into the vault.
#[derive(Debug)]
pub struct ImportSession {
    pub id: Uuid,
    pub source: ImportSource,
    pub file_path: PathBuf,
    pub decrypt_password: Option<SecureStr>,
    pub csv_mapping: Option<CsvColumnMapping>,
    pub status: ImportSessionStatus,
    pub validation_result: Option<ValidationResult>,
    pub mapped_records: Vec<MappedRecord>,
    pub failed_items: Vec<FailedItem>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Export session
// ---------------------------------------------------------------------------

/// Represents an in-progress or completed export operation.
///
/// Tracks scope, output destination, encryption password, and progress.
#[derive(Debug)]
pub struct ExportSession {
    pub id: Uuid,
    pub scope: ExportScope,
    pub export_password: SecureStr,
    pub output_path: PathBuf,
    pub status: ExportSessionStatus,
    pub record_count: usize,
    pub encrypted_size: Option<usize>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Validation result
// ---------------------------------------------------------------------------

/// Summary of validation performed on an imported file.
///
/// Breaks items into importable, needs-review, and failed categories.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub total_items: usize,
    pub importable: usize,
    pub needs_review: usize,
    pub failed: usize,
    pub review_items: Vec<ReviewItem>,
    pub failed_items: Vec<FailedItem>,
}

// ---------------------------------------------------------------------------
// Mapped record (plaintext, pre-encryption)
// ---------------------------------------------------------------------------

/// A single record mapped from an external source into vault-compatible form.
///
/// Stores plaintext field values because the import pipeline passes plaintext
/// to `VaultService::create_record()` which handles encryption internally.
/// Using `EncryptedPayload` here would require a redundant encrypt-decrypt
/// round trip.
#[derive(Debug, Clone)]
pub struct MappedRecord {
    /// Generated UUID for this mapped record.
    pub id: Uuid,
    /// Credential type inferred from source data.
    pub credential_type: CredentialType,
    /// Mapped field key-value pairs (e.g. "username" -> "alice").
    pub fields: HashMap<String, String>,
    /// Tags extracted from source.
    pub tags: Vec<String>,
    /// Whether this record is marked as a favorite.
    pub is_favorite: bool,
    /// Expiration time, if the source provides one.
    pub expires_at: Option<DateTime<Utc>>,
    /// Original identifier from the source file.
    pub source_item_id: String,
    /// Unsupported fields stored as freeform notes.
    pub notes: Option<String>,
    /// Whether a duplicate was detected in the existing vault.
    pub is_duplicate: bool,
}

// ---------------------------------------------------------------------------
// Import result
// ---------------------------------------------------------------------------

/// Final tally returned after an import session completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportResult {
    pub imported: usize,
    pub reviewed: usize,
    pub skipped: usize,
    pub failed: usize,
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Exhaustiveness tests (compile-time guarantee via exhaustive match) --

    /// If a new variant is added to `ImportSessionStatus` this match will
    /// fail to compile, alerting the developer to handle it everywhere.
    #[test]
    fn import_session_status_exhaustive_match() {
        let status = ImportSessionStatus::Created;
        let _label = match status {
            ImportSessionStatus::Created => "Created",
            ImportSessionStatus::Validating => "Validating",
            ImportSessionStatus::Validated => "Validated",
            ImportSessionStatus::Importing => "Importing",
            ImportSessionStatus::Completed => "Completed",
            ImportSessionStatus::Cancelled => "Cancelled",
            ImportSessionStatus::Failed => "Failed",
        };
    }

    /// If a new variant is added to `ExportSessionStatus` this match will
    /// fail to compile.
    #[test]
    fn export_session_status_exhaustive_match() {
        let status = ExportSessionStatus::Created;
        let _label = match status {
            ExportSessionStatus::Created => "Created",
            ExportSessionStatus::Exporting => "Exporting",
            ExportSessionStatus::Completed => "Completed",
            ExportSessionStatus::Failed => "Failed",
        };
    }

    // -- MappedRecord construction --

    #[test]
    fn mapped_record_construction_with_all_fields() {
        let record = MappedRecord {
            id: Uuid::new_v4(),
            credential_type: CredentialType::Login,
            fields: {
                let mut m = HashMap::new();
                m.insert("username".to_string(), "alice".to_string());
                m.insert("password".to_string(), "s3cret".to_string());
                m
            },
            tags: vec!["work".to_string(), "email".to_string()],
            is_favorite: true,
            expires_at: None,
            source_item_id: "kp-entry-42".to_string(),
            notes: Some("Imported from KeePass".to_string()),
            is_duplicate: false,
        };

        assert_eq!(record.fields.get("username").unwrap(), "alice");
        assert_eq!(record.tags.len(), 2);
        assert!(record.is_favorite);
        assert!(!record.is_duplicate);
        assert_eq!(record.source_item_id, "kp-entry-42");
    }

    #[test]
    fn mapped_record_minimal_fields() {
        let record = MappedRecord {
            id: Uuid::new_v4(),
            credential_type: CredentialType::Api,
            fields: HashMap::new(),
            tags: vec![],
            is_favorite: false,
            expires_at: None,
            source_item_id: "item-1".to_string(),
            notes: None,
            is_duplicate: false,
        };

        assert!(record.fields.is_empty());
        assert!(record.tags.is_empty());
        assert!(record.notes.is_none());
    }

    // -- ImportResult default / zero --

    #[test]
    fn import_result_default_is_zero() {
        let result = ImportResult::default();
        assert_eq!(result.imported, 0);
        assert_eq!(result.reviewed, 0);
        assert_eq!(result.skipped, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.duration_ms, 0);
    }

    #[test]
    fn import_result_with_counts() {
        let result = ImportResult {
            imported: 10,
            reviewed: 2,
            skipped: 1,
            failed: 0,
            duration_ms: 350,
        };
        assert_eq!(result.imported, 10);
        assert_eq!(result.duration_ms, 350);
    }

    // -- SessionStatus variant access --

    #[test]
    fn session_status_import_variant() {
        let status = SessionStatus::Import(ImportSessionStatus::Validating);
        assert_eq!(
            status,
            SessionStatus::Import(ImportSessionStatus::Validating)
        );
    }

    #[test]
    fn session_status_export_variant() {
        let status = SessionStatus::Export(ExportSessionStatus::Exporting);
        assert_eq!(
            status,
            SessionStatus::Export(ExportSessionStatus::Exporting)
        );
    }

    #[test]
    fn session_status_match_branches() {
        let import_status = SessionStatus::Import(ImportSessionStatus::Completed);
        let export_status = SessionStatus::Export(ExportSessionStatus::Failed);

        let is_done = match import_status {
            SessionStatus::Import(ImportSessionStatus::Completed) => true,
            SessionStatus::Export(ExportSessionStatus::Completed) => true,
            _ => false,
        };
        assert!(is_done);

        let is_done = match export_status {
            SessionStatus::Import(ImportSessionStatus::Completed) => true,
            SessionStatus::Export(ExportSessionStatus::Completed) => true,
            _ => false,
        };
        assert!(!is_done);
    }

    // -- ValidationResult construction --

    #[test]
    fn validation_result_construction() {
        let result = ValidationResult {
            total_items: 100,
            importable: 80,
            needs_review: 15,
            failed: 5,
            review_items: vec![ReviewItem {
                name: "entry-a".to_string(),
                reason: "missing password".to_string(),
            }],
            failed_items: vec![FailedItem {
                name: "entry-b".to_string(),
                reason: "corrupt data".to_string(),
            }],
        };

        assert_eq!(result.total_items, 100);
        assert_eq!(result.importable + result.needs_review + result.failed, 100);
        assert_eq!(result.review_items.len(), 1);
        assert_eq!(result.failed_items.len(), 1);
    }
}
