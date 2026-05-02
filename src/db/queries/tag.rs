use rusqlite::Connection;
use uuid::Uuid;

use crate::types::tag::Tag;

use super::record::get_record_tags_inner;
use super::{DbError, Result};

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
        Ok(crate::db::models::TagRow {
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
