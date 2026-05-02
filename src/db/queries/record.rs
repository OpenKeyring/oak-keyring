use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::{datetime_to_timestamp, RecordRow};
use crate::db::queries::DbError;
use crate::types::record::StoredRecord;

use super::Result;

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Get tag names for a record via JOIN on `record_tags` + `tags`.
pub(super) fn get_record_tags_inner(conn: &Connection, record_id: &Uuid) -> Result<Vec<String>> {
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
