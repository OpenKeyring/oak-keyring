// TODO: Remove `#![allow(dead_code)]` once the services layer (Plan F/S1) is wired up,
//       and all query functions are used. For now, queries are not yet consumed by any call site.
//
// TODO(#S1): Revisit `pub` visibility on all query functions before v1.0 release.
//       Currently `pub` to allow integration tests in `tests/integration/` to access them.
//       Consider `pub(crate)` + test via a test module inside the crate, or `#[cfg(test)]` gates.
#![allow(dead_code)]

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::{
    datetime_to_timestamp, timestamp_to_datetime, AuditLogRow, RecordRow, TagRow,
};
use crate::types::audit::{AuditEntry, AuditOperation};
use crate::types::history::PasswordHistory;
use crate::types::record::StoredRecord;
use crate::types::tag::Tag;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("data conversion error: {0}")]
    Data(#[from] crate::types::credential::DataError),
    #[error("invalid UUID: {0}")]
    Uuid(#[from] uuid::Error),
}

type Result<T> = std::result::Result<T, DbError>;

// ---------------------------------------------------------------------------
// Record CRUD queries
// ---------------------------------------------------------------------------

/// Insert a new record with all fields using a parameterized query.
///
/// Wrapped in a transaction so that record + tags are atomic — a partial
/// failure rolls back the entire insert.
pub fn insert_record(conn: &Connection, record: &StoredRecord) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    let aad_param: Option<&[u8]> = if record.aad.is_empty() {
        None
    } else {
        Some(&record.aad)
    };

    tx.execute(
        "INSERT INTO records
            (id, credential_type, encrypted_data, nonce, dek_version, aad,
             is_favorite, expires_at, created_at, updated_at, updated_by,
             version, deleted, deleted_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        rusqlite::params![
            record.id.to_string(),
            record.credential_type.to_db_str(),
            record.encrypted_data,
            record.nonce.as_slice(),
            record.dek_version,
            aad_param,
            record.is_favorite as i64,
            record.expires_at.map(|dt| datetime_to_timestamp(&dt)),
            datetime_to_timestamp(&record.created_at),
            datetime_to_timestamp(&record.updated_at),
            record.updated_by,
            record.version as i64,
            record.deleted as i64,
            record.deleted_at.map(|dt| datetime_to_timestamp(&dt)),
        ],
    )?;

    // Insert tags: ensure each tag exists in `tags` table and link via `record_tags`.
    for tag_name in &record.tags {
        tx.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            rusqlite::params![tag_name],
        )?;

        let tag_id: i64 = tx.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            rusqlite::params![tag_name],
            |row| row.get(0),
        )?;

        tx.execute(
            "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![record.id.to_string(), tag_id],
        )?;
    }

    tx.commit()?;
    Ok(())
}

/// Fetch a single record by ID. Returns `None` if not found.
pub fn get_record(conn: &Connection, id: &Uuid) -> Result<Option<StoredRecord>> {
    let id_str = id.to_string();
    let result = conn.query_row(
        "SELECT * FROM records WHERE id = ?1",
        rusqlite::params![id_str],
        RecordRow::from_row,
    );

    match result {
        Ok(row) => {
            let tags = get_record_tags_inner(conn, id)?;
            let record = row.to_stored_record(tags)?;
            Ok(Some(record))
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::from(e)),
    }
}

/// List all active (non-deleted) records, ordered by `updated_at` descending.
pub fn list_active_records(conn: &Connection) -> Result<Vec<StoredRecord>> {
    let mut stmt =
        conn.prepare("SELECT * FROM records WHERE deleted = 0 ORDER BY updated_at DESC")?;

    let rows = stmt.query_map([], RecordRow::from_row)?;

    let mut records = Vec::new();
    for row_result in rows {
        let row = row_result?;
        let id = Uuid::parse_str(&row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = row.to_stored_record(tags)?;
        records.push(record);
    }

    Ok(records)
}

/// Soft-delete a record: set `deleted = 1` and `deleted_at` to now.
pub fn soft_delete_record(conn: &Connection, id: &Uuid) -> Result<()> {
    let now = datetime_to_timestamp(&Utc::now());
    conn.execute(
        "UPDATE records SET deleted = 1, deleted_at = ?1 WHERE id = ?2",
        rusqlite::params![now, id.to_string()],
    )?;
    Ok(())
}

/// Restore a soft-deleted record: set `deleted = 0` and `deleted_at = NULL`.
pub fn restore_record(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "UPDATE records SET deleted = 0, deleted_at = NULL WHERE id = ?1",
        rusqlite::params![id.to_string()],
    )?;
    Ok(())
}

/// Hard-delete a record (and cascading record_tags via FK).
pub fn hard_delete_record(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM records WHERE id = ?1",
        rusqlite::params![id.to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Get tag names for a record via JOIN on `record_tags` + `tags`.
fn get_record_tags_inner(conn: &Connection, record_id: &Uuid) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT t.name FROM tags t
         INNER JOIN record_tags rt ON rt.tag_id = t.id
         WHERE rt.record_id = ?1",
    )?;

    let rows = stmt.query_map(rusqlite::params![record_id.to_string()], |row| {
        row.get::<_, String>(0)
    })?;

    let mut tags = Vec::new();
    for tag in rows {
        tags.push(tag?);
    }

    Ok(tags)
}

// ---------------------------------------------------------------------------
// Tag queries
// ---------------------------------------------------------------------------

/// Insert a new tag and return it with the auto-generated ID.
pub fn insert_tag(conn: &Connection, name: &str) -> Result<Tag> {
    conn.execute(
        "INSERT INTO tags (name) VALUES (?1)",
        rusqlite::params![name],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Tag {
        id,
        name: name.to_string(),
    })
}

/// Return an existing tag by name, or create it if missing.
pub fn get_or_create_tag(conn: &Connection, name: &str) -> Result<Tag> {
    let existing = conn.query_row(
        "SELECT id, name FROM tags WHERE name = ?1",
        rusqlite::params![name],
        |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        },
    );

    match existing {
        Ok(tag) => Ok(tag),
        Err(rusqlite::Error::QueryReturnedNoRows) => insert_tag(conn, name),
        Err(e) => Err(DbError::from(e)),
    }
}

/// List all tags ordered alphabetically by name.
pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare("SELECT id, name FROM tags ORDER BY name")?;
    let rows = stmt.query_map([], |row| {
        Ok(TagRow {
            id: row.get(0)?,
            name: row.get(1)?,
        })
    })?;

    let mut tags = Vec::new();
    for row in rows {
        tags.push(row?.to_tag());
    }
    Ok(tags)
}

/// Attach a tag to a record. Idempotent (INSERT OR IGNORE).
pub fn attach_tag(conn: &Connection, record_id: &Uuid, tag_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![record_id.to_string(), tag_id],
    )?;
    Ok(())
}

/// Detach a tag from a record.
pub fn detach_tag(conn: &Connection, record_id: &Uuid, tag_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM record_tags WHERE record_id = ?1 AND tag_id = ?2",
        rusqlite::params![record_id.to_string(), tag_id],
    )?;
    Ok(())
}

/// Detach all tags from a record.
pub fn detach_all_tags_for_record(conn: &Connection, record_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM record_tags WHERE record_id = ?1",
        rusqlite::params![record_id.to_string()],
    )?;
    Ok(())
}

/// Public wrapper: get tag names for a record.
pub fn get_record_tags(conn: &Connection, record_id: &Uuid) -> Result<Vec<String>> {
    get_record_tags_inner(conn, record_id)
}

// ---------------------------------------------------------------------------
// Audit queries
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Metadata queries
// ---------------------------------------------------------------------------

/// Get a metadata value by key. Returns `None` if the key does not exist.
pub fn get_metadata(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        rusqlite::params![key],
        |row| row.get::<_, String>(0),
    );

    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::from(e)),
    }
}

/// Set (insert or replace) a metadata key-value pair.
pub fn set_metadata(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Record update queries
// ---------------------------------------------------------------------------

/// Update a record with optimistic locking.
///
/// Returns `Ok(true)` if the update succeeded (version matched), or
/// `Ok(false)` if no rows were affected (version mismatch or record not found).
pub fn update_record(
    conn: &Connection,
    record: &StoredRecord,
    expected_version: u64,
) -> Result<bool> {
    let aad_param: Option<&[u8]> = if record.aad.is_empty() {
        None
    } else {
        Some(&record.aad)
    };

    let affected = conn.execute(
        "UPDATE records SET
            encrypted_data = ?1,
            nonce = ?2,
            dek_version = ?3,
            aad = ?4,
            is_favorite = ?5,
            expires_at = ?6,
            updated_at = ?7,
            updated_by = ?8,
            version = ?9
         WHERE id = ?10 AND version = ?11",
        rusqlite::params![
            record.encrypted_data,
            record.nonce.as_slice(),
            record.dek_version,
            aad_param,
            record.is_favorite as i64,
            record.expires_at.map(|dt| datetime_to_timestamp(&dt)),
            datetime_to_timestamp(&record.updated_at),
            record.updated_by,
            (record.version) as i64,
            record.id.to_string(),
            expected_version as i64,
        ],
    )?;

    Ok(affected > 0)
}

/// List all records (including soft-deleted) where `dek_version < target`.
/// Used for DEK rotation migration.
pub fn list_records_by_dek_version(
    conn: &Connection,
    target_version: u32,
) -> Result<Vec<StoredRecord>> {
    let mut stmt = conn.prepare("SELECT * FROM records WHERE dek_version < ?1")?;

    let rows = stmt.query_map(rusqlite::params![target_version], RecordRow::from_row)?;

    let mut records = Vec::new();
    for row_result in rows {
        let row = row_result?;
        let id = Uuid::parse_str(&row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = row.to_stored_record(tags)?;
        records.push(record);
    }

    Ok(records)
}

/// List all soft-deleted records, ordered by `deleted_at` descending.
pub fn list_deleted_records(conn: &Connection) -> Result<Vec<StoredRecord>> {
    let mut stmt =
        conn.prepare("SELECT * FROM records WHERE deleted = 1 ORDER BY deleted_at DESC")?;

    let rows = stmt.query_map([], RecordRow::from_row)?;

    let mut records = Vec::new();
    for row_result in rows {
        let row = row_result?;
        let id = Uuid::parse_str(&row.id).map_err(DbError::Uuid)?;
        let tags = get_record_tags_inner(conn, &id)?;
        let record = row.to_stored_record(tags)?;
        records.push(record);
    }

    Ok(records)
}

// ---------------------------------------------------------------------------
// Password history queries
// ---------------------------------------------------------------------------

/// Insert a password history entry for a record.
pub fn insert_password_history(
    conn: &Connection,
    record_id: &Uuid,
    encrypted_password: &[u8],
    nonce: &[u8; 24],
    dek_version: u32,
    changed_at: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO password_history (record_id, encrypted_password, nonce, dek_version, changed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            record_id.to_string(),
            encrypted_password,
            nonce.as_slice(),
            dek_version,
            changed_at,
        ],
    )?;
    Ok(())
}

/// Get password history for a record, ordered by `changed_at` descending.
pub fn get_password_history(
    conn: &Connection,
    record_id: &Uuid,
    limit: i64,
) -> Result<Vec<PasswordHistory>> {
    let mut stmt = conn.prepare(
        "SELECT id, record_id, encrypted_password, nonce, dek_version, changed_at
         FROM password_history
         WHERE record_id = ?1
         ORDER BY changed_at DESC
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(rusqlite::params![record_id.to_string(), limit], |row| {
        let nonce_vec: Vec<u8> = row.get("nonce")?;
        let nonce: [u8; 24] = nonce_vec.try_into().map_err(|_| {
            rusqlite::Error::FromSqlConversionFailure(
                24,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::other("nonce must be 24 bytes")),
            )
        })?;
        Ok(PasswordHistory {
            id: row.get("id")?,
            record_id: Uuid::parse_str(&row.get::<_, String>("record_id")?).unwrap(),
            encrypted_password: row.get("encrypted_password")?,
            nonce,
            dek_version: row.get::<_, i64>("dek_version")? as u32,
            changed_at: timestamp_to_datetime(row.get("changed_at")?),
        })
    })?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Count password history entries for a record.
pub fn count_password_history(conn: &Connection, record_id: &Uuid) -> Result<i64> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM password_history WHERE record_id = ?1",
        rusqlite::params![record_id.to_string()],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Delete the oldest password history entry for a record.
pub fn delete_oldest_password_history(conn: &Connection, record_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM password_history WHERE id = (
            SELECT id FROM password_history
            WHERE record_id = ?1
            ORDER BY changed_at ASC
            LIMIT 1
        )",
        rusqlite::params![record_id.to_string()],
    )?;
    Ok(())
}

/// Get a single password history entry by its ID.
///
/// Returns `None` if no entry with the given ID exists.
pub fn get_password_history_by_id(
    conn: &Connection,
    history_id: i64,
) -> Result<Option<PasswordHistory>> {
    let result = conn.query_row(
        "SELECT id, record_id, encrypted_password, nonce, dek_version, changed_at
         FROM password_history
         WHERE id = ?1",
        rusqlite::params![history_id],
        |row| {
            let nonce_vec: Vec<u8> = row.get("nonce")?;
            let nonce: [u8; 24] = nonce_vec.try_into().map_err(|_| {
                rusqlite::Error::FromSqlConversionFailure(
                    24,
                    rusqlite::types::Type::Blob,
                    Box::new(std::io::Error::other("nonce must be 24 bytes")),
                )
            })?;
            Ok(PasswordHistory {
                id: row.get("id")?,
                record_id: Uuid::parse_str(&row.get::<_, String>("record_id")?).unwrap(),
                encrypted_password: row.get("encrypted_password")?,
                nonce,
                dek_version: row.get::<_, i64>("dek_version")? as u32,
                changed_at: timestamp_to_datetime(row.get("changed_at")?),
            })
        },
    );

    match result {
        Ok(entry) => Ok(Some(entry)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::from(e)),
    }
}

/// Delete all password history entries for a record.
pub fn delete_password_history_by_record(conn: &Connection, record_id: &Uuid) -> Result<()> {
    conn.execute(
        "DELETE FROM password_history WHERE record_id = ?1",
        rusqlite::params![record_id.to_string()],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Batch operations
// ---------------------------------------------------------------------------

/// Batch soft-delete multiple records.
///
/// Returns the number of affected rows.
pub fn batch_soft_delete_records(
    conn: &Connection,
    ids: &[Uuid],
    deleted_by: &str,
) -> Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let now = datetime_to_timestamp(&Utc::now());

    // Build dynamic parameterized IN clause: WHERE id IN (?3, ?4, ...)
    // Parameters ?1 = deleted_at, ?2 = updated_by, ?3.. = ids
    let placeholders: Vec<String> = (3..=(ids.len() + 2)).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "UPDATE records SET deleted = 1, deleted_at = ?1, updated_by = ?2 WHERE id IN ({})",
        placeholders.join(", ")
    );

    // Build parameter list: [now, deleted_by, id1, id2, ...]
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::with_capacity(ids.len() + 2);
    params.push(Box::new(now));
    params.push(Box::new(deleted_by.to_string()));
    for id in ids {
        params.push(Box::new(id.to_string()));
    }

    let affected = conn.execute(&sql, rusqlite::params_from_iter(params))?;
    Ok(affected)
}

// ---------------------------------------------------------------------------
// Filtered audit queries
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Tag management queries
// ---------------------------------------------------------------------------

/// Rename a tag. Returns `true` if a row was updated, `false` if the old name was not found.
pub fn rename_tag(conn: &Connection, old_name: &str, new_name: &str) -> Result<bool> {
    let affected = conn.execute(
        "UPDATE tags SET name = ?1 WHERE name = ?2",
        rusqlite::params![new_name, old_name],
    )?;
    Ok(affected > 0)
}

/// Delete a tag by name. Cascade will remove associated `record_tags` entries via FK.
///
/// Returns `true` if a row was deleted, `false` if the tag was not found.
pub fn delete_tag_by_name(conn: &Connection, name: &str) -> Result<bool> {
    let affected = conn.execute("DELETE FROM tags WHERE name = ?1", rusqlite::params![name])?;
    Ok(affected > 0)
}

/// Find a tag by name. Returns `None` if not found.
pub fn get_tag_by_name(conn: &Connection, name: &str) -> Result<Option<Tag>> {
    let result = conn.query_row(
        "SELECT id, name FROM tags WHERE name = ?1",
        rusqlite::params![name],
        |row| {
            Ok(Tag {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        },
    );

    match result {
        Ok(tag) => Ok(Some(tag)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(DbError::from(e)),
    }
}

// ---------------------------------------------------------------------------
// Metadata management queries
// ---------------------------------------------------------------------------

/// Delete a metadata entry by key.
///
/// Returns `true` if a row was deleted, `false` if the key was not found.
pub fn delete_metadata(conn: &Connection, key: &str) -> Result<bool> {
    let affected = conn.execute(
        "DELETE FROM metadata WHERE key = ?1",
        rusqlite::params![key],
    )?;
    Ok(affected > 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;
    use crate::types::credential::CredentialType;

    /// Create an in-memory database with schema initialized.
    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().expect("failed to open in-memory db");
        schema::apply_pragmas(&conn);
        schema::initialize_schema(&conn);
        schema::initialize_metadata(&conn);
        conn
    }

    /// Build a test `StoredRecord` with sensible defaults.
    fn make_test_record(id: &Uuid, version: u64) -> StoredRecord {
        StoredRecord {
            id: *id,
            credential_type: CredentialType::Login,
            encrypted_data: vec![1, 2, 3, 4],
            nonce: [0u8; 24],
            dek_version: 1,
            aad: vec![],
            is_favorite: false,
            expires_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            updated_by: "test".to_string(),
            version,
            deleted: false,
            deleted_at: None,
            tags: vec![],
        }
    }

    // -----------------------------------------------------------------------
    // 1. update_record
    // -----------------------------------------------------------------------

    #[test]
    fn update_record_succeeds_when_version_matches() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        let record = make_test_record(&id, 1);
        insert_record(&conn, &record).unwrap();

        let mut updated = record.clone();
        updated.encrypted_data = vec![9, 8, 7];
        updated.updated_by = "updater".to_string();
        updated.version = 2;

        let result = update_record(&conn, &updated, 1).unwrap();
        assert!(result, "should return true when version matches");

        let fetched = get_record(&conn, &id).unwrap().unwrap();
        assert_eq!(fetched.encrypted_data, vec![9, 8, 7]);
        assert_eq!(fetched.updated_by, "updater");
        assert_eq!(fetched.version, 2);
    }

    #[test]
    fn update_record_fails_when_version_mismatch() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        let record = make_test_record(&id, 1);
        insert_record(&conn, &record).unwrap();

        let mut updated = record.clone();
        updated.version = 2;

        let result = update_record(&conn, &updated, 99).unwrap();
        assert!(!result, "should return false when version does not match");
    }

    #[test]
    fn update_record_fails_when_record_not_found() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        let record = make_test_record(&id, 1);
        // Not inserted

        let result = update_record(&conn, &record, 1).unwrap();
        assert!(!result, "should return false when record does not exist");
    }

    // -----------------------------------------------------------------------
    // 2. list_deleted_records
    // -----------------------------------------------------------------------

    #[test]
    fn list_deleted_records_returns_only_soft_deleted() {
        let conn = setup_db();
        let id_active = Uuid::new_v4();
        let id_deleted = Uuid::new_v4();

        insert_record(&conn, &make_test_record(&id_active, 1)).unwrap();
        insert_record(&conn, &make_test_record(&id_deleted, 1)).unwrap();
        soft_delete_record(&conn, &id_deleted).unwrap();

        let deleted = list_deleted_records(&conn).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].id, id_deleted);
        assert!(deleted[0].deleted);
    }

    #[test]
    fn list_deleted_records_returns_empty_when_none_deleted() {
        let conn = setup_db();
        let deleted = list_deleted_records(&conn).unwrap();
        assert!(deleted.is_empty());
    }

    // -----------------------------------------------------------------------
    // 3. insert_password_history
    // -----------------------------------------------------------------------

    #[test]
    fn insert_password_history_adds_entry() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        insert_record(&conn, &make_test_record(&id, 1)).unwrap();

        let nonce = [42u8; 24];
        let now_ts = datetime_to_timestamp(&Utc::now());
        insert_password_history(&conn, &id, &[1, 2, 3], &nonce, 1, now_ts).unwrap();

        let count = count_password_history(&conn, &id).unwrap();
        assert_eq!(count, 1);
    }

    // -----------------------------------------------------------------------
    // 4. get_password_history
    // -----------------------------------------------------------------------

    #[test]
    fn get_password_history_returns_entries_ordered_desc() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        insert_record(&conn, &make_test_record(&id, 1)).unwrap();

        let nonce = [0u8; 24];
        insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
        insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();
        insert_password_history(&conn, &id, &[3], &nonce, 1, 300).unwrap();

        let history = get_password_history(&conn, &id, 10).unwrap();
        assert_eq!(history.len(), 3);
        // Descending order by changed_at
        assert_eq!(history[0].encrypted_password, vec![3]);
        assert_eq!(history[1].encrypted_password, vec![2]);
        assert_eq!(history[2].encrypted_password, vec![1]);
    }

    #[test]
    fn get_password_history_respects_limit() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        insert_record(&conn, &make_test_record(&id, 1)).unwrap();

        let nonce = [0u8; 24];
        insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
        insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();

        let history = get_password_history(&conn, &id, 1).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].encrypted_password, vec![2]);
    }

    // -----------------------------------------------------------------------
    // 5. count_password_history
    // -----------------------------------------------------------------------

    #[test]
    fn count_password_history_returns_zero_for_no_entries() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        assert_eq!(count_password_history(&conn, &id).unwrap(), 0);
    }

    // -----------------------------------------------------------------------
    // 6. delete_oldest_password_history
    // -----------------------------------------------------------------------

    #[test]
    fn delete_oldest_password_history_removes_earliest() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        insert_record(&conn, &make_test_record(&id, 1)).unwrap();

        let nonce = [0u8; 24];
        insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
        insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();

        delete_oldest_password_history(&conn, &id).unwrap();

        let history = get_password_history(&conn, &id, 10).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].encrypted_password, vec![2]);
    }

    // -----------------------------------------------------------------------
    // 7. delete_password_history_by_record
    // -----------------------------------------------------------------------

    #[test]
    fn delete_password_history_by_record_removes_all() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        insert_record(&conn, &make_test_record(&id, 1)).unwrap();

        let nonce = [0u8; 24];
        insert_password_history(&conn, &id, &[1], &nonce, 1, 100).unwrap();
        insert_password_history(&conn, &id, &[2], &nonce, 1, 200).unwrap();
        insert_password_history(&conn, &id, &[3], &nonce, 1, 300).unwrap();

        delete_password_history_by_record(&conn, &id).unwrap();

        assert_eq!(count_password_history(&conn, &id).unwrap(), 0);
    }

    // -----------------------------------------------------------------------
    // 8. batch_soft_delete_records
    // -----------------------------------------------------------------------

    #[test]
    fn batch_soft_delete_records_deletes_multiple() {
        let conn = setup_db();
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        insert_record(&conn, &make_test_record(&id1, 1)).unwrap();
        insert_record(&conn, &make_test_record(&id2, 1)).unwrap();
        insert_record(&conn, &make_test_record(&id3, 1)).unwrap();

        let affected = batch_soft_delete_records(&conn, &[id1, id2], "admin").unwrap();
        assert_eq!(affected, 2);

        let deleted = list_deleted_records(&conn).unwrap();
        assert_eq!(deleted.len(), 2);

        // id3 should still be active
        let active = list_active_records(&conn).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id3);
    }

    #[test]
    fn batch_soft_delete_records_handles_empty_ids() {
        let conn = setup_db();
        let affected = batch_soft_delete_records(&conn, &[], "admin").unwrap();
        assert_eq!(affected, 0);
    }

    #[test]
    fn batch_soft_delete_records_updates_deleted_by() {
        let conn = setup_db();
        let id = Uuid::new_v4();
        insert_record(&conn, &make_test_record(&id, 1)).unwrap();

        batch_soft_delete_records(&conn, &[id], "deleter_user").unwrap();

        let deleted = list_deleted_records(&conn).unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].updated_by, "deleter_user");
    }

    // -----------------------------------------------------------------------
    // 9. list_audit_entries_filtered
    // -----------------------------------------------------------------------

    #[test]
    fn list_audit_entries_filtered_with_no_filters() {
        let conn = setup_db();
        insert_audit_entry(
            &conn,
            AuditOperation::RecordCreate,
            None,
            Some("test"),
            None,
        )
        .unwrap();

        let entries = list_audit_entries_filtered(&conn, None, None, None, None, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn list_audit_entries_filtered_by_operation() {
        let conn = setup_db();
        insert_audit_entry(&conn, AuditOperation::RecordCreate, None, None, None).unwrap();
        insert_audit_entry(&conn, AuditOperation::RecordDelete, None, None, None).unwrap();

        let entries =
            list_audit_entries_filtered(&conn, Some("record.create"), None, None, None, 10, 0)
                .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].operation, AuditOperation::RecordCreate);
    }

    #[test]
    fn list_audit_entries_filtered_by_time_range() {
        let conn = setup_db();
        // Insert entries with specific timestamps by inserting directly
        conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["record.create", None::<String>, "early", None::<String>, 1000],
        ).unwrap();
        conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["record.update", None::<String>, "middle", None::<String>, 2000],
        ).unwrap();
        conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["record.delete", None::<String>, "late", None::<String>, 3000],
        ).unwrap();

        let entries =
            list_audit_entries_filtered(&conn, None, Some(1500), Some(2500), None, 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].record_name.as_deref(), Some("middle"));
    }

    #[test]
    fn list_audit_entries_filtered_by_search() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["record.create", None::<String>, "GitHub Token", None::<String>, 1000],
        ).unwrap();
        conn.execute(
            "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["record.create", None::<String>, "AWS Key", "some detail", 2000],
        ).unwrap();

        let entries =
            list_audit_entries_filtered(&conn, None, None, None, Some("GitHub"), 10, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].record_name.as_deref(), Some("GitHub Token"));
    }

    #[test]
    fn list_audit_entries_filtered_respects_pagination() {
        let conn = setup_db();
        for i in 0..5 {
            conn.execute(
                "INSERT INTO audit_log (operation, record_id, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params!["record.create", None::<String>, format!("entry_{i}"), None::<String>, i * 100],
            ).unwrap();
        }

        let page = list_audit_entries_filtered(&conn, None, None, None, None, 2, 1).unwrap();
        assert_eq!(page.len(), 2);
    }

    // -----------------------------------------------------------------------
    // 10. cleanup_audit_entries
    // -----------------------------------------------------------------------

    #[test]
    fn cleanup_audit_entries_removes_old_entries() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
            rusqlite::params!["record.create", 100],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
            rusqlite::params!["record.create", 200],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
            rusqlite::params!["record.create", 300],
        )
        .unwrap();

        let deleted = cleanup_audit_entries(&conn, 250).unwrap();
        assert_eq!(deleted, 2);

        let remaining = list_audit_entries(&conn, 100, 0).unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn cleanup_audit_entries_removes_none_when_all_newer() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO audit_log (operation, occurred_at) VALUES (?1, ?2)",
            rusqlite::params!["record.create", 5000],
        )
        .unwrap();

        let deleted = cleanup_audit_entries(&conn, 100).unwrap();
        assert_eq!(deleted, 0);
    }

    // -----------------------------------------------------------------------
    // 11. count_audit_entries
    // -----------------------------------------------------------------------

    #[test]
    fn count_audit_entries_with_no_filters() {
        let conn = setup_db();
        insert_audit_entry(&conn, AuditOperation::RecordCreate, None, None, None).unwrap();
        insert_audit_entry(&conn, AuditOperation::RecordDelete, None, None, None).unwrap();

        let count = count_audit_entries(&conn, None, None, None, None).unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn count_audit_entries_with_operation_filter() {
        let conn = setup_db();
        insert_audit_entry(&conn, AuditOperation::RecordCreate, None, None, None).unwrap();
        insert_audit_entry(&conn, AuditOperation::RecordDelete, None, None, None).unwrap();

        let count = count_audit_entries(&conn, Some("record.create"), None, None, None).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn count_audit_entries_with_search_filter() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO audit_log (operation, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["record.create", "GitHub", "created new", 1000],
        ).unwrap();
        conn.execute(
            "INSERT INTO audit_log (operation, record_name, detail, occurred_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["record.create", "AWS", "created aws", 2000],
        ).unwrap();

        let count = count_audit_entries(&conn, None, None, None, Some("Git")).unwrap();
        assert_eq!(count, 1);
    }

    // -----------------------------------------------------------------------
    // 12. rename_tag
    // -----------------------------------------------------------------------

    #[test]
    fn rename_tag_succeeds_when_tag_exists() {
        let conn = setup_db();
        insert_tag(&conn, "old-name").unwrap();

        let result = rename_tag(&conn, "old-name", "new-name").unwrap();
        assert!(result);

        let tag = get_tag_by_name(&conn, "new-name").unwrap().unwrap();
        assert_eq!(tag.name, "new-name");
    }

    #[test]
    fn rename_tag_returns_false_when_not_found() {
        let conn = setup_db();
        let result = rename_tag(&conn, "nonexistent", "something").unwrap();
        assert!(!result);
    }

    // -----------------------------------------------------------------------
    // 13. delete_tag_by_name
    // -----------------------------------------------------------------------

    #[test]
    fn delete_tag_by_name_removes_tag() {
        let conn = setup_db();
        insert_tag(&conn, "to-delete").unwrap();

        let result = delete_tag_by_name(&conn, "to-delete").unwrap();
        assert!(result);

        let tag = get_tag_by_name(&conn, "to-delete").unwrap();
        assert!(tag.is_none());
    }

    #[test]
    fn delete_tag_by_name_returns_false_when_not_found() {
        let conn = setup_db();
        let result = delete_tag_by_name(&conn, "nonexistent").unwrap();
        assert!(!result);
    }

    // -----------------------------------------------------------------------
    // 14. get_tag_by_name
    // -----------------------------------------------------------------------

    #[test]
    fn get_tag_by_name_returns_tag_when_found() {
        let conn = setup_db();
        let created = insert_tag(&conn, "my-tag").unwrap();

        let found = get_tag_by_name(&conn, "my-tag").unwrap().unwrap();
        assert_eq!(found.id, created.id);
        assert_eq!(found.name, "my-tag");
    }

    #[test]
    fn get_tag_by_name_returns_none_when_not_found() {
        let conn = setup_db();
        let found = get_tag_by_name(&conn, "no-such-tag").unwrap();
        assert!(found.is_none());
    }

    // -----------------------------------------------------------------------
    // 15. delete_metadata
    // -----------------------------------------------------------------------

    #[test]
    fn delete_metadata_removes_existing_key() {
        let conn = setup_db();
        set_metadata(&conn, "test_key", "test_value").unwrap();

        let result = delete_metadata(&conn, "test_key").unwrap();
        assert!(result);

        let value = get_metadata(&conn, "test_key").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn delete_metadata_returns_false_for_missing_key() {
        let conn = setup_db();
        let result = delete_metadata(&conn, "nonexistent_key").unwrap();
        assert!(!result);
    }
}
