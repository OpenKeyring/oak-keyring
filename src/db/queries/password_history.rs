use rusqlite::Connection;
use uuid::Uuid;

use crate::db::models::timestamp_to_datetime;
use crate::types::history::PasswordHistory;

use super::Result;

fn parse_record_id(raw: String) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn parse_changed_at(ts: i64) -> rusqlite::Result<chrono::DateTime<chrono::Utc>> {
    timestamp_to_datetime(ts).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Integer, Box::new(e))
    })
}

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
        let record_id = parse_record_id(row.get("record_id")?)?;
        Ok(PasswordHistory {
            id: row.get("id")?,
            record_id,
            encrypted_password: row.get("encrypted_password")?,
            nonce,
            dek_version: row.get::<_, i64>("dek_version")? as u32,
            changed_at: parse_changed_at(row.get("changed_at")?)?,
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
            let record_id = parse_record_id(row.get("record_id")?)?;
            Ok(PasswordHistory {
                id: row.get("id")?,
                record_id,
                encrypted_password: row.get("encrypted_password")?,
                nonce,
                dek_version: row.get::<_, i64>("dek_version")? as u32,
                changed_at: parse_changed_at(row.get("changed_at")?)?,
            })
        },
    );

    match result {
        Ok(entry) => Ok(Some(entry)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(super::DbError::from(e)),
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
