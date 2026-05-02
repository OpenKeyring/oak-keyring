use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::{datetime_to_timestamp, AuditLogRow};
use crate::types::audit::{AuditEntry, AuditOperation};

use super::Result;

/// Insert an audit log entry with the current timestamp.
pub fn insert_audit_entry(
    conn: &Connection,
    operation: AuditOperation,
    record_id: Option<&Uuid>,
    record_name: Option<&str>,
    detail: Option<&str>,
) -> Result<()> {
    let now = datetime_to_timestamp(&Utc::now());
    conn.execute(
        "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            operation.to_db_str(),
            record_id.map(|u| u.to_string()),
            record_name,
            detail,
            now,
        ],
    )?;
    Ok(())
}

/// List audit entries ordered by `occurred_at` descending, with pagination.
pub fn list_audit_entries(conn: &Connection, limit: i64, offset: i64) -> Result<Vec<AuditEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, operation, record_id, record_name, detail, occurred_at
         FROM audit_log ORDER BY occurred_at DESC LIMIT ?1 OFFSET ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![limit, offset], AuditLogRow::from_row)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?.to_audit_entry()?);
    }
    Ok(entries)
}

/// List audit entries with dynamic filtering and pagination.
///
/// All filter parameters are optional; non-None parameters are combined with AND.
pub fn list_audit_entries_filtered(
    conn: &Connection,
    operation: Option<&str>,
    time_start: Option<i64>,
    time_end: Option<i64>,
    search: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEntry>> {
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(op) = operation {
        conditions.push(format!("operation = ?{param_idx}"));
        params.push(Box::new(op.to_string()));
        param_idx += 1;
    }
    if let Some(start) = time_start {
        conditions.push(format!("occurred_at >= ?{param_idx}"));
        params.push(Box::new(start));
        param_idx += 1;
    }
    if let Some(end) = time_end {
        conditions.push(format!("occurred_at <= ?{param_idx}"));
        params.push(Box::new(end));
        param_idx += 1;
    }
    if let Some(q) = search {
        conditions.push(format!(
            "(record_name LIKE ?{param_idx} OR detail LIKE ?{param_idx})"
        ));
        params.push(Box::new(format!("%{q}%")));
        param_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT id, operation, record_id, record_name, detail, occurred_at
         FROM audit_log {where_clause}
         ORDER BY occurred_at DESC
         LIMIT ?{param_idx} OFFSET ?{}",
        param_idx + 1
    );
    params.push(Box::new(limit));
    params.push(Box::new(offset));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(params), AuditLogRow::from_row)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?.to_audit_entry()?);
    }
    Ok(entries)
}

/// Delete audit entries older than the given timestamp.
///
/// Returns the number of deleted rows.
pub fn cleanup_audit_entries(conn: &Connection, before_timestamp: i64) -> Result<usize> {
    let affected = conn.execute(
        "DELETE FROM audit_log WHERE occurred_at < ?1",
        rusqlite::params![before_timestamp],
    )?;
    Ok(affected)
}

/// Count audit entries matching the given filters.
///
/// Uses the same filter parameters as `list_audit_entries_filtered`.
pub fn count_audit_entries(
    conn: &Connection,
    operation: Option<&str>,
    time_start: Option<i64>,
    time_end: Option<i64>,
    search: Option<&str>,
) -> Result<i64> {
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut param_idx = 1;

    if let Some(op) = operation {
        conditions.push(format!("operation = ?{param_idx}"));
        params.push(Box::new(op.to_string()));
        param_idx += 1;
    }
    if let Some(start) = time_start {
        conditions.push(format!("occurred_at >= ?{param_idx}"));
        params.push(Box::new(start));
        param_idx += 1;
    }
    if let Some(end) = time_end {
        conditions.push(format!("occurred_at <= ?{param_idx}"));
        params.push(Box::new(end));
        param_idx += 1;
    }
    if let Some(q) = search {
        conditions.push(format!(
            "(record_name LIKE ?{param_idx} OR detail LIKE ?{param_idx})"
        ));
        params.push(Box::new(format!("%{q}%")));
        // param_idx not incremented — count query has no LIMIT/OFFSET params
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!("SELECT COUNT(*) FROM audit_log {where_clause}");

    let count: i64 = conn.query_row(&sql, rusqlite::params_from_iter(params), |row| row.get(0))?;

    Ok(count)
}
