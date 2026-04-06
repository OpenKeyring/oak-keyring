// Queries are consumed by later tasks (services layer); suppress dead_code until then.
#![allow(dead_code)]

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::{datetime_to_timestamp, RecordRow};
use crate::types::record::StoredRecord;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub(crate) enum DbError {
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
pub(crate) fn insert_record(conn: &Connection, record: &StoredRecord) -> Result<()> {
    let aad_param: Option<&[u8]> = if record.aad.is_empty() {
        None
    } else {
        Some(&record.aad)
    };

    conn.execute(
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
        conn.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            rusqlite::params![tag_name],
        )?;

        let tag_id: i64 = conn.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            rusqlite::params![tag_name],
            |row| row.get(0),
        )?;

        conn.execute(
            "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
            rusqlite::params![record.id.to_string(), tag_id],
        )?;
    }

    Ok(())
}

/// Fetch a single record by ID. Returns `None` if not found.
pub(crate) fn get_record(conn: &Connection, id: &Uuid) -> Result<Option<StoredRecord>> {
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
pub(crate) fn list_active_records(conn: &Connection) -> Result<Vec<StoredRecord>> {
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
pub(crate) fn soft_delete_record(conn: &Connection, id: &Uuid) -> Result<()> {
    let now = datetime_to_timestamp(&Utc::now());
    conn.execute(
        "UPDATE records SET deleted = 1, deleted_at = ?1 WHERE id = ?2",
        rusqlite::params![now, id.to_string()],
    )?;
    Ok(())
}

/// Restore a soft-deleted record: set `deleted = 0` and `deleted_at = NULL`.
pub(crate) fn restore_record(conn: &Connection, id: &Uuid) -> Result<()> {
    conn.execute(
        "UPDATE records SET deleted = 0, deleted_at = NULL WHERE id = ?1",
        rusqlite::params![id.to_string()],
    )?;
    Ok(())
}

/// Hard-delete a record (and cascading record_tags via FK).
pub(crate) fn hard_delete_record(conn: &Connection, id: &Uuid) -> Result<()> {
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
