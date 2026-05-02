use rusqlite::Connection;

use super::{DbError, Result};

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
