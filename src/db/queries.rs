// Queries are consumed by later tasks (services layer); suppress dead_code until then.
#![allow(dead_code)]

use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::{datetime_to_timestamp, AuditLogRow, RecordRow, TagRow};
use crate::types::audit::{AuditEntry, AuditOperation};
use crate::types::record::StoredRecord;
use crate::types::tag::Tag;

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

// ---------------------------------------------------------------------------
// Tag queries
// ---------------------------------------------------------------------------

/// Insert a new tag and return it with the auto-generated ID.
pub(crate) fn insert_tag(conn: &Connection, name: &str) -> Result<Tag> {
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
pub(crate) fn get_or_create_tag(conn: &Connection, name: &str) -> Result<Tag> {
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
pub(crate) fn list_tags(conn: &Connection) -> Result<Vec<Tag>> {
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
pub(crate) fn attach_tag(conn: &Connection, record_id: &Uuid, tag_id: i64) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO record_tags (record_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![record_id.to_string(), tag_id],
    )?;
    Ok(())
}

/// Detach a tag from a record.
pub(crate) fn detach_tag(conn: &Connection, record_id: &Uuid, tag_id: i64) -> Result<()> {
    conn.execute(
        "DELETE FROM record_tags WHERE record_id = ?1 AND tag_id = ?2",
        rusqlite::params![record_id.to_string(), tag_id],
    )?;
    Ok(())
}

/// Public wrapper: get tag names for a record.
pub(crate) fn get_record_tags(conn: &Connection, record_id: &Uuid) -> Result<Vec<String>> {
    get_record_tags_inner(conn, record_id)
}

// ---------------------------------------------------------------------------
// Audit queries
// ---------------------------------------------------------------------------

/// Insert an audit log entry with the current timestamp.
pub(crate) fn insert_audit_entry(
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
pub(crate) fn list_audit_entries(
    conn: &Connection,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditEntry>> {
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
pub(crate) fn get_metadata(conn: &Connection, key: &str) -> Result<Option<String>> {
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
pub(crate) fn set_metadata(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )?;
    Ok(())
}
