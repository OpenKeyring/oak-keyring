// Audit log (query_audit_log, cleanup_audit_log, sync/DEK audit helpers)

#[cfg(test)]
use crate::services::vault::VaultService;
use crate::services::vault::VaultServiceImpl;
use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::commands::types::{AuditFilter, AuditTimeRange};
use crate::db::queries;
use crate::errors::mapping::vault::VaultError;
use crate::types::audit::{AuditEntry, AuditOperation};

// Default page size for audit log queries.
const DEFAULT_AUDIT_PAGE_SIZE: i64 = 50;

impl VaultServiceImpl {
    /// Internal helper: write an audit log entry.
    ///
    /// Thin wrapper around `queries::insert_audit_entry` that maps
    /// `DbError` to `VaultError`.
    pub fn _write_audit(
        &self,
        operation: AuditOperation,
        record_id: Option<Uuid>,
        record_name: Option<&str>,
        detail: Option<&str>,
    ) -> Result<(), VaultError> {
        queries::insert_audit_entry(
            &self.conn,
            operation,
            record_id.as_ref(),
            record_name,
            detail,
        )
        .map_err(record::db_error_to_vault)
    }

    /// Query the audit log with filters and pagination.
    ///
    /// Returns a tuple of `(matching entries, total count)`. The total count
    /// reflects all entries matching the filter, not just the current page.
    ///
    /// # Filter behavior
    /// - `operation` — exact match on the audit operation type.
    /// - `time_range` — constrains `occurred_at` to a time window.
    /// - `search` — case-insensitive LIKE match on `record_name` and `detail`.
    ///
    /// # Pagination
    /// The `AuditFilter` does not carry limit/offset. This method uses a
    /// default page size of 50 with offset 0, returning all matching entries
    /// up to that limit. Callers needing pagination can re-query with higher
    /// offsets.
    pub fn query_audit_log(
        &self,
        filter: &AuditFilter,
    ) -> Result<(Vec<AuditEntry>, usize), VaultError> {
        let operation_str = filter.operation.map(|op| op.to_db_str());

        let (time_start, time_end) = compute_time_bounds(filter.time_range);

        let search = filter.search.as_deref();

        let total =
            queries::count_audit_entries(&self.conn, operation_str, time_start, time_end, search)
                .map_err(record::db_error_to_vault)? as usize;

        let entries = queries::list_audit_entries_filtered(
            &self.conn,
            operation_str,
            time_start,
            time_end,
            search,
            DEFAULT_AUDIT_PAGE_SIZE,
            0,
        )
        .map_err(record::db_error_to_vault)?;

        Ok((entries, total))
    }

    /// Delete audit entries older than `retention_days` days.
    ///
    /// Returns the number of deleted entries.
    pub fn cleanup_audit_log(&mut self, retention_days: u32) -> Result<usize, VaultError> {
        let before_timestamp = (Utc::now() - Duration::days(retention_days as i64)).timestamp();
        queries::cleanup_audit_entries(&self.conn, before_timestamp)
            .map_err(record::db_error_to_vault)
    }

    /// Log that a sync conflict was resolved for a specific record.
    pub fn log_sync_conflict_resolved(
        &self,
        record_id: Uuid,
        record_name: &str,
        resolution: &str,
    ) -> Result<(), VaultError> {
        self._write_audit(
            AuditOperation::SyncConflictResolved,
            Some(record_id),
            Some(record_name),
            Some(resolution),
        )
    }

    /// Log that a batch of sync conflicts was resolved.
    pub fn log_sync_batch_conflicts_resolved(
        &self,
        count: usize,
        resolution: &str,
    ) -> Result<(), VaultError> {
        let detail = format!("count={}, resolution={}", count, resolution);
        self._write_audit(
            AuditOperation::SyncBatchConflictsResolved,
            None,
            None,
            Some(&detail),
        )
    }

    /// Log that a DEK rotation succeeded.
    pub fn log_dek_rotated(&self, detail: &str) -> Result<(), VaultError> {
        self._write_audit(AuditOperation::DekRotated, None, None, Some(detail))
    }

    /// Log that a DEK rotation failed.
    pub fn log_dek_rotation_failed(&self, detail: &str) -> Result<(), VaultError> {
        self._write_audit(AuditOperation::DekRotationFailed, None, None, Some(detail))
    }
}

/// Convert an optional `AuditTimeRange` into `(time_start, time_end)` unix
/// timestamps suitable for the filtered query functions.
fn compute_time_bounds(time_range: Option<AuditTimeRange>) -> (Option<i64>, Option<i64>) {
    match time_range {
        None | Some(AuditTimeRange::All) => (None, None),
        Some(AuditTimeRange::Today) => {
            let start_of_today = (Utc::now() - Duration::hours(24)).timestamp();
            (Some(start_of_today), None)
        }
        Some(AuditTimeRange::LastWeek) => {
            let start = (Utc::now() - Duration::weeks(1)).timestamp();
            (Some(start), None)
        }
        Some(AuditTimeRange::LastMonth) => {
            let start = (Utc::now() - Duration::days(30)).timestamp();
            (Some(start), None)
        }
        Some(AuditTimeRange::LastYear) => {
            let start = (Utc::now() - Duration::days(365)).timestamp();
            (Some(start), None)
        }
    }
}

// We need access to db_error_to_vault which lives in the record submodule.
use super::record;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries;

    use crate::db::schema::init_db_in_memory;

    /// Helper: create an in-memory VaultService with schema initialized.
    fn setup_service() -> VaultService {
        let conn = init_db_in_memory();
        VaultService::new(conn)
    }

    // =========================================================================
    // _write_audit -> query_audit_log roundtrip
    // =========================================================================

    #[test]
    fn write_audit_then_query_finds_it() {
        let svc = setup_service();

        let record_id = Uuid::new_v4();
        svc._write_audit(
            AuditOperation::RecordCreate,
            Some(record_id),
            Some("TestRecord"),
            None,
        )
        .unwrap();

        let filter = AuditFilter {
            operation: None,
            time_range: None,
            search: None,
        };
        let (entries, total) = svc.query_audit_log(&filter).unwrap();

        assert_eq!(total, 1, "total count should be 1");
        assert_eq!(entries.len(), 1, "should return 1 entry");
        assert_eq!(entries[0].operation, AuditOperation::RecordCreate);
        assert_eq!(entries[0].record_id, Some(record_id));
        assert_eq!(entries[0].record_name.as_deref(), Some("TestRecord"));
    }

    // =========================================================================
    // filter by operation returns only matching entries
    // =========================================================================

    #[test]
    fn query_audit_log_filter_by_operation() {
        let svc = setup_service();

        svc._write_audit(AuditOperation::RecordCreate, None, None, None)
            .unwrap();
        svc._write_audit(AuditOperation::RecordDelete, None, None, None)
            .unwrap();
        svc._write_audit(AuditOperation::RecordCreate, None, None, None)
            .unwrap();

        let filter = AuditFilter {
            operation: Some(AuditOperation::RecordCreate),
            time_range: None,
            search: None,
        };
        let (entries, total) = svc.query_audit_log(&filter).unwrap();

        assert_eq!(total, 2, "should find 2 RecordCreate entries");
        assert_eq!(entries.len(), 2);
        assert!(entries
            .iter()
            .all(|e| e.operation == AuditOperation::RecordCreate));
    }

    // =========================================================================
    // filter by AuditTimeRange::LastWeek
    // =========================================================================

    #[test]
    fn query_audit_log_filter_by_last_week() {
        let svc = setup_service();

        // Insert an entry with a timestamp 3 days ago
        let three_days_ago = (Utc::now() - Duration::days(3)).timestamp();
        svc.conn
            .execute(
                "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
                rusqlite::params!["record.create", three_days_ago],
            )
            .unwrap();

        // Insert an entry with a timestamp 10 days ago (outside LastWeek)
        let ten_days_ago = (Utc::now() - Duration::days(10)).timestamp();
        svc.conn
            .execute(
                "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
                rusqlite::params!["record.update", ten_days_ago],
            )
            .unwrap();

        // Insert a current entry
        svc._write_audit(AuditOperation::RecordDelete, None, None, None)
            .unwrap();

        let filter = AuditFilter {
            operation: None,
            time_range: Some(AuditTimeRange::LastWeek),
            search: None,
        };
        let (entries, total) = svc.query_audit_log(&filter).unwrap();

        assert_eq!(
            total, 2,
            "LastWeek should include entries from the last 7 days"
        );
        assert_eq!(entries.len(), 2);

        let ops: Vec<AuditOperation> = entries.iter().map(|e| e.operation).collect();
        assert!(ops.contains(&AuditOperation::RecordCreate));
        assert!(ops.contains(&AuditOperation::RecordDelete));
        assert!(!ops.contains(&AuditOperation::RecordUpdate));
    }

    // =========================================================================
    // search keyword matches record_name
    // =========================================================================

    #[test]
    fn query_audit_log_search_matches_record_name() {
        let svc = setup_service();

        svc._write_audit(
            AuditOperation::RecordCreate,
            None,
            Some("GitHub Token"),
            None,
        )
        .unwrap();
        svc._write_audit(AuditOperation::RecordCreate, None, Some("AWS Key"), None)
            .unwrap();

        let filter = AuditFilter {
            operation: None,
            time_range: None,
            search: Some("GitHub".to_string()),
        };
        let (entries, total) = svc.query_audit_log(&filter).unwrap();

        assert_eq!(total, 1);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].record_name.as_deref(), Some("GitHub Token"));
    }

    // =========================================================================
    // cleanup_audit_log deletes old entries, returns count
    // =========================================================================

    #[test]
    fn cleanup_deletes_entries_older_than_retention_days() {
        let mut svc = setup_service();

        // Insert entries at specific timestamps
        let old_timestamp = (Utc::now() - Duration::days(45)).timestamp();
        svc.conn
            .execute(
                "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
                rusqlite::params!["record.create", old_timestamp],
            )
            .unwrap();

        let also_old = (Utc::now() - Duration::days(35)).timestamp();
        svc.conn
            .execute(
                "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
                rusqlite::params!["record.update", also_old],
            )
            .unwrap();

        // Insert a recent entry
        svc._write_audit(AuditOperation::RecordDelete, None, None, None)
            .unwrap();

        // Cleanup entries older than 30 days
        let deleted = svc.cleanup_audit_log(30).unwrap();
        assert_eq!(deleted, 2, "should delete 2 entries older than 30 days");

        // Verify remaining entries
        let remaining = queries::list_audit_entries(&svc.conn, 100, 0).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].operation, AuditOperation::RecordDelete);
    }

    // =========================================================================
    // log_sync_conflict_resolved writes sync.conflict_resolved
    // =========================================================================

    #[test]
    fn log_sync_conflict_resolved_writes_correct_operation() {
        let svc = setup_service();

        let record_id = Uuid::new_v4();
        svc.log_sync_conflict_resolved(record_id, "MyRecord", "keep_local")
            .unwrap();

        let entries = queries::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, AuditOperation::SyncConflictResolved);
        assert_eq!(entries[0].record_id, Some(record_id));
        assert_eq!(entries[0].record_name.as_deref(), Some("MyRecord"));
        assert_eq!(entries[0].detail.as_deref(), Some("keep_local"));
    }

    // =========================================================================
    // log_dek_rotated writes dek.rotated
    // =========================================================================

    #[test]
    fn log_dek_rotated_writes_correct_operation() {
        let svc = setup_service();

        svc.log_dek_rotated("DEK v1 -> v2").unwrap();

        let entries = queries::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, AuditOperation::DekRotated);
        assert_eq!(entries[0].detail.as_deref(), Some("DEK v1 -> v2"));
    }

    // =========================================================================
    // log_dek_rotation_failed writes dek.rotation_failed
    // =========================================================================

    #[test]
    fn log_dek_rotation_failed_writes_correct_operation() {
        let svc = setup_service();

        svc.log_dek_rotation_failed("timeout during rotation")
            .unwrap();

        let entries = queries::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, AuditOperation::DekRotationFailed);
        assert_eq!(
            entries[0].detail.as_deref(),
            Some("timeout during rotation")
        );
    }

    // =========================================================================
    // log_sync_batch_conflicts_resolved
    // =========================================================================

    #[test]
    fn log_sync_batch_conflicts_resolved_writes_correct_operation() {
        let svc = setup_service();

        svc.log_sync_batch_conflicts_resolved(5, "keep_remote")
            .unwrap();

        let entries = queries::list_audit_entries(&svc.conn, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].operation,
            AuditOperation::SyncBatchConflictsResolved
        );
        assert!(entries[0].detail.as_deref().unwrap().contains("count=5"));
        assert!(entries[0]
            .detail
            .as_deref()
            .unwrap()
            .contains("keep_remote"));
    }

    // =========================================================================
    // query_audit_log with combined filters
    // =========================================================================

    #[test]
    fn query_audit_log_combined_operation_and_search() {
        let svc = setup_service();

        svc._write_audit(AuditOperation::RecordCreate, None, Some("GitHub"), None)
            .unwrap();
        svc._write_audit(AuditOperation::RecordDelete, None, Some("GitHub"), None)
            .unwrap();
        svc._write_audit(AuditOperation::RecordCreate, None, Some("AWS"), None)
            .unwrap();

        let filter = AuditFilter {
            operation: Some(AuditOperation::RecordCreate),
            time_range: None,
            search: Some("GitHub".to_string()),
        };
        let (entries, total) = svc.query_audit_log(&filter).unwrap();

        assert_eq!(total, 1, "only RecordCreate + GitHub matches");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, AuditOperation::RecordCreate);
        assert_eq!(entries[0].record_name.as_deref(), Some("GitHub"));
    }

    // =========================================================================
    // query_audit_log with no matches returns empty
    // =========================================================================

    #[test]
    fn query_audit_log_no_matches_returns_empty() {
        let svc = setup_service();

        let filter = AuditFilter {
            operation: Some(AuditOperation::RecordDestroy),
            time_range: None,
            search: None,
        };
        let (entries, total) = svc.query_audit_log(&filter).unwrap();

        assert_eq!(total, 0);
        assert!(entries.is_empty());
    }

    // =========================================================================
    // cleanup_audit_log with zero retention days deletes nothing recent
    // =========================================================================

    #[test]
    fn cleanup_audit_log_with_large_retention_deletes_nothing() {
        let mut svc = setup_service();

        svc._write_audit(AuditOperation::RecordCreate, None, None, None)
            .unwrap();

        let deleted = svc.cleanup_audit_log(365).unwrap();
        assert_eq!(deleted, 0, "recent entries should not be deleted");
    }
}
